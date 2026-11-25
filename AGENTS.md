# Istruzioni per l'AI (Claude, ChatGPT, Gemini, …)

Questo ramo è **un esame** per l'app Tremendous Preparator
(<https://github.com/NerNerone/tremendouspreparator>): il pacchetto dei contenuti (`content/`,
`books/`) e i progressi di chi studia (`progress/`, li scrive l'app). Rispondi nella lingua
dell'utente.

Il tuo compito di solito è uno di questi tre:

1. **preparare o ampliare il pacchetto** dal materiale dell'utente (libri, dispense, slide,
   appunti, prove degli anni passati);
2. **«rivedi i miei progressi»**: una revisione dello studio fatto finora;
3. **«leggi i feedback»**: correggere le domande segnalate dall'app.

## Primo passo, sempre: aggiornati dal modello

Prima di qualunque altra cosa (anche prima di `git pull`), se questo repository ha il remoto
`modello` o è un repository privato creato dal modello, porta qui i commit del ramo `modello-dati`
del repository dell'app (comandi in «Aggiornare dal modello», in fondo). Non è facoltativo: guida,
formati, schemi, prompt e queste stesse istruzioni cambiano lì. Se il remoto manca, aggiungilo; se
il fetch non riesce (rete, permessi), dillo all'utente e lavora con le istruzioni che hai, senza
inventare formati.

## Regole

- **Il repository deve essere privato.** Dentro ci sono progressi ed elenco dei gruppi con i loro
  codici: se è pubblico, fermati e chiedi all'utente di renderlo privato (Settings › General ›
  Change visibility). L'app stessa non sincronizza con un repository pubblico.
- **Un repository privato, un ramo per esame.** Se l'utente ha già il suo repository privato dei
  dati, un esame nuovo è un **ramo nuovo** lì (dal modello: `git switch -c <esame>
  modello/modello-dati`), non un repository nuovo.
- **Aggiornati dal modello, non ricominciare da zero.** Guida, schemi, strumenti e queste
  istruzioni arrivano dal ramo `modello-dati` del repository dell'app: non riclonarlo e non
  ricreare il repository, unisci i suoi commit (vedi «Aggiornare dal modello» qui sotto).
- Qui ci sono **solo dati**: niente codice dell'app, niente workflow (`.github/`).
- **Non modificare né cancellare** i file in `progress/`: sono lo storico delle risposte e li
  scrive l'app. Le uniche eccezioni sono `progress/assessment.json` (revisione) e
  `progress/feedback_status.json` (risposte ai feedback).
- Prima di lavorare `git pull`; prima di ogni push `git pull --rebase` (l'app committa qui).
- **Id stabili e mai riusati**: le sessioni li referenziano. Una domanda sbagliata si corregge
  mantenendo l'id solo se il significato non cambia; altrimenti nuovo id.
- Il materiale protetto da diritto d'autore resta in un repository **privato** e per uso
  personale.
- Il pacchetto d'esempio (`content/*/esempio.json` e l'esame «esempio» del syllabus) va
  sostituito con l'esame vero: cancellalo quando scrivi il tuo `content/syllabus.json`.

## Dove sono le istruzioni complete

- `docs/guida/` — la guida dell'app, in ordine: `00-con-l-ai.md` è il percorso completo dal
  materiale grezzo al pacchetto; `08-prompt.md` ha i prompt pronti per ogni file.
- `docs/FORMATI.md` — tutti i formati (syllabus, quiz, flashcard, libri, progressi, revisione).
- `docs/schema/*.schema.json` — gli schemi JSON per controllare i file.

Leggili prima di scrivere un file: i campi e i vincoli sono lì.

## Preparare il pacchetto

1. **Struttura** — `content/syllabus.json` dal programma ufficiale: materie (con `icon` o
   `icon_svg` e `color`; strumenti della materia in `tools` e testi da aprire al volo in `links`,
   che l'app mostra nel foglio «Strumenti» di Studia, mai mescolati a quiz e simulazioni), unità (con i CFU, che pesano la preparazione), argomenti con id `mat-u1-t1`,
   regole della prova (`exam`: quesiti, risposta multipla e completamento, opzioni, minuti,
   punteggi, soglia) e date degli appelli. È il primo file: gli altri usano i suoi id.
2. **Testo dei libri** — prima il testo, poi l'AI:
   - PDF con testo selezionabile o scansioni: `python3 -I tools/pdf2md.py libro.pdf raw.md
     --offset N` (serve `poppler-utils`; per le scansioni passa da solo all'OCR con
     `tesseract-ocr` e `tesseract-ocr-ita`). `--offset` allinea le pagine del PDF ai numeri
     stampati, così i rimandi «Se vuoi approfondire» portano alla pagina giusta.
   - In alternativa, nell'app: Tu › App › Pacchetto contenuti › Gestisci contenuti › Strumenti PDF › «Estrai il
     testo» o «Testo con l'OCR».
   - Ripulisci senza riassumere: titoli `#`/`##`/`###` con la numerazione del libro, elenchi,
     tabelle Markdown, formule leggibili (pedici e apici Unicode), **mantenendo i segnaposto
     `<!-- p. N -->`**.
   - `python3 -I tools/import_md.py libro.md books/<materia>/<libro>` divide in un file per
     capitolo (`NN-titolo.md`); registra il libro in `content/books.json` con gli argomenti del
     syllabus di ogni capitolo.
   - Formule a blocco (`$$…$$`) come SVG: `npm --prefix tools install && node tools/mathsvg.mjs`
     dalla radice del ramo (scrive `books/_math/`).
3. **Quiz** — `content/questions/<materia>-<fonte>.json`, un capitolo alla volta. Proporzione
   come all'esame (da `exam.mcq` e `exam.fill`, di solito ~2 risposte multiple ogni
   completamento). Risposta multipla: il numero di opzioni di `exam.options`, tutte plausibili,
   una sola corretta. Completamento: risposta breve e non ambigua con le varianti in `answers`,
   oppure `numeric` con tolleranza. **Spiegazione sempre**; `source` (da dove viene) e `ref`
   (sezione e pagina del libro) per il riquadro «Se vuoi approfondire».
4. **Flashcard** — `content/flashcards/<materia>-<fonte>.json`: una carta per definizione,
   formula, legge o eccezione da ricordare; retro breve.
5. **Priorità** — prima le unità con più CFU e gli argomenti senza domande.
6. **Verifica** — con lo strumento `prep` del repository dell'app:

   ```bash
   git clone https://github.com/NerNerone/tremendouspreparator ../tp   # una volta
   cargo run -q --manifest-path ../tp/Cargo.toml -p prep-core --bin prep -- validate .
   cargo run -q --manifest-path ../tp/Cargo.toml -p prep-core --bin prep -- check .
   ```

   `validate` deve dare **0 avvisi** (controlla anche che i `ref` puntino a file e sezioni
   esistenti); `check` segnala errori e consigli di qualità. Senza Rust: nell'app, Tu › App › Pacchetto
   contenuti › Gestisci contenuti › «Controllo dei contenuti» controlla i file importati.
7. **Commit e push** — l'app scarica il pacchetto alla sincronizzazione successiva.

## «Rivedi i miei progressi»

1. `git pull`, poi `prep -- report .` (e `prep -- report . --json` per il dettaglio).
2. Leggi le sessioni recenti in `progress/sessions/`: soprattutto risposte sbagliate, omesse e
   segnate insicure (`unsure: true`) e le simulazioni. Guarda il testo delle domande in
   `content/questions/` per capire *che tipo* di errore è (concetto, calcolo, unità di misura,
   distrazione). Usa la sezione **Tempi** del report: segnala gli argomenti lenti rispetto al
   ritmo d'esame anche quando l'accuratezza è buona. Guarda anche gli obiettivi confermati in
   `progress/goals/<giorno>/` (formato in `docs/FORMATI.md`): cosa si era deciso di fare e quanto è
   stato fatto.
3. Leggi le spiegazioni scritte sulle flashcard e confrontale con il retro: segnala gli errori
   concettuali ricorrenti.
4. Scrivi `progress/assessment.json` (formato in `docs/FORMATI.md`): `generated_at` adesso (UTC),
   `mastery` 0..1 solo dove hai evidenza (le risposte insicure ma giuste valgono poco, una
   simulazione pesa più di un allenamento), `summary` di 2–3 frasi, 3–6 `priorities`, un `plan`
   concreto per i prossimi giorni rispetto alla data d'esame.
5. Riassumi all'utente cosa va, cosa no e cosa fare; commit `progress: revisione AI <data>` e push.

## «Leggi i feedback»

1. `git pull`, poi `prep -- feedback .` (solo quelli senza risposta).
2. Per ognuno correggi testo, risposta o spiegazione della domanda segnalata (stesso id se il
   senso non cambia), o aggiungi i contenuti richiesti.
3. Aggiorna `progress/feedback_status.json` con `status` e una `reply` breve; commit e push.

## Aggiornare dal modello

All'inizio di ogni lavoro (e quando l'utente lo chiede) porta qui le novità del modello:

```bash
git remote get-url modello 2>/dev/null || git remote add modello https://github.com/NerNerone/tremendouspreparator
git fetch modello modello-dati
git merge modello/modello-dati            # la prima volta, se il ramo l'ha creato l'app: --allow-unrelated-histories
```

- In caso di conflitto, per `docs/`, `tools/`, `CLAUDE.md`, `AGENTS.md` e `README.md` tieni la
  versione del modello (`git checkout --theirs <file>`); `content/`, `books/` e `progress/` sono
  dell'esame: non vanno mai sostituiti con quelli del modello (il pacchetto d'esempio no).
- Se il modello aggiunge `content/*/esempio.json` e l'esame ha già il suo syllabus, toglili.
- Poi `git pull --rebase` e push, come sempre.
