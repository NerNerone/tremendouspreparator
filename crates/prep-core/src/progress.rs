//! Modello di stima dei progressi.
//!
//! Tutto è derivato dalle sessioni salvate in `progress/sessions/` (stesse regole
//! nell'app e nel tool `report`). La revisione AI (`progress/assessment.json`,
//! scritta da Claude su richiesta) fornisce un "ancoraggio" per argomento: dopo la
//! revisione la stima riparte da quel valore e si aggiorna con le sole nuove prove.
//! Descrizione completa in `docs/MODELLO_PROGRESSI.md`.

use crate::content::Content;
use crate::session::{Session, SessionKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// peso (in "numero di prove") dell'ancoraggio AI
pub const ANCHOR_WEIGHT: f64 = 4.0;
/// quante domande diverse servono per considerare "coperto" un argomento
pub const COVERAGE_TARGET: f64 = 6.0;
/// una flashcard vale come mezza domanda
pub const CARD_WEIGHT: f64 = 0.5;
const RECENCY: [f64; 3] = [0.6, 0.3, 0.1];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Note {
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicAssessment {
    /// padronanza 0..1 stimata dalla revisione
    pub mastery: f64,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Priority {
    pub topic: String,
    #[serde(default)]
    pub reason: String,
}

/// `progress/assessment.json` — scritto da Claude quando chiedi una revisione.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment {
    pub schema: u32,
    pub generated_at: DateTime<Utc>,
    #[serde(default)]
    pub generated_by: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub subjects: BTreeMap<String, Note>,
    #[serde(default)]
    pub units: BTreeMap<String, Note>,
    #[serde(default)]
    pub topics: BTreeMap<String, TopicAssessment>,
    #[serde(default)]
    pub priorities: Vec<Priority>,
    #[serde(default)]
    pub plan: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Attempt {
    pub at: DateTime<Utc>,
    pub score: f64,
    pub grade: Option<u8>,
    pub unsure: bool,
    /// secondi impiegati (0 = non misurato)
    pub secs: u32,
}

/// Storia ricostruita da tutte le sessioni.
#[derive(Debug, Default, Clone)]
pub struct History {
    pub questions: HashMap<String, Vec<Attempt>>,
    pub cards: HashMap<String, Vec<Attempt>>,
    /// domande/flashcard attualmente segnalate come insicure
    pub flagged: HashSet<String>,
    /// domande la cui ultima risposta è sbagliata o omessa
    pub last_wrong: HashSet<String>,
    pub sessions: usize,
    pub last_activity: Option<DateTime<Utc>>,
}

pub fn card_grade_score(grade: u8) -> f64 {
    match grade {
        0 => 0.0,
        1 => 0.4,
        2 => 0.8,
        _ => 1.0,
    }
}

pub fn build_history(sessions: &[Session]) -> History {
    let mut sorted: Vec<&Session> = sessions.iter().collect();
    sorted.sort_by_key(|s| (s.ended_at, s.id.clone()));
    let mut h = History { sessions: sorted.len(), ..Default::default() };
    for s in sorted {
        h.last_activity = Some(h.last_activity.map_or(s.ended_at, |t| t.max(s.ended_at)));
        for it in &s.items {
            if s.kind == SessionKind::Flags {
                match it.flag {
                    Some(true) => {
                        h.flagged.insert(it.id.clone());
                    }
                    Some(false) => {
                        h.flagged.remove(&it.id);
                    }
                    None => {}
                }
                continue;
            }
            if it.kind == "card" {
                let g = it.grade.unwrap_or(0);
                let mut sc = card_grade_score(g);
                if it.unsure {
                    sc = sc.min(0.5);
                }
                h.cards.entry(it.id.clone()).or_default().push(Attempt {
                    at: s.ended_at,
                    score: sc,
                    grade: Some(g),
                    unsure: it.unsure,
                    secs: it.secs,
                });
                if it.unsure {
                    h.flagged.insert(it.id.clone());
                } else if g >= 2 {
                    h.flagged.remove(&it.id);
                }
            } else {
                let correct = it.correct == Some(true) && it.given.is_some();
                let sc = if correct {
                    if it.unsure {
                        0.5
                    } else {
                        1.0
                    }
                } else {
                    0.0
                };
                h.questions.entry(it.id.clone()).or_default().push(Attempt {
                    at: s.ended_at,
                    score: sc,
                    grade: None,
                    unsure: it.unsure,
                    secs: it.secs,
                });
                if it.unsure {
                    h.flagged.insert(it.id.clone());
                } else if correct {
                    h.flagged.remove(&it.id);
                }
                if correct {
                    h.last_wrong.remove(&it.id);
                } else {
                    h.last_wrong.insert(it.id.clone());
                }
            }
        }
    }
    h
}

/// forza di un elemento: media pesata delle ultime 3 prove (più recente = più peso)
pub fn strength(attempts: &[Attempt]) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for (k, a) in attempts.iter().rev().take(3).enumerate() {
        num += RECENCY[k] * a.score;
        den += RECENCY[k];
    }
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TopicProgress {
    pub id: String,
    pub name: String,
    /// valore mostrato (0..1)
    pub mastery: f64,
    /// stima automatica pura (senza ancoraggio AI)
    pub auto: f64,
    pub accuracy: f64,
    pub coverage: f64,
    /// presente se la revisione AI ha dato un valore per questo argomento
    pub ai: Option<f64>,
    pub ai_note: String,
    pub questions: u32,
    pub cards: u32,
    pub seen_questions: u32,
    pub seen_cards: u32,
    pub attempts: u32,
    pub flagged: u32,
    pub wrong: u32,
    /// tempo medio per domanda (secondi), sulle risposte cronometrate
    pub avg_secs: f64,
    pub timed: u32,
    /// tempo totale cumulato (domande + flashcard), in secondi
    pub total_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnitProgress {
    pub id: String,
    pub n: u32,
    pub name: String,
    pub cfu: f64,
    pub mastery: f64,
    /// frazione di argomenti con almeno una prova
    pub touched: f64,
    pub questions: u32,
    pub cards: u32,
    pub flagged: u32,
    pub ai_note: String,
    pub avg_secs: f64,
    pub timed: u32,
    pub total_secs: u64,
    pub topics: Vec<TopicProgress>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectProgress {
    pub id: String,
    pub name: String,
    pub mastery: f64,
    /// punteggio atteso in simulazione (su 31)
    pub predicted: f64,
    pub flagged: u32,
    pub ai_note: String,
    pub avg_secs: f64,
    pub timed: u32,
    pub total_secs: u64,
    pub units: Vec<UnitProgress>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub overall: f64,
    /// tempo totale di studio cronometrato (secondi)
    pub total_secs: u64,
    pub avg_secs: f64,
    pub subjects: Vec<SubjectProgress>,
    pub anchored_at: Option<DateTime<Utc>>,
}

/// ritmo d'esame: secondi disponibili per domanda (50 min / 31 = ~97 s)
pub fn exam_pace(minutes: u32, questions: usize) -> f64 {
    minutes as f64 * 60.0 / questions.max(1) as f64
}

pub fn predicted_points(mastery: f64, n: usize, wrong_pts: f64) -> f64 {
    // si assume di rispondere a ciò che si sa e a metà di ciò che non si sa
    let n = n as f64;
    (n * mastery + n * (1.0 - mastery) * 0.5 * wrong_pts).clamp(0.0, n)
}

pub fn compute(content: &Content, h: &History, assessment: Option<&Assessment>) -> Progress {
    let syl = content.syllabus();
    let mut q_by_topic: HashMap<&str, Vec<&str>> = HashMap::new();
    for q in &content.questions {
        q_by_topic.entry(q.topic.as_str()).or_default().push(q.id.as_str());
    }
    let mut c_by_topic: HashMap<&str, Vec<&str>> = HashMap::new();
    for c in &content.cards {
        c_by_topic.entry(c.topic.as_str()).or_default().push(c.id.as_str());
    }
    let anchor_time = assessment.map(|a| a.generated_at);

    let mut subjects = Vec::new();
    for sub in &syl.subjects {
        let mut units = Vec::new();
        let mut sub_flagged = 0;
        for unit in &sub.units {
            let mut topics = Vec::new();
            for t in &unit.topics {
                let qs = q_by_topic.get(t.id.as_str()).cloned().unwrap_or_default();
                let cs = c_by_topic.get(t.id.as_str()).cloned().unwrap_or_default();
                let mut num = 0.0;
                let mut seen_w = 0.0;
                let mut seen_q = 0;
                let mut seen_c = 0;
                let mut attempts = 0;
                let mut flagged = 0;
                let mut wrong = 0;
                let mut t_sum = 0u64;
                let mut t_n = 0u32;
                let mut total = 0u64;
                // evidenza successiva all'ancoraggio
                let mut new_num = 0.0;
                let mut new_w = 0.0;
                for id in &qs {
                    if h.flagged.contains(*id) {
                        flagged += 1;
                    }
                    if h.last_wrong.contains(*id) {
                        wrong += 1;
                    }
                    if let Some(a) = h.questions.get(*id) {
                        for x in a.iter().filter(|x| x.secs > 0 && x.secs < 1800) {
                            t_sum += x.secs as u64;
                            t_n += 1;
                        }
                        total += a.iter().map(|x| x.secs.min(1800) as u64).sum::<u64>();
                        seen_q += 1;
                        attempts += a.len() as u32;
                        num += strength(a);
                        seen_w += 1.0;
                        if let Some(at) = anchor_time {
                            for x in a.iter().filter(|x| x.at > at) {
                                new_num += x.score;
                                new_w += 1.0;
                            }
                        }
                    }
                }
                for id in &cs {
                    if h.flagged.contains(*id) {
                        flagged += 1;
                    }
                    if let Some(a) = h.cards.get(*id) {
                        total += a.iter().map(|x| x.secs.min(600) as u64).sum::<u64>();
                        seen_c += 1;
                        attempts += a.len() as u32;
                        num += CARD_WEIGHT * strength(a);
                        seen_w += CARD_WEIGHT;
                        if let Some(at) = anchor_time {
                            for x in a.iter().filter(|x| x.at > at) {
                                new_num += CARD_WEIGHT * x.score;
                                new_w += CARD_WEIGHT;
                            }
                        }
                    }
                }
                let bank = qs.len() as f64 + CARD_WEIGHT * cs.len() as f64;
                let accuracy = if seen_w > 0.0 { num / seen_w } else { 0.0 };
                let target = COVERAGE_TARGET.min(bank);
                let coverage = if target > 0.0 { (seen_w / target).min(1.0) } else { 0.0 };
                let auto = accuracy * coverage;
                let ta = assessment.and_then(|a| a.topics.get(&t.id));
                let mastery = match ta {
                    Some(ta) => (ANCHOR_WEIGHT * ta.mastery.clamp(0.0, 1.0) + new_num) / (ANCHOR_WEIGHT + new_w),
                    None => auto,
                };
                sub_flagged += flagged;
                topics.push(TopicProgress {
                    id: t.id.clone(),
                    name: t.name.clone(),
                    mastery,
                    auto,
                    accuracy,
                    coverage,
                    ai: ta.map(|x| x.mastery),
                    ai_note: ta.map(|x| x.note.clone()).unwrap_or_default(),
                    questions: qs.len() as u32,
                    cards: cs.len() as u32,
                    seen_questions: seen_q,
                    seen_cards: seen_c,
                    attempts,
                    flagged,
                    wrong,
                    avg_secs: if t_n > 0 { t_sum as f64 / t_n as f64 } else { 0.0 },
                    timed: t_n,
                    total_secs: total,
                });
            }
            let n = topics.len().max(1) as f64;
            let mastery = topics.iter().map(|t| t.mastery).sum::<f64>() / n;
            let u_timed: u32 = topics.iter().map(|t| t.timed).sum();
            let u_avg = if u_timed > 0 {
                topics.iter().map(|t| t.avg_secs * t.timed as f64).sum::<f64>() / u_timed as f64
            } else {
                0.0
            };
            let touched = topics.iter().filter(|t| t.attempts > 0 || t.ai.is_some()).count() as f64 / n;
            units.push(UnitProgress {
                id: unit.id.clone(),
                n: unit.n,
                name: unit.name.clone(),
                cfu: unit.cfu,
                mastery,
                touched,
                questions: topics.iter().map(|t| t.questions).sum(),
                cards: topics.iter().map(|t| t.cards).sum(),
                flagged: topics.iter().map(|t| t.flagged).sum(),
                ai_note: assessment.and_then(|a| a.units.get(&unit.id)).map(|n| n.note.clone()).unwrap_or_default(),
                avg_secs: u_avg,
                timed: u_timed,
                total_secs: topics.iter().map(|t| t.total_secs).sum(),
                topics,
            });
        }
        let cfu: f64 = units.iter().map(|u| u.cfu).sum();
        let mastery = if cfu > 0.0 { units.iter().map(|u| u.cfu * u.mastery).sum::<f64>() / cfu } else { 0.0 };
        let s_timed: u32 = units.iter().map(|u| u.timed).sum();
        let s_avg = if s_timed > 0 {
            units.iter().map(|u| u.avg_secs * u.timed as f64).sum::<f64>() / s_timed as f64
        } else {
            0.0
        };
        subjects.push(SubjectProgress {
            id: sub.id.clone(),
            name: sub.name.clone(),
            mastery,
            predicted: predicted_points(mastery, syl.exam.questions, syl.exam.wrong),
            flagged: sub_flagged,
            ai_note: assessment.and_then(|a| a.subjects.get(&sub.id)).map(|n| n.note.clone()).unwrap_or_default(),
            avg_secs: s_avg,
            timed: s_timed,
            total_secs: units.iter().map(|u| u.total_secs).sum(),
            units,
        });
    }
    let overall =
        if subjects.is_empty() { 0.0 } else { subjects.iter().map(|s| s.mastery).sum::<f64>() / subjects.len() as f64 };
    let timed: u32 = subjects.iter().map(|s| s.timed).sum();
    let avg_secs =
        if timed > 0 { subjects.iter().map(|s| s.avg_secs * s.timed as f64).sum::<f64>() / timed as f64 } else { 0.0 };
    let total_secs = subjects.iter().map(|s| s.total_secs).sum();
    Progress { overall, total_secs, avg_secs, subjects, anchored_at: anchor_time }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Item, Session, SessionKind};
    use chrono::TimeZone;

    fn content() -> Content {
        let syl = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/syllabus.json")).unwrap();
        let q = r#"{"schema":1,"questions":[
          {"id":"a","topic":"fis-u1-t1","type":"mcq","text":"?","options":["x","y"],"answer":0},
          {"id":"b","topic":"fis-u1-t1","type":"mcq","text":"?","options":["x","y"],"answer":0}]}"#;
        Content::from_files([("content/syllabus.json", syl.as_str()), ("content/questions/t.json", q)])
    }

    fn session(at: i64, items: Vec<(&str, bool, bool)>) -> Session {
        let t = Utc.timestamp_opt(1_790_000_000 + at, 0).unwrap();
        Session {
            schema: 1,
            id: format!("s{at}"),
            kind: SessionKind::Practice,
            subject: None,
            unit: None,
            started_at: t,
            ended_at: t,
            app_version: String::new(),
            device: String::new(),
            items: items
                .into_iter()
                .map(|(id, ok, unsure)| Item {
                    id: id.into(),
                    topic: "fis-u1-t1".into(),
                    kind: "q".into(),
                    given: Some("A".into()),
                    correct: Some(ok),
                    unsure,
                    grade: None,
                    flag: None,
                    secs: 0,
                    note: None,
                })
                .collect(),
            score: None,
        }
    }

    fn topic(p: &Progress) -> &TopicProgress {
        &p.subjects[0].units[0].topics[0]
    }

    #[test]
    fn unsure_counts_half_and_flags() {
        let c = content();
        let h = build_history(&[session(0, vec![("a", true, true), ("b", true, false)])]);
        assert!(h.flagged.contains("a"));
        let p = compute(&c, &h, None);
        // accuratezza (0.5+1)/2, copertura 2/2
        assert!((topic(&p).mastery - 0.75).abs() < 1e-9);
        // risposta sicura e corretta toglie la segnalazione
        let h = build_history(&[session(0, vec![("a", true, true)]), session(10, vec![("a", true, false)])]);
        assert!(!h.flagged.contains("a"));
    }

    #[test]
    fn ai_anchor_then_updates() {
        let c = content();
        let s0 = session(0, vec![("a", false, false)]);
        let a = Assessment {
            schema: 1,
            generated_at: Utc.timestamp_opt(1_790_000_005, 0).unwrap(),
            generated_by: String::new(),
            summary: String::new(),
            subjects: Default::default(),
            units: Default::default(),
            topics: [("fis-u1-t1".to_string(), TopicAssessment { mastery: 0.6, note: String::new() })].into(),
            priorities: vec![],
            plan: vec![],
        };
        let p = compute(&c, &build_history(std::slice::from_ref(&s0)), Some(&a));
        assert!((topic(&p).mastery - 0.6).abs() < 1e-9);
        let p = compute(&c, &build_history(&[s0, session(10, vec![("a", true, false)])]), Some(&a));
        assert!((topic(&p).mastery - (4.0 * 0.6 + 1.0) / 5.0).abs() < 1e-9);
    }
}
