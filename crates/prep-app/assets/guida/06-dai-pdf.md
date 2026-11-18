# 6 Da PDF, slide e foto al testo

Il materiale di partenza diventa testo Markdown, che poi ripulisci (a mano o con un'AI, capitolo 8).

## 6.1 PDF con testo selezionabile

La maggior parte di dispense e slide esportate ha già il testo. Nell'app: **Strumenti PDF › «Estrai il testo grezzo dal PDF»** (dalla prima pagina della guida o da *Contenuti*) crea un file `.md` con il testo di ogni pagina e i segnaposto di pagina, e segnala le pagine senza testo (scansioni). Al computer, nel repository del progetto:

```
python3 tools/pdf2md.py libro.pdf bozza.md --offset N
```

`--offset` allinea le pagine del PDF ai numeri stampati; lo script inserisce i segnaposto di pagina (`<!-- p. N -->`, vedi 5.2) e passa dall'OCR le pagine scansionate.

## 6.2 Scansioni e foto

Serve l'OCR, il riconoscimento del testo nelle immagini:

- nell'app (Android e computer): **Strumenti PDF › «Testo con l'OCR»**, poi scegli un PDF scansionato o la foto di una pagina. Il riconoscimento avviene sul dispositivo (la prima volta scarica i modelli, 12 MB) e il risultato è un file `.md` con i segnaposto di pagina; le pagine che hanno già testo lo usano così com'è e quelle su due colonne si leggono una colonna dopo l'altra. Con il campo «Pagine» scegli solo una parte (es. un capitolo) e con «Ferma» interrompi. Nel browser l'OCR non c'è;
- al computer, nel repository, anche `pdf2md.py` usa tesseract quando una pagina non ha testo (`tesseract-ocr` e `tesseract-ocr-ita`).

Per le foto: pagina dritta, a tutta inquadratura, ben illuminata e senza ombre. L'OCR dell'app non conosce le lettere accentate: rimette da solo quelle sicure (è, perché, più, città…) e lascia le altre all'AI.

L'OCR sbaglia: parole spezzate a fine riga, `l`/`1` e `O`/`0` confusi, esponenti persi (`10-4` invece di `10⁻⁴`), formule illeggibili, tabelle sbriciolate. Per questo c'è il prompt 8.5.

## 6.3 Slide

Una slide è spesso più figura che testo. Tieni un capitolo per presentazione, una sezione `##` per slide con il titolo della slide, il testo estratto sotto e una descrizione delle figure importanti (prompt 8.6).

## 6.4 PDF troppo grandi per l'AI

Quando serve dare il PDF all'AI (passo 2 della prima pagina), **Strumenti PDF › «Dividi il PDF in più parti»** lo divide in parti sotto il limite di MB per file che scegli (5–100 MB), a pagine intere e in ordine; il nome di ogni parte dice quali pagine contiene.
