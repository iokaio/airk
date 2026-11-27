# Topical index and real-time reads: research plan

Status: proposed for the user's review, 27 November 2026. The prototype it
describes is authorized ("Let's build the prototype of the topical index and
real-time reads"). Nothing here changes the product until a decision at one
of the gates below.

[Governing strategy](retrieval-strategy-combined.md#strategy-from-27-november-2026) · [Ledger](../simplify/one-process-ledger.md) · [Evaluation tooling](../../scripts/arbiter-answer-eval/README.md)

## Correction of 27 November 2026: the cost figures below were wrong

Every cost comparison in this document made before this section was
written compared arms priced differently, and is superseded by this
section.

**What happened.** A session's reported cost prices a cache write by its
lifetime: $4.00 a million tokens for one hour, $2.50 for five minutes
(fitted from 198 sessions' reported costs, which the five prices reproduce
to the cent: input $2.00, output $10.00, cache read $0.10). The baseline
ran entirely at the one-hour price. Partway through the oracle series the
sessions changed to the five-minute price, and the prototype's series ran
almost entirely at it. The later arms looked cheaper because their cache
writes cost less, not because they used fewer tokens. `agent_cost_batch.py
report` now prices every cache write at one price and prints each arm's
five-minute share.

**The figures at one price** (20 tasks, medians of 3, Sonnet, narrow tasks
weighted four to one):

| Arm | Cost against no tool, mean of per-task percentages | Cost against no tool, in weighted dollars | Round trips |
| --- | --- | --- | --- |
| Oracle: a perfect reply from the truth | 5.7 percent lower | 8.9 percent lower ($0.459 against $0.503) | 8.8 percent fewer |
| Prototype, second version | 16.0 percent **higher** | 7.1 percent **higher** ($0.539 against $0.503) | 5.3 percent fewer |

In tokens, over 60 sessions each: the prototype arm read 35.7 million
tokens from cache against the baseline's 33.6 million, wrote 6.02 million
against 5.66 million, and output 0.86 million against 0.82 million. It
used more of everything.

**Where the money goes**, from the baseline's tokens at the fitted prices:
cache writes 66 percent (every new token entering the context: what the
agent reads, tool replies, its own earlier messages), output 24 percent,
cache reads 10 percent. The earlier estimate in "The ceiling" used
assumed ratios and is replaced by this.

**What it means.**

1. A perfect reply saves 6 to 9 percent on these tasks. That is the
   ceiling for any retrieval design, index or no index, with this agent
   and this reply layout. The stated aim of about 30 percent is out of
   reach here.
2. The prototype finds the right places far more often than the kernel
   reply (74 of 113 truth items at their lines against 32) and still
   costs the agent more than using no tool, because the agent reads what
   it would have read anyway and carries a 24,000-byte reply as well.
3. Gate 2 below is therefore met in its stopping sense: a better reply
   cannot pay on tasks like these. The decision in "If the ceiling stays
   low" is now the open one.
4. Placement results (which files and lines a reply names) do not depend
   on prices and stand as written.

## Experiment protocol (user direction, 27 November 2026)

This replaces the step list and gates further down for the work from here
on. The user set the method; the safeguards are the primary agent's and
the user agreed to them.

**Method.** One task at a time.

1. The primary agent picks a task by judgment from what is known,
   favouring tasks where the oracle shows the most room and the prototype
   falls furthest short of it.
2. For that task it runs a set of three to five experiments using the
   topical index and direct file reads. Each tests one hypothesis, is
   documented with its result, and may be followed by a small adjustment
   before the next.
3. At the end of the set it judges whether further improvement on that
   task is possible. If so it makes the improvement and tests again,
   recording the result.
4. It then picks the next task and repeats.
5. It continues until it can no longer make a real improvement, then
   reports to the user, and the next phase is planned together.

Later phases, agreed in outline: the kept design is run on all 20 tasks
with Sonnet and Opus against a pinned baseline; then on freshly chosen
tasks, measured against the Arbiter as it has evolved; with small general
changes tested between sets. The Qwen 4B model with 8-bit weights is
brought in where an experiment calls for choosing among what is gathered.

**Safeguards.**

- *Ceiling first.* Before building a new reply layout or tool shape, its
  ideal form is run with the oracle. If perfect content cannot save much,
  the build is skipped.
- *Enough repeats.* One run of a task varies by about a quarter of its
  cost. Each condition on a task is run five times, and the unaided
  baseline for that task is brought to five and rerun as a control when a
  day has passed or the cache-write price share changes.
- *One price.* Every comparison uses `agent_cost_batch.py report`, which
  prices all cache writes alike. Tokens entering the context are reported
  beside dollars.
- *Free check before a paid one.* A change first passes the replay
  placement check where that applies.
- *General mechanisms only.* A change is a rule that could help other
  tasks. No value is set from a task's truth.
- *No regression.* When work moves to a new task, the kept changes are
  rerun on the tasks already done.
- *Correctness is the guard.* The primary agent judges every kept
  condition's sessions against the frozen truth.
- *No edits under a running series.*
- *Stop rule.* If a task's best result stays under half of its oracle
  saving after its set, that is recorded as the finding for that task and
  work moves on. If that happens on three tasks running, the series stops
  and is reported.

**Per-task room, at one price** (oracle against the unaided baseline,
medians of three, Sonnet). Second-version prototype beside it.

| Task | Baseline | Oracle | Prototype v2 |
| --- | --- | --- | --- |
| munarium-T1 | $0.726 | 32.1% lower | 5.7% higher |
| munarium-N-W1 | $0.544 | 27.2% lower | 22.7% lower |
| bookbuilder-N-C3 | $0.284 | 25.9% lower | 1.3% lower |
| bookbuilder-C3 | $1.328 | 23.7% lower | 0.5% higher |
| munarium-C2 | $0.435 | 22.5% lower | 19.7% higher |
| munarium-A1 | $0.944 | 17.6% lower | 3.3% higher |
| bookbuilder-N-W1 | $0.762 | 16.3% lower | 1.5% lower |
| munarium-N-A2 | $1.072 | 11.8% lower | 1.6% lower |
| the other 12 | | under 10% lower, or higher | |

**First task: munarium-T1** (add the missing offline retry tests of the
Python client, on both transports). It has the most room, the prototype
captures none of it, and the repository is small, so calls are quick.
Next in line: bookbuilder-N-C3, in the other repository and a narrow
coding task.

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
| Oracle arm: a perfect reply from the truth (60 sessions, medians of 3) | 5.7 percent lower cost at one price (see the correction above; 22.0 percent as first reported was a pricing artifact); round trips 8.8 percent lower |

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

### Second version (27 November 2026)

Aspects 1 to 3 below added to the gather step. Same replay: at the lines
74 of 113 (68 in the first version), file reached 99 (93), file never
reached 14 (20). Every reply is now at the 24,000-byte limit, so further
gain must come from ranking what is gathered. Time per call 2 to 17 s.
Cost series `live2-sonnet-r1` (20 tasks, 3 repeats, Sonnet) measures it.

The first version's cost series was stopped when the prototype was edited
under it. Its 18 sessions that ran the first version alone show about 10
percent lower weighted cost from one repeat a task, which is indicative
only. The four sessions that mixed versions are set aside, not deleted.
Rule from this: the gather code is not edited while a series runs.

### Third version (27 November 2026)

Aspects 4 to 7, 9 and 10 joined to the map and the reply
(`live_gather_v3.py`, kept apart from the second version while its cost
series runs). Map build 27.7 s and 35.7 MB for Bookbuilder, 4.9 s and
5.0 MB for Munarium.

| Placement, 113 truth items | Kernel reply | First | Second | Third |
| --- | --- | --- | --- | --- |
| At the lines | 32 | 68 | 74 | 74 to 75 |
| File reached | 64 | 93 | 99 | 99 |
| File never reached | 49 | 20 | 14 | 14 |

Reading: the six added aspects did not raise placement. They did reach
the items they were built for (the two single rule lines in `CLAUDE.md`,
the token lifetime constant), and each gain displaced another location,
because every reply is already at the 24,000-byte limit. Tightening the
noisiest sections changed nothing. Placement by deterministic rules has
levelled off at about two thirds of the truth at its lines and seven
eighths of the files. What remains is choosing among what is gathered,
which is the reranker's job, or carrying more than 24,000 bytes, which
costs the agent tokens. The items still missed are mostly in two tracing
tasks whose chains cross languages.

## Aspects

Proposed from the 45 truth items the first version missed (in 44 of them
the deciding name was never searched for). The user chose aspects 1 to 7,
9 and 10 on 27 November 2026. Python is accepted for these experiments;
the winning design is migrated to the Rust kernel afterwards.

| # | Aspect | What is recorded or done | State |
| --- | --- | --- | --- |
| 1 | Connections between names | From the best files' hits: who uses the enclosing definition, and the rare names it refers to that another file defines | In the gather step (second version) |
| 2 | Wiring keys | Action names, routes, environment variables and similar strings in the hit ranges, searched everywhere | In the gather step (second version) |
| 3 | File outline | Each best file's definitions with lines, in the reply | In the reply (second version) |
| 4 | Test linkage | Which test file exercises which source file, by name | In the map and the reply (third version) |
| 5 | Values and units | Named literal values with their quantity in a base unit; the same quantity stated in words in documents | In the map and the reply (third version) |
| 6 | Build and configuration keys | Keys of project, settings and script files; a path named in the request is read directly | Keys in the map (third version); path mention in the gather step |
| 7 | Rules by folder and topic | Guidance files by section, and which guidance files govern a folder | In the map and the reply (third version) |
| 9 | Entry points and visibility | Files a trace starts from; public names | In the map and the reply (third version) |
| 10 | Document-to-code links | The code names each document section mentions | In the map and the reply (third version) |
| 8 | Change history | Files that changed together | Not chosen; needs repositories with history |
| 11 | A summary or vector per file | For questions with no name | Not chosen; no current task can show its value |

Aspects 4 to 7, 9 and 10 are joined as the third version in its own
module. The two modules are merged into one after the second version's
cost series finishes.

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
