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
| `pass_report.py` | Per-session and per-path figures of a graded pass (turns, bytes, tool and wall time, grades, agent and judge cost) for reports and cost projections |
| `settled.py` | The settled-index guard: semantic readiness `ready`, served mode `hybrid`, no index or vectors job queued or running, and the publication token at start and end (read-only) |
| `prompts_freeze.py` | Writes and verifies `R/design/prompts-freeze.json`: the SHA-256 and text of every agent prompt, the judge prompt and its schema, the default models and efforts, and the hashes of the runtime harness files |
| `depth.py` | The no-agent depth measurement (plan E4). For each essential note it gives the rank at which an `airk_search` of the instruction (limit 100, the tool's maximum; no paging) first covers it |
| `rescore.py` | Re-scores a graded pass under another coverage rule. It writes `coverage-rule<N>.json`, `results-rule<N>.csv` and `summary-rule<N>.json`, and leaves the original results alone |
| `rejudge.py` | Re-judges a pass's identical packets with another judge model and effort, then compares grades, per-fact verdicts and Rubric B with the original verdicts |
| `probe_tools.py` | One real call per tool through the wrapper against a live handle, with the spans `evidence.py` finds in each result |
| `mock_cli.py`, `synthetic.py` | A stand-in CLI serving canned synthetic results, and the invented fixture (the "lantern" repository) used by the tests and the smoke run |

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
- **`airk_answer`.** The wrapper always sets `schema` and `budget`. The budget
  defaults to the plan's 8 calls, 3 rounds, fanout 8, 8,192 input tokens,
  32 candidates, 262,144 bytes and 120 s. `--answer-defaults` can change it,
  or add other fields, for a configuration.
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

`test_harness.py` covers the wrapper over real stdio (newline and
`Content-Length` framing), the tool lists, the first-call rule, the turn cap,
argument injection, the log format, the coverage edge cases, the grade and
case tables, the configuration rule, headroom, the CSV columns, the audit,
judge packets and verdict validation, and a resumable pass with reuse. It
uses synthetic data only and starts no model or service. `test_fixture.py`
belongs to `fixture.py`.
