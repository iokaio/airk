# MCP recommendations from an agent-side investigation (2026-11-01)

**Status: review record with a proposed Implementation Plan appended below.** Sections 1–6 retain the original investigation and its qualification limits; the final section distinguishes verified implementation constraints from proposed work. This document records one Claude Code session that used the unified AIRK MCP server (native 1.8.2, publication generation 5325, Agent Lean index, 1,392 files, 32,747 vectors) to answer the question *"Tell me about how the code uses Session"*, then compared the results against live `rg` on the same working tree. It lists every query, what came back, what the evidence established about `Session`, and the changes that would make the tools cheaper and more accurate for coding agents. Observed behavior is stated as observed; nothing here was re-measured on another machine or platform. Follow the [MCP developer guide](../mcp-developer-guide.md) for current tool contracts and [Agent Lean planning](mcp-prioritization.md) for the efficiency goals these recommendations extend.

## 1. Summary

- AIRK answered the question completely once queries were scoped to `crates/`: six Rust `Session` definitions, all eleven Rust files that `rg -w Session crates/` finds, the `RemoteSession` trait, four resolved callers of `Session::new`, hashed excerpts for every definition, and the TypeScript session types in the extension.
- The cost was 28 MCP call attempts in seven rounds, of which 8 were rejected as busy and repeated and 20 returned results. Two `rg` commands produced the same definition list in one round. (Corrected 2026-11-01 from the query log in section 3; the first draft of this line said 22 calls and five refusals.)
- The largest gains available are in admission (queue instead of reject), identifier-aware ranking for `search` and `references`, substring matching for `symbols`/`code_facts`, struct-level caller discovery in `relationships`, and clearer tool descriptions so the client picks the right tool first time.
- `outline` is a read tool that returns a file's indexed structure. It is not an Explorer proposal; `suggest_view` is, and its `code/outline` route name is one reason the two are confused. Section 5.7 proposes description changes for both.

## 2. Method

1. Loaded the deferred `mcp__airk__*` schemas and called `status` with `response_mode=full`.
2. Ran the question four ways in parallel: `symbols`, `references`, hybrid `search`, `code_facts`, all with `name`/`query` = `Session`.
3. Scoped follow-ups with `path_prefix=crates/`, read definitions with `read_file`, tested `outline`, `relationships`, and the TypeScript side with lexical `search` and `symbols`.
4. Verified coverage with live `rg` and located the admission code paths in the repository.

All MCP responses carried the same publication token (`airk-r1:19ae18e4a4de52ae6554dac618c4c032:5325:…`), so results are combinable. `freshness.live_source_verified` was `false` throughout; live files were checked with `rg` and `sed` before anything below is asserted.

## 3. Query log

Round and call order are preserved. "Busy" rows are verbatim admission refusals; their retries appear in later rounds.

| Round | Tool | Arguments (abridged) | Outcome |
|---|---|---|---|
| 1 | `status` | `response_mode=full` | Busy: `direct_query_admission`, `retry_after_ms=25`, `query_submitted=false`. |
| 1 | `symbols` | `name=Session`, `compact=false`, `limit=20` | 9 definitions in 3 files, `truncated=true`, cursor returned. Byte budget (8192) cut the list before `limit`. |
| 1 | `references` | `name=Session`, `limit=40`, `max_bytes=16384`, full | Busy: `direct_query_setup`, `retry_after_ms=100`, `fresh_brokerage_required=true`. |
| 1 | `search` | `query="how the code uses Session"`, full | Busy: "AIRK bridge is busy; retry after outstanding requests complete." |
| 1 | `code_facts` | `name=Session`, `compact=false`, `limit=20` | Busy: `direct_query_setup`. |
| 2 | `status` | `response_mode=full` | OK. `health=ok`, `index_state=available`, discovery and semantic readiness `ready`, `files=1392`, `chunks=5211`, `indexed_vectors=32747`, version 1.8.2. |
| 3 | `references` | as round 1 | 40 hits, `truncated=true`. Order: webview TSX, Markdown docs, Python benchmark tests, a DOCX table row, prototype HTML, then Rust. Only 12 of 40 hits were Rust. |
| 3 | `search` | `query="how the code uses Session"`, hybrid, `limit=10`, full | 10 hits: `docs/workbench/design.md`, two Python scripts, `docs/explorer/*` design and research Markdown, `README.md`. Zero Rust hits. Default policy `partial_hybrid`. |
| 3 | `code_facts` | as round 1 | Busy: `direct_query_setup`. |
| 4 | `code_facts` | `name=Session`, `path_prefix=crates/`, `compact=false`, `limit=20` | 9 definitions, `truncated=true`, cursor. Same byte-budget cut as `symbols`. |
| 4 | `symbols` | `name=Session`, `path_prefix=crates/`, `limit=30`, `max_bytes=16384` (compact default) | 17 definitions across 6 files in 3,203 bytes, `truncated=false`. Complete. |
| 4 | `references` | `name=Session`, `path_prefix=crates/`, `limit=60`, `max_bytes=24576` | 52 hits in 11 files, `truncated=true` at the byte budget. Live `rg -w` counts 56 occurrences in the same 11 files. |
| 4 | `outline` | `path=crates/airk-mcp/src/response.rs`, `limit=40` | Busy: bridge refusal (fifth concurrent call). |
| 4 | `search` | `query=Session`, `mode=lexical`, `path_prefix=packages/vscode/src/`, `limit=10` | 10 hits: `workbenchContracts.ts` (`WorkbenchSession`, `WorkbenchSessionSummary`), `mcpStdioSession.ts` import sites, webview TSX, test fixtures. Useful. |
| 5 | `outline` | as round 4 | OK. 40 items: 5 imports, `GUIDANCE` const, `error_result`, `ResponseMode`, `fact_items` and call candidates, `truncated=true`, cursor. Did not reach `Session` at line 90 within the first page. |
| 5 | `read_file` | `response.rs` 80–160 | OK. Full `Session` struct, `StatusReceipt`, `receipt`/`restore`/`display`. |
| 5 | `read_file` | `embedding_client.rs` 20–70 | OK. `ProcessConnector`, `Work`, `Session` struct and `spawn`. |
| 5 | `read_file` | `embedding_pool_client.rs` 175–245 | OK. `Session`, `RemoteSession` impl, `Drop`, `PoolConnector`. CRLF line endings returned verbatim. |
| 5 | `relationships` | `symbol=syntax:crates/airk-cli/src/embedding_client.rs:1715:1823:struct_item`, incoming, full | Busy: bridge refusal. |
| 5 | `code_facts` | `name=RemoteSession`, `path_prefix=crates/` | Busy: bridge refusal (sixth concurrent call). |
| 6 | `relationships` | as round 5 | OK with **zero items**. Coverage note returned; no edge links the struct to its `Session::spawn` callers. |
| 6 | `code_facts` | as round 5 | OK. Trait at `embedding_worker.rs:99`; unrelated `struct RemoteSession` and `impl ToolSession for RemoteSession` at `tools.rs:17–26`; one call candidate. |
| 6 | `read_file` | `providers_lsp.rs` 40–75 | OK. LSP `Session` struct and `start`. |
| 6 | `read_file` | `embed_pool.rs` 40–56 | OK. Engine-side `Session { binding }` and the session map. |
| 7 | `relationships` | `symbol=Session::new`, incoming, `path_prefix=crates/` | OK. 4 `syntax_candidate` call edges from four `PoolConnector` methods in `embedding_pool_client.rs`. |
| 7 | `symbols` | `name=McpStdioSession` | OK. `class McpStdioSession` at `packages/vscode/src/mcpStdioSession.ts:29–127`. |
| 7 | `read_file` | `tests/mcp.rs` 10–40 | OK. Test harness `Session { child, input, output }`, `start` and `with_mode`. |
| 7 | `read_file` | `embedding_worker.rs` 95–105 | OK. `pub trait RemoteSession: Send + Sync { fn request(...) }`. |

Admission results by batch size: 5 calls sent, 4 refused; 3 sent, 1 refused; 5 sent, 1 refused; 6 sent, 2 refused; 4 sent, 0 refused; 4 sent, 0 refused.

Live verification commands:

```text
rg -n -e '^\s*(pub(\(crate\))?\s+)?(struct|trait|enum|type|class|interface)\s+\w*Session\b' --type rust --type ts
rg -w -c Session crates/        # 56 occurrences, 11 files
rg -n "bridge is busy|direct_query_admission|direct_query_setup" crates/airk-mcp/src crates/airk-cli/src
rg -n permits crates/airk-mcp/src/remote.rs
```

## 4. What the evidence established about `Session`

Six unrelated Rust types share the name. Each is a short-lived handle around a child process, a connection, or a receipt; none is shared across crates.

| Definition | Role | Key facts verified live |
|---|---|---|
| [response.rs:90](../../crates/airk-mcp/src/response.rs#L90) `pub(crate) struct Session` | Per-connection MCP status receipt | Holds `Mutex<StatusReceipt>` of five fixed digests. `display` strips repeated status in terse mode, keeps it for `response_mode=full`, `include_status=true`, or tools whose payload has `execution`, `adapters`, `baseline_status` or `status_detail=summary`, then applies `terse_envelope`. Held as `Arc` in [lib.rs:452](../../crates/airk-mcp/src/lib.rs#L452) and [local.rs:86](../../crates/airk-mcp/src/local.rs#L86). Unit tests at lines 361–473. |
| [embedding_client.rs:60](../../crates/airk-cli/src/embedding_client.rs#L60) `struct Session` | Direct embedding helper process | `spawn` verifies the helper beside the current executable, owns `Arc<Mutex<Child>>`, a work channel and a `failed` flag. Implements `RemoteSession`; `Drop` tears down the child. `ProcessConnector::tokenizer`/`open` create it and check protocol, version and identity. |
| [embedding_pool_client.rs:179](../../crates/airk-cli/src/embedding_pool_client.rs#L179) `struct Session` | Multiplexed pooled embedding session | `client: Arc<Client>` plus an `Incarnation` id from `service_broker::new_instance_id`. `Drop` records the id in a closed set capped at 64 or marks the client failed, then sends `Operation::Close`. `PoolConnector` creates one per `provision_model`, `provision_bundled`, `tokenizer` and `open`. |
| [embed_pool.rs:44](../../crates/airk-cli/src/embed_pool.rs#L44) `struct Session { binding }` | Pool engine's server-side record | Keyed by `(owner Incarnation, session Incarnation)` in `Engine.sessions`; inserted at line 305. Counterpart of the pooled client above. |
| [providers_lsp.rs:46](../../crates/airk-ingest/src/providers/lsp.rs#L46) `struct Session` | Language-server provider process | Owns the LSP child, writer, reader threads, deadline, request id and root URI. Reader enforces a 32 MiB cumulative output cap over a `sync_channel(4)`. |
| [tests/mcp.rs:13](../../crates/airk-cli/tests/mcp.rs#L13) `struct Session` | Integration test harness | Spawns `airk mcp` with piped stdin/stdout. `start` forces `AIRK_MCP_RESPONSE_MODE=full`; `with_mode` lets tests choose terse. Used at lines 151, 206, 321, 394, 462, 761 and in `support/current_recovery.rs:132`. |

Related names that an exact-match lookup does not reach:

- `pub trait RemoteSession` at [embedding_worker.rs:99](../../crates/airk-retrieval/src/embedding_worker.rs#L99), implemented by the two embedding sessions.
- `struct RemoteSession` at [tools.rs:17](../../crates/airk-cli/src/tools.rs#L17), an unrelated wrapper over `airk_mcp::remote::RemoteTools` implementing `ToolSession`.
- `OwnerSession` and `NativeOwnerSession` in `airk-store` (found only by `rg`).
- `ort::session::Session` used behind a mutex at [onnx.rs:30](../../crates/airk-retrieval/src/onnx.rs#L30).
- TypeScript: `McpStdioSession` at [mcpStdioSession.ts:29](../../packages/vscode/src/mcpStdioSession.ts#L29) and the persisted `WorkbenchSession`/`WorkbenchSessionSummary` contracts at [workbenchContracts.ts:6](../../packages/vscode/src/workbenchContracts.ts#L6).
- Doc comments in [runtime_setup.rs](../../crates/airk-cli/src/runtime_install/setup.rs) use "Session" for the MCP connection lifetime, not a type.

## 5. Recommendations

### 5.1 Admission: queue briefly instead of refusing

**Observed.** [remote.rs:162](../../crates/airk-mcp/src/remote.rs#L162) and [remote.rs:481](../../crates/airk-mcp/src/remote.rs#L481) create `Semaphore::new(4)`; [remote.rs:89](../../crates/airk-mcp/src/remote.rs#L89) uses `try_acquire_owned` and returns "AIRK bridge is busy" when no permit is free. Below that limit the worker still refused calls with `direct_query_setup` ([project_client.rs:85](../../crates/airk-cli/src/project_client.rs#L85)) and `direct_query_admission` ([project_worker.rs:313](../../crates/airk-cli/src/project_worker.rs#L313), [status_observation.rs:906](../../crates/airk-cli/src/project_client/status_observation.rs#L906)). Four of five first-round calls failed, including `status`.

**Why it matters.** Agent clients send independent calls concurrently. The hints say retry after 25 or 100 ms, but a model cannot sleep that long; each refusal costs a full model turn and another request through the client. Five of 22 calls in this session were pure overhead.

**Recommend.**

- Replace `try_acquire_owned` with `acquire_owned` under a bounded wait (for example up to 2 s, well below the 120 s reply timeout), returning the existing busy result only when the wait expires. Keep the typed, unsubmitted receipt so retry contracts are unchanged.
- Apply the same bounded wait in the worker setup path for `retry_after_ms` values under about 500 ms; these are too short to hand back to an LLM.
- State the limit in the server instructions: "At most four calls are admitted concurrently; the worker serializes queries. Prefer batches of four."
- Keep `query_submitted=false` semantics; nothing here changes retry safety.

### 5.2 Search: rank definitions when the query names an identifier

**Observed.** Hybrid search for "how the code uses Session" returned design Markdown, prototype HTML, Python benchmark scripts and the README. Not one of the 17 Rust definitions appeared. Lexical search scoped to `packages/vscode/src/` was useful immediately.

**Recommend.**

- When a query token exactly matches an indexed definition name (case-sensitive, CamelCase or snake_case), boost chunks containing that definition or append a one-line pointer in the response: `"definitions": {"name": "Session", "count": 17, "hint": "symbols name=Session"}`.
- Add a `language` filter (same labels as `file_info`) to `search` and `references`, and an `exclude_path_prefix` or `kind: code|document` filter. Scoping by prefix helped only because this repository keeps code and docs apart.
- Document in the `search` description that natural-language questions about an identifier should go to `symbols`/`references` first and `search` second.

### 5.3 References: order and summarize by file

**Observed.** Unscoped `references` returned 40 of an unknown total with Rust pushed to the tail. Scoped to `crates/` it returned 52 of 56 and still truncated on bytes. The agent had no way to learn the total or the per-file distribution without paging.

**Recommend.**

- Prepend a `files` summary: `[{path, count}]` for every file with a hit, before the bounded hit list. This costs a few hundred bytes and tells the agent where to read.
- Sort code hits before document hits by default, or expose `order: code_first|path|score`.
- Report `total_hits` or `more: true` with a cursor instead of only `truncated: true`.

### 5.4 Symbols and code facts: substring matching and totals

**Observed.** Exact-name lookup missed `OwnerSession`, `NativeOwnerSession`, `WorkbenchSession` and `McpStdioSession`. The `imports` tool already accepts a name substring; `symbols` and `code_facts` do not. With `compact=false` the 8 KiB default returned 9 of 17 definitions with no total.

**Recommend.**

- Add `name_match: exact|suffix|contains` (default `exact`) to `symbols` and `code_facts`.
- Return `total_matches` alongside the cursor so paging decisions are informed.
- Keep compact as the default (it already fit all 17 definitions in 3 KiB) and say in the description that `compact=false` should be requested per symbol, not per name.

### 5.5 Relationships: connect a type to its constructors

**Observed.** Incoming relationships for the embedding-client `Session` struct id returned zero items, while the written target `Session::new` returned four callers. The struct symbol and the `TypeName::method` syntax targets are not linked, so the agent must guess constructor names.

**Recommend.**

- For a `struct_item`/`impl_item` symbol id, include outgoing `contains` edges to its methods and incoming call candidates whose written target starts with `TypeName::`.
- When a lookup returns zero items, return `suggested_targets` drawn from the type's impl blocks, for example `["Session::spawn", "Session::request"]`.
- Say in the description that a struct id alone rarely has incoming edges under Agent Lean and that written targets such as `Type::new` are the reliable input.

### 5.6 Response envelope bytes

**Observed.** Every response repeats the full publication token. Full mode repeats `discovery_readiness`, `semantic_readiness` and `freshness` blocks (about 600 bytes). Terse mode already suppresses repeats via `response::Session`, which worked as designed when used.

**Recommend.**

- Keep terse as the client default and say so in the server instructions. Use `response_mode=full` once per connection, then rely on `include_status=true` when a readiness change matters.
- Consider a connection-local `{ref: publication}` for the token after the first response, as file and provenance indices already do.
- The `read.original_root` field returns an absolute path on every read; a connection-local ref would also remove that repetition.

### 5.7 Tool and server description changes

The current descriptions come from [lib.rs](../../crates/airk-mcp/src/lib.rs) and [unified.rs](../../crates/airk-mcp/src/unified.rs). Proposed wording is below; each keeps the existing limits and adds the routing cue a client needs.

| Tool | Current description (abridged) | Problem observed | Proposed description |
|---|---|---|---|
| `outline` ([lib.rs:735](../../crates/airk-mcp/src/lib.rs#L735)) | "Understand a file's structure before reading broad text: bounded language-specific definitions, imports or structured document facts with locations. Takes one exact indexed file path." | Reads like an Explorer navigation aid, and `suggest_view` has a `code/outline` route. It is actually a bounded read of the index, and the first page of a 500-line file may not reach the definition the agent wants. | "Read one indexed file's structure: imports, definitions, call candidates and document facts in file order, with lines. A read-only evidence tool, not an Explorer action (use suggest_view for that). Paged by `cursor`; takes one exact `path`. For one known definition prefer `symbols`/`code_facts` with `path_prefix`." |
| `suggest_view` ([unified.rs:106](../../crates/airk-mcp/src/unified.rs#L106)) | "Queue a suggestion to open or prefill an Explorer screen, including Review and Management guidance. Returns a proposal receipt; the user applies it." | Route names (`code/outline`, `code/symbols`, `search/query`) echo evidence tool names, inviting confusion. | "Propose an Explorer screen for the user to open; never returns evidence. Routes such as `code/outline` name UI pages, not the evidence tools of the same name. Returns a proposal receipt only; the user applies it." |
| `search` ([lib.rs:587](../../crates/airk-mcp/src/lib.rs#L587)) | "Find code or documents. Use path_prefix when the question concerns one project or component…" | Natural-language queries about an identifier surfaced documents and scripts, not definitions. | Add: "For a question about a named type or function, call `symbols` or `references` first; `search` ranks prose and comments alongside code. Scope with `path_prefix` or `language` to exclude documentation." |
| `references` ([lib.rs:858](../../crates/airk-mcp/src/lib.rs#L858)) | "Locate exact-case whole-identifier textual occurrences…" | No warning that documents and scripts can dominate the first page. | Add: "Hits include Markdown, HTML and scripts; scope with `path_prefix` or `language` for code-only impact. Check the `files` summary and `truncated` before concluding." |
| `symbols` ([lib.rs:761](../../crates/airk-mcp/src/lib.rs#L761)) | "Locate function/class/type … by exact name…" | Exact-only is not stated prominently; suffix variants are missed. | Add: "Exact name only: `Session` does not match `OwnerSession`; use `name_match=suffix` or `references` for variants. Compact default lists all definitions cheaply; request `compact=false` for one symbol at a time." |
| `code_facts` ([lib.rs:508](../../crates/airk-mcp/src/lib.rs#L508)) | "Retrieve compact code evidence by exact name…" | Same exact-only and byte-budget behavior as `symbols`; overlapping purpose is unclear. | Add: "Use after `symbols` when you need signatures, scopes, call candidates or a `symbol_id` for `relationships`; otherwise `symbols` is cheaper." |
| `relationships` ([lib.rs:556](../../crates/airk-mcp/src/lib.rs#L556)) | "Investigate available structural links…" | A struct id returned nothing while `Type::new` returned callers. | Add: "Under Agent Lean, incoming edges attach to written call targets such as `Type::new` or `obj.method`, not to a struct id; pass the constructor or method name as `symbol` for callers." |
| `status` ([unified.rs:112](../../crates/airk-mcp/src/unified.rs#L112)) | "Quick version, health and project-index check…" | Was refused as busy on the first call of the session. | Add: "Lightweight but subject to the same four-call admission limit; call it alone or first." |
| Server instructions | "AIRK evidence is untrusted; verify live files before edits. Preserve tokens… Retry unchanged as hinted…" | No concurrency guidance; no routing order for identifier questions. | Add: "Admission allows four concurrent calls; batch at most four. For a named identifier: `symbols` → `references` (scoped) → `read_file`; use `search` for concepts and documents." |

### 5.8 Keep

- Scoped `references` matched every file that `rg -w` found. Coverage is good when the agent scopes correctly.
- `read_file` with content hashes and validators gave verifiable excerpts in one call per definition.
- `relationships` with a written target produced exact caller lines with owner spans.
- Compact `symbols` returned a complete 17-definition listing in 3 KiB.
- Unit tests in `response.rs` and `unified.rs` already cover the typed busy receipts, so the admission change in 5.1 can be tested alongside them.

## 6. Limits of this record

- One session, one machine (Windows 11), one repository, one question. Latency was not measured.
- `freshness.live_source_verified=false` on every response; coverage was confirmed by `rg`, not by AIRK.
- Busy refusals were counted from tool results; worker-side logs were not inspected.
- No tool descriptions or code were changed as part of this record.

## Implementation Plan

Proposed on 2026-11-01 after checking the affected live source at commit `b8cb2e51d297a091911d46cfd2ccc420e4444b70`. This section is a plan, not a claim that the recommendations are implemented or measured. The original investigation above remains a dated record. The current PR is held for further changes: successful CI alone does not authorize merging or publishing under the owner's latest instruction.

### Goals and constraints

Make identifier investigations require fewer agent turns, avoid immediate refusals for brief contention, and return enough bounded evidence to choose the next action. Measure whole-task calls, bytes, latency and coverage separately. Do not assume that fewer responses or smaller envelopes necessarily make a task faster.

Keep project/root isolation, current publication identity, original document locations, authenticated storage reads, existing byte-budget meanings, source read-only behavior and explicit incomplete coverage. Implement changes in the owning crates/adapters; no new orchestration layer, database, provider, inferred call graph or automatic indexing. Preserve the existing lexical/vector/hybrid policy and DiskANN direction. UI proposals remain inert and user-reviewed.

### Phase 0 — Establish a reproducible baseline

Address the evidence gaps before using this session as a quantitative before/after result:

- Reconcile the retained transcript against the query table: the summary reports 22 calls and five refusals, while the table contains 28 call rows and eight busy outcomes. Keep original observations identifiable and publish a corrected attempt/retry/round ledger with its counting rules; do not derive a refusal percentage from inconsistent totals.
- Distinguish matching files, matching lines, identifier occurrences and declaration/fact records. `rg -w -c` reports matching lines, not individual occurrences. Seventeen returned definition/fact records are not seventeen distinct Rust types. Normalize overlapping chunk hits before comparing reference coverage, and use returned `syntax_candidate` labels rather than describing those edges as resolved callers.
- Freeze a public source revision and record binary/build identity, index policy, model readiness, publication, corpus identity and live-source verification. Retain the `Session` investigation as one case, alongside original fictional fixtures covering duplicate names, suffix variants, namespaces, mixed code/documents, Unicode, overloaded methods and insufficient evidence.
- Replay a fixed task script through the installed launcher with concurrency 1, 4, 6 and 8, both cold connections and reused sessions. Repeat enough sessions to report sample counts, median and p95, with outliers/errors retained. Measure initialization, queue/setup wait, execution, time to first useful evidence, time to verified task completion, retries/model rounds and complete wire bytes including discovery/envelopes. Keep deterministic tool replay separate from actual-agent routing trials and their model/configuration.
- Capture bounded, content-free admission diagnostics to identify which layer refused each attempt. This also supplies a regression baseline for status responsiveness under mixed evidence traffic; an isolated direct-worker benchmark alone cannot measure the installed path.

**Exit:** a consistent replay ledger and coverage oracle, plus reproducible baseline results. Set numeric performance targets from that baseline before tuning; this one un-timed session cannot support a speedup promise.

### Phase 1 — Correct routing guidance first (5.7, 5.8)

Update [native descriptions](../../crates/airk-mcp/src/lib.rs), [unified instructions/descriptions](../../crates/airk-mcp/src/unified.rs), and the reciprocal [MCP](../mcp-developer-guide.md) and [CLI](../cli-developer-guide.md) developer guides together:

- Make `outline` explicitly a paged indexed-evidence read in file order; make `suggest_view` explicitly a UI proposal that returns no source evidence. A route such as `code/outline` is a page name.
- Recommend compact `symbols` for known definitions, scoped `references` for exact textual uses, then bounded `read_file`/`context`; reserve conceptual `search` for prose/questions and supplementary discovery. Explain that documents, comments and scripts can appear in textual results. Explain the overlapping fact families without asserting that `symbols` is intrinsically cheaper: both use shared native fact retrieval, and their actual cost must be measured.
- State the current exact-name behavior prominently, including that `Session` does not discover all suffix variants. Explain byte limits and continuation, and prefer compact discovery before rich expansion of a selected candidate.
- Explain written relationship targets and partial coverage; a zero result for a struct does not prove no callers. Keep syntax candidates distinct from imported producer assertions and resolved references.
- Describe four as the bridge's concurrent-call cap, not a promise that four simultaneous calls always succeed. Downstream admission and setup can still refuse. Recommend small batches and obtaining status before broad fan-out, while retaining terse as the default and full/explicit status for provenance and diagnosis.

Do not advertise `language`, `name_match`, file summaries or other proposed fields until their handlers and schemas exist. The wording in 5.7 needs that sequencing adjustment. Check discovery schemas, native aliases, generated CLI help and extension adapters for agreement. Description-only changes must not invalidate extraction or embedding state.

### Phase 2 — Absorb brief contention safely (5.1)

Trace and fix the complete admission path rather than replacing one semaphore call:

1. In [remote.rs](../../crates/airk-mcp/src/remote.rs), replace immediate bridge refusal with bounded, cancellation-aware permit waiting. Start with a maximum two-second wait as a candidate to test, capped by the request's remaining deadline. Keep four executing calls and separately cap queued waiters; reject overload with a structured unsubmitted busy receipt. `acquire_owned` by itself does not bound queue size. The second `Semaphore::new(4)` cited above is in the test fixture, not a second production admission layer.
2. Audit [project_client.rs](../../crates/airk-cli/src/project_client.rs), [project_worker.rs](../../crates/airk-cli/src/project_worker.rs), [status_observation.rs](../../crates/airk-cli/src/project_client/status_observation.rs) and the existing [broker](../../crates/airk-kernel/src/service_broker.rs). Setup already has bounded retry logic, and established evidence connections already serialize through a shared state lock. Identify the frontend paths that deliberately attempt setup once before adding another retry loop. Share appropriate in-flight setup and absorb only known transient, unsubmitted refusals inside one remaining budget.
3. Revalidate grants, instance/project scope and expiry at admission after waiting. Do not hold synchronous broker locks across an async wait. Stop/drain, EOF, cancellation and supersession must retire queued work correctly; accepted native work retains ownership until actual completion/drain.
4. Preserve the separate cold-preparation and lightweight status-observation budgets. A generic two-second queue allowance must not consume or repeatedly reset those budgets, put status behind long evidence requests, or turn a timeout into false readiness. Choose queue placement/fairness from mixed-load evidence.
5. Preserve `query_submitted=false` only when that is known. Honor `fresh_brokerage_required` and do not automatically replay a submitted query, an uncertain `parse` execution, or partially completed work. Permanent schema, authorization and integrity errors fail immediately. Busy classification must be typed; do not retry by matching an error message.

**Tests:** controlled saturation, release-before-expiry, expiry with no submission, queue overflow, fairness, cancellation/EOF, shutdown, stale grants, project isolation, status under load and uncertain post-submission failure. Use synchronization and injected deadlines rather than timing sleeps as the main oracle. Measure elimination of transient refusals together with tail latency; hiding a refusal behind a long wait is not sufficient success.

### Phase 3 — Add precise filters and explicit name matching (5.2, 5.4)

Extend the shared [query contracts](../../crates/airk-core/src/lib.rs), [fact queries](../../crates/airk-core/src/code.rs), [native schemas](../../crates/airk-mcp/src/lib.rs), Kernel handlers and Store selectors before exposing new options in unified MCP, direct CLI/native tool calls and applicable Explorer forms:

- Add optional extraction-language filtering to search and references. Define labels from actual indexed metadata. Add an explicit evidence-kind filter only with a documented classification for code, documents, configuration and unknown/mixed extraction; a filename extension or prefix is not a reliable universal classifier.
- Apply filters consistently to lexical and vector branches before bounded candidate selection/ranking. Post-filtering a small top-k can discard all useful results while falsely looking exhaustive. Preserve explicit mode/readiness behavior and source locations for non-code documents. Evaluate a bounded `exclude_path_prefix` option after the initial filters, with literal path semantics and no effect on source authorization or saved index policy.
- Add `name_match: exact|suffix|contains` to symbols/code facts, keeping exact as the default and existing durable-key/filter exclusions. Specify case sensitivity and whether matching targets the simple name or qualified name; retain existing exact symbol-ID and qualified-name lookup. Do not silently change imports' existing substring semantics.
- Bind filters and match mode into cursor/query identities. Scope suffix/contains work to existing bounded discovery and verified projections. Avoid a new full-corpus scan or fact-posting format unless profiling demonstrates the need; document any separately reviewed format migration or reindex requirement.

**Tests:** exact default compatibility, suffix/contains variants, case/Unicode, same-name declarations across roots/namespaces, unknown language/kind, combined filters, both search branches, tiny budgets, pagination and expired-publication rejection. Validate old argument objects still work. A caller must explicitly select new matching/order semantics.

### Phase 4 — Make reference navigation and counts useful within budgets (5.3, 5.4)

The current [reference reader](../../crates/airk-kernel/src/lib.rs) uses bounded lexical candidates, retained-source verification and independent scan limits; it is not already an exhaustive pageable reference index. Its result wraps a search envelope. Extend that contract deliberately:

- Add deterministic reference continuation tied to identifier, filters, ordering, representation and exact publication. Resume scanning without skipping or duplicating a boundary hit; stale cursors fail explicitly. Distinguish count/byte limits, candidate/scan limits and genuine completion, including an empty page with unfinished scanning.
- Offer bounded per-file summaries, with a defined count unit and scope: returned page, scanned candidates or complete matching set. Page summaries too when necessary, and reserve space so summary metadata cannot crowd all useful evidence out of a small response. Do not promise every matching file in a few hundred bytes on arbitrary repositories.
- Keep current ordering as the compatibility default initially and add explicit `order: code_first|path` if the classification/filter work supports it. Evaluate any score order separately. Bind ordering to cursors and preserve honest coverage in every mode.
- Return an exact `total_hits`/`total_matches` only when an authenticated, bounded computation establishes it for the selected publication and filters. Otherwise disclose an unknown total or a clearly labeled lower bound and whether more scanning is possible. Do not convert a page size or truncated candidate count into a corpus total, or scan the entire collection on every compact lookup merely to count it.
- Reuse existing fact pagination for symbols/code facts, adding count precision/coverage consistently. Preserve existing response-byte semantics; complete-envelope budgeting changes require their own versioned contract and compatibility tests.

**Exit:** complete paged fixture results match the defined live oracle for the frozen corpus; deliberately incomplete scans remain explicitly incomplete. Small-budget queries make progress or return the existing precise record-too-large outcome. Compare total task bytes/time, not only the first page.

### Phase 5 — Improve identifier search and type navigation (5.2, 5.5)

For search, first trial a bounded definition hint for an unambiguous identifier token, using current declaration postings and the same scope/publication. A hint can recommend `symbols` and report a qualified count without silently rewriting the user's natural-language query or changing lexical AND semantics. If hints are insufficient, evaluate a deterministic definition boost against the baseline in [current search](../../crates/airk-kernel/src/current/search.rs); do not add an LLM reranker. Avoid hiding documentation or conceptual hits merely because a word also names a type. Keep the existing default ranking until the judged comparison supports changing it.

For relationships, audit existing containment first: [relationships.rs](../../crates/airk-store/src/current/relationships.rs) already creates `contains` candidates from retained `owner_id` values. A zero struct result therefore does not establish that containment edges are universally missing. Verify how Rust impl blocks and methods are owned, then derive bounded type-to-impl/method navigation only when retained file, owner/scope and source identity support it. Do not join every `Session::...` target to every unrelated type named Session.

Add bounded `suggested_targets` from verified associated declarations when direct relationships are absent or ambiguous, preserving the selected publication and coverage. Incoming `Type::method` matches remain separately labeled written-target call candidates, not resolved callers. Cover constructor-like names beyond `new`, trait impls, generic/nested types, aliases, duplicate names and missing facts. Do not guess an association when Agent Lean omitted the necessary evidence; report the limit and suggest scoped references instead. Prefer query-time reuse of retained facts; any extractor/posting change must quantify extra ingestion/storage and its reindex implications.

**Exit:** the Session task discovers the appropriate type/method candidates without guessed constructor names, while adversarial same-name fixtures produce no false resolved associations. Judge top-k relevance and whole-task cost across identifier and conceptual questions, including failed/unsupported cases; this one question is a regression case, not a universal ranking benchmark.

### Phase 6 — Retain safe envelope savings; defer connection-only evidence (5.6, 5.8)

Keep the existing [terse status receipts](../../crates/airk-mcp/src/response.rs), explicit full mode and `include_status` behavior. Verify receipts reset correctly across reconnect, project/incarnation/publication changes and changed readiness, while unchanging metadata stays compact. Measure actual serialized bytes, including duplicated structured/text envelopes.

Defer replacing publication tokens or `read.original_root` with connection-only references in the default protocol. Exact tokens and original locations make responses independently usable in logs, copied evidence, saved Workbench exports, CLI output and MCP-only clients. A future opt-in, versioned encoding would need explicit client negotiation, bounded reference tables, reset/expiry rules, out-of-order response handling and complete expansion on export. It must save meaningful whole-task bytes before adding that state/compatibility cost. Do not omit citation identity merely to make an individual response smaller.

Preserve the demonstrated strengths in 5.8: exact retained reads/validators, compact discovery, scoped textual coverage and qualified call locations. Performance work must not trade them away.

### Delivery, validation and documentation

Implement in reviewable groups: baseline/guidance → admission → filters/name matching → paging/summaries/count precision → measured ranking/type navigation. Envelope-reference compression is a separate deferred experiment, not a prerequisite. Each group includes its source changes, contract tests and documentation; do not advertise a future phase as shipped.

Use deterministic Rust contract tests plus end-to-end CLI/unified MCP cases, including the existing [MCP integration suite](../../crates/airk-cli/tests/mcp.rs), [code-graph tests](../../crates/airk-kernel/tests/integration/code_graph.rs), native tool schemas and extension adapter tests. Run targeted reproductions locally, then required formatting, workspace Clippy/tests, Python contracts, repository and whitespace checks from [CONTRIBUTING](../../CONTRIBUTING.md). Exercise shared fixes on Windows and local WSL Linux. Preserve routine CI cost controls: collect all results before a corrective push; Linux runs the broad suite, Windows/Apple Silicon their selected gates, and the shared Intel Mac paths remain covered by their required main-stage build/tests before any later release. Do not claim all-platform qualification from WSL or portable tests.

For each delivered group, update both developer guides, unified/native contracts, CLI help/reference, applicable Explorer schemas and guidance, README/documentation index, release notes and Marketplace description as needed. Keep native/extension versions synchronized using the approved version for that release. Report observed before/after calls, retries, wire bytes, coverage, cold/warm latency distributions and remaining unknowns in the performance report; installed responsiveness and native throughput remain separate measurements. Review the complete diff and evidence before signed-off commits/pushes. The owner's current merge/publication hold remains in force until explicitly lifted.
