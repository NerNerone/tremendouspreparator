//! Pacchetti contenuti ed esami installati.

use super::*;

impl Ctrl {
    // ---------- pacchetto contenuti ----------

    pub(super) fn pack_status(&self) -> String {
        let n = self.content.questions.len();
        let c = self.content.cards.len();
        match prep_core::pack::last_import(&self.store) {
            Some(i) => format!(
                "In uso: {n} domande, {c} flashcard. Ultimo pacchetto importato: {} del {} (importato il {}).",
                if i.label.is_empty() { "senza nome".to_string() } else { i.label.clone() },
                short_date(&i.created),
                short_date(&i.imported)
            ),
            None if self.from_repo => format!("In uso: {n} domande, {c} flashcard, sincronizzate dal repository."),
            None => format!("In uso: {n} domande, {c} flashcard, incluse nell'app."),
        }
    }

    /// Importa in background il file `path`; `remove` lo cancella dopo (file della cartella import/).
    pub(super) fn import_pack_file(&mut self, path: PathBuf, remove: bool) {
        if self.importing {
            return;
        }
        self.importing = true;
        self.ui().global::<AppState>().set_pack_busy(true);
        let base = self.store.base.clone();
        spawn_background(move || {
            // il pacchetto va nell'archivio del suo esame (nuovo o già installato)
            let res = vfs::read(&path).map_err(|e| format!("file non leggibile: {e}")).and_then(|b| {
                let (id, name) = prep_core::pack::exam_of(&b)?;
                let store = Store::open_in(&base, prep_core::exams::dir(&base, &id)).map_err(|e| e.to_string())?;
                prep_core::pack::import(&store, &b).map(|info| (info, id, name))
            });
            if remove {
                let _ = vfs::remove_file(&path);
            }
            let _ = slint::invoke_from_event_loop(move || with_ctrl(|c| c.on_pack_imported(res)));
        });
    }

    pub(super) fn on_pack_imported(&mut self, res: Result<(prep_core::pack::PackInfo, String, String), String>) {
        self.importing = false;
        self.ui().global::<AppState>().set_pack_busy(false);
        match res {
            Ok((info, id, name)) => {
                let is_new = self.exams.get(&id).is_none();
                let was_active = self.exams.active == id;
                self.exams.upsert(&id, &name);
                let _ = self.exams.save(&self.store.base);
                if was_active {
                    self.reload_content();
                    self.reload_progress();
                    self.refresh_all();
                    self.refresh_exams();
                } else {
                    self.open_exam(&id);
                }
                self.refresh_settings_info();
                self.toast(format!(
                    "{} «{name}»: {} domande, {} flashcard",
                    if is_new { "Esame aggiunto" } else { "Contenuti aggiornati" },
                    info.questions,
                    info.flashcards
                ));
            }
            Err(e) => self.toast(format!("Pacchetto non importato: {e}")),
        }
    }

    // ---------- esami installati ----------

    pub(super) fn refresh_exams(&mut self) {
        let base = self.store.base.clone();
        let v: Vec<ExamVM> = self
            .exams
            .exams
            .iter()
            .map(|e| {
                let st = Store { root: prep_core::exams::dir(&base, &e.id), base: base.clone() };
                let info = match prep_core::pack::last_import(&st) {
                    Some(i) => format!("{} domande · pacchetto del {}", i.questions, short_date(&i.created)),
                    None if e.id == self.exams.active => format!("{} domande", self.content.questions.len()),
                    None => "contenuti inclusi nell'app".into(),
                };
                let active = e.id == self.exams.active;
                let mut info = info;
                if active && !self.content.questions.is_empty() {
                    info = format!("{}% · {info}", (self.progress.overall * 100.0).round());
                }
                let branch = st.exam_branch();
                if !branch.is_empty() {
                    info = format!("{info} · ramo {branch}");
                }
                let initials: String = {
                    let mut w =
                        e.name.split_whitespace().filter(|w| w.chars().next().is_some_and(char::is_alphanumeric));
                    let a: String = w.next().map(|w| w.chars().take(2).collect()).unwrap_or_default();
                    let mut c = a.chars();
                    match (c.next(), c.next()) {
                        (Some(x), Some(y)) => format!("{}{}", x.to_uppercase(), y.to_lowercase()),
                        (Some(x), None) => x.to_uppercase().to_string(),
                        _ => "?".into(),
                    }
                };
                ExamVM { id: ss(&e.id), name: ss(&e.name), initials: ss(initials), active, info: ss(info) }
            })
            .collect();
        let name = self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_else(|| "Nessun esame".into());
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_exams(model(v));
        st.set_exam_name(ss(name));
    }

    /// Passa a un altro esame: archivio, contenuti e progressi sono quelli suoi.
    pub(super) fn open_exam(&mut self, id: &str) {
        // id vuoto = nessun esame installato (dopo l'eliminazione dell'ultimo)
        if !id.is_empty() && self.exams.get(id).is_none() {
            return;
        }
        self.flush_flags();
        if self.quiz.is_some() {
            self.pause_quiz();
        }
        self.exams.active = id.to_string();
        let _ = self.exams.save(&self.store.base);
        let base = self.store.base.clone();
        match Store::open_in(&base, prep_core::exams::dir(&base, id)) {
            Ok(st) => self.store = st,
            Err(e) => {
                self.toast(format!("Esame non apribile: {e}"));
                return;
            }
        }
        self.quiz = None;
        self.cards = None;
        self.results = None;
        self.reader = None;
        self.plan = None;
        self.plan_sel.clear();
        self.plan_extra.clear();
        self.unit_id.clear();
        self.fb_item = None;
        self.result_perm.clear();
        self.qz = QuizSetup::default();
        self.reload_content();
        self.reload_progress();
        self.refresh_all();
        self.refresh_exams();
        self.ui().global::<AppState>().set_exam_menu(false);
        self.go_tab(0);
    }

    /// Elimina un esame (contenuti e progressi): serve un secondo tocco entro 5 secondi.
    pub(super) fn remove_exam(&mut self, id: &str) {
        let armed = matches!(&self.remove_armed, Some((x, t)) if x == id && t.elapsed() < Duration::from_secs(5));
        if !armed {
            self.remove_armed = Some((id.to_string(), Instant::now()));
            let name = self.exams.get(id).map(|e| e.name.clone()).unwrap_or_default();
            self.toast(format!("Tocca di nuovo per eliminare «{name}» con i suoi progressi"));
            return;
        }
        self.remove_armed = None;
        let base = self.store.base.clone();
        let was_active = self.exams.active == id;
        self.exams.remove(id);
        prep_core::exams::remove_files(&base, id);
        let _ = self.exams.save(&base);
        if was_active || self.exams.get(&self.exams.active).is_none() {
            let next = self.exams.active.clone();
            self.open_exam(&next);
        } else {
            self.refresh_exams();
        }
        self.toast("Esame eliminato");
    }
}
