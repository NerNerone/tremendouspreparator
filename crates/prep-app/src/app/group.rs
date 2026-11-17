//! Studio di gruppo: gara, classifica, amministrazione, foto e avvisi.

use super::*;

impl Ctrl {
    // ---------- studio di gruppo ----------

    /// Colore della propria monoposto: scelto nel Profilo, altrimenti l'accento di sistema,
    /// altrimenti uno fisso ricavato dall'identificativo.
    pub(super) fn my_color(&mut self) -> u32 {
        let c = self.store.settings().car_color;
        if c != 0 {
            return c;
        }
        self.auto_color()
    }

    pub(super) fn auto_color(&mut self) -> u32 {
        if let Some(a) = self.accent {
            return a;
        }
        let id = self.device_id();
        let h = p2p::sha256_hex(id.as_bytes());
        let n = usize::from_str_radix(&h[..4], 16).unwrap_or(0);
        prep_core::group::CAR_COLORS[n % prep_core::group::CAR_COLORS.len()]
    }

    /// Colori del tema: quello scelto in Aspetto, altrimenti quelli di sistema, altrimenti il blu
    /// predefinito; aggiorna anche le proposte della sezione Aspetto e il colore automatico della
    /// monoposto.
    pub(super) fn apply_theme(&mut self) {
        let chosen = self.store.settings().theme_color;
        let tones = match (&self.system_tones, chosen) {
            (Some(t), 0) => t.clone(),
            (_, 0) => crate::palette::tones_from_seed(crate::palette::DEFAULT_SEED),
            (_, c) => crate::palette::tones_from_seed(c),
        };
        if let Some((light, dark)) = crate::palette::dyn_roles(&tones) {
            self.accent = Some(light[0]);
            let ui = self.ui();
            let m3 = ui.global::<crate::M3>();
            let to = |v: &[u32]| model(v.iter().map(|x| slint::Color::from_argb_encoded(*x)).collect::<Vec<_>>());
            m3.set_dyn_light(to(&light));
            m3.set_dyn_dark(to(&dark));
            m3.set_dynamic(true);
        }
        // proposte: «sistema» (se c'è) e i colori fissi
        let mut v = Vec::new();
        if let Some(t) = &self.system_tones {
            let sys = crate::palette::dyn_roles(t).map(|(l, _)| l[0]).unwrap_or(crate::palette::DEFAULT_SEED);
            v.push(SwatchVM { color: slint::Color::from_argb_encoded(sys), value: 0, sel: chosen == 0, auto: true });
        }
        for (i, s) in crate::palette::SEEDS.iter().enumerate() {
            let sel = chosen == *s || (chosen == 0 && self.system_tones.is_none() && i == 0);
            v.push(SwatchVM { color: slint::Color::from_argb_encoded(*s), value: *s as i32, sel, auto: false });
        }
        self.ui().global::<AppState>().set_theme_swatches(model(v));
        self.refresh_car_colors();
    }

    pub(super) fn set_theme_color(&mut self, c: u32) {
        let mut s = self.store.settings();
        // il blu predefinito senza colori di sistema equivale a «automatico»
        s.theme_color = if self.system_tones.is_none() && c == crate::palette::DEFAULT_SEED { 0 } else { c };
        let _ = self.store.save_settings(&s);
        self.apply_theme();
        self.group_send();
        self.refresh_group();
    }

    pub(super) fn set_car_color(&mut self, c: u32) {
        let mut s = self.store.settings();
        s.car_color = c;
        let _ = self.store.save_settings(&s);
        self.refresh_car_colors();
        self.group_send();
        self.refresh_group();
    }

    pub(super) fn refresh_car_colors(&mut self) {
        let current = self.store.settings().car_color;
        let auto = self.auto_color();
        let mut v =
            vec![SwatchVM { color: slint::Color::from_argb_encoded(auto), value: 0, sel: current == 0, auto: true }];
        v.extend(prep_core::group::CAR_COLORS.iter().map(|c| SwatchVM {
            color: slint::Color::from_argb_encoded(*c),
            value: *c as i32,
            sel: current == *c,
            auto: false,
        }));
        self.ui().global::<AppState>().set_car_swatches(model(v));
    }

    /// Secondi del tratto di studio in corso (non ancora nel registro).
    pub(super) fn study_running(&self, now: i64) -> u64 {
        self.study_since.map(|t| (now - t).clamp(0, 6 * 3600) as u64).unwrap_or(0)
    }

    pub(super) fn my_group_status(&mut self, now: i64) -> prep_core::group::Status {
        let color = self.my_color();
        let name = self.display_name();
        let today = Local::now().date_naive();
        let mut s = prep_core::group::my_status(
            &name,
            color,
            self.study_state,
            &self.study_log,
            self.study_running(now),
            today,
            now,
        );
        s.pit_since = self.pit_since.unwrap_or(0);
        s.doing = self.doing();
        s.uid = self.user_id();
        s.dev = self.device_id();
        let set = self.store.settings();
        s.photo = set.photo;
        s.prev = set.prev_ids;
        s
    }

    /// Identificativo dell'utente nei gruppi (uguale su tutti i propri dispositivi che usano
    /// lo stesso nome utente e la stessa password di sincronizzazione).
    pub(super) fn user_id(&mut self) -> String {
        let now = Self::now_secs();
        if let Some((id, at)) = &self.uid_cache {
            if now - at < 5 {
                return id.clone();
            }
        }
        let s = self.store.settings();
        let dev = self.device_id();
        let derived = prep_core::group::user_id(&s.username, &s.p2p_password, &dev);
        let id = self.adopt_user_id(derived);
        self.uid_cache = Some((id.clone(), now));
        id
    }

    /// Fissa l'id utente la prima volta; poi cambia solo da «dispositivo» a «ricavato da nome
    /// utente e password della sincronizzazione» (così telefono e web app diventano la stessa
    /// persona), ricordando il vecchio id e aggiornandolo nei gruppi salvati.
    pub(super) fn adopt_user_id(&mut self, derived: String) -> String {
        let mut s = self.store.settings();
        let dev = self.device_id();
        if s.user_id.is_empty() {
            s.user_id = derived;
            let _ = self.store.save_settings(&s);
        } else if s.user_id == dev && derived != dev {
            s.prev_ids.push(dev.clone());
            s.user_id = derived.clone();
            let _ = self.store.save_settings(&s);
            self.groups.rename_user(&dev, &derived);
            let _ = self.groups.save(&self.store.base);
            self.uid_cache = None;
            // gli altri lo sanno dal prossimo stato (con l'id precedente in `prev`)
            self.group_send();
        }
        s.user_id
    }

    /// Cosa si sta studiando, da quello che è aperto nell'app (resta l'ultima attività
    /// riconosciuta anche quando si passa ad altre schermate).
    pub(super) fn doing(&mut self) -> String {
        let page = self.ui().global::<AppState>().get_page();
        let now = match page {
            3 => self
                .quiz
                .as_ref()
                .map(|q| format!("{} · {}", if q.is_exam() { "Simulazione d'esame" } else { "Quiz" }, q.title)),
            5 => Some("Flashcard".to_string()),
            9 => self.reader.as_ref().map(|r| format!("Lettura · {}", r.title)),
            12 => Some("Tavola periodica".to_string()),
            _ => None,
        };
        if let Some(d) = now {
            self.last_doing = d;
        }
        self.last_doing.clone()
    }

    /// Porta nel registro il tratto di studio in corso (alla pausa, alla fine, ogni tanto).
    pub(super) fn study_flush(&mut self, now: i64) {
        if let Some(t) = self.study_since {
            let secs = (now - t).clamp(0, 6 * 3600) as u64;
            if secs > 0 {
                self.study_log.add(Local::now().date_naive(), secs);
                let _ = self.study_log.save(&self.store.base);
            }
            self.study_since = if self.study_state == prep_core::group::State::Studying { Some(now) } else { None };
        }
    }

    pub(super) fn group_send(&mut self) {
        if self.group.as_ref().is_none_or(|p| !p.started) {
            return;
        }
        let now = Self::now_secs();
        self.group_last_send = now;
        let status = self.my_group_status(now);
        self.with_engine(Sess::Group, move |e, _| vec![e.group_status(status)]);
    }

    pub(super) fn group_status_in(&mut self, from: String, status: prep_core::group::Status) {
        let now = Self::now_secs();
        let mut fresh = false;
        if !status.photo.is_empty() {
            self.photo_src.insert(status.photo.clone(), from.clone());
        }
        if let Some(g) = self.groups.active_mut() {
            let uid = if status.uid.is_empty() { from.clone() } else { status.uid.clone() };
            // dispositivo mai sentito (nuovo membro o riavvio): gli si ripetono subito le cose
            fresh = g.members.get(&uid).is_none_or(|m| !m.devices.contains_key(&from) || now - m.last_seen > 60);
            g.status_in(&from, status, now);
            let _ = self.groups.save(&self.store.base);
        }
        if fresh {
            // l'amministratore manda le impostazioni del gruppo; tutti il proprio stato
            self.group_send_info_as(true);
            if now - self.group_last_send >= 3 {
                self.group_send();
            }
        }
        self.refresh_group();
    }

    /// Collega la stanza del gruppo attivo (o chiude se non ce n'è).
    pub(super) fn group_connect(&mut self) {
        let Some(g) = self.groups.active() else {
            self.close_session(Sess::Group);
            self.refresh_group();
            return;
        };
        let room = prep_core::group::room(&g.code);
        let device = self.device_id();
        let engine = p2p::Engine::new(&device, &self.display_name(), Self::device_label()).share_only(vec![]);
        self.open_session(Sess::Group, room, engine, String::new(), String::new());
        self.refresh_group();
    }

    pub(super) fn group_create(&mut self, name: &str) {
        let name =
            if name.trim().is_empty() { format!("Gruppo di {}", self.display_name()) } else { name.trim().to_string() };
        let code = prep_core::group::new_code();
        self.groups.join(&code, &name, Self::now_secs());
        let me = self.user_id();
        if let Some(g) = self.groups.active_mut() {
            // chi crea il gruppo ne è l'amministratore
            g.admin = me;
            g.info_at = Self::now_secs();
        }
        let _ = self.groups.save(&self.store.base);
        self.group_connect();
        self.group_invite();
    }

    pub(super) fn group_join(&mut self, text: &str) {
        match prep_core::group::parse_invite(text) {
            Some((code, name)) => {
                self.groups.join(&code, &name, Self::now_secs());
                let _ = self.groups.save(&self.store.base);
                self.group_connect();
                self.ui().global::<AppState>().set_group_join_text(ss(""));
                self.toast("Sei entrato nel gruppo");
            }
            None => self.toast("Invito non valido: incolla il link o il codice di 16 caratteri"),
        }
    }

    pub(super) fn group_select(&mut self, code: &str) {
        if self.groups.groups.iter().any(|g| g.code == code) {
            self.groups.active = code.to_string();
            let _ = self.groups.save(&self.store.base);
            self.group_connect();
        }
    }

    pub(super) fn group_leave(&mut self, code: &str) {
        let armed =
            matches!(&self.group_leave_armed, Some((c, t)) if c == code && t.elapsed() < Duration::from_secs(5));
        if !armed {
            self.group_leave_armed = Some((code.to_string(), Instant::now()));
            self.toast("Tocca di nuovo per uscire dal gruppo");
            return;
        }
        self.group_leave_armed = None;
        self.groups.leave(code);
        let _ = self.groups.save(&self.store.base);
        self.group_connect();
    }

    /// Link d'invito: lo si condivide (Android, web) e resta visibile per copiarlo.
    pub(super) fn group_invite(&mut self) {
        let Some(g) = self.groups.active() else { return };
        let link = prep_core::group::invite_link(&g.code, &g.name);
        self.ui().global::<AppState>().set_group_link(ss(&link));
        let text = format!(
            "Studiamo insieme su Tremendous Preparator! Entra nel gruppo «{}» con il codice {}\n(oppure apri {link})",
            g.name, g.code
        );
        if !crate::share_text(&text) {
            self.toast("Copia il codice qui sopra e mandalo a chi vuoi invitare");
        }
    }

    pub(super) fn study_set(&mut self, state: prep_core::group::State) {
        use prep_core::group::State;
        let now = Self::now_secs();
        self.study_flush(now);
        if let Some(t) = self.pit_since.take() {
            let secs = (now - t).clamp(0, 12 * 3600) as u64;
            if secs > 0 {
                self.study_log.add_pit(Local::now().date_naive(), secs);
                let _ = self.study_log.save(&self.store.base);
            }
        }
        if state == State::Pause {
            self.pit_since = Some(now);
        }
        self.study_state = state;
        self.study_since = (state == State::Studying).then_some(now);
        // concentrazione: il servizio Android sa se si studia (solo «Termina» libera il telefono)
        crate::focus::session(match state {
            State::Idle => 0,
            State::Studying => 1,
            State::Pause => 2,
        });
        self.group_send();
        self.refresh_group();
    }

    /// Ogni secondo: cronometro, stato al gruppo ogni 10 s, circuito e classifica.
    pub(super) fn group_tick(&mut self) {
        let now = Self::now_secs();
        if self.study_since.is_some_and(|t| now - t >= 60) {
            self.study_flush(now);
        }
        if self.group.is_some() && now - self.group_last_send >= 10 {
            self.group_send();
        }
        if self.group.is_some() && now - self.info_sent >= 30 {
            self.group_send_info();
        }
        self.refresh_group();
        self.refresh_focus();
    }

    // ---------- gruppo: amministratore, foto, avvisi ----------

    pub(super) fn is_group_admin(&mut self) -> bool {
        let me = self.user_id();
        self.groups.active().is_some_and(|g| !g.admin.is_empty() && g.admin == me)
    }

    /// Può rinominare il gruppo o cambiarne la foto (vedi `Group::can_admin`).
    pub(super) fn can_admin_group(&mut self) -> bool {
        let me = self.user_id();
        self.groups.active().is_some_and(|g| g.can_admin(&me))
    }

    /// Nei gruppi precedenti all'amministratore, chi agisce per primo lo diventa.
    pub(super) fn claim_legacy_admin(&mut self) {
        let me = self.user_id();
        if let Some(g) = self.groups.active_mut() {
            if g.legacy() {
                g.admin = me;
            }
        }
    }

    /// L'amministratore ripete ogni tanto le impostazioni del gruppo (nome, foto, rimossi):
    /// così le ricevono anche i nuovi arrivati.
    pub(super) fn group_send_info(&mut self) {
        self.group_send_info_as(false);
    }

    /// `relay` = anche chi non è amministratore ripete le ultime impostazioni ricevute (a un
    /// dispositivo appena arrivato, che altrimenti aspetterebbe l'amministratore).
    pub(super) fn group_send_info_as(&mut self, relay: bool) {
        self.info_sent = Self::now_secs();
        if !self.is_group_admin()
            && !(relay && self.groups.active().is_some_and(|g| !g.admin.is_empty() && g.info_at > 0))
        {
            return;
        }
        let Some(g) = self.groups.active() else { return };
        let msg = prep_core::group::GroupMsg::Info {
            admin: g.admin.clone(),
            name: g.name.clone(),
            photo: g.photo.clone(),
            removed: g.removed.clone(),
            at: g.info_at,
        };
        self.with_engine(Sess::Group, move |e, _| vec![e.group_msg(msg, false)]);
    }

    pub(super) fn group_msg_in(&mut self, from: String, msg: prep_core::group::GroupMsg) {
        use prep_core::group::{GroupMsg, InfoResult};
        let now = Self::now_secs();
        let me = self.user_id();
        match msg {
            GroupMsg::Info { admin, name, photo, removed, at } => {
                if !photo.is_empty() {
                    self.photo_src.insert(photo.clone(), from.clone());
                }
                let Some(g) = self.groups.active_mut() else { return };
                let gname = g.name.clone();
                let code = g.code.clone();
                match g.apply_info(&me, &admin, &name, &photo, &removed, at) {
                    InfoResult::Ignored => {}
                    InfoResult::Applied => {
                        let _ = self.groups.save(&self.store.base);
                    }
                    InfoResult::Removed => {
                        self.groups.leave(&code);
                        let _ = self.groups.save(&self.store.base);
                        self.group_connect();
                        self.toast(format!("L'amministratore ti ha tolto dal gruppo «{gname}»"));
                    }
                }
            }
            GroupMsg::PhotoGet { hash } => {
                // la manda chi ce l'ha, direttamente a chi la chiede (al massimo una volta ogni
                // 3 s per foto e richiedente)
                let key = format!("{hash}>{from}");
                if self.photo_sent.get(&key).is_some_and(|t| now - t < 3) {
                    return;
                }
                if let Some(bytes) = crate::photo::load(&self.store.base, &hash) {
                    self.photo_sent.insert(key, now);
                    let h = hash.clone();
                    self.with_engine(Sess::Group, move |e, _| vec![e.group_photo(Some(&from), &h, bytes)]);
                }
            }
            GroupMsg::Photo { hash, data } => {
                use base64::Engine as _;
                let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) else { return };
                self.group_photo_in(from, hash, bytes);
            }
            GroupMsg::Ping { id: _, from, name, to, at, pit: _ } => {
                if from == me || !(to.is_empty() || to == me) || now - at > p2p::STORED_TTL {
                    return;
                }
                let Some(g) = self.groups.active_mut() else { return };
                if at <= g.ping_seen || g.removed.contains(&from) {
                    return;
                }
                g.ping_seen = at;
                let gname = g.name.clone();
                let _ = self.groups.save(&self.store.base);
                let late = now - at > 120;
                let text = match self.study_state {
                    prep_core::group::State::Studying => return,
                    prep_core::group::State::Pause => format!("{name} ti richiama dai box: torna in pista!"),
                    prep_core::group::State::Idle => format!("{name} ti chiama in pista: è ora di studiare!"),
                };
                let when = if late {
                    chrono::DateTime::from_timestamp(at, 0)
                        .map(|d| format!(" (alle {})", d.with_timezone(&Local).format("%H:%M")))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                crate::notify_now(&format!("Studio di gruppo · {gname}\n{text}{when}"));
                self.toast(format!("{text}{when}"));
            }
        }
    }

    /// Foto ricevuta: solo se è quella annunciata (hash) e una vera foto piccola.
    pub(super) fn group_photo_in(&mut self, _from: String, hash: String, bytes: Vec<u8>) {
        if prep_core::group::photo_hash(&bytes) != hash || !crate::photo::valid_received(&bytes) {
            return;
        }
        if crate::photo::save(&self.store.base, &bytes).is_ok() {
            self.photo_asked.remove(&hash);
            self.refresh_profile_photo();
            self.refresh_group();
        }
    }

    /// Immagine di una foto (se c'è); se manca la si chiede al gruppo, al massimo ogni minuto.
    pub(super) fn photo_image(&mut self, hash: &str) -> Option<slint::Image> {
        if hash.is_empty() {
            return None;
        }
        if let Some(img) = self.photos.get(hash) {
            return Some(img.clone());
        }
        match crate::photo::load(&self.store.base, hash).and_then(|b| crate::photo::to_image(&b)) {
            Some(img) => {
                self.photos.insert(hash.to_string(), img.clone());
                Some(img)
            }
            None => {
                let now = Self::now_secs();
                if self.group.is_some() && self.photo_asked.get(hash).is_none_or(|t| now - t >= 15) {
                    self.photo_asked.insert(hash.to_string(), now);
                    // a chi l'ha annunciata (collegamento diretto se c'è), altrimenti a tutti
                    let to = self.photo_src.get(hash).cloned();
                    let msg = prep_core::group::GroupMsg::PhotoGet { hash: hash.to_string() };
                    self.with_engine(Sess::Group, move |e, _| vec![e.group_msg_to(to.as_deref(), msg)]);
                }
                None
            }
        }
    }

    /// Avvisa un membro (o tutti, con `to` vuoto) di venire a studiare. Arriva subito a chi è
    /// collegato; gli altri lo ricevono quando aprono l'app (entro 24 ore).
    pub(super) fn group_ping(&mut self, to: &str) {
        let now = Self::now_secs();
        let key = if to.is_empty() { "*".to_string() } else { to.to_string() };
        if self.ping_sent.get(&key).is_some_and(|t| now - t < 60) {
            self.toast("Avviso già mandato: riprova tra un minuto");
            return;
        }
        if self.group.is_none() {
            self.toast("Nessun gruppo collegato");
            return;
        }
        self.ping_sent.insert(key, now);
        let msg = prep_core::group::GroupMsg::Ping {
            id: p2p::random_id(),
            from: self.user_id(),
            name: self.display_name(),
            to: to.to_string(),
            at: now,
            pit: self.study_state == prep_core::group::State::Pause,
        };
        self.with_engine(Sess::Group, move |e, _| vec![e.group_msg(msg, true)]);
        let who = if to.is_empty() {
            "a tutto il gruppo".to_string()
        } else {
            self.groups
                .active()
                .and_then(|g| g.members.get(to))
                .map(|m| format!("a {}", m.status.name))
                .unwrap_or_else(|| "al membro".to_string())
        };
        self.toast(format!("Avviso mandato {who}: arriva subito a chi è collegato, agli altri quando aprono l'app"));
    }

    /// Rinomina il gruppo attivo: stesso codice, stessi membri e dati; il nuovo nome arriva a
    /// tutti con le impostazioni dell'amministratore. Nei gruppi senza amministratore noto
    /// (creati con versioni precedenti) chi rinomina per primo lo diventa.
    pub(super) fn group_rename(&mut self, name: &str) {
        let name: String = name.trim().chars().take(60).collect();
        if name.is_empty() {
            self.toast("Scrivi il nuovo nome del gruppo");
            return;
        }
        let me = self.user_id();
        let now = Self::now_secs();
        if !self.can_admin_group() {
            self.toast("Solo l'amministratore può rinominare il gruppo");
            return;
        }
        self.claim_legacy_admin();
        let _ = me;
        let Some(g) = self.groups.active_mut() else { return };
        g.name = name.clone();
        g.info_at = now.max(g.info_at + 1);
        let _ = self.groups.save(&self.store.base);
        self.group_send_info();
        self.toast(format!("Gruppo rinominato in «{name}»"));
        self.refresh_group();
    }

    /// L'amministratore toglie un membro dal gruppo (tocco di conferma).
    pub(super) fn group_kick(&mut self, uid: &str) {
        if !self.is_group_admin() {
            self.toast("Solo l'amministratore del gruppo può rimuovere i membri");
            return;
        }
        let armed = matches!(&self.kick_armed, Some((u, t)) if u == uid && t.elapsed() < Duration::from_secs(5));
        if !armed {
            self.kick_armed = Some((uid.to_string(), Instant::now()));
            self.toast("Tocca di nuovo «Rimuovi» per confermare");
            return;
        }
        self.kick_armed = None;
        let now = Self::now_secs();
        if let Some(g) = self.groups.active_mut() {
            if !g.removed.iter().any(|r| r == uid) {
                g.removed.push(uid.to_string());
            }
            g.members.remove(uid);
            g.info_at = now.max(g.info_at + 1);
        }
        let _ = self.groups.save(&self.store.base);
        self.group_send_info();
        self.ui().global::<AppState>().set_member_sel(ss(""));
        self.toast("Membro rimosso dal gruppo");
        self.refresh_group();
    }

    /// Fa scegliere una foto: profilo («user») o del gruppo («group», solo l'amministratore).
    pub(super) fn pick_photo(&mut self, kind: &str) {
        if kind == "group" && !self.can_admin_group() {
            self.toast("Solo l'amministratore può cambiare la foto del gruppo");
            return;
        }
        #[cfg(target_os = "android")]
        if let Err(e) = crate::android_pick_photo(kind) {
            self.toast(format!("Selettore immagini non disponibile: {e}"));
        }
        #[cfg(target_arch = "wasm32")]
        crate::web::pick_photo(self.store.base.join("import"), if kind == "group" { "group" } else { "user" });
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        match newest_image_download() {
            Some(p) => match vfs::read(&p) {
                Ok(bytes) => self.photo_imported(kind, &bytes),
                Err(e) => self.toast(format!("Errore: {e}")),
            },
            None => self.toast("Nessuna immagine in Immagini o Download: salvane una lì e riprova"),
        }
    }

    /// Foto scelta: ritagliata, ridotta, salvata e annunciata al gruppo.
    pub(super) fn photo_imported(&mut self, kind: &str, bytes: &[u8]) {
        let jpeg = match crate::photo::prepare(bytes) {
            Ok(j) => j,
            Err(e) => {
                self.toast(e);
                return;
            }
        };
        let hash = match crate::photo::save(&self.store.base, &jpeg) {
            Ok(h) => h,
            Err(e) => {
                self.toast(format!("Errore: {e}"));
                return;
            }
        };
        if kind == "group" {
            if !self.can_admin_group() {
                return;
            }
            self.claim_legacy_admin();
            let now = Self::now_secs();
            if let Some(g) = self.groups.active_mut() {
                g.photo = hash.clone();
                g.info_at = now.max(g.info_at + 1);
            }
            let _ = self.groups.save(&self.store.base);
            self.group_send_info();
            self.toast("Foto del gruppo aggiornata");
        } else {
            let mut s = self.store.settings();
            s.photo = hash.clone();
            let _ = self.store.save_settings(&s);
            self.group_send();
            self.toast("Foto profilo aggiornata");
        }
        // gli altri vedono il nuovo hash nello stato (o nelle impostazioni del gruppo) e la
        // chiedono subito, sul collegamento diretto quando c'è
        let _ = jpeg;
        self.refresh_profile_photo();
        self.refresh_group();
    }

    pub(super) fn remove_photo(&mut self, kind: &str) {
        if kind == "group" {
            if !self.is_group_admin() {
                return;
            }
            let now = Self::now_secs();
            if let Some(g) = self.groups.active_mut() {
                g.photo.clear();
                g.info_at = now.max(g.info_at + 1);
            }
            let _ = self.groups.save(&self.store.base);
            self.group_send_info();
        } else {
            let mut s = self.store.settings();
            s.photo.clear();
            let _ = self.store.save_settings(&s);
            self.group_send();
        }
        self.refresh_profile_photo();
        self.refresh_group();
    }

    pub(super) fn refresh_profile_photo(&mut self) {
        let hash = self.store.settings().photo;
        let img = self.photo_image(&hash);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_my_has_photo(img.is_some());
        st.set_my_photo(img.unwrap_or_default());
        st.set_my_initial(ss(self
            .display_name()
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()));
    }

    pub(super) fn refresh_group(&mut self) {
        use prep_core::group::{fmt_secs, live, standings, value, Period, State, LAP_SECS};
        let now = Self::now_secs();
        let today = Local::now().date_naive();
        let me_dev = self.device_id();
        let me_id = self.user_id();
        // io: questo dispositivo più gli altri miei (stesso id utente) visti nel gruppo
        let mine = {
            let here = self.my_group_status(now);
            let mut devs = vec![(here.clone(), true)];
            if let Some(m) = self.groups.active().and_then(|g| g.members.get(&me_id)) {
                for (d, ds) in &m.devices {
                    if *d != me_dev {
                        devs.push(prep_core::group::frozen(&ds.status, ds.last_seen, now));
                    }
                }
            }
            let (mut c, _) = prep_core::group::combine(&devs, now, today);
            // nome, colore e attività sono quelli di questo dispositivo
            c.name = here.name;
            c.color = here.color;
            c.photo = here.photo;
            if self.study_state != State::Idle {
                c.state = self.study_state;
                c.pit_since = here.pit_since;
                c.doing = here.doing;
            }
            c
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let (t, _, _) = live(&mine, now, today);
        let laps = t / LAP_SECS;
        if laps > self.my_laps && self.study_state == State::Studying && self.my_laps > 0 {
            if laps.is_multiple_of(10) {
                self.toast(format!("{laps} giri: {} di studio oggi!", fmt_secs(t)));
            }
            st.set_lap_flash(st.get_lap_flash() + 1);
        }
        self.my_laps = laps;
        st.set_study_state(match self.study_state {
            State::Idle => 0,
            State::Studying => 1,
            State::Pause => 2,
        });
        st.set_study_today(ss(format!("Oggi {} · {} giri", fmt_secs(t), laps)));
        st.set_study_lap(((t % LAP_SECS) as f32) / LAP_SECS as f32);
        st.set_groups(model(
            self.groups
                .groups
                .iter()
                .map(|g| GroupVM { id: ss(&g.code), name: ss(&g.name), active: g.code == self.groups.active })
                .collect::<Vec<_>>(),
        ));
        let solo = prep_core::group::Group::default();
        let has = self.groups.active().is_some();
        let g = self.groups.active().unwrap_or(&solo);
        st.set_group_has(has);
        if !has {
            st.set_ranking(model(Vec::<RankVM>::new()));
        }
        st.set_group_has(true);
        st.set_group_name(ss(&g.name));
        st.set_group_id(ss(&g.code));
        let group_admin = g.admin.clone();
        let group_can_admin = g.can_admin(&me_id);
        let group_photo = g.photo.clone();
        let group_code = g.code.clone();
        // membri: io (sempre) e gli altri visti, ognuno con i suoi dispositivi sommati;
        // (id utente, stato, collegato, ultima volta visto)
        let mut all: Vec<(String, prep_core::group::Status, bool, i64)> = Vec::new();
        for (id, m) in &g.members {
            if *id != me_id && *id != me_dev && !g.removed.contains(id) {
                let (c, on) = prep_core::group::combine(&m.device_list(now), now, today);
                all.push((id.clone(), c, on, m.last_seen));
            }
        }
        // ordine stabile (la propria macchina per ultima, sopra le altre)
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all.push((me_id.clone(), mine.clone(), true, now));
        let on_track = all.iter().filter(|(_, s, on, _)| *on && s.state == State::Studying).count();
        // circuito del gruppo: cambia (e cresce) con il numero dei membri
        let ci = prep_core::group::circuit_for(&g.code, all.len());
        let circuit = prep_core::group::circuit(ci);
        if self.circuit_shown != Some(ci) {
            self.circuit_shown = Some(ci);
            st.set_track_cmd(ss(&circuit.commands));
            st.set_track_aspect(circuit.aspect as f32);
            st.set_pad_x(circuit.paddock.0 as f32);
            st.set_pad_y(circuit.paddock.1 as f32);
        }
        st.set_track_scale(prep_core::group::circuit_scale(all.len()) as f32);
        let parked_n = all.iter().filter(|(_, s, on, _)| !*on || s.state == State::Idle).count();
        let mut parked_i = 0usize;
        let mut lane_i = 0usize;
        let mut cars: Vec<CarVM> = Vec::new();
        let frac = (Utc::now().timestamp_subsec_millis() as f64 / 1000.0).clamp(0.0, 0.999);
        let empty_crew: ModelRc<CrewVM> = ModelRc::default();
        // soste finite: via dalla cache
        let pitting: BTreeSet<(String, i64)> = all
            .iter()
            .filter(|(_, s, on, _)| *on && s.state == State::Pause)
            .map(|(id, s, _, _)| (id.clone(), s.pit_since))
            .collect();
        self.crews.retain(|k, _| pitting.contains(k));
        for (id, s, on, _) in &all {
            let me = *id == me_id;
            let racing = *on && s.state == State::Studying;
            let pit = *on && s.state == State::Pause;
            let parked = !racing && !pit;
            let (px, py) = if parked {
                let (cx, cy) = circuit.paddock;
                let x = cx - 0.15 + 0.3 * (parked_i as f64 + 0.5) / parked_n.max(1) as f64;
                parked_i += 1;
                (x as f32, (cy + 0.02) as f32)
            } else {
                (0.0, 0.0)
            };
            // corsie: la propria al centro, le altre alternate ai lati
            let side = 1.0;
            let lane = if pit {
                0.0
            } else if racing && !me {
                lane_i += 1;
                [-1.0, 1.0][(lane_i - 1) % 2]
            } else {
                0.0
            };
            let initial = s.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            // la stessa squadra per tutta la sosta (stesso modello: niente ripartenze)
            let crew = if pit {
                let key = (id.clone(), s.pit_since);
                self.crews
                    .entry(key)
                    .or_insert_with(|| {
                        let seed = p2p::sha256_hex(format!("{id}/{}", s.pit_since).as_bytes());
                        let seed = u64::from_str_radix(&seed[..16], 16).unwrap_or(1);
                        model(
                            prep_core::group::pit_crew(seed)
                                .into_iter()
                                .map(|m| CrewVM {
                                    x: m.x as f32,
                                    y: m.y as f32,
                                    rot: m.rot as f32,
                                    tool: m.tool,
                                    tire: m.tire,
                                    sign: m.sign,
                                    alt: m.alt,
                                })
                                .collect(),
                        )
                    })
                    .clone()
            } else {
                empty_crew.clone()
            };
            // posizione con le frazioni di secondo: in pista avanza in modo continuo
            let secs = live(s, now, today).0 as f64 + if racing { frac } else { 0.0 };
            cars.push(CarVM {
                id: ss(id),
                crew,
                f: (secs / LAP_SECS as f64) as f32,
                v: if racing { 1.0 / LAP_SECS as f32 } else { 0.0 },
                side: side as f32,
                stack: 0.0,
                run: 0.0,
                color: slint::Color::from_argb_encoded(s.color),
                label: ss(initial),
                me,
                racing,
                pit,
                parked,
                offline: !*on,
                lane: lane as f32,
                px,
                py,
            });
        }
        // telecamera sulla propria macchina mentre si studia; altrimenti tutto il circuito
        st.set_race_overview(self.study_state != State::Studying);
        let my_secs = live(&mine, now, today).0 as f64 + if self.study_state == State::Studying { frac } else { 0.0 };
        st.set_cam_f((my_secs / LAP_SECS as f64) as f32);
        st.set_lap_speed(1.0 / LAP_SECS as f32);
        // piazzole dei pit stop: fuori dalla strada e senza sovrapporsi, anche quando più
        // macchine vanno in pausa nello stesso punto (calcolo ricordato finché le soste non
        // cambiano)
        let stops: Vec<f64> = cars.iter().filter(|c| c.pit).map(|c| c.f as f64).collect();
        if !stops.is_empty() {
            let key = (ci, all.len(), stops.iter().map(|f| (f * 10000.0) as i64).collect::<Vec<_>>());
            if self.pit_cache.as_ref().is_none_or(|(k, _)| *k != key) {
                self.pit_cache = Some((key, prep_core::group::pit_places(ci, all.len(), &stops)));
            }
            if let Some((_, places)) = &self.pit_cache {
                for (c, (side, level)) in cars.iter_mut().filter(|c| c.pit).zip(places) {
                    c.side = *side as f32;
                    c.stack = *level as f32;
                    c.run = prep_core::group::pit_run(ci, all.len(), *level) as f32;
                }
            }
        }
        if let Some(me) = cars.iter().find(|c| c.me) {
            st.set_cam_side(me.side);
            st.set_cam_stack(me.stack);
            st.set_cam_run(me.run);
        }
        // scheda della macchina toccata
        match all.iter().find(|(id, _, _, _)| *id == self.car_sel) {
            Some((id, s, on, seen)) => {
                let (t_day, t_week, _) = live(s, now, today);
                let short = |x: u64| if x < 60 { format!("{x} s") } else { fmt_secs(x) };
                let mut lines = Vec::new();
                lines.push(match (*on, s.state) {
                    (false, _) => format!(
                        "Non collegato · visto l'ultima volta {}",
                        chrono::DateTime::from_timestamp(*seen, 0)
                            .map(|d| d.with_timezone(&Local).format("%d/%m alle %H:%M").to_string())
                            .unwrap_or_else(|| "—".into())
                    ),
                    (true, State::Studying) => format!("In pista · giro {}", t_day / LAP_SECS + 1),
                    (true, State::Pause) if s.pit_since > 0 => {
                        format!("Pit stop da {}", short((now - s.pit_since).max(0) as u64))
                    }
                    (true, State::Pause) => "Pit stop".to_string(),
                    (true, State::Idle) => "Sessione terminata: al paddock fino alla prossima".to_string(),
                });
                lines.push(format!("Studio: oggi {} · settimana {}", fmt_secs(t_day), fmt_secs(t_week)));
                let same_day = s.day == today.format("%Y-%m-%d").to_string();
                let pits: Vec<u64> = if same_day { s.pits.clone() } else { Vec::new() };
                if pits.is_empty() {
                    lines.push("Nessun pit stop finito oggi".to_string());
                } else {
                    let tot: u64 = pits.iter().sum();
                    let list: Vec<String> = pits.iter().rev().take(6).map(|x| short(*x)).collect();
                    lines.push(format!(
                        "Pit stop oggi: {} ({} in tutto) · ultimi: {}",
                        pits.len(),
                        short(tot),
                        list.join(", ")
                    ));
                }
                if !s.doing.is_empty() {
                    lines.push(format!("Sta studiando: {}", s.doing));
                }
                let me = *id == me_id;
                let photo = self.photo_image(&s.photo.clone());
                st.set_car_info_has_photo(photo.is_some());
                st.set_car_info_photo(photo.unwrap_or_default());
                st.set_car_info_color(slint::Color::from_argb_encoded(s.color));
                st.set_car_info_initial(ss(s
                    .name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default()));
                st.set_car_info_title(ss(if me { format!("{} (tu)", s.name) } else { s.name.clone() }));
                st.set_car_info_body(ss(lines.join("\n")));
                st.set_car_sel(ss(id));
            }
            None => {
                self.car_sel.clear();
                st.set_car_sel(ss(""));
            }
        }
        let p = self.group_period;
        let refs: Vec<(&str, &prep_core::group::Status)> = all.iter().map(|(i, s, _, _)| (i.as_str(), s)).collect();
        let order = standings(refs.into_iter(), now, today, p);
        let order: Vec<(String, prep_core::group::Status)> =
            order.into_iter().map(|(i, s)| (i.to_string(), s.clone())).collect();
        let photos: Vec<Option<slint::Image>> = order
            .iter()
            .map(|(_, s)| s.photo.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| self.photo_image(&h))
            .collect();
        let ranking: Vec<RankVM> = order
            .iter()
            .zip(photos)
            .enumerate()
            .map(|(i, ((id, s), photo))| {
                let v = value(s, now, today, p);
                let on = all.iter().find(|(x, _, _, _)| x == id).is_some_and(|(_, _, o, _)| *o);
                RankVM {
                    pos: i as i32 + 1,
                    id: ss(id),
                    has_photo: photo.is_some(),
                    photo: photo.unwrap_or_default(),
                    initial: ss(s.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()),
                    admin: *id == group_admin,
                    name: ss(if *id == me_id { format!("{} (tu)", s.name) } else { s.name.clone() }),
                    color: slint::Color::from_argb_encoded(s.color),
                    value: ss(fmt_secs(v)),
                    laps: (live(s, now, today).0 / LAP_SECS) as i32,
                    status: ss(match (on, s.state) {
                        (false, _) => "non collegato",
                        (true, State::Studying) => "in pista",
                        (true, State::Pause) => "pit stop",
                        (true, State::Idle) => "al paddock",
                    }),
                    me: *id == me_id,
                }
            })
            .collect();
        let my_pos = order.iter().position(|(id, _)| *id == me_id).map(|i| i + 1).unwrap_or(1);
        // intestazione del gruppo: codice, amministratore, foto
        let gphoto = self.photo_image(&group_photo);
        st.set_group_has_photo(gphoto.is_some());
        st.set_group_photo(gphoto.unwrap_or_default());
        st.set_group_admin(group_can_admin);
        st.set_group_can_rename(group_can_admin);
        st.set_group_admin_name(ss(if group_admin.is_empty() {
            String::new()
        } else if group_admin == me_id {
            "tu".to_string()
        } else {
            all.iter().find(|(i, _, _, _)| *i == group_admin).map(|(_, s, _, _)| s.name.clone()).unwrap_or_default()
        }));
        st.set_group_code(ss(&group_code));
        st.set_group_initial(ss(st
            .get_group_name()
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()));
        st.set_race_hud(ss(match self.study_state {
            State::Idle => format!("Circuito {} · premi «Inizia a studiare»", circuit.name),
            State::Pause => format!("Pit stop · giro {} · {}° su {}", laps + 1, my_pos, all.len()),
            State::Studying => format!("Giro {} · {}° su {} · {}", laps + 1, my_pos, all.len(), fmt_secs(t)),
        }));
        let others_online = all.iter().filter(|(i, _, on, _)| *on && *i != me_id).count();
        let link = self.group.as_ref().map(|p| p.open.iter().any(|o| *o)).unwrap_or(false);
        st.set_group_info(ss(format!(
            "{} membri · {} in pista{}",
            all.len(),
            on_track,
            if !link {
                " · collegamento in corso…".to_string()
            } else if others_online == 0 {
                " · nessun altro collegato ora".to_string()
            } else if others_online == 1 {
                " · 1 collegato".to_string()
            } else {
                format!(" · {others_online} collegati")
            }
        )));
        let _ = Period::Today;
        // aggiornamento riga per riga del modello persistente
        let n_cars = cars.len();
        let m = &self.cars_model;
        for (i, c) in cars.into_iter().enumerate() {
            if i < m.row_count() {
                if m.row_data(i).as_ref() != Some(&c) {
                    m.set_row_data(i, c);
                }
            } else {
                m.push(c);
            }
        }
        while m.row_count() > n_cars {
            m.remove(m.row_count() - 1);
        }
        st.set_ranking(model(ranking));
    }

    pub(super) fn show_legal_if_needed(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_legal_text(ss(LEGAL_TEXT));
        let required = self.store.settings().legal_accepted < LEGAL_VERSION;
        st.set_legal_required(required);
        if required {
            st.set_legal_visible(true);
        }
    }

    pub(super) fn legal_accept(&mut self) {
        let mut s = self.store.settings();
        if s.legal_accepted < LEGAL_VERSION {
            s.legal_accepted = LEGAL_VERSION;
            let _ = self.store.save_settings(&s);
        }
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_legal_required(false);
        st.set_legal_visible(false);
    }

    pub(super) fn save_username(&mut self, name: &str) {
        let mut s = self.store.settings();
        s.username = name.trim().chars().take(32).collect();
        match self.store.save_settings(&s) {
            Ok(()) => {
                self.ui().global::<AppState>().set_s_username(ss(&s.username));
                self.refresh_profile_photo();
            }
            Err(e) => self.toast(format!("Errore: {e}")),
        }
    }

    pub(super) fn poll_import(&mut self) {
        if self.importing {
            return;
        }
        let files = vfs::list_dir(&self.store.base.join("import"));
        // foto scelte con il selettore (profilo o gruppo)
        if let Some(ph) = files.iter().find(|p| p.extension().is_some_and(|x| x == "photo")).cloned() {
            let bytes = vfs::read(&ph).unwrap_or_default();
            let _ = vfs::remove_file(&ph);
            let kind =
                if ph.file_name().is_some_and(|n| n.to_string_lossy().starts_with("group")) { "group" } else { "user" };
            if !bytes.is_empty() {
                self.photo_imported(kind, &bytes);
            }
            return;
        }
        // inviti a un gruppo aperti con l'app (Android li lascia qui come file .invite)
        if let Some(inv) = files.iter().find(|p| p.extension().is_some_and(|x| x == "invite")) {
            let text = vfs::read_to_string(inv).unwrap_or_default();
            let _ = vfs::remove_file(inv);
            self.group_join(&text);
            self.go_tab(4);
            return;
        }
        let next = files.into_iter().find(|p| !vfs::is_dir(p) && p.extension().is_none_or(|x| x != "tmp"));
        if let Some(p) = next {
            self.import_pack_file(p, true);
        }
    }

    pub(super) fn pick_pack(&mut self) {
        #[cfg(target_os = "android")]
        if let Err(e) = crate::android_pick_pack() {
            self.toast(format!("Selettore file non disponibile: {e}"));
        }
        #[cfg(target_arch = "wasm32")]
        crate::web::pick_pack_file(self.store.base.join("import"));
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        match newest_pack_download() {
            Some(p) => self.import_pack_file(p, false),
            None => self.toast("Nessun pacchetto (.json.gz) nella cartella Download: mettilo lì e riprova"),
        }
    }
}
