# Focus LoRA r8

This folder preserves the 17-file F1 SCORE / F2 probe / F2 TRAIN implementation
from `f2-native-worker-implementation-r8`. `support/candidate_overlay.py` supplies
the retained human identity/custody checks. Worker imports remain sibling imports.

Python 3.13.7; retained runtime versions: Torch 2.10.0+cu128, Transformers 5.10.1,
NumPy 2.3.2, safetensors 0.8.0, ONNX 1.19.0. Use the exact sealed environment,
including all native dependencies; a pip version list alone is not the runtime seal.
The publisher model is Qwen3-Reranker-0.6B. Maintained publisher source lives at
`../../export/qwen/qwen3-reranker06-experiment-r1/run_focus_export.py` relative to
this folder. The runtime helpers live at `scripts/cuda-native-execution/helpers/`.

Seed 1908; q/k/v/o LoRA rank 16, alpha 16, dropout 0; AdamW learning rate 5e-5,
weight decay 0; one epoch, microbatch 1, effective batch 32 with the final 29-example
accumulation preserved. The population is 400 code TRAIN + 45 fixed replay, with
85 DEV examples used only for instruction selection. Training permits at most 14
optimizer updates, the measured probe one update. Original caps remain 600 seconds
(120 for probe), 16 GiB host committed, 8 GiB CUDA allocator and 8 GiB minimum free.

Inputs required but not distributed: exact original reference plan/assets/runtime,
all-original1955 cohort roster, fixed45 replay draws, native census and its naturally
settled owned receipt, GPU inventory, reviewed source epoch, then actual owned SCORE
selection and measured probe receipts. Keep their existing relative layout under an
explicit private execution root. Fresh plans bind maintained helper and publisher
source hashes; this does not rewrite or qualify retained plans.

Plan preparation entrypoint (replace each uppercase token with an actual path/hash):

```powershell
python -B -S scripts/arbiter/training/focus_lora_r8/make_plan.py --execution-root EXECUTION_ROOT --mode score --source-epoch SOURCE_EPOCH --source-epoch-sha256 SOURCE_EPOCH_SHA --native-census CENSUS --native-census-sha256 CENSUS_SHA --native-owned-receipt OWNED_CENSUS --gpu-inventory GPU_INVENTORY --log-dir FRESH_LOG_DIR --output FRESH_PLAN
python -B -S scripts/arbiter/training/focus_lora_r8/launch_owned.py --help
python -B -S scripts/arbiter/training/focus_lora_r8/select_instruction.py --help
```

The same preparer accepts `--mode probe --selection SELECTION`, then `--mode train
--selection SELECTION --measured-probe OWNED_PROBE`. Worker execution is only through
its reviewed owned launcher; `--help` is not permission to bypass that owner. The
unwritten bytecode-cache prefix is now under the fresh log directory rather than a
hardcoded user's profile. Score/probe/train must share the exact continuity key.

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

`--execution-root` must resolve beneath this repository's `artifacts/`; tracked
source and external roots refuse. Selection output must remain within that root.
These location checks complement, rather than replace, the existing human/ACL guard.
