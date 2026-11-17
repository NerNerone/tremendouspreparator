//! Modalità concentrazione (Android).

use super::*;

impl Ctrl {
    // ---------- modalità concentrazione (Android) ----------

    /// Durate proposte (minuti); la più lunga è anche il massimo.
    const FOCUS_CHOICES: [u32; 5] = [25, 50, 90, 120, 180];

    pub(super) fn focus_minutes(&self) -> u32 {
        let m = self.store.settings().focus_minutes;
        if Self::FOCUS_CHOICES.contains(&m) {
            m
        } else {
            50
        }
    }

    /// Permessi, stato del blocco e lanciatore delle app permesse.
    pub(super) fn refresh_focus(&mut self) {
        if !crate::focus::available() {
            return;
        }
        let Some(j) = crate::focus::state().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
            return;
        };
        let usage = j["usage"].as_bool().unwrap_or(false);
        let overlay = j["overlay"].as_bool().unwrap_or(false);
        let active = j["active"].as_bool().unwrap_or(false);
        let until = j["until"].as_i64().unwrap_or(0);
        let now_ms = Utc::now().timestamp_millis();
        let allowed: Vec<String> = if active {
            j["allowed"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default()
        } else {
            self.store.settings().focus_allowed
        };
        if self.focus_apps.is_empty() && active {
            self.load_focus_apps();
        }
        let minutes = self.focus_minutes();
        let confirm = self.focus_confirm.is_some_and(|t| t.elapsed() < Duration::from_secs(6));
        if !confirm {
            self.focus_confirm = None;
        }
        let hm = |ms: i64| {
            chrono::DateTime::from_timestamp_millis(ms)
                .map(|d| d.with_timezone(&Local).format("%H:%M").to_string())
                .unwrap_or_default()
        };
        let status = if !active {
            String::new()
        } else if until > now_ms {
            format!(
                "Concentrazione attiva fino alle {} (ancora {} min), poi finché non termini la sessione",
                hm(until),
                (until - now_ms) / 60_000 + 1
            )
        } else {
            "Concentrazione attiva finché non termini la sessione (anche al pit stop)".to_string()
        };
        let vm = |p: &String, l: &String, i: &Option<slint::Image>, on: bool| FocusAppVM {
            pkg: ss(p),
            label: ss(l),
            has_icon: i.is_some(),
            icon: i.clone().unwrap_or_default(),
            allowed: on,
        };
        let all: Vec<FocusAppVM> = self.focus_apps.iter().map(|(p, l, i)| vm(p, l, i, allowed.contains(p))).collect();
        let chosen: Vec<FocusAppVM> =
            self.focus_apps.iter().filter(|(p, _, _)| allowed.contains(p)).map(|(p, l, i)| vm(p, l, i, true)).collect();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_focus_usage_ok(usage);
        st.set_focus_overlay_ok(overlay);
        st.set_focus_active(active);
        st.set_focus_status(ss(status));
        st.set_focus_minutes(minutes as i32);
        st.set_focus_confirm(confirm);
        st.set_focus_confirm_text(ss(format!(
            "Conferma: blocco fino alle {} (non si annulla)",
            hm(now_ms + minutes as i64 * 60_000)
        )));
        // i modelli si sostituiscono solo se cambiano (niente sfarfallii ogni secondo)
        let old = st.get_focus_apps();
        if old.row_count() != all.len()
            || (0..all.len()).any(|i| old.row_data(i).is_none_or(|r| r.allowed != all[i].allowed))
        {
            st.set_focus_apps(model(all));
        }
        let old = st.get_focus_launcher();
        if old.row_count() != chosen.len()
            || (0..chosen.len()).any(|i| old.row_data(i).is_none_or(|r| r.pkg != chosen[i].pkg))
        {
            st.set_focus_launcher(model(chosen));
        }
    }

    pub(super) fn load_focus_apps(&mut self) {
        use base64::Engine as _;
        let Some(t) = crate::focus::apps() else { return };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else { return };
        let mut apps: Vec<(String, String, Option<slint::Image>)> = v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        let p = x["p"].as_str()?.to_string();
                        let l = x["l"].as_str().unwrap_or(&p).to_string();
                        let icon = x["i"]
                            .as_str()
                            .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
                            .and_then(|b| crate::photo::to_image(&b));
                        Some((p, l, icon))
                    })
                    .collect()
            })
            .unwrap_or_default();
        apps.sort_by_key(|a| a.1.to_lowercase());
        self.focus_apps = apps;
    }

    pub(super) fn focus_setup(&mut self, open: bool) {
        self.ui().global::<AppState>().set_focus_open(open);
        if open {
            self.load_focus_apps();
        }
        self.focus_confirm = None;
        self.refresh_focus();
    }

    pub(super) fn focus_toggle(&mut self, pkg: &str) {
        let mut s = self.store.settings();
        if let Some(i) = s.focus_allowed.iter().position(|p| p == pkg) {
            s.focus_allowed.remove(i);
        } else {
            s.focus_allowed.push(pkg.to_string());
        }
        let _ = self.store.save_settings(&s);
        self.refresh_focus();
    }

    pub(super) fn focus_set_minutes(&mut self, m: i32) {
        let mut s = self.store.settings();
        s.focus_minutes = m.max(0) as u32;
        let _ = self.store.save_settings(&s);
        self.focus_confirm = None;
        self.refresh_focus();
    }

    /// Avvia concentrazione e sessione di studio, con un tocco di conferma (non si annulla).
    pub(super) fn focus_start(&mut self) {
        let j: serde_json::Value =
            crate::focus::state().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if !j["usage"].as_bool().unwrap_or(false) || !j["overlay"].as_bool().unwrap_or(false) {
            self.toast("Prima concedi «Accesso all'utilizzo» e «Mostra sopra altre app» qui sopra");
            return;
        }
        if !self.focus_confirm.is_some_and(|t| t.elapsed() < Duration::from_secs(6)) {
            self.focus_confirm = Some(Instant::now());
            self.refresh_focus();
            return;
        }
        self.focus_confirm = None;
        let minutes = self.focus_minutes() as i64;
        let until = Utc::now().timestamp_millis() + minutes * 60_000;
        let allowed = self.store.settings().focus_allowed;
        crate::focus::start(&serde_json::json!({ "until": until, "allowed": allowed }).to_string());
        self.ui().global::<AppState>().set_focus_open(false);
        self.study_set(prep_core::group::State::Studying);
        self.toast(format!("Concentrazione avviata per {minutes} minuti: buono studio!"));
        self.refresh_focus();
    }
}
