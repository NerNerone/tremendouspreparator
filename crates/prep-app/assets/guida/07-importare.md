# 7 Controllare e importare

## 7.1 Nell'app

- **Importa pacchetto** (Tu › App › Pacchetto contenuti, «Apri con» sul telefono, o ricevuto con un codice da un altro dispositivo): installa un esame nuovo o **sostituisce** i contenuti di quello con lo stesso `id`. I progressi restano. Con il repository GitHub configurato, subito dopo l'app propone di salvarlo in un **ramo nuovo** (privato) del repository dei progressi.
- **Gestisci contenuti** › **Aggiungi file**: aggiunge o sostituisce un solo file (quiz, flashcard, `syllabus.json`, `books.json`, capitolo `.md`, figura `.svg`), riconoscendone il tipo dal contenuto. Un pacchetto dello stesso esame qui si **unisce**: aggiorna i file che contiene e lascia gli altri.
- Dopo ogni aggiunta l'app mostra gli avvisi: argomenti che non esistono nell'esame, id ripetuti, file malformati. Un file malformato non viene aggiunto.
- **Controlla** (pagina *Gestisci contenuti*) passa in rassegna tutto l'esame: **errori** (file rotti, risposte mancanti, rimandi a capitoli che non ci sono), **avvisi** (opzioni ripetute o in numero sbagliato, spiegazioni mancanti, difficoltà fuori scala, flashcard vuote) e **consigli** (figure da aggiungere, argomenti con meno di 6 domande: la barra non può arrivare in fondo).
- **Esporta pacchetto** salva tutti i contenuti dell'esame in un unico `.json.gz`, da conservare o dare ad altri (anche con *Condividi con un'altra persona*).

## 7.2 Al computer

Nel repository del progetto, con la cartella dei contenuti:

```
cargo run -q -p prep-core --bin prep -- validate cartella-esame
```

controlla formati, argomenti e rimandi ai libri (deve finire con «0 avvisi»), poi

```
cargo run -q -p prep-core --bin prep -- pack cartella-esame --out=esame.json.gz
```

crea il pacchetto. `prep check cartella-esame` fa lo stesso controllo di qualità dell'app, `prep stats` mostra quante domande ha ogni argomento. Gli schemi JSON dei formati sono in `docs/schema/` (utili a un editor o da allegare a un'AI).

## 7.3 Prima di condividere

I contenuti che importi, e il diritto di usarli o condividerli, sono tua responsabilità: niente libri o eserciziari protetti da diritto d'autore in pacchetti dati ad altri.
