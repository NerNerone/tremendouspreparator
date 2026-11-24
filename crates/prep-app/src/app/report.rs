//! Invio della segnalazione di un errore (build privata): la issue si apre direttamente nel
//! repository dei progressi con il token delle Impostazioni. Se non si può (token senza il
//! permesso «Issues», niente rete) si apre nel browser la pagina già compilata dello stesso
//! repository.

use super::*;
use prep_core::github::{GitHub, RepoConfig};

/// Spiegazione nel dialogo che propone la segnalazione.
pub(super) const CRASH_TEXT: &str = "Puoi segnalarlo: diventa una issue nel tuo repository (versione dell'app, sistema e punto dell'errore, nessun dato di studio).";

impl Ctrl {
    pub(super) fn crash_send(&mut self, report: String) {
        let s = self.store.settings();
        let (owner, repo, token) = (s.owner.trim().to_string(), s.repo.trim().to_string(), s.token.trim().to_string());
        if owner.is_empty() || repo.is_empty() {
            self.toast("Imposta il repository GitHub nelle Opzioni per segnalare l'errore");
            return;
        }
        let page = format!("https://github.com/{owner}/{repo}");
        if token.is_empty() {
            self.crash_send_browser(&page, &report);
            return;
        }
        let gh = GitHub::new(RepoConfig { api_base: s.api_base.clone(), owner, repo, token });
        let title = format!("Errore: {}", crate::crash::summary(&report));
        let details: String = report.chars().take(60000).collect();
        let body = format!("Segnalazione inviata dall'app.\n\n```\n{details}\n```\n");
        self.toast("Invio la segnalazione…");
        spawn_background(move || {
            let r = gh.create_issue(&title, &body, &[]);
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| match r {
                    Ok(n) => {
                        crate::crash::clear(&c.store.base);
                        c.toast(format!("Segnalazione inviata (issue #{n})"));
                    }
                    Err(e) => {
                        eprintln!("segnalazione: {e}");
                        c.toast("Invio diretto non riuscito: apro la pagina della segnalazione");
                        c.crash_send_browser(&page, &report);
                    }
                })
            });
        });
    }

    fn crash_send_browser(&mut self, page: &str, report: &str) {
        if crate::open_url(&crate::crash::issue_url_for(page, report)) {
            crate::crash::clear(&self.store.base);
        } else {
            self.toast("Non riesco ad aprire il browser");
        }
    }
}
