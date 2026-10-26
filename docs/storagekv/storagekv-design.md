# AIRK Storage: Replacing SQLite with a Native Rust KV Engine

Oct 25, 2026 · @Tyler Jensen

## Summary

Implement the accepted [redb](https://github.com/cberner/redb) design for v1.4.5 as the page engine for an AIRK-owned collection index: authenticated catalog, DiskANN graph records, lexical inverted index and durable publication journal. Start with an adapter inside `airk-store`; `airk-kv` is a working name, not a decision to add a crate. Extract a crate only when it has an independent purpose and an explicit dependency boundary.

This records the accepted implementation plan for the locally preserved experiment. Implementation and local Windows/Linux qualification are complete to the limits below; the owner abandoned adoption on October 26, 2026. This plan is retained as an experimental implementation record, not future shipping work. See the [experiment and decision](../redb-experiment.md). The pre-cutover baseline is the 1.4.4 native service and format-140 WAL store, not the historical format-126 rollback store. Preserve concurrent committed reads during Prepared and Applying, request-held object leases, revision identity, collection/root isolation, sibling-incarnation rebuilds and bounded retirement. DiskANN's codec/search algorithm, retained objects, original locations and source read-only behavior remain unchanged.

Intended benefits are removal of collection WAL/SHM files, lexical authentication proportional to the evidence traversed, and computing lexical updates once instead of preparing, sealing, rolling back and computing them again. Pending and live records still require separate writes; this is not one physical write. Latency, space and binary-size improvements require measurement. The v1.4.5 scope includes usage, scheduler, service registry/jobs, private staging and all test/build adapters. No SQLite dependency may remain in manifests, lockfiles, resolved Cargo graphs or packaged payloads before any VSIX build. Tree-sitter grammars retain their separate native build requirements.

The costs are a new collection format, fresh setup/full indexing at the SQLite boundary, old/new redb storage overlap during later rebuilds, custom lexical indexing and backend/recovery qualification. redb is the selected engine; implementation acceptance still requires the gates below. A failed gate requires fixing or reporting the blocker, not silently shipping a SQLite fallback.

## Accepted October 26 cutover decision

Version 1.4.5 must contain no SQLite code or dependency, including runtime, migration, legacy, test, development and build paths. There is no SQLite compatibility reader, import/export bridge or separate conversion utility. Fresh setup and a full index rebuild replace the earlier preserving-upgrade proposal. Existing SQLite-backed registrations, settings, pending jobs and usage history are not migrated. Existing agent bindings may need refreshing after registration; disclose that before presenting setup as complete.

Remove old AIRK-owned SQLite databases and their recognized sidecars only after proving the owning namespace and excluding old workers, writers and readers. Cleanup uses file/receipt metadata and never opens SQLite. Preserve source files, unknown files, unrelated projects and settings stored outside the retired SQLite state. Validate resolved deletion targets against the approved owned directory; make interrupted cleanup resumable and report locked/foreign files explicitly. Do not silently redirect a stale binding to an empty index or present the reset as a successful migration.

The fresh-setup rule applies to the SQLite cutover. Subsequent format-150 redb publications and rebuilds retain the snapshot, identity, quota and sibling-handoff guarantees below.

## Pre-cutover baseline and historical scope

The pre-cutover [service publication contract](../service-publication.md) described format-140 SQLite storage. Version 1.4.5 keeps the `collections-1.4/<collection>/incarnations/<identity>` container and owned `active.json` selection manifest, while the database format is 150 and uses redb. Project workers open Kernels/stores. The service broker handles metadata, registration, grants and durable work; it does not open collection indexes or forward evidence.

| Component | Current representation | Replacement obligation |
| --- | --- | --- |
| Head and phase | Atomic SQLite rows; Ready, Prepared and Applying | One coherent committed head and recoverable intentions |
| Catalog | Height-16 authenticated map; buckets ≤ 512 KiB, values ≤ 256 KiB | Preserve canonical bytes, digest domains, membership/absence proofs and complete traversals |
| Graph | DiskANN records ≤ 16 KiB and height-32 authentication tree | Preserve physical IDs, codec and proofs |
| Lexical evidence | Path/chunk identity, citation JSON, normalized path/text terms, FTS5 physical seal | Preserve candidates, stable order, original locations and integrity failures |
| Pending publication | Captured inputs, descriptors, map/graph changes and lexical replay inputs | Authenticate replay material; never recover from live sources |
| Retained objects | Source/extraction/resource files outside the database | Preserve ownership, references and request-held object protection |
| Capacity | Durable lifetime grant for main file, capacity-derived WAL/SHM and slack | Replace engine-specific accounting without losing restart-safe admission |

Ordinary publication retains the prior committed snapshot during preparation and Applying. Service readers verify FTS shadow rows through their pinned SQL snapshot; the legacy rollback lexical cache cannot authorize WAL reads. Preparation computes a lexical seal and rolls back; Applying repeats captured lexical work and verifies that seal.

The catalog contains metadata, artifact/vector/occurrence bindings, selector postings, source measurements and retained-object handles. Committed graph reads currently need a record and 32 siblings. Separate proof cost from engine cost in benchmarks rather than assuming the engine swap removes it.

**Other SQLite users.** Inventory usage and scheduler ledgers, service registry and durable jobs/configuration/maintenance metadata, private graph overlays and vector registries before claiming dependency savings. The quota ledger uses JSON files and OS locks. Port these ledgers to redb while preserving their transactional, ownership, fencing, deduplication, usage-history and recovery contracts for new redb state. The accepted cutover resets old SQLite state rather than importing its records. This expanded scope supersedes the earlier collection-only first-release proposal.

**Historical compatibility.** Format 126 uses rollback journals and `synchronous=EXTRA`; its cache, hot-journal handling and destructive rebuild records describe the older namespace. Formats 125/126 are historical recognition/retirement scope, not the implementation baseline. Do not route normal 1.4 clients through their direct-store or deletion paths. See [architecture](../architecture.md), [storage reliability](../storage-reliability.md) and [storage ownership](../service-storage-ownership.md).

## Requirements

1. **Atomic visible publication.** Catalog, graph, lexical evidence and head become visible together. Prepare and intent transactions do not publish a new head.
2. **Captured recovery.** Prepared and Applying both retain recoverable work. Authenticate retained inputs, afterimages and deletions. Preserve existing descriptor domains where their meaning is unchanged; version changed encodings explicitly.
3. **Authenticated evidence.** Preserve catalog/graph proofs and authenticate every lexical query/update dependency. Corruption must not become an empty or shortened successful result.
4. **Lexical compatibility.** AND of shared AIRK-normalized terms, literal case-sensitive path prefix, stable-ID order, limit 1–100, `limit + 1` detection, ≤ 8,192 chunks per path, no BM25 and no duplicate source-text storage. Preserve long-term encoding semantics and citations.
5. **Request snapshots.** Pin a database snapshot and object-protection lease through publication and incarnation handoff. Preserve finite request lifetimes and the worker termination boundary. Applying must not exclude committed readers for replay's duration.
6. **Automatic admitted growth.** Establish a durable incarnation allowance before writable open, including repair/recovery, and enlarge it automatically before file growth. An initial size or operation estimate must not become a fixed file-size limit. Enforce collection/root quotas on every growth path; charge staging against its caller reservation and count overlapping redb incarnations toward quota. Report sampled logical database file sizes through `airk_status`.
7. **Fresh SQLite boundary; preserved redb rebuilds.** Format **150** stores redb state. Never open or reinterpret format-140 SQLite bytes: require fresh setup/full indexing and ownership-checked old-file cleanup. For later redb rebuilds, build and validate a sibling incarnation before atomically switching selection. Insufficient headroom preserves the existing redb index, identity, settings, usage and reader leases.
8. **Single index file per incarnation.** `current.airk` contains the collection database; objects, ownership, manifests, locks, grants and private staging remain separate owned artifacts.
9. **Platform qualification.** Target Windows, Linux and macOS, including extended paths, identity, locks, synchronization and interrupted lifecycle behavior. Record actual evidence and remaining gaps separately.
10. **Measured acceptance.** Define thresholds before comparison against retained format-140 evidence, including concurrent readers, peak disk/memory and recovery.
11. **Manual lossless compaction.** Provide **AIRK: Compact Index Storage…** and a selected-project Service Console action through the guarded native maintenance plan. Compact only the selected `current.airk` after Ready/no pending publication and reader/writer exclusion. Verify its committed head and authenticated components before and after, preserve retained external objects, and retain a fenced receipt with token and bytes before/after/reclaimed. A no-shrink outcome is valid; missing data, contention and interrupted outcomes must remain explicit.

Out of scope: historical snapshot APIs, multiple writers, agent orchestration, external databases/cloud services, DiskANN algorithm changes. Retire legacy SQL code and tests that retain the dependency as part of this cutover, preserving unrelated historical code and dated documentation.

## Selected engine

redb is selected for its pure-Rust transactional B+tree, MVCC, typed tables, single database file and public `StorageBackend`. Pin an exact reviewed version; 4.4.0 is the candidate evaluated here. Verify API, format compatibility, license and minimum Rust version against the repository before adding it.

| Alternative | Assessment for this experiment |
| --- | --- |
| fjall | LSM/segment layout adds compaction and multi-file admission work; reconsider only with supporting measurements |
| sled | Requires separate maturity and format-compatibility qualification |
| Turso | Retains SQL and requires independent FTS/ownership/durability qualification; no automatic fallback |
| Hand-written pager | Adds allocator, crash recovery and commit-protocol obligations; outside this proposal |
| LMDB/RocksDB bindings | Do not meet the pure-Rust collection-engine objective |

The [redb design](https://github.com/cberner/redb/blob/v4.4.0/docs/design.md) describes checksum-based single-sync and two-phase durable commit strategies. Do not reduce the durability argument to writing data and then a header. Phase 1 must identify the pinned implementation's actual commit strategy and supported configuration, record the rationale and qualify that exact path. Use `Durability::Immediate` for durable phase transitions. Non-cryptographic engine checksums do not replace AIRK's authenticated roots.

Primary API references: [StorageBackend](https://docs.rs/redb/4.4.0/redb/trait.StorageBackend.html), [Builder](https://docs.rs/redb/4.4.0/redb/struct.Builder.html), [Database](https://docs.rs/redb/4.4.0/redb/struct.Database.html) and [Durability](https://docs.rs/redb/4.4.0/redb/enum.Durability.html). Engine documentation is a qualification input, not demonstrated AIRK behavior.

## Proposed storage design

The developing adapter's concrete lexical codecs, scoped ordered-tree refinement,
numerical work limits and replay boundaries are recorded in
[format-150 lexical implementation notes](lexical-format.md). These mechanism
choices still require the performance and production-integration gates below.

### Ownership and adapter boundary

Implement `current/kv/{backend,db,tables,txn}.rs` inside `airk-store`. Store owns redb, filesystem access, admission and validation; kernel keeps calling store contracts. Core and retrieval gain no redb dependency. Any later crate extraction must justify an independent purpose and update the architecture dependency table.

One project worker manages a shared database handle per open incarnation. Request-held ownership retains the engine, snapshot and AIRK protection gates until materialization finishes; redb transactions also retain their underlying database independently of the outer handle. Replacement can retain old/new handles simultaneously. The metadata-only broker never owns collection I/O. Normal diagnostics use the worker and pinned snapshots; offline read-only opens require verified worker exclusion, not an interim concurrent-reader architecture.

`current.airk` replaces main/WAL/SHM only inside the new incarnation. Keep ownership/bootstrap receipts, object/resource directories, writer locks, selection gates and object leases. Private graph overlays/vector registries may use reserved `.airk` staging files with the existing coalescing, seal and cleanup receipts. Old SQLite files are removed only through the ownership-checked fresh-setup reset. Later redb incarnations retire after their retained requests drain.

### Vector and semantic storage

DiskANN remains the semantic search engine. In the redb incarnation,
`graph_record` stores each physical node's exact existing binary payload,
including its floating-point vector and neighbor links, alongside its embedding
key and live marker. `graph_node` stores the existing height-32 authentication
tree. The authenticated catalog binds embedding keys to graph IDs, path/chunk
occurrences and retained evidence. Graph, catalog and lexical changes publish
under one coherent head and captured recovery intention; redb does not compute
embeddings or replace the DiskANN algorithm.

The collection-local vector cache remains separate immutable pack files. These
are reusable embedding copies, distinct from the vectors held in live graph
records. Packs retain their checksums, dimensions, pipeline identity, bounded
indexes and exact input keys. Cache manifests and semantic policy/work records
remain immutable current resources whose references are authenticated by the
catalog. Source/extraction objects remain separate retained files, protected by
request leases and ownership-validated cleanup. Rebuild reuse requires verified
model/pipeline/input identity; a matching filename or branch name is insufficient.

Production collection routing, private graph/vector staging and coherent
whole-publication recovery now use these adapters. Store and native-worker
regressions cover captured replay, graph replacement, retained snapshots and
sibling handoff. The placement is implemented; comparative performance and
final local artifact qualification remain pending in the
[current release record](../release-1.4.5.md).

### Keyspaces and authentication

This is a logical schema. Freeze canonical encodings, digest domains and all size/count limits before the port. Existing catalog/graph constants stay unchanged; lexical bounds below are proposed.

| Table | Key | Value and authentication |
| --- | --- | --- |
| `meta` | `u8` tag | Head ≤ 64 KiB, phase, lexical manifest ≤ 1 MiB, format marker and monotonic path allocator |
| `map_bucket` / `map_node` | Existing prefix/packed height-index | Existing catalog encodings, limits and digests; canonical-empty nodes absent |
| `graph_record` / `graph_node` | Physical ID/packed height-index | Existing DiskANN codec and graph digests |
| `lex_term` / `lex_node` | Height-20 keyed-map prefix/node | Term digest → bounded header with posting-directory root and counts; new lexical digest domains |
| `lex_directory` | Term digest and page ID | Authenticated directory pages ≤ 64 KiB committing child keys, ranges, counts and digests |
| `lex_block` | Term digest and first `u64` doc key | Posting block ≤ 64 KiB and ≤ 4,096 sorted doc keys, delta/varint encoded; authenticated directory binds its range |
| `lex_document` | `u64` doc key | Authenticated path/chunk/stable ID/citation, or verified references to those retained artifact fields |
| `lex_path` | Path bytes | Authenticated ordered-directory entry: path number, chunk count, document binding and forward root |
| `lex_forward` | Path number and page ID | Authenticated sorted distinct term-digest pages ≤ 64 KiB, with counts and range commitments |
| `pending_*` | Transaction ID and original key | Canonical put/delete operation and afterimage; descriptor commits table IDs, keys, counts and digests |

The lexical manifest becomes `{version: 3, engine: "airk-kv/lexical-1", term_root, path_root, document_root, updates}`. `head.lexical_commit` authenticates the canonical manifest, not only the term root. The path root commits an ordered directory with child key ranges, counts and digests, proving prefix boundaries and completeness. Forward roots are committed by path entries. A plain redb range scan is not a completeness proof.

A doc key is `(u64::from(path_number) << 16) | chunk_index`; validate conversions/ranges. Allocate `u32` path numbers transactionally and monotonically, retain the number for replacement of an existing path, and never reuse a retired number in an incarnation. Exhaustion requires admitted replacement/rebuild. Stable IDs retain `framed("current-lexical-document/1", path, chunk)` with the existing canonical chunk encoding.

Authenticate path, chunk index, stable ID, content hash, line range and original document location. If citations come from retained artifacts, verify their artifact-root binding through the same request snapshot. Missing documents/paths, changed citations and invalid chunk counts fail integrity checks.

The 8,192-chunk limit does not bound distinct terms. Forward lists need pages, not a fixed 8,192-term array. Posting headers point to paged directories rather than arbitrarily large block-digest lists. Phase 0 must freeze total per-path/per-publication term, page and byte limits against admitted extraction limits and measured fixtures. Exceeding them fails explicitly while preserving the old head; unresolved limits block implementation.

Version deterministic block split/merge rules: split overflow at the median doc key; merge adjacent underfilled blocks only when combined bytes/count fit, processing pairs in key order. Freeze the underfill threshold in phase 0. First-key changes update directory keys atomically. Proofs cover range gaps so missing blocks cannot silently shorten results. Validate forward counts, ordering and page digests before replacement; missing terms/pages cannot silently leave stale postings.

Keep catalog `MapPlan` bytes and graph proofs unchanged. Any generic map adapter must separate lexical height/digest domains from catalog semantics. Validate packed node keys and canonical-empty handling at every height.

### Queries and work budgets

Normalize with the shared core implementation and preserve long-term identity mapping. Prove term membership/absence, traverse authenticated posting directories, stream intersections starting with the shortest list, and validate candidate document/path bindings. Apply the literal prefix to authenticated paths. A prefix-first optimization may use the ordered path directory only with complete range proofs.

After exhaustive traversal, use a bounded heap for the smallest `limit + 1` stable IDs, return the first `limit`, and retain the existing `incomplete` meaning that another matching candidate exists. Doc-key order differs from stable-ID order: early doc-key truncation cannot claim the exact first results.

Bound query terms, pages/blocks read, encoded bytes, decoded doc keys, candidate checks, prefix entries, memory and elapsed time. The proposed 1,048,576 decoded-key ceiling is only one bound. Freeze remaining numerical limits and cancellation checkpoints in phase 0; check before allocation/decode. Budget/deadline exhaustion returns an explicit typed resource/deadline failure through the existing response contract, not an arbitrary partial set labeled exact or an empty result. A future partial mode requires separate coverage/ordering schema review. Keep corruption distinct. Test common terms, disjoint intersections and selective prefixes that exhaust work before yielding matches.

### Transactions and recovery

Compute lexical changes and predicted roots once, then write pending afterimages and install them into live tables. Both stages write bytes; CoW can amplify them. Preserve retained-object references until durable head/pending cleanup permits reclamation.

| Transition | Durable action (`Durability::Immediate`) | Readers and restart |
| --- | --- | --- |
| Ready → Prepared | Verify base head; atomically write complete pending operations, authenticated descriptor and retained-input bindings | Old head remains readable; restart validates/resumes intention |
| Prepared → Applying | Verify transaction identity, base/target, pending counts/digests and durable capacity; persist Applying separately | Old evidence remains readable; restart resumes apply |
| Applying → Ready | Atomically install operations, verify resulting roots against target, write head/Ready and remove pending rows | Complete old/new snapshots; restart sees replayable Applying or completed Ready |

The Applying marker is a third durable transaction boundary. Prepared is not automatically aborted on restart. Explicit cancellation/abort follows existing authorized state rules and preserves the base/accounting; Applying settles through recovery. External grant/object cleanup needs idempotent receipts because it cannot commit atomically with database state.

Replay checks scope/incarnation, descriptor identity, base head, input digests, counts, table/key domains and afterimage/tombstone digests. Recompute roots from installed content and verified paths, not trusted target-root rows. An absent deleted key is idempotent only under the same validated transaction/base state. A matching Ready target proves completed replay; a conflicting head fails explicitly.

Separate known aborts from uncertain commit outcomes. After write/sync errors, stop using the handle when required, reopen under ownership and reconcile phase/head/transaction identity before retrying or releasing grants. A returned commit error alone does not prove rollback. Recovery uses retained inputs only.

Ordinary publication retains the shared request/object lease. Exclusive object/selection gates remain for reclamation, bounded handoff and maintenance requiring reader exclusion. Track database/object lifetimes together; async cancellation does not prove native I/O has stopped.

### Capacity and backend enforcement

Establish one durable lifetime allowance per incarnation for its admitted file ceiling and defined allocation slack before initialization, writable open or repair. Validate scope/incarnation, file identity, aliases/reparse points and length before engine access; check the internal format marker after safe opening. Opening must not silently create or repair unowned state.

`set_len` and `write(offset, data)` check overflow and synchronously request admission whenever the requested file end exceeds the live high-water. For `current.airk`, the owner first durably enlarges the lifetime quota grant and owner header; only then does the backend permit the extension. Reservation-owned private redb staging charges actual growth against the caller's reservation, with bounded granules clipped to its remaining budget. Keep exact offset I/O, handle short operations, propagate permanent failures and satisfy redb's zero-initialization contract. A later allocation failure can follow earlier transaction writes: the guarantee is bounded, quota-admitted growth and recoverable committed state, not failure before any page is written.

Replace the inherited `capacity − len + estimate` formula with a measured admission model covering pending/live copies, CoW paths, page/region granularity, fragmentation, allocator metadata, pinned reader pages and repair/compaction. Freeze it after phase 1. Estimates guide admission/batching; the backend enforces the ceiling. Do not reserve actual bytes again when already covered by the lifetime allowance; reconcile measurements/outstanding grants under the existing ledger contract.

Growth durably reserves the larger allowance before publishing owner capacity and updating the live backend ceiling in place; it does not require normal growth to close readers or reopen the database. Reconcile interrupted grant/owner/backend transitions. Recovery/cleanup need admitted headroom: a full file must not strand Applying without a valid recovery path. Staging and siblings have separate reservations alongside active capacity. Quotas remain logical admission policy, not physical filesystem quotas; qualify allocation slack and sparse files.

Compaction is admitted explicit maintenance with no live snapshots on that handle. Qualify temporary peak, sync and restart behavior before enabling it; defer rather than drop a live request's lease. Free-page reuse does not imply immediate shrinkage.

Implement real OS locks: redb's default unsupported backend lock methods are insufficient ownership evidence. Test competing-process exclusion and release on every target platform. Qualify Windows extended paths, sharing violations and flush/write-through semantics against the pinned backend; a method named `sync_data` is not platform evidence. Offline diagnostics preserve read-only behavior.

Cache budgets belong to worker/incarnation admission. A 64 MiB active-handle cache is a measurement candidate; multiple projects and replacement handles must fit machine policy. Bound overlays, decoding buffers and snapshots separately.

## Current local qualification

October 26 final local checkpoint: the complete source cutover and guard-preserving performance fixes are implemented. The matched official 1.4.4/experimental 1.4.5 Windows release comparison passes all seven unchanged <=10% p95 gates: common/rare/prefix query ratios are 0.989/1.010/1.006, 32-file replacement 0.846, deletion 0.882, vector queries 0.862 and semantic replacement 0.916. Peak sampled simultaneous native RSS increases from 92,590,080 to 133,255,168 bytes (+43.9%); peak sampled logical data increases from 7,524,678 to 14,855,128 bytes (+97.4%). Executable size is essentially unchanged (+0.24%). This is a small end-to-end synthetic workload, not an isolated engine benchmark. Exact evidence/restart identity passes. The latest debug comparison (5) still fails deletion (1.353) and semantic replacement (1.163); failed debug reports remain retained and are not reclassified as passing.

Windows unfiltered workspace tests pass 1,206 with 16 ignores after a documented disk-exhaustion compilation failure and full retry; all-feature/default-feature doctests, Clippy and formatting pass. Three ignored real-model component tests pass separately. Linux container qualification passes 1,222 tests with 17 skips, default doctests and runtime smoke. The actual owned staged official 1.4.4-to-1.4.5 upgrade, native model/client/lifecycle and five extension native scenarios pass. See [exact runtime identities, measurements and limitations](../release-1.4.5.md#final-release-comparison-and-recommendation). No native archive or installable VSIX is packaged; installed-editor, native macOS/signing, oldest-OS and hardware power-loss qualification remain unavailable. The completed service tip `66c2a2a` and main signing fixes through `217f226` are integrated locally.

**Final owner decision, October 26, 2026: abandon the redb cutover and retain SQLite.** The measured 8–15% publication/vector improvements and essentially unchanged lexical latency do not justify the larger memory/storage footprint, custom authenticated lexical implementation and forced fresh setup/full indexing. SQLite removal and removal of WAL/SHM files are real structural changes; snapshots, quota admission and compaction are not unique benefits of this engine. Preserve the implementation and evidence; the owner authorized documentation, commit and branch push only. No further packaging, implementation, revert, deletion or merge is authorized by that instruction. See the [decision record](../redb-experiment.md).

## Implementation proposal

Use the gated phases below. Capture SQLite baseline outputs as bounded original fixture/golden results during development, then remove the executable SQLite oracle and all SQLite dependencies before packaging. No feature, test-only dependency, build dependency, executable SQL oracle, migration/legacy reader, native payload or fallback may retain SQLite code. Preserve historical documentation as identifiable records. Do not build any VSIX until the entire branch cutover is complete, even for an intermediate local test.

**Phase 0: baseline and frozen contracts.** Benchmark format 140: committed graph reads, cold/warm lexical queries, 32-file/full publication, and concurrent readers during preparation/replay. Include common/rare terms, selective prefixes, repeated replacements/deletes and long-lived readers. Record p50/p95/p99 latency, throughput, peak logical/reserved/allocated disk, RSS/cache, recovery time and both runtime executable sizes. Retain fixture/source revision, hardware/filesystem, cache conditions, repetitions and variability. Set acceptance thresholds before comparison; use the accepted ceiling of 10% regression in p95 query latency and publication duration, with repeated runs and recorded variability; evaluate disk and memory limits separately. Freeze lexical codecs/proofs, all numerical budgets, format-150 compatibility and dependency inventory. Gate: reproducible baseline and reviewed contracts; unresolved limits block the port.

**Phase 1: backend qualification spike.** Build the adapter inside `airk-store`, including ownership, offset I/O, locking, ceiling enforcement, shared handle lifetime, error mapping and durable transitions. Identify the exact redb commit strategy. Exercise process death and deterministic short/torn/dropped/reordered writes, sync errors, disk exhaustion, overflow, resize refusal, repair/open failure and compaction interruption. Test below/at/above ceiling boundaries, a 4 GiB sparse-file case and repeated writes with a pinned reader. Reopen after faults and require valid old/new committed state or explicit integrity/uncertain-outcome failure; never successful torn evidence. Verify restart-persistent grants and capacity-bounded recovery. Gate: retained Linux/Windows crash/capacity evidence and native macOS evidence before three-platform release claims. A local Windows test package may record the other platforms as unqualified; it still requires the complete SQLite-free cutover. Simulation does not qualify hardware power loss. Reject/redesign if ownership or full-capacity recovery cannot be bounded.

**Phase 2: catalog, graph and journal.** Add transactional table access behind current contracts; preserve catalog/graph bytes and roots. Port overlays/vector registries under existing staging reservations. Implement all three durable transitions, authenticated afterimages and tombstones. Isolate lexical stubs in tests until phase 3. Run current/database/graph/foundation and service snapshot suites on both engines; add generated insert/update/delete/no-op/restart sequences and concurrent snapshots. Gate: differential roots, recovery correctness and no Applying read exclusion.

**Phase 3: lexical index.** Implement authenticated directories, blocks, documents, ordered paths and paged forward lists. Compare FTS5 candidates, citations, stable order and `limit + 1`, including Unicode, underscores, CamelCase, terms at/above 8,000 bytes, >8,192 distinct terms per path, splits/merges and deletion/reinsertion. Corrupt directories, blocks, documents, paths, forward pages, counts/ranges, citations and allocator state. Test every work budget and explicit failure. Intentional tokenization changes require separately reviewed contracts/fixtures; do not silently call differential failures bug fixes. Gate: completed-query equivalence, authenticated completeness and bounded failures.

**Phase 4: service integration and replacement.** Require fresh setup/full indexing for old SQLite state and ownership-checked removal under the accepted cutover decision above. Within format 150, integrate sibling build, validation, atomic selection and bounded retirement; preserve redb readers, settings, usage, completion receipts and insufficient-headroom behavior. Reuse embeddings only where identity/ownership validation supports it; measure inference/rebuild cost. Test process death before/after handoff, stale tokens, uncertain acknowledgments, grant/owner reconciliation, concurrent queries and live-lease retirement. Capability strings describe only implemented behavior. Repeat phase-0 measurements and generated differential/recovery campaigns through the service. Gate: full applicable Rust/repository checks, quota/fault campaigns, native platform evidence and Windows VSIX acceptance. Retain captured baseline evidence for comparison; remove the executable SQLite oracle before packaging and record gaps honestly.

**Phase 5: all ledgers and dependency removal.** Port usage aggregates/deduplication, scheduler intents/watermarks, service registrations, jobs, configuration results, watcher state, lifecycle/cleanup receipts and provider drains to redb typed tables. Preserve atomic request receipts, fencing epochs, pagination identity, source/data isolation, bounded retention and restart recovery. Implement and test the explicit fresh-setup/reset path for existing SQLite registrations, settings, usage and pending work. Remove only proven old AIRK database files/sidecars under exclusion; do not import or read them. Validate fresh registration and full indexing, interrupted/locked cleanup and stale agent bindings. Later redb rebuilds retain their readable collection until the sibling is validated. Remove legacy SQL adapters and replace SQL-specific tests with redb integrity/fault tests and frozen differential fixtures. Remove rusqlite/libsqlite3 dependencies from every manifest and lockfile, including CLI/kernel development dependencies. Gate: no SQLite packages in the complete Cargo metadata graph, native payload closure or build/test graph; no compiled SQL fallback.

**Phase 6: v1.4.5 local package and release gates.** Synchronize Rust workspace/local lock entries and extension/npm root versions to 1.4.5 once. Update architecture, documentation index, runtime/publication/ownership/quota guidance, README, release notes, third-party notices and VSIX/Marketplace descriptions to agree with actual behavior. Run formatting, Clippy, workspace/doc tests, repository/whitespace checks, extension checks and the local native-pair/model/archive qualification. For this branch, fetch and merge the completed `origin/feat/airksvc` changes again before packaging, preserving the redb cutover and rerunning affected checks. The October 26 owner instruction requires explicit approval before any VSIX build; that approval has now been given. After the full redb cutover/dependency-removal and qualification gates pass, build the Windows VSIX for local user testing. Do not install into the normal editor profile or publish without separate authorization. Record exact artifact paths/hashes and all unavailable qualification. Supported-platform native qualification remains a public-release gate.

No line-count or binary-saving estimate is an acceptance criterion. Size the work after proof/schema review and the backend spike. Implementation checks follow [CONTRIBUTING.md](../../CONTRIBUTING.md): formatting, Clippy, workspace tests, repository checks and whitespace checks; report unavailable checks honestly.

## Risks and decisions

| Risk | Consequence | Required evidence or decision |
| --- | --- | --- |
| CoW amplification | Large buckets and pending/live copies increase cost | Measure dirty bytes/pages; tune new lexical layouts without altering catalog proof bytes |
| Hot terms and directory churn | Replacements can touch many blocks/paths | Measure splits/merges; enforce publication term/page/byte budgets |
| Incomplete authentication | Altered paths/citations/forward pages hide or fabricate evidence | Prove all query/update dependencies and range completeness; test shortened structures |
| Reader-pinned space | Snapshots prevent page reuse | Finite lifetimes, pressure reporting and full-capacity recovery tests |
| Commit/sync ambiguity | Error alone may not identify durable outcome | Reopen/reconcile before retry or grant release |
| Replacement overlap | Old database/objects coexist with candidate | Preserve sibling admission and old reads; no destructive fallback |
| Repair/compaction work | Implicit work can exceed admission or block readers | Admit writable opens; qualify repair policy and exclusive maintenance |
| Hidden SQLite dependency | A test/build/legacy adapter could keep shipping SQLite | Audit full metadata graph, all manifests/lockfiles and payload closure before any VSIX build |
| Platform durability | Process-kill tests do not qualify power loss | Record sync semantics, fault-model limits and native receipts separately |
| redb drift | Engine upgrade may require rebuild | Pin exact version, validate internal/owner formats and preserve rebuild-only handling |

Resolve before the port: complete lexical codecs/proofs and numerical budgets, format-150 allocation, pinned commit strategy and benchmark thresholds. Freeze the measured admission model after phase 1. Existing decisions remain: project workers own collection I/O, reads stay available during Applying, and rebuilds preserve old incarnations until validated handoff. If redb fails qualification, preserve existing user state and fix or report the blocker. Do not package a partial cutover or substitute a SQLite fallback; a hand-written pager remains out of scope.
