# dump.py <capitoli>: tutte le domande costruite (in attesa) dei capitoli, testo completo
import os
import json, sys, re
S = os.environ.get("ESERC", os.path.join(os.environ.get("PREP_ROOT") or os.getcwd(), "tools/eserciziario_work"))
h = json.load(open(S + "/held.json"))
chs = {c.replace(".", "") for c in sys.argv[1:]}
for x in h["held"]:
    if re.search(r"-tb(\d\d)", x["id"]).group(1) not in chs:
        continue
    flag = " !!" if x["id"] in h["sus"] else ""
    print(f"## {x['id']} [{x['topic']}]{flag}\n{x['text']}")
    for k, o in zip("ABCDE", x.get("options", [])):
        print(f"  {k}{'*' if x.get('answer') == 'ABCDE'.index(k) else ' '} {o}")
    if x["type"] == "fill":
        print("  =", x.get("answers") or x.get("numeric"))
    print("  S:", x["explanation"][:260])
