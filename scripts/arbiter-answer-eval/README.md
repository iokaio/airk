# Answer evaluation harness

Evaluation tooling, never product. It runs the agent sessions, coverage,
judging and grading of the Answer evaluation, as set out in
[`docs/arbiter/final-plan-fp16.md`](../../docs/arbiter/final-plan-fp16.md)
(sections 3.3 to 3.6, 4 and 5.2). Reports go in
[`docs/arbiter/answer-evaluation/`](../../docs/arbiter/answer-evaluation/README.md).

**The repository pair.** Since 22 November 2026 (user direction) the
evaluation runs on **munarium** and **bookbuilder**, so the indexes under test
hold nothing about the experiment:

- No lifetime of the loop series holds an airk index. `leak_check.py` and
  `fixture.py leak-probe` fail when one exists.
- `common.REPOS` is the default pair, `("munarium", "bookbuilder")`, and every
  default that names repositories uses it (`run_pass.py`, `run_plan.py`,
  `make_loop_spec.py`, `gate.py`, `replay.py`, `settled.py`, `depth.py`,
  `probe_tools.py`, `search_replay.py`).
- `common.KNOWN_REPOS` also holds `airk`, so the one-turn history of 21 and 22
  November 2026 and the tools that read it still load (`mcp_wrapper.py --repo`,
  `make_cs_spec.py --repo`, `common.load_scenario`).
- The bookbuilder scenarios are served by the `bookbuilder-v2` index through
  the fixture root's `index-aliases.json`; `common.LOOP_INDEXES` names the two
  indexes a loop lifetime holds, in the launcher's order (`bookbuilder-v2`
  first, because it has denies; see [FIXTURE.md](FIXTURE.md)).
- Both repositories are private in reports: locations and note ids only, with
  judges' reasons withheld entirely, verdicts per note id kept
  (`gate_report.PRIVATE_REPOS`; a reason with its quotes removed still
  paraphrased the project, 23 November 2026).

The private evaluation service, its indexes and `handle.json` belong to
`fixture.py` ([FIXTURE.md](FIXTURE.md)). The harness only reads a handle. It
never starts or stops a fixture.

Python 3.13, standard library only. Run every script as
`python -I -B scripts/arbiter-answer-eval/<script>.py`, as the verified human.

## Components

| File | Purpose |
| --- | --- |
| `mcp_wrapper.py` | MCP stdio server for one session. It offers the path's tools only, injects the index id, enforces the first-call rule and the 12-turn cap, and logs every call |
| `run_session.py` | One headless agent session (Claude Code; default `claude-haiku-5-5`, `--effort medium`) in a fresh empty folder, with the wrapper as its only MCP server, followed by the audit |
| `coverage.py` | The formats.md coverage rule over a turn log: notes covered per turn, first-call yield, the turn at which every essential note is covered, bytes and tool time |
| `judge.py` | A blind judging packet per result, and a headless judge (default `claude-sonnet-5-5`, `--effort medium`, no tools) returning JSON. It supports two independent judges (`j1`, `j2`) |
| `grade.py` | Rubric A grades, the case table, the substantial-win and configuration rules, headroom and the per-step model comparisons. Writes `results.csv` and `summary.json` |
| `replay.py` | Re-runs recorded first (or all) Answer calls, or a path's canonical first call, against a live handle. Gives coverage and the evidence digest used for session reuse |
| `run_pass.py` | Runs a pass: sessions with bounded parallelism (default 4), reuse by digest, coverage, judging and grading. It can resume |
| `evidence.py` | Locations and evidence text in tool results, the coverage rule, the evidence digest and the citation audit |
| `common.py` | Handles (readiness, enabled stages), the tools catalog, one CLI process per tool call, scenarios and Claude Code process helpers |
| `tool_names.py` | The one mapping between native (`airk_search`), operation (`search`, the pre-1.9 public name) and 1.9 public (`search_passages`) tool names, read from the Rust-emitted `fixtures/split/catalog/public_tool_names.json`. The harness calls tools by native name, which every image accepts; `public(name, names="legacy")` gives an older image's public names |
| `pass_digests.py` | Computes a pass's canonical first-call digests for named results while their stage configuration is live, and prints which results agree |
| `cuda_pair.py` | Inputs of a CUDA pair lifetime (up to Compact, Large and Focus) (`gpu_fixture.py --model pair`): copied, authenticated asset directories (the fixture refuses hard links) and the pinned roster, review pins and profile rows. See [GPU_FIXTURE.md](GPU_FIXTURE.md) |
| `lg_lifetimes.py` | The LG `--lifetimes` file: one fixture root, pinned pair and measured memory per arm C, L and F (side by side). With `--base` it checks that the base lifetime closed cleanly, is restartable and holds exactly the pair's indexes. See [GPU_FIXTURE.md](GPU_FIXTURE.md) |
| `lg_bases.py` | Watches, probes and summarizes the CPU base lifetimes of the LG arms (one fixture root per arm): `wait` on the lifetimes' own handles, `probe` (settled guard, index counts, leak check, credential probes, the launcher's leak probe) and `summary`. It starts and stops nothing |
| `pass_report.py` | Per-session and per-path figures of a graded pass (turns, bytes, tool and wall time, grades, agent and judge cost) for reports and cost projections |
| `settled.py` | The settled-index guard: semantic readiness `ready`, served mode `hybrid`, no index or vectors job queued or running, and the publication token at start and end (read-only) |
| `leak_check.py` | The leak check of a lifetime, the same for every repository: each scenario's `leak_probe` phrase searched on its own repository's index, every indexed path listed, both checked against that index's exclusion list (empty for munarium, and recorded as such), the credential-like names, and no airk index in the handle, the state files or the service's catalog |
| `credential_probe.py` | Lists every indexed path of one index and names the credential-like ones (`.env*`, `*secret*`, `appsettings*.json`, `*.pfx`, `set-*-key.ps1`, `aikeys.json`), opening no file |
| `prompts_freeze.py` | Writes and verifies `R/design/prompts-freeze.json`: the SHA-256 and text of every agent prompt, the judge prompt and its schema, the default models and efforts, and the hashes of the runtime harness files |
| `depth.py` | The no-agent depth measurement (plan E4). For each essential note it gives the rank at which an `airk_search` of the instruction (limit 100, the tool's maximum; no paging) first covers it |
| `rescore.py` | Re-scores a graded pass under another coverage rule. It writes `coverage-rule<N>.json`, `results-rule<N>.csv` and `summary-rule<N>.json`, and leaves the original results alone |
| `rejudge.py` | Re-judges a pass's identical packets with another judge model and effort, then compares grades, per-fact verdicts and Rubric B with the original verdicts |
| `probe_tools.py` | One real call per tool through the wrapper against a live handle, with the spans `evidence.py` finds in each result |
| `mock_cli.py`, `synthetic.py` | A stand-in CLI serving canned synthetic results, and the invented fixture (the "lantern" repository) used by the tests and the smoke run |
| `loop_reply.py` | Answer /3 (the loop) replies: decisions keyed by stage, part, sub-need and round, sub-needs and their coverage, records, tickets, the reply summary, and the gate arms' identity check |
| `gate.py` | The LG gate tests (plan 12.7): first-reply replays per arm (tests 1 and 2), decision agreement, judged outcomes between two arms (test 3), concurrency and performance (test 4), and the FP32 against FP16 precision check with its escalation rule |
| `gate_report.py` | The gate report for the user's personal review: bars and recommendations, agreement, evidence, outcomes, a per-scenario side by side, precision, performance and receipts |
| `make_loop_spec.py` | The LG series spec and run plan (arms F, C, L and the precision arms), the L7 baseline spec and plan, `l7_plan_template.json`, and shakedown specs for the four switch combinations (arms N, F, A, C; L for LG history) |
| `smoke_summary.py` | One summary row per Answer of `gate.py smoke` receipts (decision sources, Focus and card calls, records and failures, degradations, model work) |
| `lifetime_probe.py` | Read-only probes of a ready lifetime: `airk_help` for a topic, `arbiter status-legacy` and `arbiter status`, and `airk_status` for every index |

## Paths and tools

| Path (`--path`) | Tools offered, in order | First call |
| --- | --- | --- |
| `answer`, `noModel`, `focus`, `compact`, `large` | `airk_answer`, `airk_read_file`, `airk_context`, `airk_references`, `airk_symbols`, `airk_outline` | `airk_answer`, `question` = the instruction |
| `search` | `airk_search` and the same five reads | `airk_search`, `query` = the instruction |
| `parse` | `airk_parse` | `airk_parse`, `text` = the instruction |

The Answer configurations differ in the service configuration, not in their
tools.

- **Schemas.** These come from `tools-catalog.json` (the CLI's
  `tools list --json`). A 1.9 image lists the renamed public names
  (`gather_context`, `search_passages` and the rest); `common.load_catalog`
  keys every listing by native name through `tool_names`, so pre-1.9 and 1.9
  catalogs load alike. The wrapper removes `index` and `project` from every
  schema. It also removes `schema` and `budget` from `airk_answer`, and
  `schema` from `airk_parse`.
- **`airk_answer`.** The wrapper always sets `schema` and `budget`. Since the
  loop (plan section 12) the schema is `airk.arbiter.context.request/3` and
  the budget is the contract's whole default /3 budget. The harness does not
  hold that budget: `common.ANSWER_BUDGET_DEFAULT` is read at import from the
  Rust-emitted cases file, `fixtures/split/arbiter/answer_loop_cases.json`
  (`default_budget`), in the tree the harness runs from. At `69033135` it is
  300 s, 262,144 bytes, 4 parts, 4 sub-needs, 3 rounds, 132 calls, fanout 8,
  132 candidates, 380,000 input tokens, 32,768 a round, 48 card calls, 24
  Focus calls, 192 Focus pairs; a test fails when this sentence, the plan
  template or any harness file drifts from the cases file. The one-turn
  evaluation's /2 request and budget stay as `ANSWER_SCHEMA_V2` and
  `ANSWER_BUDGET_DEFAULT_V2`, for history only. `--answer-defaults` can change
  budget fields, or add other fields, for a configuration.
- **The budget sent is recorded and frozen.** A copied 131,072-token default
  outlived the product's move to 196,608 on 22 November 2026 and would have
  taken the model away from every gate Answer after round 2 or 3. Since then:
  the prompts freeze pins the cases file's SHA-256 and the budget read from it
  (`answer_budget`), and `--verify` refuses a tree whose default differs;
  each replay row carries `budget_sent`; `replay.json`, a session's
  `session.json` (`answer_budgets_sent`, from the wrapper's turn log) and an
  arm's `arm-replays.json` (`budgets_sent`) list the distinct budgets sent
  with their call counts; and the gate report states the budget, or names the
  arms that were sent different ones.
- **Answer tickets.** A /3 reply queued at admission carries a `ticket`. The
  wrapper, `replay.py` and `gate.py` wait its `retry_after_ms` (at most 30 s a
  wait, at most 16 times) and attach with `continuation` set to the ticket id,
  like a typed retry, so the agent sees the Answer and not the queue. Each
  ticket reply is kept beside the turn and logged as a retry with code
  `ticket`; the time queued counts in the turn's `elapsed_ms`.
- **`airk_parse`.** It defaults to `mode: execute`, which the agent may
  override.
- **The first-call rule.** If a call breaks it, uses a tool outside the path,
  or comes after turn 12, the agent gets a tool error. The call is logged with
  `"turn": null, "rejected": true` and is not counted as a turn.
- **The first call is the harness's (plan 3.5; from 23 November 2026).** The
  first counted call of a session is sent with exactly the arguments of
  `replay.canonical_first_call`: the instruction as written, the default
  budget read from the cases file, and the product's defaults for everything
  else (the default response mode, with `AIRK_MCP_RESPONSE_MODE` stripped as
  for every agent call, and default verification). Any other argument the
  agent put on that call is recorded (`arguments`, and
  `first_call.ignored_arguments` in `turns.jsonl`) and not sent. From the
  second call on, the agent's own arguments are sent. The replays send the
  same first call, so a session's first reply and the gate's replay of it are
  the same request. On 23 November an agent's first call chose
  `"verification": "off"`, while the replays used the default, so the two
  measured different renderings.

## Inputs

- A fixture handle, which must be in the `ready` state:
  `R/fixture-<series>-<arm>/<lifetime>/handle.json` for a loop lifetime, or
  `R/fixture/<lifetime>/handle.json` for the one-turn history root. The harness
  reads `cli`, `control`, `environment` (the client environment) and
  `indexes.<repo>.index_id`, and resolves a scenario repository through the
  root's `index-aliases.json` (`bookbuilder` is served by `bookbuilder-v2`).
- `R/fixture/tools-catalog.json`. This is the default `--catalog`, resolved as
  the handle's grandparent folder. The CLI lists tools without the `airk_`
  prefix (`answer`, `search`, ...). The harness aliases them to the prefixed
  names it uses everywhere, and `tools call` accepts both.
- **Pilot only.** `--accept-queryable` (spec `accept_queryable`) also accepts a
  handle marked `queryable: true` whose vectors are still partial. Every
  session records the handle's state, its enabled stages and the semantic
  progress (`R/fixture/progress.json`) at its start and end.
- **Enabled stages.** These come from `<lifetime>/stages.jsonl`, which
  `fixture.py stage` writes. A spec entry may declare
  `"stages": {"focus": <profile>, "context": <profile>}`. `run_pass.py` and
  `pass_digests.py` then refuse to run that entry when the live stages differ,
  so a noModel run with Focus enabled is impossible.
- Scenarios in `R/scenarios/<repo>/<slot>.json` (formats.md), where `<repo>`
  is `munarium` or `bookbuilder`. The `airk` scenarios are history.
- For `run_pass.py`, a pass spec (JSON) naming the series, the scenarios and
  one entry per result: configuration, path, device, handle and, optionally,
  stages. The spec may also name a scenario subset (`scenario_ids`) and a
  `freeze` file. When it names one, every scenario must match its SHA-256.
  The layout is in the script's docstring.

`R` is
`artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1`, which is
git-ignored. Every output goes under it.

## Outputs

| Path | Content |
| --- | --- |
| `R/sessions/<series>/<repo>-<slot>/<result>[.s2]/` | `work/` (the agent's empty working folder), `mcp-config.json`, `instruction.txt`, `stream.jsonl` (stream-json transcript), `stderr.txt`, `brief.md`, `session.json`, `coverage.json`, and `wrapper/turns.jsonl` with `wrapper/results/turn-NN.stdout.json` (raw CLI output, SHA-256 in the log) |
| `R/passes/<series>/` | `digests.json` and `digests/` (canonical first-call replays), `reuse.json`, `pass-index.json`, `judging/<key>/j1|j2/` (`packet.json`, `output.json`, `verdict.json`), `results.csv`, `summary.json`, `pass-log.jsonl`, and the primary agent's optional `adjudications.json` |
| `--out` of `replay.py` | `replay.json` and the raw replayed results |

Every session and judge record keeps the following:

- the exact command line, with the stdin source
- the `claude --version` output and the executable path
- the model requested and the models the stream reports
- usage tokens and cost
- start and end times

The session record also holds the audit:

- **Discard.** The session is discarded if any tool outside the wrapper was
  offered or used, or if any MCP server other than the wrapper was present.
- **Citation audit.** Every `path:start-end` in the brief must overlap a
  location in a logged result.
- **Tool-call agreement.** The stream's tool calls must match the wrapper's
  log.

### `results.csv` columns (fixed)

`series, repo, slot, kind, configuration, device, session_id, reused_from,
judge1_grade, judge2_grade, adjudicated_grade, correct_finish,
turns_to_finish, first_call_yield, essential_covered, essential_total,
bytes, elapsed_ms, rubric_b_j1, rubric_b_j2, cost_usd`

`bytes` and `elapsed_ms` are what the tool calls returned and took over the
session. `cost_usd` is the agent session's cost. Judge costs are totalled in
`summary.json`.

## Use

```text
# one session
python -I -B scripts/arbiter-answer-eval/run_session.py --scenario R/scenarios/munarium/C1.json \
    --path focus --configuration focus --device gpu --handle R/fixture-<series>-F/<lifetime>/handle.json --series <series>
# coverage and judging of that session
python -I -B scripts/arbiter-answer-eval/coverage.py --scenario ... --session-dir ...
python -I -B scripts/arbiter-answer-eval/judge.py --scenario ... --session-dir ... --key munarium-C1/focus-gpu \
    --out-dir R/passes/<series>/judging/munarium-C1/focus-gpu
# a whole pass (resumable); --only follows fixture lifetimes
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec <spec.json> [--stage sessions --only focus-cpu]
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec <spec.json> --stage second   # close calls
# replay recorded Answer calls with a change set's defaults
python -I -B scripts/arbiter-answer-eval/replay.py --handle ... --sessions R/sessions/01-baseline --out R/replays/cs1-a
# leak check of a lifetime (every repository of the pair; fails on an airk index)
python -I -B scripts/arbiter-answer-eval/leak_check.py --handle R/fixture-<series>-<arm>/<lifetime>/handle.json \
    --out R/fixture-<series>-<arm>/leak-check.json
```

Use `--session-path compact` to restrict a recorded-session replay to
the combined Focus and Arbiter path. Combine it with
`--slots A1,A2,A3 --all-calls` to examine later Answer calls in the
architecture cases. The replay retains recorded arguments and reports
whether each reply matches its recorded evidence.

### Initial Search depth for Answer change set 1

Use a retained L7 pass and a settled, pinned live Search control with Focus
and Arbiter off. The control must serve the same index and publication token
as the first Answer reply.

```powershell
python -I -B scripts/arbiter-answer-eval/initial_search_depth.py --pass-dir $passDir --source canonical --handle $handle --handle-sha256 $handleSha --catalog $catalog --catalog-sha256 $catalogSha --repo bookbuilder --slot C1 --arm F --out $freshCheckedPrivateDir
```

Repeat `--repo`, `--slot` and `--arm` to select other retained rows. Use
`--source actual` for the verified first session tool turn. The diagnostic
sends only public Search reads. It preserves the retained request's query,
chunk mode and narrow limits while requesting a full transport response
and disabling saved-source verification. The widened replay changes only
the hit and byte limits, with a bounded step-down on the typed reply cap.

`initial-search-depth.json` lists note ranks and placements for the Answer
projection and the two Search observations. `queries.private.json` retains
exact request arguments and attempt hashes. Keep both in the checked
private output directory. The diagnostic covers only first-round Search
arguments still present in the Answer reply. Widening is a separate Search
request and cannot recover the internal pre-fusion pool or establish an
original Answer rank.

## The loop: gates (LG) and the new baseline (L7)

For a graded L7 pass, print the supplementary sample-one table with the
command below.

~~~powershell
python -I -B scripts/arbiter-answer-eval/baseline_summary.py --pass-dir <graded-pass-directory> --l7-metrics
~~~

Each repository/configuration row reports cumulative essential and supporting
coverage, reply text bytes, rereads of truncated and whole rows, failed tool
calls, wrapper refusals, typed retries, queue ticket attachments, deadline
partials disclosed by Answer, and native decision sources when present.
Mean wall and tool time use observed sessions. Totals include sessions reused
across passes. Dashes indicate unavailable observations. The final column
shows how many sessions supplied coverage, reread, and wrapper receipts.
The view prints only aggregate metrics and leaves all receipts unchanged.


Answer /3 is read by `loop_reply.py` against the Core contract
(`crates/airk-core/src/arbiter/CONTRACT.md`, "Answer loop wire contract
version 3"), the generated schema
(`packages/contracts-gen/arbiter.schema.json`) and the Rust-emitted cases
(`fixtures/split/arbiter/answer_loop_cases.json`), which the tests use.
`evidence.py` takes spans from the records' canonical results and from compact
per-sub-need spans (`{path, lines, excerpt}`). It skips the v3 bookkeeping
(decisions, decision log, record uses, Union, partial, ticket, degradations,
model work, omitted ids, rounds, follow-ups and unrun reads), so the evidence
digest ignores decisions and timings.

**Arms** (`make_loop_spec.py gate`, one series on both repositories of the
pair, munarium and bookbuilder):

| Arm | Result name | Focus | Context | Judged |
| --- | --- | --- | --- | --- |
| F | `focus-gpu` | the gate's Focus route (`focus-static1024-cpu` until the CUDA Focus seam closes) | none | yes (12.7.2) |
| C | `compact-gpu` | the gate's Focus route | Compact FP16 CUDA, top score | yes (12.7.1, 12.7.2) |
| L | `large-gpu` | the gate's Focus route | Large FP16 CUDA, top score | yes (12.7.1) |
| C32, L32 | `compactFp32-gpu`, `largeFp32-gpu` | the gate's Focus route | Compact or Large FP32 CUDA | replays; sessions only if escalated |
| CF16, CF32 | `compactFocusFp16-gpu`, `compactFocusFp32-gpu` | Focus FP16 or FP32 CUDA | Compact FP16 CUDA | replays; sessions only if escalated |
| A (after LG, not a gate arm) | `arbiter-gpu` | none: Focus off | Arbiter (the former Compact) FP16 CUDA, top score | L7 |

- **Arm identity.** Each entry's `arm_check` is checked on every gate replay
  and on the digest call before any session; `run_pass.py` refuses a session
  whose live reply is not the arm's. Focus only means no decision names a
  Context model and `context_absent` is disclosed. C and L mean every model
  decision is the arm's model on CUDA at its precision by `top_score`, with no
  abstention. Focus's own precision is not visible in a reply; the lifetime
  receipts record which profile was enabled.
- **A model that was present and not reached is not Focus only.** The reply
  sets `context_absent` when no Context model was present, and also when one
  was present and every call was not dispatched. The harness tells the two
  apart from the decision log's counts by source, where a decision that was
  due to a model names it (`loop_reply.context_presence`): the model took
  decisions; it was present and no call was dispatched; it was present and
  took none for other reasons (unavailable, at compile time or later, or
  abstained); or no decision was due to a model. In arm C or L the check
  fails when the model took no decision in the whole reply, and says which
  case it is. In arm F it fails when any decision was due to a model. A reply
  in which the model took decisions in one stage and none of those due in
  another passes and is flagged (`arm_flags`; `arm_flagged` in
  `arm-replays.json`): the gate report lists the flagged scenarios.
- **Arms side by side.** Each arm runs in its own fixture lifetime, with its
  own fixture root (`R/fixture-<series>-<arm>`), service and control root (the
  service's runtime directory, so the lifetimes share no port or socket). Each
  arm entry names its own lifetime's handle. `run_plan.py`'s `lifetimes` step
  first checks that the declared device and host memory of every listed
  lifetime, plus a 4 GiB device and 16 GiB host reserve, is free now. It then
  launches them together, waits until each is queryable and settled, and
  enables each one's stages. `stop_lifetimes` disables the CUDA Context stage
  before each stop (GPU_FIXTURE.md). The judged arms C, L and F run together
  (10 + 20 + 0 GiB device). The precision arms follow in phases that fit
  48 GiB: C32, then L32, then CF16 and CF32 (last, because the CUDA Focus
  seam is an open gap), each phase followed by its precision comparisons; the
  performance step runs before them. The start options of
  each lifetime come from L5C through `make_loop_spec.py gate --lifetimes
  <file>` (`{arm: {start_args, device_gib, host_gib, fixtures, lifetime,
  launcher}}`). A launch with placeholder `<...>` options is refused. The
  settled-index guard keys each lifetime's tokens by its fixture root
  (`settle_per_root`). It checks the scenario repositories (munarium and
  bookbuilder, through the root's aliases). The stage commands name no
  repository, so the launcher enrols every index of the lifetime
  (`bookbuilder-v2` and `munarium`); a row may name index names in
  `stage_repositories` instead.
- **Parallel sessions.** The judged arms' sessions run at once in one
  `run_pass.py` process (`parallel_entries`): 4 sessions an arm (12 in all),
  interleaved so every arm starts together, and 12 judges. The harness knows
  of no account rate limit for headless Haiku agents or Sonnet judges. An
  overload shows as failed sessions, which the pass records and resumes; lower
  `ARM_SESSIONS` and `JUDGE_SESSIONS` in `make_loop_spec.py` if it appears.
- **The in-lifetime Context switch** stays available as the named hook
  `switch_context` of `run_plan.py` (plan 12.5.2 item 4), for single-lifetime
  use; the gate plan does not use it. It disables the live CUDA Context (the
  release) before it enables the next profile (the warm), through
  `gpu_fixture.py stage`, and writes a receipt with release and warm time and
  device memory. Sessions groups with `"stager": "cuda"` stage through it. A
  resumed plan names the live stages with `--live-cuda`.
- **Tests.** `gate.py replays` (tests 1 and 2), `agreement` (test 1),
  `outcomes` (test 3, with the close calls written as a jobs file for
  `run_pass.py --stage second --second-jobs`), `concurrency` and
  `performance` (test 4), and `precision` (12.7.3). Gate sessions are never
  reused across arms. The concurrency test (2 and 4 concurrent Answers,
  tickets attached) runs one arm at a time before the sessions, so the arms do
  not share the device while it measures throughput. It needs L2b's job table
  on the live Answer path. If it does not run, the report says "not run, so
  not passed".
- **Conventions (stated so they can be overruled).** A Rubric A win is the
  higher adjudicated grade, then fewer turns to a correct finish. A gate close
  call is two finishes within one turn, or one finish against one essential
  fact short. Where a second session exists and its winner differs, the
  scenario is `undecided`. Decisions are compared like for like only: equal
  `roster_digest` gives `agree` or `differ`, otherwise `diverged`. Where they
  differ, the reference notes favour the arm whose evidence for that sub-need
  covers more essential, then supporting, notes. For Focus, a changed
  presented basis counts as a different choice, because Focus's ranking is in
  the decision log, not in the reply.
- **Precision escalation (12.7.3).** An FP32 arm gets judged sessions only if
  more than 5% of its stage's decisions flip, or the essential notes covered
  differ on any scenario.
- **The report.** `gate_report.py` builds the Markdown report for
  `docs/arbiter/answer-evaluation/` from `gate-report-inputs.json`. It shows
  evidence by location only, and removes quoted and code text from judges'
  reasons on bookbuilder and munarium scenarios, so no source of either
  repository is quoted. It
  applies the 12.7.1 and 12.7.2 bars and states what they indicate as
  recommendations; the user decides (plan 12.9). Since the user's direction
  of 23 November 2026 work does not stop at the gate (plan section 2, "after
  LG"). The generated file is the series' detail report
  (`<NN>-gate-lg-detail.md`); the series report beside it (`<NN>-gate-lg.md`)
  is written by hand in the form the reports README asks for. The report
  lists its evidence limits: sample size, close calls and second sessions,
  undecided and pending scenarios, arms whose identity was not met, and tests
  that did not run.
  - **Coverage rule.** The report's opening states which coverage rule
    counted the reference notes, from each arm's `arm-replays.json`. Arms
    scored under different rules are named, and their coverage figures are
    said not to compare.
  - **Decisions by source.** For each arm and stage the report counts, each
    in its own column: the decisions the model took (`model`); those that fell
    back to the deterministic order after the model was asked (`abstained`,
    `unavailable`); those never sent to the model (`not_dispatched`: no token
    budget, no time or no model for that call); and those taken without a
    model (`deterministic`) (`gate.decision_sources`, also in
    `performance.json`). A slower model can be shed more often. The counts are each reply's `decision_log.counts`,
    which cover every logged decision. The typed fallback reasons come from
    the decisions a reply includes, so they can cover fewer decisions; the
    report says how many. A second table counts each arm's replies by whether
    the Context model took part: took decisions; present, no call dispatched;
    present, took none; no model. It names the scenarios of the two middle
    columns and, per stage, the replies where the model took none of the
    decisions due to it.
  - **Budget sent.** The report states the exact budget every Answer was
    sent, from the arms' `budgets_sent`, or names the arms that were sent
    different budgets and says their figures do not compare.
- **L7.** `make_loop_spec.py l7 --handle <ready handle.json> --claude <pinned executable>`
  binds the pass to the current fixture and evaluation runner. The CUDA
  stage switches and the sessions use the same bound lifetime. The
  baseline spec and plan cover:
  Search, Parse, and the loop as Focus only (F), Arbiter with Focus (C) and
  Arbiter only (A, `arbiter-gpu`), with no CPU cells. Large was dropped after
  LG and is not in the L7 plan; the `gate` command still builds the LG series
  as it ran. Arm A's check refuses any Focus work (a Focus call, a Focus
  profile in `model_work`, or evidence without `focus_absent`); its stage
  switch releases Focus and is refused as a plan's first switch, when the live
  Focus is not known. `lg_lifetimes.py` lays out arm A with a Context-only
  pair and `--seed-cpu none`. `l7_plan_template.json` is the same plan with
  placeholders, regenerated by `make_loop_spec.py template`; a test keeps it
  current.
- **Prompts freeze.** `loop_reply.py` and `gate.py` are runtime harness files
  in `prompts_freeze.HARNESS_FILES`. The freeze written for the one-turn
  evaluation no longer verifies, because the Answer request and budget
  changed. It is re-frozen with `prompts_freeze.py --refreeze` before the first
  judged loop pass. That re-freeze is the operator's first step at LG, once
  the harness is final.
- **Scenario freeze.** `R/scenario-freeze.json` is the loop freeze of
  22 November 2026: 30 files (munarium development, bookbuilder development
  and sealed), with no airk file. The one-turn freeze is kept as
  `R/scenario-freeze.oneturn-20261121.json`, for history only. Every reader
  uses the freeze's `files` map alone (`run_pass.py`, `gate.py`, `depth.py`,
  `leak_check.py`, `fixture.py leak-probe`). The prompts freeze records the
  scenario freeze's SHA-256, and `prompts_freeze.verify` refuses a prompts
  freeze made against another scenario freeze.

```text
# LG, after L5 and its short trial (R is the private root)
python -I -B scripts/arbiter-answer-eval/prompts_freeze.py --refreeze R/design/prompts-freeze.json --reason "loop harness (Answer /3) for LG"
python -I -B scripts/arbiter-answer-eval/make_loop_spec.py gate --series <NN>-gate-lg --lifetimes <L5C lifetimes.json>
python -I -B scripts/arbiter-answer-eval/run_plan.py --plan R/passes/<NN>-gate-lg/plan.json --log R/passes/<NN>-gate-lg/plan-log.jsonl
python -I -B scripts/arbiter-answer-eval/gate_report.py --inputs R/passes/<NN>-gate-lg/gate-report-inputs.json --out docs/arbiter/answer-evaluation/<NN>-gate-lg-detail.md
```

### The four switch combinations on one lifetime (LH acceptance)

**Purpose.** The LH acceptance run (23 November 2026) switched one CUDA lifetime
through neither switch, Focus only, Arbiter only and both, and ran a smoke
Answer on a real service after each. These tools are its harness, moved here
from a one-off folder. They are evaluation tooling, never product, and a
shakedown's results are a pipeline test, never a result.

**Inputs.** A ready lifetime's `handle.json` (and its fixture root), the
private root R with the frozen scenarios, and the shakedown's own output root
(nothing of a shakedown lands in `R/passes`).

**Outputs.** One spec per arm under `<out-root>/passes/<series>/` (`spec.json`
and `gate-report-inputs.json`), `gate.py smoke` receipts, the summary rows of
`smoke_summary.py` on stdout, and the probes' receipts in `--out`. Receipts stay
in the private root; none is committed.

**Use.**

```text
# one spec per combination: N neither, F Focus only, A Arbiter only, C both
python -I -B scripts/arbiter-answer-eval/make_loop_spec.py --r-root R shakedown --series lh-accept-N --out-root <shakedown root> --handle <lifetime>/handle.json --scenarios munarium-A1,bookbuilder-A1 --arm N
# switch the lifetime to the arm's stages (gpu_fixture.py ensure/stage), then:
python -I -B scripts/arbiter-answer-eval/gate.py --r-root <shakedown root> smoke --spec <shakedown root>/passes/lh-accept-N/spec.json --arm N --out <dir>
python -I -B scripts/arbiter-answer-eval/smoke_summary.py <dir>/smoke.json [<dir2>/smoke.json ...]
python -I -B scripts/arbiter-answer-eval/lifetime_probe.py --fixtures <fixture root> --lifetime <name> --out <dir> help
python -I -B scripts/arbiter-answer-eval/lifetime_probe.py --fixtures <fixture root> --lifetime <name> --out <dir> arbiter-status --label after-switch
python -I -B scripts/arbiter-answer-eval/lifetime_probe.py --fixtures <fixture root> --lifetime <name> --out <dir> index-status
```

`--arm` takes `F`, `C`, `L`, `A` and `N`. `N` (neither: no Focus, no Context
model; path `noModel`) and `A` (Arbiter only) are shakedown arms: the LG
series still builds exactly F, C and L, and the L7 plan runs F, C and A. `L`
stays because the LG series, `gate_report.py` and the `lg_*.py` tools still
build and read it.

`fixture.py` waits for the service's `ready.json` to be a complete, parseable
file (`complete_json_observer`): while the service is still writing it, a
sharing violation (`PermissionError`) or a JSON parse error means "not ready
yet", not a failure. A file that stays unreadable for 60 s after it appears
(checked at each directory change) fails with its last error, and the
readiness deadline still bounds the wait.

### How Claude Code is started

The harness uses the newest installed VS Code extension binary
(`~/.vscode/extensions/anthropic.claude-code-*/resources/native-binary/claude.exe`).
Pass `--claude` or the spec's `claude` field to pin another. On 21 November
2026 the npm install (2.1.118) refused `claude-opus-5-5` and asked for 2.1.280
or newer. The harness updates nothing.

An agent session is started like this:

```text
claude.exe -p --model claude-haiku-5-5 --effort medium --setting-sources "" --strict-mcp-config
  --mcp-config <session>/mcp-config.json --tools <the path's mcp__airk__ tools>
  --allowedTools <same> --disallowedTools Bash,PowerShell,Read,... --permission-mode dontAsk
  --disable-slash-commands --no-session-persistence --system-prompt <harness prompt>
  --output-format stream-json --verbose        (stdin: the instruction; cwd: <session>/work)
```

- **`--setting-sources ""`.** This is what keeps the repository's `CLAUDE.md`
  and the user's memory out, even though the working folder is inside the
  repository. With it, a probe saw no instruction files. Without it, the same
  probe quoted both.
- **`--tools`.** Naming only the wrapper's tools leaves no built-in tool
  offered. `--tools ""` also hides MCP tools, so the harness does not use it.
- **Environment.** The child environment drops `AIRK_*` and the parent
  session's markers. It sets `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`,
  `ENABLE_TOOL_SEARCH=false`, `ENABLE_CLAUDEAI_MCP_SERVERS=false` and
  `DISABLE_AUTOUPDATER=1`.

The judge is started the same way, on `claude-sonnet-5-5` with `--effort medium`, with these
differences:

- `--tools ""` and no MCP configuration
- `--output-format json`, with `--json-schema` for the verdict
- the packet on stdin

## Decisions of 21 November 2026 (primary agent and user)

1. **Settled index.**
   - A judged pass runs only when every index it reads reports semantic
     readiness `ready` and served mode `hybrid`, with no index or vectors job
     queued or running.
   - `run_pass.py` snapshots each index at every sessions or digest
     invocation, at the start and at the end (`settled.jsonl`,
     `settled-start.json`).
   - A publication token that differs from the pass's first start writes
     `INVALID.json`, and the summary carries it.
   - `accept_queryable` is refused in a spec unless `"pilot": true`.
2. **Default response mode.** Agent tool calls, canonical digest replays and
   depth searches drop `AIRK_MCP_RESPONSE_MODE` from the handle's environment.
   Every turn records its effective `response_mode` and `env_stripped`.
3. **Coverage rule 2.**
   - A text span covers a note only if it overlaps at least
     min(5, the note's line count) lines of the note's range. An alternative is
     measured on its own path and range.
   - A path without a line span still covers only by symbol.
   - Rule 1 (any overlap) stays available as `rule=1` for comparison.

   **Coverage rule 3** (coordinator, 22 November 2026, before the first loop
   pass) is the current rule, `evidence.COVERAGE_RULE`:
   - Line overlap is rule 2's.
   - A path without a line span covers a note only when its text names the
     note's symbol as a whole identifier: no letter, digit or underscore
     directly before or after it. Rules 1 and 2 matched the symbol as a
     substring, so a short symbol (`run`, `main`, `turn`, `Spec`, `budgets`)
     matched almost any text of its file.
   - For a symbol qualified with `.` or `::` whose segments are all
     identifiers, the qualified form or its last segment counts
     (`Schedule::add` is named by `add`, not by `address`). A heading or other
     free text counts only as written.
   - A note without a symbol is never covered by a path alone, as before.
   - Rules 1 and 2 keep their substring match (`rule=1`, `rule=2`), because
     the one-turn passes are history and are not rescored. Every coverage
     record, arm replay and gate report states the rule it used.
4. **Parse is unchanged.** Every report states that its first call has a
   structural zero yield: the prose instruction prepares a plan that needs an
   identifier, and the call returns no evidence.
5. **Prompt freeze.** Every non-pilot spec names `prompts_freeze`, and
   `run_pass.py` refuses any difference in a prompt, the schema, the default
   models and efforts, or a runtime harness file.
6. **Models (user decision).**
   - Agents run on `claude-haiku-5-5` and judges on `claude-sonnet-5-5`, both
     at `--effort medium`.
   - A spec sets `agent_model`, `agent_effort`, `judge_model`, `judge_effort`
     and `judges`, and an entry may set its own `model` and `effort`. Both are
     recorded in every session and judge record.
   - **Reference arm.** It has its own configuration name (for example
     `reference-sonnet-noModel`) and `arm`.
   - **No reuse across agents.** A session is never reused across a different
     agent model or effort.
   - **`@better-of-search-parse`.** This path resolves, per scenario, from the
     pass's graded `results.csv`.
7. **Stages.** These are read live from `arbiter status-legacy`, because a
   fixture smoke run can change stages without logging them. When the service
   cannot answer, the harness falls back to `stages.jsonl`.

8. **The better of Parse and Search** (primary agent, 22 Nov 2026). One rule is used everywhere: grading,
   the model-step comparisons, the Rubric B summaries and the reference-sonnet-best choice.
   1. A correct finish at fewer turns.
   2. If neither finishes, more essential notes covered over the session.
   3. Then fewer session turns.
   4. Then Search.
9. **Images.** A spec may name `require_images` (image labels). `run_pass.py` then refuses to start
   sessions on any other image. An index with denies needs an image with the `filtered_paths` fix
   (o2-fix3 or later), and `images.json` cannot show that by itself.

## Conventions the plan leaves open

These are recorded in the code, so a report can state them or the primary
agent can overrule them.

- **Evidence text.** A note is covered by line overlap (rules 2 and 3: at
  least min(5, its line count) lines) only through evidence text (`hits[].text`,
  parse evidence that carries a hit). An outline or
  symbol row is a name at a place, not evidence text, so it does not count.
  `coverage.py --count-fact-rows` makes it count. Definition hints without
  text never count. Paths match after normalizing slashes and accept a
  directory-prefix suffix (`D:/x/src/a.rs` matches `src/a.rs`).
- **Grades.**
  - A brief that is "missing" a fact counts like one that is "wrong".
  - Grade 1 means exactly one essential note is a deficit: uncovered, or not
    stated correctly.
  - Misleading evidence that the agent followed gives grade 0.
- **Adjudication.**
  - Equal judge grades stand.
  - Grades one apart take the lower grade, and that judge's verdict.
  - Grades further apart stay pending until `adjudications.json` settles them
    (`{"<key>": {"grade": n, "correct_finish": bool, "reason": "..."}}`).
- **"Worse".** This also covers Answer reaching no correct finish when the
  better of Parse and Search does.
- **"Not worse on average in a kind".** This compares mean adjudicated grades.
- **Close calls.** A close call is a turn difference of -1, 0, 1 or 2 between
  two correct finishes. The L7 plan freezes the eligible Answer and stronger
  Search or Parse pairs from the first grade before any second session.
  Search and Parse run with both stages off in both samples. The plan
  refreshes the report after all requested second samples. Samples that
  differ give "undecided".
- **Reuse.** The evidence digest covers each span's path, lines, a hash of
  its text and its kind, in order, plus Answer's paragraph texts. Timings,
  receipts and publication tokens are excluded. A session is reused when the
  canonical first call gives an identical digest to an earlier completed
  session with the same tool set.
- **Failed digests.** A canonical first call that fails is not cached as a
  digest. It goes to `digest-failures.jsonl` and blocks reuse for that
  session.
- **Bare line references.** The citation audit checks `path:start-end`
  citations. Bare references such as "lines 14-26" cannot be audited, so they
  are counted (`unaudited_bare_refs`). From the pilot on, the system prompt
  asks for the full path with every range.
- **Discarded sessions.** A discarded session is rerun once. The discarded
  attempt is kept as `<result>.attempt-1`. Failures and timeouts are results,
  and the harness never retries them.

## Tests

```text
python -I -B -m unittest discover -s scripts/arbiter-answer-eval
```

`test_loop.py` covers Answer /3 parsing on the Rust-emitted cases (every
refusal case parses), the arm checks, v3 evidence, coverage and attribution,
ticket attach in the wrapper and in replays, the gate tests, the escalation
rule, the report's bars and redaction, the LG and L7 specs and plans, the
switch hook and the template's currency. `test_harness.py` covers the wrapper over real stdio (newline and
`Content-Length` framing), the tool lists, the first-call rule, the turn cap,
argument injection, the log format, the coverage edge cases, the grade and
case tables, the configuration rule, headroom, the CSV columns, the audit,
judge packets and verdict validation, and a resumable pass with reuse. It
uses synthetic data only and starts no model or service. Its synthetic
repository is named `airk`, as in the one-turn history, and its specs name it
with `"repos": ["airk"]`. `test_pair.py` covers the loop pair: the defaults,
the munarium scenarios in the loaders, the per-repository win count, the
gate report's privacy for munarium, the generic leak check (a repository with
an empty exclusion list is still probed and records it) and the no-airk check.
`test_lh_tools.py` covers `smoke_summary.py` and `lifetime_probe.py` over
canned replies and a stub command runner. `test_fixture.py` belongs to `fixture.py`
(including the `ready.json` wait, with an injected clock and reader and one
real directory-change wait), `test_lg_lifetimes.py` to
`lg_lifetimes.py` and `test_lg_bases.py` to `lg_bases.py`.

### Matched CS1 Search with Focus off and on

`search_focus_ab.py` freezes the exact first canonical Search requests from
the 20 judged CS1 Search sessions in `08-windows-cuda-cs1-r2`. Use a fresh
private output under the checked human-owned campaign root. The cohort holds
private queries and essential notes. Its inputs are the judged pass index, the
private grades CSV, and their frozen scenarios and retained first-call receipts.

- Run `freeze --pass-dir DIR --pass-index-sha256 SHA --results-sha256 SHA --output cohort.json`.
- With both optional stages disabled, run
  `collect --cohort cohort.json --cohort-sha256 SHA --handle HANDLE --handle-sha256 SHA --arm off --output off.json`.
- After enabling the admitted CUDA Focus profile alone on the same fixture, run
  the same command with `--arm focus --focus-profile MANIFEST_SHA --output focus.json`.
- Run `compare --off off.json --off-sha256 SHA --focus focus.json --focus-sha256 SHA --output comparison.json`.

Use `python -I -B scripts/arbiter-answer-eval/search_focus_ab.py` for each
command. Collection reads the live configuration and the two settled
publications before and after; the operator owns all stage changes. The
requests retain sent arguments and response mode, including the current
default 10-result and 8,192-byte limits when omitted. The comparison reports
coverage and one-based essential citation ranks under the existing rule 2,
returned counts, and elapsed CLI time. Focus receipts bind CUDA execution to
the selected profile, source publication and configuration revision and
show whether any unscored tail was preserved. Missing applications or failed
reads remain visible and cause the comparison to exit 2. This measures the
first retained Search reply, with no widening or follow-up sessions. The
result says nothing about candidates outside the returned pool. The guard
probes precede measured requests, and cache and run order can affect latency.

#### Cumulative Search milestones

Retain the original complete Focus pair comparison once. Produce a new
`compare` receipt for each milestone on the same frozen cohort. Then run:

```text
python -I -B scripts/arbiter-answer-eval/search_focus_ab.py cumulative --step N --original ORIGINAL.json --original-sha256 SHA --prior PRIOR.json --prior-sha256 SHA --current CURRENT.json --current-sha256 SHA --output CUMULATIVE.json
```

For step 1, supply the original comparison as the prior too. The original
CS1 comparison predates the embedded Focus profile, so also supply
`--original-focus-arm ORIGINAL-FOCUS.json`. Its SHA-256 comes from the pinned
original pair receipt. Later milestones use the immediately preceding pair
as the prior. Request identity, count and byte limits, response mode, note
IDs, source revisions, index identity and exact Focus manifest must match.
Step 6 alone accepts `--experimental-index` to compare changed publication
tokens against the original source revisions, with the changed fields
recorded explicitly.

The report gives matched essential gains and losses, shared-note rank
movement, top-three coverage, inverse rank, and paired CLI time deltas both
overall and by repository. The Focus interaction is the change with Focus
enabled minus the change with Focus disabled. Read it alongside cache and
fixed-arm-order limits, and for an experimental index, the changed
publications. The report does not contain query text or excerpts.

### Private Focus shortlist inputs

For the wider Focus diagnostic, a test-support service can set
`AIRK_PRIVATE_FOCUS_SHORTLIST=1` with `wide_shortlist`. This adds an
`AIRK_FOCUS_SHORTLIST_INPUTS` record to the private service log whenever the
40-candidate shortlist is formed. It binds the SHA-256 of the exact query
text, the publication, the 1024-token pair width, the ordered candidate
citations, and the selected zero-based ordinals. Each candidate contains
its zero-based ordinal, full hit-text hash, and the exact bounded Focus
window text with its UTF-8 byte range. The window uses the native bounded
Focus rule. The ordinary shortlist trace remains available.

Use this capture only in the isolated evaluation service with the pinned
development scope, query cohort, and CUDA Focus profile. The log contains
private source excerpts. Keep it and any input extracts under checked
private campaign storage, outside staging, packaging, and judged sessions.
Record its hash in diagnostic receipts. This capture establishes the
input to the proposed wider scoring experiment. A separate execution
receipt must establish actual model work, device, and timing.

### Committed vector passage coverage

`search_vector_audit.py` takes the SHA-256-pinned Search cohort and a
verified receipt for detached index databases. Its `prepare` command writes
the exact collection-relative paths, publication sequences, pipeline
digests, committed row counts and database hashes for the native probe.
Prepare copies while the fixture is settled, verify that each original WAL
is empty, and verify the copied bytes against the unchanged original.
Place each database at `db/<repository>/index.sqlite3` beside the generated
input under the ignored audit directory.

Set `AIRK_SEARCH_VECTOR_AUDIT_INPUT` and `AIRK_SEARCH_VECTOR_AUDIT_SHA256`
to the prepared input and its digest, then capture stdout and stderr from
the scoped native command.

```text
cargo nextest run --profile ci -p airk-store --lib --features test-support --locked --offline --timings --run-ignored ignored-only --no-capture -E 'test(=index::coverage_audit::committed_vector_coverage_audit)'
```

The ignored test emits one `AIRK_COMMITTED_VECTOR_CITATIONS` JSON record
per requested path, with no evidence text. Pass the log, frozen cohort,
original and current Focus comparisons, snapshot receipt and prepared
input with their hashes to `summarize`. An optional pinned service trace
can show where the same qualifying occurrence ranked inside a recorded
candidate list. Its rank does not establish position in the full vector
matrix. Keep inputs, logs and reports under the private campaign root.

The summary also reports the first covering rank in each retained semantic
branch. A note classified as union-only uses the earliest prefix with enough
distinct lines at a single accepted location. A null element means coverage
was absent in that bounded branch. A null list means the query was untraced.

### Full vector rank census

Stage a test-support CUDA image and reopen the authenticated settled
evaluation pair with both optional stages off. Pass
`--search semantic_subqueries --vector-census-input <audit-input.json>
--vector-census-sha256 <sha256>` to `gpu_fixture.py start` alongside
the settled reuse arguments. The launcher admits only the pinned two-repo
audit roster. The native service keeps the ordinary Search budget and
writes sparse exact ranks to the private service log.

Run `python -I -B scripts/arbiter-answer-eval/vector_rank_replay.py` with
the frozen cohort, snapshots, audit input, native audit log, Ready handle,
and their SHA-256 pins. Give it an unused `--output` under the private
campaign root. It sends the frozen instructions and the fact sentences
only to the isolated evaluation service. It reports instruction and
focused vector ranks, fact-derived vector ranks, and delivered ranks for the semantic-subquery
arm. Empty focused lists mean no auxiliary embedding was available for
the native request. Treat the labeled fact queries only as
diagnostics. The report covers the vector component of the rank census.
Recover exact ranks from a stopped partial replay with
`--closed-trace`, `--closed-trace-sha256` and a verified startup event
count. The report records the skipped query digests and requires an
ordered match for every other event. Run missing queries with `--case`
and `--facts-only` or `--instruction-only` on the same pinned inputs.

For the ranked lexical and fusion measurement, start a matched image
with the same roster and profile plus `--branch-census-only`. Run
`branch_rank_replay.py` with the same pin arguments and a fresh result
path. The collector sends the exact native-focused UTF-8 task fragments
as standalone queries. It records the ranked keyword/identifier stages
before their native branch limits, the deduplicated fusion order before
packing, and coverage in the delivered hits. Missing ranked branches and
dropped term groups are explicit. The branch-only mode leaves the already
measured complete vector scan out of this pass. Use `--case` for a targeted
probe and `--omit-case` to preserve that independently pinned result when
collecting the rest of its cohort.

For source custody, `source_fidelity_audit.py` reads the frozen exported
paths and closed databases named in a SHA-256-pinned fixture handle. Its
private report lists path counts, explicit denies, unclassified omissions
and stored source-hash comparisons. `compare_census_postings.py` compares
canonical FTS term, stable chunk, column and offset identities and vector
matrix hashes from two pinned closed handles. It keeps their publication
tokens separate. Run both from the checked Windows shell with fresh
`--output` paths under the private campaign root. Neither changes a
database or Git export. `corpus_path_classes.py` adds reproducible
heuristic path classes to the closed index file and vector-unit rosters
and the retained Step 7 top-40 candidate trace. Its inputs are the pinned
closed handle, frozen cohort, Focus-off receipt and paired native trace.

### Synthetic Codex tool-offer preflight

`codex_isolation_probe.py` sends a synthetic request to a separate
loopback Responses endpoint and records the outbound model, effort and
tool-schema digests. It deliberately returns a terminal HTTP error after
capture. It records no headers or raw prompt. Run it with a checked
scratch directory containing `workspace`, `logs`, `state` and `tmp`.
The workspace can contain an instruction canary. This probe does not
qualify hosted admission, verdict generation or the wrapper's call cap.

The probe reads both top-level tool definitions and Responses Lite
`additional_tools` input items. It compares namespace-qualified tool names
including duplicates and rejects built-ins outside the expected catalog.
A pinned `--model-catalog` and its SHA-256 can test a local metadata override.
The loopback-only `--app-server` variant reads the effective MCP configuration
before opening a model thread. It submits the wrapper schemas as explicit
dynamic tools. Use `--disable-mcp-server` to account for inherited servers
when diagnosing the original configuration path.

For a clean profile, use `--built-in-provider --app-server
--clean-app-server-home --codex-sha256 <digest>` with the pinned
model catalog and an empty `scratch/codex-home`. The process checks that the
MCP configuration is empty and the loopback request carries no authorization.
This uses a child-only home and a synthetic endpoint on 127.0.0.1.

Add `--round-trip` to exercise the stock Codex dynamic call bridge or a
tool-free judge's strict schema. The synthetic agent round trip verifies the
exact tool offer on every request and reconciles all attempts and returned
outputs with the wrapper log, including a wrong first call and an over-cap
refusal. The judge fixture returns a synthetic verdict under a small
schema. These checks qualify the loopback transport. Hosted model access,
production verdict generation and measured sessions remain separate.

### Pinned Codex actor and judge adapters

A new judged pass can select `"provider": "codex"` explicitly. Its
`codex_config` supplies an absolute executable and model catalog, their
SHA-256s, a dedicated signed-in ChatGPT home and the verified prompt
freeze SHA-256. The provider requires medium effort, Sol or Astra for
the actor, and two separately completed Astra judge threads. The
pass compares actual frozen file digests and rejects reuse of earlier
sessions or verdicts. Both standalone grading and pass grading revalidate
the original blinded judge packet, strict verdict and distinct thread IDs.

The ordinary actor uses the existing tool wrapper and counts every
attempted call in its reconciliation. Its strict call cap applies to the
twelve admitted calls. The judge receives no tools. Both roles retain
actual Codex thread and turn identity, observed model and effort, and
reported token usage. Cost stays unavailable unless supported by a
separate receipt. The historical runner remains available for its archived
specs.

The dedicated home must be provisioned separately. The adapters keep the
global Codex home and settings outside the production launch and use the
built-in OpenAI provider. Synthetic loopback qualifies the exact tool
offers and transport only. Hosted model admission, the fresh pass
manifests and complete-evidence qualification are separate gates.

### Windows Stage A freezes

`prompts_freeze.py --write-codex <new-manifest> --codex-inputs <json>`
pins the actual scenario freeze, stock runner and model catalog, exact
Search, Parse and Answer catalogs, maintained harness sources and strict
judge schema. Use purpose `windows-stage-a` for Sol or
`windows-stage-a-capable-panel` for Astra. The latter runs only at baseline
and change set 3. The runtime auth home remains a separately supplied
private directory.

`make_loop_spec.py l7` accepts `--codex-freeze`, `--codex-home`,
`--codex-stage` and an explicit Windows fixture `--handle`. Its Codex
spec requires the complete 20-scenario roster, a CUDA Answer arm and
fresh controls at the prescribed stages. Sol runs five arms and Astra
three. Historical specs retain their original generation and verification.

### Complete-evidence gate

`prepare_complete_evidence.py --preflight` authenticates the exact
development-case freeze, exported source custody, closed audit
database and each accepted UTF-8 line span. Its output is aggregate
coverage. Manifest generation needs explicit target publication tokens
from the admitted Answer lifetime. The custody tokens never stand in
for the target tokens.

The ceiling prompt freeze uses purpose
`windows-stage-a-complete-evidence` and pins all twenty private
manifests. `complete_evidence_pass.py` produces the separate Sol
Answer-only spec from that freeze, the matching runtime handle and
the dedicated Codex home. The actor's first canonical Answer sends
only verified cited text. Its dispatch receipt binds the actual
delivered bytes and rejects clipping before judging. The ordinary
native Answer remains available in the private run. Follow-ups use
the standard wrapper and turn cap.

Two distinct Astra sessions score the same blinded packet. The
separate tool-free Sol receipt contributes agreement diagnostics.
Normal Windows baseline admission uses
`--ceiling-disposition <receipt>` and revalidates the matched retained
actor, dispatch and scoring evidence. It requires at least 12
correct finishes overall and five per repository. Preserve an
incomplete or below-threshold disposition with its own receipts.

### Matched Window Search latency

The Windows-only, ignored `window_search_external_latency_observer` selector
in `crates/airk-cli/tests/tools.rs` measures a ready matched fixture through
standalone native Window Search. It grants the exact index and query project
from the authenticated catalog, verifies the published worktree and observed
scope, and closes the Window after serving the bounded request roster.

Set `AIRK_WINDOW_SEARCH_OBSERVER_INPUT` to the absolute private input and
`AIRK_WINDOW_SEARCH_OBSERVER_INPUT_SHA256` to its lowercase SHA-256. The input
has schema `airk.private.window-search-observer-input/1`, the absolute
`control` directory, a `repetitions` count, and `repositories`. Each repository
pins `repo`, `index_id`, `project_id`, the absolute `worktree`,
`publication_token`, and `requests`. Each request has an `id` and an exact
UTF-8 `arguments_json` string. Run the already built integration test binary
once with `--exact --ignored --nocapture window_search_external_latency_observer`
for each admitted image and input. Prepare the immutable publications, stage
settings and request bytes before timing. Warm both arms on the identical
request sequence and keep those rows separate from the measured samples.

The selector emits one `AIRK_WINDOW_SEARCH_OBSERVER_RECEIPT` line with private
per-call timings, request and result digests, projected scope and Search
execution. Check complete matched samples, pinned publications and execution
route before aggregating nearest-rank p95 per repository. The frozen whole-repo
default Search workload can measure ordinary latency. Its requests cannot
nominate the path-qualified explicit lexical repair. The injected Windows
`window_search_synthetic_repair_latency_stress` selector measures triggering
calls separately.

### Wider Search shortlist coverage

Run the paired Search collector with a fixture launched under the private
`wide_shortlist` profile. After the lifetime closes, authenticate the
retained service log and pass its `AIRK_SEARCH_SHORTLIST` events to
`search_shortlist_audit.py` using `--trace` and `--trace-sha256`.
Pass the frozen cohort, off arm and focus arm with their pins and name
a fresh private `--output` JSON file. The audit matches every query to
its publication and verifies the selected citations against both
actual responses and the ordinary Focus permutation. It reports
coverage and ranks in the full shortlist, the semantic part and each
returned arm. Keep the trace and reports under the private run root.

The private `wide_cutoff` profile uses the same paired collector with
the `AIRK_SEARCH_CUTOFF` service trace. It requires one new event from
each frozen query and exact publication, eight unchanged held hits,
and four candidate hashes. The collector validates the full native
Focus receipt on those four and checks the final two returned positions
against the permitted projection. The comparison reports accepted
CUDA applications and cutoff replacements. A failed or partial score
keeps the deterministic picks and cannot count as an accepted trial.

### Adaptive Search result count

Launch the private `adaptive_results` profile and use the same
`search_focus_ab.py collect`, `compare`, and `cumulative --step 8`
commands. The frozen 20 wire requests already omit `limit`; the
ten-hit compatibility field is the baseline cutoff. The harness admits
up to 20 hits only for a verified adaptive lifetime and an omitted limit.
It checks the returned text bytes, unique paths and complete ordinary
Focus permutation. The cumulative report keeps separate measures at
rank three, rank ten and across the full response. The original cohort
and accepted Step 5 pair keep their original identities.

### Recorded follow-up Search and indexed reads

Use `followup_search_replay.py` with the pinned development cohort, archived
pass index, settled control handle and ready matched arm handle. Supply the
SHA-256 of each input and choose the service's matching `--profile`.
The collector replays the archived sent arguments for Search and indexed
reads and writes a fresh private JSON report. It preserves archived order,
response mode and failure status, and rechecks the ready publication and
scope after replay. Run the original and adaptive profiles on the same
service image and frozen publications. Compare labeled coverage per
repository, keeping Search and indexed reads separate. Use `--target` to
reduce a loss to an exact repository, scenario and Search call ordinal
under an intermediate profile.

### Matched native embedding query routes

Use embedding_route_search_ab.py only on a private, idle, settled
Windows CUDA fixture. Pass the pinned development cohort, the closed
reference handle, and the reopened Ready handle. Name one frozen
repository/slot with --case. The script requires a live CUDA route at
admission, observes a transition to CPU and back to CUDA around the same
vector-only request, then restores the original resource configuration.
It checks the full admitted citation order and publication. The result
does not supply independent vector coordinates or per-row writer-route
provenance.

### Query-time path prior

Run path_prior_pilot.py collect on the same settled pair and test image
under none and baseline_path_prior, with optional model stages off in
both. Supply the pinned cohort, private held-out freeze manifest, closed
reference and live handle. The collector checks the explicit CPU route,
replays each ordinary request three times for p95 latency, and issues one
separate top-40 request under the same scope. The held-out cases are never
queried by this development collector.

Use path_prior_pilot.py compare with the two pinned results and their closed handles and reuse receipts. Require a net
gain of at least six top-40 essentials, preservation of every original
top-ten essential, and query p95 within 1.10x for each repository. The
pilot reuses the same writer and publications, so it runs no builds of
the corpus and produces no new index bytes. Do not open held-out probes
without a qualifying development finalist.

### Independent source anchors and closed matrix ranking

Run docx_source_anchor_audit.py with the pinned native capture input,
binding, Focus-off report, fidelity report and closed source handle.
It checks source-file hashes and independent OOXML paragraph and table
anchors for the captured DOCX text. Keep the output private.

Run closed_matrix_cross_route.py prepare with the pinned development
cohort and one frozen repository/slot. Give its exact UTF-8 query file
and SHA-256 to the ignored native MiniLM smoke through
AIRK_MINILM_FP16_PROBE_QUERY and AIRK_MINILM_FP16_PROBE_QUERY_SHA256.
The optional receipt holds exact CPU and CUDA query coordinates under
the strict original model, placement and runtime pins. Then run
closed_matrix_cross_route.py compare with the same case, the native
probe receipt hash, the closed writer handle and its matrix SHA-256.
The comparator uses independent double-precision dots over live
committed rows. Its rank agreement describes the selected closed matrix.

For a single-member stored vector, audit_committed_vector.py authenticates
its source span, committed key and physical row in the closed publication.
Use prepare to write only the exact query input, keep original bytes and
newlines, and run the same native probe. Use compare to report actual
coordinate agreement under the retained numeric tolerance for both routes.


### Gather Context development comparator

The Gather Context tools freeze an evaluator-only reference from the Windows
complete-evidence disposition, the two versioned walkthroughs and the scenario
freeze. They reopen the archived source ranges and check their exact bytes.
Author step counts keep each author's recorded units. The reference retains the
original trace text and marks its coverage limits. A source-authenticated wider excerpt covers a required range it contains. Annotated alternative
locations require independent source review.

Use the checked Windows shell from the repository root. With the archived
campaign in `$campaign`, run the following read-only preflight.

~~~powershell
$runner = 'scripts/arbiter-answer-eval/gather_context_replay.py'
$disposition = '4e0e6eedd013e5db5972804e37a20195ba141f9a46d6fbd6f184a77cc23fd0ec'
python -B $runner prepare-spec --campaign-root $campaign --disposition-sha256 $disposition --verify-presentations
~~~

Append `--output $spec` to preserve the derived spec. Preflight all output paths
and their parents with the checked execution procedure before writing, and
audit the new outputs afterward. Use fresh paths in the private campaign.
Neither command overwrites an existing result.

~~~powershell
python -B $runner freeze-reference --campaign-root $campaign --spec $spec --output $reference
python -B $runner compare --reference $reference --control $control --candidate $candidate --output $comparison
python -B $runner report --comparison $comparison --unjudged --output $diagnostic
~~~

The comparison accepts captures with the same spec SHA-256, the exact frozen
case roster, the observed scope and each case's disposition. Keep actual
actions, delivered source witnesses and metrics with their units. Use null for
an unavailable measurement. Provisional routing and byte coverage can be
reported with `--unjudged`. A report using `--judged $judged` requires the same
spec digest and a verdict for every expected case and arm.

The ordinary source adapter exposes bounded list, literal and range reads
over one authenticated export. Its matching algorithm reports scan scope and
distinguishes an incomplete scan from a complete scan with clipped output.
The presentation module verifies the original archived delivery against the
successful actor's recorded dispatch. It can reorder the same passages using
the ordinary reply's part citations. The controller supplies one approved arm
to the isolated Codex actor and retains the observed calls and replies. It
must preflight the executable, scope and fresh owned directories, then audit
child and cache outputs before consuming the receipts. The exact tool offer
requested by the driver is recorded. Historical trace annotations and
independent reference obligations stay with the evaluator.

The offline preflight makes no hosted calls. A fresh native capture and the
matched hosted comparison require their own verified runtime and session
setup. Product qualification remains subject to the Windows review criteria.

For a full ordinary-tools export check, add
`--verify-exports-with-git (Get-Command git).Source` to the preflight. This
compares every exported file to the archived Git stream and checks the
original archive digest and path roster. A missing retained tar can be
regenerated in memory from the receipt's checkout and pinned commit.



The native capture command reads a live, settled control or candidate with
both optional stages disabled. Use the same frozen spec for both arms. Pass
the current admitted handle and the paired service image SHA-256. Prepare a
fresh private capture directory and check its receipt parents and files.
The command retains an exact reply and a normalized case record for each
task, then writes `capture.json` after the final publication and stage check.
Use that file as the arm input to `compare`.

~~~powershell
python -B $runner capture --campaign-root $campaign --spec $spec --arm control --output $controlDir --handle $handle --handle-sha256 $handleSha --native-sha256 $serviceSha
~~~

The fresh diagnostic actor uses GPT-6.2 Sol medium for ordinary source-tool
reads, original archived evidence, and regrouped archived evidence. A
private source snapshot and run specification bind the actor inputs. It
runs one app-server writer at a time in the approved isolated home based on
the observed cache race. It defaults to 24 external tool calls. Use
`--tool-call-limit 12` to reproduce the original limit. An over-limit
attempt is refused and the controller retains the case and its refusal in
the roster. The verified tool evidence byte budget and deadline apply at
either call limit. Use `--case bookbuilder-C2` for a representative case.
Omit the case selector and use `--arm ordinary --tool-call-limit 24` for an
all-20 comparison against the recorded 12-call ordinary arm. Compare
original and grouped delivery using the frozen 12-call archived arms.
The dedicated judging command
reopens the completed actor captures and runs two fresh masked Astra medium
sessions per eligible result with the existing rubric. Set `--jobs 4` for guarded cohorts after the actor run settles.

~~~powershell
$pwshExe = (Get-Command pwsh).Source
$gitExe = (Get-Command git).Source
python -B scripts/arbiter-answer-eval/gather_context_diagnostic.py --campaign-root $campaign --run-root $actorDir --disposition-sha256 $disposition --expected-sid $sid --pwsh $pwshExe --git $gitExe --case bookbuilder-C2
python -B scripts/arbiter-answer-eval/gather_context_judge.py --campaign-root $campaign --actor-root $actorDir --run-root $judgeDir --actor-spec-sha256 $actorSpecSha --disposition-sha256 $disposition --expected-sid $sid --pwsh $pwshExe
~~~

The caller creates and audits fresh run directories before either command.
The diagnostic compares evidence use. Native release qualification uses the
separate frozen product comparison and review criteria.

### Gather Context loss stages

`gather_context_loss_stage.py` reads delivered Gather Context replies and
reports, for each essential note, the furthest stage its location reached:
opened by a source read, listed by another read and never opened, present in
the reply at other lines, or absent from the reply. It uses the harness's
line-overlap rule and needs no service and no model. The stages describe what
a reply shows. They do not prove why a read was or was not chosen.

Inputs are the private scenario folder (`<repository>/<slot>.json`) and a
folder of `<repository>-<slot>.reply.json` MCP results. The output is a table
on standard output and, with `--output`, a new JSON file that is never
overwritten.

~~~powershell
python -B scripts/arbiter-answer-eval/gather_context_loss_stage.py --scenarios (Join-Path $campaign 'scenarios') --replies (Join-Path $campaign 'passes/gather-context-native-candidate-final-r1')
python -B -m unittest discover -s scripts/arbiter-answer-eval -p 'test_gather_context_loss_stage.py'
~~~

### Arbiter pair probe

`arbiter_probe_cases.py` and `arbiter_pair_probe.py` test a Qwen3 reranker
checkpoint outside AIRK, before any export. The first builds evaluator-side
pair groups from the private scenarios, the pinned source exports and
delivered replies. The second scores them with PyTorch and Transformers and
reports, per use, how often the right candidate ranks first.

| Use | Query | Right candidate | Wrong candidates |
| --- | --- | --- | --- |
| `passage` | The task | An essential note's own lines | Delivered passages that overlap no note |
| `action` | The task | A path, range and first lines of a note's location | The same description of an unrelated delivered passage |
| `coverage` | One essential fact | The note's whole range | The first part of that range, and another note's range |

The reference notes locate the right candidates. They are evaluator
knowledge and never reach a product prompt. The probe uses the publisher's
prompt and reads the `yes` against `no` probability at the last position.
It needs CUDA and does not fall back to CPU. It is not the product runtime
and says nothing about an AIRK export. `--precision 4bit` loads the weights
through bitsandbytes NF4, which approximates a 4-bit build and is not the
delivered quantization.

Variants. `arbiter_probe_cases.py --query-mode clauses` cuts the task into
clauses by a fixed rule and scores each candidate against every clause,
keeping the highest score. `--query-mode fact` uses the evaluator's own
statement of each need and is an upper bound that no product prompt may
use. `--head-lines` sets how many lines describe a read,
`--publisher-instruction` uses the publisher's default instruction for
passages, and `--roster` builds AIRK's one-pass choice among eight options
together with the same options as pairs. `arbiter_pair_probe.py --precision
block` rounds every linear weight in blocks (`--bits`, `--block`,
`--clip-search`) and expands it to FP16 before the run, which stands in for
block-wise weight storage expanded at load. It accepts several `--pairs`
files with one `--output` each on a single model load.
`arbiter_pair_report.py --result <file> --other <file>` reports recall in
the first places, a separation measure and the agreement of two results.

Run it from a private virtual environment that holds `torch`, `transformers`,
`accelerate` and `bitsandbytes`. Keep that environment, the downloaded
checkpoint, the pairs and the results in the private campaign folder. Both
commands refuse to overwrite an output.

~~~powershell
python -B scripts/arbiter-answer-eval/arbiter_probe_cases.py --scenarios (Join-Path $campaign 'scenarios') --sources "munarium=$munarium" "bookbuilder=$bookbuilder" --replies $replies --output $pairs
& $venvPython -B scripts/arbiter-answer-eval/arbiter_pair_probe.py --model $model --pairs $pairs --output $result --precision fp16
python -B -m unittest discover -s scripts/arbiter-answer-eval -p 'test_arbiter_pair_probe.py'
~~~

### Caller needs on real indexes

`gather_context_needs_run.py` sends each chosen task to a ready fixture
lifetime twice through the native Answer tool: alone, and with the `needs`
and `anchors` a calling agent wrote. It keeps each reply as
`<arm>/<case>.reply.json`, the shape the loss-stage script reads, and a
`run.json` with the requests, the model stages observed, whether each job
was reused and how many sub-needs the caller stated. A reused job means the
request was not gathered for, so that arm is void. It is a development
diagnostic, not a judged pass.

~~~powershell
python -B scripts/arbiter-answer-eval/gather_context_needs_run.py --handle $handle --scenarios (Join-Path $campaign 'scenarios') --needs $needs --budget-from $settledCase --output $out --cases munarium-C3 bookbuilder-T2
python -B -m unittest discover -s scripts/arbiter-answer-eval -p 'test_gather_context_needs_run.py'
~~~

### Single-stage Arbiter loop prototype

`arbiter_loop_prototype.py` runs the single-stage loop outside the AIRK
runtime, to settle the design before it is built in the kernel. One model,
Qwen3-Reranker-4B in FP16 through PyTorch, does every model job. For each
task it takes the caller's needs and anchors and, through a ready fixture
lifetime's own read tools:

1. fetches candidates with one search per need, per anchor and for the task;
2. judges every need against its candidates;
3. reads the project's guidance files and the files beside each need's best
   passages, reads around those passages, and looks up the names the model
   chooses from them;
4. delivers each need's best passages inside one byte budget.

Its switches are the experiment: `--second-pass` adds a stricter judgment
that breaks ties among relevant passages, `--relative` keeps a passage by
its share of the need's best score, `--settled` stops fetching for a need
that has strong evidence, `--shares` gives each need its own part of the
budget, and `--floor` and `--keep` are the fixed alternatives. It never
reads a scenario's notes. It writes `<case>.reply.json` for the loss-stage
script and `<case>.trace.json` with every judged candidate.

`arbiter_loop_misses.py` is evaluator-side: for each essential note it
reports whether the passage was delivered, fetched and judged (with its
score and rank), found only in the same file, or never fetched.

The prototype needs the private environment with PyTorch and the downloaded
checkpoint. It is a development diagnostic, not product code and not a
judged pass.

~~~powershell
& $venvPython -B scripts/arbiter-answer-eval/arbiter_loop_prototype.py --handle $handle --scenarios (Join-Path $campaign 'scenarios') --needs $needs --model $model --output $out --second-pass 30 --relative 0.5 --settled 0.6 --shares --cases munarium-C3 bookbuilder-T2
python -B scripts/arbiter-answer-eval/gather_context_loss_stage.py --scenarios (Join-Path $campaign 'scenarios') --replies $out
python -B scripts/arbiter-answer-eval/arbiter_loop_misses.py --scenarios (Join-Path $campaign 'scenarios') --traces $out
python -B -m unittest discover -s scripts/arbiter-answer-eval -p 'test_arbiter_loop_prototype.py'
~~~

### Agent cost comparison

`agent_cost_run.py` runs one acting agent on one task and records what the
session cost. It is the measure of the Gather Context strategy of
27 November 2026: the result is the percentage lower model cost with the
tool, guarded by a result that is no less correct.
[Strategy](../../docs/arbiter/retrieval-strategy-combined.md#strategy-from-27-november-2026) · [Measure](../../docs/arbiter/answer-evaluation/rubrics.md#gather-context-comparison-from-27-november-2026)

- A session is headless Claude Code in a fresh private copy of the
  repository snapshot, with Read, Grep, Glob, Edit and Write and nothing
  else. It has no shell.
- `--arm plain` offers those tools only. `--arm tool` also offers the one
  Gather Context tool through `gather_context_mcp.py`, with the name,
  description and input schema the build under test lists, and the server
  guidance given in `--instructions-file`. No first call is prescribed and
  no argument is forced.
- Tool calls reach the evaluation fixture by native name in the service's
  default response mode.
- The session folder holds `session.json` (the agent's own reported cost
  and four token counts, round trips as distinct model requests, times,
  tool uses by name, and each Gather Context call's seconds and reply
  bytes), `stream.jsonl`, `answer.md`, `changes.patch` and, in the tool
  arm, `gather/` with every call's arguments and raw reply. The private
  copy of the snapshot is removed after the patch is taken.
- It does not judge. Correctness is judged afterwards against the task's
  frozen truth.

~~~powershell
python -B scripts/arbiter-answer-eval/agent_cost_run.py --task $task --snapshot $snapshot --arm plain --model claude-sonnet-5-5 --output $plainOut
python -B scripts/arbiter-answer-eval/agent_cost_run.py --task $task --snapshot $snapshot --arm tool --model claude-sonnet-5-5 --handle $handle --repo munarium --instructions-file $guidance --output $toolOut
python -B -m unittest discover -s scripts/arbiter-answer-eval -p 'test_agent_cost_run.py'
~~~

`session.json` also records the bytes each tool returned to the agent
(`tool_result_bytes`) and the share that came from its own file tools
(`own_read_bytes`). A reply the client refuses as too large shows as a
small error notice here, not as the reply's size.

`narrow_scenarios.py` checks and freezes the 7 narrow tasks of each
repository (4 coding, 2 architecture, 1 writing). Input: the private
`scenarios-narrow/<repository>/N-*.json` files of schema
`airk.private.narrow-scenario/1` and the repository snapshot. `check`
verifies that every truth item names a file and lines present in the
snapshot; `freeze` runs the same check and writes `freeze.json` with the SHA-256
of every task file.

`agent_cost_batch.py` runs a whole series and reports it. Input: the
campaign folder, a series name, a model and the arms. Output:
`<campaign>/agent-cost/<series>/<task>.<arm>.r<k>/` per session and
`report.json`. `run` skips a session whose folder exists, so an
interrupted series resumes. `report` prints the median cost and round
trips per task and arm and the percentage lower cost with the tool; the
combined figure counts a narrow task four times a broad one.

~~~powershell
python -B scripts/arbiter-answer-eval/narrow_scenarios.py check --tasks $campaign\scenarios-narrow\munarium --snapshot $snapshot
python -B scripts/arbiter-answer-eval/agent_cost_batch.py run --campaign $campaign --series baseline-sonnet-r1 --model claude-sonnet-5-5 --arms plain --repeats 3 --workers 3
python -B scripts/arbiter-answer-eval/agent_cost_batch.py report --campaign $campaign --series baseline-sonnet-r1
~~~

`agent_cost_judge.py` lays a series' sessions beside each task's frozen
truth and stores the reviewer's verdicts in the series'
`verdicts.json`. It does not judge.

### Model-dose experiment

How much local inference the cost reduction needs is measured before the
4B model is put in the runtime. The model's choice is applied between the
service and the acting agent, and the result is the agent's cost. Timing
in this tooling is not a measure.
[Plan](../../docs/arbiter/retrieval-strategy-combined.md#strategy-from-27-november-2026)

- `arbiter_judge_service.py` loads Qwen3-Reranker-4B once (FP16;
  `--bits 8` rounds the weights to 8 bits and expands them to FP16) and
  scores pairs for every session over 127.0.0.1. Input: the checkpoint
  folder. Output: a state file with its address; `GET /stats` gives the
  judgments made, seconds and peak GPU memory.
- `gather_context_dose.py` replaces the `passages` and `omitted` of a
  brief reply with the model's choice among the passages the full reply
  records for each need, spending `dose` judgments a call. Nothing else in
  the brief changes.
- `gather_context_mcp.py --judge <state.json> --dose N` applies it to each
  call; `agent_cost_run.py` and `agent_cost_batch.py` pass both through. A
  fault in the step leaves the service's own brief in place and is logged.
  `session.json` records the dose, the judgments made and the calls it
  was applied to.

~~~powershell
python -B scripts/arbiter-answer-eval/arbiter_judge_service.py --model $checkpoint --bits 8 --state $judgeState
python -B scripts/arbiter-answer-eval/agent_cost_batch.py run --campaign $campaign --series dose150-sonnet-r1 --model claude-sonnet-5-5 --arms tool --handle $handle --instructions-file $guidance --judge $judgeState --dose 150
python -B -m unittest discover -s scripts/arbiter-answer-eval -p 'test_gather_context_dose.py'
~~~
