//! Obiettivi del giorno: proposta, scelta e avanzamento.

use super::*;

const MESI_IT: [&str; 12] = [
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

impl Ctrl {
    // ---------- obiettivi del giorno ----------

    /// Obiettivi confermati con una versione che non li sincronizzava: li mette in coda una volta.
    pub(super) fn share_legacy_goals(&self) {
        let today = Local::now().date_naive();
        if let Some(mut g) = prep_core::plan::Goals::load(&self.store.root) {
            if g.is_for(today) && g.updated_at.is_none() && !g.topics.is_empty() {
                let _ = g.record(&self.store);
            }
        }
    }

    pub(super) fn next_exam(&self) -> Option<(String, NaiveDate)> {
        let today = Local::now().date_naive();
        self.content.syllabus().exam.dates.iter().find_map(|d| {
            let nd = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok()?;
            (nd >= today).then(|| (d.label.clone(), nd))
        })
    }

    pub(super) fn today_goals(&self) -> Option<prep_core::plan::Goals> {
        prep_core::plan::Goals::latest(&self.store, Local::now().date_naive()).filter(|g| !g.topics.is_empty())
    }

    /// Obiettivi di ripasso: argomenti studiati ma fermi da almeno 2 giorni.
    pub(super) fn refresh_review_goals(&self) {
        use prep_core::plan;
        let today = Local::now().date_naive();
        let items: Vec<GoalVM> =
            plan::review_due(&self.content, &self.progress, &self.history, today, REVIEW_AFTER_DAYS, 4)
                .into_iter()
                .map(|r| GoalVM {
                    topic: ss(&r.topic),
                    name: ss(&r.name),
                    info: ss(format!(
                        "{} · ultimo allenamento {} giorni fa · padronanza {}% · {} domande",
                        r.subject,
                        r.days,
                        (r.mastery * 100.0).round() as i32,
                        r.target
                    )),
                    done: 0,
                    target: r.target as i32,
                    selected: false,
                })
                .collect();
        self.ui().global::<AppState>().set_review_goals(model(items));
    }

    pub(super) fn refresh_goals(&mut self) {
        use prep_core::plan;
        self.refresh_review_goals();
        let today = Local::now().date_naive();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if let Some(g) = self.today_goals() {
            let done = plan::done_on(&self.content, &self.history, today);
            let items: Vec<GoalVM> = g
                .topics
                .iter()
                .zip(&g.targets)
                .map(|(t, &target)| {
                    let n = *done.get(t).unwrap_or(&0);
                    let tp = self.content.topic(t);
                    GoalVM {
                        topic: ss(t),
                        name: ss(tp.map(|x| x.name.as_str()).unwrap_or(t)),
                        info: ss(format!("{}/{} domande oggi", n.min(target), target)),
                        done: n as i32,
                        target: target as i32,
                        selected: true,
                    }
                })
                .collect();
            let left: u32 = items.iter().map(|g| (g.target - g.done).max(0) as u32).sum();
            st.set_goals_status(ss(if left == 0 {
                "Obiettivi di oggi completati. Ottimo lavoro!".to_string()
            } else {
                format!("Ti mancano {left} domande per chiudere la giornata.")
            }));
            st.set_goals_left(items.iter().filter(|g| g.done < g.target).count() as i32);
            st.set_goals_done_n(items.iter().filter(|g| g.done >= g.target).count() as i32);
            let n = items.len();
            let done_n = items.iter().filter(|g| g.done >= g.target).count();
            st.set_goals_status(ss(format!(
                "{done_n} di {n} fatt{} · {}",
                if done_n == 1 { "o" } else { "i" },
                if left == 0 { "giornata chiusa".to_string() } else { format!("mancano {left} domande") }
            )));
            st.set_goals_confirmed(true);
            st.set_goals(model(items));
            self.plan = None;
            self.refresh_goal_topics();
            return;
        }
        let exam = self.next_exam();
        let (due, new) = select::count_due(&self.content, &self.history, &Scope::All, Utc::now());
        let mut p = plan::suggest(
            &self.content,
            &self.progress,
            &self.history,
            today,
            exam.as_ref().map(|(l, d)| (l.as_str(), *d)),
            due + new.min(CARDS_NEW),
            4,
        );
        let per = p.suggestions.first().map(|x| x.target).unwrap_or(10);
        for t in &self.plan_extra {
            if p.suggestions.iter().any(|x| &x.topic == t) {
                continue;
            }
            if let Some(tp) = self.content.topic(t) {
                p.suggestions.push(plan::Suggestion {
                    topic: t.clone(),
                    name: tp.name.clone(),
                    subject: self.content.subject_of_topic(t).map(|x| x.name.clone()).unwrap_or_default(),
                    unit: String::new(),
                    score: 0.0,
                    reasons: vec!["scelto da te".into()],
                    target: per,
                });
            }
        }
        if self.plan_sel.len() != p.suggestions.len()
            || self.plan.as_ref().map(|x| &x.suggestions) != Some(&p.suggestions)
        {
            self.plan_sel = vec![true; p.suggestions.len()];
        }
        let items: Vec<GoalVM> = p
            .suggestions
            .iter()
            .zip(&self.plan_sel)
            .map(|(x, &sel)| GoalVM {
                topic: ss(&x.topic),
                name: ss(&x.name),
                info: ss(format!("{} · {} · {} domande", x.subject, x.reasons.join(", "), x.target)),
                done: 0,
                target: x.target as i32,
                selected: sel,
            })
            .collect();
        st.set_goals_note(ss(format!(
            "{} Oggi: ~{} domande + {} flashcard (circa {} h {:02} min). Scegli gli argomenti e conferma.",
            p.note,
            p.questions,
            p.cards_due,
            p.minutes / 60,
            p.minutes % 60
        )));
        st.set_goals_confirmed(false);
        st.set_goals_done_n(0);
        st.set_goals_left(0);
        st.set_goals(model(items));
        self.plan = Some(p);
        self.refresh_goal_topics();
    }

    pub(super) fn chosen_topics(&self) -> Vec<String> {
        match self.today_goals() {
            Some(g) => g.topics,
            None => self
                .plan
                .as_ref()
                .map(|p| {
                    p.suggestions.iter().zip(&self.plan_sel).filter(|(_, &s)| s).map(|(x, _)| x.topic.clone()).collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Elenco degli argomenti della materia scelta nella scheda Obiettivi.
    pub(super) fn refresh_goal_topics(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let mut subj = st.get_goal_subject().to_string();
        if subj.is_empty() {
            subj = self.progress.subjects.first().map(|s| s.id.clone()).unwrap_or_default();
            st.set_goal_subject(ss(&subj));
        }
        let chosen = self.chosen_topics();
        let items: Vec<GoalVM> = self
            .progress
            .subjects
            .iter()
            .filter(|s| s.id == subj)
            .flat_map(|s| s.units.iter())
            .flat_map(|u| u.topics.iter().filter(|t| t.questions > 0).map(move |t| (u, t)))
            .map(|(u, t)| GoalVM {
                topic: ss(&t.id),
                name: ss(&t.name),
                info: ss(format!("U{} · padronanza {:.0}% · {} domande", u.n, t.mastery * 100.0, t.questions)),
                done: 0,
                target: 0,
                selected: chosen.contains(&t.id),
            })
            .collect();
        st.set_goal_topics(model(items));
    }

    pub(super) fn goals_add(&mut self, topic: String) {
        if let Some(mut g) = self.today_goals() {
            if g.topics.contains(&topic) {
                self.toast("È già tra gli obiettivi di oggi");
                return;
            }
            let target = (g.targets.iter().sum::<u32>() / g.targets.len().max(1) as u32).max(10);
            g.topics.push(topic);
            g.targets.push(target);
            if let Err(e) = g.record(&self.store) {
                self.toast(format!("Errore: {e}"));
                return;
            }
            self.toast("Aggiunto agli obiettivi di oggi");
            self.write_reminder();
        } else {
            match self.plan.as_ref().and_then(|p| p.suggestions.iter().position(|x| x.topic == topic)) {
                Some(i) => {
                    if let Some(x) = self.plan_sel.get_mut(i) {
                        *x = true;
                    }
                }
                None => self.plan_extra.push(topic),
            }
            self.toast("Aggiunto alla proposta: conferma per renderlo un obiettivo");
        }
        self.refresh_goals();
    }

    pub(super) fn goals_remove(&mut self, i: i32) {
        let Some(mut g) = self.today_goals() else {
            return;
        };
        let i = i as usize;
        if i >= g.topics.len() {
            return;
        }
        g.topics.remove(i);
        g.targets.remove(i);
        if let Err(e) = g.record(&self.store) {
            self.toast(format!("Errore: {e}"));
        }
        self.refresh_goals();
        self.write_reminder();
    }

    pub(super) fn goals_toggle(&mut self, i: i32) {
        if let Some(x) = self.plan_sel.get_mut(i as usize) {
            *x = !*x;
        }
        self.refresh_goals();
    }

    pub(super) fn goals_confirm(&mut self) {
        let Some(p) = &self.plan else { return };
        let chosen: Vec<&prep_core::plan::Suggestion> =
            p.suggestions.iter().zip(&self.plan_sel).filter(|(_, &s)| s).map(|(x, _)| x).collect();
        if chosen.is_empty() {
            self.toast("Scegli almeno un argomento");
            return;
        }
        // il totale del giorno resta lo stesso, ripartito sugli argomenti scelti
        let per = (p.questions as usize).div_ceil(chosen.len()) as u32;
        let mut g = prep_core::plan::Goals {
            date: Local::now().date_naive().format("%Y-%m-%d").to_string(),
            topics: chosen.iter().map(|x| x.topic.clone()).collect(),
            targets: vec![per.max(5); chosen.len()],
            confirmed_at: Some(Utc::now()),
            updated_at: None,
        };
        match g.record(&self.store) {
            Ok(()) => {
                self.toast("Obiettivi confermati: li trovi in Home e nella notifica");
                self.refresh_goals();
                self.write_reminder();
            }
            Err(e) => self.toast(format!("Errore: {e}")),
        }
    }

    pub(super) fn goals_edit(&mut self) {
        if self.today_goals().is_some() {
            let day = Local::now().date_naive().format("%Y-%m-%d").to_string();
            let _ = prep_core::plan::Goals::cleared(&day).record(&self.store);
        }
        self.plan = None;
        self.plan_sel.clear();
        self.plan_extra.clear();
        self.refresh_goals();
        self.write_reminder();
    }

    /// Aggiorna reminder.json, letto dal promemoria giornaliero (receiver Android / desktop).
    pub(super) fn write_reminder(&self) {
        use prep_core::reminder::{parse_time, DayCount, Paused, ReminderState};
        let s = self.store.settings();
        let (hour, minute) = parse_time(&s.reminder_time).unwrap_or((18, 30));
        let paused = self.load_saved_quiz().map(|q| Paused {
            title: q.title.clone(),
            done: q.given.iter().filter(|g| g.is_some()).count(),
            total: q.ids.len(),
        });
        let today = Local::now().date_naive();
        let due = (0..8u64)
            .filter_map(|i| {
                let day = today + chrono::Days::new(i);
                let end = Local.from_local_datetime(&day.and_hms_opt(23, 59, 0)?).earliest()?;
                let (due, new) = select::count_due(&self.content, &self.history, &Scope::All, end.with_timezone(&Utc));
                Some(DayCount { date: day.format("%Y-%m-%d").to_string(), n: due + new.min(CARDS_NEW) })
            })
            .collect();
        let exam = self
            .content
            .syllabus()
            .exam
            .dates
            .iter()
            .find(|d| NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").is_ok_and(|nd| nd >= today));
        let (evening_hour, evening_minute) = parse_time(&s.evening_time).unwrap_or((21, 30));
        let goals = self.today_goals().map(|g| {
            let done = prep_core::plan::done_on(&self.content, &self.history, today);
            prep_core::reminder::GoalsState {
                date: g.date.clone(),
                items: g
                    .topics
                    .iter()
                    .zip(&g.targets)
                    .map(|(t, &target)| prep_core::reminder::GoalItem {
                        name: self.content.topic(t).map(|x| x.name.clone()).unwrap_or_else(|| t.clone()),
                        done: *done.get(t).unwrap_or(&0),
                        target,
                    })
                    .collect(),
            }
        });
        let st = ReminderState {
            evening_hour,
            evening_minute,
            goals,
            enabled: s.reminder_enabled,
            hour,
            minute,
            paused,
            due,
            last_study: self
                .history
                .last_activity
                .map(|t| t.with_timezone(&Local).format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            exam_label: exam.map(|d| d.label.clone()).unwrap_or_default(),
            exam_date: exam.map(|d| d.date.clone()).unwrap_or_default(),
            times: vec![],
            off: s.reminder_off.clone(),
            studied_today: 0,
            slots: vec![],
        };
        let mut st = st;
        st.times = if s.reminder_times.is_empty() {
            prep_core::reminder::DEFAULT_TIMES.iter().map(|t| t.to_string()).collect()
        } else {
            s.reminder_times.clone()
        };
        st.studied_today = prep_core::plan::done_on(&self.content, &self.history, today).values().sum();
        st.compute_slots(today);
        let _ = st.save(&self.store.base);
    }

    pub(super) fn save_reminder(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let raw = st.get_s_rem_time().to_string();
        let mut times: Vec<(u32, u32)> = Vec::new();
        for part in raw.split([',', ';', ' ']).filter(|x| !x.trim().is_empty()) {
            match prep_core::reminder::parse_time(part) {
                Some(t) => times.push(t),
                None => {
                    self.toast(format!("Orario non valido: \"{}\". Usa HH:MM separati da virgola, per esempio 08:30, 13:45, 17:30, 21:00", part.trim()));
                    return;
                }
            }
        }
        if times.is_empty() {
            self.toast("Indica almeno un orario, per esempio 08:30, 13:45, 17:30, 21:00");
            return;
        }
        times.sort();
        times.dedup();
        let mut s = self.store.settings();
        s.reminder_enabled = st.get_s_rem_on();
        s.reminder_times = times.iter().map(|(h, m)| format!("{h:02}:{m:02}")).collect();
        let list = s.reminder_times.join(", ");
        st.set_s_rem_time(ss(&list));
        match self.store.save_settings(&s) {
            Ok(()) => {
                self.write_reminder();
                #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
                crate::desktop_reminder::sync_autostart(s.reminder_enabled);
                self.toast(if s.reminder_enabled {
                    format!("Promemoria alle {list}")
                } else {
                    "Promemoria disattivati".into()
                });
            }
            Err(e) => self.toast(format!("Errore: {e}")),
        }
    }

    /// Orari dei promemoria come righe di Opzioni › Studio, ognuno con il suo interruttore.
    pub(super) fn refresh_rem_slots(&self) {
        let s = self.store.settings();
        let times: Vec<String> = if s.reminder_times.is_empty() {
            prep_core::reminder::DEFAULT_TIMES.iter().map(|t| t.to_string()).collect()
        } else {
            s.reminder_times.clone()
        };
        let n = times.len();
        let rows = times
            .iter()
            .enumerate()
            .map(|(k, t)| {
                let label = if k == 0 && n > 1 {
                    "Mattina · obiettivi del giorno e flashcard"
                } else if k + 1 == n {
                    "Sera · cosa manca prima di chiudere"
                } else if k == 1 {
                    "Dopo pranzo · riprendi la sessione"
                } else {
                    "Pomeriggio · a che punto sei"
                };
                RemVM { time: ss(t), label: ss(label), on: s.reminder_enabled && !s.reminder_off.contains(t) }
            })
            .collect::<Vec<_>>();
        self.ui().global::<AppState>().set_rem_slots(model(rows));
    }

    /// Accende o spegne un orario dei promemoria (spenti tutti = promemoria disattivati).
    pub(super) fn rem_toggle(&mut self, i: usize, on: bool) {
        let mut s = self.store.settings();
        if s.reminder_times.is_empty() {
            s.reminder_times = prep_core::reminder::DEFAULT_TIMES.iter().map(|t| t.to_string()).collect();
        }
        let Some(t) = s.reminder_times.get(i).cloned() else { return };
        if !s.reminder_enabled {
            // dai promemoria tutti spenti: si riaccende solo questo
            s.reminder_off = s.reminder_times.iter().filter(|x| **x != t).cloned().collect();
            s.reminder_enabled = on;
        } else if on {
            s.reminder_off.retain(|x| *x != t);
        } else if !s.reminder_off.contains(&t) {
            s.reminder_off.push(t);
        }
        if s.reminder_times.iter().all(|x| s.reminder_off.contains(x)) {
            s.reminder_enabled = false;
            s.reminder_off.clear();
        }
        self.store_reminder(s);
    }

    /// Cambia l'orario `i` dei promemoria ("HH:MM").
    pub(super) fn rem_time(&mut self, i: usize, text: &str) {
        let Some((h, m)) = prep_core::reminder::parse_time(text) else {
            self.toast(format!("Orario non valido: \"{}\". Scrivilo come 08:30", text.trim()));
            self.refresh_rem_slots();
            return;
        };
        let mut s = self.store.settings();
        if s.reminder_times.is_empty() {
            s.reminder_times = prep_core::reminder::DEFAULT_TIMES.iter().map(|t| t.to_string()).collect();
        }
        let Some(old) = s.reminder_times.get(i).cloned() else { return };
        let new = format!("{h:02}:{m:02}");
        if s.reminder_times.iter().any(|x| *x == new && *x != old) {
            self.toast(format!("C'è già un promemoria alle {new}"));
            self.refresh_rem_slots();
            return;
        }
        s.reminder_times[i] = new.clone();
        for x in s.reminder_off.iter_mut() {
            if *x == old {
                *x = new.clone();
            }
        }
        s.reminder_times.sort();
        self.store_reminder(s);
    }

    fn store_reminder(&mut self, s: prep_core::store::Settings) {
        let _ = self.store.save_settings(&s);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_s_rem_on(s.reminder_enabled);
        st.set_s_rem_time(ss(s.reminder_times.join(", ")));
        self.write_reminder();
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        crate::desktop_reminder::sync_autostart(s.reminder_enabled);
        self.refresh_rem_slots();
    }

    pub(super) fn subject_vms(&self) -> Vec<SubjectVM> {
        let now = Utc::now();
        // punteggio massimo di una prova: quesiti × punti per risposta corretta (dal pacchetto)
        let rules = &self.content.syllabus().exam;
        let max_points = rules.questions as f64 * rules.correct;
        self.progress
            .subjects
            .iter()
            .map(|s| {
                let (due, new) = select::count_due(&self.content, &self.history, &Scope::Subject(s.id.clone()), now);
                SubjectVM {
                    id: ss(&s.id),
                    n: self.content.syllabus().subjects.iter().position(|x| x.id == s.id).unwrap_or(0) as i32,
                    name: ss(&s.name),
                    short: ss(self
                        .content
                        .subject(&s.id)
                        .map(|x| x.short.trim())
                        .filter(|x| !x.is_empty())
                        .unwrap_or(&s.name)),
                    mastery: s.mastery as f32,
                    predicted: ss(format!(
                        "≈ {}/{}",
                        it1(s.predicted).trim_end_matches(",0"),
                        it1(max_points).trim_end_matches(",0")
                    )),
                    flagged: s.flagged as i32,
                    score: ss(it1(s.predicted).trim_end_matches(",0")),
                    due: due as i32,
                    new_cards: new as i32,
                    learned: select::count_learned(&self.content, &self.history, &Scope::Subject(s.id.clone()), now)
                        as i32,
                    questions: s.units.iter().map(|u| u.questions as i32).sum(),
                    note: ss(&s.ai_note),
                    pace: ss(if s.timed > 0 {
                        format!("{} di studio · media {}/dom.", hm(s.total_secs), mmss(s.avg_secs))
                    } else {
                        String::new()
                    }),
                    ..subject_look(self.content.subject(&s.id))
                }
            })
            .collect()
    }

    pub(super) fn refresh_home(&mut self) {
        self.refresh_paused();
        self.refresh_goals();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_overall(self.progress.overall as f32);
        st.set_overall_label(ss(match self.progress.anchored_at {
            Some(t) => format!(
                "Rivista da AI il {}, poi aggiornata con le nuove risposte",
                t.with_timezone(&Local).format("%d/%m %H:%M")
            ),
            None => "Stima automatica da quiz e flashcard".into(),
        }));
        let today = Local::now().date_naive();
        let dates: Vec<ExamDateVM> = self
            .content
            .syllabus()
            .exam
            .dates
            .iter()
            .filter_map(|d| {
                let nd = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok()?;
                Some(ExamDateVM {
                    label: ss(&d.label),
                    date: ss(nd.format("%d/%m/%Y").to_string()),
                    days: (nd - today).num_days() as i32,
                })
            })
            .collect();
        // fascia di Oggi: giorni al prossimo appello (con la data) e quello dopo
        let upcoming: Vec<(i64, NaiveDate, String)> = {
            let mut v: Vec<(i64, NaiveDate, String)> = self
                .content
                .syllabus()
                .exam
                .dates
                .iter()
                .filter_map(|d| {
                    let nd = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok()?;
                    let days = (nd - today).num_days();
                    (days >= 0).then(|| (days, nd, d.label.clone()))
                })
                .collect();
            v.sort_by_key(|x| x.0);
            v
        };
        match upcoming.first() {
            Some((days, nd, _)) => {
                use chrono::Datelike;
                st.set_hero_days(*days as i32);
                st.set_hero_days_label(ss(format!(
                    "{} al {} {}",
                    if *days == 1 { "giorno" } else { "giorni" },
                    nd.day(),
                    MESI_IT[nd.month0() as usize]
                )));
            }
            None => {
                st.set_hero_days(-1);
                st.set_hero_days_label(ss(""));
            }
        }
        st.set_hero_days_sub(ss(upcoming.get(1).map(|(d, _, l)| format!("{l} tra {d}")).unwrap_or_default()));
        // punteggio atteso in tutto e soglia per materia
        let rules = &self.content.syllabus().exam;
        let max_points = rules.questions as f64 * rules.correct;
        let n_sub = self.progress.subjects.len() as f64;
        let expected: f64 = self.progress.subjects.iter().map(|s| s.predicted).sum();
        if max_points > 0.0 && n_sub > 0.0 {
            let mut t = format!(
                "atteso {} / {}",
                it1(expected).trim_end_matches(",0"),
                it1(max_points * n_sub).trim_end_matches(",0")
            );
            if rules.pass > 0.0 {
                t.push_str(&format!(" · serve {} a materia", it1(rules.pass).trim_end_matches(",0")));
            }
            st.set_hero_expected(ss(t));
        } else {
            st.set_hero_expected(ss(""));
        }
        let ok = rules.pass > 0.0 && n_sub > 0.0 && self.progress.subjects.iter().all(|s| s.predicted >= rules.pass);
        st.set_pass_ok(ok);
        if rules.pass > 0.0 && max_points > 0.0 {
            st.set_pass_frac((rules.pass / max_points).clamp(0.0, 1.0) as f32);
            st.set_pass_label(ss(format!(
                "{} · {}",
                it1(rules.pass).trim_end_matches(",0"),
                if ok { "superata" } else { "per superare" }
            )));
        } else {
            st.set_pass_frac(0.0);
            st.set_pass_label(ss(""));
        }
        // settimana di studio (da lunedì) e serie di giorni con almeno un po' di studio
        {
            use chrono::Datelike;
            let log = self.study_log.combined();
            let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
            let secs: Vec<u64> = (0..7).map(|i| log.day(monday + chrono::Duration::days(i))).collect();
            let max = secs.iter().copied().max().unwrap_or(0).max(1);
            st.set_week_bars(model(secs.iter().map(|s| *s as f32 / max as f32).collect()));
            st.set_week_today(today.weekday().num_days_from_monday() as i32);
            // i numeri di ogni giorno, per il tocco sulla sua barra
            let mut q = [(0u32, 0u32); 7];
            let mut c = [0u32; 7];
            let idx = |at: &chrono::DateTime<Utc>| {
                let d = (at.with_timezone(&Local).date_naive() - monday).num_days();
                (0..7).contains(&d).then_some(d as usize)
            };
            for a in self.history.questions.values().flatten() {
                if let Some(i) = idx(&a.at) {
                    q[i].0 += 1;
                    q[i].1 += u32::from(a.score >= 0.999);
                }
            }
            for a in self.history.cards.values().flatten() {
                if let Some(i) = idx(&a.at) {
                    c[i] += 1;
                }
            }
            const DAYS: [&str; 7] = ["Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato", "Domenica"];
            let info: Vec<(SharedString, SharedString)> = (0..7)
                .map(|i| {
                    let d = monday + chrono::Duration::days(i as i64);
                    let mut parts = Vec::new();
                    if secs[i] > 0 {
                        parts.push(format!("{} di studio", hm(secs[i])));
                    }
                    if q[i].0 > 0 {
                        parts.push(format!(
                            "{} {} · {}% giuste",
                            q[i].0,
                            if q[i].0 == 1 { "domanda" } else { "domande" },
                            q[i].1 * 100 / q[i].0
                        ));
                    }
                    if c[i] > 0 {
                        parts.push(format!("{} flashcard", c[i]));
                    }
                    if parts.is_empty() {
                        parts.push("niente studio".to_string());
                    }
                    let when = if d == today { "Oggi".to_string() } else { format!("{} {}", DAYS[i], d.day()) };
                    (ss(when), ss(parts.join(" · ")))
                })
                .collect();
            st.set_week_day(model(info.iter().map(|(a, _)| a.clone()).collect::<Vec<_>>()));
            st.set_week_info(model(info.into_iter().map(|(_, b)| b).collect::<Vec<_>>()));
            let total: u64 = secs.iter().sum();
            let days = secs.iter().filter(|s| **s > 0).count() as u64;
            st.set_week_total(ss(if total == 0 {
                String::new()
            } else {
                format!("{} · media {}′", hm(total), total / days.max(1) / 60)
            }));
            let mut streak = 0;
            let mut d = if log.day(today) > 0 { today } else { today - chrono::Duration::days(1) };
            while log.day(d) > 0 && streak < 999 {
                streak += 1;
                d -= chrono::Duration::days(1);
            }
            st.set_streak_days(streak);
        }
        st.set_hero_time(ss(if self.progress.total_secs > 0 {
            format!("{} studiate", hm(self.progress.total_secs))
        } else {
            String::new()
        }));
        st.set_exam_dates(model(dates));
        {
            // «Tu»: tempo, risposte date e percentuale di giuste (domande, non flashcard)
            let all: Vec<&prep_core::progress::Attempt> = self.history.questions.values().flatten().collect();
            let right = all.iter().filter(|a| a.score >= 0.999).count();
            let secs = self.progress.total_secs;
            st.set_tu_time(ss(if secs >= 3600 {
                format!("{} h {:02}", secs / 3600, (secs % 3600) / 60)
            } else {
                format!("{} min", (secs + 30) / 60)
            }));
            st.set_tu_answers(all.len() as i32);
            st.set_tu_acc(ss(if all.is_empty() {
                "–".to_string()
            } else {
                format!("{}%", (right as f64 * 100.0 / all.len() as f64).round())
            }));
            let rules = &self.content.syllabus().exam;
            let pace = prep_core::progress::exam_pace(rules.minutes, rules.questions);
            st.set_tu_pace(ss(if self.progress.avg_secs > 0.0 {
                if rules.questions > 0 {
                    format!("{}″ a domanda, esame {}″", self.progress.avg_secs.round(), pace.round())
                } else {
                    format!("{}″ a domanda", self.progress.avg_secs.round())
                }
            } else {
                "Ancora nessuna domanda cronometrata".to_string()
            }));
        }
        st.set_total_time(ss(if self.progress.total_secs > 0 {
            format!(
                "Studio cronometrato: {} · media {}/domanda",
                hm(self.progress.total_secs),
                mmss(self.progress.avg_secs)
            )
        } else {
            String::new()
        }));
        st.set_subjects(model(self.subject_vms()));
        let qflag = self.history.flagged.iter().filter(|id| self.content.question(id).is_some()).count();
        let wrong_only = self
            .history
            .last_wrong
            .iter()
            .filter(|id| !self.history.flagged.contains(*id) && self.content.question(id).is_some())
            .count();
        st.set_flagged_count(qflag as i32);
        st.set_wrong_count(wrong_only as i32);
        let (due, new) = select::count_due(&self.content, &self.history, &Scope::All, Utc::now());
        st.set_due_count((due + new.min(CARDS_NEW)) as i32);
        match &self.assessment {
            Some(a) => {
                st.set_ai_summary(ss(&a.summary));
                let mut pr: Vec<SharedString> = a
                    .priorities
                    .iter()
                    .map(|p| {
                        let name = self.content.topic(&p.topic).map(|t| t.name.clone()).unwrap_or(p.topic.clone());
                        ss(if p.reason.is_empty() { name } else { format!("{name}: {}", p.reason) })
                    })
                    .collect();
                pr.extend(a.plan.iter().map(ss));
                st.set_ai_priorities(model(pr));
            }
            None => {
                st.set_ai_summary(ss(""));
                st.set_ai_priorities(model(vec![]));
            }
        }
        st.set_sync_status(ss(sync::status_line(&self.store)));
        st.set_sync_configured(self.sync_configured());
        let warn = if self.content.warnings.is_empty() {
            String::new()
        } else {
            format!(" · {} avvisi nei contenuti", self.content.warnings.len())
        };
        st.set_no_exam(self.content.syllabus().is_empty());
        st.set_exam_n(self.content.syllabus().exam.questions.max(1) as i32);
        st.set_content_info(ss(format!(
            "{} domande · {} flashcard · contenuti {}{} · {} sessioni",
            self.content.questions.len(),
            self.content.cards.len(),
            if self.from_repo { "dal repository" } else { "inclusi nell'app" },
            warn,
            self.sessions.len()
        )));
    }

    /// Prove ufficiali del pacchetto che hanno domande nella materia scelta.
    pub(super) fn official_tests(&self, subject: &str) -> Vec<QzOptVM> {
        self.content
            .syllabus()
            .official
            .iter()
            .filter(|t| {
                let prefix = format!("{subject}-{}-", t.tag);
                self.content.questions.iter().any(|q| q.id.starts_with(&prefix))
            })
            .map(|t| QzOptVM { id: ss(&t.tag), name: ss(&t.label), ..Default::default() })
            .collect()
    }

    pub(super) fn refresh_study(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        // materia scelta: la prima del pacchetto se quella attuale non c'è
        let subjects = &self.content.syllabus().subjects;
        let mut cur = st.get_current_subject().to_string();
        if !subjects.iter().any(|s| s.id == cur) {
            cur = subjects.first().map(|s| s.id.clone()).unwrap_or_default();
            st.set_current_subject(cur.as_str().into());
        }
        let subject = subjects.iter().position(|s| s.id == cur);
        st.set_current_subject_n(subject.unwrap_or(0) as i32);
        let subject = subject.map(|i| &subjects[i]);
        st.set_subject_periodic(subject.is_some_and(|s| s.tools.iter().any(|t| t == TOOL_PERIODIC_TABLE)));
        st.set_subject_links(model(
            subject.map(|s| s.links.iter().map(|l| ss(&l.label)).collect()).unwrap_or_default(),
        ));
        let officials = self.official_tests(&cur);
        {
            // Studia: Quiz, Flashcard e Simulazione (le prove ufficiali si scelgono dentro
            // Simulazione); gli strumenti della materia stanno a parte, dietro «Strumenti»
            let t = |kind: i32, id: &str, label: &str| ToolVM { kind, id: ss(id), label: ss(label) };
            let mut tools = vec![t(0, "", "Quiz"), t(1, "", "Flashcard")];
            if self.content.syllabus().exam.questions > 0 || !officials.is_empty() {
                tools.push(t(2, "", "Simulazione"));
            }
            let mut extra = Vec::new();
            if subject.is_some_and(|s| s.tools.iter().any(|x| x == TOOL_PERIODIC_TABLE)) {
                extra.push(t(4, "", "Tavola periodica"));
            }
            if let Some(s) = subject {
                extra.extend(s.links.iter().enumerate().map(|(i, l)| t(5, &i.to_string(), &l.label)));
            }
            extra.push(t(6, "", "Crea con l'AI"));
            tools.push(t(7, "", "Strumenti"));
            st.set_study_tools(model(tools));
            st.set_study_extra(model(extra));
        }
        st.set_official_tests(model(officials));
        // pagina «Simulazioni»: titolo della prova e prove ufficiali intere
        {
            use chrono::Datelike;
            let rules = &self.content.syllabus().exam;
            let today = Local::now().date_naive();
            let next = rules
                .dates
                .iter()
                .filter_map(|d| NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok())
                .find(|d| *d >= today);
            let per = if subjects.len() > 1 { " a materia" } else { "" };
            st.set_sim_hero(ss(match next {
                Some(d) => {
                    format!("{} domande{per}, come il {} {}", rules.questions, d.day(), MESI_IT[d.month0() as usize])
                }
                None => format!("{} domande{per}, come all'esame", rules.questions),
            }));
            st.set_sim_timed_label(ss(format!(
                "Tempo come all'esame ({} min{})",
                rules.minutes,
                if subjects.len() > 1 { " a materia" } else { "" }
            )));
            let all: Vec<QzOptVM> = self
                .content
                .syllabus()
                .official
                .iter()
                .filter_map(|t| {
                    let mut n = 0;
                    let mut parts = 0;
                    for s in subjects {
                        let prefix = format!("{}-{}-", s.id, t.tag);
                        let k = self.content.questions.iter().filter(|q| q.id.starts_with(&prefix)).count();
                        n += k;
                        parts += usize::from(k > 0);
                    }
                    (n > 0).then(|| QzOptVM {
                        id: ss(&t.tag),
                        name: ss(&t.label),
                        group: ss(if parts > 1 {
                            format!("{n} domande vere · {parts} × {} min", rules.minutes)
                        } else {
                            format!("{n} domande vere · {} min", rules.minutes)
                        }),
                        count: n as i32,
                        ..Default::default()
                    })
                })
                .collect();
            st.set_sim_officials(model(all));
        }
        let Some(sp) = self.progress.subjects.iter().find(|s| s.id == cur) else {
            return;
        };
        let units: Vec<UnitVM> = sp
            .units
            .iter()
            .map(|u| UnitVM {
                id: ss(&u.id),
                n: u.n as i32,
                name: ss(&u.name),
                cfu: ss(it1(u.cfu).trim_end_matches(",0")),
                mastery: u.mastery as f32,
                touched: u.touched as f32,
                questions: u.questions as i32,
                cards: u.cards as i32,
                flagged: u.flagged as i32,
                note: ss(&u.ai_note),
                time: ss(if u.timed > 0 {
                    format!("{} · media {}/dom.", hm(u.total_secs), mmss(u.avg_secs))
                } else {
                    String::new()
                }),
            })
            .collect();
        st.set_units(model(units));
        let books: Vec<BookVM> = self
            .content
            .books
            .iter()
            .filter(|b| b.subject == cur)
            .map(|b| BookVM {
                id: ss(&b.id),
                title: ss(&b.title),
                author: ss(&b.author),
                chapters: b.chapters.len() as i32,
                available: b.chapters.iter().any(|c| self.read_book_file(&c.file).is_some()),
            })
            .collect();
        st.set_books(model(books));
        // regole della simulazione, dai dati del pacchetto; altre schede le definisce il pacchetto
        let rules = &self.content.syllabus().exam;
        let pts = |x: f64| format!("{x:+}").replace('.', ",").replace('-', "−");
        st.set_exam_rules(ss(if rules.questions == 0 {
            String::new()
        } else {
            format!(
                "{} quesiti ({} a risposta multipla con {} opzioni, {} a completamento) in {} minuti per materia. \
                 {} corretta, {} errata, {} omessa. Superata con almeno {}.",
                rules.questions,
                rules.mcq,
                rules.options,
                rules.fill,
                rules.minutes,
                pts(rules.correct),
                pts(rules.wrong),
                pts(rules.omitted),
                rules.pass.to_string().replace('.', ",")
            )
        }));
        st.set_exam_sections(model(
            self.content
                .syllabus()
                .sections
                .iter()
                .map(|x| ExamSectionVM { title: ss(&x.title), summary: ss(&x.summary), body: ss(&x.body) })
                .collect::<Vec<_>>(),
        ));
    }

    pub(super) fn refresh_unit(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let Some(up) = self.progress.subjects.iter().flat_map(|s| s.units.iter()).find(|u| u.id == self.unit_id) else {
            return;
        };
        st.set_unit(UnitVM {
            id: ss(&up.id),
            n: up.n as i32,
            name: ss(&up.name),
            cfu: ss(it1(up.cfu).trim_end_matches(",0")),
            mastery: up.mastery as f32,
            touched: up.touched as f32,
            questions: up.questions as i32,
            cards: up.cards as i32,
            flagged: up.flagged as i32,
            note: ss(&up.ai_note),
            time: ss(if up.timed > 0 {
                format!("{} · media {}/dom.", hm(up.total_secs), mmss(up.avg_secs))
            } else {
                String::new()
            }),
        });
        // fonti delle domande dell'unità (si spengono qui o nel quiz personalizzato)
        let mut per_source: BTreeMap<String, usize> = BTreeMap::new();
        for q in &self.content.questions {
            if self.content.unit_of_topic(&q.topic).is_some_and(|u| u.id == self.unit_id) {
                *per_source.entry(q.source_group()).or_default() += 1;
            }
        }
        let mut src: Vec<(String, usize)> = per_source.into_iter().collect();
        src.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        st.set_unit_sources(model(
            src.into_iter()
                .map(|(g, n)| QzOptVM {
                    id: ss(&g),
                    name: ss(&g),
                    group: ss(""),
                    sel: !self.qz.off_sources.contains(&g),
                    count: n as i32,
                    head: false,
                    open: false,
                })
                .collect(),
        ));
        let pace = exam_pace(self.content.syllabus().exam.minutes, self.content.syllabus().exam.questions);
        let topics: Vec<TopicVM> = up
            .topics
            .iter()
            .map(|t| {
                let detail = if t.questions == 0 && t.cards == 0 {
                    "nessun contenuto ancora".to_string()
                } else {
                    let mut d = format!("{}/{} domande viste", t.seen_questions, t.questions);
                    if t.cards > 0 {
                        d.push_str(&format!(" · {}/{} carte", t.seen_cards, t.cards));
                    }
                    if t.attempts > 0 {
                        d.push_str(&format!(" · acc. {}%", (t.accuracy * 100.0).round()));
                    }
                    if t.flagged > 0 {
                        d.push_str(&format!(" · {} insicure", t.flagged));
                    }
                    if t.timed > 0 {
                        d.push_str(&format!(" · {} · media {}/dom.", hm(t.total_secs), mmss(t.avg_secs)));
                        if t.avg_secs > pace {
                            d.push_str(" (lento)");
                        }
                    }
                    d
                };
                TopicVM {
                    id: ss(&t.id),
                    name: ss(&t.name),
                    mastery: t.mastery as f32,
                    detail: ss(detail),
                    has_ai: t.ai.is_some(),
                    ai_text: ss(t.ai.map(|a| format!("AI {}%", (a * 100.0).round())).unwrap_or_default()),
                    note: ss(&t.ai_note),
                    questions: t.questions as i32,
                    line: ss(if t.attempts == 0 {
                        if t.questions == 0 && t.cards == 0 {
                            "nessun contenuto ancora".to_string()
                        } else {
                            format!("mai visto · {} domande", t.questions)
                        }
                    } else {
                        let mut l = format!("{}% giuste", (t.accuracy * 100.0).round());
                        if t.flagged > 0 {
                            l.push_str(&format!(" · {} insicure", t.flagged));
                        }
                        if t.timed > 0 {
                            l.push_str(" · ");
                        }
                        l
                    }),
                    secs: ss(if t.attempts > 0 && t.timed > 0 {
                        format!("{}″", t.avg_secs.round())
                    } else {
                        String::new()
                    }),
                    slow: t.timed > 0 && t.avg_secs > pace,
                    status: ss(match () {
                        _ if t.attempts == 0 => "mai visto",
                        _ if t.mastery >= 0.7 => "buono",
                        _ if t.mastery >= 0.4 => "in corso",
                        _ => "debole",
                    }),
                    level: if t.attempts == 0 {
                        0
                    } else if t.mastery >= 0.7 {
                        3
                    } else if t.mastery >= 0.4 {
                        2
                    } else {
                        1
                    },
                }
            })
            .collect();
        st.set_topics(model(topics));
    }

    pub(super) fn where_of(&self, topic: &str) -> String {
        match (self.content.subject_of_topic(topic), self.content.unit_of_topic(topic), self.content.topic(topic)) {
            (Some(s), Some(u), Some(t)) => format!("{} · U{} · {}", s.name, u.n, t.name),
            _ => topic.to_string(),
        }
    }

    pub(super) fn refresh_flags(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let mut ids: Vec<String> = self.history.flagged.iter().cloned().collect();
        ids.extend(self.history.last_wrong.iter().filter(|i| !self.history.flagged.contains(*i)).cloned());
        // tieni visibili anche quelle appena tolte (per poterle rimettere)
        ids.extend(self.pending_flags.iter().filter(|(_, on)| !**on).map(|(id, _)| id.clone()));
        ids.sort();
        ids.dedup();
        let mut quiz: Vec<FlagVM> = Vec::new();
        let mut cards: Vec<FlagVM> = Vec::new();
        for id in &ids {
            if let Some(q) = self.content.question(id) {
                quiz.push(FlagVM {
                    id: ss(id),
                    text: tx(&q.text),
                    r#where: ss(self.where_of(&q.topic)),
                    flagged: self.history.flagged.contains(id),
                    wrong: self.history.last_wrong.contains(id),
                });
            } else if let Some(c) = self.content.card(id) {
                cards.push(FlagVM {
                    id: ss(id),
                    text: tx(&c.front),
                    r#where: ss(self.where_of(&c.topic)),
                    flagged: self.history.flagged.contains(id),
                    wrong: false,
                });
            }
        }
        quiz.sort_by_key(|f| (!f.flagged, !f.wrong, f.r#where.to_string()));
        cards.sort_by_key(|f| (!f.flagged, f.r#where.to_string()));
        let ncards = self.flagged_cards().len();
        st.set_flags(model(quiz));
        st.set_card_flags(model(cards));
        st.set_card_flagged_count(ncards as i32);
    }

    pub(super) fn flagged_cards(&self) -> Vec<String> {
        self.history.flagged.iter().filter(|id| self.content.card(id).is_some()).cloned().collect()
    }
}

/// Icona e colore di una materia decisi dal pacchetto (vedi `prep_core::icons`).
pub(super) fn subject_look(subject: Option<&prep_core::model::Subject>) -> SubjectVM {
    use prep_core::icons::{self, Glyph};
    let Some(sub) = subject else { return SubjectVM::default() };
    let mut vm = SubjectVM::default();
    match icons::glyph(sub) {
        Glyph::Path { d, viewbox: (x, y, w, h) } => {
            vm.glyph = ss(d);
            (vm.vbx, vm.vby, vm.vbw, vm.vbh) = (x, y, w, h);
        }
        Glyph::Monogram(m) => vm.mono = ss(m),
    }
    if let Some(c) = icons::parse_color(&sub.color) {
        vm.color = slint::Color::from_argb_encoded(0xff00_0000 | c);
        vm.own_color = true;
    }
    vm
}
