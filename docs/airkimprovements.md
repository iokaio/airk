# AIRK code review and implementation plan

Review date: October 18, 2026. Scope: the current 1.1.4 working tree, including existing modified and untracked source files, based on Git commit `836688b4c68e31ee4074d3f218576b7549d385c5`. The findings below preserve the original review evidence. The implementation record tracks the subsequent authorized 1.1.4 changes.

## Implementation record - 1.1.4

The user authorized implementation, validation, documentation, PR merge and publication of 1.1.4. The existing version bump is preserved. The original findings and review-time measurements below remain historical evidence; source line anchors refer to that reviewed tree.

| Finding | Implemented change | Validation / remaining boundary |
|---|---|---|
| F01 | Warm reads authenticate every generation component and shared model assets; hybrid authenticates once. | Cold/warm corruption regressions, including whole-record swaps; no timestamp trust. |
| F02 / F03 | Private candidates retain immutable chunk ordinals; fusion precedes final text materialization and byte budgets. | Padding invariance and multiple chunks on one source line. Existing persisted indexes remain compatible. |
| F04 | Sparse scoped vector queries compare bounded matching disk records directly through the DiskANN adapter. | Alias/sparse-prefix/tie tests; 4,096 scoped candidates maximum with explicit truncation on exhaustion. |
| F05 | Profile writes use bounded handle reads, no-follow/identity checks and hardlink rejection. | Hardlink, oversized, redirected-ancestor and deterministic replacement fixtures. |
| F06 | Full request lifecycle runs on bounded blocking workers; reserved control capacity, explicit overload and shutdown drain. | Admission, cancellation, exact serialized bytes and once-only success/failure/panic accounting. See [execution](request-execution.md). |
| F07 | Usage comparisons authenticate the receipt-bound catalog projection without rehashing unused graph/model components. | Catalog/sidecar tamper tests and logical-byte/time measurements. Whole-catalog hashing remains; no unsafe cross-request cache or claimed full context reuse. |
| F08 | Revision/filter/representation-bound keyset cursors and compact outlines across CLI/MCP. | Enumeration, duplicates, tiny budgets, oversized records, tampering, changed publication, retained/expired history and conditional validators. See [discovery](discovery.md). |
| F09 / F10 | PDF worker isolation with preflight and platform resource controls; protected source traversal; suspended Windows provider startup. | Adversarial parser/source/process fixtures. See [ingest hardening](ingest-hardening.md) for exact platform assumptions and limits. |
| F11 | Private retrieval candidate, SQLite query/usage/publication, kernel discovery, MCP execution, extension view/tree/setup modules. | Public command identities and legacy response defaults preserved; existing suites check behavior. Broader history/maintenance restructuring is deferred to avoid unrelated churn. |
| F12 | Binary/source-bound persistent retrieval qualification receipts run in all native CI jobs. | Fixed judgments, latency, process counters, usage modes and worker concurrency. Real models, installed-editor lifecycle, physical full-disk/power-loss and production relevance need separate evidence; publication does not certify them. |

Final local checks and hosted release provenance are recorded in [1.1.4 release notes](release-1.1.4.md). Original review probes have become regression tests; qualification boundaries remain explicit.

## Original review assessment

The highest-priority finding is inconsistent integrity validation between warm and fresh retrieval handles. An isolated experiment returned different vector results under the same publication token after graph records changed; a fresh process correctly rejected the same graph. Fix that before further query-cache optimization.

Four additional isolated experiments reproduced hybrid candidate loss, hybrid collapse of distinct chunks, path-filter starvation in vector search, and unintended writes through hard-linked shell profiles. The automated suites at review time did not cover these particular combinations. Broader improvements should address synchronous usage work in MCP handlers, repeated whole-generation verification, discoverability under response limits, parser resource isolation, and release qualification.

The seven-crate dependency direction, explicit syntax/producer provenance, original document coordinates, collection binding, publication receipts, reader leases, conservative retention, and local-only model provisioning are sound design choices to preserve. The recommendations below keep DiskANN as the disk-backed vector engine and place changes inside the existing owning components.

## Scope and evidence

This was a repository-wide review with targeted deep reads and synthetic behavioral checks, not a claim that every line or every language adapter was independently verified. Discovery covered the native crates, extension, contributor scripts, CI configuration, architecture, operational contracts, and tests. No existing source changes were reverted, and no public issue, PR, release, or deployment was created.

AIRK MCP supplied inventories, outlines, exact definition locations, lexical searches, bounded source reads, and change/freshness observations. The indexed evidence used this exact revision:

```text
airk-r1:b261827304de8a26854c8953dd744bdf:50:d9e0a92ceefbd7e8864fdb892d8bd4e82579b55348577c3cdb20d26df438a7e2
```

`airk_changes` reported idle scheduling and `live_source_verified: false`. Its independent continuity coverage reported file/ancestry overflow after generation 33, resolution overflow after 28, and code/document unsupported coverage after 50. These are limits on change evidence, not proof that current files are absent. Selector checks returned no continuation cursor. Broad file/outline results were truncated; focused prefixes and symbol queries were used, with direct live reads for complete relevant functions, verification, and tests.

Live SHA-256 values matched AIRK citations for the inspected retrieval, kernel, usage, SQLite, ingestion, MCP, CLI-installation, and model-setup files. Selected reproducibility anchors:

| Evidence | Source SHA-256 / read validator |
|---|---|
| [persistent.rs](https://github.com/iokaio/airk/blob/f9aa5168829dc8794d241644fb8483715c2dd58a/crates/airk-retrieval/src/persistent.rs), read lines 1146–1291 | Source `0f1f18f255e05ed68ea86aaf5320482317d39a88544af590fbfaebb6cf71344e`; validator `d2f27c0732091665688f42106e1f6097512c8e2d655ff17418062dd8176d5b6e` |
| [kernel lib.rs](../crates/airk-kernel/src/lib.rs), read lines 661–824 | Source `9d185281f08c5ac8b3c05f4857e29eb3bfba1e2950ccaf4a44783699379b034c`; validator `1959dfdcce9fd662d5a7e297aa0e6cc28817395e36b39dfc7cf5880bc8443476` |
| [sqlite.rs](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/sqlite.rs), read lines 288–363 | Source `fd12c33153fb9411de58343a38db9d9ee44880190b1c0b48792834dca7bb34d5`; validator `20776cf1866c9f9002945b6e705776d87690a6be4111c19f94dd6c92ca4320a3` |

Line numbers below describe this working tree and may move after implementation. All corruption experiments used newly generated fictional fixtures in isolated temporary directories, with usage recording disabled to isolate retrieval behavior; existing workspace/index data was not modified.

## Prioritized findings

P1 means fix before the next candidate release. P2 means correctness, reliability, or scale work to schedule next. P3 means maintainability work after the behavior fixes. **Reproduced** denotes an observed result from this review; **code-backed improvement** denotes an implementation limitation or risk without a demonstrated production incident.

### F01 — P1: Bind warm vector reads to the authenticated generation

**Reproduced.** [Kernel cache reuse](../crates/airk-kernel/src/lib.rs#L729) returns an existing `PersistentIndex` when the header identity matches. [Warm verification](https://github.com/iokaio/airk/blob/f9aa5168829dc8794d241644fb8483715c2dd58a/crates/airk-retrieval/src/persistent.rs#L1295) authenticates the manifest, while [vector search](https://github.com/iokaio/airk/blob/f9aa5168829dc8794d241644fb8483715c2dd58a/crates/airk-retrieval/src/persistent.rs#L1223) reopens `vectors.bin`. It does not rebind those newly opened bytes to the manifest's component digest. [Per-record checksums](../crates/airk-retrieval/src/vector.rs#L309) detect damaged records but do not bind a valid record to its original slot or publication.

Experiment: index `a.txt = authenticate` and `b.txt = banana` with an original two-dimensional word-vector model; warm an MCP vector query for `login`; exchange complete graph records 1 and 2 without changing their valid checksums or the file length; repeat in the same connection. The top result changed from `a.txt` to `b.txt`, `isError` remained false, and the publication token was identical. A fresh CLI search exited 1 with `v2 generation component checksum mismatch`.

**Impact:** a warm connection can associate results with a revision whose authenticated graph it is no longer reading. This is a local state-integrity failure, not evidence of remote access or a separate-user security boundary. Usage recording must not serve as a substitute for retrieval validation: its errors deliberately do not alter successful tool results.

**Implementation:** first make warm readers reject changed component bytes using receipt-bound verification. Then evaluate an authenticated record/chunk scheme or retained verified handles if full verification is too expensive. Any record scheme must bind slot identity and content to the committed receipt; an unanchored checksum is insufficient. Audit cached lexical/model/catalog paths for the same fresh-versus-warm discrepancy without assuming they have the identical failure.

**Acceptance:** warm and fresh readers both reject record transplants, same-size replacements, changed model references, unexpected sidecars, and incompatible headers. Exercise lexical, vector, and hybrid calls, with usage enabled and disabled, across concurrent publication/pruning. Preserve valid pinned-generation reads.

### F02 — P2: Fuse candidate identities before applying text budgets

**Reproduced.** [Hybrid search](https://github.com/iokaio/airk/blob/f9aa5168829dc8794d241644fb8483715c2dd58a/crates/airk-retrieval/src/persistent.rs#L1252) asks each public branch for up to 100 hits but caps each branch at 65,536 text bytes before reciprocal-rank fusion. Eight 8 KiB chunks can exhaust a branch before a better combined candidate reaches the merger.

Experiment: eight long lexical matches, eight long vector-only matches, a short document matching both branches, and 100 lower-ranked vector distractors. Removing only trailing spaces from the long files changed the hybrid top result from `a0.txt` to `m.txt`. Lexical terms and normalized word vectors were unchanged. Response payload length was changing the candidate set used for ranking.

**Implementation:** add private bounded candidate APIs returning chunk identity, address, and rank/score without text. Fuse those candidates, then materialize and truncate the final hits once. Keep explicit limits on candidate count, work, and materialized bytes; preserve deterministic ties and the same-generation lease.

**Acceptance:** padding that preserves terms, embeddings, and chunk boundaries does not change ranking. Test candidates beyond the first 64 KiB, multibyte truncation, ties, and final response limits. Tiny-corpus tests alone are insufficient.

### F03 — P2: Do not identify chunks solely by path and line range

**Reproduced.** The [hybrid merge key](https://github.com/iokaio/airk/blob/f9aa5168829dc8794d241644fb8483715c2dd58a/crates/airk-retrieval/src/persistent.rs#L1263) is `(path, start_line, end_line)`. [Byte-bounded chunking](../crates/airk-ingest/src/chunking.rs#L50) can create multiple chunks on the same source line, so that tuple is not unique.

Experiment: one text file containing `needle FIRST`, padded to 8,192 bytes, followed by `needle SECOND`, without a newline. Lexical search returned two hits. Hybrid search returned one, omitted `SECOND`, and reported `truncated: false`. The merger also adds scores from distinct chunks as if they were duplicate retrievals of one chunk.

**Impact:** long/minified source lines can lose evidence and receive inflated fusion scores without an omission signal.

**Implementation:** carry a generation-local chunk ordinal or exact byte-span identity through both candidate providers. Merge only the same chunk returned by different branches. Preserve the original line/document citation separately. Include mapped/document units in the identity design; do not infer uniqueness from a display coordinate.

**Acceptance:** multi-chunk single lines, repeated text, mapped regions, and document coordinates retain distinct hits. A chunk returned by both branches merges exactly once. Define compatibility and bump the retrieval specification if persisted representation changes.

### F04 — P2: Improve vector recall inside a path prefix

**Reproduced limitation.** [Vector search](https://github.com/iokaio/airk/blob/f9aa5168829dc8794d241644fb8483715c2dd58a/crates/airk-retrieval/src/persistent.rs#L1224) obtains a global candidate set of `(limit * 4).clamp(64, 1000)` and filters paths afterward. A narrow prefix can return no evidence even though it contains a searchable vector.

Experiment: 200 distinct nearby out-of-scope vectors and one more distant document under `wanted/`; querying with `mode: vector`, `path_prefix: wanted/`, and `limit: 1` returned zero hits. The response did set `truncated: true`, so it did not claim exhaustive absence.

**Implementation:** retain DiskANN and evaluate supported filter-aware traversal or bounded adaptive overfetch. For a small filtered inventory, a bounded exact comparison can be considered inside the existing retrieval adapter. Report budget exhaustion distinctly from a fully searched empty scope; do not silently turn a vector request into lexical search.

**Acceptance:** sparse prefixes, aliases, empty scopes, and scopes behind more than 64/400 out-of-scope neighbors have measured recall and explicit work ceilings. Keep the approximate-retrieval limitations honest when the ceiling is reached.

### F05 — P2: Refuse hard-linked shell profiles during CLI setup

**Reproduced.** [appendProfile](../packages/vscode/src/cliInstall.ts#L43) rejects symlinks and non-files but accepts regular files with multiple hard links, then appends through the path. The existing [agent-config installer](../packages/vscode/src/agentConfig.ts#L64) already rejects `nlink !== 1`.

Experiment: hard-link a temporary `.profile` to a separate temporary text file and invoke the exported `configureShellPath` with an isolated home and a POSIX-style CLI directory. It returned no errors and appended the AIRK PATH block to the other file. This exercised the function on Windows; it does not qualify an installed Linux/macOS shell workflow.

**Implementation:** reject multiply linked profiles, bound profile reads, and validate the opened file identity before writing. Review redirected ancestors and the check/open race as part of the same helper. Preserve the documented append-only behavior and report manual setup instructions for refused files.

**Acceptance:** hard-linked and symlinked profiles, redirected directories, concurrent replacement, oversized files, and failed writes leave unrelated content untouched. Test ordinary Bash/Zsh/fish profiles for idempotency and existing login-profile precedence on their native platforms.

### F06 — P2: Keep usage I/O off async MCP executor threads

**Code-backed improvement.** MCP handlers move retrieval into `spawn_blocking`, but [usage-span setup](../crates/airk-mcp/src/lib.rs#L351) runs before that dispatch and [render](../crates/airk-mcp/src/lib.rs#L809) calls `finish_usage` synchronously afterward. [Usage setup/finalization](../crates/airk-kernel/src/usage.rs#L169) opens state, validates binding, calculates baselines, and writes SQLite. The [baseline path](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/sqlite.rs#L1785) can authenticate a generation before reading compact size rows.

**Impact:** slow storage and usage-lock contention can occupy runtime threads and delay unrelated protocol requests. `spawn_blocking` around retrieval alone does not isolate the whole blocking operation. This review did not measure executor starvation under sustained load.

**Implementation:** move span preparation, serialization accounting, baseline work, and recording into an explicit bounded blocking execution path. Add shared admission limits for expensive work and define cancellation/shutdown behavior. Preserve exactly one observation per outer request, truthful dropped-record counters, and failure independence. Do not introduce an unbounded telemetry queue.

**Acceptance:** a slow/contended usage store cannot starve cheap status/capability calls or protocol cancellation; sustained load has bounded outstanding work and memory. Confirm request IDs, serialized byte counts, failure records, and shutdown flushing/drop reporting.

### F07 — P2: Measure and reduce repeated integrity-verification work safely

**Code-backed improvement.** [Exact-fact transactions](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/sqlite.rs#L292) call `verify_v2_for_query`. [That verifier](../crates/airk-store/src/blobs.rs#L568) rehashes component bytes even on a semantic-validation cache hit. [Usage size lookup](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/sqlite.rs#L1840) invokes the same verification again. Model-bearing generations are excluded from the [semantic-validation cache](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/verified_cache.rs#L35), retaining additional full validation work.

**Impact:** a small result can require work proportional to generation/model size, potentially more than once per outer request. The 250 ms SQL progress deadline starts after verification and does not bound those reads. Compact source-size rows improve baseline payload work but do not remove component authentication costs.

**Implementation:** instrument verification bytes/time and distinguish retrieval, baseline, and ledger costs. Share a request-scoped authenticated read context and its lease where safe. Longer-term authenticated blocks/records may reduce whole-file reads. Do not replace digest verification with mtime/size trust or weaken the F01 fix. Keep storage and retrieval dependency direction intact through neutral contracts in core where needed.

**Acceptance:** cold/warm p50/p95, logical reads, available OS counters, and RSS are measured on small, medium, and existing scale fixtures, with usage on/off and multiple workers. Tamper tests still pass. Report logical reads separately from physical I/O and do not establish performance targets from unmeasured assumptions.

### F08 — P2: Add resumable inventory/fact discovery and compact outlines

**Code-backed and observed API limitation.** [File listing](../crates/airk-kernel/src/lib.rs#L843) always returns the first bounded matching paths; [MCP list/outline arguments](../crates/airk-mcp/src/lib.rs#L137) have no continuation selector. Fact responses can consume most of their budget on rich metadata. During this review, broad outlines of large modules contained only early imports/declarations before truncation.

**Impact:** agents must guess narrower prefixes or switch to direct filesystem discovery. A file with many definitions cannot be fully traversed through its outline alone. Truncation is disclosed, but it is not resumable.

**Implementation:** add deterministic keyset cursors tied to exact revision, tool, filters, and relevant representation options. Reuse the principles of the existing changes/history cursor contracts. Offer a compact outline representation with names, kinds, spans, and durable keys; retain rich details through code-fact reads. Budget serialized record payloads explicitly and disclose framing overhead.

**Acceptance:** enumerate a flat directory with more than 100 files and a file with more than 100 declarations without missing or duplicating entries. Test concurrent publication, expired history, changed filters, tiny budgets, aliases, and oversized first records. Preserve existing callers and conditional-validator semantics.

### F09 — P2: Put resource limits around parser execution, not only its output

**Documented hardening gap.** [PDF extraction](../crates/airk-ingest/src/documents.rs#L71) calls the in-process extractor before checking page count and before `from_blocks` checks aggregate text. Input/output ceilings therefore do not establish a peak-allocation or hard time bound. [SECURITY.md](../SECURITY.md) already acknowledges this; it is not a newly established exploit.

**Implementation:** add bounded preflight checks where the parser supports them and evaluate a local child-process extraction mode for risky document formats, owned by ingest with lifecycle dispatch in the existing CLI. Apply platform memory/time/process limits and preserve structured failure reporting. Start with PDF and measured adversarial fixtures rather than replacing every parser. Keep source mounts/read handles read-only and do not add remote extraction.

**Acceptance:** high-expansion, excessive-page, malformed, deeply nested, and hanging fixtures stop within measured resource limits, terminate descendants, and leave publication/source contracts intact. Distinguish per-file unsupported extraction from pass-wide capacity failure. Qualify the enforcement separately on Windows, Linux, and macOS.

### F10 — P2: Strengthen filesystem and subprocess race boundaries

**Documented hardening gap.** [Source reads](../crates/airk-ingest/src/lib.rs#L988) validate pathname metadata/canonicalization before and after `File::open`, but do not establish identity through the opened handle and protected traversal. A replace-and-restore race is outside those checks. [Windows provider startup](../crates/airk-ingest/src/provider_process.rs#L83) starts the process before attaching its job object, leaving a startup interval before descendant containment. Neither race was reproduced here; concurrent hostile filesystem mutation is explicitly unqualified.

**Implementation:** define the supported race model, then use platform handle-relative/no-follow opening and verify file identity through the handle. For Windows providers, investigate suspended creation, job assignment, and resume, with complete cleanup on failure. Keep platform adapters in ingest and preserve the existing local trusted-provider boundary.

**Acceptance:** deterministic hook-based replacement tests cannot read outside an authorized source root; normal edits either produce coherent indexed bytes or a retryable scan failure. A provider that immediately spawns a child is fully terminated on timeout/startup failure. Do not claim that this sandboxes arbitrary trusted providers.

### F11 — P3: Split large modules along existing responsibilities

**Maintainability improvement.** The reviewed tree has 250 Rust files, 37 extension TypeScript files, and 53 Python scripts/tests. Large modules include [SQLite storage](https://github.com/iokaio/airk/blob/25dfeb56f4a87e3c8f598371859e1d42e3ff3c10/crates/airk-store/src/sqlite.rs) at 2,640 lines, [history](../crates/airk-store/src/history.rs) at 2,271, [maintenance](../crates/airk-store/src/maintenance.rs) at 1,972, and the [extension controller](../packages/vscode/src/extension.ts) at 1,123. Line count is a navigation signal, not proof of defects.

**Implementation:** extract SQLite publication, query, and usage-size operations into private modules; separate extension collection lifecycle, setup, and dashboard commands. Consolidate repeated request validation/dispatch only where doing so preserves tool-specific contracts. Keep public APIs stable during the split and do not add crates without an independent dependency purpose.

**Acceptance:** behavior-preserving PRs have focused responsibility boundaries, unchanged serialized contracts, existing tests passing, and no unrelated formatting or feature changes. Extract shared helpers only after F01–F08 establish the needed contracts.

### F12 — P2: Turn qualification gaps into explicit release evidence

**Existing qualification gap.** Cross-platform CI is configured, but this review ran locally on Windows. Several real-model tests require explicitly provisioned assets and are ignored by default. Existing [validation](validation.md), [scale evidence](scale-qualification.md), and [roadmap](roadmap.md) correctly distinguish historical results from unqualified workloads. Notice generation still reports four upstream standalone legal-text review items.

**Implementation:** require binary/source-revision-bound receipts for the targeted warm-reader, ranking, sparse-filter, native shell, model, and fault scenarios. Add a held-out retrieval set with code, prose, long/minified files, aliases, multilingual text, and sparse path filters. Record recall/ranking metrics, latency distributions, RSS, and failures. Complete installed extension/helper upgrade/rollback, service lifecycle, and durability campaigns before making broader claims. Resolve the reported notice items through the existing release-review process.

**Acceptance:** each claimed platform/workload has an identified passing artifact; unavailable model/provider/platform checks remain visibly unqualified. No production semantic-quality, larger-than-memory, physical-I/O, power-loss, or causal token-savings claim follows from the synthetic fixtures in this review.

## Component coverage

| Area | Review focus | Result / follow-up |
|---|---|---|
| Core | Budgets, identity, citations, mapped locations, model/usage contracts | Preserve pure contracts; extend chunk/cursor identities deliberately (F03/F08). Existing adversarial tests remain important. |
| Ingest | Discovery/read limits, extraction selection, chunking, documents, local provider transport | F03, F09, F10. Individual grammar profiles were sampled through the suite, not exhaustively language-qualified. |
| Store | Receipt/pointer transactions, immutable catalogs, quotas, leases, retention, relocation, model objects, usage | Preserve conservative publication and deletion checks. F01/F07/F11; crash/power-loss qualification remains separate. |
| Retrieval | Tantivy candidate selection, DiskANN records/search, model readiness, embedding batches, hybrid fusion | F01–F04 are the principal behavior findings; real-model throughput/relevance remains a qualification task. |
| Kernel | Cache/pin lifecycle, publication, scheduling, changes/history, usage | F01/F06–F08. Existing scheduler watermarks, contention deferral, alias binding, and exact-history contracts should stay intact. |
| MCP / CLI | Schemas, bounded tool arguments, async dispatch, local tool console | F06/F08. The console reuses MCP handlers rather than duplicating tool behavior; retain that boundary. |
| VS Code | Trust gates, helper preparation, write queues, scheduling, model setup, CLI/profile installation | F05/F11/F12. Existing trust, argument-array execution, digest-pinned model download, and isolated config tests are useful coverage. |
| Scripts / CI / docs | Required checks, change-based CI plan, packaging contracts, notices, qualification claims | F12. No new remote dependency or release automation is needed to fix the reproduced issues. |

## Implementation sequence

Each step should be a reviewable PR or a small sequence of PRs. Estimates below describe scope, not elapsed-time promises.

| Step | Scope and owner | Dependencies | Completion gate |
|---|---|---|---|
| 1. Integrity regression and fix | F01; retrieval/kernel, store support as needed; medium | None | Persistent MCP fixture reproduces the current failure and passes after the fix; cold/warm, usage on/off, reader/prune tests pass. |
| 2. Candidate representation and fusion | F02/F03; retrieval, core only for a justified contract; medium | Agree chunk identity/versioning first | Private candidate APIs; byte budgets applied after fusion; padding and single-line regressions pass; old-generation compatibility tested. |
| 3. Profile-write protection | F05; extension; small | None; can be implemented independently of steps 1–2 | Isolated hard-link/race fixtures and native shell idempotency tests; existing profile contents preserved. |
| 4. Scoped vector retrieval | F04; retrieval; medium | Prefer candidate API from step 2 | Sparse-prefix fixture plus recall/work-budget matrix; explicit exhaustion reporting, no silent fallback. |
| 5. Request execution and verification cost | F06/F07; MCP/kernel/store/retrieval; medium to large | F01 integrity contract; baseline measurements before optimization | Bounded blocking work, request-scoped verification reuse, contention/cancellation tests, before/after measured costs, unchanged integrity guarantees. |
| 6. Resumable discovery | F08; core/kernel/store/MCP/CLI; medium | Explicit cursor/representation contract | Full enumeration, cursor expiry/filter-binding tests, compact outlines, backward compatibility and updated tool docs. |
| 7. Parser/process/source hardening | F09/F10; ingest with existing CLI support; large | Threat-model and platform design review | Adversarial resource/race fixtures on each supported OS; truthful failure classification and unchanged sources. |
| 8. Refactoring and release qualification | F11/F12; owning components and contributor scripts; ongoing | Behavior fixes land separately; qualification accompanies each applicable step | Smaller modules without contract drift; current platform/model/scale receipts; notice-review disposition; release gates satisfied. |

For changes to collection identity, persisted index formats, MCP contracts, or default dependencies, follow CONTRIBUTING's design-discussion requirement before implementation. This review does not authorize public posting. Preserve collection/root isolation, read-only sources, exact revision identity, original document locations, quotas, and explicit failure reporting in every step.

Before a feature/bug-fix PR is created, remind the human contributor in Codex or Claude Code to manually increment the patch/build version and synchronize native/extension manifests and lockfiles. This working tree already contains 1.1.4; do not bump it again for the same PR. Do not invent DCO sign-offs. If contributor guidance changes, keep AGENTS.md and CLAUDE.md byte-identical.

## Validation performed for this review

Environment: Windows; Rust/Cargo 1.98.0; Node 24.21.0; Python 3.13.7. Node differs from the documented 24.15.0 development pin, so this is not a clean pinned-environment qualification. Existing installed dependencies were used.

| Check | Result |
|---|---|
| `cargo build --workspace --locked` | Passed. |
| `cargo fmt --all --check` | Passed. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed. |
| `cargo test --workspace --locked` | Passed, including doc-tests; zero failures. Asset-dependent and designated subprocess-fixture tests retain their declared ignores. |
| `npm test --prefix packages/vscode` | Passed: 81 tests, 78 passed, 3 skipped. Includes TypeScript compilation. |
| `python -m unittest discover -s scripts/tests -v` | Passed: 116 tests, 4 skipped. |
| `pwsh -NoProfile -File scripts/check-repo.ps1` | Passed after report creation for 643 public files, including this document's links and contributor-guidance equality. |
| `python scripts/license-notices.py --output target/notices` | Passed for 467 packages; review notices for `adobe-cmap-parser`, `htmlescape`, `pdf-extract`, and `type1-encoding-parser` remain. |
| `git diff --check` | Passed after report creation; the new untracked document also received a direct trailing-whitespace check. |
| Isolated review experiments | Reproduced F01–F05, including two separate hybrid failures. These are review probes, not yet committed regression tests. |

Cargo was initially absent from PATH; checks passed after adding the existing `C:\Users\tdotn\.cargo\bin` for the command process. Git required a command-scoped `safe.directory` exception because the sandbox account differs from the checkout owner; no ownership, author identity, or global Git configuration was changed. Git emitted inaccessible global-ignore and CRLF-normalization warnings; the checks above still succeeded.

Not run here: Linux/macOS builds, Docker build/smoke, hosted CI, installed-editor/service lifecycle, real external providers, explicitly provisioned ONNX qualification, large-corpus benchmarks, and physical full-disk/power-loss campaigns. Existing historical receipts were not treated as fresh test results. The five synthetic reproductions establish their specific behaviors only.

Final Git-status comparison showed `docs/airkimprovements.md` as the only added status entry and no removed entries relative to the starting working tree. That was the review-only result; subsequent implementation is tracked at the top of this document.
