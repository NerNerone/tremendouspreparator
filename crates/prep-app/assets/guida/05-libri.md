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

Un capitolo d'esempio con tutto quello che il lettore riconosce:

```
<!-- p. 24 -->
# 2 Cinematica

## 2.3 Moto uniformemente accelerato

Testo normale, con **grassetto** e *corsivo*. Una formula nel testo: $v = v_0 + a t$.

$$
s = v_0 t + \frac{1}{2} a t^2
$$

- un elenco puntato
1. un elenco numerato

| Grandezza | Unità |
| --- | --- |
| velocità | m/s |

> [!NOTE] Da ricordare
> L'accelerazione è costante.

![Figura 2.1: grafico s-t](books/fisica/dispense/fig/2-1.svg)
```

- Titoli con la numerazione del libro (`#` capitolo, `##` sezione, `###` sottosezione): il lettore divide il capitolo in sezioni ai titoli.
- Il segnaposto di pagina riporta la pagina del libro originale: lo inserisce la conversione dai PDF e serve a `page` nei rimandi.
- Riquadri: `NOTE`, `TIP`, `IMPORTANT`, `WARNING`, `CAUTION`, con un titolo facoltativo dopo.
- Formule in LaTeX, tra un dollaro nel testo e tra due dollari su righe proprie per quelle grandi: l'app le mostra con simboli, apici e pedici.
- Figure: percorso completo del file, solo SVG; se manca l'immagine resta la didascalia.
