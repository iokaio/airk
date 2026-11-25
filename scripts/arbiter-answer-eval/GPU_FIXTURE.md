# Answer evaluation fixture on CUDA (`gpu_fixture.py`)

Evaluation tooling, never product. It runs the GPU cells of plan
`docs/arbiter/final-plan-fp16.md` section 3.4 on Windows with the RTX A6000,
as the verified human. It reuses `fixture.py` unchanged (imported, not edited).

## Purpose

- Start one fixture lifetime on the CUDA selector
  `cuda_answer::same_install::actual_private_arbiter_fixture_cuda_same_install`.
  With `--restart-from` it reopens the CPU fixture's own installation: the
  same control root, catalog, CPU weights and indexes. The CPU roster stays
  the restart identity; one CUDA Context profile (Compact or Large) joins the
  evaluation catalog only, and its weights are read from the pinned roster's
  files, never copied into the installation. GPU use is opted in through the
  reviewed resource request (`allow_gpu: true`, `gpu_opt_in: user_choice`).
- Stage CPU or CUDA profiles (`context-compact-cuda`, `context-large-cuda`,
  plus every CPU name of `fixture.py`).
- Run one timed Answer and record its device evidence: each Context
  assessment's runtime (`onnxruntime-cuda/1.30.0/device-execution/...` on the
  GPU) and profile, the native dispatch census from `composed-observer.jsonl`
  (profile, raw scores, inference time) and `nvidia-smi` memory before and
  after.
- Compare Answer records by `evidence.evidence_digest` (equal evidence on CPU
  and GPU for the same request).

There is no CUDA Focus route: Focus runs on CPU in every GPU lifetime.

## Inputs

- A runnable image directory: `airk-service-fixture.exe` built with
  `gpu-cuda` and both compiled CUDA approval pins, `airkctl.exe`,
  `images.json`, and the reviewed CUDA payload as direct children
  (`airk-cuda-runtime.json`, its producer receipt and the DLLs it lists).
- The CPU roster (default: `fixture.py`'s), and per model a single-entry
  CUDA roster and the reviewed final-graph placement result, pinned by
  SHA-256 in `MODELS`. The adapter UUID comes from that result.
- A fresh lifetime names `--seed-cpu all|focus|none` (which CPU weights it
  installs); a restart never seeds.

## Outputs

The same lifetime folder as `fixture.py` (`handle.json`, `fixture/`,
`commands/`, `owned.json`) plus `gpu.json` (model, pins, script hashes),
`stages.jsonl`, and one JSON record per `answer` call.

## Use

Run Python as `python -I -B scripts/arbiter-answer-eval/gpu_fixture.py …`.

```text
[--fixtures <root>] start --image <dir> --lifetime <name> --model compact|large
    (--restart-from <closed lifetime> | --seed-cpu all|focus|none)
    [--repositories bookbuilder-v2,munarium] [--synthetic-root <git tree>] [--seconds N]
[--fixtures <root>] stage --lifetime <name> --profile <name> --enable|--disable
[--fixtures <root>] answer --lifetime <name> --repository <repo> --question <text> --label <l> --output <file>
compare --records <file> <file> ... --output <file>
[--fixtures <root>] stop --lifetime <name>
```

A same-install GPU lifetime belongs in the CPU fixture's chain: start it from
the latest closed lifetime under the same fixtures root, and start the next
CPU lifetime from it. Disable the CUDA Context stage (or re-stage a CPU
profile) before stopping, so the next CPU lifetime starts from a CPU
selection.

## Change set 1 with Context on CUDA

`start --change-set-inputs … --cuda-variant-inputs …` runs the selector
`cuda_answer::cs1::actual_private_arbiter_fixture_change_set_1_cuda`. It is
the change-set lifetime: the CPU roster is the restart identity, the change
set's extras are catalog-only, and the kernel layers come from `--layers`
(default `all`). One CUDA Context profile is attached to it. That profile is a
calibration-only variant of a reviewed CUDA base (`CS1_BASES`) that carries the
provisional Read rule. The reviewed graph result names the base. The
bounded-hybrid gate admits the variant only if it equals the base except for
its Calibration asset, calibration, profile id and manifest. Graph, placement
and adapter are checked against the base.

Inputs (private, under R):

| Input | Path | SHA-256 |
| --- | --- | --- |
| Image (runnable, beside the CUDA payload) | `C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\answer-eval-gpu-r1\cs1e-cuda-f79755a7` | service `f79755a7…f9b1` |
| Change-set inputs | `R\change-set-1\profiles\change-set-inputs-r2.json` | `a0f26d834ec3fd322129e8daf28459133043f4690d17ec12d2b91afdf7094c59` |
| Compact CUDA variant inputs | `R\gpu-track\cuda-variants-r1\cuda-contextqwencompact-provisional-read-inputs.json` | `5cd0671b1adf3c9016972867d4022a246350dd1a9b18a6195d4d7568f73b31d1` |
| Large CUDA variant inputs | `R\gpu-track\cuda-variants-r1\cuda-contextqwenlarge-provisional-read-inputs.json` | `19be1393e19f91ec6ba2e490a1b92825d5e3b69662d98f6069d05239dd24d8ff` |

The Large variant reads its weights from `R\gpu-track\large-cuda-assets`, a
16 GB pinned copy. Recreate it if it is absent: see `R\gpu-track\reclaim.json`.

Manifests to stage:

| Profile | Stage | Device | Manifest |
| --- | --- | --- | --- |
| cs1-software-w16 Focus | focus | cpu | `f2d194b0070458d5af772802dcbb9ae5e27959746794f9bdfaa00eac9202830a` |
| Compact CUDA provisional-read | context | cuda | `ec3c8294a07ccbe9eb4d8bb6a3cdd33e6f85b3d0c0a12303685acc3c117517ab` |
| Large CUDA provisional-read | context | cuda | `9bc9831e32258db72ff9a5596bb19ad07be708079d1d5b7c3f007e5d0fdaa5e1` |

For the trained Focus candidate, stage its manifest from
`change-set-inputs-r2.json` the same way.

Exact commands against the real installation. `<prior>` is the latest closed
lifetime of the chain; `<name>` is a fresh lifetime. Run from the repository
root:

```text
set PY=python -I -B scripts/arbiter-answer-eval/gpu_fixture.py --fixtures D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z\answer-eval-r1\fixture
%PY% start --image C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\answer-eval-gpu-r1\cs1e-cuda-f79755a7 --lifetime <name> --model compact --restart-from <prior> --change-set-inputs D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z\answer-eval-r1\change-set-1\profiles\change-set-inputs-r2.json --change-set-inputs-sha256 a0f26d834ec3fd322129e8daf28459133043f4690d17ec12d2b91afdf7094c59 --cuda-variant-inputs D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z\answer-eval-r1\gpu-track\cuda-variants-r1\cuda-contextqwencompact-provisional-read-inputs.json --cuda-variant-inputs-sha256 5cd0671b1adf3c9016972867d4022a246350dd1a9b18a6195d4d7568f73b31d1 --layers all
%PY% stage --lifetime <name> --profile cs1-focus-software-w16 --manifest f2d194b0070458d5af772802dcbb9ae5e27959746794f9bdfaa00eac9202830a --stage-name focus --device cpu --enable
%PY% stage --lifetime <name> --profile cs1-context-compact-cuda --manifest ec3c8294a07ccbe9eb4d8bb6a3cdd33e6f85b3d0c0a12303685acc3c117517ab --stage-name context --device cuda --enable
%PY% answer --lifetime <name> --repository airk --question "<question>" --label <label> --output <record.json>
%PY% stage --lifetime <name> --profile cs1-context-compact-cuda --manifest ec3c8294a07ccbe9eb4d8bb6a3cdd33e6f85b3d0c0a12303685acc3c117517ab --stage-name context --device cuda --disable
%PY% stop --lifetime <name>
```

Use the same layer string as the CPU cells of the series for `--layers`. That
includes the cs1e tuning tokens, for example
`--layers all,pool=8,pool_full=8,read_floor=3072,kept=3,subjects=strict,governing=project`
(R\design\cs1-tuning-tokens.md); fixture.py validates it. Send change set 1's
budget with every Answer, as the harness's `answer_defaults` do:

```text
%PY% answer ... --answer-defaults "{\"budget\":{\"calls\":12,\"rounds\":3,\"fanout\":12,\"candidates\":32,\"max_bytes\":524288,\"input_tokens\":8192,\"deadline_ms\":120000}}"
```

The same applies to the harness's own MCP wrapper: give the GPU cells the same
`answer_defaults` as the CPU cells (`make_cs_spec.py` `CS1_BUDGET`).

`start` blocks for the whole lifetime, so run it in the background and wait
for `handle.json` to reach `ready`. For Large, use `--model large`, the Large
variant inputs and pin, and manifest `9bc9831e…a5e1`. Run one CUDA model per
lifetime. Disable the CUDA stage before `stop`, so the next CPU lifetime starts
from a CPU selection.

## Two CUDA Context entries in one lifetime (`--model pair`)

Plan 12.5.2 item 4 and the LG gate: Compact and Large on CUDA in one lifetime,
on one installation and one set of indexes. The Context model is switched by
ordinary Configure: `stage --disable` for the live entry, then `stage --enable`
for the other. The service warm releases the old model before it loads the new
one. `run_plan.py`'s `switch_context` hook issues exactly those two commands.

### Inputs (`cuda_pair.py`)

1. **One asset directory per profile.** `cuda_pair.py assets` copies every
   declared asset from the profile's asset map into a fresh directory. It
   authenticates each file before and after the copy: under `artifacts`, a
   single link, and the profile's declared size and SHA-256. It writes
   `<dir>.receipt.json` beside the directory, with the bytes copied.
   - Budget the copy first: Compact FP16 is 3.45 GB, Large FP16 8.05 GB and
     Focus 1024 1.14 GB.
   - **Never use hard links.** The fixture opens every input through a rooted
     directory that refuses a file with more than one link ("rooted file has
     aliases"). The warm also refuses a hard-linked CUDA payload ("redirected/
     nonregular/hardlinked GPU payload"). A link also aliases the original, so
     other lifetimes cannot open it while the link exists.
   - Pass `--artifacts D:\code\Github\airk\artifacts` when running from a
     worktree inside the main tree.
2. **One entry file per CUDA profile.** A Context entry names:
   - `profile`, `profile_sha256`;
   - `assets` (the directory from step 1);
   - `review`, `review_sha256` (the pass-1 graph diagnostic receipt);
   - `pass1_profile_sha256` (the pass-1 profile that receipt reviewed).

   A CUDA Focus entry (strict-GPU placement) names `profile`,
   `profile_sha256`, `assets` and `parity`/`parity_sha256`.
   - The parity file is the receipt of the ignored `l5c_focus_parity`
     selector: the CPU twin and the CUDA profile score the same fixed pairs
     in one process.
   - The service checks four things:
     - the profile shares the CPU twin's compiler, tokenizer and fixture
       assets, task, limits, graph and weights;
     - the parity receipt passes (at least 8 pairs, every winner agreeing,
       within 1e-3);
     - the receipt names this exact manifest;
     - the placement is strict, checked against the live session.
3. **The pair.** `cuda_pair.py pair --entry NAME=ENTRY.json ... [--row NAME=STAGE:MANIFEST ...] --out DIR`
   writes `roster.json` and `pair.json`.
   - It re-authenticates every asset and the review: complete, CPU payload
     check null, no errors, the pinned pass-1 profile, the same id, model and
     device execution except `placement` and `memory`, and a placement that
     names the review's policy.
   - It refuses two entries of one model and entries on different adapters.
   - `pair.json` carries the roster pin, the review pins
     (`AIRK_ARBITER_EVALUATION_CUDA_REVIEWS`), the adapter and the profile
     rows by name.

### Use

```text
%PY% start --image <dir> --lifetime <name> --model pair --pair <DIR>\pair.json --pair-sha256 <sha> --restart-from <prior>
%PY% stage --lifetime <name> --profile context-compact-fp16-cuda --enable
%PY% stage --lifetime <name> --profile context-compact-fp16-cuda --disable
%PY% stage --lifetime <name> --profile context-large-fp16-cuda --enable
```

The lifetime's `gpu.json` records the pair's rows as `cuda_rows`. `stage`
resolves a profile name from the CPU rows, the single-model CUDA rows and these
lifetime rows, so the names in a `run_plan.py` `switch_context` step work
unchanged. A `--row` adds a CUDA Focus (or other) row by name and manifest.

**CUDA Focus (closed 22 November 2026).** The seam also attaches one CUDA
Focus entry, on the same lease and adapter as the Context entries, in the Focus
stage's own map. It is probed under the evaluation binding and warmed through
the standard charged route, where the scorer verifies strict placement. GPU
revocation, idle unload and the release-before-reserve order cover it, as for
every stage. `stage --enable/--disable` on its row works in one lifetime.

Measured on the A6000 (receipt `R\answer-loop\L5C\runs\f3-focus-seam-summary.json`):

- **Run:** a fresh synthetic installation (`--seed-cpu focus`), pair
  `context-compact-fp16-cuda` + `focus-1024-cuda`. The profile is `a3157dd1`,
  the W8 1,024-token graph `a16fcaa1`, strict GPU.
- **Warm:** 8.6 s. **Release:** 0.69 s.
- **Per-call inference** at 1,024 tokens: 106-112 ms warm; the first call of
  each Answer took 278-480 ms.
- **Answers:** two, each with 8 Focus calls, in 2.13 s and 1.75 s, with the
  same evidence digest.
- **GPU memory used** (nvidia-smi, sampled every 200 ms): 557 MiB before, a
  2,357 MiB peak, 791 MiB after disable while the service still ran, and
  562 MiB after stop.
- **Parity with the CPU twin** (`R\answer-loop\L5C\runs\p1-focus-parity.json`,
  SHA `835017fbd8dd7987d41e4a631355a951c4edf5e55dafabb576b20c0373b43236`):
  - 15 pairs: the worst-case full-width pair and 14 fixture pairs;
  - all 15 winners agree; largest raw difference 1.7e-5, against a recorded
    tolerance of 1e-3;
  - median call 3.87 s on CPU and 111 ms on CUDA;
  - sampled process GPU peak 2.68 GB.
- **Revocation:** it releases both stages even if one fails, and runs again
  on every revocation.

**An image built in a worktree** checks every input against that worktree's
`artifacts` root, which is fixed at compile time. The check therefore needs
its inputs, the MiniLM model and the fixtures root copied under the
worktree's `artifacts`. An image built in the main tree uses the main
`artifacts`.

### Built so far (22 November 2026, pre-L4 identities)

- **Withdrawn:** the first round of inputs was built by hard link
  (`lg-inputs\pair-r1`, `pair-focus-r1`). They are invalid for the reasons in
  step 1. The link directories were removed; the originals are single-link
  again.
- **Rebuild:** after the re-emission below, build the Compact and Large
  directories again by copy.

### LG arms side by side (changes entry 41)

Arms C (Focus + Compact FP16), L (Focus + Large FP16) and F (Focus only) each
run in their own fixture root, service and lifetime, at the same time on the
A6000. `lg_lifetimes.py` writes the `--lifetimes` file for
`make_loop_spec.py gate`.

**Memory.** Declared per arm from the measured peaks, rounded up with
headroom:

| Arm | Device GiB | Host GiB |
| --- | --- | --- |
| C | 10 | 16 |
| L | 18 | 24 |
| F | 3 | 8 |

- **Device:** 31 GiB, plus run_plan's 4 GiB reserve, fits 48 GiB.
- **Host:** 48 GiB, plus the 16 GiB reserve, against 128 GiB physical (about
  95 GiB free when measured).
- **Accounting is larger than residency.** The services' own host
  reservations (32 GiB for Compact and 40 GiB for Large, lifecycle constants)
  are accounting within each service, not resident memory.
- **Probes.** Each private probe checks live free VRAM against its cold peak
  plus headroom: Focus 10, Compact 10 and Large 20 GiB. Launched in order F,
  C, L, every probe fits beside the earlier residents.

**Roots and restarts.** Each arm's root holds a closed CPU base lifetime
(`fixture.py`, the software-v2 selector), and the arm's CUDA lifetime
restarts from it. Three restart rules shape this (`restart.rs` `from_env`):

- **A GPU lifetime is never a restart origin.** Its Ready lists the CPU
  roster and the CUDA profiles, and a restart requires the Ready to list
  exactly the CPU roster. Synthetic `S2-large-restart` failed this way.
- **A restart needs the saved configuration unchanged**, so once a GPU
  lifetime has run, its root cannot restart again.
- **A newer image may restart a root.** The campaign chain did so across
  images (L21 to L43, five images) and kept the same publication token and
  pipeline digest. The current image (`3cc8bebd` era) still indexes with
  pipeline `cb2166f2…`, the same as `1fe7b600`. L4 changes only the
  retrieval Arbiter runtime.

So the bases can be built now, on the current image, and the LG image
restarts from them:
- the judged arms need three roots;
- each precision arm (C32, L32) needs its own root and base.

**Repositories.** Every LG root indexes the loop pair and nothing else:
`bookbuilder-v2` and `munarium` (user direction, 22 November 2026: the index
under test holds nothing about the experiment). No root holds an airk index.
The bookbuilder scenarios are served by `bookbuilder-v2` through the root's
`index-aliases.json`. `bookbuilder-v2` is listed first because it has denies:
`fixture.py` installs MiniLM through the first repository once its denies are
in force, and refuses an order in which a later repository still needs one
(FIXTURE.md, "Source policy comes before any index pass").

**Disk.** A software-v2 base seeds all CPU weights (24.16 GB, measured) and
holds the two snapshots and indexes. Budget 27.5 GB per root before the first
one is measured; the reservation and the measured sizes are in
`R\answer-loop\L5C\disk-reservation-lg06-roots.json`.

Copying a settled root is not isolated: its handle and restart record name
the original control root, so the copy would serve the original
installation.

**Bases (series `06-gate-lg`).** One root per arm: `R\fixture-06-gate-lg-C`,
`-L` and `-F` for the judged arms, `-C32` and `-L32` for the precision arms.
Before each start, create the root and copy `R\fixture\index-aliases.json`
into it (`bookbuilder` served by `bookbuilder-v2`). Run from the repository
root (or the worktree whose image is used), one detached process per root:

```text
python -I -B -S scripts/arbiter-answer-eval/fixture.py --fixtures R\fixture-06-gate-lg-<arm> start ^
  --image C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\answer-eval-gpu-r1\l5c-focus-12bde37b ^
  --lifetime 06-gate-lg-<arm>-base --repositories bookbuilder-v2,munarium --seconds 20000
```

`start` blocks for the lifetime. `lg_bases.py` waits on the lifetimes' own
handles, probes the ready bases and collects their facts; it starts and stops
nothing:

```text
set B=python -I -B scripts/arbiter-answer-eval/lg_bases.py --series 06-gate-lg --arms C,L,F,C32,L32
%B% wait --until semantic --seconds 19000
%B% probe
python -I -B -S scripts/arbiter-answer-eval/fixture.py --fixtures R\fixture-06-gate-lg-<arm> stop --lifetime 06-gate-lg-<arm>-base
%B% wait --until closed --seconds 3000
%B% summary --out R\answer-loop\L5C\lg-inputs\bases-06-gate-lg.json
```

- **`wait`** ends when every base has reached the state (exit 0), one has
  failed (exit 1) or the deadline passes (exit 2). `semantic` means both
  indexes are semantically ready; embedding bookbuilder-v2 on the CPU takes
  hours.
- **`probe`** runs, per ready base: the settled-index guard on munarium and
  bookbuilder (`settled.py`), one `airk_status` per index, the leak check
  (`leak_check.py`), the credential probe on each index
  (`credential_probe.py`) and the launcher's own `leak-probe`. The receipts go
  into the root: `leak-check.json`, `credential-probe-<index>.json`,
  `leak-probe-<n>.json` and `base-probe.json`. `--skip-launcher-probe
  <reason>` leaves out the launcher's `leak-probe` and records the reason; the
  leak check covers the same ground (phrases, listings, no airk index).
- **`--disk-log <file> --disk-floor <bytes>`** (before the command) appends
  the volume's free bytes at every run, for the shared disk budget's watch.
- **`stop`** is `fixture.py`'s: one command per root, after the probes.
- **`summary`** records, per base: state, restartable, error, the handle path
  and, per index, the indexing seconds, file, chunk and vector counts, the
  publication token and the index pipeline digest.

A base is a restart origin when its handle is `closed`, `restartable` is true
and it has no error. `lg_lifetimes.py --base <arm>=06-gate-lg-<arm>-base`
checks that, and that the base holds exactly the pair's indexes and the alias
file.

**History of the bases (22 November 2026).**

- `05-gate-lg` (airk and bookbuilder-v2): all three failed. Roots C and L
  queued the index pass behind a deny whose outcome was unknown; root F
  deferred a refused deny past the index pass. Their large data was reclaimed
  (`R\answer-loop\L5C\reclaim-lg-05-roots.json`).
- `06-gate-lg`, first attempt at root C (munarium, then bookbuilder-v2): the
  bookbuilder-v2 denies were in force before its index pass, but the first one
  was applied while munarium's vectors job ran, and that job failed. The
  lifetime was stopped and reclaimed
  (`R\fixture-06-gate-lg-C-stopped1`, `reclaim-lg-06-C-stopped1.json`).
- `06-gate-lg`, the five bases: started with `fixture.py` at `b1fc8da6` and
  the order above. In every root both denies were applied directly, before
  any index pass, with no busy refusal.
- All five closed on 23 November 2026, 02:28 UTC, restartable and without
  error, each with bookbuilder-v2 at 8,116 files, 65,236 chunks and 156,255
  vectors and munarium at 1,023 files, 5,380 chunks and 40,536 vectors, on
  index pipeline `cb2166f2…`; the settled guard, the leak check and the
  launcher's leak probe passed in each
  (`R\answer-loop\L5C\lg-inputs\bases-06-gate-lg.json`).
- **Base L has two lifetimes.** `06-gate-lg-L-base` ended after 3.6 h of
  embedding on one `airk_status` poll that the service refused
  (`scoped_connection_unavailable`, no query submitted); the wait treated it
  as fatal. Its service settled cleanly, so `06-gate-lg-L-base2` restarted
  from it, resumed the cancelled vectors job and reached the same counts.
  `fixture.py` now records such a refusal and observes again (at most twelve
  in a row), and `lg_bases.py --base L=06-gate-lg-L-base2` reads the base
  under its second name. Arm L restarts from `06-gate-lg-L-base2`.
- **A vectors job can fail once with `helper_unavailable`.** On the image
  built from `5f971a70`, the first vectors job of the munarium-only
  shakedown base failed 0.5 s after it started ("shared embedding helper
  temporarily unavailable: the pipe is being closed"), and the service
  registered the helper again 5 s later. `fixture.py` now queues one explicit
  index pass as the retry, after three idle observations in a row, and
  records it in the index state (`semantic_retries`). The gate's lifetimes
  restart from settled bases and embed nothing, so this is not on the gate's
  path; it is reported to the product side.

**Image.** The image must come from one commit that holds:
- L4 and the re-emitted profiles;
- the Focus seam: `loop/l5c` 3cc8bebd, 604b7675 and 3e5dc40b, or their
  integrated forms.

A worktree image checks inputs against `<worktree>\artifacts`, fixed at
compile time. Make that path a directory junction to the campaign's
`D:\code\Github\airk\artifacts`; the check canonicalizes, so the junction
resolves to the main `artifacts` and the rule is unchanged:

```text
cmd /c mklink /J <worktree>\artifacts D:\code\Github\airk\artifacts
```

Remove the junction only with `cmd /c rmdir <worktree>\artifacts`. Never use
a recursive delete: it would delete through the junction. When the worktree itself
lies under `artifacts`, as `R\answer-loop\L5C\wt` does, the junction makes a path
loop. While it exists, nothing may recursively scan, copy, size or delete `R`,
`artifacts\v190` or the worktree; size single folders such as the fixture roots
instead. Remove the junction as soon as the LG image no longer needs it.

**Commands.** The whole run, from the closed bases to the report, is in
"LG gate run" below.

**Precision arms (deferred, 23 November 2026).** The FP32 Context profiles
declare only the one-turn task, so every card call in an FP32 arm would be
not dispatched and the arms would compare nothing. The precision check (plan
12.7.3) is replay only and no gate of its own; it runs after L7, when FP32
card-task profiles exist. The LG spec and plan hold the judged arms only
(`make_loop_spec.PRECISION_DEFERRED`; the report prints the reason). The two
roots built for it, C32 and L32, are **spare roots** for the judged arms.
CF16 and CF32 cannot attach through the seam at all: the Focus binding
refuses an FP16 Focus, and a Focus whose weights differ from its CPU twin.

### Re-emitting the FP16 profiles for the loop (run when L4 integrates)

L4 moves `runtime.rs` (to `c9008eef…`). That file is an input of the FP16
math-policy digest, so the G5 and protected-MHA identities and every
bounded-context policy move, and both FP16 profiles must be emitted again.
They must also declare the four card tasks through
`airk_retrieval::arbiter::runtime::admit_loop_tasks`. That function replaces
the JointCalibration with a zero-rule `LoopCalibrationSet` and sets the
Calibration asset and the `calibration` digest.

**Code step (done, 23 November 2026).** In
`crates/airk-retrieval/tests/cuda_l5a_diagnostics.rs`, `l5a_prepare`,
protected branch, the opt-in `L5A_LOOP_TASKS=1`:
1. after `prepare_protected_mha_profile` returns, it calls
   `admit_loop_tasks(&mut profile)`;
2. the returned bytes become the calibration bytes, under the same asset
   name;
3. the manifest is recomputed under a distinct domain,
   `airk.private.cuda-protected-mha-loop-profile/1`, over the emitter's
   manifest (which already binds the measurement), the runtime-manifest SHA
   and the loop profile with its manifest zeroed;
4. `profile.validate()` runs. `validate_selected_policies` is private to the
   crate; it runs when the profile enters `EvaluationCatalog` in the graph
   and strict steps.

The edit is in a test file only, so no digest-input file moves. Both passes
take the switch, so the review receipt is of the loop profile. The bounded
policy itself reads only the model, the provider options, the graph rows and
the placement, so it does not depend on the tasks.

**Core prerequisite.** A profile that declares the card tasks must pass
Core's `Profile::validate()`. At `936c9b94` it does not: `Stage::permits`
(`crates/airk-core/src/arbiter/stages.rs`) allows only `airk.focus_rank.v1`
and `airk.context_action.v1`, and `validate_stage` and
`StageStatus::validate` apply it to a profile's tasks. Pass 1 then refuses
with "Arbiter profile has unsupported stage task", and no catalog or scorer
would open such a profile either (`EvaluationCatalog::from_native_lease` and
`Scorer::open_internal` both call `validate()`). Found on 23 November 2026
by the first pass-1 run; the fix is the integration agent's, on main. Emit
final profiles only from a build that contains it.

**Driver.** `fp16_loop_emit.py` runs the steps below for one model, with
fresh outputs, one line per step in `commands.log`, and a sampled-memory
summary beside each GPU receipt:

`python -I -B fp16_loop_emit.py --spec <inputs.json> --model compact|large --image <staged test image> --out <dir> --step all`

The private spec names the pinned inputs listed in step 2
(`L5C\lg-inputs\fp16-loop-emit-inputs.json` for the 23 November run). Its
`summary` step writes `<model>-summary.json`: the profile's identities, the
review pin, the reload time (`cold_open_us`), per-width call times and the
sampled and adapter-observed GPU memory.

**Steps.** Per model, as the Cargo owner and then the GPU owner, with the
image staged beside the CUDA payload (`cu15\gpu-provider-stage`):

1. Build:
   `cargo test -p airk-retrieval --features gpu-cuda,test-support --test cuda_l5a_diagnostics --locked --offline --timings --no-run`.
2. Pass 1 (metadata only), `l5a_prepare` with:
   - `L5A_PRECISION=protected`, `L5A_LOOP_TASKS=1`;
   - `L5A_CPU_PROFILE` and `L5A_ASSET_MAP` from `L5A\inputs` (Compact
     `compact-cpu-profile.json` `571de801…`, `compact-cpu-asset-map.json`
     `a0f909ba…`; Large `large-cpu-profile.json` `7a444c76…`,
     `large-cpu-asset-map.json` `3797800f…`);
   - `L5A_RUNTIME_MANIFEST=...\gpu-provider-stage\airk-cuda-runtime.json`
     (`94337535…`);
   - `L5A_FP16_MODEL` and `L5A_FP16_WEIGHTS` from the final asset maps
     (`compact-cutlass-protected-staged-r1`, and
     `large-direct-fp32-stage-r2\model.onnx` with
     `large-fp16-conversion-owned-r5\candidate\weights.bin`);
   - `L5A_MEASUREMENT` and `L5A_MEASUREMENT_RECEIPT` as in the final
     prepare receipts (Compact receipt `phaseB\runs\b6-compact-pass2-strict.json`
     `82c2b345…`; Large `review\runs\large-final-confirm.json` `cdc627f8…`);
   - `L5A_PLAN` (Compact `8589934592,3221225472,2147483648`; Large
     `19327352832,8589934592,2147483648`);
   - a fresh `L5A_OUTPUT`.
3. Pass-1 graph diagnostic on the GPU: `l5a_gpu` with `L5A_STEP=graph` and
   `ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO=1`. Its receipt's
   `bounded_policy` is the new review pin. It must have a null
   `cpu_payload_check`.
4. Pass 2: `l5a_prepare` again with `L5A_POLICY=<bounded_policy>`.
5. Confirm on the GPU: `l5a_gpu` with `L5A_STEP=strict` and debug 0. Then
   replay with `l5a_replay` against the new review receipt, and compare
   with `l5a_replay_compare.py`.
6. Rebuild the inputs: `cuda_pair.py assets` for each new profile (new
   directories), then the entry files (pass-1 SHA = step 2's
   `profile.json`), then `cuda_pair.py pair`.
7. Build the fixture image:
   `cargo test -p airk-service -p airk-kernel --lib --all-features --no-run`,
   staged like `cs1e-cuda`. Then start `--model pair`.

## LG gate run (procedure, written 23 November 2026 for a new operator)

This is the whole run from the closed bases to the gate report. The state it
starts from (paths, SHA-256 values, what is running) is in the private
`R\answer-loop\L5C\RESUME.md`. Read both before the first command.

Paths used below (PowerShell, run from the worktree `$WT`, as the verified
human):

```text
$R   = 'D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z\answer-eval-r1'
$L5C = "$R\answer-loop\L5C"
$WT  = "$L5C\wt"                      # branch loop/l5c; the harness and the image are built from here
$PY  = 'python -I -B scripts/arbiter-answer-eval'
$G   = "$R\passes\06-gate-lg"
```

### What can go wrong, first

- **Sealed roots give one CUDA lifetime.** A restart needs the root's saved
  configuration unchanged, and a CUDA lifetime changes it. Once an arm's
  lifetime has started, even if it then fails, that root is spent. So every
  input is checked before the launch (`lg_preflight.py`), nothing but the
  gate starts a lifetime in a `fixture-06-gate-lg-*` root (`gpu_fixture.py`
  refuses it), and the three judged lifetimes must live to the end of the
  plan. A lifetime lasts 21,000 s (5.8 h) from its start; the plan needs about
  3.5 h. Do not pause between steps. Before any resume, read each handle's
  `deadline_utc`.
- **The junction.** `$WT\artifacts` is a junction to the campaign artifacts
  and `$WT` lies under them: a path loop. Never recursively scan, copy, size
  or delete `$R`, `artifacts\v190` or `$WT`. Work on named folders only.
- **Copied constants.** Three defects of one class were found on 22 and
  23 November: a copied token budget in the harness, literal /2 budgets in
  the fixture, and a CPU Focus name left as the gate's default. Budgets and
  schemas are read from `fixtures/split/arbiter/answer_loop_cases.json` or
  from Core's defaults. Do not write such a number anywhere; if a test asks
  for one, read it from the cases file.
- **Hard links.** Never hard-link a model asset or a CUDA payload file. The
  fixture refuses a linked input, the runtime refuses a linked payload, and a
  link also breaks the original for every other image. `cuda_pair.py assets`
  and `stage_fixture_image.ps1` copy.
- **Owners.** One Cargo owner for `R\answer-loop\target-cuda` and one GPU
  owner at a time: `$L5C\CARGO-OWNER-CUDA.json` and `$L5C\GPU-OWNER.json`
  say who. Write the marker before a build or a GPU run and rename it to
  `*.at-release.json` after. Build only through `$L5C\cargo.ps1`, which sets
  the pinned environment and logs the command.
- **Do not touch** the user's installed AIRK service, `D:\code\Github\munarium`
  or `D:\code\Github\bookbuilder`, `R\fixture` (the earlier campaign root),
  the scenario files or `R\scenario-freeze.json`.

### Before the gate: one shakedown must pass end to end

**Not passed yet (23 November 2026).** The first shakedown, on the image
built from `5f971a70`, stopped at the Context warm. Three defects appeared
one after the other, each only when a real service met a loop profile:

1. The fixture built its Answer template as a /2 request and did not start
   (fixed in `evaluation_fixture_tests.rs`).
2. The fixture's Configure emitter enrolled a loop profile's four card tasks;
   an enrollment names only the stage's enrollment task
   (`airk.context_action.v1` for Context), so Configure refused
   (`arbiter_unavailable`). Fixed in the same file (`stage_enrollment`).
3. **Open, product side.** With the right enrollment, Configure is accepted
   and the native warm fails for Compact and for Large: "protected MHA Answer
   requires the original Context limits and its own policy pin".
   `crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid/layer0/answer.rs`,
   `open_private_protected_mha_answer`, requires the profile to declare
   `Task::ContextAction`; a loop profile declares the card tasks. Reported to
   the coordinator. Arms C and L cannot run until it is fixed. CUDA Focus
   warmed in 9.1 s.

The second shakedown (s5, image from `69033135`, 23 November) found two more:

4. **Fixed, harness.** The fixture service is the `airk-service` test binary and never runs the product's
   `airk` main, which sets `ORT_CUDA_GEMM_OPTIONS=2` before ONNX Runtime starts
   (`crates/airk-cli/src/startup_math.rs`). The FP16 routes refuse without it ("G5 requires unchanged
   inherited GEMM2 ..."). `gpu_fixture.py` now gives every CUDA lifetime's service that value (and refuses
   a conflicting or unknown inherited `ORT_*`); the handle's `service_environment` records it, the preflight
   checks it, and `run_plan.py` refuses a CUDA lifetime without it before enabling stages.
5. **Open, product side.** Every Answer search read fails `read_failed` ("embedding worker is unavailable;
   native adapter not installed"): the read threads of `crates/airk-kernel/src/arbiter/answer/wave.rs` do
   not bind the caller's scopes. Parts stop `refused` at Retrieve and Focus never runs. The old arm check
   passed such Focus-only replies; it now refuses them, and `gate.py smoke` catches it in one call.

**Passed on s10, 23 November 2026** (image from `24c506db`, the four product fixes of that day plus the
frame fix): smoke, all ten munarium scenarios at the default budget and verification with model-sourced
decisions, three replays, one session with two judges, the gate tooling, Large beside Compact and two
Answers at once. The s5 to s8 shakedowns found, in order: the fixture service lacked GEMM2 (harness); every
Answer search read failed on the wave's threads (product); Focus batches above the profile's 4 candidates
were refused (product); card calls were refused while the Answer deadline was over 120 s away (product);
card decisions abstained because neither fixture factory set top-score mode (harness, both
`evaluation_fixture_tests.rs` and `cuda_answer/same_install.rs`); replies with larger verification receipts
failed (product, two commits: the receipt frame and the 64 KiB worker channel); the CUDA Focus stage unloaded
for idleness mid-run (harness: `AIRK_ARBITER_EVALUATION_IDLE_UNLOAD_SECONDS=0` in every CUDA lifetime,
`gpu_fixture.py ensure` before every Answer step in `run_plan.py` and before every session in
`run_pass.py`). Build shakedown bases with `build_base.py`, which restarts a base whose first vectors job
fails and records each occurrence.

Every launch in the gate plan is now followed by `gate.py smoke` (one Answer per arm and repository: a search
record without a failure, a Focus call, and model decisions where a Context model is staged), before any
replay or session. In a shakedown, run it first: `gate.py --r-root <shakedown r> smoke --spec <spec> --arm C
--out <dir>`, with the spec from `make_loop_spec.py shakedown --series <s> --arm C --out-root <shakedown r>
--handle <lifetime handle> --scenarios munarium-A1,...` and a prompts freeze written under that root
(`prompts_freeze.py --write <shakedown r>\design\prompts-freeze.json`). A manual lifetime also needs its
tools catalog (`common.write_tools_catalog`), which `run_plan.py` writes for the gate.
`--arm` also takes `A` (Arbiter only) and `N` (neither switch), for the four combinations of LH (README, "The four
switch combinations on one lifetime").

Two bases started at the same moment both lost their first vectors job (`project_operation_failed`, 1.2 s
after it started; s6 and s7, 23 November). A restart of the base (`fixture.py start --lifetime <name>-base2
--restart-from <name>-base`) resumed the embedding. Start bases a few minutes apart.

Nothing past the warm has run with a loop Context profile: card calls
dispatched and scored, a /3 reply with model-sourced decisions, a session,
its judges and the report are unobserved on a real service. So before the
gate spends a root, run a shakedown on the gate image's source and require
all of this from it:

- a fresh root (`R\fixture-<name>`, not a gate root) with a closed
  munarium-only CPU base: `fixture.py --fixtures <root> start --lifetime
  <name>-base --image <image> --repositories munarium --seconds 7200`
  (create the root folder first), then `lg_bases.py --series <s> --arms <a>
  --base <a>=<name>-base wait --until semantic`, `fixture.py stop`, and
  `wait --until closed`;
- a pair lifetime restarted from it with the three-entry pair
  (`lg-inputs\pair-CL-shakedown`): `gpu_fixture.py --fixtures <root> start
  --image <image> --lifetime <name> --model pair --pair <pair.json>
  --pair-sha256 <sha> --repositories munarium --seconds 7200 --restart-from
  <name>-base`;
- `stage --profile focus-1024-cuda --enable`, then
  `stage --profile context-compact-fp16-cuda --enable`: both must return a
  stage record;
- `gpu_fixture.py answer`: the record must show `cuda_decisions` above zero
  and decisions with source `model` (card calls dispatched and scored);
- `gate.py replays` for two or three munarium scenarios on a spec whose arm C
  points at this lifetime: `arm_ok: true`, and coverage above zero on real
  span rows;
- one headless session through `run_pass.py --stage sessions`, then
  `coverage`, `judge` and `grade`; then `gate.py performance` and
  `gate_report.py`;
- the short trial of plan 12.5.2 item 4 in the same lifetime: disable
  Compact, enable Large, two Answers, disable Large, enable Compact, one
  Answer, with the stage records' seconds (reload), `nvidia-smi` memory and
  the replies' model timings.

Keep the lifetime alive until every part has run: a driver that stops the
lifetime after a failed step throws the root away (this happened on
23 November). The shakedown's verdicts mean nothing and are never reported
as results.

### Steps

Each step says what it must show before the next one starts. Stop and ask
the coordinator when a step does not show it and this text does not say what
to do.

1. **Rebase onto the final main commit** the coordinator names, then run the
   suite:

   ```text
   git -C $WT rebase <final main commit>
   python -I -B -m unittest discover -s scripts/arbiter-answer-eval
   ```

   Must show: no conflict; `OK`. If the coordinator says an identity moved
   (a file that an FP16 profile hashes), the profiles must be emitted again
   first ("Re-emitting the FP16 profiles for the loop") and the asset
   directories and pairs rebuilt; otherwise they stand.

2. **Build and stage the gate image.** Write the Cargo owner marker, then:

   ```text
   pwsh -NoProfile -File $L5C\cargo.ps1 lg-image test -p airk-service -p airk-kernel --lib --all-features --locked --offline --timings --no-run --message-format=json-render-diagnostics
   pwsh -NoProfile -File $L5C\cargo.ps1 lg-cli build -p airkctl --bin airkctl --locked --offline --timings
   pwsh -NoProfile -File scripts\arbiter-answer-eval\stage_fixture_image.ps1 -Prefix lg-gate -ImageLog $L5C\logs\lg-image.log -CliLog $L5C\logs\lg-cli.log -Target $R\answer-loop\target-cuda -GitHead (git -C $WT rev-parse HEAD) -Note 'LG gate image'
   ```

   To stage a new image on D: from the provisioned CUDA runtime on C:,
   pass `-Base <approved image output root on D:>` and
   `-ProviderStage <approved cu15/gpu-provider-stage on C:>`.
   Prepare the `answer-eval-gpu-r1` subdirectory under that output root
   using the checked Windows execution procedure.

   Must show: both builds `exit=0` in `$L5C\logs\cargo-commands.log`, and
   `staged <folder> service=<sha> cli=<sha>`. The folder is the image. Build
   from a clean tree: the image records the commit, and the preflight
   compares it with the tree.

   For a Windows comparison against CUDA-derived MiniLM indexes, bind the
   verified FP16 development admission to the private service build with
   `AIRK_APPROVED_MINILM_CUDA_DEVELOPMENT`. Retain the encoded admission
   checksum and the native placement receipt alongside the Cargo result.

   After the first batch completes, query the exact fixture control root
   with the staged CLI. Check again when both semantic publications settle.
   This confirms the runtime path that the comparison receives.

   ```powershell
   '"embedding_status"' | & $airkctl --output json service rpc --runtime-dir $control --stdin
   ```

   The comparison uses a completed `last_embedding` with `route` equal to
   `cuda` and an empty `fallback`. A completed `admission_refused` CPU
   fallback identifies a different index build, even when the optional
   Focus and Arbiter profiles are CUDA profiles.

3. **Refreeze the prompts**, last, from the final tree. The freeze pins the
   harness files, the scenario freeze (`26338d09…190e`) and the default
   budget with the cases file's SHA-256:

   ```text
   python -I -B scripts/arbiter-answer-eval/prompts_freeze.py --refreeze $R\design\prompts-freeze.json --reason "loop harness (Answer /3) for LG, <commit>"
   ```

   Must show: a history line with `new_sha256`. Any later change to a harness
   file, to the scenario freeze or to the cases file needs another refreeze.

4. **Write the lifetimes file, the spec and the plan.** The pairs and bases
   are in RESUME.md. Arm F reuses arm C's pair and stages only its Focus row:

   ```text
   python -I -B scripts/arbiter-answer-eval/lg_lifetimes.py --series 06-gate-lg --r-root $R --image <image folder> --pair C=<pair-C>:<sha> --pair L=<pair-L>:<sha> --pair F=<pair-C>:<sha> --base C=06-gate-lg-C-base --base L=06-gate-lg-L-base2 --base F=06-gate-lg-F-base --out $L5C\lg-inputs\lifetimes-06-gate-lg.json
   python -I -B scripts/arbiter-answer-eval/make_loop_spec.py gate --series 06-gate-lg --lifetimes $L5C\lg-inputs\lifetimes-06-gate-lg.json
   ```

   Must show: the lifetimes file's SHA-256, and `spec.json`, `plan.json` and
   `gate-report-inputs.json` under `$G`. `lg_lifetimes.py` refuses a base
   that is not closed, restartable and free of errors, a root that holds
   other indexes than `bookbuilder-v2` and `munarium`, and a root that has
   already had a CUDA lifetime. The output file is never overwritten: delete
   a wrong one by its exact path before writing it again.

5. **Preflight.** Put the image folder and the tree's commit into
   `$L5C\lg-inputs\gate-inputs.json` (`image`, `commit`), then:

   ```text
   python -I -B scripts/arbiter-answer-eval/lg_preflight.py --inputs $L5C\lg-inputs\gate-inputs.json --out $G\preflight.json
   ```

   Must show: `PREFLIGHT PASSED`. On `REFUSED`, fix the cause it names and
   run it again. Never edit an expected value to make it pass; the SHA-256
   values and manifests in `gate-inputs.json` come from RESUME.md and change
   only if profiles or pairs were rebuilt.

6. **Run the plan**, detached, and wait on its log:

   ```text
   python -I -B scripts/arbiter-answer-eval/run_plan.py --plan $G\plan.json --log $G\plan-log.jsonl
   ```

   The plan, in order: launch C, L and F side by side (memory checked; each
   root's `tools-catalog.json` is written when its lifetime is ready; the
   stages are enabled: `focus-1024-cuda` in every arm, the Context row in C
   and L); the arms' first-call replays in parallel; all judged sessions;
   coverage, judging and grading; the outcome tests; second sessions on close
   calls; outcomes again and the agreement tests; the concurrency test (2 and
   4 concurrent Answers per arm, a different scenario's question in each
   concurrent call, on the unqueued path, one arm at a time); stop the
   lifetimes; the performance figures.

   Must show: `plan-done` in the log. The first replay rows are the first
   real evidence: each arm's `arm-replays.json` must have `arm_ok: true`. In
   arms C and L the replies must carry decisions with source `model` (card
   calls dispatched and scored). If an arm shows none, stop: see the table.

7. **Write the report:**

   ```text
   python -I -B scripts/arbiter-answer-eval/gate_report.py --inputs $G\gate-report-inputs.json --out docs\arbiter\answer-evaluation\06-gate.md
   ```

   The report states the budget sent, the coverage rule, the decisions by
   source, the replies where the model was present and not reached, the parts
   per scenario, reply size and what was cut, and that the precision check
   was not run and why. It is for the user's review; the gate stops there.
   Committing it with the series belongs to the coordinator.

### When a step fails

| What happened | Is a root spent? | What to do |
| --- | --- | --- |
| Preflight refused | No | Fix the named cause; run it again. |
| The launch step refused before starting (memory, placeholder, a base check) | No | Free the memory or fix the row; run the plan again from step 1. |
| A lifetime's `start` exited, or its handle shows an error, after its folder was made | Yes, that arm's | Read `<root>\<lifetime>\outer.log` and `service.log`. Stop the other two lifetimes only if the cause is common to all. Point the arm at a spare root (below), write a new lifetimes file, spec and plan, pass the preflight, and launch that arm alone. Tell the coordinator. |
| A replay, session or gate step failed while the lifetimes are alive | No | Read the step's log under `$G`. Fix the harness cause, refreeze if a harness file changed, then resume: `run_plan.py --plan ... --log ... --from-step <N>` with the failed step's 1-based number. Sessions already complete are kept; `run_pass.py` resumes a pass. |
| An arm's replies show no model decisions (`arm_ok: false`, "took none", "not dispatched") | No, but the lifetime is running on the wrong thing | Stop. Do not run sessions. Read the lifetime's `stage` receipts and `airkctl arbiter status-legacy`. This is the case the Core and service fixes of 23 November closed; if it returns, it is a product defect: report it. |
| A lifetime reached its deadline before the plan finished | Yes, that arm's | The steps that already wrote results stand. What remains for that arm needs a spare root. Ask the coordinator before spending one. |
| Sessions fail with rate limits or overload | No | Lower `ARM_SESSIONS` in `make_loop_spec.py`, regenerate the spec and plan, refreeze, resume. |
| Two runs of one request return different read bodies | No | Stop and tell the coordinator (a known open question on combined reads). |
| Anything this table does not name | Assume yes | Stop and ask. |

### Pointing an arm at a spare root

The spare roots are `fixture-06-gate-lg-C32` and `-L32`, with the bases
`06-gate-lg-C32-base` and `06-gate-lg-L32-base`. A base is model-agnostic:
it holds the two indexes and the CPU roster, so either spare can serve C, L
or F. To run arm L in the C32 root:

```text
python -I -B scripts/arbiter-answer-eval/lg_lifetimes.py --series 06-gate-lg --r-root $R --image <image folder> --pair C=... --pair L=... --pair F=... --base C=06-gate-lg-C-base --base F=06-gate-lg-F-base --base L=06-gate-lg-C32-base --root L=C32 --out $L5C\lg-inputs\lifetimes-06-gate-lg-2.json
```

- The row keeps the lifetime name `06-gate-lg-L`, in the spare root, with
  arm L's pair and memory. Nothing else changes.
- What must match: the same image, the same pair and SHA-256, the same
  repositories, and a spare that has had no CUDA lifetime.
- Then set `roots` (`{"L": "C32"}`), `bases` and `lifetimes` in
  `gate-inputs.json`, regenerate the spec and plan, and pass the preflight.
  The preflight lists each spare as `free`, `spent` or unusable.
- The spent root keeps its files. Do not delete it during the run.

### The short trial (plan 12.5.2 item 4) and any other lifetime

Compact, then Large, then Compact in one lifetime, with peak memory,
per-call and reload times, runs in a root made for it, never in a gate
root. `R\fixture-s1-shakedown` has spent its CUDA lifetime (23 November), so
the next shakedown and trial need a new root. Such a root needs its own
closed CPU base first (`fixture.py start --repositories munarium`, about
6 min of indexing and 35 min of embedding on the CPU, 24 GB), and it too
gives one CUDA lifetime by default: run the shakedown and the trial in the same one. A
pair with both Context entries and the Focus entry is built with three
`--entry` options of `cuda_pair.py pair` (`lg-inputs\pair-CL-shakedown`).

`gpu_fixture.py stage --enrol-as TASK` sends the named enrollment tasks
instead of the emitted ones and records it in the stage record. It exists
for a lifetime whose image emits a wrong enrollment and which cannot be
restarted; it is refused in a gate root. The emitter of another image cannot
stand in: it refuses a service that is not its own binary ("actual private
fixture service identity changed").

### What the harness cannot do yet

- **Later reads against truncated rows.** The report shows reply size and
  what Union cut, per arm and repository. It does not yet count how many of
  an agent's later reads went to a location the Answer had delivered as a
  truncated row (a turn a better cut would have saved) as opposed to a
  location the reply did not hold. The data is in each session's
  `wrapper\turns.jsonl` and the Answer replies' span rows.
- **The trimming level** is not stated in a reply; the report shows what can
  be observed.
- **Per-stage model time** is in the service's decision log, not in the
  reply.
- **The precision check** is deferred (above).

## Tests

From this folder: `python -I -B -m unittest discover -s . -p test_gpu_fixture.py`
and `-p test_cuda_pair.py` (synthetic data only; they start no service and use no GPU).

## Limits

`gpu_fixture.py` starts one CUDA Context profile per lifetime with `--model compact|large`,
or up to two Context profiles and one CUDA Focus profile with `--model pair` (see above). Large on CUDA needs about 40 GiB of host reservation and
a 44 GiB device envelope. Results describe this private
image, driver and adapter, not the installed product.

## Private Arbiter decision capture

For a development-only CUDA pair, add `--card-capture` to the launch and
keep the native dispatch observer enabled. The normal default leaves
capture off. The checked native fixture writes
`development-card-calls.jsonl` and a matching
`development-card-calls.closed.json` in its ignored work directory.
Use settled development indexes and a bounded, serial Answer replay
so each call can be matched to its native admission and score receipt.
Verify native shutdown, the recorder closure, exact result ownership,
record counts, and the final digest before using the capture. Any
explicit loss or writer failure makes the capture incomplete.
The private card roster is evaluation custody and grants no read
or dispatch authority.

## Reuse settled development indexes

Pass --restart-from and --reuse-settled together for a development-only
same-install CUDA pair. The previous lifetime must have closed cleanly
with Focus and Arbiter disabled. The restart must reacquire the native
lease and prove the same source commits, index registrations, settled
publications and semantic pipeline. It compares Index and Vectors job
identities during startup and closure. Any observed drift fails the run
without a fresh build fallback. The new lifetime retains the original
control root and records its reuse evidence in reused-indexes.json.
Sealed gate roots keep their separate lifetime policy.
