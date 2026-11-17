//! Invio della segnalazione di un errore: si apre nel browser la issue già compilata nel
//! repository del progetto, che si legge e si decide se inviare.

use super::*;

/// Spiegazione nel dialogo che propone la segnalazione.
pub(super) const CRASH_TEXT: &str = "Puoi segnalarlo: si apre nel browser una segnalazione già compilata (versione dell'app, sistema e punto dell'errore, nessun dato di studio), che leggi prima di inviarla.";

impl Ctrl {
    pub(super) fn crash_send(&mut self, report: String) {
        if crate::open_url(&crate::crash::issue_url(&report)) {
            crate::crash::clear(&self.store.base);
        } else {
            self.toast("Non riesco ad aprire il browser");
        }
    }
}
