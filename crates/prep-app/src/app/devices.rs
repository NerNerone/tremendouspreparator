//! Sincronizzazione tra i propri dispositivi (relay, WebRTC, Bluetooth) e condivisione di un esame.

use super::*;

impl Ctrl {
    // ---------- sincronizzazione tra dispositivi ----------

    pub(super) fn exam_store(&self, id: &str) -> Store {
        Store { root: prep_core::exams::dir(&self.store.base, id), base: self.store.base.clone() }
    }

    pub(super) fn device_label() -> &'static str {
        if cfg!(target_os = "android") {
            "Android"
        } else if cfg!(target_arch = "wasm32") {
            "web"
        } else {
            "computer"
        }
    }

    pub(super) fn sess(&self, k: Sess) -> Option<&P2p> {
        match k {
            Sess::Sync => self.p2p.as_ref(),
            Sess::Share => self.share.as_ref(),
            Sess::Group => self.group.as_ref(),
        }
    }

    pub(super) fn sess_mut(&mut self, k: Sess) -> Option<&mut P2p> {
        match k {
            Sess::Sync => self.p2p.as_mut(),
            Sess::Share => self.share.as_mut(),
            Sess::Group => self.group.as_mut(),
        }
    }

    /// Relay scelti nelle Opzioni (salvati), o quelli predefiniti.
    pub(super) fn relays(&mut self) -> Vec<String> {
        let ui = self.ui();
        let mut s = self.store.settings();
        s.relays = ui
            .global::<AppState>()
            .get_p2p_relays()
            .split([',', '\n', ' '])
            .map(|x| x.trim().to_string())
            .filter(|x| x.starts_with("ws://") || x.starts_with("wss://"))
            .collect();
        let _ = self.store.save_settings(&s);
        if s.relays.is_empty() {
            p2p::DEFAULT_RELAYS.iter().map(|x| x.to_string()).collect()
        } else {
            s.relays
        }
    }

    pub(super) fn device_id(&mut self) -> String {
        let mut s = self.store.settings();
        if s.device_id.is_empty() {
            s.device_id = p2p::random_id();
            let _ = self.store.save_settings(&s);
        }
        s.device_id
    }

    /// Apre una sessione: collegamenti ai relay e motore.
    pub(super) fn open_session(
        &mut self,
        k: Sess,
        room: p2p::Room,
        engine: p2p::Engine,
        code: String,
        sending: String,
    ) {
        self.close_session(k);
        let relays = self.relays();
        let base = k as usize * LINKS_PER_SESS;
        let links: Vec<crate::net::Link> =
            relays.iter().take(LINKS_PER_SESS).enumerate().map(|(i, u)| crate::net::Link::open(base + i, u)).collect();
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, Duration::from_millis(160), move || with_ctrl(|c| c.p2p_pump(k)));
        let sess = P2p {
            room,
            signer: p2p::Signer::new(),
            engine: Some(engine),
            open: vec![false; links.len()],
            subscribed: vec![false; links.len()],
            links,
            relays,
            started: false,
            queue: Default::default(),
            timer,
            ticks: 0,
            dirty: false,
            received: 0,
            notice: String::new(),
            code,
            sending,
            direct: BTreeMap::new(),
            failed: BTreeSet::new(),
            rtc_seq: 0,
        };
        match k {
            Sess::Sync => self.p2p = Some(sess),
            Sess::Share => self.share = Some(sess),
            Sess::Group => self.group = Some(sess),
        }
        self.refresh_sessions();
    }

    pub(super) fn close_session(&mut self, k: Sess) {
        let old = match k {
            Sess::Sync => self.p2p.take(),
            Sess::Share => self.share.take(),
            Sess::Group => self.group.take(),
        };
        if let Some(p) = old {
            p.timer.stop();
            for l in &p.links {
                l.close();
            }
            for d in p.direct.values() {
                d.conn.close();
            }
        }
        self.refresh_sessions();
    }

    pub(super) fn p2p_connect(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let password = st.get_p2p_password().to_string();
        let mut s = self.store.settings();
        if s.username.trim().is_empty() {
            self.toast("Scegli prima un nome utente qui sopra, in Profilo");
            return;
        }
        if password.chars().count() < 8 {
            self.toast("Serve una password di almeno 8 caratteri (meglio una frase lunga)");
            return;
        }
        s.p2p_auto = st.get_p2p_auto();
        s.direct_only = st.get_p2p_direct_only();
        s.p2p_password = if s.p2p_auto { password.clone() } else { String::new() };
        let _ = self.store.save_settings(&s);
        let device = self.device_id();
        // anche se la password non resta salvata: stessi nome e password = stesso utente nei gruppi
        let derived = prep_core::group::user_id(&s.username, &password, &device);
        self.adopt_user_id(derived);
        let room = p2p::Room::derive(&s.username, &password);
        let mut engine = p2p::Engine::new(
            &device,
            &format!("{} · {}", s.username.trim(), Self::device_label()),
            Self::device_label(),
        );
        engine.profile = true;
        self.open_session(Sess::Sync, room, engine, String::new(), String::new());
    }

    pub(super) fn p2p_disconnect(&mut self) {
        self.close_session(Sess::Sync);
    }

    pub(super) fn display_name(&self) -> String {
        let n = self.store.settings().username.trim().to_string();
        if n.is_empty() {
            format!("Utente ({})", Self::device_label())
        } else {
            n
        }
    }

    /// Condivide l'esame attivo con un'altra persona: genera il codice da comunicarle.
    pub(super) fn share_start(&mut self) {
        let ui = self.ui();
        if !ui.global::<AppState>().get_share_ok() {
            self.toast("Conferma prima di avere il diritto di condividere questo materiale");
            return;
        }
        let id = self.exams.active.clone();
        let st = self.exam_store(&id);
        if !st.mirror_path("content/syllabus.json").is_some_and(|p| vfs::exists(&p)) {
            self.toast("Questo esame usa i contenuti inclusi nell'app: non c'è un pacchetto da condividere");
            return;
        }
        let name = self.exams.get(&id).map(|e| e.name.clone()).unwrap_or_default();
        let code = p2p::share_code();
        let engine =
            p2p::Engine::new(&p2p::random_id(), &self.display_name(), Self::device_label()).share_only(vec![id]);
        self.open_session(Sess::Share, p2p::share_room(&code), engine, code, name);
    }

    /// Entra nella condivisione di un'altra persona col codice che ha comunicato.
    pub(super) fn share_join(&mut self) {
        let ui = self.ui();
        let code = ui.global::<AppState>().get_share_input().to_string();
        if p2p::normalize_code(&code).len() != 12 {
            self.toast("Il codice ha 12 caratteri, per esempio K7QM-4TZP-9WXR");
            return;
        }
        let engine = p2p::Engine::new(&p2p::random_id(), &self.display_name(), Self::device_label()).share_only(vec![]);
        self.open_session(Sess::Share, p2p::share_room(&code), engine, String::new(), String::new());
    }

    pub(super) fn now_secs() -> i64 {
        Utc::now().timestamp()
    }

    /// Spedisce i messaggi prodotti dal motore: per via diretta se c'è un collegamento con
    /// il destinatario (o sta per aprirsi), altrimenti cifrati e firmati attraverso i relay.
    pub(super) fn p2p_enqueue(&mut self, k: Sess, outs: Vec<p2p::Out>) {
        let direct_only = self.store.settings().direct_only;
        let Some(p) = self.sess_mut(k) else { return };
        let Some(eng) = p.engine.as_ref() else { return };
        let now = Self::now_secs();
        for o in outs {
            let wire = eng.wire(&o);
            if o.stored {
                // avvisi del gruppo: conservati dai relay per chi non è collegato ora
                let content = p.room.seal(&wire);
                p.queue.push_back(p.signer.event_kind(p2p::KIND_STORED, &p.room.tag, &content, now));
                continue;
            }
            // presenza, appuntamenti e impostazioni del gruppo passano sempre dai relay (così non
            // dipendono dai collegamenti diretti); foto e richieste di foto vanno dirette se c'è
            // il collegamento
            let control = matches!(o.msg, p2p::Msg::Hello { .. } | p2p::Msg::Rtc { .. } | p2p::Msg::Group(_))
                || matches!(&o.msg, p2p::Msg::GroupMsg(m) if !matches!(m, prep_core::group::GroupMsg::PhotoGet { .. }));
            if let (Some(to), false) = (o.to.as_deref(), control) {
                if let Some(d) = p.direct.get_mut(to) {
                    if d.open {
                        d.conn.send(wire);
                    } else {
                        d.pending.push_back(wire);
                    }
                    continue;
                }
                if direct_only {
                    // nessun collegamento diretto con questo dispositivo: niente relay
                    continue;
                }
            } else if o.to.is_none() && direct_only && !control {
                continue;
            }
            let content = p.room.seal(&wire);
            p.queue.push_back(p.signer.event(&p.room.tag, &content, now));
        }
    }

    /// Apre (o riapre) il collegamento diretto con un dispositivo.
    pub(super) fn direct_open(&mut self, k: Sess, peer: &str, initiator: bool) {
        // solo nelle build di sviluppo: simula reti dove il collegamento diretto non riesce
        if cfg!(debug_assertions) && std::env::var("PREP_NO_DIRECT").is_ok() {
            return;
        }
        let Some(p) = self.sess_mut(k) else { return };
        if let Some(old) = p.direct.remove(peer) {
            old.conn.close();
        }
        p.failed.remove(peer);
        let conn = crate::direct::Conn::open(k as usize, peer, initiator);
        p.direct.insert(peer.to_string(), Direct { conn, open: false, pending: Default::default() });
    }

    pub(super) fn direct_event(&mut self, k: Sess, peer: String, ev: crate::direct::DirectEvent) {
        use crate::direct::DirectEvent;
        match ev {
            DirectEvent::Signal(kind, sdp) => {
                self.with_engine(k, |e, _| vec![e.signal(&peer, &kind, &sdp)]);
            }
            DirectEvent::Open => {
                if let Some(d) = self.sess_mut(k).and_then(|p| p.direct.get_mut(&peer)) {
                    d.open = true;
                    for b in d.pending.drain(..) {
                        d.conn.send(b);
                    }
                }
                self.refresh_sessions();
            }
            DirectEvent::Data(bytes) => {
                let Some(p) = self.sess_mut(k) else { return };
                p.rtc_seq += 1;
                let id = format!("rtc:{}", p.rtc_seq);
                let now = Self::now_secs();
                self.with_engine(k, |e, c| e.receive(c, &id, &bytes, now));
            }
            DirectEvent::Closed(why) => {
                let direct_only = self.store.settings().direct_only;
                let Some(p) = self.sess_mut(k) else { return };
                let Some(d) = p.direct.remove(&peer) else { return };
                if !why.is_empty() {
                    p.failed.insert(peer.clone());
                }
                // i messaggi rimasti in attesa passano dai relay (se permesso)
                if !direct_only {
                    let now = Self::now_secs();
                    for wire in d.pending {
                        let content = p.room.seal(&wire);
                        p.queue.push_back(p.signer.event(&p.room.tag, &content, now));
                    }
                }
                self.refresh_sessions();
            }
        }
    }

    /// Esegue `f` col motore e il controller (che fa da `Host`), poi spedisce quanto prodotto.
    pub(super) fn with_engine(&mut self, k: Sess, f: impl FnOnce(&mut p2p::Engine, &mut Ctrl) -> Vec<p2p::Out>) {
        let Some(mut eng) = self.sess_mut(k).and_then(|p| p.engine.take()) else { return };
        let outs = f(&mut eng, self);
        let events = std::mem::take(&mut eng.events);
        if let Some(p) = self.sess_mut(k) {
            p.engine = Some(eng);
        }
        self.p2p_enqueue(k, outs);
        self.p2p_events(k, events);
    }

    pub(super) fn p2p_announce(&mut self) {
        if self.p2p.as_ref().is_some_and(|p| p.started) {
            self.with_engine(Sess::Sync, |e, c| e.announce(c));
        }
    }

    /// Ogni 160 ms: un messaggio della coda a tutti i relay aperti; ogni secondo il motore.
    pub(super) fn p2p_pump(&mut self, k: Sess) {
        let bt_on = self.bt.connected.is_some();
        let Some(p) = self.sess_mut(k) else { return };
        p.ticks += 1;
        if p.open.iter().any(|o| *o) || bt_on {
            if let Some(f) = p.queue.pop_front() {
                for (l, o) in p.links.iter().zip(&p.open) {
                    if *o {
                        l.send(&f);
                    }
                }
                // il telefono vicino riceve gli stessi messaggi dei relay
                if bt_on {
                    crate::bt_send(f.as_bytes());
                }
            }
        }
        if bt_on && !p.started {
            p.started = true;
            let now = Self::now_secs();
            self.with_engine(k, |e, c| e.start(c, now));
        }
        let Some(p) = self.sess_mut(k) else { return };
        if p.ticks.is_multiple_of(6) {
            let started = p.started;
            let dirty = std::mem::take(&mut p.dirty);
            if started {
                let now = Self::now_secs();
                self.with_engine(k, |e, c| e.tick(c, now));
            }
            if dirty {
                self.reload_progress();
                self.refresh_all();
            }
            self.refresh_sessions();
        }
    }

    pub(super) fn net_event(&mut self, ev: crate::net::NetEvent) {
        use crate::net::NetEvent;
        let idx = match &ev {
            NetEvent::Open(i) | NetEvent::Frame(i, _) | NetEvent::Closed(i, _) => *i,
        };
        let k = Sess::from_index(idx / LINKS_PER_SESS);
        let i = idx % LINKS_PER_SESS;
        let Some(p) = self.sess_mut(k) else { return };
        match ev {
            NetEvent::Open(_) => {
                if i < p.open.len() {
                    p.open[i] = true;
                    // iscrizione agli eventi della stanza (anche quelli di pochi secondi fa)
                    p.links[i].send(&p2p::subscribe("tp", &p.room.tag, Self::now_secs() - 30));
                    p.subscribed[i] = true;
                }
                if !p.started {
                    p.started = true;
                    let now = Self::now_secs();
                    self.with_engine(k, |e, c| e.start(c, now));
                }
                self.refresh_sessions();
            }
            NetEvent::Closed(_, why) => {
                if i < p.open.len() {
                    p.open[i] = false;
                }
                if !why.is_empty() {
                    p.notice = format!("{}: {why}", p.relays.get(i).cloned().unwrap_or_default());
                }
                self.refresh_sessions();
            }
            NetEvent::Frame(_, text) => {
                if let Some((id, content)) = p2p::parse_relay(&text) {
                    if let Some(plain) = p.room.open(&content) {
                        let now = Self::now_secs();
                        self.with_engine(k, |e, c| e.receive(c, &id, &plain, now));
                    }
                } else if let Some(n) = p2p::relay_notice(&text) {
                    p.notice = n;
                }
            }
        }
    }

    pub(super) fn p2p_events(&mut self, k: Sess, events: Vec<p2p::Event>) {
        for ev in events {
            match ev {
                p2p::Event::Peer(_) => {}
                p2p::Event::Profile { from, profile } => self.profile_in(from, profile),
                p2p::Event::Group { from, status } => self.group_status_in(from, status),
                // tra i propri dispositivi passano solo le richieste di foto
                p2p::Event::GroupMsg { from, msg } if k != Sess::Group => {
                    if let prep_core::group::GroupMsg::PhotoGet { hash } = msg {
                        self.photo_send(k, from, hash);
                    }
                }
                p2p::Event::GroupMsg { from, msg } => self.group_msg_in(from, msg),
                p2p::Event::GroupPhoto { from, hash, bytes } => self.group_photo_in(from, hash, bytes),
                p2p::Event::Joined(peer) => {
                    if k == Sess::Group {
                        // chi arriva riceve subito il nostro stato (con la mappa di calore) e le
                        // impostazioni del gruppo
                        self.hist_sent = 0;
                        self.group_send();
                        self.group_send_info_as(true);
                        // nel gruppo, collegamenti diretti con al massimo GROUP_DIRECT membri
                        // (oltre bastano i relay)
                        if self.sess(k).is_some_and(|p| p.direct.len() >= GROUP_DIRECT) {
                            continue;
                        }
                    }
                    // collegamento diretto: lo apre chi ha l'id minore, l'altro aspetta l'offerta
                    let initiates = self.sess(k).and_then(|p| p.engine.as_ref()).is_some_and(|e| e.initiates(&peer));
                    if initiates {
                        self.direct_open(k, &peer, true);
                    }
                }
                p2p::Event::Signal { from, kind, sdp } => {
                    // solo nelle build di sviluppo: collegamento diretto tentato che non si apre mai
                    if cfg!(debug_assertions) && std::env::var("PREP_DROP_SIGNALS").is_ok() {
                        continue;
                    }
                    if kind == "offer" {
                        self.direct_open(k, &from, false);
                    }
                    if let Some(d) = self.sess(k).and_then(|p| p.direct.get(&from)) {
                        d.conn.signal(&kind, &sdp);
                    }
                }
                p2p::Event::Files { exam, n } => {
                    let active = exam == self.exams.active;
                    if let Some(p) = self.sess_mut(k) {
                        p.received += n;
                        p.dirty |= active;
                    }
                }
                p2p::Event::PackProgress { .. } => {}
                p2p::Event::PackDone { exam, result } => match result {
                    Ok(name) => {
                        self.refresh_exams();
                        if exam == self.exams.active {
                            self.reload_content();
                            self.reload_progress();
                            self.refresh_all();
                        }
                        self.toast(format!("Esame «{name}» ricevuto e installato"));
                    }
                    Err(e) => self.toast(format!("Esame non ricevuto: {e}")),
                },
            }
        }
    }

    pub(super) fn p2p_download(&mut self, k: Sess, peer: &str, exam: &str) {
        let now = Self::now_secs();
        let (peer, exam) = (peer.to_string(), exam.to_string());
        self.with_engine(k, move |e, _| e.download(&peer, &exam, now));
        self.refresh_sessions();
    }

    /// Stato e dispositivi trovati di una sessione, per l'interfaccia.
    pub(super) fn session_view(&self, k: Sess) -> Option<(String, Vec<PeerVM>)> {
        let p = self.sess(k)?;
        let open = p.open.iter().filter(|o| **o).count();
        let mut peers: Vec<PeerVM> = Vec::new();
        let who = if k == Sess::Sync { "dispositiv" } else { "person" };
        let mut status = if open == 0 {
            "Collegamento ai relay…".to_string()
        } else {
            format!("Collegato a {open}/{} relay", p.relays.len())
        };
        if let Some(eng) = p.engine.as_ref() {
            status.push_str(&match (eng.peers.len(), k) {
                (0, Sess::Sync) if open > 0 => " · in attesa degli altri dispositivi (apri l'app anche lì)".to_string(),
                (0, Sess::Share) if open > 0 && !p.sending.is_empty() => {
                    " · in attesa di chi deve ricevere (tieni aperta questa schermata)".to_string()
                }
                (0, Sess::Share) if open > 0 => " · in attesa di chi condivide".to_string(),
                (0, _) => String::new(),
                (1, _) => format!(" · 1 {who}{}", if k == Sess::Sync { "o" } else { "a" }),
                (n, _) => format!(" · {n} {who}{}", if k == Sess::Sync { "i" } else { "e" }),
            });
            if let Some((exam, got, total)) = eng.downloading() {
                let name = eng
                    .peers
                    .values()
                    .flat_map(|x| &x.exams)
                    .find(|e| e.id == exam)
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                status.push_str(&format!("\nRicevo «{name}»: {got}/{} pezzi", total.max(1)));
            }
            if !p.sending.is_empty() && p.queue.len() > 2 {
                status.push_str(&format!("\nInvio in corso: {} pezzi in coda", p.queue.len()));
            }
            for (id, peer) in &eng.peers {
                let mut offers: Vec<OfferVM> = Vec::new();
                for e in peer.exams.iter().filter(|e| e.shareable) {
                    let mine = self.exams.get(&e.id).map(|_| prep_core::pack::content_version(&self.exam_store(&e.id)));
                    let label = match mine {
                        None => format!("Scarica «{}»", e.name),
                        Some(v) if v != e.version => format!("Prendi i suoi contenuti di «{}»", e.name),
                        Some(_) => continue,
                    };
                    offers.push(OfferVM { peer: ss(id), exam: ss(&e.id), label: ss(label) });
                }
                let info = match k {
                    Sess::Group => String::new(),
                    Sess::Sync => match peer.exams.iter().filter(|e| self.exams.get(&e.id).is_some()).count() {
                        0 => "nessun esame in comune".to_string(),
                        1 => "1 esame in comune: progressi sincronizzati".to_string(),
                        n => format!("{n} esami in comune: progressi sincronizzati"),
                    },
                    Sess::Share if !p.sending.is_empty() => format!("collegata: può scaricare «{}»", p.sending),
                    Sess::Share => match peer.exams.first() {
                        Some(e) if offers.is_empty() => format!("condivide «{}»: hai già gli stessi contenuti", e.name),
                        Some(e) => format!("condivide «{}» con te", e.name),
                        None => "non sta condividendo nessun esame".to_string(),
                    },
                };
                let link = match p.direct.get(id) {
                    Some(d) if d.open => "collegamento diretto",
                    Some(_) => "collegamento diretto in apertura…",
                    None if p.failed.contains(id) && self.store.settings().direct_only => {
                        "collegamento diretto non riuscito (solo diretto attivo)"
                    }
                    None => "tramite relay",
                };
                peers.push(PeerVM {
                    id: ss(id),
                    name: ss(&peer.name),
                    info: ss(format!("{info} · {link}")),
                    offers: model(if k == Sess::Share && !p.sending.is_empty() { vec![] } else { offers }),
                });
            }
        }
        if p.received > 0 {
            status.push_str(&format!("\nRicevuti {} file di progressi", p.received));
        }
        if !p.notice.is_empty() && open < p.relays.len() {
            status.push_str(&format!("\n{}", p.notice));
        }
        Some((status, peers))
    }

    pub(super) fn refresh_sessions(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        match self.session_view(Sess::Sync) {
            Some((status, peers)) => {
                st.set_p2p_on(true);
                st.set_p2p_status(ss(status));
                st.set_p2p_peers(model(peers));
            }
            None => {
                st.set_p2p_on(false);
                st.set_p2p_status(ss("Non collegato"));
                st.set_p2p_peers(model(Vec::<PeerVM>::new()));
            }
        }
        let name = self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_default();
        st.set_share_exam(ss(name));
        match self.session_view(Sess::Share) {
            Some((status, peers)) => {
                let p = self.share.as_ref();
                st.set_share_on(true);
                st.set_share_sending(p.is_some_and(|p| !p.sending.is_empty()));
                st.set_share_code(ss(p.map(|p| p.code.clone()).unwrap_or_default()));
                st.set_share_status(ss(status));
                st.set_share_peers(model(peers));
            }
            None => {
                st.set_share_on(false);
                st.set_share_sending(false);
                st.set_share_code(ss(""));
                st.set_share_status(ss(""));
                st.set_share_peers(model(Vec::<PeerVM>::new()));
            }
        }
    }

    // ---------- Bluetooth (Android) ----------

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(super) fn bt_event(&mut self, kind: i32, text: String, data: Vec<u8>) {
        match kind {
            0 => {
                if let Some((name, addr)) = text.split_once('\n') {
                    if !self.bt.found.iter().any(|(_, a)| a == addr) {
                        self.bt.found.push((name.to_string(), addr.to_string()));
                    }
                }
            }
            1 => {
                self.bt.status = format!("Collegato via Bluetooth a {text}");
                self.bt.connected = Some(text);
                self.bt.found.clear();
                // ci si ripresenta in ogni sessione aperta, così l'altro telefono ci trova anche offline
                let now = Self::now_secs();
                for k in [Sess::Sync, Sess::Share, Sess::Group] {
                    if self.sess(k).is_some() {
                        if let Some(p) = self.sess_mut(k) {
                            p.started = true;
                        }
                        self.with_engine(k, |e, c| e.start(c, now));
                    }
                }
            }
            2 => {
                let Ok(frame) = String::from_utf8(data) else { return };
                let Some((id, tag, content)) = p2p::parse_client_event(&frame) else { return };
                for k in [Sess::Sync, Sess::Share, Sess::Group] {
                    let Some(p) = self.sess(k) else { continue };
                    if p.room.tag != tag {
                        continue;
                    }
                    if let Some(plain) = p.room.open(&content) {
                        let now = Self::now_secs();
                        self.with_engine(k, |e, c| e.receive(c, &id, &plain, now));
                    }
                }
            }
            3 => {
                self.bt.connected = None;
                self.bt.status = text;
            }
            _ => self.bt.status = text,
        }
        self.refresh_bt();
    }

    pub(super) fn refresh_bt(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_bt_connected(self.bt.connected.is_some());
        st.set_bt_status(ss(&self.bt.status));
        st.set_bt_devices(model(
            self.bt.found.iter().map(|(n, a)| BtDevVM { name: ss(n), address: ss(a) }).collect::<Vec<_>>(),
        ));
    }
}
