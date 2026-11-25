//! Colori di sistema (Material You, Android 12+) → ruoli Material 3 dell'interfaccia.
//!
//! Android espone cinque tavolozze (accento 1-3, neutro 1-2) di 13 tonalità ciascuna
//! (`system_accent1_0` … `system_accent1_1000`, cioè tono 100 … 0). Qui si scelgono le
//! tonalità di ogni ruolo come fa Material 3, per il tema chiaro e per quello scuro; l'ordine
//! dei ruoli è quello letto in `ui/theme.slint` (`M3.dyn`).

/// Indici delle 13 tonalità di Android: 0, 10, 50, 100, 200 … 900, 1000.
const SHADES: [u32; 13] = [0, 10, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];

/// Tavolozze nell'ordine in cui arrivano: accento 1, 2, 3, neutro 1, 2.
#[derive(Clone, Copy)]
enum Pal {
    A1 = 0,
    A2 = 1,
    A3 = 2,
    N1 = 3,
    N2 = 4,
}

fn at(tones: &[u32], p: Pal, shade: u32) -> u32 {
    let i = SHADES.iter().position(|s| *s == shade).unwrap_or(0);
    tones.get(p as usize * 13 + i).copied().unwrap_or(0xff808080)
}

/// Colori dell'icona dell'app (fondo, pagina sinistra, pagina destra, inchiostro) dalle tonalità,
/// come `system_accent1_500/10/100/700` su Android.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn icon_colors(tones: &[u32]) -> Option<[u32; 4]> {
    (tones.len() >= 65)
        .then(|| [at(tones, Pal::A1, 500), at(tones, Pal::A1, 10), at(tones, Pal::A1, 100), at(tones, Pal::A1, 700)])
}

/// Miscela di due colori ARGB (t = quota del secondo).
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xff) as f32;
    let m = |s: u32| ((ch(a, s) * (1.0 - t) + ch(b, s) * t).round() as u32) << s;
    0xff00_0000 | m(16) | m(8) | m(0)
}

/// Colore del tema predefinito quando il sistema non ne propone uno: blu.
pub const DEFAULT_SEED: u32 = 0xff1a73e8;
/// Colori proposti nella sezione Aspetto (oltre a quello di sistema).
pub const SEEDS: [u32; 8] =
    [DEFAULT_SEED, 0xff00897b, 0xff43a047, 0xff6750a4, 0xffd81b60, 0xffe53935, 0xffef6c00, 0xff6d4c41];

fn hsl_to_argb(h: f32, s: f32, l: f32) -> u32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| (((v + m).clamp(0.0, 1.0)) * 255.0).round() as u32;
    0xff00_0000 | to(r) << 16 | to(g) << 8 | to(b)
}

fn argb_to_hs(c: u32) -> (f32, f32) {
    let r = ((c >> 16) & 0xff) as f32 / 255.0;
    let g = ((c >> 8) & 0xff) as f32 / 255.0;
    let b = (c & 0xff) as f32 / 255.0;
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let l = (max + min) / 2.0;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s)
}

/// Le 65 tonalità (come quelle di sistema) ricavate da un colore: stessa tinta a saturazioni
/// diverse per accenti e neutri, chiarezza dal tono (0 = bianco, 1000 = nero).
pub fn tones_from_seed(seed: u32) -> Vec<u32> {
    let (h, s) = argb_to_hs(seed);
    let pals = [(h, s.max(0.45)), (h, s.max(0.45) * 0.35), (h + 60.0, s.max(0.45) * 0.6), (h, 0.06), (h, 0.12)];
    let mut out = Vec::with_capacity(65);
    for (ph, ps) in pals {
        for shade in SHADES {
            let l = 1.0 - shade as f32 / 1000.0;
            out.push(hsl_to_argb(ph, ps, l));
        }
    }
    out
}

/// Ruoli per il tema chiaro e quello scuro (ARGB); `None` se le tonalità non sono complete.
pub fn dyn_roles(tones: &[u32]) -> Option<(Vec<u32>, Vec<u32>)> {
    if tones.len() < 65 {
        return None;
    }
    use Pal::*;
    let t = |p, s| at(tones, p, s);
    let light = vec![
        t(A1, 600),                      // primary (tono 40)
        t(A1, 0),                        // on-primary
        t(A1, 100),                      // primary-container (90)
        t(A1, 900),                      // on-primary-container (10)
        t(A2, 600),                      // secondary
        t(A2, 100),                      // secondary-container
        t(A2, 900),                      // on-secondary-container
        t(A3, 100),                      // tertiary-container
        t(A3, 900),                      // on-tertiary-container
        t(N1, 10),                       // surface (99)
        mix(t(N1, 10), t(N1, 50), 0.5),  // surface-container-low (97)
        t(N1, 50),                       // surface-container (95)
        mix(t(N1, 50), t(N1, 100), 0.5), // surface-container-high (92)
        t(N1, 100),                      // surface-container-highest (90)
        t(N1, 900),                      // on-surface
        t(N2, 700),                      // on-surface-variant (30)
        t(N2, 500),                      // outline (50)
        t(N2, 200),                      // outline-variant (80)
        t(N1, 800),                      // inverse-surface (20)
        t(N1, 50),                       // inverse-on-surface (95)
    ];
    let dark = vec![
        t(A1, 200),                        // primary (80)
        t(A1, 800),                        // on-primary (20)
        t(A1, 700),                        // primary-container (30)
        t(A1, 100),                        // on-primary-container (90)
        t(A2, 200),                        // secondary
        t(A2, 700),                        // secondary-container
        t(A2, 100),                        // on-secondary-container
        t(A3, 700),                        // tertiary-container
        t(A3, 100),                        // on-tertiary-container
        mix(t(N1, 1000), t(N1, 900), 0.6), // surface (6)
        t(N1, 900),                        // surface-container-low (10)
        mix(t(N1, 900), t(N1, 800), 0.2),  // surface-container (12)
        mix(t(N1, 900), t(N1, 800), 0.7),  // surface-container-high (17)
        mix(t(N1, 800), t(N1, 700), 0.2),  // surface-container-highest (22)
        t(N1, 100),                        // on-surface (90)
        t(N2, 200),                        // on-surface-variant (80)
        t(N2, 400),                        // outline (60)
        t(N2, 700),                        // outline-variant (30)
        t(N1, 100),                        // inverse-surface (90)
        t(N1, 800),                        // inverse-on-surface (20)
    ];
    Some((light, dark))
}

#[cfg(test)]
mod tests {
    #[test]
    fn seed_gives_full_palette() {
        let t = super::tones_from_seed(super::DEFAULT_SEED);
        assert_eq!(t.len(), 65);
        assert_eq!(t[0], 0xffffffff); // tono 0 = bianco
        assert_eq!(t[12], 0xff000000); // tono 1000 = nero
        let (light, dark) = super::dyn_roles(&t).unwrap();
        // primario chiaro: un blu (più blu che rosso)
        assert!((light[0] & 0xff) > ((light[0] >> 16) & 0xff));
        assert_ne!(light[0], dark[0]);
    }

    use super::*;

    #[test]
    fn roles_from_tones() {
        // tonalità finte: il valore codifica tavolozza e posizione
        let tones: Vec<u32> = (0..65).map(|i| 0xff00_0000 | (i as u32)).collect();
        let (l, d) = dyn_roles(&tones).unwrap();
        assert_eq!((l.len(), d.len()), (20, 20));
        assert_eq!(l[0], 0xff00_0000 | 8); // accento 1, tonalità 600
        assert_eq!(d[0], 0xff00_0000 | 4); // accento 1, tonalità 200
        assert_eq!(mix(0xff000000, 0xffffffff, 0.5), 0xff808080);
        assert!(dyn_roles(&tones[..10]).is_none());
    }
}
