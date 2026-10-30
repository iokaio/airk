# Indexing follow-up review — 21 October 2026

This follows the [indexing speed review](indexing-speed-review-2026-10-21.md) and applies to the unreleased 1.2.5 candidate. Earlier measurements and package records describe their named binaries, not this follow-up.

## Implemented changes

1. Full rebuild intents are persisted without acquiring the collection writer or deleting derived state. Rebuild preparation remains in the executing worker, under its existing ownership and request gates.
2. Closing or changing a collection binding discards its editor-side queued revision and coverage-reconciliation memo. Reopening requests fresh work instead of replaying a prior wait result.
3. Reads and newly opened writers can reuse a bounded, process-local verified FTS5 snapshot. A successful replay carries its verified snapshot into the next publication base after SQL commit. See the integrity contract below.
4. Late-completion diagnostics consume the native error's parsed message and requested revision, including errors preceded by progress lines. Those typed fields survive bounded diagnostic-history serialization.
5. Catalog selection errors receive model-setup retry guidance. Python provisioning rejects disagreeing manifest/asset hashes before touching the destination. Native batch selection uses the catalog's platform qualification result. These changes do not enable automatic INT8.
6. Scheduler completion records bind each report to the exact interval of intent revisions satisfied by that pass. A later path-scoped pass cannot replace a full-pass report returned to an earlier waiter. History retains four passes per binding, bounded globally to 128 records and 16 MiB of report text. An expired or older unrecorded result returns an explicit superseded-report error instead of an unrelated report. The latest queue status report remains available.
7. Invalidated or not-yet-measured coverage says **Coverage pending rescan**. Coverage inventories run at startup, every five minutes and on explicit refresh; indexing does not add a second inventory. Incomplete/error observations remain unknown. Automatic reconciliation attempts are memoized by published generation within the exact collection/helper/storage binding, not by report object identity or the inventory epoch. A healthy observation clears the memo; new source events and explicit retries can still request work.

Contributor guidance now explicitly requires the authorized human contributor's DCO sign-off on every commit. `AGENTS.md` and `CLAUDE.md` remain identical.

## Lexical cache integrity contract

A matching commit digest alone is insufficient. Entries also bind the database path, filesystem identity, length, filesystem change time and the [rollback database header change counter](https://www.sqlite.org/fileformat2.html#file_change_counter). Windows additionally requires a change notification on a known local fixed volume. The 1.3.0 release-preparation correction on 2026-10-22 adds Linux inode notifications on ext4, XFS, Btrfs, tmpfs and overlayfs: consecutive writes can share a change-time tick, so timestamps alone did not invalidate a raw write with restored modification time in Docker. Unknown or remote Linux filesystem types use full verification; a pending event, watch removal, overflow or failed poll invalidates the entry without consuming/resetting the notification. A changed, unavailable, signaled or failed witness discards the entry and uses full physical verification. Remote Windows volumes do not use this cache. Other unsupported witnesses also fall back to full verification.

The cache holds at most eight snapshots with a conservative 32 MiB ledger estimate. It retains no collection gate or SQLite read transaction. Counter reads use short-lived file handles on every platform, so a peer process can rebuild/remove the database without a cached SQLite handle pinning Windows files or unlinked Unix storage. Destructive rebuild, collection removal and exclusive storage lifecycle operations also release cached entries before manipulating files. Entries expire thirty seconds after verification or authenticated publication; hits do not extend expiry. Failed preparation/replay never seeds a committed entry.

This deliberately changes the previous per-request full-row hashing contract. Warm reads trust unchanged operating-system/SQLite witnesses for that short interval. These witnesses cannot prove absence of silent device corruption or deliberate out-of-protocol tampering that defeats them; such damage can remain undetected until re-verification. Windows notifications also depend on filesystem/cache behavior; see [Microsoft's notification contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstchangenotificationw). Explicit retained-data integrity diagnostics always bypass the cache and hash the full lexical state. Citation/source authentication, every admitted live-source hash, collection isolation, budgets, one writer and DiskANN remain in force.

Preparation, rollback and captured replay remain. Changed row groups are still rehashed, and `WITHOUT ROWID` tables and final digest folding still have corpus-dependent work. Separate CLI processes do not share this cache, so repeated one-shot terminal searches still pay cold verification. A long-running MCP process and multiple semantic publications in one process are the intended beneficiaries. No measured end-to-end speedup is claimed for this follow-up.

## Validation

Regression tests cover a full intent while the writer is held, the durable earlier full report after a later scoped pass, coalesced revisions/history expiration/root isolation, close/reopen editor bindings, reconciliation retry suppression, progress-prefixed error envelopes through persisted history, model-selection retries, hash disagreement before provisioning, fixed cache expiry, Windows deletion with an idle cache, and SQL/raw-write invalidation with restored modification time.

Final build, suite and packaging results are recorded in the [1.2.5 candidate notes](releases/release-1.2.5.md). Installed-editor acceptance and other-platform qualification remain separate.
