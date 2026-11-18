//! Aggiornamento della versione Flatpak: il bundle scaricato dalla release si installa sul
//! sistema con `flatpak-spawn --host flatpak install` (serve il permesso
//! `--talk-name=org.freedesktop.Flatpak` del manifest), poi l'app si riavvia. Se il permesso
//! manca (vecchie installazioni) il bundle si apre con il gestore di software del sistema.

use std::path::Path;
use std::process::Command;

/// L'app gira dentro un Flatpak?
pub fn inside() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
}

fn app_id() -> String {
    std::env::var("FLATPAK_ID").unwrap_or_default()
}

/// Installazione «--user» o «--system», da dove sta l'app.
fn installation() -> &'static str {
    let info = std::fs::read_to_string("/.flatpak-info").unwrap_or_default();
    let system = info.lines().any(|l| l.starts_with("app-path=") && l.contains("/var/lib/flatpak/"));
    if system {
        "--system"
    } else {
        "--user"
    }
}

/// Installa il bundle e riavvia l'app. `Ok(false)`: installazione diretta non permessa, il
/// bundle è stato aperto con il gestore di software (l'utente conferma lì).
pub fn install_and_restart(bundle: &Path) -> Result<bool, String> {
    let out = Command::new("flatpak-spawn")
        .args(["--host", "flatpak", "install", installation(), "--noninteractive", "-y", "--reinstall", "--bundle"])
        .arg(bundle)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let id = app_id();
            // la nuova versione parte un attimo dopo che questa si è chiusa
            let _ = Command::new("flatpak-spawn")
                .args(["--host", "sh", "-c", &format!("sleep 2; flatpak run {id}")])
                .spawn();
            std::thread::sleep(std::time::Duration::from_millis(300));
            std::process::exit(0);
        }
        _ => {
            Command::new("xdg-open")
                .arg(bundle)
                .spawn()
                .map_err(|e| format!("impossibile aprire l'aggiornamento: {e}"))?;
            Ok(false)
        }
    }
}
