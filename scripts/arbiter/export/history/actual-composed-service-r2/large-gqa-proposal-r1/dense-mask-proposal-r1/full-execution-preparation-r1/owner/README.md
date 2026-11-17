# Historical recipe source: owner

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `capacity.py` â€” 2 original path(s); SHA-256 `2924da5947e5420f56fab8abb91adcc71e705a1a4bb6eb806822bbed316cbc78`.
  Imported modules (stdlib, external and local; not an installation lock): `ctypes`, `json`, `math`, `os`, `pathlib`, `stat`, `time`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `runner_dense_profile_only.py` â€” 1 original path(s); SHA-256 `b9c2a13a23e24bfbda094194b2eecd94a1df347bd582f51b18726de11aa65cf4`.
  Declared CLI flags: `--execute-reviewed`, `--plan`, `--plan-sha256`, `--source-check`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `capacity`, `ctypes`, `hashlib`, `importlib`, `json`, `math`, `os`, `pathlib`, `re`, `security`, `shutil`, `sys`, `time`, `watch`.
- `security.py` â€” 2 original path(s); SHA-256 `992659028a55b34bee9899d0baa0f2d36e816ecf181b15fef4d4a24d12ed7cb9`.
  Imported modules (stdlib, external and local; not an installation lock): `ctypes`, `os`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `watch.py` â€” 1 original path(s); SHA-256 `88a177ee64997f15d437217947e3856f6043c1b942d4a55a85f3b0bb60911962`.
  Imported modules (stdlib, external and local; not an installation lock): `capacity`, `ctypes`, `math`, `pathlib`, `time`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
