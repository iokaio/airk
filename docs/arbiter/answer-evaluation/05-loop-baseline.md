# Windows CUDA Answer loop baseline

## Summary

The first Windows CUDA loop baseline judged 100 sessions on Bookbuilder
and Munarium. All five paths finished correctly in zero scenarios.
No Answer configuration earned a substantial win. Search gathered more
essential evidence on Bookbuilder, and Focus matched Search's session
coverage on Munarium. Every Answer model configuration averaged more
tool turns than Search in both repositories.

## What changed

The baseline established the measurement harness and exercised the existing
Answer behavior. The harness pins the measured runtime, rejects crossed
or incomplete grading receipts, freezes the eligible second-sample roster
and keeps sample matching by judged result identity.

## What was tested

The Windows CUDA baseline used the 20 development scenarios, ten each on
Bookbuilder and Munarium. Each scenario had five first-sample sessions.
The three Answer configurations enabled Focus, Arbiter, or both. The
Search and Parse controls had both optional models off. The four-mode
shakedown also checked Answer with neither model. The shakedown and
separate retrieval probes are outside the judged baseline.

Haiku 5.5 ran the agent sessions. Two independent Sonnet 5.5 judges
assessed each session. All ran at medium effort. The harness froze the
prompts, work budget, scenarios, model identities and index publications
before the first measured session. The sealed scenarios stayed closed.

The private Windows service was built from `1f7a800f` with the CUDA score
settlement correction. It used the same Focus 1024 and Context Compact
FP16 model weights as the gate. The Arbiter profile was re-emitted to
match the corrected executable’s math policy, and its strict CUDA score
vectors matched the retained reference at all three tested lengths.
The Bookbuilder source revision was `238b22a722ce` and the Munarium
revision was `9948345d1d14`. The fixture passed 30 leak probes before
scoring. Bookbuilder and Munarium kept their settled publications
throughout the first sample.

Four separate ordinary hybrid Search requests confirmed that Focus can
rerank eligible hits on an enrolled index with Arbiter disabled. The control results below belong
to the deliberately model-free Search settings. [Focus in Search](../user-guide.md#focus-in-search)

## Results

### Correct finishes and grades

All five paths finished correctly in zero of 20 scenarios. Correct
finish turns are unavailable for those sessions. Search was the stronger
control in 13 cases and Parse in seven. The stronger control left a
theoretical substantial win available in all 20. Each Answer configuration
recorded zero substantial wins, zero ties on correct finishes, and zero
substantial-rule losses. All 20 comparisons per arm belong to the rubric's
neither-finished case. The 12-win bar and five-wins-per-repository floor
were missed for every Answer configuration. No configuration was eligible
for a second sample. All 100 grade pairs agreed and 92.4% of the 817 judged
fact pairs agreed.

Each cell gives the adjudicated Rubric A grade and essential span coverage
over the session. The stronger-control column follows the approved tie
rule. Full per-result grades, turns, reply bytes and timings are recorded
in the [results CSV](results/05-loop-baseline.csv).

| Scenario | Task | Stronger control | Search | Parse | Focus | Focus + Arbiter | Arbiter |
| --- | --- | --- | --- | --- | --- | --- | --- |
| bookbuilder-A1 | architecture | Search | 0 / 2 of 8 | 0 / 1 of 8 | 0 / 1 of 8 | 0 / 0 of 8 | 0 / 0 of 8 |
| bookbuilder-A2 | architecture | Search | 0 / 1 of 6 | 0 / 0 of 6 | 0 / 0 of 6 | 0 / 0 of 6 | 0 / 0 of 6 |
| bookbuilder-A3 | architecture | Search | 0 / 2 of 6 | 0 / 0 of 6 | 0 / 2 of 6 | 0 / 3 of 6 | 0 / 2 of 6 |
| bookbuilder-C1 | coding | Search | 0 / 3 of 7 | 0 / 3 of 7 | 0 / 1 of 7 | 0 / 1 of 7 | 0 / 5 of 7 |
| bookbuilder-C2 | coding | Parse | 0 / 1 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 2 of 5 |
| bookbuilder-C3 | coding | Parse | 0 / 3 of 6 | 0 / 3 of 6 | 0 / 2 of 6 | 0 / 1 of 6 | 0 / 1 of 6 |
| bookbuilder-T1 | test | Search | 0 / 6 of 6 | 0 / 3 of 6 | 0 / 1 of 6 | 0 / 1 of 6 | 0 / 2 of 6 |
| bookbuilder-T2 | test | Search | 0 / 3 of 6 | 0 / 0 of 6 | 0 / 0 of 6 | 0 / 1 of 6 | 0 / 1 of 6 |
| bookbuilder-W1 | writing | Search | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 3 of 5 |
| bookbuilder-W2 | writing | Parse | 0 / 3 of 6 | 0 / 3 of 6 | 0 / 2 of 6 | 0 / 1 of 6 | 0 / 1 of 6 |
| munarium-A1 | architecture | Search | 0 / 3 of 5 | 0 / 1 of 5 | 0 / 3 of 5 | 0 / 3 of 5 | 0 / 3 of 5 |
| munarium-A2 | architecture | Parse | 0 / 3 of 5 | 0 / 3 of 5 | 0 / 2 of 5 | 0 / 3 of 5 | 0 / 2 of 5 |
| munarium-A3 | architecture | Parse | 0 / 1 of 6 | 0 / 3 of 6 | 0 / 2 of 6 | 0 / 3 of 6 | 0 / 2 of 6 |
| munarium-C1 | coding | Search | 0 / 3 of 6 | 0 / 2 of 6 | 0 / 3 of 6 | 0 / 4 of 6 | 0 / 3 of 6 |
| munarium-C2 | coding | Search | 1 / 3 of 4 | 0 / 2 of 4 | 1 / 3 of 4 | 0 / 2 of 4 | 0 / 2 of 4 |
| munarium-C3 | coding | Search | 0 / 2 of 4 | 0 / 1 of 4 | 0 / 2 of 4 | 0 / 2 of 4 | 0 / 3 of 4 |
| munarium-T1 | test | Search | 1 / 4 of 5 | 0 / 3 of 5 | 0 / 4 of 5 | 0 / 4 of 5 | 0 / 4 of 5 |
| munarium-T2 | test | Search | 0 / 3 of 6 | 0 / 0 of 6 | 0 / 1 of 6 | 0 / 0 of 6 | 0 / 0 of 6 |
| munarium-W1 | writing | Parse | 0 / 4 of 6 | 0 / 4 of 6 | 0 / 5 of 6 | 0 / 1 of 6 | 0 / 3 of 6 |
| munarium-W2 | writing | Parse | 0 / 1 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 2 of 5 | 0 / 2 of 5 |

Each cell below gives mean Rubric A / mean Rubric B. All of these groups
had zero correct finishes and, for Answer, zero substantial wins. Against
the stronger comparator per case, the combined and Arbiter-only arms had
lower mean Rubric A in coding and tests. Focus had a lower mean in tests.

| Scope | Search | Parse | Focus | Focus + Arbiter | Arbiter |
| --- | ---: | ---: | ---: | ---: | ---: |
| All 20 | 0.100 / 1.931 | 0 / 0.013 | 0.050 / 1.469 | 0 / 1.512 | 0 / 1.550 |
| Bookbuilder | 0 / 1.738 | 0 / 0 | 0 / 1.375 | 0 / 1.425 | 0 / 1.413 |
| Munarium | 0.200 / 2.125 | 0 / 0.025 | 0.100 / 1.562 | 0 / 1.600 | 0 / 1.688 |
| Architecture | 0 / 2.062 | 0 / 0.042 | 0 / 1.417 | 0 / 1.354 | 0 / 1.458 |
| Coding | 0.167 / 1.792 | 0 / 0 | 0.167 / 1.458 | 0 / 1.562 | 0 / 1.562 |
| Tests | 0.250 / 1.875 | 0 / 0 | 0 / 1.531 | 0 / 1.562 | 0 / 1.656 |
| Writing | 0 / 2.000 | 0 / 0 | 0 / 1.500 | 0 / 1.625 | 0 / 1.562 |

On paired Rubric A grades, adding Arbiter to Focus tied 19 scenarios and
lost one, Munarium C2. Adding Focus to Arbiter tied all 20. There were zero
correct finishes added and no turn savings to a correct finish. In the
same paired sessions, adding Arbiter to Focus changed mean tool time by
+2.29 s on Bookbuilder and −2.57 s on Munarium. Adding Focus to Arbiter
changed it by +1.14 s and −2.75 s. The presentation score rose from 1.469
to 1.512 when Arbiter joined Focus, and fell from 1.550 to 1.512 when Focus
joined Arbiter. These are whole-session comparisons with matching
scenarios, including the agents' own call choices. All sessions were
fresh under one fixture. No scored session was reused.

### Evidence returned to agents

Each cell contains ten first-sample sessions. First and session counts
use the same essential span-coverage rule. Mean turns count tool turns,
including tool calls after the first reply.

| Repository | Path | Essential in first reply | Essential by session end | Mean tool turns |
| --- | --- | ---: | ---: | ---: |
| Bookbuilder | Search | 5 / 61 | 26 / 61 | 8.9 |
| Bookbuilder | Parse | 0 / 61 | 17 / 61 | 8.0 |
| Bookbuilder | Focus | 5 / 61 | 13 / 61 | 10.5 |
| Bookbuilder | Focus + Arbiter | 6 / 61 | 12 / 61 | 9.5 |
| Bookbuilder | Arbiter | 6 / 61 | 17 / 61 | 10.0 |
| Munarium | Search | 9 / 52 | 27 / 52 | 7.5 |
| Munarium | Parse | 0 / 52 | 21 / 52 | 6.1 |
| Munarium | Focus | 13 / 52 | 27 / 52 | 10.2 |
| Munarium | Focus + Arbiter | 13 / 52 | 24 / 52 | 10.4 |
| Munarium | Arbiter | 13 / 52 | 24 / 52 | 10.1 |

All three Answer configurations delivered more essential spans than
model-free Search in the first reply on Munarium. Over the session,
Focus matched Search on Munarium and the other Answer configurations
fell three notes short. On Bookbuilder, Search delivered nine to
fourteen more essential spans over the session than any Answer
configuration. Agents in all six Answer cells averaged more tool turns
than the Search agents for the same repository.

The separate model-free Search depth probe returned covering spans for
30 of 113 essential notes. On Bookbuilder, 2 were at ranks 1–6, 4 at
7–16, 5 at 17 or deeper, and 50 had no covering span in the returned
hits. The corresponding Munarium counts were 8, 2, 9 and 33. Among
notes without a covering span, hits did reach the same path for 11
Bookbuilder notes and 23 Munarium notes. The probe used the scenario
instruction as its query and the service’s reply caps. Those ranks
describe its returned hits. Answer’s actual query variants, retained
candidates, and delivered span boundaries are the next measurements.

### Reply and session cost

The totals below use the ten first samples for each repository and
configuration. Rereads are later tool replies that overlap a prior
Answer row. Refusals are wrapper rejections. Failed calls, typed retries
and refusals are separate observations. Times are means per session.

| Repository | Path | Supporting spans | Reply bytes | Rereads of truncated / whole rows | Failed / called | Refusals | Typed retries | Agent wall s | Tool s |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Bookbuilder | Search | 5 / 29 | 1,171,194 | 0 / 0 | 1 / 89 | 1 | 4 | 36.80 | 14.86 |
| Bookbuilder | Parse | 5 / 29 | 2,229,894 | 0 / 0 | 12 / 80 | 0 | 3 | 42.36 | 13.99 |
| Bookbuilder | Focus | 4 / 29 | 2,065,518 | 13 / 13 | 8 / 105 | 3 | 0 | 56.25 | 31.62 |
| Bookbuilder | Focus + Arbiter | 6 / 29 | 1,892,220 | 9 / 2 | 7 / 95 | 0 | 0 | 58.34 | 33.91 |
| Bookbuilder | Arbiter | 6 / 29 | 1,942,993 | 9 / 10 | 7 / 100 | 1 | 0 | 59.14 | 32.77 |
| Munarium | Search | 14 / 48 | 1,007,775 | 0 / 0 | 0 / 75 | 0 | 0 | 23.78 | 3.85 |
| Munarium | Parse | 10 / 48 | 1,913,944 | 0 / 0 | 5 / 61 | 0 | 0 | 25.14 | 2.48 |
| Munarium | Focus | 7 / 48 | 1,974,396 | 22 / 13 | 6 / 102 | 2 | 0 | 35.27 | 8.58 |
| Munarium | Focus + Arbiter | 11 / 48 | 2,001,261 | 30 / 14 | 5 / 104 | 2 | 0 | 33.20 | 6.01 |
| Munarium | Arbiter | 11 / 48 | 2,053,515 | 23 / 12 | 3 / 101 | 0 | 0 | 34.56 | 8.76 |

The scored agents received 98 Answer replies and no Answer tickets or
deadline partials. The native decision log contains 72 model and eight
deterministic decisions for Bookbuilder Arbiter, 81 and four for
Bookbuilder Focus + Arbiter, 62 and five for Munarium Arbiter, and 68 and
four for Munarium Focus + Arbiter. Focus-only rounds had 144 deterministic
decisions on Bookbuilder and 123 on Munarium. The Focus rerank work is
reported separately in the native dispatch counts. These reply logs
recorded zero fallback or undispatched Context decisions.

### Native model cost

The table gives mean CUDA inference milliseconds per successful native
model dispatch, grouped by the actual model task and index scope. The
counts cover the scored cohort windows, including their canonical
calls. An inference duration excludes admission queue time, compilation
and reply verification. Each of the 971 admitted model dispatches had
a successful score return and a job settlement. The control cohort
admitted zero optional model dispatches.

| Repository | Answer arm | Focus rank | Arbiter split | Arbiter plan | Arbiter coverage | Arbiter follow-up |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Bookbuilder | Focus | 129.83 (256) | — | — | — | — |
| Munarium | Focus | 124.50 (189) | — | — | — | — |
| Bookbuilder | Focus + Arbiter | 122.94 (144) | 225.34 (14) | 94.76 (35) | 214.31 (29) | 222.49 (3) |
| Munarium | Focus + Arbiter | 117.93 (104) | 171.20 (10) | 105.42 (25) | 232.54 (25) | 271.88 (3) |
| Bookbuilder | Arbiter | — | 235.81 (15) | 94.98 (33) | 334.21 (24) | — |
| Munarium | Arbiter | — | 208.29 (12) | 106.49 (29) | 262.69 (21) | — |

Separate representative Answer probes sampled peak device-wide CUDA
memory. The readings include the rest of the live service and GPU.
Each repository and configuration used one request, so these samples
describe those probes and do not estimate each cohort’s high-water mark.

| Repository | Answer arm | GPU peak used (MiB) | Answer elapsed (ms) | GPU samples |
| --- | --- | ---: | ---: | ---: |
| Bookbuilder | Focus | 3,052 | 1,378 | 5 |
| Munarium | Focus | 3,119 | 379 | 3 |
| Bookbuilder | Focus + Arbiter | 8,716 | 1,476 | 5 |
| Munarium | Focus + Arbiter | 8,785 | 399 | 3 |
| Bookbuilder | Arbiter | 7,250 | 1,629 | 5 |
| Munarium | Arbiter | 7,250 | 445 | 3 |

## What worked

The native CUDA settlement receipts closed every observed scored model
dispatch. The first-reply evidence lead on Munarium was shared by all
three Answer configurations. On Bookbuilder C1, Arbiter recovered five
of seven essential spans over the session, two above the stronger control.

## What fell short

Bookbuilder did not convert the first reply into broad session coverage.
Munarium T1 shows why evidence counts and correct finishes are separate.
All three Answer configurations reached four of five essential spans and
all received Rubric A grade zero. Bookbuilder T1 Search returned all six
essential spans, and its brief did not meet the correct-finish rubric.

## Why the first reply fell short

The 60 measured canonical Answer replies omitted zero complete records at
the reply frame. The projected rows still showed truncation. Over the
agent sessions, 106 later reads overlapped a previously truncated
Answer row. Those reads recovered four essential notes absent from
the first reply. The overlap establishes where the agent reread.

Focus alone ran 47 internal follow-ups across the first canonical
replies. Focus + Arbiter ran four and Arbiter ran none. An all-covered
stop coincided with a missing scenario essential in one Focus case,
14 combined cases, and 13 Arbiter-only cases. This associates the
stop rule with the missing evidence across those sessions. Exact
sub-need queries and returned spans are needed to locate the first
failure within each case.

## Next measurement

Use the actual Answer initial search arguments and their ranked results
to distinguish recall from retention. Compare follow-up choices around
covered stops. Preserve the full locations and check whether an extra
read actually recovers an essential span before carrying it into the
first judged change set.

## Limits and receipts

All 100 first-sample sessions completed. The controls session process
took 368.9 seconds, Focus 345.9 seconds, Focus + Arbiter 322.7 seconds,
and Arbiter 340.3 seconds. These are wall durations for cohorts with
parallel workers.

A separate combined-arm canonical replay produced empty stdout. The
actual first Answer in that measured session used the same exact
canonical arguments and verified model arm, index and budget. Its
unchanged result supplied the canonical evidence for that case.
The empty replay and recovery proof remain in the private receipts.

The combined launcher’s global Claude configuration audit caught a
metadata change during service warm-up. It preceded the start of
the agent-session process. The writer remains unobserved. A separate
owner and policy audit passed for all 26 entries changed under the
approved Claude configuration directory. The Arbiter-only cohort
passed the original metadata guard. Its global config timestamp and
size matched at entry and at child exit.


The baseline uses Windows CUDA and the 20 development scenarios. The
private R3 receipts retain the prompts, exact source and model
identities, publications and tool payloads. Coverage, judging and strict
grading ran through the [evaluation harness](../../../scripts/arbiter-answer-eval/README.md).
The 200 verdicts took 578.9 seconds in the judging step. The final
strict grade used the frozen empty second-job roster and existing
verdicts. It launched no additional judge sessions.

| Private receipt | SHA-256 |
| --- | --- |
| Frozen spec | `26a20f71f782ed9390f23ac229fceec0d9891684fd21084cf1409dfd21e445ad` |
| Prompt freeze | `2ec1a20cecec49f42b33f6e04abcdb0c17d9f569bb08005b6398261392087385` |
| Final grade summary | `7ffcffb2ea284f159573172690cd2e4c1e46d55567ed52b46909f55743618e3b` |
| Private adjudicated results | `32eaa53d6337f52f47bcee5fd478ebc4792fe416e55c2af4b5291cd07966d093` |
| Frozen second-sample roster | `4c77cd2af0832460f9ab2d834bd5e7d878ad591f58fec7ec00181523cfd86154` |
| Recovered canonical evidence | `d1a957a656069cff1f4d954e7a829cb716a4b3b76316e14257af00ea02b40114` |
| Combined-stage config boundary | `6a2ec87ce6bd9100dbde75af4ad47bb9923b6a5c98b66889b9ff08393707a541` |

The public CSV preserves the evaluation columns and leaves the private
agent session identifiers blank.
