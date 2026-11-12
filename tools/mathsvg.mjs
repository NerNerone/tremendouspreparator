// Impagina in SVG ogni formula a blocco ($$…$$) dei libri in books/**.md.
// Uso (dalla radice del repo):  npm --prefix tools install && node tools/mathsvg.mjs
// Scrive books/_math/<sha1(tex)>.svg (stesso nome calcolato da prep-core/src/latex.rs).
// Gli SVG usano currentColor: l'app li colora secondo il tema chiaro/scuro.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, readdirSync, statSync, mkdirSync, existsSync } from "node:fs";
import { join } from "node:path";
import { mathjax } from "mathjax-full/js/mathjax.js";
import { TeX } from "mathjax-full/js/input/tex.js";
import { SVG } from "mathjax-full/js/output/svg.js";
import { liteAdaptor } from "mathjax-full/js/adaptors/liteAdaptor.js";
import { RegisterHTMLHandler } from "mathjax-full/js/handlers/html.js";
import { AllPackages } from "mathjax-full/js/input/tex/AllPackages.js";

const EX_PX = 8.5; // 1ex in px a corpo 16-17px
const adaptor = liteAdaptor();
RegisterHTMLHandler(adaptor);
const doc = mathjax.document("", {
  InputJax: new TeX({ packages: AllPackages.filter((p) => p !== "bussproofs") }),
  OutputJax: new SVG({ fontCache: "none" }),
});

function walk(dir, out = []) {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith(".md")) out.push(p);
  }
  return out;
}

function formulas(md) {
  const res = [];
  const lines = md.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    const m = t.match(/^\$\$(.+)\$\$$/);
    if (m && !m[1].includes("$$") && m[1].trim()) res.push(m[1].trim());
    else if (t === "$$") {
      const buf = [];
      for (i++; i < lines.length && lines[i].trim() !== "$$"; i++) buf.push(lines[i].trim());
      res.push(buf.join(" ").trim());
    }
  }
  return res;
}

function toSvg(tex) {
  const node = doc.convert(tex, { display: true });
  let svg = adaptor.innerHTML(node);
  // ex -> px (resvg/Slint non conoscono la dimensione del font MathJax)
  svg = svg.replace(/(width|height)="([\d.]+)ex"/g, (_, a, v) => `${a}="${(parseFloat(v) * EX_PX).toFixed(2)}"`);
  svg = svg.replace(/ style="[^"]*"/, "");
  if (!svg.includes("xmlns=")) svg = svg.replace("<svg", '<svg xmlns="http://www.w3.org/2000/svg"');
  // colore pieno: l'app ricolora l'immagine
  svg = svg.replaceAll("currentColor", "#000");
  return svg + "\n";
}

const root = process.argv[2] || ".";
const books = join(root, "books");
const outDir = join(books, "_math");
mkdirSync(outDir, { recursive: true });
let made = 0, skipped = 0, failed = 0;
for (const f of walk(books)) {
  for (const tex of formulas(readFileSync(f, "utf8"))) {
    const key = createHash("sha1").update(tex).digest("hex");
    const out = join(outDir, `${key}.svg`);
    if (existsSync(out)) { skipped++; continue; }
    try {
      const svg = toSvg(tex);
      if (svg.includes("merror")) throw new Error("errore TeX");
      writeFileSync(out, svg);
      made++;
    } catch (e) {
      failed++;
      console.error(`✗ ${f}: ${tex}  (${e.message})`);
    }
  }
}
console.log(`formule: ${made} nuove, ${skipped} già presenti, ${failed} non valide`);
