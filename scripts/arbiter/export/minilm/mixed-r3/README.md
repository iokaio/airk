# MiniLM export: mixed-r3

Status: historical source retained for reproducibility; no export, model run or qualification performed by this migration. Original source locations: `artifacts/advanced-split/models/minilm-exports-mixed-r3`, `tools/retrieval-train`.

See the parent MiniLM README and `source-map.json` for exact source hashes, reversible header/root-path edits, repeated-byte groups, runtime prerequisites and omitted private inputs. Numerical thresholds, checkpoint/data pins and process limits are unchanged. Original files and receipts remain untouched.

Run the data-free checks from the repository root:

```powershell
python -B -S scripts/arbiter/export/minilm/test_migration.py
```

Export entry point: `python -B scripts/arbiter/export/minilm/mixed-r3/runner.py --checkpoint <private-checkpoint> --corpus <pinned-corpus.json> --documentation <pinned-documentation.json> --output <fresh-ignored-output>`. This executes export and numerical probes; use the reviewed owned launcher and verify all output parents first. Required libraries are Torch, Transformers, safetensors, tokenizers, NumPy, ONNX and ONNX Runtime in the retained pinned environment.

Historical limitation: the imported helper was not sealed in the original run. The supplied retained helper is source preservation, not proof of its historical invocation.
