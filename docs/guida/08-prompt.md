# 8 Prompt per l'AI

Questi prompt si copiano (pulsante **Copia** sotto ogni riquadro), si incollano in una chat con un'AI (ChatGPT, Claude, Gemini, …) e si completano dove c'è scritto `[…]`. Poi si salva la risposta in un file e la si aggiunge da *Gestisci contenuti*. Se preferisci dare all'AI tutti i formati insieme, copia la guida intera (prima pagina).

Regole che valgono per tutti:

- Dai all'AI **il testo vero** (capitolo, appunti, programma): incollalo o allegalo. Senza fonte inventa.
- Allega o incolla anche il tuo `syllabus.json` quando il prompt usa gli argomenti: così usa gli id giusti.
- **Controlla sempre** le risposte corrette: un'AI può sbagliare un calcolo o una chiave. Il prompt 8.3 serve a questo, meglio con un'AI diversa da quella che ha scritto le domande.
- Lavora a pezzi: un capitolo o una sezione per volta (20–30 domande), non un libro intero.
- Salva il risultato con estensione `.json` (o `.md` per i capitoli), in UTF-8. Nell'app non serve salvare: «Crea con l'AI» e Gestisci contenuti › «+» leggono direttamente la risposta incollata.
- Fai rispondere l'AI con **un solo blocco di codice per file**: l'app prende il file dai blocchi e legge le righe «DUBBI:» fuori dai blocchi.

## 8.1 Creare l'esame dal programma

```
Sei un assistente che prepara materiale di studio per un'app di quiz.
Ti do il programma ufficiale di un esame. Costruisci il file content/syllabus.json in questo formato JSON. Rispondi con il JSON valido in un solo blocco ```json; fuori dal blocco al massimo una riga «DUBBI:» con i punti incerti:

{ "schema": 1, "id": "<id-esame>", "title": "<titolo>",
  "exam": { "questions": N, "mcq": N, "fill": N, "options": 5, "minutes": N,
            "correct": 1.0, "wrong": -0.25, "omitted": 0.0, "pass": N,
            "dates": [ { "label": "1° appello", "date": "AAAA-MM-GG" } ] },
  "sections": [ { "title": "Regole d'esame", "summary": "<una riga>", "body": "<testo>" } ],
  "official": [ { "tag": "<sigla-prova>", "label": "<es. 1° appello 2025>" } ],
  "subjects": [ { "id": "<sigla>", "name": "<materia>", "short": "<nome breve>",
                  "icon": "<simbolo>", "color": "#RRGGBB",
                  "tools": ["tavola-periodica"],
                  "units": [ { "id": "<sigla>-u1", "n": 1, "name": "<unità>", "cfu": N,
                               "topics": [ { "id": "<sigla>-u1-t1", "name": "<argomento>" } ] } ] } ] }

Regole:
- id solo minuscole, cifre e trattini; materia = sigla di 3-4 lettere; unità <materia>-uN; argomenti <unità>-tN.
- Segui l'ordine e i titoli del programma; da 3 a 8 argomenti per unità, ciascuno abbastanza specifico da fare 5-15 domande.
- "exam" vale per una materia: se non trovi numeri, punteggi o date nel programma, usa questi: [domande, minuti, punteggi, soglia, date].
- "sections": riassumi le regole della prova che trovi nel testo; ometti la chiave se non ce ne sono.
- "tools" è facoltativo: gli id degli strumenti della materia. "tavola-periodica" è inclusa nell'app (materie di chimica); gli altri (tabelle, elenchi, calcolatori) si creano dopo con il prompt 8.9 e si aggiungono qui con il loro id. Se la materia non ne ha, ometti la chiave. Nell'app stanno nel foglio «Strumenti» della materia. I testi da aprire al volo ("links", capitoli dei libri) si aggiungono dopo, quando i libri sono nel pacchetto.
- "official" è facoltativo: elencalo solo se ti do prove ufficiali degli anni passati (le loro domande avranno id <materia>-<tag>-…); nell'app si scelgono dentro Simulazione. Altrimenti ometti la chiave.
- "icon" e "color" sono facoltativi: per ogni materia scegli un simbolo adatto SOLO dalla lista del capitolo 2.2 (es. biotech, science, bolt, calculate, gavel, psychology) e un colore #RRGGBB ben distinto dalle altre materie. Se nessun simbolo va bene, ometti "icon": l'app mostrerà le iniziali. Non inventare nomi.

Programma:
[incolla qui il programma o l'indice del corso]
```

## 8.2 Generare domande da un testo

Scegli una delle due varianti per la spiegazione e cancella l'altra.

```
Sei un docente che scrive domande d'esame. Dal TESTO qui sotto scrivi [N] domande per l'app di quiz, nel formato JSON seguente. Rispondi con il JSON valido in un solo blocco ```json; fuori dal blocco al massimo una riga «DUBBI:» con i punti incerti.

{ "schema": 1, "questions": [
  { "id": "<prefisso>-q001", "topic": "<id argomento>", "type": "mcq", "difficulty": 1|2|3,
    "text": "...", "options": ["...", "...", "...", "...", "..."], "answer": <indice 0-4 della giusta>,
    "explanation": "...", "source": "[nome della fonte, capitolo, pagina]", "group": "[nome breve della fonte]" },
  { "id": "<prefisso>-q002", "topic": "...", "type": "fill", "text": "... ____ ...",
    "answers": ["risposta", "variante accettata"], "explanation": "...", "source": "...", "group": "..." },
  { "id": "<prefisso>-q003", "topic": "...", "type": "fill", "text": "... ____ m/s",
    "numeric": { "value": 19.6, "tolerance": 0.02, "unit": "m/s" }, "explanation": "...", "source": "...", "group": "..." }
] }

Regole:
- Prefisso degli id: [es. fis-u1-cin]; numeri a tre cifre progressivi, mai ripetuti. Primo numero: [001].
- "topic": scegli tra questi argomenti (id: nome): [incolla l'elenco degli argomenti, o l'intero syllabus.json].
- Circa 2 domande "mcq" ogni domanda "fill". Difficoltà mista (circa 30% facili, 50% medie, 20% difficili).
- "mcq": esattamente 5 opzioni, una sola corretta, le altre plausibili e basate su errori tipici (unità sbagliata, formula invertita, segno, confusione tra concetti vicini). Niente "tutte le precedenti" o "nessuna delle precedenti". Varia la posizione della risposta giusta.
- "fill": risposta breve e non ambigua (una parola, un nome, un numero). Per i numeri usa "numeric" con l'unità e "tolerance" relativa (0.02 = 2%); per le parole metti in "answers" anche sinonimi e grafie alternative.
- Ogni domanda verifica UN concetto del testo; non usare conoscenze che il testo non contiene.
- Testo in italiano corretto; numeri con la virgola decimale nel testo, con il punto nel campo "value".
- VARIANTE A, spiegazione semplice: "explanation" = il ragionamento o il calcolo che porta alla risposta, in 1-3 frasi.
- VARIANTE B, spiegazione con note per ogni risposta: "explanation" = il ragionamento in 1-3 frasi, poi per OGNI opzione sbagliata una riga separata da "\n" nel formato "• B) <testo dell'opzione>: <perché è sbagliata, in una frase>". Le lettere seguono l'ordine di "options" (A = la prima). Più opzioni con lo stesso motivo: "• A), C) e E): <motivo>".

TESTO:
[incolla qui la sezione del libro, gli appunti o le slide]
```

## 8.3 Revisione: logica, contenuto e italiano

```
Sei un revisore esperto di [materia] e di lingua italiana. Ti do un file JSON di domande per un'app di quiz. Controlla ogni domanda:

1. Correttezza: risolvi tu la domanda da zero, PRIMA di guardare "answer"/"answers"/"numeric". La chiave deve coincidere con la tua soluzione; per i calcoli rifai i conti e controlla le unità di misura e gli ordini di grandezza.
2. Univocità: una sola opzione deve essere corretta; nessuna opzione sbagliata deve essere difendibile. Per i "fill", "answers" deve contenere le varianti accettabili (sinonimi, singolare/plurale, grafie).
3. Logica e chiarezza: il testo contiene tutti i dati necessari, non ha ambiguità, non suggerisce la risposta, le opzioni hanno forma e lunghezza simili.
4. Spiegazione: corretta, coerente con la chiave e con la posizione delle opzioni; se ci sono le righe "• X) …", ogni lettera corrisponde all'opzione giusta.
5. Lingua: grammatica, ortografia, punteggiatura, accenti, terminologia della materia, simboli e unità (m/s², 10⁻³, °C).
6. Formato: JSON valido; "answer" è un indice da 0 a 4; "options" ne ha 5; id e "topic" NON vanno cambiati.

Restituisci due cose:
A) Il JSON completo corretto, con la stessa struttura e gli stessi id. Se una domanda è irrecuperabile, toglila.
B) Dopo il JSON, un elenco delle modifiche: "id — cosa hai cambiato e perché" (scrivi "CHIAVE" se hai cambiato la risposta corretta, così la ricontrollo io).

JSON:
[incolla qui il file di domande]
```

## 8.4 Flashcard da un testo

```
Dal TESTO qui sotto crea [N] flashcard per l'app, in questo formato JSON. Rispondi con il JSON valido in un solo blocco ```json; fuori dal blocco al massimo una riga «DUBBI:».

{ "schema": 1, "cards": [
  { "id": "<prefisso>-c001", "topic": "<id argomento>", "front": "...", "back": "...", "source": "[fonte, capitolo, pagina]" }
] }

Regole:
- Prefisso degli id: [es. fis-u1-cin]; numeri progressivi a tre cifre, mai ripetuti.
- "topic" tra questi argomenti: [elenco id: nome].
- Una carta = una sola informazione da ricordare: definizioni, formule (con il significato di ogni simbolo e l'unità), leggi, eccezioni, confronti tra concetti simili, valori da sapere a memoria.
- "front": una domanda breve o un termine, senza la risposta dentro.
- "back": la risposta in 1-3 frasi, completa e comprensibile anche da sola, fedele al testo.
- Niente doppioni; preferisci ciò che il testo sottolinea o che servirebbe a rispondere a domande d'esame.

TESTO:
[incolla qui]
```

## 8.5 Ripulire un capitolo convertito da PDF o OCR

```
Ti do il testo di un capitolo estratto da un PDF (anche con OCR, quindi con errori). Riscrivilo in Markdown pulito per un lettore di libri, SENZA riassumere, togliere o aggiungere contenuti.

- Titoli con la numerazione del libro: "# 2 Titolo capitolo", "## 2.1 Sezione", "### 2.1.1 Sottosezione".
- Mantieni ESATTAMENTE i segnaposto di pagina "<!-- p. N -->" dove sono.
- Correggi gli errori di OCR: parole spezzate a fine riga, lettere confuse (l/1, O/0, rn/m), accenti, spazi.
- Togli intestazioni e piè di pagina ripetuti, numeri di pagina isolati, residui grafici.
- Elenchi con "-" o "1.", tabelle in Markdown, riquadri e box del libro come "> [!NOTE] Titolo" seguito da righe "> ".
- Formule: dentro il testo "$...$", formule importanti su righe proprie tra "$$" e "$$", in LaTeX. Ricostruisci apici, pedici e frazioni persi (es. "10-4" → "$10^{-4}$", "H2O" → "H₂O").
- Al posto di ogni figura scrivi una riga "![Figura N.M: didascalia originale](books/[materia]/[libro]/fig/N-M.svg)".
- Se un passaggio è illeggibile scrivi "[illeggibile]" invece di inventare.

Testo:
[incolla qui]
```

## 8.6 Ricomporre una figura

Allega l'immagine (foto o ritaglio della pagina) se l'AI le accetta.

```
Ti allego una figura di un libro di [materia] (didascalia: "[didascalia]"). Ricreala come SVG semplice e leggibile per un'app per telefono.

- SVG valido e autonomo: viewBox="0 0 400 300" (adatta le proporzioni), nessuna immagine esterna, nessun font speciale (font-family="sans-serif"), nessuno script.
- Solo forme essenziali: linee, frecce, rettangoli, cerchi, percorsi semplici, etichette di testo brevi (almeno 14 px). Colori pochi e ben distinguibili; sfondo trasparente.
- Tutte le etichette, i valori e gli assi dell'originale devono esserci e stare nella posizione giusta; non aggiungere informazioni che non ci sono.
- Per i grafici: assi con nome e unità, tacche principali con i loro valori, curve con la forma corretta.
- Se la figura è una foto o troppo complessa, dimmelo e scrivi invece una descrizione testuale di 3-5 righe da mettere nel capitolo al suo posto.

Rispondi con il solo codice SVG, in un blocco ```svg.
```

Salva il risultato con il nome indicato nella riga `![…](…)` del capitolo (per esempio `2-1.svg`) e aggiungilo da *Gestisci contenuti*.

## 8.7 Aggiungere note alle risposte di domande esistenti

```
Ti do un file JSON di domande a risposta multipla. Per ogni domanda "mcq", arricchisci "explanation" SENZA cambiare nient'altro (id, testo, opzioni, chiave restano identici):

- Lascia la spiegazione esistente all'inizio (correggila solo se è sbagliata, e segnalalo).
- Aggiungi, una per riga (separatore "\n"), una nota per OGNI opzione sbagliata nel formato "• B) <testo dell'opzione>: <perché è sbagliata>", una frase ciascuna. Le lettere seguono l'ordine di "options" (A = la prima, indice 0).
- La nota spiega l'errore di ragionamento che porta a quella scelta (formula sbagliata, unità confusa, concetto scambiato), non ripete solo "è errata".
- Prima di scrivere le note verifica che la chiave sia giusta; se non lo è, NON cambiarla: elenca l'id alla fine.

Rispondi con il JSON completo in un blocco ```json, poi, fuori dal blocco, «DUBBI:» con l'elenco delle eventuali chiavi sospette.

JSON:
[incolla qui]
```

## 8.8 Convertire domande da un altro formato

```
Ti do delle domande in un formato qualsiasi (testo, tabella, esportazione di un'altra app). Convertile nel formato JSON qui sotto senza cambiare testo, opzioni e risposte, tranne errori evidenti di battitura (che elenchi alla fine).

[incolla qui il formato del capitolo 3 della guida, o un esempio di domanda già nel formato giusto]

- "answer" è l'indice da 0 della risposta corretta nell'elenco "options".
- "topic": assegna a ogni domanda l'argomento più adatto tra questi: [elenco id: nome].
- Id: [prefisso]-q001, -q002, … nell'ordine.
- Se una domanda non ha la risposta corretta indicata, risolvila tu e segnalala alla fine con "CHIAVE DEDOTTA".

Domande:
[incolla qui]
```

## 8.9 Creare uno strumento (griglia, elenco, calcolatore)

Il risultato va in `content/tools/<id>.json` (capitolo 9) o si aggiunge nell'app con «+» in Gestisci contenuti. Prima di usarlo, fai il controllo dei contenuti: segnala formule che non si calcolano e caselle fuori posto.

```
Crea UNO strumento per l'app Tremendous Preparator: [cosa deve fare, per quale materia; es. «un calcolatore per le diluizioni»]

Lo strumento è un file di DATI (niente codice, niente HTML, niente Slint): l'app sa disegnare solo tre tipi e scarta qualunque altra cosa. Scegli il tipo che serve davvero:
- "grid": caselle in una griglia (tavola periodica, codice genetico, tabella di classificazione). Ogni casella ha una famiglia (colore e legenda) e una scheda di dettaglio.
- "list": schede in sezioni con la ricerca (costanti, formule da ricordare, glossario, prefissi).
- "calc": campi numerici e risultati calcolati con formule (leggi dei gas, diluizioni, pH, cinematica, conversioni).

REGOLE SEVERE (l'app controlla il file e rifiuta quello con errori: griglia fuori misura, famiglie non definite, formule che non si calcolano, id non validi):
1. Un solo blocco ```json con un solo oggetto; nient'altro dentro il blocco. Fuori dal blocco, al massimo «DUBBI:» con i punti incerti.
2. Campi comuni: "schema": 1, "id" (solo minuscole, cifre e trattini, es. "gas-perfetti"), "title" (breve), "subtitle" (facoltativo, una riga), "info" (facoltativo: come si legge o si usa, 1-4 frasi), "kind".
3. Nessun campo inventato: solo quelli scritti qui sotto (gli altri l'app li ignora, e il loro contenuto va perso). Testi in italiano corretto; simboli e unità in Unicode (m/s², H₃O⁺, 10⁻³), mai LaTeX né Markdown.
4. Dati verificati: ogni valore, ogni formula e ogni classificazione deve essere quella dei libri di testo; se un dato è incerto non metterlo e scrivilo nei DUBBI.

GRIGLIA ("kind": "grid"):
{ "schema": 1, "id": "...", "title": "...", "kind": "grid", "columns": 4, "rows": 3,
  "categories": [ { "name": "Famiglia", "color": "#RRGGBB" } ],
  "column_labels": ["..."], "row_labels": ["..."],
  "cells": [ { "x": 1, "y": 1, "label": "Fe", "top": "26", "bottom": "55,8", "category": "Famiglia",
    "title": "Ferro", "subtitle": "una riga", "details": [ { "label": "Sezione" }, { "label": "Nome", "value": "valore" } ], "text": "facoltativo" } ] }
- "columns" e "rows" da 1 a 40; "x" e "y" partono da 1 e stanno dentro la griglia; mai due caselle nella stessa posizione.
- "label": 1-4 caratteri (è la scritta grande della casella); "top" e "bottom" brevissimi (un numero, una sigla).
- "category": esattamente uno dei "name" di "categories"; da 2 a 10 famiglie con colori #RRGGBB ben distinti (l'app ne ricava le tinte chiare e scure).
- Una riga lasciata vuota (senza caselle né etichetta) diventa uno spazio stretto: usala per separare i blocchi.
- "details": righe nome-valore; una riga con il solo "label" fa da titoletto di sezione.
- "column_labels"/"row_labels" facoltativi: uno per colonna/riga, nell'ordine ("" per lasciarne una senza).

ELENCO ("kind": "list"):
{ "schema": 1, "id": "...", "title": "...", "kind": "list",
  "sections": [ { "title": "Sezione", "items": [ { "title": "Nome", "value": "valore in evidenza", "body": "spiegazione breve" } ] } ] }
- "value" corto (un numero con l'unità, una formula breve); "body" 1-2 frasi. Almeno una voce.

CALCOLATORE ("kind": "calc"):
{ "schema": 1, "id": "...", "title": "...", "kind": "calc",
  "inputs": [ { "id": "n", "label": "Moli", "unit": "mol", "default": 1 }, { "id": "T", "label": "Temperatura", "unit": "K", "default": 273.15 }, { "id": "V", "label": "Volume", "unit": "L", "default": 22.4 } ],
  "outputs": [ { "id": "P", "label": "Pressione", "unit": "atm", "formula": "n * 0.082057 * T / V", "digits": 4 } ] }
- Id dei campi e dei risultati: lettere, cifre e _, iniziale non numerica; mai "e" né "pi" (sono costanti).
- "formula": SOLO numeri col punto decimale, gli id dei campi e dei risultati PRECEDENTI, + - * / ^ e parentesi, le costanti pi ed e, le funzioni sqrt ln log (base 10) log2 exp sin cos tan asin acos atan abs round min max pow. Argomenti separati da virgola. Niente altro: niente =, niente unità dentro la formula, niente if.
- "default": un esempio realistico con cui tutte le formule danno un risultato sensato (nessuna divisione per zero); controllalo a mano.
- Unità coerenti: converti dentro la formula (es. mL → L dividendo per 1000) e scrivi l'unità del risultato in "unit". "digits": cifre significative (2-6).

Materia e argomenti: [nome della materia; se lo strumento serve solo ad alcune, elencale]
```
