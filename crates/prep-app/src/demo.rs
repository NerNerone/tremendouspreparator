//! Modalità dimostrativa per sviluppo desktop: esegue una sequenza di azioni
//! e salva screenshot. Esempio:
//!   PREP_DEMO="shot:home;tab:1;shot:studio" PREP_SHOTS=/tmp/shots cargo run --bin prepapp-desktop
//! Con PREP_DRAGPAN=1 il mouse trascina il contenuto come un dito (su desktop Slint lo
//! permette solo al tocco): «drag:x,y,dx,dy» verifica che lo scroll funzioni davvero.
//! Su Android non fa nulla.

#[allow(unused_imports)]
use crate::{AppState, AppWindow};

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn maybe_run(ui: &AppWindow) {
    use slint::ComponentHandle;
    use std::time::Duration;
    let Ok(script) = std::env::var("PREP_DEMO") else {
        return;
    };
    let dir = std::env::var("PREP_SHOTS").unwrap_or_else(|_| ".".into());
    // PREP_SIZE=LxA: finestra in pixel logici (es. 400x860 per simulare un telefono)
    if let Some((w, h)) = std::env::var("PREP_SIZE").ok().and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
    }) {
        ui.window().set_size(slint::LogicalSize::new(w, h));
    }
    if std::env::var("PREP_DRAGPAN").is_ok() {
        ui.global::<AppState>().set_drag_pan(true);
    }
    let steps: Vec<String> = script.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    // PREP_TIMING=1: per ogni comando, il tempo fino al primo fotogramma disegnato dopo
    // (creazione della pagina, impaginazione e disegno): così si vede cosa si apre lento
    let pending: std::rc::Rc<std::cell::RefCell<Option<(std::time::Instant, String)>>> = Default::default();
    if std::env::var_os("PREP_TIMING").is_some() {
        let p = pending.clone();
        let _ = ui.window().set_rendering_notifier(move |state, _| {
            if matches!(state, slint::RenderingState::AfterRendering) {
                if let Some((t, cmd)) = p.borrow_mut().take() {
                    eprintln!("fotogramma: {:>5.0} ms dopo «{cmd}»", t.elapsed().as_secs_f64() * 1000.0);
                }
            }
        });
    }
    let weak = ui.as_weak();
    let step = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let timer = Box::leak(Box::new(slint::Timer::default()));
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(700), move || {
        let Some(ui) = weak.upgrade() else { return };
        let i = step.get();
        step.set(i + 1);
        let Some(cmd) = steps.get(i) else {
            let _ = slint::quit_event_loop();
            return;
        };
        if !cmd.starts_with("shot") && !cmd.starts_with("wait") {
            *pending.borrow_mut() = Some((std::time::Instant::now(), cmd.clone()));
        }
        for cmd in cmd.split('&') {
            let (op, arg) = cmd.split_once(':').unwrap_or((cmd, ""));
            let st = ui.global::<AppState>();
            match op {
                "tab" => st.invoke_go_tab(arg.parse().unwrap_or(0)),
                "subject" => st.invoke_select_subject(arg.into()),
                "unit" => st.invoke_open_unit(arg.into()),
                "goals" => st.invoke_goals_confirm(),
                "goalspage" => st.invoke_open_goals(),
                "gopen" => {
                    // gopen:N — espande l'N-esimo obiettivo
                    let i: usize = arg.parse().unwrap_or(0);
                    if let Some(g) = slint::Model::row_data(&st.get_goals(), i) {
                        st.set_goals_open(format!("g:{}", g.topic).into());
                    }
                }
                "scroll" => st.set_home_scroll(-(arg.parse::<f32>().unwrap_or(0.0))),
                "settings" => st.invoke_open_settings(),
                "settingssec" => st.set_settings_sec(arg.parse().unwrap_or(-1)),
                // strumenti: tool:id (apre), tlsel:n (casella), tlfilter:n (famiglia), tlq:testo
                // (ricerca), tlin:i=valore (campo del calcolatore), cttools (foglio «Aggiungi»)
                "periodic" => st.invoke_open_tool("tavola-periodica".into()),
                "tool" => st.invoke_open_tool(arg.into()),
                "pt" | "tlsel" => st.invoke_tl_select(arg.parse().unwrap_or(0)),
                "ptfilter" | "tlfilter" => st.set_tl_filter(arg.parse().unwrap_or(-1)),
                "tlq" => st.invoke_tl_search(arg.into()),
                "tlin" => {
                    let (i, v) = arg.split_once('=').unwrap_or(("0", ""));
                    st.invoke_tl_input(i.parse().unwrap_or(0), v.into())
                }
                "cttools" => st.invoke_ct_tools_add_open(),
                "cttool" => st.invoke_ct_tool_menu(arg.into()),
                // cttooladd:n (n-esimo del foglio «Aggiungi»), cttoolcopy:n (n-esimo pacchetto)
                "cttooladd" => {
                    if let Some(r) = slint::Model::row_data(&st.get_ct_tool_src(), arg.parse().unwrap_or(0)) {
                        st.invoke_ct_tool_add(r.pkg, r.id)
                    }
                }
                "cttoolcopy" => {
                    if let Some(r) = slint::Model::row_data(&st.get_ct_dests(), arg.parse().unwrap_or(0)) {
                        st.invoke_ct_tool_copy(r.id)
                    }
                }
                "link" => st.invoke_open_subject_link(arg.parse().unwrap_or(0)),
                "fbpage" => st.invoke_open_feedback(),
                "wait" => {}
                // pagina «Contenuti»: contents (apre), addfile:percorso (come se fosse scelto),
                // contentsexport, contentsrm:percorso nel mirror
                "contents" => st.invoke_open_contents(),
                "qacheck" => st.invoke_contents_check(),
                "guide" => st.invoke_open_guide(),
                "photofile" | "gphotofile" => {
                    if let Ok(b) = std::fs::read(arg) {
                        crate::app::demo_photo(if op == "gphotofile" { "group" } else { "user" }, &b);
                    }
                }
                "cropok" => st.invoke_crop_done(-1.0, -1.0, 1e9),
                "rdgoto" => st.invoke_reader_goto(arg.parse().unwrap_or(0)),
                "rdtoc" => st.set_rd_toc_visible(arg == "1"),
                "contentsexport" => st.invoke_contents_export(),
                "contentsrm" => st.invoke_contents_remove(arg.into()),
                "addfile" => {
                    if let (Ok(bytes), Some(dir)) = (std::fs::read(arg), std::env::var_os("PREP_DATA")) {
                        let name =
                            std::path::Path::new(arg).file_name().unwrap_or_default().to_string_lossy().to_string();
                        let dst = std::path::Path::new(&dir).join("import").join(format!("file-1--{name}"));
                        let _ = std::fs::create_dir_all(dst.parent().unwrap_or(std::path::Path::new(".")));
                        let _ = std::fs::write(dst, bytes);
                    }
                }
                // prova della segnalazione degli errori
                "panic" => panic!("errore di prova dalla modalità dimostrativa"),
                "qz" => {
                    // qz:tipo,id — come un tocco nella scheda Quiz (subject/unit/source/kind/open/n)
                    let (k, id) = arg.split_once(',').unwrap_or((arg, ""));
                    st.invoke_qz_toggle(k.into(), id.into())
                }
                "qzstart" => st.invoke_qz_start(),
                "exammenu" => st.set_exam_menu(true),
                "nonews" => st.set_news_visible(false),
                "side" => st.set_side_page(arg.parse().unwrap_or(-1)),
                "offerbranch" => {
                    st.set_offer_branch_name("semestre-aperto".into());
                    st.set_offer_branch(true);
                }
                "fab" => st.set_fab_open(!st.get_fab_open()),
                "setting" => st.invoke_open_setting(arg.parse().unwrap_or(0)),
                // sincronizzazione tra dispositivi: user:nome; p2prelay:ws://…; p2p:password; p2pget
                "user" => st.invoke_save_username(arg.into()),
                "p2prelay" => st.set_p2p_relays(arg.into()),
                "p2p" => {
                    st.set_p2p_password(arg.into());
                    st.invoke_p2p_connect()
                }
                "legal" => st.invoke_legal_accept(),
                // studio di gruppo: gcreate:nome (stampa il link), gjoin:link, study, pit, stop
                "gcreate" => {
                    st.invoke_group_create(arg.into());
                    eprintln!("INVITO {}", st.get_group_link());
                }
                "gjoin" => st.invoke_group_join(arg.into()),
                "fxdemo" => crate::app::demo_fx(),
                "pdftools" => st.invoke_open_pdf_tools(),
                "creator" => st.invoke_open_creator(arg == "new"),
                // registro delle simulazioni: simlog (apre), simopen:N (riapre la N-esima)
                "simlog" => st.invoke_open_simlog(),
                "updcheck" => st.invoke_update_check(),
                // schermata dell'aggiornamento obbligatorio: updfake:frazione (0 = da avviare)
                "updfake" => {
                    st.set_update_version("0.3.0".into());
                    st.set_update_required(true);
                    let f: f32 = arg.parse().unwrap_or(0.0);
                    st.set_update_busy(f > 0.0);
                    st.set_update_frac(f);
                    st.set_update_phase("Scarico la nuova versione… 18,4 di 41,2 MB".into());
                }
                "rmexam" => st.invoke_remove_exam(arg.into()),
                "simopen" => {
                    if let Some(r) = slint::Model::row_data(&st.get_sim_rows(), arg.parse().unwrap_or(0)) {
                        st.invoke_simlog_open(r.id);
                    }
                }
                "crexam" => st.invoke_cr_create_exam(arg.into()),
                "cropen" => st.invoke_cr_open_file(arg.parse().unwrap_or(0), "".into()),
                "crname" => {
                    st.set_cr_name(arg.into());
                    st.invoke_cr_refresh_prompt();
                }
                "crtext" => st.set_cr_text(std::fs::read_to_string(arg).unwrap_or_default().into()),
                "crsave" => st.invoke_cr_save(),
                "crfig" => {
                    let figs = st.get_cr_figs();
                    if let Some(f) = slint::Model::row_data(&figs, arg.parse().unwrap_or(0)) {
                        st.invoke_cr_open_figure(f.path);
                    }
                }
                "crclose" => st.invoke_cr_close(),
                "crscroll" => {}
                "pdfpages" => st.set_pdf_pages(arg.into()),
                "pdfmb" => st.set_pdf_max_mb(arg.parse().unwrap_or(20.0)),
                "pdfsplit" => crate::app::demo_pdf("split", arg),
                "pdftext" => crate::app::demo_pdf("text", arg),
                "pdfocr" => crate::app::demo_pdf("ocr", arg),
                "profileme" => crate::app::demo_profile_me(),
                // heatshift:N — la propria mappa di calore N mesi indietro (negativo: avanti)
                "heatshift" => st.invoke_heat_shift(0, arg.parse().unwrap_or(1)),
                "gcal" => st.invoke_gcal_select(arg.parse().unwrap_or(0)),
                "gcalmonth" => st.invoke_gcal_mode(arg == "1"),
                "study" => st.invoke_study_start(),
                "pit" => st.invoke_study_pause(),
                "stop" => st.invoke_study_stop(),
                "cartap" => st.invoke_car_tap(arg.into()),
                "cartapme" => st.invoke_car_tap(st.get_my_user_id()),
                "carclose" => st.invoke_car_close(),
                "theme" => st.invoke_theme_color(arg.parse::<u32>().unwrap_or(0) as i32),
                "pickphoto" => st.invoke_pick_photo(arg.into()),
                "member" => st.set_member_sel(arg.into()),
                "gping" => st.invoke_group_ping(arg.into()),
                "gkick" => st.invoke_group_kick(arg.into()),
                "grename" => st.invoke_group_rename(arg.into()),
                // concentrazione (con PREP_FOCUS_FAKE sul desktop): focus, focusapp:pacchetto, focusmin:n
                "focus" => st.invoke_focus_setup(true),
                "focusapp" => st.invoke_focus_toggle(arg.into()),
                "focusmin" => st.invoke_focus_set_minutes(arg.parse().unwrap_or(50)),
                "focusstart" => st.invoke_focus_start(),
                "gkickother" => {
                    // rimuove il primo membro che non sia io (due tocchi: conferma)
                    let r = st.get_ranking();
                    if let Some(m) =
                        (0..slint::Model::row_count(&r)).filter_map(|i| slint::Model::row_data(&r, i)).find(|m| !m.me)
                    {
                        st.invoke_group_kick(m.id.clone());
                        st.invoke_group_kick(m.id);
                    }
                }
                "shareok" => st.set_share_ok(true),
                "sharestart" => {
                    st.invoke_share_start();
                    eprintln!("CODICE {}", st.get_share_code());
                }
                "sharejoin" => {
                    st.set_share_input(arg.into());
                    st.invoke_share_join()
                }
                "shareget" => {
                    let peers = st.get_share_peers();
                    let first = (0..slint::Model::row_count(&peers))
                        .filter_map(|i| slint::Model::row_data(&peers, i))
                        .find_map(|p| slint::Model::row_data(&p.offers, 0));
                    match first {
                        Some(o) => st.invoke_share_download(o.peer, o.exam),
                        None => eprintln!("shareget: niente da scaricare"),
                    }
                }
                "p2pget" => {
                    let peers = st.get_p2p_peers();
                    let first = (0..slint::Model::row_count(&peers))
                        .filter_map(|i| slint::Model::row_data(&peers, i))
                        .find_map(|p| slint::Model::row_data(&p.offers, 0));
                    match first {
                        Some(o) => st.invoke_p2p_download(o.peer, o.exam),
                        None => eprintln!("p2pget: nessun esame da scaricare"),
                    }
                }
                "useexam" => st.invoke_select_exam(arg.into()),
                "importpack" => st.invoke_import_pack(),
                "rdscroll" => st.set_rd_scroll(-(arg.parse::<f32>().unwrap_or(0.0))),
                "fakeprog" => st.set_overall(arg.parse().unwrap_or(0.42)),
                "gadd" => st.invoke_goals_add(arg.into()),
                "goalsoff" => st.invoke_goals_toggle(arg.parse().unwrap_or(0)),
                "practice" => {
                    let (k, id) = arg.split_once(',').unwrap_or((arg, ""));
                    st.invoke_start_practice(k.into(), id.into())
                }
                "exam" => st.invoke_start_exam(arg.into()),
                "official" => {
                    let (a, b) = arg.split_once(',').unwrap_or((arg, ""));
                    st.invoke_start_official(a.into(), b.into())
                }
                "review" => st.invoke_start_review(arg.into()),
                "cards" => {
                    let (k, id) = arg.split_once(',').unwrap_or((arg, ""));
                    st.invoke_start_cards(k.into(), id.into())
                }
                "opt" => st.invoke_select_option(arg.parse().unwrap_or(0)),
                "fill" => {
                    st.set_q_fill(arg.into());
                    st.invoke_fill_edited(arg.into())
                }
                "unsure" => st.invoke_toggle_unsure(),
                "confirm" => st.invoke_confirm(),
                "next" => st.invoke_next(),
                "skip" => st.invoke_skip(),
                "finish" => st.invoke_finish(),
                "ok" => st.invoke_dialog_ok(),
                "flip" => st.invoke_card_flip(),
                "grade" => st.invoke_card_grade(arg.parse().unwrap_or(2)),
                "flagres" => st.invoke_result_toggle_flag(arg.parse().unwrap_or(0)),
                "close" => st.invoke_results_close(),
                "openref" => st.invoke_open_ref_quiz(),
                "pause" => st.invoke_pause_quiz(),
                "resume" => st.invoke_resume_quiz(),
                "note" => st.set_c_note(arg.into()),
                "rate" => st.set_r_rating(arg.parse().unwrap_or(0)),
                "rtext" => st.set_r_feedback(arg.into()),
                "sendfb" => st.invoke_send_session_feedback(),
                "report" => st.invoke_report_result(arg.parse().unwrap_or(0)),
                "fbtext" => st.set_fb_text(arg.into()),
                "sendgen" => st.invoke_send_feedback(),
                "openrefcard" => st.invoke_open_ref_card(),
                "book" => st.invoke_open_book(arg.into()),
                "rnext" => st.invoke_reader_next(),
                "rgoto" => st.invoke_reader_goto(arg.parse().unwrap_or(0)),
                "back" => {
                    st.invoke_back();
                }
                "delayshot" => {
                    // delayshot:ms,nome — screenshot dopo ms (per fotografare le animazioni a metà)
                    let (ms, name) = arg.split_once(',').unwrap_or(("100", arg));
                    let weak = ui.as_weak();
                    let dir = dir.clone();
                    let name = name.to_string();
                    slint::Timer::single_shot(Duration::from_millis(ms.parse().unwrap_or(100)), move || {
                        if let Some(ui) = weak.upgrade() {
                            save_shot(&ui, &format!("{dir}/{name}.png"));
                        }
                    });
                }
                "click" => {
                    // click:x,y — tocco simulato
                    let v: Vec<f32> = arg.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if v.len() == 2 {
                        use slint::platform::{PointerEventButton, WindowEvent};
                        let p = slint::LogicalPosition::new(v[0], v[1]);
                        ui.window().dispatch_event(WindowEvent::PointerMoved { position: p });
                        ui.window().dispatch_event(WindowEvent::PointerPressed {
                            position: p,
                            button: PointerEventButton::Left,
                        });
                        ui.window().dispatch_event(WindowEvent::PointerReleased {
                            position: p,
                            button: PointerEventButton::Left,
                        });
                        ui.window().dispatch_event(WindowEvent::PointerExited);
                    }
                }
                "drag" => {
                    // drag:x,y,dx,dy — trascinamento simulato (come un dito): premi, 12 movimenti
                    // a 16 ms l'uno, rilascia; serve a verificare che lo scroll funzioni davvero
                    let v: Vec<f32> = arg.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if v.len() == 4 {
                        use slint::platform::{PointerEventButton, WindowEvent};
                        let (x, y, dx, dy) = (v[0], v[1], v[2], v[3]);
                        let pos = move |k: f32| slint::LogicalPosition::new(x + dx * k, y + dy * k);
                        ui.window().dispatch_event(WindowEvent::PointerMoved { position: pos(0.0) });
                        ui.window().dispatch_event(WindowEvent::PointerPressed {
                            position: pos(0.0),
                            button: PointerEventButton::Left,
                        });
                        let weak = ui.as_weak();
                        let step = std::rc::Rc::new(std::cell::Cell::new(0u32));
                        let t = Box::leak(Box::new(slint::Timer::default()));
                        let tp = t as *const slint::Timer;
                        t.start(
                            slint::TimerMode::Repeated,
                            Duration::from_millis(
                                std::env::var("PREP_DRAGMS").ok().and_then(|v| v.parse().ok()).unwrap_or(16),
                            ),
                            move || {
                                let Some(ui) = weak.upgrade() else { return };
                                let i = step.get() + 1;
                                step.set(i);
                                if i <= 12 {
                                    ui.window()
                                        .dispatch_event(WindowEvent::PointerMoved { position: pos(i as f32 / 12.0) });
                                } else {
                                    ui.window().dispatch_event(WindowEvent::PointerReleased {
                                        position: pos(1.0),
                                        button: PointerEventButton::Left,
                                    });
                                    ui.window().dispatch_event(WindowEvent::PointerExited);
                                    // SAFETY: il timer è stato «leaked» e resta valido; lo si ferma soltanto
                                    unsafe { (*tp).stop() };
                                }
                            },
                        );
                    }
                }
                "wheel" => {
                    use slint::platform::WindowEvent;
                    let p = slint::LogicalPosition::new(200.0, 600.0);
                    ui.window().dispatch_event(WindowEvent::PointerMoved { position: p });
                    ui.window().dispatch_event(WindowEvent::PointerScrolled {
                        position: p,
                        delta_x: 0.0,
                        delta_y: -(arg.parse::<f32>().unwrap_or(300.0)),
                    });
                }
                "shot" => {
                    if let Ok(buf) = ui.window().take_snapshot() {
                        let path = format!("{dir}/{arg}.png");
                        if let Ok(f) = std::fs::File::create(&path) {
                            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), buf.width(), buf.height());
                            enc.set_color(png::ColorType::Rgba);
                            enc.set_depth(png::BitDepth::Eight);
                            if let Ok(mut w) = enc.write_header() {
                                let _ = w.write_image_data(buf.as_bytes());
                            }
                            eprintln!("screenshot {path}");
                        }
                    }
                }
                _ => eprintln!("comando demo sconosciuto: {cmd}"),
            }
        }
    });
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn save_shot(ui: &AppWindow, path: &str) {
    use slint::ComponentHandle;
    if let Ok(buf) = ui.window().take_snapshot() {
        if let Ok(f) = std::fs::File::create(path) {
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), buf.width(), buf.height());
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            if let Ok(mut w) = enc.write_header() {
                let _ = w.write_image_data(buf.as_bytes());
            }
        }
    }
}

#[cfg(any(target_os = "android", target_arch = "wasm32"))]
pub fn maybe_run(_ui: &AppWindow) {}
