//! Flashcard: mazzi, ripasso dilazionato, valutazione.

use super::*;

impl Ctrl {
    // ---------- flashcard ----------

    pub(super) fn start_cards(&mut self, kind: &str, id: &str) {
        // «learned-…»: le carte già studiate e non in scadenza (ripasso libero)
        let (learned, kind) = match kind.strip_prefix("learned-") {
            Some(k) => (true, k),
            None => (false, kind),
        };
        let scope = Self::scope_of(kind, id);
        let ids = if kind == "flagged" {
            let mut v = self.flagged_cards();
            v.truncate(CARDS_MAX);
            v
        } else if learned {
            select::learned_cards(&self.content, &self.history, &scope, Utc::now(), CARDS_MAX)
        } else {
            select::flashcards(&self.content, &self.history, &scope, Utc::now(), CARDS_MAX, CARDS_NEW)
        };
        if ids.is_empty() {
            self.toast(if learned {
                "Nessuna flashcard già studiata qui"
            } else {
                "Nessuna flashcard da ripassare ora"
            });
            return;
        }
        let subject = match &scope {
            Scope::Subject(s) => Some(s.clone()),
            Scope::Unit(u) => self
                .content
                .unit(u)
                .and_then(|u| u.topics.first())
                .and_then(|t| self.content.subject_of_topic(&t.id))
                .map(|s| s.id.clone()),
            _ => None,
        };
        self.cards = Some(CardRun {
            subject,
            queue: ids,
            requeued: BTreeMap::new(),
            idx: 0,
            flipped: false,
            unsure: false,
            items: vec![],
            started: Utc::now(),
            shown_at: Instant::now(),
            saved: false,
        });
        self.render_card();
        self.set_page(P_CARDS);
    }

    pub(super) fn render_card(&mut self) {
        let Some(r) = &self.cards else { return };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let done = r.idx >= r.queue.len();
        st.set_c_done(done);
        st.set_c_total(r.queue.len() as i32);
        st.set_c_index(r.idx.min(r.queue.len()) as i32);
        if done {
            let unsure = r.items.iter().filter(|i| i.unsure).count();
            let good = r.items.iter().filter(|i| i.grade.unwrap_or(0) >= 2).count();
            st.set_c_summary(ss(format!(
                "Ripassate {} carte\n{} sapute · {} insicure\nRisultati salvati.",
                r.items.len(),
                good,
                unsure
            )));
            return;
        }
        let Some(card) = self.content.card(&r.queue[r.idx]) else {
            return;
        };
        st.set_c_front(tx(&card.front));
        st.set_c_back(tx(&card.back));
        st.set_c_topic(ss(self.content.topic(&card.topic).map(|t| t.name.clone()).unwrap_or_default()));
        st.set_c_ref(ss(self
            .content
            .resolve_ref(card.book_ref.as_ref(), &card.topic)
            .map(|x| self.content.ref_label(&x))
            .unwrap_or_default()));
        st.set_c_flipped(r.flipped);
        st.set_c_unsure(r.unsure);
    }

    pub(super) fn card_flip(&mut self) {
        if let Some(r) = &mut self.cards {
            r.flipped = !r.flipped;
            let ui = self.ui.upgrade().expect("finestra");
            let st = ui.global::<AppState>();
            st.set_c_note_shown(if r.flipped { st.get_c_note() } else { ss("") });
        }
        self.render_card();
    }

    pub(super) fn card_toggle_unsure(&mut self) {
        if let Some(r) = &mut self.cards {
            r.unsure = !r.unsure;
        }
        self.render_card();
    }

    pub(super) fn card_grade(&mut self, g: i32) {
        let Some(r) = &mut self.cards else { return };
        if r.idx >= r.queue.len() {
            return;
        }
        let id = r.queue[r.idx].clone();
        let topic = self.content.card(&id).map(|c| c.topic.clone()).unwrap_or_default();
        let secs = Instant::now().duration_since(r.shown_at).as_secs() as u32;
        r.items.push(Item {
            id: id.clone(),
            topic,
            kind: "card".into(),
            given: None,
            correct: None,
            unsure: r.unsure,
            grade: Some(g.clamp(0, 3) as u8),
            flag: None,
            secs,
            note: {
                let ui = self.ui.upgrade().expect("finestra");
                let st = ui.global::<AppState>();
                let n = st.get_c_note().trim().to_string();
                st.set_c_note(ss(""));
                st.set_c_note_shown(ss(""));
                (!n.is_empty()).then_some(n)
            },
        });
        if g == 0 {
            let n = r.requeued.entry(id.clone()).or_insert(0);
            if *n < 2 {
                *n += 1;
                r.queue.push(id);
            }
        }
        r.idx += 1;
        r.flipped = false;
        r.unsure = false;
        r.shown_at = Instant::now();
        let done = r.idx >= r.queue.len();
        if done {
            self.save_cards();
        }
        self.render_card();
    }

    pub(super) fn save_cards(&mut self) {
        let Some(r) = &mut self.cards else { return };
        if r.saved || r.items.is_empty() {
            return;
        }
        r.saved = true;
        let s = Session {
            schema: 1,
            id: Session::new_id(SessionKind::Flashcards, r.subject.as_deref(), r.started),
            kind: SessionKind::Flashcards,
            subject: r.subject.clone(),
            unit: None,
            started_at: r.started,
            ended_at: Utc::now(),
            app_version: APP_VERSION.into(),
            device: device_name(),
            items: r.items.clone(),
            score: None,
        };
        if let Err(e) = self.store.record_session(&s) {
            self.toast(format!("Errore salvataggio: {e}"));
        }
        self.reload_progress();
        self.refresh_all();
        // gli altri propri dispositivi collegati ricevono subito la sessione
        self.p2p_announce();
    }

    pub(super) fn cards_close(&mut self) {
        self.save_cards();
        self.cards = None;
        self.set_page(self.tab_return());
    }
}
