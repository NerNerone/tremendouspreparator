//! Integrazione con il browser (WebAssembly): i file dell'archivio locale sono salvati in
//! IndexedDB (vedi `web/storage.js`) e il pacchetto contenuti si sceglie dal selettore di file.

use prep_core::vfs;
use std::path::PathBuf;
use wasm_bindgen::prelude::*;

/// Radice dell'archivio locale nel browser (percorsi solo logici, vedi `prep_core::vfs`).
pub const DATA_ROOT: &str = "/data";

#[wasm_bindgen(module = "/web/storage.js")]
extern "C" {
    #[wasm_bindgen(js_name = loadAll)]
    async fn load_all() -> JsValue;
    #[wasm_bindgen(js_name = putFile)]
    fn put_file(path: &str, bytes: &[u8]);
    #[wasm_bindgen(js_name = deleteFile)]
    fn delete_file(path: &str);
    #[wasm_bindgen(js_name = pickFile)]
    async fn pick_file() -> JsValue;
    #[wasm_bindgen(js_name = viewportSize)]
    fn viewport_size() -> Vec<f64>;
    #[wasm_bindgen(js_name = onResize)]
    fn on_resize(cb: &Closure<dyn FnMut(f64, f64)>);
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn error(s: &str);
}

pub fn log(msg: &str) {
    error(msg);
}

/// Ricarica i file salvati e collega il salvataggio automatico di ogni modifica.
pub async fn init_storage() {
    let all = js_sys::Array::from(&load_all().await);
    let files = all.iter().filter_map(|entry| {
        let pair = js_sys::Array::from(&entry);
        let path = pair.get(0).as_string()?;
        let bytes = js_sys::Uint8Array::new(&pair.get(1)).to_vec();
        Some((path, bytes))
    });
    vfs::preload(files);
    vfs::set_persist(|path, bytes| match bytes {
        Some(b) => put_file(path, b),
        None => delete_file(path),
    });
}

/// Fa scegliere un file all'utente e lo mette nella cartella `import/`: l'app lo trova lì e
/// lo importa come gli altri pacchetti.
pub fn pick_pack_file(import_dir: PathBuf) {
    wasm_bindgen_futures::spawn_local(async move {
        let v = pick_file().await;
        if v.is_null() || v.is_undefined() {
            return;
        }
        let bytes = js_sys::Uint8Array::new(&v).to_vec();
        let _ = vfs::write(&import_dir.join("pack-web.json.gz"), &bytes);
    });
}

/// La finestra occupa tutta la vista del browser (Slint altrimenti usa la misura preferita
/// dell'interfaccia) e la segue quando cambia.
pub fn fit_window(ui: &crate::AppWindow) {
    use slint::ComponentHandle;
    let set = |ui: &crate::AppWindow, w: f64, h: f64| ui.window().set_size(slint::LogicalSize::new(w as f32, h as f32));
    if let [w, h] = viewport_size()[..] {
        set(ui, w, h);
    }
    let weak = ui.as_weak();
    let cb = Closure::<dyn FnMut(f64, f64)>::new(move |w, h| {
        if let Some(ui) = weak.upgrade() {
            set(&ui, w, h);
        }
    });
    on_resize(&cb);
    // il callback resta registrato per tutta la vita della pagina
    cb.forget();
}
