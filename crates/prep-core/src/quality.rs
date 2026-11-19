//! Controllo di qualità dei contenuti di un esame: errori (cose che non funzionano), avvisi
//! (domande o carte da sistemare) e consigli (argomenti scoperti, la barra che non può arrivare
//! in fondo). Lo usano `prep check` e la pagina Contenuti dell'app.

use crate::content::Content;
use crate::model::QKind;

/// Esito del controllo.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub tips: Vec<String>,
}

impl Report {
    pub fn summary(&self) -> String {
        if self.errors.is_empty() && self.warnings.is_empty() && self.tips.is_empty() {
            return "Tutto a posto: nessun problema trovato.".into();
        }
        format!("{} errori · {} avvisi · {} consigli", self.errors.len(), self.warnings.len(), self.tips.len())
    }
}

/// Domande diverse per argomento che servono alla barra per arrivare in fondo.
const COVER: usize = 6;

/// Controlla i contenuti; `read` legge un file del pacchetto (capitoli, figure) se c'è.
pub fn check(c: &Content, read: &dyn Fn(&str) -> Option<String>) -> Report {
    let mut r = Report::default();
    let syl = c.syllabus();
    if syl.subjects.is_empty() {
        r.errors.push(
            "manca la definizione dell'esame (content/syllabus.json): senza, materie e argomenti non esistono".into(),
        );
    }
    // file malformati, id doppi, argomenti e libri sconosciuti, domande senza risposta
    r.errors.extend(c.warnings.iter().cloned());

    // esame
    let rules = &syl.exam;
    if rules.questions > 0 && rules.mcq + rules.fill != rules.questions {
        r.warnings.push(format!(
            "regole d'esame: {} quesiti ma {} a risposta multipla + {} a completamento",
            rules.questions, rules.mcq, rules.fill
        ));
    }
    let mut topic_ids = std::collections::HashSet::new();
    for s in &syl.subjects {
        if s.units.is_empty() {
            r.warnings.push(format!("materia {}: nessuna unità", s.id));
        }
        r.warnings.extend(crate::icons::problems(s));
        for u in &s.units {
            if u.topics.is_empty() {
                r.warnings.push(format!("unità {}: nessun argomento", u.id));
            }
            for t in &u.topics {
                if !topic_ids.insert(t.id.clone()) {
                    r.errors.push(format!("argomento ripetuto: {}", t.id));
                }
            }
        }
    }

    // domande
    let options = if rules.options > 0 { rules.options } else { 5 };
    for q in &c.questions {
        if q.text.trim().chars().count() < 8 {
            r.warnings.push(format!("{}: testo troppo corto o vuoto", q.id));
        }
        if q.explanation.trim().is_empty() {
            r.warnings.push(format!("{}: manca la spiegazione", q.id));
        }
        if !(1..=3).contains(&q.difficulty) {
            r.warnings.push(format!("{}: difficoltà {} (va da 1 a 3)", q.id, q.difficulty));
        }
        if q.kind == QKind::Mcq {
            if q.options.len() != options {
                r.warnings.push(format!("{}: {} opzioni invece di {options}", q.id, q.options.len()));
            }
            let mut seen = std::collections::HashSet::new();
            if q.options.iter().any(|o| !seen.insert(o.trim().to_lowercase())) {
                r.warnings.push(format!("{}: opzioni ripetute", q.id));
            }
            if q.options.iter().any(|o| o.trim().is_empty()) {
                r.warnings.push(format!("{}: un'opzione è vuota", q.id));
            }
        }
        if let Some(n) = &q.numeric {
            if !n.value.is_finite() || n.tolerance < 0.0 || n.tolerance > 0.5 {
                r.warnings.push(format!("{}: risposta numerica o tolleranza strana", q.id));
            }
        }
    }

    // flashcard
    for k in &c.cards {
        if k.front.trim().is_empty() || k.back.trim().is_empty() {
            r.warnings.push(format!("{}: fronte o retro vuoto", k.id));
        } else if k.front.trim().eq_ignore_ascii_case(k.back.trim()) {
            r.warnings.push(format!("{}: fronte e retro uguali", k.id));
        }
    }

    // libri: capitoli, rimandi alle sezioni, figure
    for b in &c.books {
        for ch in &b.chapters {
            let Some(md) = read(&ch.file) else {
                r.errors.push(format!("libro «{}»: manca il capitolo {}", b.title, ch.file));
                continue;
            };
            let secs = crate::markdown::sections(&crate::markdown::parse(&md));
            for st in &ch.sections {
                if crate::markdown::find_section(&secs, &st.section, None).is_none() {
                    r.warnings
                        .push(format!("libro «{}»: sezione «{}» non trovata in {}", b.title, st.section, ch.file));
                }
            }
            for (alt, path) in crate::contents::figure_refs(&md) {
                let what = if alt.is_empty() { "una figura".to_string() } else { format!("«{alt}»") };
                if path.is_empty() || !path.ends_with(".svg") {
                    r.warnings.push(format!("{}: {what} senza file SVG (risalva il capitolo)", ch.title));
                } else if read(&path).is_none() {
                    r.tips.push(format!("{}: manca la figura {what} ({path})", ch.title));
                }
            }
        }
    }
    for (id, br) in c
        .questions
        .iter()
        .filter_map(|q| q.book_ref.as_ref().map(|x| (&q.id, x)))
        .chain(c.cards.iter().filter_map(|k| k.book_ref.as_ref().map(|x| (&k.id, x))))
    {
        let Some(res) = c.resolve_ref(Some(br), "") else { continue };
        match read(&res.file) {
            None => r.errors.push(format!("{id}: rimanda a un capitolo che non c'è ({})", res.file)),
            Some(md) => {
                let secs = crate::markdown::sections(&crate::markdown::parse(&md));
                if (!res.section.is_empty() || res.page.is_some())
                    && crate::markdown::find_section(&secs, &res.section, res.page).is_none()
                {
                    r.warnings.push(format!("{id}: sezione «{}» non trovata in {}", res.section, res.file));
                }
            }
        }
    }

    // copertura: la barra di un argomento arriva in fondo solo con almeno 6 domande diverse
    for s in &syl.subjects {
        for u in &s.units {
            for t in &u.topics {
                let n = c.questions.iter().filter(|q| q.topic == t.id).count();
                if n == 0 {
                    r.tips.push(format!("{} › {}: nessuna domanda", u.name, t.name));
                } else if n < COVER {
                    r.tips.push(format!(
                        "{} › {}: solo {n} {} (ne servono {COVER} perché la barra arrivi in fondo)",
                        u.name,
                        t.name,
                        if n == 1 { "domanda" } else { "domande" }
                    ));
                }
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_problems_and_gaps() {
        let syl = r#"{"schema":1,"title":"T","exam":{"questions":3,"mcq":1,"fill":1,"options":5,"minutes":10,"correct":1,"wrong":0,"omitted":0,"pass":1},
            "subjects":[{"id":"fis","name":"Fisica","units":[{"id":"fis-u1","n":1,"name":"U1","cfu":1,"topics":[{"id":"fis-u1-t1","name":"Moto"},{"id":"fis-u1-t2","name":"Forze"},{"id":"fis-u1-t3","name":"Onde"}]}]}]}"#;
        let q = r#"{"schema":1,"questions":[
            {"id":"a","topic":"fis-u1-t1","type":"mcq","text":"Quanto fa due più due?","options":["4","4","5","6"],"answer":0,"explanation":""},
            {"id":"b","topic":"fis-u1-t1","type":"fill","text":"Unità della forza: ____","answers":["N"],"explanation":"newton","difficulty":7},
            {"id":"c","topic":"fis-u1-t3","type":"fill","text":"Unità della frequenza: ____","answers":["Hz"],"explanation":"hertz"}]}"#;
        let k = r#"{"schema":1,"cards":[{"id":"c1","topic":"fis-u1-t2","front":"F","back":"f"}]}"#;
        let books = r#"{"schema":1,"books":[{"id":"lib","subject":"fis","title":"Libro","chapters":[{"file":"books/x/1.md","title":"1 Moto","topics":[]}]}]}"#;
        let md = "# 1 Moto\n![Figura 1](books/x/fig/1.svg)\n![Figura 2]()\n";
        let c = Content::from_files([
            ("content/syllabus.json", syl),
            ("content/questions/q.json", q),
            ("content/flashcards/c.json", k),
            ("content/books.json", books),
        ]);
        let r = check(&c, &|p| (p == "books/x/1.md").then(|| md.to_string()));
        let all = format!("{r:?}");
        assert!(r.warnings.iter().any(|w| w.contains("a: 4 opzioni invece di 5")), "{all}");
        assert!(r.warnings.iter().any(|w| w.contains("a: opzioni ripetute")));
        assert!(r.warnings.iter().any(|w| w.contains("a: manca la spiegazione")));
        assert!(r.warnings.iter().any(|w| w.contains("b: difficoltà 7")));
        assert!(r.warnings.iter().any(|w| w.contains("c1: fronte e retro uguali")));
        assert!(r.warnings.iter().any(|w| w.contains("regole d'esame")));
        assert!(r.warnings.iter().any(|w| w.contains("senza file SVG")));
        assert!(r.tips.iter().any(|t| t.contains("manca la figura «Figura 1»")));
        assert!(r.tips.iter().any(|t| t.contains("Moto: solo 2 domande")));
        assert!(r.tips.iter().any(|t| t.contains("Onde: solo 1 domanda (")), "{all}");
        assert!(r.tips.iter().any(|t| t.contains("Forze: nessuna domanda")));
        assert!(r.summary().contains("avvisi"));
        let empty = check(&Content::default(), &|_| None);
        assert_eq!(empty.errors.len(), 1);
    }
}
