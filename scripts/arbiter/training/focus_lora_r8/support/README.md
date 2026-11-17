# Focus custody support

`candidate_overlay.py` preserves the complete retained overlay implementation and
its synthetic self-test, including the `identity()` function used by Focus plan
preparation and selection. Existing machine identity, owner/access, explicit source
allowlist, file hash and no-overwrite checks remain unchanged. The default SID is
the retained execution host identity, not an inferred cross-host authorization.

The overlay CLI now requires `AIRK_OVERLAY_OUTPUT_ROOT` to identify an existing
private directory beneath this repository's artifacts tree. Its output paths must
match that directory exactly; maintained source directories are never output roots.
The Focus identity-only import does not require this variable. Command interface:
`python -B -S scripts/arbiter/training/focus_lora_r8/support/candidate_overlay.py --help`.
This migration does not rerun or authorize an overlay transfer.

## Source identity and retained evidence

`source-map.json` records each original relative source path and SHA-256 alongside
its maintained SHA-256 and transformations. Original artifact copies are retained.
SPDX headers and path changes give these files new identities. Historical receipts
qualify their original bytes only. Existing digest, release, deadline, ownership,
CUDA identity and finite-value checks remain enforced. No source hash is silently
updated to make a historical seal pass. Fresh execution needs a reviewed source
transition and fresh releases/seals; this migration does not authorize execution.

Only implementation and synthetic tests are included. Data, labels, checkpoints,
weights, receipts, runtime environments and machine-specific releases are excluded.
The Apache-2.0 repository license applies to this authored code; installing model
and runtime dependencies does not transfer their licenses into this package.
