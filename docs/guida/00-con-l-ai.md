# Creare un pacchetto con un'AI

**Il modo più semplice è nell'app: Home › «Crea un pacchetto con l'AI»** (o Opzioni › Pacchetto contenuti). Ti guida file per file: per ognuno prepara il prompt (con gli argomenti del tuo esame, il numero di domande e gli id già pronti), tu lo dai all'AI con il tuo materiale e incolli la risposta; l'app la legge, la salva nel posto giusto e ti dice cosa correggere.

Questa pagina è il percorso completo, in ordine: dal materiale grezzo (libri, dispense, slide) a un pacchetto con esame, quiz e flashcard. Con **Copia** nel riquadro qui sopra dai a un'AI (ChatGPT, Claude, Gemini, …) **tutta la guida in un solo testo**: istruzioni severe, formati e prompt. Con **Strumenti PDF** dividi i PDF troppo grandi ed estrai il testo.

## Passo 1: raccogli il materiale

- Il **programma ufficiale** del corso o dell'esame: diventerà la struttura (materie, unità, argomenti).
- I **testi** da cui studi: libri, dispense, slide, appunti.
- Gli **eserciziari** e le prove degli anni passati, se ci sono.

Lavora un capitolo, una lezione o una presentazione alla volta: un'AI dà risultati migliori su pezzi piccoli che su un libro intero.

## Passo 2: prima il testo, poi l'AI

Non dare all'AI il PDF intero: **prima trasformalo in testo, poi fallo correggere**. Il testo costa meno, l'AI lo legge meglio, non ha limiti di MB e tu puoi controllarlo.

1. **PDF con testo selezionabile, Word, PowerPoint, OpenDocument, EPUB** (dispense, slide, libri digitali): Strumenti PDF › «Estrai il testo». Ottieni un file `.md` con i segnaposto di pagina; da Word e PowerPoint titoli, elenchi e tabelle sono già in Markdown, dal PDF è testo grezzo. In ogni caso da revisionare al passo 3.
2. **Scansioni e foto**: passa le pagine dall'OCR (riconoscimento del testo): Strumenti PDF › «Testo con l'OCR», nell'app per Android e per computer (vedi 6.2). L'OCR sbaglia lettere, accenti ed esponenti: lo correggerà l'AI al passo 3.
3. **PDF direttamente all'AI solo quando l'OCR fallirebbe di sicuro**: pagine scritte a mano, formule lunghe o complesse, schemi e grafici da ricostruire, tabelle fitte. Se il file supera il limite di MB della tua AI, dividilo con Strumenti PDF › «Dividi il PDF in più parti» e dai le parti una alla volta (o insieme, se l'AI accetta più file).

## Passo 3: revisione del testo

Con il prompt 8.5 l'AI corregge il testo dell'OCR **senza riassumere né aggiungere nulla**: revisione **grammaticale** (ortografia, accenti, parole spezzate), **semantica** (termini della materia, simboli e unità) e **logica** (frasi e formule coerenti, numeri plausibili). Il risultato è un capitolo Markdown pulito, con titoli numerati, formule e riquadri.

- Controlla a campione 2–3 pagine contro l'originale prima di andare avanti.
- Le figure importanti si ricompongono con il prompt 8.6 (SVG semplice o descrizione).

## Passo 4: la struttura dell'esame

Con il prompt 8.1 l'AI scrive `content/syllabus.json` dal programma: materie, unità, argomenti con i loro id, regole e date della prova. È il primo file da aggiungere all'app: tutti gli altri usano i suoi id.

## Passo 5: quiz

Con il prompt 8.2, **un capitolo revisionato alla volta**, l'AI scrive le domande: circa 2 a risposta multipla ogni completamento, 5 opzioni plausibili, spiegazione sempre presente. Scegli la variante con **note su ogni risposta** (perché ogni opzione sbagliata è sbagliata) se vuoi spiegazioni più ricche. Per domande già pronte in un altro formato usa il prompt 8.8; per aggiungere le note a domande esistenti il prompt 8.7.

## Passo 6: flashcard

Con il prompt 8.4, dallo stesso capitolo: una carta per ogni definizione, formula, legge, eccezione da ricordare.

## Passo 7: verifica

Dai i file di quiz a **un'altra AI** (o a una chat nuova) con il prompt 8.3: risolve ogni domanda da zero, controlla chiavi, unità, italiano e formato, ed elenca le chiavi cambiate. Ricontrolla tu quelle segnate «CHIAVE».

## Passo 8: importa e prova

Contenuti › «Aggiungi file»: prima `syllabus.json`, poi quiz, flashcard e capitoli. L'app segnala argomenti inesistenti, id ripetuti e file malformati: correggi e riaggiungi. Poi fai un quiz di prova.

## Usare la guida intera con un'AI

1. **Copia** (o **Condividi**) nel riquadro in cima a questa pagina.
2. Apri una chat nuova, incolla il testo e completa le due righe in cima: **il compito** e **il materiale allegato**.
3. Allega i file: il testo `.md` del passo 2, il tuo `syllabus.json` se esiste già, e solo se serve il PDF (diviso).
4. L'AI risponde con un file per blocco di codice, preceduto dal percorso: salva ogni blocco con quel nome e aggiungilo da *Contenuti*.
5. Se la risposta si interrompe scrivi «continua dall'ultimo id».
