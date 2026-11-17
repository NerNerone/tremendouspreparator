# 2 L'esame: content/syllabus.json

È il primo file da preparare: senza, l'app non sa quali materie e argomenti esistono.

## 2.1 Un esempio completo

```
{
  "schema": 1,
  "id": "fisica-1",
  "title": "Fisica I — esame scritto",
  "exam": {
    "questions": 30, "mcq": 20, "fill": 10, "options": 5, "minutes": 60,
    "correct": 1.0, "wrong": -0.25, "omitted": 0.0, "pass": 18.0,
    "dates": [ { "label": "1° appello", "date": "2027-02-10" } ]
  },
  "sections": [
    { "title": "Regole d'esame", "summary": "Una prova scritta di un'ora",
      "body": "Testo completo che si apre col pulsante «i»." }
  ],
  "official": [ { "tag": "a1", "label": "1° appello 2026" } ],
  "subjects": [
    { "id": "fis", "name": "Fisica", "short": "Fisica",
      "tools": ["tavola-periodica"],
      "links": [ { "label": "Formulario", "book": "fis-dispense",
                   "file": "books/fisica/dispense/01-formulario.md", "section": "1.1" } ],
      "units": [
        { "id": "fis-u1", "n": 1, "name": "Cinematica", "cfu": 2.0,
          "topics": [
            { "id": "fis-u1-t1", "name": "Moto rettilineo" },
            { "id": "fis-u1-t2", "name": "Moto nel piano" }
          ] }
      ] }
  ]
}
```

## 2.2 I campi

- `id`: identificativo **stabile** dell'esame (minuscole, cifre, trattini). Un pacchetto con lo stesso `id` aggiorna l'esame installato; con un `id` diverso ne aggiunge un altro.
- `title`: nome mostrato nel selettore in Home.
- `exam`: regole della simulazione, **per materia**: numero di quesiti, quanti a risposta multipla (`mcq`) e quanti a completamento (`fill`), opzioni per domanda, minuti, punti per risposta giusta, sbagliata e omessa, soglia di superamento, date degli appelli (`AAAA-MM-GG`).
- `sections` (facoltativo): schede informative nelle Opzioni: titolo, riassunto di una riga e testo completo.
- `official` (facoltativo): prove ufficiali da rifare così com'erano. Le loro domande hanno id `<materia>-<tag>-…`, nell'ordine della prova.
- `subjects`: le materie, ognuna con unità e argomenti.
  - `short` (facoltativo): nome breve per le schede in alto, se il nome intero è lungo.
  - `tools` (facoltativo): strumenti dell'app da mostrare nella pagina della materia; oggi c'è `"tavola-periodica"`.
  - `links` (facoltativo): scorciatoie a capitoli dei libri (stessi campi di `ref`, capitolo 3.4).
  - `cfu`: crediti dell'unità; pesano le unità nella stima della preparazione.

I colori delle materie seguono il loro ordine nel file.

## 2.3 Consigli

- Prendi le unità e gli argomenti dal programma ufficiale del corso: è anche ciò che chiederai all'AI di rispettare.
- Argomenti né troppo larghi né troppo stretti: 3–8 per unità vanno bene. La stima della preparazione lavora per argomento.
