# review.py <capitoli...>: stampa compatta di domande problematiche e sospette
import os
import json, sys, re, subprocess
S = os.environ.get("ESERC", os.path.join(os.environ.get("PREP_ROOT") or os.getcwd(), "tools/eserciziario_work"))
chs = sys.argv[1:]
o = subprocess.run(["python3", "-I", S + "/build.py", S], capture_output=True, text=True).stdout
probs = set(re.findall(r"\b\w{3}-tb\d+-\d{3}\b", o.split("da sistemare")[-1]))
flag = dict((i, w) for i, w in json.load(open(S + "/flag.json")))
built = {x["id"]: x for x in json.load(open(os.path.join(os.environ.get("PREP_ROOT") or os.getcwd(), "content/questions/eserciziario-testbusters-2.json")))["questions"]}
for ch in chs:
    for r in json.load(open(f"{S}/split_{ch}.json")):
        i = r["id"]
        if i not in probs and i not in flag:
            continue
        tag = "PROB" if i in probs else "SOSP " + ",".join(flag[i])
        b = built.get(i)
        stem = b["text"] if b else r["stem"]
        opts = b.get("options", []) if b else r["opts"]
        print(f"[{tag}] {i} key={r['key']} p={r['page']}\n  Q: {stem[:230]}")
        for k, x in zip("ABCDE", opts):
            print(f"   {k}) {x[:95]}")
        print(f"  S: {r['expl'][:170]}")
