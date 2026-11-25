//! Pagina «Contenuti»: i file dell'esame uno per uno (aggiungere, togliere, esportare).

use super::*;
use crate::ContentFileVM;
use prep_core::contents::{self, Kind};

/// Righe dell'elenco dei file mostrate all'apertura.
pub(super) const CT_PAGE: usize = 60;

impl Ctrl {
    pub(super) fn open_contents(&mut self) {
        self.ct_limit = CT_PAGE;
        self.refresh_contents();
        self.ui().global::<AppState>().set_contents_status(ss(""));
        self.set_page(P_CONTENTS);
    }

    pub(super) fn refresh_contents(&mut self) {
        let all = contents::list(&self.store);
        // file scelti che non ci sono più (tolti o sostituiti)
        self.ct_sel.retain(|p| all.iter().any(|e| e.path == *p));
        self.refresh_ct_tools();
        let f = self.ct_filter;
        // gli strumenti hanno la loro sezione
        let shown: Vec<_> = all
            .into_iter()
            .filter(|e| e.kind != Kind::Tool)
            .filter(|e| match f {
                1 => e.kind == Kind::Questions,
                2 => e.kind == Kind::Cards,
                3 => matches!(e.kind, Kind::Books | Kind::Chapter | Kind::Figure),
                4 => matches!(e.kind, Kind::Exam | Kind::Pack),
                5 => false,
                _ => true,
            })
            .collect();
        // centinaia di file (figure, capitoli): se ne mostrano i primi, gli altri a richiesta,
        // perché ogni riga è un elemento dell'interfaccia da creare e disegnare
        let total = shown.len();
        let shown: Vec<_> = shown.into_iter().take(self.ct_limit.max(1)).collect();
        let n = shown.len();
        let mut last = None;
        let files: Vec<ContentFileVM> = shown
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                let head = last != Some(e.kind);
                last = Some(e.kind);
                ContentFileVM {
                    path: ss(&e.path),
                    kind: ss(e.kind.label()),
                    name: ss(&e.name),
                    info: ss(&e.info),
                    head,
                    icon: match e.kind {
                        Kind::Questions => 0,
                        Kind::Cards => 1,
                        Kind::Books | Kind::Chapter | Kind::Figure => 2,
                        _ => 3,
                    },
                    sel: self.ct_sel.contains(&e.path),
                    first: i == 0,
                    last: i + 1 == n,
                }
            })
            .collect();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if let Some(m) = sync_model(st.get_content_files(), files) {
            st.set_content_files(m);
        }
        st.set_contents_more((total - n) as i32);
        st.set_contents_filter_n(f);
        st.set_contents_sel_n(self.ct_sel.len() as i32);
    }

    pub(super) fn contents_filter(&mut self, k: i32) {
        self.ct_filter = k.clamp(0, 5);
        self.ct_limit = CT_PAGE;
        self.refresh_contents();
    }

    /// «Mostra altri»: la pagina successiva dell'elenco dei file.
    pub(super) fn contents_show_more(&mut self) {
        self.ct_limit += 4 * CT_PAGE;
        self.refresh_contents();
    }

    /// Scelta di un file: cambia solo la sua riga (senza rileggere i file).
    pub(super) fn contents_toggle(&mut self, path: &str) {
        let on = !self.ct_sel.remove(path);
        if on {
            self.ct_sel.insert(path.to_string());
        }
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let m = st.get_content_files();
        for i in 0..m.row_count() {
            if let Some(mut r) = m.row_data(i) {
                if r.path == path {
                    r.sel = on;
                    m.set_row_data(i, r);
                }
            }
        }
        st.set_contents_sel_n(self.ct_sel.len() as i32);
    }

    pub(super) fn contents_clear_sel(&mut self) {
        self.ct_sel.clear();
        self.refresh_contents();
    }

    /// «Togli» sui file scelti: una sola conferma per tutti.
    pub(super) fn contents_remove_selected(&mut self) {
        let v: Vec<String> = self.ct_sel.iter().cloned().collect();
        match v.len() {
            0 => {}
            1 => self.contents_remove_ask(&v[0]),
            n => {
                let what = if v.iter().any(|p| p == "content/syllabus.json") {
                    "Tra questi c'è la definizione dell'esame: senza, materie e argomenti spariscono finché non ne aggiungi un'altra. I tuoi progressi restano."
                } else {
                    "I file vengono tolti da questo dispositivo; i tuoi progressi restano."
                };
                self.open_dialog(Dialog::RemoveContents(v), &format!("Togliere {n} file?"), what, "Togli");
            }
        }
    }

    pub(super) fn contents_remove_many(&mut self, paths: &[String]) {
        let mut ok = 0;
        let mut err = None;
        for p in paths {
            match contents::remove_file(&self.store, p) {
                Ok(()) => ok += 1,
                Err(e) => err = Some(e),
            }
        }
        self.ct_sel.clear();
        match err {
            None => self.contents_changed(format!("Tolti {ok} file.")),
            Some(e) => {
                self.contents_changed(format!("Tolti {ok} file su {}.", paths.len()));
                self.toast(format!("Alcuni file non tolti: {e}"));
            }
        }
    }

    /// Dopo una modifica: contenuti, progressi e pagine si ricalcolano.
    pub(super) fn contents_changed(&mut self, status: String) {
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
        // uno strumento da solo: con più pacchetti si sceglie dove metterlo
        if contents::classify(&name, &bytes) == Ok(Kind::Tool) && self.receive_tool(bytes.clone()) {
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
            Ok(()) => {
                self.ct_sel.remove(path);
                self.contents_changed(format!("Tolto {path}."))
            }
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
        self.deliver_file(&name, &bytes);
    }

    /// Un file da condividere: su Android si sceglie dove salvarlo, sul web si scarica, sul
    /// computer va in Download.
    pub(super) fn deliver_file(&mut self, name: &str, bytes: &[u8]) {
        #[cfg(target_os = "android")]
        {
            let p = self.store.base.join("export").join(name);
            let _ = vfs::create_dir_all(&self.store.base.join("export"));
            match vfs::write(&p, bytes).map_err(|e| e.to_string()).and_then(|_| crate::android_save_file(&p, name)) {
                Ok(()) => {}
                Err(e) => self.toast(format!("Esportazione non riuscita: {e}")),
            }
        }
        #[cfg(target_arch = "wasm32")]
        crate::web::download_file(name, bytes);
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        match crate::desktop_files::save_to_downloads(name, bytes) {
            Ok(p) => {
                let msg = format!("Salvato in {}", p.display());
                self.ui().global::<AppState>().set_contents_status(ss(&msg));
                self.toast(msg);
            }
            Err(e) => self.toast(format!("Esportazione non riuscita: {e}")),
        }
    }

    /// «Controlla i contenuti»: errori, avvisi e consigli sui file dell'esame.
    pub(super) fn contents_check(&mut self) {
        let store = &self.store;
        let r = prep_core::quality::check(&self.content, &|p| {
            store.read_mirror(p).map(|b| String::from_utf8_lossy(&b).to_string())
        });
        let line = |t: String, head: bool| crate::QaLineVM { text: ss(t), head };
        let mut lines = Vec::new();
        for (title, list) in [("Errori", &r.errors), ("Avvisi", &r.warnings), ("Consigli", &r.tips)] {
            if !list.is_empty() {
                lines.push(line(format!("{title} ({})", list.len()), true));
                lines.extend(list.iter().take(60).map(|x| line(x.clone(), false)));
                if list.len() > 60 {
                    lines.push(line(format!("… e altri {}", list.len() - 60), false));
                }
            }
        }
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_qa_summary(ss(r.summary()));
        st.set_qa_lines(model(lines));
    }
}
