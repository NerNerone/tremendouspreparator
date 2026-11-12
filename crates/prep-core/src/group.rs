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
    /// identificativo dell'utente (uguale su tutti i suoi dispositivi, vedi [`user_id`]);
    /// vuoto nei messaggi delle versioni precedenti (allora vale il dispositivo)
    #[serde(default)]
    pub uid: String,
    /// dispositivo che manda lo stato
    #[serde(default)]
    pub dev: String,
    /// foto profilo (hash, vedi [`photo_hash`]); vuoto = nessuna
    #[serde(default)]
    pub photo: String,
}

/// Stato di un dispositivo di un membro e quando si è fatto sentire.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DevStatus {
    pub status: Status,
    pub last_seen: i64,
}

/// Un membro del gruppo (un utente, con uno o più dispositivi).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Member {
    /// stato più recente ricevuto (da uno qualsiasi dei suoi dispositivi)
    pub status: Status,
    /// ultima volta che si è fatto sentire (secondi Unix)
    pub last_seen: i64,
    /// stato di ciascun dispositivo: il tempo di studio è la somma
    #[serde(default)]
    pub devices: BTreeMap<String, DevStatus>,
}

/// Da quanto tempo (secondi) un dispositivo che tace è considerato non collegato.
pub const ONLINE_SECS: i64 = 45;

impl Member {
    /// Registra lo stato arrivato da un dispositivo.
    pub fn update(&mut self, dev: &str, status: Status, now: i64) {
        self.devices.insert(dev.to_string(), DevStatus { status: status.clone(), last_seen: now });
        self.status = status;
        self.last_seen = now;
    }

    /// Stati dei dispositivi con «collegato» (i membri salvati da versioni precedenti hanno
    /// solo `status`).
    pub fn device_list(&self, now: i64) -> Vec<(Status, bool)> {
        if self.devices.is_empty() {
            return vec![(self.status.clone(), now - self.last_seen < ONLINE_SECS)];
        }
        self.devices.values().map(|d| (d.status.clone(), now - d.last_seen < ONLINE_SECS)).collect()
    }
}

/// Stato complessivo di un utente a partire da quelli dei suoi dispositivi, aggiornato a `now`:
/// i tempi si sommano (ogni dispositivo conta solo il proprio studio); in pista se almeno un
/// dispositivo collegato studia, al pit stop se almeno uno è in pausa. Nome, colore e foto
/// vengono dal dispositivo sentito più di recente. Restituisce anche «collegato».
pub fn combine(devs: &[(Status, bool)], now: i64, today: NaiveDate) -> (Status, bool) {
    let Some(latest) = devs.iter().max_by_key(|(s, _)| s.at) else { return (Status::default(), false) };
    let mut out = latest.0.clone();
    let (mut t, mut w, mut tot) = (0, 0, 0);
    let mut pits = Vec::new();
    let mut state = State::Idle;
    let mut pit_since = 0;
    let mut doing = String::new();
    for (s, on) in devs {
        let (a, b, c) = live(s, now, today);
        t += a;
        w += b;
        tot += c;
        if s.day == today.format("%Y-%m-%d").to_string() {
            pits.extend(s.pits.iter().copied());
        }
        if *on && s.state == State::Studying {
            state = State::Studying;
            doing = s.doing.clone();
        } else if *on && s.state == State::Pause && state != State::Studying {
            state = State::Pause;
            pit_since = s.pit_since;
            doing = s.doing.clone();
        }
    }
    out.state = state;
    out.today = t;
    out.week = w;
    out.total = tot;
    out.day = today.format("%Y-%m-%d").to_string();
    out.at = now;
    out.pits = pits;
    out.pit_since = if state == State::Pause { pit_since } else { 0 };
    if !doing.is_empty() {
        out.doing = doing;
    }
    (out, devs.iter().any(|(_, on)| *on))
}

/// Identificativo dell'utente: con nome utente e password della sincronizzazione (gli stessi
/// su tutti i propri dispositivi) è ricavato da questi, così telefono e web app contano come
/// una sola persona; altrimenti è quello del dispositivo.
pub fn user_id(username: &str, password: &str, device: &str) -> String {
    if username.trim().is_empty() || password.len() < 8 {
        return device.to_string();
    }
    let h = crate::p2p::sha256_hex(format!("tp-uid\n{}\n{}", username.trim().to_lowercase(), password).as_bytes());
    format!("u{}", &h[..15])
}

/// Hash breve di una foto (nome del file e riferimento nei messaggi).
pub fn photo_hash(bytes: &[u8]) -> String {
    crate::p2p::sha256_hex(bytes)[..16].to_string()
}

/// Messaggi del gruppo oltre allo stato di ciascuno.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "g", rename_all = "snake_case")]
pub enum GroupMsg {
    /// impostazioni decise dall'amministratore (chi ha creato il gruppo): nome, foto, membri
    /// rimossi. `at` = quando sono state cambiate (vince la più recente).
    Info { admin: String, name: String, photo: String, removed: Vec<String>, at: i64 },
    /// richiesta di una foto che non si ha (la manda chi ce l'ha)
    PhotoGet { hash: String },
    /// una foto (JPEG in base64)
    Photo { hash: String, data: String },
    /// avviso: «vieni a studiare» a un membro (`to`) o a tutti (`to` vuoto)
    Ping { id: String, from: String, name: String, to: String, at: i64, pit: bool },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Group {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub members: BTreeMap<String, Member>,
    /// amministratore (id utente di chi ha creato il gruppo); vuoto finché non si sa
    #[serde(default)]
    pub admin: String,
    /// utenti rimossi dall'amministratore
    #[serde(default)]
    pub removed: Vec<String>,
    /// foto del gruppo (hash)
    #[serde(default)]
    pub photo: String,
    /// quando l'amministratore ha cambiato le impostazioni l'ultima volta
    #[serde(default)]
    pub info_at: i64,
    /// istante dell'ultimo avviso già mostrato (per non ripeterli)
    #[serde(default)]
    pub ping_seen: i64,
}

/// Cosa fare con un `Info` ricevuto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfoResult {
    /// ignorato (non viene dall'amministratore o è vecchio)
    Ignored,
    Applied,
    /// l'utente è stato rimosso dal gruppo
    Removed,
}

impl Group {
    /// Stato (o membro) di un utente, che arrivi da un dispositivo nuovo o già noto. Le voci
    /// salvate con l'id del dispositivo (versioni precedenti) passano sotto l'id utente.
    pub fn status_in(&mut self, dev: &str, status: Status, now: i64) {
        let uid = if status.uid.is_empty() { dev.to_string() } else { status.uid.clone() };
        if self.removed.contains(&uid) {
            return;
        }
        if uid != dev {
            if let Some(old) = self.members.remove(dev) {
                self.members.entry(uid.clone()).or_insert(old);
            }
        }
        self.members.entry(uid).or_default().update(dev, status, now);
    }

    /// Applica le impostazioni dell'amministratore. Il primo che si presenta come
    /// amministratore resta tale (lo è chi ha creato il gruppo, che le manda per primo).
    pub fn apply_info(
        &mut self,
        me: &str,
        admin: &str,
        name: &str,
        photo: &str,
        removed: &[String],
        at: i64,
    ) -> InfoResult {
        if admin.is_empty() || (!self.admin.is_empty() && self.admin != admin) || at <= self.info_at {
            return InfoResult::Ignored;
        }
        self.admin = admin.to_string();
        if !name.trim().is_empty() {
            self.name = name.trim().chars().take(60).collect();
        }
        self.photo = photo.to_string();
        self.removed = removed.to_vec();
        self.info_at = at;
        for r in removed {
            self.members.remove(r);
        }
        if removed.iter().any(|r| r == me) {
            InfoResult::Removed
        } else {
            InfoResult::Applied
        }
    }
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
            self.groups.push(Group { code: code.clone(), name, ..Default::default() });
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
        ..Default::default()
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
    /// centro del paddock, sotto il circuito (0..1 in larghezza e altezza)
    pub paddock: (f64, f64),
}

/// Centro del paddock (chi non è in gara): sotto il circuito, centrato, oltre le piazzole dei
/// pit stop del rettilineo in basso (0..1 in larghezza e altezza del circuito).
pub const PADDOCK: (f64, f64) = (0.5, 1.42);

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
        (0.0, 19.0, -90.0, true, true, false),   // gomma posteriore esterna
        (17.0, 19.0, -90.0, true, true, false),  // gomma anteriore esterna
        (36.0, 2.0, 180.0, false, false, false), // crick anteriore
        (-16.0, 2.0, 0.0, false, false, false),  // crick posteriore
        (31.0, 15.0, -135.0, true, false, false),
        (-11.0, 15.0, -45.0, true, false, false),
        (8.0, 21.0, -90.0, true, false, false),
        (26.0, 22.0, -90.0, false, false, true), // capo meccanico con il cartello
        (-8.0, 22.0, -60.0, false, false, false),
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
                x: sx + (rnd() - 0.5) * 3.0,
                // mai verso la pista: si sposta solo in fuori
                y: sy + rnd() * 2.0,
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
    Circuit { name, aspect: w / h, commands: cmd, paddock: PADDOCK }
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

/// Punti del circuito `i` a passo uniforme lungo la pista, in unità dell'altezza
/// (x da 0 a `aspect`, y da 0 a 1), con la lunghezza totale.
fn track_samples(i: usize) -> (Vec<(f64, f64)>, f64) {
    let (_, pts) = LAYOUTS[i % LAYOUTS.len()];
    let raw = sample(&segments(pts), 48);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in &raw {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let h = (y1 - y0).max(1e-6);
    let pts: Vec<(f64, f64)> = raw.iter().map(|(x, y)| ((x - x0) / h, (y - y0) / h)).collect();
    let n = pts.len();
    let mut cum = vec![0.0; n + 1];
    for k in 0..n {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        cum[k + 1] = cum[k] + ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    }
    let total = cum[n];
    // ricampionamento a passo uniforme (come `point-at` di Slint, che va per lunghezza)
    let m = 600;
    let mut out = Vec::with_capacity(m);
    let mut k = 0;
    for j in 0..m {
        let d = total * j as f64 / m as f64;
        while k + 1 < n && cum[k + 1] < d {
            k += 1;
        }
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let t = ((d - cum[k]) / (cum[k + 1] - cum[k]).max(1e-9)).clamp(0.0, 1.0);
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
    }
    (out, total)
}

/// Lunghezza del circuito `i` in altezze del circuito.
pub fn circuit_length(i: usize) -> f64 {
    track_samples(i).1
}

/// Distanza (in px della monoposto) tra il bordo della strada e il centro della macchina
/// ferma al pit stop; la stessa costante è in `pages.slint` (`pit-off`).
pub const PIT_OFF_PX: f64 = 30.0;
/// Distanza tra due piazzole in fila (px della monoposto; `pages.slint`, `car.stack`).
pub const PIT_ROW_PX: f64 = 50.0;

/// Dove si ferma ciascuna macchina al pit stop (`stops` = frazioni di giro): lato (+1 = a
/// destra della marcia, -1 = a sinistra) e fila (0 = la più vicina alla pista). Ogni
/// piazzola, con le misure del disegno, deve stare fuori dalla strada e non sovrapporsi alle
/// altre (anche quando due vanno in pausa nello stesso punto): si prova l'esterno, poi
/// l'interno, poi file sempre più lontane.
pub fn pit_places(circuit: usize, members: usize, stops: &[f64]) -> Vec<(f64, u32)> {
    let (pts, length) = track_samples(circuit);
    let m = pts.len();
    // misure in altezze del circuito, come in RaceView: carreggiata 0,13 / √scala, macchina
    // larga 0,3 della carreggiata (20 px)
    let sw = 0.13 / circuit_scale(members).sqrt();
    let px = 0.3 * sw / 20.0;
    // rettangolo della piazzola nel riferimento della macchina (px): lungo la marcia u,
    // verso l'esterno v (centro della macchina in 0, 0)
    let (u0, u1, v0, v1) = (-34.0, 36.0, -13.0, 29.0);
    let fits = |f: f64, side: f64, level: u32| -> bool {
        let j = (f.rem_euclid(1.0) * m as f64) as usize % m;
        let (p, q) = (pts[j], pts[(j + 1) % m]);
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let l = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (tx, ty) = (dx / l, dy / l);
        let (nx, ny) = (-ty * side, tx * side);
        let off = sw / 2.0 + (PIT_OFF_PX + level as f64 * PIT_ROW_PX) * px;
        let lim = (sw / 2.0 + 0.012).powi(2);
        (0..=8).all(|a| {
            (0..=5).all(|b| {
                let u = (u0 + (u1 - u0) * a as f64 / 8.0) * px;
                let v = off + (v0 + (v1 - v0) * b as f64 / 5.0) * px;
                let (x, y) = (p.0 + tx * u + nx * v, p.1 + ty * u + ny * v);
                pts.iter().all(|t| (t.0 - x).powi(2) + (t.1 - y).powi(2) >= lim)
            })
        })
    };
    let pad_len = (u1 - u0) * px + 0.02;
    let mut placed: Vec<(f64, f64, u32)> = Vec::new();
    let mut out = Vec::with_capacity(stops.len());
    for &f in stops {
        let free = |side: f64, level: u32, placed: &[(f64, f64, u32)]| {
            placed.iter().all(|(g, s, l)| {
                let d = (f - g).rem_euclid(1.0);
                *s != side || *l != level || d.min(1.0 - d) * length >= pad_len
            })
        };
        let choice = (0..4u32)
            .flat_map(|level| [(1.0, level), (-1.0, level)])
            .find(|&(side, level)| free(side, level, &placed) && fits(f, side, level))
            .unwrap_or_else(|| {
                // nessun posto libero vicino: la prima fila esterna non occupata
                (0..16).map(|l| (1.0, l)).find(|&(s, l)| free(s, l, &placed)).unwrap_or((1.0, 0))
            });
        placed.push((f, choice.0, choice.1));
        out.push(choice);
    }
    out
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

    #[test]
    fn users_with_more_devices_and_admin() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let day = "2026-11-12".to_string();
        let now = 1_000_000;
        assert_eq!(user_id("Gianni", "segreta123", "d1"), user_id(" gianni", "segreta123", "d2"));
        assert_eq!(user_id("", "", "d1"), "d1");
        let uid = user_id("Gianni", "segreta123", "d1");
        let mut g = Group { code: "X".into(), admin: "boss".into(), ..Default::default() };
        // una voce vecchia con l'id del dispositivo passa sotto l'id utente
        g.status_in("d1", Status { today: 100, day: day.clone(), at: now, ..Default::default() }, now);
        g.status_in(
            "d1",
            Status { uid: uid.clone(), today: 600, day: day.clone(), at: now, ..Default::default() },
            now,
        );
        g.status_in(
            "d2",
            Status {
                uid: uid.clone(),
                today: 300,
                day: day.clone(),
                at: now,
                state: State::Studying,
                ..Default::default()
            },
            now,
        );
        assert_eq!(g.members.len(), 1);
        let (c, on) = combine(&g.members[&uid].device_list(now + 10), now + 10, today);
        assert!(on);
        assert_eq!(c.state, State::Studying);
        assert_eq!(c.today, 600 + 300 + 10);
        // un dispositivo che tace da troppo non è più in pista, ma il suo tempo resta
        let (c, on) = combine(&g.members[&uid].device_list(now + 100), now + 100, today);
        assert!(!on && c.state == State::Idle && c.today >= 900);
        // solo l'amministratore toglie membri, e solo con impostazioni più recenti
        assert_eq!(g.apply_info("me", "altro", "N", "", std::slice::from_ref(&uid), now), InfoResult::Ignored);
        assert_eq!(g.apply_info("me", "boss", "Nuovo", "", std::slice::from_ref(&uid), now), InfoResult::Applied);
        assert!(g.members.is_empty() && g.name == "Nuovo");
        g.status_in("d2", Status { uid: uid.clone(), ..Default::default() }, now);
        assert!(g.members.is_empty());
        assert_eq!(g.apply_info("me", "boss", "Nuovo", "", std::slice::from_ref(&uid), now), InfoResult::Ignored);
        assert_eq!(g.apply_info("me", "boss", "Nuovo", "", &["me".into()], now + 1), InfoResult::Removed);
    }

    #[test]
    fn pit_stops_stay_off_track_and_apart() {
        // sul rettilineo in basso dell'ovale: fuori, prima fila
        assert_eq!(pit_places(0, 2, &[0.0]), vec![(1.0, 0)]);
        // ovunque c'è un posto vicino alla pista
        for c in 0..circuits() {
            for i in 0..50 {
                let (_, level) = pit_places(c, 3, &[i as f64 / 50.0])[0];
                assert!(level < 4, "circuito {c}, frazione {i}/50");
            }
        }
        // tre in pausa nello stesso punto: tre posti diversi
        let p = pit_places(0, 3, &[0.1, 0.1, 0.1]);
        assert!(p[0] != p[1] && p[1] != p[2] && p[0] != p[2]);
    }
}
