# Traguardo: la nuova interfaccia

Progetto approvato sulla tela di design (proposta A, «Traguardo»). Questo file raccoglie le decisioni
prese, perché il codice le segua anche quando la tela non è a portata di mano.

## Struttura

- Barra di navigazione con quattro destinazioni: **Oggi** · **Studia** · **Squadra** · **Tu**.
- Pulsante d'azione esteso **«Studia ora»** (in basso a destra, sopra la barra) che apre il menu:
  quiz rapido, ripasso flashcard, simulazione d'esame, concentrazione. Nelle schede che scorrono
  si riduce al solo simbolo.
- **Opzioni dentro «Tu»**, in quattro gruppi: Tu e l'aspetto (profilo, aspetto) · Dispositivi
  (dispositivi, condividi) · Studio (esame e barra, promemoria) · App (pacchetto contenuti, GitHub e
  aggiornamenti, novità, note legali; feedback nella versione privata).
- Stile: Material 3 Expressive chiaro e scuro (tema di sistema), un solo colore d'azione, numeri
  grandi in Roboto 900 (serve il file Roboto-Black), forme che cambiano (pillola, biscotto).
  Riferimenti dalla libreria design-md: Oura (un blu d'azione, anelli e punteggi come eroi).

## Oggi (Home)

- Fascia in alto: selettore dell'esame, «Preparazione» con la percentuale grande, giorni al
  prossimo appello, e al centro la **barra ondulata principale** con inizio e fine netti: parte
  piatta da un tappo arrotondato, l'onda cresce e torna piatta prima della fine della parte fatta;
  poi uno stacco, la traccia piatta e il punto d'arresto. Sotto, «0%» e «100%».
- Una barra ondulata più piccola per materia. Scorrendo la fascia si ritira in una striscia con la
  barra sottile (come oggi).
- Piano di oggi (obiettivi) con caselle, serie, riprendi.
- Tutto dipende dal pacchetto: nessun esame, nessuna data, da 1 a N materie.

## Squadra

- Testata: **foto del gruppo** (quadrato arrotondato 56 px) e **nome del gruppo**; toccandoli si
  aprono membri, codice e impostazioni. Accanto: calendario e invita.
- Finestra della gara grande, a tutta larghezza, nei colori del tema chiaro/scuro (mai scura fissa),
  pista senza cordoli colorati, monoposto F1 con la sagoma dell'app e **piccole** rispetto alla pista.
- Effetti: scia blu della concentrazione lunga circa quanto la monoposto (resta sull'asfalto),
  fiamme del ritmo, scintille delle gomme nuove, fumo, bandiera gialla.
- **Pit stop sul circuito**: la piazzola (piccola, senza bordo, fuori dall'asfalto, nessun raccordo,
  nessuna scritta) compare quando la monoposto si ferma e sparisce quando riparte. Animazione:
  rallenta, esce dalla pista, sosta con i meccanici, rientra con le scintille.
- **Paddock flottante** a cavallo del bordo basso della finestra: chi non è in gara (offline o fermo
  oltre 30 minuti), una piazzola per persona; i posti crescono e calano, sparisce se è vuoto.
- Classifica sotto, con moltiplicatore. Circuiti: quelli dell'app più Città, Montagna e Costa
  (più lunghi e meno schiacciati), da verificare con i test dei circuiti.

## Concentrazione

- In alto la telecamera segue la propria monoposto **sullo stesso circuito e con la stessa grafica
  della vista intera**, ingrandita; «Marta 18 m davanti · la stai prendendo».
- Timer ad anello, punti della sessione, app permesse, Pausa / Torna al quiz / Termina.

## Altre schede (sulla tela)

Studia (materie con gruppo di pulsanti collegati, unità con barre ondulate, strumenti), unità,
lettore, quiz (risposta multipla e completamento), risposta con spiegazione, risultati con «Serie
salva», flashcard fronte/retro, simulazioni, strumenti, nomenclatura, estrai testo, crea con l'AI,
gestisci contenuti (barra di selezione nei colori del tema), feedback, menu esami, calendario.

## Icone delle materie

Campi facoltativi in `syllabus.json` (vedi `prep-core/src/icons.rs`, guida 2.2): `icon` (simbolo
incluso), `icon_svg` (tracciato 24×24), `color` (`#RRGGBB`). Ordine: tracciato valido, simbolo
conosciuto, iniziali. Retrocompatibile in entrambe le direzioni.

Tela di design: <https://claude.ai/artifact/MLsVRn4Ne1ND2axEqKwPwt>
