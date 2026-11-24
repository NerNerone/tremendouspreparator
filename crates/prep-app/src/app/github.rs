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
        // dopo la prima sincronizzazione (rete e token funzionano): c'è una versione nuova?
        self.update_check();
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

// ---------- aggiornamento dell'app (Android) ----------
//
// Dopo la prima sincronizzazione si guarda l'ultima release del repository: se è più recente
// della versione installata compare il pulsante «Aggiorna» nelle Opzioni (e un avviso). Il
// download e l'installazione partono solo dal pulsante e mai durante una sessione di studio o
// un quiz; Android chiede conferma e controlla che la firma sia la stessa dell'app installata.

static UPDATE_CHECKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

/// Versione Flatpak (computer Linux): si aggiorna con il bundle della release.
fn flatpak() -> bool {
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    return crate::flatpak::inside();
    #[cfg(any(target_os = "android", target_arch = "wasm32"))]
    false
}

/// Il pacchetto da scaricare tra gli allegati della release: l'APK, o il bundle Flatpak.
fn apk_asset(r: &prep_core::github::Release) -> Option<prep_core::github::ReleaseAsset> {
    if flatpak() {
        return r.assets.iter().find(|a| a.name.ends_with(".flatpak")).cloned();
    }
    let apks: Vec<_> = r.assets.iter().filter(|a| a.name.ends_with(".apk")).collect();
    apks.iter().find(|a| a.name.contains("arm64")).or_else(|| apks.first()).map(|a| (*a).clone())
}

/// La versione più nuova di quella installata che ha già il pacchetto per questo dispositivo
/// (l'APK esce prima, il Flatpak si allega una decina di minuti dopo): versione e allegato.
fn newest_package(gh: &prep_core::github::GitHub) -> Result<Option<(String, prep_core::github::ReleaseAsset)>, String> {
    let mine = prep_core::github::version_code(env!("CARGO_PKG_VERSION")).unwrap_or(0);
    let list = gh.releases(15).map_err(|e| e.to_string())?;
    Ok(list
        .iter()
        .filter(|r| !r.draft && !r.prerelease && r.tag_name.starts_with('v'))
        .filter_map(|r| {
            let v = prep_core::github::tag_version(&r.tag_name).to_string();
            let code = prep_core::github::version_code(&v)?;
            (code > mine).then_some((code, v, apk_asset(r)?))
        })
        .max_by_key(|(code, _, _)| *code)
        .map(|(_, v, a)| (v, a)))
}

/// Il pacchetto trovato dall'ultimo controllo: si installa proprio quello.
static FOUND: std::sync::Mutex<Option<prep_core::github::ReleaseAsset>> = std::sync::Mutex::new(None);

impl Ctrl {
    /// «Cerca aggiornamenti» (Opzioni): la release più nuova con il pacchetto, subito, con l'esito.
    pub(super) fn update_check_now(&mut self) {
        let Some(gh) = github(&self.store.settings()) else {
            self.toast("Serve il token GitHub nelle Opzioni");
            return;
        };
        self.ui().global::<AppState>().set_update_checking(true);
        spawn_background(move || {
            let r = newest_package(&gh);
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| {
                    let ui = c.ui();
                    let st = ui.global::<AppState>();
                    st.set_update_checking(false);
                    match r {
                        Ok(Some((v, a))) => {
                            *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = Some(a);
                            st.on_update_install(|| with_ctrl(|c| c.update_install()));
                            st.set_update_version(ss(&v));
                            c.toast(format!("È disponibile la versione {v}: tocca «Aggiorna»"));
                        }
                        Ok(None) => c.toast(format!("Hai già l'ultima versione ({})", env!("CARGO_PKG_VERSION"))),
                        Err(e) => c.toast(format!("Controllo non riuscito: {e}")),
                    }
                })
            });
        });
    }

    pub(super) fn update_check(&mut self) {
        if !(cfg!(target_os = "android") || flatpak()) || UPDATE_CHECKED.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let ui = self.ui();
        ui.global::<AppState>().on_update_install(|| with_ctrl(|c| c.update_install()));
        let Some(gh) = github(&self.store.settings()) else { return };
        spawn_background(move || {
            if let Ok(Some((v, a))) = newest_package(&gh) {
                *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = Some(a);
                let _ = slint::invoke_from_event_loop(move || {
                    with_ctrl(|c| {
                        c.ui().global::<AppState>().set_update_version(ss(&v));
                        c.toast(format!("È disponibile la versione {v}: Opzioni › Aggiorna"));
                    })
                });
            }
        });
    }

    pub(super) fn update_install(&mut self) {
        if self.quiz.is_some() || self.study_state != prep_core::group::State::Idle {
            self.toast("Prima termina il quiz o la sessione di studio: l'aggiornamento riavvia l'app");
            return;
        }
        let Some(gh) = github(&self.store.settings()) else {
            self.toast("Serve il token GitHub nelle Opzioni");
            return;
        };
        let ui = self.ui();
        ui.global::<AppState>().set_update_busy(true);
        let dest = self.store.base.join("updates").join(if flatpak() {
            "SemestreAperto-x86_64.flatpak"
        } else {
            "aggiornamento.apk"
        });
        let found = FOUND.lock().unwrap_or_else(|e| e.into_inner()).clone();
        spawn_background(move || {
            let asset = match found {
                Some(a) => Ok(a),
                None => newest_package(&gh).and_then(|f| {
                    f.map(|(_, a)| a)
                        .ok_or_else(|| "nessuna versione più nuova con il pacchetto per questo dispositivo".to_string())
                }),
            };
            let r = asset.and_then(|a| gh.download_asset(a.id, &dest).map_err(|e| e.to_string())).map(|_| dest);
            // nel Flatpak l'installazione (lunga) si fa qui, fuori dal thread dell'interfaccia
            #[cfg(not(target_os = "android"))]
            let r = r.and_then(|p| install_apk(&p)).map(|_| None::<std::path::PathBuf>);
            #[cfg(target_os = "android")]
            let r = r.map(Some);
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| {
                    c.ui().global::<AppState>().set_update_busy(false);
                    match r.and_then(|p| p.map(|p| install_apk(&p)).unwrap_or(Ok(()))) {
                        Ok(()) => {}
                        Err(e) if e.starts_with("Si è aperto") => c.toast(e),
                        Err(e) => c.toast(format!("Aggiornamento non riuscito: {e}")),
                    }
                })
            });
        });
    }
}

/// Passa l'APK all'installatore di sistema (classe Java `Updater`, solo nella build privata).
#[cfg(target_os = "android")]
fn install_apk(path: &std::path::Path) -> Result<(), String> {
    let app = crate::ANDROID_APP.get().ok_or("attività non pronta")?;
    // SAFETY: puntatori validi per tutta la vita del processo, forniti da android-activity
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    // le classi dell'app si caricano con il class loader dell'attività
    let loader = env
        .call_method(&activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .and_then(|v| v.l())
        .map_err(|e| e.to_string())?;
    let name = env.new_string("io.github.nernerone.tremendouspreparator.Updater").map_err(|e| e.to_string())?;
    let class = env
        .call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[(&name).into()])
        .and_then(|v| v.l())
        .map_err(|e| e.to_string())?;
    let class = jni::objects::JClass::from(class);
    let p = env.new_string(path.to_string_lossy()).map_err(|e| e.to_string())?;
    let out = env
        .call_static_method(
            &class,
            "install",
            "(Landroid/app/Activity;Ljava/lang/String;)Ljava/lang/String;",
            &[(&activity).into(), (&p).into()],
        )
        .and_then(|v| v.l())
        .map_err(|e| e.to_string())?;
    let msg: String = env.get_string(&jni::objects::JString::from(out)).map_err(|e| e.to_string())?.into();
    if msg.is_empty() {
        Ok(())
    } else {
        Err(msg)
    }
}

/// Computer: il bundle Flatpak si installa sull'host e l'app si riavvia.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn install_apk(path: &std::path::Path) -> Result<(), String> {
    // se riesce l'app si riavvia; altrimenti il bundle si è aperto nel gestore di software
    crate::flatpak::install_and_restart(path)?;
    Err("Si è aperto il gestore di software: conferma lì l'aggiornamento".into())
}

#[cfg(target_arch = "wasm32")]
fn install_apk(_: &std::path::Path) -> Result<(), String> {
    Err("disponibile solo su Android e nel Flatpak".into())
}
