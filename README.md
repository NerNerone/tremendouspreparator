# Modello di repository dei dati per Tremendous Preparator

Questo ramo (`modello-dati`) è lo scheletro di un repository **di soli dati**: il pacchetto
contenuti di un esame e i progressi di chi studia. Nessun codice e nessun workflow: non consuma
minuti di GitHub Actions.

## Struttura

```text
content/syllabus.json        materie, unità, argomenti, regole e date d'esame
content/questions/*.json     quiz
content/flashcards/*.json    flashcard
books/<materia>/<libro>/*.md libri (facoltativi; figure .svg accanto)
progress/sessions/AAAA-MM/   quiz, simulazioni e ripassi (li scrive l'app)
progress/goals/<giorno>/     obiettivi del giorno (li scrive l'app)
progress/groups/             elenco dei propri gruppi di studio (lo scrive l'app)
progress/feedback/AAAA-MM/   segnalazioni (le scrive l'app)
progress/assessment.json     revisione dei progressi fatta da un'AI (facoltativo)
```

I formati sono descritti in `docs/FORMATI.md` e nella guida dentro l'app (Opzioni › Pacchetto
contenuti). Qui c'è il pacchetto d'esempio dell'app: sostituiscilo con il tuo.

L'app crea solo file nuovi in `progress/` e non li modifica mai: niente conflitti tra dispositivi.
Non cancellare i file di `progress/sessions/`, sono lo storico delle risposte.

## Un ramo per esame

Ogni ramo è un esame (o una versione dello stesso esame: per esempio due persone che preparano
la stessa prova con progressi separati, o una variante del pacchetto). Per crearne uno nuovo
parti da questo ramo:

```bash
git clone <url del tuo repository> dati && cd dati
git fetch <url del repository di Tremendous Preparator> modello-dati
git checkout -b mio-esame FETCH_HEAD
git push -u origin mio-esame
```

## Nell'app

1. Crea un repository (meglio privato) e copia qui dentro il contenuto di questo ramo.
2. Crea un token GitHub «fine-grained» limitato **solo** a quel repository, con il permesso
   *Contents: Read and write* (nient'altro).
3. App › Tu › Dispositivi › GitHub: accendi «Sincronizza con un repository GitHub», scrivi
   proprietario, nome del repository e token, poi «Salva e sincronizza».
4. «Rami»: tocca il ramo dell'esame aperto, oppure «Nuovo esame» per scaricare un esame da un
   altro ramo. Anche da Opzioni › Pacchetto contenuti › «Da un ramo del repository GitHub».

Il token resta solo sul dispositivo (impostazioni locali, mai inviato ad altri dispositivi né
messo nel repository). Con GitHub spento l'app non fa nessuna richiesta a GitHub: progressi e
gruppi viaggiano solo tra i propri dispositivi collegati.
