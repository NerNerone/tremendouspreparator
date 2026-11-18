//! Aggiornamenti dell'app dalle release del progetto su GitHub, solo negli APK pubblicati lì
//! (la CI compila con `TP_UPDATES=github`; le build di F-Droid ne restano senza). Una beta
//! controlla la release «beta» (numero di build più alto), una versione stabile l'ultima release.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use super::*;

/// Canale di distribuzione scritto dalla CI («github»; vuoto altrove).
const BUILD: &str = env!("TP_UPDATES");
const API: &str = "https://api.github.com/repos/NerNerone/tremendouspreparator";

static CHECKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Aggiornamenti attivi in questa build.
pub(super) fn enabled() -> bool {
    cfg!(target_os = "android") && BUILD.eq_ignore_ascii_case("github")
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
/// l'indirizzo dell'APK adatto all'architettura `arch`.
pub(super) fn newer(r: &serde_json::Value, mine: &str, arch: &str) -> Option<(String, String)> {
    let assets = r["assets"].as_array()?;
    let url_of = |want: &str| {
        assets.iter().find_map(|a| {
            let n = a["name"].as_str()?;
            (n.ends_with(".apk") && n.contains(want)).then(|| a["browser_download_url"].as_str().map(str::to_string))?
        })
    };
    let abi = match arch {
        "aarch64" => "arm64-v8a",
        "arm" => "armeabi-v7a",
        _ => "",
    };
    let url = (if abi.is_empty() { None } else { url_of(abi) }).or_else(|| url_of("TremendousPreparator.apk"))?;
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

#[cfg(target_os = "android")]
fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    ureq::get(url)
        .header("User-Agent", "TremendousPreparator")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
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
    Ok(newer(&v, mine, std::env::consts::ARCH))
}

impl Ctrl {
    /// All'avvio, una volta: c'è una versione più nuova? (la si propone, non si installa da sola)
    #[allow(clippy::needless_return)] // fuori da Android il corpo si ferma qui
    pub(super) fn updates_check(&mut self) {
        if !enabled() || CHECKED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        #[cfg(target_os = "android")]
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
    }

    /// «Aggiorna»: scarica l'APK e lo passa all'installatore di sistema.
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
        let dest = self.store.base.join("updates").join("aggiornamento.apk");
        #[cfg(target_os = "android")]
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
                    if let Err(e) = r.and_then(|p| crate::install_apk(&p)) {
                        c.toast(format!("Aggiornamento non riuscito: {e}"));
                    }
                })
            });
        });
        #[cfg(not(target_os = "android"))]
        let _ = (url, dest);
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
        assert_eq!(newer(&rel, "0.2.0+120-fff0000", "aarch64"), Some(("0.2.0+130-abc1234".into(), "u/arm64".into())));
        assert_eq!(newer(&rel, "0.2.0+120-fff0000", "arm").unwrap().1, "u/v7");
        assert_eq!(newer(&rel, "0.2.0+120-fff0000", "x86_64").unwrap().1, "u/uni");
        assert_eq!(newer(&rel, "0.2.0+130-abc1234", "aarch64"), None);
        let stable = serde_json::json!({"tag_name": "v0.3.0", "assets": [{"name": "TremendousPreparator.apk", "browser_download_url": "s"}]});
        assert_eq!(newer(&stable, "0.2.0", "aarch64"), Some(("0.3.0".into(), "s".into())));
        assert_eq!(newer(&stable, "0.3.0", "aarch64"), None);
        assert_eq!(build_number("0.2.0"), None);
    }
}
