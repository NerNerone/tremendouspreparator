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

/// Miscela di due colori ARGB (t = quota del secondo).
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xff) as f32;
    let m = |s: u32| ((ch(a, s) * (1.0 - t) + ch(b, s) * t).round() as u32) << s;
    0xff00_0000 | m(16) | m(8) | m(0)
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
