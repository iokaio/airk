# Workspace indexing coverage

AIRK measures how much of a folder's eligible saved content matches its indexed revision. The VS Code footer displays that measurement for the active folder; clicking it queues **Index Now**. Coverage measures freshness and eligibility, not retrieval relevance or token savings.

## Definition and eligibility

The percentage is `floor(current indexed eligible files × 100 / eligible files)`. Only a completed inventory with all eligible files current can display 100%. Zero eligible files displays **No eligible files**. An incomplete or invalidated inventory displays **Coverage unknown**; a folder with eligible content and no snapshot displays **Not indexed**.

Native ingestion shares the indexer's discovery, containment, ignore, source-size and extraction rules. Arbitrary UTF-8 files qualify without an extension allowlist. PDF, DOCX, PPTX and XLSX use their native extractors; SVG follows the structured UTF-8 adapter. Unsupported binary files, ignored/generated/secret files, symlinks, paths over 4096 bytes and files over the collection's configured file limit (16 MiB by default) are excluded. The selected data directory is excluded even when it is inside the workspace. Coverage caches bind the [collection limits](ingestion-limits.md); changing them invalidates cached eligibility.

The report separates current, stale, missing, extraction-failed, unreadable, capacity-blocked and pending-removal counts. Failed extraction stays in the eligible denominator. An unreadable candidate makes eligibility incomplete and suppresses the percentage. Deleted, newly ignored or newly unsupported indexed sources count as pending removals, outside the denominator. Capacity-blocked is a subset of stale/missing files when indexing would exceed an aggregate limit; it is not another category to add to the denominator.

Automatic collection limits allow 100,000 successfully extracted files and up to 2 GiB of eligible source, sized from detected machine memory, with twice that for extracted chunk text. Saved policies remain authoritative; see [ingestion limits](ingestion-limits.md). Hitting an aggregate limit preserves the previously published index. Unsupported binaries do not consume those eligible-corpus limits, but do consume the index pass's independent raw-read budget (four times the automatic source allowance) and 1,000,000 visited-entry budget. Directories also consume the entry budget; exhausting either budget fails the entire pass. A full index can still have pending removals; the tooltip reports them separately.

## Bounded work and persistence

`airk coverage` reads the cached measurement without traversing the workspace. `airk coverage --refresh` explicitly runs a native inventory, including before the first index. The collection's `coverage.json` cache includes its root, collection, generation, extractor/policy versions, monotonic inventory revision and measurement time. A cache with a different binding/version or a corrupt payload is unknown. A later index generation invalidates the cached percentage until refresh.

Refresh defaults to at most 50,000 visited non-ignored entries, including directories/errors, and 256 MiB of source reads, with a 15-second deadline checked between entries. Known oversized files are excluded before reading. Individual extraction has separate source/document limits; an in-progress parser is not forcibly interrupted at the deadline. Reaching any inventory limit produces a partial report. File contents are hashed each refresh; metadata is never proof of freshness. Extraction eligibility and extracted-byte counts are reused only for an identical digest and extractor version.

The report describes observations collected during a scan. It cannot promise that no file changes after its hash was read. Concurrent publication invalidates the comparison. Editor filesystem events arriving during a scan invalidate its result and queue another bounded scan. Source files remain read-only, and cache publication is serialized and replaced atomically under the collection writer lock.

## Status-bar behavior

The footer prefers the active editor's workspace folder, using the most specific root if folders overlap. With several folders and no active match it displays **Choose folder**; clicking asks which folder to update. Separate roots retain separate native collections. The tooltip names the folder, shows counts and measurement time, distinguishes pending disk changes and unsaved editor documents, and explains that only saved bytes are indexed.

**Index Now** shares the existing pending/running pass, and changes received during a pass queue another revision. Manual indexing works while automatic indexing is paused. The separate **AIRK: Open Controls** command remains in the command palette. **AIRK: Show Collection Status and Coverage** opens the full coverage report and explanation. Untrusted workspaces launch neither indexers nor coverage scanners.

Scans run serially in the background after the configured quiet/idle interval. Bursts coalesce; periodic reconciliation runs every five minutes and startup schedules a scan. Index completion or failure triggers a refresh. Storage/folder changes discard observations for the previous binding. On-save and manual modes still refresh coverage after external changes, even when those modes do not automatically index them. Indexing failure and storage-budget states remain visible. Coverage updates never fire MCP server-definition change events.

Within one extension host, coverage, indexing and pruning share a writer queue for each data directory and collection. A manual index waits for an active coverage write. Native locks still arbitrate other editor/CLI processes; contention fails explicitly, retains pending indexing work and permits a later retry. Awaited commands report failures to their callers as well as showing a notification. Read-only requests and separate collections do not share this local queue.

The [native scheduler](native-scheduler.md) persists batches and serializes indexing across hosts sharing a data root. Editor notifications are coalesced in the editor before the batch reaches native scheduling; startup inventory reconciles notifications lost when the editor was closed.

## Delivery and checks

Native tests exercise shared ignore rules, arbitrary UTF-8, SVG, PDF/DOCX/PPTX/XLSX extraction failure, per-file exclusion, aggregate capacity, new/changed/deleted files, corrupt/root-mismatched caches, empty and bounded incomplete inventories, failed publication and generation invalidation. Extension tests exercise 999/1000 rounding, unsaved-buffer explanations, 100-event batching, edits during scans, closed/reopened storage bindings, repeated manual clicks, shared writer exclusion and explicit failure/retry behavior.

Clean-editor, remote-host and cross-platform acceptance remains part of release qualification. Coverage is separate from [usage/estimated savings](usage-reporting.md): a full index does not establish retrieval relevance or token savings.
