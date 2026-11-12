//! Parser markdown minimale per il lettore dei libri.
//! Riconosce titoli, paragrafi, elenchi, citazioni, codice/tabelle, immagini e i
//! segnaposto di pagina `<!-- p. 123 -->` inseriti durante la conversione dei PDF.

use crate::answer::normalize;
use crate::latex;

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        text: String,
    },
    Para(String),
    Bullet {
        depth: u8,
        text: String,
    },
    Numbered {
        label: String,
        text: String,
    },
    Quote(String),
    /// codice: testo a larghezza fissa
    Mono(String),
    /// tabella: righe di celle (la prima è l'intestazione)
    Table(Vec<Vec<String>>),
    /// formula a blocco (`$$…$$`): sorgente LaTeX
    Math(String),
    Image(String),
    Page(u32),
    Rule,
}

/// Pulizia della formattazione inline (la UI mostra testo semplice).
pub fn inline(s: &str) -> String {
    // prima le formule (così `*` e `_` dentro il LaTeX non vengono toccati)
    let s = latex::render_inline(s);
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // ![alt](url) e [testo](url)
        if c == '!' && chars.get(i + 1) == Some(&'[') {
            i += 1;
            continue;
        }
        if c == '[' {
            if let Some(close) = chars[i..].iter().position(|&x| x == ']').map(|p| p + i) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = chars[close..].iter().position(|&x| x == ')').map(|p| p + close) {
                        out.extend(&chars[i + 1..close]);
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        if c == '*' || c == '`' || (c == '_' && chars.get(i + 1) == Some(&'_')) {
            i += if c == '_' { 2 } else { 1 };
            continue;
        }
        if c == '\\' && i + 1 < chars.len() {
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn page_marker(line: &str) -> Option<u32> {
    let t = line.trim();
    let inner = t.strip_prefix("<!--")?.strip_suffix("-->")?.trim();
    let rest = inner
        .strip_prefix("p.")
        .or_else(|| inner.strip_prefix("pag."))
        .or_else(|| inner.strip_prefix("page"))
        .or_else(|| inner.strip_prefix("p"))?;
    rest.trim().parse().ok()
}

pub fn parse(md: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut mono: Option<Vec<String>> = None;
    let mut math: Option<Vec<String>> = None;
    let mut table: Vec<String> = Vec::new();
    let flush_para = |para: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            blocks.push(Block::Para(inline(&para.join(" "))));
            para.clear();
        }
    };
    let flush_table = |table: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !table.is_empty() {
            let rows: Vec<Vec<String>> = table
                .iter()
                .filter(|r| !r.chars().all(|c| "|-: ".contains(c)))
                .map(|r| r.trim().trim_matches('|').split('|').map(|c| inline(c.trim())).collect())
                .collect();
            blocks.push(Block::Table(rows));
            table.clear();
        }
    };
    for raw in md.lines() {
        let line = raw.trim_end();
        if let Some(m) = math.as_mut() {
            if line.trim() == "$$" {
                blocks.push(Block::Math(m.join(" ").trim().to_string()));
                math = None;
            } else {
                m.push(line.trim().to_string());
            }
            continue;
        }
        if let Some(code) = mono.as_mut() {
            if line.trim_start().starts_with("```") {
                blocks.push(Block::Mono(code.join("\n")));
                mono = None;
            } else {
                code.push(line.to_string());
            }
            continue;
        }
        let t = line.trim_start();
        if t.starts_with('|') {
            flush_para(&mut para, &mut blocks);
            table.push(t.to_string());
            continue;
        } else {
            flush_table(&mut table, &mut blocks);
        }
        if t.is_empty() {
            flush_para(&mut para, &mut blocks);
            continue;
        }
        if let Some(p) = page_marker(t) {
            flush_para(&mut para, &mut blocks);
            blocks.push(Block::Page(p));
            continue;
        }
        if t.starts_with("<!--") {
            continue;
        }
        if let Some(inner) = t.strip_prefix("$$").and_then(|x| x.strip_suffix("$$")) {
            if !inner.contains("$$") && !inner.trim().is_empty() {
                flush_para(&mut para, &mut blocks);
                blocks.push(Block::Math(inner.trim().to_string()));
                continue;
            }
        }
        if t == "$$" {
            // formula su più righe
            flush_para(&mut para, &mut blocks);
            mono = None;
            math = Some(Vec::new());
            continue;
        }
        if t.starts_with("```") {
            flush_para(&mut para, &mut blocks);
            mono = Some(Vec::new());
            continue;
        }
        let hashes = t.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && t.chars().nth(hashes) == Some(' ') {
            flush_para(&mut para, &mut blocks);
            blocks.push(Block::Heading { level: hashes as u8, text: inline(t[hashes..].trim()) });
            continue;
        }
        if t == "---" || t == "***" || t == "___" {
            flush_para(&mut para, &mut blocks);
            blocks.push(Block::Rule);
            continue;
        }
        if let Some(rest) = t.strip_prefix("![") {
            flush_para(&mut para, &mut blocks);
            let alt = rest.split(']').next().unwrap_or("").to_string();
            blocks.push(Block::Image(inline(&alt)));
            continue;
        }
        if let Some(rest) = t.strip_prefix("> ").or_else(|| t.strip_prefix('>')) {
            flush_para(&mut para, &mut blocks);
            blocks.push(Block::Quote(inline(rest.trim())));
            continue;
        }
        let indent = (line.len() - t.len()) as u8 / 2;
        if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
            flush_para(&mut para, &mut blocks);
            blocks.push(Block::Bullet { depth: indent.min(3), text: inline(rest.trim()) });
            continue;
        }
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && digits < 4 && (t[digits..].starts_with(". ") || t[digits..].starts_with(") ")) {
            flush_para(&mut para, &mut blocks);
            blocks.push(Block::Numbered { label: t[..=digits].to_string(), text: inline(t[digits + 2..].trim()) });
            continue;
        }
        para.push(t.to_string());
    }
    if let Some(code) = mono {
        blocks.push(Block::Mono(code.join("\n")));
    }
    flush_table(&mut table, &mut blocks);
    flush_para(&mut para, &mut blocks);
    blocks
}

#[derive(Debug, Clone)]
pub struct Section {
    pub title: String,
    pub level: u8,
    /// indici [start, end) nei blocchi
    pub start: usize,
    pub end: usize,
    /// pagina del libro all'inizio della sezione
    pub page: Option<u32>,
}

/// Divide i blocchi in sezioni ai titoli di livello ≤ 3. Il testo prima del primo
/// titolo diventa una sezione "Inizio".
pub fn sections(blocks: &[Block]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut page: Option<u32> = None;
    let mut cur: Option<Section> = None;
    for (i, b) in blocks.iter().enumerate() {
        match b {
            Block::Page(p) => page = Some(*p),
            Block::Heading { level, text } if *level <= 3 => {
                if let Some(mut s) = cur.take() {
                    s.end = i;
                    if s.end > s.start {
                        out.push(s);
                    }
                }
                // se il segnaposto di pagina segue subito il titolo, usa quello
                let next_page = match blocks.get(i + 1) {
                    Some(Block::Page(p)) => Some(*p),
                    _ => page,
                };
                cur = Some(Section { title: text.clone(), level: *level, start: i, end: i, page: next_page });
            }
            _ => {
                if cur.is_none() {
                    cur = Some(Section { title: "Inizio".into(), level: 1, start: i, end: i, page });
                }
            }
        }
    }
    if let Some(mut s) = cur {
        s.end = blocks.len();
        if s.end > s.start {
            out.push(s);
        }
    }
    out
}

/// Trova la sezione: titolo uguale (normalizzato), poi titolo che inizia con il testo
/// cercato (es. "9.3"), poi titolo che lo contiene; altrimenti la sezione che contiene `page`.
pub fn find_section(secs: &[Section], query: &str, page: Option<u32>) -> Option<usize> {
    let q = normalize(query);
    if !q.is_empty() {
        let norm: Vec<String> = secs.iter().map(|s| normalize(&s.title)).collect();
        if let Some(i) = norm.iter().position(|t| *t == q) {
            return Some(i);
        }
        if let Some(i) = norm.iter().position(|t| t.starts_with(&q)) {
            return Some(i);
        }
        if let Some(i) = norm.iter().position(|t| t.contains(&q)) {
            return Some(i);
        }
    }
    let p = page?;
    secs.iter().rposition(|s| s.page.is_some_and(|sp| sp <= p))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = "<!-- p. 240 -->\n# 9 Fluidi\nIntro **forte**.\n\n## 9.3 Teorema di Bernoulli\n<!-- p. 245 -->\nTesto [link](http://x) e `codice`.\n- punto\n1. primo\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";

    #[test]
    fn math_blocks() {
        let b = parse(
            "Testo $v = \\frac{s}{t}$ ok\n\n$$ \\frac{1}{p} + \\frac{1}{q} = \\frac{1}{f} $$\n$$\nE = mc^2\n$$\n",
        );
        assert_eq!(b[0], Block::Para("Testo v = s/t ok".into()));
        assert_eq!(b[1], Block::Math("\\frac{1}{p} + \\frac{1}{q} = \\frac{1}{f}".into()));
        assert_eq!(b[2], Block::Math("E = mc^2".into()));
    }

    #[test]
    fn parse_and_sections() {
        let b = parse(MD);
        assert_eq!(b[0], Block::Page(240));
        assert!(b.contains(&Block::Para("Intro forte.".into())));
        assert!(b.contains(&Block::Para("Testo link e codice.".into())));
        assert!(b.contains(&Block::Table(vec![vec!["a".into(), "b".into()], vec!["1".into(), "2".into()]])));
        let s = sections(&b);
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].page, Some(245));
        assert_eq!(find_section(&s, "9.3", None), Some(1));
        assert_eq!(find_section(&s, "Teorema di bernoulli", None), Some(1));
        assert_eq!(find_section(&s, "", Some(246)), Some(1));
        assert_eq!(find_section(&s, "", Some(241)), Some(0));
    }
}
