# Historical recipe source: full-execution-preparation-r1

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `freeze_full_card.py` â€” 1 original path(s); SHA-256 `0ece4f3f8383adfee33986358b8ce4c80429fd96e2a630957238cd9209c791fa`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `copy`, `hashlib`, `json`, `math`, `os`, `pathlib`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
