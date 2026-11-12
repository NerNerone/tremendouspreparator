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

/// Durata di un giro del circuito: 1 minuto di studio.
pub const LAP_SECS: u64 = 60;
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
    /// durata dei pit stop (pause) già finiti nel giorno `day`, in secondi
    #[serde(default)]
    pub pits: Vec<u64>,
    /// inizio del pit stop in corso (secondi Unix), 0 se non è in pausa
    #[serde(default)]
    pub pit_since: i64,
    /// cosa sta studiando (es. «Quiz · Chimica»), vuoto se non si sa
    #[serde(default)]
    pub doing: String,
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
    /// durata dei pit stop finiti, per giorno (solo gli ultimi giorni)
    #[serde(default)]
    pub pits: BTreeMap<String, Vec<u64>>,
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

    /// Registra un pit stop finito (dimentica quelli di più di una settimana fa).
    pub fn add_pit(&mut self, day: NaiveDate, secs: u64) {
        let key = day.format("%Y-%m-%d").to_string();
        self.pits.entry(key).or_default().push(secs);
        let old = (day - Duration::days(7)).format("%Y-%m-%d").to_string();
        self.pits.retain(|k, _| *k >= old);
    }

    pub fn pits(&self, day: NaiveDate) -> Vec<u64> {
        self.pits.get(&day.format("%Y-%m-%d").to_string()).cloned().unwrap_or_default()
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
        pits: log.pits(today),
        pit_since: 0,
        doing: String::new(),
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

/// Un circuito: forma chiusa disegnata con curve di Bézier (dal punto di partenza, a metà del
/// rettilineo in basso, verso destra; l'esterno della pista resta alla destra di chi corre).
#[derive(Debug, Clone, PartialEq)]
pub struct Circuit {
    pub name: &'static str,
    /// larghezza / altezza
    pub aspect: f64,
    /// comandi SVG nel riquadro 0..1000 × 0..1000 (da stirare su larghezza × altezza)
    pub commands: String,
    /// centro del paddock nell'area interna (0..1 in larghezza e altezza)
    pub paddock: (f64, f64),
}

/// Centro del paddock di ciascun circuito, nello stesso ordine di `LAYOUTS` (verificato a mano:
/// un riquadro di 0,36 × 0,22 resta dentro il tracciato).
const PADDOCKS: &[(f64, f64)] = &[(0.5, 0.55), (0.5, 0.68), (0.45, 0.7), (0.5, 0.65), (0.45, 0.66), (0.5, 0.72)];

/// Un meccanico al pit stop, nelle coordinate della monoposto (36 × 20 px, muso verso +x;
/// +y è il lato esterno, lontano dalla pista): angolo in alto a sinistra della figura (16 px),
/// rotazione in gradi (verso la macchina).
#[derive(Debug, Clone, PartialEq)]
pub struct CrewSpot {
    pub x: f64,
    pub y: f64,
    pub rot: f64,
    /// ha l'avvitatore
    pub tool: bool,
    /// toglie e rimette una gomma
    pub tire: bool,
    /// tiene il cartello (capo meccanico)
    pub sign: bool,
    /// lavora in controtempo rispetto agli altri
    pub alt: bool,
}

/// La squadra di un pit stop: da 3 a 7 meccanici in posti a caso attorno alla macchina, tutti
/// sul lato esterno o alle estremità. Lo stesso `seed` (membro + inizio della sosta) dà sempre
/// la stessa squadra, così non cambia a ogni aggiornamento.
pub fn pit_crew(seed: u64) -> Vec<CrewSpot> {
    // splitmix64 per mescolare il seme, poi xorshift
    let mut x = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    x |= 1;
    let mut rnd = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 11) as f64 / (1u64 << 53) as f64
    };
    // (x, y, rotazione, avvitatore, gomma, cartello)
    let slots: [(f64, f64, f64, bool, bool, bool); 9] = [
        (0.0, 24.0, -90.0, true, true, false),   // gomma posteriore esterna
        (18.0, 24.0, -90.0, true, true, false),  // gomma anteriore esterna
        (41.0, 2.0, 180.0, false, false, false), // crick anteriore
        (-21.0, 2.0, 0.0, false, false, false),  // crick posteriore
        (34.0, 20.0, -135.0, true, false, false),
        (-13.0, 20.0, -45.0, true, false, false),
        (9.0, 30.0, -90.0, true, false, false),
        (9.0, 46.0, -90.0, false, false, true), // capo meccanico con il cartello
        (40.0, 34.0, -150.0, false, false, false),
    ];
    let n = 3 + (rnd() * 5.0) as usize; // 3..=7
    let mut idx: Vec<usize> = (0..slots.len()).collect();
    for i in (1..idx.len()).rev() {
        let j = (rnd() * (i + 1) as f64) as usize;
        idx.swap(i, j);
    }
    idx.truncate(n.min(slots.len()));
    idx.sort();
    idx.iter()
        .enumerate()
        .map(|(k, &i)| {
            let (sx, sy, r, tool, tire, sign) = slots[i];
            CrewSpot {
                x: sx + (rnd() - 0.5) * 5.0,
                // mai verso la pista: si sposta solo in fuori
                y: sy + rnd() * 4.0,
                rot: r + (rnd() - 0.5) * 24.0,
                tool,
                tire,
                sign,
                alt: k % 2 == 1,
            }
        })
        .collect()
}

/// Punti di controllo dei circuiti (unità: altezza = 1), percorsi in senso antiorario sullo
/// schermo partendo dal rettilineo in basso.
const LAYOUTS: &[(&str, &[(f64, f64)])] = &[
    (
        "Ovale",
        &[
            (1.2, 1.0),
            (1.9, 1.0),
            (2.3, 0.85),
            (2.4, 0.5),
            (2.3, 0.15),
            (1.9, 0.0),
            (1.2, 0.0),
            (0.5, 0.0),
            (0.1, 0.15),
            (0.0, 0.5),
            (0.1, 0.85),
            (0.5, 1.0),
        ],
    ),
    (
        "Fagiolo",
        &[
            (1.2, 1.0),
            (2.0, 0.97),
            (2.45, 0.55),
            (2.15, 0.05),
            (1.65, 0.1),
            (1.2, 0.38),
            (0.75, 0.1),
            (0.3, 0.05),
            (0.0, 0.5),
            (0.4, 0.97),
        ],
    ),
    (
        "Collina",
        &[
            (1.0, 1.0),
            (2.0, 1.0),
            (2.45, 0.85),
            (2.2, 0.45),
            (1.5, 0.05),
            (1.0, 0.0),
            (0.6, 0.3),
            (0.15, 0.75),
            (0.4, 1.0),
        ],
    ),
    (
        "Chicane",
        &[
            (1.2, 1.0),
            (2.0, 1.0),
            (2.4, 0.72),
            (2.2, 0.4),
            (1.85, 0.35),
            (1.65, 0.05),
            (1.0, 0.0),
            (0.65, 0.3),
            (0.3, 0.2),
            (0.0, 0.55),
            (0.35, 1.0),
        ],
    ),
    (
        "Lago",
        &[
            (1.2, 1.0),
            (2.3, 1.0),
            (2.7, 0.75),
            (2.45, 0.45),
            (2.05, 0.52),
            (1.8, 0.3),
            (1.6, 0.06),
            (1.2, 0.0),
            (0.95, 0.28),
            (0.55, 0.02),
            (0.1, 0.12),
            (0.0, 0.6),
            (0.35, 0.98),
        ],
    ),
    (
        "Gran Premio",
        &[
            (1.4, 1.0),
            (2.6, 1.0),
            (3.0, 0.8),
            (2.85, 0.45),
            (2.4, 0.5),
            (2.15, 0.25),
            (2.0, 0.04),
            (1.6, 0.0),
            (1.35, 0.3),
            (1.0, 0.3),
            (0.75, 0.0),
            (0.2, 0.05),
            (0.0, 0.45),
            (0.3, 0.6),
            (0.3, 0.88),
            (0.7, 1.0),
        ],
    ),
];

type Seg = [(f64, f64); 4];

/// Catmull-Rom chiusa → segmenti di Bézier cubiche.
fn segments(p: &[(f64, f64)]) -> Vec<Seg> {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (p0, p1, p2, p3) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n], p[(i + 2) % n]);
            [
                p1,
                (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0),
                (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0),
                p2,
            ]
        })
        .collect()
}

fn bezier(s: &Seg, t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * s[0].0 + b * s[1].0 + c * s[2].0 + d * s[3].0, a * s[0].1 + b * s[1].1 + c * s[2].1 + d * s[3].1)
}

/// Punti fitti lungo un circuito (unità del disegno, prima della normalizzazione).
fn sample(segs: &[Seg], k: usize) -> Vec<(f64, f64)> {
    segs.iter().flat_map(|s| (0..k).map(move |i| bezier(s, i as f64 / k as f64))).collect()
}

/// Quanti circuiti ci sono.
pub fn circuits() -> usize {
    LAYOUTS.len()
}

/// Il circuito numero `i`, normalizzato nel riquadro 0..1000.
pub fn circuit(i: usize) -> Circuit {
    let (name, pts) = LAYOUTS[i % LAYOUTS.len()];
    let segs = segments(pts);
    let s = sample(&segs, 32);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in &s {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let (w, h) = ((x1 - x0).max(1e-6), (y1 - y0).max(1e-6));
    let f = |p: (f64, f64)| format!("{:.1} {:.1}", (p.0 - x0) / w * 1000.0, (p.1 - y0) / h * 1000.0);
    let mut cmd = format!("M {}", f(segs[0][0]));
    for sg in &segs {
        cmd += &format!(" C {} {} {}", f(sg[1]), f(sg[2]), f(sg[3]));
    }
    cmd += " Z";
    Circuit { name, aspect: w / h, commands: cmd, paddock: PADDOCKS[i % PADDOCKS.len()] }
}

/// Il circuito di un gruppo: più membri, circuito più lungo e articolato (fra quelli adatti,
/// la scelta dipende dal codice, così tutti nel gruppo vedono lo stesso).
pub fn circuit_for(code: &str, members: usize) -> usize {
    let pool: &[usize] = match members {
        0..=2 => &[0, 1],
        3..=4 => &[1, 2, 3],
        5..=7 => &[3, 4],
        _ => &[4, 5],
    };
    let h = code.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    pool[h as usize % pool.len()]
}

/// Quanto è grande il circuito (1 = da soli): cresce con il numero di membri.
pub fn circuit_scale(members: usize) -> f64 {
    (1.0 + 0.15 * members.saturating_sub(1) as f64).min(2.2)
}

/// Area con segno (positiva = senso antiorario sullo schermo, con y verso il basso).
#[cfg(test)]
fn screen_ccw(pts: &[(f64, f64)]) -> bool {
    let n = pts.len();
    let a: f64 = (0..n).map(|i| pts[i].0 * pts[(i + 1) % n].1 - pts[(i + 1) % n].0 * pts[i].1).sum();
    a < 0.0
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
    fn circuits_are_closed_and_oriented() {
        for (i, (_, pts)) in LAYOUTS.iter().enumerate() {
            let c = circuit(i);
            assert!(c.commands.starts_with("M ") && c.commands.ends_with(" Z"));
            assert!(c.aspect > 1.5 && c.aspect < 4.0, "{} {}", c.name, c.aspect);
            // partenza in basso, verso destra, esterno a destra
            assert!(pts[0].1 >= 0.97 && pts[1].0 > pts[0].0, "{}", c.name);
            assert!(screen_ccw(&sample(&segments(pts), 16)), "{}", c.name);
        }
        assert_eq!(PADDOCKS.len(), LAYOUTS.len());
        let code = new_code();
        assert_eq!(circuit_for(&code, 2), circuit_for(&code, 2));
        assert!(circuit_for(&code, 10) >= 4);
        assert!(circuit_scale(1) == 1.0 && circuit_scale(30) <= 2.2);
    }

    #[test]
    fn pit_crews_vary_but_are_stable() {
        let mut sizes = std::collections::BTreeSet::new();
        for seed in 0..200u64 {
            let c = pit_crew(seed);
            assert!((3..=7).contains(&c.len()));
            assert_eq!(c, pit_crew(seed));
            // nessuno verso la pista (lato -y oltre la carrozzeria)
            assert!(c.iter().all(|m| m.y >= 2.0));
            sizes.insert(c.len());
        }
        assert!(sizes.len() >= 4);
    }
}
