#!/usr/bin/env python3
"""Importa un libro markdown (es. export Testbuster) in books/<materia>/<libro>/.

    python3 -I tools/import_md.py libro.md books/fisica/testbuster [--page-formula 2,8]

- titoli normalizzati dalla numerazione: "3" -> #, "3.1" -> ##, "3.1.1" -> ###
  (anche righe non marcate o in grassetto; i numeri già usati sono esercizi, non titoli)
- marcatori "Pagina N" -> <!-- p. N --> con il numero di pagina del LIBRO:
  "Pagina 101 (210/211)" -> 210, "**Pagina 190**" -> 190; "# Pagina N" senza pagina del
  libro usa --page-formula a,b (pagina = a*N + b) solo se verificata sui marcatori con
  parentesi, altrimenti viene scartato
- blocchi ```mermaid``` (ricostruzioni approssimative delle figure) rimossi; le didascalie
  "*Figura N: …*" diventano segnaposto figura
- un file per capitolo: NN-titolo.md; stampa l'elenco dei capitoli per content/books.json
"""

import argparse
import json
import os
import re
import sys
import unicodedata

PAGE_RE = re.compile(r"^\s*#*\s*\**\s*Pagina\s+(\d+)\s*(?:\((\d+)\s*[-/]\s*(\d+)\))?\s*\**\s*$", re.I)
NUM_RE = re.compile(r"^(\d+(?:\.\d+){0,3})\.?\s+(\S.*)$")
FIG_RE = re.compile(r"^\*\s*(Figura\s+\d+[^*]*)\*\s*$", re.I)


def slug(s):
    s = unicodedata.normalize("NFD", s).encode("ascii", "ignore").decode().lower()
    s = re.sub(r"[^a-z0-9]+", "-", s).strip("-")
    return s[:50].strip("-") or "capitolo"


def strip_md(s):
    return s.strip().strip("*").strip().rstrip(":").strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("src")
    ap.add_argument("dest")
    ap.add_argument("--page-formula", default="")
    a = ap.parse_args()
    raw = open(a.src, encoding="utf-8", newline="").read().replace("\r\n", "\n")
    # comandi LaTeX corrotti da un export che ha interpretato "\a" "\b" "\t" "\f" "\v"
    # come sequenze di escape (es. "$\x08eta$" invece di "$\beta$")
    raw = raw.replace("\x07", "\\a").replace("\x08", "\\b").replace("\x0c", "\\f").replace("\x0b", "\\v")
    raw = re.sub(r"\t(?=[a-z])", r"\\t", raw)
    # "\r" di \rightarrow, \rightleftharpoons, \right diventato un a-capo
    raw = re.sub(r"\n(?=ight)", r"\\r", raw)
    lines = raw.split("\n")

    # pagine: verifica della formula sui marcatori con pagina del libro
    formula = None
    if a.page_formula:
        k, b = (int(x) for x in a.page_formula.split(","))
        checks = [(int(m.group(1)), int(m.group(2))) for m in (PAGE_RE.match(l) for l in lines) if m and m.group(2)]
        if checks and all(k * n + b == p for n, p in checks):
            formula = (k, b)
            print(f"formula pagine verificata su {len(checks)} marcatori", file=sys.stderr)
        else:
            print("formula pagine NON verificata: scarto i marcatori senza pagina del libro", file=sys.stderr)

    out = []
    seen = set()
    in_mermaid = False
    pending_chapter_num = None
    for i, raw in enumerate(lines):
        line = raw.rstrip()
        t = line.strip()
        if in_mermaid:
            if t.startswith("```"):
                in_mermaid = False
            continue
        if t.startswith("```mermaid"):
            in_mermaid = True
            continue
        m = PAGE_RE.match(t)
        if m:
            n = int(m.group(1))
            if m.group(2):
                out.append(f"<!-- p. {m.group(2)} -->")
            elif t.startswith("**"):
                out.append(f"<!-- p. {n} -->")
            elif formula:
                out.append(f"<!-- p. {formula[0] * n + formula[1]} -->")
            continue
        fm = FIG_RE.match(t)
        if fm:
            out.append(f"![{fm.group(1).strip()}]()")
            continue
        # titolo "# 1" seguito dal nome del capitolo
        if re.fullmatch(r"#+\s*\d+", t):
            pending_chapter_num = t.lstrip("#").strip()
            continue
        if pending_chapter_num and t.startswith("#"):
            t = f"# {pending_chapter_num} {t.lstrip('#').strip()}"
            pending_chapter_num = None
        elif pending_chapter_num and t:
            out.append(f"# {pending_chapter_num}")
            pending_chapter_num = None

        hashes = len(t) - len(t.lstrip("#"))
        body = strip_md(t.lstrip("#")) if hashes else strip_md(t)
        nm = NUM_RE.match(body)
        is_bold = t.startswith("**") and t.endswith("**")
        candidate = nm and (hashes > 0 or is_bold or (t == body and len(body) < 140))
        if candidate:
            num, title = nm.group(1), nm.group(2)
            looks_title = (
                len(title) < 140
                and not title.rstrip().endswith(("?", ":"))
                and "$" not in title
                and not re.match(r"(Un|Una|Uno|Si|Quale|Quali|Calcola|Determina|Data|Dato|Completa)\b", title)
            )
            if looks_title and num not in seen and (hashes or len(num.split(".")) >= 2):
                seen.add(num)
                level = min(len(num.split(".")), 3)
                out.append("")
                out.append(f"{'#' * level} {num} {title.strip()}")
                out.append("")
                continue
        if hashes and not nm:
            # titoli non numerati (es. "CORREZIONI COMMENTATE"): sottosezione
            title = body
            if title.lower() in ("indice",):
                out.append(f"# {title}")
            else:
                out.append(f"### {title}" if hashes >= 2 else f"## {title}")
            continue
        out.append(line)

    # divisione in capitoli
    text = "\n".join(out)
    text = re.sub(r"\n{3,}", "\n\n", text)
    chapters = []
    cur = None
    for line in text.split("\n"):
        m = re.match(r"^# (\d+) (.+)$", line)
        if m or re.match(r"^# (Indice|Introduzione)\s*$", line):
            cur = {"num": m.group(1) if m else "00", "title": line[2:].strip(), "lines": [line]}
            chapters.append(cur)
        elif cur is not None:
            cur["lines"].append(line)
    os.makedirs(a.dest, exist_ok=True)
    listing = []
    for ch in chapters:
        if ch["title"].lower() == "indice":
            continue
        fname = f"{int(ch['num']):02d}-{slug(re.sub(r'^\d+\s+', '', ch['title']))}.md"
        path = os.path.join(a.dest, fname)
        open(path, "w", encoding="utf-8").write("\n".join(ch["lines"]).strip() + "\n")
        listing.append({"file": path.replace(os.sep, "/"), "title": ch["title"]})
        print(f"{fname}: {len(ch['lines'])} righe", file=sys.stderr)
    print(json.dumps(listing, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
