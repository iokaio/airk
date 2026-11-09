# Change tracing and bounded development indexes

Status: proposed implementation plan, 14 October 2026. This document specifies future behavior; it does not change the current storage, retention or MCP contracts. Complete the foundation gate below before expanding [language coverage](additional-languages-plan.md).

AIRK should help an agent determine which indexed evidence changed, find the current location of remembered evidence, and retrieve only what it needs next. Optimize that edit/index/read cycle for code and documents while preserving original locations, collection isolation and bounded local operation. Claude Code and Codex use the same read-only contract; session memory, reasoning, editing, builds and committed history remain with the agent and its existing tools.

The supplied versioning/lineage proposal provides the starting direction. This plan separates observations from inferred continuity, makes incomplete history explicit, and replaces unsafe demotion and identity shortcuts with implementable publication and recovery protocols.

## 1. Architectural direction

Keep immutable published revisions, atomic pointer commits, content-addressed retained objects, native extraction, Tantivy, exact-fact SQLite and the selected disk-backed DiskANN implementation. Preserve the [crate dependency direction](architecture.md), one host per user/machine, independent collections and read-only source roots. No new crate, external database, cloud provider, agent orchestration or automatic model/provider invocation is required.

The [inspiration record](inspiration.md) describes ideas reviewed from Munarium and VCP, not imported implementations. The changes below replace AIRK's own current policies and assumptions; they do not require either reference project or access to private reference material.

| Current assumption to retire | Replacement |
|---|---|
| Every obsolete revision remains until an operator prunes it | Explicit bounded retention, automatic for new collections after qualification, with visible cleanup outcomes |
| A path and byte offsets are sufficient long-lived identities | Exact revision identities plus conservative file, code and document continuity handles |
| Reorientation requires repeated lists, outlines and reads | A bounded change query and evidence-specific conditional reads |
| Extraction reuse requires the same path | Content/profile-bound extraction reuse independent of continuity inference |
| Every small edit requires inference for every chunk | Verified embedding reuse first; incremental component construction only after differential qualification |
| Every metadata request can deserialize the whole inventory | A pinned catalog cache followed by indexed metadata queries |
| Retained full indexes are the primary means of remembering changes | A bounded metadata change window independent of old searchable components and source content |

This is an evidence change service, not a version-control system or an audit log. It records successful publications, not every save or filesystem operation. Changes that occur and disappear between scans are unobserved. A scan itself is not an atomic snapshot of a changing workspace. The revision identifies the exact retained bytes AIRK actually indexed.

## 2. Verified implementation baseline

The following observations come from this checkout, rather than assuming every statement in the proposal still applies.

| Component | Current behavior and implementation consequence |
|---|---|
| [Core contracts](../crates/airk-core/src/lib.rs) | Schema 3, extractor `native-facts/7`, compact `Snapshot` inventories and `ArtifactRef` objects. There is no publication lineage or incarnation in these contracts. |
| [Ingest scan](../crates/airk-ingest/src/lib.rs) | Reuse compares the same path's source hash and extractor version. Sources are read and hashed even on a reuse pass. Extraction failures can omit individual files; discovery/read/pass-budget failures preserve the previous publication. Absence therefore does not necessarily mean deletion. |
| [Kernel publication](../crates/airk-kernel/src/lib.rs) | Ordinary indexing, initialization and SCIP import publish revisions. [OCR and local provider operations](../crates/airk-kernel/src/analysis.rs) also publish. All these paths must use the new lineage finalizer. |
| [Persistent retrieval](https://github.com/iokaio/airk/blob/f9aa5168829dc8794d241644fb8483715c2dd58a/crates/airk-retrieval/src/persistent.rs) | A build writes complete Tantivy, optional DiskANN/model, inventory and exact-fact components. Every chunk is offered to the embedding model. Revision identity hashes the engine specification and serialized inventory. |
| [Retained objects](../crates/airk-store/src/blobs.rs) | Sources and extractions are verified content-addressed objects. Extractions contain their path; retained-object loading checks that path against the inventory. `catalog()` verifies and parses the compact inventory for each opening. |
| [SQLite storage](../crates/airk-store/src/sqlite.rs) | Exact facts and relationships already use immutable per-generation databases and pinned reads. Add file/change indexes here; do not create an unrelated database service. |
| [Retention](../crates/airk-store/src/retention.rs) | Explicit pruning protects active/leased generations, then marks objects from surviving inventories. Interrupted `removing-*` directories require operator recovery. There is no history-only representation. |
| [Scheduler](../crates/airk-store/src/scheduler.rs) | A read-only status path already exposes requested/completed/running work, pause, block and failure state. Queue watermarks are not index generation numbers. |
| [Syntax evidence](../crates/airk-ingest/src/syntax.rs) | Native symbol IDs contain path and byte offsets. [Code contracts](../crates/airk-core/src/code.rs) distinguish syntax candidates from producer assertions. Neither becomes compiler identity through change matching. |

The quota ledger accounts for logical storage; the usage ledger records activity. Neither should become a source-history ledger. Search components are already persistent Tantivy and disk-backed DiskANN in this checkout; the legacy lexical/SQLite snapshot adapter remains compatibility machinery.

## 3. Required correctness properties

1. Every response identifies one exact publication. Metadata, content, locations, redirects and relationships in that response come from that publication.
2. Failed scans/builds never advance generation, change token, stamps or active pointers. Successful publication followed by failed cleanup remains successful publication with a separate maintenance failure.
3. Content retention and change coverage have independent policies. Removing old searchable or historical content cannot erase the active revision's change window. Bounded metadata eviction can shorten that window and must say so.
4. An absent record is never sufficient evidence of unchanged content, deletion, an exact move or resolved identity. Gaps, extraction failures, unsupported capabilities and ambiguous matching are explicit.
5. Reader leases protect all objects and representations needed by an operation. Cleanup never overrides a lease, active revision, explicit checkpoint or source-root boundary.
6. Ordinary MCP reads never discover live files, index, invoke providers, prune, create checkpoints or write policy. Scheduler observations describe known pending work; an idle queue cannot certify live-source freshness.
7. Every persisted collection grows only within configured logical-byte quotas. Active data, temporary builds, leases and checkpoints can prevent retention targets from being met; then report pressure or fail publication safely.

## 4. Publication identity and evidence versions

### Collection incarnation and tokens

Create a full random 128-bit collection incarnation at the first successful publication in the new format, including initialization. Store it in the committed header and immutable inventory. Collection deletion/recreation creates a new incarnation; supported relocation preserves it. Bind all lookups to the configured collection and canonical root/alias identities. Never reuse the usage ledger's identity as the collection identity.

An opaque, versioned change token encodes the full incarnation, publication sequence and full revision digest. A short prefix may be displayed but cannot be accepted as identity. Every evidence response carries the exact revision and token, not just status: otherwise a status/read race can leave an agent remembering the wrong baseline. Preserve the existing numeric `generation` field for compatibility.

Add `parent_revision`, publication reason and publication time. The parent is the last committed revision, never an abandoned build. Generation is checked, monotonic and assigned once by the kernel finalizer. Time is diagnostic and useful for thinning; sequence establishes order despite clock skew. Unchanged scans do not publish merely to update a timestamp. Metadata compaction, when necessary, is an explicit maintenance publication.

Retain bounded committed revision headers with the metadata change window so a token can be checked after its component directory is pruned. Reject malformed, foreign-incarnation and future tokens. A token outside the verifiable ancestry window returns `baseline_unavailable` with coverage boundaries; equal generation alone never proves ancestry. A restored or independently copied root can fork history, so validate the revision as well as the sequence. Supported relocation continues the chain; arbitrary copied roots are not a synchronization mechanism.

Do not implement rollback by pointing at an older generation and reusing its sequence. A later operator restore, if justified, must publish a new revision with an explicit reason and parent. Retaining a predecessor provides recovery material, not an automatic rollback promise.

### Separate the things that can change

| Identity or stamp | Meaning |
|---|---|
| `artifact_id` | Collection-local continuity handle, assigned on first observation; independent of current path |
| `source_hash` | SHA-256 of exact retained original bytes; preserve the existing `content_hash` API spelling |
| `extraction_identity` | Source hash plus adapter/profile, extractor/chunker versions and extraction options/provenance |
| `evidence_digest` | Verified extracted text, facts, locations and applicable provenance, excluding administrative change stamps |
| `first_generation` | First observed publication for this artifact handle, not when the source was created |
| `path_generation` | Last publication that changed the artifact's citation path or root binding |
| `content_generation` | Last publication observing different original bytes |
| `evidence_generation` | Last publication changing extracted evidence or its interpretation/provenance |
| Revision-level retrieval identity | Model/tokenizer/preprocessing/index configuration; a change does not imply source or native facts changed |

Changes may set several dimensions. A rename preserves content stamps but advances the path stamp. A source edit that produces identical normalized text still advances the content stamp. An OCR option/extractor change can advance evidence without changing source bytes. Provider import/invalidation and model replacement have explicit revision-level events with affected domains, even when there are no file-content changes.

Do not fill thousands of per-file events solely for a global model/configuration change. Return that invalidation alongside file changes. Queries and conditional validators must account for it. Preserve current conservative source/build-bound invalidation of SCIP/LSP/Ctags evidence; never rewrite provider assertions to make a renamed file appear valid.

## 5. Metadata change window and storage

Use the existing generation publication boundary. The initial new format contains:

```text
generations/<revision>/
  inventory.json       file states, roots, identity, parent, lineage digest
  lineage.json         bounded revision headers, events and resolver records
  manifest.json        immutable format/component declarations
  catalog.sqlite       indexed projection of files, events, facts and relationships
  lexical/             existing searchable components
  vectors.bin          optional DiskANN component
  model/               optional verified assets
```

`lineage.json` contains no source excerpts or complete old artifacts. Each build carries forward the bounded metadata window, appends its own publication delta and emits resolver state. Hash its canonical bytes into the inventory before computing the revision digest. Avoid circular hashing: the current lineage entry identifies its sequence/parent; the enclosing revision provides its own digest. Earlier committed entries can contain their full revision digest. Bind the immutable catalog to the same inventory/lineage identity and validate it before commit. Query indexes are projections, not a second independently authoritative journal.

Keep current per-artifact stamps in the inventory. Keep immutable observed events separate from the mutable-in-the-next-revision resolver projection: chain compression must not rewrite the historical meaning of A-to-B followed by B-to-C. Events contain IDs, before/after paths and hashes, change dimensions, observation reason and inference basis. They do not keep an old object alive just by mentioning its hash.

Initial proposed metadata bounds are 10,000 total event/resolver/ancestry records, 16 MiB serialized lineage and 90 days of events, within the existing inventory/object/quota ceilings. Symbol/document detail shares this budget; it is not an unbounded nested array. Treat these values as tunable starting policies to benchmark. Live file stamps remain with live files; removed handles and forwards expire.

Evict whole oldest publication groups where possible. Track separate completeness floors for file events, code detail, document detail and resolution. If one publication exceeds the detail allowance, retain a bounded gap marker and advance the affected completeness floor through that publication; never pretend to have a complete branch-switch delta. Reserve space for floors/global markers before detail records. Age expiry occurs at the next publication/maintenance opportunity, not by silently mutating an active revision.

### Query meaning

`airk_changes since=S` reports observed changes in `(S, target]`, coalesced by artifact/continuity handle. This is not a byte-for-byte endpoint diff: edit-then-revert is still an observed change. An artifact created and removed within the window remains an explicit transient event. Moves, path reuse, extraction omissions and scope removals remain distinguishable. Last-change stamps alone cannot represent all these cases.

Within the complete window, the response must account for every matching observed change. Outside it, live stamps and remembered hashes can still provide useful validation, but the response marks the affected dimensions incomplete. `not_found` with no retained tombstone is `unknown`, not `deleted`. Callers needing old content ask for an explicitly retained revision separately.

### Catalog access

First cache one verified `Arc<Catalog>` per current revision in each kernel process, preserving its lifecycle and reader leases. Recheck the committed header at every outer request; reuse only on exact revision equality. Release superseded idle caches, bound concurrent/historical handles, and expose cache pins in maintenance diagnostics. Never hold a cache mutex for extraction or long disk work.

Next add indexed `files`, `change_events`, `path_resolutions`, `revision_history` and continuity-key columns to the existing immutable SQLite catalog. Index exact path, artifact ID, source hash and `(generation, event_id)`. File lists/info/change queries should not load or scan all inventory JSON on the warm path. Retained reads load only the selected artifact. Existing textual references may still scan bounded text; catalog caching does not make that operation an indexed reference engine.

Historical reads can use a verified compact inventory without retaining a full SQLite fact catalog. No requirement for historical lexical, vector or relationship search is introduced.

## 6. File moves and extraction reuse

Continuity describes an observation, not filesystem intent. Preserve an `artifact_id` across same-path edits and unambiguous observed moves. A deletion followed by a later addition at the same path gets a new handle. Replacements that occur entirely between scans may be indistinguishable from edits; document that limitation.

Match removed and added paths only after a complete eligible scan. For cross-alias matches, require both aliases to remain present with unchanged root bindings. Removing/rebinding an alias is a scope change, not a file move. Never compare across collections.

| Evidence | Result and permitted behavior |
|---|---|
| Exactly one removed and one added entry with the same source hash and compatible extraction profile | `exact_content_move` continuity candidate; preserve handle under this documented rule |
| Several removed/added entries with identical bytes | Ambiguous candidates; filename/directory resemblance may rank them but cannot make the match exact |
| Removed file's bytes survive at an existing path | `equivalent_content` candidate; useful lookup evidence, not proof that the artifact moved |
| Empty/tiny/common boilerplate with weak identity | Do not auto-forward solely from hash equality; expose ambiguity or absence |
| Move plus edit | Later bounded similarity candidate; no automatic forwarding or identity transfer |
| Several consistent child moves | Later directory suggestion; verify each requested target exists and respect path-component boundaries |
| File omitted after exclusion, size/format limitation or failed extraction | `removed_from_index` with known reason or `unknown`; never automatically label it a source deletion |

Native events and editor rename hints can narrow work, but hashes, scope and the completed inventory remain authoritative. To distinguish deletion from exclusion, add a bounded previous-path reconciliation pass using ingest eligibility rules. Charge its source metadata/read work to the pass budget. If the cause cannot be established safely, report absence with unknown cause.

Resolution first checks the selected revision's live exact path. A path-only read never bypasses a reused live path to reach an older artifact. Hash/token/handle validation can instead explain that the old artifact moved and a different artifact now occupies the path. File forwards are handle-bound, chain-compressed in the resolver projection and cycle-checked; deleted targets become terminal records. Caps or expiry produce `resolution_unavailable`, not a guess.

Add an opt-in `follow_moves` read parameter, default false initially. Auto-follow only a unique exact-content match within the complete resolver window and the requested scope; annotate requested/resolved path, inference basis and revision. A supplied remembered hash must match the historical source of the redirect. Similar, directory and equivalent-copy candidates always require explicit target selection. An unobserved delete-and-create can resemble a rename even with identical bytes, so avoid the unqualified label `exact move`.

### Reuse must happen before extraction

The proposal's end-of-scan move detection alone cannot save parsing already performed by the scanner. Build a bounded prior extraction lookup keyed by `(source_hash, extraction_identity/profile)` before the scan. After reading/hashing a new path, reuse compatible extraction before invoking its adapter; final continuity matching remains a separate later step. Duplicate content may safely reuse extraction while identity stays ambiguous.

For the transitional path-bearing format, load one prior artifact, structurally rebind path-dependent native IDs/owners/citations, verify every reference, then retain a new artifact object. Do not perform unrestricted string-prefix substitution. Imported assertions are invalidated, not rewritten. OCR reuse requires equal source and full provenance/options; it must not trigger OCR or broaden selected pages automatically.

The target format makes extraction payloads path-independent, using local declaration/owner handles and document coordinates. Inventory/catalog binding supplies the artifact path and public IDs. Store lookup adapters in `airk-store`, extraction interfaces in `airk-core`, and matching/extraction policy in ingest/kernel. Keep one-artifact-at-a-time memory behavior; do not collect all candidate source text to detect moves.

## 7. Code and document continuity

Retain exact revision-local `symbol_id` and producer identities. Add opaque collection-local `symbol_key` and `document_key` handles as separate fields. Their validity always includes incarnation, evidence basis and revision. A key says AIRK matched evidence across observations; it is not proof of semantic equivalence or permanent identity.

Do not define a stable key as a hash of path plus ordinal. Moving a file, inserting an overload or adding a repeated heading would renumber unrelated entities. Instead persist assigned handles and reconcile old/new extracted facts with deterministic, bounded matching:

1. Match established artifact continuity first, then owner/scope continuity.
2. Match unique compatible language/profile, qualified name and kind; use untruncated signature/body fingerprints to disambiguate overloads.
3. Permit a signature/body edit to retain a handle only when the remaining identity evidence is unique. Equal-ranked candidates produce `ambiguous`; do not transfer an old key by source order.
4. Track name, signature, body, documentation and location changes separately. Line shifts preserve handles when matching is unambiguous.
5. Start new handles on unsupported adapter changes or uncertain split/merge matches. Mark the coverage gap rather than inventing continuity.

Fingerprints are versioned syntax-derived hashes, not normalized semantic programs. Preserve meaningful literals and tokens; stripping all whitespace can change string contents. Hash complete bounded source spans, not truncated display signatures. An ordinary digest cannot be compared for similarity: optional sketches must come from the underlying normalized text. Cross-file function extraction or renaming yields candidate matches unless an explicit deterministic rule establishes continuity.

Code queries accept either exact `symbol_id` or explicit `symbol_key` selectors with mutual-exclusion validation. Bind native edge sources to continuity handles where known; syntax target strings remain unresolved and must not acquire a target key merely because a name matches. Producer assertions remain source/build/producer-bound. Never redirect an imported producer ID using native heuristics.

| Document unit | Initial continuity evidence | Required limitation |
|---|---|---|
| Written requirement ID | Unique ID in the artifact and compatible category | Repeated IDs are ambiguous; equal IDs across documents do not imply the same requirement |
| Markdown/DOCX heading or section | Matched parent heading, text/kind and neighboring structure | Heading paths and ordinal positions alone are not durable IDs |
| Paragraph/list/decision/acceptance unit | Exact normalized text within matched section; later bounded similarity | Edited or repeated paragraphs may be unmatched; AIRK does not infer intent or acceptance |
| DOCX table/XLSX row or cell evidence | Existing table/sheet/row/cell coordinates plus bounded content matching | Row insertion and sheet rename can invalidate coordinates; never assume a business primary key |
| PDF/PPTX/OCR evidence | Source/page/slide/region provenance and extracted text | Layout/order/recognition changes may prevent matching; unchanged bytes can yield different evidence |

Document deltas include changed statements, written quantities/dates/modalities and structural locations only when the parser provides that evidence. They do not assert fulfilled requirements, changed meaning or code impact. Expose old/new evidence digests and locations by default; retrieve text separately. Shared chunk/continuity interfaces should support future language adapters without implementing new languages in this work.

## 8. Read-only API for agents

Add one MCP tool, `airk_changes`, backed by the same kernel service as `airk changes`. Extend capabilities/status and a small set of existing read tools. Keep client-specific configuration out of the kernel and retain existing tool names and permissions. The surface grows from fourteen to fifteen tools when this feature is enabled.

### Change query contract

Two modes share the tool: incremental changes with `since`, and validation of remembered evidence. Optional remembered selectors can accompany `since` and return a separate validation list. Validation requires at least one path, citation, artifact handle or continuity key; it cannot become an accidental unbounded listing.

| Input | Proposed bound/meaning |
|---|---|
| `since` | Opaque full change token; baseline is exclusive |
| `paths`, `citations`, `symbol_keys`, `document_keys` | At most 64 selectors total and 32 KiB input; citations may supply source/evidence hash and prior token |
| `path_prefix` | Literal collection-relative prefix, existing 4,096-byte bound; match either endpoint for moves |
| `include_symbols`, `include_documents` | Default false; detail availability and truncation reported independently |
| `limit`, `max_bytes` | Existing 1..100 and 1..65,536 bounds; new change responses charge all serialized change/validation records, including candidate paths |
| `cursor` | Versioned bounded continuation, binding incarnation, target revision, baseline, filters and deterministic position |

Pin the target revision once for all pages. A cursor does not hold a cross-request lease; if its revision is no longer available, return `cursor_expired` and the current token. Never silently continue against a different target. Keep the unpaged metadata envelope bounded separately and report it in usage measurements. If a single record cannot fit, return a bounded budget error rather than an empty non-progressing cursor.

Return the following independent dimensions, rather than a single overloaded `history: complete` flag:

```text
from_token, to_token, collection, incarnation, generation, revision
coverage: { files, code, documents, resolution, complete_since, reasons }
publication_changes: [ model_changed | extractor_changed | analysis_invalidated | ... ]
changes: [ bounded coalesced records with handles, paths, stamps and inference basis ]
validations: [ unchanged | modified | moved_candidate | removed_from_index |
               path_reused | ambiguous | unknown ]
freshness: { observation_time, queue_state, requested_work, completed_work,
             last_success, last_error, live_source_verified: false }
truncated, next_cursor
```

`coverage` can be complete for files while code/document matching is unsupported or partial. Pagination truncation is separate from missing history. Do not let a caller advance its saved baseline until all required pages are consumed; return a stable `to_token` on each page. Failed or abandoned builds are status failures, not change events.

Read scheduler status through the existing read-only path. Distinguish pending, running, paused, blocked, unregistered and unavailable states, and report observation time. Preserve serviceability if scheduler status cannot be read: evidence remains valid for its revision, with freshness unknown. Do not equate queue revision numbers with generations or describe `pending=false` as proof that an agent's edits were indexed.

### Conditional and historical reads

Use an `if_evidence` validator for `read_file`, `context`, `outline` and `file_info`. Bind it to selected artifact, evidence/provenance digest, resolved path/root and the representation's request selectors/options. Include provider-domain identity where the result includes imported facts. A raw source hash alone cannot suppress changed OCR text, extractor output, locations or metadata. Return `unchanged` only for the same representation, with its exact revision/token. A `known_hash` convenience can validate source bytes but must not mean all evidence is unchanged.

Add optional exact `revision` selection for these bounded read services after history support ships. Do not accept an unscoped generation number as persistent identity. Return `served_revision`, `current_revision`, `historical`, original root/location and current retention availability. Missing history returns `history_unavailable`, never live disk or the current revision as a fallback. Outline can use retained facts; historical graph/vector/search APIs are deferred.

Expected missing/moved/expired states are bounded structured outcomes using the existing MCP result conventions; malformed arguments and corrupt storage remain explicit errors. Reject absolute paths, traversal, foreign aliases and forged selector bindings. File text, change labels and candidate names remain untrusted evidence, never instructions.

### Agent workflow

An agent stores the token from the evidence it used. After edits or resumption, it calls `airk_changes`, checks known pending/failure state and coverage, then fetches only affected evidence. Before editing, it verifies current workspace bytes with its existing file tools. After compaction it can validate selected remembered citations even when a full delta is no longer available. Shared tokens allow several agents to compare the same collection without AIRK maintaining sessions or coordinating their work.

Keep initialization guidance within the repository's existing 512-character budget and validate actual client behavior through the [tool alignment checklist](agent-tool-alignment.md). Do not claim live Claude Code/Codex compatibility or context savings from schema tests alone. No automatic edits to AGENTS.md, CLAUDE.md or client permission lists are part of this plan.

## 9. Retention, history and recovery

### Policy and compatibility

Implement a validated per-collection `retention.json`, visible in CLI/extension status and prune previews. Recommended defaults after qualification:

| Policy | Initial value |
|---|---|
| New collection mode | Automatic, explicitly shown during setup |
| Existing collection upgrade | Preserve manual mode until the operator selects automatic retention |
| Full searchable revisions | Active plus one committed predecessor; leases can retain more |
| Optional history | Three most recent historical revisions, then newest per hour up to 24 hours; at most 12 total historical revisions |
| History logical bytes | Lesser of 256 MiB and 10% of effective collection quota, including checkpoint history |
| Checkpoints | At most four named retained revisions; creation fails if count/byte allowance cannot admit one |
| Lineage | The separate count/byte/age bounds in section 5 |

Recent/hourly selections form a union before applying caps. Protect checkpoints, then favor the newest unpinned history. Count each history-exclusive retained object once by content-addressed identity, plus history metadata, rather than once per revision. Active/searchable dependencies remain in total quota accounting. A shared object's transition into history-only ownership can increase history bytes even with no new object. Leases/checkpoints can leave policy targets exceeded; display the reason and actual retained bytes.

Automatic retention changes the documented manual-only behavior. Update [collections](collections.md), [operations](operations.md), [quotas](storage-quotas.md), extension controls and security guidance in the implementation phase that activates it. Do not silently apply a new eviction policy to existing installations. Manual mode continues to require explicit pruning; pressure in manual mode yields a useful failure, not an implicit policy change.

Checkpoints are CLI/extension mutations pinning an already committed revision. They do not read Git or capture unindexed edits. Do not install hooks or services as a side effect. Checkpoint names are bounded labels, never filesystem paths. Changing policy cannot silently revoke checkpoints.

### Safe historical representation

Do not demote by deleting files from a live immutable generation or deciding that missing `lexical/` means valid history. Missing required components currently mean corruption, and that distinction must remain.

Introduce `history/<revision>/` packages containing the original verified inventory, original manifest, lineage payload and a versioned history descriptor. The descriptor declares supported read capabilities and binds the copied metadata digests. It does not change the original revision identity or imply search components exist. Source/extraction objects remain in the existing object store.

Conversion holds the collection writer lock and the revision's exclusive reader lease. Build and charge a private history package, verify it, sync it and atomically publish it before detaching full components. Use a durable, quota-accounted maintenance intent binding incarnation, revision, source/destination and action. Readers acquire the same revision lease before choosing a representation. During overlap, both representations must describe identical evidence. After a crash, either the full revision or verified history remains usable; an intent can finish detaching disposable components.

Generalize GC to mark from every surviving full/history inventory and checkpoint/lease-protected revision. Also mark future shared models/vector caches through explicit manifests. If a surviving root cannot be verified, stop object deletion and report the failure. This requires GC changes; today's scan of `generations/` alone is insufficient. Retained event hashes do not count as object roots.

Only recover detached `removing-*` directories associated with a validated maintenance intent and matching collection/revision identity, after checking the active pointer and acquiring the canonical revision lease. Unknown paths, malformed markers, links or foreign roots require inspection. Idempotent recovery must work before and after each rename, pointer commit and delete. Relocation, collection removal and usage accounting must understand history and maintenance intents too.

### Cleanup scheduling and quota pressure

Run bounded maintenance after commit through a common kernel publication wrapper, covering initialization, index/reindex, imports and OCR/provider publications. Release obsolete local cache/build handles first. Use try-exclusive leases and never wait indefinitely behind readers. Bound each pass by candidate count/bytes and cooperative elapsed checks; persist remaining work for later writer/worker maintenance. Synchronous filesystem calls are not hard-preemptible deadlines. Idle MCP calls do not perform maintenance.

Quota exhaustion permits one automatic-policy cleanup-and-retry cycle only. Abort the failed staging attempt, close all reservation clones and release temporary allocations before recomputing quota. Retain the last committed revision throughout cleanup. Acquire a fresh reservation and rebuild/revalidate the candidate, since GC may have removed unreferenced candidate objects. Do not retry corruption, parse failures or arbitrary I/O failures as quota problems. Do not rerun external providers automatically; retry their publication only if retained output remains verified and admissible, otherwise report an explicit retry requirement.

Cleanup cannot guarantee publication will fit. If active data, two-generation build headroom, other collections, checkpoints or reader pins exhaust capacity, keep the active revision and report `quota_blocked`, needed/available capacity where known and protected bytes. Keep scheduler failure/status visible. Never prune another collection without that collection's policy and lifecycle locks.

Hold the existing lifecycle-before-writer ordering; use nonblocking generation lease acquisition, then short quota-ledger operations following the existing quota contract. Never hold a quota transaction while waiting on a writer/reader. Qualify relocation, removal, indexing and cleanup together for deadlocks.

Persist a bounded last-maintenance summary: committed publication, demoted/removed counts, reclaimed logical bytes, protected revisions, pending work and errors. Detailed reports may be truncated explicitly. A crash before summary persistence leaves maintenance status unknown/pending, never retroactively changes the publication outcome.

### Privacy and purge

History retains deleted content; lineage retains old paths/hashes. Add a separate preview/apply purge operation for historical content and, when requested, lineage metadata. Clearing lineage publishes a new revision with advanced completeness floors; it cannot edit an immutable inventory in place. Clearing checkpoint protection must be explicit. Active evidence and live leases remain protected, with deferred deletions reported.

Purge is logical removal of unreferenced AIRK state, not secure erasure of SSD blocks, backups, old relocated roots or external copies. Removing history cannot erase a secret still present in the active indexed source. Document this in [SECURITY.md](../SECURITY.md), including the need to remove/reindex current content and address retained roots separately. Do not claim purge succeeded while protected copies remain.

## 10. Publication-cost optimization

Optimize measured work in this order. Keep a full-build path as the correctness reference and recovery fallback.

| Step | Implementation | Qualification boundary |
|---|---|---|
| Catalog cache and indexes | Reuse verified metadata; indexed file/change lookup | Warm single-file requests must stop reparsing the whole inventory |
| Content-keyed extraction reuse | Pre-extraction lookup; later path-independent payloads | Same source/profile rename must avoid native extraction; duplicate bytes need not imply identity |
| Embedding cache | Content-addressed vectors keyed by complete model/tokenizer/preprocessing/pooling/normalization/dimension identity and exact embedding-input bytes | Unchanged input must avoid inference; model/option changes invalidate it; verify values/checksums/dimensions and quota before use |
| Stable bounded chunks | Code declaration boundaries with capped splits; document heading/paragraph/table/page units; bounded fallback for oversized units | Inserting text near the top should not invalidate every later chunk; preserve original locations and retrieval quality |
| Shared model objects | Verified collection-local model assets referenced by manifests rather than copied into every generation | Lease/GC/relocation/quota integration first; no cross-collection lifetime coupling |
| Incremental exact catalog | Copy verified prior catalog into private charged staging, then update affected file/fact/edge/change rows | Copy and journal/temp costs remain bounded; global extraction/provider/schema changes trigger full rebuild |
| Incremental Tantivy | Prototype only with supported immutable segment ownership and private writer metadata | Never mutate/hard-link writable files or use platform-specific link tricks as the baseline; compare results/citations with full builds |
| DiskANN construction | Initially rebuild the graph from cached vectors; reuse whole components only when their complete input identity is unchanged | Cached embeddings do not make graph construction incremental; mutable overlays/deletes/compaction need a separate measured design |

Keep logical-byte accounting by owned managed files/objects. Do not change quota semantics to “count shared inodes once” to accommodate hard links. Copy-on-write/segment sharing is an optional later implementation detail with independent accounting and platform evidence. Local providers remain explicit and model caches never trigger downloads or remote fallback.

Preserve deterministic lexical ordering when replacing global chunk ordinals: use a defined path/chunk ordering and explicit tie-breaking, including aliases and Unicode paths. A chunk identity is distinct from its location, and vector IDs remain revision-local. Include path in the embedding cache key if the actual model input includes path. Repeated text at different locations can reuse a vector while retaining separate citations.

Chunk changes must preserve exact source/document coordinate mappings and avoid omitting or accidentally duplicating evidence. An incremental build must replace dependent relationships as well as directly edited rows; native unresolved targets do not justify inferred cross-file dependency closure. ANN output need not be byte-identical if graph construction is nondeterministic: compare recall, bounded work and citation validity against a defined full-build baseline. Lexical/exact results should match exactly under deterministic ordering.

## 11. Implementation ownership and delivery gates

| Owner | Changes |
|---|---|
| `airk-core` | Versioned identities, evidence validators, bounded delta/resolution contracts, capability flags and extraction/cache traits; no filesystem/parser/transport code |
| `airk-ingest` | Eligibility-aware disappearance causes, prior-content reuse, native continuity descriptors, document/code matching and later sketches/chunking |
| `airk-store` | Persisted formats, catalog projections, lineage/history validation, policies, leases, checkpoints, GC, quota integration and recovery intents |
| `airk-retrieval` | Embedding-input identity/cache callbacks, chunk/index integration and later incremental components; continue depending only on core |
| `airk-kernel` | Shared publication finalizer, change/validation services, pinning/caches, scheduler observations and maintenance orchestration within the collection |
| `airk-mcp` | One new read-only tool, compatible selectors/envelopes, errors, capabilities and concise instructions |
| CLI / extension | Policy previews, explicit checkpoint/purge mutations, pending/error/retained-byte visibility and guidance; call kernel services |

Each phase should be delivered as reviewable changes with fixtures before enabling defaults. Proposed commands in this plan are future interfaces, not commands available today.

| Phase | Deliverable and dependency | Exit gate |
|---|---|---|
| 0. Baseline and safe read wins | Instrument scan/hash/extraction/inference/component/publication/cleanup costs; cache pinned catalog; expose known scheduler failures; record format decisions | Cold/warm benchmark baselines, bounded cache pins, unchanged public behavior and passing regression suite |
| 1. Identity and publication contract | Common finalizer for every writer; incarnation/token/parent; per-dimension stamps; versioned lineage payload with gaps; bounded catalog file/change indexes | Atomic fault tests, no-op and failed-publication behavior, migration and cross-root/incarnation rejection |
| 2. File changes and agent contract | `airk_changes`, validation, pinned pagination, structured missing outcomes, exact-content candidates and extraction reuse, representation validators | Rename/copy/path-reuse/omission tests, complete-versus-partial deltas, retained-object reads and real client task trials |
| 3. Bounded searchable retention | Policy/keep-set pruning, maintenance intents/recovery, reporting and one pressure retry; manual migration policy | Repeated-edit storage plateau within policy, live-reader protection, quota/cancellation/crash tests; only then enable automatic mode for new collections |
| 4. Development evidence continuity | Code and document handles, change dimensions, conservative matching and path-independent extraction format | Stable handles for unambiguous line/paragraph insertion and moves; explicit ambiguity, provenance and coverage on unsupported cases |
| 5. Optional short content history | Verified history packages, thinning, checkpoint/purge commands, exact historical reads, generalized GC/relocation | Crash-safe conversion, history byte accounting, expired revision behavior and privacy documentation |
| 6. Measured build improvements | Embedding reuse and chunk stability first, then incremental catalog/lexical experiments; depends on identity/GC contracts | Differential results, held-out retrieval judgments and measured inference/build/disk improvements |

The **pre-language-expansion gate** is phases 0–4 plus qualified embedding reuse and chunk/extraction identity interfaces from phase 6. History reads, fuzzy/directory moves and segment-level reuse can follow without blocking new grammars. This prevents the optional history tier from delaying the core agent workflow while ensuring new adapters inherit the right identity and caching contracts. New languages must declare continuity/chunking capabilities rather than fabricate unsupported keys.

Fuzzy matching is a later optional slice, not required for basic moves. Start with a deterministic comparison cap (10,000 candidate pairs), bounded sketches and a cooperative 250 ms target. A time cutoff can vary by machine, so record partial detection and deterministic candidate order. Calibrate score thresholds using positive and negative fixtures before choosing defaults; a generic 0.6 similarity threshold is not identity evidence.

## 12. Migration and failure design

Use explicit independent versions for inventory/storage, lineage, catalog schema, continuity matching, chunking and engine format. Adding `serde(default)` fields is not enough: even empty newly serialized fields can change the bytes used by `revision_id`. Preserve legacy decoding and original-byte digest validation; never reserialize an old snapshot with the new encoder to derive its old directory name.

Keep old revisions readable without retroactively claiming lineage. The first successful new-format publication establishes the incarnation/baseline and sets pre-migration coverage unavailable. Preserve policy, source/root binding and existing imported-evidence validity rules. Build migration privately under quota, reopen/verify, then commit once. Failure leaves the legacy pointer usable. Migration should not require re-extraction unless the extraction format/profile changes.

Bump storage/pointer versions where necessary so old binaries fail closed on new state rather than publishing an old-format child that drops lineage. Mixed-version workers need capability/version diagnostics and an upgrade/restart procedure. Existing new-version readers pinned to old revisions may finish. Downgrading a binary is not a schema downgrade; document supported restoration/reindex options without deleting user state automatically.

Cover every mutation path in the migration inventory: initialization, ordinary/full indexing, SCIP import, local provider publication, OCR, root configuration becoming effective, retention policy, checkpoints, purge, removal and relocation. Keep old analysis invalidation conservative. No component may update lineage or stamps outside the same successful publication that makes its evidence visible.

## 13. Acceptance and measurement

Use original synthetic/public fixtures, never private indexed material or reference repositories. Extend existing [kernel persistence](../crates/airk-kernel/tests/integration/persistent.rs), [streaming](../crates/airk-kernel/tests/integration/streaming.rs), [root](../crates/airk-kernel/tests/integration/roots.rs), [scheduler](../crates/airk-kernel/tests/integration/scheduler.rs) and [code graph](../crates/airk-kernel/tests/integration/code_graph.rs) tests, plus store and MCP contract tests.

| Area | Required evidence |
|---|---|
| Observation semantics | No-op, edit/revert, create/delete between publications, source edit with equal extracted text, extractor/model-only publication, provider import/invalidation, OCR rerun and failed publication |
| File continuity | Unique rename, move+edit candidate, case-only rename on applicable filesystems, alias move/removal/rebinding, copy/delete, duplicate/empty files, A-to-B-to-C, target deletion, swaps and path reuse |
| Incomplete evidence | Ignore/unsupported/oversize/extraction omission, ancestry expiry, metadata cap within one large publication, independent code/document gaps and unsupported adapters |
| Code/documents | Edit above declaration, overload insertion/reordering, duplicate scopes/names, signature/body/doc changes, duplicate requirement IDs/headings, paragraph insertion, row/sheet change and OCR provenance change |
| Read safety | Exact revision binding across concurrent publish, conditional-validator invalidation, stable paged results, expired cursors, historical read lease, missing/corrupt objects and unsafe path/alias selectors |
| Lifecycle | Two readers plus writer, long-lived cache pin, checkpoint overflow, low quota before reserve/during build, cleanup failure after commit, process exit at each maintenance transition, relocation/removal interaction |
| Isolation | Same path/hash in different collections, recreated collection, restored/forked state and relocated root; no cross-scope forwarding or GC |
| Optimization | Full versus optimized exact/lexical results, vector recall/citations, cache collision/corruption handling, model/options invalidation and parser invocation counts |

Extend [scale benchmarking](../scripts/benchmark-scale.py) and [code benchmarking](../scripts/benchmark-code.py) with 200 successful small-edit publications, 300-file moves, a large branch-like replacement, mixed code/documents, repeated boilerplate, and lexical/optional local-model modes. Run within configured ingestion limits; report any selected larger profile explicitly.

Measure cold/warm p50/p95 read latency, publish latency, source bytes read, parser and inference calls, cache hits, component writes, logical retained/staging bytes, available process RSS/CPU/I/O and cleanup duration. Distinguish OS transfer counters from physical I/O. Report machines, corpus/fixture hashes, model/configuration, policy, measurement method and unavailable counters.

Initial acceptance is structural and comparative: warm metadata lookups avoid full inventory parsing; identical embedding inputs avoid inference; compatible moved content avoids extraction; unpinned retained storage reaches a policy-bounded plateau; optimized retrieval preserves qualified results. Set absolute latency/size targets after phase 0 measurements, rather than inventing performance promises.

Run paired Claude Code and Codex tasks for session resumption, edit/index/revalidate, stale-path recovery, document requirement changes and concurrent edits. Record success, tool round trips and serialized evidence bytes using [usage reporting](usage-reporting.md). Token/billing reductions require separately supplied calibrated evidence; fewer bytes alone do not prove billed savings. Record the real client versions and tool-selection behavior.

For every implementation phase, run the checks in [CONTRIBUTING.md](../CONTRIBUTING.md): workspace build, formatting, clippy, workspace tests, repository checks, license notices, Python contract tests and whitespace checks. Add extension tests when changing extension behavior and container checks where available. Qualify Windows, macOS and Linux independently, especially leases, rename/recovery and filesystem case behavior. Unsupported/network filesystems and unperformed power-loss campaigns remain unqualified.

## 14. Deferred scope and completion criteria

Do not add unlimited history, line-level diffs, historical full-text/vector search, automatic source rollback, Git integration, cross-collection lineage, inferred requirement-to-code compliance, universal symbol resolution or a mutable multi-tier vector engine in this work. Checkpoints and history are optional bounded evidence retention, not backups.

The foundation is ready for additional languages when an agent can identify its exact baseline, receive a truthful bounded delta, validate remembered code/document evidence, recover conservative path continuity, and observe indexing/retention failures; ordinary edits reuse eligible extraction/embedding work; storage converges to its configured targets when no protected readers/checkpoints prevent it. Completion requires the stated tests and measured evidence, not only new schemas or tool descriptions.
