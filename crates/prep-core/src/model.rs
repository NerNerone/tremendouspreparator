//! Tipi dei contenuti di studio: syllabus, domande, flashcard.
//! I file JSON stanno in `content/` nel repository (vedi `docs/FORMATI.md`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Syllabus {
    pub schema: u32,
    pub title: String,
    pub exam: ExamRules,
    pub subjects: Vec<Subject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExamRules {
    pub questions: usize,
    pub mcq: usize,
    pub fill: usize,
    pub options: usize,
    pub minutes: u32,
    pub correct: f64,
    pub wrong: f64,
    pub omitted: f64,
    pub pass: f64,
    #[serde(default)]
    pub dates: Vec<ExamDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExamDate {
    pub label: String,
    /// formato YYYY-MM-DD
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    pub id: String,
    pub name: String,
    pub units: Vec<Unit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    pub id: String,
    pub n: u32,
    pub name: String,
    pub cfu: f64,
    pub topics: Vec<Topic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QKind {
    /// Risposta multipla (5 opzioni, una corretta)
    Mcq,
    /// Completamento (risposta aperta breve: parola o numero)
    Fill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Numeric {
    pub value: f64,
    /// tolleranza relativa (0.02 = 2%)
    #[serde(default = "default_tol")]
    pub tolerance: f64,
    #[serde(default)]
    pub unit: String,
}

fn default_tol() -> f64 {
    0.02
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub topic: String,
    #[serde(rename = "type")]
    pub kind: QKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// indice (0-based) dell'opzione corretta per le `mcq`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<usize>,
    /// risposte accettate per i `fill` testuali
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<String>,
    /// risposta numerica per i `fill` numerici
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeric: Option<Numeric>,
    #[serde(default)]
    pub explanation: String,
    /// provenienza: "seed", "libro:<file>#<sezione>", ...
    #[serde(default)]
    pub source: String,
    /// 1 facile .. 3 difficile
    #[serde(default = "default_diff")]
    pub difficulty: u8,
    /// rimando al libro per "Se vuoi approfondire"
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub book_ref: Option<BookRef>,
    /// gruppo di provenienza mostrato nel filtro «Fonti» del quiz (es. "EdiSES");
    /// se manca si ricava da `source` (vedi [`Question::source_group`])
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
}

/// Vedi [`Question::source_group`].
pub fn source_group(source: &str) -> String {
    let s = source.trim();
    let low = s.to_lowercase();
    if s.is_empty() {
        return "Senza fonte".into();
    }
    if low == "seed" {
        return "Domande di base".into();
    }
    if let Some(path) = s.strip_prefix("libro:") {
        // libro:books/<materia>/<libro>/<file>.md#… → «Libro <materia>/<libro>»
        let parts: Vec<&str> = path.split('#').next().unwrap_or(path).split('/').collect();
        return match parts.as_slice() {
            ["books", subj, book, ..] => format!("Libro {subj}/{book}"),
            _ => "Libro".into(),
        };
    }
    if low.starts_with("esame") || low.starts_with("prova") {
        return "Prove d'esame".into();
    }
    let head = s.split([',', '(', ';', ':']).next().unwrap_or(s).trim();
    let head = head.strip_prefix("Eserciziario ").or_else(|| head.strip_prefix("eserciziario ")).unwrap_or(head);
    match head.split_whitespace().next() {
        Some(w) if !w.is_empty() => w.to_string(),
        _ => "Senza fonte".into(),
    }
}

fn default_diff() -> u8 {
    2
}

impl Question {
    /// Fonte raggruppata per il filtro del quiz: il campo `group` se presente, altrimenti
    /// ricavata da `source` ("libro:books/chimica/x/…" → Libro chimica/x, "esame …" → Prove d'esame,
    /// "Eserciziario EdiSES, …" → EdiSES).
    pub fn source_group(&self) -> String {
        if !self.group.trim().is_empty() {
            return self.group.trim().to_string();
        }
        source_group(&self.source)
    }

    /// Testo leggibile della risposta corretta.
    pub fn correct_text(&self) -> String {
        match self.kind {
            QKind::Mcq => match self.answer.and_then(|i| self.options.get(i).map(|o| (i, o))) {
                Some((i, o)) => format!("{}) {}", letter(i), o),
                None => "?".into(),
            },
            QKind::Fill => {
                if let Some(n) = &self.numeric {
                    let v = crate::answer::format_number(n.value);
                    if n.unit.is_empty() {
                        v
                    } else {
                        format!("{} {}", v, n.unit)
                    }
                } else {
                    self.answers.first().cloned().unwrap_or_default()
                }
            }
        }
    }
}

pub fn letter(i: usize) -> char {
    (b'A' + (i as u8).min(25)) as char
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flashcard {
    pub id: String,
    pub topic: String,
    pub front: String,
    pub back: String,
    #[serde(default)]
    pub source: String,
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub book_ref: Option<BookRef>,
}

/// Rimando a un punto di un libro in `books/`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BookRef {
    /// id del libro in `content/books.json`
    pub book: String,
    /// file markdown (percorso nel repo, es. `books/fisica/giancoli/09-fluidi.md`)
    #[serde(default)]
    pub file: String,
    /// titolo (o inizio del titolo, es. "9.3") della sezione
    #[serde(default)]
    pub section: String,
    /// pagina del libro originale
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub file: String,
    pub title: String,
    /// argomenti del syllabus trattati: usati come rimando per le domande senza `ref`
    #[serde(default)]
    pub topics: Vec<String>,
    /// rimandi più precisi: argomento -> sezione del capitolo
    #[serde(default)]
    pub sections: Vec<SectionTopics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionTopics {
    /// titolo o numerazione della sezione (es. "3.3")
    pub section: String,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub subject: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookCatalog {
    pub schema: u32,
    #[serde(default)]
    pub books: Vec<Book>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuestionFile {
    pub schema: u32,
    #[serde(default)]
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlashcardFile {
    pub schema: u32,
    #[serde(default)]
    pub cards: Vec<Flashcard>,
}

/// Ordine casuale in cui mostrare le `n` opzioni: `perm[posizione mostrata] = indice originale`.
pub fn shuffled_order(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    fastrand::shuffle(&mut v);
    v
}

/// Nelle spiegazioni dei libri le opzioni sono citate per lettera ("risposta B errata",
/// "risposte A e D", "opzione C"): con le opzioni mescolate le lettere vanno rimappate
/// su quelle mostrate. `perm` come in [`shuffled_order`].
pub fn remap_letters(text: &str, perm: &[usize]) -> String {
    if perm.iter().enumerate().all(|(d, &o)| d == o) {
        return text.to_string();
    }
    let mut disp = vec![0usize; perm.len()];
    for (d, &o) in perm.iter().enumerate() {
        if o < disp.len() {
            disp[o] = d;
        }
    }
    let map = |c: char| -> char {
        let o = (c as u8).wrapping_sub(b'A') as usize;
        disp.get(o).map(|&d| letter(d)).unwrap_or(c)
    };
    // "risposta B", "risposte A, C e D", "opzione E", "alternativa C", "lettera D", "(A)", "A)"
    let re = regex_lite::Regex::new(
        r"(?i:\b(?:rispost[ae]|opzion[ei]|alternativ[ae]|letter[ae])\s+)([A-E](?:\s*(?:,|e|ed|o|ed anche)\s*[A-E])*)\b|\(([A-E])\)",
    )
    .unwrap();
    re.replace_all(text, |c: &regex_lite::Captures| {
        let whole = c.get(0).unwrap().as_str();
        let span = c.get(1).or(c.get(2)).unwrap();
        let start = span.start() - c.get(0).unwrap().start();
        let (head, tail) = whole.split_at(start);
        let letters: String = tail
            .chars()
            .map(|ch| if ch.is_ascii_uppercase() && ('A'..='E').contains(&ch) { map(ch) } else { ch })
            .collect();
        format!("{head}{letters}")
    })
    .into_owned()
}

/// Come [`remap_letters`], ma riconosce anche gli elenchi "• B) testo dell'opzione: perché…"
/// delle spiegazioni: la lettera viene presa dall'opzione il cui testo apre la riga (così è
/// giusta anche se la fonte aveva lettere sbagliate) e le righe sono riordinate per lettera.
pub fn remap_explanation(text: &str, perm: &[usize], options: &[String]) -> String {
    let norm = |s: &str| s.trim().trim_end_matches(['.', ':', ';']).to_lowercase();
    let bullet = regex_lite::Regex::new(r"^(\s*•\s*)([A-E])\)\s*(.*)$").unwrap();
    let mut out: Vec<String> = Vec::new();
    let mut block: Vec<(usize, String)> = Vec::new();
    let flush = |block: &mut Vec<(usize, String)>, out: &mut Vec<String>| {
        block.sort_by_key(|(k, _)| *k);
        out.extend(block.drain(..).map(|(_, l)| l));
    };
    for line in text.split('\n') {
        if let Some(c) = bullet.captures(line) {
            let rest = c.get(3).unwrap().as_str();
            if let Some(m) = more_letters(rest) {
                let first = (c[2].as_bytes()[0] - b'A') as usize;
                let shown = |o: usize| perm.iter().position(|&k| k == o).unwrap_or(o);
                let mut ls: Vec<usize> = std::iter::once(first).chain(m.1).map(shown).collect();
                ls.sort();
                let head = match ls.len() {
                    1 => format!("{})", letter(ls[0])),
                    n => {
                        let a: Vec<String> = ls[..n - 1].iter().map(|&l| format!("{})", letter(l))).collect();
                        format!("{} e {})", a.join(", "), letter(ls[n - 1]))
                    }
                };
                let tail = rest[m.0..].trim_start();
                let sep = if tail.starts_with([':', ',', '.']) { "" } else { " " };
                block.push((ls[0], format!("{}{}{}{}", &c[1], head, sep, remap_letters(tail, perm))));
                continue;
            }
            let orig = (c[2].as_bytes()[0] - b'A') as usize;
            // l'opzione citata per testo (la più lunga che apre la riga) vince sulla lettera
            let by_text = options
                .iter()
                .enumerate()
                .filter(|(_, o)| !norm(o).is_empty() && norm(rest).starts_with(&norm(o)))
                .max_by_key(|(_, o)| o.len())
                .map(|(i, _)| i);
            let o = by_text.unwrap_or(orig);
            let shown = perm.iter().position(|&k| k == o).unwrap_or(o);
            block.push((shown, format!("{}{}) {}", &c[1], letter(shown), remap_letters(rest, perm))));
        } else {
            flush(&mut block, &mut out);
            out.push(remap_letters(line, perm));
        }
    }
    flush(&mut block, &mut out);
    out.join("\n")
}

/// Lettere aggiuntive in testa a una riga d'elenco dopo la prima («, C) e E): …»):
/// (byte dove finisce l'elenco di lettere, indici delle opzioni).
fn more_letters(rest: &str) -> Option<(usize, Vec<usize>)> {
    let re = regex_lite::Regex::new(r"^(?:\s*(?:,|e|ed|/)\s*([A-E])\))+").unwrap();
    let m = re.find(rest)?;
    let one = regex_lite::Regex::new(r"([A-E])\)").unwrap();
    let ls = one.captures_iter(m.as_str()).map(|c| (c[1].as_bytes()[0] - b'A') as usize).collect();
    Some((m.end(), ls))
}

/// Divide la spiegazione in una parte generale e nelle motivazioni per opzione (elenchi
/// «• B) testo: perché…»), per mostrarle dentro i riquadri delle risposte. Restituisce
/// (testo generale, motivazione per ciascuna opzione nell'ordine originale).
pub fn option_notes(text: &str, options: &[String]) -> (String, Vec<String>) {
    let norm = |s: &str| s.trim().trim_end_matches(['.', ':', ';']).to_lowercase();
    let bullet = regex_lite::Regex::new(r"^\s*•\s*([A-E])\)\s*(.*)$").unwrap();
    let mut notes = vec![String::new(); options.len()];
    let mut general: Vec<&str> = Vec::new();
    let mut any = false;
    for line in text.split('\n') {
        let Some(c) = bullet.captures(line) else {
            general.push(line);
            continue;
        };
        let rest = c.get(2).unwrap().as_str();
        // «• A), C) e E): motivo comune» → la stessa nota sotto ciascuna opzione elencata
        if let Some(m) = more_letters(rest) {
            let reason = rest[m.0..].trim().trim_start_matches([':', ' ', '—', '–', '-']).trim().to_string();
            if !reason.is_empty() {
                for k in std::iter::once((c[1].as_bytes()[0] - b'A') as usize).chain(m.1) {
                    if k < notes.len() {
                        notes[k] = reason.clone();
                        any = true;
                    }
                }
            }
            continue;
        }
        let by_text = options
            .iter()
            .enumerate()
            .filter(|(_, o)| !norm(o).is_empty() && norm(rest).starts_with(&norm(o)))
            .max_by_key(|(_, o)| o.len());
        let (k, reason) = match by_text {
            Some((k, o)) => {
                let r =
                    rest.trim()[o.trim().len().min(rest.trim().len())..].trim_start_matches([':', ' ', '—', '–', '-']);
                (k, r.trim().to_string())
            }
            None => ((c[1].as_bytes()[0] - b'A') as usize, rest.trim().to_string()),
        };
        if k < notes.len() && !reason.is_empty() {
            notes[k] = reason;
            any = true;
        }
    }
    if any {
        // via l'intestazione dell'elenco («Perché le altre sono sbagliate:»)
        while general.last().is_some_and(|l| l.trim().is_empty() || l.trim_end().ends_with(':')) {
            general.pop();
        }
    }
    (general.join("\n").trim().to_string(), notes)
}

#[cfg(test)]
mod shuffle_tests {
    use super::*;

    #[test]
    fn remap() {
        // mostrate: [C, A, B, D, E] → la A originale è la B mostrata, la C originale è la A mostrata
        let perm = vec![2, 0, 1, 3, 4];
        assert_eq!(
            remap_letters("La risposta A è errata, mentre la C è giusta", &perm),
            "La risposta B è errata, mentre la C è giusta"
        );
        assert_eq!(remap_letters("(risposte A e C errate)", &perm), "(risposte B e A errate)");
        assert_eq!(remap_letters("vedi opzione B; (C) no", &perm), "vedi opzione C; (A) no");
        assert_eq!(remap_letters("Il DNA (A)", &[0, 1, 2, 3, 4]), "Il DNA (A)");
        let opts: Vec<String> =
            ["Argon", "Fluoro", "Azoto", "Neon", "Ossigeno"].iter().map(|s| s.to_string()).collect();
        // mostrate: [Fluoro, Neon, Argon, Azoto, Ossigeno]
        let perm2 = vec![1, 3, 0, 2, 4];
        let e = "Z = 9.\nPerché le altre:\n• B) Argon: Z = 18.\n• C) Azoto: Z = 7 (opzione C).\n• D) Neon: Z = 10.";
        assert_eq!(
            remap_explanation(e, &perm2, &opts),
            "Z = 9.\nPerché le altre:\n• B) Neon: Z = 10.\n• C) Argon: Z = 18.\n• D) Azoto: Z = 7 (opzione D)."
        );
        let (g, n) = option_notes(e, &opts);
        assert_eq!(g, "Z = 9.");
        assert_eq!(n[0], "Z = 18.");
        assert_eq!(n[2], "Z = 7 (opzione C).");
        assert!(n[1].is_empty());
        let e2 = "Testo.\n• A), C) e E): sono tutti gas nobili.\n• B) Neon: no.";
        let (_, n2) = option_notes(e2, &opts);
        assert_eq!(n2[0], "sono tutti gas nobili.");
        assert_eq!(n2[2], "sono tutti gas nobili.");
        assert_eq!(n2[4], "sono tutti gas nobili.");
        assert_eq!(n2[3], "no.");
        assert_eq!(
            remap_explanation(e2, &perm2, &opts).lines().find(|l| l.contains("gas")),
            Some("• C), D) e E): sono tutti gas nobili.")
        );
        let p = shuffled_order(5);
        let mut s = p.clone();
        s.sort();
        assert_eq!(s, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn source_groups() {
        assert_eq!(source_group("Eserciziario EdiSES, Chimica cap. 12, quiz n. 3"), "EdiSES");
        assert_eq!(source_group("Eserciziario Testbusters (parte 2) p. 40"), "Testbusters");
        assert_eq!(source_group("esame 2025 1° appello (TestBuddy)"), "Prove d'esame");
        assert_eq!(
            source_group("libro:books/chimica/integrazioni/02-nomenclatura.md#2.2"),
            "Libro chimica/integrazioni"
        );
        assert_eq!(source_group("seed"), "Domande di base");
        assert_eq!(source_group(""), "Senza fonte");
    }
}
