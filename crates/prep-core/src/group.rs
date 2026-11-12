//! Studio di gruppo: gruppi privati a cui ci si unisce con un invito, tempo di studio di
//! ciascuno in tempo reale e classifica. Ogni giro del circuito è un blocco di studio
//! ([`LAP_SECS`]); chi è in pausa sta ai box.
//!
//! I membri di un gruppo formano una stanza (come la sincronizzazione, vedi [`crate::p2p`])
//! ricavata dal codice del gruppo, e si mandano il proprio stato ([`Status`]) ogni pochi
//! secondi. Ogni dispositivo ricorda l'ultimo stato visto di ciascuno, così la classifica
//! resta anche quando gli altri non sono collegati.
//!
//! ```text
//! <base>/groups.json      gruppi, membri e ultimo stato visto
//! <base>/study_log.json   secondi di studio per giorno (cronometro della scheda Gruppo)
//! ```

use crate::vfs;
use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Durata di un giro del circuito: 10 minuti di studio.
pub const LAP_SECS: u64 = 10 * 60;
/// Indirizzo della web app: gli inviti sono link a questa pagina (si aprono anche nell'app).
pub const APP_URL: &str = "https://nernerone.github.io/tremendouspreparator/";
/// Colori delle macchinine (ARGB).
pub const CAR_COLORS: &[u32] = &[
    0xffe53935, 0xff1e88e5, 0xff43a047, 0xfffdd835, 0xff8e24aa, 0xfffb8c00, 0xff00acc1, 0xffd81b60, 0xff6d4c41,
    0xff546e7a,
];

const GROUPS_FILE: &str = "groups.json";
const LOG_FILE: &str = "study_log.json";

/// Cosa sta facendo un membro.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Idle,
    Studying,
    Pause,
}

/// Stato che ogni membro manda al gruppo.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Status {
    pub name: String,
    pub color: u32,
    pub state: State,
    /// secondi di studio del giorno `day` fino all'istante `at`
    pub today: u64,
    pub week: u64,
    pub total: u64,
    /// giorno a cui si riferisce `today` (YYYY-MM-DD) e lunedì della settimana di `week`
    pub day: String,
    /// istante (secondi Unix) in cui i numeri erano esatti
    pub at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Member {
    pub status: Status,
    /// ultima volta che si è fatto sentire (secondi Unix)
    pub last_seen: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Group {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub members: BTreeMap<String, Member>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Groups {
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub groups: Vec<Group>,
}

impl Groups {
    pub fn load(base: &Path) -> Groups {
        vfs::read_to_string(&base.join(GROUPS_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, base: &Path) -> std::io::Result<()> {
        vfs::write(&base.join(GROUPS_FILE), serde_json::to_string_pretty(self)?.as_bytes())
    }

    pub fn active(&self) -> Option<&Group> {
        self.groups.iter().find(|g| g.code == self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut Group> {
        let code = self.active.clone();
        self.groups.iter_mut().find(|g| g.code == code)
    }

    /// Aggiunge il gruppo (se non c'è già) e lo rende attivo.
    pub fn join(&mut self, code: &str, name: &str) {
        let code = normalize(code);
        if !self.groups.iter().any(|g| g.code == code) {
            let name = if name.trim().is_empty() { "Gruppo di studio".to_string() } else { name.trim().to_string() };
            self.groups.push(Group { code: code.clone(), name, members: BTreeMap::new() });
        }
        self.active = code;
    }

    pub fn leave(&mut self, code: &str) {
        self.groups.retain(|g| g.code != code);
        if self.active == code {
            self.active = self.groups.first().map(|g| g.code.clone()).unwrap_or_default();
        }
    }
}

const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Codice di un nuovo gruppo (80 bit): «K7QM-4TZP-9WXR-2HDA».
pub fn new_code() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("generatore casuale");
    let c: Vec<char> = b.iter().map(|x| ALPHABET[(*x as usize) % 32] as char).collect();
    c.chunks(4).map(|g| g.iter().collect::<String>()).collect::<Vec<_>>().join("-")
}

/// Codice nella forma usata per la stanza e per l'archivio.
pub fn normalize(code: &str) -> String {
    let c: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    c.as_bytes().chunks(4).map(|g| String::from_utf8_lossy(g).to_string()).collect::<Vec<_>>().join("-")
}

/// Stanza dei membri di un gruppo.
pub fn room(code: &str) -> crate::p2p::Room {
    crate::p2p::Room::derive("gruppo", &normalize(code).replace('-', ""))
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Link d'invito: apre la web app (o l'app) sulla scheda Gruppo con il codice già inserito.
pub fn invite_link(code: &str, name: &str) -> String {
    format!("{APP_URL}#g={}&n={}", normalize(code).replace('-', ""), enc(name))
}

/// Codice e nome da un link d'invito o da un codice scritto a mano.
pub fn parse_invite(text: &str) -> Option<(String, String)> {
    let t = text.trim();
    if let Some(i) = t.find("g=") {
        let frag = &t[i..];
        let mut code = String::new();
        let mut name = String::new();
        for kv in frag.split('&') {
            if let Some(v) = kv.strip_prefix("g=") {
                code = v.to_string();
            } else if let Some(v) = kv.strip_prefix("n=") {
                name = dec(v);
            }
        }
        let code = normalize(&code);
        return (code.replace('-', "").len() == 16).then_some((code, name));
    }
    let code = normalize(t);
    (code.replace('-', "").len() == 16).then_some((code, String::new()))
}

// ---------- tempo di studio ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudyLog {
    #[serde(default)]
    pub days: BTreeMap<String, u64>,
}

fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

impl StudyLog {
    pub fn load(base: &Path) -> StudyLog {
        vfs::read_to_string(&base.join(LOG_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, base: &Path) -> std::io::Result<()> {
        vfs::write(&base.join(LOG_FILE), serde_json::to_string(self)?.as_bytes())
    }

    pub fn add(&mut self, day: NaiveDate, secs: u64) {
        *self.days.entry(day.format("%Y-%m-%d").to_string()).or_default() += secs;
    }

    pub fn day(&self, day: NaiveDate) -> u64 {
        self.days.get(&day.format("%Y-%m-%d").to_string()).copied().unwrap_or(0)
    }

    pub fn week(&self, day: NaiveDate) -> u64 {
        let m = monday(day);
        (0..7).map(|i| self.day(m + Duration::days(i))).sum()
    }

    pub fn total(&self) -> u64 {
        self.days.values().sum()
    }
}

/// Lo stato da mandare: i numeri del registro più il tratto in corso (`running` secondi).
pub fn my_status(
    name: &str,
    color: u32,
    state: State,
    log: &StudyLog,
    running: u64,
    today: NaiveDate,
    now: i64,
) -> Status {
    Status {
        name: name.to_string(),
        color,
        state,
        today: log.day(today) + running,
        week: log.week(today) + running,
        total: log.total() + running,
        day: today.format("%Y-%m-%d").to_string(),
        at: now,
    }
}

/// Classifica: per oggi, per la settimana o in totale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Today,
    Week,
    Total,
}

/// Numeri di uno stato aggiornati a `now` (chi sta studiando continua a contare; un giorno o
/// una settimana passati non contano più per «oggi» e «settimana»).
pub fn live(s: &Status, now: i64, today: NaiveDate) -> (u64, u64, u64) {
    let extra = if s.state == State::Studying { (now - s.at).clamp(0, 6 * 3600) as u64 } else { 0 };
    let day = NaiveDate::parse_from_str(&s.day, "%Y-%m-%d").ok();
    let t = if day == Some(today) {
        s.today + extra
    } else if s.state == State::Studying {
        extra
    } else {
        0
    };
    let w = if day.map(monday) == Some(monday(today)) { s.week + extra } else { t };
    (t, w, s.total + extra)
}

pub fn value(s: &Status, now: i64, today: NaiveDate, p: Period) -> u64 {
    let (t, w, tot) = live(s, now, today);
    match p {
        Period::Today => t,
        Period::Week => w,
        Period::Total => tot,
    }
}

/// Ordine della classifica: (id, stato) dal primo all'ultimo.
pub fn standings<'a>(
    members: impl Iterator<Item = (&'a str, &'a Status)>,
    now: i64,
    today: NaiveDate,
    p: Period,
) -> Vec<(&'a str, &'a Status)> {
    let mut v: Vec<(&str, &Status)> = members.collect();
    v.sort_by(|a, b| value(b.1, now, today, p).cmp(&value(a.1, now, today, p)).then(a.1.name.cmp(&b.1.name)));
    v
}

pub fn fmt_secs(s: u64) -> String {
    let (h, m) = (s / 3600, (s % 3600) / 60);
    if h > 0 {
        format!("{h} h {m:02} min")
    } else {
        format!("{m} min")
    }
}

// ---------- circuito ----------

/// Punto del circuito (forma a stadio, coordinate 0..1 nella larghezza e altezza della pista)
/// a una frazione `f` di giro, con la direzione di marcia in gradi. Partenza a metà del
/// rettilineo in basso, senso antiorario visto dall'alto… cioè verso destra in basso.
pub fn track_point(f: f64, aspect: f64) -> (f64, f64, f64) {
    // stadio: rettilinei lunghi `l`, curve di raggio `r` (in unità di altezza = 1)
    let r = 0.5;
    let l = (aspect - 2.0 * r).max(0.2);
    let total = 2.0 * l + 2.0 * std::f64::consts::PI * r;
    let mut d = f.rem_euclid(1.0) * total;
    let cx0 = r; // centro della curva sinistra (in unità di altezza)
    let cx1 = r + l;
    // 1) mezzo rettilineo in basso, verso destra
    let (x, y, a);
    if d < l / 2.0 {
        (x, y, a) = (cx0 + l / 2.0 + d, 1.0, 0.0);
    } else {
        d -= l / 2.0;
        let arc = std::f64::consts::PI * r;
        if d < arc {
            // curva destra, dal basso verso l'alto
            let t = d / r; // 0..π
            let th = std::f64::consts::FRAC_PI_2 - t;
            (x, y, a) = (cx1 + r * th.cos(), 0.5 + r * th.sin(), -t.to_degrees());
        } else {
            d -= arc;
            if d < l {
                (x, y, a) = (cx1 - d, 0.0, 180.0);
            } else {
                d -= l;
                if d < arc {
                    let t = d / r;
                    let th = -std::f64::consts::FRAC_PI_2 - t;
                    (x, y, a) = (cx0 + r * th.cos(), 0.5 + r * th.sin(), 180.0 - t.to_degrees());
                } else {
                    d -= arc;
                    (x, y, a) = (cx0 + d, 1.0, 0.0);
                }
            }
        }
    }
    (x / aspect, y, a)
}

/// Posto ai box (dentro la pista, in basso) per la macchina `i` di `n`.
pub fn pit_point(i: usize, n: usize) -> (f64, f64, f64) {
    let n = n.max(1) as f64;
    (0.32 + 0.36 * (i as f64 + 0.5) / n, 0.72, 0.0)
}

/// Tratti della linea di mezzeria: (x, y, direzione) lungo la pista, uno ogni due.
pub fn centerline(aspect: f64, n: usize) -> Vec<(f64, f64, f64)> {
    (0..n).filter(|i| i % 2 == 0).map(|i| track_point(i as f64 / n as f64, aspect)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invites() {
        let code = new_code();
        assert_eq!(code.len(), 19);
        let link = invite_link(&code, "Ripasso di Anatomia");
        assert!(link.starts_with(APP_URL));
        let (c, n) = parse_invite(&link).unwrap();
        assert_eq!(c, code);
        assert_eq!(n, "Ripasso di Anatomia");
        assert_eq!(parse_invite(&code.to_lowercase().replace('-', " ")).unwrap().0, code);
        assert!(parse_invite("ciao").is_none());
        assert_eq!(room(&code).tag, room(&code.to_lowercase()).tag);
    }

    #[test]
    fn study_time_and_ranking() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap(); // giovedì
        let mut log = StudyLog::default();
        log.add(today, 1800);
        log.add(today - Duration::days(1), 600);
        log.add(today - Duration::days(10), 100);
        assert_eq!(log.day(today), 1800);
        assert_eq!(log.week(today), 2400);
        assert_eq!(log.total(), 2500);
        let now = 1_000_000;
        let me = my_status("Io", 1, State::Studying, &log, 60, today, now);
        assert_eq!(me.today, 1860);
        // chi studia continua a contare
        assert_eq!(live(&me, now + 100, today).0, 1960);
        // uno stato di ieri non conta per oggi
        let old = Status { day: "2026-11-11".into(), today: 999, week: 999, total: 999, at: now, ..Default::default() };
        assert_eq!(live(&old, now, today), (0, 999, 999));
        let a = Status { name: "A".into(), today: 10, day: "2026-11-12".into(), ..Default::default() };
        let b = Status { name: "B".into(), today: 20, day: "2026-11-12".into(), ..Default::default() };
        let order = standings([("a", &a), ("b", &b)].into_iter(), now, today, Period::Today);
        assert_eq!(order[0].0, "b");
        assert_eq!(fmt_secs(3725), "1 h 02 min");
    }

    #[test]
    fn track_is_closed_and_inside() {
        let (x0, y0, _) = track_point(0.0, 2.0);
        let (x1, y1, _) = track_point(0.999_999, 2.0);
        assert!((x0 - x1).abs() < 1e-3 && (y0 - y1).abs() < 1e-3);
        for i in 0..100 {
            let (x, y, _) = track_point(i as f64 / 100.0, 2.0);
            assert!((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y));
        }
    }
}
