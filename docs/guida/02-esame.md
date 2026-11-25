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
      "icon": "bolt", "color": "#0B57D0",
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
- `exam` (facoltativo): regole della simulazione d'esame, **per materia**. Senza `exam` (o con 0 quesiti) l'app non propone simulazioni, solo allenamento. Contiene: numero di quesiti, quanti a risposta multipla (`mcq`) e quanti a completamento (`fill`), opzioni per domanda, minuti, punti per risposta giusta, sbagliata e omessa, soglia di superamento, date degli appelli (`AAAA-MM-GG`).
- `sections` (facoltativo): schede informative nelle Opzioni: titolo, riassunto di una riga e testo completo.
- `official` (facoltativo): prove ufficiali da rifare così com'erano. Le loro domande hanno id `<materia>-<tag>-…`, nell'ordine della prova.
- `subjects`: le materie, ognuna con unità e argomenti.
  - `short` (facoltativo): nome breve per le schede in alto, se il nome intero è lungo.
  - `tools` (facoltativo): strumenti dell'app per la materia; oggi c'è `"tavola-periodica"`.
  - `links` (facoltativo): testi del pacchetto da aprire al volo, come una dispensa di nomenclatura: scorciatoie a capitoli dei libri (stessi campi di `ref`, capitolo 3.4) con un'etichetta breve.
  - In Studia restano in vista solo Quiz, Flashcard e Simulazione; `tools`, `links` e «Crea con l'AI» stanno nel foglio **Strumenti** della materia, e ognuno si apre come pagina a sé. Le prove ufficiali (`official`) si scelgono dentro Simulazione: non vanno tra i `links`.
  - `cfu`: crediti dell'unità; pesano le unità nella stima della preparazione.
  - `icon` (facoltativo): il nome di un simbolo incluso nell'app, dalla lista qui sotto.
  - `icon_svg` (facoltativo): un'icona tua, come tracciato SVG a una sola tinta nel riquadro 24×24 (solo il valore dell'attributo `d`, al massimo 2048 caratteri, es. `"M2 12h4l2.5-6 4 12 3-9 1.5 3H22"`). Ha la precedenza su `icon`.
  - `color` (facoltativo): il colore della materia, `#RRGGBB`; l'app ne ricava i toni chiari e scuri.

Sono tutti facoltativi e i pacchetti fatti prima restano validi: senza `icon` né `icon_svg` l'app mostra le iniziali del nome breve (es. «Cu» per Cultura generale), senza `color` i colori seguono l'ordine delle materie nel file. Un nome sconosciuto o un tracciato non valido non bloccano nulla: il controllo del pacchetto lo segnala e l'app usa le iniziali.

Simboli disponibili per `icon`: `accessible`, `account-balance`, `agriculture`, `air`, `analytics`, `architecture`, `article`, `attach-money`, `auto-stories`, `balance`, `bar-chart`, `bedtime`, `biotech`, `bloodtype`, `bolt`, `book`, `brush`, `bug-report`, `business-center`, `calculate`, `campaign`, `cardiology`, `castle`, `category`, `chat`, `child-care`, `church`, `cloud`, `code`, `computer`, `construction`, `coronavirus`, `data-object`, `database`, `dentistry`, `dermatology`, `description`, `device-thermostat`, `directions-car`, `directions-run`, `diversity-3`, `draw`, `ecg-heart`, `eco`, `edit-note`, `egg`, `elderly`, `electric-bolt`, `emergency`, `emoji-nature`, `energy-savings-leaf`, `engineering`, `euro`, `experiment`, `explore`, `extension`, `factory`, `family-restroom`, `favorite`, `fitness-center`, `flag`, `flight`, `forest`, `forum`, `function`, `functions`, `gastroenterology`, `gavel`, `genetics`, `globe`, `grass`, `groups`, `handshake`, `healing`, `health-and-safety`, `help`, `hematology`, `history-edu`, `hub`, `immunology`, `interests`, `kitchen`, `labs`, `lan`, `landscape`, `language`, `leaderboard`, `library-books`, `lightbulb`, `local-hospital`, `map`, `medical-services`, `medication`, `memory`, `menu-book`, `microbiology`, `monitor-heart`, `mood`, `mosque`, `movie`, `museum`, `music-note`, `nature`, `nephrology`, `neurology`, `newspaper`, `nutrition`, `oncology`, `ophthalmology`, `orthopedics`, `palette`, `park`, `payments`, `person`, `pets`, `photo-camera`, `piano`, `pie-chart`, `pill`, `policy`, `precision-manufacturing`, `pregnant-woman`, `psychology`, `psychology-alt`, `public`, `pulmonology`, `query-stats`, `quiz`, `radar`, `restaurant`, `rocket-launch`, `router`, `sailing`, `satellite-alt`, `savings`, `school`, `science`, `self-improvement`, `sentiment-satisfied`, `shapes`, `shopping-cart`, `show-chart`, `skeleton`, `solar-power`, `sports-basketball`, `sports-soccer`, `square-foot`, `star`, `stethoscope`, `store`, `straighten`, `sunny`, `synagogue`, `target`, `temple-buddhist`, `terminal`, `theater-comedy`, `thermometer`, `thermostat`, `train`, `translate`, `travel-explore`, `trending-up`, `vaccines`, `water`, `water-drop`, `waves`, `wind-power`, `work`.

## 2.3 Consigli

- Prendi le unità e gli argomenti dal programma ufficiale del corso: è anche ciò che chiederai all'AI di rispettare.
- Argomenti né troppo larghi né troppo stretti: 3–8 per unità vanno bene. La stima della preparazione lavora per argomento.
