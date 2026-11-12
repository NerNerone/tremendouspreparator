//! Pacchetto contenuti: quiz, flashcard e libri in un unico file (`.json.gz`), così i
//! contenuti si aggiornano anche senza token GitHub e senza reinstallare l'app.
//!
//! ```text
//! { "format": "semestre-aperto-contenuti", "version": 1,
//!   "created": "2026-11-12T10:00:00Z", "label": "contenuti-42 (abc1234)",
//!   "files": { "content/syllabus.json": "…", "books/fisica/…/01-x.md": "…", … } }
//! ```
//!
//! L'importazione scrive i file nel mirror locale (come una sincronizzazione) e toglie
//! quelli di `content/` e `books/` che il pacchetto non contiene più. I progressi
//! (`progress/`) non vengono mai toccati.

use crate::content::Content;
use crate::github::git_blob_sha;
use crate::store::Store;
use crate::vfs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

pub const FORMAT: &str = "semestre-aperto-contenuti";
pub const FILE_NAME: &str = "SemestreAperto-contenuti.json.gz";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackInfo {
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub files: usize,
    #[serde(default)]
    pub questions: usize,
    #[serde(default)]
    pub flashcards: usize,
    /// quando è stato importato (solo nel file locale `pack_info.json`)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub imported: String,
}

#[derive(Serialize, Deserialize)]
struct Pack {
    format: String,
    version: u32,
    #[serde(default)]
    created: String,
    #[serde(default)]
    label: String,
    files: BTreeMap<String, String>,
}

/// Percorsi che fanno parte di un pacchetto: contenuti JSON e libri (testo e formule).
pub fn in_pack(path: &str) -> bool {
    let ok = !path.starts_with('/') && !path.split('/').any(|s| s == ".." || s.is_empty());
    ok && ((path.starts_with("content/") && path.ends_with(".json"))
        || (path.starts_with("books/") && (path.ends_with(".md") || path.ends_with(".svg"))))
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
    for p in vfs::walk_files(dir) {
        if let Ok(rel) = p.strip_prefix(base) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if in_pack(&rel) {
                out.push(rel);
            }
        }
    }
}

/// Crea il pacchetto dal checkout del repository (`root` = cartella con content/ e books/).
pub fn build(root: &Path, label: &str) -> Result<(Vec<u8>, PackInfo), String> {
    let mut paths = Vec::new();
    walk(&root.join("content"), root, &mut paths);
    walk(&root.join("books"), root, &mut paths);
    paths.sort();
    let mut files = BTreeMap::new();
    for p in paths {
        let t = vfs::read_to_string(&root.join(&p)).map_err(|e| format!("{p}: {e}"))?;
        files.insert(p, t);
    }
    let pack = Pack {
        format: FORMAT.into(),
        version: 1,
        created: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        label: label.into(),
        files,
    };
    let info = check(&pack)?;
    let json = serde_json::to_vec(&pack).map_err(|e| e.to_string())?;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(&json).map_err(|e| e.to_string())?;
    Ok((gz.finish().map_err(|e| e.to_string())?, info))
}

fn parse(bytes: &[u8]) -> Result<Pack, String> {
    let mut json = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bytes)
            .take(512 << 20)
            .read_to_end(&mut json)
            .map_err(|e| format!("file compresso non leggibile: {e}"))?;
    } else {
        json = bytes.to_vec();
    }
    let pack: Pack = serde_json::from_slice(&json)
        .map_err(|_| "non è un pacchetto contenuti di Tremendous Preparator".to_string())?;
    if pack.format != FORMAT {
        return Err("non è un pacchetto contenuti di Tremendous Preparator".into());
    }
    if pack.version > 1 {
        return Err("pacchetto creato per una versione più recente dell'app: aggiornala".into());
    }
    Ok(pack)
}

/// Controlla che il pacchetto sia utilizzabile e ne riassume il contenuto.
fn check(pack: &Pack) -> Result<PackInfo, String> {
    if let Some(bad) = pack.files.keys().find(|p| !in_pack(p)) {
        return Err(format!("percorso non ammesso nel pacchetto: {bad}"));
    }
    let content = Content::from_files(
        pack.files.iter().filter(|(p, _)| p.starts_with("content/")).map(|(p, t)| (p.as_str(), t.as_str())),
    );
    if content.syllabus.is_none() {
        return Err("pacchetto incompleto: manca content/syllabus.json".into());
    }
    if content.questions.is_empty() {
        return Err("pacchetto senza domande".into());
    }
    Ok(PackInfo {
        created: pack.created.clone(),
        label: pack.label.clone(),
        files: pack.files.len(),
        questions: content.questions.len(),
        flashcards: content.cards.len(),
        imported: String::new(),
    })
}

/// Legge solo le informazioni del pacchetto (senza importarlo).
pub fn inspect(bytes: &[u8]) -> Result<PackInfo, String> {
    check(&parse(bytes)?)
}

/// Esame a cui appartiene il pacchetto: (identificativo, nome) dal suo `content/syllabus.json`.
pub fn exam_of(bytes: &[u8]) -> Result<(String, String), String> {
    let pack = parse(bytes)?;
    let t = pack.files.get("content/syllabus.json").ok_or("pacchetto incompleto: manca content/syllabus.json")?;
    let syl: crate::model::Syllabus =
        serde_json::from_str(t).map_err(|e| format!("content/syllabus.json non valido: {e}"))?;
    Ok((crate::exams::exam_id(&syl), syl.title))
}

/// Importa il pacchetto nel mirror locale. I progressi non vengono toccati.
pub fn import(store: &Store, bytes: &[u8]) -> Result<PackInfo, String> {
    let pack = parse(bytes)?;
    let mut info = check(&pack)?;
    let mut idx = store.index();
    // via i contenuti che il pacchetto non ha più (come farebbe la sincronizzazione)
    let stale: Vec<String> = idx.files.keys().filter(|p| in_pack(p) && !pack.files.contains_key(*p)).cloned().collect();
    for p in stale {
        store.remove_mirror(&p);
        idx.files.remove(&p);
    }
    let mut on_disk = Vec::new();
    let mirror = store.root.join("mirror");
    walk(&mirror.join("content"), &mirror, &mut on_disk);
    walk(&mirror.join("books"), &mirror, &mut on_disk);
    for p in on_disk.iter().filter(|p| !pack.files.contains_key(*p)) {
        store.remove_mirror(p);
    }
    for (p, t) in &pack.files {
        let bytes = t.as_bytes();
        let sha = git_blob_sha(bytes);
        let same = idx.files.get(p) == Some(&sha) && store.mirror_path(p).is_some_and(|f| vfs::exists(&f));
        if !same {
            store.write_mirror(p, bytes).map_err(|e| format!("{p}: {e}"))?;
            idx.files.insert(p.clone(), sha);
        }
    }
    idx.synced_once = true;
    store.save_index(&idx).map_err(|e| e.to_string())?;
    info.imported = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let _ = vfs::write(&store.root.join("pack_info.json"), &serde_json::to_vec_pretty(&info).unwrap_or_default());
    Ok(info)
}

/// Impronta dei contenuti nel mirror (libri, quiz, flashcard): uguale su due dispositivi
/// che hanno gli stessi file, comunque ci siano arrivati (pacchetto, GitHub, altro dispositivo).
pub fn content_version(store: &Store) -> String {
    let idx = store.index();
    let mut h = sha1_smol::Sha1::new();
    for (p, sha) in idx.files.iter().filter(|(p, _)| in_pack(p)) {
        h.update(p.as_bytes());
        h.update(b"\0");
        h.update(sha.as_bytes());
        h.update(b"\n");
    }
    h.digest().to_string()
}

/// Ultimo pacchetto importato, se c'è.
pub fn last_import(store: &Store) -> Option<PackInfo> {
    let t = vfs::read_to_string(&store.root.join("pack_info.json")).ok()?;
    serde_json::from_str(&t).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (gz, info) = build(&repo, "test").unwrap();
        assert!(info.questions > 0 && info.files > 0);
        let dir = std::env::temp_dir().join(format!("prep-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).unwrap();
        store.write_mirror("content/vecchio.json", b"{}").unwrap();
        store.write_mirror("progress/sessions/x.json", b"{}").unwrap();
        let got = import(&store, &gz).unwrap();
        assert_eq!(got.questions, info.questions);
        assert!(store.mirror_path("content/syllabus.json").unwrap().exists());
        assert!(!store.mirror_path("content/vecchio.json").unwrap().exists());
        assert!(store.mirror_path("progress/sessions/x.json").unwrap().exists());
        assert!(store.index().synced_once);
        assert_eq!(last_import(&store).unwrap().label, "test");
        assert!(inspect(b"{\"format\":\"altro\",\"version\":1,\"files\":{}}").is_err());
        assert!(!exam_of(&gz).unwrap().0.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
