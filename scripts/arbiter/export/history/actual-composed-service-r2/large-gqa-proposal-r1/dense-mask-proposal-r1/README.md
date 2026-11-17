# Historical recipe source: dense-mask-proposal-r1

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `test_dense_recipe_math.py` â€” 1 original path(s); SHA-256 `31a6fe88327b6c009fb0478d769f72a854d1e524fac567bcde5cc97b94ab868b`.
  Imported modules (stdlib, external and local; not an installation lock): `fractions`, `math`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_dense_recipe_math_r3.py` â€” 1 original path(s); SHA-256 `ba16995708166bdcef51715c668a252e61acb26dea3500c1f3d7d07dce1b5e3a`.
  Imported modules (stdlib, external and local; not an installation lock): `fractions`, `math`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
