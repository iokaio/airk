# Historical recipe source: compact-attention-proposal-r2

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `check_shapes.py` â€” 1 original path(s); SHA-256 `bd0fa7caa5ea0456f754bb8ca3d7bfa72be21bea40c2b8098cbdd07b0565d2cc`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `hashlib`, `json`, `onnxruntime`, `pathlib`, `struct`, `transformer`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `make_tiny_graph.py` â€” 4 original path(s); SHA-256 `a81238b48fba3e58fb429fbbd917ecfcb57427372ecba6158e15da26a20f2541`.
  Declared CLI flags: `--execute-fixture-creation`, `--output`, `--python-site`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `onnx`, `os`, `pathlib`, `sys`.
- `run_graph_owned.py` â€” 1 original path(s); SHA-256 `3829e6e29f3b403315fadae90103ee96bbaa09db72353f325f1a3b5fe511f847`.
  Declared CLI flags: `--execute-reviewed`, `--plan`, `--plan-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `os`, `pathlib`, `reference_owned`, `reference_parent_no_site_r2`, `reference_worker`, `sys`, `time`, `windows_job_8g`.
- `test_layout.py` â€” 5 original path(s); SHA-256 `66ce0ff705962198233af4a935b966afd221ab22cb35bca5a5f00ebadcd061ed`.
  Imported modules (stdlib, external and local; not an installation lock): `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `transformer.py` â€” 1 original path(s); SHA-256 `0579373dda9237424114ea740bcf60f78c038d508b3a984872fa86afdb04e82d`.
  Declared CLI flags: `--cpu-shape-source`, `--cpu-shape-source-sha256`, `--execute-graph-only`, `--execution-plan`, `--execution-plan-sha256`, `--output`, `--python-site`, `--source`, `--source-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `check_shapes`, `copy`, `hashlib`, `heapq`, `json`, `onnx`, `os`, `pathlib`, `reference_worker`, `struct`, `sys`, `time`.
