# Service publication mechanism spike, October 22, 2026

This record covers isolated store experiments for Phase 0 of the [service plan](airkservice-architecture.md). It changes no runtime journal mode, format, collection namespace or query contract. The service architecture and clean break remain settled; these experiments guide the storage implementation.

The executable evidence is [publication_spike_tests.rs](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/current/publication_spike_tests.rs). Fixtures use only original tiny local SQLite databases. The first test uses AIRK's real FTS5 schema, flush and physical shadow-row sealing. Its small publication marker stands in for additional transactional state; it does not test actual DiskANN publication or retained-object reclamation.

The first experiment establishes these properties:

- A reader admitted before a WAL commit retains its old coherent head, marker and verified lexical state while a new reader observes the new committed state.
- The current physical lexical seal can verify the SQLite snapshot returned through committed WAL pages. It detects a later committed evidence change that leaves the sealed head unchanged.
- Main-file bytes remain unchanged across both the valid publication and the invalid overlay. A main-file digest by itself cannot authorize lexical cache reuse in that mode. A new implementation must verify the admitted SQL snapshot or establish witnesses covering every relevant overlay and checkpoint transition.

The second experiment sets a 16-page main-file capacity and `journal_size_limit=0`, pins a reader, and repeatedly commits changes to one tiny row. WAL grows beyond the main-file capacity. A truncating checkpoint reports incomplete work until the reader releases its snapshot, then truncates successfully. SQLite defines `max_page_count` for the database file and journal-size limits for retained journals/reset WAL files; they are insufficient as an append-time WAL admission boundary. See the [SQLite pragma reference](https://sqlite.org/pragma.html#pragma_max_page_count) and [journal-size behavior](https://sqlite.org/pragma.html#pragma_journal_size_limit).

These results support snapshot-bound verification. SQLite describes independent WAL readers and checkpoint constraints in its [WAL concurrency documentation](https://sqlite.org/wal.html#concurrency). A second, [test-only native VFS prototype](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/current/wal_budget_spike.rs) now supplies the finite append/mapping boundary. Durable ownership/reservations and lifecycle integration remain unresolved before WAL can serve AIRK's authoritative index.

The VFS prototype forwards to SQLite's native platform implementation while rejecting WAL writes, growth truncations and preallocation hints above a fixed cap. It prevents chunk-size rounding from bypassing that cap and rejects unadmitted shared-memory regions before mapping. The [SQLite file interface](https://sqlite.org/c3ref/io_methods.html) supplies these callbacks; [VFS registration](https://sqlite.org/c3ref/vfs.html) retains their routing state. The prototype never replaces the default VFS, and only isolated test connections select it. Registration/name storage is retained until process exit with a 64-experiment ceiling, keeping callback lifetimes valid.

Two more tests pass on Windows and non-root Debian 12 x64. Under a pinned reader, real SQLite commits reach the 64 KiB WAL cap and receive `SQLITE_FULL` before exceeding it. A fresh reader observes exactly the last accepted commit; the pinned reader retains its old state. Checkpointing after reader release permits another update, and integrity verification succeeds. A zero shared-memory allowance rejects mapping before a sidecar grows. This proves the tested file-I/O boundary, not owner-validated sidecars, durable quota accounting, crash recovery, filesystem allocation slack or integrated DiskANN/object leases.

The intended contract for that prototype is:

1. Use an explicitly new owned format and namespace. Validate sidecar ownership and types before SQLite opens them; old helpers must reject the format.
2. Authenticate the head, FTS5 state, graph and current mappings from one admitted SQLite snapshot. Keep the existing rollback-only lexical cache disabled for WAL until replacement witnesses are proven. A supplied stale revision remains an explicit error.
3. Enforce append and shared-memory limits at a boundary that can reject growth before it exceeds durable admission. Track checkpoint/main-file overlap and failed cleanup; neither `max_page_count` nor `journal_size_limit` substitutes for that enforcement.
4. Keep resource charges through incomplete checkpoints and recovery. Pressure stops new admission and requests cancellation of old readers. A timer alone cannot release their retained objects or disk allowance.
5. Admit a read snapshot and its external-object lease atomically with publication/reclamation. Release only on completed materialization or proven process exit. Move lexical preparation outside publication exclusion without exposing partially applied evidence.
6. Build full replacements in quota-reserved owned siblings. Validate them, switch an owned manifest, drain old readers, then reclaim. Windows requires no rename/delete of an open database directory.

All four experiments passed on Windows and in the non-root Debian 12 x64 fixture. That Linux run also passed five native service process regressions, seven adapter tests and eight service-store tests. They do not complete Phase 0's crash/tamper/sidecar/quota proof or Phase 3's integrated publication work. The existing rollback-journal format remains active until the replacement contract passes those tests.
