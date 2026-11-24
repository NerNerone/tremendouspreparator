//! Feedback sui contenuti (scritto nel repository dei progressi).

use super::*;

impl Ctrl {
    // ---------- feedback ----------

    pub(super) fn session_context(&self, s: &Session) -> String {
        let sname = s.subject.as_deref().and_then(|x| self.content.subject(x)).map(|x| x.name.clone());
        let mut c = s.commit_message(sname.as_deref());
        if let Some(u) = s.unit.as_deref().and_then(|u| self.content.unit(u)) {
            c.push_str(&format!(" · U{} {}", u.n, u.name));
        }
        c
    }

    pub(super) fn save_feedback(&mut self, fb: Feedback) {
        match self.store.record_feedback(&fb) {
            Ok(()) => {
                self.refresh_feedback();
                self.start_sync(false);
            }
            Err(e) => self.toast(format!("Errore salvataggio feedback: {e}")),
        }
    }

    pub(super) fn send_session_feedback(&mut self) {
        let Some(s) = self.results.clone() else {
            return;
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let rating = st.get_r_rating();
        let text = st.get_r_feedback().trim().to_string();
        if rating == 0 && text.is_empty() {
            return;
        }
        let now = Utc::now();
        let fb = Feedback {
            schema: 1,
            id: Feedback::new_id(now),
            created_at: now,
            kind: "session".into(),
            category: match s.kind {
                SessionKind::Exam => "simulazione".into(),
                SessionKind::Review => "ripasso".into(),
                _ => "quiz".into(),
            },
            session: Some(s.id.clone()),
            context: self.session_context(&s),
            item: None,
            rating: (rating > 0).then_some(rating.clamp(1, 5) as u8),
            text,
            app_version: APP_VERSION.into(),
        };
        st.set_r_feedback_sent(true);
        self.save_feedback(fb);
        self.toast("Grazie! Feedback salvato e inviato al repository");
    }

    pub(super) fn send_feedback(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let text = st.get_fb_text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let now = Utc::now();
        let item = self.fb_item.clone();
        let fb = Feedback {
            schema: 1,
            id: Feedback::new_id(now),
            created_at: now,
            kind: if item.is_some() { "question".into() } else { "general".into() },
            category: st.get_fb_category().to_string(),
            session: self.results.as_ref().map(|s| s.id.clone()).filter(|_| item.is_some()),
            context: st.get_fb_item_text().to_string(),
            item,
            rating: None,
            text,
            app_version: APP_VERSION.into(),
        };
        st.set_fb_text(ss(""));
        self.fb_item = None;
        st.set_fb_item(ss(""));
        st.set_fb_item_text(ss(""));
        self.save_feedback(fb);
        self.toast("Feedback inviato: lo leggerà Claude alla prossima revisione");
    }

    pub(super) fn report_result(&mut self, k: usize) {
        let Some(id) = self.results.as_ref().and_then(|s| s.items.get(k)).map(|i| i.id.clone()) else {
            return;
        };
        let label = match self.content.question(&id) {
            Some(q) => format!("{id} — {}", q.text.chars().take(90).collect::<String>()),
            None => id.clone(),
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_fb_category(ss("domanda"));
        st.set_fb_item(ss(&id));
        st.set_fb_item_text(ss(label));
        self.fb_item = Some(id);
        // la pagina risultati resta: il tasto indietro ci ritorna
        st.set_page(P_FEEDBACK);
    }

    pub(super) fn refresh_feedback(&mut self) {
        let status = self.store.feedback_status();
        let list: Vec<FeedbackVM> = self
            .store
            .feedbacks()
            .into_iter()
            .take(50)
            .map(|f| {
                let pending = self.store.is_pending(&f.repo_path());
                let st = status.items.get(&f.id);
                let title = match f.kind.as_str() {
                    "session" => format!("Fine {}", f.category),
                    "question" => format!("Domanda {}", f.item.clone().unwrap_or_default()),
                    _ => f.category.clone(),
                };
                FeedbackVM {
                    date: ss(f.created_at.with_timezone(&Local).format("%d/%m %H:%M").to_string()),
                    title: ss(if f.context.is_empty() || f.kind == "question" {
                        title
                    } else {
                        format!("{title} · {}", f.context)
                    }),
                    text: ss(&f.text),
                    rating: f.rating.unwrap_or(0) as i32,
                    status: ss(match (pending, st) {
                        (true, _) => "in coda".to_string(),
                        (false, Some(s)) => s.status.clone(),
                        (false, None) => "inviato".to_string(),
                    }),
                    reply: ss(st.map(|s| s.reply.clone()).unwrap_or_default()),
                    pending,
                }
            })
            .collect::<Vec<_>>();
        // come una chat: dal più vecchio al più recente, in basso vicino al campo di testo
        let mut list = list;
        list.reverse();
        self.ui().global::<AppState>().set_feedbacks(model(list));
    }
}
