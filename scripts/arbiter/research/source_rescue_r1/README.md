# Historical source rescue, revision 1

This package accounts for the component-review lane of the 17 November source inventory: repository-shaped proposals/snapshots and Windows/workspace/packaging/platform operational scripts. The source map records each original path, SHA-256, byte count and disposition. Original ignored evidence is unchanged.

Exact historical Git matches name a reachable commit, path and blob; CRLF-to-LF comparison is explicitly identified and never described as raw-byte identity. Generated VS Code JavaScript is not copied when exact in-memory TypeScript emission matches and its TypeScript owner is independently accounted for. The compiler/config hashes and source owner are recorded.

`sources/` contains unique authored versions with original body bytes. Names carry full hashes to distinguish versions and avoid collisions. Unintegrated Rust is stored as `.rs.in`, with unchanged content; it is not an unreachable production module. No source here is mounted into Cargo, applied to current code or claimed to pass runtime qualification.

## Use and limits

`rescue.py` reads a separately authenticated private routing JSON and historical inputs, verifies hashes and inspects Git. It can repeat the read-only disposition with `python -B -S .../rescue.py --routing PATH --routing-sha256 SHA`. The optional `--write` switch creates fresh source copies and `source-map.json`; run it only after the same-human output-parent guard has checked this package and `sources/`. Audit all outputs afterward. It never executes rescued code; TypeScript emission stays in memory and creates no build artifacts.

The historical runners have old fixed paths, source pins, deadlines, process assumptions and imported sidecars. They are retained implementation sources, not safe commands for a new run. Fresh use requires an owning contract, reviewed input/output paths, authenticated dependencies and the current execution guard; no runner here authorizes account changes, model runs, building or packaging. Script-relative imports may require the original layout, recoverable from the map. Missing private inputs are real blockers; do not synthesize them or change pins. The rescue does not establish numerical or model-quality results.

Only source is preserved. No training corpus, checkpoint, indexed document, private key, token or model weights belong here. Automated private-key/token checks supplement source review; they do not prove absence of arbitrary embedded sensitive content. Machine-specific source paths and historical hashes are provenance, not credentials or execution approval.

The current utility and `test_rescue.py` enforce SPDX-prefix inversion. `replay_r1.py.in` preserves the exact original helper that produced the 416 generated-output proofs; it is historical source, not a second executable owner. `generated-replay-before-spdx.json` preserves the original proof metadata; current proof changes only that helper location and records the relocation.

Fresh-owner reconciliation added one explicit historical CPU transition test snapshot (`6decae70...--tests.rs.in`): its former tracked owner advanced during Clippy correction, so the older body now has its own exact source version. The package accounts for 1,222 paths: 806 relocated origins and 416 independently reproduced generated outputs. The original routing document remains unchanged; the map and utility declare the additional source explicitly.
