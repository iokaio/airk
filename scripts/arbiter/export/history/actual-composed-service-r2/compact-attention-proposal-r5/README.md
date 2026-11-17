# Historical recipe source: compact-attention-proposal-r5

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `check_shapes.py` â€” 1 original path(s); SHA-256 `ce4f7ee3e0c4fdc3c981bdf812a48f9206ee67ee9d2c7282eddc297b010c702e`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `gate_protocol`, `hashlib`, `json`, `onnxruntime`, `pathlib`, `struct`, `transformer`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `gate_protocol.py` â€” 1 original path(s); SHA-256 `5f2fc7dd90d15b221aee320463084dd4e98710ef50ac31faa6634ecdf6015be5`.
  Imported modules (stdlib, external and local; not an installation lock): `contextlib`, `os`, `pathlib`, `re`, `struct`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_external_values.py` â€” 2 original path(s); SHA-256 `a8933903f5c27c81b7be6860e78460f79958db989d99dcc455cfe2c07fbeb69b`.
  Imported modules (stdlib, external and local; not an installation lock): `gate_protocol`, `types`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
