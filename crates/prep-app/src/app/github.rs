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
