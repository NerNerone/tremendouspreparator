# Tremendous Preparator

App di studio per le prove del **semestre aperto di Medicina** (Fisica, Chimica e propedeutica biochimica,
Biologia), scritta in **Rust** con interfaccia [Slint](https://slint.dev) in stile Material 3.
Gira su **Android** (APK arm64), su **Linux** (Flatpak x86_64) e nel **browser**
(<https://nernerone.github.io/tremendouspreparator/>, installabile su iPhone da Safari con
*Condividi → Aggiungi alla schermata Home*).

Questo repository contiene **solo l'infrastruttura**: il codice dell'app, gli strumenti per costruire i
contenuti e la loro documentazione. Libri, quiz e flashcard non sono inclusi: si importano a parte (vedi
[Contenuti](#contenuti)). Le uniche domande presenti sono un piccolo insieme di esempio.

## Funzioni

- **Quiz** per argomento, unità o materia, con correzione immediata, spiegazione e rimando al libro
  ("Se vuoi approfondire") nel lettore integrato.
- **Simulazione d'esame** con le regole ufficiali: 31 quesiti (21 a risposta multipla + 10 a completamento),
  50 minuti, +1 / −0,1 / 0.
- **Flashcard** con ripetizione dilazionata; le carte già studiate restano ripassabili ("In ripasso").
- **Domande insicure e sbagliate** raccolte in una scheda di ripasso.
- **Stima della preparazione** per argomento, unità e materia, e **obiettivi del giorno**
  (modello in [docs/MODELLO_PROGRESSI.md](docs/MODELLO_PROGRESSI.md)).
- **Pausa e ripresa** di quiz e simulazioni, **promemoria** giornalieri.
- **Tavola periodica** interattiva.
- **Sincronizzazione facoltativa con un repository GitHub privato**: i risultati di ogni sessione vengono
  committati come file JSON e i contenuti aggiornati vengono scaricati. Senza GitHub l'app funziona offline.

## Contenuti

I contenuti stanno in un repository separato (privato, per i diritti d'autore dei libri) con questa struttura:

```text
content/syllabus.json            programma: materie, unità, argomenti, CFU
content/books.json               libri e capitoli collegati agli argomenti
content/questions/*.json         domande
content/flashcards/*.json        flashcard
books/<materia>/<libro>/*.md     testo dei libri (Markdown) e formule SVG
progress/                        sessioni, valutazioni, feedback (scritti dall'app)
```

I formati sono descritti in [docs/FORMATI.md](docs/FORMATI.md). L'app li riceve in due modi:

1. **Pacchetto contenuti** — un unico file `SemestreAperto-contenuti.json.gz` creato con
   `cargo run -p prep-core --bin prep -- pack` (nel repository dei contenuti, di solito da GitHub Actions).
   Si importa da *Opzioni → Pacchetto contenuti*; su Android anche aprendo il file con l'app. I progressi non
   vengono toccati.
2. **Sincronizzazione** — in *Opzioni → Repository GitHub* si indicano proprietario, repository e un token
   *fine-grained* con permesso *Contents: Read and write*.

### Strumenti per costruire i contenuti (`tools/`)

Si eseguono dalla cartella del repository dei contenuti (oppure con `PREP_ROOT=<cartella>`):

| Script | Uso |
| --- | --- |
| `pdf2md.py` | PDF (testo o scansione, OCR con tesseract) → Markdown con segnaposto di pagina |
| `import_md.py` | Markdown ripulito → capitoli in `books/` |
| `mathsvg.mjs` | formule `$$…$$` → SVG (MathJax) |
| `import_exam.py` | prove ufficiali → domande |
| `edises/`, `eserciziario/` | importazione e revisione di eserciziari da OCR (bozza → correzioni → file finali) |
| `build_elements.py` | dati della tavola periodica (`content/elements.json`) |

Il comando `prep` (crate `prep-core`) offre anche `validate` (controllo di formati e riferimenti), `stats`
(copertura del programma), `report` (progressi) e `feedback`.

## Struttura del codice

```text
crates/prep-core   logica senza interfaccia: modello dati, selezione delle domande, correzione,
                   ripetizione dilazionata, stima dei progressi, archivio locale, sincronizzazione GitHub,
                   pacchetto contenuti, Markdown e LaTeX
crates/prep-app    interfaccia Slint (ui/*.slint) e controller (src/app.rs); binario desktop e libreria Android
android/           codice Java (promemoria, importazione file) e risorse dell'APK
flatpak/           manifest del pacchetto desktop
```

## Sviluppo

```bash
cargo test -p prep-core
```

```bash
cargo run -p tremendouspreparator --bin prepapp-desktop
```

```bash
cargo run -q -p prep-core --bin prep -- validate
```

La modalità dimostrativa (`PREP_DEMO`, `PREP_SHOTS`, `PREP_SIZE`, `PREP_DRAGPAN`: vedi `crates/prep-app/src/demo.rs`)
esegue una sequenza di azioni e salva screenshot, utile per verificare l'interfaccia senza un dispositivo.

L'APK e il Flatpak sono costruiti da [.github/workflows/android.yml](.github/workflows/android.yml) e allegati a
una release. Per firmare l'APK con una chiave stabile servono i secret `ANDROID_KEYSTORE_B64` e
`ANDROID_KEYSTORE_PASSWORD`; senza, viene usata una chiave temporanea.

### Versione web

La stessa app compilata in WebAssembly ([.github/workflows/pages.yml](.github/workflows/pages.yml)), pubblicata su
GitHub Pages. I dati restano nel browser (IndexedDB) e funziona anche offline; i contenuti si importano con il
pacchetto (*Opzioni → Pacchetto contenuti*). La sincronizzazione con GitHub non è ancora disponibile nel browser.

```bash
rustup target add wasm32-unknown-unknown
```

```bash
cargo build --release -p tremendouspreparator --lib --target wasm32-unknown-unknown
```

```bash
wasm-bindgen --target web --no-typescript --out-dir site target/wasm32-unknown-unknown/release/prepapp.wasm && cp web/* site/
```

## Sincronizzazione tra dispositivi

In Opzioni → Dispositivi, con lo stesso **nome utente** e la stessa **password** su telefono, web
app e computer (app aperta su entrambi) i dispositivi si trovano da soli: i progressi degli esami
in comune si uniscono (le sessioni sono file che non cambiano, quindi basta scambiarsi quelli
mancanti) e un esame installato su un dispositivo si può scaricare sull'altro.

Non c'è un server dell'app: i messaggi viaggiano come eventi Nostr *effimeri* attraverso relay
pubblici (modificabili), che li inoltrano senza salvarli. Dal nome utente e dalla password si
ricava con PBKDF2 una chiave: i relay vedono solo un tag casuale e dati cifrati con
XChaCha20-Poly1305. Il protocollo è in [`prep-core/src/p2p.rs`](crates/prep-core/src/p2p.rs),
i collegamenti in [`prep-app/src/net.rs`](crates/prep-app/src/net.rs). Chi conosce nome utente e
password entra nella stanza: conviene una password lunga.

## Licenza

[GPL-3.0-or-later](LICENSE): GNU General Public License versione 3 o successive. È compatibile con la
licenza GPL-3.0 di [Slint](https://slint.dev) e con le dipendenze Apache-2.0; chi distribuisce versioni modificate
deve pubblicarne il sorgente con la stessa licenza. I font inclusi (Roboto, DejaVu) hanno le proprie
licenze, riportate in [crates/prep-app/fonts/LICENSES.txt](crates/prep-app/fonts/LICENSES.txt).
