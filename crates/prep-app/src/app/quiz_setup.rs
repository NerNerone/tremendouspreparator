//! Scheda Quiz: quiz personalizzato (materie, fonti, tipi, numero di domande).

use super::*;

impl Ctrl {
    // ---------- quiz personalizzato ----------

    pub(super) fn refresh_quiz_setup(&mut self) {
        let Some(syl) = self.content.syllabus.as_ref() else { return };
        let all: Vec<String> = syl.subjects.iter().map(|s| s.id.clone()).collect();
        let qz = &mut self.qz;
        qz.subjects.retain(|s| all.contains(s));
        if !qz.init {
            qz.subjects = all.iter().cloned().collect();
            qz.init = true;
        }
        let qz = &self.qz;
        // conteggi in un solo passaggio
        let mut per_subject: BTreeMap<String, usize> = BTreeMap::new();
        let mut per_unit: BTreeMap<String, usize> = BTreeMap::new();
        let mut per_source: BTreeMap<String, usize> = BTreeMap::new();
        let mut available = 0usize;
        for q in &self.content.questions {
            if let Some(sub) = self.content.subject_of_topic(&q.topic) {
                *per_subject.entry(sub.id.clone()).or_default() += 1;
            }
            if let Some(u) = self.content.unit_of_topic(&q.topic) {
                *per_unit.entry(u.id.clone()).or_default() += 1;
            }
            if qz.keep_scope(&self.content, q) {
                let g = q.source_group();
                if !qz.off_sources.contains(&g) {
                    available += 1;
                }
                *per_source.entry(g).or_default() += 1;
            } else {
                per_source.entry(q.source_group()).or_default();
            }
        }
        let subjects: Vec<QzOptVM> = syl
            .subjects
            .iter()
            .map(|s| QzOptVM {
                id: ss(&s.id),
                name: ss(&s.name),
                group: ss(""),
                sel: qz.subjects.contains(&s.id),
                count: *per_subject.get(&s.id).unwrap_or(&0) as i32,
                head: false,
                open: false,
            })
            .collect();
        let mut units: Vec<QzOptVM> = Vec::new();
        for s in syl.subjects.iter().filter(|s| qz.subjects.contains(&s.id)) {
            let chosen = s.units.iter().filter(|u| qz.units.contains(&u.id)).count();
            let open = qz.open.contains(&s.id);
            units.push(QzOptVM {
                id: ss(&s.id),
                name: ss(&s.name),
                group: ss(if chosen == 0 {
                    "tutte le unità".to_string()
                } else {
                    format!("{chosen} di {} unità", s.units.len())
                }),
                sel: false,
                count: 0,
                head: true,
                open,
            });
            if !open {
                continue;
            }
            for u in &s.units {
                units.push(QzOptVM {
                    id: ss(&u.id),
                    name: ss(format!("U{} · {}", u.n, u.name)),
                    group: ss(&s.name),
                    sel: qz.units.contains(&u.id),
                    count: *per_unit.get(&u.id).unwrap_or(&0) as i32,
                    head: false,
                    open: false,
                });
            }
        }
        // fonti: prima le più ricche
        let mut src: Vec<(String, usize)> = per_source.into_iter().collect();
        src.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let sources: Vec<QzOptVM> = src
            .into_iter()
            .map(|(g, n)| QzOptVM {
                id: ss(&g),
                name: ss(&g),
                group: ss(""),
                sel: !qz.off_sources.contains(&g),
                count: n as i32,
                head: false,
                open: false,
            })
            .collect();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_qz_subjects(model(subjects));
        st.set_qz_units(model(units));
        st.set_qz_sources(model(sources));
        st.set_qz_mcq(!qz.no_mcq);
        st.set_qz_fill(!qz.no_fill);
        st.set_qz_n(qz.n as i32);
        st.set_qz_available(available as i32);
    }

    pub(super) fn qz_toggle(&mut self, kind: &str, id: &str) {
        let flip = |set: &mut BTreeSet<String>, id: &str| {
            if !set.remove(id) {
                set.insert(id.to_string());
            }
        };
        match kind {
            "subject" => {
                if self.qz.subjects.contains(id) {
                    self.qz.subjects.remove(id);
                    // via anche le sue unità spuntate
                    if let Some(sub) = self.content.subject(id) {
                        for u in &sub.units {
                            self.qz.units.remove(&u.id);
                        }
                    }
                } else {
                    self.qz.subjects.insert(id.to_string());
                }
            }
            "unit" => flip(&mut self.qz.units, id),
            "open" => flip(&mut self.qz.open, id),
            "source" => flip(&mut self.qz.off_sources, id),
            "kind" if id == "mcq" => self.qz.no_mcq = !self.qz.no_mcq,
            "kind" if id == "fill" => self.qz.no_fill = !self.qz.no_fill,
            "n" => self.qz.n = id.parse().unwrap_or(self.qz.n).clamp(1, 200),
            _ => {}
        }
        self.refresh_quiz_setup();
        if kind == "source" && !self.unit_id.is_empty() {
            self.refresh_unit();
        }
    }

    pub(super) fn qz_start(&mut self) {
        let qz = &self.qz;
        let content = &self.content;
        let ids = select::practice_where(content, &self.history, |q| qz.keep(content, q), qz.n);
        let subs: Vec<&prep_core::model::Subject> =
            content.syllabus().subjects.iter().filter(|s| qz.subjects.contains(&s.id)).collect();
        let subject = if subs.len() == 1 { Some(subs[0].id.clone()) } else { None };
        let unit = if qz.units.len() == 1 { qz.units.iter().next().cloned() } else { None };
        let title = match (subs.len(), &unit) {
            (_, Some(u)) => content.unit(u).map(|x| format!("U{} {}", x.n, x.name)).unwrap_or_else(|| "Quiz".into()),
            (1, None) => format!("Quiz · {}", subs[0].name),
            _ => "Quiz personalizzato".into(),
        };
        self.start_quiz(SessionKind::Practice, ids, title, subject, unit);
    }

    pub(super) fn refresh_settings_info(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_app_version(ss(format!("Versione {APP_VERSION}")));
        st.set_sync_status(ss(sync::status_line(&self.store)));
        st.set_sync_configured(self.sync_configured());
        st.set_on_android(cfg!(target_os = "android"));
        st.set_pack_status(ss(self.pack_status()));
    }

    pub(super) fn load_settings_into_ui(&self) {
        let s = self.store.settings();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_s_owner(ss(&s.owner));
        st.set_s_repo(ss(&s.repo));
        st.set_s_branch(ss(&s.branch));
        st.set_s_token(ss(&s.token));
        st.set_s_github_on(s.github_on);
        st.set_s_username(ss(&s.username));
        st.set_p2p_auto(s.p2p_auto);
        st.set_p2p_direct_only(s.direct_only);
        st.set_p2p_password(ss(""));
        st.set_p2p_relays(ss(if s.relays.is_empty() { p2p::DEFAULT_RELAYS.join(", ") } else { s.relays.join(", ") }));
        st.set_s_rem_on(s.reminder_enabled);
        let times = if s.reminder_times.is_empty() {
            prep_core::reminder::DEFAULT_TIMES.join(", ")
        } else {
            s.reminder_times.join(", ")
        };
        st.set_s_rem_time(ss(times));
        st.set_theme_mode(s.theme_mode as i32);
        self.refresh_rem_slots();
        self.news_fill();
    }
}
