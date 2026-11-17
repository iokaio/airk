# ONNX storage normalization

This component owns the selected Focus and Context graph-storage normalization implementation. It moves eligible external tensor values inline while proving topology, tensor values and main weight declarations unchanged. It imports ONNX metadata support only; no Torch, ORT inference or neural forward belongs here. The original four artifact sources and their receipts remain unchanged; source-provenance.json records original/maintained hashes. The normalizer implementation and synthetic tests differ only by SPDX headers.

Run the worker under a fresh reviewed same-user no-forward execution seal and output/cache custody:

```powershell
python -B scripts/arbiter/export/onnx_storage/normalize_focus_artifact.py --source artifacts/<selected-focus-weight8> --output artifacts/<fresh-normalized-focus>
```

The wrapper requires a complete original selected weight8 receipt and authenticates all graph/sidecar bytes before copying. It refuses tracked-source outputs and inputs. owned_normalization.py now requires the fresh seal's normalization_source alongside normalization_output and all exact maintained source/runtime/input pins. Historical seals omit that new explicit path and cannot be reused unchanged.

Context weight8 r3 preparation and execution now bind this maintained normalizer path and hash. A historical release naming an artifact executable is rejected. Fresh source pins are mandatory; old receipts remain historical evidence. The normalizer API accepts explicit model, original-graph and proof paths; callers must provide guarded fresh private outputs.

The synthetic ONNX fixture tests create files in temporary directories and require an approved same-user test root and provisioned ONNX. Source/path admission tests require no model packages. No graph conversion or model qualification was performed during migration.

Context's versioned prepare_quant.py remains a historical preparation recipe: its other HERE-derived data, output, log and cache paths require a separate fresh adapter. Updating its normalizer pin does not make that full preparer runnable after relocation. Use the explicit selected worker interfaces only through fresh reviewed ownership and source seals; do not run the historical preparer unchanged.
