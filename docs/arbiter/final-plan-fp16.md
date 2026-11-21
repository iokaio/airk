# AIRK Arbiter plan: does Answer help an agent do software work?

Rewritten on 21 November 2026 (America/Denver) at the user's direction. It
replaces the FP16 delivery plan that stood in this file until then. The file
keeps its name so that existing links still resolve.

Status: the user read this plan on 21 November, corrected it (change sets in
place of single changes, and a report for every series), approved its main
readings and had me adopt four points from an outside review (section 2).
Later that day the user chose the agent and judge models, set one commit per
series and set three iterations before a stop for manual inspection (section
2, "Set by the user after review").
**Execution has not started.** It waits for the user's explicit word. Section
9 lists every place where I interpreted an instruction and which of those
the user has confirmed.
[Parent plan](final-plan.md) · [Ledger](../simplify/one-process-ledger.md)

## 1. The question

The user stated the purpose of this work on 21 November:

> Did the "answer" tool improve substantially the results for an AI agent
> tasked with a common set of ten software development instructions that one
> might encounter, including architectural, writing, documentation, testing,
> and coding.

Two criteria decide it, in this order.

1. **Fewer turns for an agent.** Does Answer greatly improve the information
   returned, so that an AI agent needs fewer turns to get the context it
   needs for the task?
2. **Useful detail for a person.** Lower priority. Given the same prompt, is
   a human more likely to find detail in the Answer result that helps them
   make better choices than in the `parse` and `search` results?

Everything else in this plan serves that question. A model, a weight file, a
route or a setting earns further work by moving one of those two criteria on
real repositories.

## 2. What changed on 21 November

### Retired

The earlier plan tested against publicly sourced question sets. The user's
judgment is that those tests were not getting closer to the goal, and I
agree: I reported their results as if they answered the question above, and
they do not. These are retired as acceptance evidence and as the basis for
developing weights:

- The 100-row calibration panel and the fitted cutoff for Context actions.
- Admission of other routes by drift on those 100 rows with a guard band, and
  the confirmation capture.
- The paired FP16-to-FP32 comparison with a 98% reference point, its FP32
  evaluation arm and the human review of discordant pairs.
- The TechQA stage comparison and the 142-case judged cohort.

Their receipts stay in the ledger as history. One thing they did establish is
still useful as engineering knowledge and is carried into section 3.7: how
Answer, Focus and Context behave mechanically today.

### Kept

The delivery engineering is unchanged and is listed in section 10: one FP16
weight file per model, FP32 arithmetic on it, the CPU and CUDA routes, WebGPU
as the later route for Radeon and macOS, GPU use as an opt-in setting, and
typed reasons for every fallback.

### New product rule for Large

The user decided this on 21 November. It amends the CPU ladder of approval A2.

- **Large never runs on CPU in production.**
- **Without GPU opt-in, Large is not offered.** If the user has not turned
  GPU use on, Compact is the only Context option.
- **In tests, Large on CPU is a measurement only.** It runs on one scenario
  in five, with a maximum of 180 seconds.

### Adopted from review

The user passed on an outside review of this plan on 21 November and asked me
to adopt what I judged worth adopting. I adopted all four of its points.

- **The benchmark stays out of its own index** (section 3.1). Otherwise a
  search could retrieve this plan, the scenario file or a report and appear
  to solve a task in one turn.
- **A turn count ends at a correct finish** (section 4.1): the evidence
  establishes every essential fact, and the agent's brief gets those facts
  right. The room there is for a win is reported with the baseline
  (section 4.3).
- **Each model must earn its place** (section 4.4). Every report shows what
  Focus adds over Answer with no model, what Compact adds over Focus, and
  what Large adds over Compact.
- **Shared search fusion leaves the first change set** (section 5.3), unless
  the baseline shows that essential material falls outside the candidate
  pool.

### Set by the user after review

The user set these on 21 November, after reading the plan with its open
questions.

- **The agent is headless Claude Code on Sonnet** (`claude-sonnet-5-5`). Every
  scenario session of section 3.5 runs that way.
- **The judge is headless Claude Code on Opus** (`claude-opus-5-5`). Every
  judge session of section 4.5 runs that way. My adjudication is separate
  from those sessions and is labelled as mine.
- **One commit per series.** Each full set of test reports and evaluations
  is one commit, so that commits and their results can be compared one to
  one (section 5.6).
- **Three iterations, then a stop.** Three change sets are built and tested
  back to back, with no decision between them. After the third is committed,
  work stops for the user's manual inspection. At each inspection the user
  may set the number of further iterations, more or fewer (section 5.2).
- **My defaults for everything else.** The defaults I proposed for the
  remaining open points are adopted. They are recorded where they apply: the
  private folder (section 3.1), the product rule for Large (section 7), and
  readings 12 and 13 of section 9.
- **Weights train on both repositories** (section 5.4), for a broader fit to
  general programming work.
- **No deadline is set.** Work is reported at each stop.

## 3. The test

### 3.1 Service and indexes

The user's running AIRK service is not used. Answer does not run there, and
this branch has changed a great deal beyond Arbiter.

- **Service.** A private evaluation service built from the branch as it
  builds today. It is the same service runtime as the product, hosted in a
  test image, because the installed product refuses the unqualified model
  profiles this work uses.
- **Indexes.** Two new indexes with MiniLM vectors, one for
  `D:\code\github\airk` (Rust, TypeScript and documentation; 10,457 tracked
  files) and one for `D:\code\github\bookbuilder` (WritingForge Pro: C# and
  .NET with a Vue and TypeScript front end; 8,250 tracked files).
- **Read-only sources.** Nothing is written into either repository. Index
  data, service state, session logs and raw results live under the private
  campaign folder
  `artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1/`, which is
  git-ignored. The indexer honours
  `.gitignore`, which should keep build output and `node_modules` out; step
  E1 checks what is actually in scope. Before indexing I also check that no
  credential file is in scope (bookbuilder has an `aikeys.json` at its root)
  and exclude any that is through the index's own source policy.
- **Fixed for a test.** Watching is off. Each index is built once at a
  recorded commit and is not rebuilt during a test unless an adjustment
  requires it, in which case every path is run again.
- **The benchmark is excluded from the airk index.** These paths describe the
  test itself and are denied through the index's own source policy: this
  plan, the report folder `docs/arbiter/answer-evaluation/`, the ledger
  `docs/simplify/one-process-ledger.md`, the assignment file
  `docs/agent-current-work.md` and the harness folder
  `scripts/arbiter-answer-eval/`. The reference notes are never in scope,
  because they live in the git-ignored campaign folder. No ignore file is
  added to the repository: that would change the product and the user's own
  index. Ordinary product documentation stays searchable, because finding it
  is part of the task.
- **A leak probe after every build.** A distinctive phrase from each
  scenario instruction is searched for, and the build is refused if any hit
  comes from an excluded path. The same exclusions and the same probe apply
  to every rebuild.

### 3.2 Ten scenarios per repository

Each repository gets ten scenarios with the mix the user set: three coding,
two writing, two test or QA and three architecture. The ten are the same
kinds of instruction in both repositories, written as a developer would give
them to an agent. That is 20 scenarios in all.

| # | Kind | The instruction asks the agent to | Draft for airk | Draft for bookbuilder |
| --- | --- | --- | --- | --- |
| C1 | Coding | Add a feature that crosses several layers | Add a service-wide setting that limits concurrent Answer requests, changeable in Service Admin, with validation and tests | Add another AI provider beside the existing ones, including key storage and its settings screen |
| C2 | Coding | Fix a bug described by its symptom | On a host with more than 64 logical processors every CPU model tier is refused; find the cause and fix it | A generation run that fails part-way cannot be resumed; find where run state is saved and recovered, and fix it |
| C3 | Coding | Change a shared behaviour safely | Raise the default search snippet size and update everything that depends on it | Decrypt a provider key once per run instead of once per call; change every place that reads a key |
| W1 | Writing | Write user-facing documentation | Write the user guide section on the GPU opt-in setting and its one-time reset notice | Write the help page for importing an existing manuscript |
| W2 | Writing | Explain a subsystem to a new contributor | Explain how a source file becomes searchable: scanning, chunking, embedding and publication | Explain the prompt template system |
| T1 | Test or QA | Add tests for a behaviour | Add tests proving that credential files are never indexed unless an ignore file re-includes them | Add tests for exporting a project to DOCX |
| T2 | Test or QA | Work out what must be checked | After a change to the device selection code, list the checks to run and their commands | Say which suites cover the generation pipeline and how to run them locally and in CI |
| A1 | Architecture | Trace a request through the system | Trace a search request from the MCP handler to the ranked result, naming each component and its owning contract | Trace a generation request from the front end to the AI provider and back into the project file |
| A2 | Architecture | Assess the impact of a proposed change | What changing the chunk size limit affects: stored formats, vectors, identities and recorded decisions | What supporting macOS distribution again would take: what platform code exists and what is Windows-only |
| A3 | Architecture | Say where a new capability belongs | Where a new read tool for call hierarchy belongs, and which registries, parity rules and tests apply | Where a new pipeline product is added and which rules a bundled pipeline must follow |

These are drafts so the user can see the kind of task I mean. The final
wording is written against the code in step E2 and frozen before any run. The
airk scenarios stay away from the Arbiter files this work is changing.

### 3.3 Five results per scenario

Each scenario produces five results to judge.

| Result | Path | What the agent is allowed to use |
| --- | --- | --- |
| 1 | Answer with Focus only | `airk_answer` first, then single reads for whatever is still missing |
| 2 | Answer with Focus and Compact | The same, with Context Compact enabled |
| 3 | Answer with Focus and Large | The same, with Context Large enabled |
| 4 | Parse | `airk_parse` only: text, a named recipe or an explicit plan of up to 6 steps and 12 reads in one call. It uses no Arbiter model |
| 5 | Search | `airk_search` first, then single reads: `airk_references`, `airk_symbols`, `airk_outline`, `airk_context`, `airk_read_file` |

With 20 scenarios that is 100 results in a full pass.

**The control.** The user approved one more run beside the five: Answer with
no model enabled. It is carried through every pass and every change set. It
is the only way to tell what the way Answer gathers evidence contributes from
what a model contributes (section 4.4).

**Reusing a session.** A session is reused, and not run again, when its agent
would receive identical evidence in the same order. A digest of the first
reply decides that. It applies between configurations, as when Context
cannot act and results 2 and 3 return what result 1 returns. It applies
between passes, as when a change set leaves Parse and Search untouched. Each
report says which results were reused.

### 3.4 CPU and GPU

Each Answer configuration runs on both devices. The evidence should be the
same on both, so the device pair is checked for equal evidence and timed, and
the result is judged once. If the evidence differs, both are judged and the
difference is a finding.

| Configuration | CPU | GPU (the RTX A6000, CUDA) |
| --- | --- | --- |
| Focus only | All scenarios | All scenarios |
| Focus and Compact | All scenarios | All scenarios |
| Focus and Large | 2 of each 10 scenarios, 180-second maximum, measurement only | All scenarios |

GPU runs turn GPU use on explicitly, through the opt-in setting. Section 7
lists what has to be built before every cell of this table can run.

### 3.5 How a result is produced

An agent produces each result, and its turns are logged, not estimated.

- **The agent.** A fresh headless Claude Code session on Sonnet
  (`claude-sonnet-5-5`) for each scenario and path, run by the harness as the
  same human. It receives the scenario's instruction and nothing else about
  the repository. It starts in an empty working folder under the private
  campaign folder, so it loads no project instructions, and with an MCP
  configuration that names only the wrapper. Built-in file, shell and web
  tools are denied. It may reach the index only through the tools of its
  path, through a small wrapper that enforces the path's tool list and logs
  every call: tool, arguments, bytes returned and time. The exact command
  line, Claude Code version and model identifier are recorded with every
  session.
- **Its job.** Gather the context needed to do the task, then stop and write
  a short work brief: where it would work, what it must respect and what it
  still does not know. It does not do the task itself.
- **A turn** is one tool call. A session is capped at 12 turns.
- **First call.** The first call of a path uses the instruction as written:
  as the question for Answer, as the query for Search, and as the text for
  Parse. After that the agent chooses its own follow-ups.
- **Audit.** Every location the brief cites must appear in a logged result. A
  session that used anything outside the wrapper is discarded and rerun.
- **A second session on a close call.** One session is one sample. Where a
  scenario's verdict turns on a single turn, a second session is run for that
  scenario and path, and both are reported.

Device comparison and weight adjustment do not need an agent. They replay the
recorded Answer calls and compare the returned evidence (section 5.2).

### 3.6 Reference notes

Before any run, each scenario gets reference notes written by reading the
repository directly, never through the tools under test.

- Between four and ten items of needed context, each with a file, a symbol or
  line range, and a mark of essential or helpful.
- Each essential item states the fact the agent must take from that place, in
  one sentence: the rule, the behaviour or the constraint. A location alone
  is not enough, because an agent can be handed the right file and still
  misread it.
- Alternatives are listed where more than one place would serve.
- The notes and the instructions are frozen and hashed before the first run.
  A correction after that is allowed only for a factual error, and is
  recorded with its reason.

The notes make the first criterion measurable: after each turn, a script
checks which essential items the returned evidence has covered. A judge then
checks the agent's brief against the stated facts.

### 3.7 What Answer does today, from the source

These facts bound what the test can show and what adjusting weights can
achieve.

- **One Answer request** runs one search on the question, then follow-up
  reads, and stops at 8 calls, 3 rounds, 262,144 bytes or 120 seconds by
  default.
- **The first search is small.** It returns at most 8 hits that share 4,096
  bytes, so about 500 bytes each.
- **The follow-ups are built for prose questions.** Answer can issue only two
  kinds of read: a 32-line context read after a hit, and a new search on a
  run of capitalized words found in a hit. The queue holds 8 and is filled
  hit by hit, so the top hit's context read and its capitalized-word searches
  usually fill it before a second hit is considered. That suits hopping
  between named things in articles. It does not look up a definition, the
  callers of a function, a file's tests or the contract that governs a
  folder, which is what a software task needs.
- **Focus** reorders at most 4 hits of each search that Answer makes. It has
  no threshold, and its instruction text is a generic web-search one. It
  cannot bring in a hit from outside the 8 returned.
- **Context cannot act today.** An accepted Context action can do two things
  only: move a read that is already queued to the front, or end the
  gathering. It never adds a read. And every Context profile carries a
  calibration with zero rules, under which every action abstains. So today
  Context adds time and receipts and changes no evidence. It also halves the
  token allowance Focus gets.

So the first pass of results 2 and 3 will show Context's cost and no benefit.
That is the honest starting point, and section 5 is how it changes.

## 4. Judging

I judge, with the two rubrics below. The user reviews a packet of the
results and can overrule any score.

### 4.1 Rubric A: fewer turns for an agent (first priority)

A turn count means something only if the agent ends up right. So turns are
counted to a **correct finish**, which needs both of these:

1. **The evidence establishes every essential fact.** After some turn, the
   returned evidence covers every essential item of the reference notes. A
   script checks this from the turn log.
2. **The brief gets those facts right.** The agent's work brief states each
   essential fact correctly. A judge checks this against the notes.

The turn count of a correct finish is the turn at which the first condition
was met. A session that meets the first and fails the second has no correct
finish: the evidence was there and the agent did not take it in.

Also measured from the turn log:

- **First-call yield.** The share of essential items covered by the first
  call alone.
- **Context cost.** Bytes returned to the agent over the session, and
  elapsed time. Fewer turns bought with a very large or very slow reply is
  recorded as such.

Also judged from the session:

- **Misleading or noisy evidence.** Whether irrelevant or wrong material sent
  the agent down a wrong path.

Each result gets one grade:

| Grade | Meaning |
| --- | --- |
| 4 | A correct finish after one call |
| 3 | A correct finish after two calls |
| 2 | A correct finish after three or four calls |
| 1 | A correct finish after five or more calls. Or one essential fact short: every fact was covered and the brief got one wrong, or one fact was never covered |
| 0 | More than one essential fact missing or wrong, misleading evidence that the agent followed, a failure or a timeout |

Cases that are not clear-cut are counted this way. The table is fixed before
the first run, so none of them is decided after the results are seen.

| Case | How it counts |
| --- | --- |
| Answer finishes correctly one turn sooner than the better of Parse and Search | Better, and reported as better. Not a substantial win |
| Answer and the better of them finish correctly on the same turn | A tie |
| Answer finishes correctly later than the better of them | Worse |
| Answer's evidence covers every essential fact and its brief gets one wrong | No correct finish for Answer. Not a win, whatever its turn count |
| Answer finishes correctly, and neither Parse nor Search does | A substantial win |
| Answer fails or times out, and Parse or Search finishes correctly | Worse |
| No path finishes correctly | No win. The scenario stays in the 20 and is listed as unsolved by every path |
| The verdict turns on a single turn | A second session is run (section 3.5). The verdict stands if both agree. If they differ, the scenario is reported as undecided and is not a win |

"Worse", wherever section 4.3 uses it, means one of the two rows above that
say so.

### 4.2 Rubric B: useful detail for a person (second priority)

Judged on the first reply of each path, as a person would see it for the
original prompt. Four questions, each scored 0 to 4, averaged.

| Question | 0 | 2 | 4 |
| --- | --- | --- | --- |
| Can a person find the relevant detail quickly? | Buried or absent | Present, needs hunting | Organized so the relevant parts stand out |
| Does it surface detail that would change a choice: constraints, rules, alternatives, risks? | None | Some, incidental | The governing rules and trade-offs are in view |
| Can each statement be traced to an exact source location, and is anything cut off disclosed? | No locations | Locations without ranges, or silent truncation | Exact locations, truncation disclosed |
| How much of it is useful? | Mostly noise or duplicates | Mixed | Little noise, little duplication |

### 4.3 What "substantially improves" means

The user approved these thresholds on 21 November.

- **A substantial win on a scenario:** Answer reaches a correct finish at
  least two turns sooner than the better of Parse and Search, or in one call
  where the better of them needed three or more, or at all where neither of
  them reached one.
- **A configuration substantially improves results** when it wins that way
  on at least 12 of the 20 scenarios, is worse on at most 2, wins at least 5
  in each repository, and is not worse on average in any of the four kinds
  of task.
- **The second criterion** is reported beside it as the difference in Rubric
  B averages. It does not decide the outcome. It breaks ties and guides how
  Answer presents evidence.

**Headroom.** A substantial win is possible only on a scenario where the
better of Parse and Search needs three or more calls, or never finishes
correctly. The baseline report counts those scenarios. If there are fewer
than 12, no adjustment can meet the rule. It would be a finding in itself: on
those tasks the simpler tools are already enough. Because the user set three
iterations with no decision between them (section 2), a low count does not
stop the work. It is stated at the top of the baseline report, every later
report repeats it, and the user weighs it at the inspection.

Results are reported for each configuration, each repository and each kind
of task, with every scenario listed. Twenty scenarios support a clear
difference and cannot support a fine one, and the report says which it is.

### 4.4 What each model adds

Beating Parse and Search does not decide what ships. All three Answer
configurations could win on the same scenarios while better gathering
accounts for the whole gain. So every report also sets the configurations
beside each other, on the same scenario rows:

| Step | Compared with | Devices |
| --- | --- | --- |
| Answer with no model | The better of Parse and Search | No model runs |
| Answer with Focus | Answer with no model | CPU and GPU |
| Answer with Focus and Compact | Answer with Focus | CPU and GPU |
| Answer with Focus and Large | Answer with Focus and Compact | GPU |

For each step the report gives three things: the correct finishes it adds,
the turns it saves, and the time it adds. A stage that adds time and no
correct finish has not earned its place, whatever the comparison with Parse
and Search says.

This is also how attribution survives change sets. Changes are bundled
within a layer: gathering, Focus or Context. The steps above separate the
layers.

Before the sealed set is opened, I write down which configuration the
development results support, and why. The rule of section 4.3 is then
applied to that configuration once, on the sealed set.

### 4.5 Keeping the judging honest

- **Frozen inputs.** Instructions, reference notes and the tables of section
  4.1 are fixed before any run.
- **Blind first pass.** Two independent judge sessions, each a fresh
  headless Claude Code session on Opus (`claude-opus-5-5`) with no tools
  beyond reading its judging packet, score each result from the rubric, the
  notes and the evidence, with the path names hidden
  where that is possible. Answer's reply has a recognizable shape, so the
  blinding is partial and I say so.
- **Adjudication.** I settle any scores that differ by more than one grade
  and record why.
- **The user's packet.** For each scenario: the instruction, the notes, the
  results side by side with their turn logs, the scores and my reasons.
- **A sealed second set.** Approved by the user. A second set of ten
  scenarios for each repository is written at the same time, with the same
  mix, and is not run until the adjustments are finished. Adjusting against
  20 scenarios will fit those 20. The sealed set shows whether the gain
  holds. It is run once: on the configuration named beforehand (section
  4.4), the no-model control, Parse and Search.

## 5. Developing weights from the tests

Weights are no longer developed from public question sets. After each pass
the results are analysed, and a set of changes is built and tested together.

### 5.1 What can be adjusted

"Weights" here means every adjustable value that shapes what Answer returns.
A change set draws on any of these levels at once.

| Level | What | Where it lives | Cost to change |
| --- | --- | --- | --- |
| 1 | Answer's gathering: which follow-up reads it queues and in what order, calls, rounds, fan-out, bytes per read, hits shown per record, packing | Request budget fields and code in the kernel | Hours. No model, no reindex |
| 2 | Retrieval ranking: how lexical and vector results are fused, snippet size, chunk size | Constants. Fusion gives both sides equal votes today | Minutes for fusion and snippets. Chunk size needs a reindex. Changes the Parse and Search results too |
| 3 | Focus without training: how many hits it scores, its token width, its instruction text | The model profile and the Focus compiler | Hours. The instruction is a digest input, so the product change needs a recorded decision |
| 4 | Context without training: the rules that decide which actions are accepted, and how the choices are put to the model | The calibration asset and the roster and prompt code | Hours to days. The roster and prompt are digest inputs |
| 5 | Focus model weights | The existing LoRA tooling. A pilot checkpoint exists and has never been compared | About a day per candidate on the A6000, by estimate |
| 6 | Context model weights | No trainer exists yet | Days. Decided after the first change set is judged |

### 5.2 Change sets, not single changes

The user set this on 21 November: do not alter one thing at a time. Use the
analysis to build the set of changes most likely to improve results, make
them together, and iterate. Time is short.

One iteration:

1. **Pass.** Run the agent sessions and judge them.
2. **Analyse.** For every scenario where Answer did not win, name the cause
   from the turn log: the needed item was never retrieved; was retrieved and
   not followed; was followed and dropped in packing; was present and
   buried; the model chose badly; or the reply came too late.
3. **Build the change set.** Every change that the analysis and the source
   make likely to help, at any level, built together. Each change is written
   down with the cause it addresses and the effect I expect.
4. **Replay.** Rerun the recorded Answer calls with the whole set and
   measure coverage of the reference notes mechanically. This takes minutes
   and uses no agent sessions. Starting values such as pool sizes are tuned
   here.
5. **Check what earned its place.** Replay again with one change left out,
   for each change that is costly to keep: one that needs a recorded
   decision, adds latency, or is a trained weight file. Also for any
   scenario the set made worse. Each such replay takes minutes.
6. **Pass again** with agent sessions on every Answer configuration, the
   no-model control included. Parse and Search are run again only when the
   set changes something they depend on. Otherwise their sessions are reused
   (section 3.3).
7. **Report and commit.** Write the series up in the report folder (section
   5.6): the set, each change with its cause and expected effect, the
   results, what was good, what was bad, and what was kept or dropped. The
   series is committed as one commit before the next set starts.

How it is sized:

- **Three change sets, then a stop.** The user set this on 21 November. The
  three iterations run back to back: each set is built, replayed, passed,
  reported and committed, and the next set starts from that commit with no
  decision in between. After the third commit, work stops for the user's
  manual inspection. At each inspection the user sets how many further
  iterations to run, which may be more or fewer, and the same rule applies
  to them. A genuine blocker still stops work early and is reported.
- **Slow work starts at once, in parallel:** mining the training pairs,
  training the first Focus candidate, and building the GPU paths of
  section 7. They do not wait their turn.
- **The first set is built while the baseline runs**, on a separate build,
  so the baseline is still measured on the code as it stands today.

What this gives up: a set shows that the set helped, not which change did.
Step 5 recovers most of that for the changes where it matters. I accept the
rest as the price of speed.

Experiments run in the private evaluation build. A change that the tests
favour reaches the product through the normal path: a recorded decision where
it touches a contract, a digest input or a stored format. One decision
record covers a whole kept set.

### 5.3 The first change set

This is the set I would build today, from the source analysis of section
3.7. The baseline pass may add to it or remove from it. Each row says whether
it rests on something measured or on my reading of the code.

| # | Change | Why I expect it to help | Level |
| --- | --- | --- | --- |
| 1 | Queue the follow-ups a software task needs: the definition of identifiers named in the instruction and in the top hits, their references, the outline of a hit's file, the tests that cover it, and the contract or README that governs its folder. Reuse the read plans the Parse recipes already define (definition, impact, tests, documents, component) | Read from the code: today's follow-ups are context reads and searches on capitalized word runs, a shape that suits prose questions | 1 |
| 2 | Spread follow-ups across the hits, not hit by hit | Read from the code: the top hit usually fills the queue of 8 | 1 |
| 3 | A larger first search: a wider pool of candidates and whole chunks instead of about 500 bytes a hit | Read from the code: 8 hits share 4,096 bytes. An agent needs whole functions and sections | 1 |
| 4 | Focus scores the whole pool and the best hits are kept. The pool is sized to the device: full on the GPU, smaller on CPU | Read from the code: Focus reorders 4 of 8 and cannot bring a hit in. Measured: about 3.9 s per pair on CPU at 4 threads | 3 |
| 5 | A software-task instruction for Focus in place of the web-search one | Read from the code. The size of the effect is unknown | 3 |
| 6 | Give Context something to decide and let it act: option lines that state each read in words, the wider and more varied choices of change 1, and a provisional rule that accepts its top Read choice | Read from the code: with zero rules Context never acts. A hint from the retired panel: with the read stated, its share of the score variation rose from 0.5% to 19% for Compact and from 7% to 46% for Large | 4 |
| 7 | Packing and presentation: drop overlapping evidence, group the reply by what each part is for (where to change, rules to respect, tests, documentation), keep exact locations | A hint from the retired TechQA run: 22% of delivered bytes were duplicates. Serves Rubric B directly | 1 |
| 8 | Shape the budget to the device, so Answer stays inside 120 seconds on CPU | Measured: Compact takes 9.7 s per call at 2,048 tokens and 47.6 s at 8,192 on 8 cores | 1 |

Changes 1 to 3 alter how Answer gathers evidence, not a model. I include them
because the analysis says the largest gain is there, and because Focus and
Context can only choose well among reads that are worth choosing.

The set is bundled by layer, which is what lets section 4.4 say what each
layer added. Changes 1, 2, 3, 7 and 8 are the gathering layer and work with
no model. Changes 4 and 5 are the Focus layer. Change 6 is the Context
layer. None of them alters the shared search tool, so Parse and Search
return what they returned at the baseline and their sessions are reused.

Held back from the first set:

- **How shared search fuses and ranks results:** weighting lexical and
  vector results by the kind of query, and preferring source and
  documentation over generated files. The case for it is my inference only.
  It also changes Parse and Search, which would cost 40 comparator sessions
  in a pass and blur whether Answer improved or the floor moved.
- **What brings it in.** The baseline measures, with no agent session, the
  rank at which each essential item first appears in a search of 100 hits.
  If the items are mostly inside the pool that changes 3 and 4 give Focus,
  fusion stays out. If they are missing even at that depth, fusion or
  chunking is the bottleneck and enters the next set.

Prepared in parallel for the second set:

- The training pairs of section 5.4.
- Focus candidate weights trained on them with the existing LoRA tooling,
  and the existing pilot checkpoint, both judged by the same scenarios.
- Whether to build a Context trainer. That is decided from the first pass
  with change 6 in place, which shows whether Context chooses well once it
  can act.

### 5.4 Training data for model weights

Levels 5 and 6 need training pairs, and the judged scenarios are never
training data. Mining starts at once. I propose pairs of the same shape as
the scenarios, mined from real repositories:

- A commit's message as the instruction, and the files and regions it
  changed as the needed evidence.
- A documentation section as the instruction, and the code it describes as
  the needed evidence.
- A test as the instruction, and the code under test as the needed evidence.

Mining leaves out three things: the paths excluded from the index (section
3.1), every commit made after the scenarios are frozen, and any pair whose
evidence overlaps a judged or sealed scenario's notes. Training keeps the earlier protection for existing abilities: the
original weights stay selectable, and training includes general replay.

**Every trained candidate is trained on pairs from both repositories.** The
user set this on 21 November: the weights are for general-purpose
programming work, and two repositories in different languages and stacks
(Rust, TypeScript and documentation; C#, .NET and Vue) give a broader fit
than either alone. No candidate is trained on one repository only. A
candidate trained this way may be proposed for delivery like any other.
Pairs and weight files stay in the private campaign folder until a
candidate is chosen.

### 5.5 Guards

- **Fitting the test.** The sealed set of section 4.5.
- **Fitting the repositories.** A model trained on two repositories'
  history may only have memorized them. Because every candidate trains on
  both (section 5.4), this is checked in two ways. A share of each
  repository's mined pairs is held out of training, and the candidate's
  coverage on them is compared with its coverage on the pairs it trained
  on; a wide gap is reported as memorization. And the general replay of
  section 5.4 checks that existing abilities survive.
- **Regressions.** Every pass covers all three Answer configurations, so a
  gain for one cannot silently cost another.
- **Time.** Every pass records how long each reply took, so a set that buys
  fewer turns with an unacceptable wait is visible.

### 5.6 A report for every series of changes and tests

The user required this on 21 November. Every series of changes and its test
is written up as a report in one folder,
[docs/arbiter/answer-evaluation/](answer-evaluation/README.md), so that
anyone with access to the repository can follow what was tried and what came
of it.

- **One report per series**, numbered in order: the setup, the baseline,
  each change set with the pass that tested it, the sealed set and the final
  report. The folder's README lists them with a one-line outcome each.
- **The same sections in every report:** a summary, what changed, what was
  tested, the results, what was good, what was bad, why, what happens next,
  and the limits. "What was bad" includes changes that were made and then
  dropped, regressions, failures and timeouts.
- **Readable on its own.** A report is written for someone who was not
  there. It does not assume the plan, the ledger or the code is open.
- **One commit per series.** The user set this on 21 November, so that each
  commit and its results can be compared with the others. When a series is
  judged, everything it produced lands in a single signed-off commit, and
  nothing of it is committed earlier:
  - the change set's code, or for the baseline the harness, scenarios and
    rubrics;
  - the series report;
  - its evaluation results: every scenario's grades from each judge, the
    adjudicated grade, turns to a correct finish, first-call yield, bytes
    and time, in a results file with the same columns in every series, so
    two commits can be compared with a plain diff;
  - the README's new row and the ledger entry.

  The setup pilot (`00-setup.md`) is not a full set, so it goes into the
  baseline's commit. Each series commit is pushed to `origin` under the
  branch's existing authorization. A report is not rewritten after its
  commit. A later correction is added at its top with its date, in the next
  series' commit.
- **What stays private.** Raw tool output and the agents' session logs stay
  in the campaign folder. The committed results file holds grades, counts
  and timings, not evidence text. The reports quote none of the bookbuilder
  repository's source; they name its scenarios and may cite its file paths.

The ledger keeps recording commands, identities and receipts. The reports
are the readable account; the ledger is the audit trail behind them.

## 6. Work order

Nothing here has started. The readings of section 9 are approved, and
execution waits for the user's word. Once it starts, the reports in the
folder below say how far it has got.

| Step | Work | Done when |
| --- | --- | --- |
| E0 | The user reads this plan and corrects it, and says to start | The main readings of section 9 were approved on 21 November, and the agent, judge, commit and iteration choices were set the same day. The word to start has not been given |
| E1 | Build the evaluation service from current source. Create the two indexes with vectors. Check scope and credential exclusion, and apply the benchmark exclusions of section 3.1 | Both indexes serve hybrid search, their file counts and commits are recorded, and the leak probe passes |
| E2 | Write the 20 scenarios and their reference notes, and the sealed set. Freeze them | The user has seen the scenario list; hashes recorded |
| E3 | Build the harness: the path wrapper and turn log, the headless Claude Code launcher for Sonnet agents and Opus judges, the coverage script, the judge packet | A pilot of 2 scenarios in each repository runs end to end on all five paths. Report `00-setup.md` is written, with `scenarios.md` and `rubrics.md`. It is committed with the baseline |
| E4 | Baseline pass on the code as it stands: Parse, Search, the no-model Answer control and Answer with Focus only, on all 20 scenarios. Context cannot act yet, so results 2 and 3 reuse result 1's sessions where their evidence is identical; their time is measured on CPU and GPU. With no agent session, record the rank at which each essential item first appears in a search of 100 hits | Judged and analysed. Commit 1: the harness, `00-setup.md`, `01-baseline.md` with the headroom count of section 4.3, and the baseline results file |
| E5 | Change set 1 (section 5.3), built while E1 to E4 run. Replay, then a pass on every Answer configuration and the control, CPU and GPU, with Large on CPU on 4 scenarios. Parse and Search sessions are reused, since the set leaves shared search alone | Commit 2: the set's code, `02-change-set-1.md` and its results file. Change set 2 starts at once |
| E6 | Change set 2: what the E5 pass shows, plus the first trained candidates. Replay, then a full pass | Commit 3: the set's code, `03-change-set-2.md` and its results file. Change set 3 starts at once |
| E7 | Change set 3: what the E6 pass shows | Commit 4: the set's code, `04-change-set-3.md` and its results file. **Work stops for the user's manual inspection.** The user sets how many further iterations to run, if any |
| E8 | Only on the user's word after an inspection. Write down the configuration the development results support. Run the sealed set once on it, with the control, Parse and Search | Report `05-sealed-set.md`, beside the development results, in its own commit |
| E9 | Final report `06-final-report.md`: the answer to section 1 for each configuration, the change sets and what each kept, and what to deliver | Its own commit. The user decides what ships |

Further iterations that the user sets at an inspection take the next report
numbers (`05-change-set-4.md` and on), and the sealed set and final report
move after them.

Running beside E1 to E5, each with its own agent under the rule of one Cargo
owner and one GPU owner at a time:

- **Training track.** Mine the pairs, compare the pilot Focus checkpoint,
  train the first Focus candidate.
- **GPU track.** The Answer paths for Large and for Focus on the GPU, and
  the 180-second allowance (section 7).

Every report goes in [docs/arbiter/answer-evaluation/](answer-evaluation/README.md)
(section 5.6). A step is not done until its series commit exists. The
training and GPU tracks commit nothing on their own: what they produce enters
the series commit that first uses it.

The A6000 has one owner at a time. The baseline's GPU runs come first. The
GPU track and Focus training use the device between them.

Platform work (Linux, macOS, Radeon, packaging) waits for E9, since E9 says
which stages are worth delivering.

## 7. What has to be built first

Found by reading the source. Each is a gap between today's code and section
3.

| Gap | What exists | What is needed |
| --- | --- | --- |
| Large with Answer on the GPU | A private CUDA Answer selector for Compact only | The same selector extended to Large. Test-support code |
| Focus on the GPU | Code that prepares a CUDA Focus profile, and a Focus scorer that accepts one. No selector or launcher, and it has never run | A selector and one proving run. This is the least certain item |
| 180 seconds for Large on CPU | Every layer caps a request at 120,000 ms: Core validation, the MCP handler, the kernel and the resource policy | An evaluation-only allowance of 180,000 ms. The product cap stays at 120 seconds |
| Context rules on the GPU | The CUDA Answer selector requires a calibration with zero rules | A deliberate change so an adjusted calibration can be tested there |
| The turn log | None | The wrapper of section 3.5, kept in the repository with its tests |
| Large gated by GPU opt-in | Route planning with typed reasons (commit `e587bf6d`). Large still has CPU profiles in the catalog | The product rule of section 2: no CPU candidate for Large, and Admin offers Large only with GPU use on. The test does not need it. The decision is recorded during the campaign and the product change is made after E9, unless the user asks for it sooner |

## 8. Cost

These are estimates. The pilot in step E3 replaces them with measurements.

- **A pass after a change set** is up to 80 agent sessions: four Answer
  configurations, the control among them, on 20 scenarios. It is fewer where
  evidence is identical and sessions are reused. Parse and Search add 40
  only when a set changed shared search. I estimate two to three hours of
  wall time and on the order of ten million tokens for 100 sessions with
  their judging.
- **A replay** is 20 recorded calls for each configuration and takes minutes
  with no agent sessions. The leave-one-out checks of section 5.2 are
  replays.
- **The whole campaign** is five passes as set today: the baseline, three
  change sets and the sealed set. More iterations set at an inspection add a
  pass each. The baseline is about 80 sessions (Parse,
  Search, the control and Focus only), because Context cannot act yet. The
  sealed set is about 80 (one configuration, the control, Parse and
  Search).
- **Judging.** The baseline, the last change set before each inspection and
  the sealed set get two independent Opus judge sessions for each result. A
  pass in between gets one, plus my adjudication, to save time.
- **Models.** Agent sessions run on Sonnet and judge sessions on Opus, both
  through headless Claude Code on the user's account. The token estimate
  above is for that split. Each report records the sessions and tokens it
  actually used.
- **Building a change set** is the part I cannot size yet. The first set is
  mostly gathering code and two model-prompt changes, with no training. I
  will know its real cost once it is built, and I report it then.
- **Disk.** One evaluation installation with the FP16 weight files is about
  13 GB, and the two indexes are unmeasured. D: has about 39 GB of its
  budget left.
- **If that is too much**, a lighter protocol replaces the agent sessions
  with one scripted first call for each path and a judged estimate of the
  follow-up turns. It is cheaper and less direct, because it estimates turns
  where this plan measures them.

## 9. My reading of your instructions: please confirm or correct

On 21 November the user confirmed reading 4 and then approved readings 1, 3,
5, 8 and 9. Later that day the user set readings 14 to 16 and told me to use
my defaults for the remaining choices, which settles 12 and 13 as written.
The others (2, 6, 7, 10 and 11) stand as written and are followed unless the
user corrects them. None of this was the word to start, and execution waits
for that.

1. **Twenty scenarios.** Ten for each repository, the same ten kinds of
   instruction in both. The alternative is ten in all, five in each.
2. **Five results.** The five are the three Answer configurations plus Parse
   and Search, for each scenario. CPU and GPU are two runs of the same Answer
   configuration, judged once unless their evidence differs.
3. **"After each test, adjust weights."** Each pass runs all three Answer
   configurations together, and one change set follows each pass. Your
   instruction listed the three configurations in order. With change sets
   that span every level, running them in one pass is faster and loses
   nothing, because every pass reports each configuration separately. Say if
   you want them strictly one after another.
4. **Confirmed by the user on 21 November.** "Weights" and "adjustments"
   cover the six levels of section 5.1, and not only trained model weights.
   In particular the first change set alters how Answer gathers evidence
   (changes 1 to 3 of section 5.3). The analysis says the largest gain is
   there. Change sets are built as the user directed: the most likely set of
   improvements together, then iterate, with at most three sets before the
   report.
5. **Agent sessions with a logged turn count** (section 3.5), in place of my
   estimating turns from one reply. Section 8 gives the lighter alternative.
6. **The follow-up tools of each path** (section 3.3). In particular, the
   Parse path uses Parse only.
7. **Large on CPU in tests** is 2 of each 10 scenarios, 4 in all. With GPU
   use on, a Large that cannot run on the GPU falls back to Compact and never
   to Large on CPU.
8. **The three additions I recommended, approved:** the no-model Answer
   control, now carried through every pass (section 3.3), the sealed second
   set of scenarios, and training pairs mined from repository history.
9. **The thresholds of section 4.3.**
10. **Experiments in the private evaluation build** may change gathering
    code and digest-input code such as the Focus instruction and the Context
    prompt without a recorded decision each time. A kept change set still
    needs one recorded decision before it reaches the product.
11. **Platform work waits for E9.**
12. **Scenario review. Settled by default.** The user sees the 20 final
    scenarios before the first run. Approval is not required.
13. **What the reports may say about bookbuilder. Settled by default.** The
    reports name bookbuilder's scenarios and cite its file paths, and quote
    none of its source.
14. **Set by the user.** Agent sessions run in headless Claude Code on
    Sonnet; judge sessions in headless Claude Code on Opus.
15. **Set by the user.** One commit per full set of test reports and
    evaluations (section 5.6).
16. **Set by the user.** Three iterations back to back, then a stop for
    manual inspection. The number may change at each inspection.

## 10. Engineering state carried forward

Measured before this rewrite and still valid as engineering. The ledger
entries of 21 November hold the receipts.

| Item | State |
| --- | --- |
| Weight rule | One FP16 weight file per model; arithmetic is FP32 on those weights |
| CPU route | The portable Compact graph loads and scores on AIRK's static runtime with every reference winner. On 8 cores: 9.7 s at 2,048 tokens and 47.6 s at 8,192 tokens, peak memory 11.8 GiB |
| CUDA route | Compact and Large candidates pass their width checks on Windows. Linux has not scored a model |
| WebGPU route | Measured outside the product on the A6000. With inputs padded to five fixed lengths, Compact needs 11.8 GB and Large 20.7 GB including the reserve. Not integrated; Radeon and macOS unmeasured |
| GPU opt-in | Off by default. A stored "enabled" from the old default is reset once, and Service Admin shows the notice |
| Route selection | Planned in Core with typed reasons for every fallback. Not yet run end to end against a loaded model |
| Linux | Storage policy and boot wrapper work against the real guest. No build has run. The guest and disk-image caps are the user's open decision |
| Known failing checks | One module-cycle boundary check, seven files in the formatting quarantine, Admin browser tests in teardown with Edge 158 |

## 11. Decisions in force

| Decision | Status |
| --- | --- |
| A1, the weight rule | In force |
| A2, the CPU ladder | In force for Focus and Compact. Amended on 21 November: Large has no CPU tier in production |
| A3, WebGPU for Radeon and macOS | In force; waits for E9 |
| A4, calibration transfer with a guard band | Retired with the approach it served |
| A5, evaluation scoring of the FP32 artifact | Retired with the approach it served |
| A6, memory fit against a model-free control | In force |
| GPU use is opt-in on every platform | In force and implemented |
| Memory targets: 16 GB for Compact, 24 GB for Large | In force |
| The user reviews discordant pairs of the paired comparison | Retired. The user now reviews the judging packet of section 4.5 |
| Pushes of `feat/arbiter-final` to `origin` | Authorized. No PR, merge, publication or deployment |
| Agent on Sonnet, judge on Opus, both headless Claude Code | Set on 21 November. The harness is evaluation tooling, not product |
| One commit per series of reports and evaluations | Set on 21 November |
| Three iterations, then a stop for manual inspection | Set on 21 November. The user may change the number at each inspection |
| Training data for weights | Set on 21 November: every candidate trains on pairs from both repositories, for a broader fit to general programming work |
| Other gates of the parent plan's assignment (the 0.007 raw-score ceiling, the 98% paired comparison, held-out confirmation) | Suspended for this campaign. This plan replaces them as acceptance evidence |
