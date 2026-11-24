//! Aggiornamento della versione Flatpak: dopo la verifica SHA-256 del bundle scaricato, lo
//! apre con il gestore di software di sistema (`xdg-open`) senza uscire dalla sandbox.

use std::path::Path;
use std::process::Command;

/// L'app gira dentro un Flatpak?
pub fn inside() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
}

/// Apre il bundle con il gestore di software di sistema (l'utente conferma lì).
pub fn install_and_restart(bundle: &Path) -> Result<bool, String> {
    Command::new("xdg-open").arg(bundle).spawn().map_err(|e| format!("impossibile aprire l'aggiornamento: {e}"))?;
    Ok(false)
}
