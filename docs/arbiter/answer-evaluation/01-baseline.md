# 01 Baseline: Answer as it stood at commit `1fe7b600`, on airk

Written on 22 November 2026 (UTC) from the private receipts of the airk
baseline pass (22 November, 04:25 to 05:02 UTC), of the earlier bookbuilder
history pass and of the bookbuilder-v2 baseline pass (07:36 to 08:07 UTC).
It goes into one commit with [00-setup.md](00-setup.md),
[scenarios.md](scenarios.md), [rubrics.md](rubrics.md), the harness and
[results/01-baseline.csv](results/01-baseline.csv).

> **Headroom: 10 of 10 airk scenarios and 10 of 10 bookbuilder-v2
> scenarios.** On every airk scenario the better of Parse and Search needs
> three or more calls or never finishes correctly, so a substantial win is
> possible on all ten. On bookbuilder-v2 no path finished any scenario, so
> a win is possible on all ten there too: 20 of 20. Round 1 is airk only, by
> the user's direction, so the full rule (12 of 20 scenarios, at least 5
> wins in each repository) cannot be applied to it. The bookbuilder-v2
> baseline (section 4.6) is the bookbuilder comparison base for the later
> change sets; the bookbuilder history pass of section 4.5 also has
> headroom on 10 of 10.

## 1. Summary

Nothing was changed. The baseline measured Answer, Parse and Search, as they
stood at commit `1fe7b600`, on airk's ten development scenarios with Haiku
agents and two Sonnet judges per result. The test image carries only the
evaluation-setup fixes of the setup report.

No Answer configuration reached a correct finish. Search reached two: airk
C1 after 9 calls and airk T1 after 7, both graded 1. Parse and every Answer
configuration reached none. The floor is therefore near zero, and Answer
starts below it: it is worse than the better of Parse and Search on C1 and
T1, and ties at grade 0 on the other eight. Over the session, Answer's
gathering plus the agent's follow-ups covered fewer essential facts than
Search did (12 of 53 for Answer with no model, 22 of 53 for Search). On the
first call alone they tied: 2 of 53 each. Parse's first call returns no
evidence at all.

Focus added 16.3 s of tool time per session and no correct finish. Compact
and Large are timing only, because Context cannot act at the baseline. A
Sonnet reference arm did no better than Haiku. The two judges gave equal
grades on all 71 judged sessions.

A no-agent depth measurement explains most of it. A 100-hit search of each
instruction puts the region of 5 of 53 essential notes in its pool. For 35
of them the file is not in the pool at all. Lexical search adds nothing for
a prose instruction, because it requires every word to match, and the
vector side favours prose and experiment files over the code the notes cite.

The bookbuilder-v2 baseline (section 4.6) ran the same paths on the fresh
index with the same agent and judge models. No path reached a correct finish
on any of its ten scenarios, so its headroom is also 10 of 10. All 44 judged
sessions graded 0; the two judges gave equal grades on 44 of 44 and agreed
on 95.8% of the 334 note verdicts. Over the session Search covered 26 of 61
essential notes, no model 15, Parse 14, Focus 12 and Compact 10. Its depth
measurement, on the fresh index at publication 268 with the roster fix,
equals the original index's at publication 265: 10 of 61 notes covered
under rule 2, 13 under rule 1 and 24 by path, with the same hit count and
ranks on all ten scenarios. That confirms the roster fix on bookbuilder. The
pass cost $7.8674.

## 2. What changed in this series

Nothing in Answer, Parse or Search. The baseline image
`baseline-1fe7b600-o2-fix3` is commit `1fe7b600` plus the evaluation-setup
changes described in [00-setup.md](00-setup.md) section 2: test support for
an external read-only source root, the indexer speed fixes (patch r3, which
change how fast vectors are built and not what is stored) and the search
roster fix (section 6.1 below).

## 3. What was tested

| Item | Value |
| --- | --- |
| Scenarios | The ten airk development scenarios of [scenarios.md](scenarios.md): C1 to C3 coding, W1 and W2 writing, T1 and T2 test, A1 to A3 architecture. 53 essential notes |
| Paths | Search; Parse; Answer with no model; Answer with Focus (CPU); Answer with Focus and Compact (CPU, timing only); Answer with Focus and Large (CPU, A1 and C3 only, timing only, 120 s product cap, 180 s client timeout) |
| Reference arm | Sonnet agents on Answer with no model, Answer with Focus and Search (the comparator path, section 4.4) |
| Agents | `claude-haiku-5-5`, `--effort medium`, Claude Code 2.1.330, 12 turns at most |
| Judges | Two independent `claude-sonnet-5-5` sessions per result, `--effort medium` |
| Focus profile | `focus-static1024-cpu` |
| Context profiles | `context-compact-cpu`, `context-large-cpu`, each with a calibration of zero rules |
| Image | `baseline-1fe7b600-o2-fix3` (service `ada54197…915c988`) |
| Lifetime | `L21-fix3` |
| Index | `idx_ac4e36f1c8b265719b0ae05d28894fb0`, publication generation 330, token `airk-r1:d9feb47f…:330:5d1a2e84…f515dbc9da33` |
| Settled guard | 12 snapshots. The first, from a refused attempt, had no token; the other 11 report semantic state ready, served mode hybrid, no index or vectors job, and the same token from 04:25:12 to 04:57:31 UTC. The depth run saw the same token at its start and end |
| Prompt freeze | `design/prompts-freeze.json`, SHA-256 `b4044529…f9f59`, frozen 04:25:47 UTC on 22 November. The prompt texts and the judge schema are the ones listed in [rubrics.md](rubrics.md). Its harness hashes are in [rubrics.md](rubrics.md) and section 9 |
| Scenario freeze | `scenario-freeze.json`, SHA-256 `0a7d7b0a…e7806`, unchanged |
| Results | 82: 71 sessions run and judged, 11 reused (section 4.4) |
| Second sessions | None. There was no close call, because no Answer result finished correctly |
| Cost | Agents $5.904, judges $10.8265, total $16.7305 |

The pass ran in five session steps between 04:25:56 and 04:57:31 UTC, with
coverage, judging and grading after each group. The depth measurement ran
from 05:00:07 to 05:01:58 UTC with every model stage off.

## 4. Results

**Parse's first call has a structural zero yield.** With a prose
instruction as its text, Parse prepares a plan that needs an identifier and
returns no evidence. In this pass that first reply was 1,840 to 1,847 bytes
on every scenario, and its evidence digest was the same for all ten
(`23e3386d…`). Every Parse result below starts from that empty first call.

### 4.1 Against Parse and Search

Each cell gives the essential notes covered by the end of the session, the
number of calls, and for a correct finish the turn at which the evidence
became complete and the adjudicated grade. Every other result graded 0.
"(r)" marks a reused result.

| Scenario | Search | Parse | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- | --- | --- |
| C1 coding | 5/5, 11 calls, **finish at 9, grade 1** | 0/5, 5 | 3/5, 7 | 1/5, 7 | 1/5, 7 (r) | – |
| C2 coding | 4/6, 10 | 2/6, 4 | 2/6, 8 | 2/6, 11 | 2/6, 11 (r) | – |
| C3 coding | 2/5, 10 | 2/5, 6 | 2/5, 7 | 3/5, 12 | 0/5, 8 | 0/5, 8 (r) |
| W1 writing | 0/5, 3 | 0/5, 3 | 0/5, 12 | 0/5, 4 | 0/5, 9 | – |
| W2 writing | 0/6, 7 | 1/6, 7 | 1/6, 11 | 0/6, 11 | 1/6, 11 (r) | – |
| T1 test | 5/5, 8 calls, **finish at 7, grade 1** | 2/5, 5 | 2/5, 10 | 1/5, 11 | 2/5, 12 | – |
| T2 test | 1/4, 6 | 0/4, 3 | 0/4, 6 | 2/4, 8 | 0/4, 11 | – |
| A1 architecture | 1/5, 7 | 0/5, 5 | 0/5, 9 | 0/5, 9 (r) | 0/5, 9 (r) | 0/5, 12 |
| A2 architecture | 1/6, 6 | 0/6, 8 | 2/6, 9 | 2/6, 9 (r) | 2/6, 9 (r) | – |
| A3 architecture | 3/6, 8 | 0/6, 8 | 0/6, 10 | 0/6, 10 | 0/6, 10 (r) | – |

Per path, over its rows. A reused row counts with its source's values.

| | Search | Parse | No model | Focus | Compact | Large (2 rows) |
| --- | --- | --- | --- | --- | --- | --- |
| Correct finishes | 2 | 0 | 0 | 0 | 0 | 0 |
| Essential notes covered | 22/53 | 7/53 | 12/53 | 11/53 | 8/53 | 0/10 |
| Covered by the first call | 2/53 | 0/53 | 2/53 | 2/53 | 2/53 | 0/10 |
| Mean calls per session | 7.6 | 5.4 | 8.9 | 9.2 | 9.7 | 10.0 |
| Mean bytes returned per session | 104,514 | 150,070 | 166,281 | 166,316 | 172,273 | 155,234 |
| Mean first reply | 22,644 B in 4.7 s | 1,843 B in 0.3 s | 51,326 B in 8.5 s | 51,369 B in 19.1 s | 51,047 B in 24.9 s | 45,430 B in 82.1 s |
| Mean tool time per session | 17.6 s | 18.9 s | 20.3 s | 36.7 s | 40.7 s | 112.1 s |
| Mean wall time per session | 40.1 s | 44.1 s | 44.2 s | 61.2 s | 65.7 s | 135.0 s |
| Agent cost of the sessions run | $0.1503 | $0.1987 | $0.1363 | $0.1031 | $0.0626 | $0.0221 |
| Rubric B, mean of both judges | 1.688 | 0.188 | 1.250 | 1.275 | 1.212 | 0.562 |

The two non-zero first-call yields are the same for Search and Answer: C2
(1 of 6) and T1 (1 of 5).

Bytes and time per scenario (bytes returned over the session, then tool
time and wall time):

| Scenario | Search | Parse | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | 118,635 B, 20.9 / 44.9 s | 102,388 B, 16.5 / 39.3 s | 124,477 B, 7.2 / 28.7 s | 139,217 B, 8.1 / 28.6 s | (r) | – |
| C2 | 152,272 B, 23.1 / 51.9 s | 139,068 B, 12.7 / 33.0 s | 162,621 B, 20.1 / 47.8 s | 229,433 B, 24.6 / 58.0 s | (r) | – |
| C3 | 113,997 B, 18.6 / 42.3 s | 191,264 B, 19.1 / 41.5 s | 142,474 B, 12.4 / 31.8 s | 183,035 B, 20.2 / 47.8 s | 159,361 B, 151.9 / 172.3 s | (r) |
| W1 | 34,978 B, 6.0 / 24.7 s | 48,996 B, 5.4 / 29.0 s | 189,701 B, 22.2 / 51.0 s | 140,427 B, 36.9 / 55.7 s | 153,153 B, 78.5 / 103.5 s | – |
| W2 | 100,741 B, 18.2 / 39.9 s | 212,336 B, 39.5 / 69.5 s | 238,902 B, 45.7 / 71.5 s | 149,441 B, 56.1 / 80.2 s | (r) | – |
| T1 | 104,508 B, 14.6 / 39.6 s | 158,126 B, 18.5 / 44.7 s | 212,087 B, 31.4 / 56.6 s | 180,507 B, 70.5 / 96.6 s | 165,115 B, 21.3 / 46.3 s | – |
| T2 | 99,490 B, 22.9 / 41.9 s | 84,826 B, 10.4 / 31.3 s | 125,828 B, 19.1 / 38.5 s | 171,360 B, 91.3 / 114.0 s | 167,809 B, 18.0 / 46.2 s | – |
| A1 | 108,389 B, 19.7 / 44.9 s | 123,609 B, 12.1 / 36.1 s | 124,636 B, 9.5 / 33.1 s | (r) | (r) | 151,106 B, 72.3 / 97.7 s |
| A2 | 93,474 B, 12.9 / 27.9 s | 271,426 B, 24.0 / 54.9 s | 149,200 B, 7.5 / 30.1 s | (r) | (r) | – |
| A3 | 118,661 B, 19.4 / 43.4 s | 168,658 B, 30.7 / 61.4 s | 192,884 B, 28.4 / 52.8 s | 195,902 B, 42.4 / 67.6 s | (r) | – |

Per-session agent cost is in the results file. Haiku sessions cost $0.0076
to $0.0679 each. The mean by path runs from $0.0129 (Focus) to $0.0199
(Parse), and the one Large session cost $0.0221.

**Per kind of task** (essential notes covered, then the mean adjudicated
grade):

| Kind | Notes | Search | Parse | No model | Focus | Compact |
| --- | --- | --- | --- | --- | --- | --- |
| Coding (C1 to C3) | 16 | 11, 0.333 | 4, 0 | 7, 0 | 6, 0 | 3, 0 |
| Writing (W1, W2) | 11 | 0, 0 | 1, 0 | 1, 0 | 0, 0 | 1, 0 |
| Test (T1, T2) | 9 | 6, 0.5 | 2, 0 | 2, 0 | 3, 0 | 2, 0 |
| Architecture (A1 to A3) | 17 | 5, 0 | 0, 0 | 2, 0 | 2, 0 | 2, 0 |

Every Answer configuration is worse on average than the better of Parse and
Search in coding and in test, and level in writing and architecture.

**Against the rule, per configuration** (airk only, so not a verdict):

| Configuration | Substantial wins | Ties at grade 0 | Worse |
| --- | --- | --- | --- |
| No model | 0 | 8 | 2 (C1, T1) |
| Focus | 0 | 8 | 2 (C1, T1) |
| Compact | 0 | 8 | 2 (C1, T1) |
| Large (A1, C3) | 0 | 2 | 0 |

Rubric B: every Answer configuration scores below Search (1.688) and far
above Parse (0.188), whose empty first reply has little to read.

**Per repository.** airk is above. The bookbuilder rows of section 4.5 are
history; the bookbuilder-v2 baseline is in section 4.6.

### 4.2 What each model adds

On the same ten rows, from the pass summary. Time is the mean tool time
added per session. "The better of Parse and Search" is chosen per scenario
by one rule, used for airk and bookbuilder alike in this report (the primary
agent's tie rule of 22 November): the path with a correct finish at fewer
turns; with no finish, the path that covered more essential notes over the
session, then the one with fewer session turns, then Search. On airk that is
Search on C1 and T1, which it finishes, and on C2, W1, T2, A1, A2 and A3,
and Parse on C3 and W2. The airk and r2 summaries of the harness were made
before the rule; their figures here are recomputed from the passes'
`results.csv` and `pass-report.json`.

| Step | Correct finishes added | Turns saved | Time added |
| --- | --- | --- | --- |
| No model against the better of Parse and Search | −2 (lost C1 and T1) | 0 | +0.5 s per session (5.3 s over 10 rows); +2.7 s against Search alone |
| Focus against no model (CPU) | 0 | 0 | +16.3 s per session (163.4 s over 10 rows) |
| Compact against Focus (CPU, timing only) | 0 | 0 | +4.0 s per session (40.5 s over 10 rows) |
| Large against Compact (CPU, A1 and C3, timing only) | 0 | 0 | +31.4 s per session (62.8 s over 2 rows) |

The Answer call itself, from the sessions that ran it:

| Configuration | Answer calls | Mean | Longest | Failed |
| --- | --- | --- | --- | --- |
| No model | 16 | 9.1 s | 17.0 s | 0 |
| Focus | 13 | 22.9 s | 47.9 s | 0 |
| Compact | 5 | 49.1 s | 97.7 s | 0 |
| Large | 1 | 66.4 s | 66.4 s | 0 |

In the no-agent digest replays, Compact's first call took 97,969 ms on W1
and 110,662 ms on C3, and Large's took 109,794 ms on C3 and 73,685 ms on A1.
All were under the 120 s cap.

- **No model** gathers no better than the comparators. Its first reply is
  more than twice the size of Search's and has the same first-call yield.
- **Focus** changed the evidence on 8 of 10 rows and added no essential
  fact. On A1 and A2 its first reply had the same evidence digest as no
  model, so those results were reused.
- **Compact and Large are timing only.** Context abstains under a
  calibration with no rules, so it cannot change which reads run. Compact's
  evidence equals Focus's or no model's on 6 of 10 rows. On C3, W1, T1 and T2
  it differs. That fits the known cause in [00-setup.md](00-setup.md)
  section 6: an enabled Context stage halves the token allowance Focus gets.
  Large's C3 evidence equals Compact's. On A1 it differs from Compact's,
  which there equals no model's (37,553 against 29,649 bytes in replay).
  None of these stages has earned its place at the baseline.

### 4.3 Reuse

A result is reused when its path's canonical first call has an evidence
digest identical to an earlier completed session with the same tools,
agent model and effort.

| Result | Reused from |
| --- | --- |
| A1 Focus, A1 Compact | A1 no model |
| A2 Focus, A2 Compact | A2 no model |
| W2 Compact | W2 no model |
| C1 Compact, C2 Compact | C1 Focus, C2 Focus |
| A3 Compact | A3 Focus |
| C3 Large | C3 Compact |
| Sonnet A1 Focus, Sonnet W2 Focus | Sonnet A1 no model, Sonnet W2 no model |

Eleven in all: nine in the Haiku arm and two in the Sonnet arm. The other
71 results were run as fresh sessions and judged. No session was reused
across agent models.

### 4.4 The Sonnet reference arm

The same instruction, the same first call and the same wrapper, with a
`claude-sonnet-5-5` agent at medium effort. For "the better of Parse and
Search" the reference arm ran Search on all ten scenarios (its first-call
digests equal Search's). The rule of section 4.2 would take Parse on C3 and
W2, so on those two rows the arm's comparator is Search, not the rule's
choice.

| Scenario | Sonnet, Search | Sonnet, no model | Sonnet, Focus | Haiku, Search | Haiku, no model | Haiku, Focus |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | 1/5, 3 | 1/5, 3 | 1/5, 3 | **5/5, finish at 9** | 3/5, 7 | 1/5, 7 |
| C2 | 3/6, 11 | 1/6, 9 | 1/6, 7 | 4/6, 10 | 2/6, 8 | 2/6, 11 |
| C3 | 1/5, 9 | 2/5, 9 | 2/5, 7 | 2/5, 10 | 2/5, 7 | 3/5, 12 |
| W1 | 0/5, 3 | 2/5, 4 | 0/5, 5 | 0/5, 3 | 0/5, 12 | 0/5, 4 |
| W2 | 0/6, 6 | 0/6, 7 | 0/6, 7 (r) | 0/6, 7 | 1/6, 11 | 0/6, 11 |
| T1 | 4/5, 8 | 4/5, 10 | 4/5, 10 | **5/5, finish at 7** | 2/5, 10 | 1/5, 11 |
| T2 | 0/4, 4 | 0/4, 7 | 0/4, 7 | 1/4, 6 | 0/4, 6 | 2/4, 8 |
| A1 | 0/5, 7 | 0/5, 8 | 0/5, 8 (r) | 1/5, 7 | 0/5, 9 | 0/5, 9 (r) |
| A2 | 1/6, 8 | 1/6, 6 | 1/6, 8 | 1/6, 6 | 2/6, 9 | 2/6, 9 (r) |
| A3 | 0/6, 5 | 2/6, 7 | 1/6, 6 | 3/6, 8 | 0/6, 10 | 0/6, 10 |

| | Sonnet, Search | Sonnet, no model | Sonnet, Focus |
| --- | --- | --- | --- |
| Correct finishes | 0 | 0 | 0 |
| Essential notes covered | 10/53 (Haiku 22) | 13/53 (Haiku 12) | 10/53 (Haiku 11) |
| Mean calls per session | 6.4 (Haiku 7.6) | 7.0 (Haiku 8.9) | 6.8 (Haiku 9.2) |
| Mean bytes per session | 87,652 | 136,937 | 133,425 |
| Mean agent cost per session run | $0.2154 (Haiku $0.0150) | $0.1871 (Haiku $0.0136) | $0.1508 (Haiku $0.0129) |
| Rubric B | 1.575 | 1.250 | 1.212 |

Sonnet reached no correct finish. It stopped sooner and covered less with
Search: C1 after 3 calls with 1 of 5 notes, where Haiku took 11 calls and
covered all 5. With Answer it covered about as much as Haiku. So the
conclusion drawn from Haiku, that Answer does not beat Search at the
baseline, holds for Sonnet, but on less evidence: neither Sonnet arm
finished at all. Sonnet cost 12 to 14 times as much per session.

The two arms' Focus first replies differed on two scenarios. On W2 the
Sonnet Focus reply had the same evidence as no model, while the Haiku Focus
reply differed and took 41.4 s in replay. On A2 it was the other way round:
the Haiku Focus reply equalled no model, while the Sonnet one differed and
took 25.9 s in replay. On the other eight scenarios the two arms' Focus
digests are equal. The same configuration and instruction did not always
give the same evidence; the cause is not established (section 6).

### 4.5 bookbuilder, as history

`passes/baseline-bookbuilder-r2` ran on the original bookbuilder index
(`idx_b2fa0bfd…`) at publication 265, before any deny, on image
`baseline-1fe7b600-o2-fix2` and lifetime `L14-fix2`. Its search covered the
whole corpus, so the roster defect did not affect it, and its token was
unchanged across all 8 settled snapshots. It is valid history. It is not the
comparison base for later change sets, which use `bookbuilder-v2`. It used
the same paths, models and judges, without the Sonnet arm. It had 52
results: 44 sessions run and 8 reused. Agents cost $1.0507 and judges
$6.0401.

| Scenario | Search | Parse | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- | --- | --- |
| C1 coding | 3/7, 12 | 5/7, 7 | 3/7, 10 | 3/7, 10 | 2/7, 9 | – |
| C2 coding | 0/5, 12 | 2/5, 8 | 2/5, 11 | 0/5, 10 | 0/5, 10 (r) | 2/5, 12 |
| C3 coding | 2/6, 6 | 2/6, 8 | 1/6, 9 | 1/6, 9 (r) | 1/6, 9 (r) | – |
| W1 writing | 1/5, 5 | 1/5, 4 | 1/5, 11 | 1/5, 10 | 2/5, 11 | – |
| W2 writing | 1/6, 10 | 1/6, 5 | 2/6, 12 | 3/6, 12 | 3/6, 12 (r) | – |
| T1 test | 4/6, 12 | 3/6, 11 | 0/6, 12 | 4/6, 12 | 2/6, 12 | – |
| T2 test | 2/6, 7 | 0/6, 6 | 0/6, 11 | 0/6, 11 | 2/6, 11 | – |
| A1 architecture | 0/8, 12 | 0/8, 4 | 1/8, 12 | 0/8, 12 | 0/8, 12 | 0/8, 12 (r) |
| A2 architecture | 1/6, 6 | 1/6, 9 | 0/6, 9 | 0/6, 9 (r) | 0/6, 9 (r) | – |
| A3 architecture | 3/6, 8 | 1/6, 3 | 2/6, 12 | 1/6, 12 | 1/6, 12 (r) | – |

| | Search | Parse | No model | Focus | Compact | Large (A1, C2) |
| --- | --- | --- | --- | --- | --- | --- |
| Correct finishes | 0 | 0 | 0 | 0 | 0 | 0 |
| Essential notes covered | 17/61 | 16/61 | 12/61 | 13/61 | 13/61 | 2/13 |
| Covered by the first call | 4/61 | 0/61 | 4/61 | 4/61 | 5/61 | 0/13 |
| Mean calls per session | 9.0 | 6.5 | 10.9 | 10.7 | 10.7 | 12.0 |
| Mean bytes per session | 113,800 | 192,732 | 200,738 | 192,848 | 200,511 | 182,056 |
| Mean tool / wall time | 17.2 / 40.3 s | 25.5 / 55.6 s | 30.6 / 59.7 s | 56.7 / 85.7 s | 62.5 / 92.2 s | 99.9 / 131.2 s |
| Rubric B | 1.562 | 0.000 | 1.450 | 1.462 | 1.400 | 1.125 |

By kind (essential notes covered): coding (18 notes) Search 5, Parse 9, no
model 6, Focus 4, Compact 3; writing (11) 2, 2, 3, 4, 5; test (12) 6, 3, 0,
4, 4; architecture (20) 4, 2, 3, 1, 1. Every grade is 0.

No path finished any bookbuilder scenario, so all ten are unsolved and all
ten have headroom. Model steps: no model added 11.9 s per session over the
better of Parse and Search, Focus 26.0 s over no model, Compact 5.8 s over
Focus and Large 9.5 s over Compact, with no correct finish added. The digest
replay of C2 on Large returned nothing after 120,784 ms (a timeout at the
cap). The C2 Large session's own first call returned in 89.7 s.

### 4.6 bookbuilder-v2 baseline

Run on 22 November, 07:36:20 to 08:07:27 UTC, as one driver run
(`run_baseline.py`, 1,866.6 s) on the fresh index. It is the bookbuilder
comparison base for later change sets. Search, Parse and Answer ran as for
airk, without the Sonnet arm.

| Item | Value |
| --- | --- |
| Scenarios | The ten bookbuilder development scenarios of [scenarios.md](scenarios.md): C1 to C3, W1, W2, T1, T2, A1 to A3. 61 essential notes (C1 7, C2 5, C3 6, W1 5, W2 6, T1 6, T2 6, A1 8, A2 6, A3 6) |
| Paths | Search; Parse; Answer with no model; Answer with Focus (CPU); Answer with Focus and Compact (CPU, timing only); Answer with Focus and Large (CPU, A1 and C2 only, timing only, 120 s product cap, 180 s client timeout) |
| Agents and judges | As in section 3: `claude-haiku-5-5` agents, two `claude-sonnet-5-5` judges per result, `--effort medium`, 12 turns at most |
| Image | `baseline-1fe7b600-o2-fix3` (service `ada54197…915c988`), the airk baseline's image, which has the roster fix |
| Lifetime | `L35-fix3` |
| Index | `bookbuilder-v2`, `idx_27adb04a6fee60a70005f1502e4b5e45`, served as bookbuilder through the index alias; publication generation 268, token `airk-r1:a8ee462e…:268:1b153149…705b048ba5a6` |
| Settled guard | A wait snapshot (07:36:24 UTC) and the 8 snapshots of `settled.jsonl`, the first of which is the start reference (07:36:27 to 08:02:57 UTC), all report semantic state ready, served mode hybrid, no unfinished job, no problem, and the same token. The depth run saw the same token at its start (08:06:40) and end (08:07:27); `token_unchanged` is true |
| Prompt freeze | The pass records `design/prompts-freeze.json` SHA-256 `b6ac6cea…735156`, frozen 07:37:10 UTC, 50 seconds after the pass began; `3cf8dedc…` was in force before it. The airk baseline ran under `b4044529…`. The pass therefore ran on the same prompts under a re-logged harness freeze. The history gives the reasons for the four re-freezes from `b4044529…` to `b6ac6cea…`: `run_baseline.py` (stage switching); `grade.py` and `run_pass.py` (tie rule, image check); `run_pass.py` (start-guard retry); `run_pass.py` (`check_lifetime`, additive). The manifests of those freezes were not kept, because the freeze file is overwritten at each re-freeze, so the exact list of changed hashes cannot be shown; what can be shown is below |
| Scenario freeze | `scenario-freeze.json`, SHA-256 `0a7d7b0a…e7806`, unchanged |
| Results | 52: 44 sessions run and judged, 8 reused |
| Second sessions | None; the run log records `no-close-calls` |
| Cost | Agents $1.013, judges $6.8544, total $7.8674 |

The session steps took 504.9 s (Search, Parse, no model), 434.4 s (Focus),
283.7 s (Compact) and 268.8 s (Large). Judging took 213.4 s and the depth
measurement 50.9 s.

**The freeze evidence.** The freeze history records `prompts_changed` false
at every re-freeze from `b4044529…` to `b6ac6cea…`, and the reasons name
harness code only (the entries are written by hand, not computed). The
receipts show it directly: the full Claude command line of the agent sessions
(prompt and flags, with only the paths removed) is byte-identical
between the airk baseline and the bookbuilder-v2 pass on Search, Parse, no
model and Focus, and the judge command line is identical on the one result
checked (C1 Search, judge 1). Both passes record the same scenario freeze
(`0a7d7b0a…`). The frozen files are harness files, not the rubric text; the
one scoring change between the two freezes is the tie rule of section 4.2
(`grade.better_of`). So the pass ran on the same prompts, judge prompt and
scenarios as the airk baseline.

**The result: no correct finish on any path.** All 52 results graded 0 from
both judges, and every result is "no win (neither finishes)". Headroom is 10
of 10: the better of Parse and Search never finishes correctly on any
bookbuilder-v2 scenario. Search came closest, with 5 of 7 notes on C1 and 5
of 6 on A3.

**Parse's first call has the same zero yield.** Its first reply was 1,862 to
1,866 bytes on all ten scenarios, and its evidence digest was the same for
all ten (`23e3386d…`), the digest it has on airk.

**Against Parse and Search.**

Each cell gives the essential notes covered by the end of the session and
the number of calls. No cell has a correct finish. "(r)" marks a reused
result.

| Scenario | Search | Parse | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- | --- | --- |
| C1 coding | 5/7, 10 | 1/7, 6 | 2/7, 12 | 2/7, 9 | 2/7, 9 (r) | – |
| C2 coding | 2/5, 11 | 1/5, 7 | 1/5, 9 | 2/5, 7 | 2/5, 7 (r) | 3/5, 10 |
| C3 coding | 2/6, 10 | 3/6, 4 | 2/6, 10 | 2/6, 10 (r) | 2/6, 10 (r) | – |
| W1 writing | 1/5, 5 | 1/5, 5 | 1/5, 5 | 2/5, 9 | 1/5, 11 | – |
| W2 writing | 4/6, 11 | 3/6, 5 | 2/6, 12 | 2/6, 12 | 2/6, 12 (r) | – |
| T1 test | 3/6, 11 | 2/6, 5 | 2/6, 12 | 0/6, 11 | 0/6, 11 (r) | – |
| T2 test | 2/6, 10 | 1/6, 7 | 3/6, 12 | 0/6, 8 | 0/6, 12 | – |
| A1 architecture | 0/8, 9 | 0/8, 10 | 0/8, 12 | 0/8, 12 | 0/8, 12 (r) | 0/8, 12 |
| A2 architecture | 2/6, 10 | 0/6, 8 | 0/6, 11 | 0/6, 10 | 0/6, 11 (r) | – |
| A3 architecture | 5/6, 9 | 2/6, 6 | 2/6, 12 | 2/6, 10 | 1/6, 11 | – |

Per path, over its rows. A reused row counts with its source's values.

| | Search | Parse | No model | Focus | Compact | Large (2 rows) |
| --- | --- | --- | --- | --- | --- | --- |
| Correct finishes | 0 | 0 | 0 | 0 | 0 | 0 |
| Essential notes covered | 26/61 | 14/61 | 15/61 | 12/61 | 10/61 | 3/13 |
| Covered by the first call | 4/61 | 0/61 | 4/61 | 5/61 | 4/61 | 0/13 |
| Mean calls per session | 9.6 | 6.3 | 10.7 | 9.8 | 10.6 | 11.0 |
| Mean bytes returned per session | 133,396 | 193,893 | 195,459 | 197,390 | 197,146 | 201,330 |
| Mean first reply | 20,921 B in 3.0 s | 1,863 B in 0.4 s | 49,739 B in 13.1 s | 47,601 B in 22.1 s | 47,702 B in 23.3 s | 54,515 B in 87.7 s |
| Mean tool time per session | 16.1 s | 21.3 s | 26.5 s | 70.5 s | 59.4 s | 96.2 s |
| Mean wall time per session | 40.4 s | 48.1 s | 54.1 s | 97.9 s | 88.5 s | 124.4 s |
| Agent cost of the sessions run | $0.2157 | $0.3448 | $0.2096 | $0.1680 | $0.0344 | $0.0405 |
| Rubric B, mean of both judges | 1.688 | 0.000 | 1.438 | 1.350 | 1.350 | 1.062 |

Search's first call covered 4 notes on three scenarios (C1 1, W1 1, A3 2).
No model's covered 1 each on C1, W1, W2 and A3. Focus's covered 2 on W1 and
1 each on C1, W2 and A3. Parse's and Large's covered none.

Bytes and time per scenario (bytes returned over the session, then tool
time and wall time):

| Scenario | Search | Parse | No model | Focus | Compact | Large |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | 154,100 B, 19.8 / 46.1 s | 159,318 B, 19.6 / 45.1 s | 192,622 B, 21.7 / 53.9 s | 179,154 B, 36.6 / 61.4 s | (r) | – |
| C2 | 166,236 B, 24.0 / 54.6 s | 246,042 B, 21.8 / 45.7 s | 160,155 B, 21.9 / 49.0 s | 136,875 B, 17.8 / 51.5 s | (r) | 209,594 B, 109.1 / 137.0 s |
| C3 | 166,357 B, 18.9 / 42.7 s | 144,031 B, 17.0 / 40.3 s | 172,103 B, 19.4 / 45.5 s | (r) | (r) | – |
| W1 | 48,956 B, 5.6 / 19.7 s | 168,840 B, 16.2 / 38.4 s | 128,449 B, 29.1 / 49.2 s | 211,698 B, 103.8 / 127.6 s | 159,530 B, 140.8 / 168.8 s | – |
| W2 | 181,387 B, 14.9 / 46.0 s | 192,115 B, 23.5 / 52.1 s | 201,605 B, 20.2 / 49.7 s | 275,312 B, 97.2 / 129.3 s | (r) | – |
| T1 | 149,808 B, 17.4 / 43.2 s | 178,160 B, 16.0 / 43.0 s | 223,411 B, 37.6 / 64.3 s | 234,070 B, 76.0 / 105.8 s | (r) | – |
| T2 | 116,171 B, 14.0 / 35.9 s | 162,795 B, 28.0 / 55.6 s | 165,445 B, 21.0 / 48.9 s | 108,766 B, 19.7 / 45.7 s | 161,355 B, 103.5 / 134.5 s | – |
| A1 | 123,161 B, 15.5 / 38.1 s | 265,741 B, 29.6 / 56.9 s | 232,739 B, 20.1 / 49.2 s | 208,768 B, 34.9 / 60.3 s | (r) | 193,066 B, 83.3 / 111.8 s |
| A2 | 99,676 B, 19.3 / 43.6 s | 240,228 B, 24.6 / 60.9 s | 252,932 B, 36.9 / 64.9 s | 214,787 B, 146.0 / 174.3 s | (r) | – |
| A3 | 128,107 B, 11.5 / 34.4 s | 181,661 B, 16.6 / 43.2 s | 225,130 B, 36.9 / 66.1 s | 232,370 B, 154.0 / 177.9 s | 191,361 B, 31.4 / 62.9 s | – |

Haiku sessions cost $0.0087 to $0.0929 each. The mean by path runs from
$0.0115 (Compact) to $0.0345 (Parse); Large's two sessions cost $0.0202 on
average.

**Per kind of task** (essential notes covered; every adjudicated grade is 0,
so every mean grade is 0):

| Kind | Notes | Search | Parse | No model | Focus | Compact |
| --- | --- | --- | --- | --- | --- | --- |
| Coding (C1 to C3) | 18 | 9 | 5 | 5 | 6 | 6 |
| Writing (W1, W2) | 11 | 5 | 4 | 3 | 4 | 3 |
| Test (T1, T2) | 12 | 5 | 3 | 5 | 0 | 0 |
| Architecture (A1 to A3) | 20 | 7 | 2 | 2 | 2 | 1 |

In notes covered, Search is ahead of every Answer configuration in coding,
writing and architecture. In test, no model matches Search (5 and 5) and
Focus and Compact cover none. In grade there is nothing to separate them.

**Against the rule, per configuration** (bookbuilder-v2 only, so not a
verdict). By the rule of section 4.2 the better of Parse and Search is
Search on 9 scenarios and Parse on C3 (3 of 6 notes against Search's 2).

| Configuration | Substantial wins | Ties at grade 0 | Worse |
| --- | --- | --- | --- |
| No model | 0 | 10 | 0 |
| Focus | 0 | 10 | 0 |
| Compact | 0 | 10 | 0 |
| Large (A1, C2) | 0 | 2 | 0 |

Rubric B: Search (1.688) is above every Answer configuration, and Parse
scores 0 from both judges on all ten scenarios. The better of Parse and
Search averages 1.512 (1.5 over A1 and C2, the Large rows).

**What each model adds.**

On the same ten rows, from the pass summary, with the rule of section 4.2
for the better of Parse and Search. Time is the mean tool time added per
session.

| Step | Correct finishes added | Turns saved | Time added |
| --- | --- | --- | --- |
| No model against the better of Parse and Search | 0 | 0 | +10.6 s per session (105.6 s over 10 rows) |
| Focus against no model (CPU) | 0 | 0 | +44.1 s per session (440.7 s over 10 rows) |
| Compact against Focus (CPU, timing only) | 0 | 0 | −11.1 s per session (−111.0 s over 10 rows) |
| Large against Compact (CPU, A1 and C2, timing only) | 0 | 0 | +69.8 s per session (139.7 s over 2 rows) |

Compact's negative step is the net of rows that differ widely, not a speed
gain from Compact. On A3 Compact took 31.4 s of tool time against Focus's
154.0 s, and A2's Compact row is the no-model session (36.9 s against
Focus's 146.0 s). W1 and T2 went the other way: 140.8 s against 103.8 s and
103.5 s against 19.7 s.

The Answer call itself, from the sessions that ran it:

| Configuration | Answer calls | Mean | Longest | Failed |
| --- | --- | --- | --- | --- |
| No model | 16 | 12.5 s | 18.0 s | 0 |
| Focus | 20 | 31.6 s | 67.6 s | 0 |
| Compact | 6 | 42.9 s | 86.4 s | 0 |
| Large | 2 | 87.7 s | 103.3 s | 0 |

Parse made 63 calls over its ten sessions, and 4 of them failed (T2 2, A2 1,
A3 1).

In the no-agent digest replays (`digests.json`), Compact's first call took
99,906 ms on W1 and 65,026 ms on C1, and Large's took 102,103 ms on A1 and
116,639 ms on C2. All were under the 120 s cap.

By first-reply evidence digest: Focus differs from no model on 9 of 10
scenarios (equal only on C3, so that Focus result is reused). Compact equals
Focus on A1, C1, C2, C3, T1 and W2, equals no model on A2 (and on C3, where
all three agree) and differs from both on A3, T2 and W1. Large differs from
Compact on A1 and C2. Search, Parse and no model have 10, 1 and 10 distinct
digests over the ten scenarios.

**Reuse.**

A result is reused under the rule of section 4.3.

| Result | Reused from |
| --- | --- |
| C3 Focus, C3 Compact | C3 no model |
| C1 Compact, C2 Compact, W2 Compact, T1 Compact, A1 Compact | The same scenario's Focus |
| A2 Compact | A2 no model |

Eight in all. The other 44 results were run as fresh sessions and judged.

**Depth on the fresh index.**

No agent. The same measurement as section 4.7 (`depth.py`, `limit` 100,
verification off, all model stages off, 65,536-byte frame, stepped down when
refused), run on the settled fresh index at 08:06:40 to 08:07:27 UTC, beside
the original index's measurement at publication 265, before any deny
(`depth-compare-vs-base265.json`, from `depth_compare.py`).

| | Original index (publication 265) | bookbuilder-v2 (publication 268) |
| --- | --- | --- |
| Hits returned per search | 59 to 84 (717 in all) | 59 to 84 (717 in all) |
| Byte frame used | 65,536 on 7, 32,768 on 3 | 65,536 on 7, 32,768 on 3 |
| Essential notes | 61 | 61 |
| Region covered, rule 2 | 10 | 10 |
| Covered under rule 1 | 13 | 13 |
| File in the pool (same path) | 24 | 24 |
| File in the pool, region not covered | 14 | 14 |
| File not in the pool | 37 | 37 |
| Mean / median search time | 2,702 / 2,637 ms | 2,782 / 2,726 ms |

By scenario the two measurements are equal in hits, in the three counts and
in the deepest rank: C1 has 1 of 7 notes covered, at rank 6; C3 2 of 6 (deepest
rank 35); W1 2 of 5 (17); T1 3 of 6 (68); A3 2 of 6 (10). C2, W2, T2, A1 and
A2 have none. The note ranks are equal on all ten scenarios
(`ranks_equal`).

The comparison reports `identical: false`, because the evidence digest of
three scenarios (A2, C1 and C2) differs. In those, read from the saved
replies, the differences are the collection and index identity, the
publication token, the discovery counters (8,116 artifacts on the fresh
index against 8,119 on the original) and, in two hits per scenario, the hit
text and its content hash. After converting line endings, those hit texts
are equal: the file differs by LF and CRLF. No hit differs in path, order or
rank.

**The 3-artifact gap.** The original index listed 8,119 files at
publication 265 (`credential-probe-bookbuilder.json`) and 8,117 after its two
credential-script denies (`credential-probe-bookbuilder-after-exclusion.json`,
publication 267). The two scripts (`ironspire/scripts/set-rss-key.ps1` and
`set-sendgrid-key.ps1`) are denied in `bookbuilder-v2` before its first index
pass (`credential-probe-bookbuilder-v2.json`: 8,116 files). That explains 2
of the 3. One file remains, 8,117 against 8,116, and the receipts do not say
which. Both indexes record the same source commit `238b22a7`, but the
original was indexed in place from the live checkout (the precheck found 8,250
tracked and 0 untracked, unignored files) and `bookbuilder-v2` from a
`git archive` extraction of 8,250 files. Neither the per-path listings nor
the index states are kept, so the receipts cannot show whether that
difference accounts for the last file.

**What it confirms.** The fresh index has a permanent deny, so before the
roster fix its searches would have seen only the first 4,096 paths in
alphabetical order (section 6.1). They return the same 717 hits at the same
ranks as the original index before any deny. The fix holds on bookbuilder as
on airk. The bookbuilder-v2 baseline and its depth measurement both ran with
it.

**What it shows about the baseline.** The depth is the bookbuilder history's
of section 4.7: 10 of 61 notes covered, 24 with their file in the pool and
37 with no file in it. As on airk, most essential material is outside the
pool, whichever index is used.

**Judge agreement.** 44 results have two verdicts. The judges gave equal
grades on all 44, and agreed on 95.81% of 334 per-note verdicts (section
4.8). The 8 reused results take their source's verdicts. No grade needed the
primary agent's settlement, and no close call needed a second session.

**Against the bookbuilder history pass.**

Essential notes covered over the session, on the original index (section
4.5, image `baseline-1fe7b600-o2-fix2`) and on bookbuilder-v2:

| | Search | Parse | No model | Focus | Compact | Large (A1, C2) |
| --- | --- | --- | --- | --- | --- | --- |
| History (r2) | 17/61 | 16/61 | 12/61 | 13/61 | 13/61 | 2/13 |
| bookbuilder-v2 | 26/61 | 14/61 | 15/61 | 12/61 | 10/61 | 3/13 |

The two passes differ in index, image and agent runs, and have one session
per cell. This report draws no conclusion from the difference. Both have no
correct finish on any path, and both have headroom 10 of 10.

### 4.7 Depth: essential notes in a 100-hit pool

No agent. Each instruction was searched once with `limit` 100, verification
off, all model stages off, on the settled airk index (`depth.py`). No search
returned 100 hits: the byte frame truncated every reply. A request at 65,536
bytes was refused (`scope_reply_budget`) on 9 of 10 airk scenarios, and the
harness stepped down to 49,152 and then 32,768 bytes. So a "100-hit pool" is
in practice 47 to 66 hits on airk and 59 to 84 on bookbuilder.

| | airk (publication 330) | bookbuilder (original index, publication 265) | bookbuilder-v2 (publication 268) |
| --- | --- | --- | --- |
| Hits returned per search | 47 to 66 (561 in all) | 59 to 84 (717 in all) | 59 to 84 (717 in all) |
| Byte frame used | 32,768 on 9 scenarios, 65,536 on C2 | 65,536 on 7, 32,768 on 3 | 65,536 on 7, 32,768 on 3 |
| Essential notes | 53 | 61 | 61 |
| Region covered, rule 2 | 5 | 10 | 10 |
| File in the pool, region not covered | 13 | 14 | 14 |
| File not in the pool | 35 | 37 | 37 |

airk, by scenario: C1 has 1 of 5 notes covered, at rank 13; C2 has 2 of 6,
at ranks 2 and 19; C3 has 1 of 5, at rank 53; T1 has 1 of 5, at rank 6. W1,
W2, T2, A1, A2 and A3 have none. Only two essential notes rank inside
Answer's first search of 8 hits (C2 at rank 2, T1 at rank 6). They are
exactly the two notes that every path's first call covered. Within 16 hits,
the pool change set 1 gives Focus, there are three.

**The findings it led to.**

1. **Lexical search is conjunctive.** The product lexical branch joins every
   query term with `AND` (`crates/airk-store/src/current/lexical.rs`) and
   orders matches by stable id, a hash. Materialization checks the same
   conjunction. A 40-to-70-word instruction matches no chunk, so hybrid
   search of an instruction is the vector ranking alone. This was read from
   the code (`design/search-v2.md`) and holds for both indexes. A unit test
   of change set 2 (`switch_off_hybrid_search_is_the_product_search`)
   independently shows that the product hybrid order of a prose instruction
   is the vector order.
2. **The vector side favours prose.** Of airk's 53 essential notes, 42 are
   in code files and 10 in prose. Of the 561 pooled hits, 196 are Markdown
   and 37 are Word documents, and only 67 are Rust. Another 187 come from
   `scripts/arbiter/research/`: 104 proposal patches and diffs (`.patch`,
   `.diff`), 49 `.in` history inputs, 32 scripts and 2 JSON files. On bookbuilder, 49 of
   61 notes are code; of 717 pooled hits, 335 are prose and 104 Word
   documents. The airk first Search replies show the same shape: of 100
   hits, 50 prose, 6 office documents and 22 code.
3. **The search roster cap and its fix.** Before the fix, every search of
   an index with a permanent deny saw only the first 4,096 paths in
   alphabetical order. The cause, the fix and its test are in section 6.1.
   The airk depth above and the whole baseline ran with the fix. The
   original bookbuilder depth ran before any deny, so the defect could not
   touch it. The bookbuilder-v2 depth ran with the fix on an index with
   denies and equals it (section 4.6).

Under the plan's rule (section 5.3, "What brings it in"), essential items
missing even at this depth mean that fusion or chunking is the bottleneck,
so shared search ranking enters the next set (section 8).

### 4.8 Judge agreement

| Pass | Results with two verdicts | Equal grades | Per-note verdicts that agree | Adjudications |
| --- | --- | --- | --- | --- |
| airk baseline | 71 | 71 | 93.02% of 487 pairs | None needed |
| bookbuilder history (r2) | 44 | 44 | 93.11% of 334 pairs | None needed |
| bookbuilder-v2 baseline | 44 | 44 | 95.81% of 334 pairs | None needed |

Grade agreement tests little here, because nearly every grade is 0. The
per-note figure is the informative one: about one note verdict in fourteen
differs between the two judges without changing a grade (about one in
twenty-four on bookbuilder-v2). No grade needed the primary agent's
settlement.

## 5. What was good

- **The pass was clean.** Settled index throughout, one publication token,
  no discarded session, no failed Answer call, no timeout, every judge
  verdict valid, no pending adjudication.
- **Search can reach a correct finish.** On C1 and T1 the Haiku agent
  searched, then read the files the hits pointed to at the right ranges,
  and had all essential facts by turn 9 and turn 7.
- **Reuse saved 11 sessions** with the evidence check, including every case
  where Context could not act.
- **The depth measurement settles the plan's open question.** It shows that
  the bottleneck is retrieval: most essential material is outside the pool.
  So shared search ranking moves into change set 2, as the plan required.
- **The roster defect was found and fixed before any judged airk pass**,
  with a regression test (section 6.1).
- **The Sonnet arm answers the question the user asked about the model
  change:** at the baseline Sonnet did not find more than Haiku, at 12 to
  14 times the cost per session.
- **The bookbuilder-v2 pass was clean too.** One publication token over all
  snapshots and the depth run, 52 results complete, no failed Answer call,
  no pending adjudication and no close call.
- **The roster fix holds on bookbuilder.** The fresh index, which has a
  permanent deny, returns the same 717 hits at the same ranks as the
  original index at publication 265 (section 4.6): 10 of 61 notes covered
  under rule 2, 13 under rule 1, 24 by path.
- **The judges agree** on every grade and on 93% to 96% of note verdicts
  (airk 93.0%, bookbuilder history 93.1%, bookbuilder-v2 95.8%).

## 6. What was bad

- **The floor is near zero.** Two correct finishes in 82 results, both
  Search and both after five or more calls (grade 1). Parse, Answer in every
  configuration and the whole Sonnet arm reached none. The headroom of 10 of
  10 is a measure of how little the tools achieve, not only of the room
  there is to win.
- **The bookbuilder-v2 floor is zero.** No correct finish in 52 results, on
  any path; all 44 judged sessions graded 0. Search came closest, with 5 of
  7 notes on C1 and 5 of 6 on A3. The headroom of 10 of 10 there is again
  a measure of how little the tools achieve.
- **Answer's gathering covers less than Search on bookbuilder-v2 too.** Over
  the session, 15 of 61 essential notes for no model, 12 for Focus and 10
  for Compact, against 26 for Search. Answer's first reply is 2.4 times
  Search's size (49,739 against 20,921 bytes on average) and covers 4 of 61
  for no model and 5 for Focus, as Search's covers 4. On the two test
  scenarios Focus and Compact covered none of the 12 notes.
- **Answer's gathering covers less than Search on airk.** Over the session, 12 of
  53 essential notes for no model, 11 for Focus and 8 for Compact, against
  22 for Search. Answer's first reply is 2.3 times Search's size (51,326
  against 22,644 bytes on average) and covers the same 2 of 53. It holds
  fewer search hits (5 to 8, against Search's 10), and its only follow-ups
  are context reads and literal searches.
- **Answer stops when one follow-up fails.** On A1, A2 and W2 the no-model
  first reply ended `refused`. A context read on a Word-document hit failed
  with "range crosses document locations; request one page/block/slide at a
  time", and a read error clears Answer's whole queue. On A1 the reply kept
  two records. The Sonnet arm's no-model replies on A1, A2 and W2 ended the
  same way. The same error also appears in its Focus reply on A2 and in a
  second no-model Answer call on W1.
- **Parse's first call has a structural zero yield.** Its first reply was
  1,840 to 1,847 bytes, with the same evidence digest on all ten airk
  scenarios and on all ten bookbuilder ones. Parse then covered 7 of 53
  notes in 54 calls; 4 of its calls failed (A1 1, A2 1, A3 2).
- **Focus added time and nothing else:** 16.3 s of tool time per session,
  first replies of up to 47.9 s, no essential fact added. On
  bookbuilder-v2 it added 44.1 s per session, with Answer calls of up to
  67.6 s.
- **Compact and Large added time only.** Compact's Answer calls averaged
  49.1 s and reached 97.7 s; Large took 66.4 s on A1. In the C3 Compact
  session the agent's tools took 151.9 s in all. On bookbuilder-v2 Compact's
  Answer calls averaged 42.9 s and reached 86.4 s, and Large's averaged
  87.7 s and reached 103.3 s. The digest replay of C2 on Large took 116,639
  ms, 3.4 s under the 120 s cap.
- **The same Focus request gave different evidence in two runs.** On W2 and
  A2 the Haiku and Sonnet arms' Focus first replies differ; the cause is
  not established. A Focus result is therefore one sample of a reply that
  can vary, not a fixed function of the instruction.
- **Runs invalidated or superseded along the way:**
  - *Baseline attempts 1 and 2* (04:24:27 and 04:25:12 UTC) ran no session.
    The start guard's status call got a transient `selection_busy`
    refusal and stored "no token". The second attempt then compared against
    that empty token and called it a change. The harness was fixed (the
    snapshot honours typed retries; a start reference is stored only when
    settled with a token) and re-frozen at 04:25:47 UTC, and attempt 3 is
    this pass. Marker: `INVALID.attempt-1-guard-bug.json`.
  - *`baseline-bookbuilder` (r1)* lost about 85 agent turns to typed retries
    the wrapper did not yet honour. It was rerun as r2 and kept as evidence
    of the defect.
  - *`baseline-bookbuilder-r3`* was stopped. It ran on publication 267 with
    truncated search (`TRUNCATED-SEARCH.json`). Its first marker gave the
    withdrawn vector-loss reason.
  - *Truncated search also invalidates* the pilot's airk rows and probes,
    the change-set-1 bookbuilder no-model replay and two change-set-2 depth
    runs on publication 267.
  - *Two bookbuilder depth attempts* were superseded. One was refused by
    the verification reply budget on 7 of 10 searches; the other exceeded
    the 65,536-byte reply cap on 3 of 10. The harness turned verification
    off and stepped the byte frame down.
- **The rule's text does not say which comparator a tie at grade 0
  takes.** The primary agent's tie rule of 22 November (section 4.2) fills
  the gap, and this report applies it to every pass. Search alone is given
  beside it wherever that matters (sections 4.1 and 4.2).

### 6.1 The search roster defect and its fix

- **Defect.** In `filtered_paths`
  (`crates/airk-kernel/src/current/search.rs`), any permanent deny in an
  index's source policy made kernel search build a bounded roster of allowed
  paths. The roster held at most 4,096 (`MAX_SCOPED_PATHS`), in alphabetical
  order, and every search was limited to it. The only sign was `truncated`
  in the reply.
- **Affected.** The airk index from its first pass, `bookbuilder-v2`, and the
  original bookbuilder index from publications 266 and 267.
- **Fix.** One line, at query time. A roster is built now only for a selected
  scope that omits sources of the publication, or for an explicit filter.
  The deny is still enforced by its removal publication and by the per-path
  checks of every search branch. No reindex is needed. Patch
  `trim-diag/policy-search-roster-fix.patch`.
- **Test.**
  `permanent_deny_searches_the_whole_retained_corpus_not_a_first_path_page`
  (`crates/airk-kernel/tests/source_policy.rs`) indexes more than 4,096
  paths before its target, denies a file, and checks that vector, hybrid and
  lexical search still find the late retained path and never return the
  denied file.
- **Product impact.** Only indexes whose source policy restricts paths:
  trims, split transfers and positive partitions. Ordinary credential and
  `.gitignore` exclusions are applied at discovery and do not trigger it.
  **Product adoption is recommended.** The fix is in the private evaluation
  images only and needs the normal path to reach the product.

## 7. Why

From the turn logs (`coverage.json` per session, which records each turn's
tool and the notes it newly covered) and the first replies.

- **The needed material is rarely retrieved.** For 35 of 53 essential notes
  the file never enters a 100-hit pool. Search, and Answer's own first
  search, rank prose and experiment files above code (section 4.7). Every
  path starts from the same poor ranking. Search sessions get further only
  because the agent keeps searching and reading.
- **Where coverage came from.** Over the ten Haiku Search sessions, 38
  search calls covered 10 essential notes and 25 file reads covered 12. In
  the no-model Answer sessions, 16 Answer calls covered 4 and 21 file reads
  covered 8. The 20 symbol lookups, 24 reference lookups and 8 outlines
  covered none. In the Focus sessions, 28 symbol lookups, 15 reference
  lookups and 7 outlines covered none either. Symbol and outline rows are
  names at line ranges, not text, so under the frozen rules they cannot
  cover a note. No reference lookup on any path covered an essential note.
  After Answer, the agent tends
  to spend its turns on lookups that cannot count, where a Search agent
  reads files.
- **Answer's follow-ups are built for prose.** Every Haiku first Answer
  reply holds only `search` and `context` records. The initial search is
  8 hits sharing 4,096 bytes. The follow-ups are 32-line context reads of
  the top hits and searches on runs of capitalized words, which code
  identifiers never produce. Answer looks up no definition, caller, test or
  governing contract. Packing then omits up to 4 records to fit the
  65,536-byte frame.
- **A failed read ends the gathering.** The office-document refusals of
  section 6 come from Answer's rule that any read error clears the queue.
- **Focus cannot reach what matters.** It reorders at most 4 of the at most
  8 hits of each search that Answer makes, and cannot bring in a hit from
  outside them. Within 8 hits the depth pool holds 2 essential notes; Focus
  had nothing better to promote.
- **Context cannot act.** With zero calibration rules every Context action
  abstains, so Compact and Large change no read. Where their evidence
  differs from Focus, the cause is the halved Focus allowance, not a Context
  choice.
- **Parse needs an identifier.** A prose instruction gives it none, so its
  first call returns a plan and no evidence. Its later calls are the
  agent's own plans.
- **The Sonnet agent stops earlier.** It made fewer calls on every path
  (6.4 against 7.6 on Search) and wrote its brief sooner. On C1 it stopped
  after 3 calls with 1 of 5 notes covered.

## 8. What happens next

**Change set 1** is built (image `cs1d-d316beaf`, which includes the roster
fix) and runs next on airk's ten scenarios, as the user directed. Parse and
Search are unchanged by it, so their baseline sessions are reused. It holds
the eight changes of the plan (section 5.3), as implemented in the private
evaluation build, with the primary agent's decisions:

| # | Change | Layer | Baseline evidence it answers |
| --- | --- | --- | --- |
| 1 | Software-task follow-ups: whole chunks of the kept hits, definitions and references of identifiers named in the instruction and the hits (the Parse recipe shapes), excerpts and tests from those, outlines, and governing CONTRACT.md or README files, pre-checked with `file_info` | Gathering | Answer's only follow-ups are context reads and literal searches (section 7) |
| 2 | Follow-ups spread across hits, round robin by kind, with a queue as wide as `fanout` | Gathering | The top hit fills the queue |
| 3 | A larger first search: a wider pool of snippets (16 by default in the evaluation build), with whole chunks only for the kept hits | Gathering | 8 hits share 4,096 bytes |
| 4 | Focus scores the pool and keeps the best (an evaluation-only Focus profile with `max_candidates` 16; one pool size on both devices) | Focus | Focus reorders 4 of 8 |
| 5 | The software-task Focus instruction `SoftwareEvidenceV1` in place of the web-search one (a recorded digest-input change of the private build) | Focus | Effect unknown; isolated by a leave-one-out replay |
| 6 | Context can act: reads stated in words, more varied choices, and a provisional data-only rule that accepts a Read when it is the top card (option A) | Context | Context abstains on every call |
| 7 | Packing and presentation: drop overlapping evidence, group by purpose, disclose what was left out | Gathering | Rubric B, and frame space |
| 8 | A device-shaped budget that keeps CPU inside 120 s | Gathering | Compact calls of up to 97.7 s, Large 66.4 s |

Change set 1 also makes a failed follow-up read keep the gathering going,
where today one failure clears the queue. That answers the three refused
replies of section 6. Its replay tunes the starting values with the tokens
`pool`, `pool_full`, `read_floor`, `kept`, `subjects` and `governing`. Two
known costs go into its report: the eight failing `cpu_source_transition`
tests, and the recorded digest-input changes (`runtime.rs`,
`bounded_hybrid.rs`).

What the baseline predicts for it: the first search's pool of 16 holds only
3 of 53 essential notes, so a gain in change set 1 has to come from the
follow-ups (definitions, reference excerpts, tests, governing documents),
not from Focus choosing within the pool. After change set 1 is judged, the
Context trainer decision applies its go/no-go bar: first measure how much a
forced best read gains over queue order.

**Change set 2: `search_v2`.** The depth result brings shared search
ranking in, as the plan's rule requires. `search_v2` is a switch of the
evaluation build, off at the baseline. It applies only to hybrid searches of
8 or more words:

- prose query rewriting into key and named words;
- two ranked any-term lexical branches (key words, and named words only);
- 300 candidates per branch before fusion;
- weighted fusion;
- a file-kind prior: generated output 0.3, office documents 0.5, prose
  0.75, governing files 1.0;
- per-file diversity;
- snippets centred on key words.

It changes what Parse and Search return, so from change set 2 their sessions
run again, and both repositories are used. The airk pool's 187 hits from
`scripts/arbiter/research/` (proposal patches, diffs and history inputs) are
not covered by the prior as specified; their effect should be measured in
change set 2's depth replay. The bookbuilder-v2 baseline (section 4.6) has
run, and its depth equals the original index's, so change set 2's bookbuilder
comparisons use it.

## 9. Limits and receipts

**Limits.**

- The rule of "substantially improves" needs both repositories, and round 1
  is airk only, so it cannot be applied to this round. The bookbuilder
  history rows (section 4.5) come from an index that later change sets do
  not use. The bookbuilder-v2 baseline (section 4.6) is the base for them;
  its turn logs were not analysed for the "Why" of section 7, which comes
  from airk.
- One session per scenario and path. With no close call there were no
  second sessions. Section 6 shows that a Focus reply can vary between runs.
- Haiku agents, with a Sonnet reference on three paths of airk only; none on
  bookbuilder-v2.
- CPU only. No GPU cell ran; Large ran on two scenarios as a measurement.
- Coverage is the mechanical rule 2 of [rubrics.md](rubrics.md). Outline and
  symbol rows never count, so a session that found the right function by
  name and did not read it gets no credit.
- The judges' blinding is partial, because Answer's reply has a recognizable
  shape. With nearly every grade 0, grade agreement is a weak test.
- The depth pool is truncated by the byte frame at 47 to 66 hits, not 100.
- The airk baseline ran on prompt freeze `b4044529…`, which
  [rubrics.md](rubrics.md) lists. The bookbuilder-v2 baseline recorded the
  later freeze `b6ac6cea…`, made while it ran. The prompt texts, the judge
  schema, the default models and effort are the same in all of them.
  Between the 00:40 UTC freeze `ef4dc528…`, named in [00-setup.md](00-setup.md),
  and `b4044529…`, eleven re-freezes changed only harness files, each
  recorded with its reason in `design/prompts-freeze-history.jsonl`; so did
  the one at 07:37:10 UTC. The hashes the airk baseline ran under are below.
- The depth comparison's evidence digests differ on three scenarios by hit
  line endings and index identity (section 4.6); the hit order and ranks are
  equal.
- The file-kind counts of sections 4.7 and 6 come from file extensions in
  the logged replies, counted for this report.
- Everything ran on one Windows host. Nothing here is cross-platform
  evidence.

**Receipts.** Every path below is under the git-ignored campaign folder
`artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1/`, which is
not committed.

| Receipt | SHA-256 |
| --- | --- |
| `passes/baseline/results.csv` | `893f4d90f74ee4a1754209dddc2fe7974058c651aec9dd40fbca450e810ccd31` |
| `passes/baseline/baseline-summary.json` | `5b585a234603fc71e761d6ede95a957d630ee33200b090351652c5f0480fa8ce` |
| `passes/baseline/summary.json` | `93db9f64934b446f0d58b3c83a5895f2f497743214822ebe7b0bdb333cacb939` |
| `passes/baseline/pass-report.json` | `0e59104868c7c01d7524ddb731ad31a62261f0543a958c5121bb9438cce73456` |
| `passes/baseline/pass-index.json` | `adf0110b0e345b26f735a826ac3f3462503848a49f98573294d3e6b19b20c8ef` |
| `passes/baseline/digests.json` | `8c1e59d1b9645d92c8e48c0a181700c89ad7e4552783fa4ab50bbab484adb231` |
| `passes/baseline/reuse.json` | `0fdcc18dc9f5ae8ca78f29292684a9ac882db4701bc76ffae6511b236feb5de1` |
| `passes/baseline/spec.json` | `26746957c795824ca831954ffcddbeb7bd839fff98963dc9eae1dcaa07d24839` |
| `passes/baseline/settled.jsonl` | `2919d11149b4489ea830094c12ee910ed1f527376c549d183cb29a9704fc2d84` |
| `passes/baseline/settled-start.json` | `5da4c7363fcfd060d2a036fee8304fc7c90ecaac788314195734fc1c6565c98c` |
| `passes/baseline/baseline-run.jsonl` | `8925dac34585d929c0ddd5ba6e19358d52b1069a8d872331e668625db1595820` |
| `passes/baseline/INVALID.attempt-1-guard-bug.json` | `959451b92712583e57db82c882001122fb1b31398e8acdc0198cdc1cb57ae6a5` |
| `passes/baseline/depth/airk/depth.json` | `80ad861deb51ca3ae3d4b72a54afa21c4a97fa3875c289ad7aec679c561e1c26` |
| `passes/baseline/depth/bookbuilder/depth.json` | `7e5702e16f50d37b21c89b6098e836cf43379dfff3af0fa56a22dff63a36495b` |
| `passes/baseline-bookbuilder-r2/results.csv` | `eef337548ca5dcb72070ce6e1f9af987451c7fb9241e7254c0ca178e7462db9d` |
| `passes/baseline-bookbuilder-r2/baseline-summary.json` | `652068e9a21364d768ec92a5d933a086c39b9dc4100a7822fbf99fbb49ca87cc` |
| `passes/baseline-bookbuilder-r2/summary.json` | `deefbff0c6ad35963f11f2fd52f8675aa7c2fae7092af5c945dcfb9a4a8d1301` |
| `passes/baseline-bookbuilder-r2/pass-report.json` | `f099404189d7ce128d0ffba792a01795e4741b602f195f177e6bf9b585fac370` |
| `passes/baseline-bookbuilder-r2/spec.json` | `e2d4e56d3df5142452e24fee8760d8cd96f85e7677f810b9bda42eea5321e754` |
| `passes/baseline-bookbuilder-r2/digests.json` | `b9a9ac45aa9f2943d61f6dbf0363e97f9f6e43a59a8eb27f7b413317cf2b4adc` |
| `passes/baseline-bookbuilder-r2/digest-failures.jsonl` | `83d0b38f576b615abcb5788bf3403cf66f7897397089e16fbef600608e9fe57a` |
| `passes/baseline-bookbuilder-r2/validity-credential-exclusion.json` | `0273d5e12723320376866a064ea8e7bab97e27b7a6c99f0f28bdf64fe3ecf84b` |
| `passes/baseline-bookbuilder-v2/results.csv` | `12e96f621bf17cbf3cd80124c2627e3c8d8bc5e3a7344b6c2734bf161ac85da6` |
| `passes/baseline-bookbuilder-v2/baseline-summary.json` | `87de83879dedd51a788e0949f21293d0452267225e41e8c1a313a46e8fdedf79` |
| `passes/baseline-bookbuilder-v2/summary.json` | `fe6cc2d360657c8fa90e363c1c0ee3fe47daa0987a78622d0e402a1472ab55dd` |
| `passes/baseline-bookbuilder-v2/pass-report.json` | `597a36f0ad5ee462a113eece0a5c65555a76e135b6d931d51f9cb4df0216454c` |
| `passes/baseline-bookbuilder-v2/pass-index.json` | `461ec4f0dff673f8eeee6563be2f96b07b62b6e80c63e32bc2ab8bc818228bf6` |
| `passes/baseline-bookbuilder-v2/digests.json` | `dc2ad0af8dce6febbf6e743035bf28499c4ebb2f5395e822602897d64920e142` |
| `passes/baseline-bookbuilder-v2/reuse.json` | `f4015d750fbea25552eca18b910bafcd27a36558c86370a841c79766938e1d2d` |
| `passes/baseline-bookbuilder-v2/spec.json` | `607b19cf300c561172104a6d4b5c4fe99af1dd6d09b112ea84dce5b80bb0d1cf` |
| `passes/baseline-bookbuilder-v2/settled.jsonl` | `feddc124a746c7bfde96de061e198ec6ccb20467163198d0a3ca9a00fa9d99fd` |
| `passes/baseline-bookbuilder-v2/settled-start.json` | `5bba3a5a9e8f16020c56033c6bc7612ad352f260e174ed3e3de13920059508ce` |
| `passes/baseline-bookbuilder-v2/settled-wait.jsonl` | `2516fd247d58524d9ea7cac69d04d05cd120e0f24753dd342b6b035a7c240181` |
| `passes/baseline-bookbuilder-v2/baseline-run.jsonl` | `4e2b35792e3f5cee367529a5abaf1000e9cf1ab02c2664c6696c39749ac1aaf8` |
| `passes/baseline-bookbuilder-v2/pass-log.jsonl` | `0415833b19955d7e02f1dafcf41e923781feb0c7a48f0fc2b53b6e695ab63cc4` |
| `passes/baseline-bookbuilder-v2/depth/depth.json` | `bf0517f69181f2a956a67f78fa94ae2803c7cfef5d17d004daa5b3cdc3cb7085` |
| `passes/baseline-bookbuilder-v2/depth-compare-vs-base265.json` | `c22368f28335059ef40f9ea6bc6c4b090ca0d53ca09ab47bd9c9f61919aac209` |
| `design/prompts-freeze.json` (04:25:47 UTC) | `b4044529440670ecd4c866244ff9fb746c97da8e5d32f54322b2656fc8ec9f59` |
| `design/prompts-freeze-history.jsonl` | `955140b63a83e6d11284f86d889d531cb3b1bedad091818d927889b333d1d4ed` |
| `design/change-set-1.md` | `86ee9a4992da0e7f2cffda5276859b085fd1921dda23724afe4d07cf45190069` |
| `design/change-set-1-decisions.md` | `afedb9f009606c12fda2e1846fcce3bea10bf2f3a2c869694d0416361ee43ebe` |
| `design/cs1-tuning-tokens.md` | `e340433f2d91b41248796dd290a3a934b2d9f60dcb06e9e274342b355a343b93` |
| `design/search-v2.md` (revision 2) | `283f4817ea3fb4630c7671e8432c9b1942ed17e183a71f7a997dc34bf006619d` |
| `fixture/images/baseline-1fe7b600-o2-fix3/images.json` | `3f2cddb4d7f759835cf8f4d5c1b553f92779aeb34256400914d67fd47806bcc1` |
| `fixture/images/baseline-1fe7b600-o2-fix2/images.json` | `18b5ce6574e9274c13a75cb3d51fb347c591e324f399c856b18baff5605805d3` |
| `fixture/L21-fix3/handle.json`, as the pass recorded it | `2eb1942e14ade87c7c99e36451316277bfa0151e002b7713808dfa8dd3bb25fe` |
| `trim-diag/policy-search-roster-fix.patch` | `5573907617736e2774f14be86850abd755313437a75d61668e8125b54a1d787c` |
| `scenario-freeze.json` | `0a7d7b0a478b6e9d7cdeca9bc363452a40e6afc8ae4db5f2af557856d53e7806` |

The airk lifetime handle has been rewritten since the pass; its current
SHA-256 is `91e6a9fb6996311e1f155d7c1cbe217edc25467451745922ed052e2d717e9928`.
The bookbuilder-v2 pass's handle `fixture/L35-fix3/handle.json` was recorded
as `0fe086710f10d3dec932d1e28989f476a4d969be19cc06001b3ce73f20316df0` and was
rewritten when the lifetime closed (08:08:41 UTC); its current SHA-256 is
`9d12560403c7cb51b92753a0dd769b86dc2e776a41f4e7aa6c0d256b2ee764d5`. The
freeze `b6ac6cea…` the bookbuilder-v2 pass recorded was later overwritten
in `design/prompts-freeze.json`; its hash is kept in the pass receipts and
in the freeze history. The
`search-v2.md` value differs from the one in
[00-setup.md](00-setup.md) because that note gained its revision 2 since.

The harness files the baseline ran under, as frozen at 04:25:47 UTC (the
files in `scripts/arbiter-answer-eval/` matched these values when this
report was written):

| File | SHA-256 |
| --- | --- |
| `common.py` | `d6ed9f55f92d2f570b5d2f3d77143021fa51108f8ae7e9dddd36e95f0a696d55` |
| `coverage.py` | `35f9df4953596c4f4371c8f30709463213e0708e09bd3788096e60232e8027b8` |
| `cs_replay.py` | `367bd2742482d80e0b07e66558f0dae35fe14bf6cbf40e45d55138a10fe41d74` |
| `depth.py` | `9bc8dfd68669c952078fca6e5dc5ac24eea5b1f179fca8e1b21400cbbd47705a` |
| `evidence.py` | `803adbe349b130a8c0d2499086a4d42bb4861fb2973b6f943c6d57ae83ece4cb` |
| `grade.py` | `889135d5b8a693227c065c72031e2cb54bc0658b2d9d054f0b3e137132103bbb` |
| `judge.py` | `4fe4c74a96aaaf7aef24cd4d69f050a5be69a657e7e4d0d69be2f632ad0b813a` |
| `mcp_wrapper.py` | `2e552c4bea1697c94662a2d437b22f200b96178b9ce7bc3d41d1770dac15386d` |
| `pass_digests.py` | `88365781488fec78468e3f34c3d11174f695dc0c816bb708253f0c1d51721070` |
| `pass_report.py` | `ac8fa964ff25ea7b182be243fb70075d4c942bfcc9a3fca43a4b0653b88a59c6` |
| `prompts_freeze.py` | `dd2b6cfbe342fea8a9b7a566feb3f4afcd183e55ec58f54bad0491f4f28764a3` |
| `rejudge.py` | `cd4f2d474cb479f0859c48405c0fe582aa58ab4d8e008ec93c08c01d62b33f5e` |
| `replay.py` | `dcbad1183dd36396079b6c3ae20a85f8be1854cda331d75addee54396d211df2` |
| `rescore.py` | `f914e1f57df6efbea017bd91a7ca4a086454f28c365b3ff0bdb800595306a4f4` |
| `run_baseline.py` | `6d748afd115cb870aa1c4415a3979110d0dd2b1713693d6e6d9116472d58274d` |
| `run_pass.py` | `df113ffa2665f490dda8d0096583efaedc3582c28a05de55ae2c283436dfbc5d` |
| `run_session.py` | `2e1caf1003222796af3f45dcfa4890f24bff68c4f195a5e537b8b331f69c88d7` |
| `settled.py` | `4c7d81538b3788b25d3ade31be32c9dc82c28a2dd5d75c6f4c004f9ec83a9508` |

**The results file.** [results/01-baseline.csv](results/01-baseline.csv)
holds the harness's fixed columns for the 82 airk results (series
`baseline`), the 52 bookbuilder history results (series
`baseline-bookbuilder-r2`) and the 52 bookbuilder-v2 results (series
`baseline-bookbuilder-v2`), copied from the three passes' `results.csv`
unchanged. It holds grades, counts, timings and cost, not evidence text.
Its `elapsed_ms` column is tool time; wall time is in `pass-report.json`.

**Commands that reproduce it.** Python runs as `python -I -B` from
`D:\code\Github\airk`, as the verified human. `R` is the campaign folder
above, and the service lifetime is started with `fixture.py` as in
[00-setup.md](00-setup.md) section 9.

The whole pass, attempt 3, was one driver run (2,165.2 s, exit 0):

```text
python -I -B scripts/arbiter-answer-eval/run_baseline.py --spec R/passes/baseline/spec.json --interval 30 --depth R/passes/baseline/depth/airk
```

It ran these steps, logged with their exit codes and times in
`passes/baseline/baseline-run.jsonl`:

```text
# Haiku Search, Parse and no model, and the Sonnet no-model arm, every stage off (586.4 s)
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/baseline/spec.json --r-root R --stage sessions --only search,parse,noModel,reference-sonnet-noModel
#   then --stage coverage, --stage judge, --stage grade (the Sonnet best-of arm needs these grades)
# the Sonnet arm on Search, its comparator path (99.4 s)
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/baseline/spec.json --r-root R --stage sessions --only reference-sonnet-best
# Focus on (432.4 s), then Compact (298.5 s), then Large (187.7 s)
python -I -B -S scripts/arbiter-answer-eval/fixture.py stage --lifetime L21-fix3 --profile focus-static1024-cpu --enable
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/baseline/spec.json --r-root R --stage sessions --only focus-cpu,reference-sonnet-focus-cpu
python -I -B -S scripts/arbiter-answer-eval/fixture.py stage --lifetime L21-fix3 --profile context-compact-cpu --enable
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/baseline/spec.json --r-root R --stage sessions --only compact-cpu
python -I -B -S scripts/arbiter-answer-eval/fixture.py stage --lifetime L21-fix3 --profile context-compact-cpu --disable
python -I -B -S scripts/arbiter-answer-eval/fixture.py stage --lifetime L21-fix3 --profile context-large-cpu --enable
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/baseline/spec.json --r-root R --stage sessions --only large-cpu
# coverage, judging (149.2 s) and grading over all results, then the report
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/baseline/spec.json --r-root R --stage coverage
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/baseline/spec.json --r-root R --stage judge
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec R/passes/baseline/spec.json --r-root R --stage grade
python -I -B scripts/arbiter-answer-eval/pass_report.py --pass-dir R/passes/baseline
# stages off again, then the depth measurement (110.9 s)
python -I -B scripts/arbiter-answer-eval/depth.py --handle @newest --out R/passes/baseline/depth/airk --repos airk --limit 100 --max-bytes 65536
```

The bookbuilder history pass ran the same way from
`R/passes/baseline-bookbuilder-r2/spec.json`, without the Sonnet arm.

The bookbuilder-v2 baseline ran from `R/passes/baseline-bookbuilder-v2/spec.json`
on lifetime `L35-fix3`, as one `run_baseline.py` driver run (1,866.6 s, 07:36:20
to 08:07:27 UTC) with the same steps, logged with their exit codes and times
in `passes/baseline-bookbuilder-v2/baseline-run.jsonl`: Search, Parse and no
model (504.9 s); Focus (434.4 s); Compact (283.7 s); Large on A1 and C2
(268.8 s); coverage, judging (213.4 s) and grading; the report; and the depth
measurement (50.9 s):

```text
python -I -B scripts/arbiter-answer-eval/depth.py --handle @newest --out R/passes/baseline-bookbuilder-v2/depth --repos bookbuilder --limit 100 --max-bytes 65536
python -I -B scripts/arbiter-answer-eval/depth_compare.py --a R/passes/baseline/depth/bookbuilder/depth.json --b R/passes/baseline-bookbuilder-v2/depth/depth.json --label-a base265 --label-b v2-268
```

The comparison's arguments are those its receipt records (`a`, `b` and the
two labels); its output is `depth-compare-vs-base265.json`.
