# Selected model rights evidence tooling

These scripts assemble and verify the retained publisher and AIRK-generated asset attribution basis without constructing native Profile objects or copying large weights.

- build_rights.py collects pinned publisher grants and producer evidence and preserves the original Rust-written asset-array bytes.
- verify_rights.py verifies the assembled evidence against the retained selected model roster.
- fetch_base_readme.py acquires the original publisher base-model README used for attribution.
- native_json_projection.py mechanically projects native JSON arrays while preserving string escapes, integer lexemes and byte semantics; it refuses duplicate root keys.
- test_native_json_projection.py exercises those pure projection/refusal rules (five tests passed during migration).

The builder, fetcher and verifier retain historical data/output paths and may perform IO at import. They are preserved recipes, not supported relocated commands; a new run requires explicit guarded private input/output roots and fresh source/evidence pins. The pure projection module/test has no such data dependency. Existing evidence snapshots under artifacts were not copied here. source-provenance.json records all five original/maintained hashes. No network, license grant, model execution or qualification is implied by this capture.
