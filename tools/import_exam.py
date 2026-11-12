#!/usr/bin/env python3
"""Importa una prova ufficiale commentata (PDF TestBuddy) nella banca domande.

    pdftotext -layout prova.pdf prova.txt
    python3 -I tools/import_exam.py prova.txt content/questions/esame-2025-a1.json --tag "2025 1° appello" --prefix e25a1

- riconosce sezioni (BIOLOGIA / CHIMICA… / FISICA), quesiti a risposta multipla (A–E) e a
  completamento ("Risposta: …"), risposta giusta, spiegazione e "perché le altre sono sbagliate"
- assegna a ogni quesito l'argomento del syllabus più simile, confrontando il testo con le
  sezioni dei libri mappate in content/books.json (va ricontrollato: vedi --report)
- segnala i quesiti con formule rovinate dall'estrazione (campo "_review")
"""

import argparse
import json
import math
import os
import re
import sys
import unicodedata
from collections import Counter

SECTIONS = {"BIOLOGIA": "bio", "CHIMICA E PROPEDEUTICA BIOCHIMICA": "chi", "FISICA": "fis"}
STOP = set("""a ad al alla alle allo agli ai anche che chi ci con cui da dal dalla dalle dallo dai degli dei del della delle dello di
dopo e ed gli ha hanno il in la le lo nel nella nelle nello nei negli non o per piu poi quale quali quando quindi se si
sia sono su sua sue suo suoi tra un una uno viene vengono questo questa queste questi quello quella essere come anche
loro molto ogni altre altri altro risposta giusta sbagliata perche corretta corretto valore viene quindi infatti cosi""".split())


def norm(s):
    s = unicodedata.normalize("NFD", s).encode("ascii", "ignore").decode().lower()
    return re.findall(r"[a-z][a-z0-9]{2,}", s)


def stem(w):
    # stemming minimo italiano
    for suf in ("azioni", "azione", "mente", "ici", "ico", "ica", "iche", "ali", "ale", "i", "e", "o", "a"):
        if len(w) > 5 and w.endswith(suf):
            return w[: -len(suf)]
    return w


def tokens(s):
    return [stem(w) for w in norm(s) if w not in STOP]


def load_topic_docs(root):
    syl = json.load(open(os.path.join(root, "content/syllabus.json"), encoding="utf-8"))
    books = json.load(open(os.path.join(root, "content/books.json"), encoding="utf-8"))
    docs = {}
    subj_of = {}
    for s in syl["subjects"]:
        for u in s["units"]:
            for t in u["topics"]:
                docs[t["id"]] = (t["name"] + " ") * 4
                subj_of[t["id"]] = s["id"]
    # testo delle sezioni mappate
    for b in books["books"]:
        for ch in b["chapters"]:
            path = os.path.join(root, ch["file"])
            if not os.path.exists(path):
                continue
            md = open(path, encoding="utf-8").read()
            for sec in ch.get("sections", []):
                num = sec["section"]
                m = re.search(r"^#{1,3} " + re.escape(num) + r"\b.*$", md, re.M)
                if not m:
                    continue
                start = m.start()
                level = len(m.group(0).split(" ")[0])
                nxt = re.search(r"^#{1,%d} \d" % level, md[m.end():], re.M)
                end = m.end() + nxt.start() if nxt else len(md)
                text = md[start:end][:20000]
                for t in sec["topics"]:
                    docs[t] = docs.get(t, "") + " " + text
    return docs, subj_of


class Classifier:
    def __init__(self, docs, subj_of):
        self.subj_of = subj_of
        tf = {t: Counter(tokens(d)) for t, d in docs.items()}
        df = Counter()
        for c in tf.values():
            df.update(set(c))
        n = len(tf)
        self.idf = {w: math.log((n + 1) / (k + 0.5)) for w, k in df.items()}
        self.vec = {}
        for t, c in tf.items():
            v = {w: (1 + math.log(k)) * self.idf[w] for w, k in c.items()}
            norm_ = math.sqrt(sum(x * x for x in v.values())) or 1
            self.vec[t] = {w: x / norm_ for w, x in v.items()}

    def best(self, text, subject):
        c = Counter(tokens(text))
        q = {w: (1 + math.log(k)) * self.idf.get(w, 0) for w, k in c.items()}
        scores = []
        for t, v in self.vec.items():
            if self.subj_of[t] != subject:
                continue
            scores.append((sum(x * v.get(w, 0) for w, x in q.items()), t))
        scores.sort(reverse=True)
        return scores[:3]


FOOTER = re.compile(r"Semestre filtro 20\d\d|testbuddy\.it")
Q_START = re.compile(r"^\s{0,3}(\d{1,2})\s{2,}(\S.*)$")
OPT = re.compile(r"^\s{4,12}([A-E])\s{2,}(\S.*)$")


def clean(s):
    s = s.replace("​", "").replace(" ", " ")
    s = re.sub(r"[ \t]+", " ", s)
    return fix_units(s.strip())


SUP = str.maketrans("0123456789-−+", "⁰¹²³⁴⁵⁶⁷⁸⁹⁻⁻⁺")


def fix_units(s):
    # potenze di dieci appiattite: "3 × 104" -> "3 × 10⁴", "10−12" -> "10⁻¹²"
    s = re.sub(r"([×x·⋅]\s?)10\s?([−-]?\d{1,2})\b", lambda m: m.group(1) + "10" + m.group(2).translate(SUP), s)
    s = re.sub(r"\b10([−-]\d{1,2})\b", lambda m: "10" + m.group(1).translate(SUP), s)
    # unità con esponente: m3, cm2, m/s2, km2 …
    s = re.sub(r"\b(m|cm|dm|mm|km|s|sec)([23])\b", lambda m: m.group(1) + m.group(2).translate(SUP), s)
    s = s.replace("Kg", "kg").replace("KW", "kW").replace("Km", "km")
    return s


def garbled(lines):
    """righe con formule spezzate (frazioni su più righe, esponenti staccati)"""
    for l in lines:
        if "​" in l:
            return True
        st = l.strip()
        if st and len(l) - len(l.lstrip()) > 30 and len(st) < 40:
            return True
    return False


def parse(txt):
    lines = [l.replace("\f", "") for l in txt.split("\n") if not FOOTER.search(l)]
    section = None
    qs = []
    cur = None
    mode = None
    for raw in lines:
        l = raw.rstrip()
        st = l.strip()
        if st in SECTIONS and not l.startswith(" "):
            section = SECTIONS[st]
            continue
        if section is None:
            continue
        m = Q_START.match(l)
        if m and (cur is None or mode in ("expl", "why")):
            if cur:
                qs.append(cur)
            cur = {"subject": section, "n": int(m.group(1)), "stem": [m.group(2)], "opts": {}, "fill": None,
                   "answer": None, "expl": [], "why": [], "raw": [l]}
            mode = "stem"
            continue
        if cur is None:
            continue
        cur["raw"].append(l)
        if st.startswith("Risposta giusta:"):
            cur["answer"] = st.split(":", 1)[1].strip()
            mode = "expl"
            continue
        if st.startswith("Perché le altre sono sbagliate"):
            mode = "why"
            continue
        if mode in ("stem", "opts"):
            om = OPT.match(l)
            if om:
                cur["opts"][om.group(1)] = om.group(2)
                mode = "opts"
                last = om.group(1)
                continue
            if st.startswith("Risposta:"):
                cur["fill"] = st.split(":", 1)[1].strip()
                continue
            if mode == "opts" and st:
                cur["opts"][last] += " " + st
            elif st:
                cur["stem"].append(st)
        elif mode == "expl":
            cur["expl"].append(l)
        elif mode == "why":
            cur["why"].append(l)
    if cur:
        qs.append(cur)
    return qs


def to_numeric(ans):
    a = ans.replace(" ", "")
    if re.fullmatch(r"[-+]?\d+(?:[.,]\d+)?", a):
        v = float(a.replace(",", "."))
        decimals = len(a.split(",")[1]) if "," in a else (len(a.split(".")[1]) if "." in a else 0)
        tol = 0.01 if decimals == 0 else max(0.005, 0.5 * 10 ** (-decimals) / max(abs(v), 1e-9))
        return {"value": v, "tolerance": round(min(tol, 0.03), 4)}
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("txt")
    ap.add_argument("out")
    ap.add_argument("--tag", required=True)
    ap.add_argument("--prefix", required=True)
    ap.add_argument("--root", default=".")
    ap.add_argument("--fixes", default="", help="JSON {id: {campo: valore}} con correzioni manuali")
    ap.add_argument("--topics", default="", help="JSON {id: topic} con argomenti corretti a mano")
    a = ap.parse_args()
    docs, subj_of = load_topic_docs(a.root)
    clf = Classifier(docs, subj_of)
    qs = parse(open(a.txt, encoding="utf-8").read())
    out = []
    review = 0
    for q in qs:
        stem_ = clean(" ".join(q["stem"]))
        expl = clean(" ".join(x.strip() for x in q["expl"]))
        why = []
        for x in q["why"]:
            xs = x.strip()
            if re.match(r"^[A-E]\)", xs):
                why.append(xs)
            elif why and xs:
                why[-1] += " " + xs
        why = [clean(w) for w in why]
        full_expl = expl + ("\n\nPerché le altre sono sbagliate:\n" + "\n".join("• " + w for w in why) if why else "")
        qid = f"{q['subject']}-{a.prefix}-{q['n']:02d}"
        text_for_clf = stem_ + " " + " ".join(q["opts"].values()) + " " + expl
        top = clf.best(text_for_clf, q["subject"])
        item = {"id": qid, "topic": top[0][1] if top else "", "difficulty": 3, "source": f"esame {a.tag} (TestBuddy)"}
        if q["opts"]:
            letters = sorted(q["opts"])
            item.update({"type": "mcq", "text": stem_, "options": [clean(q["opts"][k]) for k in letters]})
            ans = q["answer"].strip()[:1] if q["answer"] else ""
            item["answer"] = letters.index(ans) if ans in letters else None
        else:
            ans = (q["fill"] or q["answer"] or "").strip()
            item.update({"type": "fill", "text": stem_})
            num = to_numeric(ans)
            if num:
                item["numeric"] = num
            else:
                item["answers"] = [ans]
        item["explanation"] = full_expl
        if garbled(q["raw"]) or item.get("answer", 0) is None:
            item["_review"] = True
            review += 1
        item["_topics"] = [f"{t} {s:.2f}" for s, t in top]
        out.append(item)
    fixes = json.load(open(a.fixes, encoding="utf-8")) if a.fixes else {}
    topics = json.load(open(a.topics, encoding="utf-8")) if a.topics else {}
    for item in out:
        f = fixes.get(item["id"])
        if f:
            item.update(f)
            item.pop("_review", None)
        if item["id"] in topics:
            item["topic"] = topics[item["id"]]
    left = [x["id"] for x in out if x.get("_review")]
    if left:
        print("ancora da rivedere:", left, file=sys.stderr)
    if a.fixes:
        for x in out:
            x.pop("_topics", None)
            x.pop("_review", None)
    json.dump({"schema": 1, "questions": out}, open(a.out, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
    by = Counter((x["subject"] if "subject" in x else x["id"][:3], x["type"]) for x in out)
    print(f"{len(out)} quesiti {dict(by)}; da rivedere (formule): {review}", file=sys.stderr)


if __name__ == "__main__":
    main()
