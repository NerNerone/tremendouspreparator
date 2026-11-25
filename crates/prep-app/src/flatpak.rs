//! Aggiornamento della versione Flatpak: dopo la verifica SHA-256 del bundle scaricato lo si
//! installa sul sistema con `flatpak install --bundle` (tramite `flatpak-spawn --host`, permesso
//! `org.freedesktop.Flatpak`) e l'app si riavvia da sola. Senza quel permesso (installazioni
//! vecchie) il bundle si apre con il gestore di software di sistema e si conferma lì.

use std::path::Path;
use std::process::Command;

/// L'app gira dentro un Flatpak?
pub fn inside() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
}

fn host(args: &[&str]) -> Result<String, String> {
    let out = Command::new("flatpak-spawn")
        .arg("--host")
        .args(args)
        .output()
        .map_err(|e| format!("flatpak-spawn non disponibile: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Installa il bundle e riavvia l'app: `Ok(true)` se l'app sta per riavviarsi (il chiamante
/// esce), `Ok(false)` se il bundle è stato aperto nel gestore di software (conferma lì).
pub fn install_and_restart(bundle: &Path) -> Result<bool, String> {
    let id = std::env::var("FLATPAK_ID").unwrap_or_else(|_| "io.github.nernerone.TremendousPreparator".into());
    // senza accesso al sistema: il gestore di software
    if host(&["true"]).is_err() {
        Command::new("xdg-open").arg(bundle).spawn().map_err(|e| format!("impossibile aprire l'aggiornamento: {e}"))?;
        return Ok(false);
    }
    // stessa installazione di adesso: per utente o di sistema
    let loc = host(&["flatpak", "info", "--show-location", &id]).unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let scope = if !home.is_empty() && loc.trim().starts_with(&home) { "--user" } else { "--system" };
    let path = bundle.to_string_lossy();
    host(&["flatpak", "install", scope, "--noninteractive", "-y", "--bundle", &path])
        .map_err(|e| format!("installazione non riuscita: {e}"))?;
    // riavvio: la nuova versione parte un attimo dopo che questa si è chiusa
    let cmd = format!("sleep 1; flatpak run {id}");
    Command::new("flatpak-spawn")
        .args(["--host", "sh", "-c", &cmd])
        .spawn()
        .map_err(|e| format!("riavvio non riuscito: {e}"))?;
    Ok(true)
}
