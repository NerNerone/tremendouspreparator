//! Modello di un ramo d'esame nel repository dei dati: guida, formati, schemi, strumenti e
//! direttive per l'AI. Viene scritto nei rami nuovi creati dall'app (e coincide con il ramo
//! `modello-dati` del repository dell'app).

/// File comuni a ogni ramo d'esame: percorso nel ramo e contenuto.
const COMMON: &[(&str, &[u8])] = &[
    ("README.md", include_bytes!("../../assets/modello/README.md")),
    ("CLAUDE.md", include_bytes!("../../assets/modello/CLAUDE.md")),
    ("AGENTS.md", include_bytes!("../../assets/modello/CLAUDE.md")),
    ("docs/FORMATI.md", include_bytes!("../../../../docs/FORMATI.md")),
    ("docs/schema/syllabus.schema.json", include_bytes!("../../../../docs/schema/syllabus.schema.json")),
    ("docs/schema/questions.schema.json", include_bytes!("../../../../docs/schema/questions.schema.json")),
    ("docs/schema/flashcards.schema.json", include_bytes!("../../../../docs/schema/flashcards.schema.json")),
    ("docs/schema/books.schema.json", include_bytes!("../../../../docs/schema/books.schema.json")),
    ("docs/guida/00-con-l-ai.md", include_bytes!("../../assets/guida/00-con-l-ai.md")),
    ("docs/guida/01-panoramica.md", include_bytes!("../../assets/guida/01-panoramica.md")),
    ("docs/guida/02-esame.md", include_bytes!("../../assets/guida/02-esame.md")),
    ("docs/guida/03-quiz.md", include_bytes!("../../assets/guida/03-quiz.md")),
    ("docs/guida/04-flashcard.md", include_bytes!("../../assets/guida/04-flashcard.md")),
    ("docs/guida/05-libri.md", include_bytes!("../../assets/guida/05-libri.md")),
    ("docs/guida/06-dai-pdf.md", include_bytes!("../../assets/guida/06-dai-pdf.md")),
    ("docs/guida/07-importare.md", include_bytes!("../../assets/guida/07-importare.md")),
    ("docs/guida/08-prompt.md", include_bytes!("../../assets/guida/08-prompt.md")),
    ("tools/pdf2md.py", include_bytes!("../../../../tools/pdf2md.py")),
    ("tools/import_md.py", include_bytes!("../../../../tools/import_md.py")),
    ("tools/mathsvg.mjs", include_bytes!("../../../../tools/mathsvg.mjs")),
    ("tools/package.json", include_bytes!("../../../../tools/package.json")),
    ("books/.gitkeep", b""),
    ("progress/sessions/.gitkeep", b""),
    ("progress/goals/.gitkeep", b""),
    ("progress/groups/.gitkeep", b""),
    ("progress/feedback/.gitkeep", b""),
];

/// Pacchetto d'esempio, solo nei rami nuovi «vuoti» (da sostituire con l'esame vero).
const EXAMPLE: &[(&str, &[u8])] = &[
    ("content/syllabus.json", include_bytes!("../../assets/modello/esempio/content/syllabus.json")),
    ("content/questions/esempio.json", include_bytes!("../../assets/modello/esempio/content/questions/esempio.json")),
    ("content/flashcards/esempio.json", include_bytes!("../../assets/modello/esempio/content/flashcards/esempio.json")),
];

/// File del modello; con `example` anche il pacchetto d'esempio.
pub(super) fn files(example: bool) -> Vec<(String, Vec<u8>)> {
    COMMON.iter().chain(if example { EXAMPLE } else { &[] }).map(|(p, b)| (p.to_string(), b.to_vec())).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn template_has_directives_and_tools() {
        let f = super::files(true);
        for p in ["CLAUDE.md", "AGENTS.md", "docs/FORMATI.md", "tools/pdf2md.py", "content/syllabus.json"] {
            assert!(f.iter().any(|(x, b)| x == p && !b.is_empty()), "manca {p}");
        }
        assert!(!super::files(false).iter().any(|(p, _)| p.starts_with("content/")));
    }
}
