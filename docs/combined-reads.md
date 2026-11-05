# Combined Project reads

P7 is implemented on the unpublished **1.8.8** working build. Actual native
transport suites passed on Windows release and Apple Silicon debug pairs;
Windows legacy latency and selected-index fan-out gates passed. The
[implementation ledger](research/implementation-v190.md#p7--combined-retrieval-and-consumers)
records source/payload identities, consumer checks, prior failures and limits.
P7 installed-editor, Linux, Intel and release-signing qualification was not run.
The [design contract](decisions/002-projects-indexes-and-query-scope.md) remains
authoritative. This document describes the implemented contract.

A combined session selects one native-reviewed grant, explicit `project_ids` and
an optional `index_ids` subset. IDs and saved preferences grant no access. The
native owner validates current membership, source and policy revisions and fences
delivery. A legacy `project` keeps its original one-index meaning and cannot be
mixed with Project selectors. Missing configuration never selects the machine's
library or the Workbench dropdown implicitly.

Each selected index runs once, even when several selected Projects contain it.
Its authority is the component-bounded union of those selected memberships within
the frozen reviewed grant. A returned source is attributed only to selected
memberships that permit that source. Identical paths or bytes in distinct indexes
remain separate origins. File-specific follow-ups select one originating index;
an ambiguous path does not select an arbitrary member.

Composition starts with fan-out one, at most 32 unique indexes and deterministic
budget allocation in canonical index-ID order. Result count, text bytes, time and
saved-source checking allocations are shared. An exhausted or unavailable member
has an explicit outcome. `member_availability: report` preserves usable results
and failures; `require` returns a typed non-success if a member cannot answer.
This choice is independent of saved-source `verification`.

The `airk.combined-read/1` result retains complete native member responses under
`members[].result`, including readiness, partial outcomes, source checks, errors
and retry details. Ranking entries identify those retained records by JSON
Pointer, avoiding a second copy of evidence text. Equal-weight rank contribution
uses `airk.rank-fusion.rrf-equal/1`: `1 / (61 + zero_based_local_rank)`. It does
not compare raw vector or lexical scores across indexes or claim a global
corpus top-k. Search adds no invented continuation.

Member results contain the canonical native bodies, inline or through the
explicit lossless metadata representations described below. Transport framing such as
duplicated text/structured JSON is not a second evidence record; native error
flags remain explicit. The complete canonical combined body is bounded to
65,536 bytes, including its native delivery receipt. Public CLI/MCP JSON framing
is separately bounded to three times that allowance plus 4,096 bytes, accounting
for the structured body and its quoted text copy. Private worker/control IPC and
MCP input-line limits remain unchanged. A body that cannot fit fails explicitly with
`combined_reply_budget`; the maximum selection count does not promise that every
combination of maximum metadata and evidence allowances fits simultaneously.

The canonical `selection` observation reports explicit Project and index IDs,
the reviewed grant ID/revision, member-availability policy and an identity digest
of the complete frozen native selection. Membership/source/policy revisions and
scope predicates remain bound by that digest and the native delivery fence;
they are not repeated as full membership and union tables in every read.
Queried members retain their complete scope observations and qualified origins.
The granted-only `query_scope` sources view exposes the permitted source prefixes.
This same result contract is preserved by every transport and export.

Combined capabilities can store exactly equal native capability fields once in
`shared_values`, schema `airk.shared-capability-fields/1`. A member's reserved
`$airk_shared_capability_fields` digest names that complete field group; merging
it with the member's inline fields reconstructs the exact original native body.
Admission, runtime, scope, publication and failure observations remain attached
to their individual members. The separate top-level combined Workbench catalog
remains complete. Capability-field groups are used only by combined capabilities.
Other reads can use the native-header templates described below.

The canonical resolver validates the capability namespace, bounded table and
digest, refuses missing references, collisions and recursive references, and
materializes one exact originating body. Reference IDs cannot address arbitrary
JSON paths. Copies and exports retain both the references and their complete
shared table, including when the user explicitly selects a subset. Display
materialization does not replace or modify the retained response.

Ordinary combined reads can use `airk.shared-native-header-templates/1` in the
same shared table. Only enumerated native metadata headers participate:
collection, discovery observations/readiness, semantic readiness, execution,
freshness, generation, publication, request ID, scope observation and truncation.
Each template has a fixed object/array shape and bounded paths to typed primitive
slots. A member retains its own slot values, including observation dates and
scope identities. Missing fields stay missing. Template digests, paths, primitive
types, counts and collisions are checked before materialization; slot values
cannot inject objects, evidence rows or arbitrary patches. Unprofitable groups
and unsupported values remain inline.

Exact publication references use a bounded member offset in the complete
publication vector. The vector entry must identify the same index. Ranked
origins use `publication_ref`; native publication fields can use `{member: N}`.
The canonical resolver restores the complete original publication envelope and
continues to accept retained inline origins. Hit, fact, file, definition and
reference rows remain inline, so their original result pointers still resolve.
No metadata or evidence is silently discarded to fit the canonical 64 KiB body
budget, and the existing public transport frame bound is unchanged.

Plans, verification and presentation materialize copies for logical metadata
access. The original factored body and its delivery receipt remain unchanged in
retained results and exports. A complete logical view removes the raw delivery
receipt and identifies itself as a derived view; an expanded copy does not claim
the receipt authenticated different bytes. Source checking, grants and native
delivery admission still belong to their originating owners.

Publications form a per-index vector with a dated observation interval. They are
not one atomic revision. Comparison requires the same authority and selected
index set; changed incarnations, conflicting identities and mixed member movement
do not establish that the whole response is older or newer. Follow-up validators
retain the originating member's exact publication. Continuation identities bind
the selected scope, grant and member revisions, publications, filters, projection,
budgets and merge positions; an expired member cannot silently restart a page.

Exact source attribution uses the selected native owner's retained publication
mapping, including its published aliases. Current catalog aliases cannot
substitute for that mapping. This bounded metadata read performs no source
capture, hashing, model work or file enumeration.

Before returning a prepared combined result, the native service rechecks its
scope against both saved metadata and its in-memory admission fence. A pending
revocation blocks the final receipt even before its catalog write commits. The
returned `delivery_observation` binds that scope observation to the prepared
body's digest. It records scope admission at the protected native receipt-write
boundary; it is not a source-content signature or a guarantee that bytes already
received by a frontend can be recalled. Transport observation code preserves
this body and receipt together.

CLI calls, MCP and Workbench preserve the same complete result and supported
inputs. Structured details, JSON/Markdown copies and exports retain member
identity and failures. Plans keep explicit qualified bindings; source checks run
through each originating native owner against the final returned projection.
Saved-byte equality does not certify extraction, relevance, complete coverage,
resolved language references or unsaved buffers.

The terminal selects the reviewed session with `--combined-session` JSON.
Convenience reads accept `--project-ids`, optional `--index-ids` and
`--member-availability`; the latter two require explicit Project scope. Tool
calls and shell use the corresponding JSON fields from the connection's live
catalog. `query_scope` pages granted Projects, indexes and sources; ordinary
legacy discovery returns `configuration_required` without selecting a library.
Saving defaults and applying a Search draft do not run a query or broaden a
captured session.
