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

## Nell'app

1. Repository (meglio privato) e token GitHub «fine-grained» limitato **solo** a quel
   repository, con il permesso *Contents: Read and write*.
2. Tu › App › Repository dei progressi: accendi, scrivi proprietario, repository e token,
   «Salva e sincronizza».
3. Dal menu degli esami in Oggi: «Da un ramo» per scaricare un esame, «Nuovo ramo» per crearne
   uno da questo modello, oppure «Salva in un ramo» (tre puntini sull'esame) per mettere qui un
   esame importato da un pacchetto.

Ogni ramo è un esame, o una versione dello stesso esame con progressi separati. L'app crea solo
file nuovi in `progress/` e non li modifica: niente conflitti tra dispositivi. Il token resta
solo sul dispositivo.
