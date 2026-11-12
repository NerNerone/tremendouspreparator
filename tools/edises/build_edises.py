#!/usr/bin/env python3
"""Dalla bozza dell'eserciziario EdiSES alle domande dell'app, solo per i capitoli già rivisti.

    python3 -I tools/build_edises.py

- tools/edises_work/draft.json: bozza di tools/import_edises.py (domande, chiavi, spiegazioni);
- tools/edises_fixes.json: { "bio-ed01-n042": {campo: valore, ...} } correzioni della revisione
  (text, options, answer [lettera], answers, numeric, explanation, topic; "hold": true = fuori dall'app);
- tools/edises_reviewed.json: ["bio-01", ...] capitoli rivisti domanda per domanda: solo questi
  entrano in content/questions/edises-<materia>.json.
"""
import json
import os
import re
import sys

R = os.environ.get("PREP_ROOT") or os.getcwd()  # cartella dei contenuti (checkout privato)
TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)
import import_exam as ie  # noqa: E402

DRAFT = os.path.join(R, "tools/edises_work/draft.json")
FIXES = os.path.join(R, "tools/edises_fixes.json")
REVIEWED = os.path.join(R, "tools/edises_reviewed.json")

SUBJ_NAME = {"chi": "Chimica", "bio": "Biologia", "fis": "Fisica"}
SEC_NAME = {"c": "quiz commentato", "n": "quiz non commentato", "f": "quiz a completamento"}
# capitolo del libro -> unità del syllabus tra cui scegliere l'argomento
UNITS = {
    "chi": {1: ["chi-u1", "chi-u2"], 2: ["chi-u1"], 3: ["chi-u1"], 4: ["chi-u1"], 5: ["chi-u1"],
            6: ["chi-u2"], 7: ["chi-u3", "chi-u1"], 8: ["chi-u3"], 9: ["chi-u4", "chi-u1"], 10: ["chi-u1"],
            11: ["chi-u4"], 12: ["chi-u5", "chi-u6"], 13: ["chi-u7"]},
    "bio": {1: ["bio-u1"], 2: ["bio-u2", "bio-u3"], 3: ["bio-u3"], 4: ["bio-u4"], 5: ["bio-u5"],
            6: ["bio-u6"], 7: ["bio-u7"]},
    "fis": {1: ["fis-u1"], 2: ["fis-u2"], 3: ["fis-u2"], 4: ["fis-u3"], 5: ["fis-u5"], 6: ["fis-u6"],
            7: ["fis-u4", "fis-u7"], 8: ["fis-u7"]},
}
# immagine del PDF -> pagina stampata
OFFSET = {"part0a": -4, "part0b": 96, "part0c": 196, "part1": 300, "part2": 400, "part3": 500,
          "part4": 600, "part5": 701, "part6": 768, "part7": 835}

# testate e piè di pagina finiti nel testo
HEAD = re.compile(r"(?:\b\d{2,3}\s*\|?\s*)?(?:(?:Chimica e Propedeutica\s*\S*|Biologia e Genetica|Fisica)\s*\|?\s*)?"
                  r"(?:Quiz (?:non )?commentati|Quiz a completamento|Risposte Capitolo \d+)(?:\s*\|?\s*[\[(]?\d{2,3}[\])]?)?")
CAPLINE = re.compile(r"\s*\bCapitolo\s+\d+[.:]?\s*(?:\|\s*)?[^?!|]{0,120}?\|\s*\d{1,3}\b")
RULES = [
    (HEAD, " "),
    # lettere delle opzioni incollate alla congiunzione: "(A, B, Ce D)" → "C e D"
    (re.compile(r"\b([A-E])e ([A-E])\b"), r"\1 e \2"),
    # numero attaccato all'unità di misura: "200mL" → "200 mL"
    (re.compile(r"(?<![\w.,!*°“‘'])(\d+(?:[.,]\d+)?)(km/h|m/s|mmHg|Torr|torr|mbar|bar|kcal|cal|mmol|µmol|mol|GHz|MHz|kHz|Hz|kPa|MPa|Pa|KeV|keV|MeV|GeV|kJ|kW|MW|mV|kV|dm|cm|mm|µm|nm|km|mL|ml|atm|kg|mg|m|L|g|M|N)\b"), r"\1 \2"),
    (CAPLINE, " "),
    # testate di capitolo e piè di pagina residui
    (re.compile(r"\s*(?:l?l'?|La\s+)?organizzazione biologica e molecolare della vita\s*\|\s*\d{2,3}\b"), " "),
    (re.compile(r"\s*(?:Il\s+)?controllo dell'informazione genetica e epigenetica\s*\|\s*\d{2,3}\b"), " "),
    (re.compile(r"\s*(?:tras)?missione e controllo dei caratteri selvatici e mutati\s*\|\s*\d{2,3}\b"), " "),
    (re.compile(r"\s*(?:s?t)?rutture cellulari: biogenesi, morfologia e funzioni\s*\|\s*\d{2,3}\b"), " "),
    (re.compile(r"\s*(?:se?|a?|e?)gnalazione cellulare e la trasduzione del segnale\s*\|\s*\d{2,3}\b"), " "),
    (re.compile(r"\s*(?:_+\s*)?<P\s+ce\b"), " "),
    (re.compile(r"\b11\s*(\d{1,3}%)\b"), r"Il \1"),
    (re.compile(r"\bInuna\b"), "In una"),
    (re.compile(r"\bhauna\b"), "ha una"),
    (re.compile(r"\bLaluce\b"), "La luce"),
    (re.compile(r"\bPerla legge\b"), "Per la legge"),
    # cloro letto come "CI"
    (re.compile(r"\b(H|Na|K|Ca|Mg|Ba|Cu|Ag|Hg|Al|Fe|Zn|Li|Sr|Pb|NH₄|C|CH₃|CH₂)CI(?=[₂₃₄\s,.;:)⁻]|$)"), r"\1Cl"),
    (re.compile(r"(?:(?<=\s)|^)ll (?=[a-zàèéìòù])"), "Il "),
    # "sì" letto al posto del pronome "si" davanti a un verbo
    (re.compile(r"\bsì (?=(?:ossid|ridu|form|trov|ottien|sciogl|dissoc|dic|defin|raffredd|spost|riscald|scind|lega|comport|osserv|verific|pass|chiam|aggiung|consum|produc|ha\b|hanno\b|può\b))"), "si "),
    # fasi del ciclo cellulare con pedici persi
    (re.compile(r"\bG,, S,? (e )?G,"), lambda m: "G₁, S" + (" e " if m.group(1) else ", ") + "G₂"),
    # articoli incollati alla parola seguente: "Unatomo", "Ilbrano", "Èun"
    (re.compile(r"\b([Uu]n)(?=(?:atom|alchen|alcan|alchin|acid|ossid|elettr|isotop|ester|eter|enzim|orbital|alcol|ammin|ammid|aldeid|anion|elemento|esempio|altro|altra|insieme)[a-z]*\b)"), r"\1 "),
    (re.compile(r"\bIl(?=[b-df-hj-km-np-tv-z][a-zà-ù]{3,})"), "Il "),
    (re.compile(r"\bÈ(?=un\b|una\b|uno\b|il\b|la\b)"), "È "),
    # numeri romani letti come parole: "nel Il gruppo", "Ill gruppo"
    (re.compile(r"\b(nel|del|al|dal|il|Nel|Il|un) I[l1|]l? (gruppo|periodo)\b"), lambda m: m.group(1) + " II" + ("I" if m.group(0).count("l") + m.group(0).count("1") >= 3 else "") + " " + m.group(2)),
    (re.compile(r"\bIll (gruppo|periodo)\b"), r"III \1"),
    (re.compile(r"\s*\bCapitolo\s+\d+\b[^.?!:]{0,30}$"), ""),
    (re.compile(r"\bCapitolo\s+\d+\s+(?:Le basi\S*|Lebasi\S*|Le:?|Il flusso\S*)\s"), " "),
    (re.compile(r"\S*(?:edises|EdiSES)\S*|www\.\S+"), " "),
    (re.compile(r"\s(?:MPA|raises\.?|rises\.?|[rcp]?[a-z]?dises\S*|sas\.|paises\.?)(?:\s+(?:[A-Z%:!|=&\d][A-Za-z%:!|=&\d]{0,6}|_+))*(?=\s|$)"), " "),
    (re.compile(r"€"), "C"),
    (re.compile(r"(^|[.?:;]\s+)\|\s+(?=[a-zA-Z_])"), r"\1I "),
    (re.compile(r"^(?:l|\|)(?=(?:legami|virus|batteri|geni|lipidi|procarioti|cromosomi)\b)"), "I "),
    (re.compile(r"\bEC-\s*CETTO\b"), "ECCETTO"),
    (re.compile(r"\bNON\b-\s*"), "NON "),
    (re.compile(r"\s+([,.;:!?])(?=\s|$)"), r"\1"),
    (re.compile(r"([a-zà-ù])- ([a-zà-ù])"), r"\1\2"),
    (re.compile(r"(?:____\s*){2,}"), "____ "),
    (re.compile(r"____\.\s(?=[a-zà-ù])"), "____, "),
    (re.compile(r"(^|\s)L'?\s+____"), r"\1L'____"),
    (re.compile(r"(^|[.?:]\s+)II\s(?=[a-zA-Z_])"), r"\1Il "),
    (re.compile(r"\bH[,2]0\b|\bH,O\b"), "H₂O"),
    (re.compile(r"\bC[O0][,:;](?=\s|$)"), "CO₂"),
    (re.compile(r"\b(?:L)?O,(?=\s)"), "O₂"),
    (re.compile(r"\bNAD[*+]"), "NAD⁺"),
    (re.compile(r"\bNADP[*+]"), "NADP⁺"),
    (re.compile(r"\bFADH[;,:2]"), "FADH₂"),
    (re.compile(r"\bA([GHSU])(?=\s*[<>=])"), r"Δ\1"),
    (re.compile(r"\ba-elica"), "α-elica"),
    (re.compile(r"\bB-foglie"), "β-foglie"),
    (re.compile(r"\bum\b(?!\.a\.)"), "µm"),
    (re.compile(r"\bu\.?m\.?a\.?(?=\s|$|[,;)])"), "u.m.a."),
    (re.compile(r"\b(Na|K|H|NAD|NADP|Ca2|Mg2|Fe2|Fe3|Cl|HCO3)[*](?=[\s/,;:.)]|$)"), lambda m: m.group(1).replace("Ca2", "Ca²").replace("Mg2", "Mg²").replace("Fe2", "Fe²").replace("Fe3", "Fe³") + ("⁻" if m.group(1) in ("Cl", "HCO3") else "⁺")),
    (re.compile(r"\bC[O0]2\b"), "CO₂"),
    (re.compile(r"\bCa(?:\?\*|2\+|t\*|\?\+|\?)(?=[\s,.;:)]|$)"), "Ca²⁺"),
    (re.compile(r"\bIP[,;:3](?=[\s,.;:)]|$)"), "IP₃"),
    (re.compile(r"(^|[\s(/])[O0]2(?=[\s,/)]|$)"), r"\1O₂"),
    (re.compile(r"\b(Complesso|complesso|polimerasi|Topoisomerasi|topoisomerasi|Fotosistema|fotosistema|classe|tipo|meiosi|Meiosi|profase|metafase|anafase|telofase) Il\b"), r"\1 II"),
    (re.compile(r"\b[BRf]-(ossidazion|barrel|foglie|galattosid|globin|lattam|amiloid|tubulin|bloccant|carotene|endorfin)"), r"β-\1"),
    (re.compile(r"\ba-(elic|tubulin|amilas|globin|actinin|cheratin|amminoacid)"), r"α-\1"),
    (re.compile(r"\by(TuRC|-tubulin)"), r"γ\1"),
    (re.compile(r"\bAF508\b"), "ΔF508"),
    (re.compile(r"\s{2,}"), " "),
]


VOCAB = {}


def vocab():
    """Frequenza delle parole nella bozza: serve a dividere le parole incollate dall'OCR."""
    if not VOCAB:
        import collections
        d = json.load(open(DRAFT, encoding="utf-8"))
        c = collections.Counter()
        for it in d["mcq"]:
            for t in [it["stem"], it.get("expl", "")] + list(it["opts"].values()):
                c.update(w.lower() for w in re.findall(r"[A-Za-zÀ-ÿ]+", t))
        for it in d["fill"]:
            c.update(w.lower() for w in re.findall(r"[A-Za-zÀ-ÿ]+", it["text"]))
        VOCAB.update(c)
    return VOCAB


SHORT = {"tutti", "tutte", "qual", "un", "uno", "negli", "nei", "sul", "sulla", "ad", "ed", "il", "lo", "la", "i", "gli", "le", "un", "una", "di", "a", "da", "in", "con", "su", "per", "e", "o",
         "nel", "nella", "del", "della", "al", "alla", "è", "si", "che", "non", "dei", "delle", "tra", "fra"}


def split_glued(word):
    """"Tuttigliorganismi" -> "Tutti gli organismi" se ogni pezzo è una parola frequente."""
    v = vocab()
    lw = word.lower()
    # solo parole maiuscole a inizio frase/riga: l'OCR incolla l'articolo ("Ilsistema")
    if len(lw) < 5 or v.get(lw, 0) >= 3 or not word[0].isupper() or word[1:2].isupper():
        return word
    best = {0: (0.0, [])}
    for i in range(len(lw)):
        if i not in best:
            continue
        for j in range(i + 1, len(lw) + 1):
            piece = lw[i:j]
            f = v.get(piece, 0)
            ok = piece in SHORT or (len(piece) >= 4 and f >= 8) or (len(piece) == 3 and f >= 60)
            if not ok:
                continue
            sc = best[i][0] + (1.0 if piece in SHORT else 2.0) - 3.0  # meno pezzi possibile
            if j not in best or sc > best[j][0]:
                best[j] = (sc, best[i][1] + [(i, j)])
    if len(lw) not in best or len(best[len(lw)][1]) < 2:
        return word
    pieces = [lw[i:j] for i, j in best[len(lw)][1]]
    if pieces[0] not in SHORT or any(len(p) < 2 for p in pieces[1:]) or lw in {"ecori", "italia", "osono"}:
        return word
    return " ".join(word[i:j] for i, j in best[len(lw)][1])


SUP = str.maketrans("0123456789-–−+", "⁰¹²³⁴⁵⁶⁷⁸⁹⁻⁻⁻⁺")


def sci(t):
    """Esponenti persi dall'OCR, solo nei casi non ambigui: "· 10–12" -> "· 10⁻¹²", "m2" -> "m²"."""
    t = re.sub(r"([·×*]|\bx)\s*10\s?([–\-−]?\s?\d{1,2})\b", lambda m: m.group(1) + " 10" + m.group(2).replace(" ", "").translate(SUP), t)
    t = re.sub(r"(?<![\w.,])10\s*[-–−]\s*([1-9])\b(?!\s*[=+\-−*/])", lambda m: "10" + ("-" + m.group(1)).translate(SUP), t)
    t = re.sub(r"\b(m|cm|mm|km|dm|µm|nm|s)([23])\b(?!\s*[,.]?\d)", lambda m: m.group(1) + m.group(2).translate(SUP), t)
    return t


def clean(t, opt=False):
    t = sci(t or "")
    if opt:
        # nelle opzioni tutto ciò che segue il piè di pagina è spazzatura della pagina
        t = re.split(r"\S*(?:edises|EdiSES)\S*|www\.\S+|Capitolo\s+\d+\s*\||\s*\|?\s*\d{0,3}[.,]?\s*\|?\s*Chimica\s*e\s*Propedeutica|\s*\\?[‘']mentati\b|\s+(?:lo|No|Capitolo)\s+1?\d\s*\|", t)[0]
    for r, w in RULES:
        t = r.sub(w, t)
    t = re.sub(r"[A-Za-zÀ-ÿ]{5,}", lambda m: split_glued(m.group(0)), t)
    if re.search(r"rRNA|ribosom|subunit|proteasom|\bS\b", t):
        # coefficienti di sedimentazione: l'OCR legge la S come 5 ("185" = 18S)
        t = re.sub(r"\b(5,8|16|18|19|20|23|26|28|30|40|45|50|60|70|80)5\b", r"\1S", t)
        t = re.sub(r"\b55\b(?= rRNA|,| e )", "5S", t)
    # residui di piè di pagina dopo l'ultima frase
    m = re.search(r"([.?!:)])\s+([^.?!]*)$", t)
    if m and "____" not in m.group(2) and re.search(r"[|=_]|\b[A-Z]{2,}\b|\b[a-z]{1,4}\b\s*$", m.group(2)) \
            and len(m.group(2)) < 40 and not re.search(r"[a-zà-ù]{5,}", m.group(2)):
        t = t[:m.start(2)]
    t = t.strip()
    if opt:
        t = t.rstrip(" ;,")
    return t


KW = {k: re.compile(v) for k, v in json.load(open(os.path.join(R, "tools/edises_work/topic_keywords.json"),
                                                   encoding="utf-8")).items()}
PAGEMAP = json.load(open(os.path.join(R, "tools/edises_work/pagemap.json"), encoding="utf-8"))


def page_no(img):
    """Pagina stampata: immagine -> pagina del PDF (pagemap) + scarto della parte."""
    m = re.match(r"(part\w+)-(p?)(\d+)", img or "")
    if not m:
        return "?"
    part, n = m.group(1), int(m.group(3))
    if part in PAGEMAP:
        pg = n if m.group(2) else PAGEMAP[part]["img2page"].get(str(n), n)
        return pg + PAGEMAP[part]["delta"]
    return n + OFFSET[part]


def qid(key):
    s, cap, sec, n = key
    return f"{s}-ed{cap:02d}-{sec}{n:03d}"


def main():
    draft = json.load(open(DRAFT, encoding="utf-8"))
    fixes = json.load(open(FIXES, encoding="utf-8")) if os.path.exists(FIXES) else {}
    reviewed = set(json.load(open(REVIEWED, encoding="utf-8"))) if os.path.exists(REVIEWED) else set()
    docs, subj_of = ie.load_topic_docs(R)
    syl = json.load(open(os.path.join(R, "content/syllabus.json"), encoding="utf-8"))
    unit_topics = {u["id"]: [t["id"] for t in u["topics"]] for s_ in syl["subjects"] for u in s_["units"]}
    out = {"chi": [], "bio": [], "fis": []}
    held = 0
    # voci perse dall'OCR (es. due domande fuse in una): {"new": {"page": "partX-NNN"}, campi...}
    for i, f in fixes.items():
        if "new" in f:
            s_, rest = i.split("-ed")
            sec_ = rest[3]
            key = [s_, int(rest[:2]), sec_, int(rest[4:])]
            it = {"key": key, "page": f["new"]["page"], "stem": "", "opts": {}, "text": "", "answer": None}
            draft["fill" if sec_ == "f" else "mcq"].append(it)
    for kind in ("mcq", "fill"):
        for it in draft[kind]:
            s, cap, sec, n = it["key"]
            if f"{s}-{cap:02d}" not in reviewed:
                continue
            i = qid(it["key"])
            f = fixes.get(i, {})
            if f.get("hold"):
                held += 1
                continue
            src = f"Eserciziario EdiSES, {SUBJ_NAME[s]} cap. {cap}, {SEC_NAME[sec]} n. {n} (p. {page_no(it['page'])})"
            if kind == "mcq":
                opts = [clean(it["opts"].get(L, ""), True) for L in "ABCDE"]
                q = {"id": i, "type": "mcq", "text": clean(it["stem"]), "options": opts,
                     "answer": it["answer"], "explanation": clean(it.get("expl", ""))}
            else:
                text = clean(it["text"])
                ans = [clean(a) for a in re.split(r"\s*/\s*", it["answer"] or "") if a.strip()]
                if text.count("____") == 2 and len(ans) == 2:
                    # due spazi: il primo diventa testo, si chiede il secondo
                    text = text.replace("____", ans[0], 1)
                    ans = ans[1:]
                q = {"id": i, "type": "fill", "text": text, "answers": ans, "explanation": ""}
            for k, v in f.items():
                if k not in ("hold", "note", "new"):
                    q[k] = v
            if q["type"] == "mcq":
                q.pop("answers", None)
                q.pop("numeric", None)
                q["answer"] = "ABCDE".index(q["answer"])
            else:
                q.pop("options", None)
                q.pop("answer", None)
                if "numeric" in q:
                    q.pop("answers", None)
                if not q["explanation"] and q.get("answers"):
                    q["explanation"] = "Frase completa: " + q["text"].replace("____", q["answers"][0].upper(), 1)
            if "topic" not in q:
                allowed = [t for u in UNITS[s][cap] for t in unit_topics[u]]
                body = (q["text"] + " " + " ".join(q.get("options", [])) + " " + " ".join(q.get("answers", []))).lower()
                hits = {t: len(re.findall(KW[t], body)) for t in allowed if t in KW}
                if hits and max(hits.values()) > 0:
                    # a parità vince l'argomento elencato prima nel capitolo
                    q["topic"] = max(allowed, key=lambda t: (hits.get(t, 0), -allowed.index(t)))
            if "topic" not in q:
                clf = ie.Classifier({t: docs[t] for t in allowed}, {t: subj_of[t] for t in allowed})
                body = q["text"] + " " + " ".join(q.get("options", [])) + " " + q["explanation"]
                top = clf.best(body, s)
                q["topic"] = top[0][1] if top else allowed[0]
            if q["type"] == "mcq" and not q["explanation"]:
                # quiz non commentato: il libro dà solo la lettera, verificata in revisione
                q["explanation"] = f"Risposta corretta: «{q['options'][q['answer']]}»."
            q["difficulty"] = 2
            q["source"] = src
            out[s].append(q)
    for s, qs in out.items():
        path = os.path.join(R, f"content/questions/edises-{s}.json")
        if qs:
            order = ["id", "type", "text", "options", "answer", "answers", "numeric", "topic", "difficulty",
                     "source", "explanation"]
            qs = [{k: q[k] for k in order if k in q} for q in qs]
            json.dump({"schema": 1, "questions": qs}, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
        elif os.path.exists(path):
            os.remove(path)
    print("nell'app:", {s: len(q) for s, q in out.items()}, "tenute da parte:", held)


if __name__ == "__main__":
    main()
