# Transactional storage relocation

Relocation copies one managed data root into a new directory, verifies its contents, then activates the destination. Source workspaces and the old index payloads remain intact. It is an explicit maintenance action; changing the extension's storage setting alone does not migrate an index.

## Operator flow

Stop foreground scheduler workers and disconnect MCP/editor clients that hold cached generations. Use an explicit old `--data-dir` for every command:

~~~console
airk --data-dir /old/state storage plan /new/state
airk --data-dir /old/state storage relocate /new/state
airk --data-dir /old/state storage recover
~~~

`plan` validates boundaries, takes maintenance leases, hashes managed files and reports file bytes plus conservative copy headroom. It does not copy the index or retire the old root. The destination must not already exist, its parent must exist, and neither data root may contain the other. Relocation requires at least one collection with SQLite revision metadata; index legacy JSON-only collections first.

`relocate` repeats those checks before writing a durable recovery receipt. It requires quota headroom for the destination copy and temporary replacements. Both root and per-collection policies apply. The destination must not contain any registered primary or additional source root. A data directory inside a workspace is supported through normal data-directory exclusion; relocation adds `*` to a managed `.airkignore` in both data roots so the retained old index cannot become source input after configuration changes.

After successful activation, configure the extension, CLI and MCP clients to use the returned destination. AIRK refuses ordinary operations against the retired root and reports its recorded destination. It never silently redirects or rebinds a collection. The old payload is retained for deliberate operator inspection and later cleanup; relocation does not delete it.

## Copy and recovery contract

The operation takes an exclusive root lifecycle lease, scheduler-worker lock, every collection writer/usage lock and every generation reader lease. The initial exclusive root lease retries for up to 250 ms before refusing active clients; this tolerates short-lived reader descriptors inherited during Unix process creation. No other lock is held during this initial retry. Shared lifecycle admission and the remaining collection locks stay fail-fast, preserving lock order. Collection enumeration occurs after taking the root lease. Ordinary current-version readers, publishers, quota changes and scheduler activity acquire compatible shared lifecycle leases, preventing new activity during relocation. A cached generation still prevents maintenance until its reader lease closes.

The receipt binds canonical source/destination identities and a SHA-256 inventory. Every regular file is verified before and after copying through a temporary file. Inventory is bounded to 100,000 visited entries and a 32 MiB receipt; filesystem links and nonregular state fail closed. Runtime lock ownership, active quota records and known private staging are not copied. Fresh generation lease files are created at the destination. SQLite files and recovery journals are copied only while the root is quiescent.

The destination remains marked pending until all files verify. A retired-source marker is then written before removing the pending marker. Failures after preparation leave the old root closed to ordinary activity: run `storage recover` against that root to resume the recorded transaction. Recovery validates identities and receipts, reuses already verified copies, and replaces only transaction-owned partial files. Do not delete or edit recovery markers to bypass this check. Recovery after completed activation is idempotent and does not rewrite newer destination publications.

The test suite verifies evidence/source preservation, live-reader and active-allocation refusal, quota failure before retirement, child-process interruption after durable preparation, successful recovery and protection of newer destination state. It does not establish universal power-loss atomicity, network-filesystem locking, hostile concurrent filesystem mutation or all native-platform behavior. Unix directory syncing is used where supported; Windows flushes files without claiming portable directory-flush guarantees.

Quota policies apply per active data root. Copy admission accounts conservatively for retained old state and temporary destination bytes during relocation, but subsequent writes at the destination do not impose a physical machine-wide cap that includes indefinitely retained old roots. Keep that retained copy in capacity planning. See [storage quotas](storage-quotas.md).
