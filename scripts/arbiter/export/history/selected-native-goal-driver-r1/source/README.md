# Historical recipe source: source

Retain the exact tool-source snapshot used by a native goal experiment: training execution ownership, encoder/decoder export, numerical references and calibration/development helpers. The snapshot remains distinct from the current tools/retrieval-train owners.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `export_encoder.py` â€” 1 original path(s); SHA-256 `81173cd56dfde9032ee24cc566751fcd9fc96b75457b43129ea9ecf45800f39b`.
  Declared CLI flags: `--baseline-original`, `--checkpoint`, `--corpus`, `--documentation`, `--output`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `importlib`, `json`, `numpy`, `onnx`, `onnxruntime`, `pathlib`, `safe_int8`, `safetensors`, `time`, `tokenizers`, `torch`, `transformers`.
- `qwen_export.py` â€” 1 original path(s); SHA-256 `b45a2a3d8c3cb6aa44f8093d563d0220209c1cb5201f313265f9d8cd75f25d8d`.
  Declared CLI flags: `--assets`, `--candidate`, `--output`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `importlib`, `io`, `json`, `numpy`, `onnx`, `onnxruntime`, `pathlib`, `safetensors`, `time`, `torch`, `transformers`.
