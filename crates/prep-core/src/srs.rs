//! Ripetizione dilazionata per le flashcard (SM-2 semplificato, intervalli limitati
//! a 21 giorni perché l'esame è vicino). Lo stato è ricostruito dai log delle sessioni.

use crate::progress::Attempt;
use chrono::{DateTime, Duration, Utc};

pub const MAX_INTERVAL_DAYS: f64 = 21.0;

#[derive(Debug, Clone)]
pub struct CardState {
    pub due: DateTime<Utc>,
    pub interval_days: f64,
    pub ease: f64,
    pub reps: u32,
    pub lapses: u32,
}

pub fn replay(reviews: &[Attempt]) -> Option<CardState> {
    let first = reviews.first()?;
    let mut st = CardState { due: first.at, interval_days: 0.0, ease: 2.3, reps: 0, lapses: 0 };
    for r in reviews {
        let mut g = r.grade.unwrap_or(0);
        if r.unsure {
            g = g.min(1);
        }
        match g {
            0 => {
                st.reps = 0;
                st.lapses += 1;
                st.interval_days = 0.0;
                st.ease -= 0.2;
                st.due = r.at + Duration::minutes(10);
            }
            1 => {
                st.interval_days = if st.reps == 0 { 1.0 } else { (st.interval_days * 1.2).max(1.0) };
                st.ease -= 0.15;
                st.reps += 1;
            }
            2 => {
                st.interval_days = match st.reps {
                    0 => 1.0,
                    1 => 3.0,
                    _ => st.interval_days * st.ease,
                };
                st.reps += 1;
            }
            _ => {
                st.interval_days = if st.reps == 0 { 3.0 } else { st.interval_days * st.ease * 1.3 };
                st.ease += 0.15;
                st.reps += 1;
            }
        }
        st.ease = st.ease.clamp(1.3, 3.0);
        st.interval_days = st.interval_days.min(MAX_INTERVAL_DAYS);
        if g > 0 {
            st.due = r.at + Duration::minutes((st.interval_days * 24.0 * 60.0) as i64);
        }
    }
    Some(st)
}

pub fn is_due(reviews: Option<&Vec<Attempt>>, now: DateTime<Utc>) -> bool {
    match reviews.and_then(|r| replay(r)) {
        Some(st) => st.due <= now,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(d: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_790_000_000 + d * 86400, 0).unwrap()
    }

    #[test]
    fn grows() {
        let r = vec![
            Attempt { at: at(0), score: 0.8, grade: Some(2), unsure: false, secs: 0 },
            Attempt { at: at(1), score: 0.8, grade: Some(2), unsure: false, secs: 0 },
        ];
        let st = replay(&r).unwrap();
        assert_eq!(st.interval_days, 3.0);
        assert_eq!(st.due, at(4));
    }

    #[test]
    fn lapse() {
        let r = vec![
            Attempt { at: at(0), score: 0.8, grade: Some(3), unsure: false, secs: 0 },
            Attempt { at: at(3), score: 0.0, grade: Some(0), unsure: false, secs: 0 },
        ];
        let st = replay(&r).unwrap();
        assert_eq!(st.reps, 0);
        assert!(st.due < at(4));
    }
}
