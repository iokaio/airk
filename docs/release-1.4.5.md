# AIRK 1.4.5 development status

October 25, 2026. Implementation is in progress; this is not a release or a qualified installable candidate. No 1.4.5 VSIX has been built, installed or published.

The accepted [redb implementation plan](storagekv/storagekv-design.md) now includes every collection, usage, scheduler, service registry/job and private-staging store. The user requires the entire branch to have no SQLite dependency before any VSIX build, including build/test dependencies and legacy adapters. Supported-platform native qualification remains a public-release gate.

## Implemented so far

- Rust workspace/local Cargo lock entries and extension/npm root versions agree at 1.4.5. redb is pinned to 4.4.0.
- A store-owned backend enforces admitted length/write bounds, checks offset overflow, synchronizes file data and takes real OS file locks. Read snapshots retain the shared database handle. Durable writes select immediate durability and two-phase commit.
- Catalog planning/proof code has a storage interface with a redb typed-table adapter while preserving existing catalog digest bytes. An interim differential test compares generated insert/update/delete roots with SQLite and rejects omitted buckets.
- A graph storage interface and redb adapter preserve DiskANN record bytes and graph authentication domains. Applying a sealed delta verifies record/node streams and derives the target root/counts; focused tests cover record reads, missing-record tampering and retained snapshots.
- Redb journal primitives persist Prepared and Applying separately, authenticate bounded ordered afterimages/tombstones, and atomically install afterimages/head with a precommit verification callback. Tampering, failed verification, retained readers and actual child-process termination at phase boundaries have focused tests.
- The Windows build preflight and both extension packaging entry points invoke `scripts/check-storage-engine.py`. It rejects SQLite packages in Cargo.lock, the all-feature Cargo metadata graph and all workspace/crate dependency sections, including aliases and target/dev/build dependencies. There is no bypass flag, including `-SkipChecks`.

- Usage and scheduler ledgers now use bounded redb storage; focused suites passed 14 usage and nine scheduler tests. Large scheduler report bodies use separate typed rows. Existing SQLite ledgers are preserved and rejected pending an explicit upgrade import.
- Private graph overlays and vector registries use reservation-owned redb staging with bounded rows, checksums and capacity enforcement. The graph suite passed 12 tests and the vector-registry suite passed eight tests. A build-time compact empty database seeds small ledgers without raising their configured quotas.

The collection primitives are not yet connected to production collection or service-registry paths. They do not establish completion of the cutover or capacity/power-loss/platform qualification.

## Remaining before a local VSIX

1. Complete lexical authentication, posting/document/path/forward storage, budgets and equivalence tests.
2. Complete production graph-record integration and qualify the private-overlay/vector-registry ports; connect catalog and journal to production reads/publication/recovery with durable quota and ownership integration.
3. Implement sibling-incarnation format-150 rebuild/selection/retirement and automatic upgrade scheduling while preserving old reads and insufficient-space behavior.
4. Complete service registrations/jobs/configuration/watcher/lifecycle/maintenance ports and cross-component integration of the redb usage/scheduler ledgers. Establish and test a non-destructive upgrade path for existing settings, usage and pending work; dependency removal must not discard those records.
5. Replace the temporary executable SQLite oracle with retained bounded fixture results; remove SQL adapters and all SQLite dependencies, including CLI/kernel dev dependencies and transitive WASM packages.
6. Complete baseline/comparison measurements, fault/capacity campaigns, full Rust/repository/extension checks, notices and native model/runtime/archive verification. Build a VSIX only after the complete-cutover dependency gate passes.

## Evidence and limitations

Windows development evidence: nine focused redb tests passed across the backend/catalog/journal and graph targets. Four packaging-gate unit tests passed. A store-library run passed 299 tests before the final defensive metadata/bucket read-bound checks; the eight backend/catalog/journal tests passed again after those checks. Workspace formatting and all-target/all-feature Clippy passed again for this checkpoint, including the ledger/private-staging changes. Repository and whitespace checks, package-script syntax and offline 1.4.5 version agreement passed. These are development mechanism checks, not installed-VSIX qualification.

A previous all-feature workspace run had passing native CLI/model targets but no recorded final result. The checkpoint rerun was stopped during compilation to save the requested work-in-progress commit; full workspace tests remain outstanding and are not recorded as passing qualification.

The packaging gate currently fails as intended because SQLite dependencies remain. No end-to-end redb indexing result, comparative performance result, native Linux/macOS qualification or 1.4.5 VSIX acceptance is claimed.
