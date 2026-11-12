//! Tavola periodica interattiva: dati da `content/elements.json` (generato da
//! `tools/build_elements.py`) e conversione nei modelli della UI.
use crate::{ElementDetailVM, ElementVM};
use serde::Deserialize;
use slint::SharedString;

#[derive(Debug, Clone, Deserialize)]
pub struct Element {
    pub z: i32,
    pub sym: String,
    pub name: String,
    pub mass: Option<f64>,
    pub group: Option<i32>,
    pub period: i32,
    pub block: String,
    pub config: String,
    pub cat: String,
    pub en: Option<f64>,
    pub ea: Option<f64>,
    pub ie: Option<f64>,
    pub radius: Option<f64>,
    pub state: Option<String>,
    #[serde(default)]
    pub ox: Vec<String>,
    #[serde(default)]
    pub radioactive: bool,
    pub tendency: String,
    pub bonds: String,
}

#[derive(Deserialize)]
struct File {
    elements: Vec<Element>,
}

pub fn load() -> Vec<Element> {
    crate::bundled::BUNDLED
        .iter()
        .find(|(p, _)| *p == "content/elements.json")
        .and_then(|(_, t)| serde_json::from_str::<File>(t).ok())
        .map(|f| f.elements)
        .unwrap_or_default()
}

const CATS: [&str; 10] = [
    "Non metallo",
    "Gas nobile",
    "Metallo alcalino",
    "Metallo alcalino-terroso",
    "Semimetallo",
    "Alogeno",
    "Metallo del blocco p",
    "Metallo di transizione",
    "Lantanide",
    "Attinide",
];

fn cat_index(c: &str) -> i32 {
    CATS.iter().position(|x| *x == c).map(|i| i as i32).unwrap_or(0)
}

/// posizione nella griglia: colonna 1..18, riga 1..7; lantanidi e attinidi nelle righe 9 e 10
fn place(e: &Element) -> (i32, i32) {
    match e.group {
        Some(g) => (g, e.period),
        None if (58..=71).contains(&e.z) => (e.z - 58 + 4, 9),
        None if (90..=103).contains(&e.z) => (e.z - 90 + 4, 10),
        None => (3, e.period),
    }
}

fn dec(v: f64, digits: usize) -> String {
    format!("{v:.digits$}").replace('.', ",")
}

fn mass(e: &Element) -> String {
    match e.mass {
        Some(m) if e.radioactive && m.fract() == 0.0 => format!("[{}]", m as i64),
        Some(m) => dec(m, if m < 100.0 { 3 } else { 2 }),
        None => "—".into(),
    }
}

pub fn cell(e: &Element) -> ElementVM {
    let (col, row) = place(e);
    ElementVM {
        z: e.z,
        sym: e.sym.as_str().into(),
        name: e.name.as_str().into(),
        mass: match e.mass {
            Some(m) => dec(m, if m < 10.0 { 2 } else { 1 }).into(),
            None => SharedString::new(),
        },
        col,
        row,
        cat: cat_index(&e.cat),
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

pub fn detail(e: &Element) -> ElementDetailVM {
    let cat = cat_index(&e.cat);
    let metal = match cat {
        0 => "",
        5 => "non metallo",
        1 => "gas nobile (non metallo)",
        4 => "semimetallo",
        _ => "metallo",
    };
    let neutrons = match e.mass {
        Some(m) if !e.radioactive || m.fract() != 0.0 => {
            let a = m.round() as i32;
            format!("{} (A ≈ {a})", a - e.z)
        }
        Some(m) => format!("{} (isotopo più stabile, A = {})", m as i32 - e.z, m as i32),
        None => "—".into(),
    };
    let group = match e.group {
        Some(g) => format!("gruppo {g}"),
        None => {
            if e.z <= 71 {
                "lantanidi".into()
            } else {
                "attinidi".into()
            }
        }
    };
    let s = |v: Option<String>| v.unwrap_or_else(|| "non determinato".into());
    ElementDetailVM {
        z: e.z,
        sym: e.sym.as_str().into(),
        name: capital(&e.name).into(),
        cat,
        cat_name: e.cat.as_str().into(),
        mass: mass(e).into(),
        config: e.config.as_str().into(),
        protons: e.z.to_string().into(),
        electrons: e.z.to_string().into(),
        neutrons: neutrons.into(),
        position: format!("Z = {} · {} · periodo {} · blocco {}", e.z, group, e.period, e.block).into(),
        en: s(e.en.map(|v| dec(v, 2))).into(),
        ea: s(e.ea.map(|v| format!("{} kJ/mol", dec(v, 1)))).into(),
        ie: s(e.ie.map(|v| format!("{} kJ/mol", v.round() as i64))).into(),
        radius: s(e.radius.map(|v| format!("{} pm", v.round() as i64))).into(),
        state: s(e.state.clone()).into(),
        ox: if e.ox.is_empty() { "—".into() } else { e.ox.join(", ").replace('-', "−").into() },
        metal: metal.into(),
        tendency: e.tendency.as_str().into(),
        bonds: e.bonds.as_str().into(),
    }
}
