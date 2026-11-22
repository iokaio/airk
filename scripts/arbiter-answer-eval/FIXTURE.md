# Answer evaluation fixture (`fixture.py`)

Evaluation tooling, never product. It owns the private evaluation service that
the Answer evaluation's agent sessions call (plan
`docs/arbiter/final-plan-fp16.md`, section 3.1). It runs on Windows as the
verified human, CPU only. Nothing is written into either repository under
test.

## Purpose

- Start one **fixture lifetime**: the `airk-service` test image run on its
  `actual_private_arbiter_fixture_software_v2` selector inside an owned
  Windows Job, with the reviewed resource consent (Throughput, 64 GiB,
  4 inference threads, GPU use off).
- Make sure two indexes exist with MiniLM vectors and watching off: the
  `airk` and `bookbuilder` checkouts. The airk benchmark paths are denied
  through the index's own source policy (a `trim_path` source draft applied
  with `CatalogApply`); no ignore file is added to any repository.
- Write `handle.json`, which holds everything a client needs, then stay up
  until the stop file appears or the lifetime deadline passes, and stop
  through `service shutdown` with settlement checks.
- Reuse: `start --restart-from <closed lifetime>` reopens the same control
  root, catalog, installed model weights and indexes (same-install restart,
  `evaluation_fixture_tests/restart.rs`). Indexes are built once.
- Enable or disable Focus and Context with a named profile (`stage`).
- Check that the airk index serves nothing from the benchmark paths
  (`leak-probe`) and time one call of each read tool (`smoke`).

## Inputs

- An image folder holding `airk-service-fixture.exe`, `airkctl.exe` and
  `images.json` (SHA-256 pins, Cargo commands, Git head). The launcher checks
  both pins before launch. Build the images once and copy them out of
  `target/`, because later change sets rebuild the target.
- The profile roster (`--inputs`, default the three-profile CPU roster
  `cal100-focus1024-launch-r1/inputs.json`, SHA-256 `8fbde10f…373d`). The
  selector requires exactly one Focus, one Compact and one Large profile, and
  a fresh installation copies all three weight files (about 24.1 GB).
- The bundled MiniLM (`artifacts/advanced-split/bundled-model-v3/minilm-l6-v2-u8-v3`).
- For `leak-probe`: every scenario JSON under `<evaluation>/scenarios/*/*.json`
  and `<evaluation>/sealed/*/*.json` with `repository: airk`, or fallback
  `--phrase` values when none exist yet.

## Outputs (all under the git-ignored fixture root)

| Path | Content |
| --- | --- |
| `<lifetime>/handle.json` | State (`starting`, `indexing`, `ready`, `closed`), CLI path, control root, client environment, service environment, service PID and instance, image pins, index id, binding and publication token per repository, stop file, deadline, final status and settlement receipts |
| `<lifetime>/fixture/` | The fixture's own work folder: `ready.json`, `closed.json`, profile snapshots, the Answer template `context-request.json`, warm and dispatch journals, and (first lifetime only) `control/` |
| `<lifetime>/commands/` | Every CLI call: argv, exit code, elapsed time, stdout and stderr |
| `<lifetime>/owned.json` | The owned Job's settlement report |
| `indexes/<repo>.state.json` and `indexes/<repo>/` | Index registration, trims, indexing time and commit, and the index data |
| `leak-probe-<n>.json` | One leak probe result |

## Use

Run Python as `python -I -B -S scripts/arbiter-answer-eval/fixture.py …`.

```text
start --image <image folder> --lifetime <name> [--restart-from <closed lifetime>] [--seconds N] [--repositories airk,bookbuilder]
stop --lifetime <name>
stage --lifetime <name> --profile <name> --enable|--disable [--repositories …]
leak-probe --lifetime <name> [--phrase …]
smoke --lifetime <name> --output <file> [--with-models] [--compact]
```

More commands for a running lifetime:

```text
progress --lifetime <name>                         # sample semantic progress into progress.json (+ progress-samples.jsonl)
add-index --lifetime <name> --repository <repo>    # set up and lexically index one more repository
settle-index --lifetime <name> --repository <repo> # cancel that repository's jobs and wait until it stays idle
```

`progress` reads each index's `airk_status` and, read-only, the registry's job
events (vectors jobs per hour over 20 and 60 minutes), and projects the finish
from the published-artifact rate of the last 30 minutes.

Selectors: `start --selector software-v2` (default) or `--selector
change-set-1` (`evaluation_fixture_tests/restart/change_set.rs`) with
`--change-set-inputs <json> --change-set-inputs-sha256 <sha>`, optional
`--answer-layers all|none|<list>|all,-focus`, and on a fresh lifetime only
`--seed-cpu all|focus|none`. These become the selector's
`AIRK_ARBITER_EVALUATION_CHANGE_SET_*` and `…_ANSWER_LAYERS` variables, set
after the launcher strips inherited `AIRK_*` variables. `images.json` may use
either the baseline shape (`service.image.sha256`) or the change-set shape
(`service.sha256`).

`start` blocks for the whole lifetime; run it in the background and wait for
`handle.json` to reach `ready`. A client calls a tool as

```text
<handle.cli> --output json --service-runtime-dir <handle.control> tools call <tool> --index <handle.indexes[repo].index_id> --args <json>
```

with the environment `os.environ` minus every `AIRK_*` variable, plus
`handle.environment`.

Profiles: `focus-static1024-cpu`, `context-compact-cpu`, `context-large-cpu`.
The `*-cuda` names are hooks only; they raise with what is still missing
(plan section 7): a CUDA Focus selector, the CUDA Answer selector in
`evaluation_fixture_tests/cuda_answer.rs` (Compact only, fresh lifetime,
`gpu-cuda` image) and its extension to Large.

The stage request is written by the fixture's own
`emit_private_arbiter_configuration` selector. The launcher passes it a
SHA-pinned scope file with the repository roots and the declaration
`AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES={"mode":"read-only","source_roots":[…]}`
naming the same roots, which the emitter requires for checkouts outside
`artifacts/` (see the fixture's `CONTRACT.md`). The launcher checks the
emitted request against the requested indexes and the profile's tasks, then
applies it with `arbiter configure --legacy`.

`leak-probe` takes the airk phrases from `scenario-freeze.json` when it
exists and re-pins every listed file; otherwise it scans the scenario folders.

## Limits

One lifetime lasts at most six hours (the selector's bound). The dispatch
observer holds at most 8,192 records per lifetime; pass
`--no-dispatch-observer` for long runs that do not need it. A restart needs the
previous lifetime closed cleanly with idle stages. Results describe this
private image and host, not the installed product.

## Tests

From this folder: `python -I -B -m unittest discover -s . -p test_fixture.py`
(synthetic data only; starts no service).

## airk is indexed from a commit export

The service refuses a source root that contains its control root or index
storage, and the fixture must keep both under `artifacts/` inside the airk
checkout. So airk is indexed from `sources/airk-<commit12>/`, the tracked
tree of the pinned commit exported with `git archive` (receipt
`sources/airk-<commit12>.json`: commit, archive SHA-256, file count). This also
fixes the indexed bytes at the scenarios' `source_commit` while the checkout
keeps changing. bookbuilder is indexed in place (read-only; git queries run
with `GIT_OPTIONAL_LOCKS=0`).

## Known hazard

The same-install restart refuses a control root with any pending or running
job. Setup queues an initial index job by itself, so a lifetime must either
let every job finish or cancel it and wait until the cancellation settles
before `service shutdown` (the worker does this). A job left `running` with
its cancellation requested makes the installation unrestartable; only a fresh
installation recovers. Setup also finishes asynchronously (its `model_setup`
and `inspect` jobs, then its own index job): until then the project is
"setup pending", which the restart guard also refuses, so the worker waits for
setup's index job before cancelling it. After the service exits, the worker
reads the registry read-only for exactly the guard's conditions and records
`restartable` and `restart_blockers` in `handle.json`; a lifetime that would
not restart fails at once instead of at the next start.

A stop file written while the worker waits for an index or vectors job ends
that wait; the worker cancels and settles the job, shuts down, and the next
`start --restart-from` resumes (an interrupted embedding series is resumed by
one more index pass). Such a lifetime reports an error but stays restartable.
