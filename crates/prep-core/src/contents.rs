//! Gestione dei contenuti di un esame file per file, senza rigenerare il pacchetto: si
//! aggiungono o sostituiscono singoli file di quiz, flashcard, libri e definizione
//! dell'esame, si tolgono, si elencano e si esporta tutto come pacchetto.
//!
//! I file stanno nel mirror dell'esame (`<esame>/mirror/content/…` e `…/books/…`), come quelli
//! di un pacchetto importato; l'indice del mirror tiene l'impronta di ognuno, così anche gli
//! altri dispositivi vedono che i contenuti sono cambiati.

use crate::content::Content;
use crate::model::{Book, BookCatalog, Chapter};
use crate::store::{blob_sha, Store};
use crate::vfs;
use serde_json::Value;

/// Libro in cui finiscono i capitoli importati da soli.
pub const IMPORTED_BOOK: &str = "importati";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// definizione dell'esame (`content/syllabus.json`)
    Exam,
    Questions,
    Cards,
    /// catalogo dei libri (`content/books.json`)
    Books,
    /// capitolo di un libro (Markdown)
    Chapter,
    /// figura SVG dei libri
    Figure,
    /// pacchetto intero (`.json.gz`)
    Pack,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Exam => "Esame",
            Kind::Questions => "Quiz",
            Kind::Cards => "Flashcard",
            Kind::Books => "Catalogo dei libri",
            Kind::Chapter => "Capitolo",
            Kind::Figure => "Figura",
            Kind::Pack => "Pacchetto",
        }
    }
}

/// Un file dei contenuti, per l'elenco nella pagina «Contenuti».
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: String,
    pub kind: Kind,
    /// nome leggibile (titolo del capitolo, nome del file…)
    pub name: String,
    /// riassunto: «120 domande», «3 materie»…
    pub info: String,
}

/// Riconosce il tipo di un file dal nome e dal contenuto.
pub fn classify(name: &str, bytes: &[u8]) -> Result<Kind, String> {
    let lower = name.to_lowercase();
    if bytes.starts_with(&[0x1f, 0x8b]) || lower.ends_with(".json.gz") {
        return Ok(Kind::Pack);
    }
    if lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".txt") {
        return Ok(Kind::Chapter);
    }
    if lower.ends_with(".svg") {
        return Ok(Kind::Figure);
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| {
        "formato non riconosciuto: servono file .json (quiz, flashcard, libri, esame), .md, .svg o un pacchetto .json.gz"
            .to_string()
    })?;
    if v.get("format").is_some() && v.get("files").is_some() {
        return Ok(Kind::Pack);
    }
    if v.get("subjects").is_some() {
        Ok(Kind::Exam)
    } else if v.get("questions").is_some() {
        Ok(Kind::Questions)
    } else if v.get("cards").is_some() {
        Ok(Kind::Cards)
    } else if v.get("books").is_some() {
        Ok(Kind::Books)
    } else {
        Err("file JSON non riconosciuto: deve contenere \"questions\", \"cards\", \"books\" o \"subjects\"".into())
    }
}

/// Nome di file sicuro: minuscole, cifre e trattini, con l'estensione indicata.
fn file_name(name: &str, ext: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = base.split('.').next().unwrap_or(base);
    format!("{}.{ext}", crate::exams::slug(stem))
}

/// Titolo di un capitolo: il primo titolo Markdown, altrimenti il nome del file.
fn chapter_title(text: &str, name: &str) -> String {
    text.lines()
        .find_map(|l| l.trim_start().strip_prefix('#').map(|t| t.trim_start_matches('#').trim().to_string()))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            name.rsplit(['/', '\\']).next().unwrap_or(name).split('.').next().unwrap_or(name).to_string()
        })
}

fn files_of(store: &Store) -> Vec<(String, String)> {
    let mirror = store.root.join("mirror");
    let mut out = Vec::new();
    for dir in ["content", "books"] {
        for p in vfs::walk_files(&mirror.join(dir)) {
            if let Ok(rel) = p.strip_prefix(&mirror) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if crate::pack::in_pack(&rel) {
                    out.push((rel, vfs::read_to_string(&p).unwrap_or_default()));
                }
            }
        }
    }
    out.sort();
    out
}

fn write(store: &Store, path: &str, bytes: &[u8]) -> Result<(), String> {
    store.write_mirror(path, bytes).map_err(|e| format!("{path}: {e}"))?;
    let mut idx = store.index();
    idx.files.insert(path.to_string(), blob_sha(bytes));
    idx.synced_once = true;
    store.save_index(&idx).map_err(|e| e.to_string())
}

fn catalog(store: &Store) -> BookCatalog {
    store
        .mirror_path("content/books.json")
        .and_then(|p| vfs::read_to_string(&p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(BookCatalog { schema: 1, books: Vec::new() })
}

/// Esito di un'aggiunta: dove è finito il file e un riassunto da mostrare.
#[derive(Debug, Clone)]
pub struct Added {
    pub path: String,
    pub kind: Kind,
    pub summary: String,
    /// avvisi dei contenuti dopo l'aggiunta (argomenti sconosciuti, id doppi…)
    pub warnings: Vec<String>,
}

/// Aggiunge (o sostituisce, se ha lo stesso nome) un singolo file ai contenuti dell'esame.
/// I capitoli Markdown vanno nel libro «Materiale importato» della materia `subject`.
/// Un pacchetto intero va importato con [`crate::pack::import_merge`].
pub fn add_file(store: &Store, name: &str, bytes: &[u8], subject: &str) -> Result<Added, String> {
    let kind = classify(name, bytes)?;
    let text = std::str::from_utf8(bytes).map_err(|_| "il file non è testo UTF-8".to_string())?;
    let path = match kind {
        Kind::Pack => return Err("questo è un pacchetto intero: importalo come pacchetto".into()),
        Kind::Exam => "content/syllabus.json".to_string(),
        Kind::Books => "content/books.json".to_string(),
        Kind::Questions => format!("content/questions/{}", file_name(name, "json")),
        Kind::Cards => format!("content/flashcards/{}", file_name(name, "json")),
        Kind::Chapter => format!("books/{IMPORTED_BOOK}/{}", file_name(name, "md")),
        Kind::Figure => format!("books/{IMPORTED_BOOK}/{}", file_name(name, "svg")),
    };
    // il file deve essere leggibile prima di sostituire quello che c'è
    let mut files = files_of(store);
    files.retain(|(p, _)| *p != path);
    let summary = match kind {
        Kind::Exam => {
            let s: crate::model::Syllabus =
                serde_json::from_str(text).map_err(|e| format!("definizione dell'esame non valida: {e}"))?;
            format!("esame «{}»: {} materie", s.title, s.subjects.len())
        }
        Kind::Questions => {
            let f: crate::model::QuestionFile =
                serde_json::from_str(text).map_err(|e| format!("file di quiz non valido: {e}"))?;
            format!("{} domande", f.questions.len())
        }
        Kind::Cards => {
            let f: crate::model::FlashcardFile =
                serde_json::from_str(text).map_err(|e| format!("file di flashcard non valido: {e}"))?;
            format!("{} flashcard", f.cards.len())
        }
        Kind::Books => {
            let c: BookCatalog =
                serde_json::from_str(text).map_err(|e| format!("catalogo dei libri non valido: {e}"))?;
            format!("{} libri", c.books.len())
        }
        Kind::Chapter => format!("capitolo «{}»", chapter_title(text, name)),
        Kind::Figure => "figura".to_string(),
        Kind::Pack => unreachable!(),
    };
    files.push((path.clone(), text.to_string()));
    // un capitolo da solo entra nel libro «Materiale importato» della materia scelta
    let mut book_json = None;
    if kind == Kind::Chapter {
        let mut cat = catalog(store);
        let book = match cat.books.iter().position(|b| b.id == IMPORTED_BOOK) {
            Some(i) => &mut cat.books[i],
            None => {
                cat.books.push(Book {
                    id: IMPORTED_BOOK.into(),
                    subject: subject.to_string(),
                    title: "Materiale importato".into(),
                    author: String::new(),
                    chapters: Vec::new(),
                });
                cat.books.last_mut().expect("appena aggiunto")
            }
        };
        let title = chapter_title(text, name);
        match book.chapters.iter_mut().find(|c| c.file == path) {
            Some(c) => c.title = title,
            None => book.chapters.push(Chapter { file: path.clone(), title, topics: Vec::new(), sections: Vec::new() }),
        }
        let j = serde_json::to_string_pretty(&cat).map_err(|e| e.to_string())?;
        files.retain(|(p, _)| p != "content/books.json");
        files.push(("content/books.json".into(), j.clone()));
        book_json = Some(j);
    }
    let content = Content::from_files(files.iter().map(|(p, t)| (p.as_str(), t.as_str())));
    write(store, &path, bytes)?;
    if let Some(j) = book_json {
        write(store, "content/books.json", j.as_bytes())?;
    }
    Ok(Added { path, kind, summary, warnings: content.warnings })
}

/// Toglie un file dai contenuti (un capitolo importato esce anche dal suo libro).
pub fn remove_file(store: &Store, path: &str) -> Result<(), String> {
    if !crate::pack::in_pack(path) {
        return Err(format!("percorso non ammesso: {path}"));
    }
    store.remove_mirror(path);
    let mut idx = store.index();
    idx.files.remove(path);
    store.save_index(&idx).map_err(|e| e.to_string())?;
    if path.ends_with(".md") {
        let mut cat = catalog(store);
        let before: usize = cat.books.iter().map(|b| b.chapters.len()).sum();
        for b in &mut cat.books {
            b.chapters.retain(|c| c.file != path);
        }
        cat.books.retain(|b| !(b.id == IMPORTED_BOOK && b.chapters.is_empty()));
        if cat.books.iter().map(|b| b.chapters.len()).sum::<usize>() != before {
            let j = serde_json::to_string_pretty(&cat).map_err(|e| e.to_string())?;
            write(store, "content/books.json", j.as_bytes())?;
        }
    }
    Ok(())
}

/// Elenco dei file dei contenuti, con tipo e riassunto.
pub fn list(store: &Store) -> Vec<Entry> {
    let files = files_of(store);
    let cat = files
        .iter()
        .find(|(p, _)| p == "content/books.json")
        .and_then(|(_, t)| serde_json::from_str::<BookCatalog>(t).ok())
        .unwrap_or_default();
    let mut out: Vec<Entry> = files
        .iter()
        .map(|(path, text)| {
            let v: Value =
                if path.ends_with(".json") { serde_json::from_str(text).unwrap_or(Value::Null) } else { Value::Null };
            let count = |k: &str| v.get(k).and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0);
            let file = path.rsplit('/').next().unwrap_or(path).to_string();
            let (kind, name, info) = if path == "content/syllabus.json" {
                let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("esame").to_string();
                (Kind::Exam, title, format!("{} materie", count("subjects")))
            } else if path == "content/books.json" {
                (Kind::Books, file, format!("{} libri", count("books")))
            } else if path.starts_with("content/questions/") {
                (Kind::Questions, file, format!("{} domande", count("questions")))
            } else if path.starts_with("content/flashcards/") {
                (Kind::Cards, file, format!("{} flashcard", count("cards")))
            } else if path.ends_with(".md") {
                let found = cat.books.iter().find_map(|b| {
                    b.chapters.iter().find(|c| c.file == *path).map(|c| (b.title.clone(), c.title.clone()))
                });
                match found {
                    Some((book, ch)) => (Kind::Chapter, ch, book),
                    None => (Kind::Chapter, file, "non collegato a un libro".into()),
                }
            } else if path.ends_with(".svg") {
                (Kind::Figure, file, path.trim_start_matches("books/").to_string())
            } else {
                (Kind::Books, file, String::new())
            };
            Entry { path: path.clone(), kind, name, info }
        })
        .collect();
    let rank = |k: Kind| match k {
        Kind::Exam => 0,
        Kind::Questions => 1,
        Kind::Cards => 2,
        Kind::Books => 3,
        Kind::Chapter => 4,
        Kind::Figure => 5,
        Kind::Pack => 6,
    };
    out.sort_by(|a, b| rank(a.kind).cmp(&rank(b.kind)).then(a.path.cmp(&b.path)));
    out
}

/// Il pacchetto (`.json.gz`) con i contenuti attuali dell'esame.
pub fn export(store: &Store, label: &str) -> Result<Vec<u8>, String> {
    crate::pack::build(&store.root.join("mirror"), label).map(|(bytes, _)| bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn store(name: &str) -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("prep-contents-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let st = Store::open(&dir).unwrap();
        // un esame di partenza: l'esempio del repository
        let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/esempio");
        let (gz, _) = crate::pack::build(&ex, "test").unwrap();
        crate::pack::import(&st, &gz).unwrap();
        (st, dir)
    }

    #[test]
    fn classifies_files() {
        assert_eq!(classify("a.json", br#"{"schema":1,"questions":[]}"#).unwrap(), Kind::Questions);
        assert_eq!(classify("a.json", br#"{"schema":1,"cards":[]}"#).unwrap(), Kind::Cards);
        assert_eq!(classify("a.json", br#"{"schema":1,"subjects":[]}"#).unwrap(), Kind::Exam);
        assert_eq!(classify("a.json", br#"{"schema":1,"books":[]}"#).unwrap(), Kind::Books);
        assert_eq!(classify("x.json.gz", &[0x1f, 0x8b, 0]).unwrap(), Kind::Pack);
        assert_eq!(classify("Cap 1.md", b"# Titolo").unwrap(), Kind::Chapter);
        assert!(classify("a.json", b"{\"altro\":1}").is_err());
        assert!(classify("a.pdf", b"%PDF").is_err());
    }

    #[test]
    fn add_remove_list_export() {
        let (st, dir) = store("arle");
        let before = list(&st);
        assert!(before.iter().any(|e| e.kind == Kind::Exam));
        // quiz singolo: nuovo file, contato nell'elenco
        let q = br#"{"schema":1,"questions":[{"id":"x1","topic":"mat-u1-t1","type":"mcq","text":"?","options":["a","b","c","d","e"],"answer":0,"explanation":"."}]}"#;
        let a = add_file(&st, "Nuovi Quiz.json", q, "mat").unwrap();
        assert_eq!(a.path, "content/questions/nuovi-quiz.json");
        assert_eq!(a.summary, "1 domande");
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert!(list(&st).iter().any(|e| e.path == a.path && e.info == "1 domande"));
        // file non valido: niente cambia
        assert!(add_file(&st, "rotto.json", br#"{"questions":5}"#, "mat").is_err());
        assert!(!list(&st).iter().any(|e| e.path.contains("rotto")));
        // capitolo da solo: va nel libro «Materiale importato»
        let c = add_file(&st, "Appunti di fisica.md", b"# Cinematica\n\nTesto.", "sci").unwrap();
        assert_eq!(c.path, "books/importati/appunti-di-fisica.md");
        let ch = list(&st).into_iter().find(|e| e.path == c.path).unwrap();
        assert_eq!((ch.name.as_str(), ch.info.as_str()), ("Cinematica", "Materiale importato"));
        // esportato: il pacchetto contiene i file aggiunti e si reimporta
        let gz = export(&st, "prova").unwrap();
        let info = crate::pack::inspect(&gz).unwrap();
        assert!(info.questions > 1);
        // tolti: il libro vuoto sparisce dal catalogo
        remove_file(&st, &c.path).unwrap();
        remove_file(&st, &a.path).unwrap();
        let after = list(&st);
        assert_eq!(after.len(), before.len() + 1, "resta solo books.json in più"); // catalogo creato
        assert!(!vfs::read_to_string(&st.mirror_path("content/books.json").unwrap()).unwrap().contains(IMPORTED_BOOK));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
