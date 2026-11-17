//! Archivio locale nella cartella dati privata dell'app.
//!
//! Le impostazioni sono comuni a tutta l'app (`<base>/settings.json`); il resto è per esame,
//! in `<base>/exams/<id>/` (vedi [`crate::exams`]):
//!
//! ```text
//! <base>/settings.json          nome utente, sincronizzazione tra dispositivi, promemoria
//! <root>/mirror/<percorso repo> copia locale di content/** e progress/**
//! <root>/mirror_index.json      sha git di ogni file del mirror
//! ```

use crate::progress::Assessment;
use crate::session::Session;
use crate::vfs as fs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// notifica giornaliera di ripasso
    #[serde(default = "d_true")]
    pub reminder_enabled: bool,
    /// orario della notifica, "HH:MM" (ora locale)
    #[serde(default = "d_rem_time")]
    pub reminder_time: String,
    /// promemoria di fine giornata sugli obiettivi non completati, "HH:MM"
    #[serde(default = "d_eve_time")]
    pub evening_time: String,
    /// orari dei promemoria del giorno (mattina, dopo pranzo, pomeriggio, sera)
    #[serde(default = "d_times")]
    pub reminder_times: Vec<String>,
    /// nome con cui ci si presenta agli altri dispositivi (sincronizzazione, studio di gruppo)
    #[serde(default)]
    pub username: String,
    /// identificativo casuale di questo dispositivo (sincronizzazione tra dispositivi)
    #[serde(default)]
    pub device_id: String,
    /// collegarsi da soli all'avvio; la password resta salvata solo in questo caso
    #[serde(default)]
    pub p2p_auto: bool,
    #[serde(default)]
    pub p2p_password: String,
    /// relay Nostr da usare; vuoto = quelli predefiniti
    #[serde(default)]
    pub relays: Vec<String>,
    /// sincronizzazione e condivisione solo con collegamento diretto (niente dati attraverso i relay)
    #[serde(default)]
    pub direct_only: bool,
    /// colore della propria monoposto (ARGB); 0 = accento di sistema (o un colore fisso)
    #[serde(default)]
    pub car_color: u32,
    /// colore del tema (ARGB); 0 = quello di sistema se c'è, altrimenti il blu predefinito
    #[serde(default)]
    pub theme_color: u32,
    /// foto profilo (hash del file in `photos/`), vuoto = nessuna
    #[serde(default)]
    pub photo: String,
    /// quando la foto profilo è stata cambiata (0 = mai): tra i propri dispositivi vince la più recente
    #[serde(default)]
    pub photo_at: i64,
    /// identificativo dell'utente nei gruppi di studio (fissato la prima volta, vedi
    /// `group::user_id`); cambia solo passando da quello del dispositivo a quello ricavato
    /// dalla password di sincronizzazione, e gli id precedenti restano in `prev_ids`
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub prev_ids: Vec<String>,
    /// modalità concentrazione: app permesse durante il blocco e durata scelta (minuti)
    #[serde(default)]
    pub focus_allowed: Vec<String>,
    #[serde(default)]
    pub focus_minutes: u32,
    /// versione delle note legali accettata (0 = mai)
    #[serde(default)]
    pub legal_accepted: u32,
}

fn d_times() -> Vec<String> {
    crate::reminder::DEFAULT_TIMES.iter().map(|t| t.to_string()).collect()
}

fn d_true() -> bool {
    true
}
fn d_rem_time() -> String {
    "18:30".into()
}
fn d_eve_time() -> String {
    "21:30".into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            reminder_enabled: true,
            reminder_time: d_rem_time(),
            evening_time: d_eve_time(),
            reminder_times: d_times(),
            username: String::new(),
            device_id: String::new(),
            p2p_auto: false,
            p2p_password: String::new(),
            relays: vec![],
            direct_only: false,
            car_color: 0,
            theme_color: 0,
            photo: String::new(),
            photo_at: 0,
            user_id: String::new(),
            prev_ids: vec![],
            focus_allowed: vec![],
            focus_minutes: 0,
            legal_accepted: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MirrorIndex {
    /// true dopo il primo pacchetto installato: da lì i contenuti vengono dall'archivio
    #[serde(default)]
    pub synced_once: bool,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Store {
    /// cartella dell'esame (contenuti e progressi)
    pub root: PathBuf,
    /// cartella dati dell'app (impostazioni comuni)
    pub base: PathBuf,
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(p: &Path) -> T {
    fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn write_atomic(p: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, p)
}

/// Impronta di un file (sha1 nel formato dei blob di git), per riconoscere i file già presenti.
pub fn blob_sha(bytes: &[u8]) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    h.digest().to_string()
}

fn safe_rel(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.split('/').any(|s| s == ".." || s.is_empty())
}

impl Store {
    /// Archivio in un'unica cartella (impostazioni comprese).
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Store> {
        let root = root.into();
        Self::open_in(root.clone(), root)
    }

    /// Archivio di un esame (`root`) con le impostazioni comuni in `base`.
    pub fn open_in(base: impl Into<PathBuf>, root: impl Into<PathBuf>) -> io::Result<Store> {
        let (base, root) = (base.into(), root.into());
        fs::create_dir_all(&root.join("mirror"))?;
        Ok(Store { root, base })
    }

    pub fn settings(&self) -> Settings {
        fs::read_to_string(&self.base.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> io::Result<()> {
        write_atomic(&self.base.join("settings.json"), serde_json::to_string_pretty(s)?.as_bytes())
    }

    pub fn index(&self) -> MirrorIndex {
        read_json(&self.root.join("mirror_index.json"))
    }

    pub fn save_index(&self, idx: &MirrorIndex) -> io::Result<()> {
        write_atomic(&self.root.join("mirror_index.json"), serde_json::to_string_pretty(idx)?.as_bytes())
    }

    pub fn mirror_path(&self, repo_path: &str) -> Option<PathBuf> {
        safe_rel(repo_path).then(|| self.root.join("mirror").join(repo_path))
    }

    pub fn write_mirror(&self, repo_path: &str, bytes: &[u8]) -> io::Result<()> {
        let p = self
            .mirror_path(repo_path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "percorso non valido"))?;
        write_atomic(&p, bytes)
    }

    pub fn remove_mirror(&self, repo_path: &str) {
        if let Some(p) = self.mirror_path(repo_path) {
            let _ = fs::remove_file(&p);
        }
    }

    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
        for p in fs::walk_files(dir) {
            if p.extension().is_some_and(|x| x == "json") {
                if let Ok(rel) = p.strip_prefix(base) {
                    out.push((rel.to_string_lossy().replace('\\', "/"), p.clone()));
                }
            }
        }
    }

    /// File di contenuto: dal mirror se già sincronizzato, altrimenti quelli inclusi nell'app.
    pub fn content_files(&self, bundled: &[(&str, &str)]) -> Vec<(String, String)> {
        if self.index().synced_once {
            let mut out = Vec::new();
            let base = self.root.join("mirror");
            let mut files = Vec::new();
            Self::walk(&base.join("content"), &base, &mut files);
            for (rel, p) in files {
                if let Ok(t) = fs::read_to_string(&p) {
                    out.push((rel, t));
                }
            }
            if out.iter().any(|(p, _)| p == "content/syllabus.json") {
                return out;
            }
        }
        bundled.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect()
    }

    pub fn sessions(&self) -> (Vec<Session>, Vec<String>) {
        let base = self.root.join("mirror");
        let mut files = Vec::new();
        Self::walk(&base.join("progress").join("sessions"), &base, &mut files);
        let mut out = Vec::new();
        let mut errs = Vec::new();
        for (rel, p) in files {
            match fs::read_to_string(&p).map(|t| serde_json::from_str::<Session>(&t)) {
                Ok(Ok(s)) => out.push(s),
                Ok(Err(e)) => errs.push(format!("{rel}: {e}")),
                Err(e) => errs.push(format!("{rel}: {e}")),
            }
        }
        (out, errs)
    }

    pub fn assessment(&self) -> Option<Assessment> {
        let p = self.root.join("mirror/progress/assessment.json");
        let t = fs::read_to_string(&p).ok()?;
        serde_json::from_str(&t).ok()
    }

    /// Salva un file nuovo nell'archivio e ne registra l'impronta nell'indice.
    pub fn record_file(&self, path: String, bytes: Vec<u8>) -> io::Result<()> {
        self.write_mirror(&path, &bytes)?;
        let mut idx = self.index();
        idx.files.insert(path, blob_sha(&bytes));
        self.save_index(&idx)
    }

    pub fn record_session(&self, s: &Session) -> io::Result<()> {
        self.record_file(s.repo_path(), s.to_json().into_bytes())
    }

    pub fn read_mirror(&self, repo_path: &str) -> Option<Vec<u8>> {
        fs::read(&self.mirror_path(repo_path)?).ok()
    }
}
