//! Aggiornamenti dell'app dalle release del progetto su GitHub, solo negli APK e nel Flatpak
//! pubblicati lì (la CI compila con `TP_UPDATES=github`; le build di F-Droid ne restano senza).
//! Una beta controlla la release «beta» (numero di build più alto), una versione stabile l'ultima
//! release. Su Android l'APK va all'installatore di sistema; nel Flatpak il bundle si installa
//! con `flatpak` sull'host (vedi `crate::flatpak`).
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use super::*;

/// Canale di distribuzione scritto dalla CI («github»; vuoto altrove).
const BUILD: &str = env!("TP_UPDATES");
const API: &str = "https://api.github.com/repos/NerNerone/tremendouspreparator";

static CHECKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Aggiornamenti attivi in questa build.
pub(super) fn enabled() -> bool {
    BUILD.eq_ignore_ascii_case("github") && (cfg!(target_os = "android") || flatpak())
}

/// Versione Flatpak (computer Linux).
fn flatpak() -> bool {
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    return crate::flatpak::inside();
    #[cfg(any(target_os = "android", target_arch = "wasm32"))]
    false
}

/// Numero di build di una versione beta: «0.2.0+123-abc1234» → 123.
pub(super) fn build_number(v: &str) -> Option<u64> {
    v.split_once('+')?.1.split('-').next()?.trim().parse().ok()
}

/// versionCode Android di «a.b.c» (stesso calcolo di cargo-apk).
pub(super) fn version_code(v: &str) -> Option<u32> {
    let mut it = v.split(['.', '-', '+']).map(|p| p.trim().parse::<u8>().ok());
    let (a, b, c) = (it.next()??, it.next()??, it.next()??);
    Some(1 << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32)
}

const RELEASE_DL_PREFIX: &str = "https://github.com/NerNerone/tremendouspreparator/releases/download/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub asset_name: String,
    pub sha256: Option<String>,
    pub sums_url: String,
}

pub(super) fn allowed_download_url(url: &str) -> bool {
    url.starts_with(RELEASE_DL_PREFIX)
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
        && !url.contains("..")
        && !url.contains('@')
}

fn valid_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn parse_asset_digest(a: &serde_json::Value) -> Option<String> {
    let d = a["digest"].as_str()?;
    let hex = d.strip_prefix("sha256:").unwrap_or(d).trim();
    valid_sha256_hex(hex).then(|| hex.to_ascii_lowercase())
}

pub(super) fn parse_sha256sums(body: &str, asset_name: &str) -> Option<String> {
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(sum) = parts.next() else { continue };
        let Some(file) = parts.next() else { continue };
        let clean = file.trim_start_matches('*');
        if clean == asset_name && valid_sha256_hex(sum) {
            return Some(sum.to_ascii_lowercase());
        }
    }
    None
}

/// Versione più nuova di quella installata (`mine`) nella release `r` (JSON di GitHub), con
/// l'indirizzo verificato del pacchetto adatto e il digest atteso.
pub(super) fn newer(r: &serde_json::Value, mine: &str, arch: &str, flatpak: bool) -> Option<UpdateInfo> {
    let assets = r["assets"].as_array()?;
    let asset_of = |ext: &str, want: &str| {
        assets.iter().find_map(|a| {
            let n = a["name"].as_str()?;
            let url = a["browser_download_url"].as_str()?;
            (n.ends_with(ext) && n.contains(want) && allowed_download_url(url))
                .then(|| (n.to_string(), url.to_string(), parse_asset_digest(a)))
        })
    };
    let abi = match arch {
        "aarch64" => "arm64-v8a",
        "arm" => "armeabi-v7a",
        _ => "",
    };
    let (asset_name, url, sha256) = if flatpak {
        asset_of(".flatpak", arch)?
    } else {
        (if abi.is_empty() { None } else { asset_of(".apk", abi) })
            .or_else(|| asset_of(".apk", "TremendousPreparator.apk"))?
    };
    let sums_url = assets
        .iter()
        .find_map(|a| {
            let n = a["name"].as_str()?;
            let url = a["browser_download_url"].as_str()?;
            (n == "SHA256SUMS" && allowed_download_url(url)).then(|| url.to_string())
        })
        .unwrap_or_default();
    let base = |v: &str| v.split('+').next().unwrap_or(v).to_string();
    let version = match build_number(mine) {
        // beta: titolo «Beta 0.2.0+123-abc1234»
        Some(my_build) => {
            let remote = r["name"].as_str()?.trim_start_matches("Beta").trim().to_string();
            let rb = build_number(&remote)?;
            (rb > my_build && version_code(&base(&remote))? >= version_code(&base(mine))?).then_some(remote)?
        }
        None => {
            let tag = r["tag_name"].as_str()?.trim_start_matches('v').to_string();
            (version_code(&tag)? > version_code(mine)?).then_some(tag)?
        }
    };
    Some(UpdateInfo { version, url, asset_name, sha256, sums_url })
}

#[cfg(not(target_arch = "wasm32"))]
fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    ureq::get(url)
        .header("User-Agent", "TremendousPreparator")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn latest() -> Result<Option<UpdateInfo>, String> {
    let mine = crate::app::APP_VERSION;
    let url = if build_number(mine).is_some() {
        format!("{API}/releases/tags/beta")
    } else {
        format!("{API}/releases/latest")
    };
    // nessuna release (stabile o beta) pubblicata: niente da aggiornare
    let mut resp = match ureq::get(&url)
        .header("User-Agent", "TremendousPreparator")
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        r => r.map_err(|e| e.to_string())?,
    };
    let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(newer(&v, mine, std::env::consts::ARCH, flatpak()))
}

impl Ctrl {
    /// All'avvio, una volta: c'è una versione più nuova? (la si propone, non si installa da sola)
    pub(super) fn updates_check(&mut self) {
        if !enabled() || CHECKED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        spawn_background(move || {
            if let Ok(Some(info)) = latest() {
                let _ = slint::invoke_from_event_loop(move || {
                    with_ctrl(|c| {
                        c.update_url = info.url;
                        c.update_asset = info.asset_name;
                        c.update_sha256 = info.sha256;
                        c.update_sums_url = info.sums_url;
                        c.ui().global::<AppState>().set_update_version(ss(&info.version));
                        c.toast(format!("È disponibile la versione {}: Tu › App › Aggiorna", info.version));
                    })
                });
            }
        });
        #[cfg(target_arch = "wasm32")]
        let _ = self;
    }

    /// «Cerca aggiornamenti» nelle Opzioni: come all'avvio, ma subito e con l'esito a schermo.
    pub(super) fn updates_check_now(&mut self) {
        if !enabled() {
            return;
        }
        self.ui().global::<AppState>().set_update_checking(true);
        #[cfg(not(target_arch = "wasm32"))]
        spawn_background(move || {
            let r = latest();
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| {
                    c.ui().global::<AppState>().set_update_checking(false);
                    match r {
                        Ok(Some(info)) => {
                            c.update_url = info.url;
                            c.update_asset = info.asset_name;
                            c.update_sha256 = info.sha256;
                            c.update_sums_url = info.sums_url;
                            c.ui().global::<AppState>().set_update_version(ss(&info.version));
                            c.toast(format!("È disponibile la versione {}: tocca «Aggiorna»", info.version));
                        }
                        Ok(None) => c.toast(format!("Hai già l'ultima versione ({})", crate::app::APP_VERSION)),
                        Err(e) => c.toast(format!("Controllo non riuscito: {e}")),
                    }
                })
            });
        });
    }

    /// «Aggiorna»: scarica l'APK (o il bundle Flatpak), ne verifica lo SHA-256 e lo installa.
    pub(super) fn updates_install(&mut self) {
        if self.quiz.is_some() || self.study_state != prep_core::group::State::Idle {
            self.toast("Prima termina il quiz o la sessione di studio: l'aggiornamento riavvia l'app");
            return;
        }
        let url = self.update_url.clone();
        if url.is_empty() || !allowed_download_url(&url) {
            return;
        }
        let asset_name = self.update_asset.clone();
        let expected_sha = self.update_sha256.clone();
        let sums_url = self.update_sums_url.clone();
        self.ui().global::<AppState>().set_update_busy(true);
        let dest = self.store.base.join("updates").join(if flatpak() {
            "TremendousPreparator-x86_64.flatpak"
        } else {
            "aggiornamento.apk"
        });
        #[cfg(not(target_arch = "wasm32"))]
        spawn_background(move || {
            let r = (|| -> Result<std::path::PathBuf, String> {
                if !allowed_download_url(&url) {
                    return Err("indirizzo di aggiornamento non consentito".into());
                }
                let mut resp = get(&url)?;
                let bytes =
                    resp.body_mut().with_config().limit(200_000_000).read_to_vec().map_err(|e| e.to_string())?;
                use sha2::{Digest, Sha256};
                let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
                let expected = if let Some(h) = expected_sha {
                    Some(h)
                } else if !sums_url.is_empty() && allowed_download_url(&sums_url) {
                    let mut sresp = get(&sums_url)?;
                    let body =
                        sresp.body_mut().with_config().limit(262_144).read_to_vec().map_err(|e| e.to_string())?;
                    let txt = String::from_utf8_lossy(&body);
                    parse_sha256sums(&txt, &asset_name)
                } else {
                    None
                };
                let Some(exp) = expected else {
                    return Err("verifica SHA-256 non riuscita: digest di rilascio assente".into());
                };
                if !exp.eq_ignore_ascii_case(&actual) {
                    return Err("verifica SHA-256 non riuscita: pacchetto alterato".into());
                }
                let _ = vfs::create_dir_all(dest.parent().unwrap_or(&dest));
                vfs::write(&dest, &bytes).map_err(|e| e.to_string())?;
                Ok(dest)
            })();
            // Android: l'APK passa all'installatore di sistema dal thread dell'interfaccia
            #[cfg(target_os = "android")]
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| {
                    c.ui().global::<AppState>().set_update_busy(false);
                    if let Err(e) = r.and_then(|p| crate::install_apk(&p)) {
                        c.toast(format!("Aggiornamento non riuscito: {e}"));
                    }
                })
            });
            // Flatpak: l'installazione può durare, resta qui; se riesce l'app si riavvia
            #[cfg(not(target_os = "android"))]
            {
                let r = r.and_then(|p| crate::flatpak::install_and_restart(&p));
                let _ = slint::invoke_from_event_loop(move || {
                    with_ctrl(|c| {
                        c.ui().global::<AppState>().set_update_busy(false);
                        match r {
                            Ok(_) => c.toast("Si è aperto il gestore di software: conferma lì l'aggiornamento"),
                            Err(e) => c.toast(format!("Aggiornamento non riuscito: {e}")),
                        }
                    })
                });
            }
        });
        #[cfg(target_arch = "wasm32")]
        let _ = (url, asset_name, expected_sha, sums_url, dest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://github.com/NerNerone/tremendouspreparator/releases/download/";

    #[test]
    fn picks_newer_builds_and_the_right_apk() {
        let rel = serde_json::json!({
            "name": "Beta 0.2.0+130-abc1234",
            "tag_name": "beta",
            "assets": [
                {"name": "TremendousPreparator.apk", "browser_download_url": format!("{BASE}beta/TremendousPreparator.apk")},
                {"name": "TremendousPreparator-arm64-v8a.apk", "browser_download_url": format!("{BASE}beta/TremendousPreparator-arm64-v8a.apk"), "digest": format!("sha256:{}", "a".repeat(64))},
                {"name": "TremendousPreparator-armeabi-v7a.apk", "browser_download_url": format!("{BASE}beta/TremendousPreparator-armeabi-v7a.apk")}
            ]
        });
        let got = newer(&rel, "0.2.0+120-fff0000", "aarch64", false).unwrap();
        assert_eq!(got.version, "0.2.0+130-abc1234");
        assert_eq!(got.url, format!("{BASE}beta/TremendousPreparator-arm64-v8a.apk"));
        assert_eq!(got.sha256.as_deref(), Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert_eq!(
            newer(&rel, "0.2.0+120-fff0000", "arm", false).unwrap().url,
            format!("{BASE}beta/TremendousPreparator-armeabi-v7a.apk")
        );
        assert_eq!(
            newer(&rel, "0.2.0+120-fff0000", "x86_64", false).unwrap().url,
            format!("{BASE}beta/TremendousPreparator.apk")
        );
        assert_eq!(newer(&rel, "0.2.0+130-abc1234", "aarch64", false), None);
        let stable = serde_json::json!({
            "tag_name": "v0.3.0",
            "assets": [{"name": "TremendousPreparator.apk", "browser_download_url": format!("{BASE}v0.3.0/TremendousPreparator.apk")}]
        });
        assert_eq!(
            newer(&stable, "0.2.0", "aarch64", false).unwrap().url,
            format!("{BASE}v0.3.0/TremendousPreparator.apk")
        );
        assert_eq!(newer(&stable, "0.3.0", "aarch64", false), None);
        assert_eq!(build_number("0.2.0"), None);
        // Flatpak: il bundle x86_64, solo se c'è nella release
        let fp = serde_json::json!({"tag_name": "v0.3.0", "assets": [
            {"name": "TremendousPreparator.apk", "browser_download_url": format!("{BASE}v0.3.0/TremendousPreparator.apk")},
            {"name": "TremendousPreparator-x86_64.flatpak", "browser_download_url": format!("{BASE}v0.3.0/TremendousPreparator-x86_64.flatpak")}]});
        assert_eq!(
            newer(&fp, "0.2.0", "x86_64", true).unwrap().url,
            format!("{BASE}v0.3.0/TremendousPreparator-x86_64.flatpak")
        );
        assert_eq!(newer(&stable, "0.2.0", "x86_64", true), None);
        // URL fuori allowlist o con traversal sono rifiutati
        assert!(!allowed_download_url("https://evil.example/TremendousPreparator.apk"));
        assert!(!allowed_download_url("https://github.com/NerNerone/tremendouspreparator/releases/download/../x"));
        assert!(!allowed_download_url("http://github.com/NerNerone/tremendouspreparator/releases/download/v1/a.apk"));
        let evil = serde_json::json!({"tag_name": "v0.3.0", "assets": [
            {"name": "TremendousPreparator.apk", "browser_download_url": "https://evil.example/app.apk"}
        ]});
        assert!(newer(&evil, "0.2.0", "aarch64", false).is_none());
        let sums =
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  TremendousPreparator-x86_64.flatpak\n";
        assert_eq!(
            parse_sha256sums(sums, "TremendousPreparator-x86_64.flatpak").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
    }
}
