# Answer evaluation harness

Evaluation tooling, never product. It runs the agent sessions, coverage,
judging and grading of the Answer evaluation, as set out in
[`docs/arbiter/final-plan-fp16.md`](../../docs/arbiter/final-plan-fp16.md)
(sections 3.3 to 3.6, 4 and 5.2). Reports go in
[`docs/arbiter/answer-evaluation/`](../../docs/arbiter/answer-evaluation/README.md).
This folder is excluded from the airk index under test.

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
| `pass_digests.py` | Computes a pass's canonical first-call digests for named results while their stage configuration is live, and prints which results agree |
| `cuda_pair.py` | Inputs of a two-entry CUDA lifetime (`gpu_fixture.py --model pair`): hard-linked, authenticated asset directories and the pinned roster, review pins and profile rows. See [GPU_FIXTURE.md](GPU_FIXTURE.md) |
| `pass_report.py` | Per-session and per-path figures of a graded pass (turns, bytes, tool and wall time, grades, agent and judge cost) for reports and cost projections |
| `settled.py` | The settled-index guard: semantic readiness `ready`, served mode `hybrid`, no index or vectors job queued or running, and the publication token at start and end (read-only) |
| `prompts_freeze.py` | Writes and verifies `R/design/prompts-freeze.json`: the SHA-256 and text of every agent prompt, the judge prompt and its schema, the default models and efforts, and the hashes of the runtime harness files |
| `depth.py` | The no-agent depth measurement (plan E4). For each essential note it gives the rank at which an `airk_search` of the instruction (limit 100, the tool's maximum; no paging) first covers it |
| `rescore.py` | Re-scores a graded pass under another coverage rule. It writes `coverage-rule<N>.json`, `results-rule<N>.csv` and `summary-rule<N>.json`, and leaves the original results alone |
| `rejudge.py` | Re-judges a pass's identical packets with another judge model and effort, then compares grades, per-fact verdicts and Rubric B with the original verdicts |
| `probe_tools.py` | One real call per tool through the wrapper against a live handle, with the spans `evidence.py` finds in each result |
| `mock_cli.py`, `synthetic.py` | A stand-in CLI serving canned synthetic results, and the invented fixture (the "lantern" repository) used by the tests and the smoke run |
| `loop_reply.py` | Answer /3 (the loop) replies: decisions keyed by stage, part, sub-need and round, sub-needs and their coverage, records, tickets, the reply summary, and the gate arms' identity check |
| `gate.py` | The LG gate tests (plan 12.7): first-reply replays per arm (tests 1 and 2), decision agreement, judged outcomes between two arms (test 3), concurrency and performance (test 4), and the FP32 against FP16 precision check with its escalation rule |
| `gate_report.py` | The gate report for the user's personal review: bars and recommendations, agreement, evidence, outcomes, a per-scenario side by side, precision, performance and receipts |
| `make_loop_spec.py` | The LG series spec and run plan (arms F, C, L and the precision arms), the L7 baseline spec and plan, and `l7_plan_template.json` |

## Paths and tools

| Path (`--path`) | Tools offered, in order | First call |
| --- | --- | --- |
| `answer`, `noModel`, `focus`, `compact`, `large` | `airk_answer`, `airk_read_file`, `airk_context`, `airk_references`, `airk_symbols`, `airk_outline` | `airk_answer`, `question` = the instruction |
| `search` | `airk_search` and the same five reads | `airk_search`, `query` = the instruction |
| `parse` | `airk_parse` | `airk_parse`, `text` = the instruction |

The Answer configurations differ in the service configuration, not in their
tools.

- **Schemas.** These come from `tools-catalog.json` (the CLI's
  `tools list --json`). The wrapper removes `index` and `project` from every
  schema. It also removes `schema` and `budget` from `airk_answer`, and
  `schema` from `airk_parse`.
- **`airk_answer`.** The wrapper always sets `schema` and `budget`. Since the
  loop (plan section 12) the schema is `airk.arbiter.context.request/3` and
  the budget is the contract's whole default /3 budget
  (`common.ANSWER_BUDGET_DEFAULT`: 300 s, 262,144 bytes, 4 parts, 4 sub-needs,
  3 rounds, 132 calls, fanout 8, 132 candidates, 131,072 input tokens, 32,768
  a round, 48 card calls, 24 Focus calls, 192 Focus pairs). The one-turn
  evaluation's /2 request and budget stay as `ANSWER_SCHEMA_V2` and
  `ANSWER_BUDGET_DEFAULT_V2`, for history only. `--answer-defaults` can change
  budget fields, or add other fields, for a configuration.
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

## Inputs

- A fixture handle: `R/fixture/<lifetime>/handle.json`, which must be in the
  `ready` state. The harness reads `cli`, `control`, `environment` (the client
  environment) and `indexes.<repo>.index_id`.
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
- Scenarios in `R/scenarios/<repo>/<slot>.json` (formats.md).
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
python -I -B scripts/arbiter-answer-eval/run_session.py --scenario R/scenarios/airk/C1.json \
    --path focus --configuration focus --device cpu --handle R/fixture/<lifetime>/handle.json --series 01-baseline
# coverage and judging of that session
python -I -B scripts/arbiter-answer-eval/coverage.py --scenario ... --session-dir ...
python -I -B scripts/arbiter-answer-eval/judge.py --scenario ... --session-dir ... --key airk-C1/focus-cpu \
    --out-dir R/passes/01-baseline/judging/airk-C1/focus-cpu
# a whole pass (resumable); --only follows fixture lifetimes
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec <spec.json> [--stage sessions --only focus-cpu]
python -I -B scripts/arbiter-answer-eval/run_pass.py --spec <spec.json> --stage second   # close calls
# replay recorded Answer calls with a change set's defaults
python -I -B scripts/arbiter-answer-eval/replay.py --handle ... --sessions R/sessions/01-baseline --out R/replays/cs1-a
```

## The loop: gates (LG) and the new baseline (L7)

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

**Arms** (`make_loop_spec.py gate`, one series on both repositories):

| Arm | Result name | Focus | Context | Judged |
| --- | --- | --- | --- | --- |
| F | `focus-gpu` | the gate's Focus route (W8, 1,024 tokens) | none | yes (12.7.2) |
| C | `compact-gpu` | the gate's Focus route | Compact FP16 CUDA, top score | yes (12.7.1, 12.7.2) |
| L | `large-gpu` | the gate's Focus route | Large FP16 CUDA, top score | yes (12.7.1) |
| C32, L32 | `compactFp32-gpu`, `largeFp32-gpu` | the gate's Focus route | Compact or Large FP32 CUDA | replays; sessions only if escalated |
| CF16, CF32 | `compactFocusFp16-gpu`, `compactFocusFp32-gpu` | Focus FP16 or FP32 CUDA | Compact FP16 CUDA | replays; sessions only if escalated |

- **Arm identity.** Each entry's `arm_check` is checked on every gate replay
  and on the digest call before any session; `run_pass.py` refuses a session
  whose live reply is not the arm's. Focus only means no decision names a
  Context model and `context_absent` is disclosed. C and L mean every model
  decision is the arm's model on CUDA at its precision by `top_score`, with no
  abstention. Focus's own precision is not visible in a reply; the switch
  receipts record which profile was enabled.
- **The in-lifetime Context switch** is the named hook `switch_context` of
  `run_plan.py` (plan 12.5.2 item 4). It disables the live CUDA Context (the
  release) before it enables the next profile (the warm), through
  `gpu_fixture.py stage`, and writes a receipt with release and warm time and
  device memory. Sessions groups with `"stager": "cuda"` stage through it. A
  resumed plan names the live stages with `--live-cuda`.
- **Tests.** `gate.py replays` (tests 1 and 2), `agreement` (test 1),
  `outcomes` (test 3, with the close calls written as a jobs file for
  `run_pass.py --stage second --second-jobs`), `concurrency` and
  `performance` (test 4), and `precision` (12.7.3). Gate sessions are never
  reused across arms.
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
  reasons on bookbuilder scenarios, so no bookbuilder source is quoted.
- **L7.** `make_loop_spec.py l7 --keep compact[,large]` writes the new
  baseline's spec and plan: Search, Parse, the Focus-only loop and the kept
  CUDA routes, with no CPU cells. `l7_plan_template.json` is the same plan with
  placeholders, regenerated by `make_loop_spec.py template`; a test keeps it
  current.
- **Prompts freeze.** `loop_reply.py` and `gate.py` are runtime harness files
  in `prompts_freeze.HARNESS_FILES`. The freeze written for the one-turn
  evaluation no longer verifies, because the Answer request and budget
  changed. It is re-frozen with `prompts_freeze.py --refreeze` before the first
  judged loop pass. That re-freeze is the operator's first step at LG.

```text
# LG, after L5 and its short trial (R is the private root)
python -I -B scripts/arbiter-answer-eval/prompts_freeze.py --refreeze R/design/prompts-freeze.json --reason "loop harness (Answer /3) for LG"
python -I -B scripts/arbiter-answer-eval/make_loop_spec.py gate --series <NN>-gate-lg
python -I -B scripts/arbiter-answer-eval/run_plan.py --plan R/passes/<NN>-gate-lg/plan.json --log R/passes/<NN>-gate-lg/plan-log.jsonl
python -I -B scripts/arbiter-answer-eval/gate_report.py --inputs R/passes/<NN>-gate-lg/gate-report-inputs.json --out docs/arbiter/answer-evaluation/<NN>-gate.md
```

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

- **Evidence text.** A note is covered by line overlap (rule 2: at least
  min(5, its line count) lines) only through evidence text (`hits[].text`,
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
  two correct finishes. `--stage second` runs a second session for the Answer
  configuration and the better of Parse and Search. Samples that differ give
  "undecided".
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
uses synthetic data only and starts no model or service. `test_fixture.py`
belongs to `fixture.py`.
