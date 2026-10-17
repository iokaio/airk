# Collections, storage and DiskANN

One user/machine can host independent collections in one native data directory or one persistent container. A workspace's MCP configuration fixes its canonical root, collection ID and data directory. Clients using the same three values share the index. MCP retrieval arguments cannot select another collection.

## Identity and isolation

Collection IDs contain 1–64 lowercase ASCII letters/digits/hyphens/underscores. Separators, case aliases and reserved filesystem names are rejected. Stored root/collection identities are checked before candidate lookup. Citations use original relative paths, source hashes and source/document locations.

One canonical owner root binds each collection. Explicitly registered additional roots have unique aliases, with citation paths `@alias/<relative-path>`; the alias map is retained with each published revision. See [source roots](source-roots.md). Container root paths do not establish host identity after mounts change; never reuse an existing collection ID for a different project. Source workspaces remain read-only to AIRK.

## Published components

~~~text
<data-root>/
  models/local/                         # explicitly provisioned local model, optional
    model.json
    vocabulary.json
  collections/
    .writer-locks/<collection>           # persistent native OS lock
    .reader-locks/<collection>/<revision> # native reader leases outside deletable state
    <collection>/
      index.sqlite3                     # small transactional active revision pointer
      objects/sources/<sha256>           # deduplicated original bytes
      objects/artifacts/<sha256>         # verified per-artifact extraction payloads
      writer.lock                       # compatibility/diagnostic writer marker
      usage.sqlite3                     # bounded local counters, separate from index
      generations/<revision-sha256>/
        inventory.json                  # compact artifact references and source aliases
        catalog.sqlite                  # immutable exact facts and relationships
        manifest.json                   # engine/model/revision/count binding
        lexical/                        # immutable Tantivy segments
        vectors.bin                     # DiskANN graph/vectors, if a model is configured
        model/                          # verified immutable assets for this revision
~~~

The component identifier hashes the compact inventory and engine specification, including parser version, source hashes, content-addressed extraction references, source aliases, generation and optional model digest. Each extraction object's digest binds its text, facts and original locations. Ordinary indexing detects a model digest change and advances the generation. A numerical generation cannot accidentally select an orphan build made with a different model.

Indexing retains each successfully extracted artifact and its original bytes, then releases its text before discovering the next file. Component construction loads one verified artifact at a time into Tantivy, the optional DiskANN graph and the immutable exact-fact SQLite catalog. After components are synced and reopened, their private directory is renamed into place. A small root SQLite transaction publishes the revision pointer and inventory digest. Readers select all components from that committed revision. A build or transaction failure preserves the previous published revision; unpublished objects and completed directories are reclaimable derived state.

The active pointer uses SQLite WAL, full synchronous commits and bounded busy waits. Private exact-fact databases use a reserved page limit and no recovery journal because an interrupted private build is discarded. Queries begin a read transaction before selecting the revision, then pin the matching immutable catalog. Immutable readers use manual Tantivy reload and never watch/mutate their selected generation. Graph records have SHA-256 checksums, dimensions, bounded degree and stable generation-local IDs. Corrupt or missing required components fail explicitly; a new persistent revision does not silently become an in-memory index.

Legacy SQLite/JSON snapshots remain readable through the bounded compatibility adapter. The next successful index moves extraction payloads into content-addressed objects and publishes the compact catalog. Legacy JSON is retained for operator recovery. Migration never deletes source files or arbitrary directories.

## Retrieval and memory

Lexical search uses [Tantivy](https://docs.rs/tantivy/0.26.2/tantivy/) 0.26.2 with persistent postings and stored citation payloads. Whole identifier, underscore and camel-case aliases preserve AIRK's lexical contract. Terms use AND; path-prefix filters run in the lexical query. Result ordering remains deterministic by indexed path/chunk. Query and payload limits apply to disk retrieval too.

Vector construction and search use [Microsoft DiskANN](https://docs.rs/diskann/0.59.0/diskann/) 0.59.0 algorithms with an independent AIRK file provider. It reads adjacency lists and full-precision vectors from fixed-size local records as needed during both construction and querying. It does not restore the complete graph/vector corpus into RAM. A bounded DiskANN pruning working set is the build cache; graph degree is 48, pruned degree 32 and build search width 64. Queries cap vector distance reads at 65,536 and fail explicitly when exhausted. Kernel queries expose at most 100 hits; vector overfetch is bounded to 1,000 candidates.

The provider uses portable file seek/read operations rather than upstream's OS-specific `diskann-disk` package. This does not establish SSD throughput, async I/O, physical disk reads, bounded OS page-cache RSS, or cross-platform qualification. Tantivy memory maps its immutable files. Cold/warm latency, operating-system cache effects, full-process RSS and real-corpus recall still need release measurements.

Search and status load the small SQLite component header. File lists and coverage use compact inventory metadata; file/context services load only the requested verified artifact. Ingestion and component construction stream one artifact at a time instead of collecting all extracted text. Inventories remain bounded to 512 MiB and individual extracted-object JSON to 256 MiB; metadata and optional imported analysis are held in memory. Automatic source limits use one eighth of detected memory, capped at 2 GiB eligible source (512 MiB if unknown), with twice that extracted text and four times that raw reads across all aliases. They admit up to 256 MiB/file, 32 MiB extracted text/document and 100,000 files. Each default pass stops at 1,000,000 visited non-ignored entries or its raw-read budget, including unsupported candidates; exhaustion preserves the prior generation. Explicit [collection limits](ingestion-limits.md) permit larger measured workloads without implying qualification at the implementation maximum.

Publication reserves available root/collection capacity before source retention. Blob, Tantivy, graph, model and metadata writes charge their allocation before writing; private SQLite has a page cap, and active-pointer publication reserves its database/journal allowance. Temporary and retained state count together. Exhaustion fails the build without advancing the pointer. Conservative reservations may reject concurrent work even when its eventual compact result would fit; inspect quota status, prune obsolete state or explicitly change policy. See [storage quotas](storage-quotas.md).

Local vectors require deliberate assets; see [local models](local-models.md). `--mode vector` performs approximate retrieval. `--mode hybrid` fuses lexical and vector ranks from one pinned generation. Missing/invalid models and queries without recognized model tokens produce explicit not-ready errors. No remote inference or automatic fallback occurs. Vector path filtering happens over bounded candidates and can underfill results; `truncated` indicates an incomplete candidate scan.

## Multiple agents sharing one collection

Claude Code, Codex, Gemini CLI, Copilot and other local clients can concurrently read the same collection. Their client-side server names may differ. Each stdio connection has a separate worker; in Docker all workers run inside the one deployed container. There is no separate index per agent.

One writer publishes at a time; other writers receive a locked error. Native OS writer locks are released on process exit. A legacy marker without the new native lock version still requires deliberate operator recovery. Readers do not acquire the writer lock. Each search pins a complete immutable generation with a shared native lease. A request overlapping publication may finish on the preceding generation; the next request loads the new component header and refreshes the cache. Multiple requests are not a transaction: compare generation and hashes when combining evidence.

## Retention and removal

The [searchable retention policy](retention.md) keeps active evidence plus the configured zero to eight predecessors, with live reader leases protecting additional revisions. New collections use automatic retention; upgraded collections remain manual until an operator selects automatic retention. `collections retention show` reports effective mode/origin, protected bytes, pending work and maintenance failures. Policy preview/set/run are separate from the older explicit prune command below. History packages and checkpoints are not yet available.

~~~console
airk --data-dir /path/to/state --collection project-a collections list
airk --data-dir /path/to/state --collection project-a collections prune
airk --data-dir /path/to/state --collection project-a collections prune --apply
airk --data-dir /path/to/state collections remove obsolete-project
~~~

Pruning defaults to a preview with candidate IDs, reclaimable bytes, pinned IDs and unreferenced object counts. Explicit `--apply` is an active-only manual trim: it can remove the retention policy's predecessor, while preserving active and reader-leased evidence. Use `collections retention run --apply` to preserve the policy keep set. Object GC verifies surviving dependency roots before deleting unreferenced objects. Published local model copies are removed only with their obsolete generation. Failed-build objects can be reclaimed without disturbing published evidence. Quota-owned private staging and validated intent-bound retirements are recoverable; unknown directories, malformed markers and unrelated interrupted removals require inspection.

Collection removal validates the managed root and snapshot identity, acquires writer/worker/usage lifecycle locks and every generation lease, then deletes derived state. An active cached reader prevents removal until disconnected. Filesystem links cause maintenance to fail. Source workspaces are never deletion targets.

Automated tests exercise native generation publication, reopen/recall/corruption fixtures, model digest isolation, source rename behavior and leased pruning. Real power-loss durability, network filesystems, larger-than-memory scale and the complete native release matrix require independent qualification.

## Relocating a data root

Use `airk --data-dir OLD storage plan NEW` to preview an explicit move and `storage relocate NEW` to perform it. The destination must be a new directory with sufficient quota headroom. Active workers, writers, usage transactions or cached generation readers prevent relocation. The service verifies file hashes before activation, keeps old managed payloads, and records recoverable markers so ordinary clients cannot reopen retired or incomplete roots. `airk --data-dir OLD storage recover` resumes the recorded operation. Update client configuration explicitly after success; the storage picker does not silently migrate collections. See [relocation and recovery](storage-relocation.md) for boundaries and retained-copy capacity planning.
