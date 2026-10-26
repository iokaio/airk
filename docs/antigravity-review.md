Ran command: Git status check (`git status`)
Ran command: List repo files (`git ls-files`)
Viewed Cargo.toml:1-52
Ran command: List crate directories (`Get-ChildItem -Directory crates`)
Viewed CONTRIBUTING.md:1-44
Ran command: Cargo check workspace (`cargo check --workspace --all-targets`)
Viewed architecture.md:1-76
Checked task Cargo check workspace
Ran command: List airk-core files (`Get-ChildItem -Recurse crates/airk-core`)
Viewed Cargo.toml:1-17
Viewed lib.rs:1-353
Ran command: Cargo fmt check (`cargo fmt --all --check`)
Ran command: Run cargo clippy (`cargo clippy --workspace --all-targets --locked -- -D warnings`)
Ran command: Run check-repo.ps1 (`pwsh -NoProfile -File scripts/check-repo.ps1`)
Ran command: Run python tests (`python -m unittest discover -s scripts/tests -v`)
Ran command: Cargo test workspace (`cargo test --workspace --locked`)
Viewed location.rs:1-100
Viewed binding.rs:1-100
Ran command: List airk-ingest files (`Get-ChildItem -Recurse crates/airk-ingest`)
Ran command: List airk-ingest src files (`Get-ChildItem crates/airk-ingest/src`)
Ran command: Git log recent commits (`git log -n 5 --oneline`)
Listed 1 task
Ran command: List airk-store src files (`Get-ChildItem crates/airk-store/src`)
Viewed lib.rs:1-120
Viewed sqlite.rs:1-120
Viewed sqlite.rs:121-260
Ran command: List airk-retrieval src files (`Get-ChildItem crates/airk-retrieval/src`)
Viewed lib.rs:1-127
Viewed persistent.rs:1-100
Checked task Cargo test workspace
Ran command: List airk-kernel src files (`Get-ChildItem crates/airk-kernel/src`)
Ran command: Git log kernel (`git log -n 5 crates/airk-kernel`)
Ran command: List airk-mcp files (`Get-ChildItem -Recurse crates/airk-mcp`)
Ran command: List airk-cli files (`Get-ChildItem -Recurse crates/airk-cli`)
Viewed lib.rs:1-120
Checked task Cargo test workspace
Ran command: List retrieval tests (`Get-ChildItem crates/airk-retrieval/tests`)
Ran command: List kernel tests (`Get-ChildItem crates/airk-kernel/tests`)
Viewed lib.rs:1-120
Viewed lib.rs:121-280
Checked task Cargo test workspace
Viewed lib.rs:1-140
Viewed lib.rs:141-300
Checked task Cargo test workspace
Viewed lib.rs:301-450
Viewed lib.rs:451-550
Viewed lib.rs:551-650
Viewed lib.rs:651-750
Viewed lib.rs:751-850
Viewed lib.rs:851-950
Checked task Cargo test workspace
Viewed main.rs:1-120
Checked task Cargo test workspace
Ran command: List vscode src (`Get-ChildItem packages/vscode/src`)
Ran command: Run vscode extension tests (`npm test --prefix packages/vscode`)
Ran command: Check node_modules (`Test-Path packages/vscode/node_modules`)
Ran command: Run tsc typecheck (`npx --prefix packages/vscode tsc -p packages/vscode --noEmit`)
Ran command: Git status check (`git status`)
Ran command: Git diff check (`git diff --check`)
Ran command: Run license-notices.py (`python scripts/license-notices.py --output target/notices`)
Ran command: Count unwrap in src (`pwsh -NoProfile -Command "Get-ChildItem -Path crates -Recurse -Filter '*.rs' | Where-Object { \$_.FullName -notmatch '[\\\\/]tests[\\\\/]' } | Select-String -Pattern '\.unwrap\(\)' | Measure-Object"`)
Ran command: Search unwrap in src (`git grep -n "\.unwrap()" "crates/*/src"`)
Ran command: Search expect in src (`git grep -n "\.expect(" "crates/*/src"`)
Ran command: Search expect in crates/*/src (`git grep -n "\.expect(" -- crates/*/src`)
Ran command: Check expect in airk-store (`git grep -n "\.expect(" -- crates/airk-store/src`)
Viewed quota.rs:950-975
Ran command: Check unwrap across crates (`git grep -n "\.unwrap()" -- crates/airk-core/src crates/airk-ingest/src crates/airk-store/src crates/airk-retrieval/src crates/airk-kernel/src crates/airk-mcp/src crates/airk-cli/src`)
Viewed maintenance.rs:500-520
Viewed maintenance.rs:470-505
Viewed maintenance_legacy.rs:195-215
Viewed history.rs:1115-1130
Viewed history.rs:1100-1116
Viewed lib.rs:120-250
Viewed lib.rs:251-400
Viewed lib.rs:590-650
Viewed lib.rs:720-780
Viewed lib.rs:781-860
Viewed extension.ts:1-100
Viewed native.ts:1-100
Ran command: Git status check (`git status`)

# Code Review: AIRK (Artifact Indexing & Retrieval Kernel)

## Executive Summary

AIRK demonstrates an exceptionally high standard of systems software engineering. It implements a local, bounded, read-only indexing and retrieval kernel with zero cloud/external service dependencies, strict collection and root isolation, transactional persistence via SQLite and Tantivy/DiskANN, and rich code and document extraction capabilities.

### Verification Results

All automated quality gates were executed against the codebase:
* **Cargo Format**: `cargo fmt --all --check` passed cleanly.
* **Cargo Clippy**: `cargo clippy --workspace --all-targets --locked -- -D warnings` passed with **0 warnings** on all targets.
* **Workspace Test Suite**: `cargo test --workspace --locked` completed successfully (650+ unit and integration tests passing across all crates).
* **Repository & Whitespace Checks**: `pwsh -NoProfile -File scripts/check-repo.ps1` passed for all 556 tracked files.
* **Packaging & Contract Tests**: `python -m unittest discover -s scripts/tests -v` passed all 54 tests.
* **TypeScript Extension Typecheck**: `npx --prefix packages/vscode tsc -p packages/vscode --noEmit` passed with 0 diagnostic errors.

---

## 1. Architectural Adherence & Dependency Boundaries

AIRK strictly adheres to the layered architecture outlined in [architecture.md](architecture.md):

* **[`airk-core`](../crates/airk-core/src/lib.rs)**: Strictly pure contracts, schemas, and traits (`serde`, `sha2`, `anyhow`). It remains completely free of parsers, filesystem state, network, or transport implementations.
* **[`airk-store`](../crates/airk-store/src/lib.rs)**: Manages content-addressed object storage, compact catalogs, generation metadata, SQLite WAL storage, quotas, relocation, and retention. It depends solely on `airk-core`.
* **[`airk-retrieval`](../crates/airk-retrieval/src/lib.rs)**: Encapsulates Tantivy 0.26.2 lexical indexing, DiskANN 0.59.0 disk-backed vector retrieval, local WordPiece/BERT ONNX inference, and reciprocal rank fusion. It depends solely on `airk-core`.
* **[`airk-ingest`](../crates/airk-ingest/src/lib.rs)**: Encapsulates file discovery, Tree-sitter parsers, document converters (Markdown, PDF, DOCX, XLSX, PPTX, Notebooks), and budget caps. It depends solely on `airk-core`.
* **[`airk-kernel`](../crates/airk-kernel/src/lib.rs)**: Orchestrates collection lifecycles, coordinating store, ingestion, retrieval, and maintenance.
* **[`airk-mcp`](../crates/airk-mcp/src/lib.rs)**: Implements the Model Context Protocol over standard I/O using `rmcp`. It acts strictly as a schema and transport boundary, routing all operations through [`Kernel`](../crates/airk-kernel/src/lib.rs#L47-L58).
* **[`airk-cli`](../crates/airk-cli/src/main.rs)**: Handles CLI command parsing, service control, and lifecycle operations.
* **[`packages/vscode`](../packages/vscode/src/extension.ts)**: Implements a thin helper client that respects workspace trust, schedules background indexing requests, and displays status without owning a shadow index.

Dependency directions are strictly acyclic, unidirectional, and conform to repository isolation rules.

---

## 2. Security, Isolation & Workspace Safety

### Read-Only Source Invariant & TOCTOU Defense
AIRK treats user workspaces as immutable and authoritative.
* In [`read_source_with_budget`](../crates/airk-ingest/src/lib.rs#L934-L956), files are verified via `symlink_metadata()` both **before** and **after** reading. AIRK checks that the file is not a symlink and that `after.len() == before.len()`, preventing symlink race conditions and mid-read mutations.
* Canonical workspace containment is asserted with `path.canonicalize()?.starts_with(root)`, preventing path traversal escapes.

### Path Sanitization & Bounds Checking
* In [`bind_payload`](../crates/airk-core/src/binding.rs#L17-L40), logical workspace paths are validated to reject null bytes (`\0`), backslashes (`\`), colons (`:`), leading slashes, path lengths over 4096 bytes, and relative segments (`.` or `..`).
* All string and buffer inputs across MCP and CLI enforce maximum lengths, counts, and byte budgets (e.g. 16 MiB per-file limit, 64 KiB response cap in [`validate_budget`](../crates/airk-core/src/lib.rs#L188-L195)).

### Privacy & Telemetry
AIRK collects zero remote telemetry. Local usage recording ([`usage.rs`](../crates/airk-kernel/src/usage.rs)) tracks only aggregate operation counters and bytes without persisting search queries or document contents.

---

## 3. Storage, Concurrency & Data Integrity

### SQLite Windows Path & URI Encoding
SQLite under Windows frequently encounters URI parser quirks when dealing with extended path prefixes (`\\?\`) and UNC drives.
* [`open_immutable_catalog`](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/sqlite.rs#L156-L169) percent-encodes every byte of the native path into the `file:` URI. This bypasses SQLite's URI parser pitfalls without mangling Windows extended paths.
* [`managed_data_path`](../crates/airk-kernel/src/lib.rs#L64-L96) resolves ancestors to locate the canonical `\\?\` extended prefix without mutating disk on read-only open calls.

### Concurrency, Leases, and Retirement
* **WAL Mode & Busy Handling**: All SQLite databases use WAL mode with explicit busy timeouts (2s) and query deadlines (250ms progress handler) in [`connect_root_once`](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/sqlite.rs#L209-L234).
* **Protocol Negotiation**: [`retry_wal_protocol`](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/sqlite.rs#L185-L203) retries reads encountering `SQLITE_PROTOCOL` during concurrent reader connection churn without risking ambiguous mutation replays.
* **Two-Phase Generation Retirement**: In [`maintenance.rs`](../crates/airk-store/src/maintenance.rs), retired index generations transition through durable SQLite-backed intent stages (`Detached` -> `PhysicalRemovalComplete` -> `IntentComplete`). Interruptions at any boundary are recoverable and idempotent.
* **Reader Leases**: Active search queries and catalog readers hold shared OS file locks on generation lease files, ensuring running MCP workers are never stripped of files during background garbage collection.

---

## 4. Ingestion, Parsers & Retrieval Engines

### Streaming & Memory Discipline
* Ingestion operates in a streaming fashion ([`scan_streaming_profiled`](../crates/airk-ingest/src/lib.rs#L302-L342)), retaining at most one extracted artifact in memory before emitting content-addressed payloads to disk.
* Chunks conform to an 8 KiB / 160-line ceiling, preserving exact UTF-8 character boundaries and CRLF endings ([`chunking.rs`](../crates/airk-ingest/src/chunking.rs)).

### Retrieval Fusion & Model Execution
* Lexical search uses Tantivy with LZ4 compression and mmap postings.
* Vector search uses DiskANN 0.59.0 for disk-resident graph traversal with bounded record allocations.
* MCP search dispatches blocking I/O to dedicated worker threads via `tokio::task::spawn_blocking` ([`airk-mcp`](../crates/airk-mcp/src/lib.rs#L355-L379)), ensuring the async JSON-RPC runtime is never blocked by disk I/O.

---

## 5. Detailed Findings & Recommendations

While the codebase is exceptionally robust, the following minor areas are surfaced for consideration:

### 1. Defensive Path Unwraps in Maintenance Routines
* **Location**: [`maintenance.rs`](../crates/airk-store/src/maintenance.rs#L508-L511), [`maintenance_legacy.rs`](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/maintenance_legacy.rs#L204).
* **Observation**: Several helper routines call `.parent().unwrap()` or `path.file_name().unwrap().to_str().unwrap()`.
* **Detail**: Although paths processed in these functions are constructed from valid internal generation directories, calling `.to_str().unwrap()` on unknown disk entries could theoretically panic if a legacy directory contains non-UTF-8 characters.
* **Recommendation**: Replace `.to_str().unwrap()` with `.to_string_lossy()` or return `.context("invalid path entry")?` to preserve the zero-panic guarantee on disk anomalies.

### 2. Lock Poisoning in Retrieval Model Cache
* **Location**: [`model.rs`](../crates/airk-retrieval/src/model.rs#L623-L626).
* **Observation**: `self.0.lock().unwrap()` is used on the embedding model cache mutex.
* **Detail**: In [`Kernel`](../crates/airk-kernel/src/lib.rs#L224-L229), mutex poisoning is gracefully handled via `.map_err(|_| anyhow::anyhow!("... poisoned"))?`. In `model.rs`, a panic on another thread could leave the lock poisoned, causing subsequent calls to panic.
* **Recommendation**: Handle poisoned locks by using `.unwrap_or_else(|poisoned| poisoned.into_inner())` or mapping the lock error to an `anyhow::Result`.

### 3. Third-Party License Notices Audit
* **Location**: Output of `python scripts/license-notices.py --output target/notices`.
* **Observation**: Notice bundling completed for 466 packages, with 4 notices flagged:
  * `adobe-cmap-parser`
  * `htmlescape`
  * `pdf-extract`
  * `type1-encoding-parser`
* **Recommendation**: Ensure the license text files for these 4 packages are verified in `third_party/licenses/` before cutting the next formal release distribution.

---

## Conclusion

The AIRK codebase is in outstanding shape: clean architecture, meticulous adherence to core constraints, comprehensive test coverage (650+ tests passing), robust cross-platform path handling, and total respect for workspace safety. No files were modified during this review.
