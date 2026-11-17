# MiniLM finite encoder recovery r1

Preserves 23 source/test files, including finite r2 and native-token-admitted r3
training, prerequisite preparation, resource diagnostics and the historical process
owner regression. This is a versioned retained recipe; r2 is not silently replaced
by r3. Both retain finite loss, gradients, weights and optimizer-state checks.

Python 3.13.7, Torch 2.10.0+cu128 (CUDA 12.8, cuDNN 91002), Transformers 5.10.1,
safetensors 0.8.0, tokenizers 0.22.2, NumPy 2.3.2 were recorded in the retained
runtime seal. Runtime/driver/native dependency closure and the actual GPU identity
must also match a newly reviewed seal. No model downloads occur in this migration.

Seed 1908; one epoch, 13 steps, learning rate 2e-5, temperature 0.05, parent batch 8,
maximum 27 unique units, 256 tokens, unit microbatch 8. The retained population has
99 admitted weak parent bags out of 315 (96 QASPER, 3 Requests); no fragment-positive
labels are invented. Original limits remain 600 seconds, 8 GiB aggregate host and
4 GiB GPU. An r3 tokenizer witness binds all 180 units and 99 queries.

Set `AIRK_RESEARCH_MODELS` to an explicit private working tree with the retained
models-directory layout; execution requires an explicit path beneath repository `artifacts/`.
Data and preparation outputs use that tree, while executable siblings come from
this tracked folder and `../parent_bag_r1`. Required private inputs include
`encoder-recovery-preparation-r1/protocol.json`, approved stage releases/seals,
resource and owned receipts, tokenizer witness, `native-parent-bag-candidate-r1/`
recipe/master/batches, `native-parent-bag-units-r4/units.private.json`, and a local
MiniLM source containing tokenizer/config/model.safetensors. Those files are not
included. Fixed original pins remain explicit in the sources, including protocol
`eb6f60be6af095c2c00ec091a0afb1668dc5e02af0cb5049a4b2a1540bcfaa2a`.

Review the prerequisite sequence: `prepare_runtime_amendment.py`,
`prepare_stage_seal.py` / `check_equivalence.py`, `prepare_resource_r2.py` /
`resource_pilot.py`, then `prepare_train_r2.py` or `prepare_token_witness_r3.py` and
`prepare_train_r3.py`. Never execute these merely because inputs exist: several
preparers bind historical source hashes and intentionally refuse this transformed
source under old releases. Creating a new reviewed recipe/source epoch is required.
They must not be patched to accept whichever files happen to be present.

Exact trainer interface, to be placed in a newly sealed owned command:

```powershell
python -B -S scripts/arbiter/training/minilm/encoder_recovery_r1/train_finite_r3.py --source MINILM_SOURCE --output FRESH_PRIVATE_OUTPUT --execution-seal SEAL --execution-seal-sha256 SEAL_SHA --primary-release RELEASE --primary-release-sha256 RELEASE_SHA
```

Use `train_finite_r2.py` with the same arguments only for the r2 recipe. This direct
interface listing does not replace `tools/retrieval-train/training_execution.py`'s
owned launch/gate protocol. The frozen primary release and original owner source
hash still gate model imports and gradients. The migration does not claim a fresh
end-to-end run; absent licensed data, runtime custody or reviewed releases block it.

## Source identity and retained evidence

`source-map.json` records each original relative source path and SHA-256 alongside
its maintained SHA-256 and transformations. Original artifact copies are retained.
SPDX headers and path changes give these files new identities. Historical receipts
qualify their original bytes only. Existing digest, release, deadline, ownership,
CUDA identity and finite-value checks remain enforced. No source hash is silently
updated to make a historical seal pass. Fresh execution needs a reviewed source
transition and fresh releases/seals; this migration does not authorize execution.

Only implementation and synthetic tests are included. Data, labels, checkpoints,
weights, receipts, runtime environments and machine-specific releases are excluded.
The Apache-2.0 repository license applies to this authored code; installing model
and runtime dependencies does not transfer their licenses into this package.

## Maintained loss and private-root transition

Set `$env:AIRK_RESEARCH_MODELS` explicitly to the guarded private models directory
beneath this checkout's `artifacts/` before invoking preparation or training. Missing,
tracked-source and external roots refuse before private input reads or output writes.
The same value is propagated into new stage seals. Source maps retain the original
bytes; this restriction does not qualify old outputs under new sources.

`check_equivalence.py`, `resource_pilot.py`, `prepare_stage_seal.py` and
`prepare_resource_r2.py` additionally require `--loss-source-sha256 LOSS_SHA`, the
independently reviewed SHA-256 of maintained `../parent_bag_r1/bag_loss.py`.
The worker verifies and captures those exact bytes before importing Torch, executes
only that capture and records `maintained_loss_source_sha256`. New stage seals bind
the maintained loss path and pass the explicit argument. Original protocol pins are
still checked and never rewritten; they do not authorize the relocated helper.

`prepare_runtime_amendment.py` is a historical runtime-only adapter: it preserves
its base command and worker source roles. An old base may still name the original
artifact worker and cannot establish execution of maintained source. For a maintained
execution claim, start with a fresh maintained stage base carrying the explicit
loss-source pin; never silently retarget an old command or reuse its outcome.
