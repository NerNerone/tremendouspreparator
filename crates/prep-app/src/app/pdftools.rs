//! Pagina «Strumenti per l'AI»: dividere un PDF troppo grande per i limiti di un'AI e
//! estrarne il testo in Markdown (con i segnaposto di pagina) da far correggere.

use super::*;

/// Cosa fare con il prossimo PDF scelto.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum PdfTool {
    Split,
    Text,
}

impl Ctrl {
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
        st.set_pdf_busy(true);
        st.set_pdf_status(ss(format!("Lavoro su «{name}»…")));
        slint::Timer::single_shot(Duration::from_millis(60), move || {
            with_ctrl(|c| {
                let res = match tool {
                    PdfTool::Split => c.pdf_split(&name, &bytes),
                    PdfTool::Text => c.pdf_text(&name, &bytes),
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
        let sel = self.pdf_pages(bytes)?;
        let (md, empty) = prep_core::pdf::to_markdown(bytes, &sel, 0)?;
        let total = sel.len() as u32;
        if empty.len() as u32 == total {
            return Err(
                "nessuna pagina ha testo: è una scansione. Usa l'OCR (vedi la guida) o, se l'OCR non può farcela, passa il PDF diviso direttamente all'AI.".into(),
            );
        }
        let stem = name.trim_end_matches(".pdf").trim_end_matches(".PDF");
        let where_ = self.save_outputs(&[(format!("{stem}.md"), md.into_bytes())], "text/markdown")?;
        let mut msg = format!(
            "Testo grezzo di {} pagine su {total} estratto in «{stem}.md»{where_} Prima di usarlo fallo revisionare da un'AI con il prompt 8.5 della guida.",
            total - empty.len() as u32
        );
        if !empty.is_empty() {
            let list: Vec<String> = empty.iter().take(12).map(|p| p.to_string()).collect();
            msg.push_str(&format!(
                " {} pagine sono scansioni senza testo (p. {}{}): falle passare dall'OCR.",
                empty.len(),
                list.join(", "),
                if empty.len() > 12 { "…" } else { "" }
            ));
        }
        Ok(msg)
    }

    /// Pagine scelte nel campo «Pagine» (vuoto = tutte).
    fn pdf_pages(&self, bytes: &[u8]) -> Result<Vec<u32>, String> {
        let spec = self.ui().global::<AppState>().get_pdf_pages().to_string();
        prep_core::pdf::parse_pages(&spec, prep_core::pdf::pages(bytes)?)
    }

    /// Salva i file prodotti: cartella scelta (Android), download (browser), Download (computer).
    /// Restituisce dove sono finiti, da aggiungere al messaggio.
    fn save_outputs(&mut self, files: &[(String, Vec<u8>)], mime: &str) -> Result<String, String> {
        #[cfg(target_os = "android")]
        {
            let dir = self.store.base.join("export");
            let _ = vfs::create_dir_all(&dir);
            let mut list = Vec::new();
            for (i, (name, bytes)) in files.iter().enumerate() {
                let p = dir.join(format!("out-{i}"));
                vfs::write(&p, bytes).map_err(|e| e.to_string())?;
                list.push((p, name.clone(), mime));
            }
            crate::android_save_files(&list)?;
            return Ok(": scegli la cartella dove salvarli.".into());
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = mime;
            for (name, bytes) in files {
                crate::web::download_file(name, bytes);
            }
            return Ok(", scaricati dal browser.".into());
        }
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        {
            let _ = mime;
            let mut dir = None;
            for (name, bytes) in files {
                let p = crate::desktop_files::save_to_downloads(name, bytes)?;
                dir = p.parent().map(|d| d.display().to_string());
            }
            Ok(format!(", salvati in {}.", dir.unwrap_or_default()))
        }
    }
}
