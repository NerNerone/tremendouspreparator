//! Controller: collega la UI Slint al nucleo `prep-core`.

use crate::periodic;
use crate::{
    AppState, AppWindow, BlockVM, BookVM, BtDevVM, CarVM, CrewVM, ExamDateVM, ExamSectionVM, ExamVM, FlagVM,
    FocusAppVM, GoalVM, GroupVM, NavCellVM, NavRowVM, OfferVM, OptionVM, PeerVM, QzOptVM, RankVM, ResultItemVM,
    SubjectVM, SwatchVM, TocVM, TopicVM, UnitVM,
};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use prep_core::answer::{is_correct, Given};
use prep_core::content::Content;
use prep_core::latex;
use prep_core::markdown::{self, Block, Section};
use prep_core::model::{BookRef, QKind, Question, TOOL_PERIODIC_TABLE};
use prep_core::p2p;
use prep_core::progress::{build_history, compute, exam_pace, Assessment, History, Progress};
use prep_core::select::{self, Scope};
use prep_core::session::{score_items, Item, Session, SessionKind};
use prep_core::store::Store;
use prep_core::vfs;
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
// Instant portabile: nel browser std::time::Instant non esiste
use web_time::Instant;

/// Versione delle note legali: aumentandola l'app le richiede di nuovo.
mod books;
mod cards;
mod contents;
mod creator;
mod devices;
mod exams;
mod focus;
mod goals;
mod group;
mod pdftools;
mod quiz;
mod quiz_setup;
mod report;
mod updates;

const LEGAL_VERSION: u32 = 3;
const LEGAL_TEXT: &str = "Tremendous Preparator è un programma libero (licenza GPL-3.0 o successive), fornito «così com'è», \
senza garanzie di alcun tipo. Chi lo ha scritto non risponde di danni o conseguenze derivanti dal suo uso.

• Contenuti. L'app non contiene e non distribuisce libri, quiz, flashcard o altro materiale di studio. Quello che importi \
(pacchetti, libri o slide convertiti, domande generate con l'intelligenza artificiale) è sotto la tua esclusiva \
responsabilità: assicurati di averne il diritto, usalo solo per studio personale e non condividerlo con chi non ne ha \
diritto. Nessuno ne verifica la correttezza: controlla sempre sulle fonti ufficiali.

• Servizi esterni. L'app non ha server e non raccoglie dati. Se attivi la sincronizzazione tra dispositivi, i dati \
viaggiano cifrati, direttamente tra i dispositivi quando la rete lo permette e altrimenti attraverso relay pubblici \
gestiti da terzi, che scegli tu nelle Opzioni; per stabilire il collegamento diretto l'app chiede il proprio indirizzo \
a server STUN pubblici (Google, Cloudflare), che non ricevono dati. I relay di norma inoltrano i messaggi senza \
conservarli; fanno eccezione gli avvisi «vieni a studiare» dei gruppi di studio, che restano cifrati sui relay \
per arrivare anche a chi è offline e scadono dopo 24 ore (un relay potrebbe non rispettare la scadenza). Foto profilo \
e del gruppo e il nome utente li vedono i membri dei gruppi in cui entri. Questi servizi hanno le loro condizioni e la loro gestione dei dati (per esempio \
possono vedere l'indirizzo IP): usarli è una tua scelta, e la password della sincronizzazione va custodita da te.

• Risultati. Percentuali di preparazione, punteggi attesi, simulazioni e obiettivi sono stime indicative e non \
garantiscono l'esito di alcun esame.

Toccando «Ho letto e accetto» dichiari di aver letto queste condizioni e di accettarle.";

pub const APP_VERSION: &str = env!("APP_VERSION");

const PRACTICE_N: usize = 10;
const REVIEW_N: usize = 20;
const CARDS_MAX: usize = 30;
const CARDS_NEW: usize = 15;
/// giorni senza allenamento dopo cui un argomento va ripassato
const REVIEW_AFTER_DAYS: i64 = 2;

// pagine (vedi ui/state.slint)
const P_HOME: i32 = 0;
const P_STUDY: i32 = 1;
const P_UNIT: i32 = 2;
const P_QUIZ: i32 = 3;
const P_RESULTS: i32 = 4;
const P_CARDS: i32 = 5;
const P_FLAGS: i32 = 6;
const P_SETTINGS: i32 = 7;
const P_DECKS: i32 = 8;
const P_READER: i32 = 9;
const P_GOALS: i32 = 11;
const P_PERIODIC: i32 = 12;
const P_GROUP: i32 = 13;
const P_CONTENTS: i32 = 14;
const P_PDFTOOLS: i32 = 15;
const P_CREATOR: i32 = 16;

fn tab_page(tab: i32) -> i32 {
    match tab {
        1 => P_STUDY,
        2 => P_DECKS,
        3 => P_FLAGS,
        4 => P_GROUP,
        _ => P_HOME,
    }
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0).round() as u32;
    if s < 60 {
        format!("{s} s")
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn hm(s: u64) -> String {
    if s == 0 {
        String::new()
    } else if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min", (s + 30) / 60)
    } else {
        format!("{} h {:02} min", s / 3600, (s % 3600) / 60)
    }
}

fn it1(x: f64) -> String {
    format!("{:.1}", x).replace('.', ",")
}

/// testo con eventuale LaTeX in linea convertito in Unicode
fn tx(s: &str) -> SharedString {
    SharedString::from(latex::render_inline(s).as_str())
}

fn ss(s: impl AsRef<str>) -> SharedString {
    SharedString::from(s.as_ref())
}

type PitKey = (usize, usize, Vec<i64>);

/// Collegamenti diretti (WebRTC) al massimo per il gruppo di studio: oltre, i relay.
const GROUP_DIRECT: usize = 8;

/// Desktop: l'immagine più recente in Immagini o Download (per la foto profilo).
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn newest_image_download() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").ok()?);
    let dirs = ["Immagini", "Pictures", "Scaricati", "Downloads"];
    dirs.iter()
        .filter_map(|d| std::fs::read_dir(home.join(d)).ok())
        .flatten()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| matches!(x.to_lowercase().as_str(), "jpg" | "jpeg" | "png"))
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

struct Quiz {
    kind: SessionKind,
    subject: Option<String>,
    unit: Option<String>,
    title: String,
    questions: Vec<Question>,
    /// ordine in cui sono mostrate le opzioni: perm[i][posizione] = indice originale
    perm: Vec<Vec<usize>>,
    given: Vec<Given>,
    unsure: Vec<bool>,
    checked: Vec<bool>,
    secs: Vec<f64>,
    idx: usize,
    started: DateTime<Utc>,
    shown_at: Instant,
    deadline: Option<Instant>,
}

impl Quiz {
    fn is_exam(&self) -> bool {
        self.kind == SessionKind::Exam
    }
    fn account_time(&mut self) {
        let now = Instant::now();
        self.secs[self.idx] += now.duration_since(self.shown_at).as_secs_f64();
        self.shown_at = now;
    }
}

/// Quiz/simulazione in corso salvato su disco: permette pausa e ripresa (anche dopo
/// la chiusura dell'app). Resta solo sul dispositivo.
#[derive(Serialize, Deserialize)]
struct SavedQuiz {
    kind: SessionKind,
    subject: Option<String>,
    unit: Option<String>,
    title: String,
    ids: Vec<String>,
    given: Vec<Option<String>>,
    unsure: Vec<bool>,
    checked: Vec<bool>,
    secs: Vec<f64>,
    idx: usize,
    started: DateTime<Utc>,
    remaining_secs: Option<u64>,
    saved_at: DateTime<Utc>,
    #[serde(default)]
    perm: Vec<Vec<usize>>,
}

fn encode_given(g: &Given) -> Option<String> {
    match g {
        Given::None => None,
        Given::Option(i) => Some(format!("o:{i}")),
        Given::Text(t) => Some(format!("t:{t}")),
    }
}

fn decode_given(s: &Option<String>) -> Given {
    match s.as_deref() {
        Some(x) if x.starts_with("o:") => x[2..].parse().map(Given::Option).unwrap_or(Given::None),
        Some(x) if x.starts_with("t:") => Given::Text(x[2..].to_string()),
        _ => Given::None,
    }
}

struct CardRun {
    subject: Option<String>,
    queue: Vec<String>,
    requeued: BTreeMap<String, u8>,
    idx: usize,
    flipped: bool,
    unsure: bool,
    items: Vec<Item>,
    started: DateTime<Utc>,
    shown_at: Instant,
    saved: bool,
}

struct ReaderEntry {
    chapter: String,
    blocks: Rc<Vec<Block>>,
    section: Section,
    file: String,
}

struct Reader {
    title: String,
    entries: Vec<ReaderEntry>,
    pos: usize,
    return_page: i32,
}

#[derive(Clone)]
enum Dialog {
    QuitQuiz,
    SubmitExam,
    /// rapporto di un errore della sessione precedente, da segnalare
    Crash,
    /// togliere un file dai contenuti dell'esame
    RemoveContent(String),
    /// avviare la concentrazione (blocca il telefono)
    FocusStart,
}

pub struct Ctrl {
    ui: slint::Weak<AppWindow>,
    store: Store,
    content: Content,
    sessions: Vec<Session>,
    history: History,
    assessment: Option<Assessment>,
    progress: Progress,
    /// contenuti da un pacchetto installato (non quelli inclusi nell'app)
    from_pack: bool,
    unit_id: String,
    quiz: Option<Quiz>,
    cards: Option<CardRun>,
    results: Option<Session>,
    pending_flags: BTreeMap<String, bool>,
    dialog: Option<Dialog>,
    reader: Option<Reader>,
    last_save: Instant,
    /// proposta di obiettivi del giorno (se non ancora confermati) e selezione dell'utente
    plan: Option<prep_core::plan::DayPlan>,
    plan_sel: Vec<bool>,
    /// argomenti aggiunti a mano alla proposta (prima della conferma)
    plan_extra: Vec<String>,
    elements: Vec<periodic::Element>,
    /// ordine delle opzioni del quiz appena finito, per mostrare i risultati con le stesse lettere
    result_perm: std::collections::HashMap<String, Vec<usize>>,
    tick: Timer,
    toast_timer: Timer,
    /// controlla la cartella import/ (file passati da Android con «Apri con» o dal selettore)
    import_timer: Timer,
    importing: bool,
    flag_timer: Timer,
    qz: QuizSetup,
    /// esami installati e quello attivo
    exams: prep_core::exams::Registry,
    /// esame da eliminare in attesa del secondo tocco di conferma
    remove_armed: Option<(String, Instant)>,
    /// sincronizzazione tra i propri dispositivi in corso
    p2p: Option<P2p>,
    /// condivisione di un esame con un'altra persona in corso
    share: Option<P2p>,
    /// gruppo di studio attivo collegato
    group: Option<P2p>,
    groups: prep_core::group::Groups,
    study_log: prep_core::group::StudyLog,
    study_state: prep_core::group::State,
    /// inizio (secondi Unix) del tratto di studio in corso, non ancora nel registro
    study_since: Option<i64>,
    /// inizio del pit stop in corso
    pit_since: Option<i64>,
    /// monoposto toccata sul circuito (id del membro), per la scheda dei dettagli
    car_sel: String,
    /// ultima attività di studio riconosciuta (per «cosa sta studiando»)
    last_doing: String,
    /// circuito mostrato (indice), per non ricalcolarlo a ogni secondo
    circuit_shown: Option<usize>,
    /// monoposto in pista: modello aggiornato riga per riga (sostituirlo ricreerebbe gli
    /// elementi e farebbe ripartire le animazioni)
    cars_model: Rc<VecModel<CarVM>>,
    /// squadra dei box di ogni sosta in corso: (membro, inizio sosta) → meccanici
    crews: BTreeMap<(String, i64), ModelRc<CrewVM>>,
    /// posti dei pit stop già calcolati: (circuito, membri, soste) → (lato, fila)
    pit_cache: Option<(PitKey, Vec<(f64, u32)>)>,
    /// foto già decodificate per l'interfaccia (hash → immagine)
    photos: BTreeMap<String, slint::Image>,
    /// immagine in ritaglio: (profilo «user» o «group», immagine)
    crop: Option<(String, image::DynamicImage)>,
    /// foto chieste agli altri / mandate da poco (hash → istante), per non ripetere
    photo_asked: BTreeMap<String, i64>,
    photo_sent: BTreeMap<String, i64>,
    /// avvisi mandati da poco (destinatario → istante)
    ping_sent: BTreeMap<String, i64>,
    /// rimozione di un membro in attesa di conferma
    kick_armed: Option<(String, Instant)>,
    /// ultimo invio delle impostazioni del gruppo (amministratore)
    info_sent: i64,
    /// id utente già calcolato (e quando)
    uid_cache: Option<(String, i64)>,
    /// app installate (per la concentrazione): pacchetto, nome, icona
    focus_apps: Vec<(String, String, Option<slint::Image>)>,
    /// avvio della concentrazione in attesa di conferma
    focus_confirm: Option<Instant>,
    /// chi ha annunciato ciascuna foto (hash → dispositivo): a lui si chiede
    photo_src: BTreeMap<String, String>,
    group_period: prep_core::group::Period,
    group_timer: Timer,
    group_last_send: i64,
    /// ultima volta che si è mandata la mappa di calore al gruppo
    hist_sent: i64,
    /// calendario del gruppo: per mese (o settimana), quanti mesi/settimane indietro, giorno
    /// toccato (giorni fa)
    gcal_month: bool,
    gcal_off: i32,
    gcal_sel: Option<i64>,
    /// modalità dimostrativa: vetrina degli effetti sulle monoposto
    fx_demo: bool,
    /// ricerca tra le app da permettere nella concentrazione
    focus_query: String,
    /// telecamera della gara su un altro membro («Mostra nel circuito»)
    cam_follow: Option<String>,
    /// profilo aperto (id utente)
    profile_sel: Option<String>,
    /// strumento PDF in attesa del file scelto
    pdf_pick: Option<pdftools::PdfTool>,
    pdf_return: i32,
    /// tratto di studio in corso e suoi effetti (punteggio, vedi prep_core::score)
    run: prep_core::score::Run,
    /// concentrazione attiva (×1,5) e altri membri in pista (scia di gruppo)
    focus_on: bool,
    racing_others: usize,
    /// ultima notifica della sessione: (istante, stato)
    study_notified: (i64, i32),
    /// APK della versione più nuova trovata (vedi `updates`)
    update_url: String,
    group_leave_armed: Option<(String, Instant)>,
    /// giri completati finora oggi (per festeggiare il giro nuovo)
    my_laps: u64,
    /// collegamento Bluetooth con un telefono vicino (Android)
    bt: BtState,
    /// accento del tema in uso (primario del tema chiaro)
    accent: Option<u32>,
    /// tonalità di sistema (Material You), se il dispositivo le ha
    system_tones: Option<Vec<u32>>,
}

#[derive(Default)]
struct BtState {
    /// nome del telefono collegato
    connected: Option<String>,
    /// dispositivi trovati: (nome, indirizzo)
    found: Vec<(String, String)>,
    status: String,
}

/// Le due sessioni di rete: i propri dispositivi, o un'altra persona.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Sess {
    Sync,
    Share,
    Group,
}

impl Sess {
    fn from_index(i: usize) -> Sess {
        match i {
            0 => Sess::Sync,
            1 => Sess::Share,
            _ => Sess::Group,
        }
    }
}

/// I collegamenti di una sessione hanno indici `sessione * 16 + relay`.
const LINKS_PER_SESS: usize = 16;

/// Stanza di sincronizzazione aperta: motore, collegamenti ai relay e coda d'invio.
struct P2p {
    room: p2p::Room,
    signer: p2p::Signer,
    /// tolto temporaneamente mentre lavora (ha bisogno del controller come `Host`)
    engine: Option<p2p::Engine>,
    links: Vec<crate::net::Link>,
    relays: Vec<String>,
    open: Vec<bool>,
    subscribed: Vec<bool>,
    started: bool,
    queue: std::collections::VecDeque<String>,
    timer: Timer,
    ticks: u32,
    /// progressi ricevuti per l'esame attivo, da ricaricare
    dirty: bool,
    received: usize,
    notice: String,
    /// condivisione: codice da comunicare (vuoto per la sincronizzazione)
    code: String,
    /// condivisione: nome dell'esame mandato (vuoto se si riceve)
    sending: String,
    /// collegamenti diretti (WebRTC) con i dispositivi della stanza
    direct: BTreeMap<String, Direct>,
    /// dispositivi con cui il collegamento diretto non è riuscito (si riprova al prossimo ingresso)
    failed: BTreeSet<String>,
    /// numeratore dei messaggi arrivati per via diretta (id per scartare i doppioni)
    rtc_seq: u64,
}

/// Collegamento diretto con un dispositivo: aperto o in apertura, con i messaggi in attesa.
struct Direct {
    conn: crate::direct::Conn,
    open: bool,
    pending: std::collections::VecDeque<Vec<u8>>,
}

/// Filtri del quiz personalizzato (scheda Quiz).
struct QuizSetup {
    subjects: BTreeSet<String>,
    /// unità spuntate; una materia senza unità spuntate vale per intero
    units: BTreeSet<String>,
    /// fonti escluse (di partenza tutte incluse)
    off_sources: BTreeSet<String>,
    no_mcq: bool,
    no_fill: bool,
    n: usize,
    init: bool,
    /// materie aperte nell'elenco delle unità
    open: BTreeSet<String>,
}

impl Default for QuizSetup {
    fn default() -> Self {
        QuizSetup {
            subjects: BTreeSet::new(),
            units: BTreeSet::new(),
            off_sources: BTreeSet::new(),
            no_mcq: false,
            no_fill: false,
            n: 20,
            init: false,
            open: BTreeSet::new(),
        }
    }
}

impl QuizSetup {
    /// la domanda passa i filtri di materia, unità e tipo (la fonte si controlla a parte)
    fn keep_scope(&self, content: &Content, q: &Question) -> bool {
        if (q.kind == QKind::Mcq && self.no_mcq) || (q.kind == QKind::Fill && self.no_fill) {
            return false;
        }
        let Some(sub) = content.subject_of_topic(&q.topic) else { return false };
        if !self.subjects.contains(&sub.id) {
            return false;
        }
        if sub.units.iter().any(|u| self.units.contains(&u.id)) {
            return content.unit_of_topic(&q.topic).is_some_and(|u| self.units.contains(&u.id));
        }
        true
    }

    fn keep(&self, content: &Content, q: &Question) -> bool {
        self.keep_scope(content, q) && !self.off_sources.contains(&q.source_group())
    }
}

/// Il controller dà al motore della sincronizzazione accesso agli archivi degli esami.
impl p2p::Host for Ctrl {
    fn offers(&self) -> Vec<p2p::ExamOffer> {
        self.exams
            .exams
            .iter()
            .map(|e| {
                let st = self.exam_store(&e.id);
                let shareable = st.mirror_path("content/syllabus.json").is_some_and(|p| vfs::exists(&p));
                let version = if shareable { prep_core::pack::content_version(&st) } else { String::new() };
                p2p::ExamOffer { id: e.id.clone(), name: e.name.clone(), version, shareable }
            })
            .collect()
    }

    fn progress_files(&self, exam: &str) -> Option<Vec<(String, String)>> {
        self.exams.get(exam)?;
        let mirror = self.exam_store(exam).root.join("mirror");
        let mut v = Vec::new();
        for f in vfs::walk_files(&mirror.join("progress")) {
            let Ok(rel) = f.strip_prefix(&mirror) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if p2p::shareable_path(&rel) {
                if let Ok(b) = vfs::read(&f) {
                    v.push((rel, p2p::sha256_hex(&b)));
                }
            }
        }
        Some(v)
    }

    fn read_file(&self, exam: &str, path: &str) -> Option<Vec<u8>> {
        self.exams.get(exam)?;
        vfs::read(&self.exam_store(exam).mirror_path(path)?).ok()
    }

    fn write_file(&mut self, exam: &str, path: &str, bytes: &[u8]) -> bool {
        if self.exams.get(exam).is_none() || serde_json::from_slice::<serde_json::Value>(bytes).is_err() {
            return false;
        }
        let st = self.exam_store(exam);
        if st.mirror_path(path).is_some_and(|p| vfs::exists(&p)) {
            return false;
        }
        st.write_mirror(path, bytes).is_ok()
    }

    fn pack(&mut self, exam: &str) -> Result<Vec<u8>, String> {
        self.exams.get(exam).ok_or("esame non installato")?;
        let st = self.exam_store(exam);
        let label =
            prep_core::pack::last_import(&st).map(|i| i.label).unwrap_or_else(|| "da un altro dispositivo".into());
        prep_core::pack::build(&st.root.join("mirror"), &label).map(|(b, _)| b)
    }

    fn import_pack(&mut self, bytes: &[u8]) -> Result<String, String> {
        let (id, name) = prep_core::pack::exam_of(bytes)?;
        let st = self.exam_store(&id);
        let _ = vfs::create_dir_all(&st.root.join("mirror"));
        prep_core::pack::import(&st, bytes)?;
        self.exams.add(&id, &name);
        let _ = self.exams.save(&self.store.base);
        Ok(name)
    }
    fn profile(&self) -> Option<p2p::Profile> {
        let s = self.store.settings();
        // una foto messa prima che si sincronizzasse vale come la più vecchia possibile
        let at = if s.photo_at == 0 && !s.photo.is_empty() { 1 } else { s.photo_at };
        Some(p2p::Profile { photo: s.photo, at, uid: s.user_id })
    }
}

/// Evento di un collegamento diretto consegnato sul thread dell'interfaccia (vedi `direct`).
pub(crate) fn on_direct_event(sess: usize, peer: String, ev: crate::direct::DirectEvent) {
    with_ctrl(|c| c.direct_event(Sess::from_index(sess), peer, ev));
}

/// Modalità dimostrativa: un'immagine scelta come foto profilo (apre il ritaglio).
#[cfg_attr(any(target_os = "android", target_arch = "wasm32"), allow(dead_code))]
/// Modalità dimostrativa: un PDF «scelto» per uno strumento («split» o «text»).
#[cfg_attr(any(target_os = "android", target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn demo_pdf(tool: &str, path: &str) {
    with_ctrl(|c| {
        c.pdf_pick = Some(if tool == "split" { pdftools::PdfTool::Split } else { pdftools::PdfTool::Text });
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if let Ok(b) = std::fs::read(path) {
            let dir = c.store.base.join("import");
            let _ = vfs::create_dir_all(&dir);
            let _ = vfs::write(&dir.join(format!("file-1--{name}")), &b);
        }
    });
}

/// Modalità dimostrativa: il proprio profilo nel gruppo.
#[cfg_attr(any(target_os = "android", target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn demo_profile_me() {
    with_ctrl(|c| {
        c.profile_sel = Some(c.user_id());
        c.refresh_group();
    });
}

/// Modalità dimostrativa: vetrina degli effetti delle monoposto.
#[cfg_attr(any(target_os = "android", target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn demo_fx() {
    with_ctrl(|c| {
        c.fx_demo = true;
        c.refresh_group();
    });
}

/// Modalità dimostrativa: un'immagine scelta come foto profilo (apre il ritaglio).
#[cfg_attr(any(target_os = "android", target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn demo_photo(kind: &str, bytes: &[u8]) {
    with_ctrl(|c| c.photo_imported(kind, bytes));
}

/// Evento del collegamento Bluetooth (Android, vedi `BtLink.java`).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn on_bt_event(kind: i32, text: String, data: Vec<u8>) {
    with_ctrl(|c| c.bt_event(kind, text, data));
}

/// Evento di rete consegnato sul thread dell'interfaccia (vedi `net`).
pub(crate) fn on_net_event(ev: crate::net::NetEvent) {
    with_ctrl(|c| c.net_event(ev));
}

thread_local! {
    static CTRL: RefCell<Option<Rc<RefCell<Ctrl>>>> = const { RefCell::new(None) };
}

fn with_ctrl(f: impl FnOnce(&mut Ctrl)) {
    CTRL.with(|c| {
        if let Some(rc) = c.borrow().as_ref() {
            if let Ok(mut ctrl) = rc.try_borrow_mut() {
                f(&mut ctrl);
            }
        }
    });
}

impl Ctrl {
    fn ui(&self) -> AppWindow {
        self.ui.upgrade().expect("finestra")
    }

    fn reload_content(&mut self) {
        let files = self.store.content_files(crate::bundled::BUNDLED);
        self.from_pack = self.store.index().synced_once;
        self.content = Content::from_files(files.iter().map(|(a, b)| (a.as_str(), b.as_str())));
        if self.content.syllabus.is_none() {
            // contenuti installati rotti: quelli inclusi nell'app, se ci sono
            self.content = Content::from_files(crate::bundled::BUNDLED.iter().copied());
            self.from_pack = false;
        }
        if self.content.syllabus.is_none() {
            // nessun esame: la Home invita a importare un pacchetto
            self.content.syllabus = Some(prep_core::model::Syllabus::empty());
        }
    }

    fn reload_progress(&mut self) {
        let (sessions, _errs) = self.store.sessions();
        self.sessions = sessions;
        self.assessment = self.store.assessment();
        self.history = build_history(&self.sessions);
        for (id, on) in &self.pending_flags {
            if *on {
                self.history.flagged.insert(id.clone());
            } else {
                self.history.flagged.remove(id);
            }
        }
        self.progress = compute(&self.content, &self.history, self.assessment.as_ref());
    }

    fn toast(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if std::env::var("PREP_DEMO").is_ok() {
            eprintln!("TOAST {msg}");
        }
        let ui = self.ui();
        ui.global::<AppState>().set_toast(ss(&msg));
        let weak = self.ui.clone();
        self.toast_timer.start(TimerMode::SingleShot, Duration::from_millis(4000), move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<AppState>().set_toast(ss(""));
            }
        });
    }

    // ---------- rendering ----------

    fn refresh_all(&mut self) {
        self.refresh_home();
        self.refresh_study();
        self.refresh_unit();
        self.refresh_flags();
        self.refresh_quiz_setup();
        self.refresh_settings_info();
        self.share_legacy_goals();
        self.refresh_goals();
        self.write_reminder();
    }

    // ---------- navigazione ----------

    fn set_page(&mut self, page: i32) {
        if page != P_FLAGS && page != P_RESULTS {
            self.flush_flags();
        }
        self.ui().global::<AppState>().set_page(page);
    }

    fn go_tab(&mut self, tab: i32) {
        let ui = self.ui();
        ui.global::<AppState>().set_tab(tab);
        self.set_page(tab_page(tab));
    }

    fn back(&mut self) -> bool {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if self.dialog.is_some() {
            self.close_dialog();
            return true;
        }
        if self.crop.is_some() {
            self.crop_cancel();
            return true;
        }
        match st.get_page() {
            P_QUIZ => {
                self.open_dialog(
                    Dialog::QuitQuiz,
                    "Uscire dal quiz?",
                    if self.quiz.as_ref().is_some_and(|q| q.is_exam()) {
                        "Pausa: la simulazione si ferma (anche il timer) e la riprendi dalla Home.\nTermina: la simulazione viene annullata e non salvata."
                    } else {
                        "Pausa: riprendi più tardi dalla Home da dove eri arrivato.\nTermina: le risposte già confermate vengono salvate."
                    },
                    "Termina",
                );
                self.ui().global::<AppState>().set_dialog_alt(ss("Pausa"));
                true
            }
            P_RESULTS => {
                self.results_close();
                true
            }
            P_CARDS => {
                self.cards_close();
                true
            }
            P_PERIODIC if st.get_pt_has_sel() => {
                // prima si chiude il pannello dell'elemento
                st.set_pt_has_sel(false);
                true
            }
            P_UNIT | P_PERIODIC => {
                self.set_page(P_STUDY);
                true
            }
            P_READER => {
                if st.get_rd_toc_visible() {
                    st.set_rd_toc_visible(false);
                } else {
                    let ret = self.reader.take().map(|r| r.return_page).unwrap_or(P_STUDY);
                    self.set_page(ret);
                }
                true
            }
            P_CONTENTS => {
                self.set_page(P_SETTINGS);
                true
            }
            // nel creatore: prima si chiude l'editor del file
            P_CREATOR if st.get_cr_open() => {
                self.creator_close();
                true
            }
            P_CREATOR => {
                self.go_tab(0);
                true
            }
            P_PDFTOOLS => {
                // si torna da dove si era venuti (guida o contenuti)
                self.set_page(self.pdf_return);
                true
            }
            // dentro una sezione delle Opzioni: si torna al menu
            P_SETTINGS if st.get_settings_sec() >= 0 => {
                st.set_settings_sec(-1);
                true
            }
            P_HOME => false,
            _ => {
                self.go_tab(0);
                true
            }
        }
    }

    fn open_dialog(&mut self, d: Dialog, title: &str, text: &str, ok: &str) {
        self.dialog = Some(d);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_dialog_title(ss(title));
        st.set_dialog_text(ss(text));
        st.set_dialog_confirm(ss(ok));
        st.set_dialog_alt(ss(""));
        st.set_dialog_visible(true);
    }

    fn close_dialog(&mut self) {
        self.dialog = None;
        self.ui().global::<AppState>().set_dialog_visible(false);
    }

    fn dialog_ok(&mut self) {
        let d = self.dialog.take();
        self.ui().global::<AppState>().set_dialog_visible(false);
        match d {
            Some(Dialog::QuitQuiz) => {
                let exam = self.quiz.as_ref().is_some_and(|q| q.is_exam());
                if exam {
                    self.quiz = None;
                    self.tick.stop();
                    self.clear_saved_quiz();
                    self.set_page(P_STUDY);
                    self.ui().global::<AppState>().set_tab(1);
                } else {
                    self.finish_quiz();
                }
            }
            Some(Dialog::SubmitExam) => self.finish_quiz(),
            Some(Dialog::Crash) => self.crash_report(),
            Some(Dialog::RemoveContent(p)) => self.contents_remove(&p),
            Some(Dialog::FocusStart) => self.focus_begin(),
            None => {}
        }
    }

    fn dialog_alt(&mut self) {
        let d = self.dialog.take();
        self.ui().global::<AppState>().set_dialog_visible(false);
        match d {
            Some(Dialog::QuitQuiz) => self.pause_quiz(),
            Some(Dialog::Crash) => crate::crash::clear(&self.store.base),
            _ => {}
        }
    }

    // ---------- segnalazione degli errori ----------

    /// All'avvio: se la sessione precedente si è chiusa per un errore, propone di segnalarlo.
    fn crash_check(&mut self) {
        if self.dialog.is_some() || self.ui().global::<AppState>().get_legal_visible() {
            return;
        }
        if crate::crash::pending(&self.store.base).is_some() {
            self.open_dialog(Dialog::Crash, "L'app si è chiusa per un errore", report::CRASH_TEXT, "Segnala");
            self.ui().global::<AppState>().set_dialog_alt(ss("Non segnalare"));
        }
    }

    fn crash_report(&mut self) {
        if let Some((_, report)) = crate::crash::pending(&self.store.base) {
            self.crash_send(report);
        }
    }
}

/// Tracciato SVG della barra ondulata: una sinusoide da `x0` a `x1` (pixel logici) centrata
/// su `cy`, che scorre con `phase` (0..1 di lunghezza d'onda). Gli estremi arrotondati li
/// aggiunge il tratto (`stroke-line-cap: round`). Un tratto molto corto ha ampiezza ridotta,
/// così all'inizio la barra non è uno scarabocchio.
pub(crate) fn wave_path(x0: f32, x1: f32, cy: f32, amp: f32, wl: f32, phase: f32) -> String {
    use std::fmt::Write;
    let wl = wl.max(4.0);
    let len = (x1 - x0).max(0.0);
    if len < 0.5 {
        return format!("M {x0:.2} {cy:.2} L {:.2} {cy:.2}", x0 + 0.01);
    }
    let a = amp * (len / wl).min(1.0);
    let k = std::f32::consts::TAU / wl;
    let y = |x: f32| cy - a * (k * (x - x0) - std::f32::consts::TAU * phase).sin();
    let step = (wl / 20.0).max(1.5);
    let n = (len / step).ceil().max(1.0) as usize;
    let mut s = String::with_capacity(n * 16 + 16);
    let _ = write!(s, "M {x0:.2} {:.2}", y(x0));
    for i in 1..=n {
        let x = x0 + len * i as f32 / n as f32;
        let _ = write!(s, " L {x:.2} {:.2}", y(x));
    }
    s
}

/// Lavoro lungo (sincronizzazione, importazione) fuori dal thread dell'interfaccia. Nel browser
/// non ci sono thread: lo si esegue subito (il risultato arriva comunque con invoke_from_event_loop).
fn spawn_background(f: impl FnOnce() + Send + 'static) {
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(f);
    #[cfg(target_arch = "wasm32")]
    f();
}

fn short_date(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

/// Pacchetto più recente nella cartella Download (desktop).
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn newest_pack_download() -> Option<PathBuf> {
    let dirs = crate::desktop_files::downloads_dirs();
    dirs.iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|rd| rd.flatten().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            n.ends_with(".json.gz")
        })
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

fn device_name() -> String {
    if cfg!(target_os = "android") {
        "android".into()
    } else {
        "desktop".into()
    }
}

pub fn run(data_dir: PathBuf) -> Result<(), slint::PlatformError> {
    crate::crash::install(&data_dir);
    let ui = AppWindow::new()?;
    #[cfg(target_arch = "wasm32")]
    crate::web::fit_window(&ui);
    let content = Content::from_files(crate::bundled::BUNDLED.iter().copied());
    // un archivio per esame; al primo avvio i dati della versione a esame unico passano al loro esame
    let exams = prep_core::exams::prepare(&data_dir, content.syllabus.as_ref());
    let store = Store::open_in(&data_dir, prep_core::exams::dir(&data_dir, &exams.active)).expect("cartella dati");
    let progress = compute(&content, &History::default(), None);
    let ctrl = Rc::new(RefCell::new(Ctrl {
        ui: ui.as_weak(),
        store,
        content,
        sessions: vec![],
        history: History::default(),
        assessment: None,
        progress,
        from_pack: false,
        unit_id: String::new(),
        quiz: None,
        cards: None,
        results: None,
        pending_flags: BTreeMap::new(),
        dialog: None,
        reader: None,
        last_save: Instant::now(),
        plan: None,
        plan_sel: vec![],
        plan_extra: vec![],
        elements: Vec::new(),
        result_perm: Default::default(),
        tick: Timer::default(),
        import_timer: Timer::default(),
        importing: false,
        toast_timer: Timer::default(),
        flag_timer: Timer::default(),
        qz: QuizSetup::default(),
        exams,
        remove_armed: None,
        p2p: None,
        share: None,
        group: None,
        groups: prep_core::group::Groups::load(&data_dir),
        study_log: prep_core::group::StudyLog::load(&data_dir),
        study_state: prep_core::group::State::Idle,
        study_since: None,
        pit_since: None,
        car_sel: String::new(),
        last_doing: String::new(),
        circuit_shown: None,
        cars_model: Rc::new(VecModel::default()),
        crews: BTreeMap::new(),
        pit_cache: None,
        photos: BTreeMap::new(),
        crop: None,
        photo_asked: BTreeMap::new(),
        photo_sent: BTreeMap::new(),
        ping_sent: BTreeMap::new(),
        kick_armed: None,
        info_sent: 0,
        uid_cache: None,
        focus_apps: Vec::new(),
        focus_confirm: None,
        photo_src: BTreeMap::new(),
        group_period: prep_core::group::Period::Today,
        group_timer: Timer::default(),
        group_last_send: 0,
        hist_sent: 0,
        gcal_month: false,
        gcal_off: 0,
        gcal_sel: None,
        fx_demo: false,
        focus_query: String::new(),
        cam_follow: None,
        profile_sel: None,
        pdf_pick: None,
        pdf_return: P_HOME,
        run: Default::default(),
        focus_on: false,
        racing_others: 0,
        study_notified: (0, -1),
        update_url: String::new(),
        group_leave_armed: None,
        my_laps: 0,
        bt: BtState::default(),
        accent: None,
        system_tones: None,
    }));
    CTRL.with(|c| *c.borrow_mut() = Some(ctrl.clone()));
    {
        let mut c = ctrl.borrow_mut();
        c.reload_content();
        c.reload_progress();
        c.load_settings_into_ui();
        c.refresh_all();
        c.refresh_exams();
        c.refresh_sessions();
        c.show_legal_if_needed();
        // studio di gruppo: cronometro e circuito ogni secondo, stanza del gruppo attivo
        c.group_timer.start(TimerMode::Repeated, Duration::from_secs(1), || with_ctrl(|c| c.group_tick()));
        c.ui().global::<AppState>().set_focus_available(crate::focus::available());
        // concentrazione ancora attiva (app chiusa dal sistema, telefono riavviato): la sessione
        // di studio riprende da dove era; altrimenti il servizio deve sapere che non ce n'è una
        // (resta solo il blocco a tempo, se non è scaduto), o aspetterebbe un «Termina» invisibile
        if !c.restore_study_session() {
            crate::focus::session(0);
        }
        c.refresh_focus();
        // colori del tema: scelti in Aspetto, altrimenti di sistema (Android 12+), altrimenti blu
        c.system_tones = crate::system_tones().filter(|t| crate::palette::dyn_roles(t).is_some());
        c.apply_theme();
        c.refresh_profile_photo();
        c.ui().global::<AppState>().set_cars(ModelRc::from(c.cars_model.clone()));
        c.updates_check();
        c.ui().global::<AppState>().set_bt_available(cfg!(target_os = "android"));
        if c.groups.active().is_some() {
            c.group_connect();
        }
        c.refresh_group();
        #[cfg(target_arch = "wasm32")]
        if let Some(inv) = crate::web::take_invite() {
            c.group_join(&inv);
            c.go_tab(4);
        }
        // collegamento automatico tra dispositivi, se scelto
        if c.store.settings().p2p_auto && !c.store.settings().p2p_password.is_empty() {
            c.p2p_connect();
        }
        c.import_timer.start(TimerMode::Repeated, Duration::from_millis(1500), || with_ctrl(|c| c.poll_import()));
    }

    let st = ui.global::<AppState>();
    st.on_go_tab(|t| with_ctrl(|c| c.go_tab(t)));
    st.on_back(|| {
        let mut r = false;
        with_ctrl(|c| r = c.back());
        r
    });
    st.on_select_subject(|s| {
        with_ctrl(|c| {
            c.ui().global::<AppState>().set_current_subject(s);
            c.refresh_study();
        })
    });
    st.on_step_subject(|dir| {
        with_ctrl(|c| {
            let ui = c.ui();
            let st = ui.global::<AppState>();
            let cur = st.get_current_subject().to_string();
            let ids: Vec<String> = c.content.syllabus().subjects.iter().map(|s| s.id.clone()).collect();
            let Some(i) = ids.iter().position(|x| *x == cur) else {
                return;
            };
            let j = i as i64 + dir as i64;
            if j < 0 || j >= ids.len() as i64 {
                return;
            }
            st.set_current_subject(ids[j as usize].as_str().into());
            c.refresh_study();
        })
    });
    st.on_open_unit(|u| {
        with_ctrl(|c| {
            c.unit_id = u.to_string();
            c.refresh_unit();
            c.set_page(P_UNIT);
        })
    });
    st.on_start_practice(|kind, id| {
        with_ctrl(|c| {
            let scope = Ctrl::scope_of(&kind, &id);
            let ids = select::practice(&c.content, &c.history, &scope, PRACTICE_N);
            let (title, subject, unit) = match &scope {
                Scope::Unit(u) => {
                    let unit = c.content.unit(u);
                    let subj = unit
                        .and_then(|x| x.topics.first())
                        .and_then(|t| c.content.subject_of_topic(&t.id))
                        .map(|s| s.id.clone());
                    (unit.map(|x| format!("U{} {}", x.n, x.name)).unwrap_or_default(), subj, Some(u.clone()))
                }
                Scope::Topic(t) => (
                    c.content.topic(t).map(|x| x.name.clone()).unwrap_or_default(),
                    c.content.subject_of_topic(t).map(|s| s.id.clone()),
                    c.content.unit_of_topic(t).map(|u| u.id.clone()),
                ),
                Scope::Subject(s) => (
                    format!("Quiz misto · {}", c.content.subject(s).map(|x| x.name.clone()).unwrap_or_default()),
                    Some(s.clone()),
                    None,
                ),
                Scope::All => ("Quiz misto".into(), None, None),
            };
            c.start_quiz(SessionKind::Practice, ids, title, subject, unit);
        })
    });
    st.on_start_exam(|sid| {
        with_ctrl(|c| {
            let rules = c.content.syllabus().exam.clone();
            let ids = select::exam(&c.content, &sid, &rules);
            let name = c.content.subject(&sid).map(|s| s.name.clone()).unwrap_or_default();
            if ids.len() < rules.questions {
                c.toast(format!(
                    "Banca domande ancora piccola: simulazione con {} quesiti invece di {}",
                    ids.len(),
                    rules.questions
                ));
            }
            c.start_quiz(SessionKind::Exam, ids, name, Some(sid.to_string()), None);
        })
    });
    st.on_start_official(|sid, tag| {
        with_ctrl(|c| {
            let prefix = format!("{sid}-{tag}-");
            let mut ids: Vec<String> =
                c.content.questions.iter().filter(|q| q.id.starts_with(&prefix)).map(|q| q.id.clone()).collect();
            ids.sort();
            let name = c.content.subject(&sid).map(|s| s.name.clone()).unwrap_or_default();
            let label = c
                .content
                .syllabus()
                .official
                .iter()
                .find(|t| t.tag == tag.as_str())
                .map(|t| t.label.clone())
                .unwrap_or_else(|| tag.to_string());
            c.start_quiz(SessionKind::Exam, ids, format!("{name} · {label}"), Some(sid.to_string()), None);
        })
    });
    st.on_start_review(|sid| {
        with_ctrl(|c| {
            let scope = if sid.is_empty() { Scope::All } else { Scope::Subject(sid.to_string()) };
            let ids = select::review(&c.content, &c.history, &scope, REVIEW_N);
            c.start_quiz(SessionKind::Review, ids, "Ripasso insicure e sbagliate".into(), None, None);
        })
    });
    st.on_start_cards(|kind, id| with_ctrl(|c| c.start_cards(&kind, &id)));
    st.on_qz_toggle(|kind, id| with_ctrl(|c| c.qz_toggle(&kind, &id)));
    st.on_qz_set_n(|n| with_ctrl(|c| c.qz_toggle("n", &n.to_string())));
    st.on_qz_start(|| with_ctrl(|c| c.qz_start()));
    st.on_select_option(|k| with_ctrl(|c| c.select_option(k)));
    st.on_fill_edited(|t| with_ctrl(|c| c.fill_edited(t.to_string())));
    st.on_toggle_unsure(|| with_ctrl(|c| c.toggle_unsure()));
    st.on_confirm(|| with_ctrl(|c| c.confirm()));
    st.on_next(|| with_ctrl(|c| c.next()));
    st.on_skip(|| with_ctrl(|c| c.skip()));
    st.on_prev(|| with_ctrl(|c| c.prev()));
    st.on_goto(|k| with_ctrl(|c| c.goto(k.max(0) as usize)));
    st.on_finish(|| with_ctrl(|c| c.request_finish()));
    st.on_result_toggle_flag(|k| with_ctrl(|c| c.result_toggle_flag(k.max(0) as usize)));
    st.on_results_close(|| with_ctrl(|c| c.results_close()));
    st.on_card_flip(|| with_ctrl(|c| c.card_flip()));
    st.on_card_grade(|g| with_ctrl(|c| c.card_grade(g)));
    st.on_card_toggle_unsure(|| with_ctrl(|c| c.card_toggle_unsure()));
    st.on_flag_toggle(|id| with_ctrl(|c| c.toggle_flag(&id, true)));
    st.on_open_ref_quiz(|| {
        with_ctrl(|c| {
            let r = c.current_question_ref();
            c.open_ref(r);
        })
    });
    st.on_open_ref_result(|k| {
        with_ctrl(|c| {
            let r = c
                .results
                .as_ref()
                .and_then(|s| s.items.get(k.max(0) as usize))
                .and_then(|it| c.content.question(&it.id))
                .and_then(|q| c.content.resolve_ref(q.book_ref.as_ref(), &q.topic));
            c.open_ref(r);
        })
    });
    st.on_open_ref_card(|| {
        with_ctrl(|c| {
            let r = c
                .cards
                .as_ref()
                .and_then(|r| r.queue.get(r.idx))
                .and_then(|id| c.content.card(id))
                .and_then(|card| c.content.resolve_ref(card.book_ref.as_ref(), &card.topic));
            c.open_ref(r);
        })
    });
    st.on_open_book(|id| with_ctrl(|c| c.open_book(&id)));
    st.on_open_periodic(|| with_ctrl(|c| c.open_periodic()));
    st.on_pt_select(|z| with_ctrl(|c| c.pt_select(z)));
    st.on_open_subject_link(|i| {
        with_ctrl(|c| {
            let cur = c.ui().global::<AppState>().get_current_subject().to_string();
            let link = c
                .content
                .subject(&cur)
                .and_then(|s| s.links.get(i as usize))
                .and_then(|l| c.content.resolve_ref(Some(&l.at), ""));
            c.open_ref(link)
        })
    });
    st.on_reader_prev(|| {
        with_ctrl(|c| {
            let k = c.reader.as_ref().map(|r| r.pos.saturating_sub(1)).unwrap_or(0);
            c.reader_goto(k);
        })
    });
    st.on_reader_next(|| {
        with_ctrl(|c| {
            let k = c.reader.as_ref().map(|r| r.pos + 1).unwrap_or(0);
            c.reader_goto(k);
        })
    });
    st.on_reader_goto(|k| with_ctrl(|c| c.reader_goto(k.max(0) as usize)));
    st.on_save_reminder(|| with_ctrl(|c| c.save_reminder()));
    st.on_goals_toggle(|i| with_ctrl(|c| c.goals_toggle(i)));
    st.on_goals_add(|t| with_ctrl(|c| c.goals_add(t.to_string())));
    st.on_goals_remove(|i| with_ctrl(|c| c.goals_remove(i)));
    st.on_goals_pick_subject(|id| {
        with_ctrl(|c| {
            c.ui().global::<AppState>().set_goal_subject(id);
            c.refresh_goal_topics();
        })
    });
    st.on_open_settings(|| {
        with_ctrl(|c| {
            c.refresh_settings_info();
            c.set_page(P_SETTINGS);
        })
    });
    st.on_goals_confirm(|| with_ctrl(|c| c.goals_confirm()));
    st.on_goals_edit(|| with_ctrl(|c| c.goals_edit()));
    let weak = ui.as_weak();
    st.on_goals_train(move |t| {
        if let Some(u) = weak.upgrade() {
            u.global::<AppState>().invoke_start_practice("topic".into(), t);
        }
    });
    st.on_goals_study(|t| {
        with_ctrl(|c| {
            let r = c.content.resolve_ref(None, &t);
            if r.is_none() {
                c.toast("Nessun materiale di studio collegato a questo argomento");
            }
            c.open_ref(r);
        })
    });
    st.on_goals_review(|t| {
        with_ctrl(|c| {
            let topic = t.to_string();
            let mastery = c
                .progress
                .subjects
                .iter()
                .flat_map(|s| &s.units)
                .flat_map(|u| &u.topics)
                .find(|x| x.id == topic)
                .map(|x| x.mastery)
                .unwrap_or(0.0);
            let n = if mastery < 0.5 { 10 } else { 6 };
            let ids = select::spaced(&c.content, &c.history, &topic, n);
            let name = c.content.topic(&topic).map(|x| x.name.clone()).unwrap_or_default();
            let subject = c.content.subject_of_topic(&topic).map(|s| s.id.clone());
            let unit = c.content.unit_of_topic(&topic).map(|u| u.id.clone());
            c.start_quiz(SessionKind::Practice, ids, format!("Ripasso · {name}"), subject, unit);
        })
    });
    st.on_import_pack(|| with_ctrl(|c| c.pick_pack()));
    st.on_open_contents(|| with_ctrl(|c| c.open_contents()));
    st.on_open_guide(|| with_ctrl(|c| c.open_guide()));
    st.on_crop_done(|x, y, s| with_ctrl(|c| c.crop_done(x, y, s)));
    st.on_crop_cancel(|| with_ctrl(|c| c.crop_cancel()));
    st.on_focus_search(|q| with_ctrl(|c| c.focus_search(&q)));
    st.on_open_pdf_tools(|| with_ctrl(|c| c.open_pdf_tools()));
    st.on_open_creator(|| with_ctrl(|c| c.open_creator()));
    st.on_cr_create_exam(|n| with_ctrl(|c| c.creator_new_exam(&n)));
    st.on_cr_open_file(|k, p| with_ctrl(|c| c.creator_open_file(k, &p)));
    st.on_cr_refresh_prompt(|| with_ctrl(|c| c.creator_prompt()));
    st.on_cr_save(|| with_ctrl(|c| c.creator_save()));
    st.on_cr_open_figure(|p| with_ctrl(|c| c.creator_open_figure(&p)));
    st.on_cr_close(|| with_ctrl(|c| c.creator_close()));
    st.on_update_install(|| with_ctrl(|c| c.updates_install()));
    st.on_pdf_split(|| with_ctrl(|c| c.pdf_pick(pdftools::PdfTool::Split)));
    st.on_pdf_text(|| with_ctrl(|c| c.pdf_pick(pdftools::PdfTool::Text)));
    st.on_open_profile(|id| {
        with_ctrl(|c| {
            c.profile_sel = if id.is_empty() { None } else { Some(id.to_string()) };
            c.refresh_group();
        })
    });
    st.on_show_in_circuit(|id| {
        with_ctrl(|c| {
            c.profile_sel = None;
            c.car_sel = id.to_string();
            c.ui().global::<AppState>().set_car_sel(id.clone());
            c.cam_follow = Some(id.to_string());
            c.refresh_group();
        })
    });
    st.on_gcal_mode(|month| {
        with_ctrl(|c| {
            c.gcal_month = month;
            c.gcal_off = 0;
            c.refresh_group();
        })
    });
    st.on_gcal_shift(|d| {
        with_ctrl(|c| {
            c.gcal_off = (c.gcal_off - d).max(0);
            c.refresh_group();
        })
    });
    st.on_gcal_select(|i| {
        with_ctrl(|c| {
            c.gcal_sel = if i < 0 || c.gcal_sel == Some(i as i64) { None } else { Some(i as i64) };
            c.refresh_group();
        })
    });
    st.set_guide_llm(ss(books::guide_for_llm()));
    st.on_rd_copied(|| with_ctrl(|c| c.toast("Copiato: incollalo dove ti serve")));
    st.on_rd_share(|t| {
        with_ctrl(|c| {
            if !crate::share_text(&t) {
                c.toast("Condivisione non disponibile qui: usa «Copia»");
            }
        })
    });
    st.on_contents_add(|| with_ctrl(|c| c.contents_add()));
    st.on_contents_remove(|p| with_ctrl(|c| c.contents_remove_ask(&p)));
    st.on_contents_export(|| with_ctrl(|c| c.contents_export()));
    st.on_select_exam(|id| with_ctrl(|c| c.open_exam(&id)));
    st.on_remove_exam(|id| with_ctrl(|c| c.remove_exam(&id)));
    st.on_save_username(|n| with_ctrl(|c| c.save_username(&n)));
    st.on_legal_accept(|| with_ctrl(|c| c.legal_accept()));
    st.on_car_color(|v| with_ctrl(|c| c.set_car_color(v as u32)));
    st.on_theme_color(|v| with_ctrl(|c| c.set_theme_color(v as u32)));
    st.on_open_goals(|| with_ctrl(|c| c.set_page(P_GOALS)));
    st.on_bt_listen(|| crate::bt_cmd("btListen", None));
    st.on_bt_scan(|| {
        with_ctrl(|c| {
            c.bt.found.clear();
            c.refresh_bt();
        });
        crate::bt_cmd("btScan", None)
    });
    st.on_bt_connect(|addr| crate::bt_cmd("btConnect", Some(&addr)));
    st.on_bt_close(|| {
        crate::bt_cmd("btClose", None);
        with_ctrl(|c| {
            c.bt.connected = None;
            c.bt.status = String::new();
            c.refresh_bt();
        })
    });
    st.on_group_create(|n| with_ctrl(|c| c.group_create(&n)));
    st.on_group_join(|t| with_ctrl(|c| c.group_join(&t)));
    st.on_group_select(|code| with_ctrl(|c| c.group_select(&code)));
    st.on_group_leave(|code| with_ctrl(|c| c.group_leave(&code)));
    st.on_group_invite(|| with_ctrl(|c| c.group_invite()));
    st.on_pick_photo(|k| with_ctrl(|c| c.pick_photo(&k)));
    st.on_remove_photo(|k| with_ctrl(|c| c.remove_photo(&k)));
    st.on_group_kick(|u| with_ctrl(|c| c.group_kick(&u)));
    st.on_group_rename(|n| with_ctrl(|c| c.group_rename(&n)));
    st.on_focus_setup(|o| with_ctrl(|c| c.focus_setup(o)));
    st.on_focus_toggle(|p| with_ctrl(|c| c.focus_toggle(&p)));
    st.on_focus_set_minutes(|m| with_ctrl(|c| c.focus_set_minutes(m)));
    st.on_focus_start(|| with_ctrl(|c| c.focus_start()));
    st.on_focus_perm(|k| crate::focus::permission(&k));
    st.on_focus_launch(|p| crate::focus::launch(&p));
    st.on_focus_dial(crate::focus::dial);
    st.on_group_ping(|u| with_ctrl(|c| c.group_ping(&u)));
    st.on_study_start(|| with_ctrl(|c| c.study_set(prep_core::group::State::Studying)));
    st.on_study_pause(|| with_ctrl(|c| c.study_set(prep_core::group::State::Pause)));
    st.on_study_stop(|| with_ctrl(|c| c.study_set(prep_core::group::State::Idle)));
    st.on_car_tap(|id| {
        with_ctrl(|c| {
            c.car_sel = if c.car_sel == id.as_str() { String::new() } else { id.to_string() };
            c.refresh_group();
        })
    });
    st.on_group_set_period(|p| {
        with_ctrl(|c| {
            c.group_period = match p {
                1 => prep_core::group::Period::Week,
                2 => prep_core::group::Period::Month,
                3 => prep_core::group::Period::Total,
                _ => prep_core::group::Period::Today,
            };
            c.refresh_group();
        })
    });
    st.on_p2p_connect(|| with_ctrl(|c| c.p2p_connect()));
    st.on_p2p_disconnect(|| with_ctrl(|c| c.p2p_disconnect()));
    st.on_p2p_download(|peer, exam| with_ctrl(|c| c.p2p_download(Sess::Sync, &peer, &exam)));
    st.on_share_start(|| with_ctrl(|c| c.share_start()));
    st.on_share_join(|| with_ctrl(|c| c.share_join()));
    st.on_share_stop(|| with_ctrl(|c| c.close_session(Sess::Share)));
    st.on_share_download(|peer, exam| with_ctrl(|c| c.p2p_download(Sess::Share, &peer, &exam)));
    st.on_wave_path(|x0, x1, cy, amp, wl, phase| ss(wave_path(x0, x1, cy, amp, wl, phase)));
    st.on_dialog_ok(|| with_ctrl(|c| c.dialog_ok()));
    st.on_dialog_alt_clicked(|| with_ctrl(|c| c.dialog_alt()));
    st.on_pause_quiz(|| with_ctrl(|c| c.pause_quiz()));
    st.on_resume_quiz(|| with_ctrl(|c| c.resume_quiz()));
    st.on_discard_paused(|| with_ctrl(|c| c.discard_paused()));
    st.on_dialog_cancel(|| with_ctrl(|c| c.close_dialog()));

    // promemoria desktop: avvio automatico + processo in background
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    crate::desktop_reminder::sync_autostart(ctrl.borrow().store.settings().reminder_enabled);

    // errore nella sessione precedente: proposta di segnalazione, a interfaccia pronta
    slint::Timer::single_shot(Duration::from_millis(1200), || with_ctrl(|c| c.crash_check()));

    crate::demo::maybe_run(&ui);
    let r = ui.run();
    with_ctrl(|c| {
        c.autosave_quiz();
        c.flush_flags();
        c.save_cards();
    });
    r
}
