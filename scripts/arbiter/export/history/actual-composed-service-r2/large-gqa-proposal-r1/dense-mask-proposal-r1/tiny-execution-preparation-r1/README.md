# Historical recipe source: tiny-execution-preparation-r1

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `make_tiny_graph.py` â€” 1 original path(s); SHA-256 `7a8f37b7be733519ea4c3d299bcac21cd8a30ad3aa6156eb3b6bd4a1645af71e`.
  Declared CLI flags: `--execute-fixture-creation`, `--output`, `--python-site`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `onnx`, `os`, `pathlib`, `sys`.
- `run_tiny.py` â€” 1 original path(s); SHA-256 `ddb671a0373477ceb8a3c4868ae3f7102f20c8d711ac9dbf41e2f233eefdbfc7`.
  Declared CLI flags: `--card`, `--execute-reviewed`, `--image-inventory`, `--image-inventory-sha256`, `--preparation-sha256`, `--tokens`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `json`, `math`, `os`, `pathlib`, `re`, `runner`, `security`, `sys`, `time`.
- `stage_cards.py` â€” 1 original path(s); SHA-256 `d3c2ca87b5ca8d43df11ca269ae9cedcece077e79fef361324d99e16e82d81a8`.
  Declared CLI flags: `--stage`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `pathlib`, `runner`, `security`, `sys`.
- `test_controls.py` â€” 1 original path(s); SHA-256 `32d2ee8e014eeab28170d74f03ca89e5c803c9b5d2e3e5b01d595186b5e8b83e`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `importlib`, `pathlib`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
