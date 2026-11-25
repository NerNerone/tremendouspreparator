# Esame per Tremendous Preparator

Questo ramo è un esame per [Tremendous Preparator](https://github.com/NerNerone/tremendouspreparator):
il pacchetto dei contenuti e i progressi di chi studia. Solo dati: nessun codice dell'app e
nessun workflow, quindi non consuma minuti di GitHub Actions.

## Struttura

```text
content/syllabus.json         materie, unità, argomenti, regole e date d'esame
content/questions/*.json      quiz
content/flashcards/*.json     flashcard
content/books.json            libri registrati (facoltativo)
books/<materia>/<libro>/*.md  libri in Markdown, figure .svg accanto (facoltativi)
progress/                     sessioni, obiettivi, gruppi, feedback: li scrive l'app
progress/assessment.json      revisione dei progressi fatta da un'AI (facoltativa)

CLAUDE.md, AGENTS.md          direttive per l'AI che prepara il pacchetto
docs/guida/                   la guida dell'app, con i prompt (08-prompt.md)
docs/FORMATI.md, docs/schema/ formati dei file e schemi JSON
tools/                        estrazione del testo dai PDF (con OCR), divisione in capitoli,
                              formule in SVG
```

## Preparare il pacchetto con un'AI

Dai all'AI questo ramo (o apri il repository con Claude Code, Codex, …) e chiedi di preparare
l'esame dal tuo materiale: le istruzioni sono in `CLAUDE.md` (uguale ad `AGENTS.md`). Il percorso
completo, con i prompt da copiare, è in `docs/guida/00-con-l-ai.md`.

Strumenti (Linux o macOS):

```bash
sudo apt-get install poppler-utils tesseract-ocr tesseract-ocr-ita   # una volta
python3 -I tools/pdf2md.py libro.pdf raw.md --offset 12              # testo, con OCR dove serve
python3 -I tools/import_md.py libro.md books/fisica/mio-libro        # un file per capitolo
```

Controllo dei file con lo strumento `prep` dell'app:

```bash
git clone https://github.com/NerNerone/tremendouspreparator ../tp
cargo run -q --manifest-path ../tp/Cargo.toml -p prep-core --bin prep -- validate .
```

Il pacchetto d'esempio (`esempio`) va sostituito con il tuo esame.

## Creare il repository (privato)

Il repository dei dati **deve essere privato**: contiene i progressi e l'elenco dei gruppi con i
loro codici. L'app non sincronizza con un repository pubblico. Non usare «Fork» (il fork di un
repository pubblico resta pubblico): crea un repository **privato vuoto** su GitHub, poi

```bash
git clone --branch modello-dati --single-branch https://github.com/NerNerone/tremendouspreparator dati
cd dati
git remote rename origin modello                      # il modello resta collegato per gli aggiornamenti
git remote add origin https://github.com/<tu>/<repository-privato>.git
git push -u origin HEAD:mio-esame                     # un ramo per esame
```

Hai già il repository privato (per un altro esame)? Non crearne un altro: aggiungi un ramo.

```bash
git fetch modello modello-dati
git switch -c altro-esame modello/modello-dati
git push -u origin altro-esame
```

(dall'app: menu degli esami › «Nuovo ramo»).

Per avere guida, strumenti e istruzioni aggiornati non si riparte da zero: si uniscono i commit
del modello (`git fetch modello modello-dati && git merge modello/modello-dati`). Le istruzioni
per l'AI (`CLAUDE.md`) lo fanno fare da sé all'inizio di ogni lavoro.

## Nell'app

1. Repository **privato** e token GitHub «fine-grained» limitato **solo** a quel
   repository, con il permesso *Contents: Read and write*.
2. Tu › App › Repository dei progressi: accendi, scrivi proprietario, repository e token,
   «Salva e sincronizza».
3. Dal menu degli esami in Oggi: «Da un ramo» per scaricare un esame, «Nuovo ramo» per crearne
   uno da questo modello, oppure «Salva in un ramo» (tre puntini sull'esame) per mettere qui un
   esame importato da un pacchetto. Importando un pacchetto (o ricevendolo con un codice) l'app lo
   propone da sola.

Ogni ramo è un esame, o una versione dello stesso esame con progressi separati. L'app crea solo
file nuovi in `progress/` e non li modifica: niente conflitti tra dispositivi. Il token resta
solo sul dispositivo.
