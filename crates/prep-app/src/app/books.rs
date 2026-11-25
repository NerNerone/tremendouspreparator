//! Lettore dei libri.

use super::*;

/// Capitoli della guida ai contenuti (Markdown incluso nell'app).
const GUIDE: &[(&str, &str)] = &[
    ("guida/00-con-l-ai.md", include_str!("../../assets/guida/00-con-l-ai.md")),
    ("guida/01-panoramica.md", include_str!("../../assets/guida/01-panoramica.md")),
    ("guida/02-esame.md", include_str!("../../assets/guida/02-esame.md")),
    ("guida/03-quiz.md", include_str!("../../assets/guida/03-quiz.md")),
    ("guida/04-flashcard.md", include_str!("../../assets/guida/04-flashcard.md")),
    ("guida/05-libri.md", include_str!("../../assets/guida/05-libri.md")),
    ("guida/06-dai-pdf.md", include_str!("../../assets/guida/06-dai-pdf.md")),
    ("guida/07-importare.md", include_str!("../../assets/guida/07-importare.md")),
    ("guida/08-prompt.md", include_str!("../../assets/guida/08-prompt.md")),
    ("guida/09-strumenti.md", include_str!("../../assets/guida/09-strumenti.md")),
];

const GUIDE_TITLE: &str = "Guida ai contenuti";

/// Istruzioni in testa alla guida quando la si copia per un'AI.
const LLM_HEAD: &str = "# Istruzioni per te (AI)

Sei un assistente che prepara contenuti di studio per l'app Tremendous Preparator: quiz, flashcard e libri di un esame, in file JSON e Markdown che l'app importa. Qui sotto trovi la guida completa ai formati e i prompt di lavoro.

IL MIO COMPITO PER TE: [scrivi qui cosa vuoi, per esempio: correggi il testo OCR allegato (prompt 8.5); crea content/syllabus.json dal programma allegato (8.1); scrivi 30 domande con note su ogni risposta dal capitolo allegato (8.2); crea le flashcard (8.4); verifica il file di domande allegato (8.3); ricrea la figura allegata in SVG (8.6)]

MATERIALE ALLEGATO: [elenca i file che alleghi o incolli dopo questa guida; allega anche il tuo syllabus.json se esiste già]

REGOLE OBBLIGATORIE (valgono più di qualunque altra indicazione, anche dei singoli prompt):
1. Fonte unica: usa SOLO il materiale allegato. Non aggiungere fatti, numeri, esempi, citazioni o pagine che non ci sono. Le tue conoscenze servono solo a controllare e correggere, mai ad aggiungere contenuti.
2. Niente invenzioni: se manca qualcosa che serve (un dato, un argomento, una pagina illeggibile) scrivi [MANCA: cosa] nel punto giusto o chiedimelo; non tirare a indovinare.
3. Formato esatto: rispetta alla lettera campi, nomi, tipi e struttura della guida. Non aggiungere né togliere campi. JSON valido: virgolette doppie, nessun commento, nessuna virgola finale, UTF-8.
4. Risposta: solo i file richiesti, uno per blocco di codice, con il percorso del file (es. content/questions/fis-cinematica.json) sulla riga subito prima del blocco. Nessun altro testo, tranne l'elenco finale dei dubbi.
5. Id: nel formato della guida, stabili, mai ripetuti; argomenti (topic) presi solo dal syllabus. Non cambiare mai gli id che ti do.
6. Correttezza: per ogni domanda risolvi tu il quesito PRIMA di scrivere la chiave; una sola opzione corretta; ricontrolla calcoli, unità di misura e ordini di grandezza.
7. Lingua: italiano corretto (grammatica, accenti, punteggiatura), terminologia della materia, simboli e unità corrette (m/s², 10⁻³, °C).
8. Testi da correggere: correggi senza riassumere, togliere o riordinare; mantieni i segnaposto <!-- p. N -->.
9. Blocchi: al massimo 30 domande o carte per risposta; poi fermati e chiedimi se continuare, riprendendo la numerazione.
10. Alla fine scrivi «DUBBI:» con l'elenco di chiavi incerte, passaggi illeggibili, argomenti non assegnabili e ogni [MANCA]. Se non ce ne sono, scrivi «DUBBI: nessuno».

# Guida ai formati
";

/// Tutta la guida in un solo testo da incollare in una chat con un'AI.
pub(super) fn guide_for_llm() -> String {
    let mut out = String::from(LLM_HEAD);
    for (_, text) in GUIDE.iter().filter(|(f, _)| !f.contains("00-")) {
        out.push('\n');
        out.push_str(text.trim());
        out.push('\n');
    }
    out
}

impl Ctrl {
    // ---------- libri ----------

    pub(super) fn read_book_bytes(&self, path: &str) -> Option<Vec<u8>> {
        if let Some(b) = self.store.read_mirror(path) {
            return Some(b);
        }
        if cfg!(all(not(target_os = "android"), not(target_arch = "wasm32"))) {
            // sviluppo desktop: legge dal checkout del repository
            if let Ok(b) = vfs::read(std::path::Path::new(path)) {
                return Some(b);
            }
        }
        // copia inclusa nell'app (versione offline)
        crate::bundled::BUNDLED_BOOKS.iter().find(|(p, _)| *p == path).map(|(_, b)| b.to_vec())
    }

    pub(super) fn read_book_file(&self, path: &str) -> Option<String> {
        String::from_utf8(self.read_book_bytes(path)?).ok()
    }

    pub(super) fn build_reader(&self, book_id: &str) -> Option<Reader> {
        let book = self.content.book(book_id)?;
        let mut entries = Vec::new();
        for ch in &book.chapters {
            let Some(text) = self.read_book_file(&ch.file) else {
                continue;
            };
            let blocks = Rc::new(markdown::parse(&text));
            for sec in markdown::sections(&blocks) {
                // argomenti della sezione (rimando preciso del pacchetto), altrimenti del capitolo
                let num = sec.title.split_whitespace().next().unwrap_or("");
                let topics = ch
                    .sections
                    .iter()
                    .find(|x| x.section == sec.title || (!x.section.is_empty() && x.section == num))
                    .map(|x| x.topics.clone())
                    .unwrap_or_else(|| ch.topics.clone());
                entries.push(ReaderEntry {
                    chapter: ch.title.clone(),
                    topics,
                    blocks: blocks.clone(),
                    section: sec,
                    file: ch.file.clone(),
                });
            }
        }
        if entries.is_empty() {
            return None;
        }
        let return_page = self.ui().global::<AppState>().get_page();
        Some(Reader { title: book.title.clone(), entries, pos: 0, return_page })
    }

    /// «Guida ai contenuti»: capitoli inclusi nell'app, letti nel lettore dei libri.
    pub(super) fn open_guide(&mut self) {
        let mut entries = Vec::new();
        for (file, text) in GUIDE {
            let blocks = Rc::new(markdown::parse(text));
            let chapter = blocks
                .iter()
                .find_map(|b| match b {
                    Block::Heading { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            for sec in markdown::sections(&blocks) {
                entries.push(ReaderEntry {
                    chapter: chapter.clone(),
                    topics: vec![],
                    blocks: blocks.clone(),
                    section: sec,
                    file: file.to_string(),
                });
            }
        }
        let return_page = self.ui().global::<AppState>().get_page();
        self.reader = Some(Reader { title: GUIDE_TITLE.into(), entries, pos: 0, return_page });
        // si apre sulla prima pagina (il percorso con l'AI), non sull'indice
        self.ui().global::<AppState>().set_rd_toc_visible(false);
        self.render_reader();
        self.set_page(P_READER);
    }

    pub(super) fn open_ref(&mut self, r: Option<BookRef>) {
        let Some(r) = r else { return };
        let Some(mut reader) = self.build_reader(&r.book) else {
            self.toast("Libro non disponibile");
            return;
        };
        let in_file: Vec<usize> =
            reader.entries.iter().enumerate().filter(|(_, e)| e.file == r.file).map(|(i, _)| i).collect();
        if let Some(&first) = in_file.first() {
            let secs: Vec<Section> = in_file.iter().map(|&i| reader.entries[i].section.clone()).collect();
            reader.pos = markdown::find_section(&secs, &r.section, r.page).map(|k| in_file[k]).unwrap_or(first);
        }
        self.reader = Some(reader);
        self.ui().global::<AppState>().set_rd_toc_visible(false);
        self.render_reader();
        self.set_page(P_READER);
    }

    pub(super) fn open_book(&mut self, id: &str) {
        match self.build_reader(id) {
            Some(r) => {
                self.reader = Some(r);
                self.ui().global::<AppState>().set_rd_toc_visible(true);
                self.render_reader();
                self.set_page(P_READER);
            }
            None => self.toast("Libro non disponibile"),
        }
    }

    pub(super) fn render_reader(&mut self) {
        let Some(r) = &self.reader else { return };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let e = &r.entries[r.pos];
        let title = if e.section.title == "Inizio" { e.chapter.clone() } else { e.section.title.clone() };
        st.set_rd_book(ss(&r.title));
        st.set_rd_title(ss(&title));
        st.set_rd_page_label(ss(e.section.page.map(|p| format!("p. {p}")).unwrap_or_default()));
        // Traguardo: posizione nel libro, capitolo sopra il titolo, quiz sull'argomento
        let n = r.entries.len().max(1);
        st.set_rd_frac((r.pos + 1) as f32 / n as f32);
        st.set_rd_pos_label(ss(format!(
            "sezione {} di {}{}",
            r.pos + 1,
            n,
            e.section.page.map(|p| format!(" · p. {p}")).unwrap_or_default()
        )));
        st.set_rd_eyebrow(ss(if e.chapter != title { e.chapter.to_uppercase() } else { String::new() }));
        let topic = e.topics.iter().find(|t| self.content.topic(t).is_some()).cloned().unwrap_or_default();
        let num = title.split_whitespace().next().unwrap_or("");
        st.set_rd_quiz_label(ss(if num.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            format!("Quiz su {}", num.trim_end_matches('.'))
        } else {
            "Quiz".to_string()
        }));
        st.set_rd_topic(ss(topic));
        // guida: il riquadro «copia tutto per un'AI» e gli strumenti PDF sulla prima pagina
        st.set_rd_llm_card(r.title == GUIDE_TITLE && r.pos == 0);
        let mut blocks: Vec<BlockVM> = Vec::new();
        for b in &e.blocks[e.section.start..e.section.end] {
            if let Block::Table(rows) = b {
                let (rows, widths) = prep_core::markdown::table_columns(rows);
                for (k, row) in rows.iter().enumerate() {
                    blocks.push(BlockVM {
                        kind: 9,
                        text: ss(""),
                        level: if k == 0 { 1 } else { 0 },
                        label: ss(""),
                        cells: model(row.iter().map(ss).collect()),
                        widths: model(widths.clone()),
                        img: Default::default(),
                    });
                }
                continue;
            }
            if let Block::Figure { alt, src } = b {
                // figura del libro: SVG a colori su fondo chiaro (si legge anche col tema scuro)
                let img = self.read_book_bytes(src).and_then(|svg| slint::Image::load_from_svg_data(&svg).ok());
                blocks.push(BlockVM {
                    kind: if img.is_some() { 12 } else { 7 },
                    text: ss(alt),
                    level: 0,
                    label: ss(""),
                    cells: model(vec![]),
                    widths: model(vec![]),
                    img: img.unwrap_or_default(),
                });
                continue;
            }
            if let Block::Math(tex) = b {
                let img = self
                    .read_book_bytes(&latex::math_path(tex))
                    .and_then(|svg| slint::Image::load_from_svg_data(&svg).ok());
                blocks.push(BlockVM {
                    kind: if img.is_some() { 10 } else { 11 },
                    text: ss(latex::convert(tex)),
                    level: 0,
                    label: ss(""),
                    cells: model(vec![]),
                    widths: model(vec![]),
                    img: img.unwrap_or_default(),
                });
                continue;
            }
            blocks.push({
                let (kind, text, level, label) = match b {
                    Block::Heading { level, text } => (0, text.clone(), *level as i32, String::new()),
                    Block::Para(t) => (1, t.clone(), 0, String::new()),
                    Block::Bullet { depth, text } => (2, text.clone(), *depth as i32, String::new()),
                    Block::Numbered { label, text } => (3, text.clone(), 0, label.clone()),
                    Block::Quote(t) => (4, t.clone(), 0, String::new()),
                    Block::Mono(t) => (5, t.clone(), 0, String::new()),
                    Block::Page(p) => (6, format!("p. {p}"), 0, String::new()),
                    Block::Image(a) => (7, a.clone(), 0, String::new()),
                    Block::Rule => (8, String::new(), 0, String::new()),
                    Block::Table(_) | Block::Math(_) | Block::Figure { .. } => unreachable!(),
                };
                BlockVM {
                    kind,
                    text: ss(text),
                    level,
                    label: ss(label),
                    cells: model(vec![]),
                    widths: model(vec![]),
                    img: Default::default(),
                }
            });
        }
        st.set_rd_blocks(model(blocks));
        st.set_rd_has_prev(r.pos > 0);
        st.set_rd_has_next(r.pos + 1 < r.entries.len());
        let toc: Vec<TocVM> = r
            .entries
            .iter()
            .enumerate()
            .map(|(i, x)| TocVM {
                idx: i as i32,
                title: ss(if x.section.title == "Inizio" { x.chapter.clone() } else { x.section.title.clone() }),
                level: x.section.level as i32,
                page: ss(x.section.page.map(|p| format!("p. {p}")).unwrap_or_default()),
                current: i == r.pos,
            })
            .collect();
        st.set_rd_toc(model(toc));
        st.set_rd_scroll(0.0);
    }

    pub(super) fn reader_goto(&mut self, k: usize) {
        if let Some(r) = &mut self.reader {
            if k < r.entries.len() {
                r.pos = k;
            }
        }
        self.ui().global::<AppState>().set_rd_toc_visible(false);
        self.render_reader();
    }

    pub(super) fn current_question_ref(&self) -> Option<BookRef> {
        let q = self.quiz.as_ref()?;
        let question = &q.questions[q.idx];
        self.content.resolve_ref(question.book_ref.as_ref(), &question.topic)
    }
}
