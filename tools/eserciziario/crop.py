# crop.py <pagina PDF globale> <outdir>: 4 ritagli leggibili (colonna sx/dx, metà alta/bassa)
import sys, subprocess, tempfile, os, glob
from PIL import Image, ImageOps
U = "/root/.claude/uploads/1ae93d17-f80c-5917-a9a9-c897cb55581c"
parts = [("af8fd32e",1),("a754306e",31),("35a0e009",61),("673c1ad8",91),("dc6eedf9",121),("c58671e0",151),("c8b9f5f3",179),("03ff8dd8",209),("ed24a7f6",239),("b21b8765",269),("88cd1d98",299),("da221737",329),("daca090b",359)]
page = int(sys.argv[1]); out = sys.argv[2]
pid, off = [p for p in parts if p[1] <= page][-1]
pdf = glob.glob(f"{U}/{pid}-*.pdf")[0]
i = page - off + 1
d = tempfile.mkdtemp()
subprocess.run(["pdfimages", "-f", str(i), "-l", str(i), "-j", pdf, f"{d}/img"], check=True)
big = max(glob.glob(f"{d}/img-*"), key=os.path.getsize)
im = ImageOps.autocontrast(Image.open(big).convert("L"), cutoff=1)
w, h = im.size
cut = w // 2
for name, box in [("a", (0, 0, cut + 20, h // 2 + 40)), ("b", (0, h // 2 - 40, cut + 20, h)),
                  ("c", (cut - 20, 0, w, h // 2 + 40)), ("d", (cut - 20, h // 2 - 40, w, h))]:
    c = im.crop(box)
    c = c.resize((470, int(470 * c.size[1] / c.size[0])))
    c.save(f"{out}/p{page:03d}{name}.png")
