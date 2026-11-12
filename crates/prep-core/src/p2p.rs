//! Sincronizzazione tra dispositivi senza server proprio.
//!
//! I dispositivi con lo stesso **nome utente e password** formano una «stanza»: dalla coppia
//! si ricava (PBKDF2) una chiave segreta, da cui vengono il tag pubblico della stanza e la
//! chiave di cifratura (XChaCha20-Poly1305). I messaggi viaggiano come eventi Nostr
//! *effimeri* (kind 20000–29999: i relay li inoltrano agli iscritti senza salvarli), quindi
//! funzionano sia dall'app nativa sia dal browser, sulla porta 443, e i relay vedono solo
//! dati cifrati.
//!
//! Il protocollo è a messaggi e senza I/O ([`Engine`]): l'app gli passa i messaggi ricevuti
//! e spedisce quelli che produce. Cosa si scambia:
//! - **progressi**: ogni esame installato su entrambi i dispositivi confronta l'elenco dei
//!   file `progress/**` (sessioni, feedback: file che non cambiano più) e ognuno chiede
//!   all'altro quelli che gli mancano;
//! - **pacchetti**: un dispositivo può scaricare dall'altro un esame installato (il
//!   pacchetto contenuti ricostruito dall'archivio), a pezzi, con controllo SHA-256.

use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Tipo degli eventi Nostr usati (intervallo effimero: non vengono salvati dai relay).
pub const KIND: u32 = 23847;
/// Dimensione dei pezzi di file e pacchetti (il contenuto dell'evento resta sotto ~45 kB).
pub const CHUNK: usize = 32 * 1024;
/// Pezzi di pacchetto chiesti alla volta.
const WINDOW: u32 = 8;
const HELLO_EVERY: i64 = 20;
const PEER_TTL: i64 = 75;
const MAX_FILE: usize = 4 << 20;
const MAX_PACK: u64 = 300 << 20;
/// Relay pubblici predefiniti (modificabili nelle impostazioni).
pub const DEFAULT_RELAYS: &[&str] = &["wss://relay.damus.io", "wss://nos.lol", "wss://relay.primal.net"];

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("generatore casuale");
    b
}

/// Identificativo casuale (16 cifre esadecimali), per esempio del dispositivo.
pub fn random_id() -> String {
    hex(&random::<8>())
}

pub fn sha256_hex(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

// ---------- stanza: chiave, tag pubblico, cifratura ----------

#[derive(Clone)]
pub struct Room {
    /// tag pubblico con cui i dispositivi si trovano sui relay
    pub tag: String,
    key: [u8; 32],
}

impl Room {
    /// Stessa coppia nome utente (senza distinzione di maiuscole) e password → stessa stanza.
    pub fn derive(username: &str, password: &str) -> Room {
        let user = username.trim().to_lowercase();
        let salt = format!("tremendous-preparator/v1/{user}");
        let mut seed = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 60_000, &mut seed);
        let tag = hex(&Sha256::digest([b"room:".as_slice(), &seed].concat()))[..32].to_string();
        let key: [u8; 32] = Sha256::digest([b"key:".as_slice(), &seed].concat()).into();
        Room { tag, key }
    }

    /// Cifra: base64(nonce di 24 byte ‖ testo cifrato).
    pub fn seal(&self, plain: &[u8]) -> String {
        let nonce: [u8; 24] = random();
        let ct =
            XChaCha20Poly1305::new((&self.key).into()).encrypt(XNonce::from_slice(&nonce), plain).expect("cifratura");
        base64::engine::general_purpose::STANDARD.encode([nonce.as_slice(), &ct].concat())
    }

    /// Decifra; `None` se il messaggio non è di questa stanza o è stato alterato.
    pub fn open(&self, sealed: &str) -> Option<Vec<u8>> {
        let raw = base64::engine::general_purpose::STANDARD.decode(sealed.trim()).ok()?;
        if raw.len() < 24 + 16 {
            return None;
        }
        let (nonce, ct) = raw.split_at(24);
        XChaCha20Poly1305::new((&self.key).into()).decrypt(XNonce::from_slice(nonce), ct).ok()
    }
}

// ---------- eventi Nostr ----------

/// Chiave di firma usa e getta: serve solo perché i relay accettino gli eventi.
pub struct Signer {
    sk: k256::schnorr::SigningKey,
    pubkey: String,
}

impl Default for Signer {
    fn default() -> Self {
        Self::new()
    }
}

impl Signer {
    pub fn new() -> Signer {
        loop {
            let b: [u8; 32] = random();
            if let Ok(sk) = k256::schnorr::SigningKey::from_bytes(&b) {
                let pubkey = hex(&sk.verifying_key().to_bytes());
                return Signer { sk, pubkey };
            }
        }
    }

    /// Frame `["EVENT", {...}]` da mandare ai relay.
    pub fn event(&self, tag: &str, content: &str, created_at: i64) -> String {
        let tags = serde_json::json!([["t", tag]]);
        let ser =
            serde_json::to_string(&serde_json::json!([0, self.pubkey, created_at, KIND, tags, content])).expect("json");
        let id: [u8; 32] = Sha256::digest(ser.as_bytes()).into();
        let sig = self.sk.sign_raw(&id, &random::<32>()).expect("firma");
        serde_json::to_string(&serde_json::json!(["EVENT", {
            "id": hex(&id), "pubkey": self.pubkey, "created_at": created_at, "kind": KIND,
            "tags": tags, "content": content, "sig": hex(&sig.to_bytes()),
        }]))
        .expect("json")
    }
}

/// Frame di iscrizione agli eventi della stanza a partire da `since`.
pub fn subscribe(sub_id: &str, tag: &str, since: i64) -> String {
    serde_json::to_string(&serde_json::json!(["REQ", sub_id, {"kinds": [KIND], "#t": [tag], "since": since}]))
        .expect("json")
}

/// Evento ricevuto da un relay: (id, contenuto). Gli altri frame (EOSE, OK, NOTICE) → `None`.
pub fn parse_relay(frame: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(frame).ok()?;
    let a = v.as_array()?;
    if a.first()?.as_str()? != "EVENT" {
        return None;
    }
    let ev = a.get(2)?;
    if ev.get("kind")?.as_u64()? != KIND as u64 {
        return None;
    }
    Some((ev.get("id")?.as_str()?.to_string(), ev.get("content")?.as_str()?.to_string()))
}

/// Messaggi del relay da mostrare (es. rifiuto per limiti): `["OK", id, false, motivo]`, `NOTICE`.
pub fn relay_notice(frame: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(frame).ok()?;
    let a = v.as_array()?;
    match a.first()?.as_str()? {
        "NOTICE" => Some(a.get(1)?.as_str()?.to_string()),
        "OK" if a.get(2)?.as_bool() == Some(false) => Some(a.get(3)?.as_str().unwrap_or("rifiutato").to_string()),
        "CLOSED" => Some(a.get(2)?.as_str().unwrap_or("iscrizione chiusa").to_string()),
        _ => None,
    }
}

// ---------- messaggi ----------

/// Esame che un dispositivo ha installato.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExamOffer {
    pub id: String,
    pub name: String,
    /// versione dei contenuti (per capire se l'altro è più aggiornato)
    #[serde(default)]
    pub version: String,
    /// il dispositivo può mandare il pacchetto di questo esame
    #[serde(default)]
    pub shareable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Msg {
    /// presentazione; `reply` = risposta a un'altra presentazione (non va ricambiata)
    Hello {
        name: String,
        platform: String,
        exams: Vec<ExamOffer>,
        reply: bool,
    },
    /// file di progressi posseduti per un esame: (percorso, sha256)
    Inv {
        exam: String,
        files: Vec<(String, String)>,
    },
    Get {
        exam: String,
        paths: Vec<String>,
    },
    /// pezzo `i` di `n` di un file di progressi (il contenuto è nel carico)
    File {
        exam: String,
        path: String,
        i: u32,
        n: u32,
    },
    PackGet {
        exam: String,
        from: u32,
        count: u32,
    },
    PackPart {
        exam: String,
        sha: String,
        size: u64,
        i: u32,
        n: u32,
    },
    PackErr {
        exam: String,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    v: u8,
    from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    msg: Msg,
}

/// Messaggio da spedire: `to` = None per tutti i dispositivi della stanza.
#[derive(Debug, Clone)]
pub struct Out {
    pub to: Option<String>,
    pub msg: Msg,
    pub payload: Vec<u8>,
}

impl Out {
    fn new(to: Option<&str>, msg: Msg) -> Out {
        Out { to: to.map(str::to_string), msg, payload: vec![] }
    }
}

/// Testo in chiaro di un messaggio: lunghezza dell'intestazione (4 byte) ‖ intestazione JSON ‖ carico.
fn encode(env: &Envelope, payload: &[u8]) -> Vec<u8> {
    let head = serde_json::to_vec(env).expect("json");
    let mut out = (head.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&head);
    out.extend_from_slice(payload);
    out
}

fn decode(b: &[u8]) -> Option<(Envelope, Vec<u8>)> {
    let n = u32::from_be_bytes(b.get(..4)?.try_into().ok()?) as usize;
    let head = b.get(4..4 + n)?;
    Some((serde_json::from_slice(head).ok()?, b[4 + n..].to_vec()))
}

/// I file che si possono scambiare: solo progressi JSON, percorsi relativi puliti.
pub fn shareable_path(p: &str) -> bool {
    p.starts_with("progress/")
        && p.ends_with(".json")
        && !p.split('/').any(|s| s.is_empty() || s == ".." || s == ".")
        && !p.contains('\\')
}

// ---------- motore ----------

/// Accesso dell'[`Engine`] ai dati dell'app.
pub trait Host {
    /// esami installati da annunciare
    fn offers(&self) -> Vec<ExamOffer>;
    /// file di progressi (percorso, sha256) di un esame; `None` se non è installato
    fn progress_files(&self, exam: &str) -> Option<Vec<(String, String)>>;
    fn read_file(&self, exam: &str, path: &str) -> Option<Vec<u8>>;
    /// salva un file di progressi ricevuto; `false` se rifiutato
    fn write_file(&mut self, exam: &str, path: &str, bytes: &[u8]) -> bool;
    /// pacchetto contenuti dell'esame (gzip)
    fn pack(&mut self, exam: &str) -> Result<Vec<u8>, String>;
    /// importa un pacchetto ricevuto; restituisce il nome dell'esame
    fn import_pack(&mut self, bytes: &[u8]) -> Result<String, String>;
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub name: String,
    pub platform: String,
    pub exams: Vec<ExamOffer>,
    pub last_seen: i64,
}

/// Cose successe, per l'interfaccia.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// dispositivo trovato o aggiornato (id)
    Peer(String),
    /// file di progressi ricevuti per un esame
    Files {
        exam: String,
        n: usize,
    },
    PackProgress {
        exam: String,
        got: u32,
        total: u32,
    },
    PackDone {
        exam: String,
        result: Result<String, String>,
    },
}

/// File in arrivo a pezzi: (mittente, esame, percorso) → pezzi ricevuti.
type Partial = BTreeMap<(String, String, String), Vec<Option<Vec<u8>>>>;

struct Download {
    exam: String,
    from: String,
    sha: String,
    size: u64,
    parts: Vec<Option<Vec<u8>>>,
    /// primo pezzo non ancora chiesto
    next: u32,
    last: i64,
    started: i64,
}

pub struct Engine {
    pub device: String,
    pub name: String,
    pub platform: String,
    pub peers: BTreeMap<String, Peer>,
    pub events: Vec<Event>,
    last_hello: i64,
    /// pezzi di file in arrivo: (mittente, esame, percorso) → pezzi
    partial: Partial,
    /// pacchetti già preparati per l'invio: esame → (sha, byte)
    packs: BTreeMap<String, (String, Vec<u8>)>,
    dl: Option<Download>,
    /// id degli eventi già visti (gli stessi arrivano da più relay)
    seen: VecDeque<String>,
    seen_set: BTreeSet<String>,
}

impl Engine {
    pub fn new(device: &str, name: &str, platform: &str) -> Engine {
        Engine {
            device: device.to_string(),
            name: name.to_string(),
            platform: platform.to_string(),
            peers: BTreeMap::new(),
            events: vec![],
            last_hello: i64::MIN / 2,
            partial: BTreeMap::new(),
            packs: BTreeMap::new(),
            dl: None,
            seen: VecDeque::new(),
            seen_set: BTreeSet::new(),
        }
    }

    /// Testo in chiaro da cifrare e spedire.
    pub fn wire(&self, out: &Out) -> Vec<u8> {
        encode(&Envelope { v: 1, from: self.device.clone(), to: out.to.clone(), msg: out.msg.clone() }, &out.payload)
    }

    fn hello(&self, host: &dyn Host, to: Option<&str>, reply: bool) -> Out {
        Out::new(
            to,
            Msg::Hello { name: self.name.clone(), platform: self.platform.clone(), exams: host.offers(), reply },
        )
    }

    fn inventories(&self, host: &dyn Host, to: Option<&str>, theirs: Option<&[ExamOffer]>) -> Vec<Out> {
        let mut v = Vec::new();
        for e in host.offers() {
            if theirs.is_some_and(|t| !t.iter().any(|x| x.id == e.id)) {
                continue;
            }
            if let Some(files) = host.progress_files(&e.id) {
                v.push(Out::new(to, Msg::Inv { exam: e.id.clone(), files }));
            }
        }
        v
    }

    /// Ingresso nella stanza: ci si presenta a tutti.
    pub fn start(&mut self, host: &dyn Host, now: i64) -> Vec<Out> {
        self.last_hello = now;
        vec![self.hello(host, None, false)]
    }

    /// Nuovi progressi locali (es. fine di un quiz): li si annuncia ai dispositivi presenti.
    pub fn announce(&mut self, host: &dyn Host) -> Vec<Out> {
        if self.peers.is_empty() {
            return vec![];
        }
        self.inventories(host, None, None)
    }

    /// Da chiamare ogni secondo circa.
    pub fn tick(&mut self, host: &mut dyn Host, now: i64) -> Vec<Out> {
        let mut out = Vec::new();
        if now - self.last_hello >= HELLO_EVERY {
            self.last_hello = now;
            out.push(self.hello(host, None, true));
        }
        self.peers.retain(|_, p| now - p.last_seen < PEER_TTL);
        if let Some(d) = &mut self.dl {
            if now - d.last > 90 {
                let exam = d.exam.clone();
                self.dl = None;
                self.events.push(Event::PackDone { exam, result: Err("l'altro dispositivo non risponde".into()) });
            } else if now - d.last > 8 {
                // pezzi persi: si richiede dal primo mancante
                d.last = now;
                let first = d.parts.iter().position(|p| p.is_none()).unwrap_or(0) as u32;
                let count = if d.parts.is_empty() { 1 } else { WINDOW };
                d.next = first + count;
                out.push(Out::new(Some(&d.from), Msg::PackGet { exam: d.exam.clone(), from: first, count }));
            }
        }
        out
    }

    /// Scarica dal dispositivo `peer` il pacchetto dell'esame `exam`.
    pub fn download(&mut self, peer: &str, exam: &str, now: i64) -> Vec<Out> {
        self.dl = Some(Download {
            exam: exam.to_string(),
            from: peer.to_string(),
            sha: String::new(),
            size: 0,
            parts: vec![],
            next: 1,
            last: now,
            started: now,
        });
        vec![Out::new(Some(peer), Msg::PackGet { exam: exam.to_string(), from: 0, count: 1 })]
    }

    pub fn downloading(&self) -> Option<(&str, u32, u32)> {
        self.dl
            .as_ref()
            .map(|d| (d.exam.as_str(), d.parts.iter().filter(|p| p.is_some()).count() as u32, d.parts.len() as u32))
    }

    pub fn cancel_download(&mut self) {
        self.dl = None;
    }

    /// Evento della stanza già decifrato. `event_id` serve a scartare i doppioni.
    pub fn receive(&mut self, host: &mut dyn Host, event_id: &str, plain: &[u8], now: i64) -> Vec<Out> {
        if !self.seen_set.insert(event_id.to_string()) {
            return vec![];
        }
        self.seen.push_back(event_id.to_string());
        if self.seen.len() > 4000 {
            if let Some(old) = self.seen.pop_front() {
                self.seen_set.remove(&old);
            }
        }
        let Some((env, payload)) = decode(plain) else { return vec![] };
        if env.from == self.device || env.to.as_deref().is_some_and(|t| t != self.device) {
            return vec![];
        }
        let from = env.from.clone();
        let mut out = Vec::new();
        if let Some(p) = self.peers.get_mut(&from) {
            p.last_seen = now;
        }
        match env.msg {
            Msg::Hello { name, platform, exams, reply } => {
                let known = self.peers.contains_key(&from);
                self.peers.insert(from.clone(), Peer { name, platform, exams: exams.clone(), last_seen: now });
                self.events.push(Event::Peer(from.clone()));
                if !reply || !known {
                    if !reply {
                        out.push(self.hello(host, Some(&from), true));
                    }
                    out.extend(self.inventories(host, Some(&from), Some(&exams)));
                }
            }
            Msg::Inv { exam, files } => {
                if let Some(mine) = host.progress_files(&exam) {
                    let have: BTreeSet<&str> = mine.iter().map(|(p, _)| p.as_str()).collect();
                    let missing: Vec<String> = files
                        .into_iter()
                        .map(|(p, _)| p)
                        .filter(|p| shareable_path(p) && !have.contains(p.as_str()))
                        .collect();
                    for chunk in missing.chunks(150) {
                        out.push(Out::new(Some(&from), Msg::Get { exam: exam.clone(), paths: chunk.to_vec() }));
                    }
                }
            }
            Msg::Get { exam, paths } => {
                for p in paths.iter().take(600).filter(|p| shareable_path(p)) {
                    let Some(bytes) = host.read_file(&exam, p) else { continue };
                    if bytes.len() > MAX_FILE {
                        continue;
                    }
                    let n = bytes.len().div_ceil(CHUNK).max(1) as u32;
                    for (i, part) in bytes.chunks(CHUNK).enumerate() {
                        out.push(Out {
                            to: Some(from.clone()),
                            msg: Msg::File { exam: exam.clone(), path: p.clone(), i: i as u32, n },
                            payload: part.to_vec(),
                        });
                    }
                    if bytes.is_empty() {
                        out.push(Out::new(Some(&from), Msg::File { exam: exam.clone(), path: p.clone(), i: 0, n: 1 }));
                    }
                }
            }
            Msg::File { exam, path, i, n } => {
                if !shareable_path(&path) || n == 0 || i >= n || n as usize * CHUNK > MAX_FILE + CHUNK {
                    return out;
                }
                let key = (from.clone(), exam.clone(), path.clone());
                let parts = self.partial.entry(key.clone()).or_insert_with(|| vec![None; n as usize]);
                if parts.len() != n as usize {
                    *parts = vec![None; n as usize];
                }
                parts[i as usize] = Some(payload);
                if parts.iter().all(|p| p.is_some()) {
                    let bytes: Vec<u8> =
                        self.partial.remove(&key).unwrap_or_default().into_iter().flatten().flatten().collect();
                    if host.write_file(&exam, &path, &bytes) {
                        match self.events.iter_mut().rev().find_map(|e| match e {
                            Event::Files { exam: x, n } if *x == exam => Some(n),
                            _ => None,
                        }) {
                            Some(c) => *c += 1,
                            None => self.events.push(Event::Files { exam, n: 1 }),
                        }
                    }
                }
            }
            Msg::PackGet { exam, from: first, count } => {
                if !self.packs.contains_key(&exam) {
                    match host.pack(&exam) {
                        Ok(b) => {
                            let sha = sha256_hex(&b);
                            self.packs.insert(exam.clone(), (sha, b));
                        }
                        Err(e) => {
                            out.push(Out::new(Some(&from), Msg::PackErr { exam, error: e }));
                            return out;
                        }
                    }
                }
                let (sha, bytes) = &self.packs[&exam];
                let n = bytes.len().div_ceil(CHUNK).max(1) as u32;
                for i in first..(first + count.min(WINDOW * 2)).min(n) {
                    let a = i as usize * CHUNK;
                    let b = (a + CHUNK).min(bytes.len());
                    out.push(Out {
                        to: Some(from.clone()),
                        msg: Msg::PackPart { exam: exam.clone(), sha: sha.clone(), size: bytes.len() as u64, i, n },
                        payload: bytes[a..b].to_vec(),
                    });
                }
            }
            Msg::PackPart { exam, sha, size, i, n } => {
                let Some(d) = &mut self.dl else { return out };
                if d.exam != exam || d.from != from || size > MAX_PACK || n == 0 || i >= n {
                    return out;
                }
                if d.parts.is_empty() {
                    d.sha = sha.clone();
                    d.size = size;
                    d.parts = vec![None; n as usize];
                    d.next = 1;
                } else if d.sha != sha || d.parts.len() != n as usize {
                    return out;
                }
                d.parts[i as usize] = Some(payload);
                d.last = now;
                let got = d.parts.iter().filter(|p| p.is_some()).count() as u32;
                self.events.push(Event::PackProgress { exam: exam.clone(), got, total: n });
                if got == n {
                    let bytes: Vec<u8> = d.parts.drain(..).flatten().flatten().collect();
                    let ok = bytes.len() as u64 == d.size && sha256_hex(&bytes) == d.sha;
                    self.dl = None;
                    let result =
                        if ok { host.import_pack(&bytes) } else { Err("pacchetto arrivato danneggiato".into()) };
                    self.events.push(Event::PackDone { exam, result });
                } else if d.parts[..(d.next.min(n)) as usize].iter().all(|p| p.is_some()) && d.next < n {
                    // finestra completa: si chiede la successiva
                    let first = d.next;
                    d.next = (first + WINDOW).min(n);
                    out.push(Out::new(Some(&from), Msg::PackGet { exam, from: first, count: WINDOW }));
                }
            }
            Msg::PackErr { exam, error } => {
                if self.dl.as_ref().is_some_and(|d| d.exam == exam && d.from == from) {
                    self.dl = None;
                    self.events.push(Event::PackDone { exam, result: Err(error) });
                }
            }
        }
        out
    }

    /// Secondi dall'inizio dello scaricamento in corso.
    pub fn download_secs(&self, now: i64) -> Option<i64> {
        self.dl.as_ref().map(|d| now - d.started)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Mem {
        exams: Vec<ExamOffer>,
        files: BTreeMap<(String, String), Vec<u8>>,
        pack: Vec<u8>,
        imported: Vec<Vec<u8>>,
    }

    impl Host for Mem {
        fn offers(&self) -> Vec<ExamOffer> {
            self.exams.clone()
        }
        fn progress_files(&self, exam: &str) -> Option<Vec<(String, String)>> {
            self.exams.iter().any(|e| e.id == exam).then(|| {
                self.files
                    .iter()
                    .filter(|((e, _), _)| e == exam)
                    .map(|((_, p), b)| (p.clone(), sha256_hex(b)))
                    .collect()
            })
        }
        fn read_file(&self, exam: &str, path: &str) -> Option<Vec<u8>> {
            self.files.get(&(exam.to_string(), path.to_string())).cloned()
        }
        fn write_file(&mut self, exam: &str, path: &str, bytes: &[u8]) -> bool {
            self.files.insert((exam.to_string(), path.to_string()), bytes.to_vec());
            true
        }
        fn pack(&mut self, _exam: &str) -> Result<Vec<u8>, String> {
            Ok(self.pack.clone())
        }
        fn import_pack(&mut self, bytes: &[u8]) -> Result<String, String> {
            self.imported.push(bytes.to_vec());
            Ok("Esame".into())
        }
    }

    fn offer(id: &str) -> ExamOffer {
        ExamOffer { id: id.into(), name: id.into(), version: "1".into(), shareable: true }
    }

    /// Consegna i messaggi tra due motori (come farebbe la stanza) finché tacciono.
    fn run(a: &mut (Engine, Mem), b: &mut (Engine, Mem), room: &Room, first: Vec<Out>, lose_every: usize) {
        let mut queue: VecDeque<(bool, Out)> = first.into_iter().map(|o| (true, o)).collect();
        let mut k = 0usize;
        let mut now = 1000;
        let mut rounds = 0;
        while rounds < 200 {
            rounds += 1;
            while let Some((from_a, o)) = queue.pop_front() {
                k += 1;
                let (src, dst) = if from_a { (&*a, &mut *b) } else { (&*b, &mut *a) };
                let sealed = room.seal(&src.0.wire(&o));
                if lose_every > 0 && k.is_multiple_of(lose_every) {
                    continue; // messaggio perso
                }
                let plain = room.open(&sealed).unwrap();
                let outs = dst.0.receive(&mut dst.1, &format!("ev{k}"), &plain, now);
                queue.extend(outs.into_iter().map(|o| (!from_a, o)));
            }
            now += 10;
            let ta = a.0.tick(&mut a.1, now);
            let tb = b.0.tick(&mut b.1, now);
            if ta.is_empty() && tb.is_empty() && a.0.dl.is_none() && b.0.dl.is_none() {
                break;
            }
            queue.extend(ta.into_iter().map(|o| (true, o)));
            queue.extend(tb.into_iter().map(|o| (false, o)));
        }
    }

    #[test]
    fn room_crypto() {
        let r1 = Room::derive("Giulia", "segreta-123");
        let r2 = Room::derive(" giulia ", "segreta-123");
        let r3 = Room::derive("giulia", "altra");
        assert_eq!(r1.tag, r2.tag);
        assert_ne!(r1.tag, r3.tag);
        let s = r1.seal(b"ciao");
        assert_eq!(r2.open(&s).unwrap(), b"ciao");
        assert!(r3.open(&s).is_none());
        assert!(r1.open("non base64!").is_none());
    }

    #[test]
    fn nostr_frames() {
        let s = Signer::new();
        let f = s.event("abc", "contenuto", 1_700_000_000);
        let v: serde_json::Value = serde_json::from_str(&f).unwrap();
        let ev = &v[1];
        // id = sha256 della serializzazione canonica (NIP-01)
        let ser = serde_json::to_string(&serde_json::json!([
            0,
            ev["pubkey"],
            ev["created_at"],
            ev["kind"],
            ev["tags"],
            ev["content"]
        ]))
        .unwrap();
        assert_eq!(ev["id"].as_str().unwrap(), sha256_hex(ser.as_bytes()));
        // firma BIP-340 valida
        use k256::schnorr::signature::hazmat::PrehashVerifier;
        let vk = k256::schnorr::VerifyingKey::from_bytes(&hex_dec(ev["pubkey"].as_str().unwrap())).unwrap();
        let sig = k256::schnorr::Signature::try_from(hex_dec(ev["sig"].as_str().unwrap()).as_slice()).unwrap();
        vk.verify_prehash(&hex_dec(ev["id"].as_str().unwrap()), &sig).unwrap();
        let relay = format!("[\"EVENT\",\"sub\",{}]", serde_json::to_string(ev).unwrap());
        assert_eq!(parse_relay(&relay).unwrap().1, "contenuto");
        assert!(parse_relay("[\"EOSE\",\"sub\"]").is_none());
        assert_eq!(relay_notice("[\"OK\",\"x\",false,\"rate-limited\"]").unwrap(), "rate-limited");
        assert!(subscribe("s", "abc", 5).contains("\"#t\":[\"abc\"]"));
    }

    fn hex_dec(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn progress_and_pack_sync() {
        let room = Room::derive("u", "p");
        let mut a = (Engine::new("A", "Telefono", "android"), Mem::default());
        let mut b = (Engine::new("B", "Browser", "web"), Mem::default());
        a.1.exams = vec![offer("anat"), offer("solo-a")];
        b.1.exams = vec![offer("anat")];
        a.1.files.insert(("anat".into(), "progress/sessions/s1.json".into()), b"{\"a\":1}".to_vec());
        // file più grande di un pezzo
        a.1.files.insert(("anat".into(), "progress/sessions/big.json".into()), vec![b'x'; CHUNK * 2 + 10]);
        b.1.files.insert(("anat".into(), "progress/sessions/s2.json".into()), b"{\"b\":2}".to_vec());
        a.1.files.insert(("solo-a".into(), "progress/sessions/z.json".into()), b"{}".to_vec());
        a.1.pack = (0..(CHUNK * 20 + 123)).map(|i| (i % 251) as u8).collect();
        let first = a.0.start(&a.1, 1000);
        run(&mut a, &mut b, &room, first, 0);
        assert_eq!(a.1.files.get(&("anat".into(), "progress/sessions/s2.json".into())).unwrap(), b"{\"b\":2}");
        assert_eq!(b.1.files.get(&("anat".into(), "progress/sessions/s1.json".into())).unwrap(), b"{\"a\":1}");
        assert_eq!(b.1.files.get(&("anat".into(), "progress/sessions/big.json".into())).unwrap().len(), CHUNK * 2 + 10);
        // un esame che B non ha non viene mescolato
        assert!(!b.1.files.keys().any(|(e, _)| e == "solo-a"));
        assert_eq!(b.0.peers["A"].name, "Telefono");
        assert!(b.0.peers["A"].exams.iter().any(|e| e.id == "solo-a"));
        // B scarica il pacchetto di «solo-a», con messaggi persi lungo la strada
        let req = b.0.download("A", "solo-a", 2000);
        let mut queue_b = req;
        // le richieste partono da B: si invertono i ruoli nel giro
        let (mut bb, mut aa) = (b, a);
        let first: Vec<Out> = std::mem::take(&mut queue_b);
        run(&mut bb, &mut aa, &room, first, 7);
        assert_eq!(bb.1.imported.len(), 1);
        assert_eq!(bb.1.imported[0], aa.1.pack);
        assert!(bb.0.events.iter().any(|e| matches!(e, Event::PackDone { result: Ok(_), .. })));
    }

    #[test]
    fn rejects_unsafe_paths() {
        assert!(shareable_path("progress/sessions/a.json"));
        assert!(!shareable_path("progress/../settings.json"));
        assert!(!shareable_path("content/syllabus.json"));
        assert!(!shareable_path("progress//a.json"));
        assert!(!shareable_path("progress/a.txt"));
    }
}
