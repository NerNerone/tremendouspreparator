//! Sincronizzazione con il repository GitHub dei progressi.

use super::*;

impl Ctrl {
    // ---------- sync ----------

    pub(super) fn start_sync(&mut self, verbose: bool) {
        self.p2p_announce();
        self.sync_verbose |= verbose;
        if self.syncing {
            return;
        }
        if !self.sync_configured() {
            // GitHub spento: i progressi viaggiano solo tra i propri dispositivi (qui sopra)
            if verbose {
                self.toast("Sincronizzato con i tuoi dispositivi collegati");
            }
            self.refresh_settings_info();
            return;
        }
        self.groups_to_repo();
        self.syncing = true;
        self.ui().global::<AppState>().set_syncing(true);
        let store = self.store.clone();
        self.ui().global::<AppState>().set_sync_status(ss("Sincronizzazione in corso…"));
        spawn_background(move || {
            let rep = sync::sync_with_progress(&store, &|n, tot| {
                if tot > 0 && (n == 0 || n % 10 == 0 || n == tot) {
                    let msg = format!("Scarico i contenuti: {n}/{tot} file…");
                    let _ = slint::invoke_from_event_loop(move || {
                        with_ctrl(|c| c.ui().global::<AppState>().set_sync_status(ss(&msg)))
                    });
                }
            });
            let _ = slint::invoke_from_event_loop(move || with_ctrl(|c| c.on_sync_done(rep)));
        });
    }

    pub(super) fn on_sync_done(&mut self, rep: SyncReport) {
        self.syncing = false;
        self.ui().global::<AppState>().set_syncing(false);
        if rep.downloaded > 0 || rep.removed > 0 {
            self.reload_content();
        }
        self.groups_from_repo();
        self.reload_progress();
        self.refresh_all();
        if self.results.is_some() {
            self.render_results();
        }
        if rep.error.is_some() || self.sync_verbose || rep.downloaded > 0 {
            self.toast(rep.summary());
        }
        self.sync_verbose = false;
    }
}

// ---------- repository dei dati: interruttore, rami, gruppi ----------

/// Cartella del repository con le versioni dell'elenco dei propri gruppi.
const GROUPS_DIR: &str = "progress/groups";

impl Ctrl {
    /// Accende o spegne la sincronizzazione con GitHub (spenta di default).
    pub(super) fn gh_toggle(&mut self, on: bool) {
        let mut s = self.store.settings();
        s.github_on = on;
        let _ = self.store.save_settings(&s);
        self.refresh_settings_info();
        if on && self.sync_configured() {
            self.gh_load_branches();
        }
    }

    fn gh_client(&self) -> Option<prep_core::github::GitHub> {
        if !self.sync_configured() {
            return None;
        }
        github(&self.store.settings())
    }

    /// Elenco dei rami del repository (uno per esame o per versione di un esame).
    pub(super) fn gh_load_branches(&mut self) {
        let Some(gh) = self.gh_client() else {
            self.toast("Prima accendi GitHub e inserisci proprietario, repository e token");
            return;
        };
        self.ui().global::<AppState>().set_gh_branch_status(ss("Leggo i rami del repository…"));
        spawn_background(move || {
            let r = gh.branches();
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| {
                    let ui = c.ui();
                    let st = ui.global::<AppState>();
                    match r {
                        Ok(v) => {
                            st.set_gh_branch_status(ss(if v.is_empty() { "Nessun ramo nel repository" } else { "" }));
                            c.gh_branches = v;
                        }
                        Err(e) => st.set_gh_branch_status(ss(format!("Rami non letti: {e}"))),
                    }
                    c.refresh_gh_branches();
                })
            });
        });
    }

    pub(super) fn refresh_gh_branches(&mut self) {
        let mine = self.store.exam_branch();
        // rami già collegati a un esame installato
        let base = self.store.base.clone();
        let linked: Vec<(String, String)> = self
            .exams
            .exams
            .iter()
            .filter_map(|e| {
                let st = Store::open_in(&base, prep_core::exams::dir(&base, &e.id)).ok()?;
                let b = st.exam_branch();
                (!b.is_empty()).then(|| (b, e.name.clone()))
            })
            .collect();
        let vms: Vec<QzOptVM> = self
            .gh_branches
            .iter()
            .map(|b| QzOptVM {
                id: ss(b),
                name: ss(b),
                group: ss(linked.iter().find(|(x, _)| x == b).map(|(_, n)| n.clone()).unwrap_or_default()),
                sel: *b == mine,
                ..Default::default()
            })
            .collect();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_gh_branches(model(vms));
        st.set_gh_exam_branch(ss(&mine));
    }

    /// L'esame aperto si sincronizza da questo ramo.
    pub(super) fn gh_pick_branch(&mut self, branch: &str) {
        if self.exams.active.is_empty() {
            self.gh_add_exam(branch);
            return;
        }
        if let Err(e) = self.store.set_exam_branch(branch) {
            self.toast(format!("Errore: {e}"));
            return;
        }
        self.refresh_gh_branches();
        self.toast(format!("Questo esame si sincronizza dal ramo «{branch}»"));
        self.start_sync(true);
    }

    /// Nome di ramo valido per GitHub (lettere, cifre, trattini), o vuoto.
    fn branch_name(raw: &str) -> String {
        let mut out = String::new();
        for c in raw.trim().chars() {
            let c = if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/' {
                c.to_ascii_lowercase()
            } else {
                '-'
            };
            if !(c == '-' && out.ends_with('-')) {
                out.push(c);
            }
        }
        out.trim_matches(|c| c == '-' || c == '/' || c == '.').to_string()
    }

    /// Nuovo ramo nel repository con il modello (guida, schema, strumenti per estrarre il
    /// testo e l'OCR, direttive per l'AI) e il pacchetto d'esempio: lì si prepara un esame.
    pub(super) fn gh_new_branch(&mut self, raw: &str) {
        let name = Self::branch_name(raw);
        if name.is_empty() {
            self.toast("Scrivi il nome del ramo, per esempio anatomia-2027");
            return;
        }
        let Some(gh) = self.gh_client() else {
            self.toast("Prima accendi GitHub in Tu › App e inserisci proprietario, repository e token");
            return;
        };
        self.toast(format!("Creo il ramo «{name}» dal modello…"));
        spawn_background(move || {
            let r = gh
                .ensure_private()
                .and_then(|_| gh.publish_branch(&name, &super::modello::files(true), "Nuovo esame dal modello", None));
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| match r {
                    Ok(()) => {
                        c.toast(format!(
                            "Ramo «{name}» creato dal modello: dentro ci sono la guida e gli strumenti per \
                             preparare il pacchetto con l'AI. Quando è pronto, scaricalo con «Da un ramo»."
                        ));
                        c.gh_load_branches();
                    }
                    Err(e) => c.toast(format!("Ramo non creato: {e}")),
                })
            });
        });
    }

    /// Dopo un pacchetto importato (da file o ricevuto con un codice): con GitHub configurato e
    /// l'esame non ancora in un ramo, propone di salvarlo in un ramo nuovo del repository.
    pub(super) fn offer_branch(&mut self, id: &str) {
        if self.gh_client().is_none() || !self.exam_store(id).exam_branch().is_empty() {
            return;
        }
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_offer_branch_exam(ss(id));
        st.set_offer_branch_name(ss(Self::branch_name(id)));
        st.set_offer_branch(true);
    }

    /// «Salva nel ramo» dalla proposta: l'esame importato diventa quello in uso e va nel ramo.
    pub(super) fn offer_branch_save(&mut self, raw: &str) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_offer_branch(false);
        let id = st.get_offer_branch_exam().to_string();
        if !id.is_empty() && self.exams.active != id && self.exams.get(&id).is_some() {
            self.open_exam(&id);
        }
        self.gh_save_branch(raw);
    }

    /// Salva l'esame in uso (contenuti, libri e progressi, per esempio importati da un
    /// pacchetto locale) in un ramo nuovo basato sul modello, e da lì in poi lo sincronizza.
    pub(super) fn gh_save_branch(&mut self, raw: &str) {
        let name = Self::branch_name(raw);
        if name.is_empty() {
            self.toast("Scrivi il nome del ramo, per esempio semestre-aperto");
            return;
        }
        let Some(gh) = self.gh_client() else {
            self.toast("Prima accendi GitHub in Tu › App e inserisci proprietario, repository e token");
            return;
        };
        if self.exams.active.is_empty() {
            self.toast("Nessun esame da salvare");
            return;
        }
        let mine = self.store.mirror_files();
        if !mine.iter().any(|(p, _)| p == "content/syllabus.json") {
            self.toast("Questo esame non ha ancora contenuti da salvare");
            return;
        }
        let exam = self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_default();
        let n = mine.len();
        // il modello (guida, strumenti, direttive per l'AI) più i file dell'esame
        let mut files = super::modello::files(false);
        files.retain(|(p, _)| !mine.iter().any(|(m, _)| m == p));
        files.extend(mine);
        self.toast(format!("Salvo «{exam}» nel ramo «{name}» ({n} file)…"));
        let root = self.store.root.clone();
        spawn_background(move || {
            let r = gh
                .ensure_private()
                .and_then(|_| gh.publish_branch(&name, &files, &format!("Esame «{exam}» dall'app"), None));
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| match r {
                    Ok(()) => {
                        // l'esame attivo potrebbe essere cambiato nel frattempo: si scrive nel suo archivio
                        if let Ok(st) = Store::open_in(c.store.base.clone(), root.clone()) {
                            let _ = st.set_exam_branch(&name);
                        }
                        c.toast(format!("Esame salvato nel ramo «{name}»: da ora si sincronizza da lì"));
                        c.refresh_exams();
                        c.gh_load_branches();
                        c.start_sync(true);
                    }
                    Err(e) => c.toast(format!("Esame non salvato: {e}")),
                })
            });
        });
    }

    /// Nuovo esame scaricato da un ramo del repository (pacchetto e progressi).
    pub(super) fn gh_add_exam(&mut self, branch: &str) {
        let id: String = branch
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        if id.is_empty() {
            return;
        }
        let id = if self.exams.get(&id).is_some() { format!("{id}-gh") } else { id };
        let base = self.store.base.clone();
        let st = match Store::open_in(&base, prep_core::exams::dir(&base, &id)) {
            Ok(st) => st,
            Err(e) => {
                self.toast(format!("Errore: {e}"));
                return;
            }
        };
        let _ = st.set_exam_branch(branch);
        self.exams.add(&id, branch);
        let _ = self.exams.save(&base);
        self.open_exam(&id);
        self.toast(format!("Scarico l'esame dal ramo «{branch}»…"));
        self.start_sync(true);
    }

    /// Elenco dei propri gruppi nel repository (senza le chiavi di amministratore, che
    /// viaggiano solo cifrate tra i propri dispositivi): un file nuovo quando cambia.
    pub(super) fn groups_to_repo(&mut self) {
        let mut refs = self.groups.refs();
        for r in &mut refs {
            r.admin_sk.clear();
        }
        if refs.is_empty() && self.groups.left.is_empty() {
            return;
        }
        let body = serde_json::json!({ "groups": refs, "left": self.groups.left });
        let text = serde_json::to_string_pretty(&body).unwrap_or_default();
        let digest = prep_core::p2p::sha256_hex(text.as_bytes());
        let mark = self.store.root.join("groups_pushed.txt");
        if vfs::read_to_string(&mark).ok().as_deref() == Some(digest.as_str()) {
            return;
        }
        let now = Utc::now().format("%Y%m%dT%H%M%S%3fZ").to_string();
        let path = format!("{GROUPS_DIR}/{now}.json");
        if self
            .store
            .record_file(&format!("groups-{now}"), path, text.into_bytes(), "gruppi: elenco aggiornato")
            .is_ok()
        {
            let _ = vfs::write(&mark, digest.as_bytes());
        }
    }

    /// Gruppi scritti nel repository da altri propri dispositivi: vale l'ultima versione.
    pub(super) fn groups_from_repo(&mut self) {
        let dir = self.store.root.join("mirror").join(GROUPS_DIR);
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let Some(last) =
            rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).max()
        else {
            return;
        };
        let Ok(t) = vfs::read_to_string(&last) else { return };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else { return };
        let refs: Vec<prep_core::group::GroupRef> = serde_json::from_value(v["groups"].clone()).unwrap_or_default();
        let left: std::collections::BTreeMap<String, i64> =
            serde_json::from_value(v["left"].clone()).unwrap_or_default();
        if self.groups.merge_own(&refs, &left) {
            let _ = self.groups.save(&self.store.base);
            self.group_connect();
            self.refresh_group();
        }
    }
}

/// Client del repository dei dati (proprietario, repository e token delle Opzioni).
fn github(s: &prep_core::store::Settings) -> Option<prep_core::github::GitHub> {
    let (owner, repo, token) = (s.owner.trim(), s.repo.trim(), s.token.trim());
    if owner.is_empty() || repo.is_empty() || token.is_empty() {
        return None;
    }
    Some(prep_core::github::GitHub::new(prep_core::github::RepoConfig {
        api_base: s.api_base.clone(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        token: token.to_string(),
    }))
}
