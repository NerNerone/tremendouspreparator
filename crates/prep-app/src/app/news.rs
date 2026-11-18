//! Novità: dopo un aggiornamento, all'avvio, una finestra con i punti di `assets/novita.md`
//! non ancora visti (riconosciuti dall'impronta del testo). Alla prima installazione si
//! segnano come visti senza mostrarli; da Opzioni si rivedono tutti.

use super::*;

const NEWS: &str = include_str!("../../assets/novita.md");

/// Sezioni (data, punti) del file delle novità, dalla più recente.
fn sections() -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in NEWS.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            out.push((h.trim().to_string(), Vec::new()));
        } else if let (Some(item), Some(last)) = (line.strip_prefix("- "), out.last_mut()) {
            last.1.push(item.trim().to_string());
        } else if let (true, Some(last)) = (line.starts_with("  ") && !line.trim().is_empty(), out.last_mut()) {
            // continuazione del punto precedente
            if let Some(p) = last.1.last_mut() {
                p.push(' ');
                p.push_str(line.trim());
            }
        }
    }
    out
}

/// Impronta breve di un punto.
fn key(item: &str) -> String {
    prep_core::p2p::sha256_hex(item.as_bytes())[..12].to_string()
}

impl Ctrl {
    /// All'avvio: mostra le novità non ancora viste (non alla prima installazione).
    pub(super) fn news_check(&mut self) {
        let mut s = self.store.settings();
        let all = sections();
        let keys: Vec<String> = all.iter().flat_map(|(_, v)| v.iter().map(|i| key(i))).collect();
        let show: Vec<(String, Vec<String>)> = if !s.news_init {
            // la prima installazione non ha novità; chi aggiorna da una versione senza questa
            // finestra vede l'ultima sezione
            if s.legal_accepted == 0 {
                vec![]
            } else {
                all.into_iter().take(1).collect()
            }
        } else {
            all.into_iter()
                .map(|(d, v)| (d, v.into_iter().filter(|i| !s.news_seen.contains(&key(i))).collect::<Vec<_>>()))
                .filter(|(_, v)| !v.is_empty())
                .collect()
        };
        if !s.news_init || keys != s.news_seen {
            s.news_init = true;
            s.news_seen = keys;
            let _ = self.store.save_settings(&s);
        }
        if !show.is_empty() {
            self.news_show(show, false);
        }
    }

    /// Tutte le novità (da Opzioni).
    pub(super) fn news_all(&mut self) {
        self.news_show(sections(), true);
    }

    fn news_show(&mut self, list: Vec<(String, Vec<String>)>, all: bool) {
        let rows: Vec<NewsVM> = list
            .into_iter()
            .map(|(date, items)| NewsVM { date: ss(date), items: model(items.into_iter().map(ss).collect::<Vec<_>>()) })
            .collect();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_news_title(ss(if all { "Novità" } else { "Novità di questa versione" }));
        st.set_news_rows(model(rows));
        st.set_news_visible(true);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn news_file_has_dated_sections() {
        let s = super::sections();
        assert!(!s.is_empty() && s.iter().all(|(d, v)| !d.is_empty() && !v.is_empty()));
    }
}
