//! Learning one world: advisers, invention under a thinking budget, and
//! Bayesian model averaging.
//!
//! An adviser for a target channel is a view plus a count table: the view's
//! symbol is the context, the table counts what the channel did next in that
//! context. Its weight is two-part code length: the bits to state the view (as
//! priced by the brain's library, or by the grammar when it is new) plus the
//! bits it needed to predict the channel prequentially over the whole history.
//! The mixture is the weighted average of advisers, so no lucky adviser is
//! "followed"; a wrong one loses weight exponentially in its own errors.
//!
//! Search proposes candidate views from a list the brain supplies: library
//! proposals first, then the grammar's enumeration. Each step the learner may
//! evaluate at most `budget` candidates per channel, and it thinks only while
//! it is not yet reliable (it was wrong recently or has not mastered the
//! world). Each evaluation replays the candidate over the full history.
use crate::trust::HarmLedger;
use crate::view::{Frame, View};
use crate::world::{Spec, Step};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use crate::prior::Enumeration;

/// KT smoothing: one half pseudo-count per symbol.
pub const ALPHA: f64 = 0.5;
/// Advisers kept per channel (besides the marginal one).
pub const POOL: usize = 32;
/// Consecutive all-correct predictions that count as mastery.
pub const STREAK: usize = 20;
/// Local forecasts an inherited adviser must make before it votes (probation).
pub const PROBATION: usize = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Origin {
    /// Found by the grammar's enumeration at this position.
    Grammar(usize),
    /// Proposed by library entry `id`.
    Library(usize),
    /// An episodic memory of a world the brain lived in.
    Recall(String),
    /// The context-free marginal.
    Marginal,
    /// Invented to explain what nothing visible did: a view reading a hidden bit (T287).
    Latent,
}
impl Origin {
    pub fn inherited(&self) -> bool {
        matches!(self, Origin::Library(_) | Origin::Recall(_))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Adviser {
    pub view: Option<View>,
    pub origin: Origin,
    pub counts: BTreeMap<u32, Vec<u32>>,
    /// Bits to state the adviser (its definition, priced by its origin).
    pub prior_bits: f64,
    /// Prequential code length over the whole history so far.
    pub local_bits: f64,
    pub local_n: usize,
    pub modulus: u32,
    /// Harm relative to this world's own advisers (inherited advisers only).
    #[serde(default)]
    pub harm: HarmLedger,
}
impl Adviser {
    pub fn new(view: Option<View>, origin: Origin, prior_bits: f64, modulus: u32) -> Self {
        Adviser { view, origin, counts: BTreeMap::new(), prior_bits, local_bits: 0.0, local_n: 0, modulus, harm: HarmLedger::default() }
    }
    pub fn context(&self, f: &Frame) -> Option<u32> {
        match &self.view {
            None => Some(0),
            Some(v) => v.eval(f).map(|(x, _)| x),
        }
    }
    pub fn dist(&self, ctx: Option<u32>) -> Vec<f64> {
        let m = self.modulus as usize;
        let row = ctx.and_then(|c| self.counts.get(&c));
        match row {
            None => vec![1.0 / m as f64; m],
            Some(r) => {
                let total: u32 = r.iter().sum();
                let d = total as f64 + ALPHA * m as f64;
                r.iter().map(|&c| (c as f64 + ALPHA) / d).collect()
            }
        }
    }
    pub fn learn(&mut self, ctx: Option<u32>, y: u32) {
        if let Some(c) = ctx {
            let m = self.modulus as usize;
            self.counts.entry(c).or_insert_with(|| vec![0; m])[y as usize] += 1;
        }
    }
    pub fn score(&self) -> f64 {
        self.prior_bits + self.local_bits
    }
    /// Replay this adviser over a whole history from empty counts. `starts`
    /// marks where later episodes begin: no view looks back across one.
    pub fn replay(&mut self, spec: &Spec, hist: &[Step], ch: usize, starts: &[usize]) {
        self.counts.clear();
        self.local_bits = 0.0;
        self.local_n = 0;
        for t in 0..hist.len() {
            let s0 = starts.iter().copied().filter(|&s| s <= t).max().unwrap_or(0);
            let f = Frame { spec, hist: &hist[s0..t], action: hist[t].action, now: &hist[t].obs };
            let ctx = self.context(&f);
            let y = hist[t].obs[ch];
            let p = self.dist(ctx)[y as usize];
            self.local_bits -= p.log2();
            self.local_n += 1;
            self.learn(ctx, y);
        }
    }
}

/// log2 of small integers, looked up: every prequential code length under KT
/// smoothing is `log2(2T + m) - log2(2c + 1)` for counts c of total T.
fn lg(n: u32) -> f64 {
    static TABLE: std::sync::OnceLock<Vec<f64>> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| (0..1u32 << 16).map(|i| (i as f64).log2()).collect());
    t.get(n as usize).copied().unwrap_or_else(|| (n as f64).log2())
}

/// Score a candidate view over the whole history with dense counts. Returns
/// None as soon as its code length passes `bound` (code length only grows,
/// so it could no longer be adopted: branch and bound). On success returns
/// the code length and the count table.
pub fn score_view(view: &View, spec: &Spec, hist: &[Step], ch: usize, starts: &[usize], m: u32, bound: f64) -> (Option<(f64, BTreeMap<u32, Vec<u32>>)>, usize) {
    let mu = m as usize;
    let mut counts: Vec<u32> = Vec::new();
    let mut totals: Vec<u32> = Vec::new();
    let mut bits = 0.0;
    let mut s0 = 0;
    let mut si = 0;
    let compiled = view.compile(spec);
    for t in 0..hist.len() {
        while si < starts.len() && starts[si] <= t {
            s0 = starts[si];
            si += 1;
        }
        let y = hist[t].obs[ch] as usize;
        let x = match &compiled {
            Some(c) => c.eval(hist, s0, t),
            None => view.eval(&Frame { spec, hist: &hist[s0..t], action: hist[t].action, now: &hist[t].obs }).map(|(x, _)| x),
        };
        match x {
            Some(x) => {
                let x = x as usize;
                if totals.len() <= x {
                    totals.resize(x + 1, 0);
                    counts.resize((x + 1) * mu, 0);
                }
                bits += lg(2 * totals[x] + m) - lg(2 * counts[x * mu + y] + 1);
                counts[x * mu + y] += 1;
                totals[x] += 1;
            }
            None => bits += lg(m),
        }
        if bits > bound {
            return (None, t + 1);
        }
    }
    let table = totals.iter().enumerate().filter(|(_, &n)| n > 0).map(|(x, _)| (x as u32, counts[x * mu..(x + 1) * mu].to_vec())).collect();
    (Some((bits, table)), hist.len())
}

/// A proposal the learner may evaluate: a view and what proposed it.
#[derive(Clone, Debug)]
pub struct Proposal {
    pub view: View,
    pub origin: Origin,
    pub prior_bits: f64,
}

/// Proposals produced on demand: the library's (sorted by code length) merged
/// with the grammar's lazy enumeration, shortest first, without duplicates.
#[derive(Clone, Debug, Default)]
pub struct ProposalStream {
    pub library: VecDeque<Proposal>,
    pub grammar: Option<Enumeration>,
    lookahead: Option<(View, f64)>,
    /// Grammar views produced so far, and the most it may produce.
    pub emitted: usize,
    pub cap: usize,
    seen: crate::numbers::KeySet<View>,
    /// Where each grammar view came in the grammar's order (its search cost).
    pub positions: crate::numbers::KeyMap<View, usize>,
}

impl ProposalStream {
    pub fn new(mut library: Vec<Proposal>, grammar: Option<Enumeration>, cap: usize) -> Self {
        library.sort_by(|a, b| a.prior_bits.total_cmp(&b.prior_bits));
        ProposalStream { library: library.into(), grammar, cap, ..Default::default() }
    }
    pub fn from_list(list: Vec<Proposal>) -> Self {
        Self::new(list, None, 0)
    }
    pub fn next(&mut self) -> Option<Proposal> {
        loop {
            if self.lookahead.is_none() && self.emitted < self.cap {
                self.lookahead = self.grammar.as_mut().and_then(|g| g.next());
            }
            let take_library = match (self.library.front(), &self.lookahead) {
                (None, None) => return None,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (Some(l), Some(g)) => l.prior_bits <= g.1,
            };
            let p = if take_library {
                self.library.pop_front().unwrap()
            } else {
                let (v, bits) = self.lookahead.take().unwrap();
                let i = self.emitted;
                self.emitted += 1;
                self.positions.entry(v.clone()).or_insert(i);
                Proposal { view: v, origin: Origin::Grammar(i), prior_bits: bits }
            };
            if self.seen.insert(p.view.clone()) {
                return Some(p);
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Work {
    pub candidates: u64,
    pub replay_steps: u64,
    pub library_candidates: u64,
    pub library_replay: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelModel {
    pub ch: usize,
    pub marginal: Adviser,
    pub advisers: Vec<Adviser>,
    #[serde(skip)]
    pub proposals: Vec<Proposal>,
    #[serde(skip)]
    pub stream: ProposalStream,
    pub cursor: usize,
    #[serde(skip)]
    pub tried: crate::numbers::KeySet<View>,
    pub harm: HarmLedger,
    pub last_wrong: bool,
    /// Evaluations spent on each library entry's proposals in this world.
    pub spent: BTreeMap<usize, u64>,
    /// How many times hidden state has been proposed for this channel (T287).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub latent_rounds: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Observations after which, if a channel is still wrong, hidden state is proposed (T287). Each later
/// round re-offers the same views with about half as much evidence again.
pub const LATENT_AT: [usize; 5] = [24, 48, 96, 192, 384];

/// Hidden bits screened per round, then tried with every base (T287).
pub const LATENT_EXPAND: usize = 3;

/// What hidden state may be proposed for predicting `target`: every toggle and latch the world's
/// interface allows (sources with at most `LATENT_CARD` values), and the bases a bit is paired with
/// (none, the action now, each input now, each channel one step back, the channel's leading view).
pub fn latent_parts(spec: &Spec, target: usize, leader: Option<&View>) -> (Vec<View>, Vec<Option<View>>) {
    const LATENT_CARD: u32 = 4;
    let mut hidden = Vec::new();
    for src in 0..=spec.channels.len() {
        let card = if src == 0 { if spec.actions < 2 { continue } else { spec.actions as u32 } } else { spec.channels[src - 1].modulus };
        if card > LATENT_CARD {
            continue;
        }
        for a in 0..card as u8 {
            for b in 0..card as u8 {
                // A latch over a two-value source is that source one step back: visible, not hidden.
                if a != b && card == 2 {
                    continue;
                }
                hidden.push(View::Hidden(src as u8, a, b));
            }
        }
        // T290: counts that toggles and latches cannot say: a bit flipped by two or more values (mod 2),
        // and counters (mod 3) over one or more values. Never every value (that is a clock).
        let full = (1u32 << card) - 1;
        for mask in 1..full {
            for m in 2..=3u8 {
                if m == 2 && mask.count_ones() == 1 {
                    continue;
                }
                hidden.push(View::Count(src as u8, mask as u8, m));
            }
        }
    }
    let mut bases: Vec<Option<View>> = vec![None];
    if spec.actions > 1 {
        bases.push(Some(View::Act(0)));
    }
    for c in 0..spec.channels.len() {
        if spec.channels[c].input && c != target {
            bases.push(Some(View::Obs(c as u8, 0)));
        }
        bases.push(Some(View::Obs(c as u8, 1)));
    }
    // A leader that reads one hidden leaf may get a second beside it (T290: two hidden bits); never a
    // third, and never a leaf on itself (`latent_view`).
    if let Some(v) = leader.filter(|v| v.leaves().iter().filter(|l| matches!(l, View::Hidden(..) | View::Count(..))).count() <= 1) {
        if !bases.iter().any(|b| b.as_ref() == Some(v)) {
            bases.push(Some(v.clone()));
        }
    }
    (hidden, bases)
}

/// A hidden bit on a base, or None where it cannot be read for `target`.
fn latent_view(spec: &Spec, target: usize, h: &View, base: &Option<View>) -> Option<View> {
    if base.as_ref().map_or(false, |b| b.leaves().contains(h)) {
        return None;
    }
    let view = match base {
        None => h.clone(),
        Some(b) => View::Pair(Box::new(h.clone()), Box::new(b.clone())),
    };
    (view.fits(spec, target) && crate::world::view_card(&view, spec) <= crate::view::MAX_CARD).then_some(view)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// Candidate evaluations per channel per step.
    pub budget: usize,
    /// Use the harm ledger and probation for inherited advisers.
    pub trust: bool,
    /// Specialists (T285, after OneLife, arXiv:2510.12088), 0 = off: an adviser votes only in contexts it
    /// has seen at least this many times (a law votes only where it applies), and is not charged for the steps
    /// it sat out (sleeping experts: it is charged what the mixture lost).
    #[serde(default, skip_serializing_if = "crate::learner::off")]
    pub specialists: u32,
    /// Latent state (T287, after AutumnSynth, POPL 2023), off by default: when a channel is still being
    /// predicted wrong after `LATENT_AT` observations, propose views that read a hidden bit (`View::Hidden`)
    /// the world may be keeping, alone and paired with what the channel already uses.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub latent: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Settings { budget: 12, trust: true, specialists: 0, latent: false }
    }
}

impl ChannelModel {
    pub fn new(spec: &Spec, ch: usize, stream: ProposalStream) -> Self {
        let m = spec.channels[ch].modulus;
        ChannelModel {
            ch,
            marginal: Adviser::new(None, Origin::Marginal, 1.0, m),
            advisers: Vec::new(),
            proposals: Vec::new(),
            stream,
            cursor: 0,
            tried: Default::default(),
            harm: HarmLedger::default(),
            last_wrong: true,
            spent: BTreeMap::new(),
            latent_rounds: 0,
        }
    }

    /// Whether `a` speaks at `f` under specialists: it has seen this context `wake` times.
    fn awake(a: &Adviser, f: &Frame, wake: u32) -> bool {
        a.view.is_none() || a.context(f).and_then(|c| a.counts.get(&c)).map_or(false, |r| r.iter().sum::<u32>() >= wake)
    }

    fn voting(&self, f: &Frame, s: &Settings) -> Vec<&Adviser> {
        let (trust, specialists) = (s.trust, s.specialists);
        let mut out = vec![&self.marginal];
        for a in &self.advisers {
            if specialists > 0 && !Self::awake(a, f, specialists) {
                continue;
            }
            if a.origin.inherited() && trust {
                // Muted individually: its own excess loss over this world's own
                // advisers raised its alarm, or it is still on probation.
                if a.harm.alarmed() || a.local_n < PROBATION {
                    continue;
                }
            }
            out.push(a);
        }
        out
    }

    fn mix(advisers: &[&Adviser], f: &Frame, m: usize) -> Vec<f64> {
        let best = advisers.iter().map(|a| a.score()).fold(f64::INFINITY, f64::min);
        let mut p = vec![0.0; m];
        let mut z = 0.0;
        for a in advisers {
            let w = (-(a.score() - best)).exp2();
            if w < 1e-12 {
                continue;
            }
            let d = a.dist(a.context(f));
            for (pi, di) in p.iter_mut().zip(&d) {
                *pi += w * di;
            }
            z += w;
        }
        p.iter_mut().for_each(|x| *x /= z);
        p
    }

    /// Each voting adviser's normalized weight and forecast.
    pub fn weighted(&self, f: &Frame, s: &Settings) -> Vec<(f64, Vec<f64>)> {
        let voters = self.voting(f, s);
        let best = voters.iter().map(|a| a.score()).fold(f64::INFINITY, f64::min);
        let mut out: Vec<(f64, Vec<f64>)> =
            voters.iter().map(|a| ((-(a.score() - best)).exp2(), a.dist(a.context(f)))).filter(|(w, _)| *w > 1e-9).collect();
        let z: f64 = out.iter().map(|x| x.0).sum();
        out.iter_mut().for_each(|x| x.0 /= z);
        out
    }

    /// The model-averaged forecast for this channel.
    pub fn predict(&self, f: &Frame, s: &Settings) -> Vec<f64> {
        let m = self.marginal.modulus as usize;
        Self::mix(&self.voting(f, s), f, m)
    }

    /// Forecast from the advisers that owe nothing to experience.
    pub fn predict_own(&self, f: &Frame) -> Vec<f64> {
        let m = self.marginal.modulus as usize;
        let own: Vec<&Adviser> =
            std::iter::once(&self.marginal).chain(self.advisers.iter().filter(|a| !a.origin.inherited())).collect();
        Self::mix(&own, f, m)
    }

    /// Score the outcome, update every adviser, and the harm ledger.
    pub fn observe(&mut self, f: &Frame, y: u32, s: &Settings) -> (bool, f64) {
        let p = self.predict(f, s);
        let guess = argmax(&p);
        let right = guess == y as usize;
        let own_loss = if s.trust {
            let base = self.predict_own(f);
            let own = -base[y as usize].max(1e-12).log2();
            self.harm.record(-p[y as usize].max(1e-12).log2(), own);
            own
        } else {
            0.0
        };
        for a in std::iter::once(&mut self.marginal).chain(self.advisers.iter_mut()) {
            let ctx = a.context(f);
            // Asleep, it is charged what the mixture lost: its weight moves only on steps it spoke for.
            let asleep = s.specialists > 0 && a.view.is_some() && !ctx.and_then(|c| a.counts.get(&c)).map_or(false, |r| r.iter().sum::<u32>() >= s.specialists);
            let q = if asleep { p[y as usize] } else { a.dist(ctx)[y as usize] };
            // Judged only on forecasts it actually made: a context it has seen.
            // (Charging unseen contexts muted correct but data-hungry views.)
            if s.trust && a.origin.inherited() && ctx.map_or(false, |c| a.counts.contains_key(&c)) {
                a.harm.record(-q.max(1e-12).log2(), own_loss);
            }
            a.local_bits -= q.log2();
            a.local_n += 1;
            a.learn(ctx, y);
        }
        self.last_wrong = !right;
        (right, p[y as usize])
    }

    /// Put hidden-state views next in line when the channel is still wrong at a `LATENT_AT` point (T287).
    /// Views already tried are tried again: they now have more evidence.
    ///
    /// A phase of its own, as in AutumnSynth: every hidden-state view is scored at once against the whole
    /// history and adopted by the same rule as any proposal (it must beat the marginal, and the pool's
    /// worst when the pool is full). The ordinary search's queue, cursor and tried set are not touched, so
    /// a world in which no hidden view wins is searched exactly as before.
    pub fn invent_latent(&mut self, spec: &Spec, hist: &[Step], starts: &[usize], work: &mut Work) {
        let n = hist.len();
        if !self.last_wrong || self.latent_rounds >= LATENT_AT.len() || n < LATENT_AT[self.latent_rounds] {
            return;
        }
        self.latent_rounds += 1;
        let leader = self.leader().view.clone();
        let (hidden, bases) = latent_parts(spec, self.ch, leader.as_ref());
        // Screen: each bit beside what already explains the channel best (its leader when that reads no
        // hidden bit; else the action now; else the bit alone), as AutumnSynth asks which hidden variable
        // explains what the visible program gets wrong.
        let lead: Option<View> = match leader.filter(|v| v.leaves().iter().filter(|l| matches!(l, View::Hidden(..) | View::Count(..))).count() <= 1) {
            Some(v) => Some(v),
            None if spec.actions > 1 => Some(View::Act(0)),
            None => None,
        };
        let mut ranked: Vec<(f64, usize)> = Vec::new();
        for (i, h) in hidden.iter().enumerate() {
            if let Some(view) = latent_view(spec, self.ch, h, &lead) {
                ranked.push((self.try_latent(view, spec, hist, starts, work), i));
            }
        }
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Expand: the most promising bits, with every other base.
        for &(_, i) in ranked.iter().take(LATENT_EXPAND) {
            for base in bases.iter().filter(|b| **b != lead) {
                if let Some(view) = latent_view(spec, self.ch, &hidden[i], base) {
                    self.try_latent(view, spec, hist, starts, work);
                }
            }
        }
    }

    /// Score one hidden-state view and adopt it if it beats the marginal and the channel's leader,
    /// definition included (hidden state is invented only where the visible explains less). Returns a key
    /// for ranking: its code length when scored, else a large number less the steps it lasted.
    fn try_latent(&mut self, view: View, spec: &Spec, hist: &[Step], starts: &[usize], work: &mut Work) -> f64 {
        if self.advisers.iter().any(|a| a.view.as_ref() == Some(&view)) {
            return f64::NEG_INFINITY;
        }
        let prior_bits = view.bits();
        let bound = if self.advisers.len() < POOL {
            self.marginal.local_bits
        } else {
            let worst = self.advisers.iter().map(|a| a.score()).fold(f64::NEG_INFINITY, f64::max);
            (worst - prior_bits).min(self.marginal.local_bits)
        };
        // It must beat the leader, so its replay stops as soon as it cannot; a view whose definition alone
        // costs more than the leader's whole score is not replayed at all.
        let bound = bound.min(self.leader().score() - prior_bits);
        if bound <= 0.0 {
            return f64::INFINITY;
        }
        let (scored, steps) = score_view(&view, spec, hist, self.ch, starts, self.marginal.modulus, bound);
        work.candidates += 1;
        work.replay_steps += steps as u64;
        let Some((bits, table)) = scored else { return 1e12 - steps as f64 };
        if bits + 1e-9 >= self.marginal.local_bits || prior_bits + bits >= self.leader().score() {
            return bits;
        }
        let mut cand = Adviser::new(Some(view), Origin::Latent, prior_bits, self.marginal.modulus);
        cand.counts = table;
        cand.local_bits = bits;
        cand.local_n = hist.len();
        if self.advisers.len() < POOL {
            self.advisers.push(cand);
        } else {
            let (worst, ws) = self.advisers.iter().enumerate().map(|(i, a)| (i, a.score())).fold((0, f64::NEG_INFINITY), |acc, x| if x.1 > acc.1 { x } else { acc });
            if cand.score() < ws {
                self.advisers[worst] = cand;
            }
        }
        bits
    }

    /// Evaluate up to `budget` proposals against the full history.
    pub fn think(&mut self, spec: &Spec, hist: &[Step], starts: &[usize], budget: usize, work: &mut Work) {
        for _ in 0..budget {
            if self.cursor >= self.proposals.len() {
                if let Some(p) = self.stream.next() {
                    self.proposals.push(p);
                }
            }
            if self.proposals.is_empty() {
                return;
            }
            if self.cursor >= self.proposals.len() {
                // Everything has been tried once; later passes re-check old
                // candidates with more evidence.
                self.cursor = 0;
                self.tried.clear();
            }
            // Borrowed in place: only a view not yet tried is copied (for
            // `tried`), and only an adopted one again (for its adviser).
            let prop = &self.proposals[self.cursor];
            self.cursor += 1;
            if self.tried.contains(&prop.view) {
                continue;
            }
            self.tried.insert(prop.view.clone());
            if self.advisers.iter().any(|a| a.view.as_ref() == Some(&prop.view)) {
                continue;
            }
            // The most code length it may have and still be adopted.
            let keep_inherited = prop.origin.inherited() && hist.len() < 4 && self.advisers.len() < POOL;
            let bound = if keep_inherited || hist.is_empty() {
                f64::INFINITY
            } else if self.advisers.len() < POOL {
                self.marginal.local_bits
            } else {
                let worst = self.advisers.iter().map(|a| a.score()).fold(f64::NEG_INFINITY, f64::max);
                (worst - prop.prior_bits).min(self.marginal.local_bits)
            };
            let (scored, steps) = score_view(&prop.view, spec, hist, self.ch, starts, self.marginal.modulus, bound);
            work.candidates += 1;
            work.replay_steps += steps as u64;
            if let Origin::Library(id) = &prop.origin {
                let id = *id;
                *self.spent.entry(id).or_insert(0) += 1;
                work.library_candidates += 1;
                work.library_replay += steps as u64;
            }
            let Some((bits, table)) = scored else { continue };
            let mut cand = Adviser::new(Some(prop.view.clone()), prop.origin.clone(), prop.prior_bits, self.marginal.modulus);
            cand.counts = table;
            cand.local_bits = bits;
            cand.local_n = hist.len();
            // It must explain something the marginal does not.
            if cand.local_bits + 1e-9 >= self.marginal.local_bits && !hist.is_empty() && !keep_inherited {
                continue;
            }
            if self.advisers.len() < POOL {
                self.advisers.push(cand);
            } else {
                let (worst, ws) = self
                    .advisers
                    .iter()
                    .enumerate()
                    .map(|(i, a)| (i, a.score()))
                    .fold((0, f64::NEG_INFINITY), |acc, x| if x.1 > acc.1 { x } else { acc });
                if cand.score() < ws {
                    self.advisers[worst] = cand;
                }
            }
        }
    }

    /// The adviser carrying most weight now.
    pub fn leader(&self) -> &Adviser {
        self.advisers.iter().min_by(|a, b| a.score().total_cmp(&b.score())).unwrap_or(&self.marginal)
    }
}

pub fn argmax(p: &[f64]) -> usize {
    let mut best = 0;
    for i in 1..p.len() {
        if p[i] > p[best] {
            best = i;
        }
    }
    best
}

/// Learns one world from its stream of (action, observation).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldLearner {
    pub spec: Spec,
    pub hist: Vec<Step>,
    pub models: Vec<Option<ChannelModel>>,
    pub settings: Settings,
    pub work: Work,
    pub streak: usize,
    pub mastered_at: Option<usize>,
    /// Calibration record: (probability given to the outcome, whether argmax was right).
    pub brier: f64,
    pub confident_errors: usize,
    pub forecasts: usize,
    /// Where each later episode begins in `hist` (evidence is kept across episodes).
    #[serde(default)]
    pub starts: Vec<usize>,
}

impl WorldLearner {
    fn start(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }
    /// The current episode's history.
    pub fn episode(&self) -> &[Step] {
        &self.hist[self.start()..]
    }
    /// Begin a new episode of the same world, keeping all evidence so far.
    pub fn new_episode(&mut self) {
        self.starts.push(self.hist.len());
        self.streak = 0;
        for m in self.models.iter_mut().flatten() {
            m.last_wrong = true;
        }
    }
    pub fn new(spec: &Spec, proposals: Vec<Vec<Proposal>>, settings: Settings) -> Self {
        Self::with_streams(spec, proposals.into_iter().map(ProposalStream::from_list).collect(), settings)
    }

    pub fn with_streams(spec: &Spec, streams: Vec<ProposalStream>, settings: Settings) -> Self {
        let mut models: Vec<Option<ChannelModel>> = vec![None; spec.channels.len()];
        let mut props = streams.into_iter();
        for ch in spec.targets() {
            models[ch] = Some(ChannelModel::new(spec, ch, props.next().unwrap_or_default()));
        }
        WorldLearner {
            spec: spec.clone(),
            hist: Vec::new(),
            models,
            settings,
            work: Work::default(),
            streak: 0,
            mastered_at: None,
            brier: 0.0,
            confident_errors: 0,
            forecasts: 0,
            starts: Vec::new(),
        }
    }

    /// Forecast every target channel for `action` (inputs read from `now`).
    pub fn predict(&self, hist: &[Step], action: usize, now: &[u32]) -> Vec<Option<Vec<f64>>> {
        let f = Frame { spec: &self.spec, hist, action, now };
        self.models.iter().map(|m| m.as_ref().map(|m| m.predict(&f, &self.settings))).collect()
    }

    /// Take one observed step: score forecasts, learn, then think if unreliable.
    pub fn observe(&mut self, action: usize, obs: &[u32]) -> bool {
        let mut all_right = true;
        let hist = std::mem::take(&mut self.hist);
        let s0 = self.start();
        {
            let f = Frame { spec: &self.spec, hist: &hist[s0..], action, now: obs };
            for m in self.models.iter_mut().flatten() {
                let y = obs[m.ch];
                let p_before = m.predict(&f, &self.settings);
                let (right, p) = m.observe(&f, y, &self.settings);
                let mm = p_before.len();
                let brier: f64 = (0..mm).map(|k| (p_before[k] - if k == y as usize { 1.0 } else { 0.0 }).powi(2)).sum();
                self.brier += brier;
                self.forecasts += 1;
                if !right && p_before[argmax(&p_before)] > 0.9 {
                    self.confident_errors += 1;
                }
                let _ = p;
                all_right &= right;
            }
        }
        self.hist = hist;
        self.hist.push(Step { action, obs: obs.to_vec() });
        if all_right {
            self.streak += 1;
        } else {
            self.streak = 0;
        }
        if self.mastered_at.is_none() && self.streak >= STREAK {
            self.mastered_at = Some(self.hist.len());
        }
        // Think harder only when failing: no search once reliably right.
        let budget = self.settings.budget;
        for m in self.models.iter_mut().flatten() {
            if self.settings.latent && budget > 0 {
                m.invent_latent(&self.spec, &self.hist, &self.starts, &mut self.work);
            }
            if m.last_wrong || self.streak < STREAK {
                m.think(&self.spec, &self.hist, &self.starts, budget, &mut self.work);
            }
        }
        all_right
    }

    /// Observations before mastery began (the verification streak excluded).
    pub fn obs_to_mastery(&self) -> Option<usize> {
        self.mastered_at.map(|t| t - STREAK)
    }

    pub fn mean_brier(&self) -> f64 {
        self.brier / self.forecasts.max(1) as f64
    }
}

/// Whether a T285 count switch is off (0): such fields are not written, so a Core saved with them off is
/// byte-identical to one saved before they existed.
pub(crate) fn off(n: &u32) -> bool {
    *n == 0
}
