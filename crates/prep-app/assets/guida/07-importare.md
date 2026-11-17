# 7 Controllare e importare

## 7.1 Nell'app

- **Importa pacchetto** (Opzioni › Pacchetto contenuti): installa un esame nuovo o **sostituisce** i contenuti di quello con lo stesso `id`. I progressi restano.
- **Gestisci contenuti** › **Aggiungi file**: aggiunge o sostituisce un solo file (quiz, flashcard, `syllabus.json`, `books.json`, capitolo `.md`, figura `.svg`), riconoscendone il tipo dal contenuto. Un pacchetto dello stesso esame qui si **unisce**: aggiorna i file che contiene e lascia gli altri.
- Dopo ogni aggiunta l'app mostra gli avvisi: argomenti che non esistono nell'esame, id ripetuti, file malformati. Un file malformato non viene aggiunto.
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

crea il pacchetto. `prep stats` mostra quante domande ha ogni argomento: gli argomenti scoperti sono i primi da riempire.

## 7.3 Prima di condividere

I contenuti che importi, e il diritto di usarli o condividerli, sono tua responsabilità: niente libri o eserciziari protetti da diritto d'autore in pacchetti dati ad altri.
