#!/usr/bin/env python3
"""Conversione grezza PDF -> markdown con segnaposto di pagina `<!-- p. N -->`.

Uso:
    python3 -I tools/pdf2md.py libro.pdf books/fisica/giancoli/raw.md [--offset 12] [--ocr] [--from 1 --to 40]

- senza --ocr usa il testo del PDF (pdftotext); se una pagina non ha testo passa
  automaticamente all'OCR (pdftoppm + tesseract, lingua ita+eng).
- --offset: differenza tra pagina del PDF e numero stampato (pagina stampata = pagina PDF - offset).
  Serve perché i rimandi "Se vuoi approfondire" usino i numeri di pagina del libro.

Il risultato è grezzo: titoli, elenchi, formule e tabelle vanno poi sistemati
(lo fa Claude) prima di dividere il libro in capitoli in books/<materia>/<libro>/.
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile


def run(cmd):
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout


def page_count(pdf):
    out = run(["pdfinfo", pdf])
    for line in out.splitlines():
        if line.startswith("Pages:"):
            return int(line.split()[1])
    raise SystemExit("impossibile leggere il numero di pagine")


def text_page(pdf, n):
    return run(["pdftotext", "-layout", "-f", str(n), "-l", str(n), pdf, "-"])


def ocr_page(pdf, n, tmp):
    base = os.path.join(tmp, f"p{n}")
    subprocess.run(["pdftoppm", "-r", "300", "-gray", "-png", "-f", str(n), "-l", str(n), pdf, base], check=True)
    img = next(os.path.join(tmp, f) for f in sorted(os.listdir(tmp)) if f.startswith(f"p{n}") and f.endswith(".png"))
    out = run(["tesseract", img, "-", "-l", "ita+eng", "--psm", "3"])
    os.remove(img)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pdf")
    ap.add_argument("out")
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--ocr", action="store_true")
    ap.add_argument("--from", dest="first", type=int, default=1)
    ap.add_argument("--to", dest="last", type=int, default=0)
    a = ap.parse_args()

    total = page_count(a.pdf)
    last = a.last or total
    can_ocr = shutil.which("tesseract") is not None
    if a.ocr and not can_ocr:
        raise SystemExit("tesseract non installato (apt-get install tesseract-ocr tesseract-ocr-ita)")
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    ocr_pages = 0
    with tempfile.TemporaryDirectory() as tmp, open(a.out, "w", encoding="utf-8") as f:
        for n in range(a.first, last + 1):
            txt = "" if a.ocr else text_page(a.pdf, n)
            if len(txt.strip()) < 20 and can_ocr:
                txt = ocr_page(a.pdf, n, tmp)
                ocr_pages += 1
            printed = n - a.offset
            f.write(f"\n<!-- p. {printed} -->\n\n")
            # pdftotext -layout separa i paragrafi con righe vuote; normalizza gli spazi
            for line in txt.splitlines():
                f.write(" ".join(line.split()) + "\n")
            print(f"\rpagina {n}/{last}", end="", file=sys.stderr)
    print(f"\nscritto {a.out} ({last - a.first + 1} pagine, {ocr_pages} con OCR)", file=sys.stderr)


if __name__ == "__main__":
    main()
