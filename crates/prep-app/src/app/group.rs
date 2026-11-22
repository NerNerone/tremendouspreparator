//! Studio di gruppo: gara, classifica, amministrazione, foto e avvisi.

use super::*;

/// Dimensione massima (JSON) di un messaggio di schede: sta comodo in un evento dei relay.
const CARDS_BYTES: usize = 20_000;

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

    /// Stato da mandare al gruppo; con `hist` anche la mappa di calore (si manda ogni tanto).
    pub(super) fn my_group_status(&mut self, now: i64, hist: bool) -> prep_core::group::Status {
        let color = self.my_color();
        let name = self.display_name();
        let mut s = prep_core::group::my_status(
            &name,
            color,
            self.study_state,
            // solo lo studio di questo dispositivo: il gruppo somma quelli di ogni dispositivo
            &self.study_log,
            self.study_since,
            &prep_core::group::Clock::local(now),
            hist,
            self.score_now(now),
        );
        s.pit_since = self.pit_since.unwrap_or(0);
        s.doing = self.doing();
        s.uid = self.user_id();
        s.dev = self.device_id();
        s.since = if self.study_state == prep_core::group::State::Idle { 0 } else { self.session_start };
        let set = self.store.settings();
        s.photo = set.photo;
        s.prev = set.prev_ids;
        if hist {
            s.prof = Some(self.my_prof());
        }
        s
    }

    /// Numeri del proprio profilo sull'esame attivo: quiz, simulazioni, flashcard, obiettivi.
    pub(super) fn my_prof(&self) -> prep_core::group::Prof {
        use prep_core::session::SessionKind;
        let today = Local::now().date_naive();
        let mut p = prep_core::group::Prof {
            exam: self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_default(),
            ..Default::default()
        };
        for s in &self.sessions {
            let on_today = s.started_at.with_timezone(&Local).date_naive() == today;
            match s.kind {
                SessionKind::Practice | SessionKind::Exam | SessionKind::Review => {
                    let n = s.items.iter().filter(|i| i.given.is_some()).count() as u32;
                    let ok = s.items.iter().filter(|i| i.correct == Some(true)).count() as u32;
                    p.q_all += n;
                    p.q_all_ok += ok;
                    if on_today {
                        p.q_today += n;
                        p.q_today_ok += ok;
                    }
                    if s.kind == SessionKind::Exam {
                        p.sims += 1;
                    }
                }
                SessionKind::Flashcards => {
                    let n = s.items.iter().filter(|i| i.grade.is_some()).count() as u32;
                    p.cards_all += n;
                    if on_today {
                        p.cards_today += n;
                    }
                }
                SessionKind::Flags => {}
            }
        }
        if let Some(g) = self.today_goals() {
            let done = prep_core::plan::done_on(&self.content, &self.history, today);
            p.goals_total = g.topics.len() as u32;
            p.goals_done =
                g.topics.iter().zip(&g.targets).filter(|(t, n)| done.get(*t).copied().unwrap_or(0) >= **n).count()
                    as u32;
        }
        p
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
        let derived = if let Some(seed) = p2p::Room::decode_seed(&s.p2p_room_key) {
            p2p::Room::user_id_from_seed(&seed)
        } else if !s.p2p_password.is_empty() {
            prep_core::group::user_id(&s.username, &s.p2p_password, &dev)
        } else {
            dev
        };
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
            let now = Self::now_secs();
            for g in &mut self.groups.groups {
                if g.admin == derived && !g.admin_sk.is_empty() {
                    g.info_at = now.max(g.info_at + 1);
                    g.sign_current_info();
                }
            }
            let _ = self.groups.save(&self.store.base);
            self.uid_cache = None;
            // gli altri lo sanno dal prossimo stato e dalle nuove impostazioni firmate
            self.group_send();
            self.group_send_info();
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
        if self.last_doing.is_empty() {
            // nessuna attività nell'app da quando si studia (libro di carta, appunti…): l'esame
            return self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_default();
        }
        self.last_doing.clone()
    }

    /// Porta nel registro il tratto di studio in corso (alla pausa, alla fine, ogni tanto).
    pub(super) fn study_flush(&mut self, now: i64) {
        if let Some(t) = self.study_since {
            // diviso alle mezzanotti: un tratto dalle 23 all'una conta un'ora per giorno
            let start = t.clamp(now - 6 * 3600, now);
            if now > start {
                let what = self.doing();
                let score = self.score_now(now);
                self.study_log.add_span(start, now, &what, &Local);
                self.study_log.add_points(start, now, score.mult, &Local);
                let _ = self.study_log.save(&self.store.base);
            }
            self.study_since = if self.study_state == prep_core::group::State::Studying { Some(now) } else { None };
        }
    }

    pub(super) fn group_send(&mut self) {
        let in_group = self.group.as_ref().is_some_and(|p| p.started);
        let synced = self.p2p.as_ref().is_some_and(|p| p.started);
        if !in_group && !synced {
            return;
        }
        let now = Self::now_secs();
        self.group_last_send = now;
        // ai propri altri dispositivi basta lo stato (una sola sessione di studio alla volta)
        if synced {
            let mut light = self.my_group_status(now, false);
            light.hist = None;
            light.prof = None;
            self.with_engine(Sess::Sync, move |e, _| vec![e.group_status(light)]);
        }
        if !in_group {
            return;
        }
        // la mappa di calore viaggia al più una volta al minuto
        let hist = now - self.hist_sent >= 60;
        if hist {
            self.hist_sent = now;
        }
        let status = self.my_group_status(now, hist);
        self.with_engine(Sess::Group, move |e, _| vec![e.group_status(status)]);
    }

    pub(super) fn group_status_in(&mut self, sess: Sess, from: String, status: prep_core::group::Status) {
        let now = Self::now_secs();
        let mut fresh = false;
        if !status.photo.is_empty() {
            self.photo_src.insert(status.photo.clone(), from.clone());
        }
        // stato di un altro proprio dispositivo: una sola sessione di studio alla volta
        if !status.uid.is_empty() && status.uid == self.user_id() && from != self.device_id() {
            self.twin = Some((from.clone(), status.state, status.since, now));
            self.own_devs.insert(from.clone(), (status.clone(), now));
            self.twin_resolve(now);
        }
        if sess == Sess::Sync {
            return;
        }
        if let Some(g) = self.groups.active_mut() {
            let uid = if status.uid.is_empty() { from.clone() } else { status.uid.clone() };
            // dispositivo mai sentito (nuovo membro o riavvio): gli si ripetono subito le cose
            fresh = g.members.get(&uid).is_none_or(|m| !m.devices.contains_key(&from) || now - m.last_seen > 60);
            g.status_in(&from, status, now);
            let _ = self.groups.save(&self.store.base);
        }
        if fresh {
            // l'amministratore manda le impostazioni del gruppo; tutti il proprio stato e le
            // schede di chi ora non c'è
            self.group_send_info_as(true);
            self.group_send_cards();
            if now - self.group_last_send >= 3 {
                self.group_send();
            }
        }
        self.refresh_group();
    }

    /// Un altro proprio dispositivo ha una sessione di studio aperta (notizia recente)?
    pub(super) fn twin_active(&self, now: i64) -> bool {
        self.twin.as_ref().is_some_and(|(_, st, _, at)| *st != prep_core::group::State::Idle && now - at < 35)
    }

    /// Due propri dispositivi in sessione insieme: resta la sessione iniziata prima (a parità,
    /// quella del dispositivo con l'id minore); questo, se è il secondo, si ferma.
    fn twin_resolve(&mut self, now: i64) {
        use prep_core::group::State;
        if self.study_state == State::Idle || !self.twin_active(now) {
            return;
        }
        let Some((dev, _, since, _)) = self.twin.clone() else { return };
        let mine = self.session_start;
        let i_yield = since > 0 && (mine > since || (mine == since && self.device_id() > dev));
        if i_yield {
            self.study_set(State::Idle);
            self.toast("La sessione di studio è già aperta su un altro tuo dispositivo: continua lì");
        }
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
        self.cards_sent = 0;
        self.card_stored.0 = 0;
        self.refresh_group();
    }

    pub(super) fn group_create(&mut self, name: &str) {
        let name =
            if name.trim().is_empty() { format!("Gruppo di {}", self.display_name()) } else { name.trim().to_string() };
        let code = prep_core::group::new_code();
        self.groups.join(&code, &name, Self::now_secs());
        let me = self.user_id();
        if let Some(g) = self.groups.active_mut() {
            // chi crea il gruppo ne è l'amministratore e firma subito le impostazioni iniziali
            g.admin = me;
            g.info_at = Self::now_secs();
            g.sign_current_info();
        }
        let _ = self.groups.save(&self.store.base);
        self.group_connect();
        self.group_send_info_stored();
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
            self.toast("Tocca il codice per copiarlo e mandalo a chi vuoi invitare");
        }
    }

    pub(super) fn study_set(&mut self, state: prep_core::group::State) {
        use prep_core::group::State;
        let now = Self::now_secs();
        // una sola sessione per account: se è aperta su un altro proprio dispositivo, si
        // continua lì
        if self.study_state == State::Idle && state != State::Idle && self.twin_active(now) {
            self.toast("Hai già una sessione di studio aperta su un altro tuo dispositivo: continua lì o terminala");
            return;
        }
        self.study_flush(now);
        let mut pit_len = None;
        if let Some(t) = self.pit_since.take() {
            let secs = (now - t).clamp(0, 12 * 3600) as u64;
            pit_len = Some(secs);
            if secs > 0 {
                self.study_log.add_pit(Local::now().date_naive(), secs);
                let _ = self.study_log.save(&self.store.base);
            }
        }
        if state == State::Pause {
            self.pit_since = Some(now);
        }
        // nuova sessione: l'attività riparte da zero (finché non si apre qualcosa, vale l'esame)
        if self.study_state == State::Idle && state == State::Studying {
            self.last_doing.clear();
            self.session_start = now;
        }
        if state == State::Idle {
            self.session_start = 0;
        }
        // tratto di studio e suoi effetti (vedi prep_core::score)
        match (self.study_state, state) {
            (State::Idle, State::Studying) => self.run.start(now),
            (State::Pause, State::Studying) => {
                let pit_start = now - pit_len.unwrap_or(0) as i64;
                self.run.resume(pit_start, now);
            }
            (State::Studying, State::Pause) => self.run.pause(now),
            (_, State::Idle) => self.run = Default::default(),
            _ => {}
        }
        self.study_state = state;
        self.study_since = (state == State::Studying).then_some(now);
        self.save_study_session(now);
        self.study_notified = (0, -1);
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
        // gruppi cambiati qui (creati, lasciati, rinominati, nuova foto): lo si dice subito agli
        // altri propri dispositivi
        // (anche il proprio studio: gli altri propri dispositivi sommano i tempi)
        let sig =
            serde_json::to_string(&(self.groups.refs(), &self.groups.left, self.study_log.total())).unwrap_or_default();
        if sig != self.groups_sig {
            let first = self.groups_sig.is_empty();
            self.groups_sig = sig;
            if !first && self.p2p.as_ref().is_some_and(|p| p.started) {
                self.with_engine(Sess::Sync, |e, c| e.reannounce(c));
            }
        }
        self.study_notify(now);
        if self.study_since.is_some_and(|t| now - t >= 60) {
            self.study_flush(now);
        }
        if (self.group.is_some() || self.p2p.is_some()) && now - self.group_last_send >= 10 {
            self.group_send();
        }
        if self.group.is_some() && now - self.info_sent >= 30 {
            self.group_send_info();
        }
        self.group_store_card(now);
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
                g.ensure_admin_keys();
            }
        }
    }

    /// L'amministratore ripete ogni tanto le impostazioni del gruppo (nome, foto, rimossi):
    /// così le ricevono anche i nuovi arrivati.
    pub(super) fn group_send_info(&mut self) {
        self.group_send_info_as(false);
    }

    /// Impostazioni del gruppo da conservare sui relay (24 ore): arrivano anche a chi ora è
    /// offline, compreso chi è stato rimosso.
    fn group_send_info_stored(&mut self) {
        if self.is_group_admin() {
            if let Some(g) = self.groups.active_mut() {
                if !g.admin_sk.is_empty() && (g.admin_pk.is_empty() || g.admin_sig.is_empty()) {
                    g.sign_current_info();
                    let _ = self.groups.save(&self.store.base);
                }
            }
        }
        let Some(g) = self.groups.active() else { return };
        if g.admin_pk.is_empty() || g.admin_sig.is_empty() {
            return;
        }
        let msg = prep_core::group::GroupMsg::Info {
            admin: g.admin.clone(),
            name: g.name.clone(),
            photo: g.photo.clone(),
            removed: g.removed.clone(),
            at: g.info_at,
            pk: g.admin_pk.clone(),
            sig: g.admin_sig.clone(),
        };
        self.with_engine(Sess::Group, move |e, _| vec![e.group_msg(msg, true)]);
    }

    /// `relay` = anche chi non è amministratore ripete le ultime impostazioni ricevute (a un
    /// dispositivo appena arrivato, che altrimenti aspetterebbe l'amministratore).
    pub(super) fn group_send_info_as(&mut self, relay: bool) {
        self.info_sent = Self::now_secs();
        let is_admin = self.is_group_admin();
        if is_admin {
            if let Some(g) = self.groups.active_mut() {
                if !g.admin_sk.is_empty() && (g.admin_pk.is_empty() || g.admin_sig.is_empty()) {
                    g.sign_current_info();
                    let _ = self.groups.save(&self.store.base);
                }
            }
        }
        let Some(g) = self.groups.active() else { return };
        let signed = !g.admin.is_empty() && !g.admin_pk.is_empty() && !g.admin_sig.is_empty() && g.info_at > 0;
        if !(is_admin && signed) && !(relay && signed) {
            return;
        }
        let msg = prep_core::group::GroupMsg::Info {
            admin: g.admin.clone(),
            name: g.name.clone(),
            photo: g.photo.clone(),
            removed: g.removed.clone(),
            at: g.info_at,
            pk: g.admin_pk.clone(),
            sig: g.admin_sig.clone(),
        };
        self.with_engine(Sess::Group, move |e, _| vec![e.group_msg(msg, false)]);
    }

    /// Ripete al gruppo le schede di chi ora non è collegato (tempi, punti, stato, foto,
    /// mappa di calore): così le ha anche chi non l'ha mai incontrato. Al massimo ogni 20 s.
    pub(super) fn group_send_cards(&mut self) {
        let now = Self::now_secs();
        if self.group.is_none() || now - self.cards_sent < 20 {
            return;
        }
        let me = self.device_id();
        let Some(g) = self.groups.active() else { return };
        let cards = g.cards(now, &me);
        if cards.is_empty() {
            return;
        }
        self.cards_sent = now;
        let msgs: Vec<_> = prep_core::group::card_chunks(cards, CARDS_BYTES)
            .into_iter()
            .map(|cards| prep_core::group::GroupMsg::Cards { cards })
            .collect();
        self.with_engine(Sess::Group, move |e, _| msgs.into_iter().map(|m| e.group_msg(m, false)).collect());
    }

    /// Lascia sui relay la propria scheda (una sola per dispositivo, la più recente, che scade
    /// dopo 24 ore): chi apre l'app quando questo dispositivo è spento la trova lo stesso.
    /// Ogni 10 minuti, o un minuto dopo essere entrati o usciti dalla pista.
    fn group_store_card(&mut self, now: i64) {
        if !self.group.as_ref().is_some_and(|p| p.started) {
            return;
        }
        let (at, state) = self.card_stored;
        let due = now - at >= 600 || (state != self.study_state && now - at >= 60);
        if !due {
            return;
        }
        self.card_stored = (now, self.study_state);
        let status = self.my_group_status(now, true);
        let card = prep_core::group::Card { dev: self.device_id(), seen: now, status };
        let msg = prep_core::group::GroupMsg::Cards { cards: vec![card] };
        self.with_engine(Sess::Group, move |e, _| vec![e.group_msg(msg, true)]);
    }

    pub(super) fn group_msg_in(&mut self, from: String, msg: prep_core::group::GroupMsg) {
        use prep_core::group::{GroupMsg, InfoResult};
        let now = Self::now_secs();
        let me = self.user_id();
        match msg {
            GroupMsg::Info { admin, name, photo, removed, at, pk, sig } => {
                if !photo.is_empty() && crate::photo::valid_hash(&photo) {
                    self.photo_src.insert(photo.clone(), from.clone());
                }
                let Some(g) = self.groups.active_mut() else { return };
                let gname = g.name.clone();
                let code = g.code.clone();
                // chi è appena stato tolto (con il nome che aveva), per dirlo a schermo
                let gone: Vec<String> = removed
                    .iter()
                    .filter(|r| **r != me && !g.removed.contains(r))
                    .filter_map(|r| g.members.get(r).map(|m| m.status.name.clone()))
                    .collect();
                match g.apply_info(&me, &admin, &name, &photo, &removed, at, &pk, &sig, now) {
                    InfoResult::Ignored => {}
                    InfoResult::Applied => {
                        let _ = self.groups.save(&self.store.base);
                        for n in gone {
                            self.toast(format!("«{n}» è stato tolto dal gruppo dall'amministratore"));
                        }
                        // lo si ripete agli altri: arriva anche se l'amministratore non c'è più
                        self.group_send_info_as(true);
                        self.refresh_group();
                    }
                    InfoResult::Removed => {
                        self.groups.leave(&code);
                        let _ = self.groups.save(&self.store.base);
                        self.group_connect();
                        self.toast(format!("L'amministratore ti ha tolto dal gruppo «{gname}»"));
                    }
                }
            }
            GroupMsg::Cards { cards } => {
                let me = self.device_id();
                let Some(g) = self.groups.active_mut() else { return };
                let mut changed = false;
                for c in cards.into_iter().filter(|c| c.dev != me) {
                    let photo = c.status.photo.clone();
                    if g.card_in(c, now) {
                        changed = true;
                        // la foto si chiede a chi ha mandato la scheda (se manca)
                        if !photo.is_empty() && crate::photo::valid_hash(&photo) {
                            self.photo_src.entry(photo).or_insert_with(|| from.clone());
                        }
                    }
                }
                if changed {
                    let _ = self.groups.save(&self.store.base);
                    self.refresh_group();
                }
            }
            GroupMsg::PhotoGet { hash } => self.photo_send(Sess::Group, from, hash),
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

    /// Manda una foto a chi la chiede, direttamente (al massimo una volta ogni 3 s per foto e
    /// richiedente).
    pub(super) fn photo_send(&mut self, k: Sess, from: String, hash: String) {
        if !crate::photo::valid_hash(&hash) {
            return;
        }
        let known = self.store.settings().photo == hash
            || self
                .groups
                .active()
                .is_some_and(|g| g.photo == hash || g.members.values().any(|m| m.status.photo == hash));
        if !known {
            return;
        }
        let now = Self::now_secs();
        let key = format!("{hash}>{from}");
        if self.photo_sent.get(&key).is_some_and(|t| now - t < 3) {
            return;
        }
        if let Some(bytes) = crate::photo::load(&self.store.base, &hash) {
            self.photo_sent.insert(key, now);
            self.with_engine(k, move |e, _| vec![e.group_photo(Some(&from), &hash, bytes)]);
        }
    }

    /// Profilo di un altro proprio dispositivo: se la sua foto è più recente diventa anche
    /// la nostra (o si toglie, se lì è stata tolta); se manca il file lo si chiede a lui.
    pub(super) fn profile_in(&mut self, from: String, p: p2p::Profile) {
        self.unify_user_id(&p.uid);
        // tempo di studio: si sommano quelli di tutti i propri dispositivi
        if let Some(l) = p.study.clone() {
            if self.study_log.set_other(&from, l) {
                let _ = self.study_log.save(&self.store.base);
                self.refresh_group();
            }
        }
        // gruppi: uguali su tutti i propri dispositivi, con nome e foto più recenti
        let before = self.groups.active.clone();
        if self.groups.merge_own(&p.groups, &p.left) {
            let _ = self.groups.save(&self.store.base);
            let photos: Vec<String> =
                self.groups.groups.iter().map(|g| g.photo.clone()).filter(|h| !h.is_empty()).collect();
            for h in photos {
                self.ask_own_photo(&from, &h);
            }
            if self.groups.active != before || self.group.is_none() {
                self.group_connect();
            }
            self.refresh_group();
        }
        let mut s = self.store.settings();
        let mine = if s.photo_at == 0 && !s.photo.is_empty() { 1 } else { s.photo_at };
        if p.at < mine || (p.at == mine && p.photo <= s.photo) {
            // a parità di istante vince l'hash maggiore, così tutti arrivano alla stessa foto
            if p.photo == s.photo && !p.photo.is_empty() {
                self.ask_own_photo(&from, &p.photo);
            }
            return;
        }
        s.photo = p.photo.clone();
        s.photo_at = p.at;
        let _ = self.store.save_settings(&s);
        if !p.photo.is_empty() {
            self.ask_own_photo(&from, &p.photo);
        }
        self.group_send();
        self.refresh_profile_photo();
        self.refresh_group();
    }

    /// Un altro proprio dispositivo usa un altro identificativo utente (per esempio è entrato in
    /// un gruppo prima di collegarsi): tutti passano allo stesso (vedi `pick_user_id`), e il
    /// vecchio va in `prev_ids` così gli altri membri del gruppo uniscono le due voci.
    pub(super) fn unify_user_id(&mut self, other: &str) {
        let mut s = self.store.settings();
        if s.user_id.is_empty() {
            let _ = self.user_id();
            s = self.store.settings();
        }
        let mine = s.user_id.clone();
        let pick = prep_core::group::pick_user_id(&mine, other).to_string();
        if pick == mine || pick.is_empty() {
            return;
        }
        if !mine.is_empty() && !s.prev_ids.contains(&mine) {
            s.prev_ids.push(mine.clone());
        }
        s.user_id = pick.clone();
        let _ = self.store.save_settings(&s);
        if !mine.is_empty() {
            self.groups.rename_user(&mine, &pick);
            let _ = self.groups.save(&self.store.base);
        }
        self.uid_cache = None;
        self.group_send();
        self.refresh_group();
    }

    /// Chiede la foto `hash` al proprio dispositivo `from`, se manca (al massimo ogni 15 s).
    fn ask_own_photo(&mut self, from: &str, hash: &str) {
        if crate::photo::load(&self.store.base, hash).is_some() {
            return;
        }
        let now = Self::now_secs();
        let key = format!("own:{hash}");
        if self.photo_asked.get(&key).is_some_and(|t| now - t < 15) {
            return;
        }
        self.photo_asked.insert(key, now);
        let (to, msg) = (from.to_string(), prep_core::group::GroupMsg::PhotoGet { hash: hash.to_string() });
        self.with_engine(Sess::Sync, move |e, _| vec![e.group_msg_to(Some(&to), msg)]);
    }

    /// Foto profilo cambiata qui: la si annuncia agli altri propri dispositivi.
    fn profile_changed(&mut self) {
        let mut s = self.store.settings();
        s.photo_at = Self::now_secs().max(s.photo_at + 1);
        let _ = self.store.save_settings(&s);
        if self.p2p.as_ref().is_some_and(|p| p.started) {
            self.with_engine(Sess::Sync, |e, c| e.reannounce(c));
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
                    // a chi l'ha annunciata (collegamento diretto se c'è), altrimenti a tutti;
                    // dalla seconda volta a tutti (chi l'aveva annunciata può essere offline)
                    let again = self.photo_asked.insert(hash.to_string(), now).is_some();
                    let to = if again { None } else { self.photo_src.get(hash).cloned() };
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
        g.sign_current_info();
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
            g.sign_current_info();
        }
        let _ = self.groups.save(&self.store.base);
        self.group_send_info();
        self.group_send_info_stored();
        self.ui().global::<AppState>().set_member_sel(ss(""));
        self.toast("Membro rimosso dal gruppo: lo sapranno tutti, anche chi ora è offline");
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
        // computer (anche Flatpak): finestra di sistema, fuori dal thread dell'interfaccia
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        {
            let kind = kind.to_string();
            let title = if kind == "group" { "Scegli la foto del gruppo" } else { "Scegli la foto profilo" };
            spawn_background(move || {
                let Some(p) = crate::desktop_files::pick_file_titled(title) else { return };
                let res = std::fs::read(&p);
                let _ = slint::invoke_from_event_loop(move || {
                    with_ctrl(|c| match res {
                        Ok(bytes) => c.photo_imported(&kind, &bytes),
                        Err(e) => c.toast(format!("Errore: {e}")),
                    })
                });
            });
        }
    }

    /// Immagine scelta: si apre il ritaglio (quadrato, spostabile e ingrandibile).
    pub(super) fn photo_imported(&mut self, kind: &str, bytes: &[u8]) {
        let img = match crate::photo::decode(bytes) {
            Ok(i) => i,
            Err(e) => {
                self.toast(e);
                return;
            }
        };
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_crop_img(slint::Image::from_rgba8(buf));
        st.set_crop_w(w as f32);
        st.set_crop_h(h as f32);
        st.set_crop_title(ss(if kind == "group" { "Foto del gruppo" } else { "Foto profilo" }));
        st.set_crop_visible(true);
        self.crop = Some((kind.to_string(), img));
    }

    /// «Usa foto»: il quadrato scelto (in pixel dell'immagine) diventa la foto.
    pub(super) fn crop_done(&mut self, x: f32, y: f32, side: f32) {
        self.ui().global::<AppState>().set_crop_visible(false);
        let Some((kind, img)) = self.crop.take() else { return };
        match crate::photo::crop_encode(&img, x, y, side) {
            Ok(jpeg) => self.photo_save(&kind, jpeg),
            Err(e) => self.toast(e),
        }
    }

    pub(super) fn crop_cancel(&mut self) {
        self.ui().global::<AppState>().set_crop_visible(false);
        self.crop = None;
    }

    /// Foto pronta: salvata e annunciata al gruppo e ai propri dispositivi.
    fn photo_save(&mut self, kind: &str, jpeg: Vec<u8>) {
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
                g.sign_current_info();
            }
            let _ = self.groups.save(&self.store.base);
            self.group_send_info();
            self.toast("Foto del gruppo aggiornata");
        } else {
            let mut s = self.store.settings();
            s.photo = hash.clone();
            let _ = self.store.save_settings(&s);
            self.profile_changed();
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
                g.sign_current_info();
            }
            let _ = self.groups.save(&self.store.base);
            self.group_send_info();
        } else {
            let mut s = self.store.settings();
            s.photo.clear();
            let _ = self.store.save_settings(&s);
            self.profile_changed();
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
        use prep_core::group::{fmt_secs, live, standings, Period, State};
        use prep_core::score::LAP_PTS;
        let now = Self::now_secs();
        let ck = prep_core::group::Clock::local(now);
        let today = ck.today;
        let me_dev = self.device_id();
        let me_id = self.user_id();
        // io: questo dispositivo più gli altri miei (stesso id utente) visti nel gruppo
        let mine = {
            let here = self.my_group_status(now, true);
            let mut devs = vec![(here.clone(), true)];
            let mut seen = vec![me_dev.clone()];
            if let Some(m) = self.groups.active().and_then(|g| g.members.get(&me_id)) {
                for (d, ds) in &m.devices {
                    if *d != me_dev {
                        devs.push(prep_core::group::frozen(&ds.status, ds.last_seen, &ck));
                        seen.push(d.clone());
                    }
                }
            }
            // altri propri dispositivi non visti nel gruppo: il loro ultimo stato (contato in
            // diretta, arriva ogni 10 s dalla sincronizzazione) o, se tace da un pezzo, il registro
            let mut others: Vec<&String> = self.study_log.others.keys().chain(self.own_devs.keys()).collect();
            others.sort();
            others.dedup();
            for d in others {
                if seen.contains(d) {
                    continue;
                }
                match self.own_devs.get(d) {
                    Some((s, at)) if now - at < 120 => devs.push(prep_core::group::frozen(s, *at, &ck)),
                    _ => {
                        if let Some(l) = self.study_log.others.get(d) {
                            let log: prep_core::group::StudyLog = l.clone().into();
                            let idle = prep_core::score::Score { mult: 1.0, fx: 0, level: 0.0 };
                            let s = prep_core::group::my_status("", 0, State::Idle, &log, None, &ck, false, idle);
                            devs.push((s, false));
                        }
                    }
                }
            }
            let (mut c, _) = prep_core::group::combine(&devs, &ck);
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
        let (t, _, _, _) = live(&mine, &ck);
        let my_pts = prep_core::group::live_pts(&mine, &ck);
        let laps = (my_pts[0] / LAP_PTS) as u64;
        if laps > self.my_laps && self.study_state == State::Studying && self.my_laps > 0 {
            if laps.is_multiple_of(10) {
                self.toast(format!("{laps} giri: {} di studio oggi!", fmt_secs(t)));
            }
            st.set_lap_flash(st.get_lap_flash() + 1);
        }
        self.my_laps = laps;
        st.set_study_elsewhere(self.study_state == State::Idle && self.twin_active(now));
        st.set_study_state(match self.study_state {
            State::Idle => 0,
            State::Studying => 1,
            State::Pause => 2,
        });
        let sc = self.score_now(now);
        st.set_study_today(ss(if self.study_state == State::Studying {
            format!("Oggi {} · {} punti · ×{:.1}", fmt_secs(t), fmt_pts(my_pts[0]), sc.mult)
        } else {
            format!("Oggi {} · {} punti", fmt_secs(t), fmt_pts(my_pts[0]))
        }));
        st.set_study_lap((my_pts[0] % LAP_PTS / LAP_PTS) as f32);
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
                let (c, on) = prep_core::group::combine(&m.device_list(&ck), &ck);
                all.push((id.clone(), c, on, m.last_seen));
            }
        }
        // ordine stabile (la propria macchina per ultima, sopra le altre)
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all.push((me_id.clone(), mine.clone(), true, now));
        let on_track = all.iter().filter(|(_, s, on, _)| *on && s.state == State::Studying).count();
        // scia di gruppo: gli altri in pista adesso
        self.racing_others =
            all.iter().filter(|(id, s, on, _)| *id != me_id && *on && s.state == State::Studying).count();
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
        // paddock: un box per membro, quindi cresce con il gruppo (piccolo all'inizio)
        const SLOT: f64 = 0.075;
        let pad_w = (SLOT * all.len().max(2) as f64 + 0.05).min(0.95);
        st.set_pad_w(pad_w as f32);
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
        for (slot, (id, s, on, _)) in all.iter().enumerate() {
            let me = *id == me_id;
            let racing = *on && s.state == State::Studying;
            let pit = *on && s.state == State::Pause;
            let parked = !racing && !pit;
            let (px, py) = if parked {
                // ognuno nel proprio box (stesso posto ogni volta)
                let (cx, cy) = circuit.paddock;
                let x = cx + SLOT * (slot as f64 - (all.len() as f64 - 1.0) / 2.0);
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
            // posizione = punti di oggi (un giro ogni LAP_PTS), velocità = moltiplicatore
            let m = if s.mult > 0.0 { s.mult as f64 } else { 1.0 };
            let secs = prep_core::group::live_pts(s, &ck)[0] + if racing { frac * m } else { 0.0 };
            cars.push(CarVM {
                id: ss(id),
                crew,
                f: (secs / LAP_PTS) as f32,
                v: if racing { (m / LAP_PTS) as f32 } else { 0.0 },
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
                fx: if racing { s.fx as i32 } else { 0 },
                mult: if racing { m as f32 } else { 1.0 },
                level: s.level,
            });
        }
        // modalità dimostrativa degli effetti: altre monoposto attorno alla propria
        if self.fx_demo {
            fx_showcase(&mut cars);
        }
        // telecamera sulla propria macchina mentre si studia; altrimenti tutto il circuito
        st.set_race_overview(self.study_state != State::Studying);
        let my_m = if self.study_state == State::Studying { sc.mult } else { 1.0 };
        let my_secs = my_pts[0] + if self.study_state == State::Studying { frac * my_m } else { 0.0 };
        st.set_cam_f((my_secs / LAP_PTS) as f32);
        st.set_lap_speed((my_m / LAP_PTS) as f32);
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
        // «Mostra nel circuito»: la telecamera segue la monoposto di un altro finché è in pista e
        // la sua scheda resta aperta
        if let Some(fid) = self.cam_follow.clone() {
            match cars.iter().find(|c| c.id.as_str() == fid && c.racing) {
                Some(c) => {
                    st.set_race_overview(false);
                    st.set_cam_f(c.f + if c.v > 0.0 { c.v * frac as f32 } else { 0.0 });
                    st.set_cam_side(c.side);
                    st.set_cam_stack(c.stack);
                    st.set_cam_run(c.run);
                }
                None => self.cam_follow = None,
            }
        }
        let fx_of = |id: &str| cars.iter().find(|c| c.id.as_str() == id).map(fx_text).unwrap_or_default();
        // scheda della macchina toccata
        match all.iter().find(|(id, _, _, _)| *id == self.car_sel) {
            Some((id, s, on, seen)) => {
                let (t_day, t_week, t_month, _) = live(s, &ck);
                let short = |x: u64| if x < 60 { format!("{x} s") } else { fmt_secs(x) };
                let mut lines = Vec::new();
                lines.push(match (*on, s.state) {
                    (false, _) => format!(
                        "Non collegato · visto l'ultima volta {}",
                        chrono::DateTime::from_timestamp(*seen, 0)
                            .map(|d| d.with_timezone(&Local).format("%d/%m alle %H:%M").to_string())
                            .unwrap_or_else(|| "—".into())
                    ),
                    (true, State::Studying) => {
                        format!("In pista · giro {}", (prep_core::group::live_pts(s, &ck)[0] / LAP_PTS) as u64 + 1)
                    }
                    (true, State::Pause) if s.pit_since > 0 => {
                        format!("Pit stop da {}", short((now - s.pit_since).max(0) as u64))
                    }
                    (true, State::Pause) => "Pit stop".to_string(),
                    (true, State::Idle) => "Sessione terminata: al paddock fino alla prossima".to_string(),
                });
                lines.push(format!(
                    "Studio: oggi {} · settimana {} · mese {}",
                    fmt_secs(t_day),
                    fmt_secs(t_week),
                    fmt_secs(t_month)
                ));
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
                let fx = fx_of(id);
                if !fx.is_empty() {
                    lines.push(fx);
                }
                let me = *id == me_id;
                st.set_car_info_kind(match (*on, s.state) {
                    (false, _) => 0,
                    (true, State::Studying) => 1,
                    (true, State::Pause) => 2,
                    (true, State::Idle) => 3,
                });
                st.set_car_info_state(ss(match (*on, s.state) {
                    (false, _) => "Non collegato",
                    (true, State::Studying) => "In pista",
                    (true, State::Pause) => "Pit stop",
                    (true, State::Idle) => "Al paddock",
                }));
                st.set_car_info_today(ss(fmt_secs(t_day)));
                st.set_car_info_week(ss(fmt_secs(t_week)));
                st.set_car_info_month(ss(fmt_secs(t_month)));
                // dettagli: tutto tranne i tempi, già nei riquadri
                // (lo stato è già nell'etichetta: resta il giro)
                let details: Vec<slint::SharedString> = lines
                    .iter()
                    .filter(|l| !l.starts_with("Studio: "))
                    .map(|l| ss(l.strip_prefix("In pista · g").map(|r| format!("G{r}")).unwrap_or_else(|| l.clone())))
                    .collect();
                st.set_car_info_lines(model(details));
                st.set_car_info_me(me);
                st.set_car_info_racing(*on && s.state == State::Studying);
                st.set_car_info_following(self.cam_follow.as_deref() == Some(id.as_str()));
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
            // monoposto della vetrina degli effetti (modalità dimostrativa)
            None if cars.iter().any(|c| c.id.as_str() == self.car_sel) => {
                let c = cars.iter().find(|c| c.id.as_str() == self.car_sel).cloned().unwrap();
                st.set_car_info_has_photo(false);
                st.set_car_info_color(c.color);
                st.set_car_info_initial(c.label.clone());
                st.set_car_info_title(ss(format!("Pilota {}", c.label)));
                st.set_car_info_body(ss(format!("In pista\n{}", fx_text(&c))));
                st.set_car_info_kind(1);
                st.set_car_info_state(ss("In pista"));
                st.set_car_info_lines(model(vec![ss(fx_text(&c))]));
                st.set_car_info_me(false);
                st.set_car_info_racing(false);
                st.set_car_sel(c.id.clone());
            }
            None => {
                self.car_sel.clear();
                st.set_car_sel(ss(""));
            }
        }
        // mappe di calore: la propria e quella del membro toccato in classifica
        let all_log = self.study_log.combined();
        let my_hist = mine.hist.as_ref().map(|h| h.on(today)).unwrap_or_else(|| all_log.hist(today));
        // la propria: anche oltre le 12 settimane condivise, dal registro locale (fino a un anno)
        let my_long: Vec<u32> = (0..366)
            .map(|i| {
                let d = (today - chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
                let local = all_log.days.get(&d).copied().unwrap_or(0) as u32;
                local.max(my_hist.get(i as usize).copied().unwrap_or(0))
            })
            .collect();
        let hm = heat_month(&my_long, today, self.heat_back[0]);
        st.set_my_heat(model(hm.cells));
        st.set_my_heat_total(ss(hm.total));
        st.set_my_heat_title(ss(hm.title));
        st.set_my_heat_prev(hm.can_prev);
        st.set_my_heat_next(self.heat_back[0] > 0);
        // calendario: «Tu», gli altri membri (in ordine di nome), «Gruppo»
        let mut people: Vec<(String, String)> =
            all.iter().filter(|(id, _, _, _)| *id != me_id).map(|(id, s, _, _)| (id.clone(), s.name.clone())).collect();
        people.sort_by_key(|p| p.1.to_lowercase());
        let mut labels = vec![ss("Tu")];
        let mut ids = vec![ss("")];
        for (id, name) in &people {
            labels.push(ss(name));
            ids.push(ss(id));
        }
        labels.push(ss("Gruppo"));
        ids.push(ss("*"));
        if !self.cal_sel.is_empty() && !ids.iter().any(|i| i.as_str() == self.cal_sel) {
            self.cal_sel.clear();
        }
        let idx = ids.iter().position(|i| i.as_str() == self.cal_sel).unwrap_or(0);
        st.set_cal_labels(model(labels));
        st.set_cal_ids(model(ids));
        st.set_cal_index(idx as i32);
        let sel = if self.cal_sel == "*" { String::new() } else { self.cal_sel.clone() };
        if sel != self.heat_member {
            self.heat_member = sel.clone();
            self.heat_back[1] = 0;
        }
        let member_hist = all.iter().find(|(id, _, _, _)| *id == sel).and_then(|(_, s, _, _)| s.hist.as_ref());
        match member_hist {
            Some(h) => {
                let hm = heat_month(&h.on(today), today, self.heat_back[1]);
                st.set_member_heat(model(hm.cells));
                st.set_member_heat_title(ss(hm.title));
                st.set_member_heat_prev(hm.can_prev);
            }
            None => st.set_member_heat(model(Vec::new())),
        }
        st.set_member_heat_next(self.heat_back[1] > 0);
        self.refresh_group_calendar(&all, today);
        self.refresh_profile(&all, &cars, &ck);
        let p = self.group_period;
        let refs: Vec<(&str, &prep_core::group::Status)> = all.iter().map(|(i, s, _, _)| (i.as_str(), s)).collect();
        let order = standings(refs.into_iter(), &ck, p);
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
                let v = prep_core::group::points(s, &ck, p);
                let secs = prep_core::group::value(s, &ck, p);
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
                    value: ss(format!("{} pt", fmt_pts(v))),
                    laps: (prep_core::group::live_pts(s, &ck)[0] / LAP_PTS) as i32,
                    time: ss(fmt_secs(secs)),
                    status: ss(match (on, s.state) {
                        (false, _) => "non collegato",
                        (true, State::Studying) => "in pista",
                        (true, State::Pause) => "pit stop",
                        (true, State::Idle) => "al paddock",
                    }),
                    me: *id == me_id,
                    racing: on && s.state == State::Studying,
                    boost: ss(cars
                        .iter()
                        .find(|c| c.id.as_str() == id.as_str())
                        .filter(|c| (c.mult - 1.0).abs() > 0.04)
                        .map(|c| format!("×{:.1}", c.mult))
                        .unwrap_or_default()),
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
        // file singoli scelti in «Contenuti» (`file-<ora>--<nome>`)
        if let Some(f) = files.iter().find(|p| {
            p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("file-"))
                && p.extension().is_none_or(|x| x != "tmp")
        }) {
            if self.pdf_pick.is_some() {
                self.pdf_file(f.clone());
            } else {
                self.contents_import(f.clone());
            }
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
        // sul computer: finestra di scelta file; se non c'è, l'ultimo pacchetto in Download
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        {
            let import = self.store.base.join("import");
            spawn_background(move || {
                let picked = crate::desktop_files::pick_file().or_else(newest_pack_download);
                let msg = match picked.and_then(|p| std::fs::read(p).ok()) {
                    Some(bytes) => {
                        let _ = vfs::create_dir_all(&import);
                        let dst = import.join(format!("pack-{}.json.gz", chrono::Utc::now().timestamp_millis()));
                        let _ = vfs::write(&dst, &bytes);
                        None
                    }
                    None => Some("Nessun pacchetto scelto (o nella cartella Download)"),
                };
                if let Some(m) = msg {
                    let _ = slint::invoke_from_event_loop(move || with_ctrl(|c| c.toast(m)));
                }
            });
        }
    }
}

const MONTH_NAMES: [&str; 12] = [
    "gennaio",
    "febbraio",
    "marzo",
    "aprile",
    "maggio",
    "giugno",
    "luglio",
    "agosto",
    "settembre",
    "ottobre",
    "novembre",
    "dicembre",
];

/// Primo giorno del mese di `today` spostato indietro di `back` mesi.
fn month_start(today: chrono::NaiveDate, back: i32) -> chrono::NaiveDate {
    use chrono::Datelike;
    let mut y = today.year();
    let mut m = today.month() as i32 - back;
    while m < 1 {
        m += 12;
        y -= 1;
    }
    chrono::NaiveDate::from_ymd_opt(y, m as u32, 1).unwrap_or(today)
}

/// Mappa di calore di un mese: calendario a righe di settimane (dal lunedì), `back` mesi fa;
/// `hist[i]` = secondi di studio di `i` giorni fa. Intensità per durata assoluta.
pub(super) struct HeatMonth {
    pub cells: Vec<crate::HeatVM>,
    pub title: String,
    pub total: String,
    pub can_prev: bool,
}

fn heat_month(hist: &[u32], today: chrono::NaiveDate, back: i32) -> HeatMonth {
    use chrono::Datelike;
    const DAYS: [&str; 7] = ["lun", "mar", "mer", "gio", "ven", "sab", "dom"];
    let start = month_start(today, back);
    let next = month_start(today, back - 1);
    let first = start - chrono::Duration::days(start.weekday().num_days_from_monday() as i64);
    let weeks = ((next - first).num_days() + 6) / 7;
    let known = |date: chrono::NaiveDate| {
        let i = (today - date).num_days();
        (i >= 0 && (i as usize) < hist.len()).then(|| hist[i as usize])
    };
    let mut cells = Vec::new();
    for k in 0..weeks * 7 {
        let date = first + chrono::Duration::days(k);
        if date < start || date >= next {
            cells.push(crate::HeatVM { level: -1, label: ss(""), day: ss("") });
            continue;
        }
        let day = ss(date.day().to_string());
        let Some(secs) = known(date) else {
            // futuro o prima della storia conservata
            cells.push(crate::HeatVM { level: -2, label: ss(""), day });
            continue;
        };
        let level = match secs {
            0 => 0,
            1..1800 => 1,
            1800..5400 => 2,
            5400..10800 => 3,
            _ => 4,
        };
        let what = if secs == 0 { "niente studio".to_string() } else { prep_core::group::fmt_secs(secs as u64) };
        cells.push(crate::HeatVM {
            level,
            label: ss(format!(
                "{} {}: {what}",
                DAYS[date.weekday().num_days_from_monday() as usize],
                date.format("%d/%m")
            )),
            day,
        });
    }
    let days: Vec<u32> =
        (0..(next - start).num_days()).filter_map(|d| known(start + chrono::Duration::days(d))).collect();
    let active: Vec<u32> = days.iter().copied().filter(|x| *x > 0).collect();
    let total = if active.is_empty() {
        "Nessuno studio registrato in questo mese".to_string()
    } else {
        format!(
            "{} in {} {} · record {}",
            prep_core::group::fmt_secs(active.iter().map(|x| *x as u64).sum()),
            active.len(),
            if active.len() == 1 { "giorno" } else { "giorni" },
            prep_core::group::fmt_secs(active.iter().copied().max().unwrap_or(0) as u64)
        )
    };
    // il mese prima ha ancora giorni nella storia?
    let prev_last = start - chrono::Duration::days(1);
    HeatMonth {
        cells,
        title: format!("{} {}", MONTH_NAMES[start.month0() as usize], start.year()),
        total,
        can_prev: ((today - prev_last).num_days() as usize) < hist.len(),
    }
}

impl Ctrl {
    /// Calendario del gruppo (settimana o mese): studio di tutti per giorno; il giorno toccato
    /// mostra chi ha studiato, quanto e su cosa.
    fn refresh_group_calendar(
        &mut self,
        all: &[(String, prep_core::group::Status, bool, i64)],
        today: chrono::NaiveDate,
    ) {
        use chrono::{Datelike, Duration};
        use prep_core::group::{fmt_secs, HIST_DAYS};
        const MONTHS: [&str; 12] = MONTH_NAMES;
        const DAYS: [&str; 7] = ["lunedì", "martedì", "mercoledì", "giovedì", "venerdì", "sabato", "domenica"];
        // per membro: nome e studio per giorno (indice = giorni fa)
        let rows: Vec<(String, Vec<u32>, Option<&prep_core::group::Hist>)> = all
            .iter()
            .map(|(_, s, _, _)| {
                let h = s.hist.as_ref();
                (s.name.clone(), h.map(|h| h.on(today)).unwrap_or_else(|| vec![0; HIST_DAYS]), h)
            })
            .collect();
        let day_total = |i: i64| -> u64 {
            if !(0..HIST_DAYS as i64).contains(&i) {
                return 0;
            }
            rows.iter().map(|(_, v, _)| v[i as usize] as u64).sum()
        };
        let monday = |d: chrono::NaiveDate| d - Duration::days(d.weekday().num_days_from_monday() as i64);
        // intervallo mostrato: settimana o mese (con le settimane complete attorno)
        let (first, last, title, from, to) = if self.gcal_month {
            let mut y = today.year();
            let mut m = today.month() as i32 - self.gcal_off;
            while m < 1 {
                m += 12;
                y -= 1;
            }
            let start = chrono::NaiveDate::from_ymd_opt(y, m as u32, 1).unwrap_or(today);
            let next = if m == 12 {
                chrono::NaiveDate::from_ymd_opt(y + 1, 1, 1)
            } else {
                chrono::NaiveDate::from_ymd_opt(y, m as u32 + 1, 1)
            }
            .unwrap_or(today);
            let end = next - Duration::days(1);
            let grid_end = monday(end) + Duration::days(6);
            (monday(start), grid_end, format!("{} {y}", MONTHS[m as usize - 1]), start, end)
        } else {
            let start = monday(today) - Duration::weeks(self.gcal_off as i64);
            let end = start + Duration::days(6);
            let title = if start.month() == end.month() {
                format!("{} – {} {}", start.day(), end.day(), MONTHS[end.month0() as usize])
            } else {
                format!(
                    "{} {} – {} {}",
                    start.day(),
                    MONTHS[start.month0() as usize],
                    end.day(),
                    MONTHS[end.month0() as usize]
                )
            };
            (start, end, title, start, end)
        };
        let days: Vec<chrono::NaiveDate> = (0..=(last - first).num_days()).map(|k| first + Duration::days(k)).collect();
        let max = days.iter().map(|d| day_total((today - *d).num_days())).max().unwrap_or(0).max(1);
        let cells: Vec<crate::GHeatVM> = days
            .iter()
            .map(|d| {
                let i = (today - *d).num_days();
                let inside = *d >= from && *d <= to;
                let known = (0..HIST_DAYS as i64).contains(&i);
                let secs = day_total(i);
                crate::GHeatVM {
                    day: ss(d.day().to_string()),
                    idx: if inside && known { i as i32 } else { -1 },
                    level: if !inside || !known {
                        -1
                    } else if secs == 0 {
                        0
                    } else {
                        (1 + (secs * 3 / max) as i32).min(4)
                    },
                    sel: inside && known && self.gcal_sel == Some(i),
                    today: i == 0,
                }
            })
            .collect();
        // dettaglio del giorno toccato
        let mut lines: Vec<String> = Vec::new();
        if let Some(i) = self.gcal_sel.filter(|i| (0..HIST_DAYS as i64).contains(i)) {
            let d = today - Duration::days(i);
            type Who<'a> = (&'a str, u32, Vec<(String, u32)>);
            let mut who: Vec<Who> = rows
                .iter()
                .filter(|(_, v, _)| v[i as usize] > 0)
                .map(|(n, v, h)| {
                    (n.as_str(), v[i as usize], h.map(|h| h.what_on(today, i as usize)).unwrap_or_default())
                })
                .collect();
            who.sort_by_key(|w| std::cmp::Reverse(w.1));
            lines.push(format!(
                "{} {} {}: {}",
                DAYS[d.weekday().num_days_from_monday() as usize],
                d.day(),
                MONTHS[d.month0() as usize],
                if who.is_empty() {
                    "nessuno ha studiato".to_string()
                } else {
                    format!("{} in tutto", fmt_secs(day_total(i)))
                }
            ));
            for (name, secs, what) in who {
                let on: Vec<String> = what.iter().map(|(k, v)| format!("{k} {}", fmt_secs(*v as u64))).collect();
                lines.push(if on.is_empty() {
                    format!("{name} · {}", fmt_secs(secs as u64))
                } else {
                    format!("{name} · {} — {}", fmt_secs(secs as u64), on.join(", "))
                });
            }
        }
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_gcal_cells(model(cells));
        st.set_gcal_title(ss(title));
        st.set_gcal_month(self.gcal_month);
        st.set_gcal_lines(model(lines.iter().map(ss).collect::<Vec<_>>()));
        // si torna indietro finché c'è storia (12 settimane)
        st.set_gcal_can_prev((today - first).num_days() + 7 < HIST_DAYS as i64);
        st.set_gcal_can_next(self.gcal_off > 0);
    }
}

/// Vetrina degli effetti (solo modalità dimostrativa): la propria macchina con ritmo al massimo e
/// concentrazione, e attorno una macchina per effetto.
fn fx_showcase(cars: &mut Vec<CarVM>) {
    let Some(me) = cars.iter_mut().find(|c| c.me) else { return };
    me.fx = 1 | 4;
    me.mult = 4.5;
    me.level = 1.0;
    let base = me.clone();
    let list: [(&str, u32, f32, f32, i32, f32, f32); 6] = [
        ("A", 0xffe53935, 0.012, -1.0, 1, 2.5, 0.5),
        ("B", 0xff43a047, -0.012, 1.0, 2, 1.2, 0.0),
        ("C", 0xff8e24aa, 0.024, 1.0, 8, 0.8, 0.0),
        ("D", 0xfffb8c00, -0.024, -1.0, 16 | 1, 1.6, 0.2),
        ("E", 0xff00acc1, 0.036, 0.0, 32, 1.0, 0.0),
        ("F", 0xff6d4c41, -0.036, 0.0, 0, 1.0, 0.0),
    ];
    for (label, color, df, lane, fx, mult, level) in list {
        let mut c = base.clone();
        c.id = ss(format!("demo-{label}"));
        c.label = ss(label);
        c.me = false;
        c.color = slint::Color::from_argb_encoded(color);
        c.f += df;
        c.lane = lane;
        c.fx = fx;
        c.mult = mult;
        c.level = level;
        cars.push(c);
    }
}

/// Sessione di studio in corso, salvata per riprenderla se l'app viene chiusa dal sistema o il
/// telefono si riavvia durante la concentrazione.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedSession {
    /// 0 nessuna, 1 in pista, 2 pit stop
    state: i32,
    /// inizio del tratto in corso (in pista) o della sosta (pit stop)
    since: i64,
    /// quando è iniziata la sessione
    started: i64,
    /// tratto di studio con i suoi effetti (punteggio)
    #[serde(default)]
    run: prep_core::score::Run,
    /// inizio della sessione (per tenerne aperta una sola tra i propri dispositivi)
    #[serde(default)]
    session: i64,
}

impl Ctrl {
    fn session_path(&self) -> PathBuf {
        self.store.base.join("study_session.json")
    }

    fn save_study_session(&self, now: i64) {
        use prep_core::group::State;
        let s = SavedSession {
            state: match self.study_state {
                State::Idle => 0,
                State::Studying => 1,
                State::Pause => 2,
            },
            since: self.study_since.or(self.pit_since).unwrap_or(now),
            started: now,
            run: self.run.clone(),
            session: self.session_start,
        };
        if s.state == 0 {
            let _ = vfs::remove_file(&self.session_path());
        } else if let Ok(t) = serde_json::to_string(&s) {
            let _ = vfs::write(&self.session_path(), t.as_bytes());
        }
    }

    /// All'avvio: se la concentrazione tiene ancora una sessione aperta, la si riprende (il tempo
    /// passato da allora conta, al massimo 6 ore come ogni tratto). `false` se non c'è niente da
    /// riprendere.
    pub(super) fn restore_study_session(&mut self) -> bool {
        use prep_core::group::State;
        let Some(j) = crate::focus::state().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
            return false;
        };
        let session = j["session"].as_i64().unwrap_or(0);
        if !j["armed"].as_bool().unwrap_or(false) || session == 0 {
            return false;
        }
        let now = Self::now_secs();
        let saved: SavedSession = vfs::read_to_string(&self.session_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let since = if saved.since > 0 && now - saved.since < 6 * 3600 { saved.since } else { now };
        if session == 2 {
            self.study_state = State::Pause;
            self.pit_since = Some(since);
            self.study_since = None;
        } else {
            self.study_state = State::Studying;
            self.study_since = Some(since);
        }
        self.run = saved.run.clone();
        self.session_start = if saved.session > 0 { saved.session } else { since };
        if session != 2 && self.run.since.is_none() {
            self.run.since = Some(since);
        }
        crate::focus::session(session as i32);
        self.toast("Concentrazione ancora attiva: la sessione di studio continua");
        true
    }
}

/// Effetti in corso di una monoposto, a parole (vuoto se nessuno).
fn fx_text(c: &CarVM) -> String {
    const NAMES: [&str; 6] =
        ["ritmo", "gomme nuove", "concentrazione", "penalità", "scia di gruppo", "bandiera gialla"];
    let names: Vec<&str> = NAMES.iter().enumerate().filter(|(i, _)| c.fx & (1 << i) != 0).map(|(_, n)| *n).collect();
    if names.is_empty() && (c.mult - 1.0).abs() < 0.04 {
        return String::new();
    }
    let mut t = format!("Moltiplicatore ×{:.1}", c.mult);
    if !names.is_empty() {
        t.push_str(&format!(" · {}", names.join(", ")));
    }
    t
}

impl Ctrl {
    /// Profilo di un membro (o il proprio): stato, studio, mappa di calore e numeri dell'esame;
    /// le funzioni non usate non compaiono.
    fn refresh_profile(
        &mut self,
        all: &[(String, prep_core::group::Status, bool, i64)],
        cars: &[CarVM],
        ck: &prep_core::group::Clock,
    ) {
        use prep_core::group::{fmt_secs, live, State};
        let found = self.profile_sel.as_ref().and_then(|pid| all.iter().find(|(id, _, _, _)| id == pid)).cloned();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let Some((id, s, on, seen)) = found else {
            self.profile_sel = None;
            st.set_profile_open(false);
            return;
        };
        let pct = |ok: u32, n: u32| (ok * 100 + n / 2).checked_div(n).unwrap_or(0);
        let (t, w, m, _) = live(&s, ck);
        let mut lines: Vec<String> = Vec::new();
        let prof = s.prof.clone().unwrap_or_default();
        if !prof.exam.is_empty() {
            lines.push(format!("Esame: {}", prof.exam));
        }
        let now_line = match (on, s.state) {
            (false, _) => format!(
                "Non collegato · visto {}",
                chrono::DateTime::from_timestamp(seen, 0)
                    .map(|d| d.with_timezone(&Local).format("%d/%m alle %H:%M").to_string())
                    .unwrap_or_else(|| "—".into())
            ),
            (true, State::Studying) => "In pista".to_string(),
            (true, State::Pause) => "Al pit stop".to_string(),
            (true, State::Idle) => "Al paddock".to_string(),
        };
        lines.push(if on && s.state != State::Idle && !s.doing.is_empty() {
            format!("{now_line} · {}", s.doing)
        } else {
            now_line
        });
        lines.push(format!("Studio: oggi {} · settimana {} · mese {}", fmt_secs(t), fmt_secs(w), fmt_secs(m)));
        let hist = s.hist.as_ref().map(|h| h.on(ck.today));
        if let Some(h) = &hist {
            let days: u64 = h.iter().take(28).map(|x| *x as u64).sum();
            if days > 0 {
                lines.push(format!("Media al giorno (ultime 4 settimane): {}", fmt_secs(days / 28)));
            }
        }
        if let Some(c) = cars.iter().find(|c| c.id.as_str() == id) {
            let fx = fx_text(c);
            if !fx.is_empty() {
                lines.push(fx);
            }
        }
        if prof.goals_total > 0 {
            lines.push(format!("Obiettivi di oggi: {} su {} completati", prof.goals_done, prof.goals_total));
        }
        if prof.q_all > 0 {
            let mut q = format!(
                "Quiz: oggi {} risposte ({}% giuste) · in tutto {} ({}%)",
                prof.q_today,
                pct(prof.q_today_ok, prof.q_today),
                prof.q_all,
                pct(prof.q_all_ok, prof.q_all)
            );
            if prof.sims > 0 {
                q.push_str(&format!(" · {} simulazioni", prof.sims));
            }
            lines.push(q);
        }
        if prof.cards_all > 0 {
            lines.push(format!("Flashcard: oggi {} · in tutto {}", prof.cards_today, prof.cards_all));
        }
        let photo = self.photo_image(&s.photo.clone());
        let me = id == self.user_id();
        st.set_profile_id(ss(&id));
        st.set_profile_name(ss(if me { format!("{} (tu)", s.name) } else { s.name.clone() }));
        st.set_profile_has_photo(photo.is_some());
        st.set_profile_photo(photo.unwrap_or_default());
        st.set_profile_initial(ss(s.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()));
        st.set_profile_color(slint::Color::from_argb_encoded(s.color));
        st.set_profile_lines(model(lines.iter().map(ss).collect::<Vec<_>>()));
        match hist {
            Some(h) => {
                let hm = heat_month(&h, ck.today, self.heat_back[2]);
                st.set_profile_heat(model(hm.cells));
                st.set_profile_heat_title(ss(hm.title));
                st.set_profile_heat_prev(hm.can_prev);
            }
            None => st.set_profile_heat(model(Vec::new())),
        }
        st.set_profile_heat_next(self.heat_back[2] > 0);
        st.set_profile_racing(on && s.state == State::Studying);
        st.set_profile_open(true);
    }
}

impl Ctrl {
    /// Notifica fissa della sessione di studio (Android): a ogni cambio di stato e ogni 30 s.
    /// Cronometro sul tratto in corso; avanzamento a segmenti da 25 minuti (gli scatti del
    /// ritmo) fino a 200, o in pausa i minuti della sosta fino ai 10 di un pit stop perfetto.
    fn study_notify(&mut self, now: i64) {
        use prep_core::group::{fmt_secs, State};
        let code = match self.study_state {
            State::Idle => 0,
            State::Studying => 1,
            State::Pause => 2,
        };
        let (last, last_code) = self.study_notified;
        if code == last_code && (code == 0 || now - last < 30) {
            return;
        }
        self.study_notified = (now, code);
        if code == 0 {
            crate::study_notify(r#"{"state":0}"#);
            return;
        }
        let today = {
            let s = self.my_group_status(now, false);
            s.today
        };
        let doing = self.doing();
        let color = self.accent.unwrap_or(crate::palette::DEFAULT_SEED);
        let focus_until = crate::focus::state()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|j| j["until"].as_i64())
            .unwrap_or(0);
        let (title, since, progress, max, segment) = if self.study_state == State::Studying {
            let streak = self.run.streak(now);
            let sc = self.score_now(now);
            (
                format!("In pista ×{:.1} · oggi {}", sc.mult, fmt_secs(today)),
                now - streak,
                (streak / 60).min(200),
                200,
                25,
            )
        } else {
            let start = self.pit_since.unwrap_or(now);
            let mins = (now - start).max(0) / 60;
            (format!("Pit stop · oggi {}", fmt_secs(today)), start, mins.min(10), 10, 10)
        };
        let json = serde_json::json!({
            "state": code,
            "title": title,
            "text": doing,
            "since": since * 1000,
            "progress": progress,
            "max": max,
            "segment": segment,
            "color": color,
            "focus_until": focus_until,
        });
        crate::study_notify(&json.to_string());
    }
}

/// Punti con il separatore delle migliaia («12.480»).
pub(super) fn fmt_pts(p: f64) -> String {
    let n = p.max(0.0).round() as u64;
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

impl Ctrl {
    /// Moltiplicatore dei punti adesso (vedi prep_core::score).
    pub(super) fn score_now(&self, now: i64) -> prep_core::score::Score {
        use prep_core::group::State;
        if self.study_state != State::Studying {
            return prep_core::score::Score { mult: 1.0, fx: 0, level: 0.0 };
        }
        let today = self.study_log.combined().day(Local::now().date_naive())
            + self.study_since.map(|t| (now - t).max(0) as u64).unwrap_or(0);
        self.run.score(now, self.focus_on, self.racing_others, today)
    }
}
