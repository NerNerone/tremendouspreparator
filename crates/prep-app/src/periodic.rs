//! Tavola periodica inclusa nell'app: dati da `assets/elements.json` (generato da
//! `tools/build_elements.py`), trasformati in uno strumento `grid` come quelli dei pacchetti
//! (si apre con la stessa pagina e si può copiare in un pacchetto come file).
use prep_core::tools::{Category, Cell, Field, Grid, Tool, ToolBody};
use serde::Deserialize;

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

/// Dati degli elementi (tabella generica, inclusa nell'app).
const ELEMENTS: &str = include_str!("../assets/elements.json");

pub fn load() -> Vec<Element> {
    serde_json::from_str::<File>(ELEMENTS).map(|f| f.elements).unwrap_or_default()
}

/// famiglie (nome nella legenda, nome nei dati, colore)
const CATS: [(&str, &str, &str); 10] = [
    ("Non metalli", "Non metallo", "#1E6B36"),
    ("Gas nobili", "Gas nobile", "#5B45A8"),
    ("Metalli alcalini", "Metallo alcalino", "#A0371F"),
    ("Alcalino-terrosi", "Metallo alcalino-terroso", "#8A5600"),
    ("Semimetalli", "Semimetallo", "#146470"),
    ("Alogeni", "Alogeno", "#6B6400"),
    ("Metalli del blocco p", "Metallo del blocco p", "#3B5675"),
    ("Metalli di transizione", "Metallo di transizione", "#2659A6"),
    ("Lantanidi", "Lantanide", "#8E2C7C"),
    ("Attinidi", "Attinide", "#9A2B4A"),
];

fn cat_index(c: &str) -> usize {
    CATS.iter().position(|x| x.1 == c).unwrap_or(0)
}

/// posizione nella griglia: colonna 1..18, riga 1..7; lantanidi e attinidi nelle righe 9 e 10
fn place(e: &Element) -> (u32, u32) {
    let (x, y) = match e.group {
        Some(g) => (g, e.period),
        None if (58..=71).contains(&e.z) => (e.z - 58 + 4, 9),
        None if (90..=103).contains(&e.z) => (e.z - 90 + 4, 10),
        None => (3, e.period),
    };
    (x.max(1) as u32, y.max(1) as u32)
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

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn cell(e: &Element) -> Cell {
    let (x, y) = place(e);
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
        None if e.z <= 71 => "lantanidi".into(),
        None => "attinidi".into(),
    };
    let s = |v: Option<String>| v.unwrap_or_else(|| "non determinato".into());
    let f = |l: &str, v: String| Field { label: l.into(), value: v };
    let h = |l: &str| Field { label: l.into(), value: String::new() };
    Cell {
        x,
        y,
        label: e.sym.clone(),
        top: e.z.to_string(),
        bottom: match e.mass {
            Some(m) => dec(m, if m < 10.0 { 2 } else { 1 }),
            None => String::new(),
        },
        category: CATS[cat].0.into(),
        title: capital(&e.name),
        subtitle: format!(
            "{}{} · Z = {} · {} · periodo {} · blocco {}",
            e.cat,
            if metal.is_empty() { String::new() } else { format!(" ({metal})") },
            e.z,
            group,
            e.period,
            e.block
        ),
        details: vec![
            h("Struttura dell'atomo"),
            f("Protoni (numero atomico Z)", e.z.to_string()),
            f("Elettroni (atomo neutro)", e.z.to_string()),
            f("Neutroni (isotopo più comune)", neutrons),
            f("Massa atomica", mass(e) + " u"),
            f("Configurazione elettronica", e.config.clone()),
            h("Proprietà periodiche"),
            f("Elettronegatività (Pauling)", s(e.en.map(|v| dec(v, 2)))),
            f("Affinità elettronica", s(e.ea.map(|v| format!("{} kJ/mol", dec(v, 1))))),
            f("Energia di 1ª ionizzazione", s(e.ie.map(|v| format!("{} kJ/mol", v.round() as i64)))),
            f("Raggio atomico", s(e.radius.map(|v| format!("{} pm", v.round() as i64)))),
            f("Stato a 25 °C", s(e.state.clone())),
            h("Comportamento chimico"),
            f("Elettroni: cede o acquista?", e.tendency.clone()),
            f("Legami tipici e con chi", e.bonds.clone()),
            f("Numeri di ossidazione", if e.ox.is_empty() { "—".into() } else { e.ox.join(", ").replace('-', "−") }),
        ],
        text: String::new(),
    }
}

/// La tavola periodica come strumento `grid` (id [`prep_core::model::TOOL_PERIODIC_TABLE`]).
pub fn tool(els: &[Element]) -> Tool {
    let mut row_labels: Vec<String> = (1..=7).map(|p| p.to_string()).collect();
    row_labels.extend(["".into(), "Lantanidi".into(), "Attinidi".into()]);
    Tool {
        schema: 1,
        id: prep_core::model::TOOL_PERIODIC_TABLE.into(),
        title: "Tavola periodica".into(),
        subtitle: "Elementi, famiglie e proprietà".into(),
        icon: String::new(),
        info: "Gruppi (colonne 1–18): stessi elettroni di valenza, proprietà chimiche simili.\n\
Periodi (righe 1–7): stesso numero di gusci elettronici.\n\
Da sinistra a destra lungo il periodo e salendo nel gruppo crescono elettronegatività, energia di \
ionizzazione e affinità elettronica; il raggio atomico e il carattere metallico invece diminuiscono.\n\
A sinistra i metalli (tendono a cedere elettroni), a destra i non metalli (tendono ad acquistarli), \
lungo la scaletta i semimetalli."
            .into(),
        body: ToolBody::Grid(Grid {
            columns: 18,
            rows: 10,
            categories: CATS.iter().map(|c| Category { name: c.0.into(), color: c.2.into() }).collect(),
            column_labels: (1..=18).map(|g| g.to_string()).collect(),
            row_labels,
            cells: els.iter().map(cell).collect(),
        }),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn periodic_tool_is_valid() {
        let t = super::tool(&super::load());
        assert!(t.problems().is_empty(), "{:?}", t.problems());
        let prep_core::tools::ToolBody::Grid(g) = &t.body else { panic!() };
        assert_eq!(g.cells.len(), 118);
    }
}
