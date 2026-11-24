//! Obiettivi del giorno: quali argomenti studiare oggi e con che ritmo, dati i progressi e i
//! giorni che mancano all'appello. L'utente conferma (o modifica) la proposta; gli obiettivi
//! confermati sono salvati in `goals.json` e il loro avanzamento si calcola dallo storico.
//!
//! Sincronizzazione tra dispositivi: ogni modifica degli obiettivi crea anche un file nuovo in
//! `progress/goals/<giorno>/` (i commit dell'app creano solo file, mai li sovrascrivono); ogni
//! dispositivo usa la versione con `updated_at` più recente tra la sua e quelle scaricate.
//! Cancellare gli obiettivi registra una versione con `topics` vuoto.

use crate::content::Content;
use crate::progress::{exam_pace, History, Progress};
use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const GOALS_FILE: &str = "goals.json";
/// Cartella del repository con le versioni degli obiettivi.
pub const GOALS_DIR: &str = "progress/goals";
/// Tentativi per argomento che vorremmo aver fatto prima dell'esame.
const TARGET_ATTEMPTS: u32 = 15;
/// Giorni finali lasciati alle sole simulazioni.
const SIM_DAYS: i64 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub topic: String,
    pub name: String,
    pub subject: String,
    pub unit: String,
    pub score: f64,
    pub reasons: Vec<String>,
    /// domande consigliate oggi su questo argomento
    pub target: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DayPlan {
    pub days_left: i64,
    pub questions: u32,
    pub minutes: u32,
    pub cards_due: usize,
    pub note: String,
    pub suggestions: Vec<Suggestion>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Goals {
    /// "AAAA-MM-GG", giorno locale
    pub date: String,
    pub topics: Vec<String>,
    pub targets: Vec<u32>,
    #[serde(default)]
    pub confirmed_at: Option<DateTime<chrono::Utc>>,
    /// ultima modifica (per scegliere la versione più recente tra i dispositivi)
    #[serde(default)]
    pub updated_at: Option<DateTime<chrono::Utc>>,
}

impl Goals {
    pub fn load(dir: &Path) -> Option<Goals> {
        serde_json::from_str(&crate::vfs::read_to_string(&dir.join(GOALS_FILE)).ok()?).ok()
    }
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        crate::vfs::write(&dir.join(GOALS_FILE), serde_json::to_string_pretty(self)?.as_bytes())
    }
    pub fn is_for(&self, day: NaiveDate) -> bool {
        self.date == day.format("%Y-%m-%d").to_string()
    }

    fn stamp(&self) -> Option<DateTime<chrono::Utc>> {
        self.updated_at.or(self.confirmed_at)
    }

    /// Versione più recente per `day` tra quella locale e quelle sincronizzate. Può avere
    /// `topics` vuoto: obiettivi cancellati.
    pub fn latest(store: &crate::store::Store, day: NaiveDate) -> Option<Goals> {
        let mut best = Goals::load(&store.root).filter(|g| g.is_for(day));
        let dir = store.mirror_path(&format!("{GOALS_DIR}/{}", day.format("%Y-%m-%d")))?;
        for p in crate::vfs::list_dir(&dir) {
            let Ok(t) = crate::vfs::read_to_string(&p) else {
                continue;
            };
            let Ok(g) = serde_json::from_str::<Goals>(&t) else {
                continue;
            };
            if g.is_for(day) && best.as_ref().is_none_or(|b| g.stamp() > b.stamp()) {
                best = Some(g);
            }
        }
        best
    }

    /// Salva in locale e mette in coda il file per gli altri dispositivi.
    pub fn record(&mut self, store: &crate::store::Store) -> std::io::Result<()> {
        let now = chrono::Utc::now();
        self.updated_at = Some(now);
        self.save(&store.root)?;
        let id = format!("goals-{}", now.format("%Y%m%dT%H%M%S%3fZ"));
        let path = format!("{GOALS_DIR}/{}/{}.json", self.date, now.format("%Y%m%dT%H%M%S%3fZ"));
        let msg = if self.topics.is_empty() {
            format!("obiettivi {}: cancellati", self.date)
        } else {
            format!("obiettivi {}: {} argomenti", self.date, self.topics.len())
        };
        store.record_file(&id, path, serde_json::to_string_pretty(self)?.into_bytes(), &msg)
    }

    /// Obiettivi cancellati per il giorno `date`.
    pub fn cleared(date: &str) -> Goals {
        Goals { date: date.to_string(), ..Default::default() }
    }
}

/// Domande a cui hai risposto `day` (ora locale), per argomento.
pub fn done_on(content: &Content, h: &History, day: NaiveDate) -> HashMap<String, u32> {
    let mut out: HashMap<String, u32> = HashMap::new();
    for (qid, atts) in &h.questions {
        let Some(q) = content.question(qid) else {
            continue;
        };
        let n = atts.iter().filter(|a| a.at.with_timezone(&Local).date_naive() == day).count() as u32;
        if n > 0 {
            *out.entry(q.topic.clone()).or_default() += n;
        }
    }
    out
}

fn last_seen(content: &Content, h: &History) -> HashMap<String, NaiveDate> {
    let mut out: HashMap<String, NaiveDate> = HashMap::new();
    for (qid, atts) in &h.questions {
        let Some(q) = content.question(qid) else {
            continue;
        };
        if let Some(d) = atts.iter().map(|a| a.at.with_timezone(&Local).date_naive()).max() {
            let e = out.entry(q.topic.clone()).or_insert(d);
            if d > *e {
                *e = d;
            }
        }
    }
    out
}

/// Argomento da ripassare: già studiato ma non più toccato da qualche giorno.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewDue {
    pub topic: String,
    pub name: String,
    pub subject: String,
    /// giorni dall'ultima domanda sull'argomento
    pub days: i64,
    pub mastery: f64,
    /// domande consigliate per il ripasso
    pub target: u32,
}

/// Ripasso distanziato: argomenti su cui hai già lavorato ma che non tocchi da almeno
/// `min_days` giorni. Prima quelli fermi da più tempo e meno padroneggiati.
pub fn review_due(
    content: &Content,
    progress: &Progress,
    h: &History,
    today: NaiveDate,
    min_days: i64,
    max: usize,
) -> Vec<ReviewDue> {
    let seen = last_seen(content, h);
    let mut out: Vec<(f64, ReviewDue)> = Vec::new();
    for s in &progress.subjects {
        for u in &s.units {
            for t in &u.topics {
                let Some(&last) = seen.get(&t.id) else {
                    continue;
                };
                let days = (today - last).num_days();
                if days < min_days {
                    continue;
                }
                // la memoria decade col tempo: più giorni e meno padronanza = più urgente
                let urgency = days.min(30) as f64 * (1.2 - t.mastery.clamp(0.0, 1.0));
                let target = if t.mastery < 0.5 { 10 } else { 6 };
                out.push((
                    urgency,
                    ReviewDue {
                        topic: t.id.clone(),
                        name: t.name.clone(),
                        subject: s.name.clone(),
                        days,
                        mastery: t.mastery,
                        target,
                    },
                ));
            }
        }
    }
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    out.into_iter().take(max).map(|(_, r)| r).collect()
}

fn mmss(secs: f64) -> String {
    let s = secs.round() as u64;
    if s >= 60 {
        format!("{}:{:02}", s / 60, s % 60)
    } else {
        format!("{s} s")
    }
}

/// Proposta per `today`. `n` = numero di argomenti da suggerire.
pub fn suggest(
    content: &Content,
    progress: &Progress,
    h: &History,
    today: NaiveDate,
    exam: Option<(&str, NaiveDate)>,
    cards_due: usize,
    n: usize,
) -> DayPlan {
    let rules = &content.syllabus().exam;
    let pace = exam_pace(rules.minutes, rules.questions);
    let seen = last_seen(content, h);
    let max_cfu = progress.subjects.iter().flat_map(|s| s.units.iter().map(|u| u.cfu)).fold(1.0_f64, f64::max);
    let mut cands: Vec<Suggestion> = Vec::new();
    let mut needed: u32 = 0;
    for s in &progress.subjects {
        for u in &s.units {
            for t in &u.topics {
                if t.questions == 0 {
                    continue;
                }
                needed += TARGET_ATTEMPTS.saturating_sub(t.attempts);
                if t.mastery < 0.6 {
                    needed += 5;
                }
                let mut score = (1.0 - t.mastery) + 0.3 * u.cfu / max_cfu;
                let mut reasons = Vec::new();
                if t.seen_questions == 0 {
                    score += 0.4;
                    reasons.push("mai affrontato".to_string());
                } else {
                    reasons.push(format!("padronanza {:.0}%", t.mastery * 100.0));
                }
                if t.timed >= 3 && t.avg_secs > pace * 1.3 {
                    score += 0.25;
                    reasons.push(format!("lento: {}/domanda", mmss(t.avg_secs)));
                }
                let trouble = (t.flagged + t.wrong).min(5);
                if trouble > 0 {
                    score += 0.05 * trouble as f64;
                    reasons.push(format!("{trouble} da rivedere"));
                }
                match seen.get(&t.id).map(|d| (today - *d).num_days()) {
                    Some(d) if d <= 1 => score -= 0.3,
                    Some(d) if d >= 7 => {
                        score += 0.15;
                        reasons.push(format!("non lo vedi da {d} giorni"));
                    }
                    _ => {}
                }
                cands.push(Suggestion {
                    topic: t.id.clone(),
                    name: t.name.clone(),
                    subject: s.name.clone(),
                    unit: format!("U{} {}", u.n, u.name),
                    score,
                    reasons,
                    target: 0,
                });
            }
        }
    }
    cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    // varietà: al più 2 argomenti per materia, finché è possibile
    let mut picked: Vec<Suggestion> = Vec::new();
    let mut per_subject: HashMap<String, usize> = HashMap::new();
    for c in &cands {
        if picked.len() >= n {
            break;
        }
        let k = per_subject.entry(c.subject.clone()).or_default();
        if *k < 2 {
            *k += 1;
            picked.push(c.clone());
        }
    }
    for c in &cands {
        if picked.len() >= n {
            break;
        }
        if !picked.iter().any(|p| p.topic == c.topic) {
            picked.push(c.clone());
        }
    }

    let days_left = exam.map(|(_, d)| (d - today).num_days()).unwrap_or(60);
    let study_days = (days_left - SIM_DAYS).max(1) as u32;
    let mut questions = needed.div_ceil(study_days).clamp(25, 150);
    let note;
    if let Some((label, _)) = exam {
        if days_left < 0 {
            note = "Appelli passati: ripasso libero.".to_string();
        } else if days_left <= SIM_DAYS {
            questions = 93;
            note = format!(
                "{} tra {days_left} giorni: oggi una simulazione per materia (31 domande, 50 minuti) e ripasso degli errori.",
                label
            );
        } else {
            note = format!(
                "Mancano {days_left} giorni al {label}: circa {questions} domande al giorno per arrivare a {TARGET_ATTEMPTS} tentativi \
                 su ogni argomento, lasciando gli ultimi {SIM_DAYS} giorni alle simulazioni."
            );
        }
    } else {
        note = format!("Circa {questions} domande al giorno.");
    }
    let per = if picked.is_empty() { 0 } else { (questions as usize).div_ceil(picked.len()) as u32 };
    for p in &mut picked {
        p.target = per.max(5);
    }
    let avg = if progress.avg_secs > 0.0 { progress.avg_secs } else { pace };
    let minutes = ((questions as f64 * avg + cards_due as f64 * 20.0) / 60.0).round() as u32;
    DayPlan { days_left, questions, minutes, cards_due, note, suggestions: picked }
}

#[cfg(test)]
mod sync_tests {
    use super::*;

    #[test]
    fn latest_wins_across_devices() {
        let dir = std::env::temp_dir().join(format!("goals-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = crate::store::Store::open(&dir).unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let mut local =
            Goals { date: "2026-11-11".into(), topics: vec!["a".into()], targets: vec![10], ..Default::default() };
        local.record(&store).unwrap();
        assert_eq!(Goals::latest(&store, day).unwrap().topics, vec!["a".to_string()]);
        // versione più recente arrivata da un altro dispositivo
        let other = Goals {
            date: "2026-11-11".into(),
            topics: vec!["b".into()],
            targets: vec![12],
            confirmed_at: None,
            updated_at: Some(local.updated_at.unwrap() + chrono::Duration::minutes(5)),
        };
        store
            .write_mirror("progress/goals/2026-11-11/x.json", serde_json::to_string(&other).unwrap().as_bytes())
            .unwrap();
        assert_eq!(Goals::latest(&store, day).unwrap().topics, vec!["b".to_string()]);
        // cancellazione locale successiva
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut gone = Goals::cleared("2026-11-11");
        gone.updated_at = Some(chrono::Utc::now() + chrono::Duration::minutes(10));
        store
            .write_mirror("progress/goals/2026-11-11/y.json", serde_json::to_string(&gone).unwrap().as_bytes())
            .unwrap();
        assert!(Goals::latest(&store, day).unwrap().topics.is_empty());
        assert!(store.outbox().iter().any(|(_, e)| e.path.starts_with("progress/goals/2026-11-11/")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
