//! Strumenti PDF per preparare il materiale da dare a un'AI: dividere un PDF troppo grande in
//! parti sotto un limite di dimensione, ed estrarne il testo in Markdown (con i segnaposto di
//! pagina `<!-- p. N -->`) dalle pagine che lo contengono già.

use lopdf::Document;

/// Una parte di un PDF diviso: pagine del PDF originale che contiene (contate da 1) e contenuto.
pub struct Part {
    pub pages: Vec<u32>,
    pub bytes: Vec<u8>,
}

impl Part {
    /// Le pagine in forma breve: «25-30», «3-5, 9».
    pub fn label(&self) -> String {
        ranges_label(&self.pages)
    }
}

/// «1, 2, 3, 7, 9, 10» → «1-3, 7, 9-10».
pub fn ranges_label(pages: &[u32]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < pages.len() {
        let mut j = i;
        while j + 1 < pages.len() && pages[j + 1] == pages[j] + 1 {
            j += 1;
        }
        out.push(if i == j { pages[i].to_string() } else { format!("{}-{}", pages[i], pages[j]) });
        i = j + 1;
    }
    out.join(", ")
}

/// Pagine scelte a parole («25-30, 40, 45-»; vuoto = tutte) su un PDF di `n` pagine: in ordine
/// e senza ripetizioni.
pub fn parse_pages(spec: &str, n: u32) -> Result<Vec<u32>, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok((1..=n).collect());
    }
    let mut set = std::collections::BTreeSet::new();
    for part in spec.split([',', ';', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once(['-', '–']) {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let a: u32 = if a.is_empty() { 1 } else { a.parse().map_err(|_| format!("«{part}» non è una pagina"))? };
        let b: u32 = if b.is_empty() { n } else { b.parse().map_err(|_| format!("«{part}» non è una pagina"))? };
        if a == 0 || a > b || b > n {
            return Err(format!("«{part}»: il PDF ha le pagine da 1 a {n}"));
        }
        set.extend(a..=b);
    }
    Ok(set.into_iter().collect())
}

fn load(bytes: &[u8]) -> Result<Document, String> {
    let doc = Document::load_mem(bytes).map_err(|e| format!("PDF non leggibile: {e}"))?;
    if doc.is_encrypted() {
        return Err("PDF protetto da password: toglila prima di dividerlo".into());
    }
    Ok(doc)
}

/// Un nuovo PDF con le sole pagine `keep` di `doc`.
fn extract(doc: &Document, keep: &[u32]) -> Result<Vec<u8>, String> {
    let mut d = doc.clone();
    let drop: Vec<u32> = d.get_pages().keys().copied().filter(|p| !keep.contains(p)).collect();
    d.delete_pages(&drop);
    d.prune_objects();
    d.compress();
    let mut out = Vec::new();
    d.save_to(&mut out).map_err(|e| format!("salvataggio non riuscito: {e}"))?;
    Ok(out)
}

/// Numero di pagine.
pub fn pages(bytes: &[u8]) -> Result<u32, String> {
    Ok(load(bytes)?.get_pages().len() as u32)
}

/// Divide le pagine `sel` del PDF (vuoto = tutte) in parti di al massimo `max_bytes`
/// ciascuna, a pagine intere e in ordine. Una pagina da sola più grande del limite resta una
/// parte a sé (non si può dividere oltre).
pub fn split(bytes: &[u8], sel: &[u32], max_bytes: usize) -> Result<Vec<Part>, String> {
    let doc = load(bytes)?;
    let n = doc.get_pages().len() as u32;
    if n == 0 {
        return Err("il PDF non ha pagine".into());
    }
    let sel: Vec<u32> =
        if sel.is_empty() { (1..=n).collect() } else { sel.iter().copied().filter(|p| *p >= 1 && *p <= n).collect() };
    if sel.is_empty() {
        return Err("nessuna pagina scelta".into());
    }
    let max = max_bytes.max(1);
    if sel.len() as u32 == n && bytes.len() <= max {
        return Ok(vec![Part { pages: sel, bytes: bytes.to_vec() }]);
    }
    let avg = (bytes.len() as f64 / n as f64).max(1.0);
    let mut parts = Vec::new();
    let mut i = 0usize;
    // pagine per parte: stima dalla media, corretta su ogni parte davvero costruita
    let mut k = ((max as f64 * 0.85 / avg).floor() as usize).max(1);
    while i < sel.len() {
        let mut tries = 0;
        let (end, out) = loop {
            tries += 1;
            let end = (i + k).min(sel.len());
            let out = extract(&doc, &sel[i..end])?;
            let size = out.len();
            if size > max && end - i > 1 {
                // troppo grande: meno pagine, in proporzione
                k = ((k as f64 * max as f64 / size as f64 * 0.92).floor() as usize).clamp(1, k - 1);
                continue;
            }
            // c'è spazio per più pagine: si prova ad allargare (poche volte)
            if end < sel.len() && size < max * 7 / 10 && tries < 4 {
                let nk = ((k as f64 * max as f64 / size.max(1) as f64 * 0.9).floor() as usize).max(k + 1);
                let wend = (i + nk).min(sel.len());
                let wider = extract(&doc, &sel[i..wend])?;
                if wider.len() <= max {
                    k = nk;
                    break (wend, wider);
                }
            }
            break (end, out);
        };
        parts.push(Part { pages: sel[i..end].to_vec(), bytes: out });
        i = end;
    }
    Ok(parts)
}

/// Testo del PDF in Markdown, pagina per pagina con i segnaposto `<!-- p. N -->` (numero della
/// pagina nel PDF + `offset`). Restituisce anche le pagine senza testo (scansioni: servono
/// l'OCR o l'AI sul PDF).
pub fn to_markdown(bytes: &[u8], sel: &[u32], offset: i32) -> Result<(String, Vec<u32>), String> {
    let doc = load(bytes)?;
    let mut md = String::new();
    let mut empty = Vec::new();
    for p in doc.get_pages().keys().copied().filter(|p| sel.is_empty() || sel.contains(p)) {
        let text = doc.extract_text(&[p]).unwrap_or_default();
        let printed = p as i32 + offset;
        md.push_str(&format!("<!-- p. {printed} -->\n"));
        let clean = tidy(&text);
        if clean.trim().is_empty() {
            empty.push(p);
            md.push_str("[pagina senza testo: scansione]\n\n");
        } else {
            md.push_str(&clean);
            md.push_str("\n\n");
        }
    }
    Ok((md, empty))
}

/// Ripulitura minima del testo estratto: righe spezzate a metà parola («para-\ngrafo»), spazi
/// ripetuti, righe vuote in eccesso. Il resto lo fa la revisione con l'AI.
fn tidy(text: &str) -> String {
    let mut paras: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        let l = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() {
            if !cur.is_empty() {
                paras.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if cur.ends_with('-') && l.chars().next().is_some_and(|c| c.is_lowercase()) {
            cur.pop();
        } else if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(&l);
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    paras.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    /// PDF di prova con `n` pagine di testo (e un carico per pagina, per fare peso).
    fn sample(n: u32, pad: usize) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
        let mut kids = vec![];
        for i in 1..=n {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 12.into()]),
                    Operation::new("Td", vec![50.into(), 700.into()]),
                    Operation::new("Tj", vec![Object::string_literal(format!("Pagina {i} del capitolo"))]),
                    Operation::new("ET", vec![]),
                ],
            };
            let mut data = content.encode().unwrap();
            // rumore poco comprimibile dentro un commento, per dare peso alla pagina
            let mut rng = fastrand::Rng::with_seed(i as u64);
            let noise: String = (0..pad).map(|_| char::from(b'a' + rng.u8(0..26))).collect();
            data.extend_from_slice(format!("\n% {noise}\n").as_bytes());
            let content_id = doc.add_object(Stream::new(dictionary! {}, data));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            });
            kids.push(page_id.into());
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => kids, "Count" => n as i64, "Resources" => resources_id,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn splits_under_the_limit_in_order() {
        let pdf = sample(30, 20_000);
        assert_eq!(pages(&pdf).unwrap(), 30);
        let max = pdf.len() / 4;
        let parts = split(&pdf, &[], max).unwrap();
        assert!(parts.len() >= 2, "{} parti", parts.len());
        let mut next = 1;
        for p in &parts {
            assert_eq!(p.pages[0], next);
            assert!(p.bytes.len() <= max, "parte {}: {} > {max}", p.label(), p.bytes.len());
            assert_eq!(pages(&p.bytes).unwrap(), p.pages.len() as u32);
            next = p.pages[p.pages.len() - 1] + 1;
        }
        assert_eq!(next, 31);
        // sotto il limite: resta intero
        assert_eq!(split(&pdf, &[], pdf.len() + 1).unwrap().len(), 1);
        assert!(split(b"non un pdf", &[], 10).is_err());
        // solo alcune pagine, sempre sotto il limite
        let sel = parse_pages("25-30, 3", 30).unwrap();
        assert_eq!(sel, vec![3, 25, 26, 27, 28, 29, 30]);
        let parts = split(&pdf, &sel, max).unwrap();
        let all: Vec<u32> = parts.iter().flat_map(|p| p.pages.clone()).collect();
        assert_eq!(all, sel);
        assert!(parts.iter().map(|p| p.label()).collect::<Vec<_>>().join(" | ").contains("25"));
        assert_eq!(ranges_label(&[1, 2, 3, 7, 9, 10]), "1-3, 7, 9-10");
        assert_eq!(parse_pages("", 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_pages("28-", 30).unwrap(), vec![28, 29, 30]);
        assert!(parse_pages("40", 30).is_err());
        assert!(parse_pages("a-b", 30).is_err());
    }

    #[test]
    fn text_with_page_markers() {
        let pdf = sample(3, 10);
        let (md, empty) = to_markdown(&pdf, &[], 10).unwrap();
        let (only, _) = to_markdown(&pdf, &[2], 0).unwrap();
        assert!(only.contains("Pagina 2") && !only.contains("Pagina 3"));
        assert!(empty.is_empty());
        assert!(md.contains("<!-- p. 11 -->"));
        assert!(md.contains("Pagina 3 del capitolo"));
        assert_eq!(tidy("para-\ngrafo  lungo\n\n\n\naltro"), "paragrafo lungo\n\naltro");
    }
}
