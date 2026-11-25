# Formati dei file

Gli schemi JSON (draft 2020-12) di questi formati sono in [`schema/`](schema/): `syllabus`, `questions`,
`flashcards`, `books`. Il controllo completo (anche di qualità) si fa con `prep check <cartella>` o con
«Controlla» nella pagina Contenuti dell'app.

## Esame — `content/syllabus.json`

Ogni pacchetto contenuti descrive **un esame**. L'app tiene un archivio separato per esame
(contenuti e progressi in `exams/<id>/` nella cartella dati) e lo sceglie dal selettore in Home.

```json
{ "schema": 1, "id": "fisica-1", "title": "Fisica I — esame scritto",
  "exam": { "questions": 30, "mcq": 20, "fill": 10, "options": 5, "minutes": 60,
            "correct": 1.0, "wrong": -0.25, "omitted": 0.0, "pass": 18.0,
            "dates": [ { "label": "1° appello", "date": "2027-02-10" } ] },
  "subjects": [ … ] }
```

- `id`: identificativo **stabile** dell'esame (minuscole, cifre, trattini). Importando un pacchetto
  con un `id` già installato se ne aggiornano i contenuti; con un `id` nuovo si aggiunge un esame.
  Se manca viene ricavato dal titolo, quindi cambiare il titolo creerebbe un esame diverso:
  meglio indicarlo sempre.
- `title`: nome mostrato nel selettore.
- `exam` (facoltativo): regole della simulazione d'esame, per materia. Senza (o con `questions` 0)
  l'app non propone simulazioni, solo allenamento.
- `sections` (facoltativo): schede informative dell'esame mostrate in Tu › App, ciascuna con
  `title`, `summary` (una riga, sempre visibile) e `body` (testo completo, dietro il pulsante «i»):
  `"sections": [ { "title": "Regole d'esame", "summary": "Tre prove lo stesso giorno", "body": "…" } ]`.
- in ogni materia, facoltativi: `short`, nome breve per i selettori (es. "Chimica"); `icon`, nome di un simbolo incluso nell'app (`biotech`, `science`, `bolt`, … elenco in `crates/prep-core/src/subject_icons.rs` e nella guida 2.2); `icon_svg`, tracciato SVG 24×24 a una tinta (solo comandi di tracciato, ≤ 2048 caratteri, ha la precedenza su `icon`); `color`, `#RRGGBB`. Senza icona l'app usa le iniziali del nome breve, senza colore l'ordine delle materie; i pacchetti precedenti restano validi.
- strumenti della materia, facoltativi: `tools`, id degli strumenti della materia: quelli del pacchetto
  (`content/tools/<id>.json`, vedi sotto) e quelli inclusi nell'app (`"tavola-periodica"`); uno strumento
  del pacchetto che nessuna materia elenca vale per tutte; un id inesistente si ignora e `prep check` lo segnala; e
  `links`, testi del pacchetto da aprire al volo, con `label` e gli stessi campi di `ref` (vedi sotto):
  `"links": [ { "label": "Nomenclatura", "book": "chi-manuale", "file": "books/chimica/manuale/02-nomenclatura.md", "section": "2.1" } ]`.
  In Studia stanno nel foglio «Strumenti» (con «Crea con l'AI»), separati da Quiz, Flashcard e
  Simulazione; le prove ufficiali si scelgono dentro Simulazione e non vanno tra i `links`.
  I colori delle materie seguono il loro ordine nel file.
- `official` (facoltativo): prove ufficiali da rifare così com'erano, con le domande che hanno id
  `<materia>-<tag>-…` (in ordine): `"official": [ { "tag": "a1", "label": "1° appello 2025" } ]`.
  Si scelgono dentro Simulazione (Studia); compaiono solo quelle che hanno domande nella materia scelta.

## Domande — `content/questions/<qualsiasi>.json`

```json
{ "schema": 1, "questions": [
  { "id": "fis-u2-q001", "topic": "fis-u2-t1", "type": "mcq", "difficulty": 2, "source": "libro:fisica.md#cinematica",
    "text": "…", "options": ["…","…","…","…","…"], "answer": 2, "explanation": "…" },
  { "id": "fis-u2-q004", "topic": "fis-u2-t1", "type": "fill",
    "text": "… ____ …", "numeric": { "value": 19.6, "tolerance": 0.02, "unit": "m/s" }, "explanation": "…" },
  { "id": "fis-u3-q004", "topic": "fis-u3-t2", "type": "fill",
    "text": "… ____ …", "answers": ["Archimede"], "explanation": "…" }
]}
```

- `group` (facoltativo): nome della fonte nel filtro «Fonti» (quiz personalizzato e pagina dell'unità, valido anche
  per i quiz di argomento), es. "Eserciziario Rossi", "Slide del prof. Rossi"; se manca viene ricavato da `source`.
- `id` univoco e **stabile** (le sessioni lo referenziano): `<materia>-u<n>-q<NNN>`; i generati da un libro possono
  usare un suffisso, es. `fis-u3-b012`.
- `topic`: id di un argomento di `content/syllabus.json`.
- `mcq`: 5 opzioni, `answer` = indice 0-based.
- `fill` testuale: `answers` = varianti accettate (confronto senza maiuscole, accenti e punteggiatura ai bordi).
- `ref` (facoltativo) = rimando "Se vuoi approfondire":
  `{"book": "fis-giancoli", "file": "books/fisica/giancoli/09-fluidi.md", "section": "9.3", "page": 245}`.
  `section` può essere il titolo completo o il suo inizio (es. "9.3"); se manca, conta `page`. Senza `ref` l'app usa il
  primo capitolo di `content/books.json` che elenca l'argomento della domanda.
- `fill` numerico: `numeric.value` + `tolerance` relativa (0,02 = ±2%). L'utente può scrivere `1,5`, `3e-4`,
  `3x10^-4`, `3·10⁻⁴`, anche seguiti dall'unità.

## Flashcard — `content/flashcards/<qualsiasi>.json`

```json
{ "schema": 1, "cards": [ { "id": "bio-u3-c001", "topic": "bio-u3-t3", "front": "…", "back": "…", "source": "…" } ] }
```

## Strumenti — `content/tools/<id>.json`

Pagine da consultare (Studia › Strumenti), descritte solo con dati: l'app ne disegna tre tipi e
scarta il resto. Campi comuni: `schema` 1, `id` (minuscole, cifre, trattini; dà il nome al file),
`title`, `subtitle` e `info` facoltativi, `kind`.

- `"grid"`: `columns`, `rows` (1–40), `categories` (`name`, `color` `#RRGGBB` facoltativo),
  `column_labels`/`row_labels` facoltativi, `cells` con `x`, `y` (da 1), `label` (1–4 caratteri),
  `top`, `bottom`, `category`, e la scheda: `title`, `subtitle`, `details` (`label`/`value`; senza
  `value` è un titoletto), `text`.
- `"list"`: `sections` con `title` e `items` (`title`, `value`, `body`); si cercano nell'app.
- `"calc"`: `inputs` (`id`, `label`, `unit`, `default`) e `outputs` (`id` facoltativo, `label`,
  `unit`, `formula`, `digits`). Formule: numeri col punto, `+ - * / ^`, parentesi, gli id dei campi
  e dei risultati precedenti, `pi`, `e`, `sqrt ln log log2 exp sin cos tan asin acos atan abs round
  min max pow`; nient'altro.

Esempi in `examples/esempio/content/tools/` (tutti e tre i tipi), schema in
`docs/schema/tools.schema.json`, guida al capitolo 9. In Gestisci contenuti si copiano tra i
pacchetti e si condividono uno alla volta.

## Sessioni — `progress/sessions/AAAA-MM/<id>.json` (scritte dall'app)

```json
{ "schema": 1, "id": "20261110T182205Z-fis-exam-1a2b", "kind": "exam",
  "subject": "fis", "unit": null, "started_at": "…", "ended_at": "…", "app_version": "0.1.7+7-abc1234",
  "device": "android",
  "items": [ { "id": "fis-u2-q001", "topic": "fis-u2-t1", "kind": "q", "given": "C", "correct": true, "unsure": true, "secs": 41 } ],
  "score": { "correct": 20, "wrong": 6, "omitted": 5, "unsure": 4, "points": 19.4, "max": 31.0, "passed": true } }
```

`kind`: `practice` · `exam` · `review` · `flashcards` (item con `grade` 0–3) · `flags` (item con `flag` true/false).

## Obiettivi del giorno — `progress/goals/AAAA-MM-GG/<id>.json` (scritti dall'app)

Ogni volta che si confermano o si cambiano gli obiettivi l'app crea un file nuovo (mai sovrascritto);
vale quello con `updated_at` più recente. `topics` e `targets` sono paralleli: domande da fare oggi
per argomento. Una versione con `topics` vuoto vuol dire «obiettivi tolti».

```json
{ "date": "2026-11-25", "topics": ["bio-u2-t3", "fis-u1-t2"], "targets": [38, 38],
  "confirmed_at": "2026-11-25T07:40:00Z", "updated_at": "2026-11-25T07:40:00Z" }
```

Nella revisione servono a capire cosa si era deciso di studiare e quanto è stato fatto (le sessioni
dello stesso giorno sugli stessi argomenti).

## Valutazione esterna — `progress/assessment.json` (facoltativa)

```json
{ "schema": 1, "generated_at": "2026-11-12T20:00:00Z", "generated_by": "revisione",
  "summary": "Due-tre frasi sullo stato complessivo.",
  "subjects": { "fis": { "note": "…" } },
  "units": { "fis-u3": { "note": "…" } },
  "topics": { "fis-u3-t4": { "mastery": 0.35, "note": "confondi r² e r⁴ in Poiseuille" } },
  "priorities": [ { "topic": "fis-u3-t4", "reason": "…" } ],
  "plan": [ "Oggi: …", "Domani: …" ] }
```

## Catalogo libri — `content/books.json`

```json
{ "schema": 1, "books": [
  { "id": "fis-giancoli", "subject": "fis", "title": "Fisica — Giancoli", "author": "…",
    "chapters": [ { "file": "books/fisica/giancoli/09-fluidi.md", "title": "9 Meccanica dei fluidi",
                    "topics": ["fis-u3-t1", "fis-u3-t2", "fis-u3-t3"] } ] } ] }
```

I capitoli sono file markdown in `books/`. Segnaposto di pagina del libro originale: `<!-- p. 245 -->` (li genera
`tools/pdf2md.py`). Il lettore divide i capitoli in sezioni ai titoli `#`, `##`, `###`.
