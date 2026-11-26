# Answer evaluation reports

This folder is the record of one question and the work done to answer it:

> Did the "answer" tool improve substantially the results for an AI agent
> tasked with a common set of ten software development instructions that one
> might encounter, including architectural, writing, documentation, testing,
> and coding.

Each report covers one series: a set of changes and the test of them. A
report says what was changed, what was tested, what was good, what was bad
and what happens next. It is written for someone who was not there, and it
can be read without the plan, the ledger or the code open.

The method is in the [Arbiter plan](../final-plan-fp16.md): the scenarios,
the five results per scenario, the two rubrics and how weights are adjusted.

## Reports, in order

| # | Report | What it covers | Outcome |
| --- | --- | --- | --- |
| 00 | [Setup](00-setup.md) | The private evaluation service (commit `1fe7b600` in a test image), the airk and bookbuilder indexes with the benchmark's own files denied and the leak probe passing, an indexer fix that cut semantic indexing time by 45% with identical contents, the frozen [scenarios](scenarios.md) and [rubrics](rubrics.md), the harness, the user's change to Haiku agents and Sonnet judges, the GPU and training tracks, and a four-scenario pilot. It is committed with `01-baseline.md`. | Ready for the baseline. The pilot is not evidence about Answer: it ran while the indexes were still being written, and 16 of its 17 Answer calls with a model stage failed. It produced coverage rule 2 and the settled-index rule |
| 01 | [Baseline](01-baseline.md) | Answer, Parse and Search as they stood at commit `1fe7b600`, on airk's ten scenarios with Haiku agents, two Sonnet judges and a Sonnet reference arm; bookbuilder history from the original index; the bookbuilder-v2 baseline on the fresh index; the 100-hit depth measurement; the search roster fix. Results in [results/01-baseline.csv](results/01-baseline.csv) | Headroom 10 of 10 on airk and 10 of 10 on bookbuilder-v2. On airk no Answer configuration finished correctly; Search finished 2 (C1, T1, grade 1). On bookbuilder-v2 no path finished any scenario. Answer covered 12 of 53 essential notes against Search's 22 on airk, and 15 of 61 against 26 on bookbuilder-v2. Depth: 35 of 53 notes have no file in the pool, so `search_v2` enters change set 2; bookbuilder-v2's depth equals the original index's, which confirms the roster fix |
| 02 | [Change set 1](02-change-set-1.md) | The eight changes of plan section 5.3 in a private evaluation build (image `cs1e-1a24414d`, layers `all,pool_full=8`): software-task follow-ups, spread, a 16-snippet first search, a 16-wide Focus with the software instruction, a provisional Context Read rule, grouped presentation and a device budget; replay tuning and leave-one-outs; the airk pass with Search and Parse reused; three bugs fixed during the series and two recorded digest-input changes. Results in [results/02-change-set-1.csv](results/02-change-set-1.csv) | No new correct finish on airk; calls slower (Compact up to 146.6 s). First call 4 of 53 notes against 2; only the larger first search changed it. Focus 13 and Compact 14 of 53 over the session, Search 22. GPU replays and the Context leverage result are pending. Next: `search_v2`, pending its depth data |
| 03 | [Change set 2](03-change-set-2.md) | `search_v2` (a shared search for prose instructions, so Search and Parse ran again), Context option B (a wider roster that takes the best Read even over a terminal card) and Focus candidate 2, on top of change set 1's layers (image `cs2-9d1b9aea`); airk and bookbuilder-v2, 52 results each, against each repository's baseline; first-call replays; a replay of the recorded Search queries under the old and the new search; the run-plan naming error and the L38 unclean close with its one-time restart exception. Results in [results/03-change-set-2.csv](results/03-change-set-2.csv) | No correct finish on either repository, so 0 substantial wins in 20, and Search lost its two airk finishes. The first call covers 4 of 53 (airk) and 8 of 61 (bookbuilder) notes, against 2 and 4 to 5. The airk Search drop (22 to 15 notes) was session variance: the same queries find 15 against 13 (baseline queries) and 7 against 6 (this pass's) under `search_v2`, with A1 and C2 each losing a note. Calls slower (Compact 120.7 s, Large 165.2 s). Next: change set 3, keeping `search_v2` |
| 04 | [Gate LG](04-gate-lg.md), with its generated [detail report](04-gate-lg-detail.md) | The first series on the Answer loop (plan section 12), series `06-gate-lg`: Focus with Compact, Focus with Large and Focus only on one CUDA GPU (product commit `de0dde02`, image `lg-gate-076cc17d`), ten munarium and ten bookbuilder scenarios, Haiku agents and two Sonnet judges; the two model gates of plan sections 12.7.1 and 12.7.2. Parse and Search did not run | Neither bar met. Large won 0 and lost 1 against Compact and is dropped. Compact won 1 and lost 1 against Focus only. 0 correct finishes in 60 sessions; the first reply held 17 to 21 of 113 essential notes in every arm. The loop ran with no failed read. The precision check did not run |

| 05 | [Windows CUDA loop baseline](05-loop-baseline.md) | Twenty development scenarios on Bookbuilder and Munarium with Focus, Arbiter, both, Search and Parse. [Results](results/05-loop-baseline.csv) | Zero correct finishes and zero substantial wins. First-reply evidence improved on Munarium. Search led session coverage on Bookbuilder. |
| 06 | [Windows CUDA change set 1](06-loop-change-set-1.md) | Structural evidence before closing a need and wider first Answer retrieval. [Results](results/06-loop-change-set-1.csv) | First-call yield rose and mean Answer tool time fell across all three arms. Zero correct finishes and zero substantial wins. |

The newest report is the current standing. The final report, when it exists,
answers the question above for each Answer configuration.

**Reports 01 to 03 are history, not a baseline.** They tested the one-turn
Answer on airk and bookbuilder. On 22 November 2026 the user stopped that
series during its third change set and made Answer a local multi-round loop
(plan section 12), and replaced airk by munarium as the second repository.
Report 04 is the loop's model gate. After it the evaluation starts over on
the loop with a new baseline against Parse and Search (plan step L7).

## How the reports are named

| File | Written after |
| --- | --- |
| `00-setup.md` | The service is built, the two indexes exist with the benchmark's own files excluded and the leak probe passing, the scenarios and rubrics are frozen and the pilot has run |
| `01-baseline.md` | The baseline pass on the code as it stood before any change. It also reports the headroom: on how many scenarios a substantial win is possible at all |
| `02-change-set-1.md`, `03-change-set-2.md` | Each change set of the one-turn series and the pass that tested it. The third was stopped and has no report |
| `04-gate-lg.md`, `04-gate-lg-detail.md` | The loop's model gates. The first is written by hand in the form below; the second is generated by the harness (`gate_report.py`) and lists every scenario side by side |
| The next numbers, in order | The loop's new baseline, then each change set and the pass that tested it |
| After them | The sealed scenarios, run once on the configuration named beforehand, on the user's word; then the final report: the answer to the question, and what is recommended for delivery |

**One commit per series.** When a series is judged, everything it produced
lands in one commit: the change set's code (for the baseline, the harness,
scenarios and rubrics), the report, its results file and this README's new
row. Nothing of a series is committed earlier, so any two series commits can
be compared directly, and checking one out gives the code its results came
from. The setup report goes into the baseline's commit.

Three change sets run back to back, and then work stops for the user's
manual inspection. The user may set more or fewer further iterations at each
inspection; they take the next report numbers, and the sealed set and final
report move after them.

A report is not rewritten after its commit. If something in it proves wrong,
a dated correction is added at its top, in the next series' commit, and the
later report explains it.

Beside the reports:

- `scenarios.md` holds the scenario instructions and their kinds, once
  frozen.
- `results/` holds one results file per series, with the same columns every
  time: each scenario's grades from each judge, the adjudicated grade, turns
  to a correct finish, first-call yield, bytes and time. It holds grades,
  counts and timings, not evidence text.
- `rubrics.md` holds the two rubrics, the definition of a correct finish,
  the table of cases that are not clear-cut and the rule for "substantially
  improves", as frozen before the first judged run. It also holds the
  sufficiency standard that decides Gather Context results from
  26 November 2026.

## What every report contains

The same sections in the same order, so two reports can be compared quickly.

1. **Summary.** What was changed, what was tested and the result, in a few
   sentences a reader can stop after.
2. **What changed in this series.** Each change, the problem it was meant to
   fix, the effect expected, and whether it was kept or dropped after the
   test. The baseline report says "nothing" here.
3. **What was tested.** The Answer configurations, the devices, the source
   commit, the index revisions and the scenario set.
4. **Results.** Three parts, each with every scenario listed.
   - *Against Parse and Search.* For each Answer configuration: correct
     finishes, turns to a correct finish, substantial wins, ties and losses,
     and the two rubric scores, overall, for each repository and for each
     kind of task. A correct finish means the evidence established every
     essential fact and the agent's brief got those facts right.
   - *What each model adds.* On the same scenario rows: what Answer with no
     model does against Parse and Search, what Focus adds to that, what
     Compact adds to Focus, and what Large adds to Compact. For each step,
     the correct finishes added, the turns saved and the time added.
   - *Reuse.* Which results reused an earlier session because the agent
     would have received identical evidence, and which were run again.
5. **What was good.** What worked, with the scenarios that show it.
6. **What was bad.** What did not work, what got worse, what failed or timed
   out, and any change that was made and then dropped. Nothing is left out
   because it is unflattering.
7. **Why.** The causes behind sections 5 and 6, from the turn logs.
8. **What happens next.** The next change set and the reason for each part
   of it, or the reason to stop.
9. **Limits and receipts.** What the result does not show, where the private
   receipts are, and the commands that reproduce it.

## What stays out of these reports

Raw tool output and the agents' session logs stay in the private campaign
folder, which is not committed. The reports quote none of the bookbuilder
or munarium repositories' source. They name their scenarios and may cite
their file paths; judges' reasons on those scenarios are withheld, because
they paraphrase the source.
For this repository, short excerpts are quoted where they make a point
clearer.
