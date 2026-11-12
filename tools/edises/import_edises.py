#!/usr/bin/env python3
"""Eserciziario EdiSES (pagine 1-902): dalle pagine OCR alle domande.

    python3 -I tools/import_edises.py <cartella lavoro> <out.json>

La cartella di lavoro contiene txt/ (OCR a due colonne, ocr_page.py), txtf/ (pagine di quiz a
completamento con i vuoti come "____", fill_ocr.py) e boxes/ (tabelle di risposte nei riquadri
grigi, extract_boxes.py: {"kind": "keys"|"fill", "entries": {numero: testo}}).

Il libro ha, per materia e capitolo, le sezioni "Quiz commentati", "Quiz non commentati",
"Quiz a completamento" e in fondo al capitolo "Risposte Capitolo N" con le risposte dei
commentati (lettera + spiegazione), la tabella dei non commentati e quella dei completamenti.
Le domande sono abbinate alle risposte per (materia, capitolo, sezione, numero): numerazione
stampata, quindi affidabile. Il risultato è una bozza: va rivista prima di pubblicarla.
"""
import json
import os
import re
import sys

SUBJ = [("chi", re.compile(r"Chimica\s+e\s+Propedeutica", re.I)),
        ("bio", re.compile(r"Biologia\s*e\s*Genetica|Biologiae\s+Genetica", re.I)),
        ("fis", re.compile(r"\bFisica\b"))]
CAP = re.compile(r"\bCapitolo\s+(\d{1,2})\b")
SEC = [("rc", re.compile(r"Risposte\s+quiz\s+commentati", re.I)),
       ("rn", re.compile(r"Risposte\s+quiz\s+non\s+commentati", re.I)),
       ("rf", re.compile(r"Risposte\s+quiz\s+a\s+completament", re.I)),
       ("n", re.compile(r"Quiz\s+non\s+commentati", re.I)),
       ("f", re.compile(r"Quiz\s+a\s+completament", re.I)),
       ("c", re.compile(r"Quiz\s+commentati", re.I))]
QNUM = re.compile(r"^(\d{1,3})\s*\)\s*(.*)$")
OPT = re.compile(r"^\(?(?:([A-E])|(c|€|Cc|cC))\s*[.,:]\s*(.*)$")
ANS_C = re.compile(r"^(\d{1,3})\s*\)\s*([A-E])\s*[.,:]\s*(.*)$")
ANS_N = re.compile(r"(\d{1,3})\s*\)\s*([A-E])\b")
ANS_F = re.compile(r"(?:^|\s)(\d{1,3})\.\s+(.+?)(?=\s+\d{1,3}\.\s|$)")


WORK = "."
FILL_PAGE = {}


PAGEMAP = {}


def page_order(name):
    """Ordine delle pagine nel PDF: le immagini estratte non seguono sempre le pagine
    (pagemap.json: immagine -> pagina, da pdfimages -list); "partK-pNNN" = pagina NNN in testo."""
    m = re.match(r"part(\d)([abc]?)-(p?)(\d+)", name)
    part, idx = m.group(1) + m.group(2), int(m.group(4))
    if m.group(3):
        pg = idx
    else:
        pg = int(PAGEMAP.get("part" + part, {}).get("img2page", {}).get(str(idx), idx))
    return (int(m.group(1)), m.group(2), pg)


def load(work):
    global PAGEMAP
    pm = os.path.join(work, "pagemap.json")
    PAGEMAP = json.load(open(pm)) if os.path.exists(pm) else {}
    folder = os.path.join(work, "txt")
    pages = []
    for fn in sorted((f for f in os.listdir(folder) if f.endswith(".txt")), key=lambda f: page_order(f[:-4])):
        name = fn[:-4]
        raw = open(os.path.join(folder, fn), encoding="utf-8").read()
        lines = raw.split("\n")
        hdr = lines[0][4:] if lines and lines[0].startswith("HDR:") else ""
        body = [l.strip() for l in lines[1:] if l.strip() and l.strip() != "=====COL====="]
        ff = os.path.join(work, "txtf", fn)
        fill = [l.strip() for l in open(ff, encoding="utf-8").read().split("\n") if l.strip()] if os.path.exists(ff) else None
        bf = os.path.join(work, "boxes", name + ".json")
        boxes = json.load(open(bf, encoding="utf-8")) if os.path.exists(bf) else []
        pages.append((name, hdr, body, fill, boxes))
    return pages


def sec_of(line):
    if len(line) < 60:
        for s, rx in SEC:
            if rx.search(line):
                return s
    return None


def merge_fill(body, fill, sec):
    """Righe della pagina: dove la sezione è "a completamento" usa l'OCR con i vuoti."""
    if fill is None:
        return body
    out = []
    # parte iniziale (prima del titolo "Quiz a completamento") dall'OCR normale
    if sec != "f":
        for i, l in enumerate(body):
            if sec_of(l) == "f":
                out = body[:i]
                break
        else:
            return body
    chap = re.compile(r"^Capitolo\s+\S{1,3}\s*$")
    j = next((i for i, l in enumerate(fill) if sec_of(l) == "f"), -1)
    k = next((i for i, l in enumerate(fill) if i > j and (sec_of(l) not in (None, "f") or chap.match(l))), len(fill))
    out += fill[max(j, 0):k]
    if k < len(fill):
        if chap.match(fill[k]):
            b = next((i for i, l in enumerate(body) if chap.match(l)), None)
        else:
            nxt = sec_of(fill[k])
            b = next((i for i, l in enumerate(body) if sec_of(l) == nxt), None)
        if b is not None:
            out += body[b:]
    return out


def parse(pages):
    subj, cap, sec = None, None, None
    questions, keys, expls, fills_text, fills_ans = {}, {}, {}, {}, {}
    cur = None  # (key, field) in costruzione
    page_secs = {}
    order = [x for x, _ in SUBJ]
    phase = "q"  # q = quiz, r = risposte

    def new_cap(n, ph, title=True):
        """Il numero di capitolo letto dall'OCR vale solo se resta uguale o avanza di uno
        (l'OCR scambia 10/11 con 19 o 1); all'inizio delle risposte si riparte da 1."""
        nonlocal cap, phase, sec, cur, subj
        if ph != phase:
            if n != 1:
                return
            if ph == "q" and phase == "r" and subj in order[:-1]:
                subj = order[order.index(subj) + 1]  # dopo le risposte inizia la materia successiva
            phase, cap, cur = ph, 1, None
            sec = "rc" if ph == "r" else "c"
            return
        if cap is None or n == cap + 1:
            cap = n
            if title:  # titolo del capitolo: si riparte dai quiz commentati
                cur = None
                sec = "rc" if ph == "r" else "c"

    for name, hdr, body, fill, boxes in pages:
        if os.path.exists(os.path.join(WORK, "txt2", name + ".txt")):
            raw = open(os.path.join(WORK, "txt2", name + ".txt"), encoding="utf-8").read().split("\n")
            body = [l.strip() for l in raw[1:] if l.strip() and l.strip() != "=====COL====="]
        head = " ".join([hdr] + body[:3])
        if re.search(r"\b(SOMMARIO|INDICE)\b", " ".join(body[:6])):
            continue  # indice: elenca tutti i capitoli
        for s_, rx in SUBJ:
            if rx.search(head) and (subj is None or order.index(s_) > order.index(subj)):
                subj, cap, sec, phase, cur = s_, None, None, "q", None
                break
        m = re.search(r"Risposte\s+Capitolo\s+(\d{1,2})", head)
        if m:
            new_cap(int(m.group(1)), "r")
        body = merge_fill(body, fill, sec)
        for b in boxes:
            if subj and cap and b["kind"] in ("keys", "fill"):
                tgt = keys if b["kind"] == "keys" else fills_ans
                sk = "n" if b["kind"] == "keys" else "f"
                for n, v in b["entries"].items():
                    if v:
                        tgt.setdefault((subj, cap, sk, int(n)), v)
        boxed = {b["kind"] for b in boxes}
        page_secs[name] = {sec}
        for line in body:
            # cambi di contesto
            m = CAP.search(line)
            if m and len(line) < 80 and (line.startswith("Capitolo") or "Risposte" in line or "|" in line):
                title = bool(re.match(r"^Capitolo\s+\d{1,2}\s*$", line))
                new_cap(int(m.group(1)), "r" if "Risposte" in line else "q", title)
            elif re.match(r"^Capitolo\s+\S{1,3}\s*$", line) and cap is not None and phase == "q":
                new_cap(cap + 1, "q")  # "Capitolo n" = 11 letto male
            switched = False
            for s_, rx in SEC:
                if rx.search(line) and len(line) < 60:
                    if s_ == sec:
                        switched = True  # testata della pagina: la sezione continua (domanda a cavallo)
                    elif s_ == "rc" and subj is not None:
                        # la prima pagina delle risposte di un capitolo non ha la testata
                        # "Risposte Capitolo N": il titolo "Risposte quiz commentati" apre il capitolo
                        if phase == "q":
                            phase, cap = "r", 1
                        elif cap is not None and any(k[:3] == (subj, cap, "c") for k in keys):
                            cap += 1
                        sec, cur, switched = s_, None, True
                    elif (s_ in ("rc", "rn", "rf")) == (phase == "r"):
                        if s_ == "c" and cap is not None and any(k[:3] == (subj, cap, "n") for k in questions):
                            cap += 1  # "Quiz commentati" dopo i non commentati: capitolo nuovo
                        sec, cur, switched = s_, None, True
                    break
            page_secs[name].add(sec)
            if switched or subj is None or cap is None or sec is None:
                continue
            # titolo "Quiz non commentati" perso dall'OCR: la numerazione ripartirebbe da 1
            if sec == "c":
                mq = QNUM.match(line)
                if mq and int(mq.group(1)) == 1 and any(k[:3] == (subj, cap, "c") and k[3] >= 5 for k in questions):
                    sec = "n"
                    page_secs[name].add(sec)
            base = (subj, cap)
            if sec in ("c", "n"):
                m = QNUM.match(line)
                if m and (not cur or cur[1] != "stem" or len(questions[cur[0]]["stem"]) > 15):
                    k = base + (sec, int(m.group(1)))
                    if k not in questions:
                        questions[k] = {"stem": m.group(2), "opts": {}, "page": name}
                        cur = (k, "stem")
                        continue
                o = OPT.match(line)
                if o and cur:
                    letter = o.group(1) or "C"
                    q = questions[cur[0]]
                    if letter not in q["opts"]:
                        q["opts"][letter] = o.group(3)
                        cur = (cur[0], letter)
                        continue
                if cur:
                    q = questions[cur[0]]
                    if cur[1] == "stem":
                        q["stem"] += " " + line
                    elif len(q["opts"].get(cur[1], "")) < 220:
                        q["opts"][cur[1]] += " " + line
            elif sec == "rc":
                m = ANS_C.match(line)
                if m:
                    k = base + ("c", int(m.group(1)))
                    keys[k] = m.group(2)
                    expls[k] = m.group(3)
                    cur = (k, "expl")
                elif cur and cur[1] == "expl":
                    expls[cur[0]] += " " + line
            elif sec == "rn" and "keys" not in boxed:
                for n, L in ANS_N.findall(line.replace("€", "C").replace("È", "E")):
                    keys.setdefault(base + ("n", int(n)), L)
            elif sec == "f":
                m = re.match(r"^(\d{1,3})\.+\s*(.*)$", line)
                if m:
                    k = base + ("f", int(m.group(1)))
                    fills_text[k] = m.group(2)
                    FILL_PAGE[k] = name
                    cur = (k, "fill")
                elif cur and cur[1] == "fill":
                    fills_text[cur[0]] += " " + line
                # voce successiva finita sulla stessa riga ("... cede. 115.L ____ indica")
                while cur and cur[1] == "fill":
                    nxt = cur[0][3] + 1
                    mm = re.search(r"\s%d\s?\.+\s*(?=\S)" % nxt, fills_text[cur[0]])
                    if not mm:
                        break
                    k = base + ("f", nxt)
                    FILL_PAGE[k] = name
                    fills_text[k] = fills_text[cur[0]][mm.end():]
                    fills_text[cur[0]] = fills_text[cur[0]][:mm.start()]
                    cur = (k, "fill")
            elif sec == "rf" and "fill" not in boxed:
                for n, a in ANS_F.findall(" " + line):
                    fills_ans.setdefault(base + ("f", int(n)), a.strip())
    return questions, keys, expls, fills_text, fills_ans, page_secs


def main():
    global WORK
    work, out = sys.argv[1:3]
    WORK = work
    pages = load(work)
    qs, keys, expls, ft, fa, page_secs = parse(pages)
    # chiavi trascritte a vista dalle scansioni (tabelle illeggibili per l'OCR): prevalgono
    mk = os.path.join(os.environ.get("PREP_ROOT") or os.getcwd(), "tools", "edises_work", "manual_keys.json")
    if os.path.exists(mk):
        man = json.load(open(mk, encoding="utf-8"))
        diff = 0
        for k, v in man.items():
            if k.startswith("_"):
                continue
            s_, cap = k.split("-")
            for i, L in enumerate(v.replace(" ", ""), 1):
                kk = (s_, int(cap), "n", i)
                diff += kk in keys and keys[kk] != L
                keys[kk] = L
        for k, words in man.get("_fill", {}).items():
            s_, cap = k.split("-")
            for i, w in enumerate(words, 1):
                fa[(s_, int(cap), "f", i)] = w
        print(f"chiavi manuali: {diff} diverse dall'OCR")
    with open(out + ".fillpages", "w") as fp:
        fp.write("\n".join(n for n, ss in page_secs.items() if "f" in ss) + "\n")
    items = []
    for k, q in sorted(qs.items()):
        items.append({"key": list(k), "page": q["page"], "stem": re.sub(r"\s+", " ", q["stem"]).strip(),
                      "opts": {l: re.sub(r"\s+", " ", t).strip() for l, t in sorted(q["opts"].items())},
                      "answer": keys.get(k), "expl": re.sub(r"\s+", " ", expls.get(k, "")).strip()})
    fills = [{"key": list(k), "page": FILL_PAGE.get(k, ""), "text": re.sub(r"\s+", " ", t).strip(), "answer": fa.get(k)}
             for k, t in sorted(ft.items())]
    json.dump({"mcq": items, "fill": fills}, open(out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    from collections import Counter
    c = Counter((i["key"][0], i["key"][2]) for i in items)
    full = sum(1 for i in items if len(i["opts"]) == 5 and i["answer"])
    print(f"mcq {len(items)} (complete con chiave: {full}); per materia/sezione: {dict(c)}")
    print(f"completamenti {len(fills)} (con risposta: {sum(1 for f in fills if f['answer'])})")


if __name__ == "__main__":
    main()
