//! Avvio desktop (Linux/Flatpak e sviluppo): `cargo run --bin prepapp-desktop`.
//! Dati in `$PREP_DATA`, altrimenti in `$XDG_DATA_HOME/tremendous-preparator`
//! (in Flatpak: `~/.var/app/io.github.nernerone.TremendousPreparator/data/tremendous-preparator`).
use std::path::PathBuf;

fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("PREP_DATA") {
        return d.into();
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("tremendous-preparator")
}

fn main() {
    if std::env::args().any(|a| a == "--promemoria") {
        prepapp::desktop_reminder::run_daemon(&data_dir());
        return;
    }
    prepapp::install_fonts(&data_dir());
    prepapp::run(data_dir()).expect("avvio UI");
}
