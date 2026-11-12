#!/usr/bin/env python3
"""OCR di una pagina a due colonne: scala di grigi, ingrandimento se bassa risoluzione,
taglio al canale centrale (proiezione verticale), tesseract ita psm 4 per colonna."""
import sys, subprocess, tempfile, os
import numpy as np
from PIL import Image, ImageOps

src, out = sys.argv[1:3]
im = ImageOps.grayscale(Image.open(src))
if im.width < 1000:
    f = 2600 / im.width
    im = im.resize((int(im.width * f), int(im.height * f)), Image.LANCZOS)
a = np.asarray(im, dtype=np.float32)
ink = (a < 140).astype(np.float32)
h, w = ink.shape
# riga di testata: esclusa dal calcolo del canale
body = ink[int(h * 0.07): int(h * 0.93)]
col = body.sum(axis=0)
lo, hi = int(w * 0.38), int(w * 0.62)
win = np.convolve(col, np.ones(15) / 15, mode="same")
cut = lo + int(np.argmin(win[lo:hi]))
two = win[cut] < 0.02 * body.shape[0]
parts = [im.crop((0, 0, cut, h)), im.crop((cut, 0, w, h))] if two else [im]
texts = []
with tempfile.TemporaryDirectory() as d:
    hf = os.path.join(d, "h.png")
    im.crop((0, 0, w, int(h * 0.075))).save(hf)
    hdr = subprocess.run(["tesseract", hf, "-", "-l", "ita", "--psm", "7"], capture_output=True, text=True).stdout.strip()
    texts.append("HDR: " + hdr)
    for i, p in enumerate(parts):
        fn = os.path.join(d, f"c{i}.png")
        p.save(fn)
        r = subprocess.run(["tesseract", fn, "-", "-l", "ita", "--psm", "4"], capture_output=True, text=True)
        texts.append(r.stdout)
open(out, "w").write(("\n=====COL=====\n").join(texts))
