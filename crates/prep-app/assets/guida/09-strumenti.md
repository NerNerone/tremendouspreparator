# 9 Gli strumenti (cartella content/tools)

Uno strumento è una pagina da consultare mentre si studia: una tabella interattiva (la tavola
periodica, il codice genetico), un elenco da cercare (costanti, formule, glossario) o un
calcolatore (gas perfetti, diluizioni, pH). Si apre da Studia › Strumenti della materia.

Ogni strumento è **un file di dati**, `content/tools/<id>.json`: niente codice, niente HTML. L'app
sa disegnare tre tipi (`"kind"`) e ignora tutto il resto, così uno strumento scritto da altri non
può fare altro che mostrare dati e fare calcoli.

## 9.1 Campi comuni

- `schema`: 1. `id`: minuscole, cifre e trattini, stabile (dà il nome al file).
- `title`, `subtitle` (facoltativo, una riga), `info` (facoltativo: come si legge o si usa; pulsante ⓘ).
- `kind`: `"grid"`, `"list"` o `"calc"`.

Uno strumento vale per le materie che lo elencano in `tools` (capitolo 2); se nessuna materia lo
elenca, compare in tutte. La tavola periodica (`"tavola-periodica"`) è inclusa nell'app: basta
elencarla, oppure aggiungerla al pacchetto come file da Gestisci contenuti.

## 9.2 Griglia

```
{ "schema": 1, "id": "vitamine", "title": "Vitamine", "kind": "grid", "columns": 4, "rows": 3,
  "categories": [ { "name": "Liposolubili", "color": "#C05621" }, { "name": "Idrosolubili", "color": "#2C7A7B" } ],
  "cells": [
    { "x": 1, "y": 1, "label": "A", "bottom": "Retinolo", "category": "Liposolubili",
      "title": "Vitamina A · Retinolo",
      "details": [ { "label": "Funzione", "value": "Vista (rodopsina), epiteli" },
                   { "label": "Carenza", "value": "Cecità notturna" } ] } ] }
```

- `columns` e `rows`: da 1 a 40. Ogni casella ha `x` e `y` (da 1), una sola per posizione.
- `label`: la scritta grande (1-4 caratteri); `top` e `bottom`: le scritte piccole sopra e sotto.
- `category`: una delle `categories`, che hanno un nome (la legenda: toccandola si evidenzia la
  famiglia) e un colore `#RRGGBB` facoltativo; l'app ne ricava le tinte per il tema chiaro e scuro.
- `column_labels`, `row_labels` (facoltativi): le intestazioni, una per colonna o riga. Una riga
  senza caselle né etichetta diventa uno spazio stretto, per separare i blocchi.
- Scheda della casella: `title`, `subtitle`, `details` (righe nome–valore; una riga con il solo
  `label` fa da titoletto) e `text` libero.

## 9.3 Elenco

```
{ "schema": 1, "id": "costanti", "title": "Costanti", "kind": "list",
  "sections": [ { "title": "Chimica", "items": [
    { "title": "Numero di Avogadro", "value": "6,022·10²³ mol⁻¹", "body": "Particelle in una mole." } ] } ] }
```

Le voci si cercano per titolo, valore e testo. `value` è il dato in evidenza, `body` la spiegazione.

## 9.4 Calcolatore

```
{ "schema": 1, "id": "gas-perfetti", "title": "Gas perfetti", "subtitle": "PV = nRT", "kind": "calc",
  "inputs": [ { "id": "n", "label": "Moli", "unit": "mol", "default": 1 },
              { "id": "T", "label": "Temperatura", "unit": "K", "default": 273.15 },
              { "id": "V", "label": "Volume", "unit": "L", "default": 22.4 } ],
  "outputs": [ { "id": "P", "label": "Pressione", "unit": "atm", "formula": "n * 0.082057 * T / V", "digits": 4 },
               { "label": "Pressione", "unit": "kPa", "formula": "P * 101.325" } ] }
```

- I campi (`inputs`) hanno un `id` da usare nelle formule, un'etichetta, l'unità e un valore di
  partenza. Nell'app si scrive un numero (anche con la virgola) o un piccolo calcolo.
- Ogni risultato (`outputs`) ha una `formula` e, se serve alle formule dopo, un `id`. `digits`:
  cifre significative mostrate (predefinite 4).
- Nelle formule: numeri col punto, `+ - * / ^`, parentesi, gli id, le costanti `pi` ed `e`, le
  funzioni `sqrt ln log log2 exp sin cos tan asin acos atan abs round min max pow` (argomenti
  separati da virgola). Nient'altro: un id `e` o `pi` non è ammesso.

## 9.5 Condividere e spostare gli strumenti

In **Gestisci contenuti › Strumenti** ci sono quelli del pacchetto. Con **Aggiungi strumento** se
ne copia uno da un altro pacchetto installato o da quelli inclusi nell'app; con **⋮** uno si
salva da solo come file `.json` (da mandare a chi vuoi), si copia in un altro pacchetto o si toglie. Chi riceve il
file lo apre con l'app (o lo aggiunge con «+») e sceglie in quale pacchetto metterlo.

Il controllo dei contenuti (e `prep check`) segnala caselle fuori dalla griglia, famiglie non
definite, formule che non si calcolano, id non validi. Schema: `docs/schema/tools.schema.json`.
