//! Promemoria di studio: quattro fasce al giorno (mattina, dopo pranzo, pomeriggio, sera).
//!
//! L'app scrive `reminder.json` nella cartella dati con gli orari e, già pronti, titolo e
//! testo di ogni fascia per oggi e domani ([`ReminderState::compute_slots`]). Chi mostra la
//! notifica (il receiver Java su Android, il processo `--promemoria` sul desktop) all'orario
//! sceglie la fascia e usa quel testo, oppure un testo generico se l'app non è stata aperta
//! di recente.

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "reminder.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Paused {
    pub title: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DayCount {
    /// "AAAA-MM-GG"
    pub date: String,
    /// flashcard da ripassare entro la fine di quel giorno
    pub n: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalItem {
    pub name: String,
    pub done: u32,
    pub target: u32,
}

/// Obiettivi confermati per un giorno, con l'avanzamento al momento della scrittura.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalsState {
    pub date: String,
    pub items: Vec<GoalItem>,
}

impl GoalsState {
    pub fn complete(&self) -> bool {
        self.items.iter().all(|g| g.done >= g.target)
    }
    /// "Cinematica 4/10 · Acidi e basi 0/10"
    pub fn summary(&self) -> String {
        self.items
            .iter()
            .map(|g| format!("{} {}/{}", g.name, g.done.min(g.target), g.target))
            .collect::<Vec<_>>()
            .join(" · ")
    }
    pub fn missing(&self) -> String {
        self.items
            .iter()
            .filter(|g| g.done < g.target)
            .map(|g| format!("{} ({} domande)", g.name, g.target - g.done))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Testo pronto di una fascia. `title` vuoto = quel giorno, a quell'ora, non serve disturbare.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Slot {
    pub date: String,
    pub hour: u32,
    pub minute: u32,
    pub title: String,
    pub text: String,
}

pub const DEFAULT_TIMES: [&str; 4] = ["08:30", "13:45", "17:30", "21:00"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReminderState {
    pub enabled: bool,
    pub hour: u32,
    pub minute: u32,
    #[serde(default)]
    pub paused: Option<Paused>,
    #[serde(default)]
    pub due: Vec<DayCount>,
    /// ultimo giorno con almeno una sessione, "AAAA-MM-GG"
    #[serde(default)]
    pub last_study: String,
    #[serde(default)]
    pub exam_label: String,
    #[serde(default)]
    pub exam_date: String,
    /// orario del promemoria di fine giornata sugli obiettivi
    #[serde(default = "d_eh")]
    pub evening_hour: u32,
    #[serde(default = "d_em")]
    pub evening_minute: u32,
    #[serde(default)]
    pub goals: Option<GoalsState>,
    /// orari delle fasce, "HH:MM", in ordine
    #[serde(default)]
    pub times: Vec<String>,
    /// domande a cui hai risposto oggi (al momento della scrittura)
    #[serde(default)]
    pub studied_today: u32,
    #[serde(default)]
    pub slots: Vec<Slot>,
}

fn d_eh() -> u32 {
    21
}
fn d_em() -> u32 {
    30
}

/// "18:30" → (18, 30)
pub fn parse_time(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.trim().split_once([':', '.'])?;
    let (h, m) = (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

impl ReminderState {
    pub fn load(dir: &Path) -> Option<ReminderState> {
        serde_json::from_str(&std::fs::read_to_string(dir.join(FILE)).ok()?).ok()
    }

    /// Scrive il file solo se è cambiato (il receiver lo legge, niente scritture inutili).
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let j = serde_json::to_string_pretty(self)?;
        let p = dir.join(FILE);
        if std::fs::read_to_string(&p).ok().as_deref() == Some(j.as_str()) {
            return Ok(());
        }
        std::fs::write(p, j)
    }

    fn due_on(&self, day: &str) -> usize {
        // l'elenco copre i prossimi giorni; oltre, vale l'ultimo valore noto
        self.due.iter().rev().find(|d| d.date.as_str() <= day).or(self.due.first()).map_or(0, |d| d.n)
    }

    /// Titolo e testo della notifica per `today`, oppure None se non serve disturbare
    /// (oggi hai già studiato e non c'è niente in sospeso).
    pub fn message(&self, today: NaiveDate) -> Option<(String, String)> {
        if !self.enabled {
            return None;
        }
        let day = today.format("%Y-%m-%d").to_string();
        let due = self.due_on(&day);
        let exam = NaiveDate::parse_from_str(&self.exam_date, "%Y-%m-%d").ok().and_then(|d| {
            let n = (d - today).num_days();
            (n >= 0).then(|| match n {
                0 => format!("Oggi: {}!", self.exam_label),
                1 => format!("Domani: {}.", self.exam_label),
                n => format!("{n} giorni al {}.", self.exam_label),
            })
        });
        let tail = exam.map(|e| format!(" {e}")).unwrap_or_default();
        if let Some(p) = &self.paused {
            return Some((
                "Hai una sessione in pausa".into(),
                format!("{}: {}/{} risposte date. Riprendi da dove eri rimasto.{tail}", p.title, p.done, p.total),
            ));
        }
        if self.last_study == day {
            return (due > 0).then(|| ("Ripasso".into(), format!("Ti restano {due} flashcard da ripassare oggi.")));
        }
        let what = if due > 0 {
            format!("{due} flashcard da ripassare e un quiz di allenamento ti aspettano.")
        } else {
            "Un quiz di allenamento o una simulazione ti aspettano.".into()
        };
        Some(("È ora di studiare".into(), format!("{what}{tail}")))
    }

    /// Notifica di fine giornata: solo se ci sono obiettivi di oggi non completati.
    pub fn evening_message(&self, today: NaiveDate) -> Option<(String, String)> {
        if !self.enabled {
            return None;
        }
        let g = self.goals.as_ref()?;
        if g.date != today.format("%Y-%m-%d").to_string() || g.complete() {
            return None;
        }
        Some(("Obiettivi di oggi non finiti".into(), format!("Ti mancano: {}.", g.missing())))
    }

    fn exam_tail(&self, today: NaiveDate) -> String {
        NaiveDate::parse_from_str(&self.exam_date, "%Y-%m-%d")
            .ok()
            .and_then(|d| {
                let n = (d - today).num_days();
                (n >= 0).then(|| match n {
                    0 => format!(" Oggi: {}!", self.exam_label),
                    1 => format!(" Domani: {}.", self.exam_label),
                    n => format!(" {n} giorni al {}.", self.exam_label),
                })
            })
            .unwrap_or_default()
    }

    /// Testo della fascia `k` (0 = la prima del giorno, l'ultima = sera) su `n` fasce.
    /// `studied` = domande fatte quel giorno finora.
    pub fn slot_message(&self, day: NaiveDate, k: usize, n: usize, studied: u32) -> Option<(String, String)> {
        if !self.enabled {
            return None;
        }
        let ds = day.format("%Y-%m-%d").to_string();
        let due = self.due_on(&ds);
        let tail = self.exam_tail(day);
        let goals = self.goals.as_ref().filter(|g| g.date == ds);
        let cards = |pre: &str| {
            if due > 0 {
                format!("{pre}{due} flashcard da ripassare.")
            } else {
                String::new()
            }
        };
        let paused = self.paused.as_ref().map(|p| {
            (
                "Hai una sessione in pausa".to_string(),
                format!("{}: {}/{} risposte date. Riprendi da dove eri rimasto.{tail}", p.title, p.done, p.total),
            )
        });
        let last = k + 1 >= n;
        if k == 0 && n > 1 {
            if let Some(m) = paused {
                return Some(m);
            }
            return Some(match goals {
                Some(g) => {
                    ("Buongiorno! Obiettivi di oggi".into(), format!("{}.{}{tail}", g.summary(), cards(" Poi ")))
                }
                None => (
                    "Buongiorno!".into(),
                    format!("Scegli gli obiettivi di oggi nell'app.{}{tail}", cards(" Ci sono ")),
                ),
            });
        }
        if last {
            if let Some(g) = goals.filter(|g| !g.complete()) {
                return Some(("Obiettivi di oggi non finiti".into(), format!("Ti mancano: {}.", g.missing())));
            }
            if studied == 0 {
                return Some((
                    "Oggi non hai ancora studiato".into(),
                    format!("Bastano 10 minuti: un quiz breve o le flashcard.{}{tail}", cards(" Hai ")),
                ));
            }
            if goals.is_some() {
                return Some((
                    "Obiettivi di oggi completati".into(),
                    format!("Ottimo lavoro: {studied} domande oggi.{}", cards(" Prima di dormire: ")),
                ));
            }
            return (due > 0)
                .then(|| ("Prima di chiudere la giornata".into(), format!("{due} flashcard da ripassare.")));
        }
        // dopo pranzo / pomeriggio
        if let Some(m) = paused {
            return Some(m);
        }
        if let Some(g) = goals {
            if g.complete() {
                return (due > 0).then(|| ("Obiettivi completati".into(), cards("Se ti avanza tempo: ")));
            }
            return Some(("A che punto sei?".into(), format!("{}.", g.summary())));
        }
        Some(if studied == 0 {
            (
                "Pausa studio?".into(),
                format!(
                    "Ancora nessuna domanda oggi: un blocco da 10 domande richiede circa 15 minuti.{}{tail}",
                    cards(" Hai anche ")
                ),
            )
        } else {
            ("Continua così".into(), format!("{studied} domande oggi: un altro blocco da 10?{}", cards(" Hai anche ")))
        })
    }

    /// Prepara i testi di tutte le fasce per `today` e domani.
    pub fn compute_slots(&mut self, today: NaiveDate) {
        let times: Vec<(u32, u32)> = self.times.iter().filter_map(|t| parse_time(t)).collect();
        let mut slots = Vec::new();
        for (d, studied) in [(today, self.studied_today), (today + chrono::Days::new(1), 0)] {
            for (k, &(h, m)) in times.iter().enumerate() {
                let (title, text) = self.slot_message(d, k, times.len(), studied).unwrap_or_default();
                slots.push(Slot { date: d.format("%Y-%m-%d").to_string(), hour: h, minute: m, title, text });
            }
        }
        self.slots = slots;
    }

    /// Fascia da mostrare a `now`: l'ultima con orario già passato da meno di 3 ore.
    pub fn due_slot(&self, now: NaiveDateTime) -> Option<&Slot> {
        let ds = now.date().format("%Y-%m-%d").to_string();
        self.slots
            .iter()
            .filter(|s| s.date == ds)
            .filter_map(|s| {
                let t = now.date().and_time(NaiveTime::from_hms_opt(s.hour, s.minute, 0)?);
                (t <= now && (now - t).num_minutes() < 180).then_some((t, s))
            })
            .max_by_key(|(t, _)| *t)
            .map(|(_, s)| s)
    }

    /// Prossimo istante (ora locale) in cui mostrare il promemoria, dopo `now`.
    pub fn next_fire(&self, now: NaiveDateTime) -> Option<NaiveDateTime> {
        if !self.enabled {
            return None;
        }
        let t = NaiveTime::from_hms_opt(self.hour, self.minute, 0)?;
        let today = now.date().and_time(t);
        Some(if today > now { today } else { (now.date() + chrono::Days::new(1)).and_time(t) })
    }
}

/// Secondi da aspettare, in ora locale, fino al prossimo promemoria.
pub fn secs_until_next(state: &ReminderState) -> Option<u64> {
    let now = Local::now();
    let next = state.next_fire(now.naive_local())?;
    let next = Local.from_local_datetime(&next).earliest()?;
    Some((next - now).num_seconds().max(1) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> ReminderState {
        ReminderState {
            enabled: true,
            hour: 18,
            minute: 30,
            paused: None,
            due: vec![DayCount { date: "2026-11-10".into(), n: 3 }, DayCount { date: "2026-11-11".into(), n: 7 }],
            last_study: "2026-11-09".into(),
            exam_label: "1° appello".into(),
            exam_date: "2026-12-10".into(),
            evening_hour: 21,
            evening_minute: 30,
            goals: None,
            times: DEFAULT_TIMES.iter().map(|t| t.to_string()).collect(),
            studied_today: 0,
            slots: vec![],
        }
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn messages() {
        let mut s = st();
        let (t, m) = s.message(d("2026-11-10")).unwrap();
        assert_eq!(t, "È ora di studiare");
        assert!(m.starts_with("3 flashcard") && m.ends_with("30 giorni al 1° appello."), "{m}");
        // oltre l'elenco vale l'ultimo valore
        assert!(s.message(d("2026-11-20")).unwrap().1.starts_with("7 flashcard"));
        s.last_study = "2026-11-10".into();
        assert_eq!(s.message(d("2026-11-10")).unwrap().1, "Ti restano 3 flashcard da ripassare oggi.");
        s.due.clear();
        assert!(s.message(d("2026-11-10")).is_none());
        s.paused = Some(Paused { title: "Simulazione Fisica".into(), done: 12, total: 31 });
        assert!(s.message(d("2026-11-10")).unwrap().1.starts_with("Simulazione Fisica: 12/31"));
        s.enabled = false;
        assert!(s.message(d("2026-11-10")).is_none());
    }

    #[test]
    fn evening() {
        let mut s = st();
        assert!(s.evening_message(d("2026-11-10")).is_none());
        s.goals = Some(GoalsState {
            date: "2026-11-10".into(),
            items: vec![
                GoalItem { name: "Cinematica".into(), done: 4, target: 10 },
                GoalItem { name: "pH".into(), done: 12, target: 10 },
            ],
        });
        assert_eq!(s.evening_message(d("2026-11-10")).unwrap().1, "Ti mancano: Cinematica (6 domande).");
        assert_eq!(s.goals.as_ref().unwrap().summary(), "Cinematica 4/10 · pH 10/10");
        assert!(s.evening_message(d("2026-11-11")).is_none());
    }

    #[test]
    fn slots() {
        let mut s = st();
        let day = d("2026-11-10");
        assert_eq!(s.slot_message(day, 0, 4, 0).unwrap().0, "Buongiorno!");
        assert!(s.slot_message(day, 1, 4, 0).unwrap().1.starts_with("Ancora nessuna domanda"));
        assert_eq!(s.slot_message(day, 3, 4, 0).unwrap().0, "Oggi non hai ancora studiato");
        assert_eq!(s.slot_message(day, 3, 4, 12).unwrap().1, "3 flashcard da ripassare.");
        s.goals = Some(GoalsState {
            date: "2026-11-10".into(),
            items: vec![GoalItem { name: "pH".into(), done: 2, target: 10 }],
        });
        assert_eq!(
            s.slot_message(day, 0, 4, 0).unwrap().1,
            "pH 2/10. Poi 3 flashcard da ripassare. 30 giorni al 1° appello."
        );
        assert_eq!(s.slot_message(day, 2, 4, 2).unwrap().1, "pH 2/10.");
        assert_eq!(s.slot_message(day, 3, 4, 2).unwrap().0, "Obiettivi di oggi non finiti");
        s.paused = Some(Paused { title: "Simulazione".into(), done: 3, total: 31 });
        assert_eq!(s.slot_message(day, 1, 4, 2).unwrap().0, "Hai una sessione in pausa");
        s.studied_today = 2;
        s.compute_slots(day);
        assert_eq!(s.slots.len(), 8);
        let now = day.and_hms_opt(14, 10, 0).unwrap();
        let sl = s.due_slot(now).unwrap();
        assert_eq!((sl.hour, sl.minute), (13, 45));
        assert!(s.due_slot(day.and_hms_opt(7, 0, 0).unwrap()).is_none());
        s.enabled = false;
        assert!(s.slot_message(day, 0, 4, 0).is_none());
    }

    #[test]
    fn times() {
        assert_eq!(parse_time("7:05"), Some((7, 5)));
        assert_eq!(parse_time("18.30"), Some((18, 30)));
        assert_eq!(parse_time("24:00"), None);
        let s = st();
        let now = d("2026-11-10").and_hms_opt(19, 0, 0).unwrap();
        assert_eq!(s.next_fire(now).unwrap(), d("2026-11-11").and_hms_opt(18, 30, 0).unwrap());
        let now = d("2026-11-10").and_hms_opt(9, 0, 0).unwrap();
        assert_eq!(s.next_fire(now).unwrap(), d("2026-11-10").and_hms_opt(18, 30, 0).unwrap());
    }
}
