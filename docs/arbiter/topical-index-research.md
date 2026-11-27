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
- *Spot-check regression in every round (user, 27 November 2026).* In
  each task's round, the condition that is kept is also run on tasks
  completed in earlier rounds, three repeats each: every earlier task
  while there are three or fewer, then three chosen to include the one
  that gained most and the one that gained least. A kept change that
  makes an earlier task dearer by more than its repeat-to-repeat spread,
  or less correct, is recorded as a regression and is not kept as a
  general rule until it is resolved. In the first round, with no earlier
  task, the spot checks are two tasks not yet worked on: one where the
  oracle showed room (bookbuilder-N-C3) and one where it showed none
  (munarium-N-C2), to see early whether a change that helps one task
  hurts a cheap one.
- *Correctness is the guard.* The primary agent judges every kept
  condition's sessions against the frozen truth.
- *No edits under a running series.*
- *Record every case where cost goes up, and learn when to decline
  (user, 27 November 2026).* Whenever a condition makes a task dearer
  than its unaided baseline, the case is added to the table "Where the
  tool costs more" below with what could have been known at the time of
  the call: what the request names, how many terms and files the census
  finds, the deliverable, and the reply's size. When the cases fall into
  recognizable patterns, the tool is given a rule that declines such a
  request with an error such as "gather_context cannot process this
  request effectively", so the agent does that task itself and the tool is
  used only where a gain is probable. A decline rule may use only what
  the tool can see in the call and in its own census, never a task's
  baseline cost or truth. Declining is itself tested as an experiment:
  a declined call costs the agent one round trip, and that cost is
  measured on the tasks the rule would decline.
- *Teach the agent when not to call (user, 27 November 2026).* The same
  cases are used to rewrite the tool's description, so the agent itself
  chooses the tool only where a gain is likely. This is better than a
  decline where it works, because a call that is never made costs
  nothing, while a declined call costs a round trip. In `tool-firstcall-r1`
  every one of the 20 agents called the tool in its first round trip,
  including on the tasks where it can only lose: the current description
  says "use first", and the agents do. A description is tested like any
  other change: the measure is cost, and beside it the share of sessions
  that call the tool on tasks where it should and should not be used.
  The decline rule stays as the backstop for calls the description does
  not prevent.
- *Later experiment: a description that follows the repository (user,
  27 November 2026).* For the prototype one description is overridden by
  hand for a test. In the product the description may need to be
  composed from what the topical index finds: a repository that is
  mostly documents may need different words from one that is mostly
  Python, C# or Rust. To try once a description has been found that
  works on the current two repositories: derive a few facts from the map
  (share of files by role and language, how many guidance files, how
  large the repository is, whether tests sit beside sources) and test
  whether a description built from them beats the single best fixed
  one, on repositories of different make-up. It needs at least one
  document-heavy repository and one in a language not yet covered, so
  it belongs with the fresh-task phase.
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

### Round 1: munarium-T1

All conditions five repeats on Sonnet, at one price, compared by
`agent_cost_task.py`. The unaided baseline is $0.689 (0.54 to 0.80),
11 round trips, 154 KB read through the agent's own tools.

**Set 1: which layout, with perfect content** (ceiling first).

| Condition | Hypothesis | Median cost | Against no tool | Spread | Round trips | Own reads | Reply |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Passages (the original oracle) | reference | $0.493 | 28.6% lower | 0.46 to 0.74 | 9 | 110 KB | 10.4 KB |
| Ranges with Read offsets, no text | A small reply steers the agent to partial reads | $0.552 | 20.0% lower | 0.47 to 0.81 | 9 | 131 KB | 2.1 KB |
| Ranges plus "read only these" | Telling the agent to stay in the ranges cuts whole-file reads | $0.587 | 14.9% lower | 0.47 to 0.64 | 10 | 131 KB | 2.3 KB |
| Passages plus "do not read these again" | Text replaces reads if the agent is told it may rely on it | $0.408 | **40.8% lower** | 0.36 to 0.49 | 8 | 85 KB | 10.7 KB |

Findings:

1. Text in the reply beats pointers to it. With ranges only, the agent
   reads the files anyway, and mostly whole: the small reply saves nothing
   that the reads do not spend again.
2. A line saying the passages are the exact current text and need not be
   read again takes the saving from 29 to 41 percent and makes it
   steady: the five runs fall between $0.36 and $0.49, where every other
   condition has a run near the baseline.
3. The same kind of line on ranges does not help. Telling the agent what
   not to do works only when the reply already holds what it would
   otherwise fetch.
4. Kept for this task: passages with the line.

**Before set 2: the prototype's ranking is the weak point.** A focused
layout was added to the prototype (`live_gather.focus`): the ranges of the
six best files only, their text within 14,000 bytes, the line, a compact
census, and no outline or path list. On the replay it carries 23 truth
items as passages, up from none, and loses most of the rest: 26 of 113 at
their lines and 42 files reached, against 74 and 99 for the full layout.
The cause is the order of files: only 33 of the 85 distinct truth files
are among a task's first six ranked files, and 48 among the first 16; the
rest are found by the census and never ranked high enough to be given
ranges. So the layout that pays needs a precise short list, and the
prototype's list is not precise. Ranking is the next thing to improve,
and it is the reranker's natural place.

**Set 2: what the agent is told about the reply** (munarium-T1, five
repeats each, one price; guidance confirmed as served in each session's
record).

| Guidance served | Perfect passages | Prototype, focused layout |
| --- | --- | --- |
| Build's guidance with "Untrusted index; verify live bytes before edits" | 9.4% lower (0.47 to 0.85) | 5.9% lower (0.59 to 0.72) |
| Build's guidance without that sentence | 17.9% lower (0.45 to 0.68) | 10.2% higher (0.64 to 0.82) |
| No guidance | 28.6% and 20.0% lower (two sets; 0.43 to 0.88) | 1.8% higher (0.58 to 0.85) |
| No guidance, reply says its passages need not be read again | 40.8% lower (0.36 to 0.49) | not run |

Findings:

1. Two identical five-run sets differ by about nine points on this task,
   so neighbouring cells are not distinguishable.
2. With perfect passages the order is the one the user expected: the
   distrust sentence worst, its removal better, no guidance better
   still, a trust line in the reply best. Only the ends (9 against 41
   percent) are clearly apart.
3. With the prototype's content the sentence makes no visible
   difference: all three cells sit within noise of the unaided cost. A
   reply that carries two of five truth items is not rescued by what the
   agent is told about it.
4. The build's guidance without the sentence does no better than no
   guidance. Its other lines ("gather_context first", tools this harness
   does not offer) are candidates for the selective description test.

**Decline rule, free check (step 2).** `gather_decline.py check` on the 20
recorded calls: the first rule declined none. Two faults in the rule,
neither in the idea: the wide-intent words match needs the agent wrote
("every", "everywhere", "across" appear in 11 of 20 calls, including
the cheap tasks), and the file count is taken from the census of terms
the agent's own text names, which are common names spread over 11 to 34
files even on one-file tasks. Next version to try: judge width from the
task text alone, not the needs; count the files that *define* the named
symbols (from the map) rather than every file that mentions them; and
treat a task that names a test target and an example to copy as narrow.
The tasks it must decline are munarium-N-C2 and bookbuilder-N-C4; the
tasks it must let through are munarium-T1, munarium-N-W1, bookbuilder-N-C3,
bookbuilder-C3 and munarium-C2.

**Set 3: the allowed assurance** (munarium-T1, five repeats, one price,
no guidance, the build description of the earlier sets, series
`t1-oracle-assure-r1`). On 27 November 2026 the user allowed one editorial
assurance, worded "These passages are exact text taken from the files
referenced.", and nothing else. Served with perfect passages it gives
$0.566, 17.9 percent lower (0.43 to 0.66), 9 round trips, 136 KB read by
the agent itself. Passages with no line gave 20.0 percent (122 KB) and the
longer line 40.8 percent (85 KB).

Finding: the assurance alone does not change what the agent does; it is
within noise of no line. The saving of the longer line came from its
instruction ("Do not read those ranges again and do not search for more"),
which the no-editorial rule now excludes. Whether to allow that
instruction is the user's decision.
**Set 4: the allowed instruction** (same conditions, series
`t1-oracle-noreread-r1`). The user then allowed "There is no need to read
these ranges again." after the assurance. Result: $0.589, 14.6 percent
lower (0.54 to 0.76), 10 round trips, 133 KB read by the agent; within
noise of no line and of the assurance alone.

Finding: what moves the agent is not being told the passages are exact or
need not be re-read, but being told they are all the task needs. A traced
session shows why: after the call the agent still runs 12 searches and 12
reads, most of them whole central files, looking for what else might
matter. The 40.8 percent line said "together they are what this task
needs ... do not search for more"; with a perfect reply that claim is
true, but a real reply can only make it when its coverage supports it.
**Set 5: a completeness line** (same conditions, `--oracle-complete`,
series `t1-oracle-complete-r1`). Appended to the build's line, only when
the reply is complete and every need covered: "These passages cover every
need listed; there is no need to search for more." Result: $0.529, 23.3
percent lower (0.44 to 0.81), 9 round trips, 122 KB read by the agent;
within noise of no line. All five sessions judged correct.

Finding: neither the assurance, the no-reread instruction nor the
completeness claim moves the agent. The one part of the 40.8 percent line
not yet tested on its own is how to read: "read a file only for lines that
are not shown here, with offset and limit". Its sessions read 85 KB
against 122 to 136 KB in every other passage condition.
**Decline rule, second version (rubric approved by the user, 27 November
2026).** Decline only when the task text asks for nothing wide, names a
defined symbol or a path, and the named symbols are defined in three files
or fewer (a widely defined name is ignored beside a narrow one; a dotted
name counts by its shape; a language the task names limits the files to
that language). The needs are not read, and files that only mention a name
do not count. Refusal, user wording: "This request is specific enough that
gather_context is unlikely to be a good fit. Read files directly instead."
Free check on the 20 recorded calls (`gather_decline.py check`): declines
munarium-N-C2 and bookbuilder-N-C4 as required and lets every required
task through; it also declines munarium-N-C3 (dearer in both arms) and
bookbuilder-N-A1 (2 to 5 percent cheaper with the tool). Next: serve the
decline on the two required tasks and compare with no tool.
**Decline served** (series `decline-sonnet-r1`, live arm with
`--live-decline`, five repeats each, one price, 10 of 10 judged correct).

| Task | No tool | Perfect reply | Prototype reply | Declined |
| --- | --- | --- | --- | --- |
| munarium-N-C2 | $0.119 | 6.8% dearer | 74.8% dearer | 4.4% dearer |
| bookbuilder-N-C4 | $0.136 | 10.0% dearer | 82.9% dearer | 36.9% dearer |

Findings:

1. Declining removes most of the loss: one short extra call instead of a
   20 KB reply. On munarium-N-C2 it is close to no tool.
2. On bookbuilder-N-C4 the refusal's file list was wrong: it named the Vue
   views that define the task's setting flags, not the handler the task is
   about. Defining files of a named field are not the task's files. The
   agents ignored the list.
3. On bookbuilder-N-C4, after the refusal, agents read the project's
   `AGENTS.md` and `CLAUDE.md` (10 KB each), which no unaided session read,
   and one read a 34 KB test script. A hypothesis, not tested: the tool
   description says the tool "brings the project's rules", so a refusal
   prompts the agent to fetch them itself. The unaided baseline has three
   repeats, so this comparison is noisy.
4. Recommended: the refusal without a file list, the user's sentence only.
### Resume point (27 November 2026)

Work can be picked up from here by another session.

- *State.* Branch `feat/arbiter-final`, nothing pushed. Background agent
  on branch `agent/remove-untrusted-message` is removing the distrust
  sentence from the Rust guidance and Workbench warning; review, merge,
  regenerate and validate when it reports, then rebuild the fixture
  image only when the real service is next needed.
- *Round 1, munarium-T1, where it stands.* Layout kept: passages with a
  trust line. With perfect content it saves 41 percent; the prototype's
  focused layout is at break-even because its ranking puts only two of
  five truth items in the reply.
- *Next actions, in order.* (1) Improve the prototype's ranking of files
  and ranges, deterministic rules first, then the Qwen 4B reranker
  through `arbiter_judge_service.py` and `gather_context_dose.py`; check
  each change with `live_gather.py replay --layout focus` before paying
  for a series. (2) Add the trust line to the focused layout's `use` text
  only once the content justifies it. (3) Fix the decline rule as above,
  re-run the free check, then serve it on munarium-N-C2 and
  bookbuilder-N-C4. (4) Run `d1-selective` on the same two tasks and on
  munarium-T1, recording how often the tool is called. (5) Run the
  round's spot checks (bookbuilder-N-C3, munarium-N-C2) with the kept
  condition. (6) Judge correctness of every kept condition's sessions
  with `agent_cost_judge.py`. (7) Move to bookbuilder-N-C3.
- *How to run a condition.* The private helper
  `passes/arbiter-brief-image-r1/series.ps1` under the campaign folder:
  `-Series <name> -Arm plain|oracle|live -Tasks <id> -Repeats 5 -Variant
  none|d0-build|d0b-no-untrusted|d1-selective [-Maps topical|topical-v3]
  [-Extra '--live-layout,focus']`. Compare with `agent_cost_task.py`.
  Oracle and live arms need no fixture.
- *Rules that cost a day to learn.* Compare only at one price
  (`agent_cost_batch.py report`, `agent_cost_task.py`). Five repeats a
  condition; nine points between identical sets is noise on this task.
  Do not edit `live_gather.py`, `topical_index.py`, the runner or the
  server while a series runs. Name a variant on every series and check
  `gather_context.advertised` in a session before trusting a guidance
  result.

### Where the tool costs more

Started from the three 20-task series already run, at one price, medians
of three. A minus sign means dearer than no tool.

| Task | Unaided cost, round trips | Oracle | Prototype v2 | Prototype v3 | What the task is |
| --- | --- | --- | --- | --- | --- |
| munarium-N-C2 | $0.12, 6 | -6.7% | -74.8% | -56.1% | Add tests for one named method, beside named existing tests |
| bookbuilder-N-C4 | $0.14, 7 | -9.9% | -82.8% | -90.2% | Add tests for one named handler |
| bookbuilder-N-A2 | $0.24, 6 | -2.3% | -28.6% | -31.1% | Answer a question naming one script and one version number |
| munarium-N-C3 | $0.34, 9 | -4.8% | -15.1% | -43.9% | Add a filter to one named page |
| munarium-N-C1 | $0.53, 10 | -17.4% | -34.1% | -24.0% | Change one constant everywhere it is stated |
| munarium-N-C4 | $0.44, 8 | +7.9% | -25.8% | -8.4% | Add one method like a named existing one |
| munarium-C2 | $0.44, 10 | +22.5% | -19.7% | -2.4% | Find and fix a named symptom |
| bookbuilder-T1 | $0.35, 7 | +6.1% | -8.8% | -20.1% | Add tests for one formatter |

First patterns, to be confirmed or dropped as cases accumulate:

1. **The task is already short.** The two dearest losses are the two
   cheapest tasks, six or seven round trips unaided. A reply of 24,000
   bytes costs more to carry than those tasks cost to explore. Even the
   perfect reply loses on them. What the tool can see: the request names
   the one file, type or method to work on, and the census finds its
   terms in very few files.
2. **The request already says where.** Five of the eight name the exact
   symbol, page, script or example to follow. The agent goes straight
   there; the reply adds files it then also opens.
3. **Every-place changes of one value** (munarium-N-C1) lose even with
   the perfect reply. The agent re-verifies every place itself, so the
   reply is paid for twice. Bookbuilder-N-C2 is the same kind of task and
   came out even, so this pattern is not settled.

Candidate decline rule to test, from patterns 1 and 2: decline when the
request names a specific symbol or path, the census finds every named
term in three files or fewer, and no need asks for "every place" or a
trace.

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
