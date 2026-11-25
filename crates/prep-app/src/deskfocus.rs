//! Concentrazione sul computer (Linux, anche nel Flatpak). Sul telefono il sistema blocca le
//! altre app; qui si fa restare la persona sulle app scelte: ogni 2 secondi si guardano i
//! programmi aperti, un'app del menu non permessa riceve un avviso e dopo 10 secondi viene
//! chiusa (SIGTERM, poi SIGKILL dopo altri 5). Nel Flatpak i comandi girano sul sistema con
//! `flatpak-spawn --host` (permesso `org.freedesktop.Flatpak`).
//!
//! Lo stato ha la stessa forma di quello Android (`MainActivity.focusState`), così il resto
//! dell'app non distingue: `active`, `until`, `allowed`, `total`, `session`, più `warn` e
//! `warn_left` per l'avviso.

use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// un'app del menu: id del file .desktop, nome, comando e app Flatpak (se lo è)
#[derive(Clone, Debug)]
struct App {
    id: String,
    name: String,
    exe: String,
    flatpak: Option<String>,
}

#[derive(Default)]
struct State {
    armed: bool,
    until: i64,
    total: i64,
    session: i32,
    allowed: HashSet<String>,
    /// app non permessa trovata aperta: da quando (per l'avviso e la chiusura)
    seen: HashMap<String, Instant>,
    warn: String,
    warn_left: i64,
    monitor: bool,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Mutex::default)
}

fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// Un comando sul sistema (fuori dalla sandbox se serve); l'output standard, se riesce.
fn host(args: &[&str]) -> Option<String> {
    let out = if in_flatpak() {
        Command::new("flatpak-spawn").arg("--host").args(args).output()
    } else {
        Command::new(args[0]).args(&args[1..]).output()
    }
    .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Si può usare: Linux con accesso ai programmi del sistema (nel Flatpak serve il permesso).
pub fn available() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| host(&["true"]).is_some() && host(&["ps", "-e", "-o", "pid="]).is_some())
}

/// le app che non si chiudono mai: questa, file, impostazioni, terminali
fn always_allowed(a: &App) -> bool {
    const KEEP: &[&str] = &[
        "tremendouspreparator",
        "nautilus",
        "dolphin",
        "thunar",
        "nemo",
        "pcmanfm",
        "settings",
        "control-center",
        "systemsettings",
        "terminal",
        "konsole",
        "kgx",
        "ptyxis",
        "alacritty",
        "kitty",
        "foot",
        "wezterm",
        "xterm",
        "tilix",
    ];
    // per parole (id con i punti, eseguibile): «foot» non deve tenere aperto «football-manager»
    let k = format!("{}.{}", a.id, a.exe).to_lowercase();
    KEEP.iter().any(|w| (w.len() > 8 && k.contains(w)) || k.split(['.', '-', '_', ' ']).any(|p| p == *w))
}

/// Le app del menu (file .desktop visibili), lette una volta e poi tenute.
fn apps() -> &'static Vec<App> {
    static A: OnceLock<Vec<App>> = OnceLock::new();
    A.get_or_init(|| {
        let script = r#"for f in /usr/share/applications/*.desktop /usr/local/share/applications/*.desktop "$HOME"/.local/share/applications/*.desktop /var/lib/flatpak/exports/share/applications/*.desktop "$HOME"/.local/share/flatpak/exports/share/applications/*.desktop /var/lib/snapd/desktop/applications/*.desktop; do [ -f "$f" ] && { echo "@@ $f"; sed -n '/^\[Desktop Entry\]/,/^\[/p' "$f" | grep -E '^(Name|Exec|NoDisplay|Hidden|Type|X-Flatpak)='; }; done; true"#;
        let Some(out) = host(&["sh", "-c", script]) else { return Vec::new() };
        let mut list: Vec<App> = Vec::new();
        let mut seen = HashSet::new();
        for block in out.split("@@ ").skip(1) {
            let mut lines = block.lines();
            let path = lines.next().unwrap_or_default();
            let id = path.rsplit('/').next().unwrap_or_default().trim_end_matches(".desktop").to_string();
            let (mut name, mut exec, mut hidden, mut kind, mut fp) = (None, None, false, None, None);
            for l in lines {
                let (k, v) = l.split_once('=').unwrap_or((l, ""));
                match k {
                    "Name" if name.is_none() => name = Some(v.to_string()),
                    "Exec" if exec.is_none() => exec = Some(v.to_string()),
                    "NoDisplay" | "Hidden" if v.eq_ignore_ascii_case("true") => hidden = true,
                    "Type" => kind = Some(v.to_string()),
                    "X-Flatpak" => fp = Some(v.to_string()),
                    _ => {}
                }
            }
            if hidden || kind.as_deref().is_some_and(|k| k != "Application") || !seen.insert(id.clone()) {
                continue;
            }
            let (Some(name), Some(exec)) = (name, exec) else { continue };
            let exe = exec_name(&exec);
            if exe.is_empty() {
                continue;
            }
            let flatpak = fp.or_else(|| if exe == "flatpak" { flatpak_app(&exec) } else { None });
            list.push(App { id, name, exe, flatpak });
        }
        list.sort_by_key(|a| a.name.to_lowercase());
        list
    })
}

/// nome del programma di una riga Exec (senza percorso, variabili d'ambiente e codici %)
fn exec_name(exec: &str) -> String {
    let mut parts = exec.split_whitespace().skip_while(|t| *t == "env" || t.contains('='));
    let first = parts.next().unwrap_or_default().trim_matches('"');
    first.rsplit('/').next().unwrap_or_default().to_string()
}

/// app Flatpak lanciata da `flatpak run … <app>`
fn flatpak_app(exec: &str) -> Option<String> {
    let mut it = exec.split_whitespace().skip_while(|t| *t != "run").skip(1);
    it.find(|t| !t.starts_with('-') && !t.starts_with('@') && t.contains('.')).map(str::to_string)
}

/// Elenco delle app per la scelta (JSON come su Android: `p` id, `l` nome).
pub fn apps_json() -> String {
    let list: Vec<serde_json::Value> =
        apps().iter().filter(|a| !always_allowed(a)).map(|a| serde_json::json!({ "p": a.id, "l": a.name })).collect();
    serde_json::Value::Array(list).to_string()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Avvia o allunga il blocco: `{"until": ms, "allowed": [...]}`.
pub fn start(json: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return };
    let until = v["until"].as_i64().unwrap_or(0);
    let allowed: HashSet<String> = v["allowed"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let spawn = {
        let mut s = state().lock().unwrap();
        if !s.armed {
            s.total = ((until - now_ms()) / 1000).max(0);
            s.session = 1;
        } else {
            s.total += ((until - s.until) / 1000).max(0);
        }
        s.armed = true;
        s.until = until;
        s.allowed = allowed;
        let spawn = !s.monitor;
        s.monitor = true;
        spawn
    };
    if spawn {
        std::thread::spawn(monitor);
    }
}

/// Stato della sessione di studio: 0 ferma, 1 in pista, 2 pit stop (finito il tempo del
/// blocco, la concentrazione dura finché la sessione è aperta, come su Android).
pub fn session(v: i32) {
    state().lock().unwrap().session = v;
}

fn active(s: &State) -> bool {
    s.armed && (s.until > now_ms() || s.session != 0)
}

/// Stato in JSON, nella forma di Android.
pub fn state_json() -> String {
    let mut s = state().lock().unwrap();
    let on = active(&s);
    if s.armed && !on {
        s.armed = false;
        s.seen.clear();
        s.warn.clear();
    }
    let allowed: Vec<&String> = s.allowed.iter().collect();
    serde_json::json!({
        "usage": true, "overlay": true, "restricted": false,
        "active": on, "armed": s.armed, "until": s.until, "total": s.total, "session": s.session,
        "allowed": allowed, "warn": s.warn, "warn_left": s.warn_left,
    })
    .to_string()
}

/// Apre un'app del menu.
pub fn launch(id: &str) {
    let script = format!(
        "gtk-launch '{0}' || kioclient exec '{0}.desktop' || kioclient5 exec '{0}.desktop'",
        id.replace('\'', "")
    );
    std::thread::spawn(move || {
        let _ = host(&["sh", "-c", &script]);
    });
}

/// Il ciclo di controllo: gira finché il blocco è attivo.
fn monitor() {
    loop {
        std::thread::sleep(Duration::from_secs(2));
        let (allowed, on) = {
            let s = state().lock().unwrap();
            (s.allowed.clone(), active(&s))
        };
        if !on {
            let mut s = state().lock().unwrap();
            s.monitor = false;
            s.seen.clear();
            s.warn.clear();
            return;
        }
        let running = running_apps();
        let bad: Vec<(&App, Running)> =
            running.into_iter().filter(|(a, _)| !allowed.contains(&a.id) && !always_allowed(a)).collect();
        let mut s = state().lock().unwrap();
        let ids: HashSet<String> = bad.iter().map(|(a, _)| a.id.clone()).collect();
        s.seen.retain(|k, _| ids.contains(k));
        let mut warn = String::new();
        let mut left = 0;
        for (a, r) in &bad {
            let since = *s.seen.entry(a.id.clone()).or_insert_with(Instant::now);
            let secs = since.elapsed().as_secs() as i64;
            if secs >= 15 {
                kill(a, r, true);
            } else if secs >= 10 {
                kill(a, r, false);
            }
            if warn.is_empty() {
                warn = a.name.clone();
                left = (10 - secs).max(0);
            }
        }
        s.warn = warn;
        s.warn_left = left;
    }
}

/// Un'app aperta: le unità systemd in cui gira (KDE e GNOME avviano ogni app in una sua
/// unità `app-…-<id del menu>-….scope`) e, in mancanza, i processi trovati per nome.
#[derive(Default)]
struct Running {
    units: Vec<String>,
    pids: Vec<u32>,
}

/// id del menu come compare nei nomi delle unità systemd («-» diventa «\x2d»)
fn unit_escape(id: &str) -> String {
    id.replace('-', "\\x2d")
}

/// l'unità `unit` è stata avviata per l'app `id`? (`app[-<lanciatore>]-<id>[-<n>.scope|@<n>.service|.service]`)
fn unit_is(unit: &str, id: &str) -> bool {
    let e = unit_escape(id);
    let body = unit.strip_suffix(".scope").or_else(|| unit.strip_suffix(".service")).unwrap_or(unit);
    let body = body.split('@').next().unwrap_or(body);
    body == format!("app-{e}")
        || body.ends_with(&format!("-{e}"))
        || body.contains(&format!("-{e}-"))
        || body.starts_with(&format!("app-{e}-"))
}

/// nome di programma senza estensioni da avvio (`soffice.bin`, `code-bin`, `run.sh`)
fn bare(name: &str) -> String {
    let n = name.rsplit('/').next().unwrap_or(name).to_lowercase();
    for suf in [".bin", "-bin", ".sh", ".py", ".exe", "-wrapper"] {
        if let Some(x) = n.strip_suffix(suf) {
            return x.to_string();
        }
    }
    n
}

/// App del menu aperte adesso.
fn running_apps() -> Vec<(&'static App, Running)> {
    let list = apps();
    let mut out: Vec<(&App, Running)> = Vec::new();
    let mut add = |a: &'static App, unit: Option<String>, pid: Option<u32>| {
        let i = match out.iter().position(|(x, _)| x.id == a.id) {
            Some(i) => i,
            None => {
                out.push((a, Running::default()));
                out.len() - 1
            }
        };
        let r = &mut out[i].1;
        if let Some(u) = unit {
            if !r.units.contains(&u) {
                r.units.push(u);
            }
        }
        if let Some(p) = pid {
            r.pids.push(p);
        }
    };
    // 1. unità systemd delle app (il modo più affidabile: prende anche i processi figli)
    if let Some(units) = host(&["systemctl", "--user", "list-units", "--type=scope,service", "--state=running", "--no-legend", "--plain", "app-*"]) {
        for l in units.lines() {
            let Some(unit) = l.split_whitespace().next() else { continue };
            if let Some(a) = list.iter().find(|a| unit_is(unit, &a.id) || a.flatpak.as_deref().is_some_and(|f| unit_is(unit, f))) {
                add(a, Some(unit.to_string()), None);
            }
        }
    }
    // 2. app Flatpak in esecuzione: la sandbox (bwrap) e il programma dentro
    if let Some(ps) = host(&["flatpak", "ps", "--columns=application,pid,child-pid"]) {
        for l in ps.lines() {
            let mut it = l.split_whitespace();
            let Some(app) = it.next() else { continue };
            if let Some(a) = list.iter().find(|a| a.flatpak.as_deref() == Some(app) || a.id == app) {
                for pid in it.filter_map(|p| p.parse::<u32>().ok()) {
                    add(a, None, Some(pid));
                }
            }
        }
    }
    // 3. programmi del sistema per nome: eseguibile, primo argomento o nome dell'id del menu
    let me = std::process::id();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    if let Some(ps) = host(&["ps", "-u", &whoami(), "-o", "pid=,ppid=,comm=,args="]) {
        for l in ps.lines() {
            let mut it = l.split_whitespace();
            let (Some(pid), Some(ppid), Some(comm)) =
                (it.next().and_then(|p| p.parse::<u32>().ok()), it.next().and_then(|p| p.parse::<u32>().ok()), it.next())
            else {
                continue;
            };
            children.entry(ppid).or_default().push(pid);
            if pid == me {
                continue;
            }
            let names = [bare(comm), bare(it.next().unwrap_or_default()), bare(it.next().unwrap_or_default())];
            let hit = list.iter().find(|a| {
                if a.flatpak.is_some() {
                    return false;
                }
                let exe = bare(&a.exe);
                let tail = a.id.rsplit('.').next().unwrap_or(&a.id).to_lowercase();
                names.iter().any(|n| !n.is_empty() && (*n == exe || (exe.len() > 3 && *n == tail)))
            });
            if let Some(a) = hit {
                add(a, None, Some(pid));
            }
        }
    }
    // con i processi figli (Steam, browser, giochi lanciati da un launcher): si chiude tutto
    for (_, r) in out.iter_mut() {
        let mut todo = r.pids.clone();
        while let Some(p) = todo.pop() {
            for c in children.get(&p).into_iter().flatten() {
                if *c != me && !r.pids.contains(c) {
                    r.pids.push(*c);
                    todo.push(*c);
                }
            }
        }
    }
    out
}

fn whoami() -> String {
    std::env::var("USER").unwrap_or_else(|_| host(&["id", "-un"]).unwrap_or_default().trim().to_string())
}

/// Chiude l'app: prima con garbo (SIGTERM, come la chiusura normale), poi `hard` (SIGKILL).
fn kill(a: &App, r: &Running, hard: bool) {
    let sig = if hard { "SIGKILL" } else { "SIGTERM" };
    for u in &r.units {
        let _ = host(&["systemctl", "--user", "kill", &format!("--signal={sig}"), u]);
    }
    if let Some(app) = &a.flatpak {
        let _ = host(&["flatpak", "kill", app]);
    }
    if !r.pids.is_empty() {
        let flag = if hard { "-KILL" } else { "-TERM" };
        let mut args = vec!["kill", flag];
        let p: Vec<String> = r.pids.iter().map(u32::to_string).collect();
        args.extend(p.iter().map(String::as_str));
        let _ = host(&args);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        assert!(unit_is("app-org.kde.konsole-12345.scope", "org.kde.konsole"));
        assert!(unit_is("app-gnome-firefox-4321.scope", "firefox"));
        assert!(unit_is("app-flatpak-com.discordapp.Discord-777.scope", "com.discordapp.Discord"));
        assert!(unit_is("app-org.gnome.Nautilus@abc.service", "org.gnome.Nautilus"));
        assert!(unit_is("app-gnome-libreoffice\\x2dwriter-99.scope", "libreoffice-writer"));
        assert!(!unit_is("app-gnome-firefox-4321.scope", "fire"));
        assert_eq!(bare("/usr/lib/libreoffice/program/soffice.bin"), "soffice");
        assert_eq!(bare("Discord"), "discord");
        let app = |id: &str, exe: &str| App { id: id.into(), name: id.into(), exe: exe.into(), flatpak: None };
        assert!(always_allowed(&app("org.gnome.Settings", "gnome-control-center")));
        assert!(always_allowed(&app("org.kde.konsole", "konsole")));
        assert!(!always_allowed(&app("football-manager", "fm")));
        assert!(!always_allowed(&app("steam", "steam")));
    }

    #[test]
    fn exec_lines() {
        assert_eq!(exec_name("/usr/bin/firefox %u"), "firefox");
        assert_eq!(exec_name("env GDK_BACKEND=x11 discord"), "discord");
        assert_eq!(
            flatpak_app("/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=wikipedia org.wikipedia %U")
                .as_deref(),
            Some("org.wikipedia")
        );
    }
}
