# Vision: Aether as the Cognitive Loom

**Status:** a vision with first experiments (November 2026). This is a research project with measurable
milestones, not a guarantee.

## The idea

"The LLMs generate possibilities. Aether turns tested possibilities into a coherent, persistent body of knowledge
and capabilities."

Three levels:

1. **Raw Intelligence.** Several LLMs generating ideas, code, hypotheses and experiments.
2. **Aether, the Cognitive Loom.** Tests, reconciles, invalidates, verifies and consolidates discoveries.
3. **Persistent Brain.** Established knowledge, reusable abilities and verified experience. The accumulated learning
   informs the next experiments.

The name comes from the Loom in the series *Loki*, with one difference: "it shouldn't destroy every alternative."
Failed experiments stay as evidence, and new discoveries may challenge old assumptions.

**The tube:** ideas enter → experiments test them → evidence filters them → verified results become reusable
capabilities → experience influences the next ideas. Aether is not only filtering: it should also learn which
approaches are worth trying next.

## The research question

"Can intelligence accumulate across models, rather than disappear whenever a model's context window ends?"

The continuity belongs to the brain rather than to the LLM. **The Loom is the continuous mind**: it holds the
history, the failed experiments, the causal understanding and the open questions. Claude, Gemini or GPT is the
reasoning cortex plugged into it for one session.

The Loom wakes a fresh model with a structured working-memory state, not a raw log dump:

- what is established, and why;
- what failed, and when to reconsider it;
- what the causal model predicts next;
- what is still uncertain.

**Honest provenance:** which file hash a thing was verified on, which model ran it, and whether it went stale. The
difference between "I proved this on the current build" and "I remember this from an older version, let me recheck."

## Three phases

The original Aether Core stays separate and untouched throughout.

1. **1-LLM Cognitive Loom.** An isolated trial runner plus a persistent state store (invariants, callable
   procedures, invalidated attempts, human approval receipts) governing one fresh LLM across separate sessions.
   Pass bar: 0 silent regressions of established work across N fresh sessions; failed attempts rolled back and not
   repeated.
2. **Multi-proposer Loom.** Several LLMs submit into isolated lanes, tested against the same witnesses. Only the
   verified winner is woven in; losers are kept as evidence. Pass bar: beats a single LLM on pass rate and
   consistency.
3. **Governed Neural Organ.** A small domain network trained only on the Loom's verified episode history, promoted
   or rolled back on unseen material. Pass bar: beats baseline with 0 regression on approved cases.

Two constraints:

- "The Loom is only as strong as its witnesses." Proposers must never weaken the tests.
- Build the Loom before the Neural Organ, because the Loom creates the Neural Organ's dataset.

Mechanical correctness can be checked automatically. Artistic quality still requires a human's approval.

## Understand, not predict

The Loom keeps explanations: what causes what, why, the rival explanation it beat, and what would prove it wrong.
Prediction scores are measuring tools, not the product.

## What has to be measured

Preventing regressions proves continuity. Evidence of learning needs more: does Aether actually become better at
choosing which solutions to attempt? So every experiment needs a control from the first day: the same sessions with
Aether's chooser off or in random order.

"Preserving knowledge is achievable engineering. Developing transferable intelligence through repeated experience is
the ambitious, unproven part."

## Evidence so far (November 2026)

**What fits the Loom role.** The Core checks a rule on a held-back example before answering, gives nothing instead
of guessing, keeps a receipt for each answer, and reused a piece it had grown itself on a fresh puzzle.

**Gaps.** It checks only small exact things in text. It has no door for pictures or video. It learns only from
lessons handed to it. Its check is not airtight (one wrong answer was marked checked).

**On "better at choosing", the claim is open.**

- Against: on 60 clean whole-number rules, Aether's "measure this next" advice did not beat random choice (4.57
  against 4.66 examples in a narrow pool; 5.03 against 4.81 in a wide pool). Earlier runs found learned strategy
  preferences cost 4.01% more search, and reuse cost 25.3% more.
- For: in one test-tool run, tries per right answer fell from 91k to 27k as the library grew from 0 to 100 pieces.
- On real measured data (brightness readings from a visual effect), Aether declined 36 of 36 predictions. The reason
  is understood: its rule finder has no tolerance for noise, so one example off by one is declined.

## Limits stated from the start

Aether's program search works on small symbolic problems and cannot search billions of weights. It can verify
measurable properties but cannot tell whether an image looks beautiful without a perceptual evaluator or a human.
