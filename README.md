# Aether Loom

Aether is a research project: a persistent, general-purpose cognitive architecture that is **not** a language model.
It uses no transformer, no embeddings and no database of answers. It finds small programs that explain the examples
it is given, checks each one on an example it held back, and declines to answer when the check fails.

The **Loom** is the idea of putting that checker next to language models:

1. **Raw intelligence.** Language models propose ideas, code, hypotheses and experiments.
2. **The Loom.** Aether tests, reconciles, rejects and consolidates what they propose.
3. **Persistent brain.** What passed is kept with the reason it works. What failed is kept too, as evidence.

The goal is understanding, not prediction: for each thing kept, what causes what, why, which rival explanation it
beat, and what would prove it wrong.

## The five things Aether should have

Reasoning, thinking, analysis, speech, and learning by itself. The test: put a folder of mixed material in, and
Aether arrives at the wanted result with no agent in the middle.

## Status

Early research, by one person with AI agents. Honest state as of November 2026:

- Reasoning over short exact rules (3 to 5 steps) is the strongest part.
- Long multi-step thinking, open analysis, natural speech and self-directed learning are weak or missing.
- The first Loom tests did not show learning yet: Aether declined measured (noisy) data, and its advice on which
  experiment to run next did not beat random choice.

Negative results are kept and published on purpose.

## What is in this repository

Public documents only. The Core brain, the source code, run evidence and datasets are not here.

- [Vision: Aether as the Cognitive Loom](docs/Vision%20-%20Aether%20Cognitive%20Loom.md)
- [Aether Vision: AGI and RSI](docs/Aether%20Vision%20-%20AGI%20and%20RSI.md)
- [Aether Roadmap: AGI and RSI](docs/Aether%20Roadmap%20-%20AGI%20and%20RSI.md)
- [Aether Goal: The Five Things](docs/Aether%20Goal%20-%20The%20Five%20Things.md)

Task numbers such as T291 refer to entries on the project's internal task board.
