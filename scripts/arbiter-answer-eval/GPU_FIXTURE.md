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
    [--repositories airk,bookbuilder] [--synthetic-root <git tree>] [--seconds N]
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

### Re-emitting the FP16 profiles for the loop (run when L4 integrates)

L4 moves `runtime.rs` (to `c9008eef…`). That file is an input of the FP16
math-policy digest, so the G5 and protected-MHA identities and every
bounded-context policy move, and both FP16 profiles must be emitted again.
They must also declare the four card tasks through
`airk_retrieval::arbiter::runtime::admit_loop_tasks`. That function replaces
the JointCalibration with a zero-rule `LoopCalibrationSet` and sets the
Calibration asset and the `calibration` digest.

**Code step.** In `crates/airk-retrieval/tests/cuda_l5a_diagnostics.rs`
`l5a_prepare`, protected branch, add an opt-in `L5A_LOOP_TASKS=1`:
1. after `prepare_protected_mha_profile` returns, call
   `admit_loop_tasks(&mut profile)`;
2. take the returned bytes as the calibration bytes, under the same asset
   name;
3. recompute the manifest under a distinct domain,
   `airk.private.cuda-protected-mha-loop-profile/1`, over the base manifest,
   the runtime-manifest SHA and the loop profile with its manifest zeroed;
4. run `profile.validate()` and `validate_selected_policies`.

The edit is in a test file only, so no digest-input file moves. Pass 1 needs
the same switch, because the bounded policy is computed over the emitted
profile.

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

## Tests

From this folder: `python -I -B -m unittest discover -s . -p test_gpu_fixture.py`
and `-p test_cuda_pair.py` (synthetic data only; they start no service and use no GPU).

## Limits

`gpu_fixture.py` starts one CUDA Context profile per lifetime with `--model compact|large`,
or up to two Context profiles and one CUDA Focus profile with `--model pair` (see above). Large on CUDA needs about 40 GiB of host reservation and
a 44 GiB device envelope. Results describe this private
image, driver and adapter, not the installed product.
