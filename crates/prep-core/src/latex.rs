//! LaTeX → testo Unicode leggibile, per le formule in linea (`$…$`).
//! Le formule a blocco (`$$…$$`) sono invece impaginate come SVG (vedi `tools/mathsvg.mjs`)
//! e questa conversione resta il fallback.

fn sup(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '−' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'T' => 'ᵀ',
        ' ' => ' ',
        '°' => '°',
        '∘' => '°',
        _ => return None,
    })
}

fn sub(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '−' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        ' ' => ' ',
        _ => return None,
    })
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "cdot" => "·",
        "times" => "×",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "approx" => "≈",
        "simeq" => "≃",
        "sim" => "∼",
        "neq" => "≠",
        "ne" => "≠",
        "leq" => "≤",
        "le" => "≤",
        "geq" => "≥",
        "ge" => "≥",
        "ll" => "≪",
        "gg" => "≫",
        "equiv" => "≡",
        "propto" => "∝",
        "infty" => "∞",
        "rightarrow" => "→",
        "to" => "→",
        "leftarrow" => "←",
        "Rightarrow" => "⇒",
        "implies" => "⇒",
        "Leftarrow" => "⇐",
        "leftrightarrow" => "↔",
        "Leftrightarrow" => "⇔",
        "iff" => "⇔",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "rightleftharpoons" => "⇌",
        "longrightarrow" => "⟶",
        "circ" => "°",
        "degree" => "°",
        "partial" => "∂",
        "nabla" => "∇",
        "int" => "∫",
        "oint" => "∮",
        "sum" => "∑",
        "prod" => "∏",
        "perp" => "⊥",
        "parallel" => "∥",
        "angle" => "∠",
        "wedge" => "∧",
        "vee" => "∨",
        "cdots" => "⋯",
        "ldots" => "…",
        "dots" => "…",
        "forall" => "∀",
        "exists" => "∃",
        "in" => "∈",
        "notin" => "∉",
        "cup" => "∪",
        "cap" => "∩",
        "subset" => "⊂",
        "emptyset" => "∅",
        "hbar" => "ħ",
        "ell" => "ℓ",
        "AA" => "Å",
        "prime" => "′",
        "%" => "%",
        "_" => "_",
        "$" => "$",
        "#" => "#",
        "&" => "&",
        "{" => "{",
        "}" => "}",
        "," => "\u{2009}",
        ";" => " ",
        ":" => " ",
        "!" => "",
        " " => " ",
        "quad" => "  ",
        "qquad" => "    ",
        "left" => "",
        "right" => "",
        "big" => "",
        "Big" => "",
        "bigg" => "",
        "displaystyle" => "",
        "limits" => "",
        "nolimits" => "",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "cot" => "cot",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "max" => "max",
        "min" => "min",
        "lim" => "lim",
        "arcsin" => "arcsin",
        "arccos" => "arccos",
        "arctan" => "arctan",
        _ => return None,
    })
}

struct P<'a> {
    c: Vec<char>,
    i: usize,
    _s: &'a str,
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    /// legge un argomento: {gruppo} oppure un singolo carattere/comando
    fn arg(&mut self) -> String {
        while self.peek() == Some(' ') {
            self.i += 1;
        }
        match self.peek() {
            Some('{') => {
                self.i += 1;
                let start = self.i;
                let mut depth = 1;
                while let Some(ch) = self.peek() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
                let inner: String = self.c[start..self.i].iter().collect();
                self.i += 1;
                convert(&inner)
            }
            Some('\\') => {
                let start = self.i;
                self.i += 1;
                while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    self.i += 1;
                }
                if self.i == start + 1 {
                    self.i += 1;
                }
                let s: String = self.c[start..self.i].iter().collect();
                convert(&s)
            }
            Some(ch) => {
                self.i += 1;
                ch.to_string()
            }
            None => String::new(),
        }
    }

    /// argomento grezzo (senza conversione), per \text
    fn raw_arg(&mut self) -> String {
        while self.peek() == Some(' ') {
            self.i += 1;
        }
        if self.peek() != Some('{') {
            return self
                .peek()
                .map(|c| {
                    self.i += 1;
                    c.to_string()
                })
                .unwrap_or_default();
        }
        self.i += 1;
        let start = self.i;
        let mut depth = 1;
        while let Some(ch) = self.peek() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
        let inner: String = self.c[start..self.i].iter().collect();
        self.i += 1;
        inner.replace("\\,", "\u{2009}").replace("\\%", "%").replace("\\ ", " ")
    }
}

fn wrap_if_complex(s: &str) -> String {
    let t = s.trim();
    let simple = t.chars().all(|c| c.is_alphanumeric() || ",.′'°⁻⁰¹²³⁴⁵⁶⁷⁸⁹₀₁₂₃₄₅₆₇₈₉ₐₑₒₓ·".contains(c))
        || (t.starts_with('(') && t.ends_with(')'));
    if simple {
        t.to_string()
    } else {
        format!("({t})")
    }
}

fn script(s: &str, up: bool) -> String {
    let t = s.trim();
    let map = if up { sup } else { sub };
    if !t.is_empty() && t.chars().all(|c| map(c).is_some()) {
        t.chars().map(|c| map(c).unwrap()).collect()
    } else if up {
        format!("^({t})")
    } else {
        format!("_({t})")
    }
}

/// Converte un frammento LaTeX (senza i `$`).
pub fn convert(tex: &str) -> String {
    let mut p = P { c: tex.chars().collect(), i: 0, _s: tex };
    let mut out = String::new();
    while let Some(ch) = p.peek() {
        match ch {
            '\\' => {
                p.i += 1;
                let start = p.i;
                while p.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    p.i += 1;
                }
                let name: String = if p.i == start {
                    p.i += 1;
                    p.c.get(start).map(|c| c.to_string()).unwrap_or_default()
                } else {
                    p.c[start..p.i].iter().collect()
                };
                match name.as_str() {
                    "frac" | "dfrac" | "tfrac" => {
                        let a = p.arg();
                        let b = p.arg();
                        out.push_str(&format!("{}/{}", wrap_if_complex(&a), wrap_if_complex(&b)));
                    }
                    "sqrt" => {
                        let mut idx = String::new();
                        if p.peek() == Some('[') {
                            p.i += 1;
                            while let Some(c) = p.peek() {
                                p.i += 1;
                                if c == ']' {
                                    break;
                                }
                                idx.push(c);
                            }
                        }
                        let a = p.arg();
                        let root = match idx.as_str() {
                            "3" => "∛",
                            "4" => "∜",
                            "" => "√",
                            _ => "√",
                        };
                        out.push_str(root);
                        out.push_str(&wrap_if_complex(&a));
                    }
                    "text" | "mathrm" | "textrm" | "mbox" | "operatorname" | "textit" | "textbf" => {
                        out.push_str(&p.raw_arg());
                    }
                    "mathbf" | "boldsymbol" | "mathit" | "mathcal" | "mathbb" | "bm" => {
                        let a = p.arg();
                        out.push_str(&a);
                    }
                    "vec" | "overrightarrow" => {
                        let a = p.arg();
                        out.push_str(&a);
                        out.push('\u{20D7}');
                    }
                    "hat" => {
                        let a = p.arg();
                        out.push_str(&a);
                        out.push('\u{0302}');
                    }
                    "bar" | "overline" => {
                        let a = p.arg();
                        out.push_str(&a);
                        out.push('\u{0305}');
                    }
                    "dot" => {
                        let a = p.arg();
                        out.push_str(&a);
                        out.push('\u{0307}');
                    }
                    "tilde" => {
                        let a = p.arg();
                        out.push_str(&a);
                        out.push('\u{0303}');
                    }
                    "underbrace" | "overbrace" | "boxed" | "mathstrut" => {
                        let a = p.arg();
                        out.push_str(&a);
                    }
                    "begin" | "end" => {
                        let _ = p.raw_arg();
                    }
                    other => match symbol(other) {
                        Some(s) => {
                            out.push_str(s);
                            // spazio dopo gli operatori-funzione (sin x)
                            if ["sin", "cos", "tan", "log", "ln", "exp", "lim"].contains(&other)
                                && p.peek().is_some_and(|c| c.is_alphanumeric())
                            {
                                out.push(' ');
                            }
                        }
                        None => out.push_str(other),
                    },
                }
            }
            '^' | '_' => {
                p.i += 1;
                let a = p.arg();
                out.push_str(&script(&a, ch == '^'));
            }
            '{' => {
                let a = p.arg();
                out.push_str(&a);
            }
            '}' => p.i += 1,
            '~' => {
                p.i += 1;
                out.push(' ');
            }
            '-' => {
                p.i += 1;
                out.push('−');
            }
            '&' => p.i += 1,
            _ => {
                p.i += 1;
                out.push(ch);
            }
        }
    }
    // spazi multipli
    let mut res = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                res.push(c);
            }
            prev_space = true;
        } else {
            res.push(c);
            prev_space = false;
        }
    }
    res.trim().to_string()
}

/// Nome del file SVG di una formula a blocco: `books/_math/<sha1 del tex>.svg`
/// (stesso calcolo in `tools/mathsvg.mjs`).
pub fn math_path(tex: &str) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(tex.trim().as_bytes());
    format!("books/_math/{}.svg", h.digest())
}

/// Sostituisce ogni `$…$` (e `$$…$$` in linea) in un testo con la conversione Unicode.
pub fn render_inline(text: &str) -> String {
    if !text.contains('$') {
        return text.to_string();
    }
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < c.len() {
        if c[i] == '\\' && c.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
            continue;
        }
        if c[i] == '$' {
            let double = c.get(i + 1) == Some(&'$');
            let open = if double { 2 } else { 1 };
            let mut j = i + open;
            let mut found = None;
            while j < c.len() {
                if c[j] == '\\' {
                    j += 2;
                    continue;
                }
                if c[j] == '$' && (!double || c.get(j + 1) == Some(&'$')) {
                    found = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(end) = found {
                let inner: String = c[i + open..end].iter().collect();
                out.push_str(&convert(&inner));
                i = end + open;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        assert_eq!(convert(r"2,99 \cdot 10^8 \text{ m/s}"), "2,99 · 10⁸ m/s");
        assert_eq!(convert(r"6,94 \cdot 10^{-3} \text{ kg}"), "6,94 · 10⁻³ kg");
        assert_eq!(convert(r"v = \frac{s}{t}"), "v = s/t");
        assert_eq!(convert(r"\frac{1}{2} m v^2"), "1/2 m v²");
        assert_eq!(convert(r"{}^4_2\text{He}"), "⁴₂He");
        assert_eq!(convert(r"\Delta x"), "Δ x");
        assert_eq!(convert(r"\vec{F}"), "F\u{20D7}");
        assert_eq!(convert(r"x_{1/2} = \frac{\ln 2}{\mu}"), "x_(1/2) = (ln 2)/μ");
        assert_eq!(convert(r"\sqrt{2gh}"), "√2gh");
        assert_eq!(convert(r"\sqrt{a+b}"), "√(a+b)");
        assert_eq!(convert(r"0{,}18\text{ W/m}^2"), "0,18 W/m²");
    }

    #[test]
    fn inline_text() {
        assert_eq!(render_inline(r"La massa $m = 2\text{ kg}$ costa \$5"), "La massa m = 2 kg costa $5");
        assert_eq!(render_inline("nessuna formula"), "nessuna formula");
    }
}
