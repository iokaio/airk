# Context trainer, gate 1: oracle leverage collector
## Verify an Answer V3 development card capture

`verify_card_capture.py` checks private native score custody for one
closed Windows CUDA same-install development fixture. It does not run
an inference, replay, fitter, or grader. Use a checked same-human shell.
Audit the work directory and each file's owner before obtaining the pins.
Pass the same expected owner SID that was verified against the host session.

```powershell
python -I -B scripts/arbiter/training/context_answer_eval_r1/verify_card_capture.py
    --work <native-work-directory> --owner-sid <verified-Windows-SID>
    --capture-sha256 <development-card-calls.jsonl>
    --closure-sha256 <development-card-calls.closed.json>
    --ready-sha256 <ready.json> --closed-sha256 <closed.json>
    --journal-sha256 <composed-observer.jsonl>
    --journal-closure-sha256 <composed-observer.closed.json>
```

Run the example as one shell command and replace each receipt name in
angle brackets with its exact SHA-256. All six files must be direct
children of the supplied work directory inside `artifacts/`. Pins
authenticate exact input bytes against the checked custody that supplied
them. The native Ready and close receipts must share the service instance
and control root, and prove shutdown and lease reacquisition. Both recorder
closures must account for all bytes and records without loss. The card
capture closure also binds its stream by native SHA-256.

The verifier checks the recorded order of card segments and advice,
request-attempt and slot uniqueness, and exact native f32 score bits.
Every captured scored call must consume a distinct native dispatch with the
same profile, task, complete scope, input digest, token count, timings and
score bits. The journal must witness admission, score return, job settlement
and profile and charge release for matched calls. If the card capture missed
its compiler callback, the journal can witness that callback's compiled input.
Unmatched native card returns may be undelivered or late and remain visible
as a count. Captured refusals remain in the accounting.

The JSON report contains input hashes and aggregate counts by task and
profile. It establishes native execution custody for the retained capture.
Delivered Answer decisions and coverage require separate checks against the
replies. Keep the source receipts and development-only corpus private.

Run the in-memory regression checks with
`python -I -B scripts/arbiter/training/context_answer_eval_r1/test_verify_card_capture.py`.

Evaluation tooling of the AIRK Answer evaluation (plan `docs/arbiter/final-plan-fp16.md`,
sections 5.2 and 5.4), never product. It measures how much the Context action could change
what an Answer call delivers, before any Context training: the first gate of the Context
trainer design (its sections 2.2, 8 "Collector details", 9 step 1 and 10). No model runs.

## What it measures

For one first `airk_answer` call, the change-set gathering runs with Context at each decision
point (ordinal 1, optionally 2) doing one of:

- **queue**: abstain at every ordinal, so the queue order runs (what an abstaining chooser gets);
- **fork `oN-rK`**: accept the Read card of the K-th offered read at ordinal N, abstain elsewhere.

Every branch runs to completion under the call's own budget, and its delivered packet is
rendered as the Answer handler renders it (the agent's response mode, terse by default). The
scorer applies the harness coverage rule (`scripts/arbiter-answer-eval/evidence.py`, rule 2,
text spans only, imported, not copied) to each branch against the scenario's essential notes.

Per decision point: queue-order coverage, best forced coverage, the signed best-forced gain and
the oracle gain `max(0, best - queue)`. Per ordinal: the oracle leverage (share of calls with an
oracle gain of at least one essential note) and the mean oracle gain. A call that never reached
a Context request counts as gain 0. With Compact's recorded choices, the share of the oracle gain
provisional Compact captured. The summary applies the design's section 10 thresholds:

| Decision | Rule |
| --- | --- |
| no-go: fix the roster, not the weights | leverage < 10% of calls, or mean oracle gain < 0.2 |
| no-go: no room | Compact captures >= 70% of the oracle gain |
| go (pending the non-win attribution) | leverage >= 20%, gain >= 0.5, and Compact captures < 40% or does worse than queue order |
| between: collector and labels only | otherwise |

The go row also needs at least two non-winning scenarios caused by "the model chose badly" or
"retrieved and not followed" where the oracle fork covers the note. That is pass analysis; the
report lists the candidate calls and notes (`non_win_candidates`).

## How the collector runs

`crates/airk-mcp/src/context_action_capture_tests/software_forks.rs`, the ignored test
`collect_context_action_oracle_forks` (features `local-inference,test-support`).

- **In process:** `Kernel::answer_with_reader_v2` with the change set's layers (`Layers::parse`,
  bound once per process; Focus must be off because Focus kept-hits need the model), roster
  building, the joint actuation, packing and the Answer render. The kernel is the read-only
  observer of the live index (`Kernel::open_existing_service_observer`: no creation, recovery,
  cleanup or model initialization), under a whole-index grant carrying the live index id, its
  source root and its permanent source policy (epoch and digest). A copy of the data directory
  is refused by design: a service collection binds its owner record to its data-root path.
- **Publication:** every dispatched read carries the observer's publication as its revision.
  The collector refuses one that differs from the live token `prepare_forks.py` read, and the
  gathering marks any live reply with another token stale. The live index must not publish
  during the run (indexing and embedding settled).
- **In the live evaluation service:** every canonical inner read, through the agent's CLI tool
  path (`<cli> --output json --service-runtime-dir <control> tools call airk_<tool> --index <id>`),
  with exactly the argument rewrite of `AirkServer::workbench_read` for a standalone Answer
  (`response_mode` full, `include_status`, `verification` off, `revision` on
  read_file/file_info/outline/context, `compact` false on fact reads). Failures come back as the
  same `ReadFailure` receipts; typed retries follow the harness rule. A transport failure marks
  the branch invalid. A search reply carrying a Focus receipt stops the run: the lifetime's
  Focus stage is on, the design's collector is Focus-off, and the kernel refuses a Focus receipt
  not bound to its own Focus attempt in any case.
- **Controller:** a fixed oracle harness (`NativeController`), not a model. Abstentions and the
  forced acceptance carry fixed placeholder identities and `assessment_input_tokens` per call
  (default 2,048, about one Compact call). Each fork checks that it was offered the same roster
  as the queue branch at its ordinal (`same_roster_as_queue`, compared without card ids, which
  are minted per assessment); a differing roster is excluded. A call whose queue branch is
  invalid fails the test after every row is written.

Because the gathering runs in the collector's process, its source must be the image's.
`prepare_forks.py` compares every in-process source file (kernel, core, retrieval arbiter, MCP;
tests and the collector itself excluded) against the image's `images.json` `changed_files` and
HEAD, and refuses a mismatch unless `--allow-source-mismatch` (recorded). Build and run the test
from the same tree you checked.

## Inputs

- A live lifetime's `handle.json` (CLI, control directory, client environment, index ids and
  aliases) and its image's `images.json`. The fixture owner starts and stops lifetimes; this
  tool never does.
- The repository state files `R/fixture/indexes/<index>.state.json` (index id, binding, source
  root, data root) and each collection's `source-policy.json` (owner source root id).
- The scenarios `R/scenarios/<repo>/<slot>.json`. With `--series`, a session series whose first
  `airk_answer` calls are replayed instead of the canonical first call.

## Outputs (private, under `R/training/context-cand1/collector/`)

- `input-<lifetime>.json`: the collector input; `prepare_forks.py` prints its framed digest.
- `rows-<lifetime>.jsonl`: a `start` row, one `call` row per call (queue branch and forks: the
  Context requests seen, native stop/work/records/assessments, every live read's arguments,
  exit and time, and the rendered packet), and a `complete` row.
- `score-<lifetime>.json`: per-point coverage, summaries, Compact capture and the verdict.

## Use (the harness agent, on a live lifetime started on a cs1d or cs1e image)

Run as the verified human (`. scripts\dev\assert-human-execution.ps1`, `Assert-AirkHumanExecution`),
from `D:\code\Github\airk`, with `$R` the evaluation folder and `$L` the lifetime name.

1. **Gap and stages.** The handle is `ready`; both indexes are settled (semantic `ready`, no
   index, vectors or verify job, and two `airk_status` reads a minute apart give the same
   publication token); the lifetime's Focus stage is disabled (`fixture.py stage --lifetime $L
   --profile focus-static1024-cpu --disable`, only when no pass is using the lifetime). The
   Context stage does not matter: the collector never calls the service's Context model.
   Nothing is copied and nothing is written under the index.
2. **Prepare** (canonical first calls with change set 1's budget, ordinal 1):

   ```powershell
   python -I -B scripts\arbiter\training\context_answer_eval_r1\prepare_forks.py `
     --handle "$R\fixture\$L\handle.json" `
     --image-record "$R\fixture\images\<cs1e label>\images.json" `
     --out "$R\training\context-cand1\collector\input-$L.json"
   ```

   Add `--fork-ordinals 1,2` for the ordinal-2 forks, `--series $R\sessions\<series>
   --configurations compact` to replay recorded first calls, `--repos`/`--slots` to narrow.
3. **Collect** (the Cargo owner; no GPU). First set the image's build environment (`environment`
   in its `images.json`: `CARGO_PROFILE_DEV_OPT_LEVEL=2`, `CARGO_INCREMENTAL=1`,
   `ORT_LIB_LOCATION`, `ORT_LIB_PROFILE`, `ORT_SKIP_DOWNLOAD`, `ORT_PREFER_DYNAMIC_LINK`,
   `LIBONNXRUNTIME_NO_PKG_CONFIG`, `AIRK_BUNDLED_MODEL_DIR`); without `ORT_LIB_LOCATION` the test
   binary does not link.

   ```powershell
   $env:AIRK_CONTEXT_FORKS_INPUT = "$R\training\context-cand1\collector\input-$L.json"
   $env:AIRK_CONTEXT_FORKS_INPUT_DIGEST = '<digest printed in step 2>'
   $env:AIRK_CONTEXT_FORKS_OUTPUT = "$R\training\context-cand1\collector\rows-$L.jsonl"
   cargo nextest run -p airk-mcp --no-default-features --features local-inference,test-support `
     --offline --timings --run-ignored ignored-only --no-capture `
     -E 'test(=context_action_capture_tests::software_forks::collect_context_action_oracle_forks)'
   ```

   The default nextest profile has no slow-test termination. A full run is about 20 calls times
   up to 6 branches times about 12 live reads (one live search took about 3 s in the smoke).
   `AIRK_CONTEXT_FORKS_ONLY=<id,id>` runs a subset (one output file per run; the output must not
   exist).
4. **Score:**

   ```powershell
   python -I -B scripts\arbiter\training\context_answer_eval_r1\score_forks.py `
     --rows "$R\training\context-cand1\collector\rows-$L.jsonl" `
     --out "$R\training\context-cand1\collector\score-$L.json" `
     --compact-sessions "$R\sessions\<cs1 series>" --compact-configuration compact
   ```

   Compact's choice is read from the first Answer reply's ordinal-1 assessment; a choice that is
   not among the collector's offered reads (the pass ran with Focus on) is reported as unmatched.
   `--compact-choices <json>` supplies `{call or scenario id: {"choice": hex} | {"abstained": true}}`.
5. Re-enable the Focus stage if step 1 disabled it.

## Limits

- Forks are deterministic canonical reads against one publication; they measure what Context
  could change under option A, not what a trained model would choose.
- Focus is off in the collector, while the evaluated configurations run Focus on; the queue
  differs (design section 8). Compact's recorded choices may not map onto the collector's rosters.
- The placeholder token charge per assessment decides how many ordinals the token budget admits;
  it is recorded in every `start` row.
- One local Windows run; no cross-platform or qualification claim.

## Tests

- `python -I -B scripts\arbiter\training\context_answer_eval_r1\test_context_answer_eval.py`
  (synthetic scenarios and rows, hand-written expectations; standard library only).
- `forced_offered_read_is_accepted_and_runs_next` in the collector file (not ignored; same
  features): on a small local index, the queue branch abstains and runs the first offered read
  next, and a forced second read passes the kernel's advice checks, is selected and runs next.
