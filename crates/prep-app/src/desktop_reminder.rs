//! Promemoria giornaliero sul desktop Linux (Flatpak o binario).
//!
//! `semestre-aperto --promemoria` è un processo leggero senza finestra: ogni minuto rilegge
//! `reminder.json` (scritto dall'app) e all'orario scelto invia una notifica tramite
//! `org.freedesktop.Notifications`; cliccandola si apre l'app. Parte all'accesso grazie a
//! un file in `~/.config/autostart` e l'app lo avvia anche subito. Una sola istanza alla
//! volta (socket astratto come lucchetto).

use chrono::Local;
use prep_core::reminder::ReminderState;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const APP_ID: &str = "it.tremendouspreparator.App";
const LOCK: &[u8] = b"semestre-aperto-promemoria";

fn in_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some()
}

fn autostart_file() -> Option<PathBuf> {
    // in Flatpak XDG_CONFIG_HOME punta dentro ~/.var/app: serve la cartella reale dell'host
    let base = if in_flatpak() {
        PathBuf::from(std::env::var_os("HOME")?).join(".config")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
    };
    Some(base.join("autostart").join(format!("{APP_ID}.promemoria.desktop")))
}

fn exec_line() -> String {
    if in_flatpak() {
        format!("flatpak run --command=semestre-aperto {APP_ID} --promemoria")
    } else {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "semestre-aperto".into());
        format!("\"{exe}\" --promemoria")
    }
}

/// Crea o rimuove l'avvio automatico e, se attivo, fa partire subito il processo.
pub fn sync_autostart(enabled: bool) {
    if std::env::var_os("PREP_DEMO").is_some() {
        return;
    }
    if let Some(f) = autostart_file() {
        if enabled {
            let body = format!(
                "[Desktop Entry]\nType=Application\nName=Tremendous Preparator (promemoria)\n\
                 Comment=Promemoria giornaliero di studio\nExec={}\nIcon={APP_ID}\n\
                 NoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
                exec_line()
            );
            if std::fs::read_to_string(&f).ok().as_deref() != Some(body.as_str()) {
                let _ = std::fs::create_dir_all(f.parent().unwrap());
                let _ = std::fs::write(&f, body);
            }
        } else {
            let _ = std::fs::remove_file(&f);
        }
    }
    if enabled {
        if let Ok(exe) = std::env::current_exe() {
            if let Ok(mut child) =
                Command::new(exe).arg("--promemoria").stdin(Stdio::null()).stdout(Stdio::null()).spawn()
            {
                // se un'altra istanza è già attiva il figlio esce subito: evita lo zombie
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
        }
    }
}

fn single_instance() -> Option<std::os::unix::net::UnixListener> {
    use std::os::linux::net::SocketAddrExt;
    let addr = std::os::unix::net::SocketAddr::from_abstract_name(LOCK).ok()?;
    std::os::unix::net::UnixListener::bind_addr(&addr).ok()
}

/// Corpo del processo `--promemoria`: ogni minuto controlla se è scattata una fascia
/// (mattina, dopo pranzo, pomeriggio, sera) e la notifica una sola volta.
pub fn run_daemon(data_dir: &Path) {
    let Some(_lock) = single_instance() else {
        return;
    };
    // fasce già notificate ("AAAA-MM-GG HH:MM"), salvate: un nuovo accesso non ripete
    let fired_file = data_dir.join("reminder_fired.txt");
    let mut fired: Vec<String> = std::fs::read_to_string(&fired_file)
        .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| l.len() > 10).collect())
        .unwrap_or_default();
    loop {
        if let Some(st) = ReminderState::load(data_dir) {
            let now = Local::now().naive_local();
            let today = now.date();
            let ds = today.format("%Y-%m-%d").to_string();
            // testo pronto scritto dall'app; se è vecchio (app non aperta da ieri) lo si calcola
            let mut fresh = st.clone();
            if !st.slots.iter().any(|s| s.date == ds) {
                fresh.studied_today = 0;
                fresh.compute_slots(today);
            }
            if st.enabled {
                if let Some(slot) = fresh.due_slot(now) {
                    let key = format!("{} {:02}:{:02}", slot.date, slot.hour, slot.minute);
                    if !fired.contains(&key) {
                        fired.push(key);
                        fired.retain(|k| k.starts_with(&ds));
                        let _ = std::fs::write(&fired_file, fired.join("\n"));
                        if !slot.title.is_empty() {
                            if let Err(e) = notify(&slot.title, &slot.text) {
                                eprintln!("promemoria: notifica non inviata: {e}");
                            }
                        }
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn open_app() {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("semestre-aperto"));
    if let Ok(mut c) = Command::new(exe).stdin(Stdio::null()).spawn() {
        std::thread::spawn(move || {
            let _ = c.wait();
        });
    }
}

fn notify(title: &str, body: &str) -> zbus::Result<()> {
    use zbus::zvariant::Value;
    let conn = zbus::blocking::Connection::session()?;
    let hints: HashMap<&str, Value> = HashMap::from([("desktop-entry", Value::from(APP_ID))]);
    let reply = conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &("Tremendous Preparator", 0u32, APP_ID, title, body, vec!["default", "Apri"], hints, -1i32),
    )?;
    let id: u32 = reply.body().deserialize()?;
    // clic sulla notifica → apri l'app (in ascolto per qualche ora al massimo)
    std::thread::spawn(move || {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.Notifications")
            .and_then(|b| b.member("ActionInvoked"))
            .map(|b| b.build());
        let Ok(rule) = rule else { return };
        let Ok(it) = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(8)) else {
            return;
        };
        let start = std::time::Instant::now();
        for msg in it.flatten() {
            if let Ok((nid, _action)) = msg.body().deserialize::<(u32, String)>() {
                if nid == id {
                    open_app();
                    return;
                }
            }
            if start.elapsed() > Duration::from_secs(4 * 3600) {
                return;
            }
        }
    });
    Ok(())
}
