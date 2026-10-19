# Current-index storage foundation

This describes S7 of the [indexing optimization plan](indexingenhancements.md). S7 qualifies the store and DiskANN provider in isolation. The normal indexing and retrieval routes still use their existing publication path until S8 replaces it. These APIs do not read or convert an older index.

## Ownership and data

`airk-core::graph_storage` defines bounded record/proof contracts. `airk-retrieval::vector::mutable` runs the pinned DiskANN insertion and search algorithms. `airk-store::current` owns persistent records, authentication, gates, capacity admission and replay. SQLite is the local record provider and transaction boundary; it does not perform vector similarity search. DiskANN remains the vector engine.

The authoritative `index.sqlite3` has one current head, graph records, graph authentication nodes and at most one pending transaction. A head binds the collection, original source root, fresh incarnation, commit sequence, graph instance/specification, allocation/live counts, graph root, catalog root and lexical commit. A sequence identifies current evidence; it is not a retained searchable generation. Equality is the only relationship exposed by `CurrentToken`.

The S8 catalog step adds a proof-checked keyed map anchored by the head's catalog root. Keys hash the scope, a fixed namespace and the complete identity. The first 16 hash bits select a bucket of at most 128 sorted full keys and 512 KiB of exact values. A 16-level sparse tree authenticates the whole bucket, including membership and absence. Complete enumerations traverse nonempty branches from the trusted root with independent node, entry and byte limits; missing child rows or buckets fail the read. Artifact records contain compact captured retained references. Readers authenticate those records and still verify their referenced source/payload objects before presenting their content as evidence. The format is rebuild-only (`user_version=101`); no reader converts the preceding isolated foundation format.

A catalog-only artifact change can commit its bucket paths and new head in one rollback-journaled SQL transaction under the exclusive gate. It verifies captured retained inputs, charges a durable recovery allowance, advances the sequence on add/edit/delete and leaves an unchanged record as a no-op. A crashed SQL transaction rolls back or commits atomically; an orphan allowance remains until the owning Ready writer retires it. The combined catalog, lexical and graph update path is separate S8 work. The current runtime does not yet call these APIs.

Physical graph IDs append monotonically. ID zero is a non-live DiskANN search seed; an empty store initially has no allocated records, and the retrieval provider creates that seed. Each other ID retains its original exact embedding key. Deletion marks a node non-live while leaving its edges available for traversal. Resurrection of the same key preserves its ID. The provider filters non-live nodes before returning hits and refills within one shared read/candidate budget. A budget-limited result explicitly reports `partial`.

## Bounded integrity and preparation

Every committed graph record is authenticated through a 32-level sparse Merkle tree. Leaves bind the complete graph scope, physical ID, exact embedding key, live bit and exact fixed-size record bytes. Branches bind their position and children; canonical empty subtrees are scope/height-specific. Allocation/live counts belong to the trusted head. A checksum stored beside a mutable record alone would not protect against replacing the record and its checksum together.

DiskANN prepares updates in a private quota-backed SQLite overlay. Base reads verify committed proofs; private dirty records have scoped checksums. Repeated neighbor writes coalesce to one final afterimage per ID. Sealing propagates only those final records through their authentication paths, bounds the streamed descriptor, and drops the base read handle. It does not buffer or copy the complete graph. Any failed mutation, read limit or panic poisons the candidate. A poisoned candidate cannot be published.

The default overlay ceiling is 256 MiB, 65,536 dirty records, 10 million operations and 16 GiB of logical admitted read bytes. Hard configurable ceilings are 4 GiB, one million dirty records, 100 million operations and 128 GiB of logical reads. These are independent bounds. Logical read counters include proof bytes and are not measurements of physical device traffic. S10 evaluates caching after this correctness baseline.

## Readers, writers and recovery

The lock order is lifecycle guard, collection writer, current-index gate, then quota ledger. Gate files live outside resettable collection directories. A request owns a shared gate and its SQLite read transaction until all providers and response evidence have been released. The writer needs an exclusive gate. Idle caches must not retain current read handles; S8 must carry the same request context through every route and usage calculation.

The durable phases are:

| Phase | Read behavior | Recovery action |
| --- | --- | --- |
| Ready | Read the current head | Clean up unowned maintenance allowances if necessary |
| Prepared | Read the unchanged base head | Validate captured inputs and afterimages, then apply or explicitly abort |
| Applying | Report recovery required | Replay captured component changes and finish the exact target |

Preparation stores ordered, bounded retained-input references and the exact graph afterimages before entering Applying. Recovery verifies retained source/payload/model identities and never substitutes live source or reruns a partially completed DiskANN insertion. Graph rows, authentication nodes and the new head commit in one SQL transaction. Repeating a completed commit cannot allocate new IDs. A cleanup failure after Ready is a maintenance error on a successful commit, not a claim that publication failed.

Initialization has a separate bounded owner header and captured bootstrap marker. An interrupted bootstrap can resume only its exact empty/unversioned database or exact committed initial head. An arbitrary database error is never interpreted as an empty collection. Ownership and sidecar bounds are checked before SQLite may recover a hot journal. Combined pending input ownership, lexical commits, catalog/vector mapping updates, GC/reset coordination and all runtime routing remain S8 work.

## Capacity and durability

Authoritative SQLite uses 4 KiB pages, `journal_mode=DELETE`, `synchronous=EXTRA`, no automatic vacuum, and a fixed admitted database ceiling. The exclusive gate permits this simple journal mode. EXTRA includes the directory synchronization associated with deleting a rollback journal; see [SQLite synchronous settings](https://www.sqlite.org/pragma.html#pragma_synchronous). Statement failures abort the entire transaction. Savepoints, attached databases, virtual tables and triggers are prohibited so they cannot introduce an unaccounted second journal.

Before each write phase, admission includes remaining main-file growth and a conservative rollback-journal bound `P × (4096 + 8 + 2S) + 4S`, where P is the original page count and S is the effective sector size. The bound is tied to the pinned SQLite pager implementation; it is not a portable formula for arbitrary database settings. P is recomputed after Prepared, including before an abort. S is read through SQLite's VFS file pointer and normalized using the pinned pager's rules. Changing the SQLite version or journal restrictions requires requalification. The S8 capacity step permits monotonic ceiling growth while Ready: it reserves the larger allowance, synchronizes the new owner bound, reopens SQLite with the new page limit, and then retires the unused allowance. If the writer dies before or after that owner change, the next writer sees the exact old or new bound and explicitly retires the orphan. The runtime must choose growth increments under quota; a large ceiling causes correspondingly large conservative reservations on later writes.

Recovery allowances are separate from disposable staging reservations. Their bounded metadata survives process exit and generic dead-lease cleanup. Only the owning writer, after examining the current journal outside the quota ledger, may release them. Existing files and the outstanding allowance are both counted conservatively. A failed next-phase admission leaves Prepared readable. Hot-journal files and their original-page/sector headers must fit the admitted capacity before SQLite opens them; [SQLite's recovery protocol](https://www.sqlite.org/lockingv3.html#dealing_with_hot_journals) explains why this check precedes a database query.

Metadata replacement synchronizes the file and, on Unix, its parent directory. On Windows it closes the temporary handle and uses [`MoveFileExW` with write-through](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw). Process-death tests qualify the exercised Windows behavior. They do not establish power-loss durability on every filesystem or cross-platform qualification.

## S8 handoff

Adoption requires authenticated per-artifact catalog and exact-key/live-occurrence mappings, incremental Tantivy commits, captured replay for all components, durable database growth, request-scoped readers across every public route, retained-input ownership and bounded reclamation. Replace the old runtime at one rebuild boundary. Remove historical read/checkpoint/rollback capabilities and document current-token expiry. Preserve sources, saved configuration, original citations, collection/root isolation and explicit failure reporting. The S7 foundation alone does not claim faster user-visible indexing or completed removal of generations.
