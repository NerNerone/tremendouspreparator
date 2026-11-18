//! Testo di documenti non PDF in Markdown, per «Estrai il testo»: Word (.docx), PowerPoint
//! (.pptx), OpenDocument (.odt, .odp), EPUB, HTML e testo semplice. Sono archivi ZIP con XML
//! (o testo), letti qui senza programmi esterni. A differenza del PDF la struttura c'è nel file:
//! titoli, elenchi e tabelle diventano Markdown vero. Le pagine (le slide nelle presentazioni)
//! hanno il segnaposto `<!-- p. N -->` come per i PDF.

use std::collections::BTreeMap;
use std::io::Read;

/// Documento convertito.
pub struct Doc {
    /// tipo riconosciuto («Word», «PowerPoint»…), per i messaggi
    pub kind: &'static str,
    pub md: String,
    /// pagine o slide trovate (0 se il formato non ne ha)
    pub pages: u32,
}

/// Converte il documento (riconosciuto dal contenuto). `sel` = pagine o slide da tenere (vuoto =
/// tutte), dove il formato le ha.
pub fn to_markdown(bytes: &[u8], sel: &[u32]) -> Result<Doc, String> {
    if bytes.starts_with(b"PK\x03\x04") {
        let z = unzip(bytes)?;
        let mime = z.get("mimetype").map(|m| String::from_utf8_lossy(m).trim().to_string()).unwrap_or_default();
        if z.contains_key("word/document.xml") {
            return docx(&z, sel);
        }
        if z.contains_key("ppt/presentation.xml") {
            return pptx(&z, sel);
        }
        if mime.starts_with("application/vnd.oasis.opendocument.text") {
            return odf(&z, "Writer (ODT)", false, sel);
        }
        if mime.starts_with("application/vnd.oasis.opendocument.presentation") {
            return odf(&z, "Impress (ODP)", true, sel);
        }
        if mime == "application/epub+zip" || z.contains_key("META-INF/container.xml") {
            return epub(&z);
        }
        return Err("archivio ZIP che non è un documento Word, PowerPoint, OpenDocument o EPUB".into());
    }
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| "formato non riconosciuto (né PDF, né Word, PowerPoint, OpenDocument, EPUB o testo)")?;
    let low = text.trim_start().to_lowercase();
    if low.starts_with("<!doctype html") || low.starts_with("<html") || low.contains("<body") {
        return Ok(Doc { kind: "HTML", md: html_to_md(&text), pages: 0 });
    }
    Ok(Doc { kind: "testo", md: text.replace("\r\n", "\n"), pages: 0 })
}

// ---------- ZIP ----------

/// Contenuto di un archivio ZIP (file non compressi o compressi con deflate).
fn unzip(b: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let u16le = |i: usize| b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize);
    let u32le = |i: usize| b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize);
    // fine della directory centrale: in fondo, prima di un eventuale commento
    let eocd = (0..b.len().saturating_sub(21))
        .rev()
        .take(70_000)
        .find(|&i| b[i..].starts_with(b"PK\x05\x06"))
        .ok_or("archivio ZIP non valido")?;
    let n = u16le(eocd + 10).ok_or("ZIP")?;
    let mut at = u32le(eocd + 16).ok_or("ZIP")?;
    let mut out = BTreeMap::new();
    for _ in 0..n {
        if !b.get(at..).is_some_and(|s| s.starts_with(b"PK\x01\x02")) {
            break;
        }
        let method = u16le(at + 10).ok_or("ZIP")?;
        let csize = u32le(at + 20).ok_or("ZIP")?;
        let (nlen, xlen, clen) =
            (u16le(at + 28).ok_or("ZIP")?, u16le(at + 30).ok_or("ZIP")?, u16le(at + 32).ok_or("ZIP")?);
        let local = u32le(at + 42).ok_or("ZIP")?;
        let name = String::from_utf8_lossy(b.get(at + 46..at + 46 + nlen).ok_or("ZIP")?).to_string();
        at += 46 + nlen + xlen + clen;
        let (ln, lx) = (u16le(local + 26).ok_or("ZIP")?, u16le(local + 28).ok_or("ZIP")?);
        let start = local + 30 + ln + lx;
        let Some(data) = b.get(start..start + csize) else { continue };
        let bytes = match method {
            0 => data.to_vec(),
            8 => {
                let mut v = Vec::new();
                flate2::read::DeflateDecoder::new(data)
                    .take(64 * 1024 * 1024)
                    .read_to_end(&mut v)
                    .map_err(|e| format!("ZIP: {e}"))?;
                v
            }
            _ => continue,
        };
        out.insert(name, bytes);
    }
    Ok(out)
}

fn xml(z: &BTreeMap<String, Vec<u8>>, path: &str) -> Option<String> {
    z.get(path).map(|b| String::from_utf8_lossy(b).into_owned())
}

type Node<'a, 'i> = roxmltree::Node<'a, 'i>;

fn kids<'a, 'i>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.tag_name().name() == name)
}

/// Attributo per nome locale (senza prefisso).
fn attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

fn table_md(rows: &[Vec<String>]) -> String {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return String::new();
    }
    let line = |r: &Vec<String>| {
        let cells: Vec<String> =
            (0..cols).map(|i| r.get(i).map(|c| c.replace('|', "\\|").replace('\n', " ")).unwrap_or_default()).collect();
        format!("| {} |", cells.join(" | "))
    };
    let mut out = vec![line(&rows[0]), format!("|{}", " --- |".repeat(cols))];
    out.extend(rows[1..].iter().map(line));
    out.join("\n")
}

fn keep(sel: &[u32], page: u32) -> bool {
    sel.is_empty() || sel.contains(&page)
}

// ---------- Word ----------

/// Livello di titolo degli stili di Word («Heading 2», «Titolo 2», «Title») e stili di
/// elenco («List Bullet», «Elenco puntato», o con la numerazione nello stile).
fn docx_styles(z: &BTreeMap<String, Vec<u8>>) -> (BTreeMap<String, usize>, Vec<String>) {
    let mut out = BTreeMap::new();
    let mut lists = Vec::new();
    let Some(t) = xml(z, "word/styles.xml") else { return (out, lists) };
    let Ok(doc) = roxmltree::Document::parse(&t) else { return (out, lists) };
    for s in doc.descendants().filter(|n| n.tag_name().name() == "style") {
        let Some(id) = attr(s, "styleId") else { continue };
        let name = kids(s, "name").next().and_then(|n| attr(n, "val")).unwrap_or("").to_lowercase();
        let outline = s
            .descendants()
            .find(|n| n.tag_name().name() == "outlineLvl")
            .and_then(|n| attr(n, "val"))
            .and_then(|v| v.parse::<usize>().ok());
        let level = if name == "title" || name == "titolo" {
            Some(1)
        } else if let Some(n) = name.strip_prefix("heading ").or_else(|| name.strip_prefix("titolo ")) {
            n.trim().parse::<usize>().ok()
        } else {
            outline.map(|l| l + 1)
        };
        if let Some(l) = level.filter(|l| (1..=6).contains(l)) {
            out.insert(id.to_string(), l);
        } else if name.starts_with("list bullet")
            || name.starts_with("list number")
            || name.starts_with("elenco")
            || s.descendants().any(|n| n.tag_name().name() == "numPr")
        {
            lists.push(id.to_string());
        }
    }
    (out, lists)
}

/// Testo di un paragrafo di Word, con grassetto e corsivo; `breaks` conta i salti di pagina.
fn docx_runs(p: Node, breaks: &mut u32) -> String {
    let mut s = String::new();
    for r in p.descendants().filter(|n| n.tag_name().name() == "r") {
        let props = kids(r, "rPr").next();
        let on = |tag: &str| {
            props
                .and_then(|p| kids(p, tag).next())
                .is_some_and(|b| attr(b, "val").is_none_or(|v| v != "0" && v != "false"))
        };
        let (b, i) = (on("b"), on("i"));
        let mut t = String::new();
        for c in r.children() {
            match c.tag_name().name() {
                "t" => t.push_str(c.text().unwrap_or("")),
                "tab" => t.push(' '),
                "br" if attr(c, "type") == Some("page") => *breaks += 1,
                "br" => t.push('\n'),
                "lastRenderedPageBreak" => *breaks += 1,
                _ => {}
            }
        }
        if t.trim().is_empty() || (!b && !i) {
            s.push_str(&t);
        } else {
            let mark = if b && i {
                "***"
            } else if b {
                "**"
            } else {
                "*"
            };
            let lead = &t[..t.len() - t.trim_start().len()];
            let tail = &t[t.trim_end().len()..];
            s.push_str(&format!("{lead}{mark}{}{mark}{tail}", t.trim()));
        }
    }
    s
}

fn docx(z: &BTreeMap<String, Vec<u8>>, sel: &[u32]) -> Result<Doc, String> {
    let t = xml(z, "word/document.xml").ok_or("Word: manca il testo")?;
    let doc = roxmltree::Document::parse(&t).map_err(|e| format!("Word non leggibile: {e}"))?;
    let (styles, list_styles) = docx_styles(z);
    let body = doc.descendants().find(|n| n.tag_name().name() == "body").ok_or("Word: manca il corpo")?;
    let mut page = 1u32;
    let mut out = vec![String::from("<!-- p. 1 -->")];
    let mut blocks: Vec<(u32, String)> = Vec::new();
    for b in body.children().filter(|n| n.is_element()) {
        match b.tag_name().name() {
            "p" => {
                let mut breaks = 0;
                let text = docx_runs(b, &mut breaks);
                let ppr = kids(b, "pPr").next();
                let style = ppr.and_then(|p| kids(p, "pStyle").next()).and_then(|s| attr(s, "val")).unwrap_or("");
                let list = ppr.and_then(|p| kids(p, "numPr").next()).map(|n| {
                    kids(n, "ilvl")
                        .next()
                        .and_then(|l| attr(l, "val"))
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0)
                });
                let list = list.or_else(|| list_styles.iter().any(|l| l == style).then_some(0));
                // il salto di pagina viene prima del testo del paragrafo
                page += breaks;
                let line = text.trim();
                if !line.is_empty() {
                    let md = if let Some(l) = styles.get(style) {
                        format!("{} {}", "#".repeat(*l), line.replace("**", ""))
                    } else if let Some(depth) = list {
                        format!("{}- {line}", "  ".repeat(depth))
                    } else {
                        line.to_string()
                    };
                    blocks.push((page, md));
                }
            }
            "tbl" => {
                let mut breaks = 0;
                let rows: Vec<Vec<String>> = kids(b, "tr")
                    .map(|r| {
                        kids(r, "tc")
                            .map(|c| {
                                kids(c, "p")
                                    .map(|p| docx_runs(p, &mut breaks))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                                    .trim()
                                    .to_string()
                            })
                            .collect()
                    })
                    .collect();
                blocks.push((page, table_md(&rows)));
                page += breaks;
            }
            _ => {}
        }
    }
    let mut last = 1;
    let mut prev_list = false;
    for (p, md) in blocks.into_iter().filter(|(p, _)| keep(sel, *p)) {
        if p != last {
            out.push(format!("<!-- p. {p} -->"));
            last = p;
        }
        let is_list = md.trim_start().starts_with("- ");
        if is_list && prev_list {
            // voci di uno stesso elenco: una riga ciascuna
            if let Some(l) = out.last_mut() {
                l.push('\n');
                l.push_str(&md);
            }
        } else {
            out.push(md);
        }
        prev_list = is_list;
    }
    if !sel.is_empty() && !sel.contains(&1) {
        out.remove(0);
    }
    Ok(Doc { kind: "Word", md: out.join("\n\n") + "\n", pages: page })
}

// ---------- PowerPoint ----------

/// Paragrafi di una forma di PowerPoint, con il livello del punto elenco.
fn pptx_paras(sp: Node) -> Vec<(usize, String)> {
    sp.descendants()
        .filter(|n| n.tag_name().name() == "p" && n.tag_name().namespace().is_some_and(|ns| ns.contains("drawingml")))
        .map(|p| {
            let lvl = kids(p, "pPr").next().and_then(|r| attr(r, "lvl")).and_then(|v| v.parse().ok()).unwrap_or(0);
            let text: String = p
                .descendants()
                .filter(|n| matches!(n.tag_name().name(), "t" | "br"))
                .map(
                    |n| if n.tag_name().name() == "br" { "\n".to_string() } else { n.text().unwrap_or("").to_string() },
                )
                .collect();
            (lvl, text.trim().to_string())
        })
        .filter(|(_, t)| !t.is_empty())
        .collect()
}

fn pptx(z: &BTreeMap<String, Vec<u8>>, sel: &[u32]) -> Result<Doc, String> {
    // slide in ordine: ppt/slides/slideN.xml
    let mut slides: Vec<(u32, &String)> = z
        .keys()
        .filter_map(|k| {
            let n = k.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?.parse().ok()?;
            Some((n, k))
        })
        .collect();
    slides.sort();
    let mut out = Vec::new();
    for (i, (num, path)) in slides.iter().enumerate() {
        let n = i as u32 + 1;
        if !keep(sel, n) {
            continue;
        }
        let t = xml(z, path).unwrap_or_default();
        let Ok(doc) = roxmltree::Document::parse(&t) else { continue };
        let mut title = String::new();
        let mut body = Vec::new();
        let mut figures = 0;
        for sp in doc.descendants().filter(|n| matches!(n.tag_name().name(), "sp" | "graphicFrame" | "pic")) {
            match sp.tag_name().name() {
                "pic" => figures += 1,
                "graphicFrame" => {
                    if let Some(tbl) = sp.descendants().find(|n| n.tag_name().name() == "tbl") {
                        let rows: Vec<Vec<String>> = kids(tbl, "tr")
                            .map(|r| {
                                kids(r, "tc")
                                    .map(|c| pptx_paras(c).into_iter().map(|(_, t)| t).collect::<Vec<_>>().join(" "))
                                    .collect()
                            })
                            .collect();
                        body.push(table_md(&rows));
                    }
                }
                _ => {
                    let ph = sp.descendants().find(|n| n.tag_name().name() == "ph").and_then(|p| attr(p, "type"));
                    let paras = pptx_paras(sp);
                    if matches!(ph, Some("title" | "ctrTitle")) && title.is_empty() {
                        title = paras.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>().join(" ");
                    } else if !paras.is_empty() {
                        let many = paras.len() > 1;
                        body.push(
                            paras
                                .iter()
                                .map(
                                    |(l, t)| {
                                        if many || *l > 0 {
                                            format!("{}- {t}", "  ".repeat(*l))
                                        } else {
                                            t.clone()
                                        }
                                    },
                                )
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );
                    }
                }
            }
        }
        let mut s = format!("<!-- p. {n} -->\n\n## {}", if title.is_empty() { format!("Slide {n}") } else { title });
        for b in body {
            s.push_str("\n\n");
            s.push_str(&b);
        }
        if figures > 0 {
            s.push_str(&format!("\n\n[{figures} {} nella slide]", if figures == 1 { "figura" } else { "figure" }));
        }
        // note del relatore (collegate dalla slide)
        let rels = xml(z, &format!("ppt/slides/_rels/slide{num}.xml.rels")).unwrap_or_default();
        if let Some(target) = rels
            .split("Target=\"")
            .skip(1)
            .map(|r| r.split('"').next().unwrap_or(""))
            .find(|t| t.contains("notesSlide"))
        {
            let path = format!("ppt/notesSlides/{}", target.rsplit('/').next().unwrap_or(""));
            if let Some(nt) = xml(z, &path) {
                if let Ok(nd) = roxmltree::Document::parse(&nt) {
                    let notes: Vec<String> = nd
                        .descendants()
                        .filter(|n| n.tag_name().name() == "sp")
                        .filter(|sp| {
                            sp.descendants().find(|n| n.tag_name().name() == "ph").and_then(|p| attr(p, "type"))
                                == Some("body")
                        })
                        .flat_map(pptx_paras)
                        .map(|(_, t)| t)
                        .collect();
                    if !notes.is_empty() {
                        s.push_str(&format!("\n\n> Note: {}", notes.join(" ")));
                    }
                }
            }
        }
        out.push(s);
    }
    Ok(Doc { kind: "PowerPoint", md: out.join("\n\n") + "\n", pages: slides.len() as u32 })
}

// ---------- OpenDocument ----------

fn odf_text(n: Node) -> String {
    let mut s = String::new();
    for c in n.children() {
        if c.is_text() {
            s.push_str(c.text().unwrap_or(""));
        } else {
            match c.tag_name().name() {
                "s" | "tab" => s.push(' '),
                "line-break" => s.push('\n'),
                "note" => {}
                _ => s.push_str(&odf_text(c)),
            }
        }
    }
    s
}

fn odf_blocks(n: Node, depth: usize, out: &mut Vec<String>) {
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "h" => {
                let l = attr(c, "outline-level").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).clamp(1, 6);
                let t = odf_text(c);
                if !t.trim().is_empty() {
                    out.push(format!("{} {}", "#".repeat(l), t.trim()));
                }
            }
            "p" => {
                let t = odf_text(c);
                if !t.trim().is_empty() {
                    out.push(t.trim().to_string());
                }
            }
            "list" => {
                let mut items = Vec::new();
                for it in kids(c, "list-item") {
                    let mut sub = Vec::new();
                    odf_blocks(it, depth + 1, &mut sub);
                    for (k, line) in sub.into_iter().enumerate() {
                        if k == 0 && !line.trim_start().starts_with("- ") {
                            items.push(format!("{}- {line}", "  ".repeat(depth)));
                        } else {
                            items.push(line);
                        }
                    }
                }
                if !items.is_empty() {
                    out.push(items.join("\n"));
                }
            }
            "table" => {
                let rows: Vec<Vec<String>> = c
                    .descendants()
                    .filter(|r| r.tag_name().name() == "table-row")
                    .map(|r| kids(r, "table-cell").map(|cell| odf_text(cell).trim().to_string()).collect())
                    .collect();
                out.push(table_md(&rows));
            }
            "frame" | "custom-shape" | "text-box" | "section" | "g" => odf_blocks(c, depth, out),
            _ => {}
        }
    }
}

fn odf(z: &BTreeMap<String, Vec<u8>>, kind: &'static str, slides: bool, sel: &[u32]) -> Result<Doc, String> {
    let t = xml(z, "content.xml").ok_or("OpenDocument: manca il contenuto")?;
    let doc = roxmltree::Document::parse(&t).map_err(|e| format!("OpenDocument non leggibile: {e}"))?;
    if slides {
        let pages: Vec<Node> = doc.descendants().filter(|n| n.tag_name().name() == "page").collect();
        let mut out = Vec::new();
        for (i, p) in pages.iter().enumerate() {
            let n = i as u32 + 1;
            if !keep(sel, n) {
                continue;
            }
            let mut b = Vec::new();
            odf_blocks(*p, 0, &mut b);
            let title = attr(*p, "name").filter(|t| !t.starts_with("page")).unwrap_or("");
            let head = if title.is_empty() { format!("Slide {n}") } else { title.to_string() };
            out.push(format!("<!-- p. {n} -->\n\n## {head}\n\n{}", b.join("\n\n")));
        }
        return Ok(Doc { kind, md: out.join("\n\n") + "\n", pages: pages.len() as u32 });
    }
    let body = doc.descendants().find(|n| n.tag_name().name() == "text").ok_or("OpenDocument: manca il testo")?;
    let mut out = Vec::new();
    odf_blocks(body, 0, &mut out);
    Ok(Doc { kind, md: out.join("\n\n") + "\n", pages: 0 })
}

// ---------- EPUB e HTML ----------

fn epub(z: &BTreeMap<String, Vec<u8>>) -> Result<Doc, String> {
    let container = xml(z, "META-INF/container.xml").ok_or("EPUB: manca container.xml")?;
    let opf_path =
        container.split("full-path=\"").nth(1).and_then(|r| r.split('"').next()).ok_or("EPUB: manca l'indice")?;
    let opf = xml(z, opf_path).ok_or("EPUB: indice non trovato")?;
    let dir = opf_path.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
    let doc = roxmltree::Document::parse(&opf).map_err(|e| format!("EPUB non leggibile: {e}"))?;
    let items: BTreeMap<&str, &str> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "item")
        .filter_map(|n| Some((attr(n, "id")?, attr(n, "href")?)))
        .collect();
    let mut out = Vec::new();
    for r in doc.descendants().filter(|n| n.tag_name().name() == "itemref") {
        let Some(href) = attr(r, "idref").and_then(|id| items.get(id)) else { continue };
        let path = format!("{dir}{}", href.split('#').next().unwrap_or(href));
        if let Some(h) = xml(z, &path) {
            let md = html_to_md(&h);
            if !md.trim().is_empty() {
                out.push(md.trim().to_string());
            }
        }
    }
    Ok(Doc { kind: "EPUB", md: out.join("\n\n") + "\n", pages: 0 })
}

/// HTML (anche non ben formato) in Markdown: titoli, paragrafi, elenchi, tabelle, grassetto.
pub fn html_to_md(h: &str) -> String {
    let mut s = h.to_string();
    for (re, rep) in [
        (r"(?is)<(script|style|head)\b.*?</\s*(script|style|head)\s*>", ""),
        (r"(?is)<!--.*?-->", ""),
        (r"(?i)<br\s*/?>", "\n"),
        (r"(?i)</?(strong|b)\b[^>]*>", "**"),
        (r"(?i)</?(em|i)\b[^>]*>", "*"),
        (r"(?i)<li\b[^>]*>", "\n- "),
        (r"(?i)</(p|div|li|ul|ol|blockquote|section|article)\s*>", "\n\n"),
        (r"(?i)<(p|div|ul|ol|blockquote|section|article)\b[^>]*>", "\n\n"),
        (r"(?i)</t[dh]\s*>", " | "),
        (r"(?i)<tr\b[^>]*>", "\n| "),
        (r"(?i)</tr\s*>", ""),
        (r"(?i)</?(table|thead|tbody|tfoot|t[dh])\b[^>]*>", "\n"),
    ] {
        s = regex_lite::Regex::new(re).expect("regex").replace_all(&s, rep).into_owned();
    }
    for l in 1..=6 {
        let re = regex_lite::Regex::new(&format!(r"(?is)<h{l}\b[^>]*>(.*?)</h{l}\s*>")).expect("regex");
        s = re
            .replace_all(&s, |c: &regex_lite::Captures| format!("\n\n{} {}\n\n", "#".repeat(l), c[1].trim()))
            .into_owned();
    }
    s = regex_lite::Regex::new(r"(?s)<[^>]+>").expect("regex").replace_all(&s, "").into_owned();
    for (a, b) in [
        ("&nbsp;", " "),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
    ] {
        s = s.replace(a, b);
    }
    // righe vuote in eccesso
    let mut out: Vec<&str> = Vec::new();
    for line in s.lines().map(str::trim_end) {
        if line.trim().is_empty() && out.last().is_none_or(|l| l.trim().is_empty()) {
            continue;
        }
        out.push(line);
    }
    out.join("\n").trim().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Archivio ZIP non compresso, per i test.
    fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let off = out.len() as u32;
            let (n, d) = (name.as_bytes(), data.as_bytes());
            let crc = 0u32; // il lettore non controlla il CRC
            let mut h = b"PK\x03\x04".to_vec();
            h.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            h.extend(crc.to_le_bytes());
            h.extend((d.len() as u32).to_le_bytes());
            h.extend((d.len() as u32).to_le_bytes());
            h.extend((n.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            out.extend(&h);
            out.extend(n);
            out.extend(d);
            let mut c = b"PK\x01\x02".to_vec();
            c.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            c.extend(crc.to_le_bytes());
            c.extend((d.len() as u32).to_le_bytes());
            c.extend((d.len() as u32).to_le_bytes());
            c.extend((n.len() as u16).to_le_bytes());
            c.extend([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            c.extend(off.to_le_bytes());
            c.extend(n);
            central.extend(c);
        }
        let start = out.len() as u32;
        out.extend(&central);
        out.extend(b"PK\x05\x06\0\0\0\0");
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(start.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    #[test]
    fn word_headings_lists_tables_and_pages() {
        let doc = format!(
            r#"<w:document {W}><w:body>
            <w:p><w:pPr><w:pStyle w:val="Titolo1"/></w:pPr><w:r><w:t>Cinematica</w:t></w:r></w:p>
            <w:p><w:r><w:t xml:space="preserve">La velocità è </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>vettoriale</w:t></w:r></w:p>
            <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr><w:r><w:t>uno</w:t></w:r></w:p>
            <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr><w:r><w:t>due</w:t></w:r></w:p>
            <w:p><w:r><w:br w:type="page"/><w:t>Seconda pagina</w:t></w:r></w:p>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr>
            <w:tr><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            </w:body></w:document>"#
        );
        let styles =
            format!(r#"<w:styles {W}><w:style w:styleId="Titolo1"><w:name w:val="heading 1"/></w:style></w:styles>"#);
        let z = zip(&[("word/document.xml", &doc), ("word/styles.xml", &styles)]);
        let d = to_markdown(&z, &[]).unwrap();
        assert_eq!(d.kind, "Word");
        assert!(d.md.starts_with("<!-- p. 1 -->\n\n# Cinematica"), "{}", d.md);
        assert!(d.md.contains("La velocità è **vettoriale**"));
        assert!(d.md.contains("- uno\n- due"));
        assert!(d.md.contains("<!-- p. 2 -->\n\nSeconda pagina"));
        assert!(d.md.contains("| A | B |\n| --- | --- |\n| 1 | 2 |"));
        // solo la seconda pagina
        let p2 = to_markdown(&z, &[2]).unwrap();
        assert!(!p2.md.contains("Cinematica") && p2.md.contains("Seconda pagina"), "{}", p2.md);
    }

    #[test]
    fn powerpoint_slides_with_titles_bullets_and_notes() {
        let ns = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;
        let slide = |title: &str, items: &[&str]| {
            let paras: String = items.iter().map(|i| format!("<a:p><a:r><a:t>{i}</a:t></a:r></a:p>")).collect();
            format!(
                r#"<p:sld {ns}><p:cSld><p:spTree>
                <p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>{title}</a:t></a:r></a:p></p:txBody></p:sp>
                <p:sp><p:txBody>{paras}</p:txBody></p:sp><p:pic/></p:spTree></p:cSld></p:sld>"#
            )
        };
        let notes = format!(
            r#"<p:notes {ns}><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Ricorda il segno</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#
        );
        let z = zip(&[
            ("ppt/presentation.xml", "<p/>"),
            ("ppt/slides/slide1.xml", &slide("Leggi di Newton", &["Inerzia", "F = m·a"])),
            ("ppt/slides/slide2.xml", &slide("Lavoro", &["L = F·s"])),
            ("ppt/slides/_rels/slide2.xml.rels", r#"<R><Relationship Target="../notesSlides/notesSlide7.xml"/></R>"#),
            ("ppt/notesSlides/notesSlide7.xml", &notes),
        ]);
        let d = to_markdown(&z, &[]).unwrap();
        assert_eq!((d.kind, d.pages), ("PowerPoint", 2));
        assert!(
            d.md.contains("<!-- p. 1 -->\n\n## Leggi di Newton\n\n- Inerzia\n- F = m·a\n\n[1 figura nella slide]"),
            "{}",
            d.md
        );
        assert!(d.md.contains("<!-- p. 2 -->\n\n## Lavoro\n\nL = F·s"));
        assert!(d.md.contains("> Note: Ricorda il segno"));
        assert!(!to_markdown(&z, &[2]).unwrap().md.contains("Newton"));
    }

    #[test]
    fn opendocument_html_and_text() {
        let content = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:text>
            <text:h text:outline-level="2">Atomo</text:h><text:p>Protoni e neutroni</text:p>
            <text:list><text:list-item><text:p>elettrone</text:p></text:list-item></text:list>
            </office:text></office:body></office:document-content>"#;
        let z = zip(&[("mimetype", "application/vnd.oasis.opendocument.text"), ("content.xml", content)]);
        let d = to_markdown(&z, &[]).unwrap();
        assert!(d.md.contains("## Atomo\n\nProtoni e neutroni\n\n- elettrone"), "{}", d.md);
        let h = to_markdown(
            b"<html><body><h1>Titolo</h1><p>Un <b>testo</b> &amp; altro</p><ul><li>a</li><li>b</li></ul></body></html>",
            &[],
        )
        .unwrap();
        assert_eq!(h.kind, "HTML");
        assert!(h.md.contains("# Titolo") && h.md.contains("Un **testo** & altro") && h.md.contains("- a"), "{}", h.md);
        assert_eq!(to_markdown("# già Markdown\n".as_bytes(), &[]).unwrap().kind, "testo");
        assert!(to_markdown(b"\xff\xfe\x00binario", &[]).is_err());
    }
}
