//! Accesso ai file dell'archivio locale.
//!
//! Su desktop e Android sono i file veri (`std::fs`). Nel browser (WebAssembly) non c'è un
//! filesystem: i file vivono in memoria e ogni modifica viene passata a una funzione di
//! persistenza registrata dall'app (che la salva in IndexedDB); all'avvio l'app ricarica i
//! file salvati con [`preload`]. Il resto del codice usa solo queste funzioni.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use std::fs;

    pub fn read(p: &Path) -> io::Result<Vec<u8>> {
        fs::read(p)
    }
    pub fn write(p: &Path, bytes: &[u8]) -> io::Result<()> {
        fs::write(p, bytes)
    }
    pub fn create_dir_all(p: &Path) -> io::Result<()> {
        fs::create_dir_all(p)
    }
    pub fn remove_file(p: &Path) -> io::Result<()> {
        fs::remove_file(p)
    }
    pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
    pub fn exists(p: &Path) -> bool {
        p.exists()
    }
    pub fn is_dir(p: &Path) -> bool {
        p.is_dir()
    }
    pub fn list_dir(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default()
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    type Persist = Box<dyn Fn(&str, Option<&[u8]>)>;

    thread_local! {
        static FILES: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
        static PERSIST: RefCell<Option<Persist>> = const { RefCell::new(None) };
    }

    fn key(p: &Path) -> String {
        p.to_string_lossy().trim_end_matches('/').to_string()
    }

    fn not_found(p: &Path) -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, p.display().to_string())
    }

    fn persist(k: &str, v: Option<&[u8]>) {
        PERSIST.with(|f| {
            if let Some(f) = f.borrow().as_ref() {
                f(k, v)
            }
        })
    }

    pub fn set_persist(f: impl Fn(&str, Option<&[u8]>) + 'static) {
        PERSIST.with(|p| *p.borrow_mut() = Some(Box::new(f)));
    }

    pub fn preload(files: impl IntoIterator<Item = (String, Vec<u8>)>) {
        FILES.with(|m| m.borrow_mut().extend(files));
    }

    pub fn read(p: &Path) -> io::Result<Vec<u8>> {
        FILES.with(|m| m.borrow().get(&key(p)).cloned()).ok_or_else(|| not_found(p))
    }
    pub fn write(p: &Path, bytes: &[u8]) -> io::Result<()> {
        let k = key(p);
        FILES.with(|m| m.borrow_mut().insert(k.clone(), bytes.to_vec()));
        persist(&k, Some(bytes));
        Ok(())
    }
    pub fn create_dir_all(_: &Path) -> io::Result<()> {
        Ok(())
    }
    pub fn remove_file(p: &Path) -> io::Result<()> {
        let k = key(p);
        let had = FILES.with(|m| m.borrow_mut().remove(&k)).is_some();
        if had {
            persist(&k, None);
            Ok(())
        } else {
            Err(not_found(p))
        }
    }
    pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
        let bytes = read(from)?;
        write(to, &bytes)?;
        remove_file(from)
    }
    pub fn exists(p: &Path) -> bool {
        let k = key(p);
        FILES.with(|m| {
            let m = m.borrow();
            m.contains_key(&k)
                || m.range(format!("{k}/")..).next().is_some_and(|(x, _)| x.starts_with(&format!("{k}/")))
        })
    }
    pub fn is_dir(p: &Path) -> bool {
        let k = format!("{}/", key(p));
        FILES.with(|m| m.borrow().range(k.clone()..).next().is_some_and(|(x, _)| x.starts_with(&k)))
    }
    pub fn list_dir(dir: &Path) -> Vec<PathBuf> {
        let prefix = format!("{}/", key(dir));
        let mut out: Vec<PathBuf> = Vec::new();
        FILES.with(|m| {
            for k in m.borrow().range(prefix.clone()..).map(|(k, _)| k).take_while(|k| k.starts_with(&prefix)) {
                let rest = &k[prefix.len()..];
                let first = rest.split('/').next().unwrap_or(rest);
                let child = PathBuf::from(format!("{prefix}{first}"));
                if out.last() != Some(&child) {
                    out.push(child);
                }
            }
        });
        out
    }
}

pub use imp::*;

pub fn read_to_string(p: &Path) -> io::Result<String> {
    String::from_utf8(read(p)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Tutti i file sotto `dir` (ricorsivo).
pub fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for p in list_dir(&d) {
            if is_dir(&p) {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}
