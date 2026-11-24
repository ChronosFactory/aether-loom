# Core excerpts

Three source files from the Aether Core (Rust), published to read, not to build. They are excerpts from a larger
crate and depend on modules that are not in this repository, so they do not compile on their own.

| File | What it is |
|---|---|
| `typed_core.rs` | The typed program language the learners share, and the search that builds small programs from a library of typed pieces. |
| `self_revision.rs` | The Core revising the settings of its own search by experiment: it makes exams from its own examples, trials changes on one fold, and keeps a change only if a held-out fold shows nothing lost. |
| `learner.rs` | Learning one world with advisers weighted by two-part code length (Bayesian model averaging), under a thinking budget. |
