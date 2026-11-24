//! Archivio locale nella cartella dati privata dell'app.
//!
//! Le impostazioni sono comuni a tutta l'app (`<base>/settings.json`); il resto è per esame,
//! in `<base>/exams/<id>/` (vedi [`crate::exams`]):
//!
//! ```text
//! <base>/settings.json          nome utente, token e repository, promemoria
//! <root>/mirror/<percorso repo> copia locale di content/** e progress/**
//! <root>/mirror_index.json      sha git di ogni file del mirror
//! <root>/outbox/<id>.json       commit in attesa di invio (offline)
//! <root>/sync_state.json        esito dell'ultima sincronizzazione
//! ```

use crate::feedback::{Feedback, FeedbackStatus};
use crate::github::git_blob_sha;

/// Impronta di un file come blob git (nome usato dal codice in comune con la build pubblica).
pub fn blob_sha(bytes: &[u8]) -> String {
    git_blob_sha(bytes)
}
use crate::progress::Assessment;
use crate::session::Session;
use crate::vfs as fs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "d_api")]
    pub api_base: String,
    #[serde(default = "d_owner")]
    pub owner: String,
    #[serde(default = "d_repo")]
    pub repo: String,
    /// vuoto = branch predefinito del repository
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub token: String,
    /// sincronizzazione con un repository GitHub (spenta finché non la si accende in Opzioni)
    #[serde(default)]
    pub github_on: bool,
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
    /// orari tra `reminder_times` spenti uno per uno (la fascia resta, senza notifica)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reminder_off: Vec<String>,
    /// nome con cui ci si presenta agli altri dispositivi (sincronizzazione, studio di gruppo)
    #[serde(default)]
    pub username: String,
    /// identificativo casuale di questo dispositivo (sincronizzazione tra dispositivi)
    #[serde(default)]
    pub device_id: String,
    /// collegarsi da soli all'avvio; la chiave di stanza derivata resta salvata solo in questo caso
    #[serde(default)]
    pub p2p_auto: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub p2p_password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub p2p_room_key: String,
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
    /// chiaro o scuro: 0 = come il sistema, 1 = chiaro, 2 = scuro
    #[serde(default)]
    pub theme_mode: u8,
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
    /// da quando si usa `user_id` (0 = da prima che si registrasse, cioè il più vecchio):
    /// tra i propri dispositivi resta l'id più vecchio, quello che i gruppi già conoscono
    #[serde(default)]
    pub user_id_since: i64,
    /// id che si avrebbe con le versioni precedenti (ricavato dalla password quando la si
    /// scrive, solo su questo dispositivo): se un proprio gruppo lo conosce già, lo si riprende
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub legacy_uid: String,
    /// modalità concentrazione: app permesse durante il blocco e durata scelta (minuti)
    #[serde(default)]
    pub focus_allowed: Vec<String>,
    #[serde(default)]
    pub focus_minutes: u32,
    /// versione delle note legali accettata (0 = mai)
    #[serde(default)]
    pub legal_accepted: u32,
    /// novità già viste (impronte dei punti) e se il registro è stato avviato
    #[serde(default)]
    pub news_seen: Vec<String>,
    #[serde(default)]
    pub news_init: bool,
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

fn d_api() -> String {
    "https://api.github.com".into()
}
// nessun repository predefinito: senza proprietario, repository e token la sincronizzazione
// e i feedback per Claude restano nascosti
fn d_owner() -> String {
    String::new()
}
fn d_repo() -> String {
    String::new()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            api_base: d_api(),
            owner: d_owner(),
            repo: d_repo(),
            branch: String::new(),
            github_on: false,
            token: String::new(),
            reminder_enabled: true,
            reminder_time: d_rem_time(),
            evening_time: d_eve_time(),
            reminder_times: d_times(),
            reminder_off: vec![],
            username: String::new(),
            device_id: String::new(),
            p2p_auto: false,
            p2p_password: String::new(),
            p2p_room_key: String::new(),
            relays: vec![],
            direct_only: false,
            car_color: 0,
            theme_color: 0,
            theme_mode: 0,
            photo: String::new(),
            photo_at: 0,
            user_id: String::new(),
            prev_ids: vec![],
            user_id_since: 0,
            legacy_uid: String::new(),
            focus_allowed: vec![],
            focus_minutes: 0,
            legal_accepted: 0,
            news_seen: vec![],
            news_init: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MirrorIndex {
    /// true dopo la prima sincronizzazione riuscita: da lì i contenuti vengono dal repo
    #[serde(default)]
    pub synced_once: bool,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default)]
    pub last_ok: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub last_error: String,
    #[serde(default)]
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub path: String,
    pub message: String,
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

fn write_atomic_mode(p: &Path, bytes: &[u8], private: bool) -> io::Result<()> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = private;
    fs::rename(&tmp, p)
}

fn write_atomic(p: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_mode(p, bytes, false)
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
        fs::create_dir_all(&root.join("outbox"))?;
        Ok(Store { root, base })
    }

    /// Ramo del repository GitHub da cui si sincronizza questo esame (`repo.json` nella sua
    /// cartella); vuoto = quello delle impostazioni o il predefinito del repository.
    pub fn exam_branch(&self) -> String {
        fs::read_to_string(&self.root.join("repo.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["branch"].as_str().map(str::to_string))
            .unwrap_or_default()
    }

    pub fn set_exam_branch(&self, branch: &str) -> io::Result<()> {
        write_atomic(
            &self.root.join("repo.json"),
            serde_json::json!({ "branch": branch.trim() }).to_string().as_bytes(),
        )
    }

    pub fn settings(&self) -> Settings {
        fs::read_to_string(&self.base.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> io::Result<()> {
        write_atomic_mode(&self.base.join("settings.json"), serde_json::to_string_pretty(s)?.as_bytes(), true)
    }

    pub fn index(&self) -> MirrorIndex {
        read_json(&self.root.join("mirror_index.json"))
    }

    pub fn save_index(&self, idx: &MirrorIndex) -> io::Result<()> {
        write_atomic(&self.root.join("mirror_index.json"), serde_json::to_string_pretty(idx)?.as_bytes())
    }

    pub fn sync_state(&self) -> SyncState {
        read_json(&self.root.join("sync_state.json"))
    }

    pub fn save_sync_state(&self, s: &SyncState) -> io::Result<()> {
        write_atomic(&self.root.join("sync_state.json"), serde_json::to_string_pretty(s)?.as_bytes())
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

    /// Salva un file nuovo nel mirror e lo mette in coda per il commit.
    pub fn record_file(&self, id: &str, path: String, bytes: Vec<u8>, message: &str) -> io::Result<()> {
        self.write_mirror(&path, &bytes)?;
        let mut idx = self.index();
        idx.files.insert(path.clone(), git_blob_sha(&bytes));
        self.save_index(&idx)?;
        let entry = OutboxEntry { path, message: message.to_string() };
        write_atomic(
            &self.root.join("outbox").join(format!("{id}.json")),
            serde_json::to_string_pretty(&entry)?.as_bytes(),
        )
    }

    pub fn record_session(&self, s: &Session, message: &str) -> io::Result<()> {
        self.record_file(&s.id, s.repo_path(), s.to_json().into_bytes(), message)
    }

    pub fn record_feedback(&self, f: &Feedback) -> io::Result<()> {
        self.record_file(&f.id, f.repo_path(), f.to_json().into_bytes(), &f.commit_message())
    }

    pub fn feedbacks(&self) -> Vec<Feedback> {
        let base = self.root.join("mirror");
        let mut files = Vec::new();
        Self::walk(&base.join("progress").join("feedback"), &base, &mut files);
        let mut v: Vec<Feedback> = files
            .iter()
            .filter_map(|(_, p)| fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()))
            .collect();
        v.sort_by_key(|f| std::cmp::Reverse(f.created_at));
        v
    }

    pub fn feedback_status(&self) -> FeedbackStatus {
        read_json(&self.root.join("mirror/progress/feedback_status.json"))
    }

    pub fn is_pending(&self, repo_path: &str) -> bool {
        self.outbox().iter().any(|(_, e)| e.path == repo_path)
    }

    pub fn outbox(&self) -> Vec<(PathBuf, OutboxEntry)> {
        let mut v: Vec<(PathBuf, OutboxEntry)> = fs::list_dir(&self.root.join("outbox"))
            .into_iter()
            .filter_map(|p| {
                let t = fs::read_to_string(&p).ok()?;
                let entry: OutboxEntry = serde_json::from_str(&t).ok()?;
                Some((p, entry))
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    pub fn read_mirror(&self, repo_path: &str) -> Option<Vec<u8>> {
        fs::read(&self.mirror_path(repo_path)?).ok()
    }
}
