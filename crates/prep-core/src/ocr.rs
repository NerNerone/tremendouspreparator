//! Parte «pura» dell'OCR: ordine di lettura delle righe trovate (anche su due colonne) e
//! ripulitura del testo riconosciuto. Il riconoscimento vero (modelli `ocrs`) sta nell'app.

/// Rettangolo di una parola: sinistra, alto, destra, basso (pixel).
pub type WordBox = [f32; 4];

/// Ordine di lettura: dalle righe trovate dal rilevatore (parole in ordine da sinistra) a
/// una lista di segmenti `(riga, parole)` da riconoscere uno per uno. Se la pagina è su due
/// colonne (una fascia verticale centrale che quasi nessuna parola attraversa) ogni riga si
/// divide sulla fascia e si legge prima la colonna di sinistra, poi quella di destra; le righe
/// che attraversano tutta la pagina (titoli) fanno da separatore tra i blocchi.
pub fn reading_order(lines: &[Vec<WordBox>], width: f32) -> Vec<(usize, Vec<usize>)> {
    let top = |l: usize, ws: &[usize]| ws.iter().map(|&w| lines[l][w][1]).fold(f32::MAX, f32::min);
    let all: Vec<&WordBox> = lines.iter().flatten().collect();
    let n = all.len();
    // fascia centrale attraversata dal minor numero di parole
    let mut best = (usize::MAX, 0.0f32);
    for i in 30..=70 {
        let x = width * i as f32 / 100.0;
        let c = all.iter().filter(|b| b[0] < x && b[2] > x).count();
        if c < best.0 {
            best = (c, x);
        }
    }
    let g = best.1;
    let left = all.iter().filter(|b| b[2] <= g).count();
    let right = all.iter().filter(|b| b[0] >= g).count();
    let columns = n >= 20 && best.0 * 20 <= n && left * 5 > n && right * 5 > n;
    // una parola «ben dentro» tutte e due le metà (non una che sfiora appena la fascia)
    let deep = |b: &WordBox| (g - b[0]).min(b[2] - g) > b[3] - b[1];
    // fascia verticale occupata dalle colonne: dalle righe che non attraversano la fascia
    let side = lines.iter().filter(|l| !l.iter().any(deep)).flatten();
    let col_top = side.clone().map(|b| b[1]).fold(f32::MAX, f32::min);
    let col_bottom = side.map(|b| b[3]).fold(f32::MIN, f32::max);

    let mut items: Vec<(f32, u8, usize, Vec<usize>)> = Vec::new();
    for (l, words) in lines.iter().enumerate() {
        let idx: Vec<usize> = (0..words.len()).collect();
        if idx.is_empty() {
            continue;
        }
        // a tutta pagina (titolo in alto, piè di pagina) solo se attraversa davvero la fascia e
        // sta sopra o sotto le colonne: in mezzo è quasi sempre una riga di sinistra e una di
        // destra che il rilevatore ha unito (pagina un po' storta)
        let y0 = words.iter().map(|b| b[1]).fold(f32::MAX, f32::min);
        let y1 = words.iter().map(|b| b[3]).fold(f32::MIN, f32::max);
        if !columns || (words.iter().any(deep) && (y1 <= col_top || y0 >= col_bottom)) {
            items.push((top(l, &idx), 1, l, idx));
            continue;
        }
        let (le, ri): (Vec<usize>, Vec<usize>) = idx.into_iter().partition(|&w| (words[w][0] + words[w][2]) / 2.0 < g);
        if !le.is_empty() {
            items.push((top(l, &le), 0, l, le));
        }
        if !ri.is_empty() {
            items.push((top(l, &ri), 2, l, ri));
        }
    }
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    if !columns {
        return items.into_iter().map(|(_, _, l, w)| (l, w)).collect();
    }
    let mut out = Vec::new();
    let (mut lc, mut rc) = (Vec::new(), Vec::new());
    for (_, side, l, w) in items {
        match side {
            0 => lc.push((l, w)),
            2 => rc.push((l, w)),
            _ => {
                out.append(&mut lc);
                out.append(&mut rc);
                out.push((l, w));
            }
        }
    }
    out.append(&mut lc);
    out.append(&mut rc);
    out
}

/// I modelli conoscono solo l'alfabeto inglese: le lettere accentate escono come «?». Qui si
/// rimettono quelle sicure dell'italiano (è, perché, più, può, già, città…); il resto lo
/// sistema la revisione con l'AI.
pub fn fix_accents(text: &str) -> String {
    const WHOLE: [(&str, &str); 10] = [
        ("pi?", "più"),
        ("pu?", "può"),
        ("gi?", "già"),
        ("cos?", "così"),
        ("per?", "però"),
        ("ci?", "ciò"),
        ("perch?", "perché"),
        ("poich?", "poiché"),
        ("affinch?", "affinché"),
        ("finch?", "finché"),
    ];
    let mut out = String::with_capacity(text.len() + 16);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let words: Vec<&str> = line.split(' ').collect();
        for (j, w) in words.iter().enumerate() {
            if j > 0 {
                out.push(' ');
            }
            let next_lower = words.get(j + 1).and_then(|n| n.chars().next()).is_some_and(|c| c.is_lowercase());
            // «?» da solo in mezzo alla frase (staccato da tutte e due le parti): «è»
            if *w == "?" && j > 0 && j + 1 < words.len() {
                out.push('è');
                continue;
            }
            let (core, tail) = split_tail(w);
            let lower = core.to_lowercase();
            if let Some((_, fix)) = WHOLE.iter().find(|(k, _)| *k == lower) {
                out.push_str(&match_case(core, fix));
                out.push_str(tail);
                continue;
            }
            // parole in «-ità», «-età», «-ttà»: in italiano nessuna parola finisce in «-it»
            if lower.len() >= 5
                && lower.chars().all(|c| c.is_alphabetic() || c == '?')
                && (lower.ends_with("it?") || lower.ends_with("et?") || lower.ends_with("tt?"))
                && (next_lower || !tail.is_empty() || words.get(j + 1).is_none())
            {
                out.push_str(&core[..core.len() - 1]);
                out.push('à');
                out.push_str(tail);
                continue;
            }
            out.push_str(w);
        }
    }
    out
}

/// «più,» → («più», «,»): la punteggiatura finale (non il «?» che fa parte della parola).
fn split_tail(w: &str) -> (&str, &str) {
    let cut = w.trim_end_matches([',', '.', ';', ':', ')', '!']).len();
    w.split_at(cut)
}

fn match_case(src: &str, fix: &str) -> String {
    if src.chars().next().is_some_and(|c| c.is_uppercase()) {
        let mut c = fix.chars();
        c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
    } else {
        fix.to_string()
    }
}

/// Testo di una pagina dall'OCR: righe nell'ordine di lettura, accenti rimessi, parole
/// spezzate a fine riga riunite.
pub fn page_text(lines: &[String]) -> String {
    let joined = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
    crate::pdf::tidy(&fix_accents(&joined))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_columns_are_read_one_after_the_other() {
        // 12 righe per colonna, colonna destra da x=520; un titolo a tutta pagina in alto
        let mut lines = vec![vec![[40.0, 0.0, 300.0, 20.0], [310.0, 0.0, 700.0, 20.0]]];
        for i in 0..12 {
            let y = 40.0 + i as f32 * 30.0;
            lines.push(vec![[40.0, y, 200.0, y + 20.0], [210.0, y, 470.0, y + 20.0], [520.0, y, 980.0, y + 20.0]]);
        }
        let order = reading_order(&lines, 1000.0);
        assert_eq!(order[0], (0, vec![0, 1]));
        // poi tutta la colonna sinistra (parole 0 e 1), poi la destra (parola 2)
        assert!(order[1..13].iter().all(|(_, w)| w == &vec![0, 1]));
        assert!(order[13..].iter().all(|(_, w)| w == &vec![2]));
        assert_eq!(order[1].0, 1);
        assert_eq!(order[13].0, 1);
    }

    #[test]
    fn a_word_grazing_the_gutter_does_not_break_columns() {
        let mut lines = Vec::new();
        for i in 0..12 {
            let y = i as f32 * 30.0;
            lines.push(vec![[40.0, y, 470.0, y + 20.0], [520.0, y, 980.0, y + 20.0]]);
        }
        // riga 5: la parola di destra parte un po' prima della fascia centrale
        lines[5][1] = [490.0, 150.0, 980.0, 170.0];
        // riga 7: due righe unite dal rilevatore, con una parola ben dentro la fascia
        lines[7] = vec![[40.0, 210.0, 300.0, 230.0], [380.0, 210.0, 600.0, 230.0], [620.0, 210.0, 980.0, 230.0]];
        let order = reading_order(&lines, 1000.0);
        assert_eq!(order.len(), 24);
        assert!(order[..12].iter().all(|(_, w)| w[0] == 0), "{order:?}");
        assert!(order[12..].iter().all(|(_, w)| w[0] != 0), "{order:?}");
    }

    #[test]
    fn single_column_keeps_lines_whole() {
        let lines: Vec<Vec<WordBox>> =
            (0..30).map(|i| vec![[40.0, i as f32 * 30.0, 900.0, i as f32 * 30.0 + 20.0]]).collect();
        let order = reading_order(&lines, 1000.0);
        assert_eq!(order.len(), 30);
        assert!(order.iter().enumerate().all(|(i, (l, _))| *l == i));
    }

    #[test]
    fn accents_come_back() {
        assert_eq!(
            fix_accents("Il gruppo ? basico perch? la velocit? cresce, pi? o meno. Qual ? la citt? pi? vicina?"),
            "Il gruppo è basico perché la velocità cresce, più o meno. Qual è la città più vicina?"
        );
        assert_eq!(fix_accents("Perch? no? Si pu?."), "Perché no? Si può.");
        // il vero punto di domanda resta
        assert_eq!(fix_accents("Quale delle seguenti ? INCORRETTA?"), "Quale delle seguenti è INCORRETTA?");
    }
}
