# Historical recipe source: source

Retain the exact tool-source snapshot used by a native goal experiment: training execution ownership, encoder/decoder export, numerical references and calibration/development helpers. The snapshot remains distinct from the current tools/retrieval-train owners.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `calibration_labels.py` â€” 1 original path(s); SHA-256 `3406d294214a5b74d2a389d011935624ab29921001b752de305835baf6623e32`.
  Declared CLI flags: `--capture`, `--fixture`, `--native-input`, `--output`, `--rubric`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `goal_rubric`, `goal_sources`, `hashlib`, `json`, `pathlib`.
- `encoder_development.py` â€” 1 original path(s); SHA-256 `7710f50ebbe821ae93db827f67741b53511415aaf05906b2fcd64f6f11fa5dd1`.
  Declared CLI flags: `--corpus`, `--output`, `--source`, `--trained`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `numpy`, `pathlib`, `safetensors`, `time`, `tokenizers`, `torch`, `transformers`.
- `prepare_encoder_baselines.py` â€” 1 original path(s); SHA-256 `0bede134f2394c6d6f506a50c8505729dc853a2f94884a07501b338b98cf7680`.
  Declared CLI flags: `--output`, `--source`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `pathlib`.
- `summarize_native_calibration.py` â€” 1 original path(s); SHA-256 `6db72342c0ae7fb75b7820a9e1bfa8a5492d9c625f88f02a4e86566f105edeb6`.
  Declared CLI flags: `--scores`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `math`, `pathlib`.
- `test_training_execution.py` â€” 1 original path(s); SHA-256 `09dbc24e8bff7d82bcf064746e40ee33bbcedf3182a87c938b4aba4dccb3631d`.
  Imported modules (stdlib, external and local; not an installation lock): `hashlib`, `json`, `os`, `pathlib`, `sys`, `tempfile`, `time`, `training_execution`, `types`, `unittest`, `venv`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `training_execution.py` â€” 1 original path(s); SHA-256 `19a3a80e35123589cdb6ede96f1aad37e841f7a0913d734b79d331caed5741af`.
  Declared CLI flags: `--log-dir`, `--seal`, `--seal-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `contextlib`, `ctypes`, `hashlib`, `importlib`, `json`, `msvcrt`, `os`, `pathlib`, `subprocess`, `sys`, `time`.
- `verify_encoder_fp32.py` â€” 1 original path(s); SHA-256 `2a04374964e36d067d284ebf44cc2e57966120de44c58a3770fd730c39c629b7`.
  Declared CLI flags: `--checkpoint`, `--exports`, `--oracle`, `--output`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `numpy`, `pathlib`, `safetensors`, `torch`, `transformers`.
- `verify_encoder_inputs.py` â€” 1 original path(s); SHA-256 `c537e91ee131265f511479c671e1f7cb983ca50122ced51b070788d6fca2e831`.
  Declared CLI flags: `--exports`, `--legacy-root`, `--output`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `numpy`, `onnxruntime`, `pathlib`, `tokenizers`.
