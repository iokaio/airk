# AIRK Arbiter plan: does Answer help an agent do software work?

Rewritten on 21 November 2026 (America/Denver) at the user's direction. It
replaces the FP16 delivery plan that stood in this file until then. The file
keeps its name so that existing links still resolve.

Status: written for the user to read. Nothing in sections 3 to 7 has started,
and nothing starts until the user has read this plan and corrected it. Section
9 lists every place where I interpreted an instruction, so that a wrong
reading is easy to find.
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
  data lives under the private campaign folder. The indexer honours
  `.gitignore`, which should keep build output and `node_modules` out; step
  E1 checks what is actually in scope. Before indexing I also check that no
  credential file is in scope (bookbuilder has an `aikeys.json` at its root)
  and exclude any that is through the index's own source policy.
- **Fixed for a test.** Watching is off. Each index is built once at a
  recorded commit and is not rebuilt during a test unless an adjustment
  requires it, in which case every path is run again.

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

With 20 scenarios that is 100 results in a full pass. Results 4 and 5 do not
depend on the models, so they are run once and reused until something they
depend on changes.

I recommend one addition: a control run of Answer with no model enabled.
Without it the test cannot tell whether a gain comes from the way Answer
gathers evidence or from Focus. It needs no model and is compared with result
1 mechanically, so it adds almost no cost.

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

- **The agent.** A fresh agent session for each scenario and path. It
  receives the scenario's instruction and nothing else about the repository.
  It may reach the index only through the tools of its path, through a small
  wrapper that enforces the path's tool list and logs every call: tool,
  arguments, bytes returned and time.
- **Its job.** Gather the context needed to do the task, then stop and write
  a short work brief: where it would work, what it must respect and what it
  still does not know. It does not do the task itself.
- **A turn** is one tool call. A session is capped at 12 turns.
- **First call.** The first call of a path uses the instruction as written:
  as the question for Answer, as the query for Search, and as the text for
  Parse. After that the agent chooses its own follow-ups.
- **Audit.** Every location the brief cites must appear in a logged result. A
  session that used anything outside the wrapper is discarded and rerun.

Device comparison and weight adjustment do not need an agent. They replay the
recorded Answer calls and compare the returned evidence (section 5.2).

### 3.6 Reference notes

Before any run, each scenario gets reference notes written by reading the
repository directly, never through the tools under test.

- Between four and ten items of needed context, each with a file, a symbol or
  line range, a one-line reason, and a mark of essential or helpful.
- Alternatives are listed where more than one place would serve.
- The notes and the instructions are frozen and hashed before the first run.
  A correction after that is allowed only for a factual error, and is
  recorded with its reason.

The notes make the first criterion measurable: after each turn, a script
checks which essential items the returned evidence has covered.

### 3.7 What Answer does today, from the source

These facts bound what the test can show and what adjusting weights can
achieve.

- **One Answer request** runs a search on the question, then follow-up reads
  on its first hits: a 32-line context read for each, and searches on
  capitalized word runs found in them. It stops at 8 calls, 3 rounds,
  262,144 bytes or 120 seconds by default.
- **Focus** reorders at most 4 hits of each search that Answer makes. It has
  no threshold, and its instruction text is a generic web-search one. Answer
  follows up on the first 8 hits in any case, so reordering 4 of them changes
  little unless a budget binds.
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

Measured from the turn log and the reference notes:

- **Turns to sufficient context.** The number of the turn after which every
  essential item was covered. If the session ends without that, the result
  is "not reached" and the missing items are listed.
- **First-call yield.** The share of essential items covered by the first
  call alone.
- **Context cost.** Bytes returned to the agent over the session, and
  elapsed time. Fewer turns bought with a very large or very slow reply is
  recorded as such.

Judged from the session:

- **Misleading or noisy evidence.** Whether irrelevant or wrong material sent
  the agent down a wrong path.
- **The work brief.** Whether the agent's own account of the task is correct
  and complete against the notes.

Each result gets one grade:

| Grade | Meaning |
| --- | --- |
| 4 | Sufficient after one call, and the brief is correct |
| 3 | Sufficient after two calls |
| 2 | Sufficient after three or four calls |
| 1 | Sufficient after five or more calls, or the right area was found and an essential item is still missing |
| 0 | Not sufficient, misleading, failed or timed out |

A grade is lowered by one when the evidence misled the agent or the brief is
wrong on an essential point.

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

I propose this rule and ask the user to set the numbers.

- **A substantial win on a scenario:** the Answer result reaches sufficient
  context at least two turns sooner than the better of Parse and Search, or
  in one call where the better of them needed three or more.
- **A configuration substantially improves results** when it wins that way
  on at least 12 of the 20 scenarios, is worse on at most 2, wins at least 5
  in each repository, and is not worse on average in any of the four kinds
  of task.
- **The second criterion** is reported beside it as the difference in Rubric
  B averages. It does not decide the outcome. It breaks ties and guides how
  Answer presents evidence.

Results are reported for each configuration, each repository and each kind
of task, with every scenario listed. Twenty scenarios support a clear
difference and cannot support a fine one, and the report says which it is.

### 4.4 Keeping the judging honest

- **Frozen inputs.** Instructions and reference notes are fixed before any
  run.
- **Blind first pass.** Two independent judge sessions score each result
  from the rubric, the notes and the evidence, with the path names hidden
  where that is possible. Answer's reply has a recognizable shape, so the
  blinding is partial and I say so.
- **Adjudication.** I settle any scores that differ by more than one grade
  and record why.
- **The user's packet.** For each scenario: the instruction, the notes, the
  five results side by side with their turn logs, the scores and my reasons.
- **A sealed second set.** I recommend writing a second set of ten scenarios
  for each repository at the same time, with the same mix, and not running
  them until the adjustments are finished. Adjusting against 20 scenarios
  will fit those 20. The sealed set shows whether the gain holds.

## 5. Developing weights from the tests

Weights are no longer developed from public question sets. They are adjusted
after each test, from what the scenarios show.

### 5.1 What can be adjusted

"Weights" here means every adjustable value that shapes what Answer returns.
They are listed from cheapest to most expensive, and each test starts at the
top.

| Level | What | Where it lives | Cost to change |
| --- | --- | --- | --- |
| 1 | Answer's gathering: calls, rounds, fan-out, bytes per read, lines per context read, hits shown per record | Request budget fields and constants in the kernel | Minutes. No model, no reindex |
| 2 | Retrieval ranking: how lexical and vector results are fused, snippet size, chunk size | Constants. Fusion gives both sides equal votes today | Minutes for fusion and snippets. Chunk size needs a reindex. Changes the Parse and Search results too, so they are rerun |
| 3 | Focus without training: how many hits it scores, its token width, its instruction text | The model profile and the Focus compiler | Hours. The instruction is a digest input, so the product change needs a recorded decision |
| 4 | Context without training: the rules that decide which actions are accepted, and how the choices are put to the model | The calibration asset and the roster and prompt code | Hours to days. The roster and prompt are digest inputs |
| 5 | Focus model weights | The existing LoRA tooling. A pilot checkpoint exists and has never been compared | A day or more per candidate, on the A6000 |
| 6 | Context model weights | No trainer exists yet | Days. Built only if levels 4 and 5 show it is worth it |

Level 4 is where Context first becomes able to act at all (section 3.7).
Changing its rules cannot make it add a read, only choose among queued reads
or stop early. If the tests show that this is the limit, the adjustment is to
what Context is asked to do, and that comes to the user as a design decision.

### 5.2 The loop

For each Answer configuration, in the order of section 6:

1. **Test.** Run the agent sessions and judge them.
2. **Diagnose.** For each scenario where Answer did not win, say why from the
   turn log: the needed item was never retrieved, was retrieved and not
   followed, was followed and dropped in packing, or was present and buried.
3. **Adjust one thing.** Choose the cheapest level that addresses the most
   common cause. One change at a time.
4. **Replay.** Rerun the recorded Answer calls with the change and measure
   coverage of the reference notes mechanically. This takes minutes and uses
   no agent sessions.
5. **Confirm.** If coverage improved, rerun the agent sessions for that
   configuration and judge them again.
6. **Record.** Every adjustment goes into a table: what changed, why, the
   coverage before and after, the judged result, kept or reverted.

A configuration's loop ends when it meets the rule of section 4.3, when three
adjustments in a row bring no gain, or when the user says so.

Experiments run in the private evaluation build. A change that the tests
favour reaches the product only through the normal path: a recorded decision
where it touches a contract, a digest input or a stored format.

### 5.3 Training data for model weights

Levels 5 and 6 need training pairs, and the 20 judged scenarios are never
training data. I propose pairs of the same shape as the scenarios, mined from
real repositories:

- A commit's message as the instruction, and the files and regions it
  changed as the needed evidence.
- A documentation section as the instruction, and the code it describes as
  the needed evidence.
- A test as the instruction, and the code under test as the needed evidence.

Any mined pair whose evidence overlaps a judged or sealed scenario's notes is
left out. Training keeps the earlier protection for existing abilities: the
original weights stay selectable, and training includes general replay.

### 5.4 Guards

- **Fitting the test.** The sealed set of section 4.4.
- **Fitting the repository.** A model trained on one repository's history
  may only have memorized it. A trained candidate is also checked across
  repositories: trained on pairs from one, tested on the scenarios of the
  other.
- **Regressions.** After each kept adjustment the earlier configurations are
  replayed, so a gain for Compact does not silently cost Focus-only.

## 6. Work order

Nothing here has started.

| Step | Work | Done when |
| --- | --- | --- |
| E0 | The user reads this plan and corrects it | Section 9 is answered |
| E1 | Build the evaluation service from current source. Create the two indexes with vectors. Check scope and credential exclusion | Both indexes serve hybrid search, and their file counts and commits are recorded |
| E2 | Write the 20 scenarios and their reference notes, and the sealed set if adopted. Freeze them | The user has seen the scenario list; hashes recorded |
| E3 | Build the harness: the path wrapper and turn log, the coverage script, the judge packet | A pilot of 2 scenarios in each repository runs end to end on all five paths |
| E4 | Baseline: Parse, Search and the no-model Answer control on all 20 scenarios | Judged, and reported to the user |
| E5 | Test 1, Focus only, CPU and GPU. Judge, then the adjustment loop | The loop's end condition; results reported |
| E6 | Test 2, Focus and Compact, CPU and GPU. Judge, then the loop | The same |
| E7 | Test 3, Focus and Large, GPU, with the CPU measurement on 4 scenarios. Judge, then the loop | The same |
| E8 | Run the sealed set once on the final configurations | Reported beside the development results |
| E9 | Report: the answer to section 1 for each configuration, the adjustment table, and what to deliver | The user decides what ships |

I report to the user after E2, E4 and each of E5 to E7, because each one can
change what the next should be. Platform work (Linux, macOS, Radeon,
packaging) waits for E9, since E9 says which stages are worth delivering.

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
| Large gated by GPU opt-in | Route planning with typed reasons (commit `e587bf6d`). Large still has CPU profiles in the catalog | The product rule of section 2: no CPU candidate for Large, and Admin offers Large only with GPU use on. A recorded decision first |

## 8. Cost

These are estimates. The pilot in step E3 replaces them with measurements.

- **A full pass** is 100 agent sessions and 200 judge sessions. I estimate
  two to three hours of wall time and on the order of ten million tokens.
- **An adjustment replay** is 20 recorded calls and takes minutes with no
  agent sessions. A confirmation is 20 agent sessions.
- **The whole campaign**, with about two kept adjustments for each
  configuration, is on the order of 220 agent sessions.
- **Disk.** One evaluation installation with the FP16 weight files is about
  13 GB, and the two indexes are unmeasured. D: has about 39 GB of its
  budget left.
- **If that is too much**, a lighter protocol replaces the agent sessions
  with one scripted first call for each path and a judged estimate of the
  follow-up turns. It is cheaper and less direct, because it estimates turns
  where this plan measures them.

## 9. My reading of your instructions: please confirm or correct

1. **Twenty scenarios.** Ten for each repository, the same ten kinds of
   instruction in both. The alternative is ten in all, five in each.
2. **Five results.** The five are the three Answer configurations plus Parse
   and Search, for each scenario. CPU and GPU are two runs of the same Answer
   configuration, judged once unless their evidence differs.
3. **"After each test, adjust weights."** A test is one Answer configuration
   run over the scenarios. So the order is Focus only, adjust; Focus and
   Compact, adjust; Focus and Large, adjust.
4. **"Weights"** covers the six levels of section 5.1, cheapest first, and
   not only trained model weights.
5. **Agent sessions with a logged turn count** (section 3.5), in place of my
   estimating turns from one reply. Section 8 gives the lighter alternative.
6. **The follow-up tools of each path** (section 3.3). In particular, the
   Parse path uses Parse only.
7. **Large on CPU in tests** is 2 of each 10 scenarios, 4 in all. With GPU
   use on, a Large that cannot run on the GPU falls back to Compact and never
   to Large on CPU.
8. **The three additions I recommend:** the no-model Answer control, the
   sealed second set of scenarios, and training pairs mined from repository
   history.
9. **The thresholds of section 4.3.**
10. **Experiments in the private evaluation build** may change digest-input
    code such as the Focus instruction and the Context prompt without a
    recorded decision each time. Product changes still need one.
11. **Platform work waits for E9.**
12. **Scenario review.** Whether you want to approve the 20 final scenarios
    before the first run, or only see them.

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
| The user reviews discordant pairs of the paired comparison | Retired. The user now reviews the judging packet of section 4.4 |
| Pushes of `feat/arbiter-final` to `origin` | Authorized. No PR, merge, publication or deployment |
