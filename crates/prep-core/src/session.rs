//! Sessioni di studio: è ciò che l'app committa in `progress/sessions/`.
//! Ogni sessione è un file nuovo e immutabile → nessun conflitto tra commit.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    /// allenamento per unità/materia con correzione immediata
    Practice,
    /// simulazione d'esame (31 quesiti, 50 minuti)
    Exam,
    /// ripasso delle domande insicure/sbagliate
    Review,
    /// ripasso flashcard
    Flashcards,
    /// solo cambi di segnalazione "insicura" fatti fuori da un quiz
    Flags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// id domanda o flashcard
    pub id: String,
    pub topic: String,
    /// "q" domanda, "card" flashcard
    pub kind: String,
    /// risposta data ("B", testo digitato) — None = omessa
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub given: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correct: Option<bool>,
    /// segnalata come "insicura" durante la risposta
    #[serde(default)]
    pub unsure: bool,
    /// voto flashcard: 0 di nuovo, 1 difficile, 2 bene, 3 facile
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<u8>,
    /// per le sessioni `flags`: true = segnala, false = rimuovi segnalazione
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<bool>,
    #[serde(default)]
    pub secs: u32,
    /// flashcard: spiegazione scritta a parole prima di girare la carta
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Score {
    pub correct: u32,
    pub wrong: u32,
    pub omitted: u32,
    pub unsure: u32,
    pub points: f64,
    pub max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub schema: u32,
    pub id: String,
    pub kind: SessionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub device: String,
    pub items: Vec<Item>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<Score>,
}

impl Session {
    pub fn new_id(kind: SessionKind, subject: Option<&str>, at: DateTime<Utc>) -> String {
        let k = match kind {
            SessionKind::Practice => "practice",
            SessionKind::Exam => "exam",
            SessionKind::Review => "review",
            SessionKind::Flashcards => "cards",
            SessionKind::Flags => "flags",
        };
        format!("{}-{}-{}-{:04x}", at.format("%Y%m%dT%H%M%SZ"), subject.unwrap_or("mix"), k, fastrand::u16(..))
    }

    /// percorso nel repository
    pub fn repo_path(&self) -> String {
        format!("progress/sessions/{}/{}.json", self.started_at.format("%Y-%m"), self.id)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serializzazione sessione") + "\n"
    }

    /// messaggio di commit leggibile
    pub fn commit_message(&self, subject_name: Option<&str>) -> String {
        let who = subject_name.unwrap_or("misto");
        match (&self.kind, &self.score) {
            (SessionKind::Exam, Some(s)) => format!(
                "simulazione {who}: {:.1}/31 ({} ok, {} err, {} omesse, {} insicure)",
                s.points, s.correct, s.wrong, s.omitted, s.unsure
            ),
            (SessionKind::Practice | SessionKind::Review, Some(s)) => format!(
                "{} {who}: {}/{} corrette, {} insicure",
                if self.kind == SessionKind::Review { "ripasso" } else { "quiz" },
                s.correct,
                s.correct + s.wrong + s.omitted,
                s.unsure
            ),
            (SessionKind::Flashcards, _) => {
                let unsure = self.items.iter().filter(|i| i.unsure).count();
                format!("flashcard {who}: {} ripassate, {} insicure", self.items.len(), unsure)
            }
            (SessionKind::Flags, _) => {
                let on = self.items.iter().filter(|i| i.flag == Some(true)).count();
                let off = self.items.iter().filter(|i| i.flag == Some(false)).count();
                format!("segnalazioni: +{on} insicure, -{off} risolte")
            }
            _ => format!("sessione {}", self.id),
        }
    }
}

/// Calcola il punteggio con le regole d'esame (+1 / -0.1 / 0).
pub fn score_items(items: &[Item], correct_pts: f64, wrong_pts: f64, omitted_pts: f64) -> Score {
    let mut s = Score::default();
    for it in items {
        match (it.given.as_ref(), it.correct) {
            (None, _) => s.omitted += 1,
            (Some(_), Some(true)) => s.correct += 1,
            _ => s.wrong += 1,
        }
        if it.unsure {
            s.unsure += 1;
        }
    }
    s.points = s.correct as f64 * correct_pts + s.wrong as f64 * wrong_pts + s.omitted as f64 * omitted_pts;
    s.max = items.len() as f64 * correct_pts;
    s
}
