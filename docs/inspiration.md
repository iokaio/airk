# Inspiration and independent implementation

The initial design review used these local public repository revisions:

| Project | Reviewed revision | Useful ideas and inspected surfaces |
|---|---|---|
| [Munarium](https://github.com/iokaio/munarium) | `bb18085f3d407e9b77d551fbf7ac68a6572fce27` | `munarium-extract` explicit extraction method/status and page spans; `munarium-core/retrieval.rs` source path/hash provenance; `munarium-datastore` independently usable file artifacts, lexical/vector adapters and versioning |
| [VCP](https://github.com/iokaio/vcp) | `2d252594cbf082f9011e2f9db3cd75d0d867f19b` | `vcp-memory` search records/tokenizer/chunks; generation publication and recovery design; local embedding adapter; bounded source/context retrieval |

AIRK does not depend on, vendor, copy or link either project's implementation. Their server, agent, policy, provider, database and orchestration stacks are not part of AIRK. The workspace must build independently without sibling checkouts.

The local `temp/` example was also reviewed as inspiration. Its useful workflow is workspace discovery, incremental updates, a status tool and MCP retrieval returning cited passages for a coding agent to reason over. AIRK implements that workflow independently, with broader text-file eligibility and an integrated binary rather than copying its scripts or configuration. `temp/` is private reference material: excluded from Git, Docker and public examples.

The ease-of-use direction is one native executable across Windows/macOS/Linux or one container host per user/machine, `airk setup` for client-specific configuration, `airk index` for updates and collection-scoped read-only MCP tools. A native VS Code extension is explored separately. Published binaries/image tags will be documented only once releases actually exist.

DiskANN with actual disk-backed graph/vector access is the selected final vector tier. The inspected Munarium DiskANN provider operates in memory and reconstructs from persisted artifacts. Borrow its separation, provenance and verification ideas; do not mistake persistence alone for large-than-memory search. See [the collection/index design](collections.md).

This is a design inspiration record, not a third-party source-import inventory or endorsement. Future source reuse requires explicit approval, exact revisions, license review and preserved notices before import.

[The code-indexing review](code-indexing-options.md) evaluates Tree-sitter/SCIP/Ctags/ast-grep/Zoekt/Atlas/StakGraph recommendations against primary documentation. The authorized [implementation slice](code-facts-and-relationships.md) adds independently written extraction/rules, typed relationships, embedded SQLite and an optional SCIP adapter using official format bindings. External code-graph engines remain design references; their implementation was not copied, and no complete external producer/performance qualification is implied.
