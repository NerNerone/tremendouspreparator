#!/usr/bin/env python3
"""Stampa compatta di un capitolo della bozza EdiSES per la revisione.

    python3 -I tools/edises_scripts/dump.py bio-01 [c|n|f] [da] [a]

Ogni riga: id | testo | A..E | chiave (* = mancante/dubbia) | spiegazione accorciata.
Applica già tools/edises_fixes.json, così si vede il risultato delle correzioni."""
import json, os, re, sys
R = os.environ.get("PREP_ROOT") or os.getcwd()  # cartella dei contenuti (checkout privato)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from build_edises import clean, qid  # noqa: E402

chap, sec = sys.argv[1], (sys.argv[2] if len(sys.argv) > 2 else "c")
lo = int(sys.argv[3]) if len(sys.argv) > 3 else 1
hi = int(sys.argv[4]) if len(sys.argv) > 4 else 9999
s, cap = chap.split("-")
d = json.load(open(os.path.join(R, "tools/edises_work/draft.json"), encoding="utf-8"))
fx = os.path.join(R, "tools/edises_fixes.json")
fixes = json.load(open(fx, encoding="utf-8")) if os.path.exists(fx) else {}
kind = "fill" if sec == "f" else "mcq"
for it in d[kind]:
    k = it["key"]
    if k[0] != s or k[1] != int(cap) or k[2] != sec or not lo <= k[3] <= hi:
        continue
    i = qid(k)
    f = fixes.get(i, {})
    tag = " [HOLD]" if f.get("hold") else (" [fix]" if f else "")
    if kind == "mcq":
        text = f.get("text", clean(it["stem"]))
        opts = f.get("options", [clean(it["opts"].get(L, ""), True) for L in "ABCDE"])
        ans = f.get("answer", it["answer"])
        flag = "" if ans and all(opts) else "*"
        ex = f.get("explanation", clean(it.get("expl", "")))
        print(f"{i}{tag} | {text}")
        print("   " + " | ".join(f"{L}) {o}" for L, o in zip("ABCDE", opts)) + f"  => {ans}{flag}")
        if ex:
            print("   ~ " + (ex[:260] + "…" if len(ex) > 260 else ex))
    else:
        text = f.get("text", clean(it["text"]))
        ans = f.get("answers", it["answer"])
        flag = "" if ans and "____" in text else "*"
        print(f"{i}{tag} | {text}  => {ans}{flag}")
