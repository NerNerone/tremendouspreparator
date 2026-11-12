//! Esami installati. Ogni esame ha la sua cartella con contenuti e progressi; un pacchetto
//! contenuti appartiene all'esame indicato dal suo `content/syllabus.json` (campo `id`,
//! altrimenti ricavato dal titolo).
//!
//! ```text
//! <base>/exams.json             esami installati e quello attivo
//! <base>/exams/<id>/…           archivio dell'esame (vedi [`crate::store`])
//! ```

use crate::model::Syllabus;
use crate::vfs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "exams.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExamEntry {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub exams: Vec<ExamEntry>,
}

impl Registry {
    pub fn load(base: &Path) -> Option<Registry> {
        serde_json::from_str(&vfs::read_to_string(&base.join(FILE)).ok()?).ok()
    }

    pub fn save(&self, base: &Path) -> std::io::Result<()> {
        vfs::write(&base.join(FILE), serde_json::to_string_pretty(self)?.as_bytes())
    }

    pub fn get(&self, id: &str) -> Option<&ExamEntry> {
        self.exams.iter().find(|e| e.id == id)
    }

    /// Aggiunge l'esame (o ne aggiorna il nome) e lo rende attivo.
    pub fn upsert(&mut self, id: &str, name: &str) {
        match self.exams.iter_mut().find(|e| e.id == id) {
            Some(e) => e.name = name.to_string(),
            None => self.exams.push(ExamEntry { id: id.to_string(), name: name.to_string() }),
        }
        self.active = id.to_string();
    }

    /// Aggiunge l'esame (o ne aggiorna il nome) senza cambiare quello attivo.
    pub fn add(&mut self, id: &str, name: &str) {
        let keep = self.active.clone();
        self.upsert(id, name);
        if !keep.is_empty() {
            self.active = keep;
        }
    }

    /// Toglie l'esame dall'elenco (i file si cancellano a parte con [`remove_files`]).
    pub fn remove(&mut self, id: &str) {
        self.exams.retain(|e| e.id != id);
        if self.active == id {
            self.active = self.exams.first().map(|e| e.id.clone()).unwrap_or_default();
        }
    }
}

/// Cartella dei dati di un esame.
pub fn dir(base: &Path, id: &str) -> PathBuf {
    base.join("exams").join(id)
}

/// Identificativo leggibile e sicuro come nome di cartella: minuscole, cifre e trattini.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        let c = match c {
            'à' | 'á' | 'â' => 'a',
            'è' | 'é' | 'ê' => 'e',
            'ì' | 'í' => 'i',
            'ò' | 'ó' => 'o',
            'ù' | 'ú' => 'u',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').chars().take(48).collect::<String>();
    if out.is_empty() {
        "esame".into()
    } else {
        out
    }
}

/// Identificativo dell'esame descritto dal syllabus.
pub fn exam_id(syl: &Syllabus) -> String {
    if syl.id.trim().is_empty() {
        slug(&syl.title)
    } else {
        slug(&syl.id)
    }
}

/// File dell'archivio a una sola cartella (versioni precedenti) che passano all'esame.
const LEGACY_DIRS: &[&str] = &["mirror", "outbox"];
const LEGACY_FILES: &[&str] =
    &["mirror_index.json", "sync_state.json", "pack_info.json", "quiz_in_corso.json", crate::plan::GOALS_FILE];

fn read_syllabus(p: &Path) -> Option<Syllabus> {
    serde_json::from_str(&vfs::read_to_string(p).ok()?).ok()
}

/// Apre il registro degli esami, creandolo al primo avvio. Se ci sono dati della versione a
/// esame unico li sposta nella cartella del loro esame, così i progressi non si perdono.
/// `bundled` è il syllabus incluso nell'app, usato quando non c'è ancora niente.
pub fn prepare(base: &Path, bundled: Option<&Syllabus>) -> Registry {
    if let Some(r) = Registry::load(base) {
        if r.get(&r.active).is_some() {
            return r;
        }
        if !r.exams.is_empty() {
            let mut r = r;
            r.active = r.exams[0].id.clone();
            let _ = r.save(base);
            return r;
        }
    }
    let legacy = read_syllabus(&base.join("mirror/content/syllabus.json"));
    let Some(syl) = legacy.as_ref().or(bundled) else { return Registry::default() };
    let id = exam_id(syl);
    let target = dir(base, &id);
    for d in LEGACY_DIRS {
        let from = base.join(d);
        for f in vfs::walk_files(&from) {
            if let Ok(rel) = f.strip_prefix(base) {
                let to = target.join(rel);
                if let Some(parent) = to.parent() {
                    let _ = vfs::create_dir_all(parent);
                }
                let _ = vfs::rename(&f, &to);
            }
        }
    }
    for f in LEGACY_FILES {
        let from = base.join(f);
        if vfs::exists(&from) {
            let _ = vfs::create_dir_all(&target);
            let _ = vfs::rename(&from, &target.join(f));
        }
    }
    let mut r = Registry::default();
    r.upsert(&id, &syl.title);
    let _ = r.save(base);
    r
}

/// Cancella tutti i file di un esame (contenuti e progressi).
pub fn remove_files(base: &Path, id: &str) {
    let d = dir(base, id);
    for f in vfs::walk_files(&d) {
        let _ = vfs::remove_file(&f);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = std::fs::remove_dir_all(&d);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syl(id: &str, title: &str) -> Syllabus {
        serde_json::from_str(&format!(
            r#"{{"schema":1,"id":"{id}","title":"{title}","exam":{{"questions":1,"mcq":1,"fill":0,"options":5,"minutes":1,
            "correct":1,"wrong":0,"omitted":0,"pass":1}},"subjects":[]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Semestre aperto Medicina 2026/27"), "semestre-aperto-medicina-2026-27");
        assert_eq!(slug("  Anatomia I — orale "), "anatomia-i-orale");
        assert_eq!(slug("///"), "esame");
        assert_eq!(exam_id(&syl("", "Fisica è bella")), "fisica-e-bella");
        assert_eq!(exam_id(&syl("Biochimica", "x")), "biochimica");
    }

    #[test]
    fn migrates_legacy_archive() {
        let base = std::env::temp_dir().join(format!("prep-exams-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("mirror/progress/sessions")).unwrap();
        std::fs::create_dir_all(base.join("mirror/content")).unwrap();
        std::fs::write(base.join("mirror/progress/sessions/s1.json"), "{}").unwrap();
        std::fs::write(
            base.join("mirror/content/syllabus.json"),
            serde_json::to_string(&syl("", "Esame Prova")).unwrap(),
        )
        .unwrap();
        std::fs::write(base.join("settings.json"), "{}").unwrap();
        std::fs::write(base.join("mirror_index.json"), "{}").unwrap();
        let r = prepare(&base, Some(&syl("altro", "Altro")));
        assert_eq!(r.active, "esame-prova");
        let d = dir(&base, "esame-prova");
        assert!(d.join("mirror/progress/sessions/s1.json").exists());
        assert!(d.join("mirror_index.json").exists());
        assert!(!base.join("mirror/progress/sessions/s1.json").exists());
        assert!(base.join("settings.json").exists());
        // seconda apertura: niente da spostare, stesso registro
        let r2 = prepare(&base, None);
        assert_eq!(r2.active, "esame-prova");
        let mut r3 = r2.clone();
        r3.upsert("altro", "Altro");
        assert_eq!(r3.active, "altro");
        r3.remove("altro");
        assert_eq!(r3.active, "esame-prova");
        let _ = std::fs::remove_dir_all(&base);
    }
}
