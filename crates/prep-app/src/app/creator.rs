//! «Crea pacchetto»: l'esame e i suoi file passo passo con un'AI. Per ogni file l'app prepara
//! il prompt (con argomenti, numero di domande e id già pronti); l'utente lo dà all'AI con il
//! proprio materiale, incolla qui la risposta e l'app la legge e la salva nel file giusto.

use super::*;
use crate::{CrFigVM, CrFileVM, CrUnitVM};
use prep_core::contents::{self, Kind};
use prep_core::creator::{self, FileKind};

/// Il quinto tipo di file del creatore: una figura SVG di un capitolo.
const K_FIGURE: i32 = 4;

fn kind_of(k: i32) -> FileKind {
    match k {
        0 => FileKind::Syllabus,
        1 => FileKind::Chapter,
        2 => FileKind::Questions,
        _ => FileKind::Cards,
    }
}

impl Ctrl {
    pub(super) fn open_creator(&mut self) {
        self.ui().global::<AppState>().set_cr_open(false);
        self.refresh_creator();
        self.set_page(P_CREATOR);
    }

    /// Nuovo esame vuoto con questo nome, subito attivo.
    pub(super) fn creator_new_exam(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.toast("Scrivi il nome dell'esame");
            return;
        }
        let base = prep_core::exams::slug(name);
        let mut id = base.clone();
        let mut n = 2;
        while self.exams.get(&id).is_some() {
            id = format!("{base}-{n}");
            n += 1;
        }
        self.exams.add(&id, name);
        let _ = self.exams.save(&self.store.base);
        let _ = vfs::create_dir_all(&prep_core::exams::dir(&self.store.base, &id).join("mirror"));
        self.open_exam(&id);
        self.open_creator();
        self.toast(format!("Esame «{name}» creato: ora la sua definizione"));
    }

    pub(super) fn refresh_creator(&mut self) {
        let has_exam = self.exams.get(&self.exams.active).is_some();
        let files: Vec<CrFileVM> = if has_exam {
            contents::list(&self.store)
                .into_iter()
                .filter_map(|e| {
                    let kind = match e.kind {
                        Kind::Exam => 0,
                        Kind::Chapter => 1,
                        Kind::Questions => 2,
                        Kind::Cards => 3,
                        _ => return None,
                    };
                    Some(CrFileVM { path: ss(&e.path), kind, title: ss(&e.name), info: ss(&e.info) })
                })
                .collect()
        } else {
            Vec::new()
        };
        let syl = self.content.syllabus();
        let mut units = vec![CrUnitVM { id: ss(""), label: ss("Tutti gli argomenti") }];
        for s in &syl.subjects {
            for u in &s.units {
                units.push(CrUnitVM {
                    id: ss(&u.id),
                    label: ss(format!("{} · {}", if s.short.is_empty() { &s.name } else { &s.short }, u.name)),
                });
            }
        }
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_cr_has_exam(has_exam);
        st.set_cr_exam(ss(self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_default()));
        st.set_cr_has_syllabus(!syl.subjects.is_empty());
        st.set_cr_files(model(files));
        st.set_cr_units(model(units));
        st.set_cr_bar(ss(creator::BAR));
    }

    /// Apre l'editor di un file: nuovo (`path` vuoto) o esistente.
    pub(super) fn creator_open_file(&mut self, kind: i32, path: &str) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if kind != 0 && self.content.syllabus().subjects.is_empty() {
            self.toast("Prima la definizione dell'esame: dà gli argomenti a tutti gli altri file");
            return;
        }
        let existing = if path.is_empty() {
            String::new()
        } else {
            self.store.read_mirror(path).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default()
        };
        let name = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let title = match (kind, path.is_empty()) {
            (0, _) => "Definizione dell'esame".to_string(),
            (1, true) => "Nuovo capitolo".to_string(),
            (2, true) => "Nuovo file di quiz".to_string(),
            (3, true) => "Nuovo file di flashcard".to_string(),
            (1, false) => format!("Capitolo «{name}»"),
            (2, false) => format!("Quiz «{name}»"),
            _ => format!("Flashcard «{name}»"),
        };
        st.set_cr_kind(kind);
        st.set_cr_path(ss(path));
        st.set_cr_title(ss(title));
        st.set_cr_name(ss(if kind == 0 { String::new() } else { name }));
        st.set_cr_text(ss(existing));
        st.set_cr_status(ss(""));
        st.set_cr_count(if kind == 3 { 20 } else { 30 });
        st.set_cr_notes(false);
        st.set_cr_unit(ss(""));
        st.set_cr_parent(ss(""));
        st.set_cr_open(true);
        self.refresh_chapter_figs();
        self.creator_prompt();
    }

    /// Figure del capitolo aperto nell'editor.
    fn refresh_chapter_figs(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let path = st.get_cr_path().to_string();
        let figs: Vec<CrFigVM> = if st.get_cr_kind() == 1 && !path.is_empty() {
            contents::chapter_figures(&self.store, &path)
                .into_iter()
                .map(|f| CrFigVM {
                    alt: ss(if f.alt.is_empty() { "Figura" } else { &f.alt }),
                    path: ss(&f.path),
                    present: f.present,
                })
                .collect()
        } else {
            Vec::new()
        };
        st.set_cr_figs(model(figs));
    }

    /// Apre una figura del capitolo aperto: prompt per ricomporla, SVG da incollare.
    pub(super) fn creator_open_figure(&mut self, path: &str) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let chapter = st.get_cr_path().to_string();
        let alt = contents::chapter_figures(&self.store, &chapter)
            .into_iter()
            .find(|f| f.path == path)
            .map(|f| f.alt)
            .unwrap_or_default();
        let existing =
            self.store.read_mirror(path).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
        st.set_cr_parent(ss(&chapter));
        st.set_cr_kind(K_FIGURE);
        st.set_cr_path(ss(path));
        st.set_cr_title(ss(if alt.is_empty() { "Figura".to_string() } else { alt }));
        st.set_cr_text(ss(existing));
        st.set_cr_status(ss(""));
        st.set_cr_figs(model(Vec::<CrFigVM>::new()));
        self.creator_prompt();
    }

    /// Indietro dall'editor: da una figura si torna al suo capitolo.
    pub(super) fn creator_close(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let parent = st.get_cr_parent().to_string();
        if st.get_cr_kind() == K_FIGURE && !parent.is_empty() {
            self.creator_open_file(1, &parent);
        } else {
            st.set_cr_open(false);
        }
    }

    /// Prompt per il file aperto, con le scelte attuali.
    pub(super) fn creator_prompt(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let kind = st.get_cr_kind();
        let path = st.get_cr_path().to_string();
        let name = st.get_cr_name().to_string();
        let exam_name = self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_default();
        let syl = self.content.syllabus();
        let unit = st.get_cr_unit().to_string();
        let units: Vec<String> = if unit.is_empty() { vec![] } else { vec![unit] };
        let topics = creator::topic_lines(syl, &units);
        let label = if name.trim().is_empty() { "il mio materiale".to_string() } else { name.trim().to_string() };
        let prefix = {
            let s = prep_core::exams::slug(&label);
            s.chars().take(14).collect::<String>().trim_end_matches('-').to_string()
        };
        let prompt = match kind {
            K_FIGURE => {
                let chapter = st.get_cr_parent().to_string();
                let ch_title = contents::list(&self.store)
                    .into_iter()
                    .find(|e| e.path == chapter)
                    .map(|e| e.name)
                    .unwrap_or(chapter);
                creator::prompt_figure(st.get_cr_title().as_str(), &ch_title)
            }
            0 => creator::prompt_syllabus(&self.exams.active, &exam_name),
            1 => creator::prompt_chapter(&label),
            2 if !path.is_empty() => creator::prompt_review(st.get_cr_text().as_str()),
            2 => {
                let start = creator::next_number(self.content.questions.iter().map(|q| q.id.as_str()), &prefix, 'q');
                creator::prompt_questions(
                    &topics,
                    st.get_cr_count().max(1) as u32,
                    st.get_cr_notes(),
                    &prefix,
                    start,
                    &label,
                )
            }
            _ => {
                let start = creator::next_number(self.content.cards.iter().map(|c| c.id.as_str()), &prefix, 'c');
                creator::prompt_cards(&topics, st.get_cr_count().max(1) as u32, &prefix, start, &label)
            }
        };
        st.set_cr_prompt(ss(prompt));
    }

    /// «Salva»: legge la risposta incollata e la mette nel file giusto.
    pub(super) fn creator_save(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let kind = st.get_cr_kind();
        let path = st.get_cr_path().to_string();
        let name = st.get_cr_name().trim().to_string();
        let text = st.get_cr_text().to_string();
        if kind != 0 && path.is_empty() && name.is_empty() {
            st.set_cr_status(ss("Dai un nome al file (per esempio il titolo del capitolo o dell'argomento)."));
            return;
        }
        if kind == K_FIGURE {
            let res = creator::to_svg(&text)
                .and_then(|svg| contents::add_figure(&self.store, &path, svg.as_bytes()).map(|_| svg));
            match res {
                Ok(svg) => {
                    self.contents_changed(format!("Figura salvata ({path})."));
                    let ui = self.ui();
                    let st = ui.global::<AppState>();
                    st.set_cr_text(ss(&svg));
                    st.set_cr_status(ss(
                        "Figura salvata: la vedi nel capitolo, al suo posto. Indietro per le altre figure.",
                    ));
                    self.refresh_creator();
                }
                Err(e) => self.ui().global::<AppState>().set_cr_status(ss(format!("Non salvata: {e}"))),
            }
            return;
        }
        let reply = creator::read_reply(&text);
        let body = match creator::to_file(kind_of(kind), &reply, &self.exams.active) {
            Ok(b) => b,
            Err(e) => {
                st.set_cr_status(ss(format!(
                    "Non salvato: {e}. Controlla di aver incollato tutta la risposta dell'AI."
                )));
                return;
            }
        };
        let file = match kind {
            0 => "syllabus.json".to_string(),
            _ if !path.is_empty() => {
                std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            }
            1 => format!("{}.md", prep_core::exams::slug(&name)),
            _ => format!("{}.json", prep_core::exams::slug(&name)),
        };
        let subject = {
            let unit = st.get_cr_unit().to_string();
            self.content
                .syllabus()
                .subjects
                .iter()
                .find(|s| s.units.iter().any(|u| u.id == unit))
                .map(|s| s.id.clone())
                .unwrap_or_else(|| st.get_current_subject().to_string())
        };
        match contents::add_file(&self.store, &file, body.as_bytes(), &subject) {
            Ok(a) => {
                let mut msg = format!("Salvato: {} ({}).", a.summary, a.path);
                if !a.warnings.is_empty() {
                    msg.push_str(&format!(
                        "\nDa correggere ({}): {}",
                        a.warnings.len(),
                        a.warnings.iter().take(4).cloned().collect::<Vec<_>>().join("; ")
                    ));
                }
                if !reply.doubts.is_empty() {
                    msg.push_str(&format!("\nDubbi dell'AI: {}", reply.doubts.chars().take(400).collect::<String>()));
                }
                self.contents_changed(msg.clone());
                let ui = self.ui();
                let st = ui.global::<AppState>();
                st.set_cr_path(ss(&a.path));
                // il testo salvato (con i percorsi delle figure assegnati)
                let saved = self.store.read_mirror(&a.path).map(|b| String::from_utf8_lossy(&b).to_string());
                st.set_cr_text(ss(saved.unwrap_or(body)));
                st.set_cr_status(ss(msg));
                self.refresh_creator();
                self.refresh_chapter_figs();
                self.creator_prompt();
            }
            Err(e) => st.set_cr_status(ss(format!("Non salvato: {e}"))),
        }
    }
}
