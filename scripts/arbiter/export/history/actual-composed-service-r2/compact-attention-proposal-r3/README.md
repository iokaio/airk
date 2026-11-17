# Historical recipe source: compact-attention-proposal-r3

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `check_shapes.py` â€” 2 original path(s); SHA-256 `15d59b8abe6f362ee2dd497b4c773d882824433b95d8efbbbff3f6467330e6df`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `gate_protocol`, `hashlib`, `json`, `onnxruntime`, `pathlib`, `struct`, `transformer`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `gate_protocol.py` â€” 1 original path(s); SHA-256 `327a4d9b7f350bf2f132711ca49523785d33506714e497681676e1ba755d94a8`.
  Imported modules (stdlib, external and local; not an installation lock): `contextlib`, `os`, `pathlib`, `struct`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `run_graph_owned.py` â€” 3 original path(s); SHA-256 `c33f75c452be5a947a207ea82cad098dc5667c1d3fe60dc237f19cfdff089e2d`.
  Declared CLI flags: `--execute-reviewed`, `--plan`, `--plan-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `gate_protocol`, `hashlib`, `json`, `os`, `pathlib`, `reference_owned`, `reference_parent_no_site_r2`, `reference_worker`, `sys`, `time`, `windows_job_8g`.
- `test_gates.py` â€” 3 original path(s); SHA-256 `6bef9b9b5b0ca60bbe2e92f4e41a6b745312c029d675c6ec7ce32e4e8c131a79`.
  Imported modules (stdlib, external and local; not an installation lock): `gate_protocol`, `os`, `pathlib`, `struct`, `tempfile`, `types`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `transformer.py` â€” 1 original path(s); SHA-256 `912a72ac725a587a232de57da8182f916ccdee2d3893ef4c21204308a1702e04`.
  Declared CLI flags: `--cpu-shape-source`, `--cpu-shape-source-sha256`, `--execute-graph-only`, `--execution-plan`, `--execution-plan-sha256`, `--output`, `--python-site`, `--source`, `--source-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `check_shapes`, `copy`, `gate_protocol`, `hashlib`, `heapq`, `json`, `onnx`, `os`, `pathlib`, `reference_worker`, `struct`, `sys`, `time`.
