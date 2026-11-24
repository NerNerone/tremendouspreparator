//! Quiz e simulazioni: svolgimento, correzione, pausa e ripresa, segnalazioni.

use super::*;

impl Ctrl {
    // ---------- pausa / ripresa ----------

    pub(super) fn saved_quiz_path(&self) -> PathBuf {
        self.store.root.join("quiz_in_corso.json")
    }

    pub(super) fn autosave_quiz(&mut self) {
        let Some(q) = &mut self.quiz else { return };
        let now = Instant::now();
        // tempo della domanda corrente fino a ora, senza "consumarlo"
        let mut secs = q.secs.clone();
        if q.is_exam() || !q.checked[q.idx] {
            secs[q.idx] += now.duration_since(q.shown_at).as_secs_f64();
        }
        let saved = SavedQuiz {
            kind: q.kind,
            subject: q.subject.clone(),
            unit: q.unit.clone(),
            title: q.title.clone(),
            ids: q.questions.iter().map(|x| x.id.clone()).collect(),
            given: q.given.iter().map(encode_given).collect(),
            unsure: q.unsure.clone(),
            checked: q.checked.clone(),
            secs,
            idx: q.idx,
            started: q.started,
            remaining_secs: q.deadline.map(|d| d.saturating_duration_since(now).as_secs()),
            saved_at: Utc::now(),
            perm: q.perm.clone(),
        };
        if let Ok(j) = serde_json::to_string(&saved) {
            let _ = vfs::write(&self.saved_quiz_path(), j.as_bytes());
        }
        self.write_reminder();
    }

    pub(super) fn load_saved_quiz(&self) -> Option<SavedQuiz> {
        let t = vfs::read_to_string(&self.saved_quiz_path()).ok()?;
        serde_json::from_str(&t).ok()
    }

    pub(super) fn clear_saved_quiz(&mut self) {
        let _ = vfs::remove_file(&self.saved_quiz_path());
        self.write_reminder();
    }

    pub(super) fn pause_quiz(&mut self) {
        if self.quiz.is_none() {
            return;
        }
        self.close_dialog();
        self.autosave_quiz();
        self.quiz = None;
        self.tick.stop();
        self.refresh_home();
        self.set_page(self.tab_return());
    }

    /// L'app passa in secondo piano durante un quiz: pausa automatica (si riprende dalla barra
    /// della sessione). Solo su Android: sul computer una finestra perde il fuoco di continuo.
    pub(super) fn app_hidden(&mut self) {
        if cfg!(target_os = "android") && self.quiz.is_some() {
            self.pause_quiz();
        }
    }

    pub(super) fn resume_quiz(&mut self) {
        let Some(sq) = self.load_saved_quiz() else {
            self.refresh_home();
            return;
        };
        let mut questions = Vec::new();
        let mut given = Vec::new();
        let mut unsure = Vec::new();
        let mut checked = Vec::new();
        let mut secs = Vec::new();
        let mut idx = 0;
        for (k, id) in sq.ids.iter().enumerate() {
            if let Some(q) = self.content.question(id) {
                if k == sq.idx {
                    idx = questions.len();
                }
                questions.push(q.clone());
                given.push(decode_given(&sq.given[k]));
                unsure.push(sq.unsure[k]);
                checked.push(sq.checked[k]);
                secs.push(sq.secs[k]);
            }
        }
        if questions.is_empty() {
            self.clear_saved_quiz();
            self.refresh_home();
            self.toast("Le domande del quiz in pausa non ci sono più");
            return;
        }
        let perm: Vec<Vec<usize>> = questions
            .iter()
            .enumerate()
            .map(|(i, q)| match sq.perm.get(i) {
                Some(p) if p.len() == q.options.len() => p.clone(),
                _ => (0..q.options.len()).collect(),
            })
            .collect();
        self.quiz = Some(Quiz {
            kind: sq.kind,
            subject: sq.subject,
            unit: sq.unit,
            title: sq.title,
            questions,
            perm,
            given,
            unsure,
            checked,
            secs,
            idx,
            started: sq.started,
            shown_at: Instant::now(),
            deadline: sq.remaining_secs.map(|r| Instant::now() + Duration::from_secs(r)),
        });
        self.tick.start(TimerMode::Repeated, Duration::from_millis(500), || {
            with_ctrl(|c| c.on_tick());
        });
        self.render_question();
        self.set_page(P_QUIZ);
    }

    pub(super) fn discard_paused(&mut self) {
        self.clear_saved_quiz();
        self.refresh_home();
        self.toast("Quiz in pausa scartato");
    }

    pub(super) fn refresh_paused(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        match (self.quiz.is_none(), self.load_saved_quiz()) {
            (true, Some(sq)) => {
                let answered = sq.given.iter().filter(|g| g.is_some()).count();
                let mut info = format!(
                    "Domanda {} di {} · {} risposte · in pausa dal {}",
                    sq.idx + 1,
                    sq.ids.len(),
                    answered,
                    sq.saved_at.with_timezone(&Local).format("%d/%m %H:%M")
                );
                if let Some(r) = sq.remaining_secs {
                    info.push_str(&format!(" · {}:{:02} rimanenti", r / 60, r % 60));
                }
                st.set_paused_title(ss(&sq.title));
                st.set_paused_info(ss(info));
                st.set_paused_n((sq.idx + 1) as i32);
                st.set_paused_frac(answered as f32 / sq.ids.len().max(1) as f32);
                let mut short = format!("{} di {} · {} risposte", sq.idx + 1, sq.ids.len(), answered);
                if let Some(r) = sq.remaining_secs {
                    short.push_str(&format!(" · {}:{:02}", r / 60, r % 60));
                }
                st.set_paused_short(ss(short));
            }
            _ => {
                st.set_paused_title(ss(""));
                st.set_paused_info(ss(""));
                st.set_paused_short(ss(""));
            }
        }
    }

    // ---------- quiz ----------

    pub(super) fn scope_of(kind: &str, id: &str) -> Scope {
        match kind {
            "subject" => Scope::Subject(id.into()),
            "unit" => Scope::Unit(id.into()),
            "topic" => Scope::Topic(id.into()),
            _ => Scope::All,
        }
    }

    pub(super) fn start_quiz(
        &mut self,
        kind: SessionKind,
        ids: Vec<String>,
        title: String,
        subject: Option<String>,
        unit: Option<String>,
    ) {
        if ids.is_empty() {
            self.toast("Nessuna domanda disponibile qui (contenuti in arrivo)");
            return;
        }
        if self.load_saved_quiz().is_some() {
            self.clear_saved_quiz();
            self.toast("Il quiz che avevi in pausa è stato sostituito da questo");
        }
        let questions: Vec<Question> = ids.iter().filter_map(|id| self.content.question(id).cloned()).collect();
        let n = questions.len();
        let minutes = self.content.syllabus().exam.minutes as u64;
        let deadline = (kind == SessionKind::Exam).then(|| Instant::now() + Duration::from_secs(minutes * 60));
        // opzioni mescolate a ogni presentazione: la risposta giusta non è mai "sempre la A"
        let perm = questions.iter().map(|q| prep_core::model::display_order(&q.options)).collect();
        self.quiz = Some(Quiz {
            kind,
            subject,
            unit,
            title,
            questions,
            perm,
            given: vec![Given::None; n],
            unsure: vec![false; n],
            checked: vec![false; n],
            secs: vec![0.0; n],
            idx: 0,
            started: Utc::now(),
            shown_at: Instant::now(),
            deadline,
        });
        // cronometro: per domanda in tutte le modalità, tempo residuo in simulazione
        self.tick.start(TimerMode::Repeated, Duration::from_millis(500), || {
            with_ctrl(|c| c.on_tick());
        });
        self.render_question();
        self.set_page(P_QUIZ);
    }

    pub(super) fn on_tick(&mut self) {
        if self.quiz.is_none() {
            self.tick.stop();
            return;
        }
        self.update_elapsed();
        if self.last_save.elapsed() > Duration::from_secs(10) {
            self.autosave_quiz();
            self.last_save = Instant::now();
        }
        let Some(q) = &self.quiz else { return };
        let Some(dl) = q.deadline else { return };
        let left = dl.saturating_duration_since(Instant::now()).as_secs();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_time_left(ss(format!("{:02}:{:02}", left / 60, left % 60)));
        st.set_time_warning(left < 300);
        if left == 0 {
            self.tick.stop();
            self.close_dialog();
            self.finish_quiz();
            self.toast("Tempo scaduto: simulazione consegnata");
        }
    }

    /// tempo trascorso sulla domanda corrente (fermo dopo la conferma in allenamento)
    pub(super) fn update_elapsed(&mut self) {
        let Some(q) = &self.quiz else { return };
        let i = q.idx;
        let running = q.is_exam() || !q.checked[i];
        let secs = q.secs[i] + if running { Instant::now().duration_since(q.shown_at).as_secs_f64() } else { 0.0 };
        let pace = exam_pace(self.content.syllabus().exam.minutes, self.content.syllabus().exam.questions);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_q_elapsed(ss(mmss(secs)));
        st.set_q_slow(secs > pace);
    }

    pub(super) fn render_question(&mut self) {
        let Some(q) = &self.quiz else { return };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let i = q.idx;
        let question = &q.questions[i];
        let exam = q.is_exam();
        let answered = q.checked[i] && !exam;
        st.set_quiz_mode(match q.kind {
            SessionKind::Exam => 1,
            SessionKind::Review => 2,
            _ => 0,
        });
        st.set_quiz_title(ss(&q.title));
        st.set_q_index(i as i32);
        st.set_q_total(q.questions.len() as i32);
        st.set_q_text(tx(&question.text));
        st.set_q_topic(ss(self.where_of(&question.topic)));
        st.set_q_is_mcq(question.kind == QKind::Mcq);
        let correct = is_correct(question, &q.given[i]);
        let perm = &q.perm[i];
        // dopo la risposta la spiegazione va dentro i riquadri: perché è giusta / perché no
        let (general, notes) = prep_core::model::option_notes(&question.explanation, &question.options);
        let general = prep_core::model::remap_letters(&general, perm);
        let trivial = general.starts_with("Risposta corretta: «");
        let inline = answered && question.kind == QKind::Mcq;
        let opts: Vec<OptionVM> = perm
            .iter()
            .enumerate()
            .filter_map(|(d, &k)| question.options.get(k).map(|o| (d, k, o)))
            .map(|(d, k, o)| {
                let selected = q.given[i] == Given::Option(k);
                let state = if answered {
                    if question.answer == Some(k) {
                        2
                    } else if selected {
                        3
                    } else {
                        4
                    }
                } else if selected {
                    1
                } else {
                    0
                };
                let note = if !inline {
                    String::new()
                } else if question.answer == Some(k) {
                    if trivial {
                        String::new()
                    } else {
                        general.clone()
                    }
                } else {
                    prep_core::model::remap_letters(notes.get(k).map(|x| x.as_str()).unwrap_or(""), perm)
                };
                OptionVM { letter: ss(prep_core::model::letter(d).to_string()), text: tx(o), state, note: tx(&note) }
            })
            .collect();
        st.set_q_options(model(opts));
        st.set_q_fill(ss(match &q.given[i] {
            Given::Text(t) => t.clone(),
            _ => String::new(),
        }));
        st.set_q_unsure(q.unsure[i]);
        st.set_q_answered(answered);
        st.set_q_correct(correct);
        let correct_text = match (question.kind, question.answer) {
            (QKind::Mcq, Some(a)) => match perm.iter().position(|&k| k == a) {
                Some(d) => format!(
                    "{}) {}",
                    prep_core::model::letter(d),
                    question.options.get(a).map(|x| x.as_str()).unwrap_or("")
                ),
                None => question.correct_text(),
            },
            _ => question.correct_text(),
        };
        st.set_q_correct_text(tx(&correct_text));
        st.set_q_explanation(tx(&if inline {
            String::new()
        } else {
            prep_core::model::remap_explanation(&question.explanation, perm, &question.options)
        }));
        st.set_q_source(ss(&question.source));
        st.set_q_ref(ss(self
            .content
            .resolve_ref(question.book_ref.as_ref(), &question.topic)
            .map(|r| self.content.ref_label(&r))
            .unwrap_or_default()));
        st.set_q_can_confirm(!q.given[i].is_none());
        let answered_count = q.given.iter().filter(|g| !g.is_none()).count();
        st.set_answered_count(answered_count as i32);
        if exam {
            let cells: Vec<NavCellVM> = (0..q.questions.len())
                .map(|k| NavCellVM {
                    n: k as i32 + 1,
                    answered: !q.given[k].is_none(),
                    unsure: q.unsure[k],
                    current: k == i,
                })
                .collect();
            let rows: Vec<NavRowVM> = cells.chunks(8).map(|c| NavRowVM { cells: model(c.to_vec()) }).collect();
            st.set_nav_rows(model(rows));
        }
        self.update_elapsed();
        self.autosave_quiz();
        self.last_save = Instant::now();
    }

    pub(super) fn select_option(&mut self, k: i32) {
        let Some(q) = &mut self.quiz else { return };
        let i = q.idx;
        if q.checked[i] && !q.is_exam() {
            return;
        }
        // posizione mostrata → indice originale (lo storico usa sempre l'ordine originale)
        let k = q.perm.get(i).and_then(|p| p.get(k as usize)).copied().unwrap_or(k as usize);
        q.given[i] = if q.given[i] == Given::Option(k) && q.is_exam() { Given::None } else { Given::Option(k) };
        self.render_question();
    }

    pub(super) fn fill_edited(&mut self, text: String) {
        let Some(q) = &mut self.quiz else { return };
        let i = q.idx;
        if q.checked[i] && !q.is_exam() {
            return;
        }
        q.given[i] = Given::Text(text);
        let can = !q.given[i].is_none();
        let count = q.given.iter().filter(|g| !g.is_none()).count() as i32;
        let exam = q.is_exam();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_q_can_confirm(can);
        st.set_answered_count(count);
        drop(st);
        self.autosave_quiz();
        if exam {
            // aggiorna solo la griglia
            self.render_question();
        }
    }

    pub(super) fn toggle_unsure(&mut self) {
        let Some(q) = &mut self.quiz else { return };
        let i = q.idx;
        q.unsure[i] = !q.unsure[i];
        self.render_question();
    }

    pub(super) fn confirm(&mut self) {
        let Some(q) = &mut self.quiz else { return };
        let i = q.idx;
        if q.given[i].is_none() || q.is_exam() {
            return;
        }
        q.account_time();
        q.checked[i] = true;
        self.render_question();
    }

    pub(super) fn goto(&mut self, k: usize) {
        let Some(q) = &mut self.quiz else { return };
        if k >= q.questions.len() {
            return;
        }
        q.account_time();
        q.idx = k;
        self.render_question();
    }

    pub(super) fn next(&mut self) {
        let Some(q) = &self.quiz else { return };
        let (i, n, exam) = (q.idx, q.questions.len(), q.is_exam());
        if i + 1 < n {
            self.goto(i + 1);
        } else if !exam {
            self.finish_quiz();
        }
    }

    /// Salta la domanda: passa alla successiva lasciandola senza risposta (non viene
    /// registrata e non torna in coda). Sull'ultima domanda chiude l'allenamento, quindi si può
    /// concludere senza rispondere a tutto.
    pub(super) fn skip(&mut self) {
        let Some(q) = &mut self.quiz else { return };
        if q.is_exam() {
            return;
        }
        if !q.checked[q.idx] {
            q.account_time();
            q.given[q.idx] = Given::None;
        }
        self.next();
    }

    pub(super) fn prev(&mut self) {
        let Some(q) = &self.quiz else { return };
        if q.idx > 0 {
            let k = q.idx - 1;
            self.goto(k);
        }
    }

    pub(super) fn request_finish(&mut self) {
        let Some(q) = &self.quiz else { return };
        let omitted = q.given.iter().filter(|g| g.is_none()).count();
        let unsure = q.unsure.iter().filter(|u| **u).count();
        let text = format!(
            "{} risposte date, {} senza risposta (valgono 0), {} segnate insicure.",
            q.questions.len() - omitted,
            omitted,
            unsure
        );
        self.open_dialog(Dialog::SubmitExam, "Consegnare la prova?", &text, "Consegna");
    }

    pub(super) fn finish_quiz(&mut self) {
        self.tick.stop();
        self.clear_saved_quiz();
        let Some(mut q) = self.quiz.take() else {
            return;
        };
        q.account_time();
        self.result_perm = q.questions.iter().map(|x| x.id.clone()).zip(q.perm.iter().cloned()).collect();
        let rules = self.content.syllabus().exam.clone();
        let exam = q.is_exam();
        let mut items = Vec::new();
        for (k, question) in q.questions.iter().enumerate() {
            if !exam && !q.checked[k] {
                continue;
            }
            let given = q.given[k].to_record();
            items.push(Item {
                id: question.id.clone(),
                topic: question.topic.clone(),
                kind: "q".into(),
                correct: Some(given.is_some() && is_correct(question, &q.given[k])),
                given,
                unsure: q.unsure[k],
                grade: None,
                flag: None,
                secs: q.secs[k].round() as u32,
                note: None,
            });
        }
        if items.is_empty() {
            self.set_page(if q.unit.is_some() { P_UNIT } else { self.tab_return() });
            return;
        }
        let mut score = score_items(&items, rules.correct, rules.wrong, rules.omitted);
        if exam {
            score.passed = Some(score.points >= rules.pass);
        }
        let now = Utc::now();
        let session = Session {
            schema: 1,
            id: Session::new_id(q.kind, q.subject.as_deref(), q.started),
            kind: q.kind,
            subject: q.subject.clone(),
            unit: q.unit.clone(),
            started_at: q.started,
            ended_at: now,
            app_version: APP_VERSION.into(),
            device: device_name(),
            items,
            score: Some(score.clone()),
        };
        let sname = q.subject.as_deref().and_then(|s| self.content.subject(s)).map(|s| s.name.clone());
        let msg = session.commit_message(sname.as_deref());
        if let Err(e) = self.store.record_session(&session, &msg) {
            self.toast(format!("Errore salvataggio: {e}"));
        }
        self.reload_progress();
        self.refresh_all();
        self.results_from_log = false;
        self.show_results(&session, &q.title);
        self.start_sync(false);
    }

    pub(super) fn show_results(&mut self, s: &Session, title: &str) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let sc = s.score.clone().unwrap_or_default();
        let exam = s.kind == SessionKind::Exam;
        st.set_r_title(ss(if exam { format!("Simulazione · {title}") } else { title.to_string() }));
        st.set_r_is_exam(exam);
        let timed: Vec<u32> = s.items.iter().map(|i| i.secs).filter(|x| *x > 0).collect();
        let avg = if timed.is_empty() { 0.0 } else { timed.iter().sum::<u32>() as f64 / timed.len() as f64 };
        let pace = exam_pace(self.content.syllabus().exam.minutes, self.content.syllabus().exam.questions);
        let time_line = if timed.is_empty() {
            String::new()
        } else {
            format!(
                "\nTempo medio {} a domanda (ritmo d'esame {}) · {} oltre il ritmo",
                mmss(avg),
                mmss(pace),
                timed.iter().filter(|x| **x as f64 > pace).count()
            )
        };
        st.set_r_passed(sc.passed.unwrap_or(false));
        if exam {
            st.set_r_points(ss(format!("{} / {}", it1(sc.points), it1(sc.max))));
            st.set_r_summary(ss(format!(
                "{} corrette · {} errate · {} omesse · {} insicure{time_line}",
                sc.correct, sc.wrong, sc.omitted, sc.unsure
            )));
        } else {
            st.set_r_points(ss(format!("{}/{}", sc.correct, sc.correct + sc.wrong + sc.omitted)));
            st.set_r_summary(ss(format!("{} errate · {} insicure{time_line}", sc.wrong, sc.unsure)));
        }
        st.set_r_rating(0);
        st.set_r_feedback(ss(""));
        st.set_r_feedback_sent(false);
        self.results = Some(s.clone());
        self.render_results();
        self.set_page(P_RESULTS);
    }

    /// "C) testo" con la lettera mostrata nel quiz (le opzioni erano mescolate) per una
    /// risposta registrata con la lettera originale.
    pub(super) fn shown_answer(&self, q: Option<&Question>, given: Option<&str>) -> String {
        let Some(g) = given else { return String::new() };
        let Some(q) = q.filter(|q| q.kind == QKind::Mcq) else {
            return g.to_string();
        };
        let Some(orig) = g.chars().next().filter(|c| ('A'..='E').contains(c)).map(|c| (c as u8 - b'A') as usize) else {
            return g.to_string();
        };
        let shown = self.result_perm.get(&q.id).and_then(|p| p.iter().position(|&k| k == orig)).unwrap_or(orig);
        match q.options.get(orig) {
            Some(t) => format!("{}) {}", prep_core::model::letter(shown), t),
            None => g.to_string(),
        }
    }

    pub(super) fn render_results(&mut self) {
        let Some(s) = &self.results else { return };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let pace = exam_pace(self.content.syllabus().exam.minutes, self.content.syllabus().exam.questions);
        let items: Vec<ResultItemVM> = s
            .items
            .iter()
            .enumerate()
            .map(|(k, it)| {
                let q = self.content.question(&it.id);
                ResultItemVM {
                    n: k as i32 + 1,
                    text: tx(&q.map(|q| q.text.clone()).unwrap_or(it.id.clone())),
                    status: match (&it.given, it.correct) {
                        (None, _) => 2,
                        (_, Some(true)) => 0,
                        _ => 1,
                    },
                    unsure: self.history.flagged.contains(&it.id),
                    given: tx(&self.shown_answer(q, it.given.as_deref())),
                    correct_text: tx(&q
                        .map(|q| match q.answer {
                            Some(a) if q.kind == QKind::Mcq => {
                                self.shown_answer(Some(q), Some(&prep_core::model::letter(a).to_string()))
                            }
                            _ => q.correct_text(),
                        })
                        .unwrap_or_default()),
                    explanation: tx(&q
                        .map(|q| match self.result_perm.get(&q.id) {
                            Some(p) => prep_core::model::remap_explanation(&q.explanation, p, &q.options),
                            None => q.explanation.clone(),
                        })
                        .unwrap_or_default()),
                    ref_label: ss(q
                        .and_then(|q| self.content.resolve_ref(q.book_ref.as_ref(), &q.topic))
                        .map(|r| self.content.ref_label(&r))
                        .unwrap_or_default()),
                    secs: ss(if it.secs > 0 { mmss(it.secs as f64) } else { String::new() }),
                    slow: it.secs as f64 > pace,
                }
            })
            .collect();
        st.set_r_items(model(items));
        st.set_r_sync(ss(sync::status_line(&self.store)));
    }

    pub(super) fn result_toggle_flag(&mut self, k: usize) {
        let Some(id) = self.results.as_ref().and_then(|s| s.items.get(k)).map(|i| i.id.clone()) else {
            return;
        };
        self.toggle_flag(&id, false);
        self.render_results();
    }

    pub(super) fn results_close(&mut self) {
        {
            let ui = self.ui();
            let st = ui.global::<AppState>();
            if !st.get_r_feedback_sent() && (st.get_r_rating() > 0 || !st.get_r_feedback().trim().is_empty()) {
                self.send_session_feedback();
            }
        }
        let unit = self.results.as_ref().and_then(|s| s.unit.clone());
        self.results = None;
        self.flush_flags();
        if std::mem::take(&mut self.results_from_log) {
            self.refresh_simlog();
            self.set_page(P_SIMLOG);
        } else if let Some(u) = unit {
            self.unit_id = u;
            self.refresh_unit();
            self.set_page(P_UNIT);
        } else {
            self.set_page(self.tab_return());
        }
    }

    // ---------- segnalazioni ----------

    pub(super) fn toggle_flag(&mut self, id: &str, debounce: bool) {
        let now_on = !self.history.flagged.contains(id);
        self.pending_flags.insert(id.to_string(), now_on);
        if now_on {
            self.history.flagged.insert(id.to_string());
        } else {
            self.history.flagged.remove(id);
        }
        self.progress = compute(&self.content, &self.history, self.assessment.as_ref());
        self.refresh_flags();
        self.refresh_home();
        if debounce {
            self.flag_timer.start(TimerMode::SingleShot, Duration::from_secs(6), || {
                with_ctrl(|c| c.flush_flags());
            });
        }
    }

    pub(super) fn flush_flags(&mut self) {
        self.flag_timer.stop();
        if self.pending_flags.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_flags);
        // ricostruisci lo stato "vero" dalle sessioni e registra solo i cambi effettivi
        let base = build_history(&self.sessions);
        let items: Vec<Item> = pending
            .into_iter()
            .filter(|(id, on)| base.flagged.contains(id) != *on)
            .map(|(id, on)| {
                let topic = self
                    .content
                    .question(&id)
                    .map(|q| q.topic.clone())
                    .or_else(|| self.content.card(&id).map(|c| c.topic.clone()))
                    .unwrap_or_default();
                Item {
                    id,
                    topic,
                    kind: "flag".into(),
                    given: None,
                    correct: None,
                    unsure: on,
                    grade: None,
                    flag: Some(on),
                    secs: 0,
                    note: None,
                }
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let now = Utc::now();
        let s = Session {
            schema: 1,
            id: Session::new_id(SessionKind::Flags, None, now),
            kind: SessionKind::Flags,
            subject: None,
            unit: None,
            started_at: now,
            ended_at: now,
            app_version: APP_VERSION.into(),
            device: device_name(),
            items,
            score: None,
        };
        let msg = s.commit_message(None);
        if let Err(e) = self.store.record_session(&s, &msg) {
            self.toast(format!("Errore salvataggio: {e}"));
        }
        self.reload_progress();
        self.refresh_all();
        self.start_sync(false);
    }
}
