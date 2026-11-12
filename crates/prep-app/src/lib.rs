//! Tremendous Preparator — interfaccia Slint (Material 3) sul nucleo `prep-core`.
//! Stessa base per Android, desktop (Linux) e browser (WebAssembly).

pub mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled.rs"));
}

slint::include_modules!();

mod app;
mod demo;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub mod desktop_reminder;
mod direct;
mod net;
mod palette;
mod periodic;
#[cfg(target_arch = "wasm32")]
mod web;

pub use app::{run, APP_VERSION};

/// Font incorporati: Roboto (stile Material) come principale e DejaVu Sans come fallback
/// per simboli che Roboto o i font di sistema non hanno (⇌ ∝ ≃, pedici e apici, …).
/// Vanno scritti su disco e indicati in SLINT_FONT_PATH prima che Slint carichi i font.
#[cfg(not(target_arch = "wasm32"))]
const FONTS: &[(&str, &[u8])] = &[
    ("Roboto-Regular.ttf", include_bytes!("../fonts/Roboto-Regular.ttf")),
    ("Roboto-Medium.ttf", include_bytes!("../fonts/Roboto-Medium.ttf")),
    ("Roboto-SemiBold.ttf", include_bytes!("../fonts/Roboto-SemiBold.ttf")),
    ("Roboto-Bold.ttf", include_bytes!("../fonts/Roboto-Bold.ttf")),
    ("DejaVuSans.ttf", include_bytes!("../fonts/DejaVuSans.ttf")),
    ("DejaVuSans-Bold.ttf", include_bytes!("../fonts/DejaVuSans-Bold.ttf")),
];

#[cfg(not(target_arch = "wasm32"))]
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

/// Chiama un metodo `void` di MainActivity, con un argomento stringa facoltativo.
#[cfg(target_os = "android")]
fn android_call(method: &str, arg: Option<&str>) -> Result<(), String> {
    let app = ANDROID_APP.get().ok_or("attività non pronta")?;
    // SAFETY: puntatori validi per tutta la vita del processo, forniti da android-activity
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    match arg {
        Some(a) => {
            let js = env.new_string(a).map_err(|e| e.to_string())?;
            env.call_method(&activity, method, "(Ljava/lang/String;)V", &[(&js).into()]).map_err(|e| e.to_string())?;
        }
        None => {
            env.call_method(&activity, method, "()V", &[]).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Comandi al collegamento Bluetooth (MainActivity → BtLink): "btListen", "btScan",
/// "btConnect" (indirizzo), "btClose".
pub(crate) fn bt_cmd(method: &str, arg: Option<&str>) {
    #[cfg(target_os = "android")]
    let _ = android_call(method, arg);
    #[cfg(not(target_os = "android"))]
    let _ = (method, arg);
}

/// Manda un messaggio sul collegamento Bluetooth aperto.
pub(crate) fn bt_send(data: &[u8]) {
    #[cfg(target_os = "android")]
    let _ = (|| -> Result<(), String> {
        let app = ANDROID_APP.get().ok_or("attività non pronta")?;
        // SAFETY: puntatori validi per tutta la vita del processo, forniti da android-activity
        let vm =
            unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        let arr = env.byte_array_from_slice(data).map_err(|e| e.to_string())?;
        env.call_method(&activity, "btSend", "([B)V", &[(&arr).into()]).map_err(|e| e.to_string())?;
        Ok(())
    })();
    #[cfg(not(target_os = "android"))]
    let _ = data;
}

/// Eventi del collegamento Bluetooth da Java (BtLink.nativeBt).
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_it_tremendouspreparator_app_BtLink_nativeBt<'l>(
    mut env: jni::JNIEnv<'l>,
    _class: jni::objects::JClass<'l>,
    kind: jni::sys::jint,
    text: jni::objects::JString<'l>,
    data: jni::objects::JByteArray<'l>,
) {
    let text: String = env.get_string(&text).map(|s| s.into()).unwrap_or_default();
    let data = env.convert_byte_array(&data).unwrap_or_default();
    let _ = slint::invoke_from_event_loop(move || app::on_bt_event(kind, text, data));
}

/// Tonalità dei colori di sistema (Material You, Android 12+), vedi `palette`.
pub(crate) fn system_tones() -> Option<Vec<u32>> {
    #[cfg(target_os = "android")]
    {
        let r = (|| -> Result<Vec<u32>, String> {
            let app = ANDROID_APP.get().ok_or("attività non pronta")?;
            // SAFETY: puntatori validi per tutta la vita del processo, forniti da android-activity
            let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }.map_err(|e| e.to_string())?;
            let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
            let obj = env.call_method(&activity, "systemTones", "()[I", &[]).and_then(|v| v.l()).map_err(|e| e.to_string())?;
            let arr = jni::objects::JIntArray::from(obj);
            let n = env.get_array_length(&arr).map_err(|e| e.to_string())? as usize;
            let mut buf = vec![0i32; n];
            env.get_int_array_region(&arr, 0, &mut buf).map_err(|e| e.to_string())?;
            Ok(buf.into_iter().map(|x| x as u32).collect())
        })();
        return r.ok().filter(|v| v.len() >= 65);
    }
    #[cfg(not(target_os = "android"))]
    None
}

/// Apre la condivisione di sistema con un testo (Android, browser); `false` se non c'è.
pub(crate) fn share_text(text: &str) -> bool {
    #[cfg(target_os = "android")]
    return android_call("shareText", Some(text)).is_ok();
    #[cfg(target_arch = "wasm32")]
    return web::share_text(text);
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    {
        let _ = text;
        false
    }
}

/// Modalità concentrazione su Android: blocca lo schermo sull'app (fissaggio schermo).
pub(crate) fn focus_lock(on: bool) {
    #[cfg(target_os = "android")]
    let _ = android_call(if on { "lockStudy" } else { "unlockStudy" }, None);
    #[cfg(not(target_os = "android"))]
    let _ = on;
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

/// Avvio nel browser: prima si ricaricano i dati salvati in IndexedDB, poi parte l'interfaccia
/// (la pagina deve contenere `<canvas id="canvas">`).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_start() {
    std::panic::set_hook(Box::new(|info| web::log(&info.to_string())));
    wasm_bindgen_futures::spawn_local(async {
        web::init_storage().await;
        if let Err(e) = run(std::path::PathBuf::from(web::DATA_ROOT)) {
            web::log(&format!("errore: {e}"));
        }
    });
}
