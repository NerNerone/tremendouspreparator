#!/usr/bin/env python3
"""Unisce correzioni (JSON da stdin) in tools/edises_fixes.json e segna i capitoli rivisti.

    python3 -I tools/edises_scripts/fx.py [--reviewed bio-01] < correzioni.json

Chiavi brevi ammesse: "bio-01:c006" = "bio-ed01-c006". "answer" è una lettera."""
import json, os, sys
R = os.environ.get("PREP_ROOT") or os.getcwd()  # cartella dei contenuti (checkout privato)
FX = os.path.join(R, "tools/edises_fixes.json")
RV = os.path.join(R, "tools/edises_reviewed.json")
fixes = json.load(open(FX, encoding="utf-8")) if os.path.exists(FX) else {}
data = sys.stdin.read().strip()
new = json.loads(data) if data else {}
for k, v in new.items():
    if ":" in k:
        ch, n = k.split(":")
        s, c = ch.split("-")
        k = f"{s}-ed{c}-{n}"
    fixes.setdefault(k, {}).update(v)
json.dump(dict(sorted(fixes.items())), open(FX, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
if "--reviewed" in sys.argv:
    ch = sys.argv[sys.argv.index("--reviewed") + 1]
    rv = json.load(open(RV, encoding="utf-8")) if os.path.exists(RV) else []
    if ch not in rv:
        rv.append(ch)
    json.dump(sorted(rv), open(RV, "w", encoding="utf-8"), indent=1)
print(f"correzioni: {len(fixes)} (+{len(new)})")
