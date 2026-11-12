# addfix.py: unisce un JSON di correzioni (stdin) in tools/eserciziario_manual.json
import os
import json, sys
p = os.path.join(os.environ.get("PREP_ROOT") or os.getcwd(), "tools/eserciziario_manual.json")
m = json.load(open(p, encoding="utf-8"))
new = json.load(sys.stdin)
for k, v in new.items():
    if isinstance(v.get("answer"), int): v["answer"] = "ABCDE"[v["answer"]]
    m.setdefault(k, {}).update(v)
json.dump(m, open(p, "w", encoding="utf-8"), ensure_ascii=False, indent=1, sort_keys=True)
print(f"correzioni totali: {len(m)} (+{len(new)})")
