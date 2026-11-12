//! Controller: collega la UI Slint al nucleo `prep-core`.

use crate::periodic;
use crate::{
    AppState, AppWindow, BlockVM, BookVM, BtDevVM, CarVM, SwatchVM, DashVM, ExamDateVM, ExamVM, FeedbackVM, FlagVM, GoalVM,
    GroupVM, NavCellVM, NavRowVM, OfferVM, OptionVM, PeerVM, QzOptVM, RankVM, ResultItemVM, SubjectVM, TocVM, TopicVM,
    UnitVM,
};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use prep_core::answer::{is_correct, Given};
use prep_core::content::Content;
use prep_core::feedback::Feedback;
use prep_core::latex;
use prep_core::markdown::{self, Block, Section};
use prep_core::model::{BookRef, QKind, Question};
use prep_core::p2p;
use prep_core::progress::{build_history, compute, exam_pace, Assessment, History, Progress};
use prep_core::select::{self, Scope};
use prep_core::session::{score_items, Item, Session, SessionKind};
use prep_core::store::Store;
use prep_core::sync::{self, SyncReport};
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
const LEGAL_VERSION: u32 = 2;
const LEGAL_TEXT: &str = "Tremendous Preparator è un programma libero (licenza GPL-3.0 o successive), fornito «così com'è», \
senza garanzie di alcun tipo. Chi lo ha scritto non risponde di danni o conseguenze derivanti dal suo uso.

• Contenuti. L'app non contiene e non distribuisce libri, quiz, flashcard o altro materiale di studio. Quello che importi \
(pacchetti, libri o slide convertiti, domande generate con l'intelligenza artificiale) è sotto la tua esclusiva \
responsabilità: assicurati di averne il diritto, usalo solo per studio personale e non condividerlo con chi non ne ha \
diritto. Nessuno ne verifica la correttezza: controlla sempre sulle fonti ufficiali.

• Servizi esterni. L'app non ha server e non raccoglie dati. Se attivi la sincronizzazione tra dispositivi, i dati \
viaggiano cifrati, direttamente tra i dispositivi quando la rete lo permette e altrimenti attraverso relay pubblici \
gestiti da terzi, che scegli tu nelle Opzioni; per stabilire il collegamento diretto l'app chiede il proprio indirizzo \
a server STUN pubblici (Google, Cloudflare), che non ricevono dati. Se configuri GitHub, i progressi vanno nel tuo \
repository. Questi servizi hanno le loro condizioni e la loro gestione dei dati (per esempio \
possono vedere l'indirizzo IP): usarli è una tua scelta, e la password della sincronizzazione va custodita da te.

• Risultati. Percentuali di preparazione, punteggi attesi, simulazioni e obiettivi sono stime indicative e non \
garantiscono l'esito di alcun esame.

Toccando «Ho letto e accetto» dichiari di aver letto queste condizioni e di accettarle.";

pub const APP_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("APP_BUILD"));

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
const P_FEEDBACK: i32 = 10;
const P_GOALS: i32 = 11;
const P_PERIODIC: i32 = 12;
const P_GROUP: i32 = 13;
/// Libro con il capitolo di nomenclatura (se presente tra i contenuti importati).
const NOMENCLATURE_BOOK: &str = "chi-integrazioni";

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
/// la chiusura dell'app). Resta solo sul dispositivo, non viene committato.
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

#[derive(Clone, Copy)]
enum Dialog {
    QuitQuiz,
    SubmitExam,
}

pub struct Ctrl {
    ui: slint::Weak<AppWindow>,
    store: Store,
    content: Content,
    sessions: Vec<Session>,
    history: History,
    assessment: Option<Assessment>,
    progress: Progress,
    from_repo: bool,
    unit_id: String,
    quiz: Option<Quiz>,
    cards: Option<CardRun>,
    results: Option<Session>,
    pending_flags: BTreeMap<String, bool>,
    dialog: Option<Dialog>,
    reader: Option<Reader>,
    fb_item: Option<String>,
    last_save: Instant,
    syncing: bool,
    sync_verbose: bool,
    /// proposta di obiettivi del giorno (se non ancora confermati) e selezione dell'utente
    plan: Option<prep_core::plan::DayPlan>,
    plan_sel: Vec<bool>,
    /// argomenti aggiunti a mano alla proposta (prima della conferma)
    plan_extra: Vec<String>,
    /// la pagina Feedback è stata aperta dalle Opzioni (il tasto indietro ci ritorna)
    feedback_from_settings: bool,
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
    group_period: prep_core::group::Period,
    group_timer: Timer,
    group_last_send: i64,
    group_leave_armed: Option<(String, Instant)>,
    /// giri completati finora oggi (per festeggiare il giro nuovo)
    my_laps: u64,
    /// collegamento Bluetooth con un telefono vicino (Android)
    bt: BtState,
    /// accento di sistema (Material You), se disponibile
    accent: Option<u32>,
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
}

/// Evento di un collegamento diretto consegnato sul thread dell'interfaccia (vedi `direct`).
pub(crate) fn on_direct_event(sess: usize, peer: String, ev: crate::direct::DirectEvent) {
    with_ctrl(|c| c.direct_event(Sess::from_index(sess), peer, ev));
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
        self.from_repo = self.store.index().synced_once;
        self.content = Content::from_files(files.iter().map(|(a, b)| (a.as_str(), b.as_str())));
        if self.content.syllabus.is_none() {
            // contenuti remoti rotti: torna a quelli inclusi
            self.content = Content::from_files(crate::bundled::BUNDLED.iter().copied());
            self.from_repo = false;
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
        self.refresh_feedback();
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

    // ---------- obiettivi del giorno ----------

    /// Obiettivi confermati con una versione che non li sincronizzava: li mette in coda una volta.
    fn share_legacy_goals(&self) {
        let today = Local::now().date_naive();
        if let Some(mut g) = prep_core::plan::Goals::load(&self.store.root) {
            if g.is_for(today) && g.updated_at.is_none() && !g.topics.is_empty() {
                let _ = g.record(&self.store);
            }
        }
    }

    fn next_exam(&self) -> Option<(String, NaiveDate)> {
        let today = Local::now().date_naive();
        self.content.syllabus().exam.dates.iter().find_map(|d| {
            let nd = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok()?;
            (nd >= today).then(|| (d.label.clone(), nd))
        })
    }

    fn today_goals(&self) -> Option<prep_core::plan::Goals> {
        prep_core::plan::Goals::latest(&self.store, Local::now().date_naive()).filter(|g| !g.topics.is_empty())
    }

    /// Obiettivi di ripasso: argomenti studiati ma fermi da almeno 2 giorni.
    fn refresh_review_goals(&self) {
        use prep_core::plan;
        let today = Local::now().date_naive();
        let items: Vec<GoalVM> =
            plan::review_due(&self.content, &self.progress, &self.history, today, REVIEW_AFTER_DAYS, 4)
                .into_iter()
                .map(|r| GoalVM {
                    topic: ss(&r.topic),
                    name: ss(&r.name),
                    info: ss(format!(
                        "{} · ultimo allenamento {} giorni fa · padronanza {}% · {} domande",
                        r.subject,
                        r.days,
                        (r.mastery * 100.0).round() as i32,
                        r.target
                    )),
                    done: 0,
                    target: r.target as i32,
                    selected: false,
                })
                .collect();
        self.ui().global::<AppState>().set_review_goals(model(items));
    }

    fn refresh_goals(&mut self) {
        use prep_core::plan;
        self.refresh_review_goals();
        let today = Local::now().date_naive();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if let Some(g) = self.today_goals() {
            let done = plan::done_on(&self.content, &self.history, today);
            let items: Vec<GoalVM> = g
                .topics
                .iter()
                .zip(&g.targets)
                .map(|(t, &target)| {
                    let n = *done.get(t).unwrap_or(&0);
                    let tp = self.content.topic(t);
                    GoalVM {
                        topic: ss(t),
                        name: ss(tp.map(|x| x.name.as_str()).unwrap_or(t)),
                        info: ss(format!("{}/{} domande oggi", n.min(target), target)),
                        done: n as i32,
                        target: target as i32,
                        selected: true,
                    }
                })
                .collect();
            let left: u32 = items.iter().map(|g| (g.target - g.done).max(0) as u32).sum();
            st.set_goals_status(ss(if left == 0 {
                "Obiettivi di oggi completati. Ottimo lavoro!".to_string()
            } else {
                format!("Ti mancano {left} domande per chiudere la giornata.")
            }));
            st.set_goals_left(items.iter().filter(|g| g.done < g.target).count() as i32);
            st.set_goals_confirmed(true);
            st.set_goals(model(items));
            self.plan = None;
            self.refresh_goal_topics();
            return;
        }
        let exam = self.next_exam();
        let (due, new) = select::count_due(&self.content, &self.history, &Scope::All, Utc::now());
        let mut p = plan::suggest(
            &self.content,
            &self.progress,
            &self.history,
            today,
            exam.as_ref().map(|(l, d)| (l.as_str(), *d)),
            due + new.min(CARDS_NEW),
            4,
        );
        let per = p.suggestions.first().map(|x| x.target).unwrap_or(10);
        for t in &self.plan_extra {
            if p.suggestions.iter().any(|x| &x.topic == t) {
                continue;
            }
            if let Some(tp) = self.content.topic(t) {
                p.suggestions.push(plan::Suggestion {
                    topic: t.clone(),
                    name: tp.name.clone(),
                    subject: self.content.subject_of_topic(t).map(|x| x.name.clone()).unwrap_or_default(),
                    unit: String::new(),
                    score: 0.0,
                    reasons: vec!["scelto da te".into()],
                    target: per,
                });
            }
        }
        if self.plan_sel.len() != p.suggestions.len()
            || self.plan.as_ref().map(|x| &x.suggestions) != Some(&p.suggestions)
        {
            self.plan_sel = vec![true; p.suggestions.len()];
        }
        let items: Vec<GoalVM> = p
            .suggestions
            .iter()
            .zip(&self.plan_sel)
            .map(|(x, &sel)| GoalVM {
                topic: ss(&x.topic),
                name: ss(&x.name),
                info: ss(format!("{} · {} · {} domande", x.subject, x.reasons.join(", "), x.target)),
                done: 0,
                target: x.target as i32,
                selected: sel,
            })
            .collect();
        st.set_goals_note(ss(format!(
            "{} Oggi: ~{} domande + {} flashcard (circa {} h {:02} min). Scegli gli argomenti e conferma.",
            p.note,
            p.questions,
            p.cards_due,
            p.minutes / 60,
            p.minutes % 60
        )));
        st.set_goals_confirmed(false);
        st.set_goals_left(0);
        st.set_goals(model(items));
        self.plan = Some(p);
        self.refresh_goal_topics();
    }

    fn chosen_topics(&self) -> Vec<String> {
        match self.today_goals() {
            Some(g) => g.topics,
            None => self
                .plan
                .as_ref()
                .map(|p| {
                    p.suggestions.iter().zip(&self.plan_sel).filter(|(_, &s)| s).map(|(x, _)| x.topic.clone()).collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Elenco degli argomenti della materia scelta nella scheda Obiettivi.
    fn refresh_goal_topics(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let mut subj = st.get_goal_subject().to_string();
        if subj.is_empty() {
            subj = self.progress.subjects.first().map(|s| s.id.clone()).unwrap_or_default();
            st.set_goal_subject(ss(&subj));
        }
        let chosen = self.chosen_topics();
        let items: Vec<GoalVM> = self
            .progress
            .subjects
            .iter()
            .filter(|s| s.id == subj)
            .flat_map(|s| s.units.iter())
            .flat_map(|u| u.topics.iter().filter(|t| t.questions > 0).map(move |t| (u, t)))
            .map(|(u, t)| GoalVM {
                topic: ss(&t.id),
                name: ss(&t.name),
                info: ss(format!("U{} · padronanza {:.0}% · {} domande", u.n, t.mastery * 100.0, t.questions)),
                done: 0,
                target: 0,
                selected: chosen.contains(&t.id),
            })
            .collect();
        st.set_goal_topics(model(items));
    }

    fn goals_add(&mut self, topic: String) {
        if let Some(mut g) = self.today_goals() {
            if g.topics.contains(&topic) {
                self.toast("È già tra gli obiettivi di oggi");
                return;
            }
            let target = (g.targets.iter().sum::<u32>() / g.targets.len().max(1) as u32).max(10);
            g.topics.push(topic);
            g.targets.push(target);
            if let Err(e) = g.record(&self.store) {
                self.toast(format!("Errore: {e}"));
                return;
            }
            self.toast("Aggiunto agli obiettivi di oggi");
            self.write_reminder();
        } else {
            match self.plan.as_ref().and_then(|p| p.suggestions.iter().position(|x| x.topic == topic)) {
                Some(i) => {
                    if let Some(x) = self.plan_sel.get_mut(i) {
                        *x = true;
                    }
                }
                None => self.plan_extra.push(topic),
            }
            self.toast("Aggiunto alla proposta: conferma per renderlo un obiettivo");
        }
        self.refresh_goals();
    }

    fn goals_remove(&mut self, i: i32) {
        let Some(mut g) = self.today_goals() else {
            return;
        };
        let i = i as usize;
        if i >= g.topics.len() {
            return;
        }
        g.topics.remove(i);
        g.targets.remove(i);
        if let Err(e) = g.record(&self.store) {
            self.toast(format!("Errore: {e}"));
        }
        self.refresh_goals();
        self.write_reminder();
    }

    fn goals_toggle(&mut self, i: i32) {
        if let Some(x) = self.plan_sel.get_mut(i as usize) {
            *x = !*x;
        }
        self.refresh_goals();
    }

    fn goals_confirm(&mut self) {
        let Some(p) = &self.plan else { return };
        let chosen: Vec<&prep_core::plan::Suggestion> =
            p.suggestions.iter().zip(&self.plan_sel).filter(|(_, &s)| s).map(|(x, _)| x).collect();
        if chosen.is_empty() {
            self.toast("Scegli almeno un argomento");
            return;
        }
        // il totale del giorno resta lo stesso, ripartito sugli argomenti scelti
        let per = (p.questions as usize).div_ceil(chosen.len()) as u32;
        let mut g = prep_core::plan::Goals {
            date: Local::now().date_naive().format("%Y-%m-%d").to_string(),
            topics: chosen.iter().map(|x| x.topic.clone()).collect(),
            targets: vec![per.max(5); chosen.len()],
            confirmed_at: Some(Utc::now()),
            updated_at: None,
        };
        match g.record(&self.store) {
            Ok(()) => {
                self.toast("Obiettivi confermati: li trovi in Home e nella notifica");
                self.refresh_goals();
                self.write_reminder();
            }
            Err(e) => self.toast(format!("Errore: {e}")),
        }
    }

    fn goals_edit(&mut self) {
        if self.today_goals().is_some() {
            let day = Local::now().date_naive().format("%Y-%m-%d").to_string();
            let _ = prep_core::plan::Goals::cleared(&day).record(&self.store);
        }
        self.plan = None;
        self.plan_sel.clear();
        self.plan_extra.clear();
        self.refresh_goals();
        self.write_reminder();
    }

    /// Aggiorna reminder.json, letto dal promemoria giornaliero (receiver Android / desktop).
    fn write_reminder(&self) {
        use prep_core::reminder::{parse_time, DayCount, Paused, ReminderState};
        let s = self.store.settings();
        let (hour, minute) = parse_time(&s.reminder_time).unwrap_or((18, 30));
        let paused = self.load_saved_quiz().map(|q| Paused {
            title: q.title.clone(),
            done: q.given.iter().filter(|g| g.is_some()).count(),
            total: q.ids.len(),
        });
        let today = Local::now().date_naive();
        let due = (0..8u64)
            .filter_map(|i| {
                let day = today + chrono::Days::new(i);
                let end = Local.from_local_datetime(&day.and_hms_opt(23, 59, 0)?).earliest()?;
                let (due, new) = select::count_due(&self.content, &self.history, &Scope::All, end.with_timezone(&Utc));
                Some(DayCount { date: day.format("%Y-%m-%d").to_string(), n: due + new.min(CARDS_NEW) })
            })
            .collect();
        let exam = self
            .content
            .syllabus()
            .exam
            .dates
            .iter()
            .find(|d| NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").is_ok_and(|nd| nd >= today));
        let (evening_hour, evening_minute) = parse_time(&s.evening_time).unwrap_or((21, 30));
        let goals = self.today_goals().map(|g| {
            let done = prep_core::plan::done_on(&self.content, &self.history, today);
            prep_core::reminder::GoalsState {
                date: g.date.clone(),
                items: g
                    .topics
                    .iter()
                    .zip(&g.targets)
                    .map(|(t, &target)| prep_core::reminder::GoalItem {
                        name: self.content.topic(t).map(|x| x.name.clone()).unwrap_or_else(|| t.clone()),
                        done: *done.get(t).unwrap_or(&0),
                        target,
                    })
                    .collect(),
            }
        });
        let st = ReminderState {
            evening_hour,
            evening_minute,
            goals,
            enabled: s.reminder_enabled,
            hour,
            minute,
            paused,
            due,
            last_study: self
                .history
                .last_activity
                .map(|t| t.with_timezone(&Local).format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            exam_label: exam.map(|d| d.label.clone()).unwrap_or_default(),
            exam_date: exam.map(|d| d.date.clone()).unwrap_or_default(),
            times: vec![],
            studied_today: 0,
            slots: vec![],
        };
        let mut st = st;
        st.times = if s.reminder_times.is_empty() {
            prep_core::reminder::DEFAULT_TIMES.iter().map(|t| t.to_string()).collect()
        } else {
            s.reminder_times.clone()
        };
        st.studied_today = prep_core::plan::done_on(&self.content, &self.history, today).values().sum();
        st.compute_slots(today);
        let _ = st.save(&self.store.base);
    }

    fn save_reminder(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let raw = st.get_s_rem_time().to_string();
        let mut times: Vec<(u32, u32)> = Vec::new();
        for part in raw.split([',', ';', ' ']).filter(|x| !x.trim().is_empty()) {
            match prep_core::reminder::parse_time(part) {
                Some(t) => times.push(t),
                None => {
                    self.toast(format!("Orario non valido: \"{}\". Usa HH:MM separati da virgola, per esempio 08:30, 13:45, 17:30, 21:00", part.trim()));
                    return;
                }
            }
        }
        if times.is_empty() {
            self.toast("Indica almeno un orario, per esempio 08:30, 13:45, 17:30, 21:00");
            return;
        }
        times.sort();
        times.dedup();
        let mut s = self.store.settings();
        s.reminder_enabled = st.get_s_rem_on();
        s.reminder_times = times.iter().map(|(h, m)| format!("{h:02}:{m:02}")).collect();
        let list = s.reminder_times.join(", ");
        st.set_s_rem_time(ss(&list));
        match self.store.save_settings(&s) {
            Ok(()) => {
                self.write_reminder();
                #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
                crate::desktop_reminder::sync_autostart(s.reminder_enabled);
                self.toast(if s.reminder_enabled {
                    format!("Promemoria alle {list}")
                } else {
                    "Promemoria disattivati".into()
                });
            }
            Err(e) => self.toast(format!("Errore: {e}")),
        }
    }

    fn subject_vms(&self) -> Vec<SubjectVM> {
        let now = Utc::now();
        self.progress
            .subjects
            .iter()
            .map(|s| {
                let (due, new) = select::count_due(&self.content, &self.history, &Scope::Subject(s.id.clone()), now);
                SubjectVM {
                    id: ss(&s.id),
                    name: ss(&s.name),
                    mastery: s.mastery as f32,
                    predicted: ss(format!("≈ {}/31", it1(s.predicted))),
                    flagged: s.flagged as i32,
                    due: due as i32,
                    new_cards: new as i32,
                    learned: select::count_learned(&self.content, &self.history, &Scope::Subject(s.id.clone()), now)
                        as i32,
                    questions: s.units.iter().map(|u| u.questions as i32).sum(),
                    note: ss(&s.ai_note),
                    pace: ss(if s.timed > 0 {
                        format!("{} di studio · media {}/dom.", hm(s.total_secs), mmss(s.avg_secs))
                    } else {
                        String::new()
                    }),
                }
            })
            .collect()
    }

    fn refresh_home(&mut self) {
        self.refresh_paused();
        self.refresh_goals();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_overall(self.progress.overall as f32);
        st.set_overall_label(ss(match self.progress.anchored_at {
            Some(t) => format!(
                "Rivista da AI il {}, poi aggiornata con le nuove risposte",
                t.with_timezone(&Local).format("%d/%m %H:%M")
            ),
            None => "Stima automatica da quiz e flashcard".into(),
        }));
        let today = Local::now().date_naive();
        let dates: Vec<ExamDateVM> = self
            .content
            .syllabus()
            .exam
            .dates
            .iter()
            .filter_map(|d| {
                let nd = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok()?;
                Some(ExamDateVM {
                    label: ss(&d.label),
                    date: ss(nd.format("%d/%m/%Y").to_string()),
                    days: (nd - today).num_days() as i32,
                })
            })
            .collect();
        st.set_exam_dates(model(dates));
        st.set_total_time(ss(if self.progress.total_secs > 0 {
            format!(
                "Studio cronometrato: {} · media {}/domanda",
                hm(self.progress.total_secs),
                mmss(self.progress.avg_secs)
            )
        } else {
            String::new()
        }));
        st.set_subjects(model(self.subject_vms()));
        let qflag = self.history.flagged.iter().filter(|id| self.content.question(id).is_some()).count();
        let wrong_only = self
            .history
            .last_wrong
            .iter()
            .filter(|id| !self.history.flagged.contains(*id) && self.content.question(id).is_some())
            .count();
        st.set_flagged_count(qflag as i32);
        st.set_wrong_count(wrong_only as i32);
        let (due, new) = select::count_due(&self.content, &self.history, &Scope::All, Utc::now());
        st.set_due_count((due + new.min(CARDS_NEW)) as i32);
        match &self.assessment {
            Some(a) => {
                st.set_ai_summary(ss(&a.summary));
                let mut pr: Vec<SharedString> = a
                    .priorities
                    .iter()
                    .map(|p| {
                        let name = self.content.topic(&p.topic).map(|t| t.name.clone()).unwrap_or(p.topic.clone());
                        ss(if p.reason.is_empty() { name } else { format!("{name}: {}", p.reason) })
                    })
                    .collect();
                pr.extend(a.plan.iter().map(ss));
                st.set_ai_priorities(model(pr));
            }
            None => {
                st.set_ai_summary(ss(""));
                st.set_ai_priorities(model(vec![]));
            }
        }
        st.set_sync_status(ss(sync::status_line(&self.store)));
        st.set_sync_configured(self.sync_configured());
        let warn = if self.content.warnings.is_empty() {
            String::new()
        } else {
            format!(" · {} avvisi nei contenuti", self.content.warnings.len())
        };
        st.set_content_info(ss(format!(
            "{} domande · {} flashcard · contenuti {}{} · {} sessioni",
            self.content.questions.len(),
            self.content.cards.len(),
            if self.from_repo { "dal repository" } else { "inclusi nell'app" },
            warn,
            self.sessions.len()
        )));
    }

    fn refresh_study(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_has_nomenclature(self.content.book(NOMENCLATURE_BOOK).is_some());
        let cur = st.get_current_subject().to_string();
        let Some(sp) = self.progress.subjects.iter().find(|s| s.id == cur) else {
            return;
        };
        let units: Vec<UnitVM> = sp
            .units
            .iter()
            .map(|u| UnitVM {
                id: ss(&u.id),
                n: u.n as i32,
                name: ss(&u.name),
                cfu: ss(it1(u.cfu).trim_end_matches(",0")),
                mastery: u.mastery as f32,
                touched: u.touched as f32,
                questions: u.questions as i32,
                cards: u.cards as i32,
                flagged: u.flagged as i32,
                note: ss(&u.ai_note),
                time: ss(if u.timed > 0 {
                    format!("{} · media {}/dom.", hm(u.total_secs), mmss(u.avg_secs))
                } else {
                    String::new()
                }),
            })
            .collect();
        st.set_units(model(units));
        let books: Vec<BookVM> = self
            .content
            .books
            .iter()
            .filter(|b| b.subject == cur)
            .map(|b| BookVM {
                id: ss(&b.id),
                title: ss(&b.title),
                author: ss(&b.author),
                chapters: b.chapters.len() as i32,
                available: b.chapters.iter().any(|c| self.read_book_file(&c.file).is_some()),
            })
            .collect();
        st.set_books(model(books));
        let rules = &self.content.syllabus().exam;
        st.set_exam_rules(ss(format!(
            "{} quesiti ({} a risposta multipla con {} opzioni + {} a completamento) in {} minuti per materia. \
             +1 corretta, −0,1 errata, 0 omessa. Superata con ≥ 18; 31 = 30 e lode. \
             Le tre prove si svolgono lo stesso giorno.",
            rules.questions, rules.mcq, rules.options, rules.fill, rules.minutes
        )));
    }

    fn refresh_unit(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let Some(up) = self.progress.subjects.iter().flat_map(|s| s.units.iter()).find(|u| u.id == self.unit_id) else {
            return;
        };
        st.set_unit(UnitVM {
            id: ss(&up.id),
            n: up.n as i32,
            name: ss(&up.name),
            cfu: ss(it1(up.cfu).trim_end_matches(",0")),
            mastery: up.mastery as f32,
            touched: up.touched as f32,
            questions: up.questions as i32,
            cards: up.cards as i32,
            flagged: up.flagged as i32,
            note: ss(&up.ai_note),
            time: ss(if up.timed > 0 {
                format!("{} · media {}/dom.", hm(up.total_secs), mmss(up.avg_secs))
            } else {
                String::new()
            }),
        });
        let pace = exam_pace(self.content.syllabus().exam.minutes, self.content.syllabus().exam.questions);
        let topics: Vec<TopicVM> = up
            .topics
            .iter()
            .map(|t| {
                let detail = if t.questions == 0 && t.cards == 0 {
                    "nessun contenuto ancora".to_string()
                } else {
                    let mut d = format!("{}/{} domande viste", t.seen_questions, t.questions);
                    if t.cards > 0 {
                        d.push_str(&format!(" · {}/{} carte", t.seen_cards, t.cards));
                    }
                    if t.attempts > 0 {
                        d.push_str(&format!(" · acc. {}%", (t.accuracy * 100.0).round()));
                    }
                    if t.flagged > 0 {
                        d.push_str(&format!(" · {} insicure", t.flagged));
                    }
                    if t.timed > 0 {
                        d.push_str(&format!(" · {} · media {}/dom.", hm(t.total_secs), mmss(t.avg_secs)));
                        if t.avg_secs > pace {
                            d.push_str(" (lento)");
                        }
                    }
                    d
                };
                TopicVM {
                    id: ss(&t.id),
                    name: ss(&t.name),
                    mastery: t.mastery as f32,
                    detail: ss(detail),
                    has_ai: t.ai.is_some(),
                    ai_text: ss(t.ai.map(|a| format!("AI {}%", (a * 100.0).round())).unwrap_or_default()),
                    note: ss(&t.ai_note),
                    questions: t.questions as i32,
                }
            })
            .collect();
        st.set_topics(model(topics));
    }

    fn where_of(&self, topic: &str) -> String {
        match (self.content.subject_of_topic(topic), self.content.unit_of_topic(topic), self.content.topic(topic)) {
            (Some(s), Some(u), Some(t)) => format!("{} · U{} · {}", s.name, u.n, t.name),
            _ => topic.to_string(),
        }
    }

    fn refresh_flags(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let mut ids: Vec<String> = self.history.flagged.iter().cloned().collect();
        ids.extend(self.history.last_wrong.iter().filter(|i| !self.history.flagged.contains(*i)).cloned());
        // tieni visibili anche quelle appena tolte (per poterle rimettere)
        ids.extend(self.pending_flags.iter().filter(|(_, on)| !**on).map(|(id, _)| id.clone()));
        ids.sort();
        ids.dedup();
        let mut quiz: Vec<FlagVM> = Vec::new();
        let mut cards: Vec<FlagVM> = Vec::new();
        for id in &ids {
            if let Some(q) = self.content.question(id) {
                quiz.push(FlagVM {
                    id: ss(id),
                    text: tx(&q.text),
                    r#where: ss(self.where_of(&q.topic)),
                    flagged: self.history.flagged.contains(id),
                    wrong: self.history.last_wrong.contains(id),
                });
            } else if let Some(c) = self.content.card(id) {
                cards.push(FlagVM {
                    id: ss(id),
                    text: tx(&c.front),
                    r#where: ss(self.where_of(&c.topic)),
                    flagged: self.history.flagged.contains(id),
                    wrong: false,
                });
            }
        }
        quiz.sort_by_key(|f| (!f.flagged, !f.wrong, f.r#where.to_string()));
        cards.sort_by_key(|f| (!f.flagged, f.r#where.to_string()));
        let ncards = self.flagged_cards().len();
        st.set_flags(model(quiz));
        st.set_card_flags(model(cards));
        st.set_card_flagged_count(ncards as i32);
    }

    fn flagged_cards(&self) -> Vec<String> {
        self.history.flagged.iter().filter(|id| self.content.card(id).is_some()).cloned().collect()
    }

    // ---------- quiz personalizzato ----------

    fn refresh_quiz_setup(&mut self) {
        let Some(syl) = self.content.syllabus.as_ref() else { return };
        let all: Vec<String> = syl.subjects.iter().map(|s| s.id.clone()).collect();
        let qz = &mut self.qz;
        qz.subjects.retain(|s| all.contains(s));
        if !qz.init {
            qz.subjects = all.iter().cloned().collect();
            qz.init = true;
        }
        let qz = &self.qz;
        // conteggi in un solo passaggio
        let mut per_subject: BTreeMap<String, usize> = BTreeMap::new();
        let mut per_unit: BTreeMap<String, usize> = BTreeMap::new();
        let mut per_source: BTreeMap<String, usize> = BTreeMap::new();
        let mut available = 0usize;
        for q in &self.content.questions {
            if let Some(sub) = self.content.subject_of_topic(&q.topic) {
                *per_subject.entry(sub.id.clone()).or_default() += 1;
            }
            if let Some(u) = self.content.unit_of_topic(&q.topic) {
                *per_unit.entry(u.id.clone()).or_default() += 1;
            }
            if qz.keep_scope(&self.content, q) {
                let g = q.source_group();
                if !qz.off_sources.contains(&g) {
                    available += 1;
                }
                *per_source.entry(g).or_default() += 1;
            } else {
                per_source.entry(q.source_group()).or_default();
            }
        }
        let subjects: Vec<QzOptVM> = syl
            .subjects
            .iter()
            .map(|s| QzOptVM {
                id: ss(&s.id),
                name: ss(&s.name),
                group: ss(""),
                sel: qz.subjects.contains(&s.id),
                count: *per_subject.get(&s.id).unwrap_or(&0) as i32,
                head: false,
                open: false,
            })
            .collect();
        let mut units: Vec<QzOptVM> = Vec::new();
        for s in syl.subjects.iter().filter(|s| qz.subjects.contains(&s.id)) {
            let chosen = s.units.iter().filter(|u| qz.units.contains(&u.id)).count();
            let open = qz.open.contains(&s.id);
            units.push(QzOptVM {
                id: ss(&s.id),
                name: ss(&s.name),
                group: ss(if chosen == 0 {
                    "tutte le unità".to_string()
                } else {
                    format!("{chosen} di {} unità", s.units.len())
                }),
                sel: false,
                count: 0,
                head: true,
                open,
            });
            if !open {
                continue;
            }
            for u in &s.units {
                units.push(QzOptVM {
                    id: ss(&u.id),
                    name: ss(format!("U{} · {}", u.n, u.name)),
                    group: ss(&s.name),
                    sel: qz.units.contains(&u.id),
                    count: *per_unit.get(&u.id).unwrap_or(&0) as i32,
                    head: false,
                    open: false,
                });
            }
        }
        // fonti: prima le più ricche
        let mut src: Vec<(String, usize)> = per_source.into_iter().collect();
        src.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let sources: Vec<QzOptVM> = src
            .into_iter()
            .map(|(g, n)| QzOptVM {
                id: ss(&g),
                name: ss(&g),
                group: ss(""),
                sel: !qz.off_sources.contains(&g),
                count: n as i32,
                head: false,
                open: false,
            })
            .collect();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_qz_subjects(model(subjects));
        st.set_qz_units(model(units));
        st.set_qz_sources(model(sources));
        st.set_qz_mcq(!qz.no_mcq);
        st.set_qz_fill(!qz.no_fill);
        st.set_qz_n(qz.n as i32);
        st.set_qz_available(available as i32);
    }

    fn qz_toggle(&mut self, kind: &str, id: &str) {
        let flip = |set: &mut BTreeSet<String>, id: &str| {
            if !set.remove(id) {
                set.insert(id.to_string());
            }
        };
        match kind {
            "subject" => {
                if self.qz.subjects.contains(id) {
                    self.qz.subjects.remove(id);
                    // via anche le sue unità spuntate
                    if let Some(sub) = self.content.subject(id) {
                        for u in &sub.units {
                            self.qz.units.remove(&u.id);
                        }
                    }
                } else {
                    self.qz.subjects.insert(id.to_string());
                }
            }
            "unit" => flip(&mut self.qz.units, id),
            "open" => flip(&mut self.qz.open, id),
            "source" => flip(&mut self.qz.off_sources, id),
            "kind" if id == "mcq" => self.qz.no_mcq = !self.qz.no_mcq,
            "kind" if id == "fill" => self.qz.no_fill = !self.qz.no_fill,
            "n" => self.qz.n = id.parse().unwrap_or(self.qz.n).clamp(1, 200),
            _ => {}
        }
        self.refresh_quiz_setup();
    }

    fn qz_start(&mut self) {
        let qz = &self.qz;
        let content = &self.content;
        let ids = select::practice_where(content, &self.history, |q| qz.keep(content, q), qz.n);
        let subs: Vec<&prep_core::model::Subject> =
            content.syllabus().subjects.iter().filter(|s| qz.subjects.contains(&s.id)).collect();
        let subject = if subs.len() == 1 { Some(subs[0].id.clone()) } else { None };
        let unit = if qz.units.len() == 1 { qz.units.iter().next().cloned() } else { None };
        let title = match (subs.len(), &unit) {
            (_, Some(u)) => content.unit(u).map(|x| format!("U{} {}", x.n, x.name)).unwrap_or_else(|| "Quiz".into()),
            (1, None) => format!("Quiz · {}", subs[0].name),
            _ => "Quiz personalizzato".into(),
        };
        self.start_quiz(SessionKind::Practice, ids, title, subject, unit);
    }

    fn refresh_settings_info(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_app_version(ss(format!("Versione {APP_VERSION}")));
        st.set_sync_status(ss(sync::status_line(&self.store)));
        st.set_sync_configured(self.sync_configured());
        st.set_on_android(cfg!(target_os = "android"));
        st.set_pack_status(ss(self.pack_status()));
    }

    fn load_settings_into_ui(&self) {
        let s = self.store.settings();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_s_owner(ss(&s.owner));
        st.set_s_repo(ss(&s.repo));
        st.set_s_branch(ss(&s.branch));
        st.set_s_token(ss(&s.token));
        st.set_s_username(ss(&s.username));
        st.set_p2p_auto(s.p2p_auto);
        st.set_p2p_direct_only(s.direct_only);
        st.set_p2p_password(ss(&s.p2p_password));
        st.set_p2p_relays(ss(if s.relays.is_empty() { p2p::DEFAULT_RELAYS.join(", ") } else { s.relays.join(", ") }));
        st.set_s_rem_on(s.reminder_enabled);
        let times = if s.reminder_times.is_empty() {
            prep_core::reminder::DEFAULT_TIMES.join(", ")
        } else {
            s.reminder_times.join(", ")
        };
        st.set_s_rem_time(ss(times));
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
        match st.get_page() {
            P_QUIZ => {
                self.open_dialog(
                    Dialog::QuitQuiz,
                    "Uscire dal quiz?",
                    if self.quiz.as_ref().is_some_and(|q| q.is_exam()) {
                        "Pausa: la simulazione si ferma (anche il timer) e la riprendi dalla Home.\nTermina: la simulazione viene annullata e non salvata."
                    } else {
                        "Pausa: riprendi più tardi dalla Home da dove eri arrivato.\nTermina: le risposte già confermate vengono salvate e committate."
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
            P_FEEDBACK if self.feedback_from_settings => {
                self.feedback_from_settings = false;
                self.set_page(P_SETTINGS);
                true
            }
            P_FEEDBACK if self.results.is_some() => {
                self.set_page(P_RESULTS);
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
            None => {}
        }
    }

    fn dialog_alt(&mut self) {
        let d = self.dialog.take();
        self.ui().global::<AppState>().set_dialog_visible(false);
        if let Some(Dialog::QuitQuiz) = d {
            self.pause_quiz();
        }
    }

    // ---------- pausa / ripresa ----------

    fn saved_quiz_path(&self) -> PathBuf {
        self.store.root.join("quiz_in_corso.json")
    }

    fn autosave_quiz(&mut self) {
        let Some(q) = &mut self.quiz else { return };
        let now = Instant::now();
        // tempo della domanda corrente fino a ora, senza "consumarlo"
        let mut secs = q.secs.clone();
        if q.is_exam() || !q.checked[q.idx] {
            secs[q.idx] += now.duration_since(q.shown_at).as_secs_f64();
        }
        let saved = SavedQuiz {
            kind: q.kind,
            subject: q.subject.clone(),
            unit: q.unit.clone(),
            title: q.title.clone(),
            ids: q.questions.iter().map(|x| x.id.clone()).collect(),
            given: q.given.iter().map(encode_given).collect(),
            unsure: q.unsure.clone(),
            checked: q.checked.clone(),
            secs,
            idx: q.idx,
            started: q.started,
            remaining_secs: q.deadline.map(|d| d.saturating_duration_since(now).as_secs()),
            saved_at: Utc::now(),
            perm: q.perm.clone(),
        };
        if let Ok(j) = serde_json::to_string(&saved) {
            let _ = vfs::write(&self.saved_quiz_path(), j.as_bytes());
        }
        self.write_reminder();
    }

    fn load_saved_quiz(&self) -> Option<SavedQuiz> {
        let t = vfs::read_to_string(&self.saved_quiz_path()).ok()?;
        serde_json::from_str(&t).ok()
    }

    fn clear_saved_quiz(&mut self) {
        let _ = vfs::remove_file(&self.saved_quiz_path());
        self.write_reminder();
    }

    fn pause_quiz(&mut self) {
        if self.quiz.is_none() {
            return;
        }
        self.close_dialog();
        self.autosave_quiz();
        self.quiz = None;
        self.tick.stop();
        self.refresh_home();
        let tab = self.ui().global::<AppState>().get_tab();
        self.set_page(tab_page(tab));
        self.toast("Quiz in pausa: lo riprendi dalla Home");
    }

    fn resume_quiz(&mut self) {
        let Some(sq) = self.load_saved_quiz() else {
            self.refresh_home();
            return;
        };
        let mut questions = Vec::new();
        let mut given = Vec::new();
        let mut unsure = Vec::new();
        let mut checked = Vec::new();
        let mut secs = Vec::new();
        let mut idx = 0;
        for (k, id) in sq.ids.iter().enumerate() {
            if let Some(q) = self.content.question(id) {
                if k == sq.idx {
                    idx = questions.len();
                }
                questions.push(q.clone());
                given.push(decode_given(&sq.given[k]));
                unsure.push(sq.unsure[k]);
                checked.push(sq.checked[k]);
                secs.push(sq.secs[k]);
            }
        }
        if questions.is_empty() {
            self.clear_saved_quiz();
            self.refresh_home();
            self.toast("Le domande del quiz in pausa non ci sono più");
            return;
        }
        let perm: Vec<Vec<usize>> = questions
            .iter()
            .enumerate()
            .map(|(i, q)| match sq.perm.get(i) {
                Some(p) if p.len() == q.options.len() => p.clone(),
                _ => (0..q.options.len()).collect(),
            })
            .collect();
        self.quiz = Some(Quiz {
            kind: sq.kind,
            subject: sq.subject,
            unit: sq.unit,
            title: sq.title,
            questions,
            perm,
            given,
            unsure,
            checked,
            secs,
            idx,
            started: sq.started,
            shown_at: Instant::now(),
            deadline: sq.remaining_secs.map(|r| Instant::now() + Duration::from_secs(r)),
        });
        self.tick.start(TimerMode::Repeated, Duration::from_millis(500), || {
            with_ctrl(|c| c.on_tick());
        });
        self.render_question();
        self.set_page(P_QUIZ);
    }

    fn discard_paused(&mut self) {
        self.clear_saved_quiz();
        self.refresh_home();
        self.toast("Quiz in pausa scartato");
    }

    fn refresh_paused(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        match (self.quiz.is_none(), self.load_saved_quiz()) {
            (true, Some(sq)) => {
                let answered = sq.given.iter().filter(|g| g.is_some()).count();
                let mut info = format!(
                    "Domanda {} di {} · {} risposte · in pausa dal {}",
                    sq.idx + 1,
                    sq.ids.len(),
                    answered,
                    sq.saved_at.with_timezone(&Local).format("%d/%m %H:%M")
                );
                if let Some(r) = sq.remaining_secs {
                    info.push_str(&format!(" · {}:{:02} rimanenti", r / 60, r % 60));
                }
                st.set_paused_title(ss(&sq.title));
                st.set_paused_info(ss(info));
            }
            _ => {
                st.set_paused_title(ss(""));
                st.set_paused_info(ss(""));
            }
        }
    }

    // ---------- quiz ----------

    fn scope_of(kind: &str, id: &str) -> Scope {
        match kind {
            "subject" => Scope::Subject(id.into()),
            "unit" => Scope::Unit(id.into()),
            "topic" => Scope::Topic(id.into()),
            _ => Scope::All,
        }
    }

    fn start_quiz(
        &mut self,
        kind: SessionKind,
        ids: Vec<String>,
        title: String,
        subject: Option<String>,
        unit: Option<String>,
    ) {
        if ids.is_empty() {
            self.toast("Nessuna domanda disponibile qui (contenuti in arrivo)");
            return;
        }
        if self.load_saved_quiz().is_some() {
            self.clear_saved_quiz();
            self.toast("Il quiz che avevi in pausa è stato sostituito da questo");
        }
        let questions: Vec<Question> = ids.iter().filter_map(|id| self.content.question(id).cloned()).collect();
        let n = questions.len();
        let minutes = self.content.syllabus().exam.minutes as u64;
        let deadline = (kind == SessionKind::Exam).then(|| Instant::now() + Duration::from_secs(minutes * 60));
        // opzioni mescolate a ogni presentazione: la risposta giusta non è mai "sempre la A"
        let perm = questions.iter().map(|q| prep_core::model::shuffled_order(q.options.len())).collect();
        self.quiz = Some(Quiz {
            kind,
            subject,
            unit,
            title,
            questions,
            perm,
            given: vec![Given::None; n],
            unsure: vec![false; n],
            checked: vec![false; n],
            secs: vec![0.0; n],
            idx: 0,
            started: Utc::now(),
            shown_at: Instant::now(),
            deadline,
        });
        // cronometro: per domanda in tutte le modalità, tempo residuo in simulazione
        self.tick.start(TimerMode::Repeated, Duration::from_millis(500), || {
            with_ctrl(|c| c.on_tick());
        });
        self.render_question();
        self.set_page(P_QUIZ);
    }

    fn on_tick(&mut self) {
        if self.quiz.is_none() {
            self.tick.stop();
            return;
        }
        self.update_elapsed();
        if self.last_save.elapsed() > Duration::from_secs(10) {
            self.autosave_quiz();
            self.last_save = Instant::now();
        }
        let Some(q) = &self.quiz else { return };
        let Some(dl) = q.deadline else { return };
        let left = dl.saturating_duration_since(Instant::now()).as_secs();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_time_left(ss(format!("{:02}:{:02}", left / 60, left % 60)));
        st.set_time_warning(left < 300);
        if left == 0 {
            self.tick.stop();
            self.close_dialog();
            self.finish_quiz();
            self.toast("Tempo scaduto: simulazione consegnata");
        }
    }

    /// tempo trascorso sulla domanda corrente (fermo dopo la conferma in allenamento)
    fn update_elapsed(&mut self) {
        let Some(q) = &self.quiz else { return };
        let i = q.idx;
        let running = q.is_exam() || !q.checked[i];
        let secs = q.secs[i] + if running { Instant::now().duration_since(q.shown_at).as_secs_f64() } else { 0.0 };
        let pace = exam_pace(self.content.syllabus().exam.minutes, self.content.syllabus().exam.questions);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_q_elapsed(ss(mmss(secs)));
        st.set_q_slow(secs > pace);
    }

    fn render_question(&mut self) {
        let Some(q) = &self.quiz else { return };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let i = q.idx;
        let question = &q.questions[i];
        let exam = q.is_exam();
        let answered = q.checked[i] && !exam;
        st.set_quiz_mode(match q.kind {
            SessionKind::Exam => 1,
            SessionKind::Review => 2,
            _ => 0,
        });
        st.set_quiz_title(ss(&q.title));
        st.set_q_index(i as i32);
        st.set_q_total(q.questions.len() as i32);
        st.set_q_text(tx(&question.text));
        st.set_q_topic(ss(self.where_of(&question.topic)));
        st.set_q_is_mcq(question.kind == QKind::Mcq);
        let correct = is_correct(question, &q.given[i]);
        let perm = &q.perm[i];
        // dopo la risposta la spiegazione va dentro i riquadri: perché è giusta / perché no
        let (general, notes) = prep_core::model::option_notes(&question.explanation, &question.options);
        let general = prep_core::model::remap_letters(&general, perm);
        let trivial = general.starts_with("Risposta corretta: «");
        let inline = answered && question.kind == QKind::Mcq;
        let opts: Vec<OptionVM> = perm
            .iter()
            .enumerate()
            .filter_map(|(d, &k)| question.options.get(k).map(|o| (d, k, o)))
            .map(|(d, k, o)| {
                let selected = q.given[i] == Given::Option(k);
                let state = if answered {
                    if question.answer == Some(k) {
                        2
                    } else if selected {
                        3
                    } else {
                        4
                    }
                } else if selected {
                    1
                } else {
                    0
                };
                let note = if !inline {
                    String::new()
                } else if question.answer == Some(k) {
                    if trivial {
                        String::new()
                    } else {
                        general.clone()
                    }
                } else {
                    prep_core::model::remap_letters(notes.get(k).map(|x| x.as_str()).unwrap_or(""), perm)
                };
                OptionVM { letter: ss(prep_core::model::letter(d).to_string()), text: tx(o), state, note: tx(&note) }
            })
            .collect();
        st.set_q_options(model(opts));
        st.set_q_fill(ss(match &q.given[i] {
            Given::Text(t) => t.clone(),
            _ => String::new(),
        }));
        st.set_q_unsure(q.unsure[i]);
        st.set_q_answered(answered);
        st.set_q_correct(correct);
        let correct_text = match (question.kind, question.answer) {
            (QKind::Mcq, Some(a)) => match perm.iter().position(|&k| k == a) {
                Some(d) => format!(
                    "{}) {}",
                    prep_core::model::letter(d),
                    question.options.get(a).map(|x| x.as_str()).unwrap_or("")
                ),
                None => question.correct_text(),
            },
            _ => question.correct_text(),
        };
        st.set_q_correct_text(tx(&correct_text));
        st.set_q_explanation(tx(&if inline {
            String::new()
        } else {
            prep_core::model::remap_explanation(&question.explanation, perm, &question.options)
        }));
        st.set_q_source(ss(&question.source));
        st.set_q_ref(ss(self
            .content
            .resolve_ref(question.book_ref.as_ref(), &question.topic)
            .map(|r| self.content.ref_label(&r))
            .unwrap_or_default()));
        st.set_q_can_confirm(!q.given[i].is_none());
        let answered_count = q.given.iter().filter(|g| !g.is_none()).count();
        st.set_answered_count(answered_count as i32);
        if exam {
            let cells: Vec<NavCellVM> = (0..q.questions.len())
                .map(|k| NavCellVM {
                    n: k as i32 + 1,
                    answered: !q.given[k].is_none(),
                    unsure: q.unsure[k],
                    current: k == i,
                })
                .collect();
            let rows: Vec<NavRowVM> = cells.chunks(8).map(|c| NavRowVM { cells: model(c.to_vec()) }).collect();
            st.set_nav_rows(model(rows));
        }
        self.update_elapsed();
        self.autosave_quiz();
        self.last_save = Instant::now();
    }

    fn select_option(&mut self, k: i32) {
        let Some(q) = &mut self.quiz else { return };
        let i = q.idx;
        if q.checked[i] && !q.is_exam() {
            return;
        }
        // posizione mostrata → indice originale (lo storico usa sempre l'ordine originale)
        let k = q.perm.get(i).and_then(|p| p.get(k as usize)).copied().unwrap_or(k as usize);
        q.given[i] = if q.given[i] == Given::Option(k) && q.is_exam() { Given::None } else { Given::Option(k) };
        self.render_question();
    }

    fn fill_edited(&mut self, text: String) {
        let Some(q) = &mut self.quiz else { return };
        let i = q.idx;
        if q.checked[i] && !q.is_exam() {
            return;
        }
        q.given[i] = Given::Text(text);
        let can = !q.given[i].is_none();
        let count = q.given.iter().filter(|g| !g.is_none()).count() as i32;
        let exam = q.is_exam();
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_q_can_confirm(can);
        st.set_answered_count(count);
        drop(st);
        self.autosave_quiz();
        if exam {
            // aggiorna solo la griglia
            self.render_question();
        }
    }

    fn toggle_unsure(&mut self) {
        let Some(q) = &mut self.quiz else { return };
        let i = q.idx;
        q.unsure[i] = !q.unsure[i];
        self.render_question();
    }

    fn confirm(&mut self) {
        let Some(q) = &mut self.quiz else { return };
        let i = q.idx;
        if q.given[i].is_none() || q.is_exam() {
            return;
        }
        q.account_time();
        q.checked[i] = true;
        self.render_question();
    }

    fn goto(&mut self, k: usize) {
        let Some(q) = &mut self.quiz else { return };
        if k >= q.questions.len() {
            return;
        }
        q.account_time();
        q.idx = k;
        self.render_question();
    }

    fn next(&mut self) {
        let Some(q) = &self.quiz else { return };
        let (i, n, exam) = (q.idx, q.questions.len(), q.is_exam());
        if i + 1 < n {
            self.goto(i + 1);
        } else if !exam {
            self.finish_quiz();
        }
    }

    /// Salta la domanda: passa alla successiva lasciandola senza risposta (non viene
    /// registrata e non torna in coda). Sull'ultima domanda chiude l'allenamento, quindi si può
    /// concludere senza rispondere a tutto.
    fn skip(&mut self) {
        let Some(q) = &mut self.quiz else { return };
        if q.is_exam() {
            return;
        }
        if !q.checked[q.idx] {
            q.account_time();
            q.given[q.idx] = Given::None;
        }
        self.next();
    }

    fn prev(&mut self) {
        let Some(q) = &self.quiz else { return };
        if q.idx > 0 {
            let k = q.idx - 1;
            self.goto(k);
        }
    }

    fn request_finish(&mut self) {
        let Some(q) = &self.quiz else { return };
        let omitted = q.given.iter().filter(|g| g.is_none()).count();
        let unsure = q.unsure.iter().filter(|u| **u).count();
        let text = format!(
            "{} risposte date, {} senza risposta (valgono 0), {} segnate insicure.",
            q.questions.len() - omitted,
            omitted,
            unsure
        );
        self.open_dialog(Dialog::SubmitExam, "Consegnare la prova?", &text, "Consegna");
    }

    fn finish_quiz(&mut self) {
        self.tick.stop();
        self.clear_saved_quiz();
        let Some(mut q) = self.quiz.take() else {
            return;
        };
        q.account_time();
        self.result_perm = q.questions.iter().map(|x| x.id.clone()).zip(q.perm.iter().cloned()).collect();
        let rules = self.content.syllabus().exam.clone();
        let exam = q.is_exam();
        let mut items = Vec::new();
        for (k, question) in q.questions.iter().enumerate() {
            if !exam && !q.checked[k] {
                continue;
            }
            let given = q.given[k].to_record();
            items.push(Item {
                id: question.id.clone(),
                topic: question.topic.clone(),
                kind: "q".into(),
                correct: Some(given.is_some() && is_correct(question, &q.given[k])),
                given,
                unsure: q.unsure[k],
                grade: None,
                flag: None,
                secs: q.secs[k].round() as u32,
                note: None,
            });
        }
        if items.is_empty() {
            self.set_page(if q.unit.is_some() { P_UNIT } else { tab_page(self.ui().global::<AppState>().get_tab()) });
            return;
        }
        let mut score = score_items(&items, rules.correct, rules.wrong, rules.omitted);
        if exam {
            score.passed = Some(score.points >= rules.pass);
        }
        let now = Utc::now();
        let session = Session {
            schema: 1,
            id: Session::new_id(q.kind, q.subject.as_deref(), q.started),
            kind: q.kind,
            subject: q.subject.clone(),
            unit: q.unit.clone(),
            started_at: q.started,
            ended_at: now,
            app_version: APP_VERSION.into(),
            device: device_name(),
            items,
            score: Some(score.clone()),
        };
        let sname = q.subject.as_deref().and_then(|s| self.content.subject(s)).map(|s| s.name.clone());
        let msg = session.commit_message(sname.as_deref());
        if let Err(e) = self.store.record_session(&session, &msg) {
            self.toast(format!("Errore salvataggio: {e}"));
        }
        self.reload_progress();
        self.refresh_all();
        self.show_results(&session, &q);
        self.start_sync(false);
    }

    fn show_results(&mut self, s: &Session, q: &Quiz) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let sc = s.score.clone().unwrap_or_default();
        let exam = s.kind == SessionKind::Exam;
        st.set_r_title(ss(if exam { format!("Simulazione · {}", q.title) } else { q.title.clone() }));
        st.set_r_is_exam(exam);
        let timed: Vec<u32> = s.items.iter().map(|i| i.secs).filter(|x| *x > 0).collect();
        let avg = if timed.is_empty() { 0.0 } else { timed.iter().sum::<u32>() as f64 / timed.len() as f64 };
        let pace = exam_pace(self.content.syllabus().exam.minutes, self.content.syllabus().exam.questions);
        let time_line = if timed.is_empty() {
            String::new()
        } else {
            format!(
                "\nTempo medio {} a domanda (ritmo d'esame {}) · {} oltre il ritmo",
                mmss(avg),
                mmss(pace),
                timed.iter().filter(|x| **x as f64 > pace).count()
            )
        };
        st.set_r_passed(sc.passed.unwrap_or(false));
        if exam {
            let voto =
                if sc.points >= sc.max && sc.max > 0.0 { "30 e lode".to_string() } else { it1(sc.points.min(30.0)) };
            st.set_r_points(ss(format!("{} / 31", it1(sc.points))));
            st.set_r_summary(ss(format!(
                "Voto: {voto} · {} corrette · {} errate · {} omesse · {} insicure{time_line}",
                sc.correct, sc.wrong, sc.omitted, sc.unsure
            )));
        } else {
            st.set_r_points(ss(format!("{}/{}", sc.correct, sc.correct + sc.wrong + sc.omitted)));
            st.set_r_summary(ss(format!("{} errate · {} insicure{time_line}", sc.wrong, sc.unsure)));
        }
        st.set_r_rating(0);
        st.set_r_feedback(ss(""));
        st.set_r_feedback_sent(false);
        self.results = Some(s.clone());
        self.render_results();
        self.set_page(P_RESULTS);
    }

    /// "C) testo" con la lettera mostrata nel quiz (le opzioni erano mescolate) per una
    /// risposta registrata con la lettera originale.
    fn shown_answer(&self, q: Option<&Question>, given: Option<&str>) -> String {
        let Some(g) = given else { return String::new() };
        let Some(q) = q.filter(|q| q.kind == QKind::Mcq) else {
            return g.to_string();
        };
        let Some(orig) = g.chars().next().filter(|c| ('A'..='E').contains(c)).map(|c| (c as u8 - b'A') as usize) else {
            return g.to_string();
        };
        let shown = self.result_perm.get(&q.id).and_then(|p| p.iter().position(|&k| k == orig)).unwrap_or(orig);
        match q.options.get(orig) {
            Some(t) => format!("{}) {}", prep_core::model::letter(shown), t),
            None => g.to_string(),
        }
    }

    fn render_results(&mut self) {
        let Some(s) = &self.results else { return };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let pace = exam_pace(self.content.syllabus().exam.minutes, self.content.syllabus().exam.questions);
        let items: Vec<ResultItemVM> = s
            .items
            .iter()
            .enumerate()
            .map(|(k, it)| {
                let q = self.content.question(&it.id);
                ResultItemVM {
                    n: k as i32 + 1,
                    text: tx(&q.map(|q| q.text.clone()).unwrap_or(it.id.clone())),
                    status: match (&it.given, it.correct) {
                        (None, _) => 2,
                        (_, Some(true)) => 0,
                        _ => 1,
                    },
                    unsure: self.history.flagged.contains(&it.id),
                    given: tx(&self.shown_answer(q, it.given.as_deref())),
                    correct_text: tx(&q
                        .map(|q| match q.answer {
                            Some(a) if q.kind == QKind::Mcq => {
                                self.shown_answer(Some(q), Some(&prep_core::model::letter(a).to_string()))
                            }
                            _ => q.correct_text(),
                        })
                        .unwrap_or_default()),
                    explanation: tx(&q
                        .map(|q| match self.result_perm.get(&q.id) {
                            Some(p) => prep_core::model::remap_explanation(&q.explanation, p, &q.options),
                            None => q.explanation.clone(),
                        })
                        .unwrap_or_default()),
                    ref_label: ss(q
                        .and_then(|q| self.content.resolve_ref(q.book_ref.as_ref(), &q.topic))
                        .map(|r| self.content.ref_label(&r))
                        .unwrap_or_default()),
                    secs: ss(if it.secs > 0 { mmss(it.secs as f64) } else { String::new() }),
                    slow: it.secs as f64 > pace,
                }
            })
            .collect();
        st.set_r_items(model(items));
        st.set_r_sync(ss(sync::status_line(&self.store)));
    }

    fn result_toggle_flag(&mut self, k: usize) {
        let Some(id) = self.results.as_ref().and_then(|s| s.items.get(k)).map(|i| i.id.clone()) else {
            return;
        };
        self.toggle_flag(&id, false);
        self.render_results();
    }

    fn results_close(&mut self) {
        {
            let ui = self.ui();
            let st = ui.global::<AppState>();
            if !st.get_r_feedback_sent() && (st.get_r_rating() > 0 || !st.get_r_feedback().trim().is_empty()) {
                self.send_session_feedback();
            }
        }
        let unit = self.results.as_ref().and_then(|s| s.unit.clone());
        self.results = None;
        self.flush_flags();
        if let Some(u) = unit {
            self.unit_id = u;
            self.refresh_unit();
            self.set_page(P_UNIT);
        } else {
            let tab = self.ui().global::<AppState>().get_tab();
            self.set_page(tab_page(tab));
        }
    }

    // ---------- segnalazioni ----------

    fn toggle_flag(&mut self, id: &str, debounce: bool) {
        let now_on = !self.history.flagged.contains(id);
        self.pending_flags.insert(id.to_string(), now_on);
        if now_on {
            self.history.flagged.insert(id.to_string());
        } else {
            self.history.flagged.remove(id);
        }
        self.progress = compute(&self.content, &self.history, self.assessment.as_ref());
        self.refresh_flags();
        self.refresh_home();
        if debounce {
            self.flag_timer.start(TimerMode::SingleShot, Duration::from_secs(6), || {
                with_ctrl(|c| c.flush_flags());
            });
        }
    }

    fn flush_flags(&mut self) {
        self.flag_timer.stop();
        if self.pending_flags.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_flags);
        // ricostruisci lo stato "vero" dalle sessioni e registra solo i cambi effettivi
        let base = build_history(&self.sessions);
        let items: Vec<Item> = pending
            .into_iter()
            .filter(|(id, on)| base.flagged.contains(id) != *on)
            .map(|(id, on)| {
                let topic = self
                    .content
                    .question(&id)
                    .map(|q| q.topic.clone())
                    .or_else(|| self.content.card(&id).map(|c| c.topic.clone()))
                    .unwrap_or_default();
                Item {
                    id,
                    topic,
                    kind: "flag".into(),
                    given: None,
                    correct: None,
                    unsure: on,
                    grade: None,
                    flag: Some(on),
                    secs: 0,
                    note: None,
                }
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let now = Utc::now();
        let s = Session {
            schema: 1,
            id: Session::new_id(SessionKind::Flags, None, now),
            kind: SessionKind::Flags,
            subject: None,
            unit: None,
            started_at: now,
            ended_at: now,
            app_version: APP_VERSION.into(),
            device: device_name(),
            items,
            score: None,
        };
        let msg = s.commit_message(None);
        if let Err(e) = self.store.record_session(&s, &msg) {
            self.toast(format!("Errore salvataggio: {e}"));
        }
        self.reload_progress();
        self.refresh_all();
        self.start_sync(false);
    }

    // ---------- flashcard ----------

    fn start_cards(&mut self, kind: &str, id: &str) {
        // «learned-…»: le carte già studiate e non in scadenza (ripasso libero)
        let (learned, kind) = match kind.strip_prefix("learned-") {
            Some(k) => (true, k),
            None => (false, kind),
        };
        let scope = Self::scope_of(kind, id);
        let ids = if kind == "flagged" {
            let mut v = self.flagged_cards();
            v.truncate(CARDS_MAX);
            v
        } else if learned {
            select::learned_cards(&self.content, &self.history, &scope, Utc::now(), CARDS_MAX)
        } else {
            select::flashcards(&self.content, &self.history, &scope, Utc::now(), CARDS_MAX, CARDS_NEW)
        };
        if ids.is_empty() {
            self.toast(if learned {
                "Nessuna flashcard già studiata qui"
            } else {
                "Nessuna flashcard da ripassare ora"
            });
            return;
        }
        let subject = match &scope {
            Scope::Subject(s) => Some(s.clone()),
            Scope::Unit(u) => self
                .content
                .unit(u)
                .and_then(|u| u.topics.first())
                .and_then(|t| self.content.subject_of_topic(&t.id))
                .map(|s| s.id.clone()),
            _ => None,
        };
        self.cards = Some(CardRun {
            subject,
            queue: ids,
            requeued: BTreeMap::new(),
            idx: 0,
            flipped: false,
            unsure: false,
            items: vec![],
            started: Utc::now(),
            shown_at: Instant::now(),
            saved: false,
        });
        self.render_card();
        self.set_page(P_CARDS);
    }

    fn render_card(&mut self) {
        let Some(r) = &self.cards else { return };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let done = r.idx >= r.queue.len();
        st.set_c_done(done);
        st.set_c_total(r.queue.len() as i32);
        st.set_c_index(r.idx.min(r.queue.len()) as i32);
        if done {
            let unsure = r.items.iter().filter(|i| i.unsure).count();
            let good = r.items.iter().filter(|i| i.grade.unwrap_or(0) >= 2).count();
            st.set_c_summary(ss(format!(
                "Ripassate {} carte\n{} sapute · {} insicure\nRisultati salvati e inviati al repository.",
                r.items.len(),
                good,
                unsure
            )));
            return;
        }
        let Some(card) = self.content.card(&r.queue[r.idx]) else {
            return;
        };
        st.set_c_front(tx(&card.front));
        st.set_c_back(tx(&card.back));
        st.set_c_topic(ss(self.content.topic(&card.topic).map(|t| t.name.clone()).unwrap_or_default()));
        st.set_c_ref(ss(self
            .content
            .resolve_ref(card.book_ref.as_ref(), &card.topic)
            .map(|x| self.content.ref_label(&x))
            .unwrap_or_default()));
        st.set_c_flipped(r.flipped);
        st.set_c_unsure(r.unsure);
    }

    fn card_flip(&mut self) {
        if let Some(r) = &mut self.cards {
            r.flipped = !r.flipped;
            let ui = self.ui.upgrade().expect("finestra");
            let st = ui.global::<AppState>();
            st.set_c_note_shown(if r.flipped { st.get_c_note() } else { ss("") });
        }
        self.render_card();
    }

    fn card_toggle_unsure(&mut self) {
        if let Some(r) = &mut self.cards {
            r.unsure = !r.unsure;
        }
        self.render_card();
    }

    fn card_grade(&mut self, g: i32) {
        let Some(r) = &mut self.cards else { return };
        if r.idx >= r.queue.len() {
            return;
        }
        let id = r.queue[r.idx].clone();
        let topic = self.content.card(&id).map(|c| c.topic.clone()).unwrap_or_default();
        let secs = Instant::now().duration_since(r.shown_at).as_secs() as u32;
        r.items.push(Item {
            id: id.clone(),
            topic,
            kind: "card".into(),
            given: None,
            correct: None,
            unsure: r.unsure,
            grade: Some(g.clamp(0, 3) as u8),
            flag: None,
            secs,
            note: {
                let ui = self.ui.upgrade().expect("finestra");
                let st = ui.global::<AppState>();
                let n = st.get_c_note().trim().to_string();
                st.set_c_note(ss(""));
                st.set_c_note_shown(ss(""));
                (!n.is_empty()).then_some(n)
            },
        });
        if g == 0 {
            let n = r.requeued.entry(id.clone()).or_insert(0);
            if *n < 2 {
                *n += 1;
                r.queue.push(id);
            }
        }
        r.idx += 1;
        r.flipped = false;
        r.unsure = false;
        r.shown_at = Instant::now();
        let done = r.idx >= r.queue.len();
        if done {
            self.save_cards();
        }
        self.render_card();
    }

    fn save_cards(&mut self) {
        let Some(r) = &mut self.cards else { return };
        if r.saved || r.items.is_empty() {
            return;
        }
        r.saved = true;
        let s = Session {
            schema: 1,
            id: Session::new_id(SessionKind::Flashcards, r.subject.as_deref(), r.started),
            kind: SessionKind::Flashcards,
            subject: r.subject.clone(),
            unit: None,
            started_at: r.started,
            ended_at: Utc::now(),
            app_version: APP_VERSION.into(),
            device: device_name(),
            items: r.items.clone(),
            score: None,
        };
        let sname = r.subject.as_deref().and_then(|x| self.content.subject(x)).map(|x| x.name.clone());
        let msg = s.commit_message(sname.as_deref());
        if let Err(e) = self.store.record_session(&s, &msg) {
            self.toast(format!("Errore salvataggio: {e}"));
        }
        self.reload_progress();
        self.refresh_all();
        self.start_sync(false);
    }

    fn cards_close(&mut self) {
        self.save_cards();
        self.cards = None;
        let tab = self.ui().global::<AppState>().get_tab();
        self.set_page(tab_page(tab));
    }

    // ---------- feedback ----------

    fn session_context(&self, s: &Session) -> String {
        let sname = s.subject.as_deref().and_then(|x| self.content.subject(x)).map(|x| x.name.clone());
        let mut c = s.commit_message(sname.as_deref());
        if let Some(u) = s.unit.as_deref().and_then(|u| self.content.unit(u)) {
            c.push_str(&format!(" · U{} {}", u.n, u.name));
        }
        c
    }

    fn save_feedback(&mut self, fb: Feedback) {
        match self.store.record_feedback(&fb) {
            Ok(()) => {
                self.refresh_feedback();
                self.start_sync(false);
            }
            Err(e) => self.toast(format!("Errore salvataggio feedback: {e}")),
        }
    }

    fn send_session_feedback(&mut self) {
        let Some(s) = self.results.clone() else {
            return;
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let rating = st.get_r_rating();
        let text = st.get_r_feedback().trim().to_string();
        if rating == 0 && text.is_empty() {
            return;
        }
        let now = Utc::now();
        let fb = Feedback {
            schema: 1,
            id: Feedback::new_id(now),
            created_at: now,
            kind: "session".into(),
            category: match s.kind {
                SessionKind::Exam => "simulazione".into(),
                SessionKind::Review => "ripasso".into(),
                _ => "quiz".into(),
            },
            session: Some(s.id.clone()),
            context: self.session_context(&s),
            item: None,
            rating: (rating > 0).then_some(rating.clamp(1, 5) as u8),
            text,
            app_version: APP_VERSION.into(),
        };
        st.set_r_feedback_sent(true);
        self.save_feedback(fb);
        self.toast("Grazie! Feedback salvato e inviato al repository");
    }

    fn send_feedback(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let text = st.get_fb_text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let now = Utc::now();
        let item = self.fb_item.clone();
        let fb = Feedback {
            schema: 1,
            id: Feedback::new_id(now),
            created_at: now,
            kind: if item.is_some() { "question".into() } else { "general".into() },
            category: st.get_fb_category().to_string(),
            session: self.results.as_ref().map(|s| s.id.clone()).filter(|_| item.is_some()),
            context: st.get_fb_item_text().to_string(),
            item,
            rating: None,
            text,
            app_version: APP_VERSION.into(),
        };
        st.set_fb_text(ss(""));
        self.fb_item = None;
        st.set_fb_item(ss(""));
        st.set_fb_item_text(ss(""));
        self.save_feedback(fb);
        self.toast("Feedback inviato: lo leggerà Claude alla prossima revisione");
    }

    fn report_result(&mut self, k: usize) {
        let Some(id) = self.results.as_ref().and_then(|s| s.items.get(k)).map(|i| i.id.clone()) else {
            return;
        };
        let label = match self.content.question(&id) {
            Some(q) => format!("{id} — {}", q.text.chars().take(90).collect::<String>()),
            None => id.clone(),
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_fb_category(ss("domanda"));
        st.set_fb_item(ss(&id));
        st.set_fb_item_text(ss(label));
        self.fb_item = Some(id);
        // la pagina risultati resta: il tasto indietro ci ritorna
        st.set_page(P_FEEDBACK);
    }

    fn refresh_feedback(&mut self) {
        let status = self.store.feedback_status();
        let list: Vec<FeedbackVM> = self
            .store
            .feedbacks()
            .into_iter()
            .take(50)
            .map(|f| {
                let pending = self.store.is_pending(&f.repo_path());
                let st = status.items.get(&f.id);
                let title = match f.kind.as_str() {
                    "session" => format!("Fine {}", f.category),
                    "question" => format!("Domanda {}", f.item.clone().unwrap_or_default()),
                    _ => f.category.clone(),
                };
                FeedbackVM {
                    date: ss(f.created_at.with_timezone(&Local).format("%d/%m %H:%M").to_string()),
                    title: ss(if f.context.is_empty() || f.kind == "question" {
                        title
                    } else {
                        format!("{title} · {}", f.context)
                    }),
                    text: ss(&f.text),
                    rating: f.rating.unwrap_or(0) as i32,
                    status: ss(match (pending, st) {
                        (true, _) => "in coda".to_string(),
                        (false, Some(s)) => s.status.clone(),
                        (false, None) => "inviato".to_string(),
                    }),
                    reply: ss(st.map(|s| s.reply.clone()).unwrap_or_default()),
                    pending,
                }
            })
            .collect();
        self.ui().global::<AppState>().set_feedbacks(model(list));
    }

    // ---------- libri ----------

    fn read_book_bytes(&self, path: &str) -> Option<Vec<u8>> {
        if let Some(b) = self.store.read_mirror(path) {
            return Some(b);
        }
        if cfg!(all(not(target_os = "android"), not(target_arch = "wasm32"))) {
            // sviluppo desktop: legge dal checkout del repository
            if let Ok(b) = vfs::read(std::path::Path::new(path)) {
                return Some(b);
            }
        }
        // copia inclusa nell'app (versione offline)
        crate::bundled::BUNDLED_BOOKS.iter().find(|(p, _)| *p == path).map(|(_, b)| b.to_vec())
    }

    fn read_book_file(&self, path: &str) -> Option<String> {
        String::from_utf8(self.read_book_bytes(path)?).ok()
    }

    fn build_reader(&self, book_id: &str) -> Option<Reader> {
        let book = self.content.book(book_id)?;
        let mut entries = Vec::new();
        for ch in &book.chapters {
            let Some(text) = self.read_book_file(&ch.file) else {
                continue;
            };
            let blocks = Rc::new(markdown::parse(&text));
            for sec in markdown::sections(&blocks) {
                entries.push(ReaderEntry {
                    chapter: ch.title.clone(),
                    blocks: blocks.clone(),
                    section: sec,
                    file: ch.file.clone(),
                });
            }
        }
        if entries.is_empty() {
            return None;
        }
        let return_page = self.ui().global::<AppState>().get_page();
        Some(Reader { title: book.title.clone(), entries, pos: 0, return_page })
    }

    fn open_ref(&mut self, r: Option<BookRef>) {
        let Some(r) = r else { return };
        let Some(mut reader) = self.build_reader(&r.book) else {
            self.toast("Libro non disponibile");
            return;
        };
        let in_file: Vec<usize> =
            reader.entries.iter().enumerate().filter(|(_, e)| e.file == r.file).map(|(i, _)| i).collect();
        if let Some(&first) = in_file.first() {
            let secs: Vec<Section> = in_file.iter().map(|&i| reader.entries[i].section.clone()).collect();
            reader.pos = markdown::find_section(&secs, &r.section, r.page).map(|k| in_file[k]).unwrap_or(first);
        }
        self.reader = Some(reader);
        self.ui().global::<AppState>().set_rd_toc_visible(false);
        self.render_reader();
        self.set_page(P_READER);
    }

    // ---------- tavola periodica ----------

    fn open_periodic(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        if st.get_pt_elements().row_count() == 0 {
            let els = periodic::load();
            st.set_pt_elements(model(els.iter().map(periodic::cell).collect()));
            self.elements = els;
        }
        self.set_page(P_PERIODIC);
    }

    fn pt_select(&mut self, z: i32) {
        let Some(e) = self.elements.iter().find(|e| e.z == z) else {
            return;
        };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_pt_detail(periodic::detail(e));
        st.set_pt_has_sel(true);
    }

    fn open_book(&mut self, id: &str) {
        match self.build_reader(id) {
            Some(r) => {
                self.reader = Some(r);
                self.ui().global::<AppState>().set_rd_toc_visible(true);
                self.render_reader();
                self.set_page(P_READER);
            }
            None => self.toast("Libro non disponibile"),
        }
    }

    fn render_reader(&mut self) {
        let Some(r) = &self.reader else { return };
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let e = &r.entries[r.pos];
        let title = if e.section.title == "Inizio" { e.chapter.clone() } else { e.section.title.clone() };
        st.set_rd_book(ss(&r.title));
        st.set_rd_title(ss(title));
        st.set_rd_page_label(ss(e.section.page.map(|p| format!("p. {p}")).unwrap_or_default()));
        let mut blocks: Vec<BlockVM> = Vec::new();
        for b in &e.blocks[e.section.start..e.section.end] {
            if let Block::Table(rows) = b {
                for (k, row) in rows.iter().enumerate() {
                    blocks.push(BlockVM {
                        kind: 9,
                        text: ss(""),
                        level: if k == 0 { 1 } else { 0 },
                        label: ss(""),
                        cells: model(row.iter().map(ss).collect()),
                        img: Default::default(),
                    });
                }
                continue;
            }
            if let Block::Figure { alt, src } = b {
                // figura del libro: SVG a colori su fondo chiaro (si legge anche col tema scuro)
                let img = self.read_book_bytes(src).and_then(|svg| slint::Image::load_from_svg_data(&svg).ok());
                blocks.push(BlockVM {
                    kind: if img.is_some() { 12 } else { 7 },
                    text: ss(alt),
                    level: 0,
                    label: ss(""),
                    cells: model(vec![]),
                    img: img.unwrap_or_default(),
                });
                continue;
            }
            if let Block::Math(tex) = b {
                let img = self
                    .read_book_bytes(&latex::math_path(tex))
                    .and_then(|svg| slint::Image::load_from_svg_data(&svg).ok());
                blocks.push(BlockVM {
                    kind: if img.is_some() { 10 } else { 11 },
                    text: ss(latex::convert(tex)),
                    level: 0,
                    label: ss(""),
                    cells: model(vec![]),
                    img: img.unwrap_or_default(),
                });
                continue;
            }
            blocks.push({
                let (kind, text, level, label) = match b {
                    Block::Heading { level, text } => (0, text.clone(), *level as i32, String::new()),
                    Block::Para(t) => (1, t.clone(), 0, String::new()),
                    Block::Bullet { depth, text } => (2, text.clone(), *depth as i32, String::new()),
                    Block::Numbered { label, text } => (3, text.clone(), 0, label.clone()),
                    Block::Quote(t) => (4, t.clone(), 0, String::new()),
                    Block::Mono(t) => (5, t.clone(), 0, String::new()),
                    Block::Page(p) => (6, format!("p. {p}"), 0, String::new()),
                    Block::Image(a) => (7, a.clone(), 0, String::new()),
                    Block::Rule => (8, String::new(), 0, String::new()),
                    Block::Table(_) | Block::Math(_) | Block::Figure { .. } => unreachable!(),
                };
                BlockVM { kind, text: ss(text), level, label: ss(label), cells: model(vec![]), img: Default::default() }
            });
        }
        st.set_rd_blocks(model(blocks));
        st.set_rd_has_prev(r.pos > 0);
        st.set_rd_has_next(r.pos + 1 < r.entries.len());
        let toc: Vec<TocVM> = r
            .entries
            .iter()
            .enumerate()
            .map(|(i, x)| TocVM {
                idx: i as i32,
                title: ss(if x.section.title == "Inizio" { x.chapter.clone() } else { x.section.title.clone() }),
                level: x.section.level as i32,
                page: ss(x.section.page.map(|p| format!("p. {p}")).unwrap_or_default()),
                current: i == r.pos,
            })
            .collect();
        st.set_rd_toc(model(toc));
        st.set_rd_scroll(0.0);
    }

    fn reader_goto(&mut self, k: usize) {
        if let Some(r) = &mut self.reader {
            if k < r.entries.len() {
                r.pos = k;
            }
        }
        self.ui().global::<AppState>().set_rd_toc_visible(false);
        self.render_reader();
    }

    fn current_question_ref(&self) -> Option<BookRef> {
        let q = self.quiz.as_ref()?;
        let question = &q.questions[q.idx];
        self.content.resolve_ref(question.book_ref.as_ref(), &question.topic)
    }

    // ---------- sync ----------

    fn start_sync(&mut self, verbose: bool) {
        self.p2p_announce();
        self.sync_verbose |= verbose;
        if self.syncing {
            return;
        }
        if self.store.settings().token.trim().is_empty() {
            if verbose {
                self.toast("Imposta il token GitHub in Impostazioni per committare i risultati");
            }
            self.refresh_settings_info();
            return;
        }
        self.syncing = true;
        self.ui().global::<AppState>().set_syncing(true);
        let store = self.store.clone();
        self.ui().global::<AppState>().set_sync_status(ss("Sincronizzazione in corso…"));
        spawn_background(move || {
            let rep = sync::sync_with_progress(&store, &|n, tot| {
                if tot > 0 && (n == 0 || n % 10 == 0 || n == tot) {
                    let msg = format!("Scarico i contenuti: {n}/{tot} file…");
                    let _ = slint::invoke_from_event_loop(move || {
                        with_ctrl(|c| c.ui().global::<AppState>().set_sync_status(ss(&msg)))
                    });
                }
            });
            let _ = slint::invoke_from_event_loop(move || with_ctrl(|c| c.on_sync_done(rep)));
        });
    }

    fn on_sync_done(&mut self, rep: SyncReport) {
        self.syncing = false;
        self.ui().global::<AppState>().set_syncing(false);
        if rep.downloaded > 0 || rep.removed > 0 {
            self.reload_content();
        }
        self.reload_progress();
        self.refresh_all();
        if self.results.is_some() {
            self.render_results();
        }
        if rep.error.is_some() || self.sync_verbose || rep.downloaded > 0 {
            self.toast(rep.summary());
        }
        self.sync_verbose = false;
    }

    // ---------- pacchetto contenuti ----------

    fn pack_status(&self) -> String {
        let n = self.content.questions.len();
        let c = self.content.cards.len();
        match prep_core::pack::last_import(&self.store) {
            Some(i) => format!(
                "In uso: {n} domande, {c} flashcard. Ultimo pacchetto importato: {} del {} (importato il {}).",
                if i.label.is_empty() { "senza nome".to_string() } else { i.label.clone() },
                short_date(&i.created),
                short_date(&i.imported)
            ),
            None if self.from_repo => format!("In uso: {n} domande, {c} flashcard, sincronizzate dal repository."),
            None => format!("In uso: {n} domande, {c} flashcard, incluse nell'app."),
        }
    }

    /// Importa in background il file `path`; `remove` lo cancella dopo (file della cartella import/).
    fn import_pack_file(&mut self, path: PathBuf, remove: bool) {
        if self.importing {
            return;
        }
        self.importing = true;
        self.ui().global::<AppState>().set_pack_busy(true);
        let base = self.store.base.clone();
        spawn_background(move || {
            // il pacchetto va nell'archivio del suo esame (nuovo o già installato)
            let res = vfs::read(&path).map_err(|e| format!("file non leggibile: {e}")).and_then(|b| {
                let (id, name) = prep_core::pack::exam_of(&b)?;
                let store = Store::open_in(&base, prep_core::exams::dir(&base, &id)).map_err(|e| e.to_string())?;
                prep_core::pack::import(&store, &b).map(|info| (info, id, name))
            });
            if remove {
                let _ = vfs::remove_file(&path);
            }
            let _ = slint::invoke_from_event_loop(move || with_ctrl(|c| c.on_pack_imported(res)));
        });
    }

    fn on_pack_imported(&mut self, res: Result<(prep_core::pack::PackInfo, String, String), String>) {
        self.importing = false;
        self.ui().global::<AppState>().set_pack_busy(false);
        match res {
            Ok((info, id, name)) => {
                let is_new = self.exams.get(&id).is_none();
                let was_active = self.exams.active == id;
                self.exams.upsert(&id, &name);
                let _ = self.exams.save(&self.store.base);
                if was_active {
                    self.reload_content();
                    self.reload_progress();
                    self.refresh_all();
                    self.refresh_exams();
                } else {
                    self.open_exam(&id);
                }
                self.refresh_settings_info();
                self.toast(format!(
                    "{} «{name}»: {} domande, {} flashcard",
                    if is_new { "Esame aggiunto" } else { "Contenuti aggiornati" },
                    info.questions,
                    info.flashcards
                ));
            }
            Err(e) => self.toast(format!("Pacchetto non importato: {e}")),
        }
    }

    // ---------- esami installati ----------

    fn refresh_exams(&mut self) {
        let base = self.store.base.clone();
        let v: Vec<ExamVM> = self
            .exams
            .exams
            .iter()
            .map(|e| {
                let st = Store { root: prep_core::exams::dir(&base, &e.id), base: base.clone() };
                let info = match prep_core::pack::last_import(&st) {
                    Some(i) => format!("{} domande · pacchetto del {}", i.questions, short_date(&i.created)),
                    None if e.id == self.exams.active => format!("{} domande", self.content.questions.len()),
                    None => "contenuti inclusi nell'app".into(),
                };
                ExamVM { id: ss(&e.id), name: ss(&e.name), active: e.id == self.exams.active, info: ss(info) }
            })
            .collect();
        let name = self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_else(|| "Nessun esame".into());
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_exams(model(v));
        st.set_exam_name(ss(name));
    }

    /// Passa a un altro esame: archivio, contenuti e progressi sono quelli suoi.
    fn open_exam(&mut self, id: &str) {
        if self.exams.get(id).is_none() {
            return;
        }
        self.flush_flags();
        if self.quiz.is_some() {
            self.pause_quiz();
        }
        self.exams.active = id.to_string();
        let _ = self.exams.save(&self.store.base);
        let base = self.store.base.clone();
        match Store::open_in(&base, prep_core::exams::dir(&base, id)) {
            Ok(st) => self.store = st,
            Err(e) => {
                self.toast(format!("Esame non apribile: {e}"));
                return;
            }
        }
        self.quiz = None;
        self.cards = None;
        self.results = None;
        self.reader = None;
        self.plan = None;
        self.plan_sel.clear();
        self.plan_extra.clear();
        self.unit_id.clear();
        self.fb_item = None;
        self.result_perm.clear();
        self.qz = QuizSetup::default();
        self.reload_content();
        self.reload_progress();
        self.refresh_all();
        self.refresh_exams();
        self.ui().global::<AppState>().set_exam_menu(false);
        self.go_tab(0);
    }

    /// Elimina un esame (contenuti e progressi): serve un secondo tocco entro 5 secondi.
    fn remove_exam(&mut self, id: &str) {
        let armed = matches!(&self.remove_armed, Some((x, t)) if x == id && t.elapsed() < Duration::from_secs(5));
        if !armed {
            self.remove_armed = Some((id.to_string(), Instant::now()));
            let name = self.exams.get(id).map(|e| e.name.clone()).unwrap_or_default();
            self.toast(format!("Tocca di nuovo per eliminare «{name}» con i suoi progressi"));
            return;
        }
        self.remove_armed = None;
        let base = self.store.base.clone();
        let was_active = self.exams.active == id;
        self.exams.remove(id);
        prep_core::exams::remove_files(&base, id);
        if self.exams.exams.is_empty() {
            // resta sempre almeno l'esame incluso nell'app
            let bundled = Content::from_files(crate::bundled::BUNDLED.iter().copied());
            self.exams = prep_core::exams::prepare(&base, bundled.syllabus.as_ref());
            if self.exams.exams.is_empty() {
                if let Some(syl) = bundled.syllabus.as_ref() {
                    self.exams.upsert(&prep_core::exams::exam_id(syl), &syl.title);
                }
            }
        }
        let _ = self.exams.save(&base);
        if was_active || self.exams.get(&self.exams.active).is_none() {
            let next = self.exams.active.clone();
            self.open_exam(&next);
        } else {
            self.refresh_exams();
        }
        self.toast("Esame eliminato");
    }

    // ---------- sincronizzazione tra dispositivi ----------

    fn exam_store(&self, id: &str) -> Store {
        Store { root: prep_core::exams::dir(&self.store.base, id), base: self.store.base.clone() }
    }

    fn device_label() -> &'static str {
        if cfg!(target_os = "android") {
            "Android"
        } else if cfg!(target_arch = "wasm32") {
            "web"
        } else {
            "computer"
        }
    }

    fn sess(&self, k: Sess) -> Option<&P2p> {
        match k {
            Sess::Sync => self.p2p.as_ref(),
            Sess::Share => self.share.as_ref(),
            Sess::Group => self.group.as_ref(),
        }
    }

    fn sess_mut(&mut self, k: Sess) -> Option<&mut P2p> {
        match k {
            Sess::Sync => self.p2p.as_mut(),
            Sess::Share => self.share.as_mut(),
            Sess::Group => self.group.as_mut(),
        }
    }

    /// Relay scelti nelle Opzioni (salvati), o quelli predefiniti.
    fn relays(&mut self) -> Vec<String> {
        let ui = self.ui();
        let mut s = self.store.settings();
        s.relays = ui
            .global::<AppState>()
            .get_p2p_relays()
            .split([',', '\n', ' '])
            .map(|x| x.trim().to_string())
            .filter(|x| x.starts_with("ws://") || x.starts_with("wss://"))
            .collect();
        let _ = self.store.save_settings(&s);
        if s.relays.is_empty() {
            p2p::DEFAULT_RELAYS.iter().map(|x| x.to_string()).collect()
        } else {
            s.relays
        }
    }

    fn device_id(&mut self) -> String {
        let mut s = self.store.settings();
        if s.device_id.is_empty() {
            s.device_id = p2p::random_id();
            let _ = self.store.save_settings(&s);
        }
        s.device_id
    }

    /// Apre una sessione: collegamenti ai relay e motore.
    fn open_session(&mut self, k: Sess, room: p2p::Room, engine: p2p::Engine, code: String, sending: String) {
        self.close_session(k);
        let relays = self.relays();
        let base = k as usize * LINKS_PER_SESS;
        let links: Vec<crate::net::Link> =
            relays.iter().take(LINKS_PER_SESS).enumerate().map(|(i, u)| crate::net::Link::open(base + i, u)).collect();
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, Duration::from_millis(160), move || with_ctrl(|c| c.p2p_pump(k)));
        let sess = P2p {
            room,
            signer: p2p::Signer::new(),
            engine: Some(engine),
            open: vec![false; links.len()],
            subscribed: vec![false; links.len()],
            links,
            relays,
            started: false,
            queue: Default::default(),
            timer,
            ticks: 0,
            dirty: false,
            received: 0,
            notice: String::new(),
            code,
            sending,
            direct: BTreeMap::new(),
            failed: BTreeSet::new(),
            rtc_seq: 0,
        };
        match k {
            Sess::Sync => self.p2p = Some(sess),
            Sess::Share => self.share = Some(sess),
            Sess::Group => self.group = Some(sess),
        }
        self.refresh_sessions();
    }

    fn close_session(&mut self, k: Sess) {
        let old = match k {
            Sess::Sync => self.p2p.take(),
            Sess::Share => self.share.take(),
            Sess::Group => self.group.take(),
        };
        if let Some(p) = old {
            p.timer.stop();
            for l in &p.links {
                l.close();
            }
            for d in p.direct.values() {
                d.conn.close();
            }
        }
        self.refresh_sessions();
    }

    fn p2p_connect(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let password = st.get_p2p_password().to_string();
        let mut s = self.store.settings();
        if s.username.trim().is_empty() {
            self.toast("Scegli prima un nome utente qui sopra, in Profilo");
            return;
        }
        if password.chars().count() < 8 {
            self.toast("Serve una password di almeno 8 caratteri (meglio una frase lunga)");
            return;
        }
        s.p2p_auto = st.get_p2p_auto();
        s.direct_only = st.get_p2p_direct_only();
        s.p2p_password = if s.p2p_auto { password.clone() } else { String::new() };
        let _ = self.store.save_settings(&s);
        let device = self.device_id();
        let room = p2p::Room::derive(&s.username, &password);
        let engine = p2p::Engine::new(
            &device,
            &format!("{} · {}", s.username.trim(), Self::device_label()),
            Self::device_label(),
        );
        self.open_session(Sess::Sync, room, engine, String::new(), String::new());
    }

    fn p2p_disconnect(&mut self) {
        self.close_session(Sess::Sync);
    }

    fn display_name(&self) -> String {
        let n = self.store.settings().username.trim().to_string();
        if n.is_empty() {
            format!("Utente ({})", Self::device_label())
        } else {
            n
        }
    }

    /// Condivide l'esame attivo con un'altra persona: genera il codice da comunicarle.
    fn share_start(&mut self) {
        let ui = self.ui();
        if !ui.global::<AppState>().get_share_ok() {
            self.toast("Conferma prima di avere il diritto di condividere questo materiale");
            return;
        }
        let id = self.exams.active.clone();
        let st = self.exam_store(&id);
        if !st.mirror_path("content/syllabus.json").is_some_and(|p| vfs::exists(&p)) {
            self.toast("Questo esame usa i contenuti inclusi nell'app: non c'è un pacchetto da condividere");
            return;
        }
        let name = self.exams.get(&id).map(|e| e.name.clone()).unwrap_or_default();
        let code = p2p::share_code();
        let engine =
            p2p::Engine::new(&p2p::random_id(), &self.display_name(), Self::device_label()).share_only(vec![id]);
        self.open_session(Sess::Share, p2p::share_room(&code), engine, code, name);
    }

    /// Entra nella condivisione di un'altra persona col codice che ha comunicato.
    fn share_join(&mut self) {
        let ui = self.ui();
        let code = ui.global::<AppState>().get_share_input().to_string();
        if p2p::normalize_code(&code).len() != 12 {
            self.toast("Il codice ha 12 caratteri, per esempio K7QM-4TZP-9WXR");
            return;
        }
        let engine = p2p::Engine::new(&p2p::random_id(), &self.display_name(), Self::device_label()).share_only(vec![]);
        self.open_session(Sess::Share, p2p::share_room(&code), engine, String::new(), String::new());
    }

    fn now_secs() -> i64 {
        Utc::now().timestamp()
    }

    /// Spedisce i messaggi prodotti dal motore: per via diretta se c'è un collegamento con
    /// il destinatario (o sta per aprirsi), altrimenti cifrati e firmati attraverso i relay.
    fn p2p_enqueue(&mut self, k: Sess, outs: Vec<p2p::Out>) {
        let direct_only = self.store.settings().direct_only;
        let Some(p) = self.sess_mut(k) else { return };
        let Some(eng) = p.engine.as_ref() else { return };
        let now = Self::now_secs();
        for o in outs {
            let wire = eng.wire(&o);
            let control = matches!(o.msg, p2p::Msg::Hello { .. } | p2p::Msg::Rtc { .. } | p2p::Msg::Group(_));
            if let (Some(to), false) = (o.to.as_deref(), control) {
                if let Some(d) = p.direct.get_mut(to) {
                    if d.open {
                        d.conn.send(wire);
                    } else {
                        d.pending.push_back(wire);
                    }
                    continue;
                }
                if direct_only {
                    // nessun collegamento diretto con questo dispositivo: niente relay
                    continue;
                }
            } else if o.to.is_none() && direct_only && !control {
                continue;
            }
            let content = p.room.seal(&wire);
            p.queue.push_back(p.signer.event(&p.room.tag, &content, now));
        }
    }

    /// Apre (o riapre) il collegamento diretto con un dispositivo.
    fn direct_open(&mut self, k: Sess, peer: &str, initiator: bool) {
        let Some(p) = self.sess_mut(k) else { return };
        if let Some(old) = p.direct.remove(peer) {
            old.conn.close();
        }
        p.failed.remove(peer);
        let conn = crate::direct::Conn::open(k as usize, peer, initiator);
        p.direct.insert(peer.to_string(), Direct { conn, open: false, pending: Default::default() });
    }

    fn direct_event(&mut self, k: Sess, peer: String, ev: crate::direct::DirectEvent) {
        use crate::direct::DirectEvent;
        match ev {
            DirectEvent::Signal(kind, sdp) => {
                self.with_engine(k, |e, _| vec![e.signal(&peer, &kind, &sdp)]);
            }
            DirectEvent::Open => {
                if let Some(d) = self.sess_mut(k).and_then(|p| p.direct.get_mut(&peer)) {
                    d.open = true;
                    for b in d.pending.drain(..) {
                        d.conn.send(b);
                    }
                }
                self.refresh_sessions();
            }
            DirectEvent::Data(bytes) => {
                let Some(p) = self.sess_mut(k) else { return };
                p.rtc_seq += 1;
                let id = format!("rtc:{}", p.rtc_seq);
                let now = Self::now_secs();
                self.with_engine(k, |e, c| e.receive(c, &id, &bytes, now));
            }
            DirectEvent::Closed(why) => {
                let direct_only = self.store.settings().direct_only;
                let Some(p) = self.sess_mut(k) else { return };
                let Some(d) = p.direct.remove(&peer) else { return };
                if !why.is_empty() {
                    p.failed.insert(peer.clone());
                }
                // i messaggi rimasti in attesa passano dai relay (se permesso)
                if !direct_only {
                    let now = Self::now_secs();
                    for wire in d.pending {
                        let content = p.room.seal(&wire);
                        p.queue.push_back(p.signer.event(&p.room.tag, &content, now));
                    }
                }
                self.refresh_sessions();
            }
        }
    }

    /// Esegue `f` col motore e il controller (che fa da `Host`), poi spedisce quanto prodotto.
    fn with_engine(&mut self, k: Sess, f: impl FnOnce(&mut p2p::Engine, &mut Ctrl) -> Vec<p2p::Out>) {
        let Some(mut eng) = self.sess_mut(k).and_then(|p| p.engine.take()) else { return };
        let outs = f(&mut eng, self);
        let events = std::mem::take(&mut eng.events);
        if let Some(p) = self.sess_mut(k) {
            p.engine = Some(eng);
        }
        self.p2p_enqueue(k, outs);
        self.p2p_events(k, events);
    }

    fn p2p_announce(&mut self) {
        if self.p2p.as_ref().is_some_and(|p| p.started) {
            self.with_engine(Sess::Sync, |e, c| e.announce(c));
        }
    }

    /// Ogni 160 ms: un messaggio della coda a tutti i relay aperti; ogni secondo il motore.
    fn p2p_pump(&mut self, k: Sess) {
        let bt_on = self.bt.connected.is_some();
        let Some(p) = self.sess_mut(k) else { return };
        p.ticks += 1;
        if p.open.iter().any(|o| *o) || bt_on {
            if let Some(f) = p.queue.pop_front() {
                for (l, o) in p.links.iter().zip(&p.open) {
                    if *o {
                        l.send(&f);
                    }
                }
                // il telefono vicino riceve gli stessi messaggi dei relay
                if bt_on {
                    crate::bt_send(f.as_bytes());
                }
            }
        }
        if bt_on && !p.started {
            p.started = true;
            let now = Self::now_secs();
            self.with_engine(k, |e, c| e.start(c, now));
        }
        let Some(p) = self.sess_mut(k) else { return };
        if p.ticks.is_multiple_of(6) {
            let started = p.started;
            let dirty = std::mem::take(&mut p.dirty);
            if started {
                let now = Self::now_secs();
                self.with_engine(k, |e, c| e.tick(c, now));
            }
            if dirty {
                self.reload_progress();
                self.refresh_all();
            }
            self.refresh_sessions();
        }
    }

    fn net_event(&mut self, ev: crate::net::NetEvent) {
        use crate::net::NetEvent;
        let idx = match &ev {
            NetEvent::Open(i) | NetEvent::Frame(i, _) | NetEvent::Closed(i, _) => *i,
        };
        let k = Sess::from_index(idx / LINKS_PER_SESS);
        let i = idx % LINKS_PER_SESS;
        let Some(p) = self.sess_mut(k) else { return };
        match ev {
            NetEvent::Open(_) => {
                if i < p.open.len() {
                    p.open[i] = true;
                    // iscrizione agli eventi della stanza (anche quelli di pochi secondi fa)
                    p.links[i].send(&p2p::subscribe("tp", &p.room.tag, Self::now_secs() - 30));
                    p.subscribed[i] = true;
                }
                if !p.started {
                    p.started = true;
                    let now = Self::now_secs();
                    self.with_engine(k, |e, c| e.start(c, now));
                }
                self.refresh_sessions();
            }
            NetEvent::Closed(_, why) => {
                if i < p.open.len() {
                    p.open[i] = false;
                }
                if !why.is_empty() {
                    p.notice = format!("{}: {why}", p.relays.get(i).cloned().unwrap_or_default());
                }
                self.refresh_sessions();
            }
            NetEvent::Frame(_, text) => {
                if let Some((id, content)) = p2p::parse_relay(&text) {
                    if let Some(plain) = p.room.open(&content) {
                        let now = Self::now_secs();
                        self.with_engine(k, |e, c| e.receive(c, &id, &plain, now));
                    }
                } else if let Some(n) = p2p::relay_notice(&text) {
                    p.notice = n;
                }
            }
        }
    }

    fn p2p_events(&mut self, k: Sess, events: Vec<p2p::Event>) {
        for ev in events {
            match ev {
                p2p::Event::Peer(_) => {}
                p2p::Event::Joined(_) if k == Sess::Group => {
                    // nel gruppo bastano i relay (messaggi piccoli); chi arriva riceve subito il nostro stato
                    self.group_send();
                }
                p2p::Event::Signal { .. } if k == Sess::Group => {}
                p2p::Event::Group { from, status } => self.group_status_in(from, status),
                p2p::Event::Joined(peer) => {
                    // collegamento diretto: lo apre chi ha l'id minore, l'altro aspetta l'offerta
                    let initiates = self.sess(k).and_then(|p| p.engine.as_ref()).is_some_and(|e| e.initiates(&peer));
                    if initiates {
                        self.direct_open(k, &peer, true);
                    }
                }
                p2p::Event::Signal { from, kind, sdp } => {
                    if kind == "offer" {
                        self.direct_open(k, &from, false);
                    }
                    if let Some(d) = self.sess(k).and_then(|p| p.direct.get(&from)) {
                        d.conn.signal(&kind, &sdp);
                    }
                }
                p2p::Event::Files { exam, n } => {
                    let active = exam == self.exams.active;
                    if let Some(p) = self.sess_mut(k) {
                        p.received += n;
                        p.dirty |= active;
                    }
                }
                p2p::Event::PackProgress { .. } => {}
                p2p::Event::PackDone { exam, result } => match result {
                    Ok(name) => {
                        self.refresh_exams();
                        if exam == self.exams.active {
                            self.reload_content();
                            self.reload_progress();
                            self.refresh_all();
                        }
                        self.toast(format!("Esame «{name}» ricevuto e installato"));
                    }
                    Err(e) => self.toast(format!("Esame non ricevuto: {e}")),
                },
            }
        }
    }

    fn p2p_download(&mut self, k: Sess, peer: &str, exam: &str) {
        let now = Self::now_secs();
        let (peer, exam) = (peer.to_string(), exam.to_string());
        self.with_engine(k, move |e, _| e.download(&peer, &exam, now));
        self.refresh_sessions();
    }

    /// Stato e dispositivi trovati di una sessione, per l'interfaccia.
    fn session_view(&self, k: Sess) -> Option<(String, Vec<PeerVM>)> {
        let p = self.sess(k)?;
        let open = p.open.iter().filter(|o| **o).count();
        let mut peers: Vec<PeerVM> = Vec::new();
        let who = if k == Sess::Sync { "dispositiv" } else { "person" };
        let mut status = if open == 0 {
            "Collegamento ai relay…".to_string()
        } else {
            format!("Collegato a {open}/{} relay", p.relays.len())
        };
        if let Some(eng) = p.engine.as_ref() {
            status.push_str(&match (eng.peers.len(), k) {
                (0, Sess::Sync) if open > 0 => " · in attesa degli altri dispositivi (apri l'app anche lì)".to_string(),
                (0, Sess::Share) if open > 0 && !p.sending.is_empty() => {
                    " · in attesa di chi deve ricevere (tieni aperta questa schermata)".to_string()
                }
                (0, Sess::Share) if open > 0 => " · in attesa di chi condivide".to_string(),
                (0, _) => String::new(),
                (1, _) => format!(" · 1 {who}{}", if k == Sess::Sync { "o" } else { "a" }),
                (n, _) => format!(" · {n} {who}{}", if k == Sess::Sync { "i" } else { "e" }),
            });
            if let Some((exam, got, total)) = eng.downloading() {
                let name = eng
                    .peers
                    .values()
                    .flat_map(|x| &x.exams)
                    .find(|e| e.id == exam)
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                status.push_str(&format!("\nRicevo «{name}»: {got}/{} pezzi", total.max(1)));
            }
            if !p.sending.is_empty() && p.queue.len() > 2 {
                status.push_str(&format!("\nInvio in corso: {} pezzi in coda", p.queue.len()));
            }
            for (id, peer) in &eng.peers {
                let mut offers: Vec<OfferVM> = Vec::new();
                for e in peer.exams.iter().filter(|e| e.shareable) {
                    let mine = self.exams.get(&e.id).map(|_| prep_core::pack::content_version(&self.exam_store(&e.id)));
                    let label = match mine {
                        None => format!("Scarica «{}»", e.name),
                        Some(v) if v != e.version => format!("Prendi i suoi contenuti di «{}»", e.name),
                        Some(_) => continue,
                    };
                    offers.push(OfferVM { peer: ss(id), exam: ss(&e.id), label: ss(label) });
                }
                let info = match k {
                    Sess::Group => String::new(),
                    Sess::Sync => match peer.exams.iter().filter(|e| self.exams.get(&e.id).is_some()).count() {
                        0 => "nessun esame in comune".to_string(),
                        1 => "1 esame in comune: progressi sincronizzati".to_string(),
                        n => format!("{n} esami in comune: progressi sincronizzati"),
                    },
                    Sess::Share if !p.sending.is_empty() => format!("collegata: può scaricare «{}»", p.sending),
                    Sess::Share => match peer.exams.first() {
                        Some(e) if offers.is_empty() => format!("condivide «{}»: hai già gli stessi contenuti", e.name),
                        Some(e) => format!("condivide «{}» con te", e.name),
                        None => "non sta condividendo nessun esame".to_string(),
                    },
                };
                let link = match p.direct.get(id) {
                    Some(d) if d.open => "collegamento diretto",
                    Some(_) => "collegamento diretto in apertura…",
                    None if p.failed.contains(id) && self.store.settings().direct_only => {
                        "collegamento diretto non riuscito (solo diretto attivo)"
                    }
                    None => "tramite relay",
                };
                peers.push(PeerVM {
                    id: ss(id),
                    name: ss(&peer.name),
                    info: ss(format!("{info} · {link}")),
                    offers: model(if k == Sess::Share && !p.sending.is_empty() { vec![] } else { offers }),
                });
            }
        }
        if p.received > 0 {
            status.push_str(&format!("\nRicevuti {} file di progressi", p.received));
        }
        if !p.notice.is_empty() && open < p.relays.len() {
            status.push_str(&format!("\n{}", p.notice));
        }
        Some((status, peers))
    }

    fn refresh_sessions(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        match self.session_view(Sess::Sync) {
            Some((status, peers)) => {
                st.set_p2p_on(true);
                st.set_p2p_status(ss(status));
                st.set_p2p_peers(model(peers));
            }
            None => {
                st.set_p2p_on(false);
                st.set_p2p_status(ss("Non collegato"));
                st.set_p2p_peers(model(Vec::<PeerVM>::new()));
            }
        }
        let name = self.exams.get(&self.exams.active).map(|e| e.name.clone()).unwrap_or_default();
        st.set_share_exam(ss(name));
        match self.session_view(Sess::Share) {
            Some((status, peers)) => {
                let p = self.share.as_ref();
                st.set_share_on(true);
                st.set_share_sending(p.is_some_and(|p| !p.sending.is_empty()));
                st.set_share_code(ss(p.map(|p| p.code.clone()).unwrap_or_default()));
                st.set_share_status(ss(status));
                st.set_share_peers(model(peers));
            }
            None => {
                st.set_share_on(false);
                st.set_share_sending(false);
                st.set_share_code(ss(""));
                st.set_share_status(ss(""));
                st.set_share_peers(model(Vec::<PeerVM>::new()));
            }
        }
    }

    // ---------- Bluetooth (Android) ----------

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    fn bt_event(&mut self, kind: i32, text: String, data: Vec<u8>) {
        match kind {
            0 => {
                if let Some((name, addr)) = text.split_once('\n') {
                    if !self.bt.found.iter().any(|(_, a)| a == addr) {
                        self.bt.found.push((name.to_string(), addr.to_string()));
                    }
                }
            }
            1 => {
                self.bt.status = format!("Collegato via Bluetooth a {text}");
                self.bt.connected = Some(text);
                self.bt.found.clear();
                // ci si ripresenta in ogni sessione aperta, così l'altro telefono ci trova anche offline
                let now = Self::now_secs();
                for k in [Sess::Sync, Sess::Share, Sess::Group] {
                    if self.sess(k).is_some() {
                        if let Some(p) = self.sess_mut(k) {
                            p.started = true;
                        }
                        self.with_engine(k, |e, c| e.start(c, now));
                    }
                }
            }
            2 => {
                let Ok(frame) = String::from_utf8(data) else { return };
                let Some((id, tag, content)) = p2p::parse_client_event(&frame) else { return };
                for k in [Sess::Sync, Sess::Share, Sess::Group] {
                    let Some(p) = self.sess(k) else { continue };
                    if p.room.tag != tag {
                        continue;
                    }
                    if let Some(plain) = p.room.open(&content) {
                        let now = Self::now_secs();
                        self.with_engine(k, |e, c| e.receive(c, &id, &plain, now));
                    }
                }
            }
            3 => {
                self.bt.connected = None;
                self.bt.status = text;
            }
            _ => self.bt.status = text,
        }
        self.refresh_bt();
    }

    fn refresh_bt(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_bt_connected(self.bt.connected.is_some());
        st.set_bt_status(ss(&self.bt.status));
        st.set_bt_devices(model(
            self.bt.found.iter().map(|(n, a)| BtDevVM { name: ss(n), address: ss(a) }).collect::<Vec<_>>(),
        ));
    }

    // ---------- studio di gruppo ----------

    /// Colore della propria monoposto: scelto nel Profilo, altrimenti l'accento di sistema,
    /// altrimenti uno fisso ricavato dall'identificativo.
    fn my_color(&mut self) -> u32 {
        let c = self.store.settings().car_color;
        if c != 0 {
            return c;
        }
        self.auto_color()
    }

    fn auto_color(&mut self) -> u32 {
        if let Some(a) = self.accent {
            return a;
        }
        let id = self.device_id();
        let h = p2p::sha256_hex(id.as_bytes());
        let n = usize::from_str_radix(&h[..4], 16).unwrap_or(0);
        prep_core::group::CAR_COLORS[n % prep_core::group::CAR_COLORS.len()]
    }

    fn set_car_color(&mut self, c: u32) {
        let mut s = self.store.settings();
        s.car_color = c;
        let _ = self.store.save_settings(&s);
        self.refresh_car_colors();
        self.group_send();
        self.refresh_group();
    }

    fn refresh_car_colors(&mut self) {
        let current = self.store.settings().car_color;
        let auto = self.auto_color();
        let mut v = vec![SwatchVM { color: slint::Color::from_argb_encoded(auto), value: 0, sel: current == 0, auto: true }];
        v.extend(prep_core::group::CAR_COLORS.iter().map(|c| SwatchVM {
            color: slint::Color::from_argb_encoded(*c),
            value: *c as i32,
            sel: current == *c,
            auto: false,
        }));
        self.ui().global::<AppState>().set_car_swatches(model(v));
    }

    /// Secondi del tratto di studio in corso (non ancora nel registro).
    fn study_running(&self, now: i64) -> u64 {
        self.study_since.map(|t| (now - t).clamp(0, 6 * 3600) as u64).unwrap_or(0)
    }

    fn my_group_status(&mut self, now: i64) -> prep_core::group::Status {
        let color = self.my_color();
        let name = self.display_name();
        let today = Local::now().date_naive();
        prep_core::group::my_status(
            &name,
            color,
            self.study_state,
            &self.study_log,
            self.study_running(now),
            today,
            now,
        )
    }

    /// Porta nel registro il tratto di studio in corso (alla pausa, alla fine, ogni tanto).
    fn study_flush(&mut self, now: i64) {
        if let Some(t) = self.study_since {
            let secs = (now - t).clamp(0, 6 * 3600) as u64;
            if secs > 0 {
                self.study_log.add(Local::now().date_naive(), secs);
                let _ = self.study_log.save(&self.store.base);
            }
            self.study_since = if self.study_state == prep_core::group::State::Studying { Some(now) } else { None };
        }
    }

    fn group_send(&mut self) {
        if self.group.as_ref().is_none_or(|p| !p.started) {
            return;
        }
        let now = Self::now_secs();
        self.group_last_send = now;
        let status = self.my_group_status(now);
        self.with_engine(Sess::Group, move |e, _| vec![e.group_status(status)]);
    }

    fn group_status_in(&mut self, from: String, status: prep_core::group::Status) {
        let now = Self::now_secs();
        if let Some(g) = self.groups.active_mut() {
            g.members.insert(from, prep_core::group::Member { status, last_seen: now });
            let _ = self.groups.save(&self.store.base);
        }
        self.refresh_group();
    }

    /// Collega la stanza del gruppo attivo (o chiude se non ce n'è).
    fn group_connect(&mut self) {
        let Some(g) = self.groups.active() else {
            self.close_session(Sess::Group);
            self.refresh_group();
            return;
        };
        let room = prep_core::group::room(&g.code);
        let device = self.device_id();
        let engine = p2p::Engine::new(&device, &self.display_name(), Self::device_label()).share_only(vec![]);
        self.open_session(Sess::Group, room, engine, String::new(), String::new());
        self.refresh_group();
    }

    fn group_create(&mut self, name: &str) {
        let name =
            if name.trim().is_empty() { format!("Gruppo di {}", self.display_name()) } else { name.trim().to_string() };
        let code = prep_core::group::new_code();
        self.groups.join(&code, &name);
        let _ = self.groups.save(&self.store.base);
        self.group_connect();
        self.group_invite();
    }

    fn group_join(&mut self, text: &str) {
        match prep_core::group::parse_invite(text) {
            Some((code, name)) => {
                self.groups.join(&code, &name);
                let _ = self.groups.save(&self.store.base);
                self.group_connect();
                self.ui().global::<AppState>().set_group_join_text(ss(""));
                self.toast("Sei entrato nel gruppo");
            }
            None => self.toast("Invito non valido: incolla il link o il codice di 16 caratteri"),
        }
    }

    fn group_select(&mut self, code: &str) {
        if self.groups.groups.iter().any(|g| g.code == code) {
            self.groups.active = code.to_string();
            let _ = self.groups.save(&self.store.base);
            self.group_connect();
        }
    }

    fn group_leave(&mut self, code: &str) {
        let armed =
            matches!(&self.group_leave_armed, Some((c, t)) if c == code && t.elapsed() < Duration::from_secs(5));
        if !armed {
            self.group_leave_armed = Some((code.to_string(), Instant::now()));
            self.toast("Tocca di nuovo per uscire dal gruppo");
            return;
        }
        self.group_leave_armed = None;
        self.groups.leave(code);
        let _ = self.groups.save(&self.store.base);
        self.group_connect();
    }

    /// Link d'invito: lo si condivide (Android, web) e resta visibile per copiarlo.
    fn group_invite(&mut self) {
        let Some(g) = self.groups.active() else { return };
        let link = prep_core::group::invite_link(&g.code, &g.name);
        self.ui().global::<AppState>().set_group_link(ss(&link));
        let text = format!(
            "Studiamo insieme su Tremendous Preparator: entra nel gruppo «{}» con questo invito\n{link}",
            g.name
        );
        if !crate::share_text(&text) {
            self.toast("Copia il link qui sotto e mandalo a chi vuoi invitare");
        }
    }

    fn study_set(&mut self, state: prep_core::group::State) {
        use prep_core::group::State;
        let now = Self::now_secs();
        self.study_flush(now);
        self.study_state = state;
        self.study_since = (state == State::Studying).then_some(now);
        if self.ui().global::<AppState>().get_focus_lock() {
            crate::focus_lock(state == State::Studying);
        }
        self.group_send();
        self.refresh_group();
    }

    /// Ogni secondo: cronometro, stato al gruppo ogni 10 s, circuito e classifica.
    fn group_tick(&mut self) {
        let now = Self::now_secs();
        if self.study_since.is_some_and(|t| now - t >= 60) {
            self.study_flush(now);
        }
        if self.group.is_some() && now - self.group_last_send >= 10 {
            self.group_send();
        }
        self.refresh_group();
    }

    fn refresh_group(&mut self) {
        use prep_core::group::{fmt_secs, live, pit_point, standings, track_point, value, Period, State, LAP_SECS};
        let now = Self::now_secs();
        let today = Local::now().date_naive();
        let me_id = self.device_id();
        let mine = self.my_group_status(now);
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let (t, _, _) = live(&mine, now, today);
        let laps = t / LAP_SECS;
        if laps > self.my_laps && self.study_state == State::Studying && self.my_laps > 0 {
            self.toast(format!("Giro {laps} completato!"));
            st.set_lap_flash(st.get_lap_flash() + 1);
        }
        self.my_laps = laps;
        st.set_study_state(match self.study_state {
            State::Idle => 0,
            State::Studying => 1,
            State::Pause => 2,
        });
        st.set_study_today(ss(format!("Oggi {} · {} giri", fmt_secs(t), laps)));
        st.set_study_lap(((t % LAP_SECS) as f32) / LAP_SECS as f32);
        st.set_groups(model(
            self.groups
                .groups
                .iter()
                .map(|g| GroupVM { id: ss(&g.code), name: ss(&g.name), active: g.code == self.groups.active })
                .collect::<Vec<_>>(),
        ));
        let solo = prep_core::group::Group::default();
        let has = self.groups.active().is_some();
        let g = self.groups.active().unwrap_or(&solo);
        st.set_group_has(has);
        if !has {
            st.set_ranking(model(Vec::<RankVM>::new()));
        }
        st.set_group_has(true);
        st.set_group_name(ss(&g.name));
        st.set_group_id(ss(&g.code));
        let online = |m: &prep_core::group::Member| now - m.last_seen < 45;
        // membri: io (sempre) e gli altri visti
        let mut all: Vec<(String, prep_core::group::Status, bool)> = vec![(me_id.clone(), mine.clone(), true)];
        for (id, m) in &g.members {
            if *id != me_id {
                all.push((id.clone(), m.status.clone(), online(m)));
            }
        }
        let on_track = all.iter().filter(|(_, s, on)| *on && s.state == State::Studying).count();
        let aspect = 2.4;
        let mut pit_i = 0usize;
        let pit_n = all.iter().filter(|(_, s, on)| !(*on && s.state == State::Studying)).count();
        let mut cars: Vec<CarVM> = Vec::new();
        for (k, (id, s, on)) in all.iter().enumerate() {
            let racing = *on && s.state == State::Studying;
            let (x, y, a) = if racing {
                let (tt, _, _) = live(s, now, today);
                track_point((tt % LAP_SECS) as f64 / LAP_SECS as f64, aspect)
            } else {
                let p = pit_point(pit_i, pit_n);
                pit_i += 1;
                p
            };
            let initial = s.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            cars.push(CarVM {
                x: x as f32,
                y: y as f32,
                angle: a as f32,
                color: slint::Color::from_argb_encoded(s.color),
                label: ss(initial),
                me: *id == me_id,
                moving: racing,
                offline: !*on,
                lane: if racing { [0.0, -1.0, 1.0][k % 3] } else { 0.0 },
            });
        }
        // telecamera sulla propria macchina mentre si studia; altrimenti tutto il circuito
        st.set_race_overview(self.study_state != State::Studying);
        if let Some(me) = cars.iter().find(|c| c.me) {
            st.set_cam_x(me.x);
            st.set_cam_y(me.y);
        }
        // la propria macchina sopra le altre
        cars.sort_by_key(|c| c.me);
        let p = self.group_period;
        let refs: Vec<(&str, &prep_core::group::Status)> = all.iter().map(|(i, s, _)| (i.as_str(), s)).collect();
        let order = standings(refs.into_iter(), now, today, p);
        let ranking: Vec<RankVM> = order
            .iter()
            .enumerate()
            .map(|(i, (id, s))| {
                let v = value(s, now, today, p);
                let on = all.iter().find(|(x, _, _)| x == id).is_some_and(|(_, _, o)| *o);
                RankVM {
                    pos: i as i32 + 1,
                    name: ss(if *id == me_id { format!("{} (tu)", s.name) } else { s.name.clone() }),
                    color: slint::Color::from_argb_encoded(s.color),
                    value: ss(fmt_secs(v)),
                    laps: (live(s, now, today).0 / LAP_SECS) as i32,
                    status: ss(match (on, s.state) {
                        (false, _) => "non collegato",
                        (true, State::Studying) => "in pista",
                        (true, State::Pause) => "ai box",
                        (true, State::Idle) => "fermo",
                    }),
                    me: *id == me_id,
                }
            })
            .collect();
        let my_pos = order.iter().position(|(id, _)| *id == me_id).map(|i| i + 1).unwrap_or(1);
        st.set_race_hud(ss(match self.study_state {
            State::Idle => "Fermo ai box · premi «Inizia a studiare»".to_string(),
            State::Pause => format!("Pit stop · giro {} · {}° su {}", laps + 1, my_pos, all.len()),
            State::Studying => format!("Giro {} · {}° su {} · {}", laps + 1, my_pos, all.len(), fmt_secs(t)),
        }));
        let others_online = all.iter().filter(|(i, _, on)| *on && *i != me_id).count();
        let link = self.group.as_ref().map(|p| p.open.iter().any(|o| *o)).unwrap_or(false);
        st.set_group_info(ss(format!(
            "{} membri · {} in pista{}",
            all.len(),
            on_track,
            if !link {
                " · collegamento in corso…".to_string()
            } else if others_online == 0 {
                " · nessun altro collegato ora".to_string()
            } else if others_online == 1 {
                " · 1 collegato".to_string()
            } else {
                format!(" · {others_online} collegati")
            }
        )));
        let _ = Period::Today;
        st.set_cars(model(cars));
        st.set_ranking(model(ranking));
    }

    fn show_legal_if_needed(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_legal_text(ss(LEGAL_TEXT));
        let required = self.store.settings().legal_accepted < LEGAL_VERSION;
        st.set_legal_required(required);
        if required {
            st.set_legal_visible(true);
        }
    }

    fn legal_accept(&mut self) {
        let mut s = self.store.settings();
        if s.legal_accepted < LEGAL_VERSION {
            s.legal_accepted = LEGAL_VERSION;
            let _ = self.store.save_settings(&s);
        }
        let ui = self.ui();
        let st = ui.global::<AppState>();
        st.set_legal_required(false);
        st.set_legal_visible(false);
    }

    fn save_username(&mut self, name: &str) {
        let mut s = self.store.settings();
        s.username = name.trim().chars().take(32).collect();
        match self.store.save_settings(&s) {
            Ok(()) => self.ui().global::<AppState>().set_s_username(ss(&s.username)),
            Err(e) => self.toast(format!("Errore: {e}")),
        }
    }

    fn poll_import(&mut self) {
        if self.importing {
            return;
        }
        let files = vfs::list_dir(&self.store.base.join("import"));
        // inviti a un gruppo aperti con l'app (Android li lascia qui come file .invite)
        if let Some(inv) = files.iter().find(|p| p.extension().is_some_and(|x| x == "invite")) {
            let text = vfs::read_to_string(inv).unwrap_or_default();
            let _ = vfs::remove_file(inv);
            self.group_join(&text);
            self.go_tab(4);
            return;
        }
        let next = files.into_iter().find(|p| !vfs::is_dir(p) && p.extension().is_none_or(|x| x != "tmp"));
        if let Some(p) = next {
            self.import_pack_file(p, true);
        }
    }

    fn pick_pack(&mut self) {
        #[cfg(target_os = "android")]
        if let Err(e) = crate::android_pick_pack() {
            self.toast(format!("Selettore file non disponibile: {e}"));
        }
        #[cfg(target_arch = "wasm32")]
        crate::web::pick_pack_file(self.store.base.join("import"));
        #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
        match newest_pack_download() {
            Some(p) => self.import_pack_file(p, false),
            None => self.toast(format!(
                "Nessun {} in Download: scaricalo dalla release «Contenuti» del repository",
                prep_core::pack::FILE_NAME
            )),
        }
    }

    /// GitHub configurato: senza proprietario, repository e token il tasto Sincronizza non compare in Home
    fn sync_configured(&self) -> bool {
        let s = self.store.settings();
        !s.owner.trim().is_empty() && !s.repo.trim().is_empty() && !s.token.trim().is_empty()
    }

    fn save_settings(&mut self) {
        let ui = self.ui();
        let st = ui.global::<AppState>();
        let mut s = self.store.settings();
        s.owner = st.get_s_owner().trim().to_string();
        s.repo = st.get_s_repo().trim().to_string();
        s.branch = st.get_s_branch().trim().to_string();
        s.token = st.get_s_token().trim().to_string();
        match self.store.save_settings(&s) {
            Ok(()) => {
                self.refresh_settings_info();
                self.start_sync(true)
            }
            Err(e) => self.toast(format!("Errore: {e}")),
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
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("XDG_DOWNLOAD_DIR") {
        dirs.push(d.into());
    }
    // ~/.config/user-dirs.dirs: XDG_DOWNLOAD_DIR="$HOME/Scaricati"
    if let Ok(t) = std::fs::read_to_string(home.join(".config/user-dirs.dirs")) {
        for l in t.lines() {
            if let Some(v) = l.strip_prefix("XDG_DOWNLOAD_DIR=") {
                let v = v.trim().trim_matches('"').replace("$HOME", &home.to_string_lossy());
                dirs.push(v.into());
            }
        }
    }
    dirs.push(home.join("Downloads"));
    dirs.push(home.join("Scaricati"));
    let stem = prep_core::pack::FILE_NAME.trim_end_matches(".json.gz");
    dirs.iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|rd| rd.flatten().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            n.starts_with(stem) && n.ends_with(".gz")
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
        from_repo: false,
        unit_id: String::new(),
        quiz: None,
        cards: None,
        results: None,
        pending_flags: BTreeMap::new(),
        dialog: None,
        reader: None,
        fb_item: None,
        last_save: Instant::now(),
        syncing: false,
        sync_verbose: false,
        plan: None,
        plan_sel: vec![],
        plan_extra: vec![],
        feedback_from_settings: false,
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
        group_period: prep_core::group::Period::Today,
        group_timer: Timer::default(),
        group_last_send: 0,
        group_leave_armed: None,
        my_laps: 0,
        bt: BtState::default(),
        accent: None,
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
        c.ui().global::<AppState>().set_focus_available(cfg!(target_os = "android"));
        // colori di sistema (Android 12+): tema e monoposto
        if let Some((light, dark)) = crate::system_tones().and_then(|t| crate::palette::dyn_roles(&t)) {
            c.accent = Some(light[0]);
            let ui = c.ui();
            let m3 = ui.global::<crate::M3>();
            let to = |v: &[u32]| model(v.iter().map(|x| slint::Color::from_argb_encoded(*x)).collect::<Vec<_>>());
            m3.set_dyn_light(to(&light));
            m3.set_dyn_dark(to(&dark));
            m3.set_dynamic(true);
        }
        c.refresh_car_colors();
        c.ui().global::<AppState>().set_bt_available(cfg!(target_os = "android"));
        let dashes: Vec<DashVM> = prep_core::group::centerline(2.4, 120)
            .into_iter()
            .map(|(x, y, a)| DashVM { x: x as f32, y: y as f32, angle: a as f32 })
            .collect();
        c.ui().global::<AppState>().set_dashes(model(dashes));
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
            let label = if tag.ends_with("a1") { "1° appello 2025" } else { "2° appello 2025" };
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
    st.on_open_nomenclature(|| {
        with_ctrl(|c| {
            c.open_ref(Some(BookRef {
                book: NOMENCLATURE_BOOK.into(),
                file: "books/chimica/integrazioni/02-nomenclatura.md".into(),
                section: "2.1".into(),
                page: None,
            }))
        })
    });
    st.on_send_session_feedback(|| with_ctrl(|c| c.send_session_feedback()));
    st.on_send_feedback(|| {
        with_ctrl(|c| {
            let back_to_results = c.results.is_some() && c.fb_item.is_some();
            c.send_feedback();
            if back_to_results {
                c.set_page(P_RESULTS);
            }
        })
    });
    st.on_report_result(|k| with_ctrl(|c| c.report_result(k.max(0) as usize)));
    st.on_clear_fb_item(|| {
        with_ctrl(|c| {
            c.fb_item = None;
            let ui = c.ui();
            let st = ui.global::<AppState>();
            st.set_fb_item(ss(""));
            st.set_fb_item_text(ss(""));
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
    st.on_sync_now(|| with_ctrl(|c| c.start_sync(true)));
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
    st.on_open_feedback(|| {
        with_ctrl(|c| {
            c.feedback_from_settings = true;
            c.refresh_feedback();
            c.set_page(P_FEEDBACK);
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
    st.on_save_settings(|| with_ctrl(|c| c.save_settings()));
    st.on_import_pack(|| with_ctrl(|c| c.pick_pack()));
    st.on_select_exam(|id| with_ctrl(|c| c.open_exam(&id)));
    st.on_remove_exam(|id| with_ctrl(|c| c.remove_exam(&id)));
    st.on_save_username(|n| with_ctrl(|c| c.save_username(&n)));
    st.on_legal_accept(|| with_ctrl(|c| c.legal_accept()));
    st.on_car_color(|v| with_ctrl(|c| c.set_car_color(v as u32)));
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
    st.on_study_start(|| with_ctrl(|c| c.study_set(prep_core::group::State::Studying)));
    st.on_study_pause(|| with_ctrl(|c| c.study_set(prep_core::group::State::Pause)));
    st.on_study_stop(|| with_ctrl(|c| c.study_set(prep_core::group::State::Idle)));
    st.on_group_set_period(|p| {
        with_ctrl(|c| {
            c.group_period = match p {
                1 => prep_core::group::Period::Week,
                2 => prep_core::group::Period::Total,
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
    // sync all'avvio
    ctrl.borrow_mut().start_sync(false);

    crate::demo::maybe_run(&ui);
    let r = ui.run();
    with_ctrl(|c| {
        c.autosave_quiz();
        c.flush_flags();
        c.save_cards();
    });
    r
}
