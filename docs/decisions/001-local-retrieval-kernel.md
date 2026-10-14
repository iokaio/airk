# ADR 001: focused local retrieval kernel

Status: accepted for the scaffold and intended architecture.

## Context

Coding agents need narrow code/document evidence with low setup overhead. Reference projects demonstrate useful indexing, provenance and local retrieval patterns, but their larger product stacks are outside AIRK's scope.

## Decision

Implement an independent Rust kernel with a CLI and read-only stdio MCP. Use memory caches and user-controlled local disk. Source-specific extraction produces facts and original locations. Generic UTF-8 fallback retains unsupported-language coverage. Do not import Munarium, VCP or temp/ implementations.

Use segregated collections selected by workspace MCP configuration. Container deployment has one host per user/machine and one persistent data volume. Native deployment targets Windows, macOS and Linux. Explore a thin VS Code extension for native install/setup/scheduling rather than another retrieval implementation.

DiskANN with an actual disk provider is the selected vector design; preserve persistent lexical and exact-fact retrieval alongside it. AIRK now supplies a portable disk provider and local static word-vector inference from verified user-provisioned assets. Embedded SQLite owns source revisions, exact facts and relationships, with legacy JSON migration; persistent Tantivy owns lexical retrieval for published disk generations. [Code storage contracts](../code-facts-and-relationships.md) and [local models](../local-models.md) describe the implementation and current limits.

## Consequences

Clients need no Python bridge, model API key or database service. Source files stay authoritative and derived state is removable/rebuildable per collection. Native and container delivery require their own qualification. Disk-backed behavior, cross-platform provider APIs, large-repository performance and semantic quality must be demonstrated before release claims.
