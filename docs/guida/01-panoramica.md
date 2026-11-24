# 1 Come è fatto un pacchetto

Un **pacchetto contenuti** descrive un esame: le materie con unità e argomenti, le regole della prova, le domande, le flashcard e, se vuoi, i libri da leggere nell'app. È un unico file `.json.gz`, ma si costruisce da una cartella di file di testo che puoi scrivere a mano, con l'aiuto di un'AI (capitolo 8 e prima pagina) o un pezzo alla volta direttamente nell'app.

## 1.1 La cartella

```
content/syllabus.json          l'esame: regole, date, materie → unità → argomenti
content/questions/*.json       le domande (quanti file vuoi)
content/flashcards/*.json      le flashcard (quanti file vuoi)
content/books.json             i libri e i loro capitoli (facoltativo)
books/<materia>/<libro>/*.md   il testo dei capitoli, in Markdown
books/<materia>/<libro>/fig/   le figure dei capitoli, in SVG
```

Solo `content/syllabus.json` e almeno un file di domande sono obbligatori.

## 1.2 Gli identificativi

Ogni cosa ha un **id**: le materie (`fis`), le unità (`fis-u3`), gli argomenti (`fis-u3-t4`), le domande (`fis-u3-q012`), le flashcard (`fis-u3-c004`). Gli id collegano tutto: una domanda dice a quale argomento appartiene, l'app salva i tuoi progressi per id.

- Usa solo minuscole, cifre e trattini.
- **Non cambiare mai l'id** di una domanda o flashcard già usata, e non riusarlo per un'altra: i progressi andrebbero alla domanda sbagliata. Se correggi un errore di battitura tieni l'id; se cambi il senso della domanda, dagli un id nuovo.

## 1.3 Tre modi per crearlo

1. **Nell'app, un file alla volta**: Opzioni › Pacchetto contenuti › *Gestisci contenuti* › *Aggiungi file*. Aggiungi la definizione dell'esame, poi i file di quiz e flashcard man mano che li prepari. Niente da rigenerare.
2. **Con un'AI**: i prompt del capitolo 8 producono i file nel formato giusto a partire da programma, appunti, slide o libri. Oppure copi **tutta la guida in un colpo** (riquadro all'inizio della guida e nella pagina *Contenuti*) e la incolli in chat insieme al materiale allegato.
3. **Al computer**, con la cartella completa e lo strumento `prep` del repository (capitolo 7) per controllarla e impacchettarla.

Puoi mescolare i tre modi: per esempio esame scritto a mano, quiz generati da un'AI e controllati, capitoli importati uno alla volta.
