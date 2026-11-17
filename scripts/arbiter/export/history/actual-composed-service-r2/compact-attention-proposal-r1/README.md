# Historical recipe source: compact-attention-proposal-r1

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `test_attention_semantics.py` â€” 6 original path(s); SHA-256 `c3dcc70356bc07b5f1f8413c0d0d08680be2b3eee640632eae86db5f38652097`.
  Imported modules (stdlib, external and local; not an installation lock): `math`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
