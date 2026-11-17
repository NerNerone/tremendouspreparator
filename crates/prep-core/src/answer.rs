//! Correzione delle risposte, in particolare dei quesiti a completamento.

use crate::model::{QKind, Question};
use unicode_normalization::UnicodeNormalization;

/// Risposta data dall'utente.
#[derive(Debug, Clone, PartialEq)]
pub enum Given {
    None,
    Option(usize),
    Text(String),
}

impl Given {
    pub fn is_none(&self) -> bool {
        match self {
            Given::None => true,
            Given::Option(_) => false,
            Given::Text(t) => t.trim().is_empty(),
        }
    }

    /// rappresentazione salvata nelle sessioni: "B" oppure il testo digitato
    pub fn to_record(&self) -> Option<String> {
        match self {
            Given::None => None,
            Given::Option(i) => Some(crate::model::letter(*i).to_string()),
            Given::Text(t) if t.trim().is_empty() => None,
            Given::Text(t) => Some(t.trim().to_string()),
        }
    }
}

/// minuscolo, senza accenti, spazi compattati, senza punteggiatura ai bordi
pub fn normalize(s: &str) -> String {
    let lowered: String = s
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
        .replace(['’', '`'], "'");
    let collapsed = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim_matches(|c: char| c.is_ascii_punctuation() && c != '+' && c != '-').trim().to_string()
}

/// Forma compatta per simboli e formule: senza spazi, «_», «^», graffe e con pedici/apici
/// normali, così «K_a», «Ka», «Kₐ» e «k a» coincidono, come «μ_s» e «μs», «S_N2» e «SN2».
pub fn compact(s: &str) -> String {
    const MAP: [(char, char); 34] = [
        ('₀', '0'),
        ('₁', '1'),
        ('₂', '2'),
        ('₃', '3'),
        ('₄', '4'),
        ('₅', '5'),
        ('₆', '6'),
        ('₇', '7'),
        ('₈', '8'),
        ('₉', '9'),
        ('⁰', '0'),
        ('¹', '1'),
        ('²', '2'),
        ('³', '3'),
        ('⁴', '4'),
        ('⁵', '5'),
        ('⁶', '6'),
        ('⁷', '7'),
        ('⁸', '8'),
        ('⁹', '9'),
        ('ₐ', 'a'),
        ('ₑ', 'e'),
        ('ₒ', 'o'),
        ('ₓ', 'x'),
        ('ₕ', 'h'),
        ('ₖ', 'k'),
        ('ₗ', 'l'),
        ('ₘ', 'm'),
        ('ₙ', 'n'),
        ('ₚ', 'p'),
        ('ₛ', 's'),
        ('ₜ', 't'),
        ('ᵢ', 'i'),
        ('ᵥ', 'v'),
    ];
    s.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '_' | '^' | '{' | '}'))
        .map(|c| MAP.iter().find(|(a, _)| *a == c).map(|(_, b)| *b).unwrap_or(c))
        .collect::<String>()
        .to_lowercase()
}

/// Estrae il primo numero da un testo: accetta "1,5", "1.5", "3e-4", "3x10^-4",
/// "3·10^-4", "3*10^-4", "3 × 10⁻⁴" e ignora l'eventuale unità di misura dopo.
pub fn parse_number(s: &str) -> Option<f64> {
    let mut t = s.trim().to_string();
    // apici unicode -> normali
    let sup = [
        ('⁰', '0'),
        ('¹', '1'),
        ('²', '2'),
        ('³', '3'),
        ('⁴', '4'),
        ('⁵', '5'),
        ('⁶', '6'),
        ('⁷', '7'),
        ('⁸', '8'),
        ('⁹', '9'),
        ('⁻', '-'),
        ('⁺', '+'),
    ];
    let mut out = String::new();
    let mut in_sup = false;
    for ch in t.chars() {
        if let Some((_, n)) = sup.iter().find(|(u, _)| *u == ch) {
            if !in_sup {
                out.push('^');
                in_sup = true;
            }
            out.push(*n);
        } else {
            in_sup = false;
            out.push(ch);
        }
    }
    t = out.replace('−', "-").replace(' ', "");
    // virgola decimale (se non usata come separatore delle migliaia insieme al punto)
    if t.contains(',') && !t.contains('.') {
        t = t.replace(',', ".");
    } else {
        t = t.replace(',', "");
    }
    let bytes: Vec<char> = t.chars().collect();
    let mut i = 0;
    let mut mant = String::new();
    if i < bytes.len() && (bytes[i] == '-' || bytes[i] == '+') {
        mant.push(bytes[i]);
        i += 1;
    }
    let start_digits = i;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
        mant.push(bytes[i]);
        i += 1;
    }
    if i == start_digits {
        return None;
    }
    let mut value: f64 = mant.parse().ok()?;
    // esponente
    let rest: String = bytes[i..].iter().collect();
    let rest_l = rest.to_lowercase();
    let exp_str = if let Some(r) = rest_l.strip_prefix('e') {
        Some(r.to_string())
    } else {
        let mut r = rest_l.as_str();
        let mut matched = false;
        for p in ["x10^", "×10^", "·10^", "*10^", "x10", "×10", "·10", "*10"] {
            if let Some(x) = r.strip_prefix(p) {
                r = x;
                matched = true;
                break;
            }
        }
        if matched {
            Some(r.trim_start_matches('^').to_string())
        } else {
            None
        }
    };
    if let Some(e) = exp_str {
        let mut es = String::new();
        for (k, ch) in e.chars().enumerate() {
            if ch.is_ascii_digit() || (k == 0 && (ch == '-' || ch == '+')) {
                es.push(ch);
            } else {
                break;
            }
        }
        if let Ok(ex) = es.parse::<i32>() {
            value *= 10f64.powi(ex);
        }
    }
    Some(value)
}

pub fn format_number(v: f64) -> String {
    if v != 0.0 && (v.abs() >= 1e5 || v.abs() < 1e-3) {
        let e = v.abs().log10().floor() as i32;
        let m = v / 10f64.powi(e);
        format!("{}×10^{}", trim_float(m), e)
    } else {
        trim_float(v)
    }
}

fn trim_float(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    s.replace('.', ",")
}

pub fn is_correct(q: &Question, given: &Given) -> bool {
    match (q.kind, given) {
        (QKind::Mcq, Given::Option(i)) => q.answer == Some(*i),
        (QKind::Fill, Given::Text(t)) => {
            if t.trim().is_empty() {
                return false;
            }
            if let Some(n) = &q.numeric {
                if let Some(v) = parse_number(t) {
                    let tol = n.tolerance.max(1e-9);
                    let diff = (v - n.value).abs();
                    return if n.value == 0.0 { diff <= tol } else { diff / n.value.abs() <= tol };
                }
                false
            } else {
                let g = normalize(t);
                let gc = compact(&g);
                q.answers.iter().any(|a| {
                    let n = normalize(a);
                    n == g || (!gc.is_empty() && compact(&n) == gc)
                })
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_number("1,5"), Some(1.5));
        assert_eq!(parse_number("9.8 m/s²"), Some(9.8));
        assert!((parse_number("3x10^-4").unwrap() - 3e-4).abs() < 1e-12);
        assert!((parse_number("3·10⁻⁴").unwrap() - 3e-4).abs() < 1e-12);
        assert!((parse_number("6,02e23").unwrap() - 6.02e23).abs() < 1e18);
        assert_eq!(parse_number("-2"), Some(-2.0));
        assert_eq!(parse_number("abc"), None);
    }

    #[test]
    fn text() {
        assert_eq!(normalize("  Mitocòndrio. "), "mitocondrio");
        assert_eq!(normalize("Legge di  Stevino"), "legge di stevino");
        assert_eq!(compact(&normalize("Kₐ")), compact(&normalize("K_a")));
        assert_eq!(compact(&normalize("μs")), compact(&normalize("μ_s")));
        assert_eq!(compact(&normalize("v x")), compact(&normalize("v_x")));
    }
}
