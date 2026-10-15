# Incremental catalog experiment

Decision: retain full catalog builds in production. The isolated O2a prototype is available in `airk-store::catalog_experiment`, but no publication path calls it. It demonstrates a possible sparse-edit improvement, not a qualified replacement writer.

## Contract and correctness

`ChangedCatalogInput` binds exact base and target revision tokens. Added, removed and replaced paths are deterministically ordered; replacement compares the entire authenticated `InventoryFile`. Moves remain removal plus addition. Roots, profile tables, extractor identity, producer/import metadata or inventory schema changes select the existing full builder.

The experiment holds the base catalog lease, verifies its inventory and catalog component against the committed receipt, then copies the catalog into charged private staging. Only affected file, fact, relationship and continuity rows are replaced; bounded lineage tables and revision metadata are replaced completely. Independent imported rows sharing affected paths are restored. Private SQLite uses `journal_mode=OFF`, memory temporary storage and a verified page cap; failure cannot publish staging. Closing, integrity verification and syncing precede any hypothetical adoption.

Original differential fixtures compare every table's logical rows, schema/index definitions and lineage bytes for no-op, edit, add, delete, move and global fallback cases. Quota rejection, corrupted input and injected failure after copying leave active catalog bytes and the publication pointer unchanged.

## Measurements

Native Windows debug tests used 12 facts per file, one changed file, and three samples per size. Each sample built the full catalog before the private-copy candidate. These are warm, ordered synthetic measurements, not release-build, cold-cache or cross-platform qualification.

The final run charged the same 66,973,696-byte catalog cap to both strategies, after subtracting the final manifest and lineage allowance. Times below are microseconds.

| Files | Full samples | Copy/update samples | Full median | Copy/update median |
| --- | --- | --- | ---: | ---: |
| 16 | 1,483,049; 133,663; 137,828 | 136,714; 116,547; 117,320 | 137,828 | 117,320 |
| 1,000 | 6,608,630; 4,409,056; 4,975,713 | 4,699,008; 3,401,032; 4,124,988 | 4,975,713 | 4,124,988 |

The larger case improved by about 17.1% at the median and loaded one artifact instead of 1,000. It copied 31,494,144 bytes and produced 32,153,600 bytes, versus 31,498,240 for the full build: 655,360 extra bytes, about 2.08%. The smaller case copied 618,496 bytes and produced 634,880 bytes, versus 618,496 for full build. Neither strategy created a disk journal or temporary SQLite file in these fixtures. Private copying did not reduce charged headroom.

An earlier run showed substantial variation: the small-case medians were 91,924 microseconds full versus 92,582 copy/update, while the larger case was approximately 5.207 seconds full versus 3.439 seconds copy/update. That run allowed 135,168 additional catalog bytes; actual outputs were below both caps. Its stronger large-case improvement is not the decision baseline. The first final small-case full sample also shows contention, reinforcing the need to avoid treating these timings as a stable speed guarantee.

After timing, an additional canonical base-inventory hash check was added because the public catalog inventory can be mutated in memory. Final correctness tests include this check; recorded timings do not, and may slightly overstate the final prototype's benefit. No additional measurements were taken to conceal that limitation.

## Gate and decision limits

The complete experiment suite passed five tests, including the three-sample measurements. After the final authentication check, four non-measurement tests passed again, and `cargo clippy -p airk-store --all-targets --locked -j 1 -- -D warnings` passed. The local measurement receipt is `target/o2a-catalog-native.log`; generated build output is not a published artifact.

Full builds remain active because small workloads show parity or regression across runs, the larger benefit varies, copying increases output size and offers no quota-headroom reduction, and global changes still need full rebuilds. Adoption would require release/cold-cache/platform measurements, repeated-edit growth and compaction qualification, and renewed end-to-end publication/fault testing. Prototype correctness does not establish those production properties.
