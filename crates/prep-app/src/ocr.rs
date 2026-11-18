//! OCR sul dispositivo con `ocrs` (Robert Knight): i due modelli ONNX (~12 MB, licenza
//! CC BY-SA 4.0) si scaricano la prima volta, verificati con SHA-256, dalla copia nelle release
//! del progetto o, se manca, dalla fonte originale. Ordine di lettura e ripulitura del testo
//! stanno in `prep_core::ocr`.

use std::io::Read;
use std::path::{Path, PathBuf};

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use prep_core::pdf::Scan;
use prep_core::vfs;
use rten::Model;
use rten_imageproc::RotatedRect;
use sha2::{Digest, Sha256};

/// Modelli: nome del file e impronta SHA-256.
const MODELS: [(&str, &str); 2] = [
    ("text-detection.onnx", "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe"),
    ("text-recognition.onnx", "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5"),
];
const SOURCES: [&str; 2] = [
    "https://github.com/NerNerone/tremendouspreparator/releases/download/ocr-modelli/",
    "https://ocrs-models.s3-accelerate.amazonaws.com/",
];

pub fn models_dir(base: &Path) -> PathBuf {
    base.join("ocr")
}

/// I modelli ci sono già (scaricati e verificati)?
pub fn ready(dir: &Path) -> bool {
    MODELS.iter().all(|(name, _)| vfs::exists(&dir.join(name)))
}

/// Scarica i modelli che mancano; `progress` riceve messaggi da mostrare.
pub fn download(dir: &Path, progress: &dyn Fn(String)) -> Result<(), String> {
    vfs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for (i, (name, sum)) in MODELS.iter().enumerate() {
        let dst = dir.join(name);
        if vfs::exists(&dst) {
            continue;
        }
        let mut last = String::new();
        let mut done = None;
        for src in SOURCES {
            match fetch(&format!("{src}{name}"), &|mb| {
                progress(format!("Scarico i modelli OCR ({} di 2): {mb:.1} MB…", i + 1))
            }) {
                Ok(bytes) if hex(&Sha256::digest(&bytes)) == *sum => {
                    done = Some(bytes);
                    break;
                }
                Ok(_) => last = "file diverso da quello atteso".into(),
                Err(e) => last = e,
            }
        }
        let bytes = done.ok_or_else(|| format!("modelli OCR non scaricati: {last}"))?;
        let tmp = dir.join(format!("{name}.tmp"));
        vfs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        vfs::rename(&tmp, &dst).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn fetch(url: &str, progress: &dyn Fn(f64)) -> Result<Vec<u8>, String> {
    let mut resp = ureq::get(url).header("User-Agent", "TremendousPreparator").call().map_err(|e| e.to_string())?;
    let mut reader = resp.body_mut().with_config().limit(50_000_000).reader();
    let mut out = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    let mut shown = 0usize;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() - shown > 500_000 {
            shown = out.len();
            progress(shown as f64 / 1e6);
        }
    }
    Ok(out)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Motore pronto per leggere pagine.
pub struct Engine(OcrEngine);

impl Engine {
    pub fn load(dir: &Path) -> Result<Engine, String> {
        let model = |name: &str| {
            let bytes = vfs::read(&dir.join(name)).map_err(|e| format!("modello {name}: {e}"))?;
            Model::load(bytes).map_err(|e| format!("modello {name}: {e}"))
        };
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(model(MODELS[0].0)?),
            recognition_model: Some(model(MODELS[1].0)?),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        Ok(Engine(engine))
    }

    /// Testo di un'immagine RGB (8 bit, righe di seguito), nell'ordine di lettura.
    pub fn read_rgb(&self, rgb: &[u8], w: u32, h: u32) -> Result<String, String> {
        let src = ImageSource::from_bytes(rgb, (w, h)).map_err(|e| e.to_string())?;
        let input = self.0.prepare_input(src).map_err(|e| e.to_string())?;
        let words = self.0.detect_words(&input).map_err(|e| e.to_string())?;
        let lines = self.0.find_text_lines(&input, &words);
        let boxes: Vec<Vec<[f32; 4]>> = lines.iter().map(|l| l.iter().map(word_box).collect()).collect();
        let order = prep_core::ocr::reading_order(&boxes, w as f32);
        let segs: Vec<Vec<RotatedRect>> =
            order.iter().map(|(l, ws)| ws.iter().map(|&i| lines[*l][i]).collect()).collect();
        let text = self.0.recognize_text(&input, &segs).map_err(|e| e.to_string())?;
        let lines: Vec<String> = text.iter().flatten().map(|l| l.to_string()).collect();
        Ok(prep_core::ocr::page_text(&lines))
    }

    /// Testo di un file immagine (foto, JPEG/PNG).
    pub fn read_image(&self, bytes: &[u8]) -> Result<String, String> {
        let img = image::load_from_memory(bytes).map_err(|e| format!("immagine non leggibile: {e}"))?;
        let img = shrink(img).into_rgb8();
        let (w, h) = img.dimensions();
        self.read_rgb(img.as_raw(), w, h)
    }

    /// Testo dell'immagine di una pagina scansionata.
    pub fn read_scan(&self, scan: Scan) -> Result<String, String> {
        match scan {
            Scan::Jpeg(b) => self.read_image(&b),
            Scan::Raw { width, height, channels, data } => {
                let img = if channels == 1 {
                    image::GrayImage::from_raw(width, height, data).map(image::DynamicImage::ImageLuma8)
                } else {
                    image::RgbImage::from_raw(width, height, data).map(image::DynamicImage::ImageRgb8)
                };
                let img = shrink(img.ok_or("immagine della pagina non valida")?).into_rgb8();
                let (w, h) = img.dimensions();
                self.read_rgb(img.as_raw(), w, h)
            }
        }
    }
}

/// Le scansioni ad alta risoluzione non leggono meglio (anzi) e costano molto di più: il lato
/// lungo si porta al massimo a 2400 pixel (circa 200 dpi su A4).
fn shrink(img: image::DynamicImage) -> image::DynamicImage {
    const MAX: u32 = 2400;
    if img.width().max(img.height()) > MAX {
        img.resize(MAX, MAX, image::imageops::FilterType::Triangle)
    } else {
        img
    }
}

fn word_box(r: &RotatedRect) -> [f32; 4] {
    let c = r.corners();
    let (mut b0, mut b1, mut b2, mut b3) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in c {
        b0 = b0.min(p.x);
        b1 = b1.min(p.y);
        b2 = b2.max(p.x);
        b3 = b3.max(p.y);
    }
    [b0, b1, b2, b3]
}
