# Architecture

AIRK is an independent local indexing/retrieval kernel. Source workspaces are authoritative; all index state is derived. It does not reason, generate answers, manage agents, route model calls or require an external database.

The [October 17 release summary](releases-2026-10-17.md) covers the 1.1.0–1.1.2 changes and upgrade behavior. This page describes the resulting component boundaries; release publication and workload qualification are recorded separately.

~~~mermaid
flowchart LR
  W[Workspace files] --> I[Native ingestion and format facts]
  I --> K[Collection-scoped kernel]
  K --> S[Collection disk state]
  S --> R[Tantivy lexical and authenticated fact retrieval]
  V[Default local embeddings and disk-backed DiskANN] --> R
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
| airk-store | Content-addressed sources/extractions, authenticated SQLite catalogs/postings/graph records, usage ledgers, durable intents, locks, quotas, relocation and current-object cleanup | core |
| airk-retrieval | Persistent Tantivy, local word-vector/ONNX inference, mutable DiskANN algorithms, rank fusion and response budgets | core |
| airk-kernel | Collection-scoped publication, retrieval, coverage, scheduling, usage and maintenance services | core, ingest, store, retrieval |
| airk-mcp | Schemas, official SDK transport and result conversion | core, kernel |
| airk-cli | Commands, native client setup and lifecycle | core, kernel, mcp |

Core must remain free of parser, filesystem, inference and transport implementations. Adapters call kernel services rather than bypassing validation or collection scope. MemoryStore is an available in-memory snapshot adapter; the CLI kernel uses FileStore with the current SQLite/DiskANN store and head-bound Tantivy segments.

The [single-current-index foundation](current-index-foundation.md) provides neutral authenticated graph contracts in core, a mutable DiskANN provider in retrieval and owner-scoped journal, gate and quota support in store. S8 integrates it into normal indexing and request routing at the 1.2 rebuild-only boundary. Production routes use current publication and recovery, with no old-format reader or migration.

The [1.2 current-index contract](current-index-runtime.md) is authoritative for production storage, publication and request lifetimes. The v2 publication/history documents describe retired 1.1 implementations; store-level tests retain those primitive contracts, but the kernel does not use them as compatibility readers.

## Current indexing and retrieval

Each canonical owner root and collection selects an isolated current store. Explicit additional roots retain their citation aliases in authenticated applied configuration. Scanning uses shared discovery/source/extraction budgets, hashes saved sources and selects a complete extraction profile before reuse. Bounded workers perform pure extraction on captured inputs. The coordinator binds path-free payloads and retains source/extraction objects. Full reindexing bypasses extraction reuse.

Essential, Balanced and Detailed extraction remain separate from semantic vector inputs. New 1.2 collections default to Balanced. Native structured queries disclose deliberate omissions. Imported producer assertions keep explicit source/build/provenance binding and conservative invalidation. Original document locations, mapped source coordinates, partial extraction and read byte limits remain part of every evidence contract.

The current SQLite map authenticates scoped artifact, continuity, posting, model/resource and graph bindings. DiskANN remains the selected disk-backed vector engine. Indexing applies only changed artifact/posting/lexical/graph records unless a policy or model rebuild requires broader replacement. Tantivy forks immutable segment files and changes affected path documents; the current journal captures the selected manifest and exact afterimages. Empty-policy transactions and no-op scans have explicit semantics. Scheduled indexing first commits lexical and fact evidence with incompatible vectors retired, then commits bounded semantic batches with a source-bound pending manifest. Each batch uses the same current publication journal and advances the exact revision. The scheduler owns automatic retries, pause and failure blocking; reads disclose committed semantic coverage without launching work.

A durable Prepared/Applying journal and multiprocess request gate coordinate all components. Each read holds a shared gate through materialization; Applying and cleanup require exclusion. Prepared keeps the base readable. Interrupted Applying is unavailable until captured replay finishes. Startup recovery uses retained inputs, never changed live sources. Exact tokens and cursors expire when the current head changes; idle kernels do not retain old generations.

Search verifies current candidates against authenticated source/extraction identities and graph records. Lexical, vector and hybrid search share one request head. Scoped semantic search ranks a bounded authenticated graph subset. Conditional reads bind representations and original locations; current file/unit handles support conservative continuity. Historical search, change history, checkpoints and rollback are unsupported. Native syntax targets remain candidates, and imported resolution remains a producer assertion.

## Scheduling, cleanup and scale

The native scheduler serializes submitted work and persists revisions, pause, explicit requests and blocked failures in the 1.2 namespace. Coverage remains an independent observation of saved source files. The extension manages collection-scoped controls and coalesced requests; it neither implements indexing nor claims live editor buffers are indexed.

Quota-admitted database growth reserves the larger SQLite bound and rollback allowance before publication. Postcommit cleanup proves current ownership before removing bounded unreferenced objects and lexical forks. `collections maintain` repeats cleanup without publishing. Active requests block destructive collection operations; idle connections do not. Fresh 1.2 collection/settings ownership isolates old indexes rather than migrating them. See [collections](collections.md), [recovery](operations.md#recovery) and [quota limits](storage-quotas.md).

Changed-component counters measure localized publication work. Scanning, manifest verification and complete ownership checks still grow with corpus size. Large batches have explicit admission limits; no constant total update latency, complete language understanding, power-loss durability or cross-platform qualification is implied.

## Local usage and reporting boundary

[Local usage reporting](usage-reporting.md) uses core contracts, a bounded transactional store ledger and kernel estimation/report services. CLI/MCP serialization hooks record one observation per outer request. Aggregate counters are the default, scoped by collection/root binding without query/source content or external telemetry; diagnostic events require opt-in. The CLI exports JSON/CSV/Markdown and the extension displays the selected collection's report. Signed context estimates identify their baseline and response framing. Historical recording loss, provider consumption and billed savings remain unobserved; nonempty reports are labeled partial.

Version 1.1.3 adds one bounded cumulative native aggregate in the usage ledger, atomically updated with daily recording and cleared under the same lifecycle lock. Explicit session imports can associate bounded retained request IDs without retaining session names or query content. The extension renders those native reports and supplies guarded report controls. Content-free client-counter conversion and matched-task evaluation live in contributor scripts outside the kernel; no agent execution or external service is introduced. See [usage evaluation](usage-evaluation.md).

Version 1.1.4 stores per-source full-context JSON byte counts in each new immutable catalog. The `source-context-1` retrieval specification binds the additional rows to a distinct publication identity. Counts include nested MCP escaping and are reused only for unchanged sources from an authenticated previous catalog. Usage comparisons lease the exact response revision and read bounded size rows, retaining the existing component-integrity checks. Obsolete catalogs require rebuilding; absent or mismatched sources/revisions and limit failures become fixed reason counters, never live-source substitutions or fabricated savings. Conditional replies without an evidence payload are excluded from comparison.
