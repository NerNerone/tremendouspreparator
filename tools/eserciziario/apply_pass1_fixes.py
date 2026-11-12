#!/usr/bin/env python3
"""Applica le correzioni manuali alla prima importazione dell'eserciziario Testbusters.

    python3 -I tools/apply_pass1_fixes.py

tools/eserciziario_pass1_fixes.json: { id: {campo: valore, ...} } con i campi della domanda
(text, options, answer, answers, numeric, type, topic, explanation). Con "hold": true la
domanda resta fuori dall'app (in tools/eserciziario_pass1_held.json) finché non è corretta.
Le domande tenute da parte vengono rimesse nel file appena la correzione toglie "hold".
"""
import json, os, re

R = os.environ.get("PREP_ROOT") or os.getcwd()  # cartella dei contenuti (checkout privato)
CONTENT = os.path.join(R, "content/questions/eserciziario-testbusters.json")
FIXES = os.path.join(R, "tools/eserciziario_pass1_fixes.json")
HELD = os.path.join(R, "tools/eserciziario_pass1_held.json")

# residui OCR ricorrenti (applicati a testo, opzioni e spiegazioni)
FOOT = re.compile(r"\s*\S{0,4}\s*\S{0,4}\s*\S*(?:erciz|ommentat|Testbuster|Semestre\s+Filtro)\S*.*$", re.I)
RULES = [
    (re.compile(r"\s*Capitolo \d\s*[I|l1]\s*(?:Bio\w*|Binlaoia|Chimica e propedeutica biochimica|Chimica|Fisica)\b"), ""),
    (re.compile(r"(\d)'\s*[+→]\s*(\d)\s*['’)°]?"), r"\1' → \2'"),
    (re.compile(r"([a-zà-ù])-\s+(?:[_—–]\s*)+([a-zà-ù])"), r"\1\2"),
    (re.compile(r"(^|[.!?;:]\s+|\()1 (?=[a-zà-ù])"), r"\1I "),
    (re.compile(r"(^|[.!?;:]\s+|\()1\] ?(?=[a-zà-ù])"), r"\1I "),
    (re.compile(r"(^|[.!?;:]\s+|\()(?:11|ll|lI|IT|tl|II) (?=[a-zà-ù])"), r"\1Il "),
    (re.compile(r"\b(?:ii|li) (?=DNA|RNA)"), "il "),
    (re.compile(r"(^|[.!?;:]\s+)(?:II|IT|11|ll) (?=DNA|RNA|[A-Z]{2,})"), r"\1Il "),
    (re.compile(r"(?<=[a-zà-ù]) 0 (?=[a-zà-ù])"), " o "),
    (re.compile(r"\bI(?=(?:legami|lipidi|virus|batteri|cromosomi|geni|nucleotidi|protozoi|recettori|gameti)\b)"), "I "),
    (re.compile(r"\b(?:I'R- NA|IPRNA|I'RNA)\b"), "l'RNA"),
    (re.compile(r"\s*<?_?BL/?\s*"), " "),
    (re.compile(r"(^|[.!?;:]\s+)11 (?=\d)"), r"\1Il "),
    (re.compile(r"\(NHE\]\)"), "(NHEJ)"),
    (re.compile(r"La risposta corretta è la (?:prima|seconda|terza|quarta|quinta)\.?\s*"), ""),
    (re.compile(r"(^|[.!?;:]\s+|\s)! (?=[a-zà-ù])"), r"\1I "),
    (re.compile(r"(^|[.!?;:]\s+)(?:li|Lf|lf) (?=[a-zà-ù])"), r"\1Il "),
    (re.compile(r"\bAI (?=[a-zà-ù])"), "Al "),
    (re.compile(r"\bCAMP\b"), "cAMP"),
    (re.compile(r"\bCGMP\b"), "cGMP"),
    (re.compile(r"\bMRNA\b"), "mRNA"),
    (re.compile(r"\bomoflo\b"), "omofilo"),
    (re.compile(r"\bA([GHSU])(?=\s*[=<>°]|\s+[-+−]|\s*\)|\s+(?:è|e|sia|vale|positiv|negativ|minore|maggiore))"), r"Δ\1"),
    (re.compile(r"\bTAS\b"), "TΔS"),
    (re.compile(r"\\+Delta\s*"), "Δ"),
    (re.compile(r"&lt;"), "<"),
    (re.compile(r"&gt;"), ">"),
    # Cloro: Cl mai scritto come CI (es. AgCI, HCI, KCI, ecc.)
    (re.compile(r"\bHCI\b"), "HCl"),
    (re.compile(r"\bNaCI\b"), "NaCl"),
    (re.compile(r"\bKCI\b"), "KCl"),
    (re.compile(r"\bAgCI\b"), "AgCl"),
    (re.compile(r"\bCaCI2\b"), "CaCl₂"),
    (re.compile(r"\bBaCI2\b"), "BaCl₂"),
    (re.compile(r"\bMgCI2\b"), "MgCl₂"),
    (re.compile(r"\bMnCI,\b"), "MnCl₂"),
    (re.compile(r"\bMnCI\b"), "MnCl₂"),
    (re.compile(r"\bPCI3\b"), "PCl₃"),
    (re.compile(r"\bPCI5\b"), "PCl₅"),
    (re.compile(r"\bFeCI3\b"), "FeCl₃"),
    (re.compile(r"\bFeCI2\b"), "FeCl₂"),
    (re.compile(r"\bAlCI3\b"), "AlCl₃"),
    (re.compile(r"\bCuCI2\b"), "CuCl₂"),
    (re.compile(r"\bNH4CI\b"), "NH₄Cl"),
    (re.compile(r"\bNH,CI\b"), "NH₄Cl"),
    (re.compile(r"\bNH,\s+CI\b"), "NH₄Cl"),
    (re.compile(r"\bNacl\b"), "NaCl"),
    (re.compile(r"\bCI\s*=\s*(\d+)"), r"Cl = \1"),
    (re.compile(r"(\b(?:e|in|tra)\s+)CI(?=[⁻\-]|\b)"), r"\1Cl"),
    (re.compile(r"\bCI[-–—]\b"), "Cl⁻"),
    (re.compile(r"\bCI⁻\b"), "Cl⁻"),
    # Formule con zeri o virgole al posto di O/pedici
    (re.compile(r"\bH20\b"), "H₂O"),
    (re.compile(r"\bH,O\b"), "H₂O"),
    (re.compile(r"\bH,0\b"), "H₂O"),
    (re.compile(r"\bC02\b"), "CO₂"),
    (re.compile(r"\bNa2S0,,\b"), "Na₂SO₄,"),
    (re.compile(r"\bNa2S0\b"), "Na₂SO₄"),
    (re.compile(r"\bNa- 50\b"), "Na₂SO₄"),
    (re.compile(r"\bNa,SO,\b"), "Na₂SO₄"),
    (re.compile(r"\bH,SO,\b"), "H₂SO₄"),
    (re.compile(r"\bH2S04\b"), "H₂SO₄"),
    (re.compile(r"\bK,SO,\b"), "K₂SO₄"),
    (re.compile(r"\bH,CO,\b"), "H₂CO₃"),
    (re.compile(r"\bH,C0,\b"), "H₂CO₃"),
    (re.compile(r"\bH,PO,\b"), "H₃PO₄"),
    (re.compile(r"\bHNO,\b"), "HNO₃"),
    (re.compile(r"\bNH,N03\b"), "NH₄NO₃"),
    (re.compile(r"\bNH,NO,\b"), "NH₄NO₃"),
    (re.compile(r"\bNa0H\b"), "NaOH"),
    (re.compile(r"\bCH,COOH\b"), "CH₃COOH"),
    (re.compile(r"\bCH,COONa\b"), "CH₃COONa"),
    (re.compile(r"\bCH,COO[-⁻°]\b"), "CH₃COO⁻"),
    (re.compile(r"\bCH,C00[-⁻°]\b"), "CH₃COO⁻"),
    (re.compile(r"\bNaHCO,\b"), "NaHCO₃"),
    (re.compile(r"\bCaCl,\b"), "CaCl₂"),
    (re.compile(r"\bK,S\b"), "K₂S"),
    (re.compile(r"\bCr,0,”\b"), "Cr₂O₇²⁻"),
    (re.compile(r"\b-C00H\b"), "-COOH"),
    (re.compile(r"\b-NH2\b"), "-NH₂"),
    (re.compile(r"\b-NH\)\)\b"), "(-NH₂)"),
    # Opzioni incollate (es. Ce D -> C e D, Be D -> B e D, Ae B -> A e B)
    (re.compile(r"\b([A-E])e\s+([A-E])\b"), r"\1 e \2"),
    (re.compile(r"\b([A-E])\s+e\s+([A-E])(errat[ae]|corrett[ae])\b"), r"\1 e \2 \3"),
    (re.compile(r"\b([A-E])(errat[ae]|corrett[ae])\b"), r"\1 \2"),
    (re.compile(r"\bedEerrate\b"), "ed E errate"),
    # Unità incollate ai numeri (es. 2mol -> 2 mol, 1atm -> 1 atm, 8mL -> 8 mL)
    (re.compile(r"\b(\d+)mol\b"), r"\1 mol"),
    (re.compile(r"\b(\d+(?:,\d+)?)(atm|mmHg|Torr|bar|mL|dL|cL|kg|mg|μg|kJ|kcal|cal|Hz|kHz|MHz|GHz|pF|nF|μF|kPa|MPa|kΩ|MΩ|mV|kV|mA|μA|mm|nm|pm)\b"), r"\1 \2"),
    # Residui OCR frequenti
    (re.compile(r"\s*\bNK\b\s*"), " "),
    (re.compile(r"AFCIZIAFIN FNM MEANtata I Madicina Sameactra Ciltra lari"), "cellulari"),
    (re.compile(r"NK uitl'Ò'Ò°'00\s*"), ""),
    (re.compile(r"\s*rciziario\s*"), ""),
    (re.compile(r"\s*be za\s*$"), ""),
    (re.compile(r"\s*KWK\s*"), " "),
    (re.compile(r"chimi-\s*i\s*co"), "chimico"),
    (re.compile(r"\s*\*&-\"\s*"), " "),
    (re.compile(r"\bglicositici\b"), "glicosidici"),
    (re.compile(r"\bLonico\b"), "Ionico"),
    (re.compile(r"\bVan del Waals\b"), "Van der Waals"),
    (re.compile(r"\s*K FCanitalo 41 n\.1\s*"), " "),
    (re.compile(r"\b1! centrioli\b"), "I centrioli"),
    (re.compile(r"\bs0lamente\b"), "solamente"),
    (re.compile(r"\bN-ethyImaleimide\b"), "N-ethylmaleimide"),
    (re.compile(r"\bcrossing-0ver\b"), "crossing-over"),
    (re.compile(r"\b20grammi\b"), "20 grammi"),
    (re.compile(r"\bCapitolo 1 I BioIogja\b"), ""),
    (re.compile(r"\s{2,}"), " "),
]


def clean(t, opt=False):
    if not isinstance(t, str):
        return t
    if opt:
        t = re.sub(r"^[A-E]\.\s+", "", t.strip())
        t = FOOT.sub("", t)
        t = t[:1].upper() + t[1:] if t[:1].islower() and not re.match(r"^[a-z]{1,3}\b[^a-zà-ù ]", t) else t
    for r, w in RULES:
        t = r.sub(w, t)
    return t.strip()


data = json.load(open(CONTENT, encoding="utf-8"))
fixes = json.load(open(FIXES, encoding="utf-8")) if os.path.exists(FIXES) else {}
held = json.load(open(HELD, encoding="utf-8")) if os.path.exists(HELD) else []
pool = {q["id"]: q for q in data["questions"]}
for q in held:
    pool.setdefault(q["id"], q)
order = [q["id"] for q in data["questions"]] + [q["id"] for q in held if q["id"] not in {x["id"] for x in data["questions"]}]
out, keep_aside = [], []
for qid in sorted(set(order), key=lambda i: (i.split("-tb")[0], i)):
    q = dict(pool[qid])
    q["text"] = clean(q["text"])
    q["explanation"] = clean(q.get("explanation", ""))
    if "options" in q:
        q["options"] = [clean(o, True) for o in q["options"]]
    f = fixes.get(qid, {})
    for k, v in f.items():
        if k in ("hold", "note"):
            continue
        if k in ("text", "explanation") and isinstance(v, str):
            q[k] = clean(v)
        elif k == "options" and isinstance(v, list):
            q[k] = [clean(o, True) for o in v]
        else:
            q[k] = v
    if q.get("type") == "mcq":
        q.pop("answers", None); q.pop("numeric", None)
    elif q.get("type") == "fill":
        q.pop("options", None)
        if "numeric" in f:
            q.pop("answers", None)
        if "answers" in f:
            q.pop("numeric", None)
        if "answer" in q and isinstance(q["answer"], int):
            q.pop("answer")
    (keep_aside if f.get("hold") else out).append(q)
data["questions"] = out
json.dump(data, open(CONTENT, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
json.dump(keep_aside, open(HELD, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print(f"nell'app {len(out)}, da correggere {len(keep_aside)}, correzioni {len(fixes)}")
