# MiniLM export: visible-prefix-r1

Status: historical source retained for reproducibility; no export, model run or qualification performed by this migration. Original source locations: `artifacts/advanced-split/models/visible-prefix-export-source-r1`.

See the parent MiniLM README and `source-map.json` for exact source hashes, reversible header/root-path edits, repeated-byte groups, runtime prerequisites and omitted private inputs. Numerical thresholds, checkpoint/data pins and process limits are unchanged. Original files and receipts remain untouched.

Run the data-free checks from the repository root:

```powershell
python -B -S scripts/arbiter/export/minilm/test_migration.py
```

The runner exports a specifically pinned, preselected epoch-1 candidate. It requires checkpoint weights/config/tokenizer, reviewed TRAIN-only corpus/documentation/license inputs, and its retained helper modules. `verify_encoder_inputs.py` supplies independent fixed/masked numerical cases; `verify_encoder_fp32.py` compares the eager FP32 encoder. These gates do not establish retrieval quality.

`stage.py`/`launch.py` own the historical bounded CPU workflow. Native preparation/materialization and `run_native.py` require actual export results, fresh native images and compile/link custody before numerical or DEV execution. Grading requires exact retained captures and masks. All private JSON inputs and actual result artifacts are excluded. Preparation adapters require AIRK_RECIPE_RUN_ROOT and write generated source/metadata only there; see the private preparation interface in the MiniLM root README.

Runner command template: `python -B scripts/arbiter/export/minilm/visible-prefix-r1/runner.py --checkpoint <private-epoch-1> --corpus <pinned-corpus.json> --documentation <pinned-documentation.json> --output <fresh-ignored-output>`. Dispatch only through the reviewed stage owner after fresh source/input sealing.

See the [private preparation interface](../README.md#private-preparation-interface) for explicit run-root commands, fresh-output requirements and unchanged historical input prerequisites.
