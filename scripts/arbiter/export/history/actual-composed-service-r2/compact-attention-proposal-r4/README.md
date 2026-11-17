# Historical recipe source: compact-attention-proposal-r4

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `gate_protocol.py` â€” 1 original path(s); SHA-256 `00a5c197688a2f9812eadb60c6ab9c11121437412d211b45910127aab27190c0`.
  Imported modules (stdlib, external and local; not an installation lock): `contextlib`, `os`, `pathlib`, `re`, `struct`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `inspect_removals.py` â€” 2 original path(s); SHA-256 `26a6301f7571aa468d35eebc812b791de46cf2b7cd28e13d2075473c95b07537`.
  Declared CLI flags: `--output`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `collections`, `gate_protocol`, `hashlib`, `json`, `onnx`, `pathlib`, `sys`.
- `test_removals.py` â€” 2 original path(s); SHA-256 `1181bd66325a69201d3dc4310c3a43d401763674fbd564b3d3fc5ae7d318cfb8`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `gate_protocol`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `transformer.py` â€” 2 original path(s); SHA-256 `5e26bee74873f6e0156886dcd81d0e12eb9d97892bccae5fc4005b59f3606bf2`.
  Declared CLI flags: `--cpu-shape-source`, `--cpu-shape-source-sha256`, `--execute-graph-only`, `--execution-plan`, `--execution-plan-sha256`, `--output`, `--python-site`, `--source`, `--source-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `check_shapes`, `copy`, `gate_protocol`, `hashlib`, `heapq`, `json`, `onnx`, `os`, `pathlib`, `reference_worker`, `struct`, `sys`, `time`.
