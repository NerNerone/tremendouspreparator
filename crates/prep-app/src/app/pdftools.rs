//! Pagina «Strumenti per l'AI»: dividere un PDF troppo grande per i limiti di un'AI e
//! estrarne il testo in Markdown (con i segnaposto di pagina) da far correggere.

use super::*;

/// Cosa fare con il prossimo PDF scelto.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum PdfTool {
    Split,
    Text,
    /// testo con l'OCR: PDF scansionati o foto
    Ocr,
}

impl Ctrl {
    /// Anteprima del testo estratto (i primi caratteri) e il testo intero da copiare.
    pub(super) fn show_md(&self, name: &str, md: &str) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let preview: String = md.chars().take(1800).collect();
        st.set_pdf_md_name(ss(name));
        st.set_pdf_md_info(ss(format!(
            "{} caratteri · {} pagine segnate",
            md.chars().count(),
            md.matches("<!-- p.").count()
        )));
        st.set_pdf_md_preview(ss(if preview.len() < md.len() { format!("{preview}\n…") } else { preview }));
        st.set_pdf_md(ss(md));
    }

    pub(super) fn open_pdf_tools(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_pdf_status(ss(""));
        let here = st.get_page();
        if here != P_PDFTOOLS {
            self.pdf_return = here;
        }
        self.set_page(P_PDFTOOLS);
    }

    /// «Scegli PDF»: selettore di sistema; il file arriva in import/ come `file-…--<nome>`.
    pub(super) fn pdf_pick(&mut self, tool: PdfTool) {
        self.pdf_pick = Some(tool);
        self.pick_any_file();
    }

    /// Selettore di un file qualsiasi (Android, browser, computer): finisce in import/.
    pub(super) fn pick_any_file(&mut self) {
        #[cfg(target_os = "android")]
        if let Err(e) = crate::android_pick_file() {
            self.toast(format!("Selettore file non disponibile: {e}"));
        }
        #[cfg(target_arch = "wasm32")]
        crate::web::pick_named_file(self.store.base.join("import"));
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        {
            let import = self.store.base.join("import");
            spawn_background(move || {
                if let Some(p) = crate::desktop_files::pick_file() {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    if let Ok(bytes) = std::fs::read(&p) {
                        let _ = vfs::create_dir_all(&import);
                        let dst = import.join(format!("file-{}--{name}", chrono::Utc::now().timestamp_millis()));
                        let _ = vfs::write(&dst, &bytes);
                    }
                }
            });
        }
    }

    /// PDF scelto: si lavora un attimo dopo, così la pagina mostra subito «Lavoro…».
    pub(super) fn pdf_file(&mut self, path: PathBuf) {
        let Some(tool) = self.pdf_pick.take() else { return };
        let bytes = vfs::read(&path).unwrap_or_default();
        let _ = vfs::remove_file(&path);
        let fname = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let name = fname.split_once("--").map(|(_, n)| n.to_string()).unwrap_or(fname);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if tool == PdfTool::Ocr {
            self.ocr_start(name, bytes);
            return;
        }
        st.set_pdf_busy(true);
        st.set_pdf_status(ss(format!("Lavoro su «{name}»…")));
        slint::Timer::single_shot(Duration::from_millis(60), move || {
            with_ctrl(|c| {
                let res = match tool {
                    PdfTool::Split => c.pdf_split(&name, &bytes),
                    PdfTool::Text => c.pdf_text(&name, &bytes),
                    PdfTool::Ocr => unreachable!(),
                };
                let ui = c.ui();
                let st = ui.global::<AppState>();
                st.set_pdf_busy(false);
                match res {
                    Ok(msg) => st.set_pdf_status(ss(msg)),
                    Err(e) => st.set_pdf_status(ss(format!("«{name}»: {e}"))),
                }
            })
        });
    }

    fn pdf_split(&mut self, name: &str, bytes: &[u8]) -> Result<String, String> {
        let mb = self.ui().global::<AppState>().get_pdf_max_mb().max(0.5) as f64;
        let max = (mb * 1_000_000.0) as usize;
        let sel = self.pdf_pages(bytes)?;
        let parts = prep_core::pdf::split(bytes, &sel, max)?;
        let stem = name.trim_end_matches(".pdf").trim_end_matches(".PDF");
        let n = parts.len();
        let whole = sel.len() as u32 == prep_core::pdf::pages(bytes)?;
        if n == 1 && whole {
            return Ok(format!(
                "«{name}» ({:.1} MB) sta già sotto i {mb} MB: non serve dividerlo.",
                bytes.len() as f64 / 1e6
            ));
        }
        let files: Vec<(String, Vec<u8>)> = parts
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let name = if n == 1 {
                    format!("{stem} (pagine {}).pdf", p.label())
                } else {
                    format!("{stem} - parte {} di {n} (pagine {}).pdf", i + 1, p.label())
                };
                (name, p.bytes)
            })
            .collect();
        let biggest = files.iter().map(|(_, b)| b.len()).max().unwrap_or(0) as f64 / 1e6;
        let over = files.iter().filter(|(_, b)| b.len() > max).count();
        let where_ = self.save_outputs(&files, "application/pdf")?;
        let mut msg = if n == 1 {
            format!("Pagine scelte in un solo PDF ({biggest:.1} MB){where_}")
        } else {
            format!("Diviso in {n} parti (la più grande {biggest:.1} MB){where_}")
        };
        if over > 0 {
            msg.push_str(&format!(
                " Attenzione: {over} parti superano comunque il limite perché contengono una sola pagina molto pesante."
            ));
        }
        Ok(msg)
    }

    fn pdf_text(&mut self, name: &str, bytes: &[u8]) -> Result<String, String> {
        // non PDF: Word, PowerPoint, OpenDocument, EPUB, HTML, testo (vedi prep_core::docs)
        if !bytes.starts_with(b"%PDF") {
            return self.doc_text(name, bytes);
        }
        let sel = self.pdf_pages(bytes)?;
        let (md, empty) = prep_core::pdf::to_markdown(bytes, &sel, 0)?;
        let total = sel.len() as u32;
        if empty.len() as u32 == total {
            return Err(
                "nessuna pagina ha testo: è una scansione. Usa «Testo con l'OCR» o, se l'OCR non può farcela, passa il PDF diviso direttamente all'AI.".into(),
            );
        }
        let stem = name.trim_end_matches(".pdf").trim_end_matches(".PDF");
        self.show_md(&format!("{stem}.md"), &md);
        let where_ = self.save_outputs(&[(format!("{stem}.md"), md.into_bytes())], "text/markdown")?;
        let mut msg = format!(
            "Testo grezzo di {} pagine su {total} estratto in «{stem}.md»{where_} Prima di usarlo fallo revisionare da un'AI con il prompt 8.5 della guida.",
            total - empty.len() as u32
        );
        if !empty.is_empty() {
            let list: Vec<String> = empty.iter().take(12).map(|p| p.to_string()).collect();
            msg.push_str(&format!(
                " {} pagine sono scansioni senza testo (p. {}{}): per quelle usa «Testo con l'OCR».",
                empty.len(),
                list.join(", "),
                if empty.len() > 12 { "…" } else { "" }
            ));
        }
        Ok(msg)
    }

    /// OCR in un thread a parte: modelli (la prima volta), poi pagina per pagina con
    /// l'avanzamento nella pagina e «Ferma». Le pagine che hanno già testo lo usano così com'è.
    #[cfg(not(target_arch = "wasm32"))]
    fn ocr_start(&mut self, name: String, bytes: Vec<u8>) {
        use std::sync::atomic::Ordering;
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_pdf_busy(true);
        st.set_pdf_ocr_running(true);
        st.set_pdf_status(ss(format!("Preparo l'OCR di «{name}»…")));
        let spec = st.get_pdf_pages().to_string();
        let dir = crate::ocr::models_dir(&self.store.base);
        let stop = self.ocr_stop.clone();
        stop.store(false, Ordering::SeqCst);
        let status = |msg: String| {
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| c.ui().global::<AppState>().set_pdf_status(ss(msg)));
            });
        };
        spawn_background(move || {
            let res = (|| -> Result<(String, String), String> {
                if !crate::ocr::ready(&dir) {
                    crate::ocr::download(&dir, &status)?;
                }
                status("Carico i modelli OCR…".into());
                let engine = crate::ocr::Engine::load(&dir)?;
                // PDF riconosciuto dal contenuto, non dal nome del file
                if !bytes.starts_with(b"%PDF") {
                    status(format!("Leggo «{name}»…"));
                    let text = engine.read_image(&bytes)?;
                    return Ok((text, format!("Testo letto dall'immagine «{name}»")));
                }
                let doc = prep_core::pdf::ScanDoc::open(&bytes)?;
                let sel = prep_core::pdf::parse_pages(&spec, doc.page_count())?;
                let (mut md, mut ocr, mut text, mut bad) = (String::new(), 0, 0, Vec::new());
                let mut done = 0;
                for (i, &p) in sel.iter().enumerate() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    status(format!("OCR: pagina {p} ({} di {})…", i + 1, sel.len()));
                    md.push_str(&format!("<!-- p. {p} -->\n"));
                    match doc.page(p) {
                        prep_core::pdf::PageContent::Text(t) => {
                            text += 1;
                            md.push_str(&t);
                        }
                        prep_core::pdf::PageContent::Scan(s) => match engine.read_scan(s) {
                            Ok(t) => {
                                ocr += 1;
                                md.push_str(&t);
                            }
                            Err(_) => {
                                bad.push(p);
                                md.push_str("[pagina non letta]");
                            }
                        },
                        prep_core::pdf::PageContent::Unreadable => {
                            bad.push(p);
                            md.push_str("[pagina non letta: immagine in un formato non supportato]");
                        }
                    }
                    md.push_str("\n\n");
                    done += 1;
                }
                let mut msg = format!("{ocr} pagine lette con l'OCR");
                if text > 0 {
                    msg.push_str(&format!(", {text} con il testo già presente"));
                }
                if done < sel.len() {
                    msg.push_str(&format!(" (fermato a {done} pagine su {})", sel.len()));
                }
                if !bad.is_empty() {
                    let list: Vec<String> = bad.iter().take(12).map(|p| p.to_string()).collect();
                    msg.push_str(&format!(
                        "; {} non lette (p. {}{}): passa quelle pagine direttamente all'AI",
                        bad.len(),
                        list.join(", "),
                        if bad.len() > 12 { "…" } else { "" }
                    ));
                }
                Ok((md, msg))
            })();
            let _ = slint::invoke_from_event_loop(move || {
                with_ctrl(|c| {
                    let res = res.and_then(|(md, msg)| {
                        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name).to_string();
                        c.show_md(&format!("{stem}.md"), &md);
                        let where_ = c.save_outputs(&[(format!("{stem}.md"), md.into_bytes())], "text/markdown")?;
                        Ok(format!(
                            "{msg}. Testo in «{stem}.md»{where_} L'OCR sbaglia lettere accentate, formule e simboli: prima di usarlo fallo revisionare da un'AI con il prompt 8.5 della guida."
                        ))
                    });
                    let ui = c.ui();
                    let st = ui.global::<AppState>();
                    st.set_pdf_busy(false);
                    st.set_pdf_ocr_running(false);
                    match res {
                        Ok(m) => st.set_pdf_status(ss(m)),
                        Err(e) => st.set_pdf_status(ss(format!("OCR non riuscito: {e}"))),
                    }
                })
            });
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn ocr_start(&mut self, _name: String, _bytes: Vec<u8>) {
        self.toast("L'OCR c'è nell'app per Android e per computer");
    }

    /// Testo di un documento non PDF: qui la struttura c'è nel file, quindi titoli, elenchi e
    /// tabelle escono già in Markdown.
    fn doc_text(&mut self, name: &str, bytes: &[u8]) -> Result<String, String> {
        let spec = self.ui().global::<AppState>().get_pdf_pages().to_string();
        let sel = if spec.trim().is_empty() { Vec::new() } else { prep_core::pdf::parse_pages(&spec, 100_000)? };
        let d = prep_core::docs::to_markdown(bytes, &sel)?;
        if d.md.trim().is_empty() {
            return Err("nessun testo trovato nel documento".into());
        }
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name).to_string();
        self.show_md(&format!("{stem}.md"), &d.md);
        let where_ = self.save_outputs(&[(format!("{stem}.md"), d.md.into_bytes())], "text/markdown")?;
        let what = match (d.kind, d.pages) {
            ("PowerPoint" | "Impress (ODP)", n) if n > 0 => format!("{} slide", n),
            (_, n) if n > 1 => format!("{n} pagine"),
            _ => String::new(),
        };
        Ok(format!(
            "Testo di «{name}» ({}{}) in «{stem}.md»{where_} Titoli, elenchi e tabelle sono già in Markdown; le figure no. Prima di usarlo fallo rivedere da un'AI con il prompt 8.5 della guida.",
            d.kind,
            if what.is_empty() { String::new() } else { format!(", {what}") }
        ))
    }

    /// Pagine scelte nel campo «Pagine» (vuoto = tutte).
    fn pdf_pages(&self, bytes: &[u8]) -> Result<Vec<u32>, String> {
        let spec = self.ui().global::<AppState>().get_pdf_pages().to_string();
        prep_core::pdf::parse_pages(&spec, prep_core::pdf::pages(bytes)?)
    }

    /// Salva i file prodotti: cartella scelta (Android), download (browser), Download (computer).
    /// Restituisce dove sono finiti, da aggiungere al messaggio.
    #[cfg(target_os = "android")]
    fn save_outputs(&mut self, files: &[(String, Vec<u8>)], mime: &str) -> Result<String, String> {
        let dir = self.store.base.join("export");
        let _ = vfs::create_dir_all(&dir);
        let mut list = Vec::new();
        for (i, (name, bytes)) in files.iter().enumerate() {
            let p = dir.join(format!("out-{i}"));
            vfs::write(&p, bytes).map_err(|e| e.to_string())?;
            list.push((p, name.clone(), mime));
        }
        crate::android_save_files(&list)?;
        Ok(": scegli la cartella dove salvarli.".into())
    }

    #[cfg(target_arch = "wasm32")]
    fn save_outputs(&mut self, files: &[(String, Vec<u8>)], _mime: &str) -> Result<String, String> {
        for (name, bytes) in files {
            crate::web::download_file(name, bytes);
        }
        Ok(", scaricati dal browser.".into())
    }

    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    fn save_outputs(&mut self, files: &[(String, Vec<u8>)], _mime: &str) -> Result<String, String> {
        let mut dir = None;
        for (name, bytes) in files {
            let p = crate::desktop_files::save_to_downloads(name, bytes)?;
            dir = p.parent().map(|d| d.display().to_string());
        }
        Ok(format!(", salvati in {}.", dir.unwrap_or_default()))
    }
}
