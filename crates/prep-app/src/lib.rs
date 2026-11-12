//! App Android "Semestre Aperto" — UI Slint (Material 3) + nucleo `prep-core`.

pub mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled.rs"));
}

slint::include_modules!();

mod app;
mod demo;
#[cfg(not(target_os = "android"))]
pub mod desktop_reminder;
mod periodic;

pub use app::{run, APP_VERSION};

/// Font incorporati: Roboto (stile Material) come principale e DejaVu Sans come fallback
/// per simboli che Roboto o i font di sistema non hanno (⇌ ∝ ≃, pedici e apici, …).
/// Vanno scritti su disco e indicati in SLINT_FONT_PATH prima che Slint carichi i font.
const FONTS: &[(&str, &[u8])] = &[
    ("Roboto-Regular.ttf", include_bytes!("../fonts/Roboto-Regular.ttf")),
    ("Roboto-Medium.ttf", include_bytes!("../fonts/Roboto-Medium.ttf")),
    ("Roboto-SemiBold.ttf", include_bytes!("../fonts/Roboto-SemiBold.ttf")),
    ("Roboto-Bold.ttf", include_bytes!("../fonts/Roboto-Bold.ttf")),
    ("DejaVuSans.ttf", include_bytes!("../fonts/DejaVuSans.ttf")),
    ("DejaVuSans-Bold.ttf", include_bytes!("../fonts/DejaVuSans-Bold.ttf")),
];

pub fn install_fonts(data_dir: &std::path::Path) {
    if std::env::var_os("SLINT_FONT_PATH").is_some() {
        return;
    }
    let dir = data_dir.join("fonts");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut paths = Vec::new();
    for (name, bytes) in FONTS {
        let p = dir.join(name);
        let same = std::fs::metadata(&p).map(|m| m.len() == bytes.len() as u64).unwrap_or(false);
        if same || std::fs::write(&p, bytes).is_ok() {
            paths.push(p);
        }
    }
    // l'ordine conta: il primo è il font principale, gli altri fanno da fallback
    if let Ok(v) = std::env::join_paths(paths) {
        std::env::set_var("SLINT_FONT_PATH", v);
    }
}

#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::OnceLock<slint::android::AndroidApp> = std::sync::OnceLock::new();

/// Apre il selettore di file di sistema (MainActivity.pickContentPack): il file scelto
/// viene copiato da Java in files/import/, dove l'app lo trova e lo importa.
#[cfg(target_os = "android")]
pub(crate) fn android_pick_pack() -> Result<(), String> {
    let app = ANDROID_APP.get().ok_or("attività non pronta")?;
    // SAFETY: puntatori validi per tutta la vita del processo, forniti da android-activity
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    env.call_method(&activity, "pickContentPack", "()V", &[]).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: slint::android::AndroidApp) {
    let _ = ANDROID_APP.set(android_app.clone());
    let dir = android_app
        .internal_data_path()
        .unwrap_or_else(|| std::path::PathBuf::from("/data/data/it.tremendouspreparator.app/files"));
    install_fonts(&dir);
    slint::android::init(android_app).expect("init backend Android");
    if let Err(e) = run(dir) {
        eprintln!("errore: {e}");
    }
}
