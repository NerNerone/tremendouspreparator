//! Foto profilo e del gruppo: ritagliate al quadrato, raddrizzate (orientamento EXIF),
//! ridotte a 256×256 e salvate in JPEG in `<base>/photos/<hash>.jpg`. Stanno in un solo
//! pezzo di messaggio ([`MAX_BYTES`]): viaggiano sul collegamento diretto (WebRTC) quando c'è,
//! altrimenti attraverso i relay.

use image::ImageDecoder;
use prep_core::vfs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Lato delle foto salvate e mandate.
pub const SIDE: u32 = 256;
/// Dimensione massima di una foto (un pezzo di messaggio, con margine per l'intestazione).
pub const MAX_BYTES: usize = prep_core::p2p::CHUNK - 1024;
/// Lato massimo di un'immagine scelta dall'utente (oltre si rifiuta: memoria).
const MAX_PICK_SIDE: u32 = 12_000;

pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 16 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn path(base: &Path, hash: &str) -> PathBuf {
    base.join("photos").join(format!("{hash}.jpg"))
}

/// Dimensioni dichiarate da un'immagine, senza decodificarla.
fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()
}

/// Lato massimo dell'immagine tenuta in memoria per il ritaglio.
const EDIT_SIDE: u32 = 1600;

/// Da un'immagine qualsiasi (JPEG, PNG) alla foto da salvare: raddrizzata secondo l'EXIF,
/// ritaglio centrale quadrato, 256×256, JPEG con la qualità più alta che sta in un messaggio.
#[cfg(test)]
pub fn prepare(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = decode(bytes)?;
    let s = img.width().min(img.height());
    crop_encode(&img, ((img.width() - s) / 2) as f32, ((img.height() - s) / 2) as f32, s as f32)
}

/// Immagine scelta dall'utente, raddrizzata secondo l'EXIF e ridotta (lato al massimo
/// [`EDIT_SIDE`]) per il ritaglio.
pub fn decode(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    let (w, h) = dimensions(bytes).ok_or("immagine non leggibile")?;
    if w > MAX_PICK_SIDE || h > MAX_PICK_SIDE {
        return Err(format!("immagine troppo grande ({w} × {h})"));
    }
    let mut dec = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| format!("immagine non leggibile: {e}"))?;
    let orientation = dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(dec).map_err(|e| format!("immagine non leggibile: {e}"))?;
    img.apply_orientation(orientation);
    if img.width().max(img.height()) > EDIT_SIDE {
        img = img.resize(EDIT_SIDE, EDIT_SIDE, image::imageops::FilterType::Triangle);
    }
    Ok(img)
}

/// Il quadrato scelto (`x`, `y`, lato, in pixel dell'immagine; riportato dentro i bordi),
/// ridotto a 256×256 e codificato in JPEG con la qualità più alta che sta in un messaggio.
pub fn crop_encode(img: &image::DynamicImage, x: f32, y: f32, side: f32) -> Result<Vec<u8>, String> {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err("immagine vuota".into());
    }
    let s = (side.round() as u32).clamp(1, w.min(h));
    let x = (x.round().max(0.0) as u32).min(w - s);
    let y = (y.round().max(0.0) as u32).min(h - s);
    let sq = img.crop_imm(x, y, s, s);
    let small = sq.resize_exact(SIDE, SIDE, image::imageops::FilterType::Lanczos3).to_rgb8();
    for q in [88u8, 80, 72, 64, 55, 45, 35] {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q)
            .encode_image(&small)
            .map_err(|e| e.to_string())?;
        if out.len() <= MAX_BYTES {
            return Ok(out);
        }
    }
    Err("foto troppo complessa da comprimere".into())
}

/// Controlla una foto ricevuta da un altro membro: JPEG, al massimo 256×256 e
/// [`MAX_BYTES`] (si leggono le dimensioni prima di decodificare: un file che dichiara
/// misure enormi viene scartato senza allocare nulla).
pub fn valid_received(bytes: &[u8]) -> bool {
    bytes.len() <= MAX_BYTES
        && bytes.starts_with(&[0xFF, 0xD8])
        && dimensions(bytes).is_some_and(|(w, h)| w > 0 && h > 0 && w <= SIDE && h <= SIDE)
        && to_image(bytes).is_some()
}

/// Salva una foto (già preparata o ricevuta) e ne restituisce l'hash.
pub fn save(base: &Path, jpeg: &[u8]) -> Result<String, String> {
    let hash = prep_core::group::photo_hash(jpeg);
    let p = path(base, &hash);
    if !vfs::exists(&p) {
        if let Some(dir) = p.parent() {
            vfs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        vfs::write(&p, jpeg).map_err(|e| e.to_string())?;
    }
    Ok(hash)
}

pub fn load(base: &Path, hash: &str) -> Option<Vec<u8>> {
    if !valid_hash(hash) {
        return None;
    }
    vfs::read(&path(base, hash)).ok().filter(|b| b.len() <= MAX_BYTES)
}

/// Immagine per l'interfaccia (solo foto piccole: quelle salvate sono già controllate).
/// Come `to_image`, ma solo i pixel: si può preparare su un altro thread (l'immagine no).
pub fn to_pixels(bytes: &[u8]) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let (w, h) = dimensions(bytes)?;
    if w > SIDE * 2 || h > SIDE * 2 {
        return None;
    }
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height()))
}

pub fn to_image(jpeg: &[u8]) -> Option<slint::Image> {
    let (w, h) = dimensions(jpeg)?;
    if w > SIDE * 2 || h > SIDE * 2 {
        return None;
    }
    let img = image::load_from_memory(jpeg).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), w, h);
    Some(slint::Image::from_rgba8(buf))
}

#[cfg(test)]
mod tests {
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8]);
        }
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn prepares_square_jpeg_that_fits_a_message() {
        let jpg = super::prepare(&png(1200, 800)).unwrap();
        assert!(jpg.len() <= super::MAX_BYTES);
        let back = image::load_from_memory(&jpg).unwrap();
        assert_eq!((back.width(), back.height()), (256, 256));
        assert!(super::valid_received(&jpg));
        assert!(super::to_image(&jpg).is_some());
    }

    #[test]
    fn crops_the_chosen_square() {
        // metà sinistra rossa, metà destra blu: il quadrato a destra è tutto blu
        let mut img = image::RgbImage::new(400, 200);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 200 { image::Rgb([255, 0, 0]) } else { image::Rgb([0, 0, 255]) };
        }
        let img = image::DynamicImage::ImageRgb8(img);
        let jpg = super::crop_encode(&img, 200.0, 0.0, 200.0).unwrap();
        let back = image::load_from_memory(&jpg).unwrap().to_rgb8();
        assert_eq!((back.width(), back.height()), (256, 256));
        let p = back.get_pixel(128, 128);
        assert!(p[2] > 200 && p[0] < 60, "{p:?}");
        // valori fuori dai bordi: riportati dentro, nessun errore
        assert!(super::crop_encode(&img, 390.0, -50.0, 900.0).is_ok());
        // immagini grandi ridotte per il ritaglio
        let big = super::decode(&png(3000, 2000)).unwrap();
        assert_eq!((big.width(), big.height()), (1600, 1067));
    }

    #[test]
    fn rejects_bad_received_photos() {
        // PNG, o JPEG più grande di 256: scartati
        assert!(!super::valid_received(&png(64, 64)));
        let mut big = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(400, 400))
            .write_to(&mut std::io::Cursor::new(&mut big), image::ImageFormat::Jpeg)
            .unwrap();
        assert!(!super::valid_received(&big));
        // un JPEG che dichiara misure enormi (intestazione modificata) non viene decodificato
        let mut fake = super::prepare(&png(300, 300)).unwrap();
        if let Some(i) = fake.windows(2).position(|w| w == [0xFF, 0xC0]) {
            // SOF0: lunghezza(2) precisione(1) altezza(2) larghezza(2)
            fake[i + 5] = 0xFF;
            fake[i + 6] = 0xFF;
            fake[i + 7] = 0xFF;
            fake[i + 8] = 0xFF;
        }
        assert!(!super::valid_received(&fake));
    }

    #[test]
    fn applies_exif_orientation() {
        // JPEG 200×100 con Orientation = 6 (ruotare di 90°): il ritaglio è quadrato comunque,
        // ma il contenuto deve ruotare; qui basta che la lettura dell'orientamento non fallisca
        let mut jpg = Vec::new();
        let mut img = image::RgbImage::new(200, 100);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 100 { image::Rgb([255, 0, 0]) } else { image::Rgb([0, 0, 255]) };
        }
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut jpg), image::ImageFormat::Jpeg)
            .unwrap();
        // segmento APP1 Exif minimo con Orientation = 6, subito dopo SOI
        let tiff: Vec<u8> = vec![
            b'M', b'M', 0, 42, 0, 0, 0, 8, // intestazione TIFF big endian, IFD a 8
            0, 1, // una voce
            0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, // Orientation SHORT 1 = 6
            0, 0, 0, 0, // nessun IFD successivo
        ];
        let mut app1 = vec![0xFF, 0xE1];
        let len = (2 + 6 + tiff.len()) as u16;
        app1.extend_from_slice(&len.to_be_bytes());
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);
        let mut with = jpg[..2].to_vec();
        with.extend_from_slice(&app1);
        with.extend_from_slice(&jpg[2..]);
        let out = super::prepare(&with).unwrap();
        let back = image::load_from_memory(&out).unwrap().to_rgb8();
        // ruotata di 90° in senso orario: il rosso (a sinistra) finisce in alto
        let top = back.get_pixel(128, 10);
        let bottom = back.get_pixel(128, 245);
        assert!(top[0] > 150 && top[2] < 100, "alto {top:?}");
        assert!(bottom[2] > 150 && bottom[0] < 100, "basso {bottom:?}");
    }

    #[test]
    fn load_rejects_path_traversal() {
        let dir = std::env::temp_dir().join("tp_test_photo_sec");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("secret.jpg"), b"\xff\xd8\xff").unwrap();
        assert!(!super::valid_hash("../secret"));
        assert!(!super::valid_hash("0123456789abcde"));
        assert!(!super::valid_hash("0123456789ABCDE0"));
        assert!(super::valid_hash("0123456789abcdef"));
        assert!(super::load(&dir, "../secret").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
