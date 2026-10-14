# Architecture

AIRK is an independent local indexing/retrieval kernel. Source workspaces are authoritative; all index state is derived. It does not reason, generate answers, manage agents, route model calls or require an external database.

~~~mermaid
flowchart LR
  W[Workspace files] --> I[Native ingestion and format facts]
  I --> K[Collection-scoped kernel]
  K --> S[Collection disk state]
  S --> R[Tantivy lexical and SQLite fact retrieval]
  V[Optional local word vectors and disk-backed DiskANN] --> R
  C[CLI or optional extension] --> K
  A[Coding agent] --> M[Read-only stdio MCP]
  M --> K
  R --> K
  K --> E[Bounded evidence and original locations]
~~~

## Dependency boundaries

| Crate | Responsibility | AIRK dependencies |
|---|---|---|
| airk-core | Artifact/query/fact/citation contracts; storage and embedding traits | None |
| airk-ingest | Ignore-aware discovery, hashing, native grammars, document extraction/chunking | core |
| airk-store | Content-addressed sources/extractions, compact catalogs, SQLite facts/relationships, usage ledgers, durable intents, locks, quotas, relocation and retention | core |
| airk-retrieval | Persistent Tantivy, local word vectors, DiskANN file provider, rank fusion and response budgets | core |
| airk-kernel | Collection-scoped publication, retrieval, coverage, scheduling, usage and maintenance services | core, ingest, store, retrieval |
| airk-mcp | Schemas, official SDK transport and result conversion | core, kernel |
| airk-cli | Commands, native client setup and lifecycle | core, kernel, mcp |

Core must remain free of parser, filesystem, inference and transport implementations. Adapters call kernel services rather than bypassing validation or collection scope. MemoryStore is an available in-memory snapshot adapter; the CLI kernel uses FileStore and immutable retrieval components.

## Current indexing and retrieval

A configured owner root/collection selects one catalog directory. Explicit additional source roots receive unique citation aliases retained in the published inventory. Indexing acquires a collection writer lock, walks eligible sources across those roots under shared budgets, reads capped bytes and computes SHA-256 identities. Each source and its extracted artifact are retained as verified content-addressed objects, and the scanner keeps only compact metadata before moving to the next file. An unchanged source reuses its extraction reference when the extractor version matches. Removed, excluded or unsupported sources disappear from the next successful inventory.

Native grammar profiles produce selected definitions/imports and structured keys. Markdown, delimited data, XML/SVG and OOXML have explicit format adapters. Text chunks use contiguous 40-line blocks. Document chunks preserve page, paragraph, table, worksheet or slide locations; their line spans refer to normalized extracted text, not source-file lines. DOCX tables and XLSX stored cells retain structured row/cell facts. [The format matrix](formats.md) states exact extraction and formula/date limits.

Indexing streams one retained artifact at a time into private Tantivy, optional DiskANN and exact-fact SQLite components. Each immutable generation contains its compact inventory, normalized facts/relationships and optional bound producer analysis. Once components are synced and reopened, their directory is published and a small root SQLite WAL transaction commits the selected component header and inventory digest. Component IDs hash the compact inventory, engine specification and optional model digest; artifact references transitively bind text, facts and locations. A model change advances the generation even without a source edit. Discovery/read/corpus-limit, quota, component-build and transaction failures retain the prior published revision. Unsupported extraction is reported per file and omitted; diagnostic details are bounded. Full synchronous commits and bounded busy waits are configured; power-loss and network-filesystem durability remain unqualified.

Legacy SQLite/JSON snapshots remain readable through the bounded compatibility adapter and migrate to compact catalogs at the next successful index write. A retained old JSON file is recovery material, not an active second index. Unsupported/corrupt SQLite fails closed. Tests cover revision pinning, rollback, migration, digest verification, lazy file reads and retained-object garbage collection. [Storage contracts](collections.md) give layout, caps and recovery details.

New lexical searches read persistent Tantivy postings and stored citations, matching all normalized terms with path filters and deterministic path/chunk ordering. Search and status select the small committed component header without loading retained source bytes. File lists and coverage read compact metadata; file/context services pin a catalog and deserialize only the requested artifact. Legacy snapshots retain their lexical fallback until a successful index migrates them. Exact-fact queries begin a read transaction before selecting the active revision and acquire that generation's lease before opening its immutable SQLite indexes. Rich code evidence records scope/signature/written types and selected framework cues. Syntax targets remain candidates; optional hash-bound SCIP producer assertions are distinct from AIRK's textual references and native parsing. The SCIP adapter explicitly requires a single owner root until its producer contract supports aliased source roots.

Vector queries use Microsoft DiskANN 0.59.0 construction/search algorithms with AIRK's independent provider reading bounded graph/vector records from local files. The optional embedding adapter averages recognized pretrained vocabulary vectors and normalizes them; it is not transformer inference. Model assets are explicitly supplied, license/digest-bound and copied into their immutable generation. Vector mode fails explicitly when the model or recognized query evidence is absent. Hybrid mode applies reciprocal-rank fusion to bounded lexical/vector lists from the same generation. See [local models](local-models.md) for limitations and qualification requirements.

Multiple agent connections read the same collection through separate MCP workers, sharing disk state and retaining independent caches. Read operations do not acquire the collection writer lock. Searches and lazy catalog handles pin immutable generations with operating-system reader leases; later operations refresh their selected generation without reconnecting. Explicit pruning skips active or leased generations, then retains every content-addressed object referenced by any surviving inventory. Collection removal acquires every generation lease and refuses active readers. [Shared-collection behavior](collections.md#multiple-agents-sharing-one-collection) explains configuration and multi-call revision limits.

Every evidence envelope carries collection, generation, hash and original source location. Payload budgets count text or names/paths; metadata/framing is additional. Reads use indexed bytes so citations cannot silently pair with changed live files. Freshness requires indexing. Native [coverage](index-coverage.md) uses shared ingest eligibility rules and content hashes, with explicit partial/failure/capacity counts. Inventory work is bounded and explicitly scheduled; ordinary MCP reads and footer repaints do not traverse sources.

## Scheduling, retention and scale

[The collection design](collections.md) combines disk-backed DiskANN, persistent Tantivy and exact structured facts. DiskANN is not a substitute for lexical or structural indexing. The provider's bounded record reads and synthetic recall tests do not establish larger-than-memory performance, physical SSD I/O, process RSS or production semantic quality.

One deployed container per user/machine stores independently scoped collections in one persistent volume. MCP clients use docker exec to create workers inside that host; caches remain per process. The [native scheduler](native-scheduler.md) persists indexing intents in SQLite and serializes work across a shared data root with an operating-system worker lock. Requested/completed revisions, full-rebuild requests, pause and failures survive helper exits. An explicit foreground watch worker drains the queue and periodically reconciles registered roots without an editor, retaining bounded scan watermarks. No operating-system service is installed automatically.

Native operation targets Windows, macOS and Linux with the same collection contracts. The extension remains a thin helper client: its debounce/idle/save controls batch notifications before native submission, and it displays coverage/usage without owning another index. Notifications still in the editor buffer are reconciled after reopening. Native writer locks and reader leases remain authoritative for publication and pruning.

[Storage quotas](storage-quotas.md) reserve shared root and per-collection logical bytes before participating writers grow files. Retrieval providers accept a neutral charging callback, preserving their dependency on core rather than store. Private SQLite catalogs have reserved page caps, while active-pointer, scheduler and usage journals retain explicit allowances. The ledger counts temporary and retained generations together, recovers abandoned reservation staging and refuses policy changes that revoke live allocations. Filesystem allocation overhead and external writers are outside this application quota.

[Explicit relocation](storage-relocation.md) freezes the shared root lifecycle and every writer/reader lease, hashes and verifies a private destination, then activates it using recoverable source/destination markers. Old managed state remains retained, and source workspaces are unchanged. Current clients fail closed on a retired or pending root; configuration must deliberately select the new destination. This preserves identity instead of silently rebinding a live collection.

Ingestion and component construction retain at most one extracted artifact at a time; the compact inventory and optional producer analysis remain memory-resident within their metadata limits. Legacy migration may load an older bounded snapshot once. Safety limits remain 1 MiB/source, 4 MiB extracted/document, 64 MiB eligible source, 128 MiB extracted/collection and 10,000 indexed files across all aliases. Independent index-pass budgets cap visited non-ignored entries at 100,000 and raw source reads at 256 MiB, including unsupported candidates; exhaustion retains the previous generation. The inventory is capped at 128 MiB and individual extracted-object JSON at 64 MiB. Native parsers are not forcibly preempted, so these are not hard wall-clock guarantees. Raising these safety caps requires scale/fault qualification of parser working sets, metadata, model assets, OS caches and storage behavior.

Further adapters belong inside their owning crates: ingest format/syntax adapters; store source blobs/manifests/publication/retention; retrieval lexical/vector/fusion; kernel scheduler/inspection; MCP fact contracts. Add another crate only for a real independent dependency boundary.

[The code-indexing recommendation review](code-indexing-options.md) led to richer Tree-sitter facts, typed relationships, bundled SQLite and optional revision-bound SCIP imports. [The implementation contract](code-facts-and-relationships.md) documents producer/source/build binding, conservative invalidation and bounded queries. Live LSP/Ctags integrations remain later enhancements. SQLite supplies exact structured evidence alongside Tantivy and DiskANN; no external code-graph engine implementation was adopted.

## Local usage and reporting boundary

[Local usage reporting](usage-reporting.md) uses core contracts, a bounded transactional store ledger and kernel estimation/report services. CLI/MCP serialization hooks record one observation per outer request. Aggregate counters are the default, scoped by collection/root binding without query/source content or external telemetry; diagnostic events require opt-in. The CLI exports JSON/CSV/Markdown and the extension displays the selected collection's report. Signed context estimates identify their baseline and response framing. Historical recording loss, provider consumption and billed savings remain unobserved; nonempty reports are labeled partial.
