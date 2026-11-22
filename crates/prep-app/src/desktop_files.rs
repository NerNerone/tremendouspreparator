//! File sul computer (Linux): scelta di un file con la finestra di sistema (portale XDG,
//! funziona anche nel Flatpak; altrimenti zenity o kdialog) e salvataggio in Download.

use std::collections::HashMap;
use std::path::PathBuf;

/// Finestra «Apri file» di sistema; `None` se annullata o non disponibile.
pub fn pick_file() -> Option<PathBuf> {
    pick_file_titled("Aggiungi un file ai contenuti")
}

/// Come [`pick_file`], con il titolo della finestra.
pub fn pick_file_titled(title: &str) -> Option<PathBuf> {
    portal_open(title).or_else(|| {
        command("zenity", &["--file-selection", &format!("--title={title}")])
            .or_else(|| command("kdialog", &["--getopenfilename", "."]))
    })
}

fn command(cmd: &str, args: &[&str]) -> Option<PathBuf> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !p.is_empty()).then(|| PathBuf::from(p))
}

/// org.freedesktop.portal.FileChooser.OpenFile: la risposta arriva come segnale sulla
/// richiesta creata dal portale.
fn portal_open(title: &str) -> Option<PathBuf> {
    use zbus::zvariant::{OwnedValue, Value};
    let conn = zbus::blocking::Connection::session().ok()?;
    let token = format!("tp{}", std::process::id());
    let sender = conn.unique_name()?.trim_start_matches(':').replace('.', "_");
    let request = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    // ci si iscrive al segnale prima della chiamata, così la risposta non si perde
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")
        .ok()?
        .member("Response")
        .ok()?
        .path(request.as_str())
        .ok()?
        .build();
    let it = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(4)).ok()?;
    let opts: HashMap<&str, Value> =
        HashMap::from([("handle_token", Value::from(token.as_str())), ("modal", Value::from(true))]);
    conn.call_method(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        Some("org.freedesktop.portal.FileChooser"),
        "OpenFile",
        &("", title, opts),
    )
    .ok()?;
    for msg in it.flatten() {
        let Ok((code, results)) = msg.body().deserialize::<(u32, HashMap<String, OwnedValue>)>() else {
            continue;
        };
        if code != 0 {
            return None;
        }
        let uris: Vec<String> = results.get("uris")?.try_clone().ok()?.try_into().ok()?;
        let uri = uris.into_iter().next()?;
        return Some(PathBuf::from(decode(uri.strip_prefix("file://")?)));
    }
    None
}

/// Decodifica percentuale di un percorso `file://`.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Cartella Download dell'utente (XDG), anche se ha un nome tradotto.
pub fn downloads_dirs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("XDG_DOWNLOAD_DIR") {
        dirs.push(d.into());
    }
    // ~/.config/user-dirs.dirs: XDG_DOWNLOAD_DIR="$HOME/Scaricati"
    if let Ok(t) = std::fs::read_to_string(home.join(".config/user-dirs.dirs")) {
        for l in t.lines() {
            if let Some(v) = l.strip_prefix("XDG_DOWNLOAD_DIR=") {
                let v = v.trim().trim_matches('"').replace("$HOME", &home.to_string_lossy());
                dirs.push(v.into());
            }
        }
    }
    dirs.push(home.join("Downloads"));
    dirs.push(home.join("Scaricati"));
    dirs
}

fn sanitize_filename(name: &str) -> String {
    let base = std::path::Path::new(name).file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let mut clean: String = base.chars().filter(|c| !c.is_control() && *c != '/' && *c != '\\').collect();
    while clean.contains("..") {
        clean = clean.replace("..", "_");
    }
    let clean = clean.trim_matches('.');
    if clean.is_empty() || clean.chars().all(|c| c == '_') {
        "file".to_string()
    } else {
        clean.to_string()
    }
}

/// Salva `bytes` nella cartella Download (senza sovrascrivere: aggiunge un numero).
pub fn save_to_downloads(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = downloads_dirs().into_iter().find(|d| d.is_dir()).ok_or("cartella Download non trovata")?;
    let safe = sanitize_filename(name);
    let (stem, ext) = safe.split_once('.').unwrap_or((&safe, ""));
    let mut p = dir.join(&safe);
    let mut n = 2;
    while p.exists() {
        p = if ext.is_empty() { dir.join(format!("{stem}-{n}")) } else { dir.join(format!("{stem}-{n}.{ext}")) };
        n += 1;
    }
    std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_uri_paths() {
        assert_eq!(super::decode("/home/a/Quiz%20nuovi.json"), "/home/a/Quiz nuovi.json");
        assert_eq!(super::decode("/x/caff%C3%A8.md"), "/x/caffè.md");
        assert_eq!(super::decode("/x/100%"), "/x/100%");
    }

    #[test]
    fn sanitizes_filenames() {
        assert_eq!(super::sanitize_filename("../../.bashrc"), "bashrc");
        assert_eq!(super::sanitize_filename("a/b/..c.json"), "_c.json");
        assert_eq!(super::sanitize_filename("..."), "file");
        assert_eq!(super::sanitize_filename("esame.json.gz"), "esame.json.gz");
    }
}
