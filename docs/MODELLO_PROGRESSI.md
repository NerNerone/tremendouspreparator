# Modello di stima dei progressi

Implementato in `crates/prep-core/src/progress.rs`, identico nell'app e nel comando `prep report`.

## Dalle sessioni ai punteggi

Ogni risposta registrata in `progress/sessions/` vale:

| esito | punteggio |
|---|---|
| corretta e sicura | 1,0 |
| corretta ma segnata *insicura* | 0,5 |
| sbagliata o omessa | 0 |

Flashcard: *Ancora* 0 · *Difficile* 0,4 · *Bene* 0,8 · *Facile* 1,0 (se *insicura*, al massimo 0,5).

**Forza** di una domanda/carta = media pesata delle ultime 3 prove (pesi 0,6 · 0,3 · 0,1, la più recente pesa di più).

## Argomento

- *accuratezza* = media delle forze degli elementi visti (una flashcard pesa 0,5 rispetto a una domanda);
- *copertura* = elementi diversi visti / min(6, elementi disponibili) — fino a 1;
- **stima automatica = accuratezza × copertura**.

Un argomento senza contenuti vale 0 (e nel report appare come SCOPERTO).

## Revisione AI (ancoraggio)

`progress/assessment.json` contiene, per alcuni argomenti, un valore `mastery` deciso da Claude al momento
`generated_at`, tenendo conto di cose che la formula non vede (tipo di errori, difficoltà delle domande, argomenti
mai testati ma noti, ecc.). Per questi argomenti:

```
valore = (4 · mastery_AI + Σ punteggi delle risposte successive) / (4 + numero di risposte successive)
```

Quindi subito dopo la revisione la barra mostra il valore dell'AI, poi si aggiorna con le nuove risposte (l'ancora
vale come 4 risposte). Gli argomenti non citati nella revisione usano la stima automatica.

## Aggregazione

- unità = media degli argomenti;
- materia = media delle unità pesata per CFU (come il peso nel compito);
- totale = media delle tre materie;
- punteggio atteso in simulazione = 31·p − 0,1·31·(1−p)·0,5 (si assume di rispondere a metà di ciò che non si sa).
