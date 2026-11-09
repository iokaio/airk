# Architecture v2 publication and lifecycle baseline

Phase 0 inventory, 14 October 2026. Prerequisite: snapshot schema **3**, extractor **`native-facts/7`**, compact storage **1**, root SQLite `user_version` **2**. This records existing code and integration obligations; it does not qualify v2 behavior. The [implementation guide](architecture-v2.md) owns the requirements and gate definitions. Source references name functions as well as files so they remain useful after line movement.

## Publishers and mutation boundaries

| Entry point | Existing publication behavior | Required finalizer integration |
|---|---|---|
| `Kernel::initialize` in [kernel/lib.rs](../crates/airk-kernel/src/lib.rs) | Writer lock; creates empty compact snapshot only when absent; configured model; reserved publication | Establish incarnation only with successful initial commit; return exact empty-revision token |
| `index`, `reindex`, `index_with_mode` in [kernel/lib.rs](../crates/airk-kernel/src/lib.rs) | Streaming scan, retained artifacts and configured roots/model; scanner owns normal generation decisions; full rebuild clears prior extractor version; model/storage changes can increment generation; unchanged scan verifies persistent components without publishing | Assign sequence/parent centrally; preserve no-op behavior and force-extraction distinction; compute stamps/events from observations, including profile/root/model changes and omissions |
| `import_scip` in [kernel/lib.rs](../crates/airk-kernel/src/lib.rs) | Validates source/build binding and extractor; rejects aliases; increments generation; retains artifacts; publishes imported facts | Preserve producer validity and failure atomicity; finalizer owns generation, provider-domain event and lineage |
| `run_provider` in [kernel/analysis.rs](../crates/airk-kernel/src/analysis.rs) | Both LSP and Ctags; verifies inventory before/after provider; charges temporary allowance; applies analysis; increments generation | Same finalizer for both adapters; output binding must survive any publication retry; never rerun external producer automatically |
| `extract_ocr` in [kernel/analysis.rs](../crates/airk-kernel/src/analysis.rs) | Requires applied roots/current extractor; rereads selected bytes; bounds source/corpus; retains artifact; clears imported domain; increments generation | Distinguish equal source bytes from changed OCR evidence/options; preserve locations and provider invalidation; centrally assign stamps |
| Effective roots/model/limits changes | [roots.rs](../crates/airk-kernel/src/roots.rs), [limits.rs](../crates/airk-kernel/src/limits.rs) save pending configuration; `configured_model` is applied during publication | Persist effective decisions in committed inventory; extraction policy, lineage compaction and lineage purge must use finalizer when they change evidence/coverage |

All current production evidence publishers reach `Kernel::publish`, which streams retrieval construction and calls `FileStore::publish_catalog`. It is already a common component builder, but is **not yet** the v2 finalizer: generation is assigned by callers, and incarnation, stamps, lineage, publication receipts and postcommit maintenance are absent.

[CLI main](../crates/airk-cli/src/main.rs) routes init/setup/index/reindex, analysis imports/providers and OCR into these services. [Kernel scheduler](../crates/airk-kernel/src/scheduler.rs) dispatches index/reindex rather than writing an alternate pointer. Usage policy setup can call `initialize` through [kernel/usage.rs](../crates/airk-kernel/src/usage.rs); initial publication therefore is not exclusive to the explicit `init` command.

Store-level `SnapshotStore::save`, `FileStore::save_with_components`, `prepare_catalog` and `publish_catalog` in [store/lib.rs](../crates/airk-store/src/lib.rs) remain public compatibility/test primitives. `MemoryStore` is a snapshot adapter. They are not additional production kernel publishers, but their compatibility checks must prevent a legacy or direct writer from overwriting v2 lineage. Do not mistake private catalog preparation for pointer commit.

Other durable writes are configuration, coverage caches, usage ledgers and scheduler intents. They do not currently publish evidence. Preserve their quota/lifecycle participation; policy/checkpoint mutations need separate durable receipts rather than silently advancing file evidence stamps.

## Persistence, migration and readers

| Boundary | Existing implementation | V2 obligation |
|---|---|---|
| Revision digest | `revision_id` in [retrieval/persistent.rs](https://github.com/iokaio/airk/blob/f9aa5168829dc8794d241644fb8483715c2dd58a/crates/airk-retrieval/src/persistent.rs) hashes engine plus serialized snapshot; inventory verifies original persisted bytes in [store/blobs.rs](../crates/airk-store/src/blobs.rs) | Preserve legacy encoded bytes when adding default fields; never reserialize an old value to discover its old directory. Freeze noncircular inventory/lineage/catalog/manifest digest order |
| Private build | `prepare_streaming` writes charged temporary retrieval/inventory/manifest files, invokes private catalog callback, syncs and reopens components, creates external canonical lease and renames generation | Include lineage and all new dependencies; verify before pointer commit; distinguish reusable component inputs from publication identity |
| Pointer commit | `publish_catalog`/`publish_pointer` in [store/sqlite.rs](../crates/airk-store/src/sqlite.rs) admit main/WAL/SHM growth then transact small snapshot/header in root SQLite | Atomically bind incarnation, full revision, inventory digest and final manifest digest in retained receipts as well as active pointer; account receipts and journals |
| Legacy migration | `FileStore::load`, `catalog`, SQLite readers and [kernel streaming tests](../crates/airk-kernel/tests/integration/streaming.rs) support legacy snapshot to compact objects | Private quota-bounded migration; first v2 publication establishes unavailable earlier coverage; preserve roots, original locations and valid imported domains; fail closed for old writer versions without deleting old pointer |
| Retrieval cache | `pin_revision` and cached `PersistentIndex` in [kernel/lib.rs](../crates/airk-kernel/src/lib.rs) retry header/lease races and verify identity | Cache verified catalog too; exact token lookup and bounded historical handles; release superseded idle pins before cleanup |
| Catalog/fact reads | `FileStore::catalog` in [blobs.rs](../crates/airk-store/src/blobs.rs) pins lifecycle/revision; [sqlite.rs](../crates/airk-store/src/sqlite.rs) has pinned connections and coherent read transactions | Warm indexed file/change lookup without inventory deserialization; no cache mutex over disk/parsing; historical selector verifies retained receipt even after ancestry expires |

The current root SQLite compatibility version and snapshot schema are different contracts. Incrementing only the latter is not sufficient mixed-worker protection. Freeze independent inventory, lineage, catalog, profile, mapping, chunking, continuity and retrieval versions at the phase 1 gate; this inventory does not allocate new numbers.

## Lifecycle and ownership

| Boundary | Current safety mechanism and source | Extension required |
|---|---|---|
| Root lifecycle | [quota.rs](../crates/airk-store/src/quota.rs) `lifecycle_lease`/`relocation_lifecycle_lease`; retired/pending root markers reject entry | Preserve lifecycle-before-writer order across every new maintenance/history/cache path |
| Collection writer | [store/lib.rs](../crates/airk-store/src/lib.rs) `FileStore::lock`: external OS lock plus compatibility marker, symlink checks, lifecycle held with lock | One finalizer under writer exclusion; legacy markers remain fail closed; no waiting for writer while quota transaction held |
| Canonical reader lease | [blobs.rs](../crates/airk-store/src/blobs.rs) `reader_lease_path`; external `.reader-locks/<collection>/<revision>` file; [retention.rs](../crates/airk-store/src/retention.rs) try-exclusive lease | History/full representations share canonical lease; choose representation only after pin; show protected pins |
| Logical quotas | [quota.rs](../crates/airk-store/src/quota.rs) reserves root/collection bytes, charges before writes, caps private SQLite, counts retained and staging bytes; OS leases recover abandoned reservations | Count history, receipts, intents, caches and models; typed exhaustion permits one automatic-mode cleanup/rebuild retry after all reservation clones/staging close |
| Physical capacity | [capacity.rs](../crates/airk-store/src/capacity.rs) supplies separate point-in-time free-space observations | Preserve distinction from logical quota and actual physical-write qualification |
| Prune | `Kernel::prune_indexes` holds writer through active lookup; [retention.rs](../crates/airk-store/src/retention.rs) protects active/pinned generations, renames to `removing-*`, then deletes | Add validated durable intents and bounded postcommit work; arbitrary interrupted names cannot be automatically removed; cleanup errors after commit are maintenance results |
| Object GC | `prune_objects` verifies surviving inventories before marking source/artifact hashes; dry run models candidate removal | Mark every full/history/checkpoint/leased inventory plus explicit dependency manifests; stop deletion on unverifiable root; lineage hash mentions do not retain content |
| Collection removal | [store/lib.rs](../crates/airk-store/src/lib.rs) validates managed path/identity, takes scheduler worker and writer/usage guards, acquires all generation leases before deletion; kernel releases own cache | Include history/intents/receipts/models and all canonical leases; no source writes or cross-collection cleanup |
| Relocation | [relocation.rs](../crates/airk-store/src/relocation.rs) freezes root, scheduler, writers and readers; validates source-root boundaries; charges peak copy; digests files; recoverable prepare/copy/activation markers; old state remains | Preserve incarnation and receipts; enumerate all new roots and owned intents; no copied live OS lock; expired ancestry must not break surviving checkpoint verification |
| Scheduler/usage | [store/scheduler.rs](../crates/airk-store/src/scheduler.rs), [store/usage.rs](../crates/airk-store/src/usage.rs) use lifecycle/quota admission and bounded SQLite | Keep failed/blocked job visibility; ordinary reads and UI repaint never trigger maintenance |

Current prune is manual and has no history/checkpoints or automatic intent recovery. Existing collections must remain manual during upgrade. New automatic defaults may activate only after phase 4 qualification. Release obsolete build/cache handles before try-exclusive cleanup; never wait for readers under a quota ledger transaction. A retention target never revokes a live lease or checkpoint.

## Existing fixtures and missing evidence

These are source inventories, not fresh execution receipts. Fixtures use original fictional sources and temporary state.

| Existing fixture source | Assertions already present |
|---|---|
| [store/sqlite.rs tests](../crates/airk-store/src/sqlite.rs) | WAL reader coherence; failed transaction retains prior metadata/facts/revision; compact catalog admission; pointer WAL checkpoint growth allowance; legacy migration with no corruption fallback |
| [store/quota.rs tests](../crates/airk-store/src/quota.rs) | Shared root/collection limits; concurrent charged writes; sparse/escape rejection before source touch; private SQLite page cap; process exit releases reservations and only owned staging; bounded busy-ledger admission; lifecycle contention |
| [store/retention.rs tests](../crates/airk-store/src/retention.rs) | Active/leased generation protection, preview/apply and collection lease exclusion |
| [store/relocation.rs tests](../crates/airk-store/src/relocation.rs) | Source preservation; newer destination state survives recovery; active reservation/reader rejection; insufficient peak headroom; process exit after preparation and recovery |
| [kernel/streaming.rs](../crates/airk-kernel/tests/integration/streaming.rs) | Compact catalogs, lazy retained payload reads/corruption, object deduplication, old catalog leases, path/alias digest binding, quota failure preserves publication, orphan GC, legacy citations |
| [kernel/persistent.rs](../crates/airk-kernel/tests/integration/persistent.rs) | Lexical/DiskANN identity; invalid model failure; model change and old reader lease; missing persistent generation/model errors |
| [kernel/workspace.rs](../crates/airk-kernel/tests/integration/workspace.rs) | No-op/edit/delete/reopen/cache refresh; full reindex failure; root/schema/writer isolation; removal/recreation cache identity; Windows long paths |
| [kernel/roots.rs](../crates/airk-kernel/tests/integration/roots.rs), [limits.rs](../crates/airk-kernel/tests/integration/limits.rs) | Alias origin binding, pending root protection, pass-budget failure, OCR/provider admission rejects before launch |
| [kernel/code_graph.rs](../crates/airk-kernel/tests/integration/code_graph.rs) | SCIP source/build invalidation and malformed producer/range binding preserves revision |
| [kernel/scheduler.rs](../crates/airk-kernel/tests/integration/scheduler.rs) | Durable queue, paused/manual behavior, fairness, crashed worker locks, reconciliation and root rebinding rejection |
| [check-storage-lifecycle.py](../scripts/check-storage-lifecycle.py) | Public CLI aliases, retained origin until index, quota rollback, prune, relocation and source-safe removal |
| [check-publication-faults.py](../scripts/check-publication-faults.py) | Timed process kills leave old/new coherent publication and recover; optional isolated Linux tmpfs pre-admission exhaustion |

The timed process-kill campaign explicitly does not prove every internal transition, physical power loss, mid-publication ENOSPC or device-cache loss. Relocation's after-prepare crash test is not a complete conversion/cleanup fault matrix.

Required additions at the owning gates:

- Phase 2: exact tokens, original-byte legacy digest vectors, old/new binary rejection, private migration exhaustion, finalizer no-op/failure checks for **every** writer, provider/OCR equal-byte evidence changes, pointer/receipt atomicity and reopening.
- Phase 3: foreign/recreated/forked tokens, equal sequence with different digest, observation gaps and overflow, edit/revert/transient history, conditional validators and page expiry.
- Phase 4: two readers plus writer, superseded cache pins, 200 successful small-edit plateau, single typed quota retry, cleanup failure after committed publication, and deterministic crash injection around each intent/rename/delete/commit boundary.
- H1/H2/O1b: full-to-history transitions, checkpoint lease/admission and receipt longevity after ancestry expiry, historical-only dependency GC, shared-model roots, purge/privacy reporting and relocation/removal under retained history/intents.
- Q1: native Windows/macOS/Linux lock, filesystem and package runs; network filesystems, physical full-disk races and power-loss behavior remain separately unqualified.

## Phase 0 handoff receipt

Changed file: this inventory only. No runtime API/schema/profile/dependency changes; no new fixture or license obligation. Existing user changes in the plan and documentation index were preserved. Inspected required repository guidance, current publishers, store/retrieval lifecycle implementations and the listed fixture sources. Source searches confirmed schema 3 and `native-facts/7` and the production publication convergence above.

This worker checked all local document links and trailing whitespace. No cargo tests or platform campaign was run by this inventory worker; the integrator owns repository-wide checks to avoid concurrent builds. The integrator reported a passing local workspace build, formatting, clippy and workspace tests, including 28 store unit tests, 3 scheduler-storage tests, 10 store usage tests and all kernel tests. The reported Python packaging suite passed 39 tests with one Linux-only skip; the 430-entry license bundle passed with four existing missing-license-text warnings. The execution ledger owns final gate receipts, including repository and whitespace checks from [CONTRIBUTING.md](../CONTRIBUTING.md). These local results do not establish other native-platform qualification.

Next action: integrator freezes the phase 1 identity/digest/receipt and lock contracts, then assigns one owner for store migration and one owner for finalizer integration. Existing architecture prose describes live LSP/Ctags as future work while [kernel/analysis.rs](../crates/airk-kernel/src/analysis.rs) already exposes them; update public capability wording only with the corresponding integration/qualification evidence.
