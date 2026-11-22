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
**Execution started on 21 November 2026** at the user's word ("Begin
execution of this plan … do the first three iterations and report"). The
same day the user changed the models to Haiku agents and Sonnet judges at
medium effort, and had the first round run on airk alone while the
bookbuilder index builds (section 2, "Changed by the user during
execution"). Section 9 lists every place where I interpreted an instruction
and which of those the user has confirmed.
**Redirected on 22 November 2026.** After the baseline and change sets 1 and
2, the user stopped the evaluation runs during change set 3, at about 10:20 UTC, and
directed that Answer itself become a local, multi-round inference loop
(section 12). The goal of section 1 is unchanged; only the Answer
implementation that serves it changes. Implementation starts after the user
has reviewed this revision.
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
- **Amended on 22 November** (section 12.5): a Large that cannot run on the
  GPU falls back to Compact on the GPU, then to Compact on the CPU. The
  Answer loop's evaluation runs no Large-on-CPU measurement.

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

- **The agent is headless Claude Code** (`claude-sonnet-5-5` as set here;
  changed to Haiku below). Every scenario session of section 3.5 runs that
  way.
- **The judge is headless Claude Code** (`claude-opus-5-5` as set here;
  changed to Sonnet below). Every judge session of section 4.5 runs that
  way. My adjudication is separate from those sessions and is labelled as
  mine.
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

### Changed by the user during execution

The user set these on 21 November 2026, after execution started.

- **The first round runs on airk alone.** While the bookbuilder index
  builds, the baseline and change set 1 use airk's ten development
  scenarios only. From change set 2 on, both repositories are used. The
  configuration rule of section 4.3 needs 20 scenarios and at least 5 wins
  in each repository, so it cannot be applied to an airk-only round; those
  reports give the airk results with the same per-scenario rules and say
  that the full rule waits for the rounds with both repositories.
- **The agent is headless Claude Code on Haiku** (`claude-haiku-5-5`), at
  medium effort (`--effort medium`), to save cost. The question the
  evaluation answers becomes whether Answer helps a smaller agent. On the
  airk baseline only, the Answer and the better comparator sessions are
  also run with a Sonnet agent as a reference point, so a reader can see
  whether a conclusion drawn from Haiku holds for Sonnet.
- **More than three rounds.** On 22 November at about 04:00 UTC the user
  wrote: "If you can do more than three rounds in the next 10 hours, do it."
  So change sets continue back to back past the third while they fit in that
  window, each a full series with its own report and commit, and work then
  stops for the user's inspection.
- **Commits may wait.** The user then wrote that commits and pushes can wait
  until several rounds are completed and brought into alignment. Each series
  still gets its own commit, so commits and results still compare one to
  one, but the commits are made in order after several rounds, not straight
  after each pass.
- **The judge is headless Claude Code on Sonnet** (`claude-sonnet-5-5`), at
  medium effort. Before the baseline, Sonnet re-judges the pilot's packets
  and its grades are compared with the pilot's Opus verdicts. If they
  disagree, the disagreement is reported and the user decides.
- **No deadline is set.** Work is reported at each stop.

### Changed by the user on 22 November: Answer becomes a local loop

On 22 November 2026, after reading the results of the baseline and change
sets 1 and 2, the user wrote: "We need to modify Answer to do exactly what you
have described … a local inference multi-turn loop that saves on expensive
agent turns with costly tokens." The user set these points.

- **The goal is unchanged.** Section 1 still decides the work: fewer turns
  for an agent, then useful detail for a person. Only the Answer
  implementation that serves it changes.
- **The evaluation runs stop; no prototype is built.** Change set 3 stopped
  during its first step of replays (receipts kept; see section 6). The loop of section 12
  is built in Rust directly, not first as a script.
- **Answer becomes a bounded loop of five stages:** Plan, Retrieve per
  sub-need, Read and check coverage, Follow up, and Compose (section 12).
- **Compound requests split into parts.** The user then added step 0, Split,
  which divides a compound request into parts with one loop each, and step 6,
  Union, which combines the parts' results (section 12.1).
- **Answer may take much longer.** Several local inference turns inside one
  call need a far larger time allowance than today's 120-second cap. The cap
  and everything that enforces it change (section 12.4).
- **The GPU matters more.** Every stage runs a model several times per
  call, so GPU use for Focus becomes much more important, and for Compact
  most of all, because that is where the user expects most users to be.
- **Large runs on the GPU only.** A Large that cannot run on the GPU falls
  back to Compact on the GPU, and then to Compact on the CPU. Large never
  runs on the CPU (section 12.5).
- **The user reviews this revision and its commit before implementation
  starts.**
- **Decisions after review, 22 November:**
  - **Accepted as I recommended:**
    - selection by the scoring models, with no text generation;
    - a 600 s ceiling and a 300 s default;
    - the many-agent design of section 12.4.1;
    - no second loaded copy of any model.
  - **Changed by the user:**
    - **No backward compatibility.** The new Answer version replaces v2 at
      once.
    - **The one-turn evaluation is history.** Its pending work is committed
      and pushed now, as it stands. The new Answer comes first, and then
      the evaluation, testing and weight refinement start over, because
      results against the one-turn Answer say nothing about the loop.

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
| Focus and Large | 2 of each 10 scenarios, measurement only, under the 120-second product cap (the 180-second allowance was not built, section 7) | All scenarios |

GPU runs turn GPU use on explicitly, through the opt-in setting. Section 7
lists what has to be built before every cell of this table can run.

### 3.5 How a result is produced

An agent produces each result, and its turns are logged, not estimated.

- **The agent.** A fresh headless Claude Code session on Haiku
  (`claude-haiku-5-5`, medium effort; Sonnet until the change of section 2) for each scenario and path, run by the harness as the
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

Learned from the source and the three passes by 22 November, and the basis of
section 12:

- **One Answer call is already a small loop,** a kernel state machine
  (`Gathering`) that runs rounds of queued reads. The models only score
  fixed candidate lists: Focus gives a relevance logit per hit, and Context
  gives one score per action card. Nothing generates text, and Context can
  reorder queued reads or stop, never write a new query.
- **The software follow-ups of change sets 1 to 3 exist only in the private
  evaluation build.** These are symbol lookups, references, callers, tests,
  outlines and governing CONTRACT/README probes. In a product build every
  evaluation layer is off.
- **Single-shot gathering has a ceiling.** The wider pool and `search_v2`
  raised pool coverage on airk from 5 to 8 of 53 essential notes, but the
  first reply still covered about 4, because the added notes sat at ranks
  22 to 40. Search sessions reached 22 of 53 by iterating: query, read,
  notice a gap, and query again with words taken from what they had read.
- **Query wording dominates.** The same queries gave nearly the same coverage
  under either search, 13 against 15 of 53. Weaker queries gave 6 to 7.

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
  headless Claude Code session on Sonnet (`claude-sonnet-5-5`, medium effort; Opus until the change of section 2) with no tools
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

E0 to E6 ran on 21 and 22 November. On 22 November the user stopped E7
during its first step of replays and redirected the work to section 12, whose
own work order (section 12.8) now governs. The table is kept as the record of
the evaluation steps; E8 and E9 now follow the loop's evaluation (L7). State
at the stop:

- **Judged:** the baseline on both repositories, change set 1 on airk,
  and change set 2 on both repositories.
- **E7 stopped.** Change set 3 completed two of its four replay variants.
  The best was the 40-candidate pool with 3 keyword searches and a strict
  deadline: 5–6 of 53 essential notes on airk and 10 of 61 on bookbuilder in
  the first reply, against 2 and 4–5 at baseline. No judged pass ran.
- **Not run:** the Context leverage measurement and the GPU scenario replays.
- **Commits.** None of the series is committed yet (the user allowed the
  commits to wait); landing them is step L0.

| Step | Work | Done when |
| --- | --- | --- |
| E0 | The user reads this plan and corrects it, and says to start | The main readings of section 9 were approved on 21 November, and the agent, judge, commit and iteration choices were set the same day. The user gave the word to start on 21 November |
| E1 | Build the evaluation service from current source. Create the two indexes with vectors. Check scope and credential exclusion, and apply the benchmark exclusions of section 3.1 | Both indexes serve hybrid search, their file counts and commits are recorded, and the leak probe passes |
| E2 | Write the 20 scenarios and their reference notes, and the sealed set. Freeze them | The user has seen the scenario list; hashes recorded |
| E3 | Build the harness: the path wrapper and turn log, the headless Claude Code launcher for the agents and judges, the coverage script, the judge packet | A pilot of 2 scenarios in each repository runs end to end on all five paths. Report `00-setup.md` is written, with `scenarios.md` and `rubrics.md`. It is committed with the baseline |
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
| Large with Answer on the GPU | A private CUDA Answer selector for Compact only | The same selector extended to Large. Test-support code. **Built on 21 November:** a same-install selector runs Compact or Large on CUDA against the CPU installation and its indexes |
| Focus on the GPU | Code that prepares a CUDA Focus profile, and a Focus scorer that accepts one. No selector or launcher, and it has never run | A selector and one proving run. This is the least certain item. **Blocked on 21 November:** no CUDA profile exists for the 1,024-token Focus graph, and there is no reviewed Focus placement or Focus-stage CUDA warm route; all three sit in digest-input files. Focus runs on CPU in every configuration, and the GPU cells are Context on CUDA with Focus on CPU. **Required since 22 November:** the loop runs Focus several times per call, so a reviewed Focus CUDA route is product work in section 12.5, no longer an evaluation convenience |
| 180 seconds for Large on CPU | Every layer caps a request at 120,000 ms: Core validation, the MCP handler, the kernel and the resource policy | An evaluation-only allowance of 180,000 ms. The product cap stays at 120 seconds. **Not built:** it needs edits to Core budget validation, the MCP clip and the kernel caps, which test-support cannot reach, so Large on CPU is measured under the 120-second product cap and a call past it counts as a timeout (decided 22 November). **Superseded on 22 November:** the loop raises Answer's product time allowance itself (section 12.4), and Large never runs on CPU (section 12.5) |
| Context rules on the GPU | The CUDA Answer selector requires a calibration with zero rules | A deliberate change so an adjusted calibration can be tested there. **Decided on 22 November:** the bounded-hybrid gate (a digest input, allowed in this private build by reading 10) admits a calibration-only variant of a reviewed CUDA profile and verifies graph and placement against the reviewed base |
| The turn log | None | The wrapper of section 3.5, kept in the repository with its tests |
| Large gated by GPU opt-in | Route planning with typed reasons (commit `e587bf6d`). Large still has CPU profiles in the catalog | The product rule of section 2: no CPU candidate for Large, and Admin offers Large only with GPU use on. The test does not need it. The decision is recorded during the campaign and the product change is made after E9, unless the user asks for it sooner. **Moved into the loop work on 22 November:** the device ladder of section 12.5 makes it part of L5 |

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
  the sealed set get two independent judge sessions for each result. A
  pass in between was to get one, plus my adjudication, to save time. On 22
  November the user said there is ample credit for the headless sessions,
  so every pass gets two judges and a second session on every close call.
- **Models.** Agent sessions run on Haiku and judge sessions on Sonnet, both at medium effort and
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
    Sonnet; judge sessions in headless Claude Code on Opus. Changed by the
    user during execution to Haiku agents and Sonnet judges at medium effort
    (section 2).
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
| CUDA Context, measured 21–22 November | On the RTX A6000 with `onnxruntime-cuda/1.30.0`: a warm Compact Context call takes 0.22–0.23 s at about 1,190 input tokens, and the first after warm-up 0.60–0.66 s. Large takes 0.53–0.57 s warm and 0.76–0.98 s first. GPU memory in use was 9.0 GB with Compact and 17.2 GB with Large (section 10.1) |
| Focus on CPU, measured | 3.7–3.8 s per Focus score at 1,024 tokens on every lifetime, CPU-only or with Context on CUDA, because Focus has no GPU route. In a GPU lifetime Focus is about 15 s of a 17 s Answer call |

### 10.1 GPU and CUDA: what the 21–22 November work found

Receipts are in the private GPU-track folder and in the CUDA image records;
`00-setup.md` section 2.7 and `02-change-set-1.md` section 4.5 give the details.

- **Built:**
  - a CUDA evaluation image from the baseline commit (402 s build, Clippy
    clean), then the CUDA images of change set 1 (`cs1b-cuda`, `cs1e-cuda`,
    474.5 s);
  - CUDA-capable images for change sets 2 and 3;
  - a same-install selector that runs Compact or Large on CUDA against the CPU
    evaluation installation and its indexes, reading the weights from pinned
    files without copying them.
- **Measured on a 7-file synthetic repository** (not the evaluation indexes):

  | Answer configuration | Call time |
  | --- | --- |
  | No model | 0.38 s |
  | Focus on CPU | 15.2–15.4 s |
  | Focus on CPU, Compact on CUDA | 17.0 s |
  | Focus on CPU, Large on CUDA | 18.0 s (17.4 s warm) |

  The per-call model times are in the table above. Every CUDA Context
  assessment abstained, as on CPU, because the shipped calibration has no
  rules.
- **Measured against the CPU route:**
  - **Compact:** a warm Context call on CUDA takes about 0.23 s at 1,190
    tokens, against 9.7 s on CPU at 2,048 tokens. The input lengths differ,
    so the ratio is approximate, but the gap is more than an order of
    magnitude. In the evaluation passes, two Compact Answer calls on CPU
    exceeded the 120-second cap.
  - **Large:** on CPU, about 100 s per Answer call in the first-call
    replays and about 300 s of tool time per airk session. On CUDA, a Large
    Context call takes under 1 s.
  - **Focus:** on CPU it costs more than every Context call together. Even
    a GPU lifetime spends about 90% of an Answer call in Focus on the CPU.
- **Evidence is the same on both devices.** On the synthetic repository a
  Context reply kept 7 records against 8 without Context. The cause is that
  an enabled Context stage halves Focus's token allowance, not the device. A
  CPU and a GPU cell with the same stages gather the same evidence.
- **Blocked or not done:**
  - **No GPU route for Focus.** There is no CUDA profile for the 1,024-token
    Focus graph, no reviewed Focus placement and no Focus warm route on
    CUDA. All three sit in digest-input files.
  - **Calibration-only variants.** The CUDA gate binds a reviewed graph
    result to an exact profile manifest, so it refuses a Context profile
    that differs only in calibration. The private build admits such a
    variant under option A, checking graph and placement against the
    reviewed base. That changes the gate's validator identity and needs a
    recorded decision before any product use.
  - **Digest change from the Answer routing.** The evaluation routing in
    `runtime.rs` changes the CUDA FP16 math-policy digest. The evaluation's
    CUDA cells are FP32.
  - **Restart across profiles.** Restarting a Large lifetime on a Compact
    lifetime's installation is refused, so Large needed its own
    installation.
  - **No GPU scenario replays.** No change set ran its GPU cells on the
    evaluation indexes. Every evaluation lifetime ran on CPU, which is why
    the GPU showed almost no activity during the passes.
- **What it means for section 12:**
  - A loop that runs Compact several times per call is practical on a GPU,
    at a fraction of a second per step. On a CPU it costs about 10 s or more
    per step.
  - Focus is the bottleneck on every device until it has a GPU route.
  - Large is fast enough on the GPU and too slow on the CPU, which supports
    the GPU-only rule.

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
| Agent on Haiku, judge on Sonnet, both headless Claude Code at medium effort | Set on 21 November (first Sonnet and Opus, changed during execution). The harness is evaluation tooling, not product |
| One commit per series of reports and evaluations | Set on 21 November |
| Three iterations, then a stop for manual inspection | Set on 21 November. The user may change the number at each inspection |
| Training data for weights | Set on 21 November: every candidate trains on pairs from both repositories, for a broader fit to general programming work |
| Other gates of the parent plan's assignment (the 0.007 raw-score ceiling, the 98% paired comparison, held-out confirmation) | Suspended for this campaign. This plan replaces them as acceptance evidence |
| Answer becomes a local multi-round loop (section 12) | Set by the user on 22 November. This is the recorded decision to change Answer's wire shape, budgets and digest inputs; the exact shapes are settled in L1 and recorded in the owning contracts |
| Evaluation runs stopped; no script prototype | Set by the user on 22 November. The harness, scenarios and rubrics are kept for measuring the loop (L7) |
| Device ladder: Large on GPU only, then Compact on GPU, then Compact on CPU; Focus on GPU where available, else CPU | Set by the user on 22 November. Replaces the "Large not offered without GPU opt-in" wording of section 2 by a fallback, and makes the Focus and Compact GPU routes product priorities |
| Answer's time allowance raised above 120 s | Set by the user on 22 November: a 600 s ceiling and a 300 s default, accepted the same day, with the default lowered if L2 finds client timeouts shorter |
| Selection by the scoring models, no text generation | Accepted by the user on 22 November. Generation is a later option, decided only after the loop has been measured |
| No backward compatibility for Answer | Set by the user on 22 November. The new request and response replace v2 at once; v1 and v2 are removed |
| The one-turn evaluation is history | Set by the user on 22 November. Its pending work is committed and pushed now as it stands; the loop comes first, then the evaluation and weight refinement start over with a new baseline. Nothing is compared against or fitted from the one-turn results |
| Implementation starts after the user reviews this revision | Set by the user on 22 November |
| Many-agent queueing (section 12.4.1) | Reviewed and accepted by the user on 22 November; its open points take the stated defaults unless the user changes them |
| Large-against-Compact decision gate on CUDA (section 12.7.1) | Set by the user on 22 November. Runs before L7's comparisons with Parse and Search. The user reviews the results personally and may drop Large altogether |
| No second loaded copy of a model on any device | Set by the user on 22 November. Capacity comes from batching, concurrent runs on one session and overflow to another device, decided at runtime (section 12.5.1) |

## 12. Answer as a local multi-round loop

Set by the user on 22 November 2026 (section 2, "Answer becomes a local
loop"). This section is the design that implementation follows once the user
has reviewed it. Section 1 remains the goal: one Answer call should do
locally, with local models, the query, read, notice-a-gap and query-again
work that an agent otherwise pays for in turns and tokens.

### 12.1 The stages

One Answer call first splits the request into parts (step 0). It then runs one
bounded loop per part (steps 1 to 5), and finally unites the parts' results
(step 6). Every read is still a queued dispatch in the existing `Gathering`
state machine (`crates/airk-kernel/src/arbiter.rs`, `arbiter/joint.rs`), so
call, round, candidate, byte and deadline accounting, revision fences and
receipts apply unchanged to every step. Steps 0 and 6 were added by the user
on 22 November.

0. **Split.** An agent's request may be compound. For example: "explain how
   the restart guard works, and update the README section on search." Split
   divides it into independent parts, each with its own loop, budget share
   and result.
   - **How it chooses:**
     - Code proposes the breaks: separate sentences or questions,
       enumerations, several imperative verbs, and joins such as "and also".
     - Compact scores whether to split.
     - When the model abstains or is absent, code splits only on explicit
       enumerations and separate questions.
   - **A simple request stays one part,** so Split adds nothing to it.
   - **Name:** the stage is called Split, not Parse, because `parse` is
     already an AIRK tool (`airk_parse`) and one of the evaluation's paths.
1. **Plan.** The question is split into two to four sub-needs, for example
   "where is X defined", "who calls it", "what tests cover it" and "which
   contract governs it". Each sub-need gets its own query wording, which also
   avoids the lexical AND behaviour that makes long queries vector-only.
2. **Retrieve per sub-need.** For each sub-need, run a search, then the
   structural reads that software work needs: symbols, then references,
   imports and outline, the governing CONTRACT.md and README, and tests.
   These are the follow-ups of change sets 1 to 3, promoted from the private
   evaluation layers into the product path.
3. **Read and check coverage.** Focus scores the candidate spans against
   each sub-need. Compact (or Large on a GPU) then judges each sub-need as
   covered, partly covered or missing.
4. **Follow up.** Each gap gets one targeted query or structural hop, built
   from terms found in what was just read: identifiers, paths, error strings
   and names. Steps 3 and 4 repeat for a bounded number of rounds. They stop
   early when every sub-need is covered or a round adds nothing new.
5. **Compose.** For its part, the loop groups evidence by sub-need, with
   locations and per-round provenance. It marks any sub-need still missing,
   and lists the follow-up reads it did not run, so the agent can run them
   itself.
6. **Union.** The parts' results are combined into one reply. This is more
   than concatenation.
   - **Dedup:** evidence found by more than one part is merged by location
     and content digest, and each copy keeps its provenance.
   - **Layout:** the reply is grouped by part, then by sub-need. Gaps and
     unrun follow-ups are reported per part.
   - **Conflicts:** evidence in one part that contradicts evidence in
     another is noted, not silently resolved.
   - **Bytes:** parts share the 64 KiB reply frame. A part shortened to fit
     says so.
   - **One part:** the reply is that part's Compose output, unchanged.

**The parts' loops are interleaved, not truly parallel.**
- Their reads (search, symbols, references and so on) may run
  concurrently.
- Their model calls cannot: each stage has one loaded model per device, and
  the inference queue already limits jobs per index. So the parts take turns
  round by round.
- **Shared by all parts:** one read cache, so a file two parts need is read
  once; one model queue; and one deadline.
- **Budget:** each part gets a share of the budget, and a part that
  finishes early hands its unused share to the others.

**Why the parts take turns, and what changes so the lane stays busy.**
Today the service has one model compute thread (`airk-arbiter-compute`) with
one resident session per stage. Its queue admits at most 8 jobs in all, and
at most one per index, queued or running
(`crates/airk-inference/src/inference_engine/pool/arbiter.rs`, score
admission). All parts of one Answer target the same index, so a second part
could not even queue its model call while the first part's runs.

A second loaded copy of a model is never the remedy, and is excluded
outright (section 12.5.1). It would cost the full model memory again: about 9
GB of GPU memory for Compact, 17 GB for Large, and up to 12 GiB of RAM for
Compact on CPU. One call already occupies most of the GPU or most of the CPU
cores. Three changes are made instead, with the runtime capacity rules of
section 12.5.1 deciding how far each goes:

1. **The parts of one Answer may each queue a model job.** The admission
   rule counts jobs per Answer attempt within an index, not per index alone.
   The one-job limit stays across different Answers and different indexes,
   so a compound request cannot monopolize the lane. The lane then moves
   straight from one part's call to the next instead of idling while a part
   runs its reads.
2. **Candidates from several parts are batched into one model call** where
   the task allows it.
   - **Compact:** natural for the coverage and follow-up rosters. One call
     already scores up to 8 cards.
   - **Focus:** needs a change first, because its pairs share one question
     per call today. A per-candidate question, or one call per sub-need
     group, is decided in L4.
   - **Accounting:** each batched call charges each part's token and
     deadline accounting for its own candidates.
3. **The parts' reads run truly in parallel.** Search, symbols, references
   and file reads do not touch the model lane. They run concurrently under
   the existing read limits and the shared read cache.

Concurrent runs on the one loaded session are covered by section 12.5.1.
A second loaded session of the same model on any device is excluded.

**Follow-on, not in the first pass:** Split and Union fit Answer across
several indexes, which is refused today when the indexes are mixed. Each
index (or each part and index) would run its own loop, and Union would merge
them while keeping each index's identity and revision. It is recorded here and
decided after L7.

### 12.2 How the models take part

The models in AIRK today are scorers. Focus returns a relevance logit for
each (question, passage) pair. Compact and Large return one score for each
action card in a roster. None of them generates text. The loop therefore
works by **selection**:

- **Deterministic code proposes, a model chooses.**
  - **Plan:** the candidate sub-needs come from the question. They are its
    identifiers, quoted terms and path hints, plus the task-kind templates
    of the ten scenario kinds: definition, callers, tests, contract,
    documentation, configuration and so on. Compact scores which sub-needs
    apply.
  - **Follow-up:** the candidate queries come from the text read so far.
    Compact scores which ones best close a gap.
- **Coverage is an action roster too.** For each sub-need the cards are
  "covered", "read more here" and "search for this term", scored by Compact
  under the existing joint-ledger rules.
- **When a model abstains or is absent, the loop still runs.** The shipped
  Context calibration has no rules, so every Context action abstains today.
  An abstained or unavailable model falls back to a deterministic order:
  every proposed sub-need, the top Focus span per sub-need, and the first
  extracted term per gap. The no-model loop is therefore a real product
  path and the evaluation's control.
- **Generating text, in plan steps, gap descriptions or the final summary,
  is not part of this design.** It would need a new decode runtime on CPU,
  CUDA and later WebGPU. It is listed as a later option, decided only after
  the selection loop has been measured. **Accepted by the user on 22
  November.**
- **Calibration.** Compact's decisions need accepted rules, not the
  zero-rule calibration. The receipts of the one-turn evaluation do not
  describe the loop's decisions, so rules are fitted from the new
  evaluation on the loop (L7), on both repositories (section 5.4). Until
  then the loop runs its deterministic order wherever the model abstains.
  The loop records each decision with its score and rule, as joint scoring
  does today.

### 12.3 Contracts and shapes that change

These need the contract decisions of section 11, settled in L1 and recorded
in the owning `CONTRACT.md` files.

- **Request and response.**
  - `airk-core` `ContextRequestV2`, `ContextResponseV2` and `Budget`
    (`crates/airk-core/src/arbiter/`) gain a new version.
  - The response adds parts, and within each part sub-needs, per-sub-need
    coverage, per-round provenance and unrun follow-ups. It also adds
    Union's dedup and conflict notes and a partial-result marker.
  - The budget gains per-round limits, per-part shares and the new time
    ceiling.
  - The TypeScript mirror comes from the approved schema-to-TypeScript
    generation, and the semantic cases are emitted by Rust.
  - **No backward compatibility (user, 22 November).** The new version
    replaces v2 at once. v2's request and response types, the v1 types and
    `Kernel::answer_with_reader` (which has no production caller) are
    removed in the same change, with their callers and tests moved to the
    new version. The CLI, the VS Code extension and the MCP schema switch
    together in one release.
- **Ledger and token pool.** Today the joint ledger allows at most
  `min(rounds, 3)` Context slots and `min(calls, 12)` Focus slots, all in one
  8,192-token pool.
  - The loop needs more slots and a per-round token allowance.
  - This changes `airk-core` `context_action.rs` and the service
    `joint_ledger.rs`, both digest inputs.
- **Task policies.** The split, plan, coverage and follow-up rosters are new task
  policies beside `context_action.rs` and `context_wide.rs` in
  `airk-retrieval`, each with its own hashed policy identity.
- **Promoted follow-ups.**
  - The software follow-ups leave the test-support gate.
  - They become product code with their own policy identity.
  - The evaluation layer tokens are removed once the loop replaces them.
- **Pinned identities.** The `cpu_source_transition` pins, the CUDA FP16
  math-policy digest and the bounded-hybrid validator identity move
  because their source files change.
  - Each is updated through a recorded decision with its retained-fixture
    proof.
  - None is re-pinned merely to make a check pass.

### 12.4 Time

Several local model turns per call make Answer much slower than one search.
The user accepts that: the loop is worth it if it saves agent turns. These
numbers were accepted by the user on 22 November:

- **Deadline.** Answer's product ceiling rises from 120 s to 600 s, with a
  default of 300 s. Search and the other reads keep their present limits.
  - **Where 120 s is enforced today:** the MCP handler's clamp
    (`crates/airk-mcp/src/lib.rs`), Core `Budget` validation, the kernel's
    native-budget cap in `joint.rs`, the supervisor's IPC clamps
    (`crates/airk-supervisor/src/runtime_metadata/arbiter.rs`), the joint
    ledger's deadline window, and the read attempt's 125 s
    `MAX_READ_BUDGET` (`crates/airk-core/src/read_attempt.rs`).
  - Every one of these changes for Answer only.
- **Per-stage budgets.** Each part gets a share of the deadline, and each
  round a share of its part's, with a reserve kept for Compose and Union, so
  a slow model or a single part cannot consume the whole call. A part that
  finishes early returns its unused share to the others.
  The existing gate estimates (12 s first Context, 30 s single call, 10 s
  read reserve) are re-measured per device.
- **Partial results.** At the deadline, Answer returns what it has, grouped
  by sub-need. It marks the reply partial, says which stage stopped, and
  lists the unrun follow-ups. A call past its deadline is never an empty
  failure.
- **Progress.** Answer reports progress (round, stage, sub-needs covered)
  through MCP progress notifications, per part, so a client that supports them can
  see that a long call is advancing.
- **Client timeouts.** A client's own tool-call timeout can cut a call off
  before AIRK's deadline. The setup documentation says how to raise it for
  supported clients. The deadline a client sends is honoured, so a client
  can ask for less time.
- **Expected times** (to be measured in L7):
  - **With Compact and Focus both on a GPU:** a few seconds per round.
  - **With Compact on a GPU and Focus on CPU:** about 4 s of Focus per
    search, as today.
  - **With Compact on CPU:** 10 s or more per Compact step. Three rounds
    with two to four sub-needs can then take several minutes, which is why
    the ceiling is 600 s.
- **The default against client timeouts.** Section 12.4.1 keeps the default
  inside common client tool timeouts. If L2 finds that Claude Code or other
  supported clients cut calls off sooner than 300 s, the default is lowered
  to fit them, and 600 s stays the ceiling for clients that ask for more.
  That is a measurement recorded in L2, not a new decision.

### 12.4.1 Many agents at once: queueing without abandoned calls

**Reviewed and accepted by the user on 22 November 2026.** It was first
written as speculation for review. Its open points take the defaults at the
end of this section unless the user changes them.

**The concern.** An AI harness may launch several agents that all call
Answer at once. Answer calls will last minutes, and the model lane runs one
job at a time. An agent at the back of the queue could then wait long
enough for its client to time out and abandon the call. Its queued work
would be wasted, and the agent would have spent a turn for nothing.

The design rests on six ideas.

**1. Admission never blocks silently.** When an Answer arrives, the service
estimates two things:
- the wait, from the queue ahead of it;
- the work, from measured per-device stage costs and the request's parts.

What it does then depends on the result:
- **The estimate fits the caller's deadline:** the call is admitted and runs
  as today.
- **It does not fit:** the call is not left to hang. Answer applies the
  first of these that works:
  - **a. Degrade to fit.** Plan fewer rounds or sub-needs, or move down the
    device ladder (Large to Compact, or Compact to the no-model loop). The
    reply discloses the degradation.
  - **b. Return a ticket at once.** The reply holds a handle, the queue
    position, an estimated start and finish, and the exact retry time. The
    agent spends one cheap call to learn when to come back, instead of
    timing out.

**2. The work outlives the call.**
- An admitted Answer is a job owned by the service, not by the client's
  connection.
- If the client disconnects or times out, the job keeps running to its own
  deadline.
- Its result is kept for a bounded time, keyed by the request's digest and
  the publication it ran on.

**3. Retrying attaches; it does not replay.**
- If the same agent, or any agent in the same scope, sends the same request
  on the same publication, the call attaches to the running or finished
  job.
- This follows the rule that uncertain work is never replayed. A retry
  after a timeout costs one call and gets the finished result, or the
  progress so far.
- An explicit handle (a continuation) does the same for an agent that kept
  the ticket.

**4. Fair scheduling across agents.** Several independent Answers share the
lane:
- **Turns:** they take turns per round, as the parts of one Answer do.
  Interleaving across Answers works the same way as within one.
- **Order:** the earliest deadline goes first, with an ageing guard so no
  Answer starves.
- **Fairness:** one client cannot fill the lane, because each attempt is
  limited to one queued job per part.
- **Sharing:** the read cache is shared across Answers on the same
  publication, so agents asking about the same repository reuse each
  other's reads.
- **Limit:** a global limit caps how many Answers gather at once.
  Requests beyond it wait in an admission queue that holds no model or
  memory reservation. That queue reports its position and estimate.

**5. Keeping the client's clock alive.**
- **Progress notifications.** Answer sends MCP progress notifications per
  part and round. The MCP specification lets a client reset its timeout on
  progress, but not every client does, so this cannot be relied on alone.
- **Asynchronous calls.** Where the client supports MCP's asynchronous task
  support, Answer runs as a task the client polls. Which clients support
  either, including Claude Code, is checked in L2 and recorded in the setup
  documentation.
- **A default inside common client timeouts.** The deadline Answer proposes
  by default stays inside common client tool timeouts. An agent that can
  wait longer asks for more. Answer then returns a partial result with a
  continuation before the client's clock runs out, rather than at AIRK's
  own ceiling.

**6. Visibility.**
- Status shows the Answer queue: depth, the Answer running, estimated waits
  and recent per-device stage costs.
- Every ticket, degradation and attachment is recorded in the reply's
  receipts, so an agent or a person can see why an answer was late or
  smaller.

**What this would add to the contracts of section 12.3:**
- a ticket and continuation shape in the response;
- an attach-by-digest rule in the kernel;
- a service-owned job table with bounded retention of results;
- the admission estimator;
- earliest-deadline scheduling in the lane.

**Defaults for the open points** (my proposal, adopted with the design;
the user may change any of them):
- **Degradation and tickets: both.** Answer first degrades to fit the
  deadline. It returns a ticket only when even the smallest disclosed
  answer cannot fit.
- **Retention:** a finished result is kept for 10 minutes after it
  finishes, or until its publication changes, whichever comes first. Its
  memory counts in the service's budget.
- **Global gathering limit:** set by the runtime capacity rules of section
  12.5.1 from the model lanes actually available, with a floor of 2 and a
  user setting to cap it. It is not a fixed number.

### 12.5 Devices

The GPU matters more than before, because every stage runs a model several
times per call.

- **Context ladder:**
  - Large runs on the GPU only.
  - If Large cannot run on the GPU, Context falls back to Compact on the
    GPU, and then to Compact on the CPU.
  - Large never runs on the CPU, in production or as a fallback.
  - Each fallback carries its typed reason, as route selection does today.
  - Without GPU opt-in, Compact on the CPU is the Context stage.
- **Compact on the GPU is the main product path.** The user expects most
  users to be there. Its CUDA route exists and has been measured
  (section 10.1); it must become a reviewed product route with accepted
  calibration rules, not only the private calibration-only variant.
- **Focus on the GPU becomes required work.**
  - **What to build:** a CUDA profile for the 1,024-token Focus graph, a
    reviewed Focus placement and a Focus warm route.
  - **Why:** Focus on the CPU is about 90% of an Answer call even with
    Context on the GPU, and the loop calls it per sub-need.
  - **Fallback:** Focus on the CPU remains whenever no GPU is available or
    opted in.
- **CPU fallback is always available.** It is Focus and Compact on the CPU,
  or the no-model loop. Answer never refuses for lack of a GPU.
- **Platform order.** WebGPU for Radeon and macOS stays decision A3 and
  follows CUDA, with the same ladder.

**Unloading idle models.** Today a loaded model stays resident until its
stage is disabled, its profile changes, its device faults or the service
stops; there is no idle timeout. Large holds about 17 GB of GPU memory and
Compact about 9 GB, or up to 12 GiB of RAM on the CPU. Most users will run
Compact on a desktop GPU they also use for other work, and agent use comes
in bursts. So:

1. **Default on, with a long timeout.** A stage's model unloads after 30
   minutes with no Answer call, so a working session never sees a reload. A
   setting changes the time, and 0 means never unload (for a dedicated
   GPU).
2. **Unload through the existing safe path.** The release and drain used
   when a profile changes (`release_profile`) runs only when nothing for
   that stage is queued or running.
3. **The reload is handled inside an Answer, not by blocking it.**
   - The admission estimator of section 12.4.1 counts the reload time.
   - While a model reloads, Answer begins with reads, which need no model,
     or starts one step down the ladder: Compact while Large reloads, or
     the no-model loop. The reply says so.
   - An Answer never times out because of a reload.
4. **Optional pre-warm.** A client starting a working session may ask the
   service to warm the models before its first Answer. Status shows whether
   each stage is loaded, unloading or warming.
5. **Per stage.** Focus and Context each have their own timeout setting.
   Both start with the same default; Focus, being smaller, may later keep a
   longer one.

The reload and re-warm time is unmeasured. L5 measures it for each model on
each device, and those figures feed the admission estimator.

### 12.5.1 Using local resources at runtime, with one copy of each model

Set by the user on 22 November 2026.

- **One loaded copy only.** AIRK never loads a second copy of a model's
  weights on any device, whatever the headroom.
- **Getting more work done.** Throughput comes from three places, decided
  at runtime from measured resources and load:
  - batching;
  - concurrent runs on the one loaded session;
  - overflow to the same model loaded on another device.
- **GPU size floors.** These floors for one loaded model on a GPU are
  unchanged: Focus 8 GB, Compact 16 GB, Large 24 GB. Extra concurrency only
  uses memory above them.

**The levers, cheapest first.**
1. **Batching.** More candidates in one model call: up to 8 per call
   today, and across parts and across Answers where the task allows
   (section 12.1). Almost no extra memory.
2. **Concurrent runs on one session.** ONNX Runtime allows several runs
   at once on one loaded session.
   - **Memory:** the weights are shared, and each extra run needs only its
     own working memory for that call's intermediate results.
   - **On CUDA:** each concurrent run needs its own GPU stream to overlap.
   - **On the CPU:** concurrent runs split the cores. They help only when
     one run leaves cores idle, which is common for Focus with short
     inputs.
3. **Overflow to another device.** A model loaded on one device may be
   loaded once on another, for example Compact on the GPU and Compact on
   the CPU. That is a second route, not a second copy on the same memory.
   - **Which device takes a job:** each job goes to whichever route
     finishes it soonest, counting the queue wait and the run time.
   - **When the CPU takes Compact work:** Compact on the CPU takes about
     10 s against 0.23 s on the GPU. So it takes Compact work only when the
     GPU queue wait exceeds that, which is the many-agent case.
   - **Large has no CPU route** (section 12.5).
   - **Gate:** each device's own memory rule applies, and the user's GPU
     opt-in still governs every GPU route.

**What the runtime measures.**
- **Per model, when it warms.** A short calibration run records:
  - the weights' footprint;
  - peak working memory for one run at the stage's largest input;
  - the time of one run;
  - how busy one run keeps the device.
- **Per device, continuously:**
  - free GPU memory and GPU busy time, through NVIDIA's management library
    for CUDA, with Radeon and macOS queries added with WebGPU;
  - free system RAM, physical cores and CPU load.

  Other applications count. A game or another model on the same GPU
  shrinks AIRK's room.
- **Per stage, from the queue:** queue depth, waits against deadlines, and
  throughput achieved.

**The rule for adding a concurrent run to a stage.** All of these must hold:
1. **Memory:** free memory on the device, after the run's measured working
   memory, stays above a reserve floor. The floor is the larger of 1.5 GB
   and 10% of the device, for the system, the display and other work.
   Stages sharing a device are counted together. On a 24 GB card with Large
   there is little room for Focus beside it. On the A6000 (48 GB), Large,
   Focus and extra runs fit.
2. **Need:** more than one job has waited for a sustained period, or
   deadlines are at risk.
3. **Gain:** one run leaves the device underused.
4. **Proof:** after the run is added, throughput rises by a clear margin
   (at least 1.3 times), and the existing runs stay within their budgets.
   Otherwise the run is removed.

**Under pressure, shed quickly and recover slowly.**
- When free memory falls below the floor, or another application's load
  rises, concurrent runs are removed down to one per stage.
- They are added back one at a time after a quiet period, so the count does
  not oscillate.
- Shedding never interrupts a running call. It stops new admissions to the
  extra run and lets running calls finish.

**What changes around it.**
- **Queue and ledger:**
  - The lane's admission counts capacity per stage, set by these rules, in
    place of the fixed one job per index.
  - Token and deadline accounting allows concurrent reservations.
  - Fairness across Answers and parts (sections 12.1 and 12.4.1) still
    holds.
- **Device faults:** a CUDA fault marks the whole process as poisoned
  today, so every concurrent run on that GPU fails together. Recovery
  drains them all and falls back down the ladder of section 12.5.
- **Mixed devices:** a CPU run and a GPU run can score slightly differently,
  because their arithmetic is ordered differently, and that could change a
  close selection.
  - Every decision records the device it ran on.
  - L7 checks whether overflow changes evidence. If it does, overflow is
    restricted to the same device class.
- **Settings and status:**
  - Capacity defaults to "auto", capped at 2 concurrent runs per stage until
    L7 measures more.
  - User settings cap the concurrent runs per stage and the total GPU memory
    AIRK may use.
  - Status shows, per stage and device, the current runs and why each was
    added or removed.
  - These figures also set the global gathering limit of section 12.4.1.

### 12.6 What is kept from the evaluation

- **The evaluation starts over on the loop (user, 22 November).** The
  results measured against the one-turn Answer are kept as history in their
  reports, but they are not a baseline for the loop. Nothing is compared
  against them, and no weight or calibration is fitted from them. After
  the loop is built (L1 to L6), L7 runs:
  - a new baseline;
  - the change sets and weight refinement of section 5;
  - the per-series reports and commits of section 5.6.
- **Kept tools:** the harness, the 20 scenarios, both rubrics, the judges
  and the reports folder measure the loop. The question asked of them is
  unchanged.
- **Kept findings and fixes:**
  - the search roster fix and the settled-index guard;
  - `search_v2`, which finds slightly more on the same queries;
  - the 40-candidate pool;
  - the indexer pipelining fixes.
- The private evaluation layers and the restart exceptions stay
  evaluation-only.

### 12.7 Acceptance

The loop is accepted when the gates below are met. The gates are those of
sections 4.1 to 4.3, measured on both repositories.

- **First priority, agent turns:** Answer with the loop reaches correct
  finishes in fewer turns than the better of Parse and Search.
- **Coverage over a session:** Answer covers at least the essential notes
  that Search covers. The new baseline of L7 measures Parse and Search
  again beside the loop; the one-turn figures (Search at 22 of 53 on airk and
  26 of 61 on bookbuilder) are history only.
- **Every device and fallback route** answers within its deadline or
  returns a disclosed partial result.

### 12.7.1 Decision gate: Large on CUDA against Compact on CUDA

Set by the user on 22 November 2026. This gate decides whether Large is worth
further development. It stands on its own, apart from the later comparisons
with Parse and Search in L7.

- **If Large is not significantly better than Compact,** further
  development of Large may be dropped altogether.
- **The user reviews the results personally** from the gate report and
  makes the decision.

**What is compared.** The same loop, on the same build, indexes,
scenarios and Focus device, with the Context model as the only
difference:
- **arm C:** Compact on CUDA;
- **arm L:** Large on CUDA.
- **Focus device:** Focus runs on the GPU if its CUDA route is ready (L5),
  otherwise on the CPU in both arms.
- **Decision rule:** both arms take every Context decision by top score,
  with no abstention and no calibration rules.
  - **Why:** under today's zero-rule calibration both models abstain and
    the arms would be identical; fitted rules would compare the rules as
    much as the models.
  - **Calibrated arms:** if L7 later fits rules for both models, the gate's
    tests are repeated with them as a second reading.

**Tests.**
1. **Decision agreement.** Run all 20 development scenarios on both
   repositories without agent sessions, and count:
   - how often the two models choose the same split, sub-needs, coverage
     verdicts and follow-ups;
   - where they differ, which choice the reference notes favour.

   Near-total agreement already says Large adds little.
2. **Evidence quality, first reply.** For each scenario and arm, record:
   - essential and supporting notes covered (coverage rule 2);
   - bytes;
   - sub-needs covered and missing.
3. **Agent sessions.** One judged session per scenario and arm, with the
   harness and judges of section 4 (Haiku agent, two Sonnet judges at
   medium effort). Measure, under Rubric A and Rubric B:
   - correct finishes;
   - turns to a correct finish;
   - the judges' verdicts.

   A close call gets a second session, as in section 4.5.
4. **Performance.** For each arm, record:
   - end-to-end Answer time;
   - per-stage and per-call model time;
   - rounds used;
   - partial results at the deadline;
   - peak GPU memory;
   - warm and reload times;
   - throughput with 2 and 4 concurrent Answers, using the queueing of
     section 12.4.1.

**What counts as significantly better.** This is my proposed bar. The user
decides.
- **Correct finishes:** Large wins at least 3 more correct finishes than
  Compact across the 20 scenarios.
- **Or a clear Rubric A lead:** Large wins Rubric A on at least 8 scenarios
  and loses on at most 3, with the rest tied.
- **Coverage:** in both cases, Large's first-reply essential coverage is no
  lower than Compact's on either repository.
- **Time:** in both cases, Large's median end-to-end time is no more than
  twice Compact's.

If none of the outcome conditions is met, the report recommends dropping
Large.

**The report.** `docs/arbiter/answer-evaluation/` gets a gate report. It
is written for the user's personal review and contains:
- a per-scenario side-by-side for both arms, with the evidence each
  returned, the judges' verdicts and the reasons;
- the agreement tables and the performance tables;
- the receipts;
- my recommendation.

The gate and its report are one series with one commit.

**Order and what continues during the review.**
- **When it runs:** as soon as both CUDA routes run the loop, after L5 and
  before L7.
- **During the user's review:** work that does not depend on Large
  continues, which is everything except Large-specific tuning.
- **Large waits:** calibration fitting, runtime-capacity tuning and the
  memory and placement work for Large wait for the user's decision. Until
  then Large keeps only the CUDA route and the ladder position the gate
  needs.
- **If Large is dropped:** the ladder becomes Compact on the GPU, then
  Compact on the CPU. Large's catalog entries, profiles and code paths are
  removed with evidence, under the deletion rule of the assignment.

### 12.8 Work order

| Step | Work | Done when |
| --- | --- | --- |
| L0 | Commit and push the pending one-turn evaluation work now, as it stands (user, 22 November). Include the harness, the reports, the results files, the change-set code, the indexer and search fixes, and the ledger entry. Leave out private roots. Its results are history and not a baseline | One signed commit on `feat/arbiter-final` with repository checks passing; the branch is pushed |
| L1 | Contract decisions and Core types: the new request, response and budget version replacing v2 at once (v1 and v2 removed), the ledger slots and token allowance, and the deadline ceiling, in the owning `CONTRACT.md` files, with generated TypeScript and Rust-emitted semantic cases | Contracts recorded; Core builds and its tests pass |
| L2 | Time: the new ceiling through MCP, Core, kernel, supervisor, ledger and read attempt; per-stage budgets; partial results; progress notifications; which clients reset timeouts on progress or support asynchronous tasks, recorded | Tests show a disclosed partial result at the deadline on every layer |
| L2b | Many agents (section 12.4.1, accepted), with its global limit taken from L5b: the service-owned Answer job table, attach-by-digest, tickets and continuations, the admission estimator, degrade-to-fit, earliest-deadline scheduling across Answers, the global gathering limit, and queue status | Tests with several concurrent Answers show no starved request, attach after a client timeout, and a ticket or disclosed degradation when the deadline cannot be met |
| L3 | Kernel loop: Split, then per part Plan, Retrieve per sub-need, Read and check coverage, Follow up and Compose inside `Gathering`, interleaved across parts with a shared read cache, then Union; with the deterministic no-model path, and the software follow-ups promoted from the evaluation layers | Kernel tests cover each stage, single-part and compound requests, budget hand-back between parts, Union dedup and frame sharing, early stop, budget and deadline stop, and the no-model path |
| L4 | Service and retrieval: the split, plan, coverage and follow-up task policies, ledger slots, per-attempt lane admission for parts, cross-part candidate batching (and the Focus per-candidate question decision), and the calibration format with the deterministic order used where the model abstains | Service tests pass; policy identities are recorded |
| L5 | Devices: the Focus CUDA route, Compact on CUDA as a product route, the Context ladder with typed reasons, and idle unload with pre-warm and reload handling (section 12.5) | Each route and fallback runs on the A6000 and on CPU, with receipts; reload and re-warm times measured per model and device; an Answer arriving during a reload starts on reads or a lower rung and does not time out |
| L5b | Runtime capacity (section 12.5.1): the warm-time calibration run, device and queue measurements, concurrent runs on one session with per-run CUDA streams, overflow routing across devices by earliest finish, shed-and-recover, capacity-based lane admission, settings and status | Tests show no second copy is ever loaded, capacity rises only within the memory floor and when measured gain holds, sheds under injected memory pressure without interrupting running calls, and overflow records its device |
| LG | The Large-against-Compact gate on CUDA (section 12.7.1) | Gate report and commit; **stop for the user's personal review and decision on Large**, while work not depending on Large continues |
| L6 | Workspace gate | Clippy and tests across the workspace on the matrix of CONTRIBUTING; known failures listed with their owners |
| L7 | The evaluation starts over on the loop. First, a new baseline: the loop on every device route, the no-model loop, Parse and Search, on both repositories, with CPU and GPU cells. Then change sets with weight and calibration refinement as in section 5, each series judged and committed on its own | The new baseline report, then one report and commit per series; stop for the user's inspection after the iterations the user sets |

E8 (the sealed set) and E9 (the final report) follow L7 on the user's word.
The new evaluation's reports are numbered after the one-turn reports, which
are kept as history.

### 12.9 Proceeding without interruption

This section lists what implementation may do without asking, the defaults
it uses, and the only points where it stops. It applies once the user has
given the final word on section 12.

**Authorized by the user's decisions on 22 November.**
- **Contract changes.**
  - Change Answer's wire shape, budgets and response, replacing v2 at once.
  - Remove v1, v2 and `Kernel::answer_with_reader`.
  - Raise Answer's time limits on every layer named in 12.4.
  - Change the joint ledger's slots and token pool.
  - Add the split, plan, coverage and follow-up task policies.
- **Digest inputs and identities.** Edit the source files that are digest
  inputs for these changes, and update the identities they move: Core
  context and Focus semantics, retrieval task policies, `runtime.rs` and the
  compiler, the `cpu_source_transition` pins, the CUDA FP16 math-policy
  digest, and the bounded-hybrid validator.
  - Each update is a recorded decision in the owning `CONTRACT.md`, with
    its retained-fixture proof.
  - The proof shows the old identity no longer describes the new source,
    not that a check now passes.
  - The 8 `cpu_source_transition` tests that fail today are resolved this
    way in L4, when the evaluation routing in `runtime.rs` is replaced by
    the product loop.
- **Promotion.** Move the software follow-ups out of the test-support gate
  into product code. Remove the evaluation layer tokens once the loop
  replaces them. Keep the evaluation fixture's restart exception
  evaluation-only.
- **Focus on CUDA.** Build the Focus CUDA profile for the 1,024-token graph,
  its placement and its warm route. Emit them as reviewed private profiles,
  under the same review rule as the existing CUDA Context profiles.
  - Product profiles need the independent review below.
  - If the Focus CUDA route cannot be made to work, Focus stays on the
    CPU, the gate and L7 run with Focus on the CPU, and the blocker is
    recorded. Neither stops.
- **Compact on CUDA.** Make it a reviewed product route. Retire the
  calibration-only variant mechanism from the product path, or record it as
  a decision if the product needs it.
- **Device work.** The Context ladder, idle unload, the many-agent queueing
  and runtime capacity, as specified in 12.4.1, 12.5 and 12.5.1.
- **Commits and pushes.**
  - Each L step is reviewed, validated and committed with sign-off, and
    `feat/arbiter-final` is pushed after each step.
  - No PR, merge, publication or deployment.

**Defaults that implementation uses.** Each is recorded in the owning
contract when it is made.
- **Limits per call.**
  - At most 4 parts per request, and at most 4 sub-needs per part.
  - At most 3 rounds of read and follow-up per part.
  - The budget's call and candidate limits are raised to what these allow.
  - The reply frame stays at 64 KiB, shared by all parts. It is raised only
    if L7 shows it truncates essential evidence.
- **Deadline.** 300 s by default and 600 s at most. Answer-only. A reserve
  is kept for Compose and Union.
- **Many agents.** As in 12.4.1:
  - degrade first, then a ticket;
  - 10-minute retention of finished results;
  - the global limit set from runtime capacity, with a floor of 2.
- **Idle unload.** 30 minutes, per stage.
- **Runtime capacity.** "Auto", with at most 2 concurrent runs per stage.
  - Reserve floor: the larger of 1.5 GB and 10% of the device.
  - A concurrent run stays only if it gains at least 1.3 times.
- **Focus batching across parts.** The per-candidate question is decided in
  L4 by measured cost.
- **Test models in L7 and the gate.** Haiku agents and Sonnet judges at
  medium effort, through headless Claude Code, as in section 2.
  - The 20 development scenarios with their frozen reference notes, on the
    existing evaluation indexes: airk at publication 330 and bookbuilder-v2
    at 268.
  - The sealed set stays unused.
- **L7 iterations.** Three change sets after the new baseline, back to back,
  then a stop for inspection. That is the user's last setting for
  iterations.
- **Version.** Workspace and extension versions are synchronized and bumped
  once, before the first installer or PR that carries the loop, as the
  assignment requires. No installer or PR is built in L1 to L7 without the
  user's word.

**Reviews.**
- Contract and wire changes (L1), the kernel loop (L3), and the service,
  device and capacity work (L4, L5, L5b) each get an independent review by
  a delegated agent before their commit. The agent is given the code, not my
  conclusions.
- Findings are fixed before the commit.
- Work done alone is labelled self-review.
- The security-relevant parts get a separate security review: the job table
  and its retention, attach-by-digest, and the queue status.

**Resources.**
- One Cargo owner and one GPU owner at a time, with the marker files of the
  one-turn evaluation.
- Disk is budgeted per CLAUDE.md before any large write. The Large CUDA
  weight copy and the evaluation installation are reused.
- Builds use the pinned toolchain and the offline Cargo home already in use.

**Known failing checks carried in.** Each has an owner, the primary agent,
and is listed in every validation record until resolved:
- the 8 `cpu_source_transition` tests, resolved in L4;
- the retrieval version-boundary test, checked at `1fe7b600` in L1 and
  fixed or recorded;
- the intermittent store cache test, pre-existing at `1fe7b600`;
- the module-cycle boundary check, the formatting quarantine and the Admin
  browser teardown of section 10.

**The only stops.**
1. The Large-against-Compact gate report (LG), for the user's personal
   decision on Large. Work that does not depend on Large continues.
2. The inspection after L7's iterations.
3. Anything the permission classifier denies, or anything outside this
   assignment: a new crate, a new external provider or service, a PR,
   publication or deployment. These are reported to the user and not worked
   around.

A failed check, a blocked dependency or a review finding is not a stop. It
is fixed, or recorded with its owner, while independent work continues.
