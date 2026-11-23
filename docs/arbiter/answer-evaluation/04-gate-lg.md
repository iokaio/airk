# 04 Gate LG: Large against Compact, and Compact against Focus only, on the Answer loop

Written on 23 November 2026 (UTC) by the primary agent from the private
receipts of series `06-gate-lg` (lifetimes ready at 11:04 UTC, report figures
built at 11:49 UTC). It goes into one commit with the evaluation harness for
the loop, the generated [detail report](04-gate-lg-detail.md) and
[results/04-gate-lg.csv](results/04-gate-lg.csv).

This is the first report on the Answer loop. Reports 01 to 03 tested the
earlier one-turn Answer on airk and bookbuilder; their results are history
and not a baseline. This gate does not compare Answer with Parse and Search.
That comparison is the new baseline of plan step L7.

> **Neither bar was met.** Large did not beat Compact (it won on no scenario
> and lost on one), so Large is dropped. Compact did not beat Focus only by
> the proposed bar (one win, one loss). **No session in any arm finished
> correctly:** 0 of 60. The first Answer reply held 17 to 21 of the 113
> essential reference notes, and the three arms differ by less than the
> scenarios themselves do. What limits the result is what the loop retrieves,
> not which model judges it.

## 1. Summary

The gate asked two questions set by the user on 22 November (plan sections
12.7.1 and 12.7.2): is the Large Context model worth keeping over Compact,
and does Compact earn its place in the loop over Focus alone. Three arms ran
the same loop on the same build, indexes and scenarios, on one GPU: Focus
with Compact (C), Focus with Large (L) and Focus only (F). Each arm gave one
first reply per scenario (20 replays) and one judged agent session per
scenario (20 sessions, two judges each), on ten munarium and ten bookbuilder
scenarios.

| | C: Focus + Compact | L: Focus + Large | F: Focus only |
| --- | --- | --- | --- |
| Correct finishes, of 20 sessions | 0 | 0 | 0 |
| Sessions graded above 0 on Rubric A | 1 | 0 | 1 |
| Essential notes covered over the session, of 113 | 39 | 37 | 39 |
| on bookbuilder, of 61 | 10 | 15 | 14 |
| on munarium, of 52 | 29 | 22 | 25 |
| Essential notes in the first reply, of 113 | 21 | 20 | 17 |
| on bookbuilder, of 61 | 7 | 6 | 5 |
| on munarium, of 52 | 14 | 14 | 12 |
| First reply, median time | 8.3 s | 11.7 s | 9.5 s |
| First reply, median size | 84 KB | 91 KB | 82 KB |
| Tool calls per session, median (limit 12) | 10 | 11 | 9.5 |

- **Large against Compact (12.7.1): bar not met.** Large added no correct
  finish, won Rubric A on 0 scenarios and lost on 1, covered one essential
  note fewer on bookbuilder and the same number on munarium, and took 1.4
  times Compact's time. Large is dropped, as the user directed in advance on
  23 November and as the bar indicates.
- **Compact against Focus only (12.7.2): bar not met.** Compact added no
  correct finish and won and lost one scenario each. Its first replies held 4
  more essential notes (21 against 17) and came 1.2 s sooner at the median.
  The bar indicates shipping the loop with Focus only. Nothing is removed on
  this result: by the user's direction of 23 November, Compact stays as
  "Arbiter" and Focus and Arbiter become separate switches. Which
  combination is the default is the user's to decide at the stop after L7.
- **The loop ran cleanly.** 60 replays and 103 Answers inside sessions had no
  failed read, no partial result and no ticket, and every decision due to a
  model was taken by it.

## 2. What changed in this series

Everything since report 03 is new to the evaluation. The changes are in
earlier commits on the branch; this commit adds the harness that tested
them.

| Change | Problem it was meant to fix | Kept |
| --- | --- | --- |
| Answer became a local multi-round loop (plan section 12, steps L1 to L5): Split, then per part Plan, Retrieve, Read and check coverage, Follow up and Compose, then Union | One turn of retrieval could not gather the facts a task needs, so an agent kept reading after the Answer | Yes |
| Compact and Large score the loop's four card decisions as FP16 on CUDA; Focus scores candidate pairs in batches on CUDA | The loop needs many model calls inside one Answer, at GPU speed | Compact and Focus yes; Large dropped by this gate |
| munarium replaced airk as the second repository (user, 22 November); ten new scenarios with reviewed reference notes | An index of airk could answer questions about the experiment itself | Yes |
| The duplicate rule: a repeated or overlapping span is delivered once (`5f971a70`) | Repeats wasted the reply's space | Yes |
| The Answer token pool raised to 380,000 (`d1378ee5`) | The earlier pool cut model calls in later rounds | Yes |
| Nine defects found by the shakedown on a real service and fixed before the gate (`ffa08c04`, `69033135`, `1b5882ee`, `dd6b6b88`, `cad8f5b9`, `07d0a51c`, `24c506db`, `de0dde02`) | Loop profiles refused by Core and the service; reads losing the caller's scopes; Focus accounting; Focus batches capped at 4; card calls refused by a 120 s limit; the verification receipt and the reply sized for a 64 KiB channel; line-window reads on documents | Yes |
| The harness for the loop (this commit): fixture lifetimes on CUDA, the gate plan, replays, the arm identity check, coverage rule 3, the gate report builder | The one-turn harness could not read an Answer /3 reply | Yes |

Not in the gate image, on their own branches: reproducible record
identities, the revised Answer jobs and the Workbench move to Answer /3.

## 3. What was tested

- **Build.** Product source at commit `de0dde02`, hosted as the service's
  test binary (image `lg-gate-076cc17d`), with the harness at branch commit
  `c3b077f6`.
- **Device.** One NVIDIA RTX A6000 (48 GB) on Windows, CUDA only. The three
  arms' services ran side by side on it. No CPU cell and no Linux run.
- **Models in the loop.** Focus: the CUDA entry with a 1,024-token window.
  Compact and Large: FP16 on CUDA, profiles emitted at `dd6b6b88`. Card
  decisions were taken by top score, the gate's mode: no calibration is
  fitted yet, so the model's highest-scoring option stands.
- **Budget, every Answer.** 380,000 input tokens, 300 s, 3 rounds, up to 4
  parts and 4 sub-needs, the product's default response mode and source
  verification.
- **Indexes.** munarium at its commit `9948345d` and bookbuilder-v2 at
  `238b22a7`, each indexed from a `git archive` export of that commit.
  Neither repository was written to.
- **Scenarios.** Ten munarium and ten bookbuilder development scenarios,
  frozen (SHA-256 `26338d09…9190e`), with 52 and 61 essential reference
  notes. The ten sealed bookbuilder scenarios did not run.
- **Agents and judges.** Headless Claude Code: agents on Haiku
  (`claude-haiku-5-5`), two judges per session on Sonnet
  (`claude-sonnet-5-5`), all at medium effort. An agent had Answer, read,
  context, references, symbols and outline, and at most 12 tool calls. The
  harness issued each session's first call itself, the scenario's instruction
  as written, so a session's first reply and the replay are the same
  request.
- **Coverage rule.** A reference note counts as covered when the evidence
  overlaps at least min(5, the note's line count) of its lines, or names the
  note's symbol as a whole identifier where the evidence has no line span.

## 4. Results

Every scenario is listed in the [detail report](04-gate-lg-detail.md), side by
side for each pair of arms, with the locations each reply returned and each
judge's verdict per note. The judges' reasons are withheld there because they
paraphrase private source.

### 4.1 Against Parse and Search

Not run in this gate. The loop's baseline (plan step L7) runs Parse and
Search on the same scenarios.

### 4.2 What each model adds

| Step | Correct finishes added | Rubric A won / lost / tied | First-reply essential notes | Median time of the first reply |
| --- | --- | --- | --- | --- |
| Compact added to Focus (C against F) | 0 | 1 / 1 / 18 | 21 against 17 | 8.3 s against 9.5 s |
| Large in place of Compact (L against C) | 0 | 0 / 1 / 19 | 20 against 21 | 11.7 s against 8.3 s |

The two judges gave different grades on 2 of 60 sessions. No pair of sessions
was a close call, so no second session ran.

**Decisions.** In arm C the model took 95 of 100 decisions and in arm L 114
of 124; the others are recorded as taken without a model call. None was
left undispatched and none fell back. Arm F took its 152 decisions by the
deterministic rules. Compact and Large chose the same split on 17 of 20
scenarios, the same sub-needs on 8 and the same coverage verdicts on 5; the
two replies held identical evidence on 2.

**Rounds.** Compact ended 18 of 20 Answers after the first round, Large 8,
and Focus only 4 (it used all three rounds on 10).

**Speed and memory.** All Answers completed. The slowest first reply took
24.8 s (Large). With two Answers at once the arms served 11.3 (C), 12.3 (F)
and 9.5 (L) Answers a minute; with four at once, 7.8, 8.1 and 8.5. The three
services together peaked at 27.4 GiB of GPU memory. Warm-up took 8 to 10 s
for Focus, 10.2 s for Compact and 17.7 s for Large.

### 4.3 Reuse

None. Every result is from its own session.

## 5. What was good

- **The loop works end to end on a real service.** 163 Answers across the
  replays and sessions, in three arms on two repositories, with no failed
  read, no partial result, no ticket and no decision left without its model.
  Before the shakedown of 22 and 23 November, no Answer had completed with a
  model's decisions standing.
- **It is fast enough to use.** A median first reply in 8 to 12 s, and four
  at once without a refusal.
- **The arms were what they claimed to be.** The identity check passed on
  every reply: the model took decisions in every C and L reply and in no F
  reply.
- **The duplicate rule earns its place.** A median of 8 to 11 repeated spans
  per reply were delivered once instead of twice.
- **Compact costs nothing in time.** It was faster than Focus only at the
  median, because it stops earlier (see section 7).

## 6. What was bad

- **No session finished correctly, in any arm.** Sessions still used a
  median of 10 tool calls of the 12 allowed, the first Answer included, and
  ended with about a third of the essential notes (37 to 39 of 113). The
  gate shows no saved turn.
- **The first reply holds little of what the task needs:** 17 to 21 of 113
  essential notes, and on bookbuilder 5 to 7 of 61. On six of the twenty
  scenarios no arm's first reply held any essential note.
- **The loop believes it is done when it is not.** Compact marked 32 of its
  41 sub-needs covered and 1 missing, in replies that held 21 of 113
  essential notes.
- **Every reply delivered cut rows:** a median of 18 to 22 rows per reply
  marked truncated, of a median of 52 to 65. About a fifth to a quarter of
  the agents' later reads went back to a row the Answer had already
  delivered cut (38 of 157 for C, 32 of 158 for F, 40 of 164 for L).
- **About half of the agents' later reads went to places the reply did not
  hold** (68, 78 and 76 of those reads).
- **Replies are large for what they carry:** a median of 82 to 91 KB with
  roughly one essential note per 90 KB. The source verification receipt
  takes 20 to 29 KB of most replies.
- **Large is slower and heavier for no gain:** 1.4 times Compact's time,
  15.3 GB against 6.7 GB at its profile's peak, one scenario lost.
- **The gate's services did not close cleanly.** Each ended on a limit of
  the evaluation fixture (1,024 native dispatches per lifetime), which a
  gate-length lifetime exceeds. Every reply was rechecked afterwards and no
  result is affected; the three roots cannot be reused.
- **The precision check (12.7.3) did not run.** The FP32 profiles declare
  only the one-turn task, so an FP32 arm would have compared nothing.
- **Seventeen tool calls inside sessions were refused** as invalid requests
  written by the agents, and each cost a turn: 8 reads past the end of a
  file, 5 reference lookups (4 with a name that was not one identifier, 1
  with an unknown field) and 4 Answers (3 with an unknown field such as a
  budget figure at the top level, 1 with verification limits while
  verification was off).

## 7. Why

These are readings of the figures above. Where one was not measured
separately it says so.

- **Retrieval limits the result, not the judging model.** All three arms
  search the same index with the same search, and their first replies cover
  nearly the same notes: on 15 of 20 scenarios the three arms' first-reply
  counts are equal. Changing the model changes which sub-needs are asked
  (identical evidence on only 2 of 20 between C and L) but not whether the
  files the task turns on are found. On bookbuilder, documents and Markdown
  outrank the source files the notes cite; on two scenarios checked before
  the gate, none of the cited source files appeared in the reply at all.
- **Top-score decisions end the loop early.** With no calibration, the
  coverage decision takes the model's best-scoring option, and for Compact
  that was "covered" nearly every time: 18 of 20 Answers stopped after one
  round. This is why Compact is faster than Focus only.
- **More rounds did not find more.** Focus only ran three rounds on half the
  scenarios and still held fewer essential notes than Compact (17 against
  21). Compact's small lead comes with its splitting the instruction into
  two or more parts on 18 scenarios, where Focus only split 4; whether the
  split is the cause was not measured separately.
- **Rows are cut by the per-row limit, not by the reply's size.** No reply
  left out a record or a hit for space, yet every reply had cut rows.
- **For orientation only, from history:** on the same ten bookbuilder
  scenarios, report 03's one-turn sessions covered 10 (Compact), 14 (Focus),
  18 (Parse) and 27 (Search) of 61 essential notes, under an earlier
  coverage rule. The loop's sessions covered 10 (C), 14 (F) and 15 (L). The
  loop has not yet moved this figure, and the old Search covered more.

## 8. What happens next

The user gave the decisions for after this gate on 23 November, so work does
not stop here. The next stop is after L7.

1. **Large is removed** with evidence, the remaining Context model is called
   Arbiter, and Focus and Arbiter become separate switches for Answer:
   neither, Focus only, Arbiter only, or both (plan step LH).
2. **Linux CUDA** for the loop (plan step LX), then the workspace gate (L6).
3. **L7 on Windows and Linux CUDA:** a new baseline with Parse and Search on
   the same scenarios, then three change sets. This gate points them at, in
   order:
   - what the first search returns, source files against documents first;
   - the coverage decision, which needs calibration before it may end a
     loop;
   - cut rows: the two experiments of plan section 12.10 (choosing the
     lines inside a span, and spending the reply's space by rank);
   - the size of the verification receipt;
   - requests an agent gets refused for (a read past the end of a file
     could return the lines that exist).
4. **Before another gate-length lifetime,** the fixture's dispatch limit is
   raised or scaled.
5. **The precision check** runs after L7, when FP32 profiles carry the
   loop's tasks.

## 9. Limits and receipts

- One judged session per scenario and arm. A margin of one or two scenarios
  is within what a second sample can change; the gap between any arm and a
  correct finish is not.
- First-reply figures come from one replay per scenario and arm. Run-to-run
  variation of the loop was not measured in this series.
- One GPU, Windows, CUDA. Peak memory is for the whole device with three
  services side by side. Concurrency was measured without the job queue, a
  different scenario's question in each call.
- Agents ran on Haiku at medium effort with at most 12 tool calls. A
  stronger agent or a longer session may finish where these did not.
- The reference notes decide what counts as covered. They were written and
  reviewed in the private campaign folder and are not in this repository.
- Top-score decisions are the gate's mode, not the product's. A calibrated
  model may behave differently.
- Cost: 0.93 US dollars for the 60 agent sessions and 24.42 for the judges.

Receipts are in the private campaign folder under `passes/06-gate-lg/`
(figures in `gate-report.json`, the plan and its log, the arms' replays, the
agreement and outcome files, `performance.json`) and
`sessions/06-gate-lg/`; the detail report lists their SHA-256 digests. The
commands that reproduce the series are in the
[harness README](../../../scripts/arbiter-answer-eval/README.md), "The loop:
gates (LG) and the new baseline (L7)".
