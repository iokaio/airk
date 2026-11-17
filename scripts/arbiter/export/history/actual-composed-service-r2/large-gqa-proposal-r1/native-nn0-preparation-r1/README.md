# Historical recipe source: native-nn0-preparation-r1

Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `freeze_large_metadata.py` â€” 1 original path(s); SHA-256 `482dd4bebed7634bf7c8318f960f89f550a91ca4574baedc2b6835687ed13286`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `copy`, `hashlib`, `json`, `os`, `pathlib`, `subprocess`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `run_nn0_owned.py` â€” 1 original path(s); SHA-256 `c5f7be42f67ae69e3b5b93a48f389a4d8a5555d6ecce9b326b4fb23f2c60c3f0`.
  Declared CLI flags: `--execute-reviewed`, `--input-sha256`, `--kind`, `--staging`, `--staging-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `os`, `pathlib`, `reference_owned`, `reference_parent_no_site_r2`, `reference_worker`, `sys`, `time`, `windows_job_8g`.
