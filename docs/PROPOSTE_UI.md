# Proposte di interfaccia (anteprima)

Tre direzioni per la revisione dell'interfaccia, tutte **Material 3 Expressive**: stessi ruoli colore
(compresi Material You e il colore scelto in Aspetto), Roboto, pulsanti a pillola che si deformano alla
pressione, movimento a molla, barre di progresso ondulate, aree di tocco di almeno 48 px.
Cambiano gerarchia, superfici, forme, densità, barre e navigazione. Si provano in
**Opzioni › Aspetto › Stile (anteprima)** (o con `PREP_DESIGN=1|2|3` sul desktop) e sono definite nel
global `Design` di `crates/prep-app/ui/theme.slint`.

Riferimenti: principi tratti dalla raccolta *awesome-design-md* (Linear, Notion, Wise, Revolut, Airbnb,
Apple), mai palette o elementi di marchio.

## In comune (correzioni valide per tutte)

- Opzioni: ogni voce ha un'icona; dentro una sezione il titolo non si ripete (è già nella barra in alto).
- Dispositivi: segnaposto della password più corto (prima finiva sotto l'icona dell'occhio).
- Flashcard: il fronte della carta, finché non si gira, ha il testo più grande (prima la carta restava
  quasi vuota).
- Pagine interne e quiz: barra in alto compatta anche quando le schede principali hanno il titolo grande.

## 1 · Focus — calma e densità

Per chi studia molte ore: più informazione per schermata, niente decorazioni.

| Token | Valore |
|---|---|
| superfici | tonali (`surface-container`), **nessuna ombra** |
| angoli | card 20 px, righe 14 px (più squadrati) |
| spaziature | interne 14 px, tra elementi 8 px |
| tipografia | titoli 650, numeri in evidenza 600 (52 px) |
| navigazione | barra compatta (64 px), etichetta solo sulla scheda attiva |
| elenchi | righe su fondo tonale, icona in un quadrato arrotondato |

Regole: un solo accento (primario) per le azioni; il resto in toni neutri. Il colore delle materie solo
su pallini e barre.

## 2 · Energia — Expressive pieno

Per motivare: numeri grandi, colori pieni, forme morbide, una navigazione che «respira».

| Token | Valore |
|---|---|
| superfici | pagina `surface-container-low`, card bianche con ombra morbida |
| angoli | card 32 px, righe 24 px, intestazione Home 40 px |
| tipografia | titoli delle schede 32 px peso 800, numeri 76 px peso 900 |
| intestazione Home | blocco **primario pieno** con la percentuale in bianco |
| navigazione | **barra flottante** staccata dal bordo; la scheda attiva diventa una pillola con icona ed etichetta |
| pulsanti | principali alti 56 px |
| elenchi | righe con bordo sottile e icona in un cerchio colorato |

Regole: il primario pieno solo sull'intestazione e sulle azioni principali; i colori delle materie sui
numeri di padronanza.

## 3 · Chiarezza — aria e contorni

Per leggere a lungo: fondo chiaro uniforme, contorni sottili al posto dei riempimenti, testo più grande.

| Token | Valore |
|---|---|
| superfici | card con **bordo sottile** (`outline-variant`) su fondo `surface`, nessuna ombra |
| angoli | card 24 px |
| spaziature | interne 20 px, tra elementi 16 px |
| tipografia | titoli delle schede 28 px, testo piccolo 13 px, numeri in evidenza peso 500 in colore primario |
| navigazione | barra standard su fondo pagina con un filo sopra |
| elenchi | righe senza fondo con **divisori**, icone semplici in colore primario |

Regole: il colore è un segnale (azioni, percentuali), mai uno sfondo grande.
