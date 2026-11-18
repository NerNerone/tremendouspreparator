//! Registro delle simulazioni: tutte quelle fatte per l'esame in uso, con media, migliore,
//! andamento e il dettaglio di ciascuna da rivedere (stessa pagina dei risultati).

use super::*;

/// Barre dell'andamento: le ultime N simulazioni.
const BARS: usize = 20;

impl Ctrl {
    /// Simulazioni dell'esame in uso, dalla più recente.
    fn exam_sessions(&self) -> Vec<&Session> {
        let mut v: Vec<&Session> =
            self.sessions.iter().filter(|s| s.kind == SessionKind::Exam && s.score.is_some()).collect();
        v.sort_by_key(|s| std::cmp::Reverse(s.ended_at));
        v
    }

    /// Nome di una simulazione: prova ufficiale, materia o tutto l'esame.
    fn sim_title(&self, s: &Session) -> String {
        let syl = self.content.syllabus();
        let subject = s.subject.as_deref().and_then(|id| self.content.subject(id)).map(|x| x.name.clone());
        if let Some(sid) = s.subject.as_deref() {
            for t in &syl.official {
                let prefix = format!("{sid}-{}-", t.tag);
                if !s.items.is_empty() && s.items.iter().all(|i| i.id.starts_with(&prefix)) {
                    return format!("{} · {}", subject.clone().unwrap_or_default(), t.label);
                }
            }
        }
        subject.unwrap_or_else(|| "Tutte le materie".into())
    }

    pub(super) fn open_simlog(&mut self) {
        self.refresh_simlog();
        self.set_page(P_SIMLOG);
    }

    /// Riepilogo (anche per le schede Studio e Quiz) e righe del registro.
    pub(super) fn refresh_simlog(&mut self) {
        let list = self.exam_sessions();
        let n = list.len();
        let scores: Vec<(f64, f64)> =
            list.iter().filter_map(|s| s.score.as_ref()).map(|sc| (sc.points, sc.max.max(1.0))).collect();
        let same_max = scores.windows(2).all(|w| (w[0].1 - w[1].1).abs() < 1e-9);
        let avg_frac = if n == 0 { 0.0 } else { scores.iter().map(|(p, m)| p / m).sum::<f64>() / n as f64 };
        let avg = if n == 0 {
            String::new()
        } else if same_max {
            let mean = scores.iter().map(|(p, _)| p).sum::<f64>() / n as f64;
            format!("{} / {}", it1(mean), it1(scores[0].1))
        } else {
            format!("{:.0}%", avg_frac * 100.0)
        };
        let best = list
            .iter()
            .filter_map(|s| s.score.as_ref())
            .max_by(|a, b| (a.points / a.max.max(1.0)).total_cmp(&(b.points / b.max.max(1.0))))
            .map(|sc| format!("{} / {}", it1(sc.points), it1(sc.max)))
            .unwrap_or_default();
        let passed = list.iter().filter(|s| s.score.as_ref().and_then(|sc| sc.passed) == Some(true)).count();
        let secs: Vec<i64> = list.iter().map(|s| (s.ended_at - s.started_at).num_seconds().max(0)).collect();
        let avg_time = if n == 0 { 0.0 } else { secs.iter().sum::<i64>() as f64 / n as f64 };
        let summary = if n == 0 {
            "Nessuna simulazione ancora".to_string()
        } else {
            format!("{n} {} · media {avg} · migliore {best}", if n == 1 { "simulazione" } else { "simulazioni" })
        };
        let details = if n == 0 {
            String::new()
        } else {
            format!(
                "Superate {passed} su {n} · media {:.0}% dei punti · durata media {}",
                avg_frac * 100.0,
                hm(avg_time)
            )
        };
        let rows: Vec<SimRowVM> = list
            .iter()
            .map(|s| {
                let sc = s.score.clone().unwrap_or_default();
                let at = s.ended_at.with_timezone(&Local);
                SimRowVM {
                    id: ss(&s.id),
                    date: ss(at.format("%d/%m/%Y · %H:%M").to_string()),
                    title: ss(self.sim_title(s)),
                    points: ss(format!("{} / {}", it1(sc.points), it1(sc.max))),
                    frac: (sc.points / sc.max.max(1.0)).clamp(0.0, 1.0) as f32,
                    has_pass: sc.passed.is_some(),
                    passed: sc.passed == Some(true),
                    detail: ss(format!(
                        "{} corrette · {} errate · {} omesse · {}",
                        sc.correct,
                        sc.wrong,
                        sc.omitted,
                        hm((s.ended_at - s.started_at).num_seconds().max(0) as f64)
                    )),
                }
            })
            .collect();
        // andamento: dalla più vecchia alla più recente
        let bars: Vec<f32> = rows.iter().take(BARS).rev().map(|r| r.frac).collect();
        let rules = self.content.syllabus().exam.clone();
        let pass_frac = if rules.questions > 0 {
            let max = rules.questions as f64 * rules.correct;
            if max > 0.0 {
                (rules.pass / max).clamp(0.0, 1.0) as f32
            } else {
                0.0
            }
        } else {
            0.0
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_sim_count(n as i32);
        st.set_sim_summary(ss(summary));
        st.set_sim_details(ss(details));
        st.set_sim_rows(model(rows));
        st.set_sim_bars(model(bars));
        st.set_sim_pass_frac(if same_max { pass_frac } else { 0.0 });
    }

    /// Riapre una simulazione del registro nella pagina dei risultati.
    pub(super) fn simlog_open(&mut self, id: &str) {
        let Some(s) = self.sessions.iter().find(|s| s.id == id).cloned() else { return };
        let title = self.sim_title(&s);
        // le risposte registrate usano le lettere originali: niente rimescolamento da applicare
        self.result_perm.clear();
        self.results_from_log = true;
        self.show_results(&s, &title);
    }
}

/// «1 h 05 min», «42 min».
fn hm(secs: f64) -> String {
    let m = (secs / 60.0).round() as i64;
    if m >= 60 {
        format!("{} h {:02} min", m / 60, m % 60)
    } else {
        format!("{m} min")
    }
}
