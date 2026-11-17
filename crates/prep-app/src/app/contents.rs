//! Pagina «Contenuti»: i file dell'esame uno per uno (aggiungere, togliere, esportare).

use super::*;
use crate::ContentFileVM;
use prep_core::contents::{self, Kind};

impl Ctrl {
    pub(super) fn open_contents(&mut self) {
        self.refresh_contents();
        self.ui().global::<AppState>().set_contents_status(ss(""));
        self.set_page(P_CONTENTS);
    }

    pub(super) fn refresh_contents(&mut self) {
        let mut last = None;
        let files: Vec<ContentFileVM> = contents::list(&self.store)
            .into_iter()
            .map(|e| {
                let head = last != Some(e.kind);
                last = Some(e.kind);
                ContentFileVM {
                    path: ss(&e.path),
                    kind: ss(e.kind.label()),
                    name: ss(&e.name),
                    info: ss(&e.info),
                    head,
                }
            })
            .collect();
        self.ui().global::<AppState>().set_content_files(model(files));
    }

    /// Dopo una modifica: contenuti, progressi e pagine si ricalcolano.
    fn contents_changed(&mut self, status: String) {
        self.reload_content();
        self.reload_progress();
        self.refresh_all();
        self.refresh_settings_info();
        self.refresh_contents();
        self.ui().global::<AppState>().set_contents_status(ss(&status));
        self.p2p_announce();
    }

    /// «Aggiungi file»: selettore di sistema; il file arriva in import/ come `file-…--<nome>`.
    pub(super) fn contents_add(&mut self) {
        self.pdf_pick = None;
        self.pick_any_file();
    }

    /// File scelto dall'utente (dalla cartella import/): pacchetto o file singolo.
    pub(super) fn contents_import(&mut self, path: PathBuf) {
        let bytes = vfs::read(&path).unwrap_or_default();
        let _ = vfs::remove_file(&path);
        let fname = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let name = fname.split_once("--").map(|(_, n)| n.to_string()).unwrap_or(fname);
        if bytes.is_empty() {
            self.toast("File vuoto o non leggibile");
            return;
        }
        if contents::classify(&name, &bytes) == Ok(Kind::Pack) {
            // un pacchetto dello stesso esame si unisce; di un altro esame si importa come sempre
            match prep_core::pack::exam_of(&bytes) {
                Ok((id, _)) if id == self.exams.active => match prep_core::pack::import_merge(&self.store, &bytes) {
                    Ok(info) => self.contents_changed(format!(
                        "Pacchetto unito: {} domande e {} flashcard nel pacchetto, i file che non conteneva sono rimasti.",
                        info.questions, info.flashcards
                    )),
                    Err(e) => self.toast(format!("Pacchetto non importato: {e}")),
                },
                Ok(_) => {
                    let p = self.store.base.join("import").join(format!("pack-{}.json.gz", chrono::Utc::now().timestamp_millis()));
                    if vfs::write(&p, &bytes).is_ok() {
                        self.import_pack_file(p, true);
                    }
                }
                Err(e) => self.toast(format!("Pacchetto non importato: {e}")),
            }
            return;
        }
        let subject = self.ui().global::<AppState>().get_current_subject().to_string();
        match contents::add_file(&self.store, &name, &bytes, &subject) {
            Ok(a) => {
                let warn = match a.warnings.len() {
                    0 => String::new(),
                    1 => format!(" Avviso: {}", a.warnings[0]),
                    n => format!(" {n} avvisi, il primo: {}", a.warnings[0]),
                };
                self.contents_changed(format!("Aggiunto «{name}» ({}, {}).{warn}", a.kind.label(), a.summary));
            }
            Err(e) => {
                self.ui().global::<AppState>().set_contents_status(ss(format!("«{name}» non aggiunto: {e}")));
                self.toast(format!("File non aggiunto: {e}"));
            }
        }
    }

    pub(super) fn contents_remove_ask(&mut self, path: &str) {
        let what = if path == "content/syllabus.json" {
            "È la definizione dell'esame: senza, materie e argomenti spariscono finché non ne aggiungi un'altra."
        } else {
            "Il file viene tolto da questo dispositivo; i tuoi progressi restano."
        };
        self.open_dialog(Dialog::RemoveContent(path.to_string()), &format!("Togliere {path}?"), what, "Togli");
    }

    pub(super) fn contents_remove(&mut self, path: &str) {
        match contents::remove_file(&self.store, path) {
            Ok(()) => self.contents_changed(format!("Tolto {path}.")),
            Err(e) => self.toast(format!("Non tolto: {e}")),
        }
    }

    /// «Esporta pacchetto»: tutti i contenuti dell'esame in un file .json.gz.
    pub(super) fn contents_export(&mut self) {
        let label = format!("esportato il {}", chrono::Local::now().format("%d/%m/%Y"));
        let bytes = match contents::export(&self.store, &label) {
            Ok(b) => b,
            Err(e) => {
                self.toast(format!("Esportazione non riuscita: {e}"));
                return;
            }
        };
        let name = format!("{}.json.gz", prep_core::exams::slug(&self.exams.active));
        #[cfg(target_os = "android")]
        {
            let p = self.store.base.join("export").join(&name);
            let _ = vfs::create_dir_all(&self.store.base.join("export"));
            match vfs::write(&p, &bytes).map_err(|e| e.to_string()).and_then(|_| crate::android_save_file(&p, &name)) {
                Ok(()) => {}
                Err(e) => self.toast(format!("Esportazione non riuscita: {e}")),
            }
        }
        #[cfg(target_arch = "wasm32")]
        crate::web::download_file(&name, &bytes);
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        match crate::desktop_files::save_to_downloads(&name, &bytes) {
            Ok(p) => {
                let msg = format!("Pacchetto salvato in {}", p.display());
                self.ui().global::<AppState>().set_contents_status(ss(&msg));
                self.toast(msg);
            }
            Err(e) => self.toast(format!("Esportazione non riuscita: {e}")),
        }
    }
}
