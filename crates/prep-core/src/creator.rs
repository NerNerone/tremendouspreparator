//! Creatore di pacchetti guidato: un prompt pronto per ogni file (esame, capitolo, quiz,
//! flashcard, verifica) da dare a un'AI insieme al proprio materiale, e la lettura della sua
//! risposta (blocchi di codice, percorsi, dubbi) per salvarla nel file giusto.

use crate::model::{Flashcard, FlashcardFile, Question, QuestionFile, Syllabus};

/// Cosa contiene un file del pacchetto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Syllabus,
    Chapter,
    Questions,
    Cards,
}

/// Risposta dell'AI ridotta all'essenziale.
#[derive(Debug, Default, PartialEq)]
pub struct Reply {
    /// contenuti dei blocchi di codice (o tutto il testo, se non ce ne sono)
    pub blocks: Vec<String>,
    /// «DUBBI: …» scritti dall'AI fuori dai blocchi
    pub doubts: String,
}

/// Separa i blocchi di codice (```…```), i percorsi e i dubbi dal resto della risposta.
pub fn read_reply(text: &str) -> Reply {
    let mut r = Reply::default();
    let mut cur: Option<Vec<&str>> = None;
    let mut outside: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            match cur.take() {
                Some(b) => r.blocks.push(b.join("\n").trim().to_string()),
                None => cur = Some(Vec::new()),
            }
            continue;
        }
        match cur.as_mut() {
            Some(b) => b.push(line),
            None => outside.push(line),
        }
    }
    // blocco lasciato aperto (risposta interrotta): si tiene quello che c'è
    if let Some(b) = cur {
        r.blocks.push(b.join("\n").trim().to_string());
    }
    if let Some(i) = outside.iter().position(|l| l.trim_start().to_uppercase().starts_with("DUBBI")) {
        let first = outside[i].trim_start();
        let rest = first.split_once(':').map(|x| x.1).unwrap_or("").trim();
        let mut d: Vec<&str> = if rest.is_empty() { vec![] } else { vec![rest] };
        d.extend(outside[i + 1..].iter().map(|l| l.trim()).filter(|l| !l.is_empty()));
        r.doubts = d.join("\n");
        if r.doubts.to_lowercase().starts_with("nessun") {
            r.doubts.clear();
        }
    }
    if r.blocks.is_empty() {
        // niente blocchi: il testo intero, senza la parte dei dubbi
        let body: Vec<&str> = match outside.iter().position(|l| l.trim_start().to_uppercase().starts_with("DUBBI")) {
            Some(i) => outside[..i].to_vec(),
            None => outside,
        };
        let t = body.join("\n").trim().to_string();
        if !t.is_empty() {
            r.blocks.push(t);
        }
    }
    r.blocks.retain(|b| !b.is_empty());
    r
}

/// Il testo da salvare per un file del tipo `kind` dalla risposta `reply` (blocchi uniti:
/// per quiz e flashcard tutte le domande o carte dei vari blocchi, anche se l'AI ha mandato solo
/// l'elenco). Per l'esame fissa l'id a `exam_id`.
pub fn to_file(kind: FileKind, reply: &Reply, exam_id: &str) -> Result<String, String> {
    let first = reply.blocks.first().ok_or("la risposta è vuota: incolla tutto il testo dell'AI")?;
    match kind {
        FileKind::Chapter => {
            let md = reply.blocks.iter().max_by_key(|b| b.len()).unwrap_or(first);
            if !md.lines().any(|l| l.starts_with('#')) {
                return Err("manca il titolo del capitolo (una riga che inizia con #)".into());
            }
            Ok(format!("{}\n", md.trim()))
        }
        FileKind::Syllabus => {
            let mut v: serde_json::Value =
                serde_json::from_str(clean_json(first)).map_err(|e| format!("JSON dell'esame non valido: {e}"))?;
            if let Some(o) = v.as_object_mut() {
                o.insert("id".into(), exam_id.into());
                o.entry("schema").or_insert(1.into());
            }
            let s: Syllabus = serde_json::from_value(v.clone()).map_err(|e| format!("esame incompleto: {e}"))?;
            if s.subjects.is_empty() {
                return Err("l'esame non ha materie".into());
            }
            serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
        }
        FileKind::Questions => {
            let mut all: Vec<Question> = Vec::new();
            for b in &reply.blocks {
                let v: serde_json::Value =
                    serde_json::from_str(clean_json(b)).map_err(|e| format!("JSON delle domande non valido: {e}"))?;
                let list = if v.is_array() { v } else { v["questions"].clone() };
                let qs: Vec<Question> = serde_json::from_value(list).map_err(|e| format!("domande non valide: {e}"))?;
                all.extend(qs);
            }
            if all.is_empty() {
                return Err("nessuna domanda nella risposta".into());
            }
            serde_json::to_string_pretty(&QuestionFile { schema: 1, questions: all }).map_err(|e| e.to_string())
        }
        FileKind::Cards => {
            let mut all: Vec<Flashcard> = Vec::new();
            for b in &reply.blocks {
                let v: serde_json::Value =
                    serde_json::from_str(clean_json(b)).map_err(|e| format!("JSON delle flashcard non valido: {e}"))?;
                let list = if v.is_array() { v } else { v["cards"].clone() };
                let cs: Vec<Flashcard> =
                    serde_json::from_value(list).map_err(|e| format!("flashcard non valide: {e}"))?;
                all.extend(cs);
            }
            if all.is_empty() {
                return Err("nessuna flashcard nella risposta".into());
            }
            serde_json::to_string_pretty(&FlashcardFile { schema: 1, cards: all }).map_err(|e| e.to_string())
        }
    }
}

/// La figura SVG nella risposta dell'AI (da `<svg` a `</svg>`).
pub fn to_svg(text: &str) -> Result<String, String> {
    let start = text.find("<svg").ok_or("nella risposta non c'è un'immagine SVG (<svg …>)")?;
    let end = text[start..].rfind("</svg>").map(|i| start + i + 6).ok_or("l'SVG è incompleto (manca </svg>)")?;
    let svg = &text[start..end];
    if svg.contains("<script") || svg.contains("<image") || svg.contains("href=\"http") {
        return Err("l'SVG contiene script o immagini esterne: chiedi all'AI una versione con sole forme".into());
    }
    let svg = if svg.contains("xmlns=") {
        svg.to_string()
    } else {
        svg.replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1)
    };
    Ok(format!("{svg}\n"))
}

/// Prompt per ricomporre una figura di un capitolo come SVG.
pub fn prompt_figure(caption: &str, chapter: &str) -> String {
    format!(
        "Ti allego una figura del capitolo «{chapter}» (didascalia: «{caption}»). Ricreala come immagine SVG semplice e leggibile per un'app su telefono (Tremendous Preparator).

{RULES}

ISTRUZIONI:
- SVG valido e autonomo: viewBox=\"0 0 400 300\" (adatta le proporzioni alla figura), xmlns=\"http://www.w3.org/2000/svg\", nessuna immagine esterna, nessuno script, font-family=\"sans-serif\".
- Solo forme essenziali: linee, frecce, rettangoli, cerchi, percorsi semplici; etichette di testo brevi (almeno 14 px); pochi colori ben distinguibili su sfondo bianco.
- Tutte le etichette, i valori e gli assi dell'originale devono esserci e stare al posto giusto; non aggiungere informazioni che non ci sono.
- Grafici: assi con nome e unità, tacche principali con i valori, curve con la forma corretta.
- Se la figura è una foto o è troppo complessa, scrivi in «DUBBI:» una descrizione di 3-5 righe da mettere nel capitolo al suo posto, e disegna solo uno schema semplice.
- Rispondi con un solo blocco ```svg, poi «DUBBI:».

FIGURA: [allega la foto o il ritaglio della pagina con la figura]"
    )
}

/// JSON senza quello che a volte l'AI mette attorno (testo prima della prima graffa o quadra).
fn clean_json(s: &str) -> &str {
    let s = s.trim();
    let start = s.find(['{', '[']).unwrap_or(0);
    let end = s.rfind(['}', ']']).map(|i| i + 1).unwrap_or(s.len());
    if start < end {
        &s[start..end]
    } else {
        s
    }
}

/// Regole che valgono per ogni prompt.
const RULES: &str = "REGOLE OBBLIGATORIE (valgono più di qualunque altra indicazione):
1. Fonte unica: usa SOLO il materiale che ti allego o incollo. Non aggiungere fatti, numeri, esempi o citazioni che non ci sono; le tue conoscenze servono solo a controllare e correggere.
2. Niente invenzioni: se manca qualcosa scrivi [MANCA: cosa] o chiedimelo.
3. Formato esatto: rispetta alla lettera campi, nomi e tipi qui sotto, senza aggiungerne né toglierne. JSON valido (virgolette doppie, nessun commento, nessuna virgola finale).
4. Risposta: SOLO un blocco di codice con il file, poi una riga «DUBBI:» con l'elenco di cose incerte (o «DUBBI: nessuno»). Nessun altro testo.
5. Lingua: italiano corretto, terminologia della materia, simboli e unità corretti (m/s², 10⁻³, °C).";

/// Come si muove la barra di preparazione (anche per chi crea i contenuti).
pub const BAR: &str = "Per ogni argomento la preparazione è l'accuratezza recente (ultime 3 risposte; una risposta insicura vale metà) moltiplicata per la copertura (domande diverse viste, fino a 6). Un'unità è la media dei suoi argomenti, una materia la media delle unità pesata per i crediti (CFU). Per questo ogni argomento dovrebbe avere almeno 6 domande diverse: con meno la barra non può arrivare in fondo.";

/// Prompt per la definizione dell'esame (content/syllabus.json).
pub fn prompt_syllabus(exam_id: &str, exam_name: &str) -> String {
    format!(
        "Sei un assistente che prepara un pacchetto di studio per l'app Tremendous Preparator. Ti allego il programma ufficiale dell'esame «{exam_name}». Costruisci la definizione dell'esame.

{RULES}

FORMATO (un solo blocco ```json):
{{ \"schema\": 1, \"id\": \"{exam_id}\", \"title\": \"{exam_name}\",
  \"exam\": {{ \"questions\": N, \"mcq\": N, \"fill\": N, \"options\": 5, \"minutes\": N,
            \"correct\": 1.0, \"wrong\": -0.25, \"omitted\": 0.0, \"pass\": N,
            \"dates\": [ {{ \"label\": \"1° appello\", \"date\": \"AAAA-MM-GG\" }} ] }},
  \"sections\": [ {{ \"title\": \"Regole d'esame\", \"summary\": \"una riga\", \"body\": \"testo\" }} ],
  \"subjects\": [ {{ \"id\": \"sigla\", \"name\": \"Materia\", \"short\": \"Nome breve\",
                  \"icon\": \"simbolo\", \"color\": \"#RRGGBB\",
                  \"units\": [ {{ \"id\": \"sigla-u1\", \"n\": 1, \"name\": \"Unità\", \"cfu\": N,
                               \"topics\": [ {{ \"id\": \"sigla-u1-t1\", \"name\": \"Argomento\" }} ] }} ] }} ] }}

ISTRUZIONI:
- \"id\" e \"title\" restano esattamente come sopra.
- Materie, unità e argomenti seguono l'ordine e i titoli del programma. Id: materia = sigla di 3-4 lettere minuscole; unità <materia>-uN; argomenti <unità>-tN.
- Da 3 a 8 argomenti per unità, ciascuno abbastanza specifico da farci 6-15 domande.
- \"cfu\": i crediti dell'unità se il programma li indica, altrimenti 1.
- \"exam\": SOLO se il programma descrive una prova scritta (numero di domande, minuti, punteggi). Se non c'è, togli del tutto la chiave \"exam\": l'app non proporrà simulazioni. Valori per UNA materia.
- \"sections\": le regole della prova che trovi nel testo; se non ce ne sono togli la chiave.
- \"icon\" e \"color\" (facoltativi): per ogni materia un simbolo adatto scelto SOLO da questo elenco: {icons}. Se nessuno va bene togli \"icon\" (l'app mostrerà le iniziali); non inventare nomi. \"color\": un colore #RRGGBB ben distinto da quello delle altre materie.

PROGRAMMA: [allega o incolla qui il programma]",
        icons = crate::icons::names().collect::<Vec<_>>().join(", ")
    )
}

/// Prompt per ripulire un capitolo (testo da PDF o OCR) in Markdown.
pub fn prompt_chapter(title: &str) -> String {
    format!(
        "Ti do il testo di un capitolo («{title}») estratto da un PDF, anche con l'OCR, quindi con errori. Riscrivilo in Markdown pulito per il lettore di libri dell'app Tremendous Preparator.

{RULES}

ISTRUZIONI:
- Revisione grammaticale (ortografia, accenti, parole spezzate a fine riga, lettere confuse dall'OCR come l/1, O/0, rn/m), semantica (termini della materia, simboli, unità) e logica (frasi e formule coerenti). NON riassumere, NON togliere, NON aggiungere contenuti, NON riordinare.
- Titoli con la numerazione del libro: \"# 2 Titolo del capitolo\", \"## 2.1 Sezione\", \"### 2.1.1 Sottosezione\". La prima riga è il titolo del capitolo.
- Mantieni ESATTAMENTE i segnaposto di pagina \"<!-- p. N -->\" dove sono.
- Togli intestazioni e piè di pagina ripetuti e numeri di pagina isolati.
- Elenchi con \"-\" o \"1.\", tabelle in Markdown, riquadri del libro come \"> [!NOTE] Titolo\" seguito da righe \"> \".
- Formule in LaTeX: \"$...$\" nel testo, \"$$\" su righe proprie per quelle grandi. Ricostruisci apici e pedici persi (\"10-4\" → \"$10^{{-4}}$\", \"H2O\" → \"H₂O\").
- Al posto di ogni figura scrivi \"![Figura N: didascalia originale]()\".
- Passaggi illeggibili: \"[illeggibile]\".
- Rispondi con un solo blocco ```markdown con tutto il capitolo, poi «DUBBI:».

TESTO: [allega il file .md estratto con Strumenti PDF, o incolla qui il testo]"
    )
}

/// Argomenti dell'esame (tutti o di alcune unità) come righe «id: nome (unità)».
pub fn topic_lines(syl: &Syllabus, units: &[String]) -> String {
    let mut out = Vec::new();
    for s in &syl.subjects {
        for u in &s.units {
            if !units.is_empty() && !units.contains(&u.id) {
                continue;
            }
            for t in &u.topics {
                out.push(format!("{}: {} ({} › {})", t.id, t.name, s.name, u.name));
            }
        }
    }
    out.join("\n")
}

/// Prompt per un file di quiz: `n` domande dal materiale allegato, sugli argomenti indicati,
/// con id `<prefix>-q<numero>` a partire da `start`; con `notes` una nota per ogni risposta sbagliata.
pub fn prompt_questions(topics: &str, n: u32, notes: bool, prefix: &str, start: u32, source: &str) -> String {
    let fill = (n / 3).max(if n >= 3 { 1 } else { 0 });
    let mcq = n - fill;
    let expl = if notes {
        "\"explanation\": il ragionamento in 1-3 frasi, poi per OGNI opzione sbagliata una riga separata da \"\\n\" nel formato \"• B) testo dell'opzione: perché è sbagliata\" (una frase). Le lettere seguono l'ordine di \"options\" (A = la prima). Più opzioni con lo stesso motivo: \"• A), C) e E): motivo\"."
    } else {
        "\"explanation\": il ragionamento o il calcolo che porta alla risposta, in 1-3 frasi."
    };
    format!(
        "Sei un docente che scrive domande d'esame per l'app Tremendous Preparator. Dal MATERIALE che ti allego scrivi ESATTAMENTE {n} domande: {mcq} a risposta multipla e {fill} a completamento.

{RULES}
6. Correttezza: per ogni domanda risolvi tu il quesito PRIMA di scrivere la chiave; una sola opzione corretta; ricontrolla calcoli, unità e ordini di grandezza.

FORMATO (un solo blocco ```json):
{{ \"schema\": 1, \"questions\": [
  {{ \"id\": \"{prefix}-q{start:03}\", \"topic\": \"id argomento\", \"type\": \"mcq\", \"difficulty\": 2,
    \"text\": \"...\", \"options\": [\"...\", \"...\", \"...\", \"...\", \"...\"], \"answer\": 0,
    \"explanation\": \"...\", \"source\": \"{source}\" }},
  {{ \"id\": \"{prefix}-q{n2:03}\", \"topic\": \"...\", \"type\": \"fill\", \"text\": \"... ____ ...\",
    \"answers\": [\"risposta\", \"variante accettata\"], \"explanation\": \"...\", \"source\": \"{source}\" }},
  {{ \"id\": \"{prefix}-q{n3:03}\", \"topic\": \"...\", \"type\": \"fill\", \"text\": \"... ____ m/s\",
    \"numeric\": {{ \"value\": 19.6, \"tolerance\": 0.02, \"unit\": \"m/s\" }}, \"explanation\": \"...\", \"source\": \"{source}\" }}
] }}

ISTRUZIONI:
- Id: \"{prefix}-q\" e un numero di tre cifre, da {start:03} in su, senza ripetizioni.
- \"topic\": SOLO uno di questi argomenti (id: nome):
{topics}
- Distribuisci le domande sugli argomenti che il materiale tratta; per ognuno punta ad almeno 6 domande diverse (la barra di preparazione conta fino a 6 domande diverse per argomento).
- \"mcq\": esattamente 5 opzioni, una sola corretta; le altre plausibili e basate su errori tipici (unità sbagliata, formula invertita, segno, concetti vicini). Niente \"tutte/nessuna delle precedenti\". \"answer\" è la posizione della giusta CONTANDO DA 0; varia la posizione.
- \"fill\": risposta breve e non ambigua (parola, nome o numero). Numeri: \"numeric\" con unità e \"tolerance\" relativa (0.02 = 2%); parole: \"answers\" con sinonimi e grafie alternative.
- \"difficulty\": 1 facile, 2 media, 3 difficile (circa 30% / 50% / 20%).
- {expl}
- Una domanda verifica UN concetto del materiale; niente conoscenze che il materiale non contiene.
- Nel testo i numeri con la virgola decimale; nel campo \"value\" con il punto.

MATERIALE: [allega il capitolo, gli appunti o le slide]",
        n2 = start + 1,
        n3 = start + 2,
    )
}

/// Prompt per un file di flashcard.
pub fn prompt_cards(topics: &str, n: u32, prefix: &str, start: u32, source: &str) -> String {
    format!(
        "Dal MATERIALE che ti allego crea ESATTAMENTE {n} flashcard per l'app Tremendous Preparator.

{RULES}

FORMATO (un solo blocco ```json):
{{ \"schema\": 1, \"cards\": [
  {{ \"id\": \"{prefix}-c{start:03}\", \"topic\": \"id argomento\", \"front\": \"...\", \"back\": \"...\", \"source\": \"{source}\" }}
] }}

ISTRUZIONI:
- Id: \"{prefix}-c\" e un numero di tre cifre, da {start:03} in su, senza ripetizioni.
- \"topic\": SOLO uno di questi argomenti (id: nome):
{topics}
- Una carta = una sola informazione da ricordare: definizioni, formule (con il significato di ogni simbolo e l'unità), leggi, eccezioni, confronti tra concetti simili, valori da sapere a memoria.
- \"front\": domanda breve o termine, senza la risposta dentro. \"back\": la risposta in 1-3 frasi, completa e comprensibile da sola, fedele al materiale.
- Niente doppioni; preferisci quello che il materiale sottolinea e che servirebbe all'esame.

MATERIALE: [allega il capitolo, gli appunti o le slide]"
    )
}

/// Prompt per far verificare un file di quiz da un'altra AI.
pub fn prompt_review(json: &str) -> String {
    format!(
        "Sei un revisore esperto della materia e di lingua italiana. Controlla queste domande per l'app Tremendous Preparator.

{RULES}

CONTROLLI, per ogni domanda:
1. Correttezza: risolvila tu da zero PRIMA di guardare \"answer\"/\"answers\"/\"numeric\"; la chiave deve coincidere. Rifai i conti; controlla unità e ordini di grandezza.
2. Univocità: una sola opzione corretta, nessuna sbagliata difendibile. Nei completamenti, \"answers\" con le varianti accettabili.
3. Chiarezza: dati sufficienti, nessuna ambiguità, nessun indizio sulla risposta, opzioni di forma simile.
4. Spiegazione corretta e coerente con la chiave; nelle righe \"• X) …\" ogni lettera corrisponde all'opzione giusta.
5. Lingua: grammatica, ortografia, punteggiatura, accenti, terminologia, simboli e unità.
6. Formato: JSON valido; \"answer\" da 0 a 4; 5 opzioni. NON cambiare id e topic.

Rispondi con il JSON completo corretto (stessa struttura e stessi id; togli le domande irrecuperabili) in un solo blocco ```json, poi «DUBBI:» con l'elenco delle modifiche «id — cosa e perché», scrivendo CHIAVE quando cambi la risposta corretta.

DOMANDE:
```json
{json}
```"
    )
}

/// Prossimo numero libero per gli id `<prefix>-<k><numero>` (k = 'q' o 'c').
pub fn next_number<'a>(ids: impl Iterator<Item = &'a str>, prefix: &str, k: char) -> u32 {
    let head = format!("{prefix}-{k}");
    ids.filter_map(|id| id.strip_prefix(&head)?.parse::<u32>().ok()).max().map(|m| m + 1).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_replies_with_blocks_paths_and_doubts() {
        let text =
            "Ecco il file:\ncontent/questions/cin.json\n```json\n{\"schema\":1,\"questions\":[]}\n```\nDUBBI: nessuno";
        let r = read_reply(text);
        assert_eq!(r.blocks, vec!["{\"schema\":1,\"questions\":[]}"]);
        assert!(r.doubts.is_empty());
        let r = read_reply("```markdown\n# 1 Titolo\ntesto\n```\nDUBBI:\n- p. 3 illeggibile");
        assert_eq!(r.doubts, "- p. 3 illeggibile");
        // senza blocchi: il testo intero
        assert_eq!(read_reply("  [1, 2]  \n").blocks, vec!["[1, 2]"]);
        // risposta interrotta
        assert_eq!(read_reply("```json\n{\"a\":1}").blocks, vec!["{\"a\":1}"]);
    }

    #[test]
    fn turns_replies_into_files() {
        let q = r#"```json
{"schema":1,"questions":[{"id":"cin-q001","topic":"fis-u1-t1","type":"mcq","text":"?","options":["a","b","c","d","e"],"answer":2,"explanation":"x"}]}
```
```json
[{"id":"cin-q002","topic":"fis-u1-t1","type":"fill","text":"____","answers":["y"],"explanation":"z"}]
```"#;
        let f = to_file(FileKind::Questions, &read_reply(q), "e").unwrap();
        let back: QuestionFile = serde_json::from_str(&f).unwrap();
        assert_eq!(back.questions.len(), 2);
        let syl = "```json\n{\"title\":\"Fisica\",\"subjects\":[{\"id\":\"fis\",\"name\":\"Fisica\",\"units\":[{\"id\":\"fis-u1\",\"n\":1,\"name\":\"Cinematica\",\"cfu\":1,\"topics\":[{\"id\":\"fis-u1-t1\",\"name\":\"Moto\"}]}]}]}\n```";
        let f = to_file(FileKind::Syllabus, &read_reply(syl), "mio-esame").unwrap();
        let s: Syllabus = serde_json::from_str(&f).unwrap();
        assert_eq!(s.id, "mio-esame");
        assert_eq!(s.exam.questions, 0);
        assert!(to_file(FileKind::Chapter, &read_reply("solo testo"), "e").is_err());
        assert!(to_file(FileKind::Chapter, &read_reply("# 1 Capitolo\ntesto"), "e").is_ok());
        assert!(to_file(FileKind::Questions, &read_reply("non json"), "e").is_err());
        assert_eq!(next_number(["cin-q001", "cin-q007", "altro-q050"].into_iter(), "cin", 'q'), 8);
        assert_eq!(next_number(std::iter::empty(), "cin", 'c'), 1);
        let svg = to_svg("Ecco:\n```svg\n<svg viewBox=\"0 0 4 3\"><line/></svg>\n```\nDUBBI: nessuno").unwrap();
        assert!(svg.starts_with("<svg xmlns=") && svg.trim_end().ends_with("</svg>"));
        assert!(to_svg("niente").is_err());
        assert!(to_svg("<svg><script>x</script></svg>").is_err());
        let p = prompt_questions("fis-u1-t1: Moto", 30, true, "cin", 8, "Dispense");
        assert!(p.contains("ESATTAMENTE 30 domande: 20 a risposta multipla e 10 a completamento"));
        assert!(p.contains("cin-q008") && p.contains("• B)"));
    }
}
