# Windows CUDA Answer change set 1

## Summary

Wider initial Answer retrieval and a structural coverage guard
increased early evidence on Windows CUDA. In the controlled combined
replay, first replies recovered eight net essential spans. The
100 judged sessions showed higher first-call yield and lower
average tool time for every Answer arm. The series ended with
zero correct finishes and zero substantial wins.


## What changed

The [Windows baseline](05-loop-baseline.md) found that many Arbiter-backed
Answers had stopped as covered while missing essential evidence. The
first change requires a native definition, caller or governing-contract
witness before Arbiter can close those structural needs. The existing
follow-up path can continue within the caller’s original budget while
that evidence is missing.

The second change widens Answer’s first part-level Search request from
16 to 32 hits and its hit-text cap from 12,288 to 24,576 bytes. It keeps
the separate sub-need recipes, retention accounting, reply disclosure
and verification. The [ordinary Search behavior](../user-guide.md#focus-in-search)
is exercised in its own control lane.

The fixture startup can observe an already complete Windows Ready file
within 1.5 seconds after a missed notification. The original readiness
deadline, STOP and child-exit checks remain enforced. The report harness
joins each repeat with its own grade and timings. The original scenario,
agent and judge prompts, model choices, rubric and scoring budget were
verified against the baseline freeze.

## What was tested

The scored Windows CUDA pass uses the twenty development scenarios
on Bookbuilder revision `238b22a722ce` and Munarium revision
`9948345d1d14`. Haiku 5.5 agents and two independent Sonnet 5.5 judges
use medium effort. The three Answer configurations are Focus only,
Arbiter only, and Focus with Arbiter. Search and Parse use fresh
sessions with Focus and Arbiter disabled and CUDA MiniLM still
active. The control receipts use this candidate's publications.

The private combined service SHA-256 is
`aa1a8481e8755318ad56b5b153823ee8000ccdddab0c85d718c84e5440700f60`.
The scoring specification SHA-256 is
`2e75b0e847b22d71836a5007f1e540b08500bb871d04d867abf8e75bfdf03fb9`.
Every native replay uses three rounds and the same bounded Answer
request, including its 300-second deadline, 132 read calls and
262,144-byte reply cap. The candidate's first startup exposed a CPU
fallback when the build omitted the private MiniLM CUDA admission.
That fixture produced no canonical Answer comparisons or scored
sessions. The corrected image was rebuilt, and the native embedding
status confirmed CUDA with no fallback after both indexes settled.


## Why these changes were selected

The baseline returned adequate first-reply evidence for 19 of 113
essentials in the combined arm. The retained-argument depth diagnostic
then reran 78 first-round Search records from its 20 first replies,
using the same published indexes with both optional stages disabled.
The two parts completed 253 requests, with duplicate calls reused within each part. Across both repositories,
none of the matched-bound replays exposed an extra missed essential.
The widened results exposed 23 missed essentials. Nine had ranks up
to 32 on a part-level query and four more reached that rank only on
sub-need queries. Seventy-one misses remained outside the observed
returned spans, including 35 for which a result reached the right
path. The tested ranks are bounded public Search ranks for retained
queries. This measure has no visibility into the pre-fusion pool.

| Repository | Essentials in baseline first Answer | Missed, exposed by widening | Within rank 32 on a part query | Missed in both tested bounds |
| --- | ---: | ---: | ---: | ---: |
| Bookbuilder | 6 / 61 | 9 | 3 | 46 |
| Munarium | 13 / 52 | 14 | 6 | 25 |

The combined baseline stopped every part as covered with an essential
miss in 14 of 20 scenarios. The Arbiter-only baseline did so in 13.
The decision receipts include model-approved caller and contract needs
whose cited records offered only Search hits. CS1 tests a structural
gate for those decisions, retaining the record of available evidence
and allowing the usual bounded follow-up.

## Structural-only replay

A separate CUDA image applied the structural stop requirement with
the original Answer lookup limits. Both pinned publications settled
after native CUDA embedding was observed. Both repositories passed
model-sourced smoke tests. All 20 canonical combined-arm replays
qualified without model-placement flags or publication changes.

| Repository | Baseline / structural first essentials | Baseline / structural all-covered stops with an essential miss | Baseline / structural follow-ups | Baseline / structural median observed verification interval |
| --- | ---: | ---: | ---: | ---: |
| Bookbuilder | 6 / 6 of 61 | 6 / 5 | 2 / 3 | 576 / 525 ms |
| Munarium | 13 / 13 of 52 | 8 / 5 | 2 / 8 | 116 / 105 ms |

The guard made the loop pursue seven more follow-ups and reduced all-covered
stops with a missing essential by four cases. The first reply still
covered 19 essential spans. The structural run checked 160 sources and
1,151 references. The baseline checked 144 sources and 1,108 references.
The verifier’s interval is the time between its recorded observation
bounds. Baseline canonical requests ran alongside agent work, while
the structural test ran as a standalone replay. Timing comparisons
between the two have that concurrency difference. Both sets had zero
omitted reply-frame records and zero deadline partials.

The private structural replay receipt SHA-256 is
`483366baae8dcec4c1841bb31e7badfdc38645c6e34797baeaba1fb8a9fee7c3`.

## Combined replay

The full candidate passed the leak check and both live model-sourced
smoke cases. The 20 matched combined-arm requests used the same
scenario and request budgets as the structural-only replay. Both
fresh publications remained settled. Every case qualified without
placement flags. Every reply retained its 32-hit part-level request
in the evidence record.

| Repository | Structural / combined first essentials | Newly covered | Previously covered and now missing | Structural / combined median source verification | Structural / combined median native time |
| --- | ---: | ---: | ---: | ---: | ---: |
| Bookbuilder | 6 / 9 of 61 | 3 | 0 | 525 / 502.5 ms | 9.774 / 8.107 s |
| Munarium | 13 / 18 of 52 | 6 | 1 | 105 / 104.5 ms | 3.438 / 3.557 s |

The wider read recovered the nine exact essential spans predicted by
the part-level rank probe. Total first-reply coverage reached 27 of
113, up from 19 in both the baseline and structural-only replay.
The combined candidate kept the structural run’s 11 follow-ups and
10 all-covered stops with an essential miss. Source verification
observed 1,666 references across 150 checked sources, against 1,151
references across 160 checked sources in the structural replay.
Both replay sets had zero frame-omitted records and zero partial
replies. These are serial native replay measurements of the two
bundles on separate authenticated publications. Correct finishes and
agent turns are measured in the judged pass.

The private combined replay receipt SHA-256 is
`eebd01d3d53af6106f25c68b7bf3daee016c8572725b2292e8f20c56839102c2`.

## Ordinary Search with Focus enabled

Two live hybrid Search calls used the same CUDA Focus profile, with
Arbiter off. Each returned ten hits. Focus scored four, and the six
unscored hits retained their native order on both Bookbuilder and
Munarium. The calls succeeded without retries and left the
publications unchanged. The corresponding stage-off Search control
measures retrieval with CUDA embeddings and neither optional model.

The private Search smoke receipt SHA-256 is
`3fca1aacc7d14d2c61b4e604c1765987fee64240bd9a94c78d106e55ebbd5649`.

## Model placement and replay memory

All three Answer configurations retained the settled publications and passed
the native model-placement checks in every serial replay. GPU
memory is device-wide usage sampled during each replay batch.

| Answer configuration | Qualified | First essentials, Bookbuilder | First essentials, Munarium | Batch elapsed | Peak GPU memory | Samples |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Focus only | 20 / 20 | 6 / 61 | 18 / 52 | 158.5 s | 3,947 MiB | 282 |
| Arbiter only | 20 / 20 | 9 / 61 | 19 / 52 | 78.8 s | 7,801 MiB | 140 |
| Focus + Arbiter | 20 / 20 | 9 / 61 | 18 / 52 | 130.0 s | 10,105 MiB | 231 |

Every replay returned within its budget with zero partial replies.

The private Focus-only replay receipt SHA-256 is
`49d51698b2a8c58b54c4b3ee7b8a5291a339db778f70105ada7bc12bcb7c033f`.
The Arbiter-only replay receipt SHA-256 is
`67cc372dfaedff14012e7f50f3a0d57b682d2b1852a1046a108f2e4c28df40d3`.

## First-sample results

All five paths completed 20 sessions each. Every path had zero
correct finishes. Each Answer arm had zero substantial wins against
the stronger of Search and Parse. The rubric left room for a
substantial win in all 20 scenarios, so the result missed both the
12-win threshold and the five-win floor in each repository.
Architecture averaged below the stronger control for every Answer
arm. Focus alone also averaged below the stronger control for
coding. The combined arm improved on the stronger control in
test-task grade, with means of 0.25 and 0.

| Configuration | Baseline / CS1 first-call essential yield | Baseline / CS1 session essentials | CS1 mean agent turns | Baseline / CS1 mean tool time |
| --- | ---: | ---: | ---: | ---: |
| Search | 13.55 / 13.55% | 53 / 55 of 113 | 8.65 | 9.351 / 10.306 s |
| Parse | 0 / 0% | 38 / 33 of 113 | 6.00 | 8.238 / 7.269 s |
| Focus only | 16.88 / 22.46% | 40 / 39 of 113 | 10.15 | 20.098 / 16.709 s |
| Arbiter only | 18.55 / 26.63% | 41 / 44 of 113 | 9.85 | 20.765 / 17.693 s |
| Focus + Arbiter | 18.55 / 25.80% | 36 / 37 of 113 | 9.70 | 19.961 / 16.098 s |

First-call yield is the mean scenario proportion. Session counts
deduplicate essential spans across each agent's calls. The wider
first read improved first-call yield in every Answer arm. The effect
attenuated over the agent's session. Arbiter-only gained three
essential spans and the combined arm gained one. Focus-only lost one.
Search still led final coverage on both repositories. Each Answer
arm averaged more tool turns than Search and Parse.

Arbiter-only reached 17 of 61 essentials on Bookbuilder and 27 of 52
on Munarium, the strongest final coverage of the three Answer arms.
Search led with 25 and 30. The corresponding combined totals were
12 and 25, and Focus alone reached 14 and 25. Support coverage was
6 of 29 and 11 of 48 for Arbiter, 6 and 13 for the combined arm,
and 7 and 11 for Focus. Search covered 7 and 15, and Parse covered
6 and 11.

Native score observation recorded 842 successful returns with
matching settlements across the first samples. The controls added
no optional-model dispatch. None of the 85 observed final Answer
replies disclosed a deadline partial. The Answer arms recorded 137 truncated and 71
whole evidence rereads. Search and Parse used fresh
control runs on the candidate publications. Every observed answer
or failure remains in the retained receipts.

### Results by scenario

Search was the stronger control in 14 cases and Parse in
6. The two judges agreed on 98 of 100 grades.
No case needed manual adjudication or qualified for a second sample.
Each cell shows the adjudicated grade and the number of essential
spans gathered over the session. The full timings and grades are in
the [results CSV](results/06-loop-change-set-1.csv).

| Scenario | Task | Stronger control | Search | Parse | Focus | Focus + Arbiter | Arbiter |
| --- | --- | --- | --- | --- | --- | --- | --- |
| bookbuilder-A1 | architecture | Parse | 0 / 0 of 8 | 0 / 0 of 8 | 0 / 1 of 8 | 0 / 0 of 8 | 0 / 0 of 8 |
| bookbuilder-A2 | architecture | Parse | 0 / 1 of 6 | 0 / 3 of 6 | 0 / 0 of 6 | 0 / 1 of 6 | 0 / 0 of 6 |
| bookbuilder-A3 | architecture | Search | 0 / 4 of 6 | 0 / 2 of 6 | 0 / 3 of 6 | 0 / 1 of 6 | 0 / 1 of 6 |
| bookbuilder-C1 | coding | Search | 0 / 4 of 7 | 0 / 0 of 7 | 0 / 3 of 7 | 0 / 1 of 7 | 0 / 2 of 7 |
| bookbuilder-C2 | coding | Parse | 0 / 0 of 5 | 0 / 0 of 5 | 0 / 2 of 5 | 0 / 0 of 5 | 0 / 0 of 5 |
| bookbuilder-C3 | coding | Parse | 0 / 2 of 6 | 0 / 2 of 6 | 0 / 0 of 6 | 0 / 2 of 6 | 0 / 4 of 6 |
| bookbuilder-T1 | test | Search | 0 / 6 of 6 | 0 / 3 of 6 | 0 / 2 of 6 | 0 / 3 of 6 | 0 / 4 of 6 |
| bookbuilder-T2 | test | Search | 0 / 3 of 6 | 0 / 1 of 6 | 0 / 0 of 6 | 0 / 0 of 6 | 0 / 1 of 6 |
| bookbuilder-W1 | writing | Search | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 3 of 5 | 0 / 2 of 5 |
| bookbuilder-W2 | writing | Search | 0 / 3 of 6 | 0 / 2 of 6 | 0 / 1 of 6 | 0 / 1 of 6 | 0 / 3 of 6 |
| munarium-A1 | architecture | Search | 1 / 4 of 5 | 0 / 0 of 5 | 0 / 1 of 5 | 0 / 1 of 5 | 0 / 2 of 5 |
| munarium-A2 | architecture | Search | 0 / 3 of 5 | 0 / 3 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 3 of 5 |
| munarium-A3 | architecture | Search | 0 / 1 of 6 | 0 / 0 of 6 | 0 / 2 of 6 | 0 / 2 of 6 | 0 / 2 of 6 |
| munarium-C1 | coding | Search | 0 / 4 of 6 | 0 / 2 of 6 | 0 / 3 of 6 | 0 / 2 of 6 | 0 / 3 of 6 |
| munarium-C2 | coding | Search | 1 / 3 of 4 | 0 / 2 of 4 | 0 / 2 of 4 | 1 / 3 of 4 | 1 / 3 of 4 |
| munarium-C3 | coding | Parse | 0 / 1 of 4 | 0 / 2 of 4 | 0 / 2 of 4 | 0 / 3 of 4 | 0 / 3 of 4 |
| munarium-T1 | test | Search | 0 / 4 of 5 | 0 / 3 of 5 | 0 / 5 of 5 | 1 / 4 of 5 | 0 / 4 of 5 |
| munarium-T2 | test | Search | 0 / 2 of 6 | 0 / 0 of 6 | 0 / 1 of 6 | 0 / 2 of 6 | 0 / 1 of 6 |
| munarium-W1 | writing | Parse | 1 / 5 of 6 | 0 / 5 of 6 | 0 / 5 of 6 | 0 / 4 of 6 | 0 / 4 of 6 |
| munarium-W2 | writing | Search | 0 / 3 of 5 | 0 / 1 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 2 of 5 |

### Native scoring time

These are per-dispatch means from all successful scored-session
returns. The native scorer does not separately time downstream
decision policy here. Serial replay memory is reported above.

| Task | Dispatches | Queue | Compile | Inference |
| --- | ---: | ---: | ---: | ---: |
| Focus rank | 594 | 20.3 ms | 0.7 ms | 122.4 ms |
| Arbiter split | 46 | 10.1 ms | 2.7 ms | 255.1 ms |
| Arbiter plan | 108 | 24.8 ms | 2.7 ms | 97.4 ms |
| Arbiter coverage | 77 | 17.0 ms | 7.4 ms | 268.1 ms |
| Arbiter follow-up | 17 | 24.4 ms | 13.7 ms | 250.0 ms |

### Remaining losses

The combined arm increased the mean Rubric A grade from 0 to 0.10.
The two grade-1 cases were Munarium C2 and T1. Arbiter alone reached
grade 1 on Munarium C2. Search also reached grade 1 there, as well
as Munarium W1 and A1. Mean Rubric B was 1.525 for Focus-only,
1.631 for Arbiter-only, and 1.575 for both. The stronger comparators
averaged 1.350 on Rubric B. Better presentation and earlier
retrieval did not yet produce a correct finish.

The next bundle measures where Focus reorders away useful evidence
and where actual Arbiter choices lead to weaker subsequent reads.
It compares the original Focus prompt with the software-evidence
instruction on identical inputs and measures calibrated decisions
on real development rosters before proposing a new configuration.

## Limits and receipts

Judges agreed on 92.99% of 756 paired fact assessments. The final
reconciliation used the first 200 verdicts after the approved
second-sample roster came back empty. The strict summary SHA-256
is `27ace7f317efde98796379fa68d63909937bd9e7e3fe5e1e1cfd0ad7c76a75aa`.
The public CSV SHA-256 is
`c11188766a855d44b5c487abe6ad1feabd09c8200d1546f005ea5e674b0f87c0`.
The private campaign series is `08-windows-cuda-cs1-r2`. The plan
ran its four session cohorts, then the existing coverage, judge
and strict grade stages of `run_pass.py`. The final grade stage
used the pinned specification and froze the same 20 development
inputs as the baseline.

Both optional stages were disabled before successful managed
teardown. The service lease was reacquired, and the native
recorder closed without losses or writer failure. The fixture
owner receipt SHA-256 is
`4572d6fcd6f3a3a18cbd9fc6c8726326e6b30c1b13e20684a6dcd6271446b383`.

These results cover the exact Windows CUDA fixture and frozen
development set. No model weights were fitted in this series.
The Munarium sealed inputs were prepared separately after the CS1
packet was frozen, before the next model-fitting work. No sealed
benchmark or other platform ran for this series.
