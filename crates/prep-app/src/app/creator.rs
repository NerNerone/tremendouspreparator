//! «Crea pacchetto»: l'esame e i suoi file passo passo con un'AI. Per ogni file l'app prepara
//! il prompt (con argomenti, numero di domande e id già pronti); l'utente lo dà all'AI con il
//! proprio materiale, incolla qui la risposta e l'app la legge e la salva nel file giusto.

use super::*;
use crate::{CrFileVM, CrUnitVM};
use prep_core::contents::{self, Kind};
use prep_core::creator::{self, FileKind};

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
        st.set_cr_open(true);
        self.creator_prompt();
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
                st.set_cr_text(ss(&body));
                st.set_cr_status(ss(msg));
                self.refresh_creator();
                self.creator_prompt();
            }
            Err(e) => st.set_cr_status(ss(format!("Non salvato: {e}"))),
        }
    }
}
