# Formati dei file

## Esame — `content/syllabus.json`

Ogni pacchetto contenuti descrive **un esame**. L'app tiene un archivio separato per esame
(contenuti e progressi in `exams/<id>/` nella cartella dati) e lo sceglie dal selettore in Home.

```json
{ "schema": 1, "id": "semestre-aperto-medicina", "title": "Semestre aperto Medicina 2026/27",
  "exam": { "questions": 31, "mcq": 21, "fill": 10, "options": 5, "minutes": 50,
            "correct": 1.0, "wrong": -0.1, "omitted": 0.0, "pass": 18.0,
            "dates": [ { "label": "1° appello", "date": "2026-12-10" } ] },
  "subjects": [ … ] }
```

- `id`: identificativo **stabile** dell'esame (minuscole, cifre, trattini). Importando un pacchetto
  con un `id` già installato se ne aggiornano i contenuti; con un `id` nuovo si aggiunge un esame.
  Se manca viene ricavato dal titolo, quindi cambiare il titolo creerebbe un esame diverso:
  meglio indicarlo sempre.
- `title`: nome mostrato nel selettore.

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

- `group` (facoltativo): nome della fonte nel filtro «Fonti» del quiz (es. "Eserciziario EdiSES",
  "Slide del prof. Rossi"); se manca viene ricavato da `source`.
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

## Sessioni — `progress/sessions/AAAA-MM/<id>.json` (scritte dall'app)

```json
{ "schema": 1, "id": "20261110T182205Z-fis-exam-1a2b", "kind": "exam",
  "subject": "fis", "unit": null, "started_at": "…", "ended_at": "…", "app_version": "0.1.7+7-abc1234",
  "device": "android",
  "items": [ { "id": "fis-u2-q001", "topic": "fis-u2-t1", "kind": "q", "given": "C", "correct": true, "unsure": true, "secs": 41 } ],
  "score": { "correct": 20, "wrong": 6, "omitted": 5, "unsure": 4, "points": 19.4, "max": 31.0, "passed": true } }
```

`kind`: `practice` · `exam` · `review` · `flashcards` (item con `grade` 0–3) · `flags` (item con `flag` true/false).

## Revisione AI — `progress/assessment.json` (scritta da Claude)

```json
{ "schema": 1, "generated_at": "2026-11-12T20:00:00Z", "generated_by": "Claude (chat)",
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

## Feedback — `progress/feedback/AAAA-MM/<id>.json` (scritti dall'app)

```json
{ "schema": 1, "id": "20261110T181205Z-fb-90ab", "created_at": "…", "kind": "session",
  "category": "quiz", "session": "20261110T…-fis-practice-1a2b", "context": "quiz Fisica: 7/10 corrette · U2 Meccanica",
  "item": null, "rating": 4, "text": "Troppo facili le domande di cinematica", "app_version": "…" }
```

`kind`: `session` (fine quiz/simulazione/ripasso, con `rating` 1–5) · `general` (scheda Feedback, `category`
contenuti/domanda/app/altro) · `question` (segnalazione su una domanda, `item` = id).

## Risposte ai feedback — `progress/feedback_status.json` (scritto da Claude)

```json
{ "schema": 1, "items": {
  "20261110T181205Z-fb-90ab": { "status": "integrato", "reply": "Aggiunte 8 domande più difficili di cinematica.",
                                "updated_at": "2026-11-11T09:00:00Z" } } }
```

`status`: `letto` · `in lavorazione` · `integrato` · `non applicabile`. L'app mostra stato e risposta nella scheda Feedback.
