#!/usr/bin/env python3
"""Seconda passata sull'eserciziario: recupera gli esercizi non importati.

    python3 -I tools/eserciziario_pass2.py <cartella txt OCR> <auto.json> <da_rivedere.json>

1. abbina per numero esplicito ("37.") domanda e correzione con lo stesso numero, se i
   controlli di coerenza passano (chiave non dichiarata errata, risultato numerico presente);
2. tutto il resto finisce in <da_rivedere.json> con pagina, testo OCR e correzione, per la
   trascrizione manuale dalle immagini (le formule perdono pedici/apici con l'OCR).
"""

import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(__file__))
import import_eserciziario as E  # noqa: E402
import import_exam as ie  # noqa: E402

NUM_IN_STEM = re.compile(r"^.{0,40}?\b(\d{1,3})\s*[.,]\s+(?=\S)")


def main():
    folder, out_auto, out_todo = sys.argv[1:4]
    root = "."
    lines = E.load_pages(folder)
    plan = json.load(open(os.path.join(root, "tools/eserciziario_plan.json"), encoding="utf-8"))
    have = json.load(open(os.path.join(root, "content/questions/eserciziario-testbusters.json"), encoding="utf-8"))
    have_ids = {q["id"] for q in have["questions"]}
    docs, subj_of = ie.load_topic_docs(root)
    syl = json.load(open(os.path.join(root, "content/syllabus.json"), encoding="utf-8"))
    unit_topics = {u["id"]: [t["id"] for t in u["topics"]] for s in syl["subjects"] for u in s["units"]}
    auto, todo = [], []
    for meta in plan:
        sid = meta["subject"]
        tag = meta["tag"].replace(".", "")
        allowed = [t for u in meta["units"] for t in unit_topics[u]]
        clf = ie.Classifier({t: docs[t] for t in allowed}, {t: subj_of[t] for t in allowed})
        qb = [(p, l) for p, l in lines if meta["start"] <= p < meta["corr"]]
        cb = [(p, l) for p, l in lines if meta["corr"] <= p <= meta["end"]]
        cs = E.parse_corrections(cb)
        qs = E.parse_questions(qb)
        by_num = {}
        for q in qs:
            m = NUM_IN_STEM.match(q["text"])
            if m:
                n = int(m.group(1))
                q2 = dict(q, text=q["text"][m.end():], n=n)
                by_num.setdefault(n, q2)
        # testo OCR grezzo per numero (anche senza opzioni riconosciute)
        raw_by_num, cur, seen = {}, None, set()
        for p, l in qb:
            fl = E.fix_ocr(l)
            m = E.QN_DOT.match(fl)
            if m and not E.OPT.match(fl):
                n = int(m.group(1))
                # "B." letto come "8.", "D." come "0.": dentro le opzioni non è una nuova domanda
                if cur is not None and n == 8 and "A" in seen and "B" not in seen:
                    fl = "B. " + m.group(2)
                elif cur is not None and n == 0 and "C" in seen and "D" not in seen:
                    fl = "D. " + m.group(2)
                elif 1 <= n <= 150:
                    cur, seen = n, set()
                    raw_by_num.setdefault(cur, {"page": p, "lines": []})
            om = E.OPT.match(fl)
            if om:
                seen.add(om.group(1))
            if cur is not None and len(raw_by_num[cur]["lines"]) < 45:
                raw_by_num[cur]["lines"].append(fl)
        for n in sorted(cs):
            qid = f"{sid}-tb{tag}-{n:03d}"
            if qid in have_ids:
                continue
            c = cs[n]
            q = by_num.get(n)
            ok = False
            if os.environ.get("NOAUTO") is None and q and c.get("answer") and not E.garbled_text(q["text"]):
                ans = "ABCDE".index(c["answer"])
                opts = [E.tidy(o) for o in q["options"]]
                contradict = re.search(r"risposta\s+%s\s+errat" % c["answer"], c["explanation"], re.I)
                if all(opts) and not contradict and E.numeric_consistent(q["options"], ans, c["explanation"]):
                    text = ie.clean(E.BLANK_RE.sub(" ", q["text"]))
                    top = clf.best(text + " " + " ".join(opts) + " " + c["explanation"], sid)
                    auto.append({
                        "id": qid, "topic": top[0][1] if top else allowed[0], "type": "mcq", "difficulty": 2,
                        "source": f"Eserciziario Testbusters {meta['tag']} n.{n} (p. PDF {q['page']})",
                        "text": text, "options": opts, "answer": ans, "explanation": E.tidy(c["explanation"]),
                    })
                    ok = True
            if not ok:
                raw = raw_by_num.get(n, {})
                todo.append({
                    "id": qid, "subject": sid, "chapter": meta["tag"], "n": n, "units": meta["units"],
                    "page": raw.get("page") or (q or {}).get("page"),
                    "q_pages": [meta["start"], meta["corr"] - 1],
                    "ocr": " ".join(raw.get("lines", []))[:900],
                    "answer": c.get("answer"), "word": c.get("word"),
                    "explanation": E.tidy(c["explanation"]),
                })
    json.dump({"schema": 1, "questions": auto}, open(out_auto, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    json.dump(todo, open(out_todo, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print(f"recuperate automaticamente: {len(auto)}; da trascrivere: {len(todo)}", file=sys.stderr)


if __name__ == "__main__":
    main()
