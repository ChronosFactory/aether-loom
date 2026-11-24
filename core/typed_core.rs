//! One typed program core across domains.
//!
//! Aether's learners each have their own grammar: formulas over integer
//! sequences (`numbers2.rs`), primitive chains over grids (`grid.rs`), and
//! byte programs for coding. A concept learned in one cannot be used in
//! another. This module is a small typed language they can share:
//!
//! - **Types:** `Int`, `Bool`, `Seq` (a list of integers), `Grid`, and
//!   functions between them.
//! - **Terms:** variables (the task's inputs), literals, primitives from a
//!   library, application (partial application makes function values), and
//!   bodies over a loop variable (sums and products).
//! - **A library** of typed primitives, in domains: the *core* combinators
//!   (`map`, `scan`, `fold`, `zip`, `compose`, `cond`, and a bounded
//!   `iterate`), the *numbers* leaves and operators, and *grid* operations
//!   (rows and columns as sequences, and primitives of `grid.rs`). Learned
//!   macros are entries like any other, typed by inference.
//! - **Search:** type-directed enumeration bottom-up by size, with
//!   observational equivalence on the task's examples, priced by the
//!   library's counts (two-part MDL, as in `numbers2.rs`).
//! - **Sleep:** recurring closed sub-terms become macros when that shortens
//!   the description of what was solved (`learn`).
//!
//! Because every domain's primitives are leaves of one language and macros
//! are typed, a macro mined from sequence solutions (`scan(add)`, say) is a
//! candidate wherever its type fits, including inside `map_rows` over a
//! grid. The transfer test in this module measures exactly that: fewer
//! candidates enumerated in the second domain, against a fresh control.
//! Counts, exact evaluation and code length only; no vectors, no network.
use crate::grid::Grid;
use crate::numbers2::{self, Expr, Op};
use crate::schemes::Scheme;
use serde::{Deserialize, Serialize};
use crate::numbers::KeyMap;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Ty {
    Int,
    Bool,
    Seq,
    Grid,
    Fun(Vec<Ty>, Box<Ty>),
}

impl Ty {
    pub fn fun(args: &[Ty], ret: Ty) -> Ty {
        if args.is_empty() { ret } else { Ty::Fun(args.to_vec(), Box::new(ret)) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Seq(Vec<i64>),
    Grid(Grid),
    /// A function value: a primitive or macro, with arguments bound so far.
    Closure(usize, Vec<Value>),
    /// A function of the loop variable.
    Body(Box<Term>),
    /// A function of a loop index and a sequence accumulator.
    Body2(Box<Term>),
}

impl Value {
    pub fn ty(&self, lib: &Library) -> Ty {
        match self {
            Value::Int(_) => Ty::Int,
            Value::Bool(_) => Ty::Bool,
            Value::Seq(_) => Ty::Seq,
            Value::Grid(_) => Ty::Grid,
            Value::Closure(p, bound) => {
                let (args, ret) = lib.entries[*p].signature();
                Ty::fun(&args[bound.len()..], ret)
            }
            Value::Body(_) => Ty::fun(&[Ty::Int], Ty::Int),
            Value::Body2(_) => Ty::fun(&[Ty::Int, Ty::Seq], Ty::Seq),
        }
    }
    pub fn int(&self) -> Option<i64> {
        if let Value::Int(x) = self { Some(*x) } else { None }
    }
    fn seq(&self) -> Option<&Vec<i64>> {
        if let Value::Seq(x) = self { Some(x) } else { None }
    }
    fn grid(&self) -> Option<&Grid> {
        if let Value::Grid(x) = self { Some(x) } else { None }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Term {
    /// The i-th input of the task (or of the macro being evaluated).
    Var(usize),
    /// The loop variable of the enclosing body.
    Loop,
    Lit(Value),
    /// A library entry (primitive or macro).
    Lib(usize),
    App(Box<Term>, Vec<Term>),
    /// A function of the loop variable (produced by translation).
    Body(Box<Term>),
    /// The sequence accumulator of the enclosing indexed loop.
    Acc,
    /// A function of a loop index and a sequence accumulator (translation of
    /// the byte VM's `iterate`).
    Body2(Box<Term>),
}

pub const LIMIT: i64 = 1_000_000_000_000_000;
/// Most evaluation steps for one term on one example.
pub const FUEL: usize = 20_000;

fn bounded(v: i64) -> Option<i64> {
    (v.unsigned_abs() <= LIMIT as u64).then_some(v)
}

/// The arithmetic the numbers domain shares with `numbers2`.
pub fn arith(op: Op, x: i64, y: i64) -> Option<i64> {
    let v = match op {
        Op::Add => x.checked_add(y)?,
        Op::Sub => x.checked_sub(y)?,
        Op::Mul => x.checked_mul(y)?,
        Op::Div => {
            if y == 0 {
                return None;
            }
            x.checked_div_euclid(y)?
        }
        Op::Mod => {
            if y <= 0 {
                return None;
            }
            x.rem_euclid(y)
        }
        Op::Pow => {
            if !(0..=62).contains(&y) {
                return None;
            }
            x.checked_pow(y as u32)?
        }
        Op::Binom => return None,
    };
    bounded(v)
}

/// What a primitive does.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Prim {
    // core
    Map,
    Scan,
    Fold,
    Zip,
    Compose,
    Cond,
    /// Apply a function to a value k times (a bounded fixpoint).
    Iterate,
    Length,
    /// The k-th last element of a sequence (a(n-k) of a history).
    Last(u8),
    Index,
    SumSeq,
    MaxSeq,
    Range,
    /// Int -> Seq of one element (the byte VM's `byte`; 0..=255).
    ByteOf,
    Concat,
    /// iterate(count, start, body(index, acc)) over 0..count (the byte VM's loop).
    IterateIdx,
    NonZero,
    BoolInt,
    CondSeq,
    // numbers
    Arith(Op),
    Max,
    Min,
    Eq,
    Lt,
    Even,
    IntLit(i64),
    /// Sum of the body over the loop variable 0..=n (translation of `Sum`).
    SumTo,
    /// Product of the body over 1..=n.
    ProdTo,
    // grid
    /// A grid's cells as one sequence, row-major, width first.
    Rows,
    /// Column-major, height first.
    Cols,
    MapRows,
    MapCols,
    GridOp(crate::grid::Prim),
    Height,
    Width,
    // T549 skill bridge
    /// A checked program another skill owns, run by that skill's own
    /// interpreter: an opaque typed callable. Never part of a saved library;
    /// the bridge appends these to a search copy only (`core_connections`).
    Foreign(Box<Foreign>),
}

/// T549: a program in another skill's language, called as one typed piece.
/// Each owning skill supplies only its interpreter here, not new primitives.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Foreign {
    /// The `programs` skill (`curiosity.rs` `Op` chains), run by `curiosity::execute`.
    Programs(Vec<crate::curiosity::Op>),
    /// T573: the same chain run on the text a sequence of letter codes spells; text in the typed language is the
    /// sequence of its letters. Built for one answer only (`core_connections::growth_answer_text`), never saved.
    Letters(Vec<crate::curiosity::Op>),
}

/// T573: the text a sequence of letter codes spells, and back.
pub fn spell(v: &[i64]) -> Option<String> {
    v.iter().map(|&c| u32::try_from(c).ok().and_then(char::from_u32)).collect()
}
pub fn letters(s: &str) -> Vec<i64> {
    s.chars().map(|c| c as i64).collect()
}

impl Foreign {
    /// Run the piece on typed arguments through its owner's interpreter.
    /// Only one integer or sequence argument maps onto the owner's data.
    pub fn run(&self, args: &[Value]) -> Option<Value> {
        match self {
            Foreign::Programs(p) => {
                use crate::curiosity::Datum;
                let x = match args {
                    [Value::Int(n)] => Datum::Number(*n),
                    [Value::Seq(v)] => Datum::List(v.clone()),
                    _ => return None,
                };
                if !x.valid() {
                    return None;
                }
                match crate::curiosity::execute(p, &x)? {
                    Datum::Number(n) => Some(Value::Int(bounded(n)?)),
                    Datum::List(v) => Some(Value::Seq(v)),
                    Datum::Text(_) => None,
                }
            }
            Foreign::Letters(p) => {
                use crate::curiosity::Datum;
                let [Value::Seq(v)] = args else { return None };
                let x = Datum::Text(spell(v)?);
                if !x.valid() {
                    return None;
                }
                match crate::curiosity::execute(p, &x)? {
                    Datum::Number(n) => Some(Value::Int(bounded(n)?)),
                    Datum::List(v) => Some(Value::Seq(v)),
                    Datum::Text(s) => Some(Value::Seq(letters(&s))),
                }
            }
        }
    }
    /// T573: how many steps the piece holds; a piece is never priced as fewer than one.
    pub fn steps(&self) -> usize {
        match self {
            Foreign::Programs(p) | Foreign::Letters(p) => p.len().max(1),
        }
    }
    /// A short readable name for receipts.
    pub fn label(&self) -> String {
        match self {
            Foreign::Programs(p) => {
                let ops: Vec<String> = p.iter().map(|o| match o {
                    crate::curiosity::Op::Formula(_) => "Formula".to_string(),
                    o => format!("{o:?}"),
                }).collect();
                format!("programs:{}", ops.join("+"))
            }
            Foreign::Letters(p) => format!("letters:{}", p.iter().map(|o| format!("{o:?}")).collect::<Vec<_>>().join("+")),
        }
    }
}

/// A library entry: a primitive or a learned macro.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    Prim { prim: Prim, args: Vec<Ty>, ret: Ty, domain: String },
    Macro { body: Term, args: Vec<Ty>, ret: Ty, name: String },
}

impl Entry {
    pub fn signature(&self) -> (&[Ty], Ty) {
        match self {
            Entry::Prim { args, ret, .. } | Entry::Macro { args, ret, .. } => (args, ret.clone()),
        }
    }
    /// How many arguments the entry takes: `signature().0.len()`, without
    /// copying the return type.
    pub fn arity(&self) -> usize {
        match self {
            Entry::Prim { args, .. } | Entry::Macro { args, .. } => args.len(),
        }
    }
    pub fn name(&self) -> String {
        match self {
            Entry::Prim { prim, .. } => format!("{prim:?}"),
            Entry::Macro { name, .. } => name.clone(),
        }
    }
    pub fn domain(&self) -> &str {
        match self {
            Entry::Prim { domain, .. } => domain,
            Entry::Macro { .. } => "learned",
        }
    }
}

/// T573 switch, read once from AETHER_MACRO_STEPS: `1` prices a learned macro by the steps its body holds
/// (`Library::steps`), in every table the search, the doubt and the closing stage read. Bridge pieces are priced
/// by `piece_steps` in the door, not here. Anything else is the old pricing, bit for bit.
pub fn macro_steps() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("AETHER_MACRO_STEPS").as_deref() == Ok("1"))
}

/// The shared library: typed entries with counts for pricing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Library {
    pub entries: Vec<Entry>,
    pub counts: Vec<f64>,
    /// Domains whose entries the search may use (learned macros always may).
    pub enabled: Vec<String>,
    /// What the mind that invented a learned entry remembers of it: the kinds
    /// of problems it has helped solve, alone or inside another entry. Such an
    /// entry is recalled first only for those kinds (`solve`); an entry with
    /// no memory is always recalled.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub worked_on: BTreeMap<usize, BTreeSet<Kind>>,
}

/// A kind of problem: its input types, in type order rather than position,
/// and its output type.
pub type Kind = (Vec<Ty>, Ty);

/// The kind of a problem with these input and output types.
pub fn kind(in_types: &[Ty], out: &Ty) -> Kind {
    let mut ins = in_types.to_vec();
    ins.sort();
    (ins, out.clone())
}

impl Library {
    /// The core combinators and the numbers and grid domains (grid disabled
    /// until `enable("grid")`).
    pub fn new() -> Library {
        let mut l = Library::default();
        let (i, s, b, g) = (Ty::Int, Ty::Seq, Ty::Bool, Ty::Grid);
        let ii_i = Ty::fun(&[i.clone(), i.clone()], i.clone());
        let i_i = Ty::fun(&[i.clone()], i.clone());
        let s_s = Ty::fun(&[s.clone()], s.clone());
        let core = [
            (Prim::Map, vec![i_i.clone(), s.clone()], s.clone()),
            (Prim::Scan, vec![ii_i.clone(), s.clone()], s.clone()),
            (Prim::Fold, vec![ii_i.clone(), i.clone(), s.clone()], i.clone()),
            (Prim::Zip, vec![ii_i.clone(), s.clone(), s.clone()], s.clone()),
            (Prim::Compose, vec![s_s.clone(), s_s.clone(), s.clone()], s.clone()),
            (Prim::Cond, vec![b.clone(), i.clone(), i.clone()], i.clone()),
            (Prim::Iterate, vec![s_s.clone(), i.clone(), s.clone()], s.clone()),
            (Prim::Length, vec![s.clone()], i.clone()),
            (Prim::Index, vec![s.clone(), i.clone()], i.clone()),
            (Prim::SumSeq, vec![s.clone()], i.clone()),
            (Prim::MaxSeq, vec![s.clone()], i.clone()),
            (Prim::Range, vec![i.clone()], s.clone()),
            (Prim::ByteOf, vec![i.clone()], s.clone()),
            (Prim::Concat, vec![s.clone(), s.clone()], s.clone()),
            (Prim::IterateIdx, vec![i.clone(), s.clone(), Ty::fun(&[i.clone(), s.clone()], s.clone())], s.clone()),
            (Prim::NonZero, vec![i.clone()], b.clone()),
            (Prim::BoolInt, vec![b.clone()], i.clone()),
            (Prim::CondSeq, vec![b.clone(), s.clone(), s.clone()], s.clone()),
        ];
        for (p, args, ret) in core {
            l.add_prim(p, args, ret, "core");
        }
        for op in [Op::Add, Op::Sub, Op::Mul, Op::Div, Op::Mod] {
            l.add_prim(Prim::Arith(op), vec![i.clone(), i.clone()], i.clone(), "numbers");
        }
        l.add_prim(Prim::Max, vec![i.clone(), i.clone()], i.clone(), "numbers");
        l.add_prim(Prim::Min, vec![i.clone(), i.clone()], i.clone(), "numbers");
        l.add_prim(Prim::Eq, vec![i.clone(), i.clone()], b.clone(), "numbers");
        l.add_prim(Prim::Lt, vec![i.clone(), i.clone()], b.clone(), "numbers");
        l.add_prim(Prim::Even, vec![i.clone()], b.clone(), "numbers");
        for c in 0..=3 {
            l.add_prim(Prim::IntLit(c), vec![], i.clone(), "numbers");
        }
        for k in 1..=4u8 {
            l.add_prim(Prim::Last(k), vec![s.clone()], i.clone(), "numbers");
        }
        l.add_prim(Prim::SumTo, vec![i.clone(), i_i.clone()], i.clone(), "numbers");
        l.add_prim(Prim::ProdTo, vec![i.clone(), i_i.clone()], i.clone(), "numbers");
        l.add_prim(Prim::Rows, vec![g.clone()], s.clone(), "grid");
        l.add_prim(Prim::Cols, vec![g.clone()], s.clone(), "grid");
        l.add_prim(Prim::MapRows, vec![s_s.clone(), g.clone()], g.clone(), "grid");
        l.add_prim(Prim::MapCols, vec![s_s.clone(), g.clone()], g.clone(), "grid");
        l.add_prim(Prim::Height, vec![g.clone()], i.clone(), "grid");
        l.add_prim(Prim::Width, vec![g.clone()], i.clone(), "grid");
        for p in [
            crate::grid::Prim::Rot90,
            crate::grid::Prim::Rot180,
            crate::grid::Prim::FlipH,
            crate::grid::Prim::FlipV,
            crate::grid::Prim::Transpose,
            crate::grid::Prim::MirrorRight,
            crate::grid::Prim::MirrorDown,
            crate::grid::Prim::Scale(2),
            crate::grid::Prim::CropContent,
        ] {
            l.add_prim(Prim::GridOp(p), vec![g.clone()], g.clone(), "grid");
        }
        l.enabled = vec!["core".into(), "numbers".into()];
        l
    }

    pub fn add_prim(&mut self, prim: Prim, args: Vec<Ty>, ret: Ty, domain: &str) -> usize {
        self.entries.push(Entry::Prim { prim, args, ret, domain: domain.into() });
        self.counts.push(0.0);
        self.entries.len() - 1
    }

    pub fn add_macro(&mut self, name: &str, body: Term, args: Vec<Ty>, ret: Ty) -> usize {
        self.entries.push(Entry::Macro { body, args, ret, name: name.into() });
        self.counts.push(0.0);
        self.entries.len() - 1
    }

    pub fn enable(&mut self, domain: &str) {
        if !self.enabled.iter().any(|d| d == domain) {
            self.enabled.push(domain.into());
        }
    }

    pub fn find(&self, prim: &Prim) -> Option<usize> {
        self.entries.iter().position(|e| matches!(e, Entry::Prim { prim: p, .. } if p == prim))
    }

    pub fn macros(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e, Entry::Macro { .. })).count()
    }

    pub fn usable(&self, i: usize) -> bool {
        let d = self.entries[i].domain();
        d == "learned" || self.enabled.iter().any(|x| x == d)
    }

    /// Code length of one use of entry `i`: Dirichlet(1)-smoothed counts over
    /// the usable entries.
    pub fn cost(&self, i: usize) -> f64 {
        let usable: Vec<usize> = (0..self.entries.len()).filter(|&j| self.usable(j)).collect();
        let total: f64 = usable.iter().map(|&j| self.counts[j]).sum();
        let own = -((self.counts[i] + 1.0) / (total + usable.len() as f64)).log2();
        if !macro_steps() {
            return own;
        }
        own + self.length_cost(i, (total + usable.len() as f64).log2())
    }

    /// T573: how many steps entry `i` holds: a learned macro is the steps of the entries its body calls (at
    /// least one); every other entry is one step.
    pub fn steps(&self, i: usize) -> usize {
        self.steps_in(i, 0)
    }

    fn steps_in(&self, i: usize, depth: usize) -> usize {
        fn body_steps(lib: &Library, t: &Term, depth: usize) -> usize {
            match t {
                Term::Lib(j) => lib.steps_in(*j, depth + 1),
                Term::App(f, args) => body_steps(lib, f, depth) + args.iter().map(|a| body_steps(lib, a, depth)).sum::<usize>(),
                Term::Body(b) | Term::Body2(b) => body_steps(lib, b, depth),
                _ => 0,
            }
        }
        match self.entries.get(i) {
            // A body may only call earlier entries; the depth bound guards a malformed library.
            Some(Entry::Macro { body, .. }) if depth < 16 => body_steps(self, body, depth).max(1),
            _ => 1,
        }
    }

    /// T573, with `AETHER_MACRO_STEPS=1`: what a macro costs beyond its own price, `unit` bits (the price of an
    /// unused entry) for each step past the first. Zero for every other entry.
    fn length_cost(&self, i: usize, unit: f64) -> f64 {
        (self.steps(i) - 1) as f64 * unit
    }

    /// `cost` of every entry, computed once (the same numbers, bit for bit).
    pub fn costs(&self) -> Vec<f64> {
        let usable: Vec<usize> = (0..self.entries.len()).filter(|&j| self.usable(j)).collect();
        let total: f64 = usable.iter().map(|&j| self.counts[j]).sum();
        if macro_steps() {
            let unit = (total + usable.len() as f64).log2();
            return (0..self.entries.len()).map(|i| -((self.counts[i] + 1.0) / (total + usable.len() as f64)).log2() + self.length_cost(i, unit)).collect();
        }
        (0..self.entries.len()).map(|i| -((self.counts[i] + 1.0) / (total + usable.len() as f64)).log2()).collect()
    }

    /// Code length of a term.
    pub fn term_cost(&self, t: &Term) -> f64 {
        match t {
            Term::Var(_) | Term::Loop => 1.0,
            Term::Lit(Value::Int(c)) => 2.0 + (c.unsigned_abs() as f64 + 1.0).log2(),
            Term::Lit(_) => 8.0,
            Term::Lib(i) => self.cost(*i),
            Term::App(f, args) => self.term_cost(f) + args.iter().map(|a| self.term_cost(a)).sum::<f64>(),
            Term::Body(b) | Term::Body2(b) => 1.0 + self.term_cost(b),
            Term::Acc => 1.0,
        }
    }

    /// Whether entry `i` comes to mind first for a problem of kind `k`.
    pub fn recalled(&self, i: usize, k: &Kind) -> bool {
        let Some(kinds) = self.worked_on.get(&i) else { return true };
        if kinds.contains(k) {
            return true;
        }
        if kinds.is_empty() {
            return false;
        }
        let Some(entry) = self.entries.get(i) else { return false };
        let (args, ret) = entry.signature();
        if args.is_empty() {
            return false;
        }
        // Type-directed bridge recall:
        // 1. A sequence operator (returning Seq, taking Int*/Seq) that worked on a
        //    Seq task bridges to tasks with Seq inputs (e.g. [Seq] -> Int) and,
        //    when grid primitives are enabled and its trailing argument is Seq,
        //    to Grid tasks via MapRows/MapCols/Rows/Cols.
        let worked_seq = kinds.iter().any(|(ins, out)| ins.contains(&Ty::Seq) || *out == Ty::Seq);
        if ret == Ty::Seq && worked_seq && args.contains(&Ty::Seq) && args.iter().all(|t| matches!(t, Ty::Int | Ty::Seq)) {
            if k.0.contains(&Ty::Seq) {
                return true;
            }
            if k.0.contains(&Ty::Grid) && self.enabled.iter().any(|d| d == "grid") && args.last() == Some(&Ty::Seq) && args[..args.len() - 1].iter().all(|t| *t == Ty::Int) {
                return true;
            }
        }
        // 2. A curried integer operator (returning Int, taking Int+) that worked on
        //    an Int-output task bridges to Seq/Grid tasks via Map(f, seq).
        let worked_int = kinds.iter().any(|(_, out)| *out == Ty::Int);
        if ret == Ty::Int && worked_int && args.iter().all(|t| *t == Ty::Int) {
            if k.0.contains(&Ty::Seq) || (k.0.contains(&Ty::Grid) && self.enabled.iter().any(|d| d == "grid")) {
                return true;
            }
        }
        false
    }

    /// Whether recall for kind `k` leaves out any usable entry.
    pub fn recall_narrows(&self, k: &Kind) -> bool {
        (0..self.entries.len()).any(|i| self.usable(i) && !self.recalled(i, k))
    }

    /// `costs` among the entries recalled for kind `k`, whose counts make the
    /// code; the others are not tried.
    pub fn recall_costs(&self, k: &Kind) -> Vec<f64> {
        let first: Vec<bool> = (0..self.entries.len()).map(|i| self.usable(i) && self.recalled(i, k)).collect();
        let n = first.iter().filter(|&&f| f).count() as f64;
        let total: f64 = (0..self.entries.len()).filter(|&i| first[i]).map(|i| self.counts[i]).sum();
        if macro_steps() {
            let unit = (total + n).log2();
            return (0..self.entries.len()).map(|i| if first[i] { -((self.counts[i] + 1.0) / (total + n)).log2() + self.length_cost(i, unit) } else { f64::INFINITY }).collect();
        }
        (0..self.entries.len()).map(|i| if first[i] { -((self.counts[i] + 1.0) / (total + n)).log2() } else { f64::INFINITY }).collect()
    }

    /// Every memory of where an entry worked names a learned entry.
    pub fn check_worked_on(&self) -> Result<(), String> {
        match self.worked_on.keys().find(|&&i| !matches!(self.entries.get(i), Some(Entry::Macro { .. }))) {
            Some(i) => Err(format!("library.worked_on: entry {i} is not a learned entry")),
            None => Ok(()),
        }
    }

    /// Re-estimate the counts from solutions.
    pub fn recount(&mut self, solutions: &[Term]) {
        self.counts = vec![0.0; self.entries.len()];
        for s in solutions {
            count_entries(s, &mut self.counts);
        }
    }
}

/// Number of nodes of a term, applications included.
pub fn term_size(t: &Term) -> usize {
    match t {
        Term::App(f, args) => 1 + term_size(f) + args.iter().map(term_size).sum::<usize>(),
        Term::Body(b) | Term::Body2(b) => 1 + term_size(b),
        _ => 1,
    }
}

/// Add one to `counts[i]` for each use of library entry `i` in `t`.
pub fn count_entries(t: &Term, counts: &mut [f64]) {
    match t {
        Term::Lib(i) => counts[*i] += 1.0,
        Term::App(f, args) => {
            count_entries(f, counts);
            for a in args {
                count_entries(a, counts);
            }
        }
        Term::Body(b) | Term::Body2(b) => count_entries(b, counts),
        _ => {}
    }
}

/// Infer a term's type under the input types `env`.
pub fn type_of(lib: &Library, t: &Term, env: &[Ty], in_body: bool) -> Option<Ty> {
    match t {
        Term::Var(i) => env.get(*i).cloned(),
        Term::Loop => in_body.then_some(Ty::Int),
        Term::Acc => in_body.then_some(Ty::Seq),
        Term::Body2(b) => (type_of(lib, b, env, true)? == Ty::Seq).then(|| Ty::fun(&[Ty::Int, Ty::Seq], Ty::Seq)),
        Term::Lit(v) => Some(v.ty(lib)),
        Term::Lib(i) => {
            let (args, ret) = lib.entries.get(*i)?.signature();
            Some(Ty::fun(args, ret))
        }
        Term::App(f, args) => {
            let Ty::Fun(params, ret) = type_of(lib, f, env, in_body)? else { return None };
            if args.is_empty() || args.len() > params.len() {
                return None;
            }
            for (a, p) in args.iter().zip(&params) {
                if type_of(lib, a, env, in_body)? != *p {
                    return None;
                }
            }
            Some(Ty::fun(&params[args.len()..], *ret))
        }
        Term::Body(b) => (type_of(lib, b, env, true)? == Ty::Int).then(|| Ty::fun(&[Ty::Int], Ty::Int)),
    }
}

/// Evaluation state: inputs and fuel.
struct Ev<'a> {
    lib: &'a Library,
    env: &'a [Value],
    fuel: usize,
    /// T567 speed: emptied argument vectors kept for reuse, so a step does not allocate a new one. No effect on results.
    bufs: Vec<Vec<Value>>,
}

impl Ev<'_> {
    fn tick(&mut self) -> Option<()> {
        self.fuel = self.fuel.checked_sub(1)?;
        Some(())
    }

    fn eval(&mut self, t: &Term, lp: Option<i64>) -> Option<Value> {
        self.eval_in(t, lp, None)
    }

    fn eval_in(&mut self, t: &Term, lp: Option<i64>, acc: Option<&Value>) -> Option<Value> {
        self.tick()?;
        match t {
            Term::Var(i) => self.env.get(*i).cloned(),
            Term::Loop => lp.map(Value::Int),
            Term::Acc => acc.cloned(),
            Term::Body2(b) => Some(Value::Body2(b.clone())),
            Term::Lit(v) => Some(v.clone()),
            Term::Lib(i) => {
                if self.lib.entries.get(*i)?.arity() == 0 { self.call(*i, Vec::new()) } else { Some(Value::Closure(*i, Vec::new())) }
            }
            Term::App(f, args) => {
                // A conditional evaluates only the branch it takes, so a guard
                // (`if i < length(x) then x[i] else 0`) works as in the byte VM.
                if let (Term::Lib(i), 3) = (&**f, args.len()) {
                    if matches!(self.lib.entries.get(*i), Some(Entry::Prim { prim: Prim::Cond | Prim::CondSeq, .. })) {
                        self.tick()?;
                        let Value::Bool(c) = self.eval_in(&args[0], lp, acc)? else { return None };
                        return self.eval_in(&args[if c { 1 } else { 2 }], lp, acc);
                    }
                }
                let fv = self.eval_in(f, lp, acc)?;
                let mut vals = self.buf();
                vals.reserve(args.len());
                for a in args {
                    vals.push(self.eval_in(a, lp, acc)?);
                }
                self.apply(fv, vals)
            }
            Term::Body(b) => Some(Value::Body(b.clone())),
        }
    }

    fn apply(&mut self, f: Value, mut vals: Vec<Value>) -> Option<Value> {
        match f {
            Value::Body(b) => {
                if vals.len() != 1 {
                    return None;
                }
                let m = vals[0].int()?;
                self.eval(&b, Some(m))
            }
            Value::Body2(b) => {
                if vals.len() != 2 {
                    return None;
                }
                let i = vals[0].int()?;
                if vals[1].seq().is_none() {
                    return None;
                }
                self.eval_in(&b, Some(i), Some(&vals[1]))
            }
            Value::Closure(i, mut bound) => {
                let n = self.lib.entries[i].arity();
                if bound.is_empty() {
                    bound = vals;
                } else {
                    bound.append(&mut vals);
                }
                if bound.len() < n {
                    return Some(Value::Closure(i, bound));
                }
                if bound.len() > n {
                    return None;
                }
                self.call(i, bound)
            }
            _ => None,
        }
    }

    /// `apply(f.clone(), vals)` without copying `f`: the same checks in the
    /// same order, and no fuel spent before `eval`/`call`, as there.
    fn apply_ref(&mut self, f: &Value, vals: Vec<Value>) -> Option<Value> {
        match f {
            Value::Body(b) => {
                if vals.len() != 1 {
                    return None;
                }
                let m = vals[0].int()?;
                self.eval(b, Some(m))
            }
            Value::Body2(b) => {
                if vals.len() != 2 {
                    return None;
                }
                let i = vals[0].int()?;
                if vals[1].seq().is_none() {
                    return None;
                }
                self.eval_in(b, Some(i), Some(&vals[1]))
            }
            Value::Closure(i, bound) => {
                let n = self.lib.entries[*i].arity();
                let args = if bound.is_empty() {
                    vals
                } else {
                    let mut args = Vec::with_capacity(bound.len() + vals.len());
                    args.extend_from_slice(bound);
                    args.extend(vals);
                    args
                };
                if args.len() < n {
                    return Some(Value::Closure(*i, args));
                }
                if args.len() > n {
                    return None;
                }
                self.call(*i, args)
            }
            _ => None,
        }
    }

    /// An empty argument vector, reused when one is free.
    fn buf(&mut self) -> Vec<Value> {
        self.bufs.pop().unwrap_or_default()
    }

    fn apply1(&mut self, f: &Value, a: Value) -> Option<Value> {
        let mut b = self.buf();
        b.push(a);
        self.apply_ref(f, b)
    }

    fn apply2(&mut self, f: &Value, a: Value, c: Value) -> Option<Value> {
        let mut b = self.buf();
        b.push(a);
        b.push(c);
        self.apply_ref(f, b)
    }

    fn call(&mut self, i: usize, mut v: Vec<Value>) -> Option<Value> {
        let r = self.call_in(i, &v);
        v.clear();
        if self.bufs.len() < 32 {
            self.bufs.push(v);
        }
        r
    }

    fn call_in(&mut self, i: usize, v: &Vec<Value>) -> Option<Value> {
        self.tick()?;
        let lib = self.lib;
        let prim = match &lib.entries[i] {
            Entry::Macro { body, .. } => {
                // Borrowed from the library, not copied: a learned entry costs
                // what its body costs to run, as a primitive does.
                let mut inner = Ev { lib, env: v, fuel: self.fuel, bufs: std::mem::take(&mut self.bufs) };
                let r = inner.eval(body, None);
                self.fuel = inner.fuel;
                self.bufs = inner.bufs;
                return r;
            }
            Entry::Prim { prim, .. } => prim.clone(),
        };
        let int = |k: usize| v.get(k)?.int();
        let seq = |k: usize| v.get(k)?.seq();
        Some(match prim {
            Prim::Map => {
                let f = &v[0];
                let xs = seq(1)?;
                let mut out = Vec::with_capacity(xs.len());
                for &x in xs {
                    out.push(self.apply1(f, Value::Int(x))?.int()?);
                }
                Value::Seq(out)
            }
            Prim::Scan => {
                let f = &v[0];
                let xs = seq(1)?;
                let mut out = Vec::with_capacity(xs.len());
                let mut acc: Option<i64> = None;
                for &x in xs {
                    let next = match acc {
                        None => x,
                        Some(a) => self.apply2(f, Value::Int(a), Value::Int(x))?.int()?,
                    };
                    acc = Some(next);
                    out.push(next);
                }
                Value::Seq(out)
            }
            Prim::Fold => {
                let f = &v[0];
                let mut acc = int(1)?;
                for &x in seq(2)? {
                    acc = self.apply2(f, Value::Int(acc), Value::Int(x))?.int()?;
                }
                Value::Int(acc)
            }
            Prim::Zip => {
                let f = &v[0];
                let (xs, ys) = (seq(1)?, seq(2)?);
                let mut out = Vec::new();
                for (&x, &y) in xs.iter().zip(ys) {
                    out.push(self.apply2(f, Value::Int(x), Value::Int(y))?.int()?);
                }
                Value::Seq(out)
            }
            Prim::Compose => {
                let g = self.apply1(&v[1], v[2].clone())?;
                self.apply1(&v[0], g)?
            }
            Prim::Cond => {
                let Value::Bool(c) = v[0] else { return None };
                if c { v[1].clone() } else { v[2].clone() }
            }
            Prim::Iterate => {
                let f = &v[0];
                let k = int(1)?;
                if !(0..=64).contains(&k) {
                    return None;
                }
                let mut x = v[2].clone();
                for _ in 0..k {
                    x = self.apply1(f, x)?;
                }
                x
            }
            Prim::Length => Value::Int(seq(0)?.len() as i64),
            Prim::Last(k) => {
                let s = seq(0)?;
                let k = k as usize;
                if s.len() < k {
                    return None;
                }
                Value::Int(s[s.len() - k])
            }
            Prim::Index => {
                let s = seq(0)?;
                let j = int(1)?;
                if j < 0 || j as usize >= s.len() {
                    return None;
                }
                Value::Int(s[j as usize])
            }
            Prim::SumSeq => {
                let mut t = 0i64;
                for &x in seq(0)? {
                    t = arith(Op::Add, t, x)?;
                }
                Value::Int(t)
            }
            Prim::MaxSeq => Value::Int(*seq(0)?.iter().max()?),
            Prim::Range => {
                let n = int(0)?;
                if !(0..=4096).contains(&n) {
                    return None;
                }
                Value::Seq((0..=n).collect())
            }
            Prim::ByteOf => {
                let x = int(0)?;
                if !(0..=255).contains(&x) {
                    return None;
                }
                Value::Seq(vec![x])
            }
            Prim::Concat => {
                let mut a = seq(0)?.clone();
                a.extend(seq(1)?);
                if a.len() > 4096 {
                    return None;
                }
                Value::Seq(a)
            }
            Prim::IterateIdx => {
                let count = int(0)?;
                if !(0..=4096).contains(&count) {
                    return None;
                }
                let f = &v[2];
                let mut acc = v[1].clone();
                if acc.seq().is_none() {
                    return None;
                }
                for i in 0..count {
                    acc = self.apply2(f, Value::Int(i), acc)?;
                    if acc.seq().is_none() {
                        return None;
                    }
                }
                acc
            }
            Prim::NonZero => Value::Bool(int(0)? != 0),
            Prim::BoolInt => {
                let Value::Bool(c) = v[0] else { return None };
                Value::Int(c as i64)
            }
            Prim::CondSeq => {
                let Value::Bool(c) = v[0] else { return None };
                if c { v[1].clone() } else { v[2].clone() }
            }
            Prim::Arith(op) => Value::Int(arith(op, int(0)?, int(1)?)?),
            Prim::Max => Value::Int(int(0)?.max(int(1)?)),
            Prim::Min => Value::Int(int(0)?.min(int(1)?)),
            Prim::Eq => Value::Bool(int(0)? == int(1)?),
            Prim::Lt => Value::Bool(int(0)? < int(1)?),
            Prim::Even => Value::Bool(int(0)? % 2 == 0),
            Prim::IntLit(c) => Value::Int(c),
            Prim::SumTo | Prim::ProdTo => {
                let n = int(0)?;
                let f = &v[1];
                let is_sum = prim == Prim::SumTo;
                let mut t = if is_sum { 0 } else { 1 };
                let lo = if is_sum { 0 } else { 1 };
                for m in lo..=n {
                    let x = self.apply1(f, Value::Int(m))?.int()?;
                    t = arith(if is_sum { Op::Add } else { Op::Mul }, t, x)?;
                }
                Value::Int(t)
            }
            Prim::Rows => {
                let g = v[0].grid()?;
                let mut s = vec![g.first().map_or(0, |r| r.len()) as i64];
                for r in g {
                    s.extend(r.iter().map(|&c| c as i64));
                }
                Value::Seq(s)
            }
            Prim::Cols => {
                let g = v[0].grid()?;
                let (h, w) = (g.len(), g.first().map_or(0, |r| r.len()));
                let mut s = vec![h as i64];
                for c in 0..w {
                    for r in g.iter().take(h) {
                        s.push(r[c] as i64);
                    }
                }
                Value::Seq(s)
            }
            Prim::MapRows | Prim::MapCols => {
                let f = &v[0];
                let g = v[1].grid()?;
                let rows = prim == Prim::MapRows;
                let (h, w) = (g.len(), g.first().map_or(0, |r| r.len()));
                let mut out = g.clone();
                let lines = if rows { h } else { w };
                for a in 0..lines {
                    let line: Vec<i64> = if rows { g[a].iter().map(|&c| c as i64).collect() } else { (0..h).map(|r| g[r][a] as i64).collect() };
                    let n = line.len();
                    let r = self.apply1(f, Value::Seq(line))?;
                    let r = r.seq()?;
                    if r.len() != n || r.iter().any(|&c| !(0..=9).contains(&c)) {
                        return None;
                    }
                    for (b, &c) in r.iter().enumerate() {
                        if rows { out[a][b] = c as u8 } else { out[b][a] = c as u8 }
                    }
                }
                Value::Grid(out)
            }
            Prim::GridOp(p) => Value::Grid(crate::grid::apply(p, v[0].grid()?)?),
            Prim::Height => Value::Int(v[0].grid()?.len() as i64),
            Prim::Width => Value::Int(v[0].grid()?.first().map_or(0, |r| r.len()) as i64),
            Prim::Foreign(f) => f.run(v)?,
        })
    }
}

/// The evaluator as it was before the copies were removed (T183 lane mind),
/// for the tests and the `tc_check` check build only.
#[cfg(any(test, feature = "tc_check"))]
#[path = "typed_core_reference.rs"]
mod reference;

/// Check build: the new and the reference evaluator must agree on the value
/// and on the fuel left; the first difference aborts the run.
#[cfg(feature = "tc_check")]
fn tc_check(what: &str, new: &(Option<Value>, usize), old: &(Option<Value>, usize), show: &dyn Fn() -> String) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static AGREED: AtomicU64 = AtomicU64::new(0);
    if new != old {
        eprintln!("TCCHECK DIFFER {what}: {} new={new:?} reference={old:?}", show());
        std::process::abort();
    }
    let n = AGREED.fetch_add(1, Ordering::Relaxed) + 1;
    if n.is_power_of_two() || n % 1_000_000 == 0 {
        eprintln!("TCCHECK {n} evaluations agree (value and fuel)");
    }
}

/// Evaluate `t` on inputs `env`.
pub fn eval(lib: &Library, t: &Term, env: &[Value]) -> Option<Value> {
    let mut ev = Ev { lib, env, fuel: FUEL, bufs: Vec::new() };
    let r = ev.eval(t, None);
    #[cfg(feature = "tc_check")]
    {
        let new = (r, ev.fuel);
        tc_check("eval", &new, &reference::eval_ref(lib, t, env), &|| format!("term={t:?} env={env:?}"));
        return new.0;
    }
    #[cfg(not(feature = "tc_check"))]
    r
}

/// T567 speed: `eval` and the fuel it spent (always the same for the same term and inputs).
pub fn eval_used(lib: &Library, t: &Term, env: &[Value]) -> Option<(Value, usize)> {
    let mut ev = Ev { lib, env, fuel: FUEL, bufs: Vec::new() };
    let r = ev.eval(t, None)?;
    Some((r, FUEL - ev.fuel))
}

/// T567 speed: what `eval_used(lib, App(Lib(i), args), env)` returns, built from each argument's own
/// `eval_used` result on the same `env` (`vals`, `used`; every argument is defined there). It spends fuel
/// exactly as the full evaluation does: one for the application, one for the function, each argument's own
/// use, then the call. A conditional charges its test and only the branch it takes, as `eval_in` does.
/// `term` is the whole application, for the cases this does not cover.
pub fn eval_app_cached(lib: &Library, term: &Term, i: usize, vals: &[&Value], used: &[usize], env: &[Value]) -> Option<(Value, usize)> {
    let Some(entry) = lib.entries.get(i) else { return eval_used(lib, term, env) };
    if entry.arity() == 0 || vals.is_empty() {
        return eval_used(lib, term, env);
    }
    let mut fuel = FUEL.checked_sub(1)?;
    if vals.len() == 3 && matches!(entry, Entry::Prim { prim: Prim::Cond | Prim::CondSeq, .. }) {
        fuel = fuel.checked_sub(1)?.checked_sub(used[0])?;
        let Value::Bool(c) = vals[0] else { return None };
        let k = if *c { 1 } else { 2 };
        fuel = fuel.checked_sub(used[k])?;
        return Some((vals[k].clone(), FUEL - fuel));
    }
    fuel = fuel.checked_sub(1)?;
    for u in used {
        fuel = fuel.checked_sub(*u)?;
    }
    let mut ev = Ev { lib, env, fuel, bufs: Vec::new() };
    let r = ev.apply(Value::Closure(i, Vec::new()), vals.iter().map(|v| (*v).clone()).collect())?;
    Some((r, FUEL - ev.fuel))
}

/// T567 speed: `AETHER_EVAL_CHECK=1` makes the searches compare every cached evaluation with the full one
/// and stop the process on any difference (a diagnostic; slow).
pub fn eval_check() -> bool {
    std::env::var("AETHER_EVAL_CHECK").as_deref() == Ok("1")
}

/// Apply a function value to arguments.
pub fn apply_value(lib: &Library, f: &Value, args: Vec<Value>) -> Option<Value> {
    #[cfg(feature = "tc_check")]
    let old = reference::apply_value_ref(lib, f, args.clone());
    let mut ev = Ev { lib, env: &[], fuel: FUEL, bufs: Vec::new() };
    let r = ev.apply_ref(f, args);
    #[cfg(feature = "tc_check")]
    {
        let new = (r, ev.fuel);
        tc_check("apply_value", &new, &old, &|| format!("f={f:?}"));
        return new.0;
    }
    #[cfg(not(feature = "tc_check"))]
    r
}

/// A task: typed inputs per example and the expected output.
#[derive(Clone, Debug)]
pub struct Task {
    pub in_types: Vec<Ty>,
    pub out_type: Ty,
    pub examples: Vec<(Vec<Value>, Value)>,
}

/// The result of a search.
#[derive(Clone, Debug)]
pub struct Found {
    pub term: Term,
    pub cost: f64,
    /// Candidates evaluated before the answer was found.
    pub enumerated: usize,
}

/// All ways to split `budget` into `parts` positive sizes.
fn compositions(budget: usize, parts: usize) -> Vec<Vec<usize>> {
    fn rec(budget: usize, parts: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if parts == 1 {
            if budget >= 1 {
                cur.push(budget);
                out.push(cur.clone());
                cur.pop();
            }
            return;
        }
        for first in 1..=budget.saturating_sub(parts - 1) {
            cur.push(first);
            rec(budget - first, parts - 1, cur, out);
            cur.pop();
        }
    }
    let mut out = Vec::new();
    if parts >= 1 && budget >= parts {
        rec(budget, parts, &mut Vec::new(), &mut out);
    }
    out
}

/// Type-directed search: the cheapest term of the task's output type that
/// matches every example, enumerating bottom-up by size with observational
/// equivalence on the examples. `beam` terms survive per (type, size).
///
/// Recall first: when the library remembers where its learned entries worked,
/// only those remembered for this kind of problem are tried at first, with
/// the code their counts give. The rest are reached for only if that fails;
/// `enumerated` then counts both tries. A library with no such memory, or a
/// kind for which recall leaves nothing out, searches as before.
pub fn solve(lib: &Library, task: &Task, max_size: usize, beam: usize) -> Option<Found> {
    let k = kind(&task.in_types, &task.out_type);
    if !lib.recall_narrows(&k) {
        return solve_priced(lib, task, max_size, beam, &lib.costs(), usize::MAX).0;
    }
    let (found, first) = solve_priced(lib, task, max_size, beam, &lib.recall_costs(&k), usize::MAX);
    if found.is_some() {
        return found;
    }
    solve_priced(lib, task, max_size, beam, &lib.costs(), usize::MAX).0.map(|f| Found { enumerated: first + f.enumerated, ..f })
}

/// `solve` with each entry's code length given (a relevance table for this
/// task, say; `Library::costs` reproduces `solve`) and at most `limit`
/// candidates evaluated. Also returns how many were evaluated in all, the
/// search's cost whether or not it found an answer.
pub fn solve_priced(lib: &Library, task: &Task, max_size: usize, beam: usize, costs: &[f64], limit: usize) -> (Option<Found>, usize) {
    let r = search_priced(lib, task, max_size, beam, costs, limit);
    #[cfg(feature = "tc_check")]
    {
        use std::sync::atomic::{AtomicU64, Ordering};
        static AGREED: AtomicU64 = AtomicU64::new(0);
        let key = |r: &(Option<Found>, usize)| (r.0.as_ref().map(|f| (f.term.clone(), f.cost.to_bits(), f.enumerated)), r.1);
        let old = reference::solve_priced(lib, task, max_size, beam, costs, limit);
        if key(&r) != key(&old) {
            eprintln!("TCCHECK DIFFER solve_priced: task={task:?} max_size={max_size} beam={beam} limit={limit} new={r:?} reference={old:?}");
            std::process::abort();
        }
        let n = AGREED.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_power_of_two() || n % 1000 == 0 {
            eprintln!("TCCHECK {n} searches agree (term, cost bits, enumerated)");
        }
    }
    r
}

/// T567 speed: how many candidates are evaluated together before their offers are applied in order.
pub const EVAL_BATCH: usize = 4096;

/// T567 speed: threads for candidate evaluation: `AETHER_THREADS` when a positive number, else one per core.
/// Results never depend on it.
pub fn eval_workers() -> usize {
    std::env::var("AETHER_THREADS").ok().and_then(|x| x.trim().parse::<usize>().ok()).filter(|&n| n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
}

/// T567 speed: `f(i)` for every i in 0..n, results in order of i, on up to `eval_workers()` threads that take
/// blocks of 32 from one queue. `f` must be pure. Fewer than 128 items stay on the calling thread.
pub fn par_map<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    const BLOCK: usize = 32;
    let w = if n < 128 { 1 } else { eval_workers().min(n / BLOCK) };
    if w <= 1 {
        return (0..n).map(f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done: Vec<Vec<(usize, Vec<T>)>> = std::thread::scope(|scope| {
        let (f, next) = (&f, &next);
        let hs: Vec<_> = (0..w).map(|_| scope.spawn(move || {
            let mut out = Vec::new();
            loop {
                let lo = next.fetch_add(BLOCK, std::sync::atomic::Ordering::Relaxed);
                if lo >= n {
                    break;
                }
                out.push((lo, (lo..(lo + BLOCK).min(n)).map(f).collect::<Vec<T>>()));
            }
            out
        })).collect();
        hs.into_iter().map(|h| h.join().expect("evaluation thread")).collect()
    });
    let mut blocks: Vec<(usize, Vec<T>)> = done.into_iter().flatten().collect();
    blocks.sort_by_key(|b| b.0);
    blocks.into_iter().flat_map(|b| b.1).collect()
}

thread_local! {
    /// T571: the grain the searches on this thread use; 0 is off.
    static GRAIN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// T571: run `f` with the cheapest-first grain the Core holds in its own settings (`Arch::search_grain`; 0 is
/// off: levels are sizes, as before). `AETHER_CHEAPEST_FIRST` (0 to 8) replaces the Core's value inside the same
/// scope, for experiments only.
pub fn with_grain<T>(grain: usize, f: impl FnOnce() -> T) -> T {
    let g = std::env::var("AETHER_CHEAPEST_FIRST").ok().and_then(|x| x.trim().parse::<usize>().ok()).unwrap_or(grain).min(8);
    let old = GRAIN.with(|c| c.replace(g));
    let r = f();
    GRAIN.with(|c| c.set(old));
    r
}

/// T571: the grain of the search running now.
pub fn cheapest_first() -> usize {
    GRAIN.with(|c| c.get())
}

/// T571: how many search levels one use of each entry takes when levels are price bands. The unit is the median
/// price of the usable entries divided by `grain`, so a median entry takes `grain` levels, a cheap one fewer (at
/// least 1) and a dear one more (at most `top`). With every entry at 1 a level is a size.
fn price_weights(costs: &[f64], usable: &[usize], grain: usize, top: usize) -> Vec<usize> {
    let mut finite: Vec<f64> = usable.iter().map(|&i| costs[i]).filter(|c| c.is_finite() && *c > 0.0).collect();
    if finite.is_empty() {
        return vec![1; costs.len()];
    }
    finite.sort_by(|a, b| a.total_cmp(b));
    let unit = finite[finite.len() / 2] / grain as f64;
    costs.iter().map(|&c| if c.is_finite() && c > 0.0 { ((c / unit).round() as usize).clamp(1, top.max(1)) } else { 1 }).collect()
}

/// T571: with cheapest-first on, the top level is `grain` times the size limit, scaled by
/// `AETHER_CHEAPEST_TOP` percent (default 100).
fn price_top(max_size: usize, grain: usize) -> usize {
    let pct = std::env::var("AETHER_CHEAPEST_TOP").ok().and_then(|x| x.trim().parse::<usize>().ok()).filter(|n| (10..=400).contains(n)).unwrap_or(100);
    (grain * max_size * pct / 100).max(2)
}

/// T569 memory: a 128-bit fingerprint of a candidate's outputs on the examples (two unrelated 64-bit hashes).
/// The searches remember this for every candidate instead of a copy of the outputs, which held hundreds of
/// megabytes in one search. Two different outputs sharing a fingerprint is a chance of about 1 in 10^25 a search.
pub(crate) fn outs_print(outs: &[Value]) -> u128 {
    use std::hash::{Hash, Hasher};
    let mut a = std::collections::hash_map::DefaultHasher::new();
    let mut b = crate::numbers::KeyHasher::default();
    outs.hash(&mut a);
    outs.hash(&mut b);
    ((a.finish() as u128) << 64) | b.finish() as u128
}

/// `solve_priced`'s search.
fn search_priced(lib: &Library, task: &Task, max_size: usize, beam: usize, costs: &[f64], limit: usize) -> (Option<Found>, usize) {
    // T571: off (grain 0), every entry takes one level and a level is a size: the search below is what it
    // always was. On, a level is a price band, so cheap entries are tried first and reach further.
    let grain = cheapest_first();
    let usable: Vec<usize> = (0..lib.entries.len()).filter(|&i| lib.usable(i)).collect();
    let max_size = if grain == 0 { max_size } else { price_top(max_size, grain) };
    let weight: Vec<usize> = if grain == 0 { vec![1; lib.entries.len()] } else { price_weights(costs, &usable, grain, max_size) };
    #[derive(Clone)]
    struct E {
        term: Term,
        cost: f64,
        /// T567 speed: the term's value on each example input and the fuel it spent there.
        outs: Vec<Value>,
        used: Vec<usize>,
    }
    type Outs = (Vec<Value>, Vec<usize>);
    let outputs: Vec<&Value> = task.examples.iter().map(|(_, o)| o).collect();
    let evaluate = |t: &Term| -> Option<Outs> { task.examples.iter().map(|(i, _)| eval_used(lib, t, i)).collect::<Option<Vec<_>>>().map(|v| v.into_iter().unzip()) };
    // T567 speed: an application of entry `i` to kept terms, from their stored values instead of re-running them.
    let check = eval_check();
    let cached = |t: &Term, i: usize, args: &[&E]| -> Option<Outs> {
        let mut outs = Vec::with_capacity(task.examples.len());
        let mut used = Vec::with_capacity(task.examples.len());
        for (e, (input, _)) in task.examples.iter().enumerate() {
            let vals: Vec<&Value> = args.iter().map(|a| &a.outs[e]).collect();
            let us: Vec<usize> = args.iter().map(|a| a.used[e]).collect();
            let r = eval_app_cached(lib, t, i, &vals, &us, input);
            if check && r != eval_used(lib, t, input) {
                eprintln!("EVALCHECK DIFFER search_priced: term={t:?} input={input:?} cached={r:?} full={:?}", eval_used(lib, t, input));
                std::process::abort();
            }
            let (v, u) = r?;
            outs.push(v);
            used.push(u);
        }
        Some((outs, used))
    };
    // levels[size]: the kept terms of each type at that size. A term of size
    // `size` only reads smaller sizes, so those are borrowed, not copied, while
    // the current size grows. Both maps are look-ups only (each level is sorted
    // on its own), so the hasher cannot change what is found.
    type Seen = KeyMap<Ty, KeyMap<u128, f64>>;
    let mut levels: Vec<KeyMap<Ty, Vec<E>>> = (0..=max_size.max(1)).map(|_| KeyMap::default()).collect();
    let mut seen: Seen = KeyMap::default();
    let enumerated = std::cell::Cell::new(0usize);
    let mut best: Option<Found> = None;
    // `pre` is the term's evaluation when a batch already did it (T567 speed); the decisions are unchanged.
    let offer = |ty: &Ty, term: Term, cost: f64, pre: Option<Option<Outs>>, level: &mut KeyMap<Ty, Vec<E>>, seen: &mut Seen, best: &mut Option<Found>| {
        if !cost.is_finite() || enumerated.get() >= limit || best.as_ref().map_or(false, |b| b.cost <= cost) {
            return;
        }
        enumerated.set(enumerated.get() + 1);
        let enumerated = enumerated.get();
        let Some((outs, used)) = (match pre { Some(o) => o, None => evaluate(&term) }) else { return };
        if !seen.contains_key(ty) {
            seen.insert(ty.clone(), KeyMap::default());
        }
        let by_outs = seen.get_mut(ty).unwrap();
        let print = outs_print(&outs);
        if by_outs.get(&print).map_or(false, |&c| c <= cost) {
            return;
        }
        let solves = *ty == task.out_type && outs.iter().zip(&outputs).all(|(a, b)| a == *b);
        by_outs.insert(print, cost);
        if solves {
            *best = Some(Found { term: term.clone(), cost, enumerated });
        }
        if !level.contains_key(ty) {
            level.insert(ty.clone(), Vec::new());
        }
        let lvl = level.get_mut(ty).unwrap();
        lvl.push(E { term, cost, outs, used });
        if lvl.len() > 4 * beam {
            lvl.sort_by(|a, b| a.cost.total_cmp(&b.cost));
            lvl.truncate(beam);
        }
    };
    // T567 speed: candidates wait here; a flush evaluates them together (pure, any thread) and then applies
    // their offers one by one in the order they were made, so dedup, beam, best and the limit see what they
    // always saw. A candidate the current best already prices out is not evaluated (best only gets cheaper).
    let flush = |pending: &mut Vec<(Ty, Term, f64, usize, Vec<&E>)>, level: &mut KeyMap<Ty, Vec<E>>, seen: &mut Seen, best: &mut Option<Found>| {
        let bound = best.as_ref().map(|b| b.cost);
        let full = enumerated.get() >= limit;
        let outs = par_map(pending.len(), |k| {
            let (_, t, c, i, args) = &pending[k];
            if full || !c.is_finite() || bound.map_or(false, |b| b <= *c) { None } else { Some(cached(t, *i, args)) }
        });
        for ((ty, term, cost, _, _), pre) in pending.drain(..).zip(outs) {
            offer(&ty, term, cost, pre, level, seen, best);
        }
    };
    for (i, ty) in task.in_types.iter().enumerate() {
        offer(ty, Term::Var(i), 1.0, None, &mut levels[1], &mut seen, &mut best);
    }
    for &i in &usable {
        let (args, ret) = lib.entries[i].signature();
        offer(&Ty::fun(args, ret), Term::Lib(i), costs[i], None, &mut levels[weight[i].min(max_size.max(1))], &mut seen, &mut best);
    }
    for size in 2..=max_size {
        if enumerated.get() >= limit {
            break;
        }
        let (smaller, current) = levels.split_at_mut(size);
        let level = &mut current[0];
        let mut pending: Vec<(Ty, Term, f64, usize, Vec<&E>)> = Vec::new();
        // T571: candidates of this level the best answer so far does not already price out.
        let mut open = 0usize;
        for &i in &usable {
            let (args, ret) = lib.entries[i].signature();
            if args.is_empty() || !costs[i].is_finite() || size <= weight[i] {
                continue;
            }
            let fcost = costs[i];
            // Full and partial applications: the first j arguments.
            for j in 1..=args.len() {
                let ret_j = Ty::fun(&args[j..], ret.clone());
                for comp in compositions(size - weight[i], j) {
                    if enumerated.get() >= limit {
                        break;
                    }
                    let lists: Option<Vec<&Vec<E>>> = args[..j].iter().zip(&comp).map(|(t, &s)| smaller[s].get(t)).collect();
                    let Some(lists) = lists else { continue };
                    if lists.iter().any(|l| l.is_empty()) {
                        continue;
                    }
                    let mut idx = vec![0usize; j];
                    loop {
                        if enumerated.get() >= limit {
                            break;
                        }
                        let cost = fcost + idx.iter().zip(&lists).map(|(&k, l)| l[k].cost).sum::<f64>();
                        // T571: with cheapest-first on, a candidate the best answer prices out is not built.
                        if grain == 0 || !best.as_ref().map_or(false, |b| b.cost <= cost) {
                            open += 1;
                            let terms: Vec<Term> = idx.iter().zip(&lists).map(|(&k, l)| l[k].term.clone()).collect();
                            let kept: Vec<&E> = idx.iter().zip(&lists).map(|(&k, l)| &l[k]).collect();
                            pending.push((ret_j.clone(), Term::App(Box::new(Term::Lib(i)), terms), cost, i, kept));
                            if pending.len() >= EVAL_BATCH || enumerated.get() + pending.len() >= limit {
                                flush(&mut pending, level, &mut seen, &mut best);
                            }
                        }
                        let mut d = j;
                        let mut done = false;
                        loop {
                            if d == 0 {
                                done = true;
                                break;
                            }
                            d -= 1;
                            idx[d] += 1;
                            if idx[d] < lists[d].len() {
                                break;
                            }
                            idx[d] = 0;
                        }
                        if done {
                            break;
                        }
                    }
                }
            }
        }
        flush(&mut pending, level, &mut seen, &mut best);
        for lvl in levels.iter_mut().flat_map(|m| m.values_mut()) {
            lvl.sort_by(|a, b| a.cost.total_cmp(&b.cost));
            lvl.truncate(beam);
        }
        // T571: levels are price bands, so once an answer exists and a whole band was priced out, later bands are too.
        if grain > 0 && best.is_some() && open == 0 {
            break;
        }
    }
    (best, enumerated.get())
}

/// The code length of `t` under `costs` (`Library::term_cost` with a cost table).
pub fn priced(lib: &Library, t: &Term, costs: &[f64]) -> f64 {
    match t {
        Term::Lib(i) => costs.get(*i).copied().unwrap_or(f64::INFINITY),
        Term::App(f, args) => priced(lib, f, costs) + args.iter().map(|a| priced(lib, a, costs)).sum::<f64>(),
        Term::Body(b) | Term::Body2(b) => 1.0 + priced(lib, b, costs),
        other => lib.term_cost(other),
    }
}

/// One-piece neighbours of `t` (T279): every term that differs from `t` in exactly one place, a
/// library entry replaced by another usable entry of the same signature, an input by another input
/// of the same type, or an integer literal by its neighbours. Two of Aether's own concepts that fit
/// the same examples are most often siblings like this, so a doubt is checked here first: dozens of
/// evaluations, not a search.
pub fn swaps(lib: &Library, t: &Term, in_types: &[Ty], costs: &[f64]) -> Vec<Term> {
    let mut out = Vec::new();
    match t {
        Term::Lib(i) => {
            let sig = lib.entries[*i].signature();
            for j in 0..lib.entries.len() {
                if j != *i && lib.usable(j) && costs.get(j).map_or(false, |c| c.is_finite()) && lib.entries[j].signature() == sig {
                    out.push(Term::Lib(j));
                }
            }
        }
        Term::Var(i) => {
            for j in 0..in_types.len() {
                if j != *i && in_types.get(*i) == Some(&in_types[j]) {
                    out.push(Term::Var(j));
                }
            }
        }
        Term::Lit(Value::Int(c)) => {
            for d in [c - 1, c + 1] {
                out.push(Term::Lit(Value::Int(d)));
            }
        }
        Term::App(f, args) => {
            for g in swaps(lib, f, in_types, costs) {
                out.push(Term::App(Box::new(g), args.clone()));
            }
            for (k, a) in args.iter().enumerate() {
                for b in swaps(lib, a, in_types, costs) {
                    let mut na = args.clone();
                    na[k] = b;
                    out.push(Term::App(f.clone(), na));
                }
            }
        }
        Term::Body(b) => out.extend(swaps(lib, b, in_types, costs).into_iter().map(|x| Term::Body(Box::new(x)))),
        Term::Body2(b) => out.extend(swaps(lib, b, in_types, costs).into_iter().map(|x| Term::Body2(Box::new(x)))),
        _ => {}
    }
    out
}

/// The cheapest one-piece neighbour of `answer_term` (`swaps`) of code length at most `bound` that
/// fits every example of `task` but answers `queries` differently from `answer` (T279). Returns it
/// and how many neighbours were evaluated.
pub fn rival_swap(lib: &Library, task: &Task, queries: &[Vec<Value>], answer: &[Value], answer_term: &Term, costs: &[f64], bound: f64) -> (Option<Found>, usize) {
    let mut best: Option<Found> = None;
    let mut n = 0;
    for t in swaps(lib, answer_term, &task.in_types, costs) {
        let cost = priced(lib, &t, costs);
        if !(cost <= bound) || best.as_ref().map_or(false, |b| b.cost <= cost) {
            continue;
        }
        n += 1;
        if !task.examples.iter().all(|(i, o)| eval(lib, &t, i).as_ref() == Some(o)) {
            continue;
        }
        let Some(outs) = queries.iter().map(|q| eval(lib, &t, q)).collect::<Option<Vec<Value>>>() else { continue };
        if outs != answer {
            best = Some(Found { term: t, cost, enumerated: n });
        }
    }
    (best, n)
}

/// A rival to an answer (T279, abstain on ambiguity): a term of the task's
/// output type, of code length at most `bound` under `costs`, that matches
/// every example of `task` but whose outputs on `queries` are not `answer`.
/// The enumeration is `search_priced`'s, with two differences:
/// - Observational equivalence is on the examples and the queries together,
///   so two terms that agree on every example but not on a query are both kept.
/// - A term dearer than `bound` is never tried. Code lengths only add up, so
///   no part of a rival can cost more than the rival itself.
///
/// Returns the first rival met, and how many candidates were evaluated. The
/// queries' outputs are never known here, only compared.
pub fn rival_priced(lib: &Library, task: &Task, queries: &[Vec<Value>], answer: &[Value], max_size: usize, beam: usize, costs: &[f64], bound: f64, limit: usize) -> (Option<Found>, usize) {
    #[derive(Clone)]
    struct E {
        term: Term,
        cost: f64,
        outs: Vec<Value>,
        used: Vec<usize>,
    }
    let shown = task.examples.len();
    let inputs: Vec<&Vec<Value>> = task.examples.iter().map(|(i, _)| i).chain(queries).collect();
    let outputs: Vec<&Value> = task.examples.iter().map(|(_, o)| o).collect();
    type Outs = (Vec<Value>, Vec<usize>);
    let evaluate = |t: &Term| -> Option<Outs> { inputs.iter().map(|i| eval_used(lib, t, i)).collect::<Option<Vec<_>>>().map(|v| v.into_iter().unzip()) };
    // T567 speed, as in `search_priced`: an application from its kept arguments' stored values and fuel.
    let check = eval_check();
    let cached = |t: &Term, i: usize, args: &[&E]| -> Option<Outs> {
        let mut outs = Vec::with_capacity(inputs.len());
        let mut used = Vec::with_capacity(inputs.len());
        for (e, input) in inputs.iter().enumerate() {
            let vals: Vec<&Value> = args.iter().map(|a| &a.outs[e]).collect();
            let us: Vec<usize> = args.iter().map(|a| a.used[e]).collect();
            let r = eval_app_cached(lib, t, i, &vals, &us, input);
            if check && r != eval_used(lib, t, input) {
                eprintln!("EVALCHECK DIFFER rival_priced: term={t:?} input={input:?} cached={r:?} full={:?}", eval_used(lib, t, input));
                std::process::abort();
            }
            let (v, u) = r?;
            outs.push(v);
            used.push(u);
        }
        Some((outs, used))
    };
    type Seen = KeyMap<Ty, KeyMap<u128, f64>>;
    let mut levels: Vec<KeyMap<Ty, Vec<E>>> = (0..=max_size.max(1)).map(|_| KeyMap::default()).collect();
    let mut seen: Seen = KeyMap::default();
    let enumerated = std::cell::Cell::new(0usize);
    let mut rival: Option<Found> = None;
    // `pre` is the term's evaluation when a batch already did it; the decisions are unchanged.
    let offer = |ty: &Ty, term: Term, cost: f64, pre: Option<Option<Outs>>, level: &mut KeyMap<Ty, Vec<E>>, seen: &mut Seen, rival: &mut Option<Found>| {
        if rival.is_some() || !cost.is_finite() || cost > bound || enumerated.get() >= limit {
            return;
        }
        enumerated.set(enumerated.get() + 1);
        let Some((outs, used)) = (match pre { Some(o) => o, None => evaluate(&term) }) else { return };
        if !seen.contains_key(ty) {
            seen.insert(ty.clone(), KeyMap::default());
        }
        let by_outs = seen.get_mut(ty).unwrap();
        let print = outs_print(&outs);
        if by_outs.get(&print).map_or(false, |&c| c <= cost) {
            return;
        }
        if *ty == task.out_type && outs[..shown].iter().zip(&outputs).all(|(a, b)| a == *b) && outs[shown..] != *answer {
            *rival = Some(Found { term, cost, enumerated: enumerated.get() });
            return;
        }
        by_outs.insert(print, cost);
        if !level.contains_key(ty) {
            level.insert(ty.clone(), Vec::new());
        }
        let lvl = level.get_mut(ty).unwrap();
        lvl.push(E { term, cost, outs, used });
        if lvl.len() > 4 * beam {
            lvl.sort_by(|a, b| a.cost.total_cmp(&b.cost));
            lvl.truncate(beam);
        }
    };
    // Candidates wait here; a flush evaluates them together (pure, any thread) and applies their offers in
    // the order they were made, so dedup, beam, the first rival and the limit are what they always were.
    let flush = |pending: &mut Vec<(Ty, Term, f64, usize, Vec<&E>)>, level: &mut KeyMap<Ty, Vec<E>>, seen: &mut Seen, rival: &mut Option<Found>| {
        let skip = rival.is_some() || enumerated.get() >= limit;
        let outs = par_map(pending.len(), |k| {
            let (_, t, _, i, args) = &pending[k];
            if skip { None } else { Some(cached(t, *i, args)) }
        });
        for ((ty, term, cost, _, _), pre) in pending.drain(..).zip(outs) {
            offer(&ty, term, cost, pre, level, seen, rival);
        }
    };
    for (i, ty) in task.in_types.iter().enumerate() {
        offer(ty, Term::Var(i), 1.0, None, &mut levels[1], &mut seen, &mut rival);
    }
    let usable: Vec<usize> = (0..lib.entries.len()).filter(|&i| lib.usable(i)).collect();
    for &i in &usable {
        let (args, ret) = lib.entries[i].signature();
        offer(&Ty::fun(args, ret), Term::Lib(i), costs[i], None, &mut levels[1], &mut seen, &mut rival);
    }
    for size in 2..=max_size {
        if enumerated.get() >= limit || rival.is_some() {
            break;
        }
        let (smaller, current) = levels.split_at_mut(size);
        let level = &mut current[0];
        let mut pending: Vec<(Ty, Term, f64, usize, Vec<&E>)> = Vec::new();
        for &i in &usable {
            let (args, ret) = lib.entries[i].signature();
            if args.is_empty() || !costs[i].is_finite() || costs[i] > bound {
                continue;
            }
            let fcost = costs[i];
            for j in 1..=args.len() {
                let ret_j = Ty::fun(&args[j..], ret.clone());
                for comp in compositions(size - 1, j) {
                    if enumerated.get() >= limit || rival.is_some() {
                        break;
                    }
                    let lists: Option<Vec<&Vec<E>>> = args[..j].iter().zip(&comp).map(|(t, &s)| smaller[s].get(t)).collect();
                    let Some(lists) = lists else { continue };
                    if lists.iter().any(|l| l.is_empty()) {
                        continue;
                    }
                    let mut idx = vec![0usize; j];
                    loop {
                        if enumerated.get() >= limit || rival.is_some() {
                            break;
                        }
                        let terms: Vec<Term> = idx.iter().zip(&lists).map(|(&k, l)| l[k].term.clone()).collect();
                        let cost = fcost + idx.iter().zip(&lists).map(|(&k, l)| l[k].cost).sum::<f64>();
                        // An offer over the bound or not finite is refused whatever the state, so it never waits.
                        if cost.is_finite() && cost <= bound {
                            let kept: Vec<&E> = idx.iter().zip(&lists).map(|(&k, l)| &l[k]).collect();
                            pending.push((ret_j.clone(), Term::App(Box::new(Term::Lib(i)), terms), cost, i, kept));
                            if pending.len() >= EVAL_BATCH || enumerated.get() + pending.len() >= limit {
                                flush(&mut pending, level, &mut seen, &mut rival);
                            }
                        }
                        let mut d = j;
                        let mut done = false;
                        loop {
                            if d == 0 {
                                done = true;
                                break;
                            }
                            d -= 1;
                            idx[d] += 1;
                            if idx[d] < lists[d].len() {
                                break;
                            }
                            idx[d] = 0;
                        }
                        if done {
                            break;
                        }
                    }
                }
            }
        }
        flush(&mut pending, level, &mut seen, &mut rival);
        for lvl in levels.iter_mut().flat_map(|m| m.values_mut()) {
            lvl.sort_by(|a, b| a.cost.total_cmp(&b.cost));
            lvl.truncate(beam);
        }
    }
    (rival, enumerated.get())
}

/// Translate a `numbers2` formula into the core. Inputs: `Var(0)` is the
/// history a(0..n) as a `Seq`, `Var(1)` is n. Library entries without a
/// blank are inlined. Blanks, number theory functions, lags beyond four and
/// schemes other than a cumulative sum of the history do not translate (None).
pub fn from_numbers2(lib: &Library, k: &numbers2::Knowledge, e: &Expr) -> Option<Term> {
    translate(lib, k, e, false)
}

fn translate(lib: &Library, k: &numbers2::Knowledge, e: &Expr, in_body: bool) -> Option<Term> {
    let p = |prim: Prim| lib.find(&prim).map(Term::Lib);
    let n_term = if in_body { Term::Loop } else { Term::Var(1) };
    Some(match e {
        Expr::N => n_term,
        Expr::A(j) if *j <= 4 && !in_body => Term::App(Box::new(p(Prim::Last(*j))?), vec![Term::Var(0)]),
        Expr::A(_) => return None,
        Expr::C(c) => p(Prim::IntLit(*c))?,
        Expr::Lib(i) => translate(lib, k, &k.library[*i], in_body)?,
        Expr::Apply(..) | Expr::Hole | Expr::Fun(..) => return None,
        Expr::Bin(op, l, r) => {
            if matches!(op, Op::Binom) {
                return None;
            }
            Term::App(Box::new(p(Prim::Arith(*op))?), vec![translate(lib, k, l, in_body)?, translate(lib, k, r, in_body)?])
        }
        Expr::Sum(x) | Expr::Prod(x) => {
            let body = Term::Body(Box::new(translate(lib, k, x, true)?));
            let which = if matches!(e, Expr::Sum(_)) { Prim::SumTo } else { Prim::ProdTo };
            Term::App(Box::new(p(which)?), vec![n_term, body])
        }
        Expr::Sch1(Scheme::CumSum, x) if **x == Expr::A(1) && !in_body => {
            Term::App(Box::new(p(Prim::Last(1))?), vec![Term::App(Box::new(p(Prim::Scan)?), vec![p(Prim::Arith(Op::Add))?, Term::Var(0)])])
        }
        Expr::Sch1(..) | Expr::Sch2(..) => return None,
    })
}

/// Sleep: recurring closed sub-terms (using no input) become macros when
/// that shortens the description of the solutions. Returns the names added.
pub fn learn(lib: &mut Library, solutions: &[Term], env_types: &[Ty], max_new: usize) -> Vec<String> {
    fn subs(t: &Term, out: &mut Vec<Term>) {
        if let Term::App(f, args) = t {
            // The application and every partial application that leads to it.
            for j in 1..=args.len() {
                out.push(Term::App(f.clone(), args[..j].to_vec()));
            }
            subs(f, out);
            for a in args {
                subs(a, out);
            }
        }
    }
    fn has_var(t: &Term) -> bool {
        match t {
            Term::Var(_) | Term::Loop | Term::Acc => true,
            Term::App(f, args) => has_var(f) || args.iter().any(has_var),
            Term::Body(b) => has_var(b),
            // A two-argument body binds its index and accumulator, not the inputs.
            Term::Body2(b) => reads_input(b),
            _ => false,
        }
    }
    fn reads_input(t: &Term) -> bool {
        match t {
            Term::Var(_) => true,
            Term::App(f, args) => reads_input(f) || args.iter().any(reads_input),
            Term::Body(b) | Term::Body2(b) => reads_input(b),
            _ => false,
        }
    }
    let mut occ: HashMap<Term, usize> = HashMap::new();
    for s in solutions {
        let mut v = Vec::new();
        subs(s, &mut v);
        v.sort_by_key(|a| format!("{a:?}"));
        v.dedup();
        for t in v {
            if !has_var(&t) {
                *occ.entry(t).or_insert(0) += 1;
            }
        }
    }
    let leaf = ((lib.entries.len() + 1) as f64).log2();
    let mut gains: Vec<(f64, Term)> = occ
        .into_iter()
        .filter(|(t, n)| *n >= 2 && !lib.entries.iter().any(|e| matches!(e, Entry::Macro { body, .. } if body == t)))
        .map(|(t, n)| {
            let body = lib.term_cost(&t);
            (n as f64 * (body - leaf) - body, t)
        })
        .filter(|(g, _)| *g > 0.0)
        .collect();
    gains.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| format!("{:?}", a.1).cmp(&format!("{:?}", b.1))));
    let mut added = Vec::new();
    // The solutions written with the new macros, so that recounting credits them.
    let mut written = solutions.to_vec();
    for (_, t) in gains.into_iter().take(max_new) {
        let Some(ty) = type_of(lib, &t, env_types, false) else { continue };
        let (args, ret) = match ty {
            Ty::Fun(a, r) => (a, *r),
            t => (Vec::new(), t),
        };
        let name = format!("M{}", lib.macros());
        // A function-valued sub-term becomes a macro over its own arguments.
        let body = if args.is_empty() { t.clone() } else { Term::App(Box::new(t.clone()), (0..args.len()).map(Term::Var).collect()) };
        let id = lib.add_macro(&name, body, args, ret);
        written = written.iter().map(|s| use_macro(s, &t, id)).collect();
        added.push(name);
    }
    lib.recount(&written);
    added
}

/// `t` with each occurrence of `sub` (also as the leading arguments of a
/// longer application) replaced by library entry `id`, whose body is `sub`
/// applied to the remaining arguments. Bodies are left as they are, as in
/// `learn`'s search for sub-terms.
fn use_macro(t: &Term, sub: &Term, id: usize) -> Term {
    if t == sub {
        return Term::Lib(id);
    }
    let Term::App(f, args) = t else { return t.clone() };
    if let Term::App(sf, sargs) = sub {
        if f == sf && args.len() > sargs.len() && args[..sargs.len()] == sargs[..] {
            return Term::App(Box::new(Term::Lib(id)), args[sargs.len()..].iter().map(|a| use_macro(a, sub, id)).collect());
        }
    }
    Term::App(Box::new(use_macro(f, sub, id)), args.iter().map(|a| use_macro(a, sub, id)).collect())
}

pub fn show(lib: &Library, t: &Term) -> String {
    match t {
        Term::Var(i) => format!("x{i}"),
        Term::Loop => "m".into(),
        Term::Lit(Value::Int(c)) => c.to_string(),
        Term::Lit(v) => format!("{v:?}"),
        Term::Lib(i) => lib.entries[*i].name(),
        Term::App(f, args) => format!("{}({})", show(lib, f), args.iter().map(|a| show(lib, a)).collect::<Vec<_>>().join(", ")),
        Term::Body(b) => format!("m -> {}", show(lib, b)),
        Term::Acc => "acc".into(),
        Term::Body2(b) => format!("(i, acc) -> {}", show(lib, b)),
    }
}

/// Translate a byte-VM program (`aether_framework::byte_program_vm`, the
/// coding learner's language: JSON with a `body` and named `procedures`)
/// into the core. Bytes are sequences of integers 0..=255; `x` is `Var(0)`.
/// Each procedure becomes a macro over one sequence, added to `lib`. Integer
/// comparisons become `BoolInt(Eq)`/`BoolInt(Lt)`; a branch condition is
/// `NonZero` of its integer. Returns the body's term.
pub fn from_byte_vm(lib: &mut Library, program: &serde_json::Value) -> Option<Term> {
    let procs = program.get("procedures")?;
    let mut names: HashMap<String, usize> = HashMap::new();
    if let serde_json::Value::Object(map) = procs {
        for (name, p) in map {
            let body = from_byte_vm(lib, p)?;
            let ret = type_of(lib, &body, &[Ty::Seq], false)?;
            let id = lib.add_macro(&format!("proc_{name}"), body, vec![Ty::Seq], ret);
            names.insert(name.clone(), id);
        }
    }
    vm_expr(lib, program.get("body")?, &names)
}

fn vm_expr(lib: &Library, e: &serde_json::Value, procs: &HashMap<String, usize>) -> Option<Term> {
    use serde_json::Value as J;
    let p = |prim: Prim| lib.find(&prim).map(Term::Lib);
    let app = |f: Term, args: Vec<Term>| Term::App(Box::new(f), args);
    Some(match e {
        J::Number(n) => {
            let c = n.as_i64()?;
            match lib.find(&Prim::IntLit(c)) {
                Some(i) => Term::Lib(i),
                None => Term::Lit(Value::Int(c)),
            }
        }
        J::String(s) if s == "x" => Term::Var(0),
        J::String(s) if s == "empty" => Term::Lit(Value::Seq(Vec::new())),
        J::String(s) if s == "index" => Term::Loop,
        J::String(s) if s == "accumulator" => Term::Acc,
        J::Array(items) if !items.is_empty() => {
            let op = items[0].as_str()?;
            let arg = |i: usize| vm_expr(lib, items.get(i)?, procs);
            match op {
                "length" => app(p(Prim::Length)?, vec![arg(1)?]),
                "byte" => app(p(Prim::ByteOf)?, vec![arg(1)?]),
                "get" => app(p(Prim::Index)?, vec![arg(1)?, arg(2)?]),
                "concat" => app(p(Prim::Concat)?, vec![arg(1)?, arg(2)?]),
                "add" => app(p(Prim::Arith(Op::Add))?, vec![arg(1)?, arg(2)?]),
                "sub" => app(p(Prim::Arith(Op::Sub))?, vec![arg(1)?, arg(2)?]),
                "eq" => app(p(Prim::BoolInt)?, vec![app(p(Prim::Eq)?, vec![arg(1)?, arg(2)?])]),
                "lt" => app(p(Prim::BoolInt)?, vec![app(p(Prim::Lt)?, vec![arg(1)?, arg(2)?])]),
                "if" => {
                    let c = app(p(Prim::NonZero)?, vec![arg(1)?]);
                    let (a, b) = (arg(2)?, arg(3)?);
                    // Branch type decides the conditional: integers or sequences.
                    let env = [Ty::Seq];
                    let ta = type_of(lib, &a, &env, true);
                    if ta == Some(Ty::Seq) { app(p(Prim::CondSeq)?, vec![c, a, b]) } else { app(p(Prim::Cond)?, vec![c, a, b]) }
                }
                "call" => {
                    let name = items.get(1)?.as_str()?;
                    app(Term::Lib(*procs.get(name)?), vec![arg(2)?])
                }
                "iterate" => app(p(Prim::IterateIdx)?, vec![arg(1)?, arg(2)?, Term::Body2(Box::new(arg(3)?))]),
                _ => return None,
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rng;

    fn int(x: i64) -> Value {
        Value::Int(x)
    }

    #[test]
    fn terms_are_typed_evaluated_and_searched() {
        let lib = Library::new();
        let add = Term::Lib(lib.find(&Prim::Arith(Op::Add)).unwrap());
        let t = Term::App(Box::new(add.clone()), vec![Term::Var(0), Term::Var(1)]);
        assert_eq!(type_of(&lib, &t, &[Ty::Int, Ty::Int], false), Some(Ty::Int));
        assert_eq!(eval(&lib, &t, &[int(2), int(3)]), Some(int(5)));
        let scan = Term::Lib(lib.find(&Prim::Scan).unwrap());
        let t = Term::App(Box::new(scan.clone()), vec![add.clone(), Term::Var(0)]);
        assert_eq!(type_of(&lib, &t, &[Ty::Seq], false), Some(Ty::Seq));
        assert_eq!(eval(&lib, &t, &[Value::Seq(vec![1, 2, 3, 4])]), Some(Value::Seq(vec![1, 3, 6, 10])));
        // A partial application is a function value.
        let part = Term::App(Box::new(scan), vec![add]);
        assert_eq!(type_of(&lib, &part, &[], false), Some(Ty::fun(&[Ty::Seq], Ty::Seq)));
        let f = eval(&lib, &part, &[]).unwrap();
        assert_eq!(apply_value(&lib, &f, vec![Value::Seq(vec![2, 2])]), Some(Value::Seq(vec![2, 4])));
        // Search finds x0 + x1 from examples.
        let task = Task { in_types: vec![Ty::Int, Ty::Int], out_type: Ty::Int, examples: (1..6).map(|i| (vec![int(i), int(2 * i)], int(3 * i))).collect() };
        let f = solve(&lib, &task, 4, 200).expect("found");
        assert_eq!(eval(&lib, &f.term, &[int(7), int(9)]), Some(int(16)), "{}", show(&lib, &f.term));
        assert_eq!(compositions(4, 2), vec![vec![1, 3], vec![2, 2], vec![3, 1]]);
    }

    #[test]
    fn numbers2_formulas_translate_and_agree() {
        let lib = Library::new();
        let k = numbers2::Knowledge::default();
        let mut r = Rng(3);
        let e = Expr::Bin(Op::Add, Box::new(Expr::Bin(Op::Mul, Box::new(Expr::A(1)), Box::new(Expr::C(2)))), Box::new(Expr::Bin(Op::Sub, Box::new(Expr::N), Box::new(Expr::A(3)))));
        let t = from_numbers2(&lib, &k, &e).expect("translates");
        for _ in 0..50 {
            let hist: Vec<i64> = (0..8).map(|_| r.below(50) as i64 - 20).collect();
            let n = hist.len() as i64;
            let a = [hist[7], hist[6], hist[5]];
            assert_eq!(k.eval(&e, n, a), eval(&lib, &t, &[Value::Seq(hist.clone()), int(n)]).and_then(|v| v.int()));
        }
        // A sum over a closed body, and a product.
        for e in [Expr::Sum(Box::new(Expr::Bin(Op::Mul, Box::new(Expr::N), Box::new(Expr::C(2))))), Expr::Prod(Box::new(Expr::Bin(Op::Add, Box::new(Expr::N), Box::new(Expr::C(1)))))] {
            let t = from_numbers2(&lib, &k, &e).expect("loop translates");
            for n in 0..10 {
                assert_eq!(k.eval(&e, n, [0; 3]), eval(&lib, &t, &[Value::Seq(vec![0; n as usize]), int(n)]).and_then(|v| v.int()), "{}", show(&lib, &t));
            }
        }
    }

    #[test]
    fn guarded_byte_programs_run_and_learned_macros_never_capture_an_input() {
        use serde_json::json;
        let mut lib = Library::new();
        // The byte VM never reads x[5] when x is shorter; neither may the core.
        let guarded = from_byte_vm(&mut lib, &json!({"procedures": {}, "body": ["if", ["lt", 5, ["length", "x"]], ["get", "x", 5], 0]})).expect("translates");
        assert_eq!(eval(&lib, &guarded, &[Value::Seq(vec![1, 2, 3])]), Some(int(0)));
        assert_eq!(eval(&lib, &guarded, &[Value::Seq(vec![1, 2, 3, 4, 5, 6])]), Some(int(6)));
        // A loop body that reads the input is not a closed sub-term.
        let repeat = from_byte_vm(&mut lib, &json!({"procedures": {}, "body": ["iterate", 3, "empty", ["concat", "accumulator", "x"]]})).expect("translates");
        assert_eq!(eval(&lib, &repeat, &[Value::Seq(vec![7, 8])]), Some(Value::Seq(vec![7, 8, 7, 8, 7, 8])));
        let before = lib.entries.len();
        learn(&mut lib, &[repeat.clone(), repeat.clone(), repeat], &[Ty::Seq], 4);
        for id in before..lib.entries.len() {
            if lib.entries[id].signature().0.is_empty() {
                assert!(eval(&lib, &Term::Lib(id), &[]).is_some(), "macro {} needs no input", show(&lib, &Term::Lib(id)));
            }
        }
    }

    #[test]
    fn a_macro_learned_on_sequences_is_reused_on_grids_and_shortens_the_search() {
        let mut lib = Library::new();
        let scan = lib.find(&Prim::Scan).unwrap();
        let add = lib.find(&Prim::Arith(Op::Add)).unwrap();
        let mut sols = Vec::new();
        let mut r = Rng(5);
        for _ in 0..4 {
            let examples: Vec<(Vec<Value>, Value)> = (0..4)
                .map(|_| {
                    let xs: Vec<i64> = (0..5).map(|_| r.below(5) as i64).collect();
                    let mut acc = 0;
                    let ys: Vec<i64> = xs
                        .iter()
                        .map(|&x| {
                            acc += x;
                            acc
                        })
                        .collect();
                    (vec![Value::Seq(xs)], Value::Seq(ys))
                })
                .collect();
            let task = Task { in_types: vec![Ty::Seq], out_type: Ty::Seq, examples };
            let f = solve(&lib, &task, 4, 300).expect("running sums found");
            sols.push(f.term);
        }
        assert!(sols.iter().all(|s| matches!(s, Term::App(f, a) if **f == Term::Lib(scan) && a[0] == Term::Lib(add))), "{:?}", sols.iter().map(|s| show(&lib, s)).collect::<Vec<_>>());
        let added = learn(&mut lib, &sols, &[Ty::Seq], 2);
        assert!(!added.is_empty(), "scan(add) recurs and becomes a macro");
        let m = lib.entries.iter().position(|e| matches!(e, Entry::Macro { .. })).unwrap();
        assert_eq!(lib.counts[m], sols.len() as f64, "the new macro is credited with every use");
        // The grid domain: each row becomes its running sum.
        lib.enable("grid");
        let g = |rows: Vec<Vec<u8>>| Value::Grid(rows);
        let examples = vec![
            (vec![g(vec![vec![1, 0, 1], vec![0, 2, 0]])], g(vec![vec![1, 1, 2], vec![0, 2, 2]])),
            (vec![g(vec![vec![2, 1, 0, 1], vec![1, 1, 1, 1]])], g(vec![vec![2, 3, 3, 4], vec![1, 2, 3, 4]])),
        ];
        let task = Task { in_types: vec![Ty::Grid], out_type: Ty::Grid, examples };
        let with = solve(&lib, &task, 4, 300).expect("solved with the macro");
        let mut fresh = Library::new();
        fresh.enable("grid");
        let without = solve(&fresh, &task, 4, 300).expect("solved from scratch");
        eprintln!("with macro: {} ({} candidates, {:.1} bits); fresh: {} ({} candidates, {:.1} bits)", show(&lib, &with.term), with.enumerated, with.cost, show(&fresh, &without.term), without.enumerated, without.cost);
        assert!(with.enumerated < without.enumerated, "the macro shortens the grid search: {} vs {}", with.enumerated, without.enumerated);
        assert!(matches!(&with.term, Term::App(_, a) if matches!(a[0], Term::Lib(i) if matches!(lib.entries[i], Entry::Macro { .. }))), "the macro is what was reused: {}", show(&lib, &with.term));
    }

    // T183 lane mind: the evaluator without copies, and the search with
    // borrowed levels and KeyMap look-ups, against the verbatim old code.

    /// A library with every domain on, and macros (one built on another).
    fn checked_library() -> Library {
        let mut lib = Library::new();
        lib.enable("grid");
        let p = |lib: &Library, prim: Prim| Term::Lib(lib.find(&prim).unwrap());
        let scan_add = Term::App(Box::new(p(&lib, Prim::Scan)), vec![p(&lib, Prim::Arith(Op::Add)), Term::Var(0)]);
        let m = lib.add_macro("scan_add", scan_add, vec![Ty::Seq], Ty::Seq);
        let twice = Term::App(Box::new(Term::Lib(m)), vec![Term::App(Box::new(Term::Lib(m)), vec![Term::Var(0)])]);
        lib.add_macro("scan_add_twice", twice, vec![Ty::Seq], Ty::Seq);
        let step = Term::App(Box::new(p(&lib, Prim::Arith(Op::Add))), vec![Term::Var(0), Term::Var(1)]);
        lib.add_macro("plus", step, vec![Ty::Int, Ty::Int], Ty::Int);
        lib
    }

    fn small_int(r: &mut Rng) -> i64 {
        match r.below(12) {
            0 => r.below(5000) as i64,
            1 => *r.pick(&[0, 1, -1, 64, 65, 255, 256, 4096, 4097, LIMIT, -LIMIT, i64::MAX, i64::MIN]),
            _ => r.below(21) as i64 - 6,
        }
    }

    fn random_value(r: &mut Rng, ty: &Ty) -> Value {
        match ty {
            Ty::Int => Value::Int(small_int(r)),
            Ty::Bool => Value::Bool(r.chance(0.5)),
            Ty::Seq => Value::Seq((0..r.below(9)).map(|_| if r.chance(0.9) { r.below(12) as i64 - 3 } else { small_int(r) }).collect()),
            Ty::Grid => {
                let (h, w) = (r.below(4) + usize::from(r.chance(0.9)), 1 + r.below(4));
                Value::Grid((0..h).map(|_| (0..w).map(|_| r.below(10) as u8).collect()).collect())
            }
            Ty::Fun(..) => Value::Int(0),
        }
    }

    /// A random term, mostly of type `ty` under `env` (body: 0 none, 1 loop
    /// variable, 2 loop variable and accumulator), now and then ill-typed.
    fn random_term(r: &mut Rng, lib: &Library, ty: &Ty, env: &[Ty], body: u8, depth: usize) -> Term {
        if r.chance(0.04) {
            let other = r.pick(&[Ty::Int, Ty::Seq, Ty::Bool, Ty::Grid, Ty::fun(&[Ty::Int], Ty::Int)]).clone();
            if &other != ty {
                return random_term(r, lib, &other, env, body, depth);
            }
        }
        enum Opt {
            Leaf(Term),
            App(usize, usize),
            Body,
            Body2,
        }
        let mut opts = Vec::new();
        for (i, t) in env.iter().enumerate() {
            if t == ty {
                opts.push(Opt::Leaf(Term::Var(i)));
            }
        }
        if *ty == Ty::Int {
            opts.push(Opt::Leaf(Term::Lit(Value::Int(small_int(r)))));
            if body >= 1 {
                opts.push(Opt::Leaf(Term::Loop));
            }
        }
        if *ty == Ty::Seq && body == 2 {
            opts.push(Opt::Leaf(Term::Acc));
        }
        for i in 0..lib.entries.len() {
            let (args, ret) = lib.entries[i].signature();
            if Ty::fun(args, ret.clone()) == *ty {
                opts.push(Opt::Leaf(Term::Lib(i)));
            }
            if depth > 0 {
                for j in 1..=args.len() {
                    if Ty::fun(&args[j..], ret.clone()) == *ty {
                        opts.push(Opt::App(i, j));
                    }
                }
            }
        }
        if depth > 0 && *ty == Ty::fun(&[Ty::Int], Ty::Int) {
            opts.push(Opt::Body);
        }
        if depth > 0 && *ty == Ty::fun(&[Ty::Int, Ty::Seq], Ty::Seq) {
            opts.push(Opt::Body2);
        }
        if opts.is_empty() {
            return Term::Lit(random_value(r, ty));
        }
        let k = r.below(opts.len());
        match opts.swap_remove(k) {
            Opt::Leaf(t) => t,
            Opt::Body => Term::Body(Box::new(random_term(r, lib, &Ty::Int, env, 1.max(body), depth - 1))),
            Opt::Body2 => Term::Body2(Box::new(random_term(r, lib, &Ty::Seq, env, 2, depth - 1))),
            Opt::App(i, j) => {
                let (args, _) = lib.entries[i].signature();
                let args: Vec<Term> = args[..j].iter().map(|a| random_term(r, lib, a, env, body, depth - 1)).collect();
                Term::App(Box::new(Term::Lib(i)), args)
            }
        }
    }

    /// Value and fuel left, new and reference; true when the value is Some.
    fn same_as_reference(lib: &Library, t: &Term, env: &[Value]) -> Option<Value> {
        let mut ev = Ev { lib, env, fuel: FUEL, bufs: Vec::new() };
        let new = (ev.eval(t, None), ev.fuel);
        assert_eq!(new, reference::eval_ref(lib, t, env), "{} on {env:?}", show(lib, t));
        new.0
    }

    #[test]
    fn evaluator_matches_the_reference_value_and_fuel() {
        let lib = checked_library();
        let mut r = Rng(20261027);
        let types = [Ty::Int, Ty::Seq, Ty::Bool, Ty::Grid, Ty::fun(&[Ty::Int], Ty::Int), Ty::fun(&[Ty::Seq], Ty::Seq), Ty::fun(&[Ty::Int, Ty::Int], Ty::Int), Ty::fun(&[Ty::Int, Ty::Seq], Ty::Seq)];
        let (mut some, mut none, mut funs, mut out_of_fuel) = (0, 0, 0, 0);
        for _ in 0..60_000 {
            let n_env = 1 + r.below(3);
            let env_ty: Vec<Ty> = (0..n_env).map(|_| r.pick(&types[..4]).clone()).collect();
            let ty = r.pick(&types).clone();
            let depth = 1 + r.below(4);
            let t = random_term(&mut r, &lib, &ty, &env_ty, 0, depth);
            for _ in 0..3 {
                let env: Vec<Value> = env_ty.iter().map(|ty| random_value(&mut r, ty)).collect();
                let v = same_as_reference(&lib, &t, &env);
                let mut ev = Ev { lib: &lib, env: &env, fuel: FUEL, bufs: Vec::new() };
                if ev.eval(&t, None).is_none() && ev.fuel == 0 {
                    out_of_fuel += 1;
                }
                let Some(f) = v else {
                    none += 1;
                    continue;
                };
                some += 1;
                // A function value applied, by reference, owned, and through
                // `apply_value`, to arguments of every arity around its own.
                if matches!(f, Value::Closure(..) | Value::Body(_) | Value::Body2(_)) {
                    funs += 1;
                    let Ty::Fun(params, _) = f.ty(&lib) else { continue };
                    for n in [params.len(), params.len().saturating_sub(1), params.len() + 1] {
                        let args: Vec<Value> = (0..n).map(|k| random_value(&mut r, params.get(k).unwrap_or(&Ty::Int))).collect();
                        let old = reference::apply_value_ref(&lib, &f, args.clone());
                        let mut a = Ev { lib: &lib, env: &[], fuel: FUEL, bufs: Vec::new() };
                        assert_eq!((a.apply_ref(&f, args.clone()), a.fuel), old, "apply_ref {f:?} {args:?}");
                        let mut b = Ev { lib: &lib, env: &[], fuel: FUEL, bufs: Vec::new() };
                        assert_eq!((b.apply(f.clone(), args.clone()), b.fuel), old, "apply {f:?} {args:?}");
                        assert_eq!(apply_value(&lib, &f, args), old.0);
                    }
                }
            }
        }
        eprintln!("evaluator vs reference: {some} values, {none} undefined, {funs} function values, {out_of_fuel} out of fuel");
        assert!(some > 50_000 && none > 10_000 && funs > 5_000, "{some} {none} {funs}");
    }

    #[test]
    fn evaluator_matches_the_reference_at_the_fuel_limit() {
        let lib = checked_library();
        let p = |prim: Prim| Term::Lib(lib.find(&prim).unwrap());
        let app = |f: Term, a: Vec<Term>| Term::App(Box::new(f), a);
        let lit = |x: i64| Term::Lit(Value::Int(x));
        let (mut found, mut starved) = (0, 0);
        // Loops whose last step lands just before, on, or past the fuel limit.
        let bodies = [
            Term::Body(Box::new(Term::Loop)),
            Term::Body(Box::new(app(p(Prim::Arith(Op::Add)), vec![Term::Loop, lit(1)]))),
            Term::Body(Box::new(app(Term::Lib(lib.entries.len() - 1), vec![Term::Loop, Term::Loop]))),
        ];
        for (b, per) in bodies.iter().zip([3usize, 6, 9]) {
            for n in (FUEL / per).saturating_sub(40)..FUEL / per + 40 {
                for prim in [Prim::SumTo, Prim::ProdTo] {
                    let t = app(p(prim), vec![lit(n as i64), b.clone()]);
                    match same_as_reference(&lib, &t, &[]) {
                        Some(_) => found += 1,
                        None => starved += 1,
                    }
                }
            }
        }
        // Iterate and the indexed loop at their bounds, nested to run out.
        let inc = Term::Body(Box::new(app(p(Prim::Arith(Op::Add)), vec![Term::Loop, lit(1)])));
        for k in [0, 1, 63, 64, 65] {
            let it = app(p(Prim::Iterate), vec![inc.clone(), lit(k), lit(5)]);
            same_as_reference(&lib, &it, &[]);
            let nested = app(p(Prim::Iterate), vec![app(p(Prim::Iterate), vec![inc.clone(), lit(64)]), lit(k), lit(0)]);
            same_as_reference(&lib, &nested, &[]);
        }
        let grow = Term::Body2(Box::new(app(p(Prim::Concat), vec![Term::Acc, app(p(Prim::ByteOf), vec![lit(7)])])));
        for count in [0, 1, 255, 1000, 2000, 4096, 4097] {
            let t = app(p(Prim::IterateIdx), vec![lit(count), Term::Lit(Value::Seq(vec![])), grow.clone()]);
            match same_as_reference(&lib, &t, &[]) {
                Some(_) => found += 1,
                None => starved += 1,
            }
        }
        // A macro shares its caller's fuel.
        let twice = lib.entries.iter().position(|e| e.name() == "scan_add_twice").unwrap();
        for len in [0, 10, 1000, 4000, 4096] {
            let xs = Value::Seq((0..len).collect());
            same_as_reference(&lib, &app(Term::Lib(twice), vec![Term::Var(0)]), &[xs.clone()]);
            same_as_reference(&lib, &app(p(Prim::Map), vec![inc.clone(), app(Term::Lib(twice), vec![Term::Var(0)])]), &[xs]);
        }
        assert!(found > 50 && starved > 50, "{found} {starved}");
    }

    #[test]
    fn search_matches_the_reference_search() {
        let lib = checked_library();
        let mut r = Rng(20261028);
        let in_kinds = [vec![Ty::Int], vec![Ty::Seq], vec![Ty::Int, Ty::Int], vec![Ty::Seq, Ty::Int], vec![Ty::Grid]];
        let outs = [Ty::Int, Ty::Seq, Ty::Bool, Ty::Grid];
        let (mut solved, mut unsolved) = (0, 0);
        for case in 0..90 {
            let in_types = r.pick(&in_kinds).clone();
            let out_type = r.pick(&outs).clone();
            // Outputs of a hidden term (so some searches succeed), or noise.
            let hidden = random_term(&mut r, &lib, &out_type, &in_types, 0, 2);
            let examples: Vec<(Vec<Value>, Value)> = (0..2 + r.below(4))
                .map(|_| {
                    let ins: Vec<Value> = in_types.iter().map(|t| random_value(&mut r, t)).collect();
                    let out = if case % 3 == 2 { None } else { eval(&lib, &hidden, &ins) };
                    let out = out.unwrap_or_else(|| random_value(&mut r, &out_type));
                    (ins, out)
                })
                .collect();
            let task = Task { in_types, out_type, examples };
            let mut costs = lib.costs();
            if case % 4 == 1 {
                for c in costs.iter_mut() {
                    *c += r.below(4) as f64 * 0.5;
                    if r.chance(0.1) {
                        *c = f64::INFINITY;
                    }
                }
            }
            let (max_size, beam) = (2 + r.below(3), *r.pick(&[0, 1, 5, 20, 60]));
            let limit = if case % 5 == 3 { r.below(3000) } else { usize::MAX };
            let new = search_priced(&lib, &task, max_size, beam, &costs, limit);
            let old = reference::solve_priced(&lib, &task, max_size, beam, &costs, limit);
            let key = |r: &(Option<Found>, usize)| (r.0.as_ref().map(|f| (f.term.clone(), f.cost.to_bits(), f.enumerated)), r.1);
            assert_eq!(key(&new), key(&old), "case {case}: {task:?} size {max_size} beam {beam} limit {limit}");
            if new.0.is_some() { solved += 1 } else { unsolved += 1 }
        }
        eprintln!("search vs reference: {solved} solved, {unsolved} not");
        assert!(solved > 20 && unsolved > 10, "{solved} {unsolved}");
    }
}
