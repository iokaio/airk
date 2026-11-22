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

## Tests

From this folder: `python -I -B -m unittest discover -s . -p test_gpu_fixture.py`
(synthetic data only; starts no service).

## Limits

`gpu_fixture.py` starts one CUDA Context profile per lifetime. The selector
itself accepts two (one Compact and one Large, plan 12.5.2 item 4): a two-entry
CUDA roster plus `AIRK_ARBITER_EVALUATION_CUDA_REVIEWS`, one pinned pass-1
graph diagnostic receipt per entry. Switching between them is an ordinary
`stage --disable` / `stage --enable`, or a direct Configure: the warm releases
the old model before it loads the new one. The launcher options for that mode
are not written yet. Large on CUDA needs about 40 GiB of host reservation and
a 44 GiB device envelope. Results describe this private
image, driver and adapter, not the installed product.
