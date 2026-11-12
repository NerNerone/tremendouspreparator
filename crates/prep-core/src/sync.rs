//! Sincronizzazione con il repository GitHub:
//! 1. invia i commit in coda (sessioni nuove) via Contents API;
//! 2. scarica i file di `content/` e `progress/` cambiati (confronto sha git).

use crate::github::GhError;
#[cfg(not(target_arch = "wasm32"))]
use crate::github::{git_blob_sha, GitHub, RepoConfig};
use crate::store::{Store, SyncState};
#[cfg(not(target_arch = "wasm32"))]
use crate::vfs as fs;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashSet;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct SyncReport {
    pub pushed: usize,
    pub downloaded: usize,
    pub removed: usize,
    pub pending: usize,
    pub branch: String,
    pub error: Option<String>,
    /// errore nell'invio dei commit (il download può comunque essere riuscito)
    pub push_error: Option<String>,
    pub downloaded_ok: bool,
}

impl SyncReport {
    pub fn summary(&self) -> String {
        match &self.error {
            Some(e) if self.downloaded_ok => format!("Scaricati {} file, ma {e}", self.downloaded),
            Some(e) => format!("Errore: {e}"),
            None => format!(
                "Sincronizzato ({}): {} inviati, {} scaricati{}",
                self.branch,
                self.pushed,
                self.downloaded,
                if self.pending > 0 { format!(", {} in coda", self.pending) } else { String::new() }
            ),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn tracked(path: &str) -> bool {
    (path.ends_with(".json") && (path.starts_with("content/") || path.starts_with("progress/")))
        || ((path.ends_with(".md") || path.ends_with(".svg")) && path.starts_with("books/"))
}

pub fn sync(store: &Store) -> SyncReport {
    sync_with_progress(store, &|_, _| {})
}

/// Come [`sync`], con un callback `(scaricati, da_scaricare)` chiamato durante il download.
pub fn sync_with_progress(store: &Store, progress: &(dyn Fn(usize, usize) + Sync)) -> SyncReport {
    let mut rep = SyncReport::default();
    let res = run(store, &mut rep, progress);
    rep.pending = store.outbox().len();
    let mut st = store.sync_state();
    let err = match res {
        Ok(()) => rep.push_error.clone(),
        Err(e) => Some(e.to_string()),
    };
    match err {
        None => {
            st.last_ok = Some(chrono::Utc::now());
            st.last_error.clear();
            st.branch = rep.branch.clone();
        }
        Some(e) => {
            if rep.downloaded_ok {
                // il download è riuscito: solo l'invio ha avuto problemi
                st.last_ok = Some(chrono::Utc::now());
                st.branch = rep.branch.clone();
            }
            st.last_error = e.clone();
            rep.error = Some(e);
        }
    }
    let _ = store.save_sync_state(&st);
    rep
}

/// Download paralleli: i file sono tanti e piccoli (formule SVG), uno alla volta è lento.
#[cfg(not(target_arch = "wasm32"))]
const WORKERS: usize = 6;
/// Ogni quanti file salvare l'indice: un'interruzione non fa ripartire da zero.
#[cfg(not(target_arch = "wasm32"))]
const SAVE_EVERY: usize = 40;

/// Nel browser la sincronizzazione (rete bloccante e thread) non è ancora disponibile:
/// i contenuti si importano con il pacchetto.
#[cfg(target_arch = "wasm32")]
fn run(_: &Store, _: &mut SyncReport, _: &(dyn Fn(usize, usize) + Sync)) -> Result<(), GhError> {
    Err(GhError::Network(
        "sincronizzazione non ancora disponibile nella versione web: importa il pacchetto contenuti".into(),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn run(store: &Store, rep: &mut SyncReport, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<(), GhError> {
    let s = store.settings();
    if s.token.trim().is_empty() {
        return Err(GhError::Auth(0, "token non impostato (Impostazioni)".into()));
    }
    let gh = GitHub::new(RepoConfig {
        api_base: s.api_base.clone(),
        owner: s.owner.trim().to_string(),
        repo: s.repo.trim().to_string(),
        token: s.token.trim().to_string(),
    });
    let branch = if s.branch.trim().is_empty() { gh.default_branch()? } else { s.branch.trim().to_string() };
    rep.branch = branch.clone();

    // 1. invio: un errore qui (es. token in sola lettura) non blocca il download
    let mut pending_paths: HashSet<String> = HashSet::new();
    for (file, entry) in store.outbox() {
        let Some(bytes) = store.read_mirror(&entry.path) else {
            let _ = fs::remove_file(&file);
            continue;
        };
        match gh.create_file(&entry.path, &bytes, &entry.message, &branch) {
            Ok(_) => {
                let _ = fs::remove_file(&file);
                rep.pushed += 1;
            }
            Err(GhError::Network(m)) => return Err(GhError::Network(m)),
            Err(e) => {
                pending_paths.insert(entry.path.clone());
                rep.push_error = Some(match e {
                    GhError::Auth(403, m) | GhError::Http(404, m) => format!(
                        "invio non riuscito: il token non può scrivere nel repository \
                         (serve il permesso \"Contents: Read and write\"). {m}"
                    ),
                    e => format!("invio non riuscito: {e}"),
                });
                break;
            }
        }
    }

    // 2. download
    let tree = gh.tree(&branch)?;
    let mut idx = store.index();
    let mut remote: HashSet<String> = HashSet::new();
    let mut todo = Vec::new();
    for e in tree.iter().filter(|e| e.kind == "blob" && tracked(&e.path)) {
        remote.insert(e.path.clone());
        if idx.files.get(&e.path) == Some(&e.sha) && store.mirror_path(&e.path).is_some_and(|p| p.exists()) {
            continue;
        }
        // file già presente (sync precedente interrotta prima di salvare l'indice)
        if let Some(bytes) = store.read_mirror(&e.path) {
            if git_blob_sha(&bytes) == e.sha {
                idx.files.insert(e.path.clone(), e.sha.clone());
                continue;
            }
        }
        todo.push(e);
    }
    let total = todo.len();
    progress(0, total);
    let next = AtomicUsize::new(0);
    let done = Mutex::new((idx, 0usize, None::<GhError>));
    std::thread::scope(|sc| {
        for _ in 0..WORKERS.min(total) {
            sc.spawn(|| loop {
                if done.lock().map_or(true, |d| d.2.is_some()) {
                    return;
                }
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(e) = todo.get(i) else { return };
                // un secondo tentativo per gli errori di rete occasionali
                let res = gh.blob(&e.sha).or_else(|err| match err {
                    GhError::Network(_) => gh.blob(&e.sha),
                    other => Err(other),
                });
                let mut d = done.lock().unwrap();
                match res {
                    Ok(bytes) => {
                        if store.write_mirror(&e.path, &bytes).is_ok() {
                            d.0.files.insert(e.path.clone(), e.sha.clone());
                            d.1 += 1;
                            if d.1.is_multiple_of(SAVE_EVERY) {
                                let _ = store.save_index(&d.0);
                            }
                            progress(d.1, total);
                        }
                    }
                    Err(err) => {
                        if d.2.is_none() {
                            d.2 = Some(err);
                        }
                    }
                }
            });
        }
    });
    let (mut idx, downloaded, err) = done.into_inner().unwrap();
    rep.downloaded = downloaded;
    if let Some(err) = err {
        let _ = store.save_index(&idx);
        return Err(err);
    }
    // file rimossi dal repository (ma non quelli ancora da inviare)
    let still_pending: HashSet<String> = store.outbox().into_iter().map(|(_, e)| e.path).collect();
    let stale: Vec<String> = idx
        .files
        .keys()
        .filter(|p| !remote.contains(*p) && !still_pending.contains(*p) && !pending_paths.contains(*p))
        .cloned()
        .collect();
    for p in stale {
        store.remove_mirror(&p);
        idx.files.remove(&p);
        rep.removed += 1;
    }
    if remote.iter().any(|p| p == "content/syllabus.json") {
        idx.synced_once = true;
    }
    store.save_index(&idx).map_err(|e| GhError::Parse(e.to_string()))?;
    rep.downloaded_ok = true;
    Ok(())
}

/// Stato sintetico per la UI.
pub fn status_line(store: &Store) -> String {
    let st: SyncState = store.sync_state();
    let pending = store.outbox().len();
    let base = match st.last_ok {
        Some(t) => format!("Ultima sync {}", t.with_timezone(&chrono::Local).format("%d/%m %H:%M")),
        None => "Mai sincronizzato".into(),
    };
    let mut s = base;
    if pending > 0 {
        s.push_str(&format!(" · {pending} commit in coda"));
    }
    if !st.last_error.is_empty() {
        s.push_str(&format!(" · errore: {}", st.last_error));
    }
    s
}
