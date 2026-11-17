//! Tremendous Preparator — interfaccia Slint (Material 3) sul nucleo `prep-core`.
//! Stessa base per Android, desktop (Linux) e browser (WebAssembly).

pub mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled.rs"));
}

slint::include_modules!();

mod app;
mod crash;
mod demo;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
mod desktop_files;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub mod desktop_reminder;
mod direct;
mod net;
mod palette;
mod periodic;
mod photo;
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

/// Selettore di un file qualsiasi (pagina «Contenuti»): Java lo copia in files/import/ come
/// `file-<ora>--<nome originale>`.
#[cfg(target_os = "android")]
pub(crate) fn android_pick_file() -> Result<(), String> {
    android_call("pickContentFile", None)
}

/// Fa scegliere dove salvare il file `path` (selettore «Salva» di sistema) e lo copia lì.
#[cfg(target_os = "android")]
pub(crate) fn android_save_file(path: &std::path::Path, name: &str) -> Result<(), String> {
    android_call("saveFile", Some(&format!("{}\n{name}", path.display())))
}

/// Fa scegliere una cartella e ci salva i file `(percorso, nome, tipo)` (poi tolti dall'app).
#[cfg(target_os = "android")]
pub(crate) fn android_save_files(files: &[(std::path::PathBuf, String, &str)]) -> Result<(), String> {
    let arg: Vec<String> = files.iter().map(|(p, n, t)| format!("{}\t{n}\t{t}", p.display())).collect();
    android_call("saveFilesToFolder", Some(&arg.join("\n")))
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

/// Notifica di sistema immediata (Android, browser): titolo e testo separati da un a capo.
pub(crate) fn notify_now(text: &str) {
    #[cfg(target_os = "android")]
    let _ = android_call("notifyNow", Some(text));
    #[cfg(target_arch = "wasm32")]
    web::notify(text);
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    let _ = text;
}

/// Selettore di immagini di sistema (Android): l'immagine arriva in `import/<kind>-….photo`.
#[cfg(target_os = "android")]
pub(crate) fn android_pick_photo(kind: &str) -> Result<(), String> {
    android_call("pickPhoto", Some(kind))
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
            let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }
                .map_err(|e| e.to_string())?;
            let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
            let obj = env
                .call_method(&activity, "systemTones", "()[I", &[])
                .and_then(|v| v.l())
                .map_err(|e| e.to_string())?;
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

/// Apre un indirizzo nel browser di sistema.
pub(crate) fn open_url(url: &str) -> bool {
    #[cfg(target_os = "android")]
    return android_call("openUrl", Some(url)).is_ok();
    #[cfg(target_arch = "wasm32")]
    return web::open_url(url);
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    return std::process::Command::new("xdg-open").arg(url).spawn().is_ok();
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

/// Chiama un metodo di MainActivity che restituisce una stringa (argomento stringa facoltativo).
#[cfg(target_os = "android")]
fn android_call_str(method: &str, arg: Option<&str>) -> Result<String, String> {
    let app = ANDROID_APP.get().ok_or("attività non pronta")?;
    // SAFETY: puntatori validi per tutta la vita del processo, forniti da android-activity
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let obj = match arg {
        Some(a) => {
            let js = env.new_string(a).map_err(|e| e.to_string())?;
            env.call_method(&activity, method, "(Ljava/lang/String;)Ljava/lang/String;", &[(&js).into()])
        }
        None => env.call_method(&activity, method, "()Ljava/lang/String;", &[]),
    }
    .and_then(|v| v.l())
    .map_err(|e| e.to_string())?;
    let js = jni::objects::JString::from(obj);
    let out: String = env.get_string(&js).map_err(|e| e.to_string())?.into();
    Ok(out)
}

/// Modalità concentrazione (Android, `FocusService.java`). Su desktop e web non c'è; con
/// `PREP_FOCUS_FAKE` il desktop finge di averla, per le prove dell'interfaccia.
pub(crate) mod focus {
    /// Disponibile su questo dispositivo.
    pub fn available() -> bool {
        cfg!(target_os = "android") || std::env::var("PREP_FOCUS_FAKE").is_ok()
    }

    /// Permessi e stato in JSON (vedi `MainActivity.focusState`).
    pub fn state() -> Option<String> {
        #[cfg(target_os = "android")]
        return super::android_call_str("focusState", None).ok();
        #[cfg(not(target_os = "android"))]
        std::env::var("PREP_FOCUS_FAKE").ok().map(|_| {
            r#"{"usage":true,"overlay":false,"until":0,"armed":false,"session":0,"active":false,"allowed":[]}"#
                .to_string()
        })
    }

    /// App avviabili con icona (JSON, vedi `MainActivity.focusApps`).
    pub fn apps() -> Option<String> {
        #[cfg(target_os = "android")]
        return super::android_call_str("focusApps", None).ok();
        #[cfg(not(target_os = "android"))]
        std::env::var("PREP_FOCUS_FAKE").ok().map(|_| {
            r#"[{"p":"org.wikipedia","l":"Wikipedia"},{"p":"com.google.android.calculator","l":"Calcolatrice"},{"p":"com.instagram.android","l":"Instagram"},{"p":"com.spotify.music","l":"Spotify"},{"p":"org.mozilla.firefox","l":"Firefox"}]"#.to_string()
        })
    }

    fn call(method: &str, arg: Option<&str>) {
        #[cfg(target_os = "android")]
        let _ = super::android_call(method, arg);
        #[cfg(not(target_os = "android"))]
        let _ = (method, arg);
    }

    /// Avvia il blocco: `{"until": ms, "allowed": [...]}`.
    pub fn start(json: &str) {
        call("focusStart", Some(json));
    }

    /// Stato della sessione di studio: 0 ferma, 1 in pista, 2 pit stop.
    pub fn session(state: i32) {
        call("focusSession", Some(&state.to_string()));
    }

    /// Apre le impostazioni di un permesso: "usage", "overlay", "appinfo".
    pub fn permission(kind: &str) {
        call("focusPermission", Some(kind));
    }

    pub fn launch(pkg: &str) {
        call("launchApp", Some(pkg));
    }

    pub fn dial() {
        call("dialPhone", None);
    }
}

/// Nome del pacchetto Android (applicationId) del processo, da /proc/self/cmdline
/// (i processi secondari si chiamano «pacchetto:nome»).
#[cfg(target_os = "android")]
fn android_package() -> String {
    let raw = std::fs::read("/proc/self/cmdline").unwrap_or_default();
    let name = raw.split(|b| *b == 0 || *b == b':').next().unwrap_or_default();
    String::from_utf8_lossy(name).into_owned()
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: slint::android::AndroidApp) {
    let _ = ANDROID_APP.set(android_app.clone());
    let dir = android_app
        .internal_data_path()
        .unwrap_or_else(|| std::path::PathBuf::from(format!("/data/data/{}/files", android_package())));
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
