# Historical source snapshots

These Rust and TypeScript templates preserve native naming/diagnostic preimages,
CLI parity helper bodies, Admin activity/catalog request extraction baselines,
and Workbench native metadata/open-origin decoder proposals. They document exact
historical implementations and intermediate edits; `.in` files are not built.

Regeneration requires the eight original JSON containers named in
source-provenance.json. Run `python -B -S scripts/arbiter/source_rescue/embedded_history/snapshot_plan.py`
from the repository root to produce a read-only plan. Pipe that reviewed plan to
`python -B -S scripts/arbiter/source_rescue/embedded_history/snapshot_plan.py apply`
only after a same-human guarded shell audits every output parent; audit each
created file before consuming it. Apply refuses to overwrite source files.
The map pins each original container, RFC6901 pointer, decoded UTF-8 source,
optional SPDX prefix, and maintained bytes. Exact duplicates share a destination;
current tracked owners are reused. Git blob reachability does not imply that an
absent historical blob is a current maintained owner.

No historical code has been executed. These fragments require their original
surrounding modules and version-specific APIs; use current product sources for
execution. Replacement offsets and helper-name metadata remain in original
receipts, and the contracts prose field is not copied. No private fixtures,
indexed text, training examples, or JSON receipts are included.
