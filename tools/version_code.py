#!/usr/bin/env python3
"""versionCode Android della versione in Cargo.toml.

Lo calcola come cargo-apk: 1 << 24 | major << 16 | minor << 8 | patch (ogni parte < 256).
Il valore è scritto anche nel commento accanto alla versione (`# versionCode N`), dove lo legge
chi controlla gli aggiornamenti (F-Droid).

  version_code.py           stampa il versionCode
  version_code.py --name    stampa la versione
  version_code.py --check   verifica che il commento corrisponda alla versione
"""
import pathlib
import re
import sys

toml = (pathlib.Path(__file__).resolve().parent.parent / "Cargo.toml").read_text()
m = re.search(r'^version = "([^"]+)"(?:\s*# versionCode (\d+))?', toml, re.M)
version = m.group(1)
major, minor, patch = (int(x) for x in re.split(r"[.+-]", version)[:3])
assert max(major, minor, patch) < 256, "ogni parte della versione deve essere < 256"
code = 1 << 24 | major << 16 | minor << 8 | patch
if "--check" in sys.argv:
    if m.group(2) != str(code):
        sys.exit(f'Cargo.toml: scrivi  version = "{version}"  # versionCode {code}')
    print(f"{version} → versionCode {code}")
else:
    print(version if "--name" in sys.argv else code)
