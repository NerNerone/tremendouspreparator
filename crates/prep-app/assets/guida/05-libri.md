# 5 I libri: content/books.json e i capitoli

I libri sono capitoli Markdown da leggere nell'app; le domande ci rimandano con `ref`.

## 5.1 Il catalogo

```
{ "schema": 1, "books": [
  { "id": "fis-dispense", "subject": "fis", "title": "Dispense di fisica", "author": "…",
    "chapters": [
      { "file": "books/fisica/dispense/02-cinematica.md", "title": "2 Cinematica",
        "topics": ["fis-u1-t1", "fis-u1-t2"] }
    ] }
] }
```

`topics` dice quali argomenti tratta il capitolo: serve ai rimandi delle domande che non hanno `ref`. Per rimandi più precisi aggiungi `"sections": [ { "section": "2.3", "topics": ["fis-u1-t2"] } ]`.

Se aggiungi un capitolo `.md` da solo dalla pagina *Contenuti*, l'app lo mette nel libro «Materiale importato» della materia aperta in Studio.

## 5.2 Scrivere un capitolo

- Titoli con la numerazione del libro: `# 2 Cinematica`, `## 2.3 Moto uniformemente accelerato`, `### …`. Il lettore divide il capitolo in sezioni ai titoli.
- Segnaposto di pagina del libro originale: `<!-- p. 24 -->` (li inserisce la conversione dai PDF; servono a `page` nei rimandi).
- Elenchi `-` e `1.`, citazioni `>`, tabelle Markdown, codice tra tre apici inversi.
- Riquadri: `> [!NOTE] Titolo`, `> [!TIP]`, `> [!WARNING]`.
- Formule: `$v = \frac{s}{t}$` nel testo, `$$ … $$` su righe proprie per quelle grandi (LaTeX). L'app le mostra come testo con simboli, apici e pedici.
- Figure: `![Figura 2.1: grafico s-t](books/fisica/dispense/fig/2-1.svg)`, con percorso completo. Solo SVG; se manca l'immagine resta la didascalia.
