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
use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Durata di un giro del circuito: 1 minuto di studio.
pub const LAP_SECS: u64 = 60;
/// Tratto di studio più lungo che si conta senza che il dispositivo si rifaccia vivo.
const MAX_RUN: i64 = 6 * 3600;
/// Giorni della mappa di calore mandati agli altri (12 settimane).
pub const HIST_DAYS: usize = 84;
/// Giorni per cui si manda anche su cosa si è studiato (il mese del calendario del gruppo).
pub const WHAT_DAYS: usize = 42;
/// Attività ricordate per giorno (le più lunghe).
const WHAT_TOP: usize = 3;
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
    /// identificativi usati prima da questo utente (es. prima di impostare la password di
    /// sincronizzazione): chi li riceve sposta lì sotto membro, amministratore e rimossi
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prev: Vec<String>,
    /// secondi di studio del mese di `day` (dal primo del mese alle 00:00)
    #[serde(default)]
    pub month: u64,
    /// inizio della sessione di studio in corso (secondi Unix), 0 se non ce n'è: tra i propri
    /// dispositivi ne resta aperta una sola, la prima iniziata
    #[serde(default)]
    pub since: i64,
    /// studio degli ultimi giorni, per la mappa di calore (mandato ogni tanto, non sempre)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hist: Option<Hist>,
    /// profilo: numeri dell'esame attivo (mandato con la mappa di calore)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prof: Option<Prof>,
    /// punti (vedi [`crate::score`]) di oggi, settimana, mese e in tutto, all'istante `at`
    #[serde(default)]
    pub pts: [f64; 4],
    /// moltiplicatore in corso (0 nelle versioni senza punteggio = ×1) ed effetti visibili
    #[serde(default)]
    pub mult: f32,
    #[serde(default)]
    pub fx: u8,
    /// intensità del ritmo (0..1)
    #[serde(default)]
    pub level: f32,
    /// concentrazione attiva su questo dispositivo: vale anche per la sessione di studio aperta
    /// su un altro dispositivo dello stesso utente (i moltiplicatori si sommano)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focus: bool,
}

/// Numeri del profilo per l'esame su cui si studia; a zero le funzioni non usate (che il
/// profilo non mostra).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Prof {
    /// nome dell'esame (pacchetto caricato)
    pub exam: String,
    /// risposte ai quiz oggi e in tutto, e quante giuste
    pub q_today: u32,
    pub q_today_ok: u32,
    pub q_all: u32,
    pub q_all_ok: u32,
    /// simulazioni d'esame fatte
    pub sims: u32,
    /// flashcard ripassate oggi e in tutto
    pub cards_today: u32,
    pub cards_all: u32,
    /// obiettivi di oggi completati e fissati (0 = non usati)
    pub goals_done: u32,
    pub goals_total: u32,
}

/// Secondi di studio per giorno: `secs[0]` è il giorno `day`, `secs[i]` quello di `i` giorni prima.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Hist {
    pub day: String,
    pub secs: Vec<u32>,
    /// su cosa (attività, secondi), per i primi [`WHAT_DAYS`] giorni
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub what: Vec<Vec<(String, u32)>>,
}

impl Hist {
    /// I giorni riallineati su `today` (`[0]` = oggi), lunghi [`HIST_DAYS`].
    pub fn on(&self, today: NaiveDate) -> Vec<u32> {
        let Ok(d) = NaiveDate::parse_from_str(&self.day, "%Y-%m-%d") else { return vec![0; HIST_DAYS] };
        let off = (today - d).num_days();
        (0..HIST_DAYS as i64)
            .map(|i| if i - off < 0 { 0 } else { self.secs.get((i - off) as usize).copied().unwrap_or(0) })
            .collect()
    }

    /// Su cosa si è studiato `i` giorni prima di `today`.
    pub fn what_on(&self, today: NaiveDate, i: usize) -> Vec<(String, u32)> {
        let Ok(d) = NaiveDate::parse_from_str(&self.day, "%Y-%m-%d") else { return vec![] };
        let k = i as i64 - (today - d).num_days();
        if k < 0 {
            return vec![];
        }
        self.what.get(k as usize).cloned().unwrap_or_default()
    }
}

/// Le attività più lunghe, dalla più lunga.
fn top(map: &BTreeMap<String, u64>) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = map.iter().map(|(k, x)| (k.clone(), (*x).min(u32::MAX as u64) as u32)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(WHAT_TOP);
    v
}

/// L'istante `now` e l'inizio esatto (mezzanotte locale) del giorno, della settimana (lunedì)
/// e del mese che lo contengono: «oggi», «settimana» e «mese» vanno da lì fino a `now`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    pub now: i64,
    pub today: NaiveDate,
    pub day0: i64,
    pub week0: i64,
    pub month0: i64,
}

/// Mezzanotte del giorno `d` nel fuso `tz` (dove l'ora legale salta la mezzanotte, il primo
/// istante del giorno).
fn midnight<Tz: TimeZone>(tz: &Tz, d: NaiveDate) -> i64 {
    (0..3)
        .find_map(|h| tz.from_local_datetime(&d.and_hms_opt(h, 0, 0)?).earliest().map(|x| x.timestamp()))
        .unwrap_or_else(|| d.and_hms_opt(0, 0, 0).map(|x| x.and_utc().timestamp()).unwrap_or(0))
}

impl Clock {
    pub fn new<Tz: TimeZone>(now: i64, tz: &Tz) -> Clock {
        let today = tz.timestamp_opt(now, 0).earliest().map(|d| d.date_naive()).unwrap_or_default();
        Clock {
            now,
            today,
            day0: midnight(tz, today),
            week0: midnight(tz, monday(today)),
            month0: midnight(tz, today.with_day(1).unwrap_or(today)),
        }
    }

    /// Con il fuso del dispositivo.
    pub fn local(now: i64) -> Clock {
        Clock::new(now, &chrono::Local)
    }
}

/// Secondi del tratto `start..end` che cadono dopo `from`.
fn part(start: i64, end: i64, from: i64) -> u64 {
    (end - start.max(from)).max(0) as u64
}

fn same_month(a: NaiveDate, b: NaiveDate) -> bool {
    (a.year(), a.month()) == (b.year(), b.month())
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
    pub fn update(&mut self, dev: &str, mut status: Status, now: i64) {
        // la mappa di calore arriva solo ogni tanto: resta quella di prima
        if status.hist.is_none() {
            status.hist = self.devices.get(dev).and_then(|d| d.status.hist.clone());
        }
        if status.prof.is_none() {
            status.prof = self.devices.get(dev).and_then(|d| d.status.prof.clone());
        }
        self.devices.insert(dev.to_string(), DevStatus { status: status.clone(), last_seen: now });
        self.status = status;
        self.last_seen = now;
    }

    /// Stati dei dispositivi con «collegato» (i membri salvati da versioni precedenti hanno
    /// solo `status`).
    pub fn device_list(&self, c: &Clock) -> Vec<(Status, bool)> {
        if self.devices.is_empty() {
            return vec![frozen(&self.status, self.last_seen, c)];
        }
        self.devices.values().map(|d| frozen(&d.status, d.last_seen, c)).collect()
    }
}

/// Lo stato di un dispositivo e se è collegato. Chi non si fa più sentire smette di contare
/// all'ultima volta che è stato visto (non continua a «studiare» per ore).
pub fn frozen(s: &Status, last_seen: i64, c: &Clock) -> (Status, bool) {
    if c.now - last_seen < ONLINE_SECS {
        return (s.clone(), true);
    }
    let mut f = s.clone();
    let to = last_seen.max(f.at);
    let (t, w, m, tot) = advance(s, to, c);
    f.pts = advance_pts(s, to, c);
    f.today = t;
    f.week = w;
    f.month = m;
    f.total = tot;
    f.day = c.today.format("%Y-%m-%d").to_string();
    f.at = to;
    f.state = State::Idle;
    (f, false)
}

/// Stato complessivo di un utente a partire da quelli dei suoi dispositivi, aggiornato a `now`:
/// i tempi si sommano (ogni dispositivo conta solo il proprio studio); in pista se almeno un
/// dispositivo collegato studia, al pit stop se almeno uno è in pausa. Nome, colore e foto
/// vengono dal dispositivo sentito più di recente. Restituisce anche «collegato».
pub fn combine(devs: &[(Status, bool)], c: &Clock) -> (Status, bool) {
    let (now, today) = (c.now, c.today);
    let Some(latest) = devs.iter().max_by_key(|(s, _)| s.at) else { return (Status::default(), false) };
    let mut out = latest.0.clone();
    let (mut t, mut w, mut m, mut tot) = (0, 0, 0, 0);
    let mut pts = [0.0f64; 4];
    let (mut mult, mut fx, mut level) = (0.0f32, 0u8, 0.0f32);
    let mut hist = vec![0u32; HIST_DAYS];
    let mut what: Vec<BTreeMap<String, u64>> = vec![BTreeMap::new(); WHAT_DAYS];
    let mut any_hist = false;
    let mut pits = Vec::new();
    let mut state = State::Idle;
    let mut pit_since = 0;
    let mut doing = String::new();
    // colore: quello del dispositivo che ha aperto la sessione in corso (se c'è)
    let mut session_color: Option<u32> = None;
    for (s, on) in devs {
        if *on && s.state != State::Idle && (session_color.is_none() || s.state == State::Studying) {
            session_color = Some(s.color);
        }
        let (a, b, mm, d) = live(s, c);
        for (x, y) in pts.iter_mut().zip(live_pts(s, c)) {
            *x += y;
        }
        // moltiplicatore ed effetti: quelli del dispositivo che sta studiando
        if *on && s.state == State::Studying && s.mult >= mult {
            mult = s.mult;
            fx = s.fx;
            level = s.level;
        }
        t += a;
        w += b;
        m += mm;
        tot += d;
        if let Some(h) = &s.hist {
            any_hist = true;
            for (x, y) in hist.iter_mut().zip(h.on(today)) {
                *x += y;
            }
            for (i, w) in what.iter_mut().enumerate() {
                for (k, v) in h.what_on(today, i) {
                    *w.entry(k).or_default() += v as u64;
                }
            }
        }
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
    if let Some(c) = session_color {
        out.color = c;
    }
    out.today = t;
    out.week = w;
    out.month = m;
    out.total = tot;
    out.pts = pts;
    // i punti qui sopra sono già quelli giusti: ×1 se nessuno studia (0 vorrebbe dire «versione
    // senza punteggio» e li si ricalcolerebbe dai secondi)
    out.mult = if mult > 0.0 { mult } else { 1.0 };
    out.fx = fx;
    out.level = level;
    out.day = today.format("%Y-%m-%d").to_string();
    out.at = now;
    out.hist = any_hist.then(|| Hist { day: out.day.clone(), secs: hist, what: what.iter().map(top).collect() });
    // profilo: quello del dispositivo sentito più di recente che lo ha mandato (i progressi
    // sono sincronizzati tra i propri dispositivi, quindi i numeri coincidono)
    out.prof =
        devs.iter().filter(|(s, _)| s.prof.is_some()).max_by_key(|(s, _)| s.at).and_then(|(s, _)| s.prof.clone());
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
    let seed = crate::p2p::Room::derive_seed(username, password);
    crate::p2p::Room::user_id_from_seed(&seed)
}

/// Identificativo con la formula delle versioni precedenti (hash veloce di nome utente e
/// password), solo per riprendere la propria identità nei gruppi dopo il passaggio da quelle
/// versioni: i membri dei gruppi lo conoscono già, quindi riusarlo non espone nulla di nuovo.
/// I nuovi utenti usano [`user_id`].
pub fn legacy_user_id(username: &str, password: &str) -> String {
    let h = crate::p2p::sha256_hex(format!("tp-uid\n{}\n{}", username.trim().to_lowercase(), password).as_bytes());
    format!("u{}", &h[..15])
}

/// Come [`pick_user_id`], ma tra due id ricavati dalla password resta il più vecchio (`since`,
/// 0 = da prima che si registrasse): un dispositivo appena installato si allinea all'id che i
/// gruppi già conoscono, così aggiornando nessuno perde gruppi e ruoli.
pub fn pick_user_id_since<'a>(a: &'a str, a_since: i64, b: &'a str, b_since: i64) -> &'a str {
    let derived = |x: &str| x.starts_with('u') && x.len() == 16;
    if derived(a) && derived(b) && a_since != b_since {
        return if a_since < b_since { a } else { b };
    }
    pick_user_id(a, b)
}

/// Tra due identificativi dello stesso utente (due suoi dispositivi) quello da tenere: uno
/// ricavato da nome utente e password (`u…`) vince su quello di un dispositivo, e a parità il
/// minore. Tutti i dispositivi arrivano così allo stesso, in qualunque ordine si incontrino.
pub fn pick_user_id<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.is_empty() {
        return b;
    }
    if b.is_empty() {
        return a;
    }
    let derived = |x: &str| x.starts_with('u') && x.len() == 16;
    match (derived(a), derived(b)) {
        (true, false) => a,
        (false, true) => b,
        _ => a.min(b),
    }
}

/// Hash breve di una foto (nome del file e riferimento nei messaggi).
pub fn photo_hash(bytes: &[u8]) -> String {
    crate::p2p::sha256_hex(bytes)[..16].to_string()
}

const MAX_INFO_SKEW: i64 = 300;

fn hex_enc(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hex_dec<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Genera una coppia di chiavi BIP-340 Schnorr `(sk_hex, pk_hex)` per autenticare l'amministratore del gruppo.
pub fn new_admin_keypair() -> (String, String) {
    let mut b = [0u8; 32];
    loop {
        getrandom::getrandom(&mut b).expect("generatore casuale");
        if let Ok(sk) = k256::schnorr::SigningKey::from_bytes(&k256::FieldBytes::from(b)) {
            let pk = hex_enc(&sk.verifying_key().to_bytes());
            return (hex_enc(&b), pk);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn info_digest(
    code: &str,
    admin: &str,
    name: &str,
    photo: &str,
    removed: &[String],
    at: i64,
    circuit: Option<u32>,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    // con il circuito scelto dall'amministratore la firma copre anche quello (v2); senza resta
    // quella di prima (v1)
    h.update(if circuit.is_some() { b"tp-group-info-v2\0" } else { b"tp-group-info-v1\0" });
    let push_str = |h: &mut Sha256, s: &str| {
        h.update((s.len() as u32).to_be_bytes());
        h.update(s.as_bytes());
    };
    push_str(&mut h, code);
    push_str(&mut h, admin);
    push_str(&mut h, name);
    push_str(&mut h, photo);
    h.update((removed.len() as u32).to_be_bytes());
    for r in removed {
        push_str(&mut h, r);
    }
    h.update(at.to_be_bytes());
    if let Some(c) = circuit {
        h.update(c.to_be_bytes());
    }
    h.finalize().into()
}

/// Firma un `GroupMsg::Info` con la chiave privata dell'amministratore.
#[allow(clippy::too_many_arguments)]
pub fn sign_info(
    code: &str,
    admin: &str,
    name: &str,
    photo: &str,
    removed: &[String],
    at: i64,
    circuit: Option<u32>,
    sk_hex: &str,
) -> Option<String> {
    let sk_bytes: [u8; 32] = hex_dec(sk_hex.trim())?;
    let sk = k256::schnorr::SigningKey::from_bytes(&k256::FieldBytes::from(sk_bytes)).ok()?;
    let mut aux = [0u8; 32];
    getrandom::getrandom(&mut aux).expect("generatore casuale");
    let digest = info_digest(code, admin, name, photo, removed, at, circuit);
    let sig = sk.sign_raw(&digest, &aux).ok()?;
    Some(hex_enc(&sig.to_bytes()))
}

/// Verifica la firma di un `GroupMsg::Info` con la chiave pubblica dell'amministratore.
#[allow(clippy::too_many_arguments)]
pub fn verify_info_sig(
    code: &str,
    admin: &str,
    name: &str,
    photo: &str,
    removed: &[String],
    at: i64,
    circuit: Option<u32>,
    pk_hex: &str,
    sig_hex: &str,
) -> bool {
    use k256::schnorr::signature::hazmat::PrehashVerifier;
    let Some(pk_bytes) = hex_dec::<32>(pk_hex.trim()) else {
        return false;
    };
    let Some(sig_bytes) = hex_dec::<64>(sig_hex.trim()) else {
        return false;
    };
    let Ok(vk) = k256::schnorr::VerifyingKey::from_bytes(&k256::FieldBytes::from(pk_bytes)) else {
        return false;
    };
    let Ok(sig) = k256::schnorr::Signature::try_from(sig_bytes.as_slice()) else {
        return false;
    };
    let digest = info_digest(code, admin, name, photo, removed, at, circuit);
    vk.verify_prehash(&digest, &sig).is_ok()
}

/// Messaggi del gruppo oltre allo stato di ciascuno.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "g", rename_all = "snake_case")]
pub enum GroupMsg {
    /// impostazioni decise dall'amministratore (chi ha creato il gruppo): nome, foto, membri
    /// rimossi. `at` = quando sono state cambiate (vince la più recente), firmate con `pk`/`sig`.
    Info {
        admin: String,
        name: String,
        photo: String,
        removed: Vec<String>,
        at: i64,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        pk: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        sig: String,
        /// circuito scelto dall'amministratore (assente = uno diverso ogni giorno)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        circuit: Option<u32>,
    },
    /// richiesta di una foto che non si ha (la manda chi ce l'ha)
    PhotoGet { hash: String },
    /// una foto (JPEG in base64; le versioni nuove usano `p2p::Msg::GroupPhoto`)
    Photo { hash: String, data: String },
    /// avviso: «vieni a studiare» a un membro (`to`) o a tutti (`to` vuoto)
    Ping { id: String, from: String, name: String, to: String, at: i64, pit: bool },
    /// schede dei membri (stato, tempi, punti, foto, mappa di calore), ripetute da chi le
    /// conosce o conservate sui relay: arrivano anche quando l'interessato è offline
    Cards { cards: Vec<Card> },
}

/// Ultimo stato noto di un dispositivo di un membro, con quando si è fatto sentire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub dev: String,
    pub seen: i64,
    pub status: Status,
}

/// Schede ripetute da altri: al massimo questi dispositivi, sentiti negli ultimi giorni.
const CARDS_MAX: usize = 60;
const CARDS_DAYS: i64 = 90;

/// Divide le schede in gruppi che stanno in un messaggio (`max` byte di JSON ciascuno).
pub fn card_chunks(cards: Vec<Card>, max: usize) -> Vec<Vec<Card>> {
    let mut out: Vec<Vec<Card>> = Vec::new();
    let mut size = 0;
    for c in cards {
        let n = serde_json::to_string(&c).map(|j| j.len()).unwrap_or(0);
        if n > max {
            continue;
        }
        if out.is_empty() || size + n > max {
            out.push(Vec::new());
            size = 2;
        }
        size += n + 1;
        out.last_mut().expect("gruppo").push(c);
    }
    out
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
    /// chiave pubblica dell'amministratore del gruppo (BIP-340 hex)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub admin_pk: String,
    /// chiave privata dell'amministratore (solo su dispositivi dell'admin; sincronizzata via Sess::Sync)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub admin_sk: String,
    /// ultima firma valida delle impostazioni correnti (per poterle ripetere agli altri membri)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub admin_sig: String,
    /// utenti rimossi dall'amministratore
    #[serde(default)]
    pub removed: Vec<String>,
    /// foto del gruppo (hash)
    #[serde(default)]
    pub photo: String,
    /// quando l'amministratore ha cambiato le impostazioni l'ultima volta
    #[serde(default)]
    pub info_at: i64,
    /// circuito fissato dall'amministratore; `None` = uno diverso ogni giorno
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circuit: Option<u32>,
    /// istante dell'ultimo avviso già mostrato (per non ripeterli)
    #[serde(default)]
    pub ping_seen: i64,
    /// quando ci si è entrati da questo dispositivo; 0 = gruppo salvato da una versione
    /// precedente all'amministratore (vedi [`Group::legacy`])
    #[serde(default)]
    pub joined_at: i64,
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
    /// Gruppo creato prima che esistesse l'amministratore: nessuno lo è, e chi lo rinomina per
    /// primo lo diventa. Non vale per chi entra adesso in un gruppo il cui amministratore non
    /// si è ancora fatto sentire.
    pub fn legacy(&self) -> bool {
        self.admin.is_empty() && self.joined_at == 0
    }

    /// Si può rinominare (e cambiare la foto): l'amministratore, o chiunque nei gruppi
    /// precedenti all'amministratore.
    pub fn can_admin(&self, me: &str) -> bool {
        (!self.admin.is_empty() && self.admin == me) || self.legacy()
    }

    pub fn ensure_admin_keys(&mut self) {
        if self.admin_sk.is_empty() {
            let (sk, pk) = new_admin_keypair();
            self.admin_sk = sk;
            self.admin_pk = pk;
        } else if self.admin_pk.is_empty() {
            if let Some(sk_bytes) = hex_dec::<32>(self.admin_sk.trim()) {
                if let Ok(sk) = k256::schnorr::SigningKey::from_bytes(&k256::FieldBytes::from(sk_bytes)) {
                    self.admin_pk = hex_enc(&sk.verifying_key().to_bytes());
                }
            }
        }
    }

    pub fn sign_current_info(&mut self) {
        self.ensure_admin_keys();
        // si firma il nome come lo salveranno gli altri, così chi lo ripete lo ripete uguale
        let name: String = self.name.trim().chars().take(60).collect();
        if !name.is_empty() {
            self.name = name;
        }
        if let Some(sig) = sign_info(
            &self.code,
            &self.admin,
            &self.name,
            &self.photo,
            &self.removed,
            self.info_at,
            self.circuit,
            &self.admin_sk,
        ) {
            self.admin_sig = sig;
        }
    }

    /// Sposta solo i dispositivi del membro sotto `new` in seguito a `status.prev`, senza mai
    /// toccare privilegi amministrativi o lista rimossi.
    fn merge_peer_member(&mut self, old: &str, new: &str) {
        if self.removed.contains(&old.to_string()) {
            return;
        }
        if old == self.admin {
            // gruppo firmato: l'amministratore è la sua chiave e cambia id solo con impostazioni
            // firmate; gruppo di prima delle firme: come nelle versioni precedenti
            if !self.admin_pk.is_empty() {
                return;
            }
            self.admin = new.to_string();
        }
        if let Some(m) = self.members.remove(old) {
            let e = self.members.entry(new.to_string()).or_default();
            for (d, s) in m.devices {
                e.devices.entry(d).or_insert(s);
            }
            if m.last_seen > e.last_seen {
                e.status = m.status;
                e.last_seen = m.last_seen;
            }
        }
    }

    /// Stato (o membro) di un utente, che arrivi da un dispositivo nuovo o già noto. Le voci
    /// salvate con l'id del dispositivo (versioni precedenti) passano sotto l'id utente.
    pub fn status_in(&mut self, dev: &str, status: Status, now: i64) {
        let uid = if status.uid.is_empty() { dev.to_string() } else { status.uid.clone() };
        // id precedenti dello stesso utente (e il dispositivo, per le versioni vecchie)
        let mut olds: Vec<String> = status.prev.clone();
        olds.push(dev.to_string());
        if self.removed.contains(&uid) || olds.iter().any(|o| self.removed.contains(o)) {
            // `removed` resta quello firmato dall'amministratore (si ripete agli altri così com'è)
            self.members.remove(&uid);
            return;
        }
        for old in olds.iter().filter(|o| **o != uid) {
            self.merge_peer_member(old, &uid);
        }
        // uno stato più vecchio di quello già noto (arrivato in ritardo per un'altra strada)
        // non deve annullare quello nuovo
        if let Some(d) = self.members.get(&uid).and_then(|m| m.devices.get(dev)) {
            if status.at < d.status.at {
                return;
            }
        }
        self.members.entry(uid).or_default().update(dev, status, now);
    }

    /// Schede da ripetere agli altri: i dispositivi dei membri che ora non sono collegati (chi
    /// è collegato manda da sé il proprio stato), dal più recente, tranne `skip`.
    pub fn cards(&self, now: i64, skip: &str) -> Vec<Card> {
        let mut v: Vec<Card> = self
            .members
            .iter()
            .filter(|(uid, _)| !self.removed.contains(uid))
            .flat_map(|(_, m)| m.devices.iter())
            .filter(|(dev, d)| {
                dev.as_str() != skip && now - d.last_seen >= ONLINE_SECS && now - d.last_seen < CARDS_DAYS * 86400
            })
            .map(|(dev, d)| Card { dev: dev.clone(), seen: d.last_seen, status: d.status.clone() })
            .collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.seen));
        v.truncate(CARDS_MAX);
        v
    }

    /// Applica una scheda ripetuta da un altro (o conservata sui relay): vale solo se più
    /// recente di quanto già si sa di quel dispositivo, e non lo fa mai sembrare collegato
    /// (conta fino a quando si è fatto sentire). `true` se ha cambiato qualcosa.
    pub fn card_in(&mut self, card: Card, now: i64) -> bool {
        let Card { dev, seen, mut status } = card;
        let uid = if status.uid.is_empty() { dev.clone() } else { status.uid.clone() };
        if dev.is_empty()
            || uid.is_empty()
            || self.removed.contains(&uid)
            || self.removed.contains(&dev)
            || status.prev.iter().any(|p| self.removed.contains(p))
        {
            return false;
        }
        let seen = seen.min(now - ONLINE_SECS);
        let m = self.members.entry(uid).or_default();
        let old = m.devices.get(&dev);
        if old.is_some_and(|d| status.at <= d.status.at) {
            return false;
        }
        if status.hist.is_none() {
            status.hist = old.and_then(|d| d.status.hist.clone());
        }
        if status.prof.is_none() {
            status.prof = old.and_then(|d| d.status.prof.clone());
        }
        let last_seen = old.map_or(seen, |d| d.last_seen.max(seen));
        if status.at >= m.status.at || m.devices.is_empty() {
            m.status = status.clone();
        }
        m.last_seen = m.last_seen.max(last_seen);
        m.devices.insert(dev, DevStatus { status, last_seen });
        true
    }

    /// Sposta un utente sotto un nuovo identificativo: voce del membro, amministratore e
    /// rimossi.
    pub fn rename_user(&mut self, old: &str, new: &str) {
        if let Some(m) = self.members.remove(old) {
            let e = self.members.entry(new.to_string()).or_default();
            for (d, s) in m.devices {
                e.devices.entry(d).or_insert(s);
            }
            if m.last_seen > e.last_seen {
                e.status = m.status;
                e.last_seen = m.last_seen;
            }
        }
        if self.admin == old {
            self.admin = new.to_string();
        }
        for r in self.removed.iter_mut() {
            if r == old {
                *r = new.to_string();
            }
        }
    }

    /// Applica le impostazioni dell'amministratore autenticate dalla sua firma Schnorr.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_info(
        &mut self,
        me: &str,
        admin: &str,
        name: &str,
        photo: &str,
        removed: &[String],
        at: i64,
        pk: &str,
        sig: &str,
        circuit: Option<u32>,
        now: i64,
    ) -> InfoResult {
        // impostazioni senza firma (versioni precedenti, compresa la privata): valgono solo nei
        // gruppi nati prima delle firme e solo per chi è l'amministratore, nome e foto, come
        // allora; mai per togliere membri
        if pk.is_empty() && sig.is_empty() {
            if admin.is_empty() || !self.admin_pk.is_empty() || at <= self.info_at || at > now + MAX_INFO_SKEW {
                return InfoResult::Ignored;
            }
            if !self.admin.is_empty() && self.admin != admin {
                return InfoResult::Ignored;
            }
            self.admin = admin.to_string();
            if !name.trim().is_empty() {
                self.name = name.trim().chars().take(60).collect();
            }
            self.photo = photo.to_string();
            self.info_at = at;
            return InfoResult::Applied;
        }
        if admin.is_empty()
            || pk.is_empty()
            || sig.is_empty()
            || at <= 0
            || at <= self.info_at
            || at > now + MAX_INFO_SKEW
        {
            return InfoResult::Ignored;
        }
        if !verify_info_sig(&self.code, admin, name, photo, removed, at, circuit, pk, sig) {
            return InfoResult::Ignored;
        }
        if !self.admin_pk.is_empty() {
            if self.admin_pk != pk {
                return InfoResult::Ignored;
            }
        } else if !self.admin.is_empty() && self.admin != admin {
            return InfoResult::Ignored;
        }
        self.admin = admin.to_string();
        self.admin_pk = pk.to_string();
        self.admin_sig = sig.to_string();
        if !name.trim().is_empty() {
            self.name = name.trim().chars().take(60).collect();
        }
        self.photo = photo.to_string();
        self.removed = removed.to_vec();
        self.circuit = circuit;
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
    /// gruppi lasciati (codice → quando), perché l'uscita valga anche sugli altri propri
    /// dispositivi e un gruppo lasciato non ritorni con la sincronizzazione
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub left: BTreeMap<String, i64>,
}

/// Un gruppo come lo si passa ai propri altri dispositivi (senza membri né stati: quelli
/// arrivano entrando nella stanza del gruppo).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GroupRef {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub admin: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub admin_pk: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub admin_sk: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub admin_sig: String,
    #[serde(default)]
    pub photo: String,
    #[serde(default)]
    pub info_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circuit: Option<u32>,
    #[serde(default)]
    pub joined_at: i64,
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
    /// Aggiunge il gruppo (se non c'è già; altrimenti nome e dati restano quelli che ha) e lo
    /// rende attivo. `now` = quando ci si entra (vedi [`Group::legacy`]).
    pub fn join(&mut self, code: &str, name: &str, now: i64) {
        let code = normalize(code);
        if !self.groups.iter().any(|g| g.code == code) {
            let name = if name.trim().is_empty() { "Gruppo di studio".to_string() } else { name.trim().to_string() };
            self.groups.push(Group { code: code.clone(), name, joined_at: now.max(1), ..Default::default() });
        }
        self.active = code;
    }

    /// Il proprio id utente è cambiato: lo si aggiorna in tutti i gruppi (membro,
    /// amministratore, rimossi), così chi aveva creato un gruppo ne resta amministratore.
    pub fn rename_user(&mut self, old: &str, new: &str) {
        for g in &mut self.groups {
            g.rename_user(old, new);
        }
    }

    pub fn leave(&mut self, code: &str) {
        self.leave_at(code, chrono::Utc::now().timestamp());
    }

    /// Esce dal gruppo e ricorda quando (vedi [`Groups::left`]).
    pub fn leave_at(&mut self, code: &str, now: i64) {
        self.groups.retain(|g| g.code != code);
        self.left.insert(code.to_string(), now.max(1));
        if self.active == code {
            self.active = self.groups.first().map(|g| g.code.clone()).unwrap_or_default();
        }
    }

    /// I propri gruppi da passare agli altri propri dispositivi.
    pub fn refs(&self) -> Vec<GroupRef> {
        self.groups
            .iter()
            .map(|g| GroupRef {
                code: g.code.clone(),
                name: g.name.clone(),
                admin: g.admin.clone(),
                admin_pk: g.admin_pk.clone(),
                admin_sk: g.admin_sk.clone(),
                admin_sig: g.admin_sig.clone(),
                photo: g.photo.clone(),
                info_at: g.info_at,
                circuit: g.circuit,
                joined_at: g.joined_at,
            })
            .collect()
    }

    /// Unisce i gruppi di un altro proprio dispositivo: aggiunge quelli che mancano (salvo
    /// quelli lasciati dopo esserci entrati), toglie quelli lasciati là dopo esserci entrati
    /// qui e, a parità di gruppo, tiene nome, foto e amministratore più recenti. `true` se
    /// qualcosa è cambiato.
    pub fn merge_own(&mut self, refs: &[GroupRef], left: &BTreeMap<String, i64>) -> bool {
        let before = (self.refs(), self.left.clone(), self.active.clone());
        for (code, &at) in left {
            let mine = self.left.entry(code.clone()).or_insert(0);
            *mine = (*mine).max(at);
            if self.groups.iter().any(|g| &g.code == code && g.joined_at <= at) {
                self.groups.retain(|g| &g.code != code);
            }
        }
        for r in refs {
            if self.left.get(&r.code).is_some_and(|&at| at >= r.joined_at) {
                continue;
            }
            match self.groups.iter_mut().find(|g| g.code == r.code) {
                None => self.groups.push(Group {
                    code: r.code.clone(),
                    name: r.name.clone(),
                    admin: r.admin.clone(),
                    admin_pk: r.admin_pk.clone(),
                    admin_sk: r.admin_sk.clone(),
                    admin_sig: r.admin_sig.clone(),
                    photo: r.photo.clone(),
                    info_at: r.info_at,
                    circuit: r.circuit,
                    joined_at: r.joined_at.max(1),
                    ..Default::default()
                }),
                Some(g) => {
                    if r.info_at > g.info_at {
                        g.name = r.name.clone();
                        g.photo = r.photo.clone();
                        g.circuit = r.circuit;
                        g.info_at = r.info_at;
                        if !r.admin.is_empty() {
                            g.admin = r.admin.clone();
                        }
                        if !r.admin_pk.is_empty() {
                            g.admin_pk = r.admin_pk.clone();
                        }
                        if !r.admin_sig.is_empty() {
                            g.admin_sig = r.admin_sig.clone();
                        }
                    }
                    if g.admin.is_empty() && !r.admin.is_empty() {
                        g.admin = r.admin.clone();
                    }
                    if g.admin_pk.is_empty() && !r.admin_pk.is_empty() {
                        g.admin_pk = r.admin_pk.clone();
                    }
                    if g.admin_sk.is_empty() && !r.admin_sk.is_empty() {
                        g.admin_sk = r.admin_sk.clone();
                    }
                }
            }
        }
        if !self.groups.iter().any(|g| g.code == self.active) {
            self.active = self.groups.first().map(|g| g.code.clone()).unwrap_or_default();
        }
        (self.refs(), self.left.clone(), self.active.clone()) != before
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
    crate::p2p::Room::derive_code("gruppo", &normalize(code).replace('-', ""))
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

impl From<DevLog> for StudyLog {
    fn from(d: DevLog) -> StudyLog {
        StudyLog { days: d.days, pts: d.pts, what: d.what, ..Default::default() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StudyLog {
    #[serde(default)]
    pub days: BTreeMap<String, u64>,
    /// durata dei pit stop finiti, per giorno (solo gli ultimi giorni)
    #[serde(default)]
    pub pits: BTreeMap<String, Vec<u64>>,
    /// su cosa si è studiato, per giorno: attività → secondi (solo le ultime settimane)
    #[serde(default)]
    pub what: BTreeMap<String, BTreeMap<String, u64>>,
    /// punti per giorno (vedi [`crate::score`])
    #[serde(default)]
    pub pts: BTreeMap<String, f64>,
    /// lo studio fatto sugli altri propri dispositivi (id del dispositivo → registro): i
    /// totali sono la somma (vedi [`StudyLog::combined`])
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub others: BTreeMap<String, DevLog>,
}

/// Lo studio fatto su un dispositivo, come lo si passa agli altri propri dispositivi.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DevLog {
    #[serde(default)]
    pub days: BTreeMap<String, u64>,
    #[serde(default)]
    pub pts: BTreeMap<String, f64>,
    /// su cosa (solo le ultime settimane)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub what: BTreeMap<String, BTreeMap<String, u64>>,
}

fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

impl StudyLog {
    pub fn load(base: &Path) -> StudyLog {
        let mut log: StudyLog = vfs::read_to_string(&base.join(LOG_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        // giorni registrati prima del punteggio: un punto al secondo
        let missing: Vec<(String, u64)> =
            log.days.iter().filter(|(d, _)| !log.pts.contains_key(*d)).map(|(d, s)| (d.clone(), *s)).collect();
        for (d, s) in missing {
            log.pts.insert(d, s as f64);
        }
        log
    }

    pub fn save(&self, base: &Path) -> std::io::Result<()> {
        vfs::write(&base.join(LOG_FILE), serde_json::to_string(self)?.as_bytes())
    }

    /// Lo studio fatto su questo dispositivo, da passare agli altri propri dispositivi.
    pub fn own(&self) -> DevLog {
        DevLog { days: self.days.clone(), pts: self.pts.clone(), what: self.what.clone() }
    }

    /// Registro arrivato dall'altro proprio dispositivo `dev` (sostituisce il precedente: è
    /// lui a sapere quanto ha studiato). `true` se è cambiato.
    pub fn set_other(&mut self, dev: &str, log: DevLog) -> bool {
        if dev.is_empty() || self.others.get(dev) == Some(&log) {
            return false;
        }
        self.others.insert(dev.to_string(), log);
        true
    }

    /// Questo dispositivo più gli altri propri: i numeri da mostrare e da mandare al gruppo
    /// (una sessione di studio è aperta su un dispositivo alla volta, quindi niente doppioni).
    pub fn combined(&self) -> StudyLog {
        let mut out = StudyLog { pits: self.pits.clone(), ..self.own().into() };
        for o in self.others.values() {
            for (d, v) in &o.days {
                *out.days.entry(d.clone()).or_default() += v;
            }
            for (d, v) in &o.pts {
                *out.pts.entry(d.clone()).or_default() += v;
            }
            for (d, m) in &o.what {
                let e = out.what.entry(d.clone()).or_default();
                for (k, v) in m {
                    *e.entry(k.clone()).or_default() += v;
                }
            }
        }
        out
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

    /// Registra il tratto di studio `start..end` (secondi Unix) dividendolo alle mezzanotti del
    /// fuso `tz`: le 23:00–01:00 sono un'ora per giorno.
    /// `what` è l'attività (es. «Quiz · Chimica»), per il calendario del gruppo.
    pub fn add_span<Tz: TimeZone>(&mut self, start: i64, end: i64, what: &str, tz: &Tz) {
        let mut cur = start;
        let mut last = None;
        while cur < end {
            let day = Clock::new(cur, tz).today;
            let next = midnight(tz, day + Duration::days(1));
            let stop = if next > cur { end.min(next) } else { end };
            self.add(day, (stop - cur) as u64);
            let label = if what.trim().is_empty() { "Studio" } else { what.trim() };
            *self.what.entry(day.format("%Y-%m-%d").to_string()).or_default().entry(label.to_string()).or_default() +=
                (stop - cur) as u64;
            last = Some(day);
            cur = stop;
        }
        if let Some(day) = last {
            let old = (day - Duration::days(WHAT_DAYS as i64 + 7)).format("%Y-%m-%d").to_string();
            self.what.retain(|k, _| *k >= old);
        }
    }

    /// Punti del tratto `start..end` con moltiplicatore `mult`, divisi alle mezzanotti.
    pub fn add_points<Tz: TimeZone>(&mut self, start: i64, end: i64, mult: f64, tz: &Tz) {
        let mut cur = start;
        while cur < end {
            let day = Clock::new(cur, tz).today;
            let next = midnight(tz, day + Duration::days(1));
            let stop = if next > cur { end.min(next) } else { end };
            *self.pts.entry(day.format("%Y-%m-%d").to_string()).or_default() += (stop - cur) as f64 * mult;
            cur = stop;
        }
    }

    /// Punti di oggi, della settimana, del mese e in tutto (al giorno `day`).
    pub fn points(&self, day: NaiveDate) -> [f64; 4] {
        let get = |d: NaiveDate| self.pts.get(&d.format("%Y-%m-%d").to_string()).copied().unwrap_or(0.0);
        let m = monday(day);
        let week = (0..7).map(|i| m + Duration::days(i)).filter(|d| *d <= day).map(get).sum();
        let first = day.with_day(1).unwrap_or(day).format("%Y-%m-%d").to_string();
        let month = self.pts.range(first..=day.format("%Y-%m-%d").to_string()).map(|(_, v)| v).sum();
        [get(day), week, month, self.pts.values().sum()]
    }

    /// Su cosa si è studiato il giorno `day` (le attività più lunghe).
    pub fn what(&self, day: NaiveDate) -> Vec<(String, u32)> {
        self.what.get(&day.format("%Y-%m-%d").to_string()).map(top).unwrap_or_default()
    }

    pub fn month(&self, day: NaiveDate) -> u64 {
        let first = day.with_day(1).unwrap_or(day).format("%Y-%m-%d").to_string();
        let last = day.format("%Y-%m-%d").to_string();
        self.days.range(first..=last).map(|(_, v)| v).sum()
    }

    /// Gli ultimi [`HIST_DAYS`] giorni fino a `day` (`[0]` = `day`).
    pub fn hist(&self, day: NaiveDate) -> Vec<u32> {
        (0..HIST_DAYS as i64).map(|i| self.day(day - Duration::days(i)).min(u32::MAX as u64) as u32).collect()
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

/// Lo stato da mandare: i numeri del registro più il tratto in corso (iniziato a `since`),
/// diviso esattamente tra oggi, settimana e mese. Con `hist` anche la mappa di calore.
#[allow(clippy::too_many_arguments)]
pub fn my_status(
    name: &str,
    color: u32,
    state: State,
    log: &StudyLog,
    since: Option<i64>,
    c: &Clock,
    hist: bool,
    score: crate::score::Score,
) -> Status {
    let (now, today) = (c.now, c.today);
    let start = since.map(|t| t.clamp(now - MAX_RUN, now)).unwrap_or(now);
    let t = log.day(today) + part(start, now, c.day0);
    let base = log.points(today);
    let m = if state == State::Studying { score.mult } else { 1.0 };
    let pts = [
        base[0] + part(start, now, c.day0) as f64 * m,
        base[1] + part(start, now, c.week0) as f64 * m,
        base[2] + part(start, now, c.month0) as f64 * m,
        base[3] + (now - start) as f64 * m,
    ];
    Status {
        name: name.to_string(),
        color,
        state,
        today: t,
        week: log.week(today) + part(start, now, c.week0),
        month: log.month(today) + part(start, now, c.month0),
        total: log.total() + (now - start) as u64,
        day: today.format("%Y-%m-%d").to_string(),
        at: now,
        pits: log.pits(today),
        pts,
        mult: if state == State::Studying { score.mult as f32 } else { 1.0 },
        fx: if state == State::Studying { score.fx } else { 0 },
        level: score.level,
        hist: hist.then(|| {
            let mut h = log.hist(today);
            h[0] = t.min(u32::MAX as u64) as u32;
            let what = (0..WHAT_DAYS as i64).map(|i| log.what(today - Duration::days(i))).collect();
            Hist { day: today.format("%Y-%m-%d").to_string(), secs: h, what }
        }),
        ..Default::default()
    }
}

/// Classifica: per oggi, per la settimana o in totale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Today,
    Week,
    Month,
    Total,
}

/// Numeri di uno stato portati all'istante `to` (oggi, settimana, mese, totale secondo `c`):
/// chi sta studiando continua a contare, e il tratto in corso si divide esattamente alla
/// mezzanotte, al lunedì alle 00:00 e al primo del mese alle 00:00. I numeri di un giorno, una
/// settimana o un mese ormai passati non contano più.
fn advance(s: &Status, to: i64, c: &Clock) -> (u64, u64, u64, u64) {
    let extra = if s.state == State::Studying { (to - s.at).clamp(0, MAX_RUN) } else { 0 };
    let start = to - extra;
    let day = NaiveDate::parse_from_str(&s.day, "%Y-%m-%d").ok();
    let bt = if day == Some(c.today) { s.today } else { 0 };
    let bw = if day.map(monday) == Some(monday(c.today)) { s.week } else { 0 };
    let bm = if day.is_some_and(|d| same_month(d, c.today)) { s.month } else { 0 };
    (
        bt + part(start, to, c.day0),
        bw + part(start, to, c.week0),
        bm + part(start, to, c.month0),
        s.total + extra as u64,
    )
}

/// Moltiplicatore di uno stato (le versioni senza punteggio contano ×1).
fn mult_of(s: &Status) -> f64 {
    if s.mult > 0.0 {
        s.mult as f64
    } else {
        1.0
    }
}

/// Punti di uno stato portati all'istante `to` (oggi, settimana, mese, totale), con gli stessi
/// confini esatti di [`advance`]: il tratto in corso vale il moltiplicatore per i secondi.
fn advance_pts(s: &Status, to: i64, c: &Clock) -> [f64; 4] {
    let extra = if s.state == State::Studying { (to - s.at).clamp(0, MAX_RUN) } else { 0 };
    let start = to - extra;
    let m = mult_of(s);
    // versioni senza punteggio: un punto al secondo
    let base = if s.mult == 0.0 { [s.today as f64, s.week as f64, s.month as f64, s.total as f64] } else { s.pts };
    let day = NaiveDate::parse_from_str(&s.day, "%Y-%m-%d").ok();
    let bt = if day == Some(c.today) { base[0] } else { 0.0 };
    let bw = if day.map(monday) == Some(monday(c.today)) { base[1] } else { 0.0 };
    let bm = if day.is_some_and(|d| same_month(d, c.today)) { base[2] } else { 0.0 };
    [
        bt + part(start, to, c.day0) as f64 * m,
        bw + part(start, to, c.week0) as f64 * m,
        bm + part(start, to, c.month0) as f64 * m,
        base[3] + extra as f64 * m,
    ]
}

/// Punti di uno stato adesso: (oggi, settimana, mese, totale).
pub fn live_pts(s: &Status, c: &Clock) -> [f64; 4] {
    advance_pts(s, c.now, c)
}

/// Numeri di uno stato adesso: (oggi, settimana, mese, totale) in secondi.
pub fn live(s: &Status, c: &Clock) -> (u64, u64, u64, u64) {
    advance(s, c.now, c)
}

pub fn value(s: &Status, c: &Clock, p: Period) -> u64 {
    let (t, w, m, tot) = live(s, c);
    match p {
        Period::Today => t,
        Period::Week => w,
        Period::Month => m,
        Period::Total => tot,
    }
}

/// Punti nel periodo `p` (la classifica è per punti).
pub fn points(s: &Status, c: &Clock, p: Period) -> f64 {
    live_pts(s, c)[match p {
        Period::Today => 0,
        Period::Week => 1,
        Period::Month => 2,
        Period::Total => 3,
    }]
}

/// Ordine della classifica: (id, stato) dal primo all'ultimo (al secondo).
pub fn standings<'a>(
    members: impl Iterator<Item = (&'a str, &'a Status)>,
    c: &Clock,
    p: Period,
) -> Vec<(&'a str, &'a Status)> {
    let mut v: Vec<(&str, &Status)> = members.collect();
    v.sort_by(|a, b| {
        points(b.1, c, p)
            .total_cmp(&points(a.1, c, p))
            .then(value(b.1, c, p).cmp(&value(a.1, c, p)))
            .then(a.1.name.cmp(&b.1.name))
    });
    v
}

/// Durata con ore, minuti e secondi («1 h 02 min 05 s», «4 min 09 s», «12 s»).
pub fn fmt_secs(s: u64) -> String {
    let (h, m, x) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h} h {m:02} min {x:02} s")
    } else if m > 0 {
        format!("{m} min {x:02} s")
    } else {
        format!("{x} s")
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

/// Nome del circuito `i` (per il selettore dell'amministratore).
pub fn circuit_name(i: usize) -> &'static str {
    LAYOUTS[i % LAYOUTS.len()].0
}

/// Il circuito del giorno: quello fissato dall'amministratore, altrimenti uno diverso ogni
/// giorno fra tutti (`day` = giorni dal 1970, UTC: lo stesso per tutti nel gruppo; dipende anche
/// dal codice, così gruppi diversi non corrono tutti sullo stesso).
pub fn circuit_today(code: &str, fixed: Option<u32>, day: i64) -> usize {
    if let Some(c) = fixed {
        if (c as usize) < LAYOUTS.len() {
            return c as usize;
        }
    }
    let h = code.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let n = LAYOUTS.len() as i64;
    ((day + h as i64 % n).rem_euclid(n)) as usize
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
/// Tratto percorso lungo la pista per uscire fino alla piazzola (px della monoposto, lunga
/// 36): la macchina continua dritta e scarta dolcemente; più lungo per le file lontane, così
/// la sterzata resta morbida. Le stesse costanti sono in `ui/pages/group.slint` (`run-px`).
pub const PIT_RUN_PX: f64 = 170.0;
pub const PIT_RUN_ROW_PX: f64 = 90.0;

/// Frazione di giro percorsa per uscire di pista fino alla piazzola della fila `level`.
pub fn pit_run(circuit: usize, members: usize, level: u32) -> f64 {
    let sw = 0.13 / circuit_scale(members).sqrt();
    let px = 0.3 * sw / 20.0;
    (PIT_RUN_PX + level as f64 * PIT_RUN_ROW_PX) * px / circuit_length(circuit)
}

/// Dove si ferma ciascuna macchina al pit stop (`stops` = frazioni di giro in cui ha lasciato
/// la corsia; la piazzola è più avanti di [`pit_run`]): lato (+1 = a destra della marcia, -1 =
/// a sinistra) e fila (0 = la più vicina alla pista). Ogni
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
        let f = f + pit_run(circuit, members, level);
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
                // stessa fila: stessa distanza percorsa per uscire, conta il punto di partenza
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

    const ONE: crate::score::Score = crate::score::Score { mult: 1.0, fx: 0, level: 0.0 };

    #[test]
    fn cards_reach_who_was_offline() {
        let now = 1_000_000;
        let st = |uid: &str, at: i64, total: u64| Status {
            name: uid.to_uppercase(),
            uid: uid.into(),
            at,
            total,
            state: State::Studying,
            ..Default::default()
        };
        // A ha visto B (ora offline) e C (collegato adesso)
        let mut a = Group { code: "X".into(), ..Default::default() };
        a.status_in("b1", st("b", now - 600, 3000), now - 600);
        a.status_in("c1", st("c", now - 5, 100), now - 5);
        let cards = a.cards(now, "a1");
        assert_eq!(cards.len(), 1, "solo chi non è collegato");
        assert_eq!(cards[0].dev, "b1");
        // D arriva dopo e non ha mai incontrato B: la scheda lo porta in classifica, non collegato
        let mut d = Group { code: "X".into(), ..Default::default() };
        assert!(d.card_in(cards[0].clone(), now));
        let m = &d.members["b"];
        assert_eq!(m.status.total, 3000);
        assert!(now - m.devices["b1"].last_seen >= ONLINE_SECS);
        // la stessa scheda (o una più vecchia) non cambia niente; lo stato vero vince
        assert!(!d.card_in(cards[0].clone(), now));
        d.status_in("b1", st("b", now, 3600), now);
        assert!(!d.card_in(cards[0].clone(), now + 1));
        assert_eq!(d.members["b"].status.total, 3600);
        // i rimossi non tornano
        let mut e = Group { code: "X".into(), removed: vec!["b".into()], ..Default::default() };
        assert!(!e.card_in(cards[0].clone(), now));
        // le schede stanno in messaggi piccoli
        let many: Vec<Card> = (0..50).map(|i| Card { dev: format!("d{i}"), ..cards[0].clone() }).collect();
        let chunks = card_chunks(many, 4000);
        assert!(chunks.len() > 1 && chunks.iter().all(|c| serde_json::to_string(c).unwrap().len() <= 4000));
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), 50);
    }

    /// Orologio di prova nel fuso di Roma d'inverno (UTC+1).
    fn clock(y: i32, mo: u32, d: u32, h: u32, mi: u32, sec: u32) -> Clock {
        let tz = chrono::FixedOffset::east_opt(3600).unwrap();
        let now = tz.with_ymd_and_hms(y, mo, d, h, mi, sec).unwrap().timestamp();
        Clock::new(now, &tz)
    }

    #[test]
    fn study_time_and_ranking() {
        let c = clock(2026, 11, 12, 10, 0, 0); // giovedì
        let today = c.today;
        let mut log = StudyLog::default();
        log.add(today, 1800);
        log.add(today - Duration::days(1), 600);
        log.add(today - Duration::days(10), 100);
        assert_eq!(log.day(today), 1800);
        assert_eq!(log.week(today), 2400);
        assert_eq!(log.month(today), 2500);
        assert_eq!(log.total(), 2500);
        let me = my_status("Io", 1, State::Studying, &log, Some(c.now - 60), &c, true, ONE);
        assert_eq!((me.today, me.week, me.month), (1860, 2460, 2560));
        assert_eq!(me.hist.as_ref().unwrap().secs[..2], [1860, 600]);
        // chi studia continua a contare, al secondo
        let later = Clock { now: c.now + 101, ..c };
        assert_eq!(live(&me, &later).0, 1961);
        // uno stato di ieri non conta per oggi (ma per la settimana e il mese sì)
        let old = Status {
            day: "2026-11-11".into(),
            today: 999,
            week: 999,
            month: 999,
            total: 999,
            at: c.now,
            ..Default::default()
        };
        assert_eq!(live(&old, &c), (0, 999, 999, 999));
        let a = Status { name: "A".into(), today: 10, day: "2026-11-12".into(), ..Default::default() };
        let b = Status { name: "B".into(), today: 11, day: "2026-11-12".into(), ..Default::default() };
        let order = standings([("a", &a), ("b", &b)].into_iter(), &c, Period::Today);
        assert_eq!(order[0].0, "b");
        assert_eq!(fmt_secs(3725), "1 h 02 min 05 s");
        assert_eq!(fmt_secs(249), "4 min 09 s");
        assert_eq!(fmt_secs(12), "12 s");
    }

    #[test]
    fn periods_start_exactly_at_midnight() {
        // domenica 31/1 ore 23:00 → lunedì 1/2 ore 01:00: giorno, settimana e mese nuovi
        let tz = chrono::FixedOffset::east_opt(3600).unwrap();
        let start = clock(2027, 1, 31, 23, 0, 0);
        let c = clock(2027, 2, 1, 1, 0, 0);
        let s = Status {
            state: State::Studying,
            day: "2027-01-31".into(),
            today: 500,
            week: 5000,
            month: 50_000,
            total: 90_000,
            at: start.now,
            ..Default::default()
        };
        // solo l'ora dopo mezzanotte conta per oggi, la settimana e il mese nuovi
        assert_eq!(live(&s, &c), (3600, 3600, 3600, 90_000 + 7200));
        // a mezzanotte e un secondo, solo quel secondo
        let c1 = clock(2027, 2, 1, 0, 0, 1);
        assert_eq!(live(&s, &c1).0, 1);
        // il registro divide il tratto alla mezzanotte
        let mut log = StudyLog::default();
        log.add_span(start.now, c.now, "Quiz · Chimica", &tz);
        assert_eq!(log.what(c.today), vec![("Quiz · Chimica".to_string(), 3600)]);
        assert_eq!(log.day(start.today), 3600);
        assert_eq!(log.day(c.today), 3600);
        assert_eq!(log.month(c.today), 3600);
        assert_eq!(log.week(c.today), 3600);
        // chi manda lo stato fa lo stesso conto
        let me = my_status("Io", 1, State::Studying, &StudyLog::default(), Some(start.now), &c, false, ONE);
        assert_eq!((me.today, me.week, me.month, me.total), (3600, 3600, 3600, 7200));
        // un dispositivo sparito ieri sera non porta niente a oggi
        let (f, on) = frozen(&s, start.now + 1800, &c);
        assert!(!on);
        assert_eq!(live(&f, &c), (0, 0, 0, 90_000 + 1800));
        // mappa di calore riallineata su oggi
        let h = Hist { day: "2027-01-31".into(), secs: vec![7, 8], what: vec![vec![("Flashcard".into(), 7)]] };
        assert_eq!(h.on(c.today)[..3], [0, 7, 8]);
        assert_eq!(h.what_on(c.today, 1), vec![("Flashcard".to_string(), 7)]);
        assert!(h.what_on(c.today, 0).is_empty());
        // punti: il moltiplicatore vale per il tratto in corso, con gli stessi confini
        let mut p = s.clone();
        p.pts = [100.0, 1000.0, 5000.0, 9000.0];
        p.mult = 2.0;
        assert_eq!(live_pts(&p, &c), [7200.0, 7200.0, 7200.0, 9000.0 + 14400.0]);
        let mut log = StudyLog::default();
        log.add_points(start.now, c.now, 2.0, &tz);
        assert_eq!(log.points(c.today), [7200.0, 7200.0, 7200.0, 14400.0]);
        // identificativi dello stesso utente: tutti scelgono lo stesso
        assert_eq!(pick_user_id("u123456789abcdef", "0011223344556677"), "u123456789abcdef");
        assert_eq!(pick_user_id("0011223344556677", "u123456789abcdef"), "u123456789abcdef");
        assert_eq!(pick_user_id("u2", "u1"), pick_user_id("u1", "u2"));
        assert_eq!(pick_user_id("", "abc"), "abc");
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
        let ck = |n: i64| Clock { now: n, today, day0: 0, week0: 0, month0: 0 };
        let (c, on) = combine(&g.members[&uid].device_list(&ck(now + 10)), &ck(now + 10));
        assert!(on);
        assert_eq!(c.state, State::Studying);
        assert_eq!(c.today, 600 + 300 + 10);
        // un dispositivo che tace da troppo non è più in pista, ma il suo tempo resta
        let (c, on) = combine(&g.members[&uid].device_list(&ck(now + 100)), &ck(now + 100));
        assert!(!on && c.state == State::Idle && c.today >= 900);
        // solo l'amministratore autenticato toglie membri, e solo con firma valida e tempo coerente
        let (sk, pk) = new_admin_keypair();
        let (other_sk, other_pk) = new_admin_keypair();
        let sig = sign_info("X", "boss", "Nuovo", "", std::slice::from_ref(&uid), now, None, &sk).unwrap();
        let other_sig = sign_info("X", "altro", "N", "", std::slice::from_ref(&uid), now, None, &other_sk).unwrap();
        assert_eq!(
            g.apply_info("me", "altro", "N", "", std::slice::from_ref(&uid), now, &other_pk, &other_sig, None, now),
            InfoResult::Ignored
        );
        assert_eq!(
            g.apply_info("me", "boss", "Nuovo", "", std::slice::from_ref(&uid), now, &pk, "", None, now),
            InfoResult::Ignored
        );
        assert_eq!(
            g.apply_info("me", "boss", "Nuovo", "", std::slice::from_ref(&uid), now + 1000, &pk, &sig, None, now),
            InfoResult::Ignored
        );
        assert_eq!(
            g.apply_info("me", "boss", "Nuovo", "", std::slice::from_ref(&uid), now, &pk, &sig, None, now),
            InfoResult::Applied
        );
        assert!(g.members.is_empty() && g.name == "Nuovo");
        // chiave diversa non può sostituirsi nemmeno conoscendo l'id dell'amministratore
        let spoof_sig = sign_info("X", "boss", "Falso", "", &[], now + 1, None, &other_sk).unwrap();
        assert_eq!(
            g.apply_info("me", "boss", "Falso", "", &[], now + 1, &other_pk, &spoof_sig, None, now + 1),
            InfoResult::Ignored
        );
        g.status_in("d2", Status { uid: uid.clone(), ..Default::default() }, now);
        assert!(g.members.is_empty());
        assert_eq!(
            g.apply_info("me", "boss", "Nuovo", "", std::slice::from_ref(&uid), now, &pk, &sig, None, now),
            InfoResult::Ignored
        );
        let kick_me = sign_info("X", "boss", "Nuovo", "", &["me".into()], now + 1, None, &sk).unwrap();
        assert_eq!(
            g.apply_info("me", "boss", "Nuovo", "", &["me".into()], now + 1, &pk, &kick_me, None, now + 1),
            InfoResult::Removed
        );
    }

    #[test]
    fn oldest_user_id_wins_between_own_devices() {
        // telefono aggiornato con l'id di sempre (0) e computer appena installato (adesso)
        assert_eq!(pick_user_id_since("ufffffffffffffff", 0, "u000000000000000", 1_700_000_000), "ufffffffffffffff");
        assert_eq!(pick_user_id_since("u000000000000000", 1_700_000_000, "ufffffffffffffff", 0), "ufffffffffffffff");
        // id del dispositivo contro id ricavato: vince sempre quello ricavato
        assert_eq!(pick_user_id_since("d1", 0, "u000000000000000", 9), "u000000000000000");
        // stessa età: come prima, il minore
        assert_eq!(pick_user_id_since("ub00000000000000", 5, "ua00000000000000", 5), "ua00000000000000");
    }

    #[test]
    fn unsigned_info_only_names_the_admin_of_legacy_groups() {
        let now = 3_000_000;
        // gruppo nato nella versione privata: l'amministratore si sa dalle impostazioni senza firma
        let mut g = Group { code: "L".into(), ..Default::default() };
        assert_eq!(
            g.apply_info("me", "uold", "Serali", "", &["uvittima".into()], now, "", "", None, now),
            InfoResult::Applied
        );
        assert_eq!(g.admin, "uold");
        assert_eq!(g.name, "Serali");
        assert!(g.removed.is_empty(), "senza firma non si toglie nessuno");
        // un altro non può prendersi l'amministrazione
        assert_eq!(g.apply_info("me", "uevil", "X", "", &[], now + 1, "", "", None, now + 1), InfoResult::Ignored);
        // gruppo già firmato: le impostazioni senza firma non valgono più
        let mut adm = Group { code: "L".into(), admin: "uold".into(), info_at: now + 2, ..Default::default() };
        adm.sign_current_info();
        assert_eq!(
            g.apply_info("me", "uold", &adm.name, "", &[], now + 2, &adm.admin_pk, &adm.admin_sig, None, now + 2),
            InfoResult::Applied
        );
        assert_eq!(g.apply_info("me", "uold", "Falso", "", &[], now + 3, "", "", None, now + 3), InfoResult::Ignored);
    }

    #[test]
    fn legacy_id_matches_previous_versions() {
        // stessa formula delle versioni precedenti: maiuscole e spazi del nome non contano
        let a = legacy_user_id("Gianni", "segreta123");
        assert_eq!(a, legacy_user_id(" gianni ", "segreta123"));
        assert!(a.starts_with('u') && a.len() == 16);
        assert_ne!(a, user_id("Gianni", "segreta123", "d1"), "i nuovi id restano quelli rafforzati");
    }

    #[test]
    fn session_device_gives_the_color() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let now = 1_000_000;
        let ck = Clock { now, today, day0: 0, week0: 0, month0: 0 };
        let pc = Status { color: 0x0000ff, state: State::Studying, at: now - 5, ..Default::default() };
        let phone = Status { color: 0xff0000, state: State::Idle, focus: true, at: now, ..Default::default() };
        let (c, _) = combine(&[(pc.clone(), true), (phone.clone(), true)], &ck);
        assert_eq!(c.color, 0x0000ff, "colore del PC che studia, anche se il telefono è più recente");
        let (c, _) = combine(&[(Status { state: State::Idle, ..pc }, true), (phone, true)], &ck);
        assert_eq!(c.color, 0xff0000, "nessuna sessione: il più recente");
    }

    #[test]
    fn signed_info_survives_relay_and_legacy_admin_moves() {
        let now = 2_000_000;
        // A (amministratore) firma, B applica e ripete, C applica la copia di B
        let mut a =
            Group { code: "Q".into(), admin: "ua".into(), name: "  Studio serale  ".into(), ..Default::default() };
        a.removed.push("ux".into());
        a.info_at = now;
        a.sign_current_info();
        assert_eq!(a.name, "Studio serale");
        let mut b = Group { code: "Q".into(), admin: "ua".into(), ..Default::default() };
        assert_eq!(
            b.apply_info(
                "ub",
                &a.admin,
                &a.name,
                &a.photo,
                &a.removed,
                a.info_at,
                &a.admin_pk,
                &a.admin_sig,
                None,
                now
            ),
            InfoResult::Applied
        );
        // chi era stato rimosso torna con un id nuovo: resta fuori, e `removed` non cambia
        b.status_in("dx", Status { uid: "ux2".into(), prev: vec!["ux".into()], ..Default::default() }, now);
        assert!(!b.members.contains_key("ux2") && b.removed == vec!["ux".to_string()]);
        let mut c = Group { code: "Q".into(), ..Default::default() };
        assert_eq!(
            c.apply_info(
                "uc",
                &b.admin,
                &b.name,
                &b.photo,
                &b.removed,
                b.info_at,
                &b.admin_pk,
                &b.admin_sig,
                None,
                now
            ),
            InfoResult::Applied
        );
        // gruppo di prima delle firme: l'amministratore che cambia id resta amministratore
        let mut old = Group { code: "Q".into(), admin: "uold".into(), ..Default::default() };
        old.status_in("dA", Status { uid: "unew".into(), prev: vec!["uold".into()], ..Default::default() }, now);
        assert_eq!(old.admin, "unew");
        let mut adm = Group { code: "Q".into(), admin: "unew".into(), info_at: now, ..Default::default() };
        adm.sign_current_info();
        assert_eq!(
            old.apply_info("uz", "unew", "N", "", &[], now, &adm.admin_pk, &adm.admin_sig, None, now),
            InfoResult::Ignored,
            "firma su un nome diverso"
        );
        assert_eq!(
            old.apply_info("uz", &adm.admin, &adm.name, "", &[], now, &adm.admin_pk, &adm.admin_sig, None, now),
            InfoResult::Applied
        );
        // da firmato, un «prev» non basta più a prendersi l'amministrazione
        old.status_in("dE", Status { uid: "uevil".into(), prev: vec!["unew".into()], ..Default::default() }, now);
        assert_eq!(old.admin, "unew");
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

    #[test]
    fn user_id_changes_and_vanished_members() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let now = 1_000_000;
        let mut g = Group { code: "X".into(), admin: "d1".into(), removed: vec!["d9".into()], ..Default::default() };
        g.status_in("d1", Status { today: 100, day: "2026-11-12".into(), at: now, ..Default::default() }, now);
        // gruppo firmato: status.prev remoto non può riassegnare g.admin
        let mut signed = g.clone();
        signed.admin_pk = "00".repeat(32);
        let st = Status { uid: "u1".into(), prev: vec!["d1".into()], at: now + 1, ..Default::default() };
        signed.status_in("d1", st.clone(), now + 1);
        assert_eq!(signed.admin, "d1");
        // gruppo di prima delle firme: l'amministratore passa dall'id del dispositivo a quello
        // dell'utente come nelle versioni precedenti (altrimenti perderebbe il gruppo)
        g.status_in("d1", st, now + 1);
        assert_eq!(g.admin, "u1");
        // anche un utente non amministratore mantiene la migrazione dispositivi in members
        g.status_in("d2", Status { today: 50, day: "2026-11-12".into(), at: now, ..Default::default() }, now);
        g.status_in(
            "d2",
            Status { uid: "u2".into(), prev: vec!["d2".into()], at: now + 1, ..Default::default() },
            now + 1,
        );
        assert!(g.members.contains_key("u2") && !g.members.contains_key("d2"));
        // anche un rimosso resta rimosso sotto il nuovo id
        g.status_in("d9", Status { uid: "u9".into(), prev: vec!["d9".into()], ..Default::default() }, now);
        assert!(!g.members.contains_key("u9"));
        // anche quando la sua scheda arriva ripetuta da un altro
        let card = Card {
            dev: "d9".into(),
            seen: now,
            status: Status { uid: "u9".into(), prev: vec!["d9".into()], ..Default::default() },
        };
        assert!(!g.card_in(card, now + 100));
        assert!(!g.members.contains_key("u9"));
        // uno stato vecchio arrivato in ritardo non annulla quello nuovo
        g.status_in(
            "d1",
            Status { uid: "u1".into(), state: State::Studying, at: now - 5, ..Default::default() },
            now + 2,
        );
        assert_eq!(g.members["u1"].devices["d1"].status.at, now + 1);
        // chi sparisce mentre studia smette di contare
        let mut m = Member::default();
        m.update(
            "d1",
            Status { state: State::Studying, today: 60, day: "2026-11-12".into(), at: now, ..Default::default() },
            now + 30,
        );
        let ck = Clock { now: now + 4000, today, day0: 0, week0: 0, month0: 0 };
        let (c, on) = combine(&m.device_list(&ck), &ck);
        assert!(!on);
        assert_eq!(c.today, 90);
        assert_eq!(c.state, State::Idle);
    }

    #[test]
    fn admin_survives_own_id_change_and_late_joiners_cannot_take_over() {
        let mut gs = Groups::default();
        gs.join("AAAA-BBBB-CCCC-DDDD", "Prova", 100);
        gs.active_mut().unwrap().admin = "d1".into();
        // l'amministratore imposta la password: il suo id cambia anche nei gruppi salvati
        gs.rename_user("d1", "u1");
        assert!(gs.active().unwrap().can_admin("u1"));
        // chi entra ora non sa ancora chi è l'amministratore, ma non può diventarlo
        let mut other = Groups::default();
        other.join("AAAA-BBBB-CCCC-DDDD", "", 200);
        let g = other.active().unwrap();
        assert!(!g.legacy() && !g.can_admin("u2"));
        // rientrare con un vecchio invito non cambia il nome
        gs.active_mut().unwrap().name = "Anatomia".into();
        gs.join("AAAABBBBCCCCDDDD", "Prova", 300);
        assert_eq!(gs.active().unwrap().name, "Anatomia");
        // un gruppo salvato da una versione precedente (senza joined_at) è «legacy»
        let old: Group = serde_json::from_str(r#"{"code":"X","name":"Vecchio"}"#).unwrap();
        assert!(old.legacy() && old.can_admin("chiunque"));
    }

    #[test]
    fn groups_follow_between_own_devices() {
        let mut phone = Groups::default();
        phone.join("AAAA-BBBB-CCCC-DDDD", "Larping University", 100);
        phone.groups[0].admin = "u1".into();
        phone.groups[0].photo = "p1".into();
        phone.groups[0].info_at = 100;
        let mut pc = Groups::default();
        // il computer riceve il gruppo che aveva solo il telefono
        assert!(pc.merge_own(&phone.refs(), &phone.left));
        assert_eq!(pc.groups.len(), 1);
        assert_eq!(pc.active, "AAAA-BBBB-CCCC-DDDD");
        assert_eq!(pc.groups[0].photo, "p1");
        // di nuovo: niente da cambiare
        assert!(!pc.merge_own(&phone.refs(), &phone.left));
        // foto e nome più recenti vincono
        pc.groups[0].photo = "p2".into();
        pc.groups[0].name = "LU".into();
        pc.groups[0].info_at = 200;
        assert!(phone.merge_own(&pc.refs(), &pc.left));
        assert_eq!((phone.groups[0].photo.as_str(), phone.groups[0].name.as_str()), ("p2", "LU"));
        // uscito dal telefono: esce anche dal computer, e non ritorna
        phone.leave_at("AAAA-BBBB-CCCC-DDDD", 300);
        assert!(pc.merge_own(&phone.refs(), &phone.left));
        assert!(pc.groups.is_empty());
        assert!(!phone.merge_own(&pc.refs(), &pc.left));
        assert!(phone.groups.is_empty());
        // rientrato dopo l'uscita: vale il nuovo ingresso
        phone.join("AAAA-BBBB-CCCC-DDDD", "LU", 400);
        assert!(pc.merge_own(&phone.refs(), &phone.left));
        assert_eq!(pc.groups.len(), 1);
    }

    #[test]
    fn study_time_adds_up_across_own_devices() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let mut phone = StudyLog::default();
        phone.add(day, 600);
        phone.pts.insert("2026-11-18".into(), 600.0);
        let mut pc = StudyLog::default();
        pc.add(day, 1200);
        pc.pts.insert("2026-11-18".into(), 1500.0);
        assert!(phone.set_other("pc", pc.own()));
        assert!(!phone.set_other("pc", pc.own()));
        let all = phone.combined();
        assert_eq!(all.day(day), 1800);
        assert_eq!(all.points(day)[0], 2100.0);
        // il registro proprio resta solo di questo dispositivo
        assert_eq!(phone.day(day), 600);
        // aggiornamento dall'altro: sostituisce, non somma due volte
        pc.add(day, 60);
        phone.set_other("pc", pc.own());
        assert_eq!(phone.combined().day(day), 1860);
    }
}
