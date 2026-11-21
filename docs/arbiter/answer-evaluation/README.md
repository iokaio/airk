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
| — | None yet | The plan's readings were approved on 21 November 2026, and the same day the user set the agent (Sonnet) and judge (Opus) models, one commit per series and three iterations before a stop. Execution has not started; it waits for the user's word. The first commit holds `00-setup.md` and `01-baseline.md`. | — |

The newest report is the current standing. The final report, when it exists,
answers the question above for each Answer configuration.

## How the reports are named

| File | Written after |
| --- | --- |
| `00-setup.md` | The service is built, the two indexes exist with the benchmark's own files excluded and the leak probe passing, the scenarios and rubrics are frozen and the pilot has run |
| `01-baseline.md` | The baseline pass on the code as it stood before any change. It also reports the headroom: on how many scenarios a substantial win is possible at all |
| `02-change-set-1.md`, `03-change-set-2.md`, `04-change-set-3.md` | Each change set and the pass that tested it |
| `05-sealed-set.md` | The sealed scenarios, run once on the configuration named beforehand |
| `06-final-report.md` | The answer to the question, and what is recommended for delivery |

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
  improves", as frozen before the first judged run.

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
repository's source. They name its scenarios and may cite its file paths.
For this repository, short excerpts are quoted where they make a point
clearer.
