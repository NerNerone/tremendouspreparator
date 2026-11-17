# 6 Da PDF, slide e foto al testo

Il materiale di partenza diventa testo Markdown, che poi ripulisci (a mano o con un LLM, capitolo 8).

## 6.1 PDF con testo selezionabile

La maggior parte di dispense e slide esportate ha già il testo. Al computer: `python3 tools/pdf2md.py libro.pdf bozza.md --offset N` (nel repository del progetto); `--offset` allinea le pagine del PDF ai numeri stampati e inserisce i segnaposto `<!-- p. N -->`.

## 6.2 Scansioni e foto

Serve l'OCR, il riconoscimento del testo nelle immagini:

- al computer, lo stesso `pdf2md.py` usa tesseract quando una pagina non ha testo (`tesseract-ocr` e `tesseract-ocr-ita`);
- sul telefono, finché l'app non avrà un suo OCR, puoi usare il riconoscimento del testo del sistema (copia il testo dalla foto) e incollarlo in un file `.md`.

L'OCR sbaglia: parole spezzate a fine riga, `l`/`1` e `O`/`0` confusi, esponenti persi (`10-4` invece di `10⁻⁴`), formule illeggibili, tabelle sbriciolate. Per questo c'è il prompt 8.5.

## 6.3 Slide

Una slide è spesso più figura che testo. Tieni un capitolo per presentazione, una sezione `##` per slide con il titolo della slide, il testo estratto sotto e una descrizione delle figure importanti (prompt 8.6).
