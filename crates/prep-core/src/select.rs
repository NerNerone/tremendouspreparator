//! Scelta delle domande per allenamento, ripasso e simulazione d'esame.

use crate::content::Content;
use crate::model::{ExamRules, QKind, Question};
use crate::progress::{strength, History};
use crate::srs;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum Scope {
    All,
    Subject(String),
    Unit(String),
    Topic(String),
}

impl Scope {
    pub fn matches(&self, content: &Content, topic: &str) -> bool {
        match self {
            Scope::All => true,
            Scope::Topic(t) => t == topic,
            Scope::Unit(u) => content.unit_of_topic(topic).is_some_and(|x| &x.id == u),
            Scope::Subject(s) => content.subject_of_topic(topic).is_some_and(|x| &x.id == s),
        }
    }
}

fn priority(h: &History, id: &str) -> f64 {
    let jitter = fastrand::f64() * 0.5;
    if h.flagged.contains(id) {
        return jitter;
    }
    if h.last_wrong.contains(id) {
        return 1.0 + jitter;
    }
    match h.questions.get(id) {
        None => 2.0 + jitter,
        Some(a) => 3.0 + strength(a) * 2.0 + jitter,
    }
}

/// Allenamento: prima insicure, poi sbagliate, poi mai viste, poi le più deboli.
pub fn practice(content: &Content, h: &History, scope: &Scope, n: usize) -> Vec<String> {
    practice_where(content, h, |q| scope.matches(content, &q.topic), n)
}

/// Come [`practice`], ma con un filtro qualsiasi (quiz personalizzato: materie, unità, fonti, tipo).
pub fn practice_where(content: &Content, h: &History, keep: impl Fn(&Question) -> bool, n: usize) -> Vec<String> {
    let mut pool: Vec<(f64, &Question)> =
        content.questions.iter().filter(|q| keep(q)).map(|q| (priority(h, &q.id), q)).collect();
    pool.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut out: Vec<String> = pool.into_iter().take(n).map(|(_, q)| q.id.clone()).collect();
    fastrand::shuffle(&mut out);
    out
}

/// Ripasso distanziato di un argomento: circa 2/3 domande già viste (prima insicure,
/// sbagliate e più deboli) e il resto mai viste, mescolate.
pub fn spaced(content: &Content, h: &History, topic: &str, n: usize) -> Vec<String> {
    let mut old: Vec<(f64, &Question)> = Vec::new();
    let mut new: Vec<&Question> = Vec::new();
    for q in content.questions.iter().filter(|q| q.topic == topic) {
        if h.questions.contains_key(&q.id) || h.flagged.contains(&q.id) {
            old.push((priority(h, &q.id), q));
        } else {
            new.push(q);
        }
    }
    old.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    fastrand::shuffle(&mut new);
    let want_old = (n * 2).div_ceil(3).min(old.len());
    let mut out: Vec<String> = old.iter().take(want_old).map(|(_, q)| q.id.clone()).collect();
    out.extend(new.iter().take(n - out.len()).map(|q| q.id.clone()));
    // se mancano domande nuove, completa con altre già viste
    for (_, q) in old.iter().skip(want_old) {
        if out.len() >= n {
            break;
        }
        out.push(q.id.clone());
    }
    fastrand::shuffle(&mut out);
    out
}

/// Ripasso: solo domande segnalate come insicure o sbagliate l'ultima volta.
pub fn review(content: &Content, h: &History, scope: &Scope, n: usize) -> Vec<String> {
    let mut pool: Vec<(f64, &Question)> = content
        .questions
        .iter()
        .filter(|q| scope.matches(content, &q.topic))
        .filter(|q| h.flagged.contains(&q.id) || h.last_wrong.contains(&q.id))
        .map(|q| (priority(h, &q.id), q))
        .collect();
    pool.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    pool.into_iter().take(n).map(|(_, q)| q.id.clone()).collect()
}

/// ripartizione col metodo dei resti più grandi
fn apportion(weights: &[f64], total: usize) -> Vec<usize> {
    let sum: f64 = weights.iter().sum();
    if sum <= 0.0 || weights.is_empty() {
        return vec![0; weights.len()];
    }
    let exact: Vec<f64> = weights.iter().map(|w| w / sum * total as f64).collect();
    let mut base: Vec<usize> = exact.iter().map(|x| x.floor() as usize).collect();
    let mut rest = total - base.iter().sum::<usize>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|&a, &b| (exact[b] - exact[b].floor()).partial_cmp(&(exact[a] - exact[a].floor())).unwrap());
    for i in order {
        if rest == 0 {
            break;
        }
        base[i] += 1;
        rest -= 1;
    }
    base
}

fn pick_kind(content: &Content, subject: &str, rules_n: usize, kind: QKind) -> Vec<String> {
    let Some(sub) = content.subject(subject) else {
        return vec![];
    };
    let weights: Vec<f64> = sub.units.iter().map(|u| u.cfu).collect();
    let quotas = apportion(&weights, rules_n);
    let mut chosen: Vec<String> = Vec::new();
    let mut leftovers: Vec<String> = Vec::new();
    for (unit, quota) in sub.units.iter().zip(quotas) {
        let mut pool: Vec<String> =
            content.questions_in_unit(unit).filter(|q| q.kind == kind).map(|q| q.id.clone()).collect();
        fastrand::shuffle(&mut pool);
        let take = quota.min(pool.len());
        chosen.extend(pool.drain(..take));
        leftovers.extend(pool);
    }
    fastrand::shuffle(&mut leftovers);
    while chosen.len() < rules_n {
        match leftovers.pop() {
            Some(x) => chosen.push(x),
            None => break,
        }
    }
    // ordine del syllabus (unità crescente), come in un compito reale
    chosen.sort_by_key(|id| {
        content.question(id).and_then(|q| content.unit_of_topic(&q.topic)).map(|u| u.n).unwrap_or(99)
    });
    chosen
}

/// Simulazione: 21 risposta multipla + 10 completamento, ripartite per CFU.
pub fn exam(content: &Content, subject: &str, rules: &ExamRules) -> Vec<String> {
    let mut v = pick_kind(content, subject, rules.mcq, QKind::Mcq);
    v.extend(pick_kind(content, subject, rules.fill, QKind::Fill));
    v
}

/// Flashcard: prima quelle scadute (le più in ritardo), poi nuove.
pub fn flashcards(
    content: &Content,
    h: &History,
    scope: &Scope,
    now: DateTime<Utc>,
    max: usize,
    max_new: usize,
) -> Vec<String> {
    let mut due: Vec<(DateTime<Utc>, String)> = Vec::new();
    let mut new: Vec<String> = Vec::new();
    for c in content.cards.iter().filter(|c| scope.matches(content, &c.topic)) {
        match h.cards.get(&c.id).and_then(|r| srs::replay(r)) {
            Some(st) if st.due <= now => due.push((st.due, c.id.clone())),
            Some(_) => {}
            None => new.push(c.id.clone()),
        }
    }
    due.sort();
    let mut out: Vec<String> = due.into_iter().map(|(_, id)| id).take(max).collect();
    let room = max.saturating_sub(out.len()).min(max_new);
    out.extend(new.into_iter().take(room));
    out
}

/// Carte già studiate e non ancora in scadenza («in ripasso»): restano ripassabili quando si
/// vuole, in ordine di scadenza (prima quelle che tornerebbero prima).
pub fn learned_cards(content: &Content, h: &History, scope: &Scope, now: DateTime<Utc>, max: usize) -> Vec<String> {
    let mut v: Vec<(DateTime<Utc>, String)> = content
        .cards
        .iter()
        .filter(|c| scope.matches(content, &c.topic))
        .filter_map(|c| {
            h.cards.get(&c.id).and_then(|r| srs::replay(r)).filter(|st| st.due > now).map(|st| (st.due, c.id.clone()))
        })
        .collect();
    v.sort();
    v.into_iter().map(|(_, id)| id).take(max).collect()
}

pub fn count_learned(content: &Content, h: &History, scope: &Scope, now: DateTime<Utc>) -> usize {
    learned_cards(content, h, scope, now, usize::MAX).len()
}

pub fn count_due(content: &Content, h: &History, scope: &Scope, now: DateTime<Utc>) -> (usize, usize) {
    let mut due = 0;
    let mut new = 0;
    for c in content.cards.iter().filter(|c| scope.matches(content, &c.topic)) {
        match h.cards.get(&c.id).and_then(|r| srs::replay(r)) {
            Some(st) if st.due <= now => due += 1,
            Some(_) => {}
            None => new += 1,
        }
    }
    (due, new)
}

#[cfg(test)]
mod tests {
    use super::apportion;

    #[test]
    fn apportion_cfu() {
        // Fisica: 0.2 1.4 1.2 0.4 1.0 1.2 0.6 su 21 domande
        let q = apportion(&[0.2, 1.4, 1.2, 0.4, 1.0, 1.2, 0.6], 21);
        assert_eq!(q.iter().sum::<usize>(), 21);
        assert_eq!(q[1], 5);
    }
}
