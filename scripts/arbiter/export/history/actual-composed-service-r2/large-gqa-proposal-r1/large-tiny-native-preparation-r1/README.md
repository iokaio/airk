# Historical recipe source: large-tiny-native-preparation-r1

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `run_tiny.py` â€” 1 original path(s); SHA-256 `0802a595c192e3bf5ccd99ae4af3bddc936706960ba6fe148ca785d1de112e7e`.
  Declared CLI flags: `--execute-reviewed`, `--image-inventory`, `--image-inventory-sha256`, `--preparation-sha256`, `--tokens`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `json`, `math`, `os`, `pathlib`, `re`, `runner`, `security`, `sys`, `time`.
