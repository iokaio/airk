# Historical recipe source: owner

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `runner_large_profile_only.py` â€” 1 original path(s); SHA-256 `bba1e24441ff4d6b25a3a3f3f215bf7ad9b10e348e3150c943b901d18d744817`.
  Declared CLI flags: `--execute-reviewed`, `--plan`, `--plan-sha256`, `--source-check`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `capacity`, `ctypes`, `hashlib`, `importlib`, `json`, `math`, `os`, `pathlib`, `re`, `security`, `shutil`, `sys`, `time`, `watch`.
- `watch.py` â€” 1 original path(s); SHA-256 `c049a2b156ff961c7c93e2e998e1529a05ee4f00fd04a35ddc4c4e6661ed6f51`.
  Imported modules (stdlib, external and local; not an installation lock): `capacity`, `ctypes`, `math`, `pathlib`, `time`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
