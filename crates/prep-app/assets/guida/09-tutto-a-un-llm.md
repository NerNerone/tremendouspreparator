# 9 Tutta la guida a un LLM

Invece di copiare un prompt alla volta puoi dare a un LLM **l'intera guida in un solo testo**: istruzioni per il modello, tutti i formati del pacchetto e i prompt del capitolo 8. Il modello sa così come devono essere fatti i file, e tu gli chiedi il lavoro sul materiale che alleghi.

## 9.1 Come si usa

1. Tocca **Copia** nel riquadro qui sopra (o **Condividi** per mandarla direttamente all'app della chat).
2. Apri una chat nuova con un LLM e incolla il testo. In cima trovi due righe da completare: **il compito** (per esempio «crea syllabus.json dal programma allegato», «scrivi 30 domande con note su ogni risposta dal capitolo allegato», «crea le flashcard», «rivedi il file di domande allegato», «ripulisci questo capitolo convertito da PDF») e **l'elenco del materiale** che alleghi.
3. Allega i file nella stessa risposta: PDF, foto delle pagine, appunti, slide, e il tuo `syllabus.json` se esiste già (così il modello usa gli argomenti giusti).
4. Il modello risponde con un file per blocco di codice, preceduto dal percorso. Salva ogni blocco in un file con quel nome e aggiungilo da *Gestisci contenuti*.
5. Controlla le chiavi: incolla il file di domande ottenuto in un'altra chat, meglio con un modello diverso, insieme al prompt 8.3.

## 9.2 Consigli

- La guida è lunga (circa 4000 parole): usa un modello che accetta testi lunghi e allegati.
- Una richiesta, un pezzo di materiale: un capitolo o una lezione per volta danno domande migliori di un libro intero.
- Se la risposta si interrompe scrivi «continua dall'ultimo id»; nella stessa chat il modello ricorda guida e numerazione.
- Per un nuovo lavoro sullo stesso esame puoi restare nella stessa chat e scrivere solo il nuovo compito.
