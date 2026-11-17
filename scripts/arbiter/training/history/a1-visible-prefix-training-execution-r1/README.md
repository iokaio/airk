# Historical recipe source: a1-visible-prefix-training-execution-r1

Probe and seal the MiniLM training runtime, verify its retained closure, and preserve the early process-owner implementation/tests from before suspended child creation. The pre-suspend owner is historical evidence, not the corrected supported launcher.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `probe.py` â€” 1 original path(s); SHA-256 `f56ec0e0338cc0aae2439b820a59fe0d76ec7c732e30cf1aa831b367b0cc735d`.
  Imported modules (stdlib, external and local; not an installation lock): `ctypes`, `importlib`, `json`, `numpy`, `os`, `pathlib`, `safetensors`, `subprocess`, `sys`, `tokenizers`, `torch`, `transformers`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `seal.py` â€” 1 original path(s); SHA-256 `6683e5d392fc89d5aa658d8f79d3e0f33d93210707ba877efab58ce602a9c881`.
  Imported modules (stdlib, external and local; not an installation lock): `hashlib`, `json`, `os`, `pathlib`, `platform`, `sys`, `time`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_training_execution-before-suspend.py` â€” 1 original path(s); SHA-256 `227c06490b650203f50e2b4691c21effc9dd34a9f5651e506627b3fc0af454c0`.
  Imported modules (stdlib, external and local; not an installation lock): `hashlib`, `json`, `os`, `pathlib`, `sys`, `tempfile`, `training_execution`, `types`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `training_execution-before-suspend.py` â€” 1 original path(s); SHA-256 `fa0c607d8668c2b50d829069eeb2509350f066a1756d8ff7892a145d65d83b86`.
  Declared CLI flags: `--log-dir`, `--seal`, `--seal-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `ctypes`, `hashlib`, `importlib`, `json`, `os`, `pathlib`, `subprocess`, `sys`, `time`.
- `verify.py` â€” 1 original path(s); SHA-256 `7472d7ff569e5c7d2cf4cdc1e2416cbce84690370810dfb8204d525d5ca4b6a9`.
  Imported modules (stdlib, external and local; not an installation lock): `json`, `pathlib`, `sys`, `time`, `training_execution`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
