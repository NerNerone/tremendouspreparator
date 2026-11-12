#!/usr/bin/env python3
"""Genera content/elements.json (tavola periodica interattiva) dal database del pacchetto
Python `mendeleev` (dati IUPAC/NIST: masse, elettronegatività di Pauling, affinità
elettronica, energia di ionizzazione, raggi, stati di ossidazione, transizioni di fase).

    pip download mendeleev --no-deps -d /tmp/pkg && unzip /tmp/pkg/*.whl -d /tmp/pkg/x
    python3 -I tools/build_elements.py /tmp/pkg/x/mendeleev/elements.db

Le descrizioni (tendenza a cedere/acquistare elettroni, legami tipici) sono ricavate da
regole per famiglia, con eccezioni scritte a mano per gli elementi più usati nei quiz."""
import json, os, sqlite3, sys

R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
NOMI = """idrogeno elio litio berillio boro carbonio azoto ossigeno fluoro neon sodio magnesio
alluminio silicio fosforo zolfo cloro argon potassio calcio scandio titanio vanadio cromo
manganese ferro cobalto nichel rame zinco gallio germanio arsenico selenio bromo kripton
rubidio stronzio ittrio zirconio niobio molibdeno tecnezio rutenio rodio palladio argento
cadmio indio stagno antimonio tellurio iodio xeno cesio bario lantanio cerio praseodimio
neodimio promezio samario europio gadolinio terbio disprosio olmio erbio tulio itterbio
lutezio afnio tantalio tungsteno renio osmio iridio platino oro mercurio tallio piombo
bismuto polonio astato radon francio radio attinio torio protoattinio uranio nettunio
plutonio americio curio berkelio californio einsteinio fermio mendelevio nobelio laurenzio
rutherfordio dubnio seaborgio bohrio hassio meitnerio darmstadtio roentgenio copernicio
nihonio flerovio moscovio livermorio tennesso oganesson""".split()
assert len(NOMI) == 118

# famiglie (classificazione usata nei libri di testo italiani)
CAT = {1: "Non metallo", 2: "Gas nobile", 3: "Metallo alcalino", 4: "Metallo alcalino-terroso",
       5: "Semimetallo", 6: "Alogeno", 7: "Metallo del blocco p", 8: "Metallo di transizione",
       9: "Lantanide", 10: "Attinide"}
SUP = str.maketrans("0123456789", "⁰¹²³⁴⁵⁶⁷⁸⁹")


def conf(s):
    # "[Ar] 3d6 4s2" → "[Ar] 3d⁶ 4s²"; i sottolivelli con 1 elettrone sono scritti senza numero
    out = []
    for p in s.split():
        if p.startswith("["):
            out.append(p)
            continue
        orb, n = p[:2], p[2:] or "1"
        out.append(orb + n.translate(SUP))
    return " ".join(out)


def ox(v):
    return ("+" if v > 0 else "") + str(v) if v else "0"


# legami ed esempi scritti a mano per gli elementi più ricorrenti nei quiz
NOTE = {
    "H": ("Può cedere l'elettrone (H⁺, negli acidi) o acquistarne uno (H⁻, negli idruri metallici).",
          "Legami covalenti con i non metalli (H₂O, CH₄, NH₃, HCl); ionici con i metalli alcalini e alcalino-terrosi (idruri come NaH). Forma legami a idrogeno quando è legato a F, O o N."),
    "C": ("Né cede né acquista facilmente elettroni: li condivide (tetravalente).",
          "Quattro legami covalenti, anche multipli, con sé stesso (catene, anelli: chimica organica) e con H, O, N, S, alogeni. Ibridazione sp³, sp², sp."),
    "N": ("Tende ad acquistare elettroni (fino a N³⁻) ma per lo più li condivide.",
          "Covalente con H, C, O (NH₃, ammine, HNO₃, ossidi di azoto); triplo legame in N₂. Nitruri ionici con i metalli più attivi (Li₃N)."),
    "O": ("Tende fortemente ad acquistare 2 elettroni (O²⁻): è il secondo elemento più elettronegativo.",
          "Ionico con i metalli (ossidi basici: Na₂O, CaO, Fe₂O₃); covalente polare con i non metalli (H₂O, CO₂, ossidi acidi, ossoacidi). Nei perossidi ha n.o. −1."),
    "F": ("È l'elemento più elettronegativo: acquista sempre 1 elettrone (F⁻) e ha sempre n.o. −1.",
          "Ionico con i metalli (NaF, CaF₂); covalente fortemente polare con i non metalli (HF, che forma legami a idrogeno)."),
    "Na": ("Cede facilmente il suo unico elettrone di valenza diventando Na⁺ (configurazione del neon).",
           "Ionico con i non metalli, soprattutto alogeni e ossigeno (NaCl, Na₂O, NaOH); legame metallico allo stato elementare. Reagisce violentemente con l'acqua."),
    "K": ("Cede facilmente l'elettrone di valenza diventando K⁺.",
          "Ionico con alogeni e ossigeno (KCl, KOH, KNO₃); legame metallico allo stato elementare."),
    "Mg": ("Cede 2 elettroni diventando Mg²⁺.",
           "Ionico con i non metalli (MgO, MgCl₂); presente nella clorofilla come ione coordinato."),
    "Ca": ("Cede 2 elettroni diventando Ca²⁺.",
           "Ionico con i non metalli (CaO, CaCl₂, CaCO₃, CaF₂); legame metallico allo stato elementare."),
    "Cl": ("Tende ad acquistare 1 elettrone diventando Cl⁻; negli ossoacidi può avere n.o. positivi (+1, +3, +5, +7).",
           "Ionico con i metalli (NaCl, CaCl₂); covalente con i non metalli (HCl, Cl₂, CCl₄)."),
    "S": ("Tende ad acquistare 2 elettroni (S²⁻) ma con l'ossigeno assume n.o. positivi (+4, +6).",
          "Ionico con i metalli (solfuri: Na₂S, FeS); covalente con i non metalli (H₂S, SO₂, SO₃, H₂SO₄); ponti disolfuro nelle proteine."),
    "P": ("Tende a condividere elettroni; n.o. tipici −3, +3, +5.",
          "Covalente con O e H (H₃PO₄, fosfati, PCl₃, PCl₅); presente in ATP, DNA, fosfolipidi."),
    "Fe": ("Cede 2 o 3 elettroni (Fe²⁺, Fe³⁺).",
           "Ionico con i non metalli (FeO, Fe₂O₃, FeCl₃); legami di coordinazione nell'eme dell'emoglobina; legame metallico allo stato elementare."),
    "Cu": ("Cede 1 o 2 elettroni (Cu⁺, Cu²⁺).",
           "Ionico con i non metalli (CuO, CuSO₄); complessi colorati; ottimo conduttore (legame metallico)."),
    "Zn": ("Cede 2 elettroni (Zn²⁺).",
           "Ionico con i non metalli (ZnO, ZnCl₂); cofattore di molti enzimi."),
    "Ag": ("Cede 1 elettrone (Ag⁺).", "Ionico con alogeni e zolfo (AgCl, Ag₂S); legame metallico."),
    "Al": ("Cede 3 elettroni (Al³⁺).", "Ionico con ossigeno e alogeni (Al₂O₃, AlCl₃, quest'ultimo con carattere covalente); il suo ossido è anfotero."),
    "Si": ("Condivide elettroni (n.o. tipico +4).", "Covalente, soprattutto con l'ossigeno (SiO₂, silicati); semiconduttore."),
    "Br": ("Tende ad acquistare 1 elettrone (Br⁻).", "Ionico con i metalli (NaBr, KBr); covalente con i non metalli (HBr, Br₂)."),
    "I": ("Tende ad acquistare 1 elettrone (I⁻).", "Ionico con i metalli (KI, NaI); covalente con i non metalli (HI, I₂); presente negli ormoni tiroidei."),
    "Li": ("Cede facilmente l'elettrone di valenza (Li⁺).", "Ionico con i non metalli (LiF, Li₂O, LiBr); legame metallico."),
    "He": ("Guscio completo (1s²): non cede né acquista elettroni.", "Praticamente non forma composti."),
}


def note(e, cat):
    s, g = e["sym"], e["group"]
    if s in NOTE:
        return NOTE[s]
    if cat == "Gas nobile":
        return ("Ha l'ottetto completo: non tende né a cedere né ad acquistare elettroni.",
                "È quasi inerte; solo i più pesanti (Kr, Xe) formano pochi composti con F e O.")
    if cat == "Metallo alcalino":
        return (f"Cede facilmente l'unico elettrone di valenza ({s}⁺).", f"Ionico con alogeni e ossigeno; reagisce con l'acqua formando l'idrossido {s}OH.")
    if cat == "Metallo alcalino-terroso":
        return (f"Cede 2 elettroni ({s}²⁺).", f"Ionico con i non metalli (ossido {s}O, cloruro {s}Cl₂).")
    if cat == "Alogeno":
        return (f"Tende ad acquistare 1 elettrone ({s}⁻).", f"Ionico con i metalli, covalente con i non metalli (H{s}).")
    if cat in ("Metallo di transizione", "Lantanide", "Attinide", "Metallo del blocco p"):
        return ("Tende a cedere elettroni formando cationi" + (" con più numeri di ossidazione." if cat != "Metallo del blocco p" else "."),
                "Legame ionico con i non metalli (ossidi, sali), legame metallico allo stato elementare" + ("; forma complessi spesso colorati." if cat == "Metallo di transizione" else "."))
    if cat == "Semimetallo":
        return ("Comportamento intermedio: può cedere o condividere elettroni.", "Prevalentemente covalente; spesso semiconduttore.")
    return ("Tende ad acquistare o condividere elettroni.", "Covalente con i non metalli, ionico con i metalli.")


def main():
    db = sqlite3.connect(sys.argv[1])
    ox_main = {}
    for z, v, cat in db.execute("select atomic_number, oxidation_state, category from oxidationstates"):
        if cat == "main":
            ox_main.setdefault(z, []).append(v)
    ie = {z: e for z, d, e in db.execute("select atomic_number, ion_charge, ionization_energy from ionizationenergies") if d == 0}
    phase = {}
    for z, m, b in db.execute("select atomic_number, melting_point, boiling_point from phasetransitions"):
        phase.setdefault(z, (m, b))
    out = []
    for row in db.execute("""select atomic_number, symbol, atomic_weight, en_pauling, electron_affinity, group_id,
            period, block, electronic_configuration, series_id, atomic_radius, density, is_radioactive
            from elements where atomic_number <= 118 order by atomic_number"""):
        z, sym, w, en, ea, g, per, block, ec, ser, rad, dens, radio = row
        cat = CAT.get(ser, "")
        if sym == "H":
            cat = "Non metallo"
        if 57 <= z <= 71:
            cat = "Lantanide"
        if 89 <= z <= 103:
            cat = "Attinide"
        m, b = phase.get(z, (None, None))
        stato = "solido" if sym == "C" else None  # il carbonio sublima: non ha punto di ebollizione
        if stato is None and m is not None and b is not None:
            stato = "solido" if m > 298.15 else ("liquido" if b > 298.15 else "gassoso")
        e = {"z": z, "sym": sym, "name": NOMI[z - 1], "mass": round(w, 3) if w else None,
             "group": g, "period": per, "block": block, "config": conf(ec), "cat": cat,
             "en": en, "ea": round(ea * 96.485, 1) if ea is not None else None,
             "ie": round(ie[z] * 96.485) if z in ie and ie[z] else None,
             "radius": round(rad) if rad else None, "density": round(dens, 3) if dens else None,
             "state": stato, "ox": [ox(v) for v in sorted(set(ox_main.get(z, [])), reverse=True)],
             "radioactive": bool(radio)}
        e["electrons"], e["protons"] = z, z
        e["tendency"], e["bonds"] = note(e, cat)
        out.append(e)
    json.dump({"schema": 1, "elements": out}, open(os.path.join(R, "content/elements.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    print(len(out), "elementi")


main()
