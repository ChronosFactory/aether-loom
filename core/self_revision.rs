//! T568 self-revision: the Core revises the settings of its own growth door by experiment.
//!
//! The growth door's search sizes, beams, candidate limits and doubt bound were constants an agent chose.
//! Here they are a record the Core owns (`Arch`). On an `Improve` request the Core
//!   1. makes exams from its own remembered inputs: each worked-example input with four or more pairs
//!      gives one item whose last pair is hidden and asked, so the truth is the user's own example;
//!   2. sits one fold of them and reads its own receipts to say which setting bound each miss;
//!   3. proposes one-step changes to the settings its evidence names;
//!   4. trials each proposal on a second fold against its current settings;
//!   5. sits the best one on a third fold no proposal was chosen on, and keeps it only if nothing it had
//!      right is lost, wrong answers do not rise and the cost stays inside the bound.
//! Then it diagnoses again under the settings it kept. Every trial is kept as a receipt, and a tally of
//! which kinds of change paid orders the next proposals.
//!
//! The acceptance rule, the folds and the ladders are fixed here and are not settings the Core can revise:
//! a learner that could change its own examiner could pass by weakening it. A kept change is the Core's
//! own verdict on its own exams. Joining it into the live Core still needs the outside battery.
//!
//! T569: besides settings, the Core proposes new pieces for its growth library. It studies each diagnosed
//! miss with a search one size larger and four times the limit than its door uses, keeps a found program only
//! if it predicts the hidden example, and proposes that program (and its constants-as-parameters form) as a
//! piece. A piece goes through the same test fold and regression fold as a setting.
use crate::brain::Brain;
use crate::capability_growth as cg;
use crate::curiosity::Datum;
use crate::typed_core as tc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The growth door's settings. The default is the door as it was before T568, value for value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arch {
    pub first_size: usize,
    pub first_beam: usize,
    pub doubt_bits: usize,
    pub doubt_limit: usize,
    pub bridge_size: usize,
    pub bridge_beam: usize,
    pub bridge_limit: usize,
    pub closing_size: usize,
    pub closing_beam: usize,
    pub closing_limit: usize,
    pub closing_classes: usize,
    /// T571: the grain of the cheapest-first search; 0 searches size by size, as before. Not written while 0,
    /// so a Core that has not chosen a grain keeps its bytes.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub search_grain: usize,
    /// T573: 1 prices a kept piece by the steps it holds where the second search judges rivals, 2 also where it
    /// searches; 0 prices every piece as one symbol, as before. Not written while 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub piece_steps: usize,
    /// T573: 1 gives working back from the target a second round that carries joined results forward; 0 is one
    /// round, as before. Not written while 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub closing_carry: usize,
    /// T573: 1 lets the growth door read text examples as sequences of letters; 0 leaves text to the other skills,
    /// as before. Not written while 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub text_letters: usize,
}
fn is_zero(n: &usize) -> bool {
    *n == 0
}
impl Default for Arch {
    fn default() -> Self {
        Arch { first_size: 4, first_beam: 64, doubt_bits: 8, doubt_limit: 40_000, bridge_size: 5, bridge_beam: 64, bridge_limit: 200_000,
            closing_size: 5, closing_beam: 64, closing_limit: 200_000, closing_classes: 0, search_grain: 0, piece_steps: 0, closing_carry: 0, text_letters: 0 }
    }
}

/// One setting and the values it may take. A proposal moves it one place along its ladder.
pub struct Knob {
    pub name: &'static str,
    pub ladder: &'static [usize],
    pub get: fn(&Arch) -> usize,
    pub set: fn(&mut Arch, usize),
}
pub const KNOBS: [Knob; 15] = [
    Knob { name: "text_letters", ladder: &[0, 1], get: |a| a.text_letters, set: |a, v| a.text_letters = v },
    Knob { name: "closing_carry", ladder: &[0, 1], get: |a| a.closing_carry, set: |a, v| a.closing_carry = v },
    Knob { name: "piece_steps", ladder: &[0, 1, 2], get: |a| a.piece_steps, set: |a, v| a.piece_steps = v },
    Knob { name: "search_grain", ladder: &[0, 2, 3, 4], get: |a| a.search_grain, set: |a, v| a.search_grain = v },
    Knob { name: "first_size", ladder: &[3, 4, 5], get: |a| a.first_size, set: |a, v| a.first_size = v },
    Knob { name: "first_beam", ladder: &[32, 64, 128], get: |a| a.first_beam, set: |a, v| a.first_beam = v },
    Knob { name: "doubt_bits", ladder: &[4, 6, 8, 10, 12], get: |a| a.doubt_bits, set: |a, v| a.doubt_bits = v },
    Knob { name: "doubt_limit", ladder: &[20_000, 40_000, 80_000], get: |a| a.doubt_limit, set: |a, v| a.doubt_limit = v },
    Knob { name: "bridge_size", ladder: &[4, 5, 6], get: |a| a.bridge_size, set: |a, v| a.bridge_size = v },
    Knob { name: "bridge_beam", ladder: &[32, 64, 128], get: |a| a.bridge_beam, set: |a, v| a.bridge_beam = v },
    Knob { name: "bridge_limit", ladder: &[100_000, 200_000, 400_000, 800_000], get: |a| a.bridge_limit, set: |a, v| a.bridge_limit = v },
    Knob { name: "closing_size", ladder: &[4, 5, 6], get: |a| a.closing_size, set: |a, v| a.closing_size = v },
    Knob { name: "closing_beam", ladder: &[32, 64, 128], get: |a| a.closing_beam, set: |a, v| a.closing_beam = v },
    Knob { name: "closing_limit", ladder: &[100_000, 200_000, 400_000], get: |a| a.closing_limit, set: |a, v| a.closing_limit = v },
    Knob { name: "closing_classes", ladder: &[0, 1], get: |a| a.closing_classes, set: |a, v| a.closing_classes = v },
];
fn knob(name: &str) -> Option<&'static Knob> {
    KNOBS.iter().find(|k| k.name == name)
}
/// The setting one place up or down its ladder; `None` at the end, or when the value is off the ladder.
fn stepped(a: &Arch, k: &Knob, up: bool) -> Option<usize> {
    let at = k.ladder.iter().position(|v| *v == (k.get)(a))?;
    if up { k.ladder.get(at + 1).copied() } else { at.checked_sub(1).map(|i| k.ladder[i]) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub right: usize,
    pub wrong: usize,
    pub declined: usize,
    pub ms: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    pub tried: u32,
    pub kept: u32,
}
/// One experiment on one proposed change, kept whatever its verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trial {
    pub generation: u32,
    pub knob: String,
    pub from: usize,
    pub to: usize,
    /// How many of the diagnosed misses named this change, and the first reason given.
    pub evidence: usize,
    pub reason: String,
    pub test_before: Score,
    pub test_after: Score,
    /// Items right before the change and not right after it, on the test fold.
    pub test_lost: usize,
    pub regression_before: Option<Score>,
    pub regression_after: Option<Score>,
    pub regression_lost: Option<usize>,
    pub verdict: String,
    /// T569: for a proposed library piece (`knob` is "piece"), its body as shown; `from` and `to` count learned pieces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece: Option<String>,
}
type Check = (Vec<tc::Value>, tc::Value);
/// One proposed change to myself: a setting moved one step, or a new piece for the growth library.
enum Change {
    Knob(&'static Knob, usize),
    Piece(cg::Abstraction, Vec<Check>, String),
}
struct Proposal {
    change: Change,
    evidence: usize,
    reason: String,
}
/// Most piece proposals trialled in one round.
pub const PIECES: usize = 4;
fn int_literals(t: &tc::Term, out: &mut Vec<i64>) {
    match t {
        tc::Term::Lit(tc::Value::Int(c)) => {
            if !out.contains(c) {
                out.push(*c);
            }
        }
        tc::Term::App(f, a) => {
            int_literals(f, out);
            a.iter().for_each(|x| int_literals(x, out));
        }
        tc::Term::Body(b) | tc::Term::Body2(b) => int_literals(b, out),
        _ => {}
    }
}
fn with_parameters(t: &tc::Term, lits: &[i64], first: usize) -> tc::Term {
    match t {
        tc::Term::Lit(tc::Value::Int(c)) => tc::Term::Var(first + lits.iter().position(|x| x == c).unwrap()),
        tc::Term::App(f, a) => tc::Term::App(Box::new(with_parameters(f, lits, first)), a.iter().map(|x| with_parameters(x, lits, first)).collect()),
        tc::Term::Body(b) => tc::Term::Body(Box::new(with_parameters(b, lits, first))),
        tc::Term::Body2(b) => tc::Term::Body2(Box::new(with_parameters(b, lits, first))),
        _ => t.clone(),
    }
}
/// The pieces one studied program proposes: itself with its whole numbers turned into parameters (when it
/// has one to three different ones), then itself as it is. Each comes with the examples it must replay.
fn pieces_of(term: &tc::Term, checks: &[Check], ins: &[tc::Ty], out: &tc::Ty) -> Vec<(cg::Abstraction, Vec<Check>)> {
    let mut v = Vec::new();
    let mut lits = Vec::new();
    int_literals(term, &mut lits);
    if !lits.is_empty() && lits.len() <= 3 {
        let args: Vec<tc::Ty> = ins.iter().cloned().chain(lits.iter().map(|_| tc::Ty::Int)).collect();
        let with: Vec<Check> = checks.iter().map(|(x, y)| (x.iter().cloned().chain(lits.iter().map(|c| tc::Value::Int(*c))).collect(), y.clone())).collect();
        v.push((cg::Abstraction { body: with_parameters(term, &lits, ins.len()), args, ret: out.clone() }, with));
    }
    v.push((cg::Abstraction { body: term.clone(), args: ins.to_vec(), ret: out.clone() }, checks.to_vec()));
    v
}
pub const TRIALS_KEPT: usize = 128;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub arch: Arch,
    /// How many changes were kept to reach `arch`.
    pub generation: u32,
    pub trials: Vec<Trial>,
    /// Per kind of change (`knob+` or `knob-`): how often it was tried and how often kept.
    pub ledger: BTreeMap<String, Tally>,
}
impl Memory {
    pub fn is_empty(&self) -> bool {
        *self == Memory::default()
    }
}

/// One exam item made from a remembered input: its last example hidden and asked.
pub struct Item {
    /// T569: the remembered question exactly as it was asked.
    pub asked: String,
    pub text: String,
    pub truth: Value,
    pub fold: u8,
}
fn shown(d: &Datum) -> String {
    match d {
        Datum::Number(n) => n.to_string(),
        Datum::List(v) => format!("[{}]", v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")),
        Datum::Text(s) => format!("\"{s}\""),
    }
}
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}
/// The exam item of one remembered input, when it is worked examples with at least four pairs and the hidden
/// pair's input is not among the shown ones. The item must read back as exactly the pairs it was made from.
pub fn item(input: &str) -> Option<Item> {
    let (pairs, _) = crate::core_route::examples(input)?;
    if pairs.len() < 4 {
        return None;
    }
    let (hidden, rest) = pairs.split_last()?;
    if rest.iter().any(|p| p.0 == hidden.0) {
        return None;
    }
    let mut text = rest.iter().map(|(a, b)| format!("{} -> {}", shown(a), shown(b))).collect::<Vec<_>>().join("; ");
    text.push_str(&format!("; {} -> ?", shown(&hidden.0)));
    let (back, q) = crate::core_route::examples(&text)?;
    if back != rest || q != hidden.0 {
        return None;
    }
    Some(Item { asked: input.to_string(), fold: (fnv(&text) % 3) as u8, truth: serde_json::to_value(&hidden.1).ok()?, text })
}
pub const EXAM_MOST: usize = 240;
pub const FOLD_LEAST: usize = 8;
pub const PROPOSALS: usize = 6;

/// The value a door reply committed to; `None` when it declined or left the question open.
pub fn given(reply: &Value) -> Option<Value> {
    let a = reply.get("answer")?;
    if a.is_null() || a.get("undetermined").is_some() {
        return None;
    }
    if a["checked"] == json!(true) {
        return a.get("output").filter(|o| !o.is_null()).cloned();
    }
    if a["solved"] == json!(true) {
        return a["next"].as_array().and_then(|n| n.first().cloned());
    }
    if a["status"] == json!("completed") {
        return a["output"]["Text"].get("text").cloned();
    }
    None
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Right,
    Wrong,
    Declined,
}

/// What a declined or wrong item's own growth receipt says bound it: `(setting, up, why)`.
pub fn limits(a: &Arch, outcome: Outcome, r: &Value) -> Vec<(&'static str, bool, &'static str)> {
    let mut out = Vec::new();
    if outcome == Outcome::Wrong {
        out.push(("doubt_bits", true, "a wrong answer passed my doubt: a rival further away may have caught it"));
        out.push(("doubt_limit", true, "a wrong answer passed my doubt: the rival search may have stopped too early"));
        return out;
    }
    if r.get("undetermined").is_some() {
        out.push(("doubt_bits", false, "my first search had an answer and a rival inside my doubt bound withheld it"));
    } else if r.get("program").is_some() && r.get("bridge").is_none() && r.get("closing").is_none() {
        out.push(("first_beam", true, "my first search's cheapest fit failed the held-back example"));
    }
    if let Some(b) = r.get("bridge") {
        match b["status"].as_str() {
            Some("no_fit") if b["enumerated"].as_u64().unwrap_or(0) as usize >= a.bridge_limit =>
                {
                    out.push(("bridge_limit", true, "my second search stopped at its candidate limit before anything fitted"));
                    out.push(("search_grain", true, "my second search spent its candidate limit size by size instead of trying my cheapest pieces first"));
                }
            Some("no_fit") => {
                out.push(("search_grain", true, "my second search went size by size and never used my prices to choose what to try first"));
                out.push(("bridge_size", true, "my second search tried every program up to its size and none fitted"));
                out.push(("bridge_beam", true, "my second search kept too few programs per size to reach a fit"));
            }
            Some("held_out_failed") => out.push(("bridge_beam", true, "my second search's cheapest fit failed the held-back example")),
            Some("rival_declined") => {
                out.push(("piece_steps", true, "a rival built from a longer kept piece cost no more than a short one and withheld my second search's answer"));
                out.push(("doubt_bits", false, "my second search had an answer and a rival inside my doubt bound withheld it"));
            }
            _ => {}
        }
    }
    if let Some(c) = r.get("closing") {
        match c["status"].as_str() {
            Some("no_fit") | Some("left_out_failed") | Some("held_out_failed") => {
                if c["enumerated"].as_u64().unwrap_or(0) as usize >= a.closing_limit {
                    out.push(("closing_limit", true, "working back from the target stopped at its candidate limit"));
                } else {
                    out.push(("closing_size", true, "working back from the target found no program within its size"));
                    out.push(("closing_beam", true, "working back from the target kept too few values per size"));
                }
                if a.closing_carry == 0 {
                    out.push(("closing_carry", true, "working back from the target stopped after one join and never carried a result forward"));
                }
                if a.closing_classes == 0 {
                    out.push(("closing_classes", true, "working back from the target never tried one-case programs as their own class"));
                }
            }
            Some("rival_declined") => out.push(("doubt_bits", false, "working back had an answer and a rival inside my doubt bound withheld it")),
            _ => {}
        }
    }
    out
}

struct Sat {
    score: Score,
    outcomes: Vec<Outcome>,
    /// T569: what the door answered to each question exactly as it was asked (no truth is known for these).
    asked: Vec<Option<Value>>,
}
/// Items that were right and no longer are, and questions as asked whose answer changed or vanished. A real
/// question's answer has no known truth, so any change to one counts as a loss.
fn lost(before: &Sat, after: &Sat) -> usize {
    (0..before.outcomes.len()).filter(|&i| (before.outcomes[i] == Outcome::Right && after.outcomes[i] != Outcome::Right)
        || (before.asked[i].is_some() && after.asked[i] != before.asked[i])).count()
}
/// The fixed test rule. A change that answers more must lose nothing, add no wrong answer and stay within
/// twice the time (plus a second); a change that answers the same must lose nothing and cost 15% less.
fn passes_test(before: &Sat, after: &Sat) -> Option<&'static str> {
    if lost(before, after) > 0 || after.score.wrong > before.score.wrong {
        return None;
    }
    if after.score.right > before.score.right && after.score.ms <= 2 * before.score.ms + 1000 {
        return Some("answers more");
    }
    if after.score.right == before.score.right && after.score.wrong == before.score.wrong && after.score.ms * 100 <= before.score.ms * 85 {
        return Some("costs less");
    }
    None
}
/// The fixed regression rule, on the fold no proposal was chosen on.
fn passes_regression(before: &Sat, after: &Sat, kind: &str) -> bool {
    if lost(before, after) > 0 || after.score.wrong > before.score.wrong || after.score.right < before.score.right {
        return false;
    }
    if kind == "costs less" { after.score.ms <= before.score.ms } else { after.score.ms <= 2 * before.score.ms + 1000 }
}

/// Make one proposed change on `b`. Returns the trial's name, the value before and after, and a piece's body.
fn apply(change: &Change, b: &mut Brain) -> Result<(String, usize, usize, Option<String>), String> {
    match change {
        Change::Knob(kn, to) => {
            let from = (kn.get)(&b.memory.self_revision.arch);
            (kn.set)(&mut b.memory.self_revision.arch, *to);
            Ok((kn.name.to_string(), from, *to, None))
        }
        Change::Piece(a, checks, shown) => {
            let g = b.memory.capability_growth.as_mut().ok_or("no growth library")?;
            let from = g.library.macros();
            g.admit(a.clone(), checks)?;
            Ok(("piece".to_string(), from, from + 1, Some(shown.clone())))
        }
    }
}

impl Brain {
    /// T569: a deeper look at one of my own misses: the growth search one size larger and with four times the
    /// limit my door's second search has, over the shown examples only. The program counts only if it also
    /// predicts the hidden example. Returns it with every example it replays and the task's types.
    fn study(&self, it: &Item) -> Option<(tc::Term, Vec<Check>, Vec<tc::Ty>, tc::Ty)> {
        use crate::core_connections::value;
        let g = self.memory.capability_growth.as_ref()?;
        let (pairs, q) = crate::core_route::examples(&it.text)?;
        let mut checks: Vec<Check> = pairs.iter().map(|(i, o)| Some((vec![value(i)?], value(o)?))).collect::<Option<_>>()?;
        let truth: Datum = serde_json::from_value(it.truth.clone()).ok()?;
        let hidden = (vec![value(&q)?], value(&truth)?);
        let lib = g.study_library();
        let task = tc::Task { in_types: checks[0].0.iter().map(|v| v.ty(&lib)).collect(), out_type: checks[0].1.ty(&lib), examples: checks.clone() };
        if checks.iter().chain([&hidden]).any(|(x, y)| x[0].ty(&lib) != task.in_types[0] || y.ty(&lib) != task.out_type) {
            return None;
        }
        let a = &self.memory.self_revision.arch;
        let (found, _) = tc::solve_priced(&lib, &task, (a.bridge_size + 1).min(8), a.bridge_beam, &lib.costs(), (a.bridge_limit * 4).min(3_200_000));
        let f = found?;
        if tc::eval(&lib, &f.term, &hidden.0).as_ref() != Some(&hidden.1) {
            return None;
        }
        checks.push(hidden);
        Some((f.term, checks, task.in_types, task.out_type))
    }
    fn sit(&self, root: &std::path::Path, items: &[&Item]) -> Sat {
        let t = std::time::Instant::now();
        let mut s = Sat { score: Score::default(), outcomes: Vec::with_capacity(items.len()), asked: Vec::with_capacity(items.len()) };
        for it in items {
            // T569: the question exactly as it was asked, then its hidden-example form, whose truth is known.
            // Right: answered as asked, and right on the hidden-example form. Wrong: wrong on that form.
            let asked = given(&self.route_input_read(root, &it.asked));
            let o = match given(&self.route_input_read(root, &it.text)) {
                Some(v) if v == it.truth => if asked.is_some() { Outcome::Right } else { Outcome::Declined },
                Some(_) => Outcome::Wrong,
                None => Outcome::Declined,
            };
            s.asked.push(asked);
            match o {
                Outcome::Right => s.score.right += 1,
                Outcome::Wrong => s.score.wrong += 1,
                Outcome::Declined => s.score.declined += 1,
            }
            s.outcomes.push(o);
        }
        s.score.ms = t.elapsed().as_millis() as u64;
        s
    }
    /// The exam this Core can make from what it remembers: declined inputs first, then its experience, newest first.
    pub fn self_exam(&self) -> Vec<Item> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        let complete = self.memory.experience.iter().rev().filter(|e| e.input.chars().count() < 256).map(|e| e.input.as_str());
        for input in self.memory.unsolved.iter().rev().map(String::as_str).chain(complete) {
            if out.len() >= EXAM_MOST {
                break;
            }
            if let Some(it) = item(input) {
                if seen.insert(it.text.clone()) {
                    out.push(it);
                }
            }
        }
        out
    }
    /// Up to `rounds` rounds of diagnose, propose, trial and regression. With `learn` the settings kept, the
    /// trials and the tally are stored; without it the same experiments run and nothing is stored.
    pub fn self_revise(&mut self, root: &std::path::Path, rounds: usize, learn: bool) -> Value {
        let items = self.self_exam();
        let fold = |f: u8| items.iter().filter(|i| i.fold == f).collect::<Vec<&Item>>();
        let (fa, fb, fc) = (fold(0), fold(1), fold(2));
        let mut report = json!({"exam": {"items": items.len(), "diagnosis_fold": fa.len(), "test_fold": fb.len(), "regression_fold": fc.len(),
            "made_from": "my own remembered questions, each sat exactly as asked and again with its last worked example hidden and asked; right means answered as asked and right on the hidden example; an answer as asked that changes counts as lost"},
            "settings_before": self.memory.self_revision.arch, "generation_before": self.memory.self_revision.generation, "rounds": []});
        if rounds == 0 {
            report["trials_kept"] = json!(self.memory.self_revision.trials);
            report["ledger"] = json!(self.memory.self_revision.ledger);
            return report;
        }
        if fa.len() < FOLD_LEAST || fb.len() < FOLD_LEAST || fc.len() < FOLD_LEAST {
            report["status"] = json!("not enough remembered worked examples to examine myself");
            return report;
        }
        let mut work = self.clone();
        let mut mem = self.memory.self_revision.clone();
        let (mut base_b, mut base_c) = (work.sit(root, &fb), work.sit(root, &fc));
        report["before"] = json!({"test_fold": base_b.score, "regression_fold": base_c.score});
        for _ in 0..rounds {
            let arch = mem.arch.clone();
            // 1. Diagnose: sit the diagnosis fold and read my own receipt for every miss.
            let diag = work.sit(root, &fa);
            let mut evidence: BTreeMap<(&'static str, bool), (usize, &'static str)> = BTreeMap::new();
            let (mut misses, mut unexplained) = (0, 0);
            for (it, o) in fa.iter().zip(&diag.outcomes) {
                if *o == Outcome::Right {
                    continue;
                }
                misses += 1;
                // T569: read the receipt for the question as asked, unless the miss is a wrong answer on the hidden-example form.
                let named = limits(&arch, *o, &work.growth_answer(if *o == Outcome::Wrong { &it.text } else { &it.asked }));
                if named.is_empty() {
                    unexplained += 1;
                }
                for (k, up, why) in named {
                    let e = evidence.entry((k, up)).or_insert((0, why));
                    e.0 += 1;
                }
            }
            // 2. Propose: one step along the ladder for every setting the evidence names; then, for settings
            // nothing named as too small, one step down, which can only pay by costing less.
            let weight = |k: &str, up: bool, n: usize| {
                let t = mem.ledger.get(&format!("{k}{}", if up { '+' } else { '-' })).copied().unwrap_or_default();
                n as f64 * (t.kept as f64 + 1.0) / (t.tried as f64 + 2.0)
            };
            let mut proposals: Vec<(f64, &'static Knob, bool, usize, usize, String)> = Vec::new();
            for ((k, up), (n, why)) in &evidence {
                let Some(kn) = knob(k) else { continue };
                if let Some(to) = stepped(&arch, kn, *up) {
                    proposals.push((weight(k, *up, *n), kn, *up, to, *n, format!("{why} ({n} of {misses} misses)")));
                }
            }
            proposals.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap().then(x.1.name.cmp(y.1.name)));
            proposals.truncate(PROPOSALS.saturating_sub(2));
            let mut cheaper: Vec<_> = KNOBS.iter().filter(|k| !evidence.contains_key(&(k.name, true)) && k.name != "doubt_bits" && k.name != "doubt_limit")
                .filter_map(|k| stepped(&arch, k, false).map(|to| (weight(k.name, false, 1), k, false, to, 0usize,
                    "none of my misses named this setting as too small: a smaller one may cost less".to_string()))).collect();
            cheaper.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap().then(x.1.name.cmp(y.1.name)));
            proposals.extend(cheaper.into_iter().take(PROPOSALS - proposals.len()));
            let mut proposals: Vec<Proposal> = proposals.into_iter().map(|(_, kn, _, to, n, why)| Proposal { change: Change::Knob(kn, to), evidence: n, reason: why }).collect();
            // T569: study each miss with a deeper search; what it finds and checks on the hidden example becomes a proposed piece.
            let mut found: Vec<(cg::Abstraction, Vec<Check>, usize)> = Vec::new();
            let mut studied = 0;
            for (it, o) in fa.iter().zip(&diag.outcomes) {
                if *o == Outcome::Right {
                    continue;
                }
                let Some((term, checks, ins, out)) = work.study(it) else { continue };
                studied += 1;
                for (a, c) in pieces_of(&term, &checks, &ins, &out) {
                    match found.iter_mut().find(|p| p.0.body == a.body && p.0.args == a.args) {
                        Some(p) => p.2 += 1,
                        None => found.push((a, c, 1)),
                    }
                }
            }
            found.sort_by(|x, y| y.2.cmp(&x.2));
            let shown_in = work.memory.capability_growth.as_ref().map(|g| g.library.clone());
            for (a, c, n) in found.into_iter().take(PIECES) {
                let shown = shown_in.as_ref().map_or(String::new(), |l| tc::show(l, &a.body));
                proposals.push(Proposal { reason: format!("a deeper search than my door's solved {n} of {misses} misses with this program, and it predicted the hidden example"),
                    evidence: n, change: Change::Piece(a, c, shown) });
            }
            let mut round = json!({"settings": arch, "diagnosis": {"sat": diag.score, "misses": misses, "misses_no_setting_explains": unexplained, "misses_solved_by_deeper_study": studied,
                "limits_named": evidence.iter().map(|((k, up), (n, why))| json!({"setting": k, "direction": if *up { "raise" } else { "lower" }, "misses": n, "why": why})).collect::<Vec<_>>()},
                "trials": []});
            // 3 and 4. Implement each proposal on a copy of myself in memory and sit the test fold.
            let mut best: Option<(usize, u64, usize, Sat, &'static str)> = None;
            let mut trials: Vec<Trial> = Vec::new();
            for (i, p) in proposals.iter().enumerate() {
                let mut trial = work.clone();
                let (name, from, to, piece) = match apply(&p.change, &mut trial) {
                    Ok(x) => x,
                    Err(e) => {
                        trials.push(Trial { generation: mem.generation, knob: "piece".into(), from: 0, to: 0, evidence: p.evidence, reason: p.reason.clone(),
                            test_before: base_b.score, test_after: base_b.score, test_lost: 0, regression_before: None, regression_after: None, regression_lost: None,
                            verdict: format!("not admitted: {e}"), piece: match &p.change { Change::Piece(_, _, s) => Some(s.clone()), _ => None } });
                        continue;
                    }
                };
                let after = trial.sit(root, &fb);
                let pass = passes_test(&base_b, &after);
                trials.push(Trial { generation: mem.generation, knob: name, from, to, evidence: p.evidence, reason: p.reason.clone(),
                    test_before: base_b.score, test_after: after.score, test_lost: lost(&base_b, &after), regression_before: None, regression_after: None,
                    regression_lost: None, verdict: match pass { Some(k) => format!("passed the test fold: {k}"), None => "rejected on the test fold".to_string() }, piece });
                if let Some(kind) = pass {
                    let better = best.as_ref().map_or(true, |b| after.score.right > b.0 || (after.score.right == b.0 && after.score.ms < b.1));
                    if better {
                        best = Some((after.score.right, after.score.ms, i, after, kind));
                    }
                }
            }
            // 5. Regression: only the best proposal looks at the third fold, once.
            let mut kept = false;
            if let Some((_, _, i, after_b, kind)) = best {
                let mut trial = work.clone();
                let _ = apply(&proposals[i].change, &mut trial);
                let after_c = trial.sit(root, &fc);
                let ok = passes_regression(&base_c, &after_c, kind);
                let t = &mut trials[i];
                t.regression_before = Some(base_c.score);
                t.regression_after = Some(after_c.score);
                t.regression_lost = Some(lost(&base_c, &after_c));
                t.verdict = if ok { format!("kept: {kind} on the test fold and passed the regression fold") } else { format!("rejected: {kind} on the test fold but failed the regression fold") };
                if ok {
                    mem.arch = trial.memory.self_revision.arch.clone();
                    mem.generation += 1;
                    work = trial;
                    base_b = after_b;
                    base_c = after_c;
                    kept = true;
                }
            }
            for t in &trials {
                let up = knob(&t.knob).map_or(true, |k| k.ladder.iter().position(|v| *v == t.to) > k.ladder.iter().position(|v| *v == t.from));
                let e = mem.ledger.entry(format!("{}{}", t.knob, if up { '+' } else { '-' })).or_default();
                e.tried += 1;
                if t.verdict.starts_with("kept") {
                    e.kept += 1;
                }
            }
            round["trials"] = json!(trials);
            round["kept"] = json!(kept);
            mem.trials.extend(trials);
            let n = mem.trials.len();
            if n > TRIALS_KEPT {
                mem.trials.drain(..n - TRIALS_KEPT);
            }
            // T569: each finished round goes to the log at once, so a stopped run still shows what it did.
            eprintln!("SELF-REVISION round {}", round);
            report["rounds"].as_array_mut().unwrap().push(round);
            if !kept {
                break;
            }
        }
        report["after"] = json!({"test_fold": base_b.score, "regression_fold": base_c.score});
        report["settings_after"] = json!(mem.arch);
        report["generation_after"] = json!(mem.generation);
        report["stored"] = json!(learn);
        report["standing"] = json!("my own verdict on my own exams; the outside battery must pass before this is joined into the live Core");
        if learn {
            self.memory.self_revision = mem;
            // T569: the pieces kept live in the growth library; nothing else of the working copy differs.
            self.memory.capability_growth = work.memory.capability_growth;
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_default_is_the_door_as_it_was() {
        let a = Arch::default();
        assert_eq!((a.first_size, a.first_beam, a.doubt_bits, a.doubt_limit), (4, 64, 8, 40_000));
        assert_eq!((a.bridge_size, a.bridge_beam, a.bridge_limit), (crate::core_connections::BRIDGE_SIZE, 64, crate::core_connections::BRIDGE_LIMIT));
        assert_eq!((a.closing_size, a.closing_beam, a.closing_classes), (crate::core_connections::CLOSING_SIZE, crate::core_connections::CLOSING_BEAM, 0));
        assert!(Memory::default().is_empty());
        for k in &KNOBS {
            assert!(k.ladder.contains(&(k.get)(&a)), "{} is off its ladder", k.name);
        }
    }
    #[test]
    fn an_item_hides_the_last_example() {
        let it = item("1 -> 2; 2 -> 4; 3 -> 6; [1, 2] -> 9; 5 -> 10; 7 -> ?").unwrap();
        assert_eq!(it.text, "1 -> 2; 2 -> 4; 3 -> 6; [1, 2] -> 9; 5 -> ?");
        assert_eq!(it.truth, json!(10));
        assert!(item("1 -> 2; 2 -> 4; 3 -> 6; 7 -> ?").is_none());
        assert!(item("1 -> 2; 2 -> 4; 3 -> 6; 1 -> 2; 7 -> ?").is_none());
    }
    #[test]
    fn a_step_stays_on_the_ladder() {
        let a = Arch::default();
        let k = knob("bridge_limit").unwrap();
        assert_eq!(stepped(&a, k, true), Some(400_000));
        assert_eq!(stepped(&a, k, false), Some(100_000));
        assert_eq!(stepped(&a, knob("closing_classes").unwrap(), false), None);
    }
    #[test]
    fn the_rules_never_trade_answers() {
        let sat = |o: Vec<Outcome>, ms| Sat { score: Score { right: o.iter().filter(|x| **x == Outcome::Right).count(), wrong: o.iter().filter(|x| **x == Outcome::Wrong).count(), declined: 0, ms }, asked: o.iter().map(|x| (*x == Outcome::Right).then(|| json!(1))).collect(), outcomes: o };
        use Outcome::*;
        let before = sat(vec![Right, Declined, Declined], 1000);
        assert_eq!(passes_test(&before, &sat(vec![Right, Right, Declined], 1500)), Some("answers more"));
        assert_eq!(passes_test(&before, &sat(vec![Declined, Right, Right], 1000)), None);
        assert_eq!(passes_test(&before, &sat(vec![Right, Right, Wrong], 1000)), None);
        assert_eq!(passes_test(&before, &sat(vec![Right, Declined, Declined], 800)), Some("costs less"));
        assert_eq!(passes_test(&before, &sat(vec![Right, Declined, Declined], 900)), None);
        assert!(!passes_regression(&before, &sat(vec![Declined, Right, Right], 1000), "answers more"));
        // T569: a question as asked whose answer changes is a loss, even when every hidden-example outcome holds.
        let mut changed = sat(vec![Right, Right, Declined], 1000);
        changed.asked[0] = Some(json!(2));
        assert_eq!(passes_test(&before, &changed), None);
    }
}
