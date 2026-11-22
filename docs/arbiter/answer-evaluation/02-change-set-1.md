# 02 Change set 1: software-task gathering, a wider Focus pool and a Context that can act, on airk

Written on 22 November 2026 (UTC) from the private receipts of the change-set-1
replays (05:16 to 06:18 UTC), the judged airk pass (06:20 to 06:48 UTC) and the
images built for the series. It goes into one commit with the change set's code
and [results/02-change-set-1.csv](results/02-change-set-1.csv).

> **Headroom: 10 of 10 airk scenarios**, unchanged from the baseline: Parse and
> Search were not changed, and their results are the baseline's. Round 1 is
> airk only, by the user's direction, so the full rule (12 of 20 scenarios, at
> least 5 wins in each repository) cannot be applied yet.

## 1. Summary

Change set 1 rebuilt how Answer gathers evidence for a software task (eight
changes in three layers: gathering, Focus and Context) in a private evaluation
build, tuned its starting values by replay, and was tested on airk's ten
scenarios with Haiku agents and two Sonnet judges per result.

**No new correct finish.** Answer reached none in any configuration, as at the
baseline. Search, unchanged, keeps its two (C1 and T1, grade 1). Every Answer
configuration is still worse than Search on C1 and T1 and ties at grade 0 on
the other eight.

What moved is small. The first Answer call now covers 4 of 53 essential notes
(baseline 2 of 53), against Search's 2. Over the session, Focus covered 13 and
Compact 14 notes (baseline 11 and 8); Answer with no model stayed at 12. Search
covers 22. One no-model session (T1) had evidence covering all five essential
notes and still failed, because the agent's brief left two of them out.

**Calls got slower.** The no-model Answer call averaged 12.6 s (baseline 9.1 s),
Focus 27.5 s (22.9 s), Compact 57.7 s (49.1 s) and Large 82.3 s (66.4 s). Two
Compact calls took 146.6 s and 123.6 s, over the 120 s deadline the request
sets.

**Only one of the six gathering layers measurably changed the first call.** In
leave-one-out replays, removing the larger first search (`pool`) lost a note;
removing any other layer changed nothing. The Focus instruction, the trained
Focus candidate and the provisional Context rule each left first-call coverage
exactly where it was. Three bugs found during the series were fixed before the
pass, and two recorded digest-input changes and eight known failing tests are
part of what this build costs (section 6.1).

The GPU-cell replays and the Context leverage measurement are pending
(sections 4.5 and 4.6).

## 2. What changed in this series

The changes live in a private evaluation build. Every layer is off unless the
evaluation fixture binds `AIRK_ARBITER_EVALUATION_ANSWER_LAYERS` for its
process; a product build always gets every layer off, so product gathering,
packing and Focus are unchanged (`crates/airk-kernel/src/arbiter/CONTRACT.md`,
"Private evaluation change set 1"). The judged pass ran with the layers
`all,pool_full=8`.

The measured effects below are from the no-agent replays of the first Answer
call of each airk baseline session (section 4.4), scored with the same coverage
rule as the pass. They measure the first call only.

| # | Change | Cause it addressed (baseline) | Expected effect | Measured effect | Kept or dropped |
| --- | --- | --- | --- | --- | --- |
| 1 | **Software-task follow-ups** (layer `followups`): a whole-chunk read of each kept hit; `symbols` and `references` lookups for identifiers named in the instruction and the kept hits, with `read_file` excerpts at their locations, callers and tests (the Parse recipe shapes); file outlines; the CONTRACT.md or README that governs a hit's folder, queued only after `file_info` shows it indexed. A failed optional follow-up keeps its failure receipt and the rest of the queue | Answer's only follow-ups were 32-line context reads and searches on runs of capitalized words, and one failed read cleared the whole queue (A1, A2 and W2 no-model replies ended `refused`) | The largest gain of the set, on coding, test and architecture notes (definitions, callers, tests, contracts) | Leaving it out: 3 of 53 on every configuration, the same as with it. Without it, the no-model call was slower (18.6 s against 9.5 s mean). No first reply ended `refused` in the pass (baseline: 4) | Kept in the pass. No first-call coverage gain measured |
| 2 | **Spread follow-ups across hits** (`spread`): one read per kind per cycle, queue up to `fanout.min(candidates)` | The top hit filled the queue | Reads for several hits within the first calls | Leaving it out: 3 of 53 on every configuration, no change | Kept in the pass. No measured effect |
| 3 | **A larger first search** (`pool`): 16 focused snippets instead of 8 hits sharing 4,096 bytes, whole chunks only for kept hits | 8 hits shared 4,096 bytes | Items ranked 9 to 16 enter the pool | Leaving it out: 2 of 53 on every configuration (C1 lost). **The only layer whose removal changed coverage** | Kept |
| 4 | **Focus scores the pool and the best are kept** (`focus`): an evaluation-only Focus profile 16 wide, at most six pairs per Answer, bounded windows, kept hits by Focus relevance; one pool size on both devices | Focus reordered at most 4 of 8 hits | Promotion of items ranked 5 to 16 into the kept set | Focus against no model, same layers: 3 of 53 each, the same notes (C1, C2, T1). Focus added 9.0 s per first call in replay (18.5 s against 9.5 s) | Kept in the pass. No first-call gain |
| 5 | **The software-task Focus instruction** `SoftwareEvidenceV1` in place of the web-search one (digest-input change, section 6.1) | Effect unknown | Unknown; isolate by leaving it out | Leaving it out (publisher instruction, same width): 3 of 53, the same as with it, at 24.7 s against 18.5 s mean (one replay each) | Kept in the pass. No measured effect, and it costs a digest-input change |
| 6 | **Context can act**: reads stated in words, more varied choices from changes 1 and 2, and a provisional data-only rule that accepts a Read when it is the top card (option A) | With zero rules Context abstained on every call | Compact and Large differ from Focus because Context chooses | Compact with the original zero-rule calibration: 3 of 53, the same as with the provisional rule. In the pass, Compact's first-call evidence differed from Focus's on A1, A3, T1 and W1 and was identical on C1, C2, W2, T2 and A2 | Kept in the pass. Whether Context chose anything is the pending leverage result (section 4.6) |
| 7 | **Packing and presentation** (`present`): each hit once, overlaps dropped, grouped by purpose with exact locations, a closing disclosure, least useful kinds omitted first | Rubric B; frame space | Rubric B up; more room for records | Leaving it out: 3 of 53, no change. Rubric B rose a little on every path (section 4.1) | Kept in the pass |
| 8 | **Device-shaped budget** (`device`): a Context call is admitted only when its measured cost plus a read reserve fits the deadline | Compact calls of up to 97.7 s, Large 66.4 s | No CPU call over 120 s | Leaving it out: 3 of 53; Compact replay mean 28.3 s against 25.9 s with it, longest 112.2 s against 100.0 s. In the pass two Compact calls exceeded 120 s (section 6) | Kept in the pass. It did not keep CPU inside 120 s |

**The tuning tokens.** Six replay-only tokens (`pool`, `pool_full`,
`read_floor`, `kept`, `subjects`, `governing`) were added so starting values
could be tuned without a rebuild; omitting them is change set 1 exactly.

| Variant | No model | Focus | Compact | No-model first call, mean |
| --- | --- | --- | --- | --- |
| `all` (change set 1 as built) | 3/53 (C1, C2, T1) | 3/53 | 3/53 | 9.5 s |
| `all,pool_full=8`: 16 snippets, then one full-chunk search of the top 8 | **4/53** (C1, C2 twice, T1) | 4/53 | 4/53 | 14.7 s |
| `all,pool=8,pool_full=8,read_floor=3072,kept=3,subjects=strict,governing=project` | 3/53 (C2 twice, T1; C1 lost) | 3/53 | 3/53 | 11.7 s |
| Recorded baseline first calls | 2/53 | 2/53 | 2/53 | 8.5 s |

`pool_full=8` was kept for the pass: it gained one C2 note on all three
configurations at 5.2 s more per no-model first call. The full tuned set was
dropped: it gained the same C2 note and lost C1.

**Weights tried and not used.** Focus candidate 1, trained on pairs from both
repositories, covered 3 of 53 in replay, the same notes as the base weights at
the same width, and was slower (22.9 s against 18.5 s mean). The pass used the
base weights.

**Evaluation-setup fixes carried in the images.** The search roster fix of
[01-baseline.md](01-baseline.md) section 6.1 and the indexer speed fixes are
in every change-set image from `cs1d` on, as in the baseline image. They are
reported there.

## 3. What was tested

| Item | Value |
| --- | --- |
| Scenarios | The ten airk development scenarios of [scenarios.md](scenarios.md), 53 essential notes |
| Paths | Search and Parse (reused from the baseline, section 4.3); Answer with no model; Answer with Focus (CPU); Answer with Focus and Compact (CPU); Answer with Focus and Large (CPU, A1 and C3 only, timing only, 180 s client timeout) |
| Agents | `claude-haiku-5-5`, `--effort medium`, Claude Code 2.1.330, 12 turns at most |
| Judges | Two independent `claude-sonnet-5-5` sessions per result, `--effort medium` |
| Answer budget, every call | calls 12, rounds 3, fanout 12, candidates 32, input tokens 8,192, max bytes 524,288, deadline 120,000 ms |
| Answer layers | `all,pool_full=8` |
| Focus profile | `focus-static1024-cpu-cs1-software-w16` (16 wide, `SoftwareEvidenceV1`) |
| Context profiles | `context-compact-cpu-cs1-provisional-read`, `context-large-cpu-cs1-provisional-read` (provisional Read rule; terminal cards unsupported) |
| Change-set inputs | `change-set-1/profiles/change-set-inputs-r2.json` |
| Image | `cs1e-1a24414d` (service `18693f1a…6663e5`), from `1fe7b600` plus the uncommitted working tree described in section 9 |
| Lifetime | `L31-cs1e` |
| Index | `idx_ac4e36f1c8b265719b0ae05d28894fb0`, publication generation 330, token `airk-r1:d9feb47f…:330:5d1a2e84…f515dbc9da33`, the baseline's |
| Settled guard | 13 snapshots. 12 report the same token from 06:20:44 to 06:45:50 UTC. One, at 06:30:06, got `scoped_connection_unavailable` during a stage warm and stored no token; that stopped attempt 1 (section 6) |
| Prompt freeze | Attempt 1 under `8096d3c6…`, attempt 2 under `3cf8dedc…` (frozen 06:33:26 UTC). Both changed harness files only (`prompts_changed: false`) |
| Scenario freeze | `scenario-freeze.json`, SHA-256 `0a7d7b0a…e7806`, unchanged |
| Results | 52: 27 sessions run and judged, 25 reused (section 4.3) |
| Second sessions | None: no close call |
| Cost | Agents $0.3662, judges $6.1348, total $6.501 |

The pass ran as two driver attempts. Attempt 1 (06:20:30 to 06:30:06 UTC,
576.3 s, exit 1) completed the no-model and Focus sessions and stopped at the
start of Compact. Attempt 2 (06:33:34 to 06:48:14 UTC, 880.4 s, exit 0) kept
those sessions and ran Compact, Large, coverage, judging and grading.

## 4. Results

### 4.1 Against Parse and Search

Each cell gives the essential notes covered by the end of the session and the
number of calls. Every result graded 0 except Search on C1 and T1 (finish at
9 and 7, grade 1). "(r)" marks a reused result.

| Scenario | Search (r) | Parse (r) | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- | --- | --- |
| C1 coding | 5/5, 11, **finish at 9, grade 1** | 0/5, 5 | 3/5, 11 | 3/5, 10 | 3/5, 10 (r) | – |
| C2 coding | 4/6, 10 | 2/6, 4 | 3/6, 12 | 2/6, 9 | 2/6, 9 (r) | – |
| C3 coding | 2/5, 10 | 2/5, 6 | 1/5, 11 | 2/5, 8 | 2/5, 12 | 3/5, 7 |
| W1 writing | 0/5, 3 | 0/5, 3 | 0/5, 6 | 1/5, 8 | 1/5, 7 | – |
| W2 writing | 0/6, 7 | 1/6, 7 | 0/6, 10 | 0/6, 8 | 0/6, 8 (r) | – |
| T1 test | 5/5, 8, **finish at 7, grade 1** | 2/5, 5 | **5/5, 12, no finish** | 4/5, 11 | 4/5, 12 | – |
| T2 test | 1/4, 6 | 0/4, 3 | 0/4, 10 | 0/4, 7 | 0/4, 7 (r) | – |
| A1 architecture | 1/5, 7 | 0/5, 5 | 0/5, 11 | 0/5, 12 | 1/5, 12 | 0/5, 10 |
| A2 architecture | 1/6, 6 | 0/6, 8 | 0/6, 11 | 1/6, 10 | 1/6, 10 (r) | – |
| A3 architecture | 3/6, 8 | 0/6, 8 | 0/6, 8 | 0/6, 8 | 0/6, 9 | – |

Per path, over its rows, with the baseline in brackets. A reused row counts
with its source's values.

| | Search | Parse | No model | Focus | Compact | Large (2 rows) |
| --- | --- | --- | --- | --- | --- | --- |
| Correct finishes | 2 | 0 | 0 (0) | 0 (0) | 0 (0) | 0 (0) |
| Essential notes covered | 22/53 | 7/53 | 12/53 (12) | 13/53 (11) | 14/53 (8) | 3/10 (0) |
| Covered by the first call | 2/53 | 0/53 | 4/53 (2) | 4/53 (2) | 4/53 (2) | 0/10 (0) |
| Mean calls per session | 7.6 | 5.4 | 10.2 (8.9) | 9.1 (9.2) | 9.6 (9.7) | 8.5 (10.0) |
| Mean bytes per session | 104,514 | 150,069 | 183,559 (166,281) | 155,466 (166,316) | 175,846 (172,273) | 164,060 (155,234) |
| Mean first reply | 22,644 B in 4.7 s | 1,843 B in 0.3 s | 54,015 B in 13.2 s (51,326 B, 8.5 s) | 53,694 B in 28.7 s (51,369 B, 19.1 s) | 53,683 B in 38.0 s (51,047 B, 24.9 s) | 53,926 B in 87.8 s (45,430 B, 82.1 s) |
| Mean tool time per session | 17.6 s | 18.9 s | 30.0 s (20.3) | 42.3 s (36.7) | 79.2 s (40.7) | 129.0 s (112.1) |
| Mean wall time per session | 40.1 s | 44.1 s | 56.8 s (44.2) | 64.1 s (61.2) | 103.5 s (65.7) | 153.6 s (135.0) |
| Agent cost of the sessions run | – | – | $0.1376 | $0.1241 | $0.0794 (5 run) | $0.0252 |
| Rubric B, mean of both judges | 1.688 | 0.188 | 1.312 (1.250) | 1.325 (1.275) | 1.300 (1.212) | 1.125 (0.562) |

The first-call notes are the same for all three Answer configurations: C1 1 of
5, C2 2 of 6 and T1 1 of 5. At the baseline they were C2 1 of 6 and T1 1 of 5,
the same as Search's first call.

Bytes and time per scenario (bytes returned over the session, then tool time
and wall time):

| Scenario | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- |
| C1 | 206,392 B, 20.2 / 45.5 s | 179,970 B, 68.1 / 90.3 s | (r) | – |
| C2 | 191,590 B, 30.5 / 60.2 s | 164,426 B, 27.7 / 46.9 s | (r) | – |
| C3 | 161,170 B, 26.2 / 54.6 s | 134,037 B, 23.7 / 42.6 s | 243,514 B, 176.1 / 208.0 s | 165,551 B, 157.9 / 179.6 s |
| W1 | 173,263 B, 28.5 / 50.7 s | 151,831 B, 18.5 / 39.5 s | 166,026 B, 68.5 / 92.6 s | – |
| W2 | 141,580 B, 24.9 / 52.4 s | 114,747 B, 65.1 / 85.0 s | (r) | – |
| T1 | 134,203 B, 22.1 / 53.8 s | 179,373 B, 23.7 / 50.4 s | 173,935 B, 21.6 / 54.7 s | – |
| T2 | 233,333 B, 48.6 / 74.3 s | 132,637 B, 66.2 / 87.2 s | (r) | – |
| A1 | 188,136 B, 39.7 / 67.4 s | 177,416 B, 19.2 / 45.5 s | 188,957 B, 20.9 / 45.0 s | 162,570 B, 100.1 / 127.6 s |
| A2 | 187,532 B, 21.2 / 48.8 s | 185,968 B, 67.1 / 89.8 s | (r) | – |
| A3 | 218,398 B, 37.6 / 59.9 s | 134,256 B, 43.5 / 64.2 s | 208,288 B, 210.2 / 235.8 s | – |

Search and Parse are the baseline's (see [01-baseline.md](01-baseline.md)
section 4.1).

**Per kind of task** (essential notes covered; every Answer grade is 0):

| Kind | Notes | Search | Parse | No model | Focus | Compact |
| --- | --- | --- | --- | --- | --- | --- |
| Coding (C1 to C3) | 16 | 11, mean grade 0.333 | 4 | 7 (7) | 7 (6) | 7 (3) |
| Writing (W1, W2) | 11 | 0 | 1 | 0 (1) | 1 (0) | 1 (1) |
| Test (T1, T2) | 9 | 6, mean grade 0.5 | 2 | 5 (2) | 4 (3) | 4 (2) |
| Architecture (A1 to A3) | 17 | 5 | 0 | 0 (2) | 1 (2) | 2 (2) |

Every Answer configuration is still worse on average than the better of Parse
and Search in coding and in test, and level in writing and architecture.

**Against the rule, per configuration** (airk only, so not a verdict):

| Configuration | Substantial wins | Ties at grade 0 | Worse |
| --- | --- | --- | --- |
| No model | 0 | 8 | 2 (C1, T1) |
| Focus | 0 | 8 | 2 (C1, T1) |
| Compact | 0 | 8 | 2 (C1, T1) |
| Large (A1, C3) | 0 | 2 | 0 |

Rubric B: the harness's better-of comparator scores 1.475 on these rows
(Search alone 1.688). Every Answer configuration is below it.

**Per repository.** airk only. The bookbuilder pass of change set 1 was not run
(round 1 is airk only); its spec is prepared.

### 4.2 What each model adds

On the same ten rows, from the pass summary (baseline in brackets). The
comparator is the better of Parse and Search per scenario under the one rule of
[01-baseline.md](01-baseline.md) section 4.2: Search on eight scenarios and
Parse on C3 and W2.

| Step | Correct finishes added | Turns saved | Time added (mean tool time per session) |
| --- | --- | --- | --- |
| No model against the better of Parse and Search | −2 (lost C1 and T1) | 0 | +10.1 s (+0.5 s); 101.4 s over 10 rows |
| Focus against no model (CPU) | 0 | 0 | +12.3 s (+16.3 s); 123.3 s over 10 rows |
| Compact against Focus (CPU) | 0 | 0 | +36.9 s (+4.0 s); 368.7 s over 10 rows |
| Large against Compact (CPU, A1 and C3, timing only) | 0 | 0 | +30.5 s (+31.4 s); 61.0 s over 2 rows |

The Answer call itself, from the sessions that ran it:

| Configuration | Answer calls | Mean | Longest | Failed |
| --- | --- | --- | --- | --- |
| No model | 18 | 12.6 s (9.1) | 19.2 s (17.0) | 0 |
| Focus | 13 | 27.5 s (22.9) | 62.3 s (47.9) | 0 |
| Compact (5 sessions run) | 8 | 57.7 s (49.1) | 146.6 s (97.7) | 0 |
| Large | 3 | 82.3 s (66.4) | 92.1 s (66.4) | 0 |

- **Focus over no model:** one more note over the session (13 against 12), the
  same first-call notes, 12.3 s more tool time per session. Its evidence
  differed from no model's on all ten first calls. Three Focus first calls took
  57.3 to 62.3 s (C1, W2, T2), and the second Answer call on A2 took 49.9 s.
  Focus has not earned its place on airk.
- **Compact over Focus:** one more note (A1), no finish, 36.9 s more per
  session. On five rows its first-call evidence equalled Focus's, so those
  results were reused. Where it ran, C3 and A3 took 176.1 s and 210.2 s of tool
  time. The digest replay of C3 Compact returned nothing after 120,338 ms.
- **Large on CPU** (measurement only; Large never runs on CPU in production):
  C3 covered 3 of 5 against Compact's 2, A1 none. First calls 83.6 s (C3) and
  92.1 s (A1).

### 4.3 Reuse

A result is reused when its path's canonical first call has an evidence digest
identical to an earlier completed session with the same tools, agent model and
effort.

| Result | Reused from | Why |
| --- | --- | --- |
| Search, all ten | Baseline Search sessions | First-call digest equal to the baseline's on all ten. Change set 1 does not touch shared search |
| Parse, all ten | Baseline Parse sessions | First-call digest equal on all ten (the same empty-plan digest `23e3386d…`) |
| Compact on C1, C2, W2, T2, A2 | This pass's Focus sessions | Compact's first-call evidence equal to Focus's |

25 reused, 27 run and judged. Compact ran fresh on A1, A3, T1 and W1, where its
evidence differed from Focus's, and on C3, where its digest replay timed out.
No Focus result was reused from no model: their evidence differed on every row.
Large ran fresh on both rows.

### 4.4 The replays

No agent. Each replay re-ran the first Answer call of the ten airk baseline
sessions on the change-set image with one variant, and scored the reply with
the pass's coverage rule. All ran on the settled index (the token above,
unchanged at the start and end of each replay), CPU only, with the change-set
budget. 10 of 10 calls returned in every replay below. None returned evidence
equal to the recorded baseline call.

| Replay | Lifetime | No model | Focus | Compact |
| --- | --- | --- | --- | --- |
| `all` (as built) | L22-cs1e | 3/53, 9.5 s | 3/53, 18.5 s | 3/53, 25.9 s |
| `all,pool_full=8` (the pass) | L25-cs1loo | 4/53, 14.7 s | 4/53, 27.0 s | 4/53, 26.9 s |
| Tuned token set | L24-cs1loo | 3/53, 11.7 s | 3/53, 19.8 s | 3/53, 27.6 s |
| `all,-followups` | L26-cs1loo | 3/53, 18.6 s | 3/53, 24.4 s | 3/53, 29.8 s |
| `all,-spread` | L27-cs1loo | 3/53, 8.7 s | 3/53, 17.6 s | 3/53, 24.8 s |
| `all,-pool` | L28-cs1loo | **2/53**, 12.2 s | **2/53**, 19.5 s | **2/53**, 26.4 s |
| `all,-present` | L29-cs1loo | 3/53, 9.6 s | 3/53, 21.5 s | 3/53, 28.1 s |
| `all,-device` | L30-cs1loo | 3/53, 9.4 s | 3/53, 19.0 s | 3/53, 28.3 s |
| Focus candidate 1 weights | L22-cs1e | – | 3/53, 22.9 s | – |
| Focus with the publisher instruction | L22-cs1e | – | 3/53, 24.7 s | – |
| Compact with the zero-rule calibration | L22-cs1e | – | – | 3/53, 28.8 s |
| Large, A1 and C3 (`all`) | L22-cs1e | – | – | Large: 0/10, 93.0 s |
| Recorded baseline first calls | – | 2/53, 8.5 s | 2/53, 19.1 s | 2/53, 24.9 s |

Times are the mean first-call time. The longest Compact replay call was
112.2 s (`all,-device`); the longest with `all` was 100.0 s.

### 4.5 GPU cells: not run

The GPU scenario replays never ran. The CUDA image `cs1e-cuda-f79755a7` was
built at 05:44 UTC on 22 November (474.5 s; its unit filter passed 19 of
19). On 22 November the user stopped the evaluation runs and redirected the
plan to a multi-round Answer loop (plan section 12) before any GPU window
opened. So this report has no GPU coverage, first-call time or Large-over-Compact
step on the evaluation indexes.

**What the GPU track did measure**, on the change set 1 provisional-read
CUDA variants (option A) and on a 7-file synthetic repository, from the
GPU-track receipts:

| Context call on the RTX A6000 (`onnxruntime-cuda/1.30.0`) | First call | Warm calls | Input tokens |
| --- | --- | --- | --- |
| Compact, change set 1 variant | 656 ms | 231 ms | 1,191–1,195 |
| Large, change set 1 variant | 982 ms | 563–569 ms | 1,191–1,195 |
| Compact, synthetic lifetime | 596 ms | 217–225 ms | 1,144–1,191 |
| Large, synthetic lifetime | 762–957 ms | 533–558 ms | 1,144–1,191 |

- **Context abstained on CUDA too.** Every assessment abstained, as on CPU,
  so the device changed timing only.
- **Focus is the bottleneck.** It has no GPU route, so it ran on CPU in
  every lifetime, at 3.7–3.8 s per score at 1,024 tokens. That is about
  15 s of a 17–18 s synthetic Answer call with Context on CUDA.
- **Large on the GPU is fast enough to use.** Large on CPU took about 100 s
  per Answer call in this pass's replays (section 4.4). On CUDA a Large
  Context call is under a second.
- **Why the GPU showed almost no activity during the passes:** every
  evaluation lifetime of this campaign ran on CPU.

The plan's section 10.1 gives the full GPU and CUDA summary and what it means
for the Answer loop.

### 4.6 Context leverage: pending

Not in the receipts when this report was written. The Context trainer design
sets a go/no-go before any Context training: first measure, on the change-set-1
scenarios' first Answer calls and with no training, the oracle leverage (the
share of calls where some offered Read, forced to the front, raises delivered
essential coverage over queue order) and how much of that gain the provisional
Compact rule already captures. When it has run, this report gains:

- the oracle leverage (share of calls) and the mean oracle gain in essential
  notes per call;
- provisional Compact's chosen read against the oracle and against queue order,
  including how often Context accepted a Read at all;
- the go, no-go or in-between verdict under the design's thresholds (no-go
  below 10% of calls or 0.2 notes per call; go at 20% and 0.5 notes with
  Compact capturing under 40%), and what it means for change set 2 (training,
  or Context option B);
- the receipts, with SHA-256.

Until then this report does not say whether Context chose well, badly or not
at all: it says only that Compact's evidence differed from Focus's on four of
the nine rows that could be compared and that coverage did not change.

### 4.7 Judge agreement

| Pass | Results with two verdicts | Equal grades | Per-note verdicts that agree | Adjudications |
| --- | --- | --- | --- | --- |
| Change set 1, airk | 27 | 27 | 92.43% of 185 pairs | None needed |
| Baseline, airk | 71 | 71 | 93.02% of 487 pairs | None needed |

As at the baseline, grade agreement tests little when every Answer grade is 0.
About one note verdict in thirteen differs between the judges without changing
a grade.

## 5. What was good

- **The first call covers more.** 4 of 53 essential notes on all three Answer
  configurations, twice the baseline and twice Search's first call. The gain
  is C1 (1 of 5) and a second C2 note.
- **No gathering stops on one failed read.** At the baseline the no-model first
  reply ended `refused` on A1, A2 and W2. In this pass the same office-document
  refusal ("range crosses document locations") appears in the A1 and W2
  no-model first replies, and the gathering went on to its budget. No first
  reply in the pass ended `refused`.
- **The Answer call itself now covers more over a session.** 18 no-model Answer
  calls newly covered 8 notes (baseline: 16 calls, 4 notes); 13 Focus Answer
  calls covered 9.
- **Compact and Large finally differ from Focus** on some rows, which the
  baseline could not show.
- **Rubric B rose slightly** on every Answer path (no model 1.312 against
  1.250; Large 1.125 against 0.562), still below Search.
- **The pass was clean** apart from the guard stop of attempt 1: one
  publication token throughout, no failed Answer call, every judge verdict
  valid, no adjudication pending.
- **Reuse saved 25 sessions** on evidence equality, including all 20 comparator
  results.
- **The replays isolated the layers** in about an hour of no-agent replay
  (05:16 to 06:18 UTC), which is what showed that only the pool layer carried
  first-call coverage.

## 6. What was bad

- **No new correct finish.** Zero for every Answer configuration, as at the
  baseline. Answer is still worse than Search on C1 and T1 and nowhere better.
- **Calls got slower without a gain.** The no-model Answer call is 3.5 s slower
  on average, Focus 4.6 s, Compact 8.6 s and Large 15.9 s. Mean tool time per
  session rose from 20.3 to 30.0 s with no model and from 40.7 to 79.2 s with
  Compact. Two Compact Answer calls took 146.6 s (C3) and 123.6 s (A3), over
  the request's 120 s deadline; the C3 Compact digest replay timed out at
  120,338 ms. The device budget (change 8) did not keep CPU inside 120 s.
- **T1 with no model had all the evidence and still failed.** Its session
  covered all 5 essential notes, the last at call 11 of 12; its first Answer
  call already held one of them, the existing full-scan test. Both judges
  found the brief right on 3 notes and missing 2: it did not describe the
  direct-path check for selected files, and it stated that no existing test
  covered credential exclusion, when the first reply had shown one. Even with
  a correct brief it would have finished at 11, after Search's 7. This is the
  case [rubrics.md](rubrics.md) counts as no correct finish.
- **Seven of the eight changes have no measured first-call effect.** Follow-ups,
  spread, presentation, the device budget, the Focus instruction and the
  provisional Context rule each left first-call coverage at 3 of 53 when
  removed; Focus against no model was 3 against 3. Leaving out follow-ups made
  the no-model call slower, not faster. The replays measure the first call
  only, so an effect on later calls is not ruled out, and the pass, with every
  layer on, cannot attribute one.
- **The agent still spends turns on lookups that cannot count.** In the
  no-model sessions 28 `symbols`, 29 `references` and 5 `outline` calls covered
  no note; in Focus sessions 23, 29 and 4. Answer now queues symbols and
  references itself, and the agent repeats them.
- **Focus candidate 1 and the Context rule showed nothing.** The trained Focus
  candidate covered the same notes as the base weights; Compact's coverage was
  the same with and without the provisional rule.
- **Runs invalidated or repeated along the way:**
  - *Pass attempt 1* stopped at 06:30:06 UTC, at the start of Compact. The
    start guard's status and search calls got `scoped_connection_unavailable`
    while a Context stage was warming, and the guard compared that missing
    token as a change (marker `INVALID.attempt-1-guard-none-token.json`). The
    harness was fixed (the guard retries an unsettled start and never compares
    a missing token) and re-frozen at 06:33:26 UTC. Attempt 2 kept the
    completed no-model and Focus sessions, whose start and end snapshots were
    settled with the same token.
  - *Replay L23* (tuned token set): 9 of 10 no-model calls failed with CLI exit
    8 or 4 and output that was not JSON, and the Focus stage then failed to
    enable. It was restarted as L24, whose results are above. The cause was not
    established.
  - *The first Focus candidate and publisher-instruction replays* failed to
    enable their stage: the change-set image refuses a direct switch from one
    Focus profile to another (`arbiter_unavailable`). The harness now disables
    the live profile first (freeze of 05:29:29 UTC), and both were rerun.
  - *The change-set-1 bookbuilder no-model replay* ran on truncated search
    before the roster fix and is invalid ([01-baseline.md](01-baseline.md)
    section 6).

### 6.1 Bugs found and fixed during the series, and what the build costs

Items 1, 2 and 5 are bugs fixed before the pass. Items 3 and 4 are recorded
digest-input changes the build carries.

Five images were built for the series. Each fix below is in the image that
ran the replays and the pass (`cs1e-1a24414d`).

| Image | Built (UTC) | What it added |
| --- | --- | --- |
| `cs1-f5d29b41` | 21 Nov 17:31 to 17:42 | The eight changes |
| `cs1b-8117a9c5` | 22 Nov 00:23 to 00:28 | Focus candidate admission, the indexer fixes |
| `cs1c-a4fcb934` | 02:50 to 02:53 | The catalog-only warm gate and the whole-chunk fix (change-set-2 code present behind its switches) |
| `cs1d-d316beaf` | 03:37 to 03:39 | The search roster fix |
| `cs1e-1a24414d` | 04:09 to 04:14 | The tuning tokens |

1. **Catalog-only extras never warmed.** The evaluation's extra profiles (the
   16-wide Focus profiles, Focus candidate 1, the provisional Compact and
   Large) are registered in the evaluation catalog without a store receipt,
   read in place from authenticated read-only files. The stage lifecycle
   counted only a store receipt as provisioned, so such a stage never warmed.
   The symptom on `cs1b`: the bookbuilder replay's Focus enable waited 900 s
   and failed ("focus warm completion did not arrive within 900 s", 02:30
   UTC).
   **Fix:** three availability hunks in
   `crates/airk-service/src/service_runtime/arbiter/lifecycle.rs`, authorized
   by the user on 22 November, treat exactly a held registration as
   provisioned: Status projects `Requested`, the cached-intent gate counts the
   assets ready, and the restore path validates enrollments and launches.
   Configure, scope, enrollment, hardware admission, warm and the deadlines
   still govern Ready; no receipt, persisted format or wire field is added.
   **Proof:** the warm smoke on `cs1c` (L3, 413.07 s, pass) warmed all four
   extras through native warm admission between 02:56:01 and 02:59:24 UTC: the
   16-wide Focus, Focus candidate 1, provisional Compact and provisional Large.
   Its first two Context enables were refused with "variant change requires
   explicit resident drain" and later enables reached ready. The service tests
   for it (`catalog_only` and `restart::change_set`) pass: 12 passed, 3
   ignored.
2. **Whole-chunk reads repeated the snippet.** With the larger first search, a
   pool hit cites only its focused snippet, often 1 to 12 lines. The
   whole-chunk read used that range, so it returned the snippet again. **Fix
   (`cs1c`):** the read now covers at least 40 lines (`WHOLE_CHUNK_LINES`) past
   the snippet's first line, at most 100
   (`crates/airk-kernel/src/arbiter/software/plan.rs`), with a test that a
   30-line citation still reads 40 lines.
3. **The `runtime.rs` digest-input change and its 8 failing tests.** Running
   `SoftwareEvidenceV1` (change 5) needs a routing in
   `crates/airk-retrieval/src/arbiter/runtime.rs`, under
   `cfg(any(test, feature = "test-support"))` only, for a profile whose
   FocusRank policy is exactly the software policy. `runtime.rs` is a digest
   input: the legacy CPU Focus source transition pins its source, and the CUDA
   FP16 math-policy digest includes it. The routing moves its LF SHA-256 from
   `7b465076…f2449b3` to `258d4371…3bb698e0`. It is kept as a recorded
   digest-input change of the private build, not a re-pin, so the 8
   `cpu_source_transition` tests fail there as a known failing check (owner:
   the primary agent); the CUDA FP16 math-policy digest changes with it (the
   evaluation's CUDA cells are FP32). Before any of change set 1 reaches the
   product, a recorded decision must remove the routing or add a reviewed
   source row.
4. **The bounded-hybrid option-A digest change.** The CUDA bounded-hybrid gate
   binds a reviewed graph result to an exact profile manifest, so a
   calibration-only Context variant (the provisional Read rule) was refused on
   CUDA. Option A: `bounded_hybrid.rs`, a digest input, gated by test-support,
   admits a variant equal to the reviewed CUDA profile except the Calibration
   asset, calibration field, profile id and manifest, and verifies graph and
   placement against the reviewed base. Its validator identity
   `airk.private.cuda.bounded-hybrid-validator/2` changes with the source (LF
   SHA-256 `34110d17…1631e61781` at HEAD, `4b6c33f9…6601566d8e` in the working
   tree). It needs a recorded decision before any product use.
5. **Test-support items in product builds.** Clippy with `-D warnings`
   refused items that only the evaluation uses but that compiled into builds
   where nothing called them: `register` and `authenticated` in
   `catalog_only.rs` (service lib) and the `AIRK_ARBITER_EVALUATION_SEARCH`
   constant in the no-default-features kernel build (a change-set-2 item that
   was in the same tree). They were gated: `register` is now `cfg(test)` and
   `catalog_only.rs` compiles only with local-inference and test-support, and
   the constant is now `cfg(feature = "test-support")`. The service Clippy
   rerun passed. For the kernel no-default-features build, the last retained
   Clippy log (`cs1c-kernnd2.log`, 02:50 UTC) still shows the constant
   refused; no passing rerun after the gate is in the receipts.

## 7. Why

From the turn logs (`coverage.json` per session), the first replies and the
replays.

- **Retrieval is still the bottleneck.** The baseline's depth measurement put
  35 of 53 essential notes in files that never enter a 100-hit pool, and only
  three notes inside the first 16 hits. A larger first search can reach those
  three, and that is the whole first-call gain: the pool layer is the only one
  whose removal changed coverage, and `pool_full=8` added one note by fetching
  the top hits whole.
- **Follow-ups look up names, not the notes.** The new follow-ups run from the
  kept hits and the identifiers in the instruction and hits. When the kept
  hits are prose or experiment files, their definitions, callers and governing
  files are not where the notes are. The replays show no first-call note from
  follow-ups.
- **Focus has little to choose from.** With at most six pairs per Answer on CPU
  and three notes in the pool, Focus picked differently from no model on every
  row and covered the same notes.
- **Context's choice is unmeasured.** Its evidence differed from Focus's on four
  rows and coverage did not move; section 4.6 decides whether there is anything
  for it to choose.
- **Slower calls come from more reads, the full-chunk search and the model
  stages.** The no-model first reply is about 54 KB, as before, but now takes
  13.2 s against 8.5 s; `pool_full=8` added 5.2 s in replay. On Compact the
  device gate admits a Context call from its measured cost and still let calls
  run past 120 s.
- **The agent repeats what Answer did.** After Answer, the Haiku agent spends
  most of its calls on `symbols` and `references`, which cannot cover a note,
  and covers notes with `read_file`, as at the baseline.
- **Evidence in hand is not enough.** T1 shows that a Haiku agent can miss a fact
  that is in its first reply when the reply is long; the judges' Rubric B notes
  for that reply say the useful parts were mixed with patches, README text and
  repeated documentation.

## 8. What happens next

**Change set 2: retrieval first, with `search_v2`.** The baseline depth result
and this series point the same way: Answer cannot gather what search does not
return. `search_v2` (described in [01-baseline.md](01-baseline.md) section 8)
applies to hybrid searches of 8 or more words: prose query rewriting into key
and named words, two ranked any-term lexical branches, 300 candidates per
branch before fusion, weighted fusion, a file-kind prior, per-file diversity
and snippets centred on key words. It changes Parse and Search too, so their
sessions run again, and both repositories are used from change set 2 on.

It is **pending its depth data**. No valid `search_v2` depth measurement is in
the receipts yet: the two change-set-2 depth runs so far are bookbuilder runs
on publication 267, both marked invalid for truncated search. Change set 2 is built and tuned once
a 100-hit depth run with `search_v2` on, on the settled airk index and on
`bookbuilder-v2`, shows how many of the 53 and 61 essential notes enter the
pool, against the baseline's 5 and 10. The bookbuilder-v2 baseline has run:
see section 4.6 of [01-baseline.md](01-baseline.md).

Also for change set 2, from this series:

- the Context direction from section 4.6: training Compact if the leverage
  measurement says go, Context option B (a wider roster that takes the best
  Read even when a terminal card wins, already built behind the `context_b`
  token) if it says no-go;
- the CPU time limit: Compact calls ran past 120 s, so the device gate needs
  its estimate checked against these calls;
- the change-set-1 layers stay on unless a replay on the new search shows they
  cost time with no effect; the replays here measured only the first call on
  the old search.

## 9. Limits and receipts

**Limits.**

- airk only, ten scenarios, one session per scenario and path. The rule needs
  both repositories and cannot be applied.
- CPU only. No GPU cell ran (section 4.5).
- The replays measure the first Answer call only. Effects on later calls and on
  the agent are measured only by the pass, with all layers on together.
- The no-model and Focus sessions ran under prompt freeze `8096d3c6…` and the
  Compact and Large sessions under `3cf8dedc…`; both differ from the baseline's
  `b4044529…` in harness files only, and the prompt texts and judge schema are
  unchanged.
- All model times are for this one CPU host.
- Coverage is the mechanical rule 2 of [rubrics.md](rubrics.md); symbol and
  outline rows never count.
- Grade agreement is a weak test with every Answer grade 0.
- The image is the uncommitted working tree at 04:08 UTC; its diff digest
  changed during the build because two `#[cfg(test)]` test modules of other
  crates were edited, which the service image does not compile
  (`images.json`). This report's commit carries that code.
- Everything ran on one Windows host. Nothing here is cross-platform evidence.

**Receipts.** Every path below is under the git-ignored campaign folder
`artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1/`, which is not
committed.

| Receipt | SHA-256 |
| --- | --- |
| `passes/cs1-airk/results.csv` | `7472cfa05dea9f9fa545c6f53a57fd5d95be4cf14c02b8a0c45dd0eaf222da05` |
| `passes/cs1-airk/baseline-summary.json` | `7551fe87fb4a8313dcd2fc428a7b43706469698c28f1f1c5dd93c60ca36bb7c5` |
| `passes/cs1-airk/summary.json` | `9f6b7eaa5315135a1425f78e725c0f52031c6c61d689e7a70ca505546fbabea1` |
| `passes/cs1-airk/compare-vs-baseline.json` | `04d9de22ecaaa04df51e50d76e838d081ec0f428b284074c2b240e5abf0168c7` |
| `passes/cs1-airk/pass-report.json` | `8b35f75eb53751574688126603fa35f869a7d721d83fa5c06ecad55e4d8293f4` |
| `passes/cs1-airk/pass-index.json` | `1d44de91e6c89e179df21153ea672faf8d63ed17c080239e176c23287c56c260` |
| `passes/cs1-airk/digests.json` | `9e9ea16cd471dca98ba9a01b992244e57e481426d97deeb243d214874bbcdf63` |
| `passes/cs1-airk/digest-failures.jsonl` | `50ec7b2a39c1ee911cc39e28183eb2dd9a7c2cb0ea7321d5390e3b8e463e948e` |
| `passes/cs1-airk/reuse.json` | `89b26f16d42623fdaa0f08b9f97665f3db65983a1f675db6c610ecf299c2a20a` |
| `passes/cs1-airk/spec.json` | `3daee5fb603518f567a98dba92094179a9f62bd6825e130e1127846bada6fc3f` |
| `passes/cs1-airk/settled.jsonl` | `55d2b7d9ec33e02e880101daf613612d8f43f2e9288c1e4f9831a728158f9b9b` |
| `passes/cs1-airk/settled-start.json` | `8c0bc6d7d1a381f358a9240ca17040408fddf080beffd8437dc2c6c504e5c4d9` |
| `passes/cs1-airk/baseline-run.jsonl` | `d56bf048c3c1930916fa5e4eb0555293d2fda779effcca981f98b10aadad5fca` |
| `passes/cs1-airk/pass-log.jsonl` | `26ad70937c2e1539b82dabb20b9824b763a9dbbd5048f2c187129d883c8d4588` |
| `passes/cs1-airk/INVALID.attempt-1-guard-none-token.json` | `170a7a21af0d626429b35a8680d21587465d5488c91fd22022de2a133e24517d` |
| `change-set-1/replays/airk/replays-summary.jsonl` | `37501555b97058de1a637286e4d8df768cdf8f2cf9153affc245b619da40f410` |
| `change-set-1/replays/airk/loo-run.jsonl` | `ae564e490e27ecb10e3227506e7a808e28f7fce0de56b5e10f265d81038a6da3` |
| `change-set-1/replays/airk/stages-run.jsonl` | `6c5469261efb268ec5446a5c0ad5f9daeb1b27ba05563de1ef3e043c964e4a69` |
| `change-set-1/replays/airk-all.log` | `e913585a59012212d86c0da650b2ec0bcfbff637df688ccdd47fb376fada7382` |
| `change-set-1/replays/airk-loo.log` | `76f87b2417c3410ed08e8e96a1ef0eab7c6cabed263135110a90a7c65e49dbcc` |
| `change-set-1/replays/airk-loo-2.log` | `6a3f618ddd7bc0b63a3bc85cce25f10561043e079cf43fee41603b003bc4107d` |
| `change-set-1/replays/airk/all-noModel/cs-replay.json` | `3e069c396d61fb71ae1fcc880a527896220420e2a3722595ca2ee01451c45fde` |
| `change-set-1/replays/airk/all-focus-w16/cs-replay.json` | `654d9926d52bf78cff6fb99368de949b892878620b3db458d80a65b4d7543efd` |
| `change-set-1/replays/airk/all-focus-cand1/cs-replay.json` | `f707c61d3bbeaa74028d1785b5e6b2368cb771d4cb2c8688ccd957d055667d85` |
| `change-set-1/replays/airk/all-compact/cs-replay.json` | `1e7575eda05d40c92a66838f029660d9d0b6767880384aaa72b52a57b75ee893` |
| `change-set-1/replays/airk/all-large/cs-replay.json` | `c85ae060a0f1201a648a64a00357534ec52ce80102e0e27ee056899d87cea364` |
| `change-set-1/replays/airk/loo-focus-publisher/cs-replay.json` | `4ab073ab5fe79721666a262be5aa93f3f29ddc4c08c34441176b9b4942ef7534` |
| `change-set-1/replays/airk/loo-compact-original-context/cs-replay.json` | `a8773ebc990e2077e5b39e064aa9c79a0d01b8c4c5c313468bcb572c19aa22f9` |
| `change-set-1/replays/airk/loo-allpool_full=8-noModel/cs-replay.json` | `56cb507c5e0c786c0d8ab6b41b9b2d27cc26af683899ab23cafc82ebd4693c5f` |
| `change-set-1/replays/airk/loo-allpool_full=8-focus/cs-replay.json` | `f33ce8c5ef6af7edf1afa4ba5a2e554f0abbcc4c2491217ebe67cc4436a2fbcb` |
| `change-set-1/replays/airk/loo-allpool_full=8-compact/cs-replay.json` | `c9b92f9c5dc5dc7f722014c2f7bfef11a6a087ae0f6df4eae79dfc812f000980` |
| `change-set-1/replays/airk/loo-allpool=8pool_full=8read_floor=3072kept=3subjects=strictgoverning=project-noModel/cs-replay.json` | `bc2ef78edffbd365403db6a7bfbebe8070a5ccc63888c0b8368b27a5b763c975` |
| `…-focus/cs-replay.json` (same variant) | `03a16d9c5e7ae1a3b115a49b69c4ba7e0ba692984d371017fac015b4b3f06551` |
| `…-compact/cs-replay.json` (same variant) | `756b2e827b470bb58a5051d12accb3b066dde83de9d201b998fb2460e725ca49` |
| `change-set-1/replays/airk/loo-allminus-followups-noModel/cs-replay.json` | `38d9702ec28594d038b94f61848240f4f2741e773ba64caa51c6b311f27ec7b9` |
| `…-followups-focus` | `cb134499876b579e40c2fa1d96c151c69e30f1f96f4533e0225cf6c3fd351c2a` |
| `…-followups-compact` | `f8de888c1edfaf380d6c9d408a88a8ec6dd85710ee32e6dd76df7726c485ecca` |
| `…-spread-noModel` | `07d415e66fe30810d37963d3953aab1d96e5f150530fcb4f8f135e4e2885471a` |
| `…-spread-focus` | `e93564a6ec38703563c6a06d14113bc758c80fb1bea5e069d60caed293b4886f` |
| `…-spread-compact` | `d67ed6bc0acfaae8d5d2f6ba54113e8a65aa33e7d53ccb67af1ac62a475ee858` |
| `…-pool-noModel` | `7ecbd44d1a2f4c5ad8d34558e7a37a60029d574589d6997549790ff63d06e1a2` |
| `…-pool-focus` | `4e5a206303f65dafe45714cb0d6fcea9da2a41be1020a0c693912884b7e6b03c` |
| `…-pool-compact` | `a629ab5ed4c0cb751020ce1dfb10ffc7f3cc16775a46ed4b21818b5e60631834` |
| `…-present-noModel` | `6f5ad1761c5a4dc9d926d308f2c288d4eb315446fba46774fda2bb9fceb6ed7a` |
| `…-present-focus` | `8243fe056eca79ded13719c95a8a330b668733e74739653e3c024f5306d30bab` |
| `…-present-compact` | `bf0c75302a758d4841e658100a8bbf61145948ad8e8664c88994e5f353cf5e3a` |
| `…-device-noModel` | `2ed3cf2675d85b728a6e6e0de3a03f2921d82f03541c5a4b55fc6d21b56f41f4` |
| `…-device-focus` | `23219d85f71f179e4ac6814ae8727f492eff097ec7a82aa22648937839f0a162` |
| `…-device-compact` | `6768c6ada69292a32f5597da859d47058fb3527d2499fb80c512a61026b75d12` |
| `change-set-1/smoke/L1.log` (fixture smoke, image `cs1`, 76.50 s, pass) | `0a88cbb90333d173586b32a1958e2dd6cc6ea597d54d825c78830db7d1044200` |
| `change-set-1/smoke/L2-candidate.log` (image `cs1b`, 76.17 s, pass) | `d7528df43491bf16e0fbdcea567865f5e7107149746d678218a67460c8f921c8` |
| `change-set-1/smoke/L3.log` (warm proof, image `cs1c`, 413.07 s, pass) | `c9d6d3ce4bc9872d8c36c2d07e35eed285e06740f00da3fabbb0d99d57d3096e` |
| `change-set-1/smoke/L3/warm-completions.jsonl` | `7e9de5d5661f2cb50726de4f4e8a1d6a115f33968eecb3a9947b34c411da23b4` |
| `change-set-1/smoke/L3/change-set.json` | `6437e2e9c98b4dd8383dc48ca22d8675b9f6f05dd9b10c9aad31e059b5ad9a48` |
| `change-set-1/replays/bookbuilder/steps/021539-stage-on-focus-static1024-cpu-cs1-software-w16-a1.log` (the 900 s warm failure) | `d21500191b2837147cf53a4e6817a04958cc1bbedc3f710579455ec17896baa8` |
| `change-set-1/build/clippy-service-1.stderr` | `3ad163b3e465c14732969315547e55f8c236ffa495321bd770bbc39b6e870f06` |
| `change-set-1/build/cs1c-kernnd2.log` | `ca8da81bbe8b05969b0be1b358c0316dc904dda6d495cad73426815fd8c7eb41` |
| `change-set-1/build/cs1g-tests.log` | `8e4153e1ec3f5b6c7fae3570616b122a6894be5c23fb10a5b5ae3aeb55e740fe` |
| `change-set-1/profiles/change-set-inputs-r2.json` | `a0f26d834ec3fd322129e8daf28459133043f4690d17ec12d2b91afdf7094c59` |
| `change-set-1/profiles/private-focus-static1024-cpu-cs1-software-w16.json` | `244352af823c94aabfcfbe11822efd7e282e122e55fed3625ad4faf8411d4f36` |
| `change-set-1/profiles/private-focus-static1024-cpu-cs1-publisher-w16.json` | `dc579e617d92516cd8d4b44534b73a60eea691b7f263f8c88978ccfee1970ca0` |
| `change-set-1/profiles/contextqwencompact-provisional-read-calibration.json` | `7dbcee510ecda34f1c08b694fd7cece0233eafa501ba77561989953dab5b7643` |
| `change-set-1/profiles/contextqwenlarge-provisional-read-calibration.json` | `c9632366e8c5731cea62ee24b610f8c011c80af6493741a3ac5b76e247e0dcd3` |
| `fixture/images/cs1-f5d29b41/images.json` | `016a44a633185d1899f780c2641c1fdbc97e46a209262fe1c8c238160e1e139c` |
| `fixture/images/cs1b-8117a9c5/images.json` | `a2232ccd264b1c86b948c0c58efba9dd8a4e3d298174c7c0d76c06bde95586db` |
| `fixture/images/cs1c-a4fcb934/images.json` | `684b18e98bba024b953f36b46e83e5453501275e13625d9f7b731f0d5d65c14a` |
| `fixture/images/cs1d-d316beaf/images.json` | `78da9eddc68157424cdef1d4ba2f04b72384d3345b93b176e676f46ddb648259` |
| `fixture/images/cs1e-1a24414d/images.json` | `c11d860f78ff4428ae738c1893f2ba22f0e71cf3982739dff7f617c75e046d1f` |
| `fixture/images/cs1e-cuda-f79755a7/images.json` | `a37309063c38b5e7a7405a1d666691da388b29e523d1d9dd7dec5048265985f5` |
| `design/change-set-1.md` | `86ee9a4992da0e7f2cffda5276859b085fd1921dda23724afe4d07cf45190069` |
| `design/change-set-1-decisions.md` | `afedb9f009606c12fda2e1846fcce3bea10bf2f3a2c869694d0416361ee43ebe` |
| `design/cs1-tuning-tokens.md` | `e340433f2d91b41248796dd290a3a934b2d9f60dcb06e9e274342b355a343b93` |
| `design/context-trainer.md` | `b0dc0d0d2523c657b2d5db98cc45beb7259343bdf01f954012cfa470718cd76d` |
| `design/search-v2.md` (as of this report) | `c158de57b96ed16c67322b94c4beb3beb04a913b9f5a07183732b05d18124ddd` |
| `design/prompts-freeze.json` (06:33:26 UTC) | `3cf8dedcdef43609f6890f87852a30ecb80b8e17ac16c3d53fff2853719ce88b` |
| `design/prompts-freeze-history.jsonl` | `22eeddf6d10778530cc0ee374dbdb86f6253aeae3fabdbc2ac2763f85dbbb115` |
| `scenario-freeze.json` | `0a7d7b0a478b6e9d7cdeca9bc363452a40e6afc8ae4db5f2af557856d53e7806` |
| `fixture/L31-cs1e/handle.json`, as the pass recorded it | `bade511d301f903ce1c175b4c27b1be78910a950cc14b72abc12b4b331abba3b` |

The lifetime handle has been rewritten since the pass; its current SHA-256 is
`a1aeed3b67523b7db34483891f145587c6f78d56b3f3be2152f22962ece8f17d`.

**The results file.** [results/02-change-set-1.csv](results/02-change-set-1.csv)
holds the harness's fixed columns for the 52 airk results (series
`cs1-airk`), copied from the pass's `results.csv` unchanged, with the same
columns as [results/01-baseline.csv](results/01-baseline.csv). Reused rows
carry their source's grades, counts and timings, a `reused_from` value and an
agent cost of 0. Its `elapsed_ms` column is tool time; wall time is in
`pass-report.json`.

**Commands that reproduce it.** Python runs as `python -I -B` from
`D:\code\Github\airk`, as the verified human. `R` is the campaign folder above,
and the service lifetime is started with `fixture.py` as in
[00-setup.md](00-setup.md) section 9, with the layers in
`AIRK_ARBITER_EVALUATION_ANSWER_LAYERS`.

The pass, attempt 2 (880.4 s, exit 0; attempt 1 was the same command, 576.3 s,
exit 1):

```text
python -I -B scripts/arbiter-answer-eval/run_baseline.py --spec R/passes/cs1-airk/spec.json --interval 30
```

It ran these steps, logged with their exit codes and times in
`passes/cs1-airk/baseline-run.jsonl`: Search, Parse and no model with every
stage off (233.7 s in attempt 1); Focus on (310.6 s in attempt 1); Compact on
(370.8 s); Large on (274.6 s); coverage, judging (142.5 s), grading and
`pass_report.py --pass-dir R/passes/cs1-airk`.

The replays ran with `scripts/arbiter-answer-eval/run_cs_replays.py` (series
`baseline`, repository airk, the change-set budget) on lifetime `L22-cs1e`,
05:16:40 to 05:33:34 UTC, and the leave-one-outs with
`scripts/arbiter-answer-eval/cs_loo.py` (one restart per variant), 05:34:22 to
06:18:31 UTC, with the variants
`all,pool=8,pool_full=8,read_floor=3072,kept=3,subjects=strict,governing=project;all,pool_full=8;all,-followups;all,-spread;all,-pool;all,-present;all,-device`.
Their steps and exit codes are in `replays/airk/stages-run.jsonl` and
`replays/airk/loo-run.jsonl`.
