# AIRK MCP developer guide

The 1.8.8 working build supplies optional [evidence guidance](evidence-guidance.md)
from returned metadata. Advice is bounded and inert; CLI/MCP/Workbench and
complete/selected exports retain applicable reasons without additional reads.

This guide explains **the public tools in the AIRK 1.8.8 manual-test working build**: why each exists, when to choose it, how it works, its inputs/results, a request example, and the owning implementation/tests. It complements the [catalog and connection contract](unified-mcp.md), [native protocol/evidence contract](mcp.md), [response modes](mcp-response-modes.md) and [CLI developer guide](cli-developer-guide.md). Current qualification is in the [1.8.8 working build record](releases/release-1.8.8.md); the inherited 1.8.7 contracts and evidence remain in their [dated record](releases/release-1.8.7.md).

## Start with the right layer

Native reads accept optional `reply_detail: lean|rich`. It selects available
fact projection independently of retained indexing detail or metadata
response mode. Omission preserves legacy defaults; conflicts with `compact` reject. Search
`full_chunks` remains independent. See the [reply guide](reply-detail.md) for per-family behavior,
keyed-read limits and continuation rules. Native input catalogs expose the enum.

The CLI composes the unified MCP frontend. Native schemas and rendering belong to `airk-mcp`; read behavior belongs to `airk-kernel` and retained storage to `airk-store`. Optional Explorer Assistance supplies bundled guidance, templates and guarded UI proposals. Kernel/store do not depend on the editor. Preserve the [architecture dependency direction](architecture.md) when changing a tool.

| Need | Tools |
|---|---|
| Select a configured scope and inspect readiness | projects, status; diagnostics only when details are needed |
| Discover evidence and exact paths | search, list_files, capabilities |
| Read and interpret a specific retained source | file_info, read_file, context, outline |
| Inspect written code/document structure | symbols, imports, code_facts, document_facts, references, relationships |
| Revalidate remembered evidence | changes and conditional read validators |
| Prepare or run a bounded native evidence recipe | parse |
| Learn AIRK or construct an editor proposal | help, list_templates, get_template, draft_search, draft_strategy, draft_plan, suggest_view, validate_draft, proposal_status, get_handoff |

Use `tools/list` on the actual MCP connection as the authoritative public catalog.
Client namespaces such as `mcp__airk__` are client presentation. Native CLI sessions expose 19 prefixed operations, including `airk_query_scope`;
reviewed combined sessions additionally expose their selector controls. Unified MCP adds `projects`,
`query_scope` and nine Assistance operations. Legacy `query_scope` reports
`configuration_required`; it never enumerates the machine library. `projects`
keeps the existing connection-binding meaning. CLI `projects` remains a separate
administrative service-registry read. [Combined-read contracts](combined-reads.md)
record the new session, authority and qualification boundary.

## Shared request and evidence rules

- For a named identifier, start with compact `symbols`, inspect scoped `references`, then expand one chosen definition using `read_file`/`context`. Use `search` for concepts and supplementary code/document discovery. `symbols` and `code_facts` share the native fact reader; their cost depends on filters and requested detail, not just the tool name.
- Obtain `status` before broad evidence fan-out and prefer small batches. Concurrent evidence capacity defaults to 16 and is configurable in Management → Service to 4, 8, 16, 32 or 64. Matching native channels and project processing have the same allowance; status has one additional slot. A bounded 128-request queue absorbs brief contention, but downstream setup, ownership and deadlines can still refuse unsubmitted work. Honor typed busy receipts without treating saturation as missing evidence.
- In a multi-root connection, obtain selectors from projects and pass `project` only where the schema accepts it. Explorer UI selection never changes retrieval scope. A returned configured selector and the native service registry binding are different identity roles.
- Native evidence is an indexed snapshot. Preserve exact publication tokens, content hashes, validators and original locations. Verify relevant live bytes before editing; neither a successful tool call nor a matching indexed hash proves live-source freshness.
- Use literal relative prefixes where supported. `path` selects an exact indexed path. Scope before raising budgets. `limit`, `max_bytes`, compactness and pagination have per-tool contracts; a small response is not proof that internal work is constant.
- Terse is the default where response options exist. `response_mode: full` adds explicit provenance/coverage; `include_status` repeats current observations. `compact: false` controls record richness separately. Do not add these keys to template/proposal calls whose schemas omit them.
- Exact publication tokens and original source locations remain in evidence. Terse status receipts suppress repeated scalar observations; they are not connection-only replacements for citation identity. Copied replies and exports must remain independently interpretable.
- Repeat continuation requests with the same filters and representation. Current-only revision tokens/cursors expire when superseded. Finish every needed changes page before advancing its baseline; independent coverage floors can make remembered history unavailable.
- Typed busy/retry responses must be honored exactly as returned. Retry unchanged only where the receipt explicitly says the query was not submitted or permits that exact read retry. Transport failure after submission is uncertain; do not blindly replay a plan or operation.
- Native evidence tools do not write project sources or expose management commands. Normal connection, cache, usage-accounting and owned service lifecycle metadata can still be maintained. UI proposals are separately marked as drafts and require user review.

Examples below are **tool argument objects**, not executed calls. Source paths/names are fictional. Replace angle-bracket placeholders only with actual returned tokens or IDs; they are not valid synthetic capabilities. Draft examples illustrate schema shape and must not be posted to a live window merely to test documentation.

## Tool index

- [projects](#projects)
- [query_scope](#query_scope)
- [status](#status)
- [diagnostics](#diagnostics)
- [capabilities](#capabilities)
- [search](#search)
- [list_files](#list_files)
- [file_info](#file_info)
- [read_file](#read_file)
- [context](#context)
- [outline](#outline)
- [symbols](#symbols)
- [imports](#imports)
- [code_facts](#code_facts)
- [document_facts](#document_facts)
- [references](#references)
- [relationships](#relationships)
- [changes](#changes)
- [parse](#parse)
- [help](#help)
- [list_templates](#list_templates)
- [get_template](#get_template)
- [draft_search](#draft_search)
- [draft_strategy](#draft_strategy)
- [draft_plan](#draft_plan)
- [suggest_view](#suggest_view)
- [validate_draft](#validate_draft)
- [proposal_status](#proposal_status)
- [get_handoff](#get_handoff)

## query_scope

**Why and when:** Discover Projects, indexes and sources within the connection's
frozen native-reviewed grant before forming a combined request.

**Inputs:** `view: projects` accepts no IDs; `indexes` requires `project_id`;
`sources` requires both `project_id` and `index_id`. Optional `limit` is 1–32,
`max_bytes` is 1–65,536, and `cursor` continues the exact view and revisions.
These are new opaque `prj_`/`idx_` identities, separate from legacy `project`.
For a connection with configured legacy bindings, the optional transport
`project` selects the advertised binding and is required when no single default
exists. It never follows the Explorer selection. Combined sessions and the
native prefixed handler omit that transport selector.

**Result and limits:** Granted-only metadata, exact scope/grant revisions,
observation, pagination and truncation. Combined catalog discovery needs no source
scan or evidence worker. A frozen single-index connection reports its exact
installed grant, `scope_observation` and `observed_selection`, with
`mode: frozen_single_index` and `configuration_required` for current catalog
discovery. Its catalog rows remain empty and it has no continuation: it does not
invent Project names or restore/widen a grant. An unscoped legacy connection
reports `configuration_required` without an installed grant. A changed authority
expires the page; it never restarts against broader scope.

**Implementation:** [selected catalog](../crates/airk-mcp/src/combined.rs),
[native owner](../crates/airk-cli/src/combined_frontend.rs), and
[actual-process tests](../crates/airk-cli/tests/combined_reads.rs).
The [single-index handler](../crates/airk-kernel/src/scoped_reads.rs) and
[configured-binding adapter](../crates/airk-mcp/src/unified.rs) retain their
separate authority and transport selection roles.

## projects

**Why and when:** Choose an explicitly configured source scope before reading evidence in a multi-root connection. This prevents an editor selection from silently redirecting an agent.

**How it works:** The unified adapter enumerates its configured project map and default selector. It neither scans all machine registrations nor changes the current project.

**Inputs:** No arguments. Reuse a returned project selector only on the connection that advertised it.

**Result and limits:** `default_project` and configured `projects` entries. These connection selectors are distinct from native registry binding identities.

```json
{}
```

**Implementation:** [unified.rs](../crates/airk-mcp/src/unified.rs). **Contract/regression evidence:** [unified.rs](../crates/airk-mcp/src/unified.rs).

## status

A completed status query that overlaps ordinary scalar-cache invalidation returns its original snapshot successfully with `status_observation.superseded: true`, `reason: scalar_invalidation` and an explanatory message. Original publication and observation age are retained. The superseded snapshot does not repopulate the current cache or acknowledge an obsolete status receipt; the query is not replayed. Project/namespace changes, authenticated worker replacement, expired source authorization and shutdown retain their existing rejection behavior. Cached reuse has its five-second limit; configured evidence capacity has one additional reserved status slot.

**Why and when:** Answer the inexpensive question: what publication and readiness does this configured project currently expose?

**How it works:** The frontend prepares the selected runtime, then obtains bounded scalar metadata. Successful observations can be reused for five seconds with their original observation time and cache age. The cold preparation allowance is ten seconds; established observation has a separate two-second allowance.

Deadline cancellation of setup admission retains its typed expiry through the shared outcome, including for later waiters. Accepted exact work stays owned until it settles; another pipeline operation cannot start on that expired attempt. The existing `status_timeout` with stage `setup` remains an unconfirmed observation. Genuine setup errors and session EOF retain their original failure classification.

**Inputs:** Optional configured `project`, `response_mode` and `include_status`. It does not accept a request to run diagnostics.

**Result and limits:** `query_concurrency` (configured frontend evidence capacity at admission), version, index state, counts, publication, freshness and semantic readiness. A successful envelope can still report unavailable or unknown evidence. Require `index_state: available` and a publication; never turn a timeout into proof that the index is unavailable.

```json
{
  "response_mode": "full"
}
```

**Implementation:** [status_observation.rs](../crates/airk-cli/src/status_observation.rs). **Contract/regression evidence:** [unified.rs](../crates/airk-mcp/src/unified.rs).

## diagnostics

**Why and when:** Investigate an unavailable, stale, slow or incorrectly attached project after a lightweight status check is insufficient.

**How it works:** The native adapter gathers explicitly requested runtime/process, worker/job, queue, watcher and sampled-wait observations. The unified adapter can add optional Explorer attachment information. Each observation retains its own availability and age.

**Inputs:** Optional configured `project` and response options. Request explicitly; do not poll it as an automatic substitute for status.

**Result and limits:** `query_concurrency` (the same admission observation as status), detailed coverage and runtime/work observations, including missing/failed observations and optional editor binding/revisions. It does not start indexing, repair storage or prove that an old progress sample is advancing.

```json
{
  "response_mode": "full"
}
```

**Implementation:** [runtime_metadata.rs](../crates/airk-cli/src/runtime_metadata.rs). **Contract/regression evidence:** [local.rs](../crates/airk-mcp/src/local.rs).

## capabilities

**Why and when:** Determine what extraction formats and explicitly provisioned local adapters can support before choosing a retrieval approach.

**How it works:** The native tool reports supported formats, provider identities and limits, with an optional exact adapter profile. Capability information is separate from a collection's current model, OCR or index readiness.

**Inputs:** `adapter` selects a returned adapter ID; `rich` expands profiles. Response options and configured project selection are available.

**Result and limits:** Capability/profile metadata and limitations, including provider provenance where available. A listed format or adapter is not evidence that every file was extracted successfully.

```json
{
  "rich": true,
  "response_mode": "full"
}
```

**Implementation:** [lib.rs](../crates/airk-mcp/src/lib.rs). **Contract/regression evidence:** [local.rs](../crates/airk-mcp/src/local.rs).

## search

**Why and when:** Discover relevant code and documents together when the exact source path or declaration is not yet known.

**How it works:** The kernel selects a current indexed head, applies literal path scope and optional indexed extraction filters, then performs lexical, vector or hybrid retrieval. Filters constrain authenticated candidate paths before either branch's bounded top-k selection. It materializes bounded, source-bound evidence at original locations. Local model/index readiness governs vector modes.

**Inputs:** `query`; optional `mode`, `path_prefix`, `language`, `evidence_kind`, `exclude_path_prefix`, `limit`, `max_bytes`, `full_chunks` and response options. Language is an exact saved extraction label. Evidence kind is `code`, `document`, `configuration` or `unknown`, derived from indexed extraction metadata rather than a filename guess. Exclusion is one literal relative prefix for this query, not a change to indexing or authorization. Lexical terms are ANDed. Omitted mode follows saved collection policy; an explicit mode does not silently mean another mode.

**Result and limits:** Ranked hits, citations, exact publication and truncation/coverage information. Filter candidate discovery has its own finite work limit; a truncated roster is disclosed rather than reported as an exhaustive absence. Optional bounded `definitions` hints identify verified native declaration records from the same sealed read, with exact/unknown count precision and cited candidate pointers; imported assertions are excluded from this hint family. Their pointer list can be shorter than the qualified count; they do not change search ranking or lexical AND semantics. Focused snippets are the default; `full_chunks` changes returned text, not the indexed source. Lexical matching requires normalized AND terms in a candidate chunk or path; punctuation and quotes separate terms, and quotes do not enable phrase matching. Lexical candidate order is stable index identity, not relevance. A contiguous original query spelling can guide excerpt focus inside an already selected chunk without changing matching, candidate order or limits. Hybrid results while semantic work is partial do not prove complete vector coverage. Prose, comments and scripts can rank alongside code; for a known type/function use definition lookup first.

```json
{
  "query": "retry budget",
  "mode": "lexical",
  "path_prefix": "src/",
  "limit": 5,
  "max_bytes": 8192
}
```

**Implementation:** [search.rs](../crates/airk-kernel/src/current/search.rs). **Contract/regression evidence:** [persistent.rs](../crates/airk-kernel/tests/integration/persistent.rs).

## list_files

**Why and when:** Discover exact indexed paths before guessing filenames or requesting broad file reads.

**How it works:** The inventory reader pages the published snapshot under count and encoded-byte budgets. It does not walk the live filesystem.

**Inputs:** `prefix` is a literal collection-relative prefix, not a glob. Keep it, response options and page budgets stable when consuming a returned `cursor`.

**Result and limits:** Indexed paths, publication, bounded continuation and truncation information. Inventory is not a complete live-workspace listing or proof of extraction coverage.

```json
{
  "prefix": "src/",
  "limit": 20,
  "max_bytes": 8192
}
```

**Implementation:** [discovery.rs](../crates/airk-kernel/src/discovery.rs). **Contract/regression evidence:** [current_inspection.rs](../crates/airk-kernel/tests/integration/current_inspection.rs).

## file_info

**Why and when:** Establish the hash, extraction method and coordinate basis before interpreting a hit or issuing conditional reads.

**How it works:** The current conditional metadata reader returns the retained file/profile identity. Optional validators bind the indexed representation; it never hashes the live editor buffer for you.

**Inputs:** Exact `path`; optional `artifact_id`, current `revision`, `known_hash`, `if_evidence` and conservative `follow_moves`, plus response options.

**Result and limits:** Language/profile, parse/extraction state, source hash and indexed metadata, or an explicit conditional/no-content outcome. A source hash identifies saved indexed bytes, not live-file freshness.

```json
{
  "path": "src/client.rs",
  "response_mode": "full"
}
```

**Implementation:** [conditional.rs](../crates/airk-kernel/src/current/conditional.rs). **Contract/regression evidence:** [current_inspection.rs](../crates/airk-kernel/tests/integration/current_inspection.rs).

## read_file

**Why and when:** Read the smallest retained evidence range needed to verify or extend a search result, preserving its original citation.

**How it works:** The reader materializes a bounded current indexed range and representation validator. Native-source line coordinates differ from normalized extracted-document text coordinates.

**Inputs:** `path`, 1-based inclusive `start_line` and `end_line`; optional current `revision`, `known_hash`, `if_evidence`, move handling and `max_bytes`.

**Result and limits:** Cited text and a scoped read outcome/validator. Missing, moved, ambiguous, unchanged and expired evidence are explicit; a superseded token does not authorize replacement with today's live bytes.

```json
{
  "path": "src/client.rs",
  "start_line": 1,
  "end_line": 20,
  "max_bytes": 8192
}
```

**Implementation:** [conditional.rs](../crates/airk-kernel/src/current/conditional.rs). **Contract/regression evidence:** [file_continuity_v2.rs](../crates/airk-kernel/tests/integration/file_continuity_v2.rs).

## context

**Why and when:** Expand around a known hit without loading an entire source file or losing its citation.

**How it works:** The range reader selects neighboring indexed lines around one coordinate and applies the same source/representation identity rules as conditional file reads.

**Inputs:** `path`, `line`, `before`, `after`, `max_bytes`; optional exact current revision/hash/validator and move handling.

**Result and limits:** Bounded cited neighboring text or a typed conditional outcome. For PDF/DOCX/PPTX, stay within the intended original page/block/slide and interpret extracted coordinates using file metadata.

```json
{
  "path": "src/client.rs",
  "line": 40,
  "before": 6,
  "after": 10,
  "max_bytes": 8192
}
```

**Implementation:** [conditional.rs](../crates/airk-kernel/src/current/conditional.rs). **Contract/regression evidence:** [header_context_boundaries_v2.rs](../crates/airk-kernel/tests/integration/header_context_boundaries_v2.rs).

## outline

**Why and when:** Understand one file's structure before spending a large text budget on it.

**How it works:** The discovery/conditional reader selects definitions, imports, available call candidates or structured document elements from one retained file, preserving source order, locations and continuation. This is an indexed-evidence read. The `code/outline` route accepted by suggest_view instead names an Explorer UI page.

**Inputs:** Exact `path`; optional `compact`, `limit`, `max_bytes`, `cursor`, current revision/hash/validator and response options.

**Result and limits:** A bounded outline. `compact: false` requests richer facts and IDs; it is independent of response mode. Partial syntax or extraction remains partial, not resolved types or runtime behavior.

```json
{
  "path": "src/client.rs",
  "compact": false,
  "limit": 20,
  "max_bytes": 8192
}
```

**Implementation:** [discovery.rs](../crates/airk-kernel/src/discovery.rs). **Contract/regression evidence:** [current_inspection.rs](../crates/airk-kernel/tests/integration/current_inspection.rs).

## symbols

**Why and when:** Locate a written definition name directly rather than relying on broad text search.

**How it works:** The discovery reader selects grammar-derived definitions from indexed code facts. A requested language/profile must match the index's vocabulary.

**Inputs:** Optional `name`, `language`, `path_prefix`, `compact`, budgets and continuation. Default name matching is exact: `Session` does not discover `OwnerSession`. Explicit `name_match: suffix|contains` compares simple or qualified names case-sensitively; explicit `exact` retains exact name/qualified name/symbol-ID lookup. Every explicit mode, including `exact`, requires a nonempty name. `include_count` requests count precision, not an unlimited count pass. Omit name and match mode to list definitions in a bounded scope.

**Result and limits:** Definition signatures and locations; rich output adds IDs/details usable in later inspection. Start compact and expand a chosen candidate when needed; `compact: false` can exhaust bytes before the record limit. Optional top-level `match_count` reports a nullable exact total, verified lower bound, completeness and count scope. A partial page scan is not a whole-query total. Matching mode and representation bind the continuation, and equal written names can have multiple candidates. This is not a cross-language name resolver.

```json
{
  "name": "Owner",
  "path_prefix": "src/",
  "compact": false,
  "limit": 10
}
```

**Implementation:** [discovery.rs](../crates/airk-kernel/src/discovery.rs). **Contract/regression evidence:** [code_graph.rs](../crates/airk-kernel/tests/integration/code_graph.rs).

## imports

**Why and when:** Inspect written static dependencies while keeping them distinct from transitive or runtime dependency resolution.

**How it works:** The discovery reader selects retained import/use/include declarations with their source locations.

**Inputs:** Optional name substring, `language`, `path_prefix`, `compact`, budgets and cursor. Omitted `name_match` preserves the existing case-insensitive substring behavior. Explicit `exact|suffix|contains` selects the same qualified-name matching rules as symbols, and `include_count` requests qualified count metadata; other import semantics remain unchanged.

**Result and limits:** Bounded import declarations and provenance. Absence does not prove that a dynamic dependency does not exist.

```json
{
  "path_prefix": "src/",
  "name": "serde",
  "limit": 20
}
```

**Implementation:** [discovery.rs](../crates/airk-kernel/src/discovery.rs). **Contract/regression evidence:** [code_graph.rs](../crates/airk-kernel/tests/integration/code_graph.rs).

## code_facts

**Why and when:** Inspect richer declarations, written calls and other structural evidence, or reselect one remembered durable code identity.

**How it works:** The kernel queries retained syntax facts and optional source-hash-bound producer assertions. Coverage and producer basis distinguish written candidates from imported assertions.

**Inputs:** Exact-default `name`/qualified name/symbol ID and optional kind/language/path filters, or one durable `symbol_key` without textual filters. Explicit `name_match: suffix|contains` compares simple or qualified names case-sensitively; opaque symbol IDs are not substring targets. Every explicit mode, including `exact`, requires a nonempty name. `include_count` requests the same qualified page-count metadata as symbols. Durable-key lookup excludes explicit matching/count options. `compact`, budgets and cursor control representation/paging.

**Result and limits:** Fact records with locations, signatures, scope and provenance. Syntax does not resolve a call target; an imported SCIP assertion is evidence from its producer, not a runtime call observation.

```json
{
  "name": "Owner",
  "kind": "definition",
  "path_prefix": "src/",
  "compact": false,
  "limit": 10
}
```

**Implementation:** [discovery.rs](../crates/airk-kernel/src/discovery.rs). **Contract/regression evidence:** [code_graph.rs](../crates/airk-kernel/tests/integration/code_graph.rs).

## document_facts

**Why and when:** Find written requirements, decisions, dates and structured document elements without treating prose retrieval as an inferred obligation.

**How it works:** The document-fact reader selects extracted statements/elements, preserving original page, block, row, cell or slide locations and extraction qualifiers.

**Inputs:** Optional `kind`, name/statement substring, language/profile and literal path prefix; alternatively a durable `document_key` alone. Supports compact output, budgets and cursor.

**Result and limits:** Cited statements and structured facts. Modality, headings and original locations are evidence to inspect; OCR and document extraction retain their stated limitations.

```json
{
  "kind": "requirement",
  "name": "retry",
  "path_prefix": "docs/",
  "limit": 10
}
```

**Implementation:** [discovery.rs](../crates/airk-kernel/src/discovery.rs). **Contract/regression evidence:** [instruction_documents_v2.rs](../crates/airk-kernel/tests/integration/instruction_documents_v2.rs).

## references

**Why and when:** Collect bounded exact identifier mentions for impact investigation, including strings/comments that a structural query could omit.

**How it works:** Indexed lexical candidates are verified against retained source for exact-case whole-identifier matches. A complete CamelCase identifier remains one identifier: references for `Owner` do not find the name `OwnerSession`. Use explicit suffix/contains definition lookup when that is the intended discovery.

**Inputs:** Exact `name`; optional literal `path_prefix`, indexed `language`, `evidence_kind`, `exclude_path_prefix`, `limit`, `max_bytes` and response options. A new filter, `cursor`, explicit `order: index|path|code_first`, `include_files` or `include_count` selects the paged contract. That contract defaults to deterministic stable-ID `index` order. Omit all new options to preserve the preceding selection and response contract. Repeat identifier, filters, order and representation with continuation; count/text budgets may increase within supported maxima, including to satisfy `required_record_bytes`.

**Result and limits:** Exact-case whole-identifier matches verified within lexical candidates, including comments, prose and scripts. These are not semantically resolved references or executed test coverage. The paged response adds `page` with continuation, incomplete reason and explicit `coverage`. UTF-8 source matches reconstruct complete physical lines across retained chunk splits, preserving original CRLF/line coordinates. Extracted non-UTF-8 text can instead return qualified fragments; inspect `count_unit` and `coverage.line_completeness`. When the next record cannot fit the current text budget, an empty page can carry `required_record_bytes` and a resumable cursor. A record over the absolute maximum or an artifact over finite scan/deduplication capacity instead reports a terminal incomplete result.

Optional file summaries count only returned-page records in the reported unit and have an additional 16-row/4-KiB metadata allowance beyond the text budget. `match_count` can be exact for the bounded `verified_lexical_candidates` scope, or a lower bound for `page_scan`; it is never a whole-query total. `coverage.query_complete` and `token_boundary_complete` remain false and the global query total remains unknown: an identifier split across index chunk tokenization can be absent from candidate postings. Candidate-scan completion and zero returned matches do not prove no uses. Use live verification or further scoped source reads when completeness matters. Scan, candidate, text and metadata limits remain distinct; a superseded-publication cursor fails explicitly.

```json
{
  "name": "Owner",
  "path_prefix": "src/",
  "language": "rust",
  "order": "path",
  "include_files": true,
  "include_count": true,
  "limit": 20,
  "max_bytes": 8192
}
```

**Implementation:** [lib.rs](../crates/airk-kernel/src/lib.rs). **Contract/regression evidence:** [code_graph.rs](../crates/airk-kernel/tests/integration/code_graph.rs).

## relationships

**Why and when:** Investigate supported incoming/outgoing structure around a known symbol with explicit depth and evidence limits.

**How it works:** The kernel traverses indexed structural edges or source-bound imported assertions under the current revision. Written target names remain candidates unless stronger producer evidence is present.

**Inputs:** `symbol` from returned evidence, direction incoming/outgoing, optional kind, literal path scope, `hops` from 1 to 3 and output budgets.

**Result and limits:** Bounded related nodes/edges with their basis and coverage. Current associated-type navigation is Rust-only: where retained same-file owner/scope evidence establishes declarations for an exact type/impl ID, bounded `suggested_targets` can provide written method spellings for follow-up queries. Duplicate names, aliases, cross-file impls, missing facts and ambiguous associations do not become resolved links. Incoming callers still attach to written targets such as `Owner::new`, not automatically to every type named Owner. Missing edges do not prove no impact; no complete call graph or runtime execution claim follows.

```json
{
  "symbol": "<returned-symbol-id>",
  "direction": "incoming",
  "kind": "call",
  "hops": 1,
  "limit": 20
}
```

**Implementation:** [lib.rs](../crates/airk-kernel/src/lib.rs). **Contract/regression evidence:** [code_graph.rs](../crates/airk-kernel/tests/integration/code_graph.rs).

## changes

**Why and when:** Reuse remembered evidence safely and learn what the index observed changing since an exact baseline.

**How it works:** The kernel validates remembered selectors or pages bounded retained changes. Independent coverage floors, publication identity and retained metadata govern what can be answered.

**Inputs:** `since` and/or typed path/artifact/symbol/document/citation selectors; optional literal `path_prefix`, cursor, inclusion flags and budgets. Consume every needed page before advancing the baseline.

**Result and limits:** Added/modified/removed observations, selector validity and explicit unavailable/history outcomes. It is not arbitrary historical search, a Git diff or proof of a filesystem move. Do not replay an expired token as a live-source read.

```json
{
  "since": "<returned-publication-token>",
  "path_prefix": "src/",
  "limit": 10,
  "max_bytes": 8192
}
```

**Implementation:** [changes.rs](../crates/airk-kernel/src/current/changes.rs). **Contract/regression evidence:** [changes_v2.rs](../crates/airk-kernel/tests/integration/changes_v2.rs).

## parse

**Why and when:** Share bounded evidence recipes between Workbench, MCP and CLI, instead of implementing an agent or unbounded tool orchestrator in the kernel.

**How it works:** The native Workbench runner resolves text, a named recipe or explicit steps into a finite plan, then optionally executes eligible read-only steps with publication, scope, call/fanout/byte/time budgets.

**Inputs:** `schema: airk.parse.request/1`; exactly one text/recipe/plan; mode prepare/execute. Preparation is the default. Explicit plan execution must retain required choices and exact revision identity.

Optional `reply_detail` supplies a default to steps that omit their own choice;
an explicit step choice overrides it. Effective arguments remain in the plan's
digest and per-step reports. Rich cannot recreate evidence omitted at indexing.

`plan`, `recipe`, `budgets` and `output` are advertised as optional JSON objects in
1.8.8. For an explicit plan, pass an object such as
`{"steps":[{"id":"ready","tool":"status","args":{}}]}`. Encoded strings,
arrays and booleans are rejected. Refresh a client's catalog if it retains an old
unconstrained/string declaration. Nested contents use canonical kernel validation.

All 17 inner read operations invoke the canonical MCP handlers and retain their complete full response in `steps[].result`, including `items`, provenance, readiness, coverage, continuations and error receipts. Workbench explicitly requests rich facts unless compact is selected. Inner reads share the outer plan's admitted lifetime and usage entry. Completed status/diagnostics steps include the configured `query_concurrency`; each observation retains its own time and cache identity. See the [interface parity audit](read-interface-parity.md) for defaults and transport differences.

Prepare/Execute requires the original publication for retained literal syntax/provider IDs, including pasted/raw-text forms; preparation cannot substitute today's token for that identity, so preserve stored text and use a retained inspection action or a fresh name/current binding.

Compact discovery resolves page-local file/provenance bindings before aggregation
and dependent reads; cited cards and complete exports retain the reconstructed
hash/location. `unique_definition_path` supports `outline`, `file_info`, `symbols`
and `code_facts`. For `selected_symbol_ids`, a compact row without IDs requires
an explicit `compact:false` symbols/code-facts lookup at that path first. This
read is charged to the plan, never performed implicitly. Malformed tables,
out-of-scope rows, ambiguous definitions and incomplete enumeration fail closed.

**Result and limits:** A prepared/needs-input/completed result with plan tier, per-step outcomes, citations and completeness. Finite supported parsing can execute when requested; learned proposals remain review-only until explicit plan submission. It never edits source or executes tests.

```json
{
  "schema": "airk.parse.request/1",
  "text": "Find the definition of Owner",
  "mode": "prepare"
}
```

**Implementation:** [bounded kernel runner](../crates/airk-kernel/src/workbench.rs), [canonical MCP read adapter](../crates/airk-mcp/src/workbench.rs). **Contract/regression evidence:** [native full-result comparisons](../crates/airk-mcp/src/workbench.rs), [actual CLI/MCP process checks](../crates/airk-cli/tests/mcp.rs).

## help

**Why and when:** Explain AIRK itself and discover the installed schemas without confusing product guidance with evidence from the user's repository.

**How it works:** The unified adapter combines native topics/generated CLI help/public schemas with optional bundled UI guide search. Native help does not require an indexed publication; UI guidance requires the verified bundled adapter, not an open editor window.

**Inputs:** Omit selectors for overview; use `topic`, `command`, `tool`, `query` or `topic_id` according to the advertised schema. Native line/byte continuation and UI guide detail options differ.

**Result and limits:** Documentation and schemas. It returns neither project search evidence nor current runtime health; use search/status/diagnostics for those questions.

```json
{
  "tool": "search"
}
```

**Implementation:** [help.rs](../crates/airk-mcp/src/help.rs). **Contract/regression evidence:** [explorerHelp.test.ts](../packages/vscode/src/test/explorerHelp.test.ts).

## list_templates

**Why and when:** Discover installed strategy IDs and availability before constructing a Studio proposal.

**How it works:** The optional Assistance adapter pages the bundled core pack without executing operators or opening a project index.

**Inputs:** `offset`, `limit` and `max_bytes`. This catalog call does not take a project selector.

**Result and limits:** Template IDs, versions, purposes, availability and `next_offset`. Preview availability is not execution qualification.

```json
{
  "offset": 0,
  "limit": 5,
  "max_bytes": 8192
}
```

**Implementation:** [tools.ts](../packages/vscode/src/explorerAssistance/tools.ts). **Contract/regression evidence:** [explorerAssistanceTools.test.ts](../packages/vscode/src/test/explorerAssistanceTools.test.ts).

## get_template

**Why and when:** Read the exact parameters, defaults and operator contracts of one installed strategy before filling it in.

**How it works:** The Assistance adapter looks up a known ID in the bundled core pack and returns its complete definition and referenced operator specifications.

**Inputs:** Exact `template_id` returned by list_templates; no project selector.

**Result and limits:** One inert template definition, budgets/instructions and runnable/preview information. Unknown IDs fail; this call does not execute a strategy.

```json
{
  "template_id": "airk.core/bug"
}
```

**Implementation:** [tools.ts](../packages/vscode/src/explorerAssistance/tools.ts). **Contract/regression evidence:** [explorerAssistanceTools.test.ts](../packages/vscode/src/test/explorerAssistanceTools.test.ts).

## draft_search

**Why and when:** Hand a proposed query to the user's Search form without executing it or changing collection policy.

**How it works:** The adapter schema-validates the envelope, then a matching trusted Explorer window revalidates binding/revision and admits the suggestion to its Inbox.

**Inputs:** `schema_version: 1.0`, idempotency key, title and nested `draft` query/mode/scope/budgets; optional workspace/target/evidence guards and project selector.

**Result and limits:** Proposal receipt and review state, never search hits. The user chooses whether to apply and run it. Retrying an idempotency key requires the identical body.

```json
{
  "schema_version": "1.0",
  "idempotency_key": "guide-search-001",
  "title": "Inspect retry handling",
  "draft": {
    "query": "retry",
    "mode": "lexical",
    "path_prefix": "src/"
  }
}
```

**Implementation:** [tools.ts](../packages/vscode/src/explorerAssistance/tools.ts). **Contract/regression evidence:** [explorerAssistanceHost.test.ts](../packages/vscode/src/test/explorerAssistanceHost.test.ts).

## draft_strategy

**Why and when:** Offer a reproducible Studio investigation recipe while retaining human control over execution.

**How it works:** The adapter validates a core template or supported finite operators/bindings and queues an inert Studio draft for a trusted window. Kernel/store do not depend on this editor adapter.

**Inputs:** Versioned/idempotent title envelope; template ID and parameters and/or bounded steps using supported operators. Capture binding/target revisions from diagnostics when guarding a concrete window.

**Result and limits:** Inbox proposal receipt. Applying fills a Studio draft; it neither executes the strategy nor saves it to a source folder.

```json
{
  "schema_version": "1.0",
  "idempotency_key": "guide-strategy-001",
  "title": "Collect retry evidence",
  "steps": [
    {
      "id": "find",
      "op": "airk.search",
      "args": {
        "query": "retry",
        "mode": "lexical"
      }
    }
  ]
}
```

**Implementation:** [tools.ts](../packages/vscode/src/explorerAssistance/tools.ts). **Contract/regression evidence:** [explorerAssistanceTools.test.ts](../packages/vscode/src/test/explorerAssistanceTools.test.ts).

## draft_plan

**Why and when:** Transfer a proposed task breakdown to Planner without writing a plan file or delegating work to agents.

**How it works:** The adapter validates bounded task IDs, dependencies and text, then the attached trusted window offers them in the Suggestions Inbox.

**Inputs:** Versioned/idempotent envelope and tasks such as T1/T2; optional objective, acceptance, instructions and relative path hint. There is no save field.

**Result and limits:** Proposal receipt. The user must apply and separately save any plan; no task or command is run.

```json
{
  "schema_version": "1.0",
  "idempotency_key": "guide-plan-001",
  "title": "Review retry behavior",
  "tasks": [
    {
      "task_id": "T1",
      "title": "Inspect cited source",
      "acceptance": "Verify the live implementation against indexed evidence."
    }
  ]
}
```

**Implementation:** [tools.ts](../packages/vscode/src/explorerAssistance/tools.ts). **Contract/regression evidence:** [explorerAssistanceTools.test.ts](../packages/vscode/src/test/explorerAssistanceTools.test.ts).

## suggest_view

**Why and when:** Direct a user to the appropriate Navigation, Review or Management screen while leaving actions under user control.

**How it works:** The unified wrapper chooses a nested schema by kind, then routes it to the same guarded proposal mechanism as other drafts.

**Inputs:** `kind` is navigation/review/management; `draft` contains the matching versioned/idempotent envelope and route, review scope or fixed management intent.

**Result and limits:** Inbox proposal receipt. Management guidance does not start indexing, repair, tests or service actions. Applied means the user copied the suggestion into a screen/draft, not that an operation executed.

```json
{
  "kind": "navigation",
  "draft": {
    "schema_version": "1.0",
    "idempotency_key": "guide-view-001",
    "title": "Inspect definitions",
    "route": "code/symbols",
    "filters": {
      "name": "Owner"
    }
  }
}
```

**Implementation:** [unified.rs](../crates/airk-mcp/src/unified.rs). **Contract/regression evidence:** [explorerAssistanceHost.test.ts](../packages/vscode/src/test/explorerAssistanceHost.test.ts).

## validate_draft

**Why and when:** Check a complete proposed tool argument object before offering it to the Inbox, without consuming proposal admission or idempotency state.

**How it works:** The same strict validator runs offline. If a trusted target is attached, the adapter can additionally check binding, target revision and idempotency conflicts.

**Inputs:** `kind` and `draft`, where draft is the complete corresponding proposal arguments including its envelope, not merely its inner Search/Studio form.

**Result and limits:** `valid`, errors and live-check availability, always `admitted: false` and `executed: false`. Offline validity does not establish that a particular live window will accept it.

```json
{
  "kind": "search",
  "draft": {
    "schema_version": "1.0",
    "idempotency_key": "guide-validate-001",
    "title": "Inspect retry handling",
    "draft": {
      "query": "retry",
      "mode": "lexical"
    }
  }
}
```

**Implementation:** [tools.ts](../packages/vscode/src/explorerAssistance/tools.ts). **Contract/regression evidence:** [explorerAssistanceTools.test.ts](../packages/vscode/src/test/explorerAssistanceTools.test.ts).

## proposal_status

**Why and when:** Distinguish pending, rejected, expired or applied UI suggestions instead of assuming that a submitted form was acted on.

**How it works:** The matching trusted window looks up the opaque proposal receipt and returns bounded review metadata without draft contents.

**Inputs:** `proposal_id` from a real proposal receipt and optional configured project.

**Result and limits:** Review state, reason, admitted/current/applied target revisions and expiry. `executed` remains false; applying a proposal does not establish source changes, execution or a saved file.

```json
{
  "proposal_id": "<returned-proposal-id>"
}
```

**Implementation:** [tools.ts](../packages/vscode/src/explorerAssistance/tools.ts). **Contract/regression evidence:** [explorerAssistanceHost.test.ts](../packages/vscode/src/test/explorerAssistanceHost.test.ts).

## get_handoff

**Why and when:** Retrieve only a complete Studio prompt or Planner task the user explicitly chose to share, instead of reading arbitrary editor drafts.

**How it works:** The adapter resolves an expiring opaque share token against the matching trusted window. Draft, binding or evidence-publication changes revoke the share.

**Inputs:** `token` supplied by the user, plus configured project where needed. Tokens are capabilities; do not fabricate or publish them.

**Result and limits:** The explicitly shared text, SHA-256, draft revision, workspace binding and evidence publication where present. It does not save, modify or execute the shared material.

```json
{
  "token": "<user-issued-share-token>"
}
```

**Implementation:** [explorerHandoffs.ts](../packages/vscode/src/explorerHandoffs.ts). **Contract/regression evidence:** [explorerAssistanceHost.test.ts](../packages/vscode/src/test/explorerAssistanceHost.test.ts).

## Workflows and maintenance

For a code investigation: projects → status → scoped search → file_info/outline → bounded read_file/context → exact symbols/facts/references as needed. Preserve the response identities rather than combining unrelated revisions into one asserted result. Use document_facts for written requirements and relationships for qualified structural candidates.

For a change check: retain the publication and selectors, call changes, consume its bounded continuation, then decide whether to recollect evidence. A history-unavailable result is an explicit gap, not permission to reconstruct history from live files.

For a UI proposal: read the appropriate template/help, explicitly inspect diagnostics if a current window binding/revision is needed, validate the complete argument object, submit once with a stable idempotency key, then inspect proposal_status. The user still applies/runs/saves it. For handoff, wait for a user-created share token.

When modifying a tool, read its schema, dispatcher, kernel/adapter contract and linked tests first. Preserve exact scopes, request/response identity, source read-only behavior, original locations, validators, budgets and failure distinctions. Update this guide and the catalog/native contracts together, and verify current names against the advertised catalog. Native catalog/argument tests live in [MCP lib.rs](../crates/airk-mcp/src/lib.rs), unified catalog/alias/deadline tests in [unified.rs](../crates/airk-mcp/src/unified.rs), and native transport/client qualification in [check-service-clients.py](../scripts/check-service-clients.py). A linked test is evidence for its asserted contract, not a claim of complete platform or language qualification.

## Request capacity in status, diagnostics and plans

Successful MCP `status` and `diagnostics` responses (including the `airk_status` and `airk_diagnostics` aliases) include top-level `query_concurrency`, for example `"query_concurrency": 16`, in both JSON text and structured content. It is the evidence request limit read by the frontend for this request's admission: default 16, configured choices 4, 8, 16, 32 and 64. The separate reserved status/control slot is additional. It does not report free permits, queue depth or guaranteed admission. For a multi-project bridge, it is the minimum configured backend limit used by that bridge.

CLI status/diagnostics include top-level `query_concurrency`. Workbench `/status` and `/diagnostics`, CLI-executed plans and MCP `parse` retain it inside each completed step result. The shared frontend/CLI response adapter reuses the outer request's admission observation; payload accounting and output limits still apply. Previously saved replies retain their original fields; rerun them with 1.8.7 or later to obtain the new data.

The value reuses the existing admission policy read; reporting adds no configuration read, collection access or service RPC. It is refreshed for each request even when the index status snapshot is cached. Original publication, observation time and cache age continue to describe the index snapshot. Error receipts remain unchanged. Both terse and full responses include the field; status retains its lightweight scope. The [bridge implementation and regression](../crates/airk-mcp/src/remote.rs) cover live capacity changes, aliases, cached metadata, text/structured agreement, error preservation and one policy read per request.

Status and diagnostics preserve recognized temporary publication/selection contention as the existing typed read retry receipt (`publication_busy` or `selection_busy`, with `action: repeat_same_request`). This is not evidence that the index is unavailable. Honor the exact retry delay and repeat only that read. Diagnostic runtime/work context does not suppress the receipt. Missing or damaged evidence retains its separate existing reporting path.

The 1.8.8 candidate retains the correction introduced on `5d8912b1...`:
genuine bounded quota-ledger
**read admission** uses the existing `selection_busy` hint with a **25 ms** delay.
Repeat only the identical read within its original deadline. Ledger mutations and
jobs are not replayed; job diagnostics retain `quota_ledger_busy`. Generic lookalike
text, permission failures and corruption do not acquire this hint.

See [CLI, MCP and Workbench parity](read-interface-parity.md) for the complete command audit, input/output contracts and regression coverage.
