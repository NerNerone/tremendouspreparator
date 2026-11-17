//! Segnalazione degli errori gravi. Un panic (Rust) o un'eccezione non gestita (Java, su
//! Android) viene scritto in `crash/` nella cartella dati; al successivo avvio l'app propone di
//! segnalarlo. Non parte niente da solo: «Segnala» apre nel browser una issue già compilata
//! nel repository del progetto, che si legge e si decide se inviare.

use prep_core::vfs;
use std::path::{Path, PathBuf};

/// Lunghezza massima del rapporto nell'indirizzo della issue (i browser accettano ~8000
/// caratteri; il testo codificato si allunga).
const MAX_BODY: usize = 3500;

pub fn dir(base: &Path) -> PathBuf {
    base.join("crash")
}

fn header() -> String {
    format!(
        "Versione: {}\nSistema: {} {}\nData: {}",
        crate::app::APP_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    )
}

/// Registra i panic nella cartella `crash/`, poi lascia proseguire il comportamento normale.
pub fn install(base: &Path) {
    let d = dir(base);
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let text = format!("{}\n\n{info}\n\n{bt}", header());
        let name = chrono::Utc::now().format("rust-%Y%m%dT%H%M%S.txt").to_string();
        let _ = vfs::create_dir_all(&d);
        let _ = vfs::write(&d.join(name), text.as_bytes());
        prev(info);
    }));
}

/// Il rapporto più recente non ancora segnalato, se c'è.
pub fn pending(base: &Path) -> Option<(PathBuf, String)> {
    let mut files: Vec<PathBuf> =
        vfs::walk_files(&dir(base)).into_iter().filter(|p| p.extension().is_some_and(|e| e == "txt")).collect();
    files.sort();
    let p = files.pop()?;
    let t = vfs::read_to_string(&p).ok()?;
    Some((p, t))
}

/// Toglie tutti i rapporti (segnalati o ignorati).
pub fn clear(base: &Path) {
    for f in vfs::walk_files(&dir(base)) {
        let _ = vfs::remove_file(&f);
    }
}

/// Riassunto dell'errore per il titolo della issue: per un panic il messaggio e il punto
/// (`messaggio (file.rs:riga)`), per un'eccezione Java la sua prima riga.
pub fn summary(report: &str) -> String {
    let mut lines = report.lines().skip_while(|l| !l.trim().is_empty()).filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or("errore").trim();
    let text = match first.strip_prefix("panicked at ") {
        Some(loc) => {
            let loc = loc.trim_end_matches(':');
            let file = loc.rsplit('/').next().unwrap_or(loc);
            let file = file.rsplitn(2, ':').nth(1).unwrap_or(file);
            format!("{} ({file})", lines.next().unwrap_or("panic").trim())
        }
        None => first.to_string(),
    };
    text.chars().take(100).collect()
}

/// Indirizzo della pagina «nuova issue» del repository del progetto, già compilata.
#[allow(dead_code)] // non usata dalle build che inviano la segnalazione altrove
pub fn issue_url(report: &str) -> String {
    issue_url_for(env!("CARGO_PKG_REPOSITORY"), report)
}

/// Come [`issue_url`], per il repository all'indirizzo `repo`.
pub fn issue_url_for(repo: &str, report: &str) -> String {
    let repo = repo.trim_end_matches('/');
    let mut details: String = report.chars().take(MAX_BODY).collect();
    if details.len() < report.len() {
        details.push_str("\n…");
    }
    let body = format!(
        "**Cosa stavi facendo quando l'app si è chiusa?**\n\n\n\n**Dettagli** (scritti dall'app)\n\n```\n{details}\n```\n"
    );
    format!("{repo}/issues/new?title={}&body={}", encode(&format!("Errore: {}", summary(report))), encode(&body))
}

/// Codifica percentuale per i parametri di un indirizzo.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_from_report() {
        let r = "Versione: 0.2.0\nSistema: android aarch64\n\npanicked at src/app.rs:10:5:\nindice fuori dai limiti\n\nstack…";
        assert_eq!(summary(r), "indice fuori dai limiti (app.rs:10)");
        assert_eq!(summary("V\n\njava.lang.IllegalStateException: no\n\tat x"), "java.lang.IllegalStateException: no");
        let u = issue_url(r);
        assert!(u.contains("/issues/new?title=Errore%3A%20indice"));
        assert!(!u.contains(' ') && !u.contains('\n'));
        let long = format!("x\n\n{}", "y".repeat(20000));
        assert!(issue_url(&long).len() < 12000);
    }
}
