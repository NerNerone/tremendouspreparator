//! Strumento da riga di comando per lavorare sul repository (usato da Claude in chat).
//!
//!   cargo run -p prep-core --bin prep -- validate          controlla i contenuti
//!   cargo run -p prep-core --bin prep -- stats             copertura domande/flashcard per argomento
//!   cargo run -p prep-core --bin prep -- report            report progressi (markdown)
//!   cargo run -p prep-core --bin prep -- report --json     stesso report in JSON
//!   cargo run -p prep-core --bin prep -- feedback          feedback senza risposta (--all per tutti)
//!   cargo run -p prep-core --bin prep -- pack --out=F.json.gz [--label=L]
//!                                                         pacchetto contenuti (quiz, flashcard, libri)

use prep_core::content::Content;
use prep_core::model::QKind;
use prep_core::progress::{build_history, compute, exam_pace, Assessment};
use prep_core::session::{Session, SessionKind};
use std::fs;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

fn load_content(root: &Path) -> Content {
    let mut files = Vec::new();
    walk(&root.join("content"), &mut files);
    let texts: Vec<(String, String)> = files
        .iter()
        .map(|p| {
            (
                p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"),
                fs::read_to_string(p).unwrap_or_default(),
            )
        })
        .collect();
    Content::from_files(texts.iter().map(|(a, b)| (a.as_str(), b.as_str())))
}

fn load_sessions(root: &Path) -> Vec<Session> {
    let mut files = Vec::new();
    walk(&root.join("progress/sessions"), &mut files);
    let mut v = Vec::new();
    for p in files {
        match fs::read_to_string(&p).map(|t| serde_json::from_str::<Session>(&t)) {
            Ok(Ok(s)) => v.push(s),
            _ => eprintln!("sessione non leggibile: {}", p.display()),
        }
    }
    v.sort_by_key(|s| s.ended_at);
    v
}

fn mmss(s: f64) -> String {
    if s <= 0.0 {
        "-".into()
    } else {
        format!("{}:{:02}", (s as u32) / 60, (s as u32) % 60)
    }
}

fn hm(s: u64) -> String {
    if s == 0 {
        "-".into()
    } else if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min", (s + 30) / 60)
    } else {
        format!("{} h {:02} min", s / 3600, (s % 3600) / 60)
    }
}

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("report");
    let json = args.iter().any(|a| a == "--json");
    let root =
        args.iter().skip(1).find(|a| !a.starts_with("--")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let content = load_content(&root);
    let opt = |k: &str| args.iter().find_map(|a| a.strip_prefix(&format!("--{k}=")).map(|s| s.to_string()));
    match cmd {
        "pack" => {
            let out = opt("out").unwrap_or_else(|| prep_core::pack::FILE_NAME.to_string());
            let label = opt("label").unwrap_or_default();
            match prep_core::pack::build(&root, &label) {
                Ok((bytes, info)) => {
                    fs::write(&out, &bytes).expect("scrittura del pacchetto");
                    println!(
                        "{out}: {} file, {} domande, {} flashcard, {:.1} MB",
                        info.files,
                        info.questions,
                        info.flashcards,
                        bytes.len() as f64 / 1e6
                    );
                }
                Err(e) => {
                    eprintln!("ERRORE: {e}");
                    std::process::exit(1);
                }
            }
        }
        "validate" => {
            if content.syllabus.is_none() {
                eprintln!("ERRORE: content/syllabus.json mancante o non valido");
                std::process::exit(1);
            }
            let mut warnings = content.warnings.clone();
            // rimandi ai libri: file esistente e sezione/pagina trovata
            for b in &content.books {
                for ch in &b.chapters {
                    let Ok(md) = fs::read_to_string(root.join(&ch.file)) else {
                        warnings.push(format!("libro {}: file mancante {}", b.id, ch.file));
                        continue;
                    };
                    let secs = prep_core::markdown::sections(&prep_core::markdown::parse(&md));
                    for st in &ch.sections {
                        if prep_core::markdown::find_section(&secs, &st.section, None).is_none() {
                            warnings
                                .push(format!("libro {}: sezione '{}' non trovata in {}", b.id, st.section, ch.file));
                        }
                    }
                }
            }
            let refs = content
                .questions
                .iter()
                .map(|q| (q.id.as_str(), q.book_ref.as_ref()))
                .chain(content.cards.iter().map(|c| (c.id.as_str(), c.book_ref.as_ref())));
            for (id, r) in refs {
                let Some(r) = r else { continue };
                let Some(r) = content.resolve_ref(Some(r), "") else {
                    continue;
                };
                match fs::read_to_string(root.join(&r.file)) {
                    Err(_) => warnings.push(format!("{id}: file del libro mancante {}", r.file)),
                    Ok(md) => {
                        let secs = prep_core::markdown::sections(&prep_core::markdown::parse(&md));
                        if (!r.section.is_empty() || r.page.is_some())
                            && prep_core::markdown::find_section(&secs, &r.section, r.page).is_none()
                        {
                            warnings.push(format!("{id}: sezione '{}' non trovata in {}", r.section, r.file));
                        }
                    }
                }
            }
            for w in &warnings {
                println!("⚠ {w}");
            }
            println!(
                "{} domande ({} mcq, {} completamento), {} flashcard, {} avvisi",
                content.questions.len(),
                content.questions.iter().filter(|q| q.kind == QKind::Mcq).count(),
                content.questions.iter().filter(|q| q.kind == QKind::Fill).count(),
                content.cards.len(),
                warnings.len()
            );
            if !warnings.is_empty() {
                std::process::exit(2);
            }
        }
        "feedback" => {
            // feedback ancora senza risposta (o tutti con --all)
            let all = args.iter().any(|a| a == "--all");
            let status: prep_core::feedback::FeedbackStatus =
                fs::read_to_string(root.join("progress/feedback_status.json"))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default();
            let mut files = Vec::new();
            walk(&root.join("progress/feedback"), &mut files);
            let mut list: Vec<prep_core::feedback::Feedback> = files
                .iter()
                .filter_map(|p| fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()))
                .collect();
            list.sort_by_key(|f| f.created_at);
            let mut n = 0;
            for f in list {
                let st = status.items.get(&f.id);
                if !all && st.is_some() {
                    continue;
                }
                n += 1;
                println!(
                    "## {} · {} {}{}",
                    f.id,
                    f.kind,
                    f.category,
                    f.rating.map(|r| format!(" · voto {r}/5")).unwrap_or_default()
                );
                if !f.context.is_empty() {
                    println!("contesto: {}", f.context);
                }
                if let Some(s) = &f.session {
                    println!("sessione: {s}");
                }
                if let Some(i) = &f.item {
                    match content.question(i) {
                        Some(q) => println!("domanda {i}: {}", q.text),
                        None => println!("elemento: {i}"),
                    }
                }
                println!("\n{}\n", if f.text.is_empty() { "(nessun testo)" } else { &f.text });
                if let Some(st) = st {
                    println!("→ {}: {}\n", st.status, st.reply);
                }
            }
            if n == 0 {
                println!("Nessun feedback da leggere.");
            }
        }
        "stats" => {
            for s in &content.syllabus().subjects {
                println!("## {}", s.name);
                for u in &s.units {
                    let mcq = content.questions_in_unit(u).filter(|q| q.kind == QKind::Mcq).count();
                    let fill = content.questions_in_unit(u).filter(|q| q.kind == QKind::Fill).count();
                    let cards = content.cards_in_unit(u).count();
                    println!("- U{} {} (CFU {}): {mcq} mcq, {fill} fill, {cards} flashcard", u.n, u.name, u.cfu);
                    for t in &u.topics {
                        let q = content.questions.iter().filter(|q| q.topic == t.id).count();
                        let c = content.cards.iter().filter(|c| c.topic == t.id).count();
                        let mark = if q == 0 { " ← SCOPERTO" } else { "" };
                        println!("    - {} {}: {q} domande, {c} flashcard{mark}", t.id, t.name);
                    }
                }
            }
        }
        _ => {
            let sessions = load_sessions(&root);
            let assessment: Option<Assessment> = fs::read_to_string(root.join("progress/assessment.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok());
            let h = build_history(&sessions);
            let p = compute(&content, &h, assessment.as_ref());
            if json {
                let flagged: Vec<_> = h.flagged.iter().collect();
                let wrong: Vec<_> = h.last_wrong.iter().collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "progress": p, "flagged": flagged, "last_wrong": wrong, "sessions": sessions.len()
                    }))
                    .unwrap()
                );
                return;
            }
            println!("# Report progressi\n");
            println!(
                "Sessioni: {} · ultima attività: {} · revisione AI: {}\n",
                sessions.len(),
                h.last_activity.map(|t| t.to_rfc3339()).unwrap_or("-".into()),
                assessment.as_ref().map(|a| a.generated_at.to_rfc3339()).unwrap_or("nessuna".into())
            );
            println!("**Totale: {}**\n", pct(p.overall));
            for s in &p.subjects {
                println!(
                    "## {} — {} · punteggio atteso {:.1}/31 · {} insicure\n",
                    s.name,
                    pct(s.mastery),
                    s.predicted,
                    s.flagged
                );
                println!("| Unità/argomento | mostrato | auto | AI | acc | cop | viste q/c | prove | insic. | err. | t medio | t totale |");
                println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
                for u in &s.units {
                    println!(
                        "| **U{} {}** (CFU {}) | **{}** | | | | | | | {} | | {} | {} |",
                        u.n,
                        u.name,
                        u.cfu,
                        pct(u.mastery),
                        u.flagged,
                        mmss(u.avg_secs),
                        hm(u.total_secs)
                    );
                    for t in &u.topics {
                        println!(
                            "| {} {} | {} | {} | {} | {} | {} | {}/{} · {}/{} | {} | {} | {} | {} | {} |",
                            t.id,
                            t.name,
                            pct(t.mastery),
                            pct(t.auto),
                            t.ai.map(pct).unwrap_or("-".into()),
                            pct(t.accuracy),
                            pct(t.coverage),
                            t.seen_questions,
                            t.questions,
                            t.seen_cards,
                            t.cards,
                            t.attempts,
                            t.flagged,
                            t.wrong,
                            mmss(t.avg_secs),
                            hm(t.total_secs)
                        );
                    }
                }
                println!();
            }
            let pace = exam_pace(content.syllabus().exam.minutes, content.syllabus().exam.questions);
            println!("## Tempi (ritmo d'esame: {} per domanda)\n", mmss(pace));
            let mut slow: Vec<_> = p
                .subjects
                .iter()
                .flat_map(|s| s.units.iter().flat_map(|u| u.topics.iter()))
                .filter(|t| t.timed >= 2)
                .collect();
            slow.sort_by(|a, b| b.avg_secs.partial_cmp(&a.avg_secs).unwrap());
            println!("- **Totale**: {} di studio cronometrato, media {} a domanda", hm(p.total_secs), mmss(p.avg_secs));
            for s in &p.subjects {
                println!(
                    "- {}: {} in totale, media {} a domanda su {} risposte",
                    s.name,
                    hm(s.total_secs),
                    mmss(s.avg_secs),
                    s.timed
                );
                for u in &s.units {
                    if u.total_secs > 0 {
                        println!("  - U{} {}: {} · media {}", u.n, u.name, hm(u.total_secs), mmss(u.avg_secs));
                    }
                }
            }
            println!("\nArgomenti più lenti:");
            for t in slow.iter().take(10) {
                let flag = if t.avg_secs > pace { " ⚠ oltre il ritmo d'esame" } else { "" };
                println!(
                    "- {} {}: {} ({} risposte, acc {}){flag}",
                    t.id,
                    t.name,
                    mmss(t.avg_secs),
                    t.timed,
                    pct(t.accuracy)
                );
            }
            println!();
            println!("## Domande segnalate come insicure\n");
            let mut fl: Vec<&String> = h.flagged.iter().collect();
            fl.sort();
            for id in fl {
                if let Some(q) = content.question(id) {
                    println!("- `{id}` ({}) {}", q.topic, q.text.replace('\n', " "));
                } else if let Some(c) = content.card(id) {
                    println!("- `{id}` (flashcard, {}) {}", c.topic, c.front.replace('\n', " "));
                } else {
                    println!("- `{id}` (non più nei contenuti)");
                }
            }
            println!("\n## Ultima risposta sbagliata/omessa\n");
            let mut wr: Vec<&String> = h.last_wrong.iter().collect();
            wr.sort();
            for id in wr {
                if let Some(q) = content.question(id) {
                    println!("- `{id}` ({}) {}", q.topic, q.text.replace('\n', " "));
                }
            }
            println!("\n## Spiegazioni scritte sulle flashcard (ultime 30)\n");
            let mut notes: Vec<(&Session, &prep_core::session::Item)> = sessions
                .iter()
                .flat_map(|s| s.items.iter().filter(|i| i.note.is_some()).map(move |i| (s, i)))
                .collect();
            notes.sort_by_key(|(s, _)| std::cmp::Reverse(s.ended_at));
            for (s, it) in notes.into_iter().take(30) {
                let card = content.card(&it.id);
                println!(
                    "- {} `{}` voto {}{}\n  - carta: {}\n  - risposta: {}\n  - spiegazione: {}",
                    s.ended_at.format("%Y-%m-%d"),
                    it.id,
                    it.grade.unwrap_or(0),
                    if it.unsure { " (insicura)" } else { "" },
                    card.map(|c| c.front.as_str()).unwrap_or("?"),
                    card.map(|c| c.back.as_str()).unwrap_or("?"),
                    it.note.as_deref().unwrap_or("")
                );
            }
            println!("\n## Simulazioni d'esame\n");
            for s in sessions.iter().filter(|s| s.kind == SessionKind::Exam) {
                if let Some(sc) = &s.score {
                    println!(
                        "- {} {}: {:.1} punti ({} ok / {} err / {} omesse, {} insicure)",
                        s.ended_at.format("%Y-%m-%d %H:%M"),
                        s.subject.clone().unwrap_or_default(),
                        sc.points,
                        sc.correct,
                        sc.wrong,
                        sc.omitted,
                        sc.unsure
                    );
                }
            }
        }
    }
}
