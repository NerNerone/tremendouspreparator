//! Strumenti della materia: la pagina che apre uno strumento (griglia, elenco, calcolatore) e,
//! in «Gestisci contenuti», la sezione che li elenca, li copia tra i pacchetti e li condivide uno
//! alla volta. La tavola periodica inclusa nell'app è uno strumento `grid` come gli altri.

use super::*;
use crate::{ExamVM, ToolAxisVM, ToolCatVM, ToolCellVM, ToolFieldVM, ToolInVM, ToolItemVM, ToolOutVM, ToolRowVM};
use prep_core::contents::{self, Kind};
use prep_core::model::TOOL_PERIODIC_TABLE;
use prep_core::tools::{format_number, Tool, ToolBody};

/// Colori di riserva per le famiglie senza colore (stesso ordine per tutti gli strumenti).
const FALLBACK: [&str; 8] = ["#2659A6", "#1E6B36", "#A0371F", "#5B45A8", "#8A5600", "#146470", "#8E2C7C", "#3B5675"];

/// (titolo, riga sotto, tipo) degli strumenti inclusi nell'app, senza costruirli.
fn builtin_head(id: &str) -> Option<(&'static str, &'static str, i32)> {
    (id == TOOL_PERIODIC_TABLE).then_some(("Tavola periodica", "Elementi, famiglie e proprietà", 0))
}

fn kind_n(t: &Tool) -> i32 {
    match t.body {
        ToolBody::Grid(_) => 0,
        ToolBody::List(_) => 1,
        ToolBody::Calc(_) => 2,
    }
}

// ---------- colori: da un solo colore, le tinte per i due temi ----------

fn rgb(hex: &str) -> Option<(f32, f32, f32)> {
    let v = prep_core::icons::parse_color(hex)?;
    Some((((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0))
}

fn to_hsl((r, g, b): (f32, f32, f32)) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> slint::Color {
    let f = |n: f32| {
        let k = (n + h * 12.0) % 12.0;
        let a = s * l.min(1.0 - l);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    slint::Color::from_rgb_u8(c(f(0.0)), c(f(8.0)), c(f(4.0)))
}

/// Fondo e accento chiari e scuri: stessa tinta, luminosità fisse (le caselle restano leggibili
/// qualunque colore scelga il pacchetto).
fn shades(name: &str, hex: &str) -> ToolCatVM {
    let (h, s, _) = to_hsl(rgb(hex).unwrap_or((0.15, 0.35, 0.65)));
    ToolCatVM {
        name: ss(name),
        bg_l: from_hsl(h, (s * 0.75).min(0.7), 0.88),
        bg_d: from_hsl(h, (s * 0.45).min(0.4), 0.26),
        ac_l: from_hsl(h, s.min(0.75), 0.28),
        ac_d: from_hsl(h, (s * 0.8).min(0.75), 0.76),
    }
}

impl Ctrl {
    /// Strumento incluso nell'app con questo id (costruito alla prima richiesta).
    fn builtin_tool(&mut self, id: &str) -> Option<Tool> {
        if id != TOOL_PERIODIC_TABLE {
            return None;
        }
        if self.periodic.is_none() {
            self.periodic = Some(periodic::tool(&periodic::load()));
        }
        self.periodic.clone()
    }

    /// Lo strumento `id`: quello del pacchetto se c'è, altrimenti quello incluso nell'app.
    fn find_tool(&mut self, id: &str) -> Option<Tool> {
        match self.content.tools.iter().find(|t| t.id == id) {
            Some(t) => Some(t.clone()),
            None => self.builtin_tool(id),
        }
    }

    /// Strumenti della materia per Studia › Strumenti: quelli che la materia elenca in `tools`,
    /// più quelli del pacchetto che nessuna materia elenca (valgono per tutte).
    pub(super) fn subject_tools(&self, subject: Option<&prep_core::model::Subject>) -> Vec<ToolVM> {
        let syl = self.content.syllabus();
        let listed = |id: &str| syl.subjects.iter().any(|s| s.tools.iter().any(|t| t == id));
        let mut out: Vec<ToolVM> = Vec::new();
        let mut push = |id: &str, title: &str, sub: &str, tk: i32| {
            if !out.iter().any(|t| t.id == id) {
                out.push(ToolVM { kind: 4, id: ss(id), label: ss(title), sub: ss(sub), tk });
            }
        };
        for id in subject.map(|s| s.tools.as_slice()).unwrap_or_default() {
            if let Some(t) = self.content.tools.iter().find(|t| &t.id == id) {
                push(&t.id, &t.title, &t.subtitle, kind_n(t));
            } else if let Some((title, sub, tk)) = builtin_head(id) {
                push(id, title, sub, tk);
            }
        }
        for t in self.content.tools.iter().filter(|t| !listed(&t.id)) {
            push(&t.id, &t.title, &t.subtitle, kind_n(t));
        }
        out
    }

    // ---------- pagina dello strumento ----------

    pub(super) fn open_tool(&mut self, id: &str) {
        let Some(t) = self.find_tool(id) else {
            self.toast("Strumento non trovato in questo pacchetto");
            return;
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_tl_title(ss(&t.title));
        st.set_tl_subtitle(ss(&t.subtitle));
        st.set_tl_info(ss(&t.info));
        st.set_tl_kind(kind_n(&t));
        st.set_tl_sel(-1);
        st.set_tl_filter(-1);
        st.set_tl_query(ss(""));
        match &t.body {
            ToolBody::Grid(g) => {
                let cats: Vec<ToolCatVM> = g
                    .categories
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        shades(&c.name, if c.color.is_empty() { FALLBACK[i % FALLBACK.len()] } else { &c.color })
                    })
                    .collect();
                // righe vuote (senza caselle né etichetta) più basse: separano i blocchi
                let rows = g.rows.max(1) as usize;
                let mut pos = Vec::with_capacity(rows);
                let mut y = 0f32;
                for r in 1..=rows as u32 {
                    pos.push(y);
                    let used = g.cells.iter().any(|c| c.y == r)
                        || g.row_labels.get(r as usize - 1).is_some_and(|l| !l.is_empty());
                    y += if used { 1.0 } else { 0.35 };
                }
                let row_labels = g
                    .row_labels
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| !l.is_empty())
                    .map(|(i, l)| {
                        let first = g.cells.iter().filter(|c| c.y == i as u32 + 1).map(|c| c.x).min().unwrap_or(1);
                        // un'etichetta lunga si allarga sulle colonne libere prima della prima casella
                        let span = if l.chars().count() > 3 { first.saturating_sub(1) as i32 } else { 0 };
                        ToolAxisVM { text: ss(l), pos: pos[i.min(rows - 1)], span }
                    })
                    .collect();
                let names: Vec<&str> = g.categories.iter().map(|c| c.name.as_str()).collect();
                let cells: Vec<ToolCellVM> = g
                    .cells
                    .iter()
                    .filter(|c| c.x >= 1 && c.y >= 1 && c.x <= g.columns && c.y <= g.rows)
                    .map(|c| ToolCellVM {
                        x: c.x as i32,
                        y: c.y as i32,
                        label: ss(&c.label),
                        top: ss(&c.top),
                        bottom: ss(&c.bottom),
                        cat: names.iter().position(|n| *n == c.category).map(|i| i as i32).unwrap_or(-1),
                    })
                    .collect();
                st.set_tl_cols(g.columns.max(1) as i32);
                st.set_tl_rows_h(y);
                st.set_tl_row_pos(model(pos));
                st.set_tl_cats(model(cats));
                st.set_tl_cells(model(cells));
                st.set_tl_col_labels(model(g.column_labels.iter().map(ss).collect()));
                // colonna delle etichette larga quanto la più lunga di quelle che non si allargano
                let longest = g
                    .row_labels
                    .iter()
                    .enumerate()
                    .filter(|(i, l)| {
                        let first = g.cells.iter().filter(|c| c.y == *i as u32 + 1).map(|c| c.x).min().unwrap_or(1);
                        l.chars().count() <= 3 || first <= 1
                    })
                    .map(|(_, l)| l.chars().count())
                    .max()
                    .unwrap_or(0);
                st.set_tl_lab_w((longest as i32 * 8 + 10).clamp(22, 72));
                st.set_tl_row_labels(model(row_labels));
            }
            ToolBody::List(_) => {}
            ToolBody::Calc(c) => {
                self.tl_values = c.inputs.iter().map(|i| i.default).collect();
                st.set_tl_inputs(model(
                    c.inputs
                        .iter()
                        .map(|i| ToolInVM {
                            label: ss(&i.label),
                            unit: ss(&i.unit),
                            text: ss(format_number(i.default, 8)),
                        })
                        .collect(),
                ));
            }
        }
        self.tool = Some(t);
        self.tl_refresh_list();
        self.tl_refresh_calc();
        self.set_page(P_TOOL);
    }

    /// Scheda di una casella della griglia (−2: come si legge, −1: chiusa).
    pub(super) fn tl_select(&mut self, i: i32) {
        let Some(Tool { body: ToolBody::Grid(g), .. }) = &self.tool else {
            return;
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let shown: Vec<&prep_core::tools::Cell> =
            g.cells.iter().filter(|c| c.x >= 1 && c.y >= 1 && c.x <= g.columns && c.y <= g.rows).collect();
        let Some(c) = usize::try_from(i).ok().and_then(|i| shown.get(i)) else {
            st.set_tl_sel(i.max(-2));
            return;
        };
        let cat = g.categories.iter().position(|x| x.name == c.category).map(|i| i as i32).unwrap_or(-1);
        st.set_tl_d_cell(ToolCellVM {
            x: c.x as i32,
            y: c.y as i32,
            label: ss(&c.label),
            top: ss(&c.top),
            bottom: ss(&c.bottom),
            cat,
        });
        st.set_tl_d_title(ss(if c.title.is_empty() { &c.label } else { &c.title }));
        st.set_tl_d_sub(ss(&c.subtitle));
        st.set_tl_d_text(ss(&c.text));
        st.set_tl_d_fields(model(
            c.details.iter().map(|f| ToolFieldVM { label: ss(&f.label), value: ss(&f.value) }).collect(),
        ));
        st.set_tl_sel(i);
    }

    pub(super) fn tl_search(&mut self, q: &str) {
        self.ui().global::<AppState>().set_tl_query(ss(q));
        self.tl_refresh_list();
    }

    /// Elenco filtrato dalla ricerca: sezioni con le loro voci (le sezioni vuote spariscono).
    fn tl_refresh_list(&mut self) {
        let Some(Tool { body: ToolBody::List(l), .. }) = &self.tool else {
            return;
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let q = st.get_tl_query().to_lowercase();
        let q = q.trim();
        let mut rows = Vec::new();
        for s in &l.sections {
            let sec_hit = !q.is_empty() && s.title.to_lowercase().contains(q);
            let items: Vec<_> = s
                .items
                .iter()
                .filter(|it| {
                    q.is_empty()
                        || sec_hit
                        || [&it.title, &it.value, &it.body].iter().any(|t| t.to_lowercase().contains(q))
                })
                .collect();
            if items.is_empty() {
                continue;
            }
            if !s.title.is_empty() {
                rows.push(ToolRowVM { head: true, title: ss(&s.title), ..Default::default() });
            }
            let n = items.len();
            rows.extend(items.into_iter().enumerate().map(|(i, it)| ToolRowVM {
                head: false,
                title: ss(&it.title),
                value: ss(&it.value),
                body: ss(&it.body),
                first: i == 0,
                last: i + 1 == n,
            }));
        }
        if let Some(m) = sync_model(st.get_tl_rows(), rows) {
            st.set_tl_rows(m);
        }
    }

    /// Un campo del calcolatore cambiato: numero (anche con la virgola) o piccola espressione.
    pub(super) fn tl_input(&mut self, i: i32, text: &str) {
        let Ok(i) = usize::try_from(i) else { return };
        let t = text.trim().replace(',', ".");
        let v = if t.is_empty() {
            Some(0.0)
        } else {
            prep_core::tools::eval(&t, &Default::default()).ok().filter(|v| v.is_finite())
        };
        if let (Some(v), Some(slot)) = (v, self.tl_values.get_mut(i)) {
            *slot = v;
        } else if let Some(slot) = self.tl_values.get_mut(i) {
            *slot = f64::NAN;
        }
        let ui = self.ui();
        let m = ui.global::<AppState>().get_tl_inputs();
        if let Some(mut r) = m.row_data(i) {
            r.text = ss(text);
            m.set_row_data(i, r);
        }
        self.tl_refresh_calc();
    }

    /// Valori di partenza del calcolatore.
    pub(super) fn tl_reset(&mut self) {
        if let Some(t) = self.tool.as_ref().map(|t| t.id.clone()) {
            let sel = self.ui().global::<AppState>().get_tl_sel();
            self.open_tool_keep(&t, sel);
        }
    }

    fn open_tool_keep(&mut self, id: &str, sel: i32) {
        self.open_tool(id);
        if sel == -2 {
            self.ui().global::<AppState>().set_tl_sel(-2);
        }
    }

    fn tl_refresh_calc(&mut self) {
        let Some(Tool { body: ToolBody::Calc(c), .. }) = &self.tool else {
            return;
        };
        let bad = self.tl_values.iter().any(|v| v.is_nan());
        let res = Tool::compute(c, &self.tl_values);
        let outs: Vec<ToolOutVM> = c
            .outputs
            .iter()
            .zip(res)
            .map(|(o, r)| {
                let (value, ok) = match r {
                    _ if bad => ("—".to_string(), false),
                    Ok(v) if v.is_finite() => (format_number(v, o.digits), true),
                    Ok(_) => ("non definito".to_string(), false),
                    Err(e) => (e, false),
                };
                ToolOutVM { label: ss(&o.label), unit: ss(&o.unit), value: ss(value), ok }
            })
            .collect();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if let Some(m) = sync_model(st.get_tl_outputs(), outs) {
            st.set_tl_outputs(m);
        }
    }

    pub(super) fn tool_title(&self) -> Option<String> {
        self.tool.as_ref().map(|t| t.title.clone())
    }

    // ---------- Gestisci contenuti › Strumenti ----------

    /// Archivio di un altro esame installato.
    fn other_store(&self, id: &str) -> Option<Store> {
        if id == self.exams.active {
            return None;
        }
        Store::open_in(&self.store.base, prep_core::exams::dir(&self.store.base, id)).ok()
    }

    /// Strumenti nei file di un archivio (`content/tools/*.json`).
    fn store_tools(store: &Store) -> Vec<Tool> {
        let dir = store.root.join("mirror").join("content").join("tools");
        let mut out: Vec<Tool> = vfs::list_dir(&dir)
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .filter_map(|p| vfs::read_to_string(&p).ok())
            .filter_map(|t| serde_json::from_str::<Tool>(&t).ok())
            .collect();
        out.sort_by(|a, b| a.title.cmp(&b.title));
        out
    }

    fn item(t: &Tool, pkg: &str, pkg_name: &str, builtin: bool, info: String) -> ToolItemVM {
        ToolItemVM {
            id: ss(&t.id),
            title: ss(&t.title),
            info: ss(info),
            kind: kind_n(t),
            pkg: ss(pkg),
            pkg_name: ss(pkg_name),
            builtin,
            ..Default::default()
        }
    }

    fn mark_ends(v: &mut [ToolItemVM]) {
        let n = v.len();
        for i in 0..n {
            v[i].first = i == 0 || v[i].head || v[i - 1].pkg != v[i].pkg;
            v[i].last = i + 1 == n || v[i + 1].pkg != v[i].pkg;
        }
    }

    /// Strumenti del pacchetto attivo: i suoi file e quelli inclusi che una materia elenca.
    pub(super) fn refresh_ct_tools(&mut self) {
        let syl = self.content.syllabus().clone();
        let mut items: Vec<ToolItemVM> = self
            .content
            .tools
            .iter()
            .map(|t| {
                let used: Vec<&str> = syl
                    .subjects
                    .iter()
                    .filter(|s| s.tools.contains(&t.id))
                    .map(|s| if s.short.is_empty() { s.name.as_str() } else { s.short.as_str() })
                    .collect();
                let info = format!(
                    "{} · {}",
                    t.kind_label(),
                    if used.is_empty() { "tutte le materie".to_string() } else { used.join(", ") }
                );
                Self::item(t, "", "", false, info)
            })
            .collect();
        let mut listed: Vec<String> = Vec::new();
        for s in &syl.subjects {
            for id in &s.tools {
                if !self.content.tools.iter().any(|t| &t.id == id) && builtin_head(id).is_some() && !listed.contains(id)
                {
                    listed.push(id.clone());
                }
            }
        }
        for id in listed {
            let (title, _, kind) = builtin_head(&id).expect("controllato");
            items.push(ToolItemVM {
                id: ss(&id),
                title: ss(title),
                info: ss("inclusa nell'app"),
                kind,
                builtin: true,
                ..Default::default()
            });
        }
        Self::mark_ends(&mut items);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if let Some(m) = sync_model(st.get_ct_tools(), items) {
            st.set_ct_tools(m);
        }
    }

    /// «Aggiungi strumento»: quelli degli altri pacchetti e quelli inclusi nell'app.
    pub(super) fn ct_tools_add_open(&mut self) {
        let mut src: Vec<ToolItemVM> = Vec::new();
        let here: Vec<String> = self.content.tools.iter().map(|t| t.id.clone()).collect();
        for (id, _) in [(TOOL_PERIODIC_TABLE, ())] {
            if !here.iter().any(|h| h == id) {
                if let Some(t) = self.builtin_tool(id) {
                    let mut it = Self::item(&t, "", "Inclusi nell'app", true, t.subtitle.clone());
                    it.head = src.is_empty();
                    src.push(it);
                }
            }
        }
        for e in self.exams.exams.clone() {
            let Some(store) = self.other_store(&e.id) else { continue };
            let tools = Self::store_tools(&store);
            for (i, t) in tools.iter().enumerate() {
                let info = format!(
                    "{} · {}",
                    t.kind_label(),
                    if here.contains(&t.id) { "sostituisce quello di qui" } else { "nuovo qui" }
                );
                let mut it = Self::item(t, &e.id, &e.name, false, info);
                it.head = i == 0;
                src.push(it);
            }
        }
        Self::mark_ends(&mut src);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_ct_tool_src(model(src));
        st.set_ct_sheet(1);
    }

    /// Copia nel pacchetto attivo lo strumento `id` del pacchetto `pkg` ("" = incluso nell'app).
    pub(super) fn ct_tool_add(&mut self, pkg: &str, id: &str) {
        let tool = if pkg.is_empty() {
            self.builtin_tool(id)
        } else {
            self.other_store(pkg).and_then(|s| Self::store_tools(&s).into_iter().find(|t| t.id == id))
        };
        let Some(t) = tool else {
            self.toast("Strumento non trovato");
            return;
        };
        self.ui().global::<AppState>().set_ct_sheet(0);
        let bytes = serde_json::to_vec_pretty(&t).unwrap_or_default();
        match contents::add_file(&self.store, &format!("{}.json", t.id), &bytes, "") {
            Ok(_) => self.contents_changed(format!("Strumento «{}» aggiunto a questo pacchetto.", t.title)),
            Err(e) => self.toast(format!("Strumento non aggiunto: {e}")),
        }
    }

    /// Azioni su uno strumento del pacchetto (foglio dal basso).
    pub(super) fn ct_tool_menu(&mut self, id: &str) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let m = st.get_ct_tools();
        let Some(it) = (0..m.row_count()).filter_map(|i| m.row_data(i)).find(|t| t.id == id) else {
            return;
        };
        st.set_ct_tool(it);
        self.ct_pending = None;
        st.set_ct_dests(model(self.dest_exams(false)));
        st.set_ct_sheet(2);
    }

    /// Esami in cui copiare uno strumento (`with_active`: anche quello in uso, per primo).
    fn dest_exams(&self, with_active: bool) -> Vec<ExamVM> {
        let mut v: Vec<ExamVM> = self
            .exams
            .exams
            .iter()
            .filter(|e| with_active || e.id != self.exams.active)
            .map(|e| ExamVM {
                id: ss(&e.id),
                name: ss(&e.name),
                initials: ss(e.name.chars().take(2).collect::<String>()),
                active: e.id == self.exams.active,
                info: ss(if e.id == self.exams.active { "In uso" } else { "" }),
            })
            .collect();
        v.sort_by_key(|e| !e.active);
        v
    }

    /// Il file dello strumento scelto nel menu (incluso nell'app: costruito qui).
    fn ct_tool_bytes(&mut self) -> Option<(String, Vec<u8>)> {
        let id = self.ui().global::<AppState>().get_ct_tool().id.to_string();
        let t = self.find_tool(&id)?;
        Some((t.title.clone(), serde_json::to_vec_pretty(&t).ok()?))
    }

    /// «Condividi»: il solo file dello strumento, da mandare a chi vuoi.
    pub(super) fn ct_tool_share(&mut self) {
        let Some((_, bytes)) = self.ct_tool_bytes() else { return };
        let id = self.ui().global::<AppState>().get_ct_tool().id.to_string();
        self.ui().global::<AppState>().set_ct_sheet(0);
        self.deliver_file(&format!("strumento-{id}.json"), &bytes);
    }

    /// Copia lo strumento scelto (o quello ricevuto) nel pacchetto `dest`.
    pub(super) fn ct_tool_copy(&mut self, dest: &str) {
        let (title, bytes) = match self.ct_pending.take() {
            Some(b) => (String::new(), b),
            None => match self.ct_tool_bytes() {
                Some(x) => x,
                None => return,
            },
        };
        self.ui().global::<AppState>().set_ct_sheet(0);
        let name = self.exams.get(dest).map(|e| e.name.clone()).unwrap_or_default();
        let res = if dest == self.exams.active {
            contents::add_file(&self.store, "strumento.json", &bytes, "")
        } else {
            match self.other_store(dest) {
                Some(s) => contents::add_file(&s, "strumento.json", &bytes, ""),
                None => Err("pacchetto non trovato".into()),
            }
        };
        match res {
            Ok(a) if dest == self.exams.active => self.contents_changed(format!("Aggiunto: {}.", a.summary)),
            Ok(a) => {
                let what = if title.is_empty() { a.summary } else { format!("«{title}»") };
                self.toast(format!("Strumento {what} copiato in «{name}»"));
                self.refresh_ct_tools();
            }
            Err(e) => self.toast(format!("Strumento non copiato: {e}")),
        }
    }

    pub(super) fn ct_tool_remove(&mut self) {
        let it = self.ui().global::<AppState>().get_ct_tool();
        self.ui().global::<AppState>().set_ct_sheet(0);
        if it.builtin {
            self.toast("È inclusa nell'app: per toglierla da una materia modifica la definizione dell'esame");
            return;
        }
        self.contents_remove_ask(&format!("content/tools/{}.json", it.id));
    }

    /// Uno strumento ricevuto (file singolo): si sceglie in quale pacchetto metterlo.
    pub(super) fn receive_tool(&mut self, bytes: Vec<u8>) -> bool {
        if contents::classify("strumento.json", &bytes) != Ok(Kind::Tool) {
            return false;
        }
        let Ok(t) = serde_json::from_slice::<Tool>(&bytes) else { return false };
        if self.exams.exams.len() <= 1 {
            return false;
        }
        self.ct_pending = Some(bytes);
        if self.ui().global::<AppState>().get_page() != P_CONTENTS {
            self.open_contents();
        }
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_ct_tool(Self::item(&t, "", "", false, t.kind_label().to_string()));
        st.set_ct_dests(model(self.dest_exams(true)));
        st.set_ct_sheet(3);
        true
    }
}
