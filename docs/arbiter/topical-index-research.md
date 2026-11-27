# Topical index and real-time reads: research plan

Status: proposed for the user's review, 27 November 2026. The prototype it
describes is authorized ("Let's build the prototype of the topical index and
real-time reads"). Nothing here changes the product until a decision at one
of the gates below.

[Governing strategy](retrieval-strategy-combined.md#strategy-from-27-november-2026) · [Ledger](../simplify/one-process-ledger.md) · [Evaluation tooling](../../scripts/arbiter-answer-eval/README.md)

## Why this research

The user's question, 27 November 2026: is indexing every file in full the
wrong foundation, and would a small topical index of files, with search and
parsing done on the live files at request time, serve an agent better?
Files that cannot be read live (PDF, Office, archives) would still be
indexed in advance.

What was measured that day, on 20 tasks over two repositories:

| Measurement | Result |
| --- | --- |
| Agent with plain file tools (Sonnet, 60 sessions) | 60 of 60 correct; about $0.50 a task; median 10 round trips |
| Current Gather Context reply, 113 truth items | 7 carried, 16 named at their lines, 9 nearby, 32 file only, 49 file never reached |
| Index lookups by exact name, 97 named truth items | definition lookup 84, exact lexical search 79, either 90 |
| Index semantic search, given the truth's own description | 62 of 97 |
| Oracle arm: a perfect reply from the truth (60 sessions, medians of 3) | 22.0 percent lower cost weighted four to one for narrow tasks; cheaper on all 20 tasks; round trips 8.8 percent lower |

Three readings follow.

1. **Exact names and parsed structure find the evidence; ranked semantic
   chunks do not.** The first two need no index of file contents: exact
   search over live files and parsing a file both take milliseconds.
2. **The largest gap is the file, not the line.** For 49 of 113 items the
   current reply never reached the right file. A map of files is aimed at
   that gap.
3. **A perfect reply saves about a fifth, and the agent's own checking is
   what limits it.** Given a perfect reply, agents still made about 15 reads
   and 14 searches a task, against 15 and 16 without it. They check before
   they act, and a file must be read before it can be edited. The saving
   came from reading less in each read (153 KB against 192 KB) and one
   fewer round trip. See "The ceiling" below. This limits any design, this
   one included.

Correction, same day: the first reading of the oracle, taken when one
repeat of each task had finished, was about 5 percent weighted. With all
three repeats the figure is 22.0 percent. A single repeat of a task varies
by a quarter of its cost, so the early figure should not have been stated
as a result.

## The ceiling

Where an unaided session's cost goes, estimated from its token counts with
the usual price ratios (a cache read one tenth of an input token, a cache
write one and a quarter, an output token five):

| Part of the cost | Share | Can a retrieval tool reduce it? |
| --- | --- | --- |
| New content entering the context (tool results the agent reads, its own messages) | about half | Only by making the agent read less in total |
| The agent's output (edits, answers, reasoning) | about a quarter to a third | No |
| Context read again on later round trips | about a fifth to a quarter | Yes, by fewer round trips and a smaller context |

An earlier ledger entry said re-read context carries almost all the cost.
That is true in tokens and wrong in money, and it is corrected there.

So a tool can only win by making the agent read and search less. The oracle
shows that correct passages alone do not do that: the agent reads the files
anyway. What may do it is the part of a reply an agent cannot cheaply
verify for itself and has reason to trust: a complete census ("these are
all 14 places; 9,812 files were searched") and exact ranges to read in
place of whole files. The prototype is built to test that.

## Hypotheses

- **H1, file recall.** A deterministic file map plus live exact search
  reaches the right file for at least 90 percent of truth items, against 57
  percent for the current reply.
- **H2, line recall.** Reading around live hits and parsed definitions
  names the right lines for at least 70 percent, against 28 percent.
- **H3, cost.** A reply that is a complete, trusted map lowers agent cost
  by more than the oracle's passages did. This is the only hypothesis that
  decides anything, and it needs a paid series.
- **H4, no content index.** None of H1 to H3 needs embeddings or a stored
  copy of file contents for text files.

## What is built

All of it is evaluation tooling in `scripts/arbiter-answer-eval/`, in
Python, over the frozen repository snapshots. It is a research prototype
and not a second implementation of the product: nothing is ported until a
gate says so.

### 1. The file map (the topical index)

One small record per file, built by one pass over the snapshot and
rebuilt for a file when its hash changes. No embeddings. Dimensions:

| Dimension | Content | How it is derived |
| --- | --- | --- |
| Identity | path, size, lines, hash, language | file system and extension |
| Role | source, test, mock, configuration, documentation, build, generated, binary | path and name rules, then content markers |
| Defines | names of types, functions, constants, headings, keys | light per-language patterns; no full parse |
| Uses | identifiers and imports the file refers to | tokenization; imports by pattern |
| Wiring | routes, handlers, commands, registrations, test targets | patterns per framework seen in the two repositories |
| Area | feature words from the path, headings and most frequent identifiers | split identifiers into words; rank by rarity across the repository |
| Rules | the nearest guidance files that govern the file's folder | walk up from the file |

Inverted tables over Defines, Uses and Area give "which files" in one
lookup. Expected size is a few hundred bytes a file.

### 2. Real-time reads

Given an agent's call (task, needs, anchors, deliverable):

1. **Terms.** Take the anchors as given. Derive more from the task and
   needs: identifiers, quoted strings, numbers with units, and feature
   words. Expand each through the map to the names files actually define
   (`retention` to `DefaultRetentionDays`, `logRetentionDays`).
2. **Census.** Exact search of the live files for each term, whole
   repository or a folder scope, all occurrences counted. Record what was
   searched and how many files, so an absence can be stated.
3. **Files.** Rank files by how many distinct terms they hold, weighted
   by rarity and by role for the deliverable (tests for a test task).
4. **Ranges.** In the top files, parse on demand: the enclosing
   definition of each hit, its signature, and for a named definition its
   callers by exact search. Merge into line ranges.
5. **Rules.** Add the governing guidance sections for the folders
   touched.

No model is used. Where a choice among ranges is needed, the first version
uses counts and rarity; the reranker dose can be applied later through the
existing dose step.

### 3. The reply

The same brief layout the service returns, so both can be served to an
agent and scored by the same tools, with two changes the oracle result
argues for:

- a **census** section first: each term, its total occurrences, the files
  holding it with line numbers, and the count of files searched;
- **ranges before passages**: every range to read, with one line saying
  what is there; source text only for short ranges, so the reply does not
  duplicate what the agent will read anyway.

## How it is measured

| Step | Tool | Cost | Answers |
| --- | --- | --- | --- |
| A | `gather_context_call_replay.py` with the 20 recorded first calls | none | H1, H2: the same six-stage placement as the current reply's 7 of 113 |
| B | Map size, build time, and time per call | none | H4, and whether it is practical |
| C | An oracle arm with the new layout (census and ranges, no long passages) | about $25 | whether the layout raises the ceiling before any retrieval is judged |
| D | The prototype served to agents: 20 tasks, 3 repeats, Sonnet | about $30 | H3 |
| E | Correctness of D, judged against the frozen truth | none | the guard |

Steps A and B run on every change. C and D run once each unless a result
calls for a repeat. Opus and the ten held-out tasks come only if D shows a
gain worth confirming.

## Results so far

### Step A and B, first version (27 November 2026)

`topical_index.py` and `live_gather.py`, first version, no tuning pass, no
model, no content index. Replay of the same 20 recorded agent calls, 113
located essential truth items:

| Placement of a truth item | Current kernel reply | Prototype |
| --- | --- | --- |
| Carried as a passage | 7 | 11 |
| Named at its lines | 16 | 31 |
| Named at overlapping lines | 9 | 26 |
| **At the lines, total** | **32 (28%)** | **68 (60%)** |
| Only the file named | 32 | 25 |
| **Pointed at the file, total** | **64 (57%)** | **93 (82%)** |
| File never reached | 49 | 20 |

| Practical measure | Bookbuilder (8,080 text files) | Munarium (1,017) |
| --- | --- | --- |
| Map build, one pass | 8.8 s | 1.8 s |
| Map size as JSON | 32 MB | 4.4 MB |
| Time per call, files already in memory | 3 to 6 s | 1 to 2 s |

Readings:

- H1 passes gate 1 (82 percent of files against a bar of 80). H2 is at 60
  percent against a target of 70.
- The map is about 4 KB a file, ten times the size expected. Most of it is
  the list of defined names with lines. It is still far smaller than a
  content index, and it holds no file text.
- The per-call time is pure Python over files held in memory, scanning
  every file for every term. It says nothing about a real implementation.
- Caution on tuning: the prototype was written by someone who had seen
  these tasks, although no value in it was fitted to them. The ten
  held-out tasks are the check for that.
- This is placement, not cost. It says the reply points at the right
  places far more often. Whether agents then spend less is step D.

## Gates

1. **After A.** If file recall is under 80 percent, the map's dimensions
   are wrong: revise them before anything else.
2. **After C.** If the new layout's oracle saves under 10 percent
   weighted, a better reply cannot pay on tasks like these. Stop and
   decide where AIRK should be aimed (see "If the ceiling stays low").
3. **After D.** If the prototype reaches at least half of the oracle's
   saving with correctness held, write the product plan: what replaces the
   content index, what stays, and the migration. That plan needs the
   user's approval, because it changes persisted formats and the indexing
   contract.

## If the ceiling stays low

The evidence so far comes from mid-sized repositories, coding tasks and a
strong agent with local file access. There, an agent's own tools are close
to the best case. Places a prepared map could matter more, none measured
yet:

- repositories too large to search in one pass, or not checked out;
- questions with no name to search for;
- documents that cannot be read live;
- several repositories at once;
- weaker or cheaper agent models, which search less well.

Choosing among these is the user's decision and would come with its own
task set.

## What this does not decide

- The existing index, kernel loop and Arbiter stay as they are while the
  research runs. The merged gathering fixes and the brief reply stay in
  the branch.
- The model-dose experiment is paused, not dropped: it is applied to
  whichever gathering wins.
- Earlier direction said not to prototype the loop in Python again. This
  prototype is a different question, a no-index comparison the user asked
  for on 27 November 2026, and it is recorded here as the exception.

## Open questions for the user

1. Is the ceiling result enough to change the goal? The stated win was
   about 30 percent lower cost. A perfect reply in the current layout
   saves 22 percent, so 30 percent needs a reply that also changes how
   much the agent checks for itself, which step C tests.
2. Should the task set gain harder cases (larger repositories, questions
   with no name) before judging the design, since the current tasks are
   ones a plain agent already does well and cheaply?
3. Is a Python prototype acceptable through gate 3, with Rust only after
   the product plan is approved?
