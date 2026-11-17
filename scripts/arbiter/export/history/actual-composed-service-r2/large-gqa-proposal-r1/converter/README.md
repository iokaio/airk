# Historical recipe source: converter

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `check_shapes.py` â€” 1 original path(s); SHA-256 `08794a4fcddddf55faff25434d9fdc0120b2b058b3691683473b967c9d457a50`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `gate_protocol`, `hashlib`, `json`, `onnxruntime`, `pathlib`, `struct`, `transformer`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `create_tiny_fixture_owned.py` â€” 1 original path(s); SHA-256 `42e32003975755e4f5986d26aed6fac5662e30d96111bb331f4f2acb08aaf24f`.
  Declared CLI flags: `--execute-reviewed`, `--plan`, `--plan-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `gate_protocol`, `hashlib`, `json`, `os`, `pathlib`, `reference_owned`, `reference_parent_no_site_r2`, `reference_worker`, `sys`, `time`, `windows_job_8g`.
- `gate_protocol.py` â€” 1 original path(s); SHA-256 `33f4c2657bea16d8926672949abc8bf3de78bffb710ca862b524dc84b33817db`.
  Imported modules (stdlib, external and local; not an installation lock): `contextlib`, `os`, `pathlib`, `re`, `struct`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `make_tiny_graph.py` â€” 1 original path(s); SHA-256 `98aa118e791925efa0df3b54d8fe40b3073b47c6696c8d5611a4fdcdf1ed8657`.
  Declared CLI flags: `--execute-fixture-creation`, `--output`, `--python-site`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `onnx`, `os`, `pathlib`, `sys`.
- `run_graph_owned.py` â€” 1 original path(s); SHA-256 `b6981241ce6a0c866a938f88aaac7e181ab46b2d89828102e316f46de76306d2`.
  Declared CLI flags: `--execute-reviewed`, `--plan`, `--plan-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `gate_protocol`, `hashlib`, `json`, `os`, `pathlib`, `reference_owned`, `reference_parent_no_site_r2`, `reference_worker`, `sys`, `time`, `windows_job_8g`.
- `test_gates.py` â€” 1 original path(s); SHA-256 `f804698bc33ee291debdd4bf9ecf0f8d0a12e56e7aeff18cb7b6a66638e0aeb7`.
  Imported modules (stdlib, external and local; not an installation lock): `gate_protocol`, `os`, `pathlib`, `struct`, `tempfile`, `types`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_large_literals.py` â€” 1 original path(s); SHA-256 `c002b6edd486ad1d942536d52c6bb2e21c2f66e2d240d480d11b47f64af2d9ff`.
  Imported modules (stdlib, external and local; not an installation lock): `gate_protocol`, `struct`, `test_attention_semantics`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_positions.py` â€” 1 original path(s); SHA-256 `bac1b4192a43bae4058fbe81f1adecc3ad6988aa46eee156bc99bb792729d624`.
  Imported modules (stdlib, external and local; not an installation lock): `math`, `pathlib`, `sys`, `test_attention_semantics`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_removals.py` â€” 1 original path(s); SHA-256 `90b02e8f7d8d4109eb136ad16952a7c11c59368f65349bd435694f15c8a981ff`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `gate_protocol`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `transformer.py` â€” 1 original path(s); SHA-256 `cb67415cbc1c527f1ba744f7cd726bd9c5620fd70dbe923b7b48d2c9e5547205`.
  Declared CLI flags: `--cpu-shape-source`, `--cpu-shape-source-sha256`, `--execute-graph-only`, `--execution-plan`, `--execution-plan-sha256`, `--output`, `--python-site`, `--source`, `--source-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `check_shapes`, `copy`, `gate_protocol`, `hashlib`, `heapq`, `json`, `onnx`, `os`, `pathlib`, `reference_worker`, `struct`, `sys`, `time`.
