# Historical recipe source: native-parent-bag-native-r95-preparation-r1

Prepare parent-bag native experiment packets, bind compiler/image/runtime evidence, retain owned process lifetime and phase deadlines, and validate source/receipt closure before numerical or DEV execution.

Source preservation only; do not execute/import these historical entries in place.
See the nearest history README/CONTRACT and source-map.json for reconstruction,
deduplication and missing private/runtime prerequisites. No model/data/run evidence
is bundled or inferred. Original bodies and their fixed pins remain unchanged.

## Preserved entries

- `anchor_bridge.ps1` â€” 3 original path(s); SHA-256 `30ad38eafb4e82448d8b382a16d2aa0ed60b834cc9e5f76e1758ad0205800bf0`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `anchor_runtime.py` â€” 2 original path(s); SHA-256 `389b16bb6882e3dc456753129ec1e43b40417365cf902d5af37ebf446076dbee`.
  Imported modules (stdlib, external and local; not an installation lock): `hashlib`, `json`, `os`, `pathlib`, `subprocess`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `finalize_fresh_packet.py` â€” 1 original path(s); SHA-256 `51044517e7427dc3870030df357fddbc232e3a2b2a8cbe740336138b57fed23c`.
  Declared CLI flags: `--cargo-json`, `--cargo-json-sha256`, `--output`, `--parent`, `--parent-sha256`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `pathlib`, `sys`.
- `finalize_proposed_packet.py` â€” 1 original path(s); SHA-256 `53eeb0d74d7b3d466a64973cd10aebb9902a77c2880ce48b2c234993e73a0e1f`.
  Imported modules (stdlib, external and local; not an installation lock): `hashlib`, `json`, `pathlib`, `sys`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `prepare_archive_binding.py` â€” 1 original path(s); SHA-256 `9b4a0c73fabe85ee3f448298a90315be8f83dcdaaee7e433f1bf2c7ad886ed8f`.
  Imported modules (stdlib, external and local; not an installation lock): `hashlib`, `json`, `pathlib`, `shutil`, `subprocess`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `prepare_packet.py` â€” 3 original path(s); SHA-256 `ed1a1beac9536e143b215b1dd16ed4c11f57bed1e62efde53c16f7e489f68570`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `pathlib`, `shutil`, `subprocess`, `sys`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `retain_compiler.py` â€” 3 original path(s); SHA-256 `d553798dbc45f6e38b1f6d1ac50c1cab7a2f26060874550d5eca802904021084`.
  Declared CLI flags: `--log`, `--output`, `--receipt`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `json`, `pathlib`, `shutil`, `subprocess`.
- `run_native.py` â€” 2 original path(s); SHA-256 `140997c10440c9d0f82c48f86ca7cebd95c19e84b0820bf54cdc278cefee9cb5`.
  Declared CLI flags: `--execute-reviewed`, `--packet`, `--packet-sha256`, `--stage`.
  Imported modules (stdlib, external and local; not an installation lock): `argparse`, `hashlib`, `importlib`, `json`, `os`, `pathlib`, `subprocess`, `time`.
- `test_anchor_runtime.py` â€” 3 original path(s); SHA-256 `4f7180b38fa0b25a09d2625743c843943be1ddd1e8e792bb269abbba36d63606`.
  Imported modules (stdlib, external and local; not an installation lock): `anchor_runtime`, `pathlib`, `tempfile`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_phase_deadline.py` â€” 2 original path(s); SHA-256 `d001bdcb8b8ab8704fdf13e467088552d13e9f4c5f9f44418690d619d7e9fa35`.
  Imported modules (stdlib, external and local; not an installation lock): `json`, `pathlib`, `run_native`, `tempfile`, `types`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
- `test_retention_refusals.py` â€” 3 original path(s); SHA-256 `be726b52a58c1f98ebce26ee16bc02f83008adfa6c51fa5dac9b906a7f84e43e`.
  Imported modules (stdlib, external and local; not an installation lock): `json`, `pathlib`, `retain_compiler`, `tempfile`, `unittest`.
  No argparse interface inferred; may execute on import or be a library/test. Inspect before any future use.
