# qcrop.py <pagina> <n1,n2,...> <outdir>: ritaglia le singole domande numerate di una pagina
import sys, subprocess, tempfile, glob, os, csv, re
from PIL import Image, ImageOps
sys.path.insert(0, os.path.dirname(__file__))
U = "/root/.claude/uploads/1ae93d17-f80c-5917-a9a9-c897cb55581c"
parts = [("af8fd32e",1),("a754306e",31),("35a0e009",61),("673c1ad8",91),("dc6eedf9",121),("c58671e0",151),("c8b9f5f3",179),("03ff8dd8",209),("ed24a7f6",239),("b21b8765",269),("88cd1d98",299),("da221737",329),("daca090b",359)]
page = int(sys.argv[1]); want = {int(x) for x in sys.argv[2].split(",") if x}; out = sys.argv[3]
pid, off = [p for p in parts if p[1] <= page][-1]
pdf = glob.glob(f"{U}/{pid}-*.pdf")[0]
d = tempfile.mkdtemp()
subprocess.run(["pdfimages", "-f", str(page - off + 1), "-l", str(page - off + 1), "-j", pdf, f"{d}/img"], check=True)
big = max(glob.glob(f"{d}/img-*"), key=os.path.getsize)
subprocess.run(["python3", "-I", os.path.dirname(__file__) + "/prep3.py", big, f"{d}/L.png", f"{d}/R.png"], capture_output=True)
orig = ImageOps.autocontrast(Image.open(big).convert("L"), cutoff=1)
W, H = orig.size
Lw = Image.open(f"{d}/L.png").size[0]
starts = []  # (col, y, n)
for col, img, x0 in (("L", f"{d}/L.png", 0), ("R", f"{d}/R.png", Lw)):
    r = subprocess.run(["tesseract", img, "-", "-l", "ita", "--psm", "4", "tsv"], capture_output=True, text=True,
                       env=dict(os.environ, OMP_THREAD_LIMIT="1")).stdout
    rows = list(csv.DictReader(r.splitlines(), delimiter="\t", quoting=csv.QUOTE_NONE))
    lines = {}
    for w in rows:
        if w.get("level") == "5" and w["text"].strip():
            k = (w["block_num"], w["par_num"], w["line_num"])
            lines.setdefault(k, []).append(w)
    for ws in lines.values():
        ws.sort(key=lambda w: int(w["left"]))
        m = re.match(r"^(\d{1,3})[.,]$", ws[0]["text"].strip())
        if m and int(ws[0]["left"]) < (Lw if col == "L" else W - Lw) * 0.25:
            starts.append((col, int(ws[0]["top"]), int(m.group(1)), x0))
starts.sort(key=lambda s: (s[0], s[1]))
for i, (col, y, n, x0) in enumerate(starts):
    if n not in want:
        continue
    nxt = [s for s in starts[i + 1:] if s[0] == col]
    y1 = nxt[0][1] - 10 if nxt else H
    x1 = x0 + (Lw if col == "L" else W - Lw)
    crop = orig.crop((max(0, x0 - 10), max(0, y - 15), min(W, x1 + 10), min(H, y1)))
    crop = crop.resize((560, max(40, int(560 * crop.size[1] / crop.size[0]))))
    crop.save(f"{out}/q{page:03d}_{n:03d}.png")
    print(f"q{page:03d}_{n:03d}.png {crop.size}")
print("numeri trovati:", [(s[0], s[2]) for s in starts])
found = {s[2] for s in starts}
miss = sorted(want - found)
if miss:
    print("non trovate su questa pagina:", miss)
