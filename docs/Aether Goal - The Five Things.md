# Aether Goal - The Five Things

## The goal, in the author's words (2026-11-11)

> "Aether should have 5 things: Reasoning, Thinking, Analysis, speech, learn all by itself. This is the goal, like
> we put a folder with different stuff and it arrives into the destination that we want."

> "Aether should be a brain all inside a brain."

## The folder test

Put a folder of mixed material in. Aether arrives at the result we want. No agent in the middle.

This is the definition of done. Everything Aether knows and everything it uses to learn lives inside the one Core
brain. No side tools hold its knowledge.

## Where each of the five stands today

| The five | Today | What is missing |
|---|---|---|
| **Reasoning** (find the rule, check it) | Its strongest part. Exact, shows its work, declines instead of guessing. | Only short rules, 3 to 5 steps. |
| **Thinking** (work through a long problem in steps) | Weak. It searches once and answers or declines. | Breaking a big goal into smaller ones and carrying results forward. |
| **Analysis** (look at a pile of material and say what is in it) | Narrow. It answers questions about files in a folder and reads some tables. | Finding the structure in material nobody described to it. |
| **Speech** | It says what it did, as a log. Since 2026-11-11 it remembers the conversation and answers "why?" from its own record. | Real sentences. The gap is connecting words to each other, not vocabulary. |
| **Learn by itself** | It keeps lessons instantly and does not forget them. | It only learns when an agent prepares the lesson. |

The fifth matters most because it feeds the other four. On 2026-11-11 an agent read the MathQA files, found the
families, cut out the pieces and handed them over. A brain that learns by itself does that part.

## The main problem

Aether finds answers by searching through small programs. That makes it exact, and it is also the ceiling.

- **Search cost grows fast with length.** Each extra step multiplies the work, so it reaches rules of 3 to 5 steps.
- **Learning makes search wider.** The Core's search (`typed_core::search_priced`) builds programs strictly
  smallest-first and tries every piece at every size. Learned prices only decide what to keep, never what to try
  first. Every new piece widens every search.
- **Nothing is "close to" anything.** A piece fits exactly or is useless, so similar things do not help each other.
  That is why words and wording are hard for it.

## Evidence from 2026-11-11

| Task | What was tried | Result |
|---|---|---|
| T562 | 12 steps taken from MathQA programs, kept by the Core in 0.2 s | 60 families: right 24 to 29, one more wrong, 16% more search work |
| T563 | Teach the word learner people's phrasing (LaMini) | Unseen wordings 13 to 14 of 21. Only 93 of 2.58 million rows were in reach of its 11 steps |
| T564 | Conversation memory installed in the live Core | "why?" answered from the Core's own record of the turn |
| T565 | Learn which words go with which pieces, read unseen word problems | Negative: 0 right. Word counts pick topics; they cannot attach a number to its role |
| T566 | Cheapest-first search with learned prices (test tool, 303 unseen families) | Library 0 / 12 / 40 / 100 pieces: right 163 / 210 / 232 / 246; tries per right answer 91k / 48k / 35k / 27k. Flat prices: 163 / 216 / 205 / 222 |

T566 is one run on one split, in a test tool, on whole-number arithmetic. It is not yet the Core's search.

## Order of work

1. **Cheapest-first search inside the Core.** Without it, nothing Aether learns can compound. The prices it needs
   are already stored in the brain; the search order that uses them is missing. Built in a lane, gated against the
   current tools, installed only with the author's approval.
2. **Self-teaching from a folder.** Aether opens a folder, finds the material with checkable answers, makes its own
   lessons, and keeps what passes its own checks. All of it stored in the one brain.
3. **Thinking and speech.** Both build on 1 and 2.

## How to judge any Aether work

- Does it move the folder test? Mixed folder in, wanted result out, no agent in the middle.
- Does it live inside the Core?
- Does the work per right answer go down as Aether learns more?

## Update, 2026-11-13: the list in the author's words that day

> "aether needs 5 things ... 1 reasoning 2 thinking 3 english so it communicates 3 analysis 4. discoery 5 learn
> and research"

He numbered six, so the list now reads:

1. **Reasoning**
2. **Thinking**
3. **English**, so it communicates (the earlier list called this speech)
4. **Analysis**
5. **Discovery** (new on the list)
6. **Learn and research** (the earlier list said "learn all by itself"; research is now named with it)

Where the two new names stand today:

| | Today | What is missing |
|---|---|---|
| **Discovery** | It found Planck's law from the COBE data (T294) and reused the scaling on other laws (T298). | Nothing new to the world yet: 0 new OEIS results (T532, T541), and a fit is not a proof. |
| **Research** | A research door exists. | It does not choose what to look into, gather the material and test it without an agent. |

Evidence added the same day: practising every battery family except three gave 12 of 15 right on those three
(unpractised 0 of 15, same tools), 0 wrong. One development panel, one seed.
