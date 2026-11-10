# Architecture

AIRK is an independent local indexing and retrieval kernel. Source workspaces
are authoritative and all index state is derived from them. AIRK does not
reason, generate answers, manage agents, route model calls or require an
external database or cloud provider.

This page describes current boundaries: which crate owns what, which way
dependencies point, and which component contracts govern each concern. It
does not record status, qualification or history. The phase log this page
used to hold (through 9 November 2026) is kept in
[docs/history/architecture-log-2026-11-09.md](history/architecture-log-2026-11-09.md).
Release state and qualification evidence are in the
[documentation index](README.md) and [release records](releases/).

## Dependency boundaries

| Crate | Responsibility | AIRK dependencies |
|---|---|---|
| airk-core | Artifact, query, fact and citation contracts; transport-neutral service, catalog and read contracts; storage and embedding traits | None |
| airk-process | Native owned-process containment and confirmed descendant cleanup; no indexing or service policy | None |
| airk-ingest | Ignore-aware discovery, hashing, native grammars, document extraction and chunking | core, process |
| airk-store | Content-addressed sources and extractions; authenticated SQLite catalogs, FTS5, postings and graph records; the service registry and index catalog; usage ledgers, durable intents, locks, quotas, relocation and current-object cleanup | core |
| airk-retrieval | Local ONNX and word-vector inference, the embedded qualified model catalog, mutable DiskANN algorithms and retrieval primitives | core |
| airk-kernel | Collection-scoped publication, retrieval and rank fusion, response budgets, coverage, scheduling, usage and maintenance services, and the Workbench plan runner | core, ingest, store, retrieval |
| airk-mcp | MCP schemas, the official SDK transport, canonical read handlers and result conversion | core, kernel |
| airk-cli | Commands, the native service and its workers, local HTTPS ingress, installation and lifecycle, and process and IPC adapters | core, process, kernel, mcp, store, retrieval |

Rules:

- Core stays free of parser, filesystem, inference and transport
  implementations. Adapters call kernel services rather than bypassing
  validation or collection scope.
- The crate edges above are the allowed set. `scripts/dev/boundary_check.py`
  enforces them, together with module honesty and a ratchet on module
  cycles, against the reviewed `scripts/dev/boundary-exceptions.json`. A new
  edge or crate is a reviewed change against this page and
  [simplify-plan.md](simplify-plan.md) §4.1.
- Shared contracts live in Core only where they cross owners. Contract
  changes land as their own step, before the component work that consumes
  them.
- Scope stays local: file discovery, native extraction, indexes and bounded
  retrieval. No agent reasoning, orchestration layer, gateway process,
  external database or cloud provider.
- DiskANN is the selected disk-backed vector engine.
- Components with a `CONTRACT.md` (currently
  `crates/airk-cli/src/install_files/`,
  `crates/airk-cli/src/install_layout/` and
  `crates/airk-cli/src/native_command/`) state their interface, owned state,
  invariants and test command there.

## Runtime topology

One per-user AIRK service hosts independent collections. Workspace MCP
configuration selects which collection a client reads. Native operation
targets Windows, macOS and Linux. Each platform's manager launches the
sealed main executable directly.

~~~mermaid
flowchart TD
    C[VS Code, Codex and other MCP clients] -->|Authenticated loopback HTTPS / MCP| H
    T[Local developer tools] -->|Authenticated HTTPS JSON API| H
    X[CLI and extension management] -->|Owner-checked named pipe or Unix socket| M
    subgraph S[One per-user AIRK service: single executable]
        H[Bounded HTTPS listener and isolated sessions]
        H --> A[Canonical MCP read handlers and shared admission]
        A --> P[Bounded native worker connection pools]
        M[Metadata and lifecycle actor]
        E[Shared in-process MiniLM inference owner]
    end
    M -. supervises .-> W
    P --> W[Supervised index and source workers: same executable]
    W -->|Bounded embedding requests| E
    W --> D[Independent collection storage]
    W --> R[Bounded evidence and original locations]
~~~

- **Ingress.** `airk-cli` owns the loopback listener, the installation-local
  TLS identity, the protected credential ledger and native scope validation.
  `airk-mcp` owns the Streamable HTTP adapter and the canonical read handlers.
  The [local HTTPS API](local-https-api.md) exposes those same handlers to
  local programs. HTTP exposes no lifecycle or index-management commands.
- **Management.** Bootstrap and management stay on owner-checked IPC. The
  metadata actor is metadata-only and never opens project indexes. Evidence
  does not queue behind it.
- **Workers.** Only native worker roles open project kernels and stores.
  Evidence travels from workers through the service's connection pools to the
  caller. See [service processes](service-processes.md) and
  [request execution](request-execution.md).
- **Admission.** Connections share bounded native query admission. A
  disconnecting caller suppresses delivery but does not release capacity for
  accepted native work. A typed refusal before native acceptance may be
  retried once within its original budget. Uncertain executions and
  management mutations are never replayed.
- **Inference.** MiniLM inference runs inside the service through one shared,
  bounded model owner. Retrieval owns the model catalog and the ONNX
  environment lifetime. See [service embedding](service-embedding.md).

## Read parity

CLI, Workbench and MCP reads share the canonical native handlers and return
the same complete result data: status, provenance, readiness, coverage,
continuations, and error and retry details. Workbench's read operations call
the canonical MCP handlers through an in-process callback inside the outer
admitted lifetime. They do not recursively call MCP or add a scheduler.
[Read-interface parity](read-interface-parity.md) maps every read and its
inputs. [Combined reads](combined-reads.md) and the
[unified MCP endpoint](unified-mcp.md) define multi-collection and composed
reads.

## State ownership

- **Registry and catalog.** Store owns the service registry and index catalog
  under the control root: transactional source and data isolation, durable
  jobs and receipts, and catalog Genesis. See
  [service storage ownership](service-storage-ownership.md).
- **Collections.** Each collection's current store is isolated. Publication
  runs through a durable Prepared/Applying journal and a multiprocess request
  gate. Reads hold request-scoped object leases. Store owns the authenticated
  current map, FTS5 and DiskANN state, quota admission and owned cleanup.
  Kernel owns collection-scoped publication and retrieval. See
  [current index runtime](current-index-runtime.md) and
  [service publication](service-publication.md).
- **Scheduling.** Kernel owns admission and traversal, and the native CLI owns
  process supervision. See [service source watching](service-source-watching.md)
  and [service maintenance](service-maintenance.md).
- **Usage.** Usage reporting uses Core contracts, a bounded store ledger and
  kernel estimation. It keeps no query or source content and sends no external
  telemetry. See [usage reporting](usage-reporting.md).
- **Diagnostics.** CLI owns the bounded process diagnostic ring. A logging
  failure cannot block health or shutdown. See
  [service logging](service-logging.md).
- **Extension.** The extension hosts presentation, reviewed workspace
  selection and guarded request preparation. It never owns a running index
  job, and native service and index ownership is independent of extension
  disposal.

## Compatibility boundary

1.8.16 established the `airk-state/1` state contract. Every earlier
installation goes through a native clean reset and fresh indexing; old index
IDs, grants, credentials and accepted operations are never translated. CLI
owns version and state-contract admission, reviewed capture of sources and
preferences, shutdown, reset recovery and cleanup of ownership-proven legacy
artifacts. Store owns fresh namespace and catalog Genesis. Original sources
are read-only.

There is no standalone adoption and no in-place catalog upgrade. A pending
predecessor upgrade is refused with a typed error. An interrupted adoption
left by an earlier release is settled when encountered, and its pre-Ready
catalog rows are reported for a rebuild. The catalog schema is built in
layers (Genesis, authority, armed contract, policy and split layers), and
existing installations upgrade lazily through those layers. Retained schema
fixtures prove the current code still produces each layer exactly; see
[S3 schema](simplify/s3-schema.md). Later ordinary upgrades are governed by
the state contract, independent of application version. See
[upgrade and rebuild fallback](upgrade-rebuild-fallback.md) and
[service installation](service-installation.md).
