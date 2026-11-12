#!/usr/bin/env python3
"""Importa l'eserciziario Testbusters (scansioni OCR) nella banca domande.

    python3 -I tools/import_eserciziario.py <cartella txt OCR> <out.json> [--dump sezioni.txt]

Le pagine OCR (pNNN.txt, NNN = pagina del PDF) vengono concatenate. Per ogni capitolo
dell'eserciziario ci sono gli esercizi numerati (5 opzioni A–E) seguiti dalla
"Correzione commentata" (spiegazione + "Risposta corretta X."). Le domande vengono
abbinate alle correzioni per numero; l'argomento del syllabus si sceglie confrontando
il testo con le sezioni dei libri (come per le prove ufficiali).
"""

import argparse
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(__file__))
import import_exam as ie  # noqa: E402  (classificatore e pulizia testo)

JUNK = re.compile(
    r"Scanned with|CamScanner|Testbusters\s*\|?\s*Eserciziario|Eserciziario commentato|Medicina Semestre|"
    r"^\s*(Capitolo|Biologia|Chimica|Fisica)\s*\d*\s*\|?\s*(Biologia|Chimica|Fisica)?\s*\d*\s*$|^\s*\d{1,3}\s*$",
    re.I,
)
Q_NUM = re.compile(r"^\s*(\d{1,3})\s*[.,)]\s*(\S.*)$")
OPT = re.compile(r"^\s*([A-E])\s*[.,:;)]\s*(\S.*)$")
ANS = re.compile(r"Risposta\s+corretta\s*:?\s*([A-E])\b", re.I)
ANS_WORD = re.compile(r"Risposta\s+corretta\s*:\s*([^\n]{1,60}?)\s*\.(?:\s|$)", re.I)
BLANK_RE = re.compile(r"(?:[_\-—]*\b[A-Z_|Il1]*(?:BLA|LANK|BLAN|ANK|LAN)[A-Z_|]*\b[_\-—,]*\s*)+")
SECTION = re.compile(r"^\s*(\d)\.(\d)\s+([A-ZÀ-Ü][^.?]{8,})$")


def load_pages(folder):
    files = sorted(f for f in os.listdir(folder) if re.fullmatch(r"p\d{3}\.txt", f))
    lines = []
    for f in files:
        page = int(f[1:4])
        for l in open(os.path.join(folder, f), encoding="utf-8", errors="replace").read().split("\n"):
            if not l.strip() or JUNK.search(l):
                continue
            lines.append((page, l.rstrip()))
    return lines


def dehyphen(parts):
    out = ""
    for p in parts:
        p = p.strip()
        if not p:
            continue
        if out.endswith("-") and p[:1].islower():
            out = out[:-1] + p
        else:
            out = (out + " " + p).strip()
    return out


def fix_ocr(s):
    s = s.replace("|", "I").replace("’", "'").replace("‘", "'")
    s = re.sub(r"\bl ", "I ", s) if s.startswith("l ") else s
    s = re.sub(r"\s+", " ", s)
    return s.strip()


def split_blocks(lines):
    """Divide in blocchi: ('q', [righe esercizi]) e ('c', [righe correzioni]), in ordine."""
    blocks = []
    mode = "q"
    cur = []
    for page, l in lines:
        if re.search(r"Correzione\s+commentata", l, re.I):
            blocks.append((mode, cur))
            mode, cur = "c", []
            continue
        if mode == "c":
            # le correzioni di un capitolo non ricominciano mai da 1: un "1." dopo correzioni già
            # concluse è il primo esercizio del capitolo successivo
            m = Q_NUM.match(fix_ocr(l))
            if m and m.group(1) == "1" and any(ANS.search(x) for _, x in cur):
                blocks.append((mode, cur))
                mode, cur = "q", []
        cur.append((page, l))
    blocks.append((mode, cur))
    return [(m, b) for m, b in blocks if b]


def _looks_question_start(lines, page, l):
    return bool(re.search(r"\?|:\s*$|seguenti|Quale|Qual|Indicare|Cosa", l))


QN_DOT = re.compile(r"^\s*(\d{1,3})\s*[.,]\s*(\S.*)$")


def parse_questions(block):
    """Segmenta per struttura: [testo] A. B. C. D. E. [testo] A. … Il numero, se l'OCR lo ha
    perso, si ricava dalla sequenza."""
    items = []  # (page, kind, letter, text)
    for page, raw in block:
        l = fix_ocr(raw)
        om = OPT.match(l)
        items.append((page, "opt" if om else "txt", om.group(1) if om else None, om.group(2) if om else l))
    qs = []
    stem, opts, last, page0 = [], {}, None, None

    def flush():
        nonlocal stem, opts, last
        if len(opts) == 5 and stem:
            qs.append((page0, stem, opts))
        stem, opts, last = [], {}, None

    for page, kind, letter, text in items:
        if kind == "opt":
            if letter == "A" and opts:
                # A senza testo prima: opzione spezzata, ignora la domanda precedente incompleta
                flush()
            if letter == "A" or (last and "ABCDE".index(letter) == "ABCDE".index(last) + 1):
                opts[letter] = [text]
                last = letter
                continue
            # lettera fuori sequenza: trattala come continuazione
        if last == "E":
            prev = opts["E"][-1]
            starts_new = text[:1].isupper() or QN_DOT.match(text) or text[:1].isdigit()
            if starts_new and not prev.endswith("-") and len(opts["E"]) >= 1:
                flush()
                page0 = page
                stem = [text]
                continue
            opts["E"].append(text)
            continue
        if last:
            opts[last].append(text)
        else:
            if not stem:
                page0 = page
            stem.append(text)
    flush()
    out = []
    expect = 1
    for page, stem_lines, opts in qs:
        text = dehyphen(stem_lines)
        m = QN_DOT.match(text)
        n = expect
        if m and expect <= int(m.group(1)) <= expect + 3:
            n = int(m.group(1))
            text = m.group(2)
        elif m and int(m.group(1)) < 200 and len(stem_lines) and QN_DOT.match(stem_lines[0]):
            text = m.group(2)
        # titolo di capitolo finito nel testo della prima domanda
        text = re.sub(r"^\d\.\d\s+[^.?]*?(?=\d{1,2}\s*[.,]\s)", "", text)
        m2 = QN_DOT.match(text)
        if m2 and expect <= int(m2.group(1)) <= expect + 3:
            n, text = int(m2.group(1)), m2.group(2)
        expect = n + 1
        out.append({"n": n, "page": page, "text": text, "options": [dehyphen(opts[k]) for k in "ABCDE"]})
    return out


def parse_fills(block):
    """Esercizi a completamento: paragrafi numerati senza opzioni con il campo vuoto."""
    items, cur = [], None
    for page, raw in block:
        l = fix_ocr(raw)
        m = QN_DOT.match(l)
        if m and not OPT.match(l):
            if cur:
                items.append(cur)
            cur = {"n": int(m.group(1)), "page": page, "lines": [m.group(2)], "opt": False}
            continue
        if cur is None:
            continue
        if OPT.match(l):
            cur["opt"] = True
        cur["lines"].append(l)
    if cur:
        items.append(cur)
    out = []
    for it in items:
        if it["opt"]:
            continue
        text = dehyphen(it["lines"])
        if not BLANK_RE.search(text):
            continue
        text = BLANK_RE.sub(" ____ ", text)
        text = re.sub(r"(?:____\s*(?:[A-Z]{1,2}\b\s*)?)+", "____ ", text)
        text = re.sub(r"____\s*(?:BLANK\w*|BLA\w*|LANK\w*|_+)", "____", text)
        text = re.sub(r"\s+", " ", text).strip()
        if text.count("____") != 1 or len(text) < 25:
            continue
        out.append({"n": it["n"], "page": it["page"], "text": text})
    return out


def parse_corrections(block):
    text_lines = [fix_ocr(l) for _, l in block]
    res = {}
    cur_n, buf = None, []
    expect = 1
    for l in text_lines:
        m = Q_NUM.match(l)
        if m and int(m.group(1)) in (expect, expect + 1, expect + 2) and (
            cur_n is None or ANS.search(" ".join(buf)) or re.search(r"Risposta\s+corretta\s*:", " ".join(buf), re.I)
        ):
            if cur_n is not None:
                res[cur_n] = buf
            cur_n, buf = int(m.group(1)), [m.group(2)]
            expect = cur_n + 1
            continue
        if cur_n is not None:
            buf.append(l)
    if cur_n is not None:
        res[cur_n] = buf
    out = {}
    for n, b in res.items():
        t = dehyphen(b)
        m = ANS.search(t)
        mw = ANS_WORD.search(t)
        if m and (not mw or m.start() <= mw.start()) and re.match(r"[A-E]\b", t[m.start(1):]):
            out[n] = {"answer": m.group(1).upper(), "explanation": t[: m.start()].strip()}
        elif mw:
            word = BLANK_RE.sub(" ", mw.group(1)).strip(" «»\"'")
            word = " ".join(word.split()[:4])
            word = re.split(r"\s+\d{1,3}\s*[.,]|\s+[A-Z]{2,}\b", word)[0].strip(" ,;")
            if word:
                out[n] = {"answer": None, "word": word, "explanation": t[: mw.start()].strip()}
    return out


FOOT = re.compile(r"\s*\S*(?:serciz|ommentat|Testbuster|Semestre\s+Filtro)\S*.*$", re.I)


SUP = str.maketrans("0123456789-−+", "⁰¹²³⁴⁵⁶⁷⁸⁹⁻⁻⁺")


def light_clean(s):
    """come import_exam.clean ma senza trasformare gli intervalli "10-50" in potenze"""
    s = s.replace("\u200b", "").replace("\u00a0", " ")
    s = re.sub(r"([×x·⋅]\s?)10\s?([−-]?\d{1,2})\b", lambda m: m.group(1) + "10" + m.group(2).translate(SUP), s)
    s = re.sub(r"\b(m|cm|dm|mm|km|s)([23])\b", lambda m: m.group(1) + m.group(2).translate(SUP), s)
    return re.sub(r"[ \t]+", " ", s).strip()


def tidy(s):
    s = BLANK_RE.sub(" ", s)
    s = FOOT.sub("", s)
    return light_clean(s)


def garbled_text(s):
    """testo rovinato dall'OCR (colonne mescolate, numeri di altri esercizi, simboli)"""
    if re.search(r"\b\d{1,3}\s*\.\s+\d{1,3}\s*\.", s):
        return True
    letters = sum(c.isalpha() for c in s)
    return len(s) > 0 and letters / len(s) < 0.6


def _nums(s):
    """numeri presenti in un testo, normalizzati (virgola decimale, spazi)"""
    out = set()
    for m in re.finditer(r"\d+(?:[.,]\d+)?", s.replace(" ", "")):
        t = m.group(0).replace(",", ".").rstrip("0").rstrip(".") if "." in m.group(0) or "," in m.group(0) else m.group(0)
        out.add(t)
    return out


def numeric_consistent(options, answer, explanation):
    """Per opzioni numeriche: il valore della risposta deve comparire nella spiegazione."""
    numeric = [o for o in options if re.match(r"^\s*[-+]?\d", o) and len(o) < 30]
    if len(numeric) < 3:
        return True
    lead = re.match(r"^\s*([-+]?\d+(?:[.,]\d+)?)", options[answer])
    if not lead:
        return True  # es. "Nessuna delle precedenti"
    return bool(_nums(lead.group(1)) & _nums(explanation))


def _toks(s):
    return set(w for w in ie.tokens(s) if len(w) > 3)


def align(q, cs, used, window=6, min_score=0.08):
    """Sceglie la correzione più coerente con la domanda tra i numeri vicini.
    Punteggio: sovrapposizione lessicale tra (testo + opzioni) e spiegazione, con bonus se la
    spiegazione cita il testo dell'opzione indicata come corretta. Scarta le incoerenze
    ("risposta X errata" proprio per la X indicata come corretta)."""
    qt = _toks(q["text"] + " " + " ".join(q["options"]))
    best, best_n, second = 0.0, None, 0.0
    for n in range(max(1, q["n"] - window), q["n"] + window + 1):
        c = cs.get(n)
        if not c or n in used or not c.get("answer"):
            continue
        et = _toks(c["explanation"])
        if not et or not qt:
            continue
        score = len(qt & et) / len(qt | et)
        ans = "ABCDE".index(c["answer"])
        ot = _toks(q["options"][ans])
        if ot and len(ot & et) / len(ot) >= 0.5:
            score += 0.05
        if re.search(r"risposta\s+%s\s+errat" % c["answer"], c["explanation"], re.I):
            score = 0
        if n == q["n"]:
            score += 0.02
        if score > best:
            best, best_n, second = score, n, best
        elif score > second:
            second = score
    if best_n is None or best < min_score or best - second < 0.02:
        return None, None
    return cs[best_n], best_n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("folder")
    ap.add_argument("out")
    ap.add_argument("--root", default=".")
    ap.add_argument("--plan", required=True,
                    help="JSON: [{subject, tag, start, corr, end, units}] pagine PDF di esercizi e correzioni")
    ap.add_argument("--topics", default="", help="JSON {id: topic} correzioni manuali degli argomenti")
    a = ap.parse_args()
    lines = load_pages(a.folder)
    plan = json.load(open(a.plan, encoding="utf-8"))
    docs, subj_of = ie.load_topic_docs(a.root)
    syl = json.load(open(os.path.join(a.root, "content/syllabus.json"), encoding="utf-8"))
    unit_topics = {u["id"]: [t["id"] for t in u["topics"]] for s_ in syl["subjects"] for u in s_["units"]}
    manual = json.load(open(a.topics, encoding="utf-8")) if a.topics else {}
    report, out = [], []
    for meta in plan:
        allowed = [t for u in meta["units"] for t in unit_topics[u]]
        clf = ie.Classifier({t: docs[t] for t in allowed}, {t: subj_of[t] for t in allowed})
        qb = [(p, l) for p, l in lines if meta["start"] <= p < meta["corr"]]
        cb = [(p, l) for p, l in lines if meta["corr"] <= p <= meta["end"]]
        # sulla prima pagina degli esercizi può esserci la coda delle correzioni precedenti
        for i, (p, l) in enumerate(qb):
            if p != meta["start"]:
                break
            if re.match(r"^\s*\d\s*[.,]\s*\d\s", l) or re.search(r"Risposta\s+corretta", l, re.I):
                qb = qb[i + 1:] if not re.match(r"^\s*\d\s*[.,]\s*\d\s", l) else qb[i:]
        qs = parse_questions(qb)
        cs = parse_corrections(cb)
        ok = 0
        rejected = 0
        sid = meta["subject"]
        used = set()
        for q in qs:
            if garbled_text(q["text"]) or any(not o.strip() or garbled_text(o) and len(o) > 12 for o in q["options"]):
                rejected += 1
                continue
            c, n_used = align(q, cs, used)
            if not c:
                rejected += 1
                continue
            if not numeric_consistent(q["options"], "ABCDE".index(c["answer"]), c["explanation"]):
                rejected += 1
                continue
            used.add(n_used)
            q["n"] = n_used
            qid = f"{sid}-tb{meta['tag'].replace('.', '')}-{q['n']:03d}"
            text = BLANK_RE.sub(" ", q["text"])
            lead = re.match(r"^.{0,40}?\b(\d{1,3})\s*[.,]\s+(?=[A-ZÀ-Ü])", text)
            if lead:
                text = text[lead.end():]
            text = light_clean(text)
            opts = [tidy(o) for o in q["options"]]
            if any(not o for o in opts):
                rejected += 1
                continue
            top = clf.best(text + " " + " ".join(opts) + " " + c["explanation"], sid)
            out.append({
                "id": qid, "topic": manual.get(qid, top[0][1] if top else allowed[0]), "type": "mcq", "difficulty": 2,
                "source": f"Eserciziario Testbusters {meta['tag']} n.{q['n']} (p. PDF {q['page']})",
                "text": text, "options": opts, "answer": "ABCDE".index(c["answer"]),
                "explanation": tidy(c["explanation"]),
            })
            ok += 1
        # completamenti
        fills = parse_fills(qb)
        okf = 0
        for f in fills:
            if garbled_text(f["text"].replace("____", "")):
                continue
            best, best_n, second = 0.0, None, 0.0
            ft = _toks(f["text"])
            for n in range(max(1, f["n"] - 4), f["n"] + 5):
                c = cs.get(n)
                if not c or n in used or not c.get("word"):
                    continue
                et = _toks(c["explanation"])
                sc = len(ft & et) / max(1, len(ft | et)) + (0.02 if n == f["n"] else 0)
                if sc > best:
                    best, best_n, second = sc, n, best
                elif sc > second:
                    second = sc
            if best_n is None or best < 0.07 or best - second < 0.02:
                continue
            used.add(best_n)
            c = cs[best_n]
            word = c["word"]
            qid = f"{sid}-tb{meta['tag'].replace('.', '')}-{best_n:03d}"
            item = {
                "id": qid, "type": "fill", "difficulty": 2,
                "source": f"Eserciziario Testbusters {meta['tag']} n.{best_n} (p. PDF {f['page']})",
                "text": light_clean(f["text"]), "explanation": tidy(c["explanation"]),
            }
            num = ie.to_numeric(re.sub(r"\s*[a-zA-Zµ°/²³%]+.*$", "", word)) if re.match(r"^[-+]?\d", word) else None
            if num and not (_nums(word) & _nums(c["explanation"])) and len(c["explanation"]) > 40:
                continue  # risultato non coerente con la spiegazione
            if num:
                item["numeric"] = num
                unit = word[len(re.match(r"^[-+]?[\d.,]+", word).group(0)):].strip()
                if unit:
                    item["numeric"]["unit"] = unit
            else:
                variants = [word]
                if word.lower() != word:
                    variants.append(word.lower())
                item["answers"] = variants
            top = clf.best(item["text"] + " " + word + " " + c["explanation"], sid)
            item["topic"] = manual.get(qid, top[0][1] if top else allowed[0])
            out.append(item)
            okf += 1
        report.append(f"{meta['tag']} ({sid}): {len(qs)} esercizi a scelta, {ok} abbinati, {rejected} scartati; "
                      f"{len(fills)} completamenti, {okf} abbinati; {len(cs)} correzioni")
    json.dump({"schema": 1, "questions": out}, open(a.out, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
    print("\n".join(report), file=sys.stderr)
    print(f"totale: {len(out)} domande", file=sys.stderr)


if __name__ == "__main__":
    main()
