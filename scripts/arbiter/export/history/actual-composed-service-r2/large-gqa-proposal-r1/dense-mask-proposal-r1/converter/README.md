# Historical recipe source: converter

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `check_shapes.py` â€” 1 original path(s); SHA-256 `17d943ba820eb3a787fc66cf402b73f7625ed91a6dbeb1112fde88cda52d2a0b`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `gate_protocol`, `hashlib`, `json`, `onnxruntime`, `pathlib`, `struct`, `transformer`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `gate_protocol.py` â€” 1 original path(s); SHA-256 `d253295d2211b4a860f9e0c6b65b7538c148ded631969830fda3200fa81555c8`.
  Imported modules (stdlib, external and local; not an installation lock): `contextlib`, `os`, `pathlib`, `re`, `struct`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `run_graph_owned.py` â€” 1 original path(s); SHA-256 `73a84cb05889ceb6bf2086cdc86eae5e3806128152e175f11eefa58b2cdc82ad`.
  Declared CLI flags: `--execute-reviewed`, `--plan`, `--plan-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `gate_protocol`, `hashlib`, `json`, `os`, `pathlib`, `reference_owned`, `reference_parent_no_site_r2`, `reference_worker`, `sys`, `time`, `windows_job_8g`.
- `test_dense_masks.py` â€” 1 original path(s); SHA-256 `db6ab3fcb7a5a062e704b74e05ed7e0c87884aefeff60a18676d50ef640c7733`.
  Imported modules (stdlib, external and local; not an installation lock): `gate_protocol`, `math`, `struct`, `transformer`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `transformer.py` â€” 1 original path(s); SHA-256 `40252fafe90bcf77d090964ddfe82c8e54df59905981056cc79e3b13edca5e06`.
  Declared CLI flags: `--execute-graph-only`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `check_shapes`, `copy`, `gate_protocol`, `hashlib`, `json`, `onnx`, `os`, `pathlib`, `reference_worker`, `struct`, `sys`, `time`.
