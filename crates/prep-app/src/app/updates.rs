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

/// Versione più nuova di quella installata (`mine`) nella release `r` (JSON di GitHub), con
/// l'indirizzo del pacchetto adatto: l'APK per l'architettura `arch`, o il bundle Flatpak.
pub(super) fn newer(r: &serde_json::Value, mine: &str, arch: &str, flatpak: bool) -> Option<(String, String)> {
    let assets = r["assets"].as_array()?;
    let url_of = |ext: &str, want: &str| {
        assets.iter().find_map(|a| {
            let n = a["name"].as_str()?;
            (n.ends_with(ext) && n.contains(want)).then(|| a["browser_download_url"].as_str().map(str::to_string))?
        })
    };
    let abi = match arch {
        "aarch64" => "arm64-v8a",
        "arm" => "armeabi-v7a",
        _ => "",
    };
    let url = if flatpak {
        url_of(".flatpak", arch)?
    } else {
        (if abi.is_empty() { None } else { url_of(".apk", abi) })
            .or_else(|| url_of(".apk", "TremendousPreparator.apk"))?
    };
    let base = |v: &str| v.split('+').next().unwrap_or(v).to_string();
    match build_number(mine) {
        // beta: titolo «Beta 0.2.0+123-abc1234»
        Some(my_build) => {
            let remote = r["name"].as_str()?.trim_start_matches("Beta").trim().to_string();
            let rb = build_number(&remote)?;
            (rb > my_build && version_code(&base(&remote))? >= version_code(&base(mine))?).then_some((remote, url))
        }
        None => {
            let tag = r["tag_name"].as_str()?.trim_start_matches('v').to_string();
            (version_code(&tag)? > version_code(mine)?).then_some((tag, url))
        }
    }
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
fn latest() -> Result<Option<(String, String)>, String> {
    let mine = crate::app::APP_VERSION;
    let url = if build_number(mine).is_some() {
        format!("{API}/releases/tags/beta")
    } else {
        format!("{API}/releases/latest")
    };
    let mut resp = get(&url)?;
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
            if let Ok(Some((v, url))) = latest() {
                let _ = slint::invoke_from_event_loop(move || {
                    with_ctrl(|c| {
                        c.update_url = url;
                        c.ui().global::<AppState>().set_update_version(ss(&v));
                        c.toast(format!("È disponibile la versione {v}: Opzioni › Aggiorna"));
                    })
                });
            }
        });
        #[cfg(target_arch = "wasm32")]
        let _ = self;
    }

    /// «Aggiorna»: scarica l'APK (o il bundle Flatpak) e lo installa.
    pub(super) fn updates_install(&mut self) {
        if self.quiz.is_some() || self.study_state != prep_core::group::State::Idle {
            self.toast("Prima termina il quiz o la sessione di studio: l'aggiornamento riavvia l'app");
            return;
        }
        let url = self.update_url.clone();
        if url.is_empty() {
            return;
        }
        self.ui().global::<AppState>().set_update_busy(true);
        let dest = self.store.base.join("updates").join(if flatpak() {
            "TremendousPreparator-x86_64.flatpak"
        } else {
            "aggiornamento.apk"
        });
        #[cfg(not(target_arch = "wasm32"))]
        spawn_background(move || {
            let r = get(&url)
                .and_then(|mut resp| {
                    resp.body_mut().with_config().limit(200_000_000).read_to_vec().map_err(|e| e.to_string())
                })
                .and_then(|bytes| {
                    let _ = vfs::create_dir_all(dest.parent().unwrap_or(&dest));
                    vfs::write(&dest, &bytes).map_err(|e| e.to_string()).map(|_| dest)
                });
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| {
                    c.ui().global::<AppState>().set_update_busy(false);
                    if let Err(e) = r.and_then(|p| install(&p)) {
                        c.toast(format!("Aggiornamento non riuscito: {e}"));
                    }
                })
            });
        });
        #[cfg(target_arch = "wasm32")]
        let _ = (url, dest);
    }
}

/// Installa il pacchetto scaricato: APK (Android) o bundle Flatpak (computer).
#[cfg(not(target_arch = "wasm32"))]
fn install(p: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return crate::install_apk(p);
    #[cfg(not(target_os = "android"))]
    match crate::flatpak::install_and_restart(p)? {
        true => Ok(()),
        false => {
            Err("installazione diretta non permessa: conferma l'aggiornamento nel gestore di software che si è aperto"
                .into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_newer_builds_and_the_right_apk() {
        let rel = serde_json::json!({
            "name": "Beta 0.2.0+130-abc1234",
            "tag_name": "beta",
            "assets": [
                {"name": "TremendousPreparator.apk", "browser_download_url": "u/uni"},
                {"name": "TremendousPreparator-arm64-v8a.apk", "browser_download_url": "u/arm64"},
                {"name": "TremendousPreparator-armeabi-v7a.apk", "browser_download_url": "u/v7"}
            ]
        });
        assert_eq!(
            newer(&rel, "0.2.0+120-fff0000", "aarch64", false),
            Some(("0.2.0+130-abc1234".into(), "u/arm64".into()))
        );
        assert_eq!(newer(&rel, "0.2.0+120-fff0000", "arm", false).unwrap().1, "u/v7");
        assert_eq!(newer(&rel, "0.2.0+120-fff0000", "x86_64", false).unwrap().1, "u/uni");
        assert_eq!(newer(&rel, "0.2.0+130-abc1234", "aarch64", false), None);
        let stable = serde_json::json!({"tag_name": "v0.3.0", "assets": [{"name": "TremendousPreparator.apk", "browser_download_url": "s"}]});
        assert_eq!(newer(&stable, "0.2.0", "aarch64", false), Some(("0.3.0".into(), "s".into())));
        assert_eq!(newer(&stable, "0.3.0", "aarch64", false), None);
        assert_eq!(build_number("0.2.0"), None);
        // Flatpak: il bundle x86_64, solo se c'è nella release
        let fp = serde_json::json!({"tag_name": "v0.3.0", "assets": [
            {"name": "TremendousPreparator.apk", "browser_download_url": "s"},
            {"name": "TremendousPreparator-x86_64.flatpak", "browser_download_url": "f"}]});
        assert_eq!(newer(&fp, "0.2.0", "x86_64", true), Some(("0.3.0".into(), "f".into())));
        assert_eq!(newer(&stable, "0.2.0", "x86_64", true), None);
    }
}
