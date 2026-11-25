//! Strumenti della materia (`content/tools/<id>.json`): pagine che un pacchetto porta con sé,
//! descritte solo con dati (niente codice). L'app sa disegnare tre tipi:
//!
//! - `grid`: caselle in una griglia, ognuna con una categoria (colore, legenda che filtra) e una
//!   scheda di dettaglio — la tavola periodica, il codice genetico, una tabella delle vitamine;
//! - `list`: schede raggruppate in sezioni, con la ricerca — costanti, formule, glossari;
//! - `calc`: campi numerici e risultati calcolati con formule — gas perfetti, diluizioni,
//!   conversioni. Le formule usano solo aritmetica e funzioni matematiche (vedi [`eval`]).
//!
//! La tavola periodica inclusa nell'app è anch'essa uno strumento `grid` (id
//! [`crate::model::TOOL_PERIODIC_TABLE`]), costruito dai dati degli elementi.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    #[serde(default = "one")]
    pub schema: u32,
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    /// simbolo (nome della lista delle icone delle materie); vuoto = quello del tipo
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// come si usa (pulsante ⓘ)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub info: String,
    #[serde(flatten)]
    pub body: ToolBody,
}

fn one() -> u32 {
    1
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ToolBody {
    Grid(Grid),
    List(List),
    Calc(Calc),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Grid {
    pub columns: u32,
    pub rows: u32,
    #[serde(default)]
    pub categories: Vec<Category>,
    /// intestazioni facoltative di colonne e righe (vuote = niente)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_labels: Vec<String>,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Category {
    pub name: String,
    /// `#RRGGBB`; vuoto = un colore del tema
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Cell {
    /// colonna e riga, da 1
    pub x: u32,
    pub y: u32,
    /// scritta grande (es. «Fe»), piccola in alto (es. «26») e in basso (es. «55,8»)
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub top: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bottom: String,
    /// nome di una delle `categories`
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// scheda di dettaglio: titolo (es. «Ferro»), una riga sotto, righe nome–valore (una riga con
    /// il solo `label` fa da titoletto di sezione), testo libero
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<Field>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Field {
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct List {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Section {
    pub title: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub title: String,
    /// in evidenza a destra (es. un valore: «6,022·10²³ mol⁻¹»)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Calc {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Input {
    /// nome usato nelle formule (lettere, cifre e `_`)
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default)]
    pub default: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Output {
    /// facoltativo: per usare il risultato nelle formule successive
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    pub formula: String,
    /// cifre significative mostrate (predefinito 4)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub digits: u32,
}

impl Tool {
    /// Tipo, a parole (per gli elenchi).
    pub fn kind_label(&self) -> &'static str {
        match self.body {
            ToolBody::Grid(_) => "griglia",
            ToolBody::List(_) => "elenco",
            ToolBody::Calc(_) => "calcolatore",
        }
    }

    /// Il file `content/tools/<id>.json` di questo strumento.
    pub fn path(&self) -> String {
        format!("content/tools/{}.json", self.id)
    }

    /// Problemi del file (per `prep check` e il controllo dei contenuti): vuoto se va bene.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let id = &self.id;
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            out.push(format!("strumento «{id}»: id solo con minuscole, cifre e trattini"));
        }
        if self.title.trim().is_empty() {
            out.push(format!("strumento {id}: manca il titolo"));
        }
        if !self.icon.is_empty() && crate::icons::lookup(&self.icon).is_none() {
            out.push(format!("strumento {id}: icona «{}» sconosciuta", self.icon));
        }
        match &self.body {
            ToolBody::Grid(g) => {
                if g.columns == 0 || g.rows == 0 || g.columns > 40 || g.rows > 40 {
                    out.push(format!("strumento {id}: griglia da 1 a 40 colonne e righe"));
                }
                let names: Vec<&str> = g.categories.iter().map(|c| c.name.as_str()).collect();
                for c in &g.categories {
                    if !c.color.is_empty() && crate::icons::parse_color(&c.color).is_none() {
                        out.push(format!("strumento {id}: colore «{}» non valido", c.color));
                    }
                }
                let mut seen = std::collections::HashSet::new();
                for c in &g.cells {
                    if c.x == 0 || c.y == 0 || c.x > g.columns || c.y > g.rows {
                        out.push(format!(
                            "strumento {id}: casella «{}» fuori dalla griglia ({}, {})",
                            c.label, c.x, c.y
                        ));
                    }
                    if !seen.insert((c.x, c.y)) {
                        out.push(format!("strumento {id}: due caselle in ({}, {})", c.x, c.y));
                    }
                    if !c.category.is_empty() && !names.contains(&c.category.as_str()) {
                        out.push(format!(
                            "strumento {id}: casella «{}», categoria «{}» non definita",
                            c.label, c.category
                        ));
                    }
                }
                if g.cells.is_empty() {
                    out.push(format!("strumento {id}: nessuna casella"));
                }
            }
            ToolBody::List(l) => {
                if l.sections.iter().all(|s| s.items.is_empty()) {
                    out.push(format!("strumento {id}: elenco vuoto"));
                }
            }
            ToolBody::Calc(c) => {
                let mut vars: HashMap<String, f64> = HashMap::new();
                for i in &c.inputs {
                    if !is_name(&i.id) {
                        out.push(format!(
                            "strumento {id}: campo «{}», id non valido (lettere, cifre, _; non «e» né «pi»)",
                            i.id
                        ));
                    }
                    vars.insert(i.id.clone(), i.default);
                }
                for o in &c.outputs {
                    if !o.id.is_empty() && !is_name(&o.id) {
                        out.push(format!("strumento {id}: risultato «{}», id non valido", o.id));
                    }
                    match eval(&o.formula, &vars) {
                        Ok(v) => {
                            if !o.id.is_empty() {
                                vars.insert(o.id.clone(), v);
                            }
                        }
                        Err(e) => out.push(format!("strumento {id}: formula di «{}»: {e}", o.label)),
                    }
                }
                if c.outputs.is_empty() {
                    out.push(format!("strumento {id}: nessun risultato"));
                }
            }
        }
        out
    }

    /// Risultati del calcolatore con i valori `values` (uno per campo, nell'ordine): testo
    /// già formattato, o il motivo per cui non si può calcolare.
    pub fn compute(c: &Calc, values: &[f64]) -> Vec<Result<f64, String>> {
        let mut vars: HashMap<String, f64> = c.inputs.iter().zip(values).map(|(i, v)| (i.id.clone(), *v)).collect();
        c.outputs
            .iter()
            .map(|o| {
                let r = eval(&o.formula, &vars);
                if let (Ok(v), false) = (&r, o.id.is_empty()) {
                    vars.insert(o.id.clone(), *v);
                }
                r
            })
            .collect()
    }
}

fn is_name(s: &str) -> bool {
    if s == "e" || s == "pi" {
        return false;
    }
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
}

/// Numero con `digits` cifre significative, virgola decimale, notazione ·10ⁿ per i valori molto
/// grandi o piccoli.
pub fn format_number(v: f64, digits: u32) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let d = if digits == 0 { 4 } else { digits.min(12) } as i32;
    if v == 0.0 {
        return "0".into();
    }
    let exp = v.abs().log10().floor() as i32;
    if !(-4..=6).contains(&exp) {
        let m = v / 10f64.powi(exp);
        let s = trim(format!("{:.*}", (d - 1).max(0) as usize, m));
        return format!("{s}·10{}", superscript(exp));
    }
    let decimals = (d - 1 - exp).max(0) as usize;
    trim(format!("{v:.decimals$}"))
}

fn trim(s: String) -> String {
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', ",")
}

fn superscript(n: i32) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}

/// Valuta una formula: numeri (col punto decimale: la virgola separa gli argomenti), variabili, `+ - * / ^`, parentesi,
/// costanti `pi` ed `e`, funzioni `sqrt ln log log10 log2 exp sin cos tan asin acos atan abs
/// min max pow round`. Nient'altro: le formule dei pacchetti non possono fare altro che calcoli.
pub fn eval(src: &str, vars: &HashMap<String, f64>) -> Result<f64, String> {
    if src.len() > 500 {
        return Err("formula troppo lunga".into());
    }
    let toks = lex(src)?;
    let mut p = Parser { t: &toks, i: 0, vars, depth: 0 };
    let v = p.expr()?;
    if p.i != toks.len() {
        return Err(format!("simbolo inatteso «{}»", toks[p.i].show()));
    }
    Ok(v)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(char),
}

impl Tok {
    fn show(&self) -> String {
        match self {
            Tok::Num(n) => n.to_string(),
            Tok::Name(s) => s.clone(),
            Tok::Op(c) => c.to_string(),
        }
    }
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if i < c.len()
                && (c[i] == 'e' || c[i] == 'E')
                && c.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == '-' || *d == '+')
            {
                i += 2;
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[start..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("numero non valido «{t}»"))?));
        } else if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Name(c[start..i].iter().collect()));
        } else if "+-*/^()×·÷;,".contains(ch) {
            out.push(Tok::Op(match ch {
                '×' | '·' => '*',
                '÷' => '/',
                ';' => ',',
                o => o,
            }));
            i += 1;
        } else {
            return Err(format!("carattere non ammesso «{ch}»"));
        }
    }
    Ok(out)
}

struct Parser<'a> {
    t: &'a [Tok],
    i: usize,
    vars: &'a HashMap<String, f64>,
    depth: u32,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn op(&mut self, o: char) -> bool {
        if self.peek() == Some(&Tok::Op(o)) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expr(&mut self) -> Result<f64, String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("formula troppo annidata".into());
        }
        let mut v = self.term()?;
        loop {
            if self.op('+') {
                v += self.term()?;
            } else if self.op('-') {
                v -= self.term()?;
            } else {
                break;
            }
        }
        self.depth -= 1;
        Ok(v)
    }
    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.unary()?;
        loop {
            if self.op('*') {
                v *= self.unary()?;
            } else if self.op('/') {
                v /= self.unary()?;
            } else {
                break;
            }
        }
        Ok(v)
    }
    fn unary(&mut self) -> Result<f64, String> {
        if self.op('-') {
            return Ok(-self.unary()?);
        }
        if self.op('+') {
            return self.unary();
        }
        self.power()
    }
    fn power(&mut self) -> Result<f64, String> {
        let base = self.atom()?;
        if self.op('^') {
            // a destra: 2^3^2 = 2^9
            let e = self.unary()?;
            return Ok(base.powf(e));
        }
        Ok(base)
    }
    fn atom(&mut self) -> Result<f64, String> {
        match self.t.get(self.i).cloned() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(n)
            }
            Some(Tok::Op('(')) => {
                self.i += 1;
                let v = self.expr()?;
                if !self.op(')') {
                    return Err("manca «)»".into());
                }
                Ok(v)
            }
            Some(Tok::Name(n)) => {
                self.i += 1;
                if self.op('(') {
                    let mut args = vec![self.expr()?];
                    while self.op(',') {
                        args.push(self.expr()?);
                    }
                    if !self.op(')') {
                        return Err("manca «)»".into());
                    }
                    return func(&n, &args);
                }
                match n.as_str() {
                    "pi" | "π" => Ok(std::f64::consts::PI),
                    "e" => Ok(std::f64::consts::E),
                    _ => self.vars.get(&n).copied().ok_or_else(|| format!("«{n}» non è un campo né un risultato")),
                }
            }
            Some(t) => Err(format!("simbolo inatteso «{}»", t.show())),
            None => Err("formula incompleta".into()),
        }
    }
}

fn func(name: &str, a: &[f64]) -> Result<f64, String> {
    let one = |f: fn(f64) -> f64| if a.len() == 1 { Ok(f(a[0])) } else { Err(format!("{name}() vuole 1 valore")) };
    let two =
        |f: fn(f64, f64) -> f64| if a.len() == 2 { Ok(f(a[0], a[1])) } else { Err(format!("{name}() vuole 2 valori")) };
    match name {
        "sqrt" => one(f64::sqrt),
        "ln" => one(f64::ln),
        "log" | "log10" => one(f64::log10),
        "log2" => one(f64::log2),
        "exp" => one(f64::exp),
        "sin" => one(f64::sin),
        "cos" => one(f64::cos),
        "tan" => one(f64::tan),
        "asin" => one(f64::asin),
        "acos" => one(f64::acos),
        "atan" => one(f64::atan),
        "abs" => one(f64::abs),
        "round" => one(f64::round),
        "pow" => two(f64::powf),
        "min" if !a.is_empty() => Ok(a.iter().copied().fold(f64::INFINITY, f64::min)),
        "max" if !a.is_empty() => Ok(a.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        _ => Err(format!("funzione «{name}» sconosciuta")),
    }
}

/// Strumenti del pacchetto dai file `content/tools/*.json` (quelli malformati vanno in `warnings`).
pub fn from_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>, warnings: &mut Vec<String>) -> Vec<Tool> {
    let mut out: Vec<Tool> = Vec::new();
    for (path, text) in files {
        if !(path.starts_with("content/tools/") && path.ends_with(".json")) {
            continue;
        }
        match serde_json::from_str::<Tool>(text) {
            Ok(t) => {
                out.retain(|x| x.id != t.id);
                out.push(t);
            }
            Err(e) => warnings.push(format!("{path}: {e}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(pairs: &[(&str, f64)]) -> HashMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn formulas() {
        let e = |s: &str| eval(s, &v(&[("n", 2.0), ("T", 300.0), ("V", 0.05)])).unwrap();
        assert_eq!(e("1 + 2 * 3"), 7.0);
        assert_eq!(e("(1 + 2) * 3"), 9.0);
        assert_eq!(e("2^3^2"), 512.0);
        assert_eq!(e("-2^2"), -4.0);
        assert_eq!(e("1.5 × 2"), 3.0);
        assert_eq!(e("pow(2, 3)"), 8.0);
        assert_eq!(e("max(1, 7, 3)"), 7.0);
        assert!((e("n * 0.0821 * T / V") - 985.2).abs() < 1e-9);
        assert!((e("log(1000) + ln(e) + sqrt(16)") - 8.0).abs() < 1e-12);
        assert_eq!(e("max(1; 7; 3)"), 7.0);
        assert!((e("6.022e23 / 1e23") - 6.022).abs() < 1e-12);
        assert!(eval("x + 1", &v(&[])).unwrap_err().contains("«x»"));
        assert!(eval("system(1)", &v(&[])).unwrap_err().contains("sconosciuta"));
        assert!(eval("1 +", &v(&[])).is_err());
        assert!(eval("\"rm\"", &v(&[])).is_err());
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!(format_number(985.2, 4), "985,2");
        assert_eq!(format_number(0.0123456, 3), "0,0123");
        assert_eq!(format_number(6.02214e23, 4), "6,022·10²³");
        assert_eq!(format_number(1.6e-19, 2), "1,6·10⁻¹⁹");
        assert_eq!(format_number(2.0, 4), "2");
    }

    #[test]
    fn tool_files_and_problems() {
        let calc = r#"{"id":"gas","title":"Gas perfetti","kind":"calc",
            "inputs":[{"id":"n","label":"Moli","unit":"mol","default":1},{"id":"T","label":"Temperatura","unit":"K","default":273.15},{"id":"V","label":"Volume","unit":"L","default":22.4}],
            "outputs":[{"id":"P","label":"Pressione","unit":"atm","formula":"n*0.082057*T/V","digits":3},{"label":"In kPa","unit":"kPa","formula":"P*101.325"}]}"#;
        let grid = r##"{"id":"mini","title":"Mini","kind":"grid","columns":2,"rows":1,
            "categories":[{"name":"A","color":"#ff0000"}],
            "cells":[{"x":1,"y":1,"label":"H","category":"A"},{"x":3,"y":1,"label":"X","category":"B"}]}"##;
        let list = r#"{"id":"costanti","title":"Costanti","kind":"list","sections":[{"title":"Chimica","items":[{"title":"Avogadro","value":"6,022·10²³ mol⁻¹"}]}]}"#;
        let mut w = Vec::new();
        let tools = from_files(
            [
                ("content/tools/gas.json", calc),
                ("content/tools/mini.json", grid),
                ("content/tools/c.json", list),
                ("content/tools/rotto.json", "{"),
            ],
            &mut w,
        );
        assert_eq!(tools.len(), 3);
        assert_eq!(w.len(), 1);
        let gas = &tools[0];
        assert!(gas.problems().is_empty(), "{:?}", gas.problems());
        let ToolBody::Calc(c) = &gas.body else { panic!() };
        let r = Tool::compute(c, &[1.0, 273.15, 22.4]);
        assert_eq!(format_number(*r[0].as_ref().unwrap(), 3), "1");
        assert!((r[1].as_ref().unwrap() - 101.325).abs() < 0.1);
        let p = tools[1].problems().join("\n");
        assert!(p.contains("fuori dalla griglia") && p.contains("categoria «B» non definita"), "{p}");
        assert!(tools[2].problems().is_empty());
    }
}
