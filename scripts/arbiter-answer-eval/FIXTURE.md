# Answer evaluation fixture (`fixture.py`)

Evaluation tooling, never product. It owns the private evaluation service that
the Answer evaluation's agent sessions call (plan
`docs/arbiter/final-plan-fp16.md`, section 3.1). It runs on Windows as the
verified human, CPU only. Nothing is written into either repository under
test.

For a fresh FP16 MiniLM candidate, set `AIRK_EVALUATION_MINILM_DIRECTORY`
to the provisioned profile directory before starting the fixture or base
driver. The native config emitter authenticates that exact directory against
the built image, and the fixture records the requested path. Retained
historical runs use their original MiniLM inputs.

## Purpose

- Start one **fixture lifetime**: the `airk-service` test image run on its
  `actual_private_arbiter_fixture_software_v2` selector inside an owned
  Windows Job, with the reviewed resource consent (Throughput, 64 GiB,
  4 inference threads, GPU use off).
- Make sure the named indexes exist with MiniLM vectors and watching off. The
  default is the loop series' pair, `bookbuilder-v2` and `munarium` (user
  direction, 22 November 2026: the index under test holds nothing about the
  experiment, so no loop lifetime holds an airk index). Each is indexed from
  a `git archive` export of one commit; neither checkout is written to. A
  repository's denies are applied through the index's own source policy (a
  `trim_path` source draft applied with `CatalogApply`), before its first
  index pass; no ignore file is added to any repository. `airk` and the
  in-place `bookbuilder` index remain known names for the one-turn history
  root.
- Write `handle.json`, which holds everything a client needs, then stay up
  until the stop file appears or the lifetime deadline passes, and stop
  through `service shutdown` with settlement checks.
- Reuse: `start --restart-from <closed lifetime>` reopens the same control
  root, catalog, installed model weights and indexes (same-install restart,
  `evaluation_fixture_tests/restart.rs`). Indexes are built once.
- Enable or disable Focus and Context with a named profile (`stage`).
- Check that no index serves a path its own exclusion list names, and that
  the lifetime holds no airk index (`leak-probe`), and time one call of each
  read tool (`smoke`).

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
- For `leak-probe`: the scenario JSON files under
  `<evaluation>/scenarios/*/*.json` and `<evaluation>/sealed/*/*.json` of each
  probed index's repository (the index's own name and every name that
  `<fixtures>/index-aliases.json` points at it, so `bookbuilder-v2` takes the
  `bookbuilder` scenarios), or fallback `--phrase` values when none exist yet.

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
start --image <image folder> --lifetime <name> [--restart-from <closed lifetime>] [--seconds N] [--repositories bookbuilder-v2,munarium]
stop --lifetime <name>
stage --lifetime <name> --profile <name> --enable|--disable [--repositories …]
leak-probe --lifetime <name> [--repositories …] [--phrase …] [--allow-airk-index]
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

`leak-probe` probes every index of the lifetime (or `--repositories`) against
that index's own exclusion list:

- It takes a repository's phrases from `scenario-freeze.json` when the freeze
  holds scenarios of that repository, re-pinning every listed file; otherwise
  it scans the scenario folders.
- It searches each phrase and checks every returned path, then lists every
  indexed path and checks it.
- An index without exclusions (munarium) is probed the same way. Its result
  records the empty list, so the receipt shows that the probe ran and what it
  compared against.
- It fails when the lifetime holds an airk index: in the handle, as a state
  file, or in the service's own index catalog (collection name or source
  root). `--allow-airk-index` is for the one-turn history root only.

The result is `<fixtures>/leak-probe-<n>.json`, schema
`airk.private.answer-eval-leak-probe/2`, with one entry per repository.

## Limits

One lifetime lasts at most six hours (the selector's bound). The dispatch
observer admits 1,024 native dispatches per lifetime unless the lifetime names
a census: `start --dispatch-census N` (1 to 65,536; the service variable
`AIRK_NATIVE_DISPATCH_CENSUS_BOUND`), and its record and byte bounds grow with
it. A gate lifetime gets N from its plan (`make_loop_spec.py gate`: the
Answers the plan sends it times the budget's Focus and Context calls); the LG
gate's lifetimes ended on the old fixed 1,024. Pass `--no-dispatch-observer`
for long runs that do not need the observer. A restart needs the
previous lifetime closed cleanly with idle stages. Results describe this
private image and host, not the installed product.

## Tests

From this folder: `python -I -B -m unittest discover -s . -p test_fixture.py`
(synthetic data only; starts no service).

## Repositories are indexed from commit exports

A repository in `SNAPSHOTS` is indexed from `sources/<repo>-<commit12>/`, the
tracked tree of its pinned commit exported with `git archive` (receipt
`sources/<repo>-<commit12>.json`: commit, archive SHA-256, file count). The
export fixes the indexed bytes at the scenarios' `source_commit` while the
checkout keeps changing, and the checkout is only read (git queries run with
`GIT_OPTIONAL_LOCKS=0`).

| Index | Checkout | Commit | Tracked files | Denies |
| --- | --- | --- | --- | --- |
| `bookbuilder-v2` | `D:\code\Github\bookbuilder` | `238b22a722ce…` | 8,250 | `ironspire/scripts/set-rss-key.ps1`, `ironspire/scripts/set-sendgrid-key.ps1` |
| `munarium` | `D:\code\Github\munarium` | `9948345d1d14…` (feat/clearview) | 1,038 | none |
| `airk` (history) | `D:\code\Github\airk` | `1fe7b6003574…` | 10,459 | the five benchmark paths |

**munarium has no deny.** It holds none of the benchmark's own files. Its two
credential-like names at the commit are examples without a secret, read at the
commit on 22 November 2026:

- `docs/lab/example/.env.example`: every credential value is empty;
- `server/deploy/terraform/example-aks/example.tfvars`: its header says every
  value is invented, and none is a credential.

A scan of the commit for private-key blocks and cloud-key shapes found
nothing. An index's source policy holds no rule until a deny is applied (a
fresh index's catalog entry has `policy: null`), so no policy leaves such
names out. What the built index holds was measured on root
`06-gate-lg-C` (22 November 2026, paths only):

- the index lists 1,023 of the 1,038 tracked files;
- the 15 it leaves out by itself are 13 PNG images, one JAR and
  `docs/lab/example/.env.example`;
- `server/deploy/terraform/example-aks/example.tfvars` is indexed;
- the credential probe's one match is a guide,
  `server/docs/guides/managing-key-and-secrets.md`.

The credential probe on each built index lists the credential-like names that
were in fact indexed, for a reader (`<root>/credential-probe-<index>.json`).

airk needed an export for another reason too: the service refuses a source
root that contains its control root or index storage, and the fixture keeps
both under `artifacts/` inside the airk checkout. The in-place `bookbuilder`
index is history.

## Source policy comes before any index pass

The launcher sets up every repository and brings its denies into force before
it queues any index pass. Four facts of the service shape this. Each was
found on 22 November 2026, when three `05-gate-lg` bases and the first
`06-gate-lg` base failed or were stopped.

1. **A deny needs a quiet installation.** The change's authority fence drains
   the provider the projects share.
   - The service refuses the change's review while its own index has jobs
     (`source_change_busy`, code `source_change_jobs_pending`). A `verify` job
     follows a policy application when the setup's index job had already begun
     before it was cancelled.
   - Applied while another project's `vectors` job ran, a deny ended with an
     unknown outcome in two roots (`catalog_outcome_unknown`, fence pending;
     it completed about a second later). In a third it returned applied, but
     the other project's vectors job was pushed back to pending and failed
     (`project_operation_failed`).
2. **`progress` alone is not proof of quiet.** One sample fell into the 48 ms
   between an index pass's success and its vectors job. The registry's
   `service_background` flags hold the work the service has promised but not
   yet queued, and the service sets them in the transaction that completes the
   earlier job.
3. **The MiniLM install starts an index pass.** Installing MiniLM through a
   project queues that project's index pass by itself (181 ms after the
   install's configure job, with no request from the launcher).
4. **An unknown outcome is not a refusal.** The service refuses an index pass
   queued behind a pending fence (`invalid_registry_request`).

What the launcher does about them:

- **Order.** MiniLM is installed through the first repository, once its
  denies are in force. Only the first repository may still need a deny on an
  installation without MiniLM; `start` refuses any other order before launch
  (`repository_order_problem`). So the pair is listed `bookbuilder-v2,munarium`.
  An installation that already has MiniLM takes a new repository with denies
  whenever it is quiet.
- **Quiet first.** Before each change the launcher waits until `progress`
  shows no unfinished job for any project and the registry's own snapshot
  agrees: no pending or running job (a parked automatic audit apart), no
  promised follow-up, no unfinished setup (`registry_pending`, read without
  writing). The wait has a deadline of 900 s and names what was unfinished.
- **Typed busy.** `source_change_busy` and, for the read-only review only,
  `registry_busy` have no effect. The launcher returns to the wait and records
  each refusal in the repository state.
- **Unknown outcome.** The launcher observes the change until the service's
  policy status names the deny, then goes on. A change that is not in force by
  the deadline fails the lifetime before any index pass is queued.
- **No deferral.** A deny that cannot be brought into force raises. The
  earlier fallback, which applied a refused deny after the first index pass,
  is gone.
- **The install's pass is adopted.** The launcher waits for the index pass the
  install started instead of queuing a second one. A pass that a stop
  cancelled is not adopted by the next lifetime.

The repository state (`indexes/<repo>.state.json`) records for each deny its
outcome, the wait, any busy refusals, the fence observation and the artifacts
the application removed.

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
