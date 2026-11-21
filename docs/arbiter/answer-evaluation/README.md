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
| — | None yet | The work has not started. The plan awaits the user's read. | — |

The newest report is the current standing. The final report, when it exists,
answers the question above for each Answer configuration.

## How the reports are named

| File | Written after |
| --- | --- |
| `00-setup.md` | The service is built, the two indexes exist, the scenarios and rubrics are frozen and the pilot has run |
| `01-baseline.md` | The baseline pass on the code as it stood before any change |
| `02-change-set-1.md`, `03-change-set-2.md`, `04-change-set-3.md` | Each change set and the pass that tested it |
| `05-sealed-set.md` | The sealed scenarios, run once on the final configurations |
| `06-final-report.md` | The answer to the question, and what is recommended for delivery |

A report is committed when its series is judged, before the next series
starts. A report is not rewritten later. If something in it proves wrong, a
dated correction is added at its top and the later report explains it.

Beside the reports:

- `scenarios.md` holds the scenario instructions and their kinds, once
  frozen.
- `rubrics.md` holds the two rubrics and the rule for "substantially
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
4. **Results.** Turns to sufficient context and the two rubric scores, for
   each configuration against Parse and Search: overall, for each
   repository, for each kind of task and for each scenario.
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
