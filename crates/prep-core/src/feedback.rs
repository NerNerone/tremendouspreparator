//! Feedback scritto dall'app (`progress/feedback/AAAA-MM/<id>.json`, immutabile)
//! e risposte di Claude (`progress/feedback_status.json`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub schema: u32,
    pub id: String,
    pub created_at: DateTime<Utc>,
    /// "session" (fine quiz/simulazione), "general" (scheda Feedback), "question" (segnalazione su una domanda)
    pub kind: String,
    /// contenuti | domanda | app | altro
    #[serde(default)]
    pub category: String,
    /// sessione a cui si riferisce
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// es. "simulazione Fisica: 19,4/31" — contesto leggibile
    #[serde(default)]
    pub context: String,
    /// domanda/flashcard segnalata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// gradimento 1..5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<u8>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub app_version: String,
}

impl Feedback {
    pub fn new_id(at: DateTime<Utc>) -> String {
        format!("{}-fb-{:04x}", at.format("%Y%m%dT%H%M%SZ"), fastrand::u16(..))
    }

    pub fn repo_path(&self) -> String {
        format!("progress/feedback/{}/{}.json", self.created_at.format("%Y-%m"), self.id)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serializzazione feedback") + "\n"
    }

    pub fn commit_message(&self) -> String {
        let stars = self.rating.map(|r| format!(" {r}/5")).unwrap_or_default();
        let head: String = self.text.chars().take(60).collect();
        let tail = if self.text.chars().count() > 60 { "…" } else { "" };
        format!("feedback {}{stars}: {head}{tail}", if self.category.is_empty() { &self.kind } else { &self.category })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackReply {
    /// letto | in lavorazione | integrato | non applicabile
    pub status: String,
    #[serde(default)]
    pub reply: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackStatus {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub items: BTreeMap<String, FeedbackReply>,
}
