# Topical index and real-time reads: research plan

Status: proposed for the user's review, 27 November 2026. The prototype it
describes is authorized ("Let's build the prototype of the topical index and
real-time reads"). Nothing here changes the product until a decision at one
of the gates below.

[Governing strategy](retrieval-strategy-combined.md#strategy-from-27-november-2026) · [Ledger](../simplify/one-process-ledger.md) · [Evaluation tooling](../../scripts/arbiter-answer-eval/README.md)

## Proposed amendment for review on 28 November 2026

**Accepted 28 November 2026.** The user authorized implementation of this plan through a decision about taking the solution into the Rust kernel, including further improvements supported by evidence. Review local results before selecting an end-to-end candidate. Keep the public contract and Rust port as separate product decisions.

**State at review.** The superseded native fixture closed and its service lease was reacquired. Fourteen historical Sonnet sessions cost $1.0075331. None of the four candidate research sessions called Gather, and the native arm enrolled superseded weights. They cannot establish current Arbiter performance. Keep the ordinary-tools results under their original input identities. The single-Arbiter local replay started under the preceding instruction and stopped at the review request after two complete Bookbuilder tasks (A1 and C3) of twenty. Both passed the independent prompt and delivered-source audit. The partial JSONL SHA-256 is e884a62d497f6a520667cd2ca33a481a99b0ace0caeac2e8cc29864382ceed36. The selector has three passing synthetic tests. No comparative adaptive-budget replay has run. These pilot results establish harness operation only.

### 1. Establish the model and input identities

Use one inference stage, Arbiter, with the pinned Qwen3-Reranker-4B revision, 8-bit block-rounded weights expanded at load for FP16 computation on CUDA. Deterministic code finds candidates and assembles replies. Sonnet is the external calling agent in a later end-to-end trial. Keep the seven-tool shipping skill naturally available in the AIRK conditions.

Before scoring, verify checkpoint shard and tokenizer hashes, actual loaded model class, precision, device, quantization census, yes/no tokens and the publisher's system prefix and assistant suffix. Reject a mock, CPU fallback or superseded model. In end-to-end work, record the model behind every routed tool, including ordinary search, and distinguish offered tools, actual invocations, model judgments, failures and evidence delivered to the caller. An unknown submission retains its original error and replay status.

Freeze caller inputs, source revisions, exclusions, candidate paths, tie order, budgets and independent grading inputs. Exclude denied paths before any source read or map build. All arms use the same 40 eligible file identities per task. Retain raw sanitized extraction differences separately. Removing two denied Bookbuilder files shifted two C3 pool positions and a deep reference rank from 853 to 851. Pinning allowed file identities isolates the scoring experiment; report any remaining divergence from historical rankings or evidence. Grading notes never enter candidate selection or a model input.

### 2. Test the scoring question at a fixed output budget

On the 20 development tasks, compare the existing bundled task/needs/anchors file query against one need per judgment with task context and the same cards. Preserve the trained wrapper. Give each need a turn to nominate a file and deduplicate the result. This tests scoring and the aggregation needed to turn multiple scores into one reply. Run both a full 40 files per need and a fixed 40 total judgments per call. The latter uses a reproducible allocation by pool position for this experiment; its result is specific to that allocation. Report actual GPU invocations separately from identical pairs reused from cache. Retain standalone timing checks for both preselected A1 tasks.

At the fixed pair count, compare labeled task/need/anchor fields, removal of anchors from reranking while preserving retrieval anchors, a direct-evidence instruction that admits useful partial evidence, and line-complete excerpts with a verified declaration or heading when available. Keep the full task for the first comparison. Consider shortening it only if measured token allocation or clipping warrants it. The trial uses 1,024 input tokens and caps each scoring excerpt at 480 characters. A prior reconstructed input used 586 tokens and fit. A later evidence-kind prompt or score threshold needs its own justification and held-out calibration.

Separate the short range Arbiter scores from the full source range AIRK returns. Record the need, both ranges, publication/source identity, rendered input digest, score and returned text digest. Preserve a decoded model input per condition. File nominations and passage links are distinct. Declare unsatisfied needs explicitly and never equate a positive score with proof that a need was answered. Keep six files and 14,000 UTF-8 **passage** bytes constant during prompt comparison. Returning only the scoring excerpt made a whole reference structurally unreachable for 35 of 86 scorable needs in the design audit.

### 3. Test passage allocation without further inference

Replay identical scored nominations against fixed 14,000 and 20,000 passage-byte ceilings, a 20,000-byte control with need-first ordering, and the proposed adaptive selector with the same upper bound. Use the need-first control to distinguish the effect of ordering from the effect of stopping. Give an unrepresented need first opportunity for a complete range that adds a new nonblank line. For additional ranges, initially require at least half of the nonblank lines to be new by source location and normalized text. Treat that fraction as a frozen test hypothesis. Preserve original source ranges, deduplicate, enforce file and byte limits, and report omissions and reasons. Matching text at distinct locations can be meaningful, particularly for callers; inspect losses on such cases.

The product hard limit is the caller's budget for the **whole serialized MCP reply**, including metadata and citations. The offline ceilings above measure source text alone. Before proposing a product default, reserve envelope space, serialize and verify the complete reply against the request, and test small caller limits and multibyte text. This allocation requires no second inference stage. The earlier 20-task cap increase recovered six additional reference notes at 29.5% more passage bytes, while some tasks spent more and gained nothing.

### 4. Judge delivered evidence and decide

An independent checker reconstructs scorer inputs and verifies the bytes actually returned at their cited paths and ranges. Report full and partial reference delivery, per-need coverage, omissions, unsupported coverage claims, file-only pointers, pair counts, clipping, GPU time and bytes. Count eligible needs separately: of 98 caller needs, 86 have positive located references, 11 lack a positive mapping and one has only absence evidence. Report these limitations and per-task gains and losses, including broad and narrow strata. A location or a range that was merely proposed receives no delivered-text credit.

Prefer the least expensive variant that maintains evidence quality. Advance a change that adds verified evidence at a justified cost, or holds quality while reducing work. Review consequential per-task regressions explicitly. If a larger cap or a new prompt only adds bytes or GPU work, keep the simpler control. These are development results. Preserve the sealed checkpoint for independent confirmation and bring the paired evidence to the user before spending on agent sessions.

### 5. Validate the agent workflow before a Rust decision

After review, compare ordinary tools with the selected single-Arbiter prototype on matched tasks. If comparing two Gather implementations, require both to serve the approved single-stage model, or label them explicitly as two prototype implementations. A qualifying native 4B implementation is currently unavailable. Reuse historical controls only when inputs, environment, pricing and grading remain comparable. Any fresh evaluation should use unseen tasks for its generalization claim.

Use the same acting model, permissions, test access and stopping rules. Use genuine indexed editable workspaces for changes, verify publication after edits, and let the agent run tests, inspect failures and repair in the same session. Run independent final checks. Preserve bypass and typed failures as observations. Primary outcomes are independently verified final correctness and total caller spend through completion. Report first-pass results, cache pricing, round trips, local GPU time and startup costs separately. The existing aspiration is about 30% lower average caller cost with correctness held. A Rust port decision follows review of those results, including source isolation, freshness, budgets and error semantics.

**Resume after approval.** Verify the partial receipt and exact source, harness, scorer and grading identities. Continue missing tasks into a fresh receipt and require unique coverage. If an input or behavior changed, rerun the affected cases under a new identity and explain why. Then run the budget replay and bring the paired report to review. The saved pilot is never silently counted twice.

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
4. The user dropped the file list the same day; the refusal is the user's
   sentence only.

**Ranking, first rule (free checks, no agents).** `rank_check.py` reports
where each task's truth files fall in the prototype's file ranking (first
recorded call of each of the 20 tasks, 168 located truth files). Baseline:
43 in the first six (the focused layout), 23 more in the first sixteen
(the full layout), 77 later, 25 never reached by a request term. Documents
almost never rank in the first sixteen (2 of 22). The first rule: when the
task text names a language ("the Python client", ".NET"), code files in
other languages rank at 0.3 of their score, but only under a top folder
that also holds files in the named language. That penalizes sibling
clients, not the server a client calls; munarium-N-C4, a .NET task whose
route is in the Rust server, lost two server files to a first, unscoped
version. Result: 47 in the first six, 25 more in the first sixteen, 71
later, 25 unreached; no truth file fell to a worse group. Focused replay
(`replay/live-focus-lang-r1`): 28 of 113 notes at the lines (was 26), 43
pointed at the file (was 42). The language table moved from
`gather_decline.py` to `live_gather.py`, which both now use. Next ranking
targets: the 25 files no request term reaches, and documents.
A second rule was tried and rejected: searching every second-hop term
(not 24 cut alphabetically) at 0.15 weight. Files improved (47 in the
first six, 29 more in the first sixteen, 20 unreached) but the focused
replay (`replay/live-focus-hop-r1`) fell to 9 of 113 notes at the lines
and 28 pointed at the file, at twice the time per call. Hop terms also
add their lines to the files that hold them, which steers passage choice;
`rank_check.py` cannot see that, so every rule must also pass the replay.
The handoff page names the score-only version to try next.

**Ranking and focused passages, Windows free replay, 27 November 2026.** Each condition used the same first calls on 20 tasks, 168 task/file pairs and 113 located essential notes. The control reproduced all 113 retained note classifications. “At lines” includes a qualifying passage, location or partial overlap under the existing replay classifier. “At file” includes those line hits. Counts describe the recorded first reply. Agent correctness and spending require a separate series.

| Condition | First six | Ranks 7–16 | Unreached | At lines | At file |
| --- | ---: | ---: | ---: | ---: | ---: |
| Retained language rule | 47 | 25 | 25 | 28 | 43 |
| Extra hop terms, score only | 46 | 30 | 20 | 29 | 43 |
| Sibling tests by source name | 47 | 25 | 24 | 28 | 43 |
| Nearest unique ancestor project | 47 | 25 | 23 | 28 | 43 |
| Boost documents with a literal request match | 41 | 36 | 25 | 25 | 42 |
| Prefer direct terms when choosing passages | 47 | 25 | 25 | 28 | 43 |
| Give each of six files a first turn | 47 | 25 | 25 | 30 | 52 |
| Offer focus the locations before full-layout trimming | 47 | 25 | 25 | 28 | 44 |
| Both focus changes | 47 | 25 | 25 | 30 | 53 |
| Both focus changes with 14-line excerpts | 47 | 25 | 25 | 29 | 53 |

Extra score-only hops found five reference files at ranks 113–283 and exchanged three first-six gains for four losses. Their mean gather time was 9.57 seconds versus the retained run’s 5.3 seconds. Direct document matches moved 12 of 22 document references into the first sixteen, up from two, at the cost of six first-six references overall. File links recovered one sibling test at rank 109 and two project manifests at ranks 87 and 90. None changed an essential note classification.

The most promising condition gives six files a first turn and selects from ranges before the full reply trims them. First-six membership stayed identical on every task. Four notes gained passages and two fell from passage to file, on munarium-C2 and munarium-N-C4. The missed count fell from 70 to 60. Keep this as a replay candidate. Its effect on agent cost and correctness is open. Uniformly shortening excerpts reduced line hits from 30 to 29 and fully qualifying passages from 28 to 26.

On munarium-T1 the retained ranking already places the three files containing all five essential notes at ranks 1, 2 and 6. The full reply reduces 62 candidate locations to ten and excludes every location from the sixth file, `_specs.py`. With both focus changes the reply points to files for all five notes and still covers only two at their lines. Even the entire 62-location pool qualifies only those same two. The next experiment should change how candidate ranges are chosen within files. Use each caller-supplied need to nominate a range, deduplicate overlaps, and compare against the present four ranges per file under the same limits. Check all 20 calls for lost passages. Hold the paid series until authorized.

**Stage trace and file-nomination trial, 27 November 2026.** The 20 control calls reproduced all 113 note classifications for both the original focus layout and the retained all-spread candidate. Search terms hit a reference interval for 89 notes. Accepted terms hit 83. The complete merged ranges of the sixteen selected files overlapped 61 notes (55 meeting the coverage threshold and six partial). The four-range cut retained overlap for 49. In the first six ranked files the corresponding figures were 46 and 38. The focused all-spread reply reached 30 at the lines. A nearby term can expand into a reference interval, so raw hits and range overlap measure different things.

On munarium-T1, N1 and N6 reached the reply as passages. N2 in `rest.py` had an accepted hit and a partial overlap before the cut. N4 in the same file had an accepted hit and a fully qualifying range. Both lost their ranges at the four-per-file cut. N5 in `_specs.py` had no search hit in its reference interval and no overlapping range.

Across the working set, 23 notes with accepted hits had primary files ranked below sixteen and no overlapping range in the selected files. The trial targeted this larger loss first. Each caller-supplied need nominated one file using rarity-weighted word matches against file paths and accepted terms. The trial alternated original file leaders with distinct nominees before selecting sixteen files. It kept the existing range selector, six-file focus layout and passage budget.

| Measure | Retained all-spread | File nomination |
| --- | ---: | ---: |
| Reference files in first six / 168 | 47 | 41 |
| Reference files in first sixteen / 168 | 72 | 72 |
| Essential notes overlapping a merged range / 113 | 61 | 61 |
| Notes at their lines in focused reply / 113 | 30 | 26 |
| Notes qualifying as passages / 113 | 28 | 24 |
| Notes pointed to at least at file level / 113 | 53 | 48 |

Reject file nomination. It added four first-six references and displaced ten. Two of the 23 excluded notes gained a qualifying range, and only Bookbuilder T1 N5 reached the reply as a passage. Munarium T1 remained at two passages and N5 lost its file pointer. The next free trial should keep file order fixed and let the needs nominate **ranges within each file** before the four-range cut. Score N2 as partial unless the nominated passage meets the existing coverage rule. N5 needs a separate search or expansion hypothesis. For the planned local Qwen 4B file-reranker comparison, 15 of the 23 excluded notes across ten tasks have their primary files at ranks 17–40. Eight lie outside its proposed 40-file window. This is candidate availability only.

**Budget-constrained passage oracle, 27 November 2026.** Each feasible selection could contain at most six files, four exact ranges per file and 14,000 UTF-8 passage bytes. The following ceilings use independent optimizations for fully qualifying passages and for any overlap. The offline solver sees the frozen references, and the replay classifier checked the selected ranges and their byte counts.

| Candidate pool | Fully qualifying notes | At least partial overlap |
| --- | ---: | ---: |
| Retained all-spread replies | 28 | 30 |
| First six files, current four range candidates per file | 36 | 38 |
| First six files, any merged range | 42 | 46 |
| First 40 files, current four range candidates per file | 60 | 65 |
| First 40 files, any merged range | 70 | 77 |

The first six files supplied 14 of 42 recoverable fully qualifying notes (33.3 percent), below the predeclared majority trigger for a deterministic passage-diversity trial. Including partial overlaps gave 16 of 47 (34.0 percent). The wider pool raised the fully qualifying ceiling on 15 of 20 tasks. The 60-note ceiling with the existing four range candidates per file assumes perfect passage packing as well as file selection. Qwen remains unmeasured.

Munarium-T1 is an exception to the aggregate result. The retained reply fully covers two of five notes. Both pools top out at three fully qualifying notes (N1, N4, N6), or four counting partial overlap (adds N2). Expanding the pool to 40 added no attainable coverage on this task. The study measures the first recorded replies and provides no new agent cost or correctness evidence.

**Local Qwen3-Reranker-4B, first 40 files, 27 November 2026.** The 20 original calls reproduced the retained all-spread controls on all 113 essential notes and 168 task/file pairs. Qwen scored 800 file cards and reordered the existing first 40. Both arms used the same six-file, four-range and 14,000-byte passage limits. Each card contained the file path and the first 480 characters from each of up to four original candidate ranges. The query contained the recorded task, needs and anchors.

| Measure | Deterministic | Qwen |
| --- | ---: | ---: |
| First-six reference files / 168 | 47 | 64 |
| First-sixteen reference files / 168 | 72 | 86 |
| Fully qualifying passages, by essential note / 113 | 28 | 40 |
| Notes at least partially reached at their lines / 113 | 30 | 43 |
| Notes pointed to at least at file level / 113 | 53 | 68 |

Seventeen notes gained qualifying passages and five lost them. On the full-passage measure 11 tasks improved, six tied and three regressed. Among the 15 previously excluded notes whose primary files were eligible at ranks 17–40, Qwen promoted 13 into the first sixteen and nine into the first six. Seven became qualifying passages. The earlier 60-note ceiling includes ideal packing as well as ideal file choice.

Munarium-T1 stayed at two fully qualifying notes out of five. Its N5 file fell from sixth to thirteenth and lost its pointer. The largest regression was munarium-N-W1, from three qualifying notes to zero. Two notes in reports_api.rs became file pointers as it moved from first to fifth. The API-types lib.rs fell from fifth to twenty-fifth and its note disappeared. Bookbuilder-N-C2 lost a passage when its file moved from second to eighth. Munarium-N-A1 lost a passage as its file moved from first to second. A file making the shortlist does not guarantee that its essential range fits the reply.

The A6000 loaded and block-rounded the local weights in 15.16 s. Warm scoring of 40 pairs took a median 3.31 s per task (2.27–4.26 s), 66.04 s total across the 20 calls. Three of 800 pairs hit the 1,024-token limit. The timing covers tokenization and scoring. The replay shared deterministic scans between arms and provides no independent end-to-end candidate latency. Keep this as the stronger replay candidate. The regression cases and paired agent cost and correctness remain open before a port decision.

**Qwen passage budget, 27 November 2026.** Replayed all 20 saved top-40 orders at passage caps of 14,000 and 20,000 bytes. The 14,000-byte controls reproduced every note and byte count. Fully covered notes rose from 40 to 46 of 113 across five tasks, with no note regressing. Three additional missed notes gained a file pointer. Actual passage bytes rose from 254,878 to 330,032 across the calls (29.5 percent). Bookbuilder-T1, Munarium-A1, C2 and N-W1 gained one qualifying passage each, and Munarium-N-A1 gained two. Munarium-T1 stayed at two fully covered notes while passage bytes rose from 13,984 to 19,965. Retain the 14,000-byte default for Aardvark. The larger cap is a candidate for measuring actual model spending and correctness on a task where it changes coverage.

**Protected leader and Aardvark live parity.** Using the original deterministic leader first and Qwen for the remaining order produced 41 fully covered notes against plain Qwen at 40. Five tasks changed. Bookbuilder-N-A2, Munarium-N-C4 and Munarium-N-W1 each gained one fully covered note. Bookbuilder-N-W1 and Munarium-N-C2 each lost one. The rule recovered Munarium-N-W1 N4 but failed the no-new-task-regression criterion. Keep plain Qwen and the 14,000-byte limit.

Of the five passages lost to plain Qwen, Bookbuilder-N-C2 N2 fell outside the six focused files and Munarium-N-W1 N6 fell outside the sixteen-file range pool. Relaxing only the byte cap restored the other three (Munarium-N-A1 N11 and Munarium-N-W1 N4 and N5). The leader rule restored N4 fully and N5 only partially. The 20,000-byte experiment above remains separate.

A real loopback Qwen scorer and stdio MCP tool reproduced the 40-file order, exact source text, ranges, and full reply except elapsed time on Munarium-T1 (7 passages, 13,984 bytes), Bookbuilder-N-C3 (11, 9,739) and Munarium-N-C2 (18, 12,050). The first paid Aardvark launch was rejected by automatic approval review before a session started because it requires explicit authorization for the private task and source evidence sent to Anthropic. The user subsequently authorized this transfer for Aardvark.

From 27 November 2026 every agent test series is named after an animal or
insect, each starting with the next letter of the alphabet. The first is
Aardvark.
### Aardvark Sonnet results

On 27 November 2026 the user authorized the private evidence transfer for this series. All 22 sessions completed on Windows with Claude Code 2.1.330 and `claude-sonnet-5-5`. The live arm offered Gather Context with local Qwen3-Reranker-4B, a 40-file rerank and a 14,000-byte passage cap. The change in cache-write lifetime triggered contemporaneous unaided controls. The 22 session receipts reported $2.4532 in total. The common-price fit reproduced their costs and valued all cache writes at $4 per million tokens.

| Task (repeats per arm) | Median cost unaided → offered | Range unaided / offered | Median trips | Median Read calls | Median entering tokens | Correct unaided → offered | Called when offered |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Munarium-T1 (5) | $0.1900 → $0.1866 | $0.167–0.212 / $0.179–0.225 | 8 → 7 | 5 → 5 | 25,399 → 26,614 | 1/5 → 0/5 | 5/5 |
| Bookbuilder-N-C3 (3) | $0.1026 → $0.0958 | $0.081–0.122 / $0.082–0.109 | 7 → 8 | 3 → 4 | 13,573 → 10,777 | 3/3 → 3/3 | 0/3 |
| Munarium-N-C2 (3) | $0.0875 → $0.0853 | $0.085–0.096 / $0.085–0.088 | 7 → 6 | 5 → 3 | 11,251 → 11,237 | 3/3 → 3/3 | 0/3 |

Munarium-T1 saved 1.8% at the median, inside the observed spread. Its historical $0.689 unaided median would overstate the saving. The median live reply was 22,221 bytes including metadata. Own-tool reads fell from 46,209 to 27,689 bytes. The five agents still made 21 Read calls after Gather, including 12 to files already excerpted. These counts include reads of other ranges in those files. On their actual queries, deterministic replay supplied eight fully covered passages across 25 essential-note opportunities and Qwen supplied ten. Every Qwen reply omitted the retry-class spec definition and lacked a qualifying passage for the retryability and delivery predicates.

Direct review and offline pytest against the frozen client found four assisted patches and four controls reused HTTPX responses across sync and async cases. The fifth assisted patch expected an untyped HTTP 500 to be retried. The successful control passed all 68 cases. Every timeseries patch passed its six tests. All six Bookbuilder edits covered the shortcut on both tab states in source review, compiled both changed Vue scripts and templates, and passed the modified shortcut test assertions. Browser behavior was not exercised. Both spot tasks went unaided even when the tool was offered. Their results measure tool adoption only with respect to Qwen.

Hold the Rust port. Before further paid ranking or budget comparisons, use the successful control and the failed patches to identify which repository passages could support a working offline test. Check their availability on the five saved queries. A fresh oracle comparison with executable correctness grading would establish how much benefit the current agent can realize from ideal evidence.

### Badger skill results

On 27 November 2026, five interleaved pairs used the frozen Munarium-T1 task, Sonnet 5.5, the same Qwen-backed Gather Context tool and the 14,000-byte passage limit. The control offered MCP alone. The treatment enabled the AIRK skill and explicitly asked Sonnet to invoke it. The private checkout held identical source in both conditions. The launcher supplied the skill from the AIRK evaluation plugin. All ten sessions completed with zero permission denials and tool errors. Their reported cost totaled $1.61885, excluding the earlier synthetic smoke tests.

| Metric | MCP offered | MCP offered with explicit skill |
| --- | --- | --- |
| Fully correct patches | 4/5 | 2/5 |
| Sessions calling Gather Context | 0/5 | 5/5 |
| Median cost at one cache-write price | $0.19175 | $0.18671 |
| Cost range | $0.17235–0.26474 | $0.18550–0.21832 |
| Median round trips | 8 | 9 |
| Median Read calls | 5 | 6 |
| Median input plus cache-write tokens | 28,366 | 24,907 |
| Essential notes shown before first edit, out of 25 opportunities | 23 | 20 |

All five explicit sessions successfully loaded the intended skill, requested Gather once and received qualifying passages for the same two notes. They followed up with 3–8 Read calls (median six). Numbered source text from Gather, Read and Grep supplied four of five essential notes before the first edit in every explicit run. The retry-class definition remained missing under the campaign's line-coverage measure. The controls used ordinary file tools and reached all five notes in three runs. Evidence coverage measures source shown to the agent and does not establish the cause of a test failure.

Direct patch review and offline execution against the frozen Python client found that all four partial patches failed async tests after reusing HTTPX response instances. The control patches passed completely in repeats 1, 3, 4 and 5, and the explicit-skill patches in repeats 3 and 5. The skill increased MCP adoption and lowered median normalized cost by 2.6%, inside the observed spread. The correctness result ends this skill trial before natural activation, cross-client replication or default installation. The next useful comparison remains ideal evidence versus contemporaneous controls, graded by executing the patches.

### Caribou paired evidence results

Ten matched pairs repeated the frozen Munarium-T1 retry task on Windows with Sonnet 5.5, local file tools and natural discovery of the AIRK skill. All sessions saw the same corrected native HTTP MCP guidance and seven complete tool definitions. The six companion tools queried the frozen Munarium publication. Gather supplied either the live Qwen-ranked packet or hand-selected source passages. Each agent received the same task prompt and could choose its own tools. Patch verification used the frozen client and an independent review of both transports, request counts, command key reuse, gateway errors and plain writes. A two-session pilot with outdated native tool names was excluded before grading. The user replaced the skill after pair 3 and requested separate reporting.

| Skill offered | Evidence arm | Fully correct patches | Gather used | Fresh-response example from Gather | Median round trips | Median reported cost |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Original, pairs 1–3 | Qwen | 2/3 | 3/3 | 0/3 | 9 | $0.1650 |
| Original, pairs 1–3 | Hand-selected | 3/3 | 3/3 | 3/3 | 7 | $0.1159 |
| Revised, pairs 4–10 | Qwen | 1/7 | 4/7 | 0/7 | 8 | $0.1501 |
| Revised, pairs 4–10 | Hand-selected | 2/7 | 6/7 | 6/7 | 7 | $0.1338 |

All 20 sessions completed in 6–10 of 80 permitted round trips. No transcript shows a Skill invocation or a direct Read of SKILL.md. The descriptions were discoverable. The chronological cohorts cannot estimate the effect of changing the skill text. This is one task repeated across sessions. In the original cohort both arms passed pairs 1 and 2. Only the hand-selected arm passed pair 3, where the live patch had a pytest collection error. In the revised cohort both arms passed pair 5. Only the hand-selected arm passed pair 10, where it bypassed Gather. The other five pairs failed in both arms.

**Cause and repair.** All eleven failing revised patches hit the HTTPX async stream assertion. In pair 4 the live patch shared ready-made responses in pytest parameters. The hand-selected patch created fresh failures but shared success responses as module constants. Both patterns let a real sync or async client bind the response stream before the other client receives the same object. The hand-selected packet included two legitimate repository examples: a sync-only retry responder that returns a scripted response and a responder used by both modes that constructs a new response per call. Five of the six revised hand-selected sessions that received the latter example before editing still failed.

Each reproduced async case passed alone. Running both modes in either order failed on the second mode. Reconstructing a fresh buffered response inside the test responder changed the full live module from 54/58 to 58/58 and the hand-selected module from 40/52 to 52/52. Assertions and client code remained identical. The original patches retain their original verdicts.

**Campaign decision.** Keep the retrieval budget and weights fixed for the next comparison. The acting prompt explicitly forbids running tests and gives the agent no shell. A useful next comparison gives a solver the full changed-module pytest result and one repair turn, then independently regrades the module and coverage. This could test whether executable feedback improves completion. The current results establish the fixture repair and do not measure the benefit of a different prompt or specialized model weights.

### Dragonfly matched repair results

Of 40 attempts at one Munarium coding task, 15 passed on the first attempt. Each of the 25 failing patches received two independent Sonnet repairs, starting from the same archived patch. One agent saw the full-module pytest failure and the other received the task and patch without that diagnostic. Both repaired the same 24 patches. Keeping the 15 initial passes, verified completion rose from 15/40 to 39/40 in each condition, a gain of 60 percentage points.

| Incremental repair metric | Without pytest output | With pytest output |
|---|---:|---:|
| Fully passing repairs | 24/25 | 24/25 |
| Total model cost | $4.357053 | $1.294704 |
| Total round trips | 207 | 88 |
| Median round trips | 8 | 3 |
| Median elapsed seconds | 83.2 | 30.9 |

Raw feedback reduced the incremental repair cost by 70.3%. In the remaining case, both repairs fixed collection, after which pytest exposed three async failures. Each session had one repair opportunity. This series measures repeated patches for one task. It does not estimate a cross-repository success rate.

All 50 original and final grade records were checked against 300 artifact digests. No original test function disappeared or gained a skip. Twenty-two repairs changed assertions or decorators and were flagged for semantic review. On two representative passing repairs, disabling read retries caused 18 failures per repair, allowing gateway command repeats caused six, and allowing plain write retries caused twelve. The checks establish sensitivity for those two repairs. The frozen private summary is agent-cost/Dragonfly/verified-repairs.json (SHA-256 ed08146b02b9c5415a0df9dc45f1374dadf616994ed8ecdccd045a4594f38bdf).

### Six-task end-to-end checkpoint disposition

The user confirmed the single-stage Arbiter design during the checkpoint. The historical native fixture had enrolled two superseded models. This invalidates the intended comparison with current Arbiter. Paid launches stopped after 14 of 18 sessions. None of the four candidate research sessions invoked Gather, so no paid observation in this panel measures Qwen3-Reranker-4B file scoring. The correct [working model decision](retrieval-strategy-combined.md#strategy-from-27-november-2026) specifies Qwen3-Reranker-4B with 8-bit weights and FP16 computation. The prototype approximates storage quantization by rounding in blocks and expanding at load.

| Actual condition | Sessions | Full passes | Reported actor spend |
|---|---:|---:|---:|
| Ordinary file tools and public verification | 6 | 4 | $0.3705207 |
| Historical native fixture, research tasks only | 4 | 4 | $0.3256852 |
| Candidate offered, two research tasks with one ancillary search each | 2 | 2 | $0.2093055 |
| Candidate offered, two research tasks using ordinary reads alone | 2 | 1 | $0.1020217 |

Total spend was $1.0075331. On research alone the ordinary-tools arm passed 2/4. Both ordinary-tools code patches passed independent behavior checks and their regression tests failed against stock. The historical native broad Bookbuilder Gather returned direct_observation_unavailable. Both candidate ancillary searches returned the same typed failure, with unknown submission and zero evidence delivered to Sonnet. The other two candidate research sessions made no AIRK call. The initial Bookbuilder N2 candidate post-run inventory failed, and a separately authenticated later observation matched its prior eligible bytes, full checkout, publication and public authority. No additional paid attempt was made. The private partial grade manifest SHA-256 is 7f40e9ef68cb71b6031dd437c4040005dc7a3bcebadb2dcf6283433ce8f905be. Preserve all six ordinary-tool outcomes for reuse on exactly matched inputs.

For future model-bearing comparisons, bind the actual serving model identity, precision and enabled auxiliary routes to the single-stage decision before any paid actor launch. Trace actual inference separately from availability. The local 4B preflight made 40 judgments. Its separately reconstructed example for the REST transport file used 586 tokens without clipping. The scorer documentation identifies the exact instruction, training-template boundaries, query assembly and excerpt limit. The user's approved skill remains enabled and unchanged. Review the disposition before funding any replacement sessions or deciding on a Rust port.

### Measuring normal AIRK coding work

The user expects coding agents using AIRK to have test access in nearly all
ordinary coding sessions. For future end-to-end coding comparisons, give
both arms the same ability to run relevant tests, inspect diagnostics, edit
and rerun tests within the same session. Keep task, model, starting source,
test permissions, stopping rule and independent final grader matched. Change
the AIRK retrieval condition being studied. Use verified task completion
and total model cost through verification as primary outcomes. Record first
patch correctness, repair attempts, test commands and round trips to explain
differences. The historical shell-free runs measure first-pass work; a
separate fresh-session feedback experiment measures one repair opportunity.

#### Proposed comparison before selecting the kernel implementation

Use the existing broad/narrow task mix and its agreed weights. Select held-out
changes and questions from multiple repositories. Keep the repeated Munarium
retry task as a diagnostic case outside the held-out headline. Include work
where the task already gives a sufficient local path, a change crossing
multiple files, and a citation question with incomplete evidence. Freeze
instructions, source revisions, independent checks and scoring before seeing
candidate results.

For the kernel decision, compare current native Gather Context against the
candidate on the same seven-tool surface. Keep the skill, client, shell,
test permissions, model revision and stopping rule identical. A separate
ordinary-tools control measures the end-to-end benefit of enabling AIRK.
Keep hand-selected packets as a diagnostic upper bound for retrieval quality.
Test genuine natural discovery in the client configuration the product will
use. Exercise an indexed follow-up after an edit, including publication or
freshness behavior. Verify observable defaults, budgets, errors and
continuations on the real MCP connection.

For coding, count verified final correctness and total agent spend from
task receipt through the last verification, including unsuccessful attempts
and repair cycles. For research questions, check cited evidence against
the frozen source. Keep independent acceptance checks out of the solver
prompt. Where the risk warrants it, test a representative patch against
deliberately broken behavior in an isolated copy. Report voluntary use and
bypass, per-task gains and regressions, round trips, and uncertainty across
distinct tasks. Interleave conditions and record cache state. Report local
reranker runtime and resource use separately from agent billing. Repeat on
a second supported client before generalizing a discovery or skill result
across clients. This is a proposed next campaign, not part of Dragonfly's
50 paid repair sessions.

### Historical resume point before Aardvark (27 November 2026)

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
