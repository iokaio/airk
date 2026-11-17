# Historical recipe source: context-action-qwen-shared-experiment-r1

Compare and prepare context/action model reference behavior for the retained shared experiment, preserving its input, source and owned-runtime bindings.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `prepare_train_label_queue.py` â€” 1 original path(s); SHA-256 `61e931dad590693e751ed7a2e2203f47c78514711dc75c868b84faf54f2ef7da`.
  Imported modules (stdlib, external and local; not an installation lock): `hashlib`, `json`, `pathlib`, `re`, `threading`, `time`, `types`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_human_guard.py` â€” 1 original path(s); SHA-256 `140ab3343e4570c652d93b599d7b4734dbc08f2aa895624de0b6d647d69ab416`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `json`, `pathlib`, `types`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_label_audit.py` â€” 1 original path(s); SHA-256 `6c1e9137ca1fbb9694ddd3de02036e4956d775b8a8b79402eb0f9cc140f70bad`.
  Imported modules (stdlib, external and local; not an installation lock): `copy`, `json`, `pathlib`, `types`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `validate_human_responses.py` â€” 1 original path(s); SHA-256 `aeed31799b33766184a40b21f80d75153c184943308ad9f8e8626a25bb649d33`.
  Declared CLI flags: `--responses`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `pathlib`.
