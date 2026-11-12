//! Caricamento e indicizzazione dei contenuti (syllabus + banche domande/flashcard).

use crate::model::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TopicRef {
    pub subject: usize,
    pub unit: usize,
    pub topic: usize,
}

#[derive(Debug, Default, Clone)]
pub struct Content {
    pub syllabus: Option<Syllabus>,
    pub questions: Vec<Question>,
    pub cards: Vec<Flashcard>,
    pub books: Vec<Book>,
    q_index: HashMap<String, usize>,
    c_index: HashMap<String, usize>,
    topic_index: HashMap<String, TopicRef>,
    /// avvisi non bloccanti (file malformati, riferimenti a topic inesistenti, id duplicati)
    pub warnings: Vec<String>,
}

impl Content {
    /// `files`: coppie (percorso relativo al repo, testo JSON). Riconosce
    /// `content/syllabus.json`, `content/questions/**.json`, `content/flashcards/**.json`.
    pub fn from_files<'a, I>(files: I) -> Content
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut c = Content::default();
        let mut files: Vec<(&str, &str)> = files.into_iter().collect();
        files.sort_by(|a, b| a.0.cmp(b.0));
        for (path, text) in files {
            if !path.ends_with(".json") {
                continue;
            }
            if path == "content/syllabus.json" {
                match serde_json::from_str::<Syllabus>(text) {
                    Ok(s) => c.syllabus = Some(s),
                    Err(e) => c.warnings.push(format!("{path}: {e}")),
                }
            } else if path == "content/books.json" {
                match serde_json::from_str::<BookCatalog>(text) {
                    Ok(b) => c.books = b.books,
                    Err(e) => c.warnings.push(format!("{path}: {e}")),
                }
            } else if path.starts_with("content/questions/") {
                match serde_json::from_str::<QuestionFile>(text) {
                    Ok(f) => c.questions.extend(f.questions),
                    Err(e) => c.warnings.push(format!("{path}: {e}")),
                }
            } else if path.starts_with("content/flashcards/") {
                match serde_json::from_str::<FlashcardFile>(text) {
                    Ok(f) => c.cards.extend(f.cards),
                    Err(e) => c.warnings.push(format!("{path}: {e}")),
                }
            }
        }
        c.reindex();
        c
    }

    fn reindex(&mut self) {
        self.topic_index.clear();
        if let Some(s) = &self.syllabus {
            for (si, sub) in s.subjects.iter().enumerate() {
                for (ui, unit) in sub.units.iter().enumerate() {
                    for (ti, t) in unit.topics.iter().enumerate() {
                        self.topic_index.insert(t.id.clone(), TopicRef { subject: si, unit: ui, topic: ti });
                    }
                }
            }
        }
        // dedup: l'ultimo file vince
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut qs: Vec<Question> = Vec::new();
        for q in self.questions.drain(..) {
            if let Some(&i) = seen.get(&q.id) {
                self.warnings.push(format!("domanda duplicata: {}", q.id));
                qs[i] = q;
            } else {
                seen.insert(q.id.clone(), qs.len());
                qs.push(q);
            }
        }
        self.questions = qs;
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut cs: Vec<Flashcard> = Vec::new();
        for card in self.cards.drain(..) {
            if let Some(&i) = seen.get(&card.id) {
                self.warnings.push(format!("flashcard duplicata: {}", card.id));
                cs[i] = card;
            } else {
                seen.insert(card.id.clone(), cs.len());
                cs.push(card);
            }
        }
        self.cards = cs;
        for q in &self.questions {
            if self.syllabus.is_some() && !self.topic_index.contains_key(&q.topic) {
                self.warnings.push(format!("{}: topic sconosciuto {}", q.id, q.topic));
            }
            match q.kind {
                QKind::Mcq => {
                    if q.options.len() < 2 || q.answer.is_none_or(|a| a >= q.options.len()) {
                        self.warnings.push(format!("{}: mcq malformata", q.id));
                    }
                }
                QKind::Fill => {
                    if q.answers.is_empty() && q.numeric.is_none() {
                        self.warnings.push(format!("{}: fill senza risposte", q.id));
                    }
                }
            }
        }
        for card in &self.cards {
            if self.syllabus.is_some() && !self.topic_index.contains_key(&card.topic) {
                self.warnings.push(format!("{}: topic sconosciuto {}", card.id, card.topic));
            }
        }
        for b in &self.books {
            for ch in &b.chapters {
                for t in ch.topics.iter().chain(ch.sections.iter().flat_map(|s| s.topics.iter())) {
                    if self.syllabus.is_some() && !self.topic_index.contains_key(t) {
                        self.warnings.push(format!("libro {}: topic sconosciuto {}", b.id, t));
                    }
                }
            }
        }
        let refs = self
            .questions
            .iter()
            .filter_map(|q| q.book_ref.as_ref().map(|r| (q.id.clone(), r.clone())))
            .chain(self.cards.iter().filter_map(|c| c.book_ref.as_ref().map(|r| (c.id.clone(), r.clone()))))
            .collect::<Vec<_>>();
        for (id, r) in refs {
            if !self.books.iter().any(|b| b.id == r.book) {
                self.warnings.push(format!("{id}: libro sconosciuto {}", r.book));
            }
        }
        self.q_index = self.questions.iter().enumerate().map(|(i, q)| (q.id.clone(), i)).collect();
        self.c_index = self.cards.iter().enumerate().map(|(i, c)| (c.id.clone(), i)).collect();
    }

    pub fn syllabus(&self) -> &Syllabus {
        self.syllabus.as_ref().expect("syllabus mancante")
    }

    pub fn question(&self, id: &str) -> Option<&Question> {
        self.q_index.get(id).map(|&i| &self.questions[i])
    }

    pub fn card(&self, id: &str) -> Option<&Flashcard> {
        self.c_index.get(id).map(|&i| &self.cards[i])
    }

    pub fn topic_ref(&self, topic_id: &str) -> Option<&TopicRef> {
        self.topic_index.get(topic_id)
    }

    pub fn topic(&self, topic_id: &str) -> Option<&Topic> {
        let r = self.topic_index.get(topic_id)?;
        Some(&self.syllabus().subjects[r.subject].units[r.unit].topics[r.topic])
    }

    pub fn unit_of_topic(&self, topic_id: &str) -> Option<&Unit> {
        let r = self.topic_index.get(topic_id)?;
        Some(&self.syllabus().subjects[r.subject].units[r.unit])
    }

    pub fn subject_of_topic(&self, topic_id: &str) -> Option<&Subject> {
        let r = self.topic_index.get(topic_id)?;
        Some(&self.syllabus().subjects[r.subject])
    }

    pub fn subject(&self, id: &str) -> Option<&Subject> {
        self.syllabus.as_ref()?.subjects.iter().find(|s| s.id == id)
    }

    pub fn unit(&self, id: &str) -> Option<&Unit> {
        self.syllabus.as_ref()?.subjects.iter().flat_map(|s| s.units.iter()).find(|u| u.id == id)
    }

    pub fn questions_in_unit<'a>(&'a self, unit: &'a Unit) -> impl Iterator<Item = &'a Question> + 'a {
        self.questions.iter().filter(move |q| unit.topics.iter().any(|t| t.id == q.topic))
    }

    pub fn cards_in_unit<'a>(&'a self, unit: &'a Unit) -> impl Iterator<Item = &'a Flashcard> + 'a {
        self.cards.iter().filter(move |c| unit.topics.iter().any(|t| t.id == c.topic))
    }

    pub fn book(&self, id: &str) -> Option<&Book> {
        self.books.iter().find(|b| b.id == id)
    }

    /// Rimando "Se vuoi approfondire": quello esplicito, altrimenti il primo capitolo
    /// di un libro che tratta l'argomento.
    pub fn resolve_ref(&self, explicit: Option<&BookRef>, topic: &str) -> Option<BookRef> {
        if let Some(r) = explicit {
            let mut r = r.clone();
            if r.file.is_empty() {
                r.file = self.book(&r.book)?.chapters.first()?.file.clone();
            }
            return Some(r);
        }
        for b in &self.books {
            for ch in &b.chapters {
                if let Some(sec) = ch.sections.iter().find(|s| s.topics.iter().any(|t| t == topic)) {
                    return Some(BookRef {
                        book: b.id.clone(),
                        file: ch.file.clone(),
                        section: sec.section.clone(),
                        page: None,
                    });
                }
            }
        }
        for b in &self.books {
            for ch in &b.chapters {
                if ch.topics.iter().any(|t| t == topic) {
                    return Some(BookRef {
                        book: b.id.clone(),
                        file: ch.file.clone(),
                        section: String::new(),
                        page: None,
                    });
                }
            }
        }
        None
    }

    /// "Fisica — Giancoli · 9.3 Teorema di Bernoulli · p. 245"
    pub fn ref_label(&self, r: &BookRef) -> String {
        let Some(b) = self.book(&r.book) else {
            return r.book.clone();
        };
        let mut s = b.title.clone();
        let where_ = if !r.section.is_empty() {
            r.section.clone()
        } else {
            b.chapters.iter().find(|c| c.file == r.file).map(|c| c.title.clone()).unwrap_or_default()
        };
        if !where_.is_empty() {
            s.push_str(" · ");
            s.push_str(&where_);
        }
        if let Some(p) = r.page {
            s.push_str(&format!(" · p. {p}"));
        }
        s
    }

    pub fn questions_in_subject<'a>(&'a self, sid: &'a str) -> impl Iterator<Item = &'a Question> + 'a {
        self.questions.iter().filter(move |q| self.subject_of_topic(&q.topic).is_some_and(|s| s.id == sid))
    }
}
