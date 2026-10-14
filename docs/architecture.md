# Architecture

AIRK is an independent local indexing/retrieval kernel. Source workspaces are authoritative; all index state is derived. It does not reason, generate answers, manage agents, route model calls or require an external database.

~~~mermaid
flowchart LR
  W[Workspace files] --> I[Native ingestion and format facts]
  I --> K[Collection-scoped kernel]
  K --> S[Collection disk state]
  S --> R[Lexical and exact fact retrieval]
  V[Planned local embeddings and disk-backed DiskANN] --> R
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
| airk-store | SQLite revision/fact/relationship transactions, legacy JSON migration, memory snapshots, collection maintenance/lock | core |
| airk-retrieval | Tokenization, lexical candidates and text budgets; future vector provider | core |
| airk-kernel | Shared use cases, root/collection isolation, freshness boundary | core, ingest, store, retrieval |
| airk-mcp | Schemas, official SDK transport and result conversion | core, kernel |
| airk-cli | Commands, native client setup and lifecycle | core, kernel, mcp |

Core must remain free of parser, filesystem, inference and transport implementations. Adapters call kernel services rather than bypassing validation or collection scope. MemoryStore is an available embedding adapter; the CLI kernel currently uses FileStore.

## Current indexing and retrieval

A configured root/collection selects one snapshot directory. Indexing acquires a collection writer lock, walks eligible sources, reads capped bytes and computes SHA-256 identities. An unchanged source reuses extraction/facts/chunks when the extractor version matches. Removed, excluded or unsupported sources disappear from the next successful snapshot.

Native grammar profiles produce selected definitions/imports and structured keys. Markdown, delimited data, XML/SVG and OOXML have explicit format adapters. Text chunks use contiguous 40-line blocks. Document chunks preserve page, paragraph or slide labels; their line spans refer to normalized extracted text, not source-file lines. [The format matrix](formats.md) states actual extraction coverage.

SQLite WAL transactions publish retained revision data, normalized facts, relationships and optional analysis manifests together. Discovery/read/corpus-limit and transaction failures retain the prior revision. Unsupported extraction is reported per file and omitted. Full synchronous commits and bounded busy waits are configured; power-loss and network-filesystem durability remain unqualified.

Legacy JSON snapshots remain readable when no SQLite catalog exists and migrate at the next successful index write. A retained old JSON file is recovery material, not an active second index. Unsupported/corrupt SQLite fails closed. Tests cover WAL revision pinning, rollback and migration. [Storage contracts](code-facts-and-relationships.md) give caps and recovery details.

Lexical/text queries reopen retained snapshot data, and each worker caches an in-memory inverted map by generation. Lexical search matches all normalized terms and orders by path/chunk. Exact facts and relationships query SQLite indexes directly, within one read transaction. Rich code evidence records scope/signature/written types and selected framework cues. Syntax targets remain candidates; optional hash-bound SCIP producer assertions are distinct from AIRK's textual references and native parsing.

Multiple agent connections may read the same collection concurrently through separate MCP workers; they share disk state and retain independent memory caches. Read operations do not acquire the collection writer lock. Atomic publication keeps each operation on one complete snapshot, and later operations refresh their generation/cache without reconnecting. The final disk-backed provider must preserve this contract with immutable generation handles and reader leases before pruning. [Shared-collection behavior](collections.md#multiple-agents-sharing-one-collection) explains configuration and multi-call revision limits.

Every evidence envelope carries collection, generation, hash and original source location. Payload budgets count text or names/paths; metadata/framing is additional. Reads use indexed bytes so citations cannot silently pair with changed live files. Freshness requires a native indexing call; the optional extension schedules those calls from coalesced filesystem events or explicit controls. Its pending revision state is per editor host, not yet a persisted machine-wide event watermark.

## Final index and hosting direction

[The collection design](collections.md) selects disk-backed DiskANN for vector retrieval, a persistent lexical adapter (Tantivy) and exact structured facts, with coherent immutable generation manifests and bounded memory caches. DiskANN is not a substitute for lexical or structural indexing.

One deployed container per user/machine stores all independently scoped collections in one persistent volume. Current MCP clients use docker exec to create workers inside that host; shared caches and centralized scheduling are future daemon work. Native use targets Windows, macOS and Linux with the same collection data contracts. The optional implemented extension under packages/vscode wraps the native helper and never owns a second index engine. Its UI/event batching calls the kernel through the CLI; native writer locks/publication remain authoritative. Centralized scheduling and retention belong in the kernel as those durable services are implemented.

Next modules belong inside their owning crates: ingest format/syntax adapters; store source blobs/manifests/publication/retention; retrieval lexical/vector/fusion; kernel indexing scheduler/inspection; MCP fact contracts. Add another crate only for a real independent dependency boundary.

[The code-indexing recommendation review](code-indexing-options.md) led to richer Tree-sitter facts, typed relationships, bundled SQLite and optional revision-bound SCIP imports. [The implementation contract](code-facts-and-relationships.md) documents producer/source/build binding, conservative invalidation and bounded queries. Live LSP/Ctags integrations remain later enhancements. SQLite does not replace DiskANN or change the Tantivy plan; no external code-graph engine implementation was adopted.

## Local usage and reporting boundary

[The usage plan](usage-reporting.md) adds `airk-core::usage` contracts and `airk-kernel::usage` estimation/report helpers. Aggregate counters are the planned default, scoped by collection/root binding without query/source content or external telemetry. A future store adapter owns bounded persistence/retention; kernel spans and adapter serialization hooks supply one observation per outer request; the extension displays reports. No recorder or report command is wired into current requests yet. Estimates identify their comparison baseline and response framing; they are not measured provider consumption or billed savings.
