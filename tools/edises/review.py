#!/usr/bin/env python3
"""Vista compatta per la revisione: una riga per domanda con la sola risposta indicata.

    python3 -I tools/edises_scripts/review.py bio-01 [c|n|f|all]

Formato: n chiave | domanda | => testo della risposta indicata   [! motivi di sospetto]
Sospetti automatici: opzioni mancanti, chiave mancante, testo sporco, e per i commentati
una spiegazione che "parla" di un'altra opzione più che di quella indicata."""
import json, os, re, sys
R = os.environ.get("PREP_ROOT") or os.getcwd()  # cartella dei contenuti (checkout privato)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from build_edises import clean, qid  # noqa: E402

STOP = set("il lo la i gli le un una uno di a da in con su per tra fra e o ed che è non sono del della dei delle al alla ai alle nel nella si come più anche può essere sia".split())


def words(t):
    return {w for w in re.findall(r"[a-zà-ù0-9]{3,}", t.lower()) if w not in STOP}


def suspicious(text, opts, ans, ex):
    why = []
    if not ans:
        why.append("chiave?")
    if any(not o for o in opts):
        why.append("opzioni?")
    if re.search(r"[|®©_]{1}|[^\x00-\x7Fàèéìòùáíóú’‘“”–—°±·×÷²³¹⁰-⁹₀-₉αβγδΔμπσλωΩ→←↔⇌≈≠≤≥√∞°%€]", text + " ".join(opts)):
        why.append("sporco")
    if ex and ans and all(opts):
        w = words(ex)
        sc = [len(words(o) & w) / (len(words(o)) or 1) for o in opts]
        k = "ABCDE".index(ans)
        if max(sc) > 0 and sc[k] < max(sc) and sorted(sc)[-1] - sc[k] > 0.34:
            why.append(f"spieg→{'ABCDE'[sc.index(max(sc))]}")
    return why


chap, sec = sys.argv[1], (sys.argv[2] if len(sys.argv) > 2 else "all")
s, cap = chap.split("-")
d = json.load(open(os.path.join(R, "tools/edises_work/draft.json"), encoding="utf-8"))
fx = os.path.join(R, "tools/edises_fixes.json")
fixes = json.load(open(fx, encoding="utf-8")) if os.path.exists(fx) else {}
for kind in ("mcq", "fill"):
    for it in d[kind]:
        k = it["key"]
        if k[0] != s or k[1] != int(cap) or (sec != "all" and k[2] != sec):
            continue
        i = qid(k)
        f = fixes.get(i, {})
        if f.get("hold"):
            print(f"{k[2]}{k[3]:03d} HOLD")
            continue
        if kind == "mcq":
            text = f.get("text", clean(it["stem"]))
            opts = f.get("options", [clean(it["opts"].get(L, ""), True) for L in "ABCDE"])
            ans = f.get("answer", it["answer"])
            ex = f.get("explanation", clean(it.get("expl", "")))
            why = suspicious(text, opts, ans, ex)
            at = opts["ABCDE".index(ans)] if ans else "?"
            line = f"{k[2]}{k[3]:03d} {ans or '?'} | {text[:220]} | => {at[:120]}"
            if why:
                line += "   [! " + ", ".join(why) + "] " + " / ".join(f"{L}){o[:50]}" for L, o in zip("ABCDE", opts))
            print(line + (" (fix)" if f else ""))
        else:
            text = f.get("text", clean(it["text"]))
            ans = f.get("answers", clean(it["answer"] or "") or None)
            if "text" not in f and text.count("____") == 2 and ans and ans.count("/") == 1:
                a1, a2 = [x.strip() for x in ans.split("/")]
                text, ans = text.replace("____", a1, 1), a2
            flag = "" if ans and "____" in text else "   [! vuoto/risposta?]"
            print(f"{k[2]}{k[3]:03d} | {text[:260]} => {ans}{flag}" + (" (fix)" if f else ""))
