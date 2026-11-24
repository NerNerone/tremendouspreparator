//! Icone e colori delle materie decisi dal pacchetto.
//!
//! In `syllabus.json` ogni materia può indicare, tutti facoltativi:
//! - `icon`: il nome di un simbolo fra quelli inclusi nell'app ([`names`]);
//! - `icon_svg`: un tracciato SVG a una tinta nel riquadro 24×24 (solo comandi di tracciato);
//! - `color`: il colore della materia, `#RRGGBB`.
//!
//! L'app prova nell'ordine `icon_svg` valido, `icon` conosciuto, poi il monogramma (iniziali del
//! nome breve). Un pacchetto senza questi campi resta come prima; le versioni precedenti dell'app
//! ignorano i campi nuovi.

use crate::model::Subject;

include!("subject_icons.rs");

/// Lunghezza massima di `icon_svg`.
pub const SVG_MAX: usize = 2048;

/// Come disegnare l'icona di una materia.
#[derive(Debug, Clone, PartialEq)]
pub enum Glyph {
    /// Tracciato SVG con il suo riquadro (x, y, larghezza, altezza).
    Path { d: String, viewbox: (f32, f32, f32, f32) },
    /// Una o due lettere.
    Monogram(String),
}

/// Nomi dei simboli disponibili per `icon`, in ordine alfabetico.
pub fn names() -> impl Iterator<Item = &'static str> {
    SUBJECT_ICONS.iter().map(|(n, _)| *n)
}

/// Tracciato del simbolo `name` (riquadro 0 -960 960 960), se esiste. Accetta anche `_` al posto
/// di `-` e maiuscole.
pub fn lookup(name: &str) -> Option<&'static str> {
    let key = name.trim().to_ascii_lowercase().replace('_', "-");
    SUBJECT_ICONS.binary_search_by(|(n, _)| n.cmp(&key.as_str())).ok().map(|i| SUBJECT_ICONS[i].1)
}

/// Controlla un tracciato `icon_svg`: solo comandi di tracciato con il giusto numero di valori,
/// coordinate dentro (o poco fuori) il riquadro 24×24, al massimo [`SVG_MAX`] caratteri.
pub fn check_svg(d: &str) -> Result<(), String> {
    if d.len() > SVG_MAX {
        return Err(format!("troppo lungo ({} caratteri, massimo {SVG_MAX})", d.len()));
    }
    let mut toks = Tokens { s: d.as_bytes(), i: 0 };
    let mut cmd: Option<u8> = None;
    let mut any = false;
    loop {
        toks.skip();
        let Some(c) = toks.peek() else { break };
        let letter = c.is_ascii_alphabetic();
        let op = if letter {
            toks.i += 1;
            c
        } else {
            match cmd {
                // valori ripetuti dopo un comando: stessa regola (M diventa L)
                Some(b'M') => b'L',
                Some(b'm') => b'l',
                Some(p) => p,
                None => return Err("deve cominciare con un comando (M)".into()),
            }
        };
        if !any && !matches!(op, b'M' | b'm') {
            return Err("deve cominciare con M".into());
        }
        let n = match op.to_ascii_uppercase() {
            b'M' | b'L' | b'T' => 2,
            b'H' | b'V' => 1,
            b'C' => 6,
            b'S' | b'Q' => 4,
            b'A' => 7,
            b'Z' => 0,
            _ => return Err(format!("comando «{}» non ammesso", op as char)),
        };
        for k in 0..n {
            toks.skip();
            let v = toks.number().ok_or_else(|| format!("mancano valori dopo «{}»", op as char))?;
            if !v.is_finite() || v.abs() > 1000.0 {
                return Err("valore fuori misura".into());
            }
            // coordinate assolute: devono stare vicino al riquadro 0..24
            let coord = op.is_ascii_uppercase() && op != b'A' || (op == b'A' && k >= 5);
            if coord && !(-4.0..=28.0).contains(&v) {
                return Err("coordinate fuori dal riquadro 24×24".into());
            }
        }
        if n == 0 && !letter {
            return Err("valori dopo Z".into());
        }
        cmd = Some(op);
        any = true;
    }
    if !any {
        return Err("vuoto".into());
    }
    Ok(())
}

struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
}

impl Tokens<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn skip(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b',' || c == b'\n' || c == b'\t' || c == b'\r' {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    fn number(&mut self) -> Option<f64> {
        let start = self.i;
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.i += 1;
        }
        let mut dot = false;
        let mut digits = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                digits = true;
                self.i += 1;
            } else if c == b'.' && !dot {
                dot = true;
                self.i += 1;
            } else {
                break;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) && digits {
            self.i += 1;
            if matches!(self.peek(), Some(b'-' | b'+')) {
                self.i += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        if !digits {
            self.i = start;
            return None;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }
}

/// Colore `#RRGGBB` → 0xRRGGBB.
pub fn parse_color(s: &str) -> Option<u32> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    u32::from_str_radix(h, 16).ok()
}

/// Una o due lettere dal nome breve (o dal nome): «Cultura generale» → «Cu», «Diritto
/// privato» → «Dp».
pub fn monogram(subject: &Subject) -> String {
    let name = if subject.short.trim().is_empty() { &subject.name } else { &subject.short };
    let words: Vec<&str> =
        name.split_whitespace().filter(|w| w.chars().next().is_some_and(char::is_alphanumeric)).collect();
    let mut out = String::new();
    match words.as_slice() {
        [] => out.push('?'),
        [w] => {
            let mut cs = w.chars();
            out.extend(cs.next().into_iter().flat_map(char::to_uppercase));
            out.extend(cs.next().into_iter().flat_map(char::to_lowercase));
        }
        [a, b, ..] => {
            out.extend(a.chars().next().into_iter().flat_map(char::to_uppercase));
            out.extend(b.chars().next().into_iter().flat_map(char::to_lowercase));
        }
    }
    out
}

/// L'icona da disegnare per una materia: `icon_svg` valido, poi `icon` conosciuto, poi il
/// monogramma. Non fallisce mai.
pub fn glyph(subject: &Subject) -> Glyph {
    if !subject.icon_svg.trim().is_empty() && check_svg(&subject.icon_svg).is_ok() {
        return Glyph::Path { d: subject.icon_svg.trim().to_string(), viewbox: (0.0, 0.0, 24.0, 24.0) };
    }
    if let Some(d) = lookup(&subject.icon) {
        return Glyph::Path { d: d.to_string(), viewbox: (0.0, -960.0, 960.0, 960.0) };
    }
    Glyph::Monogram(monogram(subject))
}

/// Problemi delle icone e dei colori di una materia (per `prep check` e la pagina Contenuti):
/// avvisi, mai errori, perché l'app ripiega sul monogramma.
pub fn problems(subject: &Subject) -> Vec<String> {
    let mut out = Vec::new();
    if !subject.icon.trim().is_empty() && lookup(&subject.icon).is_none() {
        out.push(format!(
            "materia {}: icona «{}» sconosciuta, verrà usato il monogramma (nomi validi in docs/FORMATI.md)",
            subject.id, subject.icon
        ));
    }
    if !subject.icon_svg.trim().is_empty() {
        if let Err(e) = check_svg(&subject.icon_svg) {
            out.push(format!("materia {}: icon_svg non valido ({e})", subject.id));
        }
    }
    if !subject.color.trim().is_empty() && parse_color(&subject.color).is_none() {
        out.push(format!("materia {}: colore «{}» non valido (serve #RRGGBB)", subject.id, subject.color));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subj(name: &str, short: &str, icon: &str, svg: &str) -> Subject {
        Subject {
            id: "x".into(),
            name: name.into(),
            short: short.into(),
            units: vec![],
            tools: vec![],
            links: vec![],
            icon: icon.into(),
            icon_svg: svg.into(),
            color: String::new(),
        }
    }

    #[test]
    fn table_is_sorted_and_lookup_works() {
        assert!(SUBJECT_ICONS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(SUBJECT_ICONS.len() >= 150);
        assert!(lookup("biotech").is_some());
        assert!(lookup("Medical_Services").is_some());
        assert!(lookup("non-esiste").is_none());
    }

    #[test]
    fn svg_checks_accept_good_and_refuse_bad() {
        assert!(check_svg("M10 3a2 2 0 0 1 4 0v2h4a1 1 0 0 1 1 1v4h-2a2 2 0 0 0 0 4h2v4a1 1 0 0 1-1 1h-4v-2a2 2 0 0 0-4 0v2H6a1 1 0 0 1-1-1v-4h2a2 2 0 0 0 0-4H5V6a1 1 0 0 1 1-1h4z").is_ok());
        assert!(check_svg("M2 12h4l2.5-6 4 12 3-9 1.5 3H22").is_ok());
        assert!(check_svg("M1 1 L 2").is_err(), "valori mancanti");
        assert!(check_svg("<script>").is_err());
        assert!(check_svg("L1 1").is_err(), "deve cominciare con M");
        assert!(check_svg("M500 500 L600 600").is_err(), "fuori dal riquadro");
        assert!(check_svg(&"M1 1 L2 2 ".repeat(300)).is_err(), "troppo lungo");
        assert!(check_svg("").is_err());
    }

    #[test]
    fn glyph_falls_back_to_monogram() {
        assert!(
            matches!(glyph(&subj("Biologia", "", "biotech", "")), Glyph::Path { viewbox: (_, y, _, _), .. } if y == -960.0)
        );
        assert!(
            matches!(glyph(&subj("Logica", "", "", "M2 12h4")), Glyph::Path { viewbox: (_, y, _, _), .. } if y == 0.0)
        );
        // tracciato rotto e nome sconosciuto: monogramma, senza errori
        assert_eq!(glyph(&subj("Cultura generale", "", "boh", "M1")), Glyph::Monogram("Cg".into()));
        assert_eq!(glyph(&subj("Chimica e propedeutica", "Chimica", "", "")), Glyph::Monogram("Ch".into()));
        assert_eq!(problems(&subj("Cultura", "", "boh", "M1")).len(), 2);
        assert!(problems(&subj("Biologia", "", "biotech", "")).is_empty());
    }

    #[test]
    fn guide_and_prompt_list_every_symbol() {
        let guide = include_str!("../../prep-app/assets/guida/02-esame.md");
        let prompt = crate::creator::prompt_syllabus("x", "X");
        for n in names() {
            assert!(guide.contains(&format!("`{n}`")), "la guida 2.2 non elenca {n}");
            assert!(prompt.contains(n), "il prompt non elenca {n}");
        }
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#B3261E"), Some(0xB3261E));
        assert_eq!(parse_color("B3261E"), None);
        assert_eq!(parse_color("#fff"), None);
    }
}
