//! Punteggio dello studio di gruppo: ogni secondo di studio vale punti moltiplicati per un
//! moltiplicatore che dipende da come si studia (tetto ×5). La regola di base: chi studia 6 ore
//! di fila batte chi ne fa 3 + 3 separate.
//!
//! | effetto | quando | quanto |
//! | --- | --- | --- |
//! | ritmo | ogni 25 min di studio senza pause lunghe | +0,25, da ×1 a ×3 |
//! | gomme nuove | pausa ≤ 10 min dopo almeno 25 min di studio | ×1,2 per 25 min |
//! | pit lento | pausa tra 10 e 30 min | il ritmo si dimezza |
//! | sosta lunga, «Termina» | pausa oltre 30 min | il ritmo riparte da ×1 |
//! | box troppo frequenti | più di 2 pause in 30 min | ×0,8 per 15 min |
//! | concentrazione | modalità concentrazione attiva | ×1 (solo la scia blu: niente bonus) |
//! | scia di gruppo | altri membri in pista | +0,1 per ciascuno, al massimo ×1,4 |
//! | limite giornaliero | oltre 10 ore nel giorno | ×1 fisso |

use serde::{Deserialize, Serialize};

pub const STEP_SECS: i64 = 25 * 60;
pub const STEP: f64 = 0.25;
pub const RITMO_MAX: f64 = 3.0;
pub const TYRES: f64 = 1.2;
pub const PERFECT_PIT: i64 = 10 * 60;
pub const LONG_PIT: i64 = 30 * 60;
pub const SMOKE: f64 = 0.8;
pub const SMOKE_SECS: i64 = 15 * 60;
pub const PITS_WINDOW: i64 = 30 * 60;
pub const FOCUS: f64 = 1.0;
pub const DRAFT_EACH: f64 = 0.1;
pub const DRAFT_MAX: f64 = 1.4;
pub const CAP: f64 = 5.0;
pub const DAILY_CAP_SECS: u64 = 10 * 3600;
/// Punti per giro del circuito (a ×1 un giro al minuto, come prima).
pub const LAP_PTS: f64 = 60.0;

/// Effetti visibili sulla monoposto (bit di `CarVM.fx`).
pub const FX_RITMO: u8 = 1;
pub const FX_TYRES: u8 = 2;
pub const FX_FOCUS: u8 = 4;
pub const FX_SMOKE: u8 = 8;
pub const FX_DRAFT: u8 = 16;

/// Il tratto di studio in corso, con i suoi effetti.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// secondi di studio del tratto già chiusi (prima della ripresa in corso)
    pub acc: i64,
    /// inizio della ripresa in corso (in pista)
    pub since: Option<i64>,
    /// gomme nuove fino a
    pub tyres_until: i64,
    /// penalità (box troppo frequenti) fino a
    pub smoke_until: i64,
    /// inizio delle pause recenti
    pub pits: Vec<i64>,
}

/// Moltiplicatore in un istante, con gli effetti da mostrare.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub mult: f64,
    pub fx: u8,
    /// intensità del ritmo, 0..1 (lunghezza della fiamma)
    pub level: f32,
}

impl Run {
    /// Si entra in pista da fermi: tratto nuovo.
    pub fn start(&mut self, now: i64) {
        *self = Run { since: Some(now), ..Default::default() };
    }

    /// Pit stop: il tempo di pista finora resta nel tratto.
    pub fn pause(&mut self, now: i64) {
        if let Some(s) = self.since.take() {
            self.acc += (now - s).max(0);
        }
    }

    /// Si riparte dopo una pausa iniziata a `pit_start`.
    pub fn resume(&mut self, pit_start: i64, now: i64) {
        let len = (now - pit_start).max(0);
        if len > LONG_PIT {
            self.acc = 0;
            self.tyres_until = 0;
        } else if len > PERFECT_PIT {
            self.acc /= 2;
        } else if self.acc >= STEP_SECS {
            self.tyres_until = now + STEP_SECS;
        }
        self.pits.push(pit_start);
        self.pits.retain(|t| now - t <= PITS_WINDOW);
        if self.pits.len() > 2 {
            self.smoke_until = now + SMOKE_SECS;
        }
        self.since = Some(now);
    }

    /// Secondi di studio del tratto fino a `now`.
    pub fn streak(&self, now: i64) -> i64 {
        self.acc + self.since.map(|s| (now - s).max(0)).unwrap_or(0)
    }

    pub fn ritmo(&self, now: i64) -> f64 {
        (1.0 + STEP * (self.streak(now) / STEP_SECS) as f64).min(RITMO_MAX)
    }

    /// Moltiplicatore a `now`: `focus` = concentrazione attiva, `others` = altri membri in pista,
    /// `today` = secondi già studiati oggi.
    pub fn score(&self, now: i64, focus: bool, others: usize, today: u64) -> Score {
        if today >= DAILY_CAP_SECS {
            return Score { mult: 1.0, fx: 0, level: 0.0 };
        }
        let ritmo = self.ritmo(now);
        let mut fx = 0;
        let mut m = ritmo;
        if ritmo > 1.0 {
            fx |= FX_RITMO;
        }
        if now < self.tyres_until {
            m *= TYRES;
            fx |= FX_TYRES;
        }
        if now < self.smoke_until {
            m *= SMOKE;
            fx |= FX_SMOKE;
        }
        if focus {
            m *= FOCUS;
            fx |= FX_FOCUS;
        }
        if others > 0 {
            m *= (1.0 + DRAFT_EACH * others as f64).min(DRAFT_MAX);
            fx |= FX_DRAFT;
        }
        Score { mult: m.min(CAP), fx, level: ((ritmo - 1.0) / (RITMO_MAX - 1.0)) as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Punti di una serie di tratti di studio (minuti) separati da pause (minuti).
    fn points(plan: &[(i64, i64)], focus: bool) -> f64 {
        let mut run = Run::default();
        let mut t = 0i64;
        let mut pts = 0.0;
        let mut first = true;
        for &(study, pause) in plan {
            if first {
                run.start(t);
                first = false;
            }
            for _ in 0..study * 60 {
                pts += run.score(t, focus, 0, 0).mult;
                t += 1;
            }
            if pause > 0 {
                run.pause(t);
                let ps = t;
                t += pause * 60;
                run.resume(ps, t);
            }
        }
        pts
    }

    #[test]
    fn six_hours_in_a_row_beat_three_plus_three() {
        let giovanni = points(&[(360, 0)], false);
        let tu = points(&[(180, 120), (180, 0)], false);
        assert!(giovanni > tu * 1.25, "{giovanni} vs {tu}");
        // pause brevi tengono il ritmo (e danno le gomme nuove): meglio di una sosta lunga
        let pomodoro = points(&[(50, 5), (50, 5), (50, 5), (50, 5), (50, 5), (50, 5), (60, 0)], false);
        assert!(pomodoro > tu, "{pomodoro} vs {tu}");
        // la concentrazione non dà punti in più (solo la scia blu), e mai oltre ×5
        assert_eq!(points(&[(360, 0)], true), giovanni);
        let r = Run { acc: 10 * 3600, since: Some(0), tyres_until: 100, ..Default::default() };
        assert!(r.score(1, true, 9, 0).mult <= CAP);
    }

    #[test]
    fn pits_and_penalties() {
        let mut r = Run::default();
        r.start(0);
        r.pause(30 * 60);
        r.resume(30 * 60, 35 * 60); // pit perfetto dopo 30 min
        assert!(r.score(36 * 60, false, 0, 0).fx & FX_TYRES != 0);
        assert_eq!(r.ritmo(36 * 60), 1.25);
        r.pause(40 * 60);
        r.resume(40 * 60, 60 * 60); // pit lento: ritmo dimezzato
        assert_eq!(r.streak(60 * 60), (35 * 60) / 2);
        r.pause(61 * 60);
        r.resume(61 * 60, 62 * 60);
        r.pause(63 * 60);
        r.resume(63 * 60, 64 * 60); // terza pausa in 30 minuti: fumo
        let s = r.score(65 * 60, false, 0, 0);
        assert!(s.fx & FX_SMOKE != 0 && s.mult < 1.0);
        r.pause(70 * 60);
        r.resume(70 * 60, 120 * 60); // sosta lunga: si riparte da capo
        assert_eq!(r.ritmo(121 * 60), 1.0);
        // oltre 10 ore al giorno niente bonus
        assert_eq!(r.score(121 * 60, true, 3, DAILY_CAP_SECS).mult, 1.0);
        // scia di gruppo con un limite
        assert_eq!(Run::default().score(0, false, 9, 0).mult, DRAFT_MAX);
    }
}
