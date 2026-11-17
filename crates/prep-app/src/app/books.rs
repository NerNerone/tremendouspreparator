//! Lettore dei libri e tavola periodica.

use super::*;

impl Ctrl {
    // ---------- libri ----------

    pub(super) fn read_book_bytes(&self, path: &str) -> Option<Vec<u8>> {
        if let Some(b) = self.store.read_mirror(path) {
            return Some(b);
        }
        if cfg!(all(not(target_os = "android"), not(target_arch = "wasm32"))) {
            // sviluppo desktop: legge i contenuti dalla cartella del progetto
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
                entries.push(ReaderEntry {
                    chapter: ch.title.clone(),
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

    // ---------- tavola periodica ----------

    pub(super) fn open_periodic(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if st.get_pt_elements().row_count() == 0 {
            let els = periodic::load();
            st.set_pt_elements(model(els.iter().map(periodic::cell).collect()));
            self.elements = els;
        }
        self.set_page(P_PERIODIC);
    }

    pub(super) fn pt_select(&mut self, z: i32) {
        let Some(e) = self.elements.iter().find(|e| e.z == z) else {
            return;
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_pt_detail(periodic::detail(e));
        st.set_pt_has_sel(true);
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
        st.set_rd_title(ss(title));
        st.set_rd_page_label(ss(e.section.page.map(|p| format!("p. {p}")).unwrap_or_default()));
        let mut blocks: Vec<BlockVM> = Vec::new();
        for b in &e.blocks[e.section.start..e.section.end] {
            if let Block::Table(rows) = b {
                for (k, row) in rows.iter().enumerate() {
                    blocks.push(BlockVM {
                        kind: 9,
                        text: ss(""),
                        level: if k == 0 { 1 } else { 0 },
                        label: ss(""),
                        cells: model(row.iter().map(ss).collect()),
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
                BlockVM { kind, text: ss(text), level, label: ss(label), cells: model(vec![]), img: Default::default() }
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
