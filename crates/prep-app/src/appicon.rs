//! Icona dell'app sul computer (Linux) nel colore scelto in Aspetto: il libro con il segnalibro
//! a scacchi, disegnato qui in SVG e in PNG alle misure solite e copiato nelle icone
//! dell'utente (`~/.local/share/icons/hicolor`), che il desktop preferisce a quelle installate.
//! Dal Flatpak la copia la fa il sistema (`flatpak-spawn --host`, lo stesso permesso degli
//! aggiornamenti); i file stanno prima nella cartella dei dati dell'app, che ha lo stesso
//! percorso dentro e fuori dal Flatpak.

use std::path::{Path, PathBuf};
use std::process::Command;

const ID: &str = "io.github.nernerone.TremendousPreparator";
const SIZES: [u32; 8] = [16, 22, 24, 32, 48, 64, 128, 256];

/// Fondo, pagina sinistra, pagina destra, inchiostro (ARGB).
pub type Colors = [u32; 4];

fn hex(c: u32) -> String {
    format!("#{:06X}", c & 0xff_ffff)
}

fn book_l() -> &'static str {
    "M54,40C45,34 35,33 27,36L27,74C35,71 45,72 54,78Z"
}
fn book_r() -> &'static str {
    "M54,40C63,34 73,33 81,36L81,74C73,71 63,72 54,78Z"
}

/// Caselle nere del segnalibro (x, y in unità dell'icona 108).
fn checks() -> Vec<(f32, f32)> {
    let mut v = Vec::new();
    for r in 0..5 {
        for c in 0..3 {
            if (r + c) % 2 == 0 {
                v.push((60.0 + c as f32 * 4.0, 30.0 + r as f32 * 4.0));
            }
        }
    }
    v
}

pub fn svg(c: Colors) -> String {
    let squares: String = checks().iter().map(|(x, y)| format!("M{x},{y}h4v4h-4z")).collect();
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="12 12 84 84" width="256" height="256">
  <defs><clipPath id="c"><rect x="12" y="12" width="84" height="84" rx="24" ry="24"/></clipPath></defs>
  <g clip-path="url(#c)">
    <rect x="0" y="0" width="108" height="108" fill="{bg}"/>
    <g transform="translate(54 54) scale(0.92) translate(-54 -54)">
      <path fill="{pl}" d="{l}"/>
      <path fill="{pr}" d="{r}"/>
      <path fill="{pl}" stroke="{ink}" stroke-width="1" stroke-linejoin="round" d="M60,30h12v24l-6,-5l-6,5z"/>
      <path fill="{ink}" d="{squares}"/>
    </g>
  </g>
</svg>
"##,
        bg = hex(c[0]),
        pl = hex(c[1]),
        pr = hex(c[2]),
        ink = hex(c[3]),
        l = book_l(),
        r = book_r(),
    )
}

// ---------- disegno in PNG (poche forme: poligoni campionati, 4×4 campioni per pixel) ----------

type Pt = (f32, f32);

fn cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, out: &mut Vec<Pt>) {
    for i in 1..=16 {
        let t = i as f32 / 16.0;
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        out.push((a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1));
    }
}

fn page(dir: f32) -> Vec<Pt> {
    // il contorno di una pagina: dal dorso in alto al bordo esterno e di nuovo al dorso in basso
    let x = |v: f32| 54.0 + dir * (v - 54.0);
    let mut v = vec![(54.0, 40.0)];
    cubic((54.0, 40.0), (x(45.0), 34.0), (x(35.0), 33.0), (x(27.0), 36.0), &mut v);
    v.push((x(27.0), 74.0));
    cubic((x(27.0), 74.0), (x(35.0), 71.0), (x(45.0), 72.0), (54.0, 78.0), &mut v);
    v
}

fn inside(poly: &[Pt], p: Pt) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            c = !c;
        }
    }
    c
}

fn near_edge(poly: &[Pt], p: Pt, w: f32) -> bool {
    let n = poly.len();
    (0..n).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (qx, qy) = (a.0 + t * dx - p.0, a.1 + t * dy - p.1);
        qx * qx + qy * qy <= w * w
    })
}

fn in_round_rect(p: Pt) -> bool {
    // riquadro 12..96 con angoli di raggio 24
    let (x, y) = (p.0.clamp(36.0, 72.0), p.1.clamp(36.0, 72.0));
    let (dx, dy) = (p.0 - x, p.1 - y);
    (12.0..=96.0).contains(&p.0) && (12.0..=96.0).contains(&p.1) && dx * dx + dy * dy <= 24.0 * 24.0
}

pub fn png(c: Colors, size: u32) -> Vec<u8> {
    let (left, right) = (page(1.0), page(-1.0));
    let ribbon: Vec<Pt> = vec![(60.0, 30.0), (72.0, 30.0), (72.0, 54.0), (66.0, 49.0), (60.0, 54.0)];
    let squares = checks();
    let rgba = |c: u32| [((c >> 16) & 255) as f32, ((c >> 8) & 255) as f32, (c & 255) as f32];
    let mut img = image::RgbaImage::new(size, size);
    const S: u32 = 4;
    for py in 0..size {
        for px in 0..size {
            let (mut acc, mut cov) = ([0f32; 3], 0f32);
            for sy in 0..S {
                for sx in 0..S {
                    let u = 12.0 + (px as f32 + (sx as f32 + 0.5) / S as f32) / size as f32 * 84.0;
                    let v = 12.0 + (py as f32 + (sy as f32 + 0.5) / S as f32) / size as f32 * 84.0;
                    if !in_round_rect((u, v)) {
                        continue;
                    }
                    // il disegno è ridotto a 0,92 attorno al centro
                    let q = (54.0 + (u - 54.0) / 0.92, 54.0 + (v - 54.0) / 0.92);
                    let mut col = c[0];
                    if inside(&left, q) {
                        col = c[1];
                    }
                    if inside(&right, q) {
                        col = c[2];
                    }
                    if inside(&ribbon, q) || near_edge(&ribbon, q, 0.5) {
                        col = if near_edge(&ribbon, q, 0.5) { c[3] } else { c[1] };
                        if squares.iter().any(|(x, y)| q.0 >= *x && q.0 < x + 4.0 && q.1 >= *y && q.1 < y + 4.0) {
                            col = c[3];
                        }
                    }
                    let k = rgba(col);
                    for i in 0..3 {
                        acc[i] += k[i];
                    }
                    cov += 1.0;
                }
            }
            if cov > 0.0 {
                let a = cov / (S * S) as f32;
                img.put_pixel(
                    px,
                    py,
                    image::Rgba([
                        (acc[0] / cov).round() as u8,
                        (acc[1] / cov).round() as u8,
                        (acc[2] / cov).round() as u8,
                        (a * 255.0).round() as u8,
                    ]),
                );
            }
        }
    }
    let mut out = Vec::new();
    let _ = image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png);
    out
}

/// Scrive le icone in `dir/hicolor/...` (SVG e PNG).
fn write_theme(dir: &Path, c: Colors) -> std::io::Result<()> {
    let sc = dir.join("hicolor/scalable/apps");
    std::fs::create_dir_all(&sc)?;
    std::fs::write(sc.join(format!("{ID}.svg")), svg(c))?;
    for s in SIZES {
        let d = dir.join(format!("hicolor/{s}x{s}/apps"));
        std::fs::create_dir_all(&d)?;
        std::fs::write(d.join(format!("{ID}.png")), png(c, s))?;
    }
    Ok(())
}

/// Icona nei colori `c` per il desktop dell'utente (in background; solo se i colori cambiano).
pub fn install(data_dir: PathBuf, c: Colors) {
    std::thread::spawn(move || {
        let mark = data_dir.join("appicon/colori");
        let key = format!("{:08x}{:08x}{:08x}{:08x}", c[0], c[1], c[2], c[3]);
        if std::fs::read_to_string(&mark).ok().as_deref() == Some(key.as_str()) {
            return;
        }
        let stage = data_dir.join("appicon");
        if write_theme(&stage, c).is_err() {
            return;
        }
        let src = stage.join("hicolor");
        let ok = if crate::flatpak::inside() {
            // fuori dal Flatpak: la cartella dei dati ha lo stesso percorso
            let script = format!(
                "d=\"${{XDG_DATA_HOME:-$HOME/.local/share}}/icons\"; mkdir -p \"$d\" && cp -r '{}' \"$d/\" && touch \"$d/hicolor\"",
                src.display()
            );
            Command::new("flatpak-spawn").args(["--host", "sh", "-c", &script]).status().is_ok_and(|s| s.success())
        } else {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
            match base {
                Some(b) => copy_dir(&src, &b.join("icons/hicolor")).is_ok(),
                None => false,
            }
        };
        if ok {
            let _ = std::fs::write(&mark, key);
        }
    });
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let p = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &p)?;
        } else {
            std::fs::copy(e.path(), &p)?;
        }
    }
    let _ = std::fs::File::open(to).and_then(|f| f.set_modified(std::time::SystemTime::now()));
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_draws() {
        let c = [0xff1a73e8, 0xfff8faff, 0xffd3e3fd, 0xff0842a0];
        let p = super::png(c, 64);
        let img = image::load_from_memory(&p).unwrap().to_rgba8();
        // angolo trasparente, centro della pagina sinistra chiaro, fondo blu in alto al centro
        assert_eq!(img.get_pixel(0, 0)[3], 0);
        assert!(img.get_pixel(18, 34)[0] > 200);
        assert_eq!(img.get_pixel(32, 4)[2], 0xe8);
        assert!(super::svg(c).contains("#1A73E8"));
    }
}
