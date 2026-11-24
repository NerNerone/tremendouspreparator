# 3 Le domande (cartella content/questions)

Un file contiene un elenco di domande; puoi dividerle in quanti file vuoi (per materia, per fonte, per capitolo).

## 3.1 Risposta multipla

```
{ "schema": 1, "questions": [
  { "id": "fis-u1-q001", "topic": "fis-u1-t1", "type": "mcq", "difficulty": 2,
    "text": "Un'auto percorre 120 km in 1 ora e 30 minuti. Qual è la sua velocità media?",
    "options": ["60 km/h", "72 km/h", "80 km/h", "90 km/h", "100 km/h"],
    "answer": 2,
    "explanation": "v = s / t = 120 km / 1,5 h = 80 km/h.",
    "source": "Dispense di cinematica", "group": "Dispense" }
] }
```

- `options`: 5 risposte plausibili, una sola giusta; `answer` è la sua posizione **a partire da 0** (qui 2 = «80 km/h»). L'app mescola le opzioni a ogni domanda.
- `difficulty`: 1 facile, 2 media, 3 difficile.
- `source` / `group`: da dove viene la domanda; `group` è il nome della fonte nel filtro «Fonti» (quiz personalizzato e pagina dell'unità, che vale anche per i quiz di argomento). Se manca si ricava da `source`. Conviene metterlo uguale in tutte le domande di uno stesso materiale.

## 3.2 Note su ogni risposta (arricchimenti)

Nella spiegazione, una riga per opzione che inizia con `• lettera)` diventa una nota **dentro il riquadro di quella risposta** dopo la conferma: chi sbaglia legge perché proprio quella scelta è sbagliata. La parte senza elenco resta la spiegazione generale (sotto la risposta giusta).

```
"explanation": "v = s / t = 120 km / 1,5 h = 80 km/h.\n• A) 60 km/h: hai diviso per 2 ore invece che per 1,5.\n• B) 72 km/h: 1 h 30 min non è 1,3 h ma 1,5 h.\n• D) 90 km/h: hai usato 1 h 20 min.\n• E) 100 km/h: hai sommato invece di dividere."
```

Le lettere si riferiscono all'ordine del file (A = prima opzione); l'app le aggiorna quando mescola. Più opzioni con lo stesso motivo: `• A), C) e E): …`.

## 3.3 Completamento

Risposta testuale (sono accettate tutte le varianti in `answers`; maiuscole, accenti e punteggiatura ai bordi non contano):

```
{ "id": "fis-u1-q002", "topic": "fis-u1-t1", "type": "fill",
  "text": "Il principio secondo cui un corpo immerso riceve una spinta verso l'alto è detto principio di ____.",
  "answers": ["Archimede"], "explanation": "…" }
```

Risposta numerica, con tolleranza relativa (0,02 = ±2%) e unità:

```
{ "id": "fis-u1-q003", "topic": "fis-u1-t1", "type": "fill",
  "text": "Un sasso cade da fermo per 2 s (g = 9,8 m/s²). Che velocità raggiunge? ____ m/s",
  "numeric": { "value": 19.6, "tolerance": 0.02, "unit": "m/s" }, "explanation": "v = g·t = 19,6 m/s." }
```

Chi risponde può scrivere `19,6`, `1.96e1`, `19,6 m/s`, anche `3x10^-4` o `3·10⁻⁴` per i numeri piccoli.

## 3.4 Rimando al libro

`ref` porta al punto preciso di un capitolo nel riquadro «Se vuoi approfondire»:

```
"ref": { "book": "fis-dispense", "file": "books/fisica/dispense/02-cinematica.md", "section": "2.3", "page": 24 }
```

`section` può essere il titolo intero o il suo inizio (es. `"2.3"`). Senza `ref` l'app usa il primo capitolo che tratta l'argomento della domanda.

## 3.5 Proporzioni e qualità

- Come all'esame: circa 2 domande a risposta multipla per ogni completamento.
- Una domanda, un concetto. Niente «tutte le precedenti» o «nessuna delle precedenti».
- Opzioni sbagliate plausibili: errori tipici (unità sbagliata, formula scambiata, segno), non risposte assurde.
- Spiegazione sempre presente: il passaggio chiave, non solo la risposta.
