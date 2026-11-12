# build.py: unisce i todo ripuliti (split_*.json) con le correzioni manuali e scrive il file finale
import json, glob, os, re, sys
TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import import_exam as ie
import import_eserciziario as E
S = sys.argv[1]
R = os.environ.get("PREP_ROOT") or os.getcwd()  # cartella dei contenuti (checkout privato)
manual = json.load(open(R + "/tools/eserciziario_manual.json", encoding="utf-8"))
plan = {p["tag"]: p for p in json.load(open(R + "/tools/eserciziario_plan.json"))}
docs, subj_of = ie.load_topic_docs(R)
syl = json.load(open(R + "/content/syllabus.json"))
unit_topics = {u["id"]: [t["id"] for t in u["topics"]] for s in syl["subjects"] for u in s["units"]}
reviewed = set(json.load(open(R + "/tools/eserciziario_reviewed.json")))
# argomenti già pubblicati: restano stabili anche se cambia il testo della spiegazione
OUTF = R + "/content/questions/eserciziario-testbusters-2.json"
prev_pub = {q["id"]: q for q in json.load(open(OUTF, encoding="utf-8"))["questions"]} if os.path.exists(OUTF) else {}
prev_topic = {i: q["topic"] for i, q in prev_pub.items()}
JUNK = re.compile(r"(?:\s+(?:[xX]|NK|[A-Z]?[A-Z]K|BLANK\w*|[^\w\s.,;:)?!%°'’\]]{1,3}))+\s*$")
def junk(s):
    s = JUNK.sub("", s.strip())
    return s[:1].upper() + s[1:] if s[:1].islower() and not re.match(r"^[a-z]{1,3}\b[^a-z]", s) else s
GARBLE = re.compile(r"[“”\"]|\b[A-Z][a-z]?,[A-Z0-9]|\b[A-Z][a-z]?\d?\?|^\W*$|^\W\S{0,3}$|\b(?:BLANK|NK)\b")
def garbled(t):
    return bool(GARBLE.search(t)) or len(t.strip()) < 2

def clean_expl(e):
    e = re.sub(r"^[!|1l]l\s", "Il ", e)
    e = re.sub(r"^l[ìi]\s(?=[a-z])", "Il ", e)
    e = re.sub(r"(^|[.!?]\s)[!|]l\s", r"\1Il ", e)
    e = re.sub(r"(^|[.!?]\s)1 (?=[a-z“\"])", r"\1I ", e)
    e = re.sub(r"(\d)' ?[+→] ?(\d)'?", r"\1' → \2'", e)
    e = re.sub(r"(?<=[a-zà-ù]) 0 (?=[a-zà-ù])", " o ", e)
    e = re.sub(r"(?<=[a-zà-ù])- (?=[a-zà-ù])", "", e)
    e = re.sub(r"\s*Capitolo \d\s*[I|]\s*(?:Biologia|Chimica e propedeutica biochimica|Fisica)\s*", " ", e)
    return re.sub(r"\s{2,}", " ", e).strip()
out, problems, held, sus = [], [], [], {}
for f in sorted(glob.glob(S + "/split_*.json")):
    ch = f.split("split_")[1][:-5]
    meta = plan[ch]; sid = meta["subject"]
    allowed = [t for u in meta["units"] for t in unit_topics[u]]
    clf = ie.Classifier({t: docs[t] for t in allowed}, {t: subj_of[t] for t in allowed})
    for r in json.load(open(f)):
        m = manual.get(r["id"], {})
        if m.get("skip"):
            continue
        key = m.get("answer", r["key"])
        text = m.get("text", r["stem"])
        expl = m.get("explanation", clean_expl(E.tidy(r["expl"])))
        src = f"Eserciziario Testbusters {ch} n.{r['n']} (p. PDF {r['page']})"
        is_fill = m.get("type") == "fill" or (m.get("type") != "mcq" and str(key) not in list("ABCDE"))
        if is_fill and "text" not in m:
            t = re.sub(r"\s*(?:____\s*)+[/|]?\s*$", "", text.strip())     # campo vuoto spurio in fondo
            if "____" not in t:
                t = t.rstrip(" .:") + " ____."
            first = t.index("____")
            t = t[:first + 4] + t[first + 4:].replace("____", "")        # tieni solo il primo campo vuoto
            text = re.sub(r"\s+", " ", t).replace(" .", ".").replace(" ,", ",").strip()
        if not is_fill:
            opts = m.get("options", r["opts"])
            if len(opts) != 5 or any(not o for o in opts) or len(set(opts)) < 5 or key not in list("ABCDE"):
                problems.append(r["id"]); continue
            if "options" not in m:
                opts = [junk(o) for o in opts]
            item = {"id": r["id"], "type": "mcq", "text": text, "options": opts, "answer": "ABCDE".index(key)}
        else:
            ans = m.get("answers") or [key]
            if text.count("____") != 1 or not ans or not ans[0] or len(text.replace("____", "").strip(" .")) < 15:
                problems.append(r["id"]); continue
            item = {"id": r["id"], "type": "fill", "text": text}
            if m.get("numeric"):
                item["numeric"] = m["numeric"]
            else:
                item["answers"] = ans
        top = clf.best(text + " " + " ".join(item.get("options", [])) + " " + expl, sid)
        item.update({"topic": m.get("topic", prev_topic.get(r["id"], top[0][1] if top else allowed[0])), "difficulty": 2, "source": src,
                     "explanation": expl})
        if "text" not in m:
            item["text"] = junk(item["text"]) if not is_fill else re.sub(r"(____)\s+(?:NK|[A-Z]?[A-Z]K)", r"", item["text"])
        bad = [x for x in [item["text"]] + item.get("options", []) if garbled(x)]
        if bad and r["id"] not in m:
            sus[r["id"]] = bad[:3]
        # le modifiche fatte direttamente sul file pubblicato (es. spiegazioni arricchite da altri
        # revisori) restano: vince tools/eserciziario_manual.json, poi il file pubblicato, poi l'OCR
        old = prev_pub.get(r["id"])
        if old and old.get("type") == item["type"]:
            for f in ("text", "options", "answer", "answers", "numeric", "explanation"):
                if f in old and f not in m:
                    item[f] = old[f]
        if ch not in reviewed:
            held.append(item); continue
        out.append(item)
json.dump({"schema": 1, "questions": out}, open(R + "/content/questions/eserciziario-testbusters-2.json", "w", encoding="utf-8"),
          ensure_ascii=False, indent=2)
json.dump({"held": held, "sus": sus}, open(S + "/held.json", "w"), ensure_ascii=False, indent=1)
print(f"pubblicate {len(out)} (capitoli rivisti: {sorted(reviewed)}); in attesa {len(held)}; testo sospetto {len(sus)}")
print(f"scritte {len(out)}; da sistemare {len(problems)}: {' '.join(problems)}")
