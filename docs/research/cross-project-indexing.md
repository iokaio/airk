# Projects, user-managed indexes and cross-project queries

**Date:** 2 November 2026. **Baseline:** AIRK 1.8.7 source inspection.
**Status:** feature design for 1.9.0; proposed contracts and UI, not implemented
functionality. This document supersedes the folder-driven setup and connection
organization proposals identified below. It preserves the original scenario
analysis in [folders/workspaces research](vs-code-folders-workspaces.md).

## P3 implementation decisions

[ADR 002](../decisions/002-projects-indexes-and-query-scope.md) and the [contract pack](contracts-v190/README.md) settle this design for implementation: typed `project_ids`/`index_ids`, granted-only `query_scope`, initial fan-out one, registry schema 12, exact membership attribution, original remainder plus one split destination, and persistent native policy outside deletable collection data. Immutable backing identity survives legacy registration purge; its optional active-registration FK uses `ON DELETE SET NULL`. Existing data-root relocation must settle before upgrade. Ordinary opens continue rejecting WAL overlays; consistent backup is an explicit owned migration operation. [ADR 003](../decisions/003-dated-saved-source-verification.md) fixes dated verification and explicit budgets; the [family inventory](profile-family-inventory-v190.md) separates legacy retention from future profiles. These decisions supersede illustrative alternatives below. Actual migration/policy/transport/performance acceptance remains in P4-P14.

## Outcome and examples

The hierarchy is **Project → one or more Indexes → source folders**. A Project is
a reusable native metadata definition above indexes: it declares which indexes
are available for querying and which subset is selected by default. An index owns
its source assignments and retained evidence independently of Projects and VS
Code windows. Multiple Projects may reference the same index without copying it.

A trusted VS Code window can open/use one or more AIRK Projects on its AIRK host.
A query explicitly selects one or more available Projects and may narrow their
index members further. A Project may expose sources not open as workspace folders.
The Project–Index relationship is many-to-many: the same index may belong to
several Projects. Opening a folder suggests choosing/configuring Projects rather
than automatically creating indexes, starting watchers or scanning descendants.

Example: **Payments code** indexes `src/payments`; **Product specifications**
indexes a separate specifications folder; **Shared platform** indexes another
repository. A Payments window can use a **Payments** Project containing all three
and search code and requirements together. A documentation window uses a **Writing**
Project that references only Product specifications. Both reuse that index without
copying it. Closing either window does not delete a Project or index or silently
change its saved indexing policy. Two windows may also use the same Project with
different temporary query selections.

Alternatively, a user can create **Payments product** containing code and specs
as explicit roots in one index. This shares publication and budgets, subject to
existing alias/provider constraints. Searching three indexes keeps three independent
publications and budgets. The UI must explain this choice before creating storage.

Primary value: user control over resource use and source boundaries; reuse across
windows; investigations spanning implementation, specifications and shared code;
fewer accidental duplicate indexes; and evidence whose origin remains inspectable.

## Current implementation and design constraints

Inspected live files and contracts:

- [Collections](../collections.md) bind an owner root and optional source aliases
  to independent storage. Current publications, source identity, quota, locks,
  recovery and deletion remain collection-scoped.
- [Root configuration](../../crates/airk-store/src/roots.rs) supports 16 explicit
  aliases. Combining sources must honor existing overlap, isolation, coordinate
  and imported-analysis restrictions; this feature does not qualify new ones.
- [Workspace bindings](../../packages/vscode/src/serviceWorkspace.ts) reconcile
  opened folders with native registrations and save choices by opened path.
  [Launch assembly](../../packages/vscode/src/model.ts) builds a primary root plus
  additional roots from those selections, rather than an independent window set.
- [Unified MCP](../../crates/airk-mcp/src/unified.rs) already supports 1–32
  configured projects, lists only those projects and requires a single selected
  project when there is no default. This is multi-project routing, not a combined
  query. The [public contract](../unified-mcp.md) preserves configured connection
  scope independently of Explorer's current project selector.
- [Current search](../../crates/airk-kernel/src/current/search.rs) retrieves within
  one authenticated current read. Cross-index results need explicit composition;
  no existing single token can represent an atomic multi-index publication.

The design retains one native service/container per user/machine, independent
collections and local retrieval. It adds deterministic bounded query composition,
not agent reasoning, cloud search, cross-machine transport or another database.
Source files remain read-only. The registry remains metadata-only and the existing
project workers perform evidence reads directly.

## Terminology and persistent model

| Concept | Meaning and owner | Lifecycle |
| --- | --- | --- |
| Project definition | Native registry metadata above indexes: stable Project ID, name, member index references with allowed root/prefix subsets, default query selection and revision | Reusable across windows; changing its name does not rebuild an index; deleting it does not delete referenced indexes |
| Index definition | Native registry metadata: stable opaque index ID, editable display name, backing registration/collection, owner root, named source aliases, settings and definition revision | Exists independently of any window; rename changes presentation, not source identity |
| Source assignment | Explicit canonical root or existing alias in an index, its stable root identity and eligibility policy | Changes use reviewed native configuration and publish through existing jobs |
| Active access grant | Versioned native permission snapshot derived from a reviewed Project revision, index/source identities and optional narrower client restrictions | A Project reference alone is not a grant; expansion needs review, removal/revocation fences reads |
| Window association | Extension-owned mapping from a VS Code context to one or more Project IDs and an active grant, with temporary query selection | Saving/reordering a workspace changes neither Project/index identity nor assignments |
| Query selection | One or more permitted Projects and optional narrower index members for this request/session | Explicit subset of the connection grant; never expands authorization |
| Publication vector | Exact member index IDs, incarnations, scope digests and current tokens observed for a combined read | Response-bound observation; not a global transaction, checkpoint or retained history |

Use new registry index IDs as stable handles to existing registrations; do not
rewrite source-root hashes, collection IDs or physical paths during migration.
Names need not be unique: pickers show short IDs and roots to disambiguate. An
index ID cannot silently acquire a different backing registration/incarnation.
Recreation, root moves and relinking need explicit binding review; rename alone
cannot authorize them. Owner-root identity remains part of the storage contract.

The first version groups sources through existing owner-root/alias mechanisms;
it does not redesign storage into an arbitrary rootless index. Replacing/removing
the owner root, physically merging/splitting existing collections, or moving a
source between indexes is not a metadata drag-and-drop operation. The reviewed
split workflow below adds an explicit two-index partition with independent builds,
durable cutover and inherited exclusions. It supersedes the earlier deferral of
splitting; physical merging and general root relinking remain deferred. No
automatic destructive move or cross-collection object sharing is introduced.

Default to no source overlap within one index under existing rules. Across
indexes, X1 must implement new bounded source-to-source observations to warn about
duplicate work, including alias and retained roots. Existing isolation checks
compare sources with AIRK's private state, not other sources. Allow overlapping indexes
only after review. Never infer ownership from `.git`, a common parent or matching
file bytes. Repository markers provide bounded suggestions only.

### Project membership and multi-Project scope

A Project owns metadata, not index data. Its member rows reference stable index
IDs, allowed source-root/prefix subsets and query defaults. The same index can
appear in Payments, Writing and Shared platform Projects while retaining one
physical collection, watcher, publication stream and indexing policy. Index
profile/watch changes affect every Project using it and need a shared-impact
review. Project-specific permitted subsets/defaults do not modify that index.

Removing membership, closing a Project or deleting a Project must never delete
the shared index or pause its updates. Removing the index itself is a separate
reviewed operation that invalidates every referencing membership and lists the
affected Projects. Project names are editable; IDs, membership revisions and
foreign-key relationships determine identity. Empty Projects may be saved for
configuration but cannot supply a successful nonempty retrieval scope.

Connections may allow several Projects; query selection is a subset of those
grants, independent of which Project detail is visible. For a query selecting
Projects A and B, resolve their current, authorized memberships and deduplicate
index IDs before execution. If index I appears in both, query I once over the
union of the permitted prefixes from the selected memberships, intersected with
connection/source authorization and request filters. Do not grant any prefix
merely because another, unselected Project includes I. Preserve canonical prefix
sets rather than broadening disjoint prefixes to their common ancestor.

Each result records its index origin and exactly those selected Project
memberships whose permitted scope contains it. A result allowed only through B
cannot be attributed to A. No duplicate score boost, repeated vector count,
duplicate verification or additional worker occurs because an index belongs to
two selected Projects. Shared physical roots in two genuinely different indexes
remain separate provenance, as described below.

Project edits use expected revisions and reviewed scope changes. New memberships
or wider prefixes never silently expand existing client grants. Removing a
membership narrows that route immediately; another independently authorized,
selected membership may still permit the same evidence. Project-wide revocation
and index-wide revocation have different effects and need separate tests.

## User choices and first-use behavior

When an unconfigured trusted window opens, show one non-modal **Choose Projects**
entry point. Offer:

1. **Use existing Projects:** open one or more reviewed same-host Projects. Also
   offer **Create Project** and add existing indexes, including unopened sources.
   No files are copied or reindexed by creating membership.
2. **Create an index:** choose a name, one primary source and optional aliases;
   review storage, ignores, size estimates/unknowns, profile, vectors and update
   policy before registration and indexing.
3. **Keep folders separate:** explicitly create selected indexes and add them to
   the chosen Project(s), reusing existing indexes where appropriate.
4. **Combine selected folders:** create one index using supported source aliases,
   with shared budgets and provider limitations shown.
5. **Do not index now:** persist the choice and leave AIRK available for setup later.

Offer the same choices on folder additions; do not auto-open Projects or register
new folders. Existing explicitly saved assignments and automatic-update policies
continue quietly. Bounded container inspection may suggest direct children but
must show truncation and never recursively register discovered repositories.
Opening a nested package offers an existing index with a narrowed access prefix,
or a reviewed separate index. Use actual roots to calculate the prefix, including
aliases; never guess from a display path.

Folder names and order are presentation. Removing a workspace folder does not
close a Project or change its index memberships. Closing a window does not pause
watching. **Close Project in this window**, **remove index from Project**,
**delete Project definition**, **pause index updates** and **remove index data**
are distinct operations with explicit effects.

With zero usable Project/index members, keep Indexing and native setup/help available but
publish no evidence-capable MCP definition. Do not invent a default or attach a
machine index to satisfy the current endpoint's minimum-one-project requirement.
Any later help-only endpoint needs its own explicitly advertised capability set.

## Explorer: new top-level Indexing screen

Add **Indexing** as a top-level Explorer navigation destination, alongside Search,
Code and Documents. It is the entry point for defining indexes and window access;
it replaces the assumption that Management's selected project owns setup. Keep
Service, Recovery, Storage, Agents, Usage and Logs in Management with links from
Indexing. Existing indexing routes deep-link to the relevant new detail panel.

| Screen area | Information | Actions and behavior |
| --- | --- | --- |
| This window | Open Projects, current query Project selection, effective deduplicated indexes and defaults | Open/close one or more Projects, select Projects for a query, narrow indexes; zero open Projects supported |
| Projects | Bounded paged Project catalog, name, member indexes, source scopes, default selection and affected windows | Create/rename/delete metadata, add/remove existing indexes, review membership and grant changes |
| Index library | Bounded paged same-host catalog, name/root search, membership counts and duplicate-source warnings | Create, inspect and add to one or more Projects; metadata listing never enables evidence access |
| Index detail | Stable ID, source roots/aliases, configured versus published settings, counts, coverage age, jobs, vector readiness and storage cost | Rename, add supported source, review settings, index now, pause/resume, Split index; native recovery/storage links |
| Sources | Root-to-index assignment table, overlaps, permanent path blocks, missing roots and pending changes | Split selected sources; Remove path and block permanently; separate reviewed Unblock path; explain shared-Project effects and cleanup |
| Review drawer | Exact before/after assignments or grants, scope, storage/job effect, estimates and unknowns | Apply/cancel, stale-review conflict handling and durable receipt recovery |

Example layout (functional sketch, not a screenshot):

```text
Indexing                 [Create Project] [Create index]
Open Projects             [Open existing Project]
  [x] Payments             indexes: Payments code, Product specifications
  [x] Platform             indexes: Shared platform, Product specifications
Query scope: 2 Projects / 3 distinct indexes      [Narrow indexes]
Product specifications is shared; it will be queried once.

Index library (this AIRK host)            [Filter...]
  Name | Sources | Published coverage | Updates | Attached here
```

The query picker distinguishes **open/allowed Projects**, **selected Projects**
and their effective index subset. Deselecting a query checkbox does not edit
membership or revoke access. Closing a Project revokes that window's Project
route; editing shared Project membership can affect all its active windows.
Review both effects explicitly. Shared-index operations say “affects N Projects”
and explain update-policy impact. Do not leak other windows' session content.
Destructive index removal retains native review; sources are never deleted.

Support keyboard navigation, focus restoration, screen-reader labels, meaningful
empty/unknown/partial states and paginated lists. Use loaded native observations,
not index-directory scans in the webview. Opening an index result outside open
workspace folders uses the verified source location and an explicit open action;
it does not add that folder, execute code or trust a remote document automatically.

## Split indexes and permanently remove paths

The full 1.9.0 target includes M0–M2 from the
[index-management contract](plan-v190.md#index-management-split-and-permanent-path-removal).
These actions change index contents for every referencing Project. They are
explicit native management operations with prepare/apply/receipt lookup and
equivalent CLI and Explorer flows; evidence queries never perform them.

**Split index** partitions the original into its retained remainder and one new
independent index. The user chooses roots, directory subtrees or exact files,
names the destination, reviews both effective source predicates and chooses the
Project membership changes. Selected and remaining sets must be nonempty,
disjoint and cover eligible input under the reviewed policy. Preserve the original
index/primary-root identity and use versioned positive scope predicates where
whole-root aliases cannot express the destination. New descendants follow selected
subtree rules; an exact-file selection never becomes an ancestor-directory grant.

Reserve space for both collections and temporary/cleanup work. Build the destination
through existing captured-source indexing with independent object ownership, not
by copying SQLite tables or sharing owned packs. Keep it unavailable to ordinary
clients until its required base/semantic policy validates. A durable native receipt
then coordinates denial of transferred paths in the original, residual publication
and the reviewed membership/grant cutover. There is no atomic transaction across
both collection databases and the registry. Before cutover, failures preserve the
original and leave the destination ungranted. After cutover starts, resume saved
steps with restrictive fences intact; do not undo blocks or replay an uncertain
operation. Readiness and source observations retain per-index times/identities.

A shared Project receives the new index only through the reviewed membership plan;
others retain the original remainder and see the reduced scope. Existing agent
grants cannot silently expand. Previously blocked locations stay blocked wherever
the resulting indexes cover them. Confirm both final partitions, grants and
cleanup outcomes before declaring split completion.

**Remove path and block permanently** first saves an index-level exact-file or
subtree deny and advances a native policy epoch, then removes evidence through
existing service publication/reclamation. The deny has no expiry and survives
rebuild/repair, watch events, manual indexing, profile migration and re-registration.
It overrides includes and ignore-file negations; renaming/removing/readding an
alias for the same canonical source cannot bypass it. Only a separate reviewed
Unblock path policy operation can remove it. No source files or source ignore
files are edited. Other indexes covering the location are unaffected and named
in the review; this is not a machine-wide path ban.

Enforce exclusion before discovery/capture/hash/extraction/embedding and before
every retained read, including exact locators, cached/conditional replies, graph
endpoints, combined queries and verification. Observe the current policy epoch
before delivery; invalidate stale cursors/permissions as necessary. Transitional
tools that cannot safely filter must return typed unavailable rather than leak
blocked content. Removal failures keep the deny active.

Publish removal of text/facts/postings/vectors/relationships under existing leases
and quotas; rewrite packs that contain surviving and blocked evidence before
reclaiming obsolete objects. Show blocking, current-index removal and physical
cleanup as separate states. Account for cached sessions, delivered/exported results
and private upgrade backups; offer separate supported local cache/session cleanup
and never promise secure erasure or recall of exports. Split/trim mutate no source.

Store rules by index identity, canonical source-root identity, normalized relative
path and exact/subtree kind. Maintain mapping across aliases, root rebinding and
upgrade or stop for explicit reconciliation. Keep authoritative policy revision
in native collection configuration and enforce it for direct CLI as well as
service reads; a registry/UI-only rule is insufficient. Reuse existing jobs,
receipts and object ownership with schema/format refusal by unsupported runtimes.

M0 designs the predicates, epochs, storage compatibility and operation stages
before X1 freezes its migration. M1 implements permanent removal and cleanup;
M2 builds splitting on that enforcement. X6 qualifies all steps, including crashes,
quota exhaustion, source churn, denied descendants, alias/case/link variants,
active readers/workers, shared packs/Projects, upgrade and rebuild preservation.

## Access, persistence and MCP connections

Store Project definitions, memberships, indexes and reviewed grants in the existing
private native registry under its bounded transactional migration. Store editor context
associations locally, with a versioned format. Do not commit machine paths,
grants or native bindings to `.code-workspace` or repository files automatically.
An optional exported mapping is an inert template needing local resolution/review.

For a folder window, derive a context from the host/service namespace and canonical
folder identity; for a saved workspace, use its canonical workspace-file identity.
Untitled and empty windows get bounded private local contexts; offer explicit
reuse/copy of Project associations when saving, rather than silently unioning
grants. Context is a preference key, not proof of authorization. Concurrent windows
using the same workspace start from the same saved Project selection but receive
separate active grants;
“Save choices for this workspace” updates future opens, not other active windows.
Profile/build changes may recover templates, but must never auto-attach additional
indexes. Native metadata, not editor `globalState`, remains the scope authority.

All attached indexes must initially belong to one service/host authority. Local,
SSH, WSL and container catalogs are separate. Do not turn a local path into a
remote grant or combine evidence across services in 1.9.0. Same-service indexes
in different supported data roots may be selected; retain each root's admission
and worker checks. Missing/nonpersistent remote storage is shown explicitly.

The native connection receives a reviewed grant ID/revision for one or more
Project IDs/revisions, resolved member indexes and explicit source allowlists.
Access-set metadata is the grant implementation, not a second user-facing
organization above Project. The exact launcher/API spelling is proposed work.
Effective evidence scope is the intersection of that grant, current active
registrations, allowed source-root identities/prefixes and request filters.
Attaching an index authorizes only its reviewed roots: adding an alias later
must not silently give existing clients new source access. Exclusions/removals
apply before any new read, including retained old-publication evidence.

Additions require reviewed grant expansion and reconnect/renewal; Explorer's
selected row cannot expand a running agent connection. Revocations fence new
admission and cached reads immediately and cancel/discard in-flight delivery
where revocation is observed. Already delivered evidence cannot be recalled.
Check grant revision before returning accepted work; unknown scope state fails
closed. Prevent a stale config or revived process from reinstating removed access.
These are AIRK scope controls, not an OS sandbox against the same user's tools.

Keep one MCP definition assembled from a canonical sorted set. Folder reorder or
rename must not restart it. Report a pending permission/configuration update
separately from active access. Manual MCP configuration owns its own connection;
report per-member conflicts and offer a guarded preview instead of overwriting
it or suppressing every unrelated index. External agents use a separately reviewed
saved Project selection/grant snapshot; they do not require a live Explorer window.
Keep existing `projects` discovery as a legacy index-binding catalog during the
compatibility period; do not reinterpret its IDs as new Project IDs. Add a
versioned discovery read, illustratively `query_scope`, listing only granted
Projects, their available indexes, defaults and revisions. Native administrative
Project/index library listing remains separate from agent-visible discovery.

The management API needs bounded catalog/detail reads; source-assignment,
index-definition and Project/membership prepare/apply operations; grant prepare/
apply/revoke; and receipt lookup. Reuse current reviewed service request identities, expected
registry/definition revisions and recovery conventions. A UI timeout must inspect
the saved receipt rather than replay an uncertain create/delete/grant change.
Keep these mutations out of the read-only evidence MCP catalog; equivalent CLI
management commands and trusted Explorer forms share the native implementation.

## Query contract and tool behavior

Preserve legacy single-`project` calls and their exact single-index semantics;
that existing parameter identifies a native index binding, not the new metadata
Project. Add explicit nonempty `project_ids` for one or more new Projects, plus
optional nonempty `index_ids` to narrow their resolved union. Without `index_ids`,
query all permitted member indexes of the explicitly selected Projects, accounting
for unavailable members rather than omitting them. UI/
session default subsets resolve to explicit selectors before execution. Reject
mixed legacy/new selectors, duplicate/unknown/ungranted IDs, or an `index_ids`
member outside the selected Project union before fan-out. New IDs come from
versioned connection discovery; the registry library is never a scope wildcard.
Saved defaults must be visibly selected and explicitly configured for agents;
omission cannot broaden an old single-index call. The first design's proposed
plural `projects` index selector is superseded before implementation.
Example schemas below are illustrative, not accepted arguments today:

```json
{"tool":"search","args":{"project_ids":["payments-project","platform-project"],"index_ids":["payments-code-index","specifications-index"],"query":"refund policy","mode":"lexical","limit":12,"max_bytes":8192,"member_availability":"report","verification":"off"}}
```

Record Project-level resolution outcomes as well as index execution outcomes.
A deleted/inaccessible Project, empty Project, removed membership or unavailable
backing index cannot disappear from accounting. Unknown/ungranted selectors reject
before execution; known selected members that become unavailable follow the
explicit failure policy. A changed Project/membership revision expires a scoped
continuation or requires fresh grant resolution; it cannot widen a running query.

| Operation family | Multi-index semantics |
| --- | --- |
| Search | Run the same explicit query/mode under each member's scope, then bound and combine candidates as described below |
| Symbols, code/document facts, references, list files | Return a qualified union with per-member coverage, exact count precision and continuations; identical names/paths stay distinct |
| Status, diagnostics, capabilities | Selected Project membership/revisions, resolved unique-index observations and bounded aggregates; no invented global publication or double-counted shared-index totals |
| File info, read file, context, imports, outline | Scope may name one or more Projects, but require exactly one originating index and qualified path/locator within their permitted union; ambiguous `README.md` is an error |
| Relationships | Follow supported edges inside the originating member. Search for textual candidates elsewhere explicitly; do not infer cross-project resolved edges |
| Changes and conditional validators | Remain member-specific; optional batching retains each token/validator and failure separately |
| Parse, recipes and strategies | Explicit member selectors and qualified bindings; ambiguity across definitions must request selection, never bind to the first project |
| Draft/UI tools and mutations | Target one explicit context/index or a reviewed administrative batch; no implicit “apply to all” |

For single-file operations, several selected Projects are valid when the explicit
index/locator resolves to one authorized member; unresolved multiple-index targets
reject as ambiguous. Input catalogs
must disclose supported selectors per tool. Reuse the same canonical handlers
for CLI convenience reads, tools/shell, MCP aliases and Workbench; no UI-only
federation. This is deterministic retrieval composition, not agent orchestration.

## Bounded query composition, ranking and failures

1. Validate and freeze selected Project IDs/revisions, membership revisions,
   deduplicated index scopes, grant revision, filters, explicit mode,
   failure policy and total budgets. Admit the outer request through existing
   bounded frontend admission; child operations consume global capacity rather
   than multiplying each index's configured concurrency.
2. Acquire only ordinary bounded member reads through existing scoped workers.
   Record a token per member; do not hold a multi-index write lock. If an exact
   expected member token expires, retain that typed failure. No historical read
   or automatic mutation is introduced.
3. Allocate candidate count, bytes, time and verification budgets across members
   using a documented deterministic plan. Reserve bounded framing/error space.
   Initially cap unique selected indexes at the current 32-connection ceiling,
   bound Project and membership counts independently, and choose a
   lower qualified fan-out/concurrency default after measurement. Reject
   impossible budgets or declare unqueried members explicitly; never report them
   as successful zero-hit searches. Share one deadline and propagate cancellation.
4. Apply per-member authorization, metadata filters and prefix intersection before
   branch candidate limits. Use the same explicit mode for each member. If mode is
   omitted, disclose each member's effective saved policy; do not imply homogeneous
   search. Missing vectors produce disclosed partial/unavailable outcomes, never
   silent lexical fallback or an automatic model/provider change.
   The authorized disjoint-prefix union requires canonical native predicate support;
   do not fetch unrestricted top-k and filter afterward or query a shared index
   repeatedly under different Projects. Preserve all existing isolation checks.
5. Combine bounded search candidates using a versioned equal-weight rank-based
   policy, initially reciprocal-rank contribution `1 / (61 + local rank)` with
   zero-based rank and stable member/citation tie-breaking. Do not compare raw
   embedding/lexical scores across indexes or call the merged score a probability.
   Preserve branch/member rank and policy identity. No weights by index size or
   hidden preference for the first workspace folder.
6. Apply one total result/text limit and an explicit optional diversity policy.
   Retain per-member candidate cutoffs, budget shares, selected counts and reason
   for omissions. This is a bounded federated ranking, not equivalent to top-k
   over a physically combined corpus. Sparse results from one member may allow a
   bounded documented redistribution; no unbounded refill/search loop.
7. Return qualified evidence and the publication vector with a query fingerprint,
   selected Project revisions, membership-to-index resolution and per-result
   permitted Project associations. Shared indexes appear once in the vector.
   Distinguish an unavailable member from a completed member returning zero hits.
   `member_availability=report` returns usable evidence plus explicit member failures; `require`
   returns typed incomplete/non-success when any requested member failed. Neither
   policy claims complete corpus coverage merely because every worker replied.

No cross-index atomic snapshot is promised. If a member changes while reading,
its normal current-token rules apply. Completed member results retain their own
observation times; a vector records an observation interval, not simultaneous
filesystem state. Explicit caller retries may start a new vector; never combine
old and newly retried results while retaining the old response identity.

For paged union tools, use a bounded opaque composite cursor binding selected
Project/membership revisions, deduplicated index/prefix set, grant revision,
full token vector, filters, projection, budgets and member cursor/
merge positions. Any changed member token expires the combined cursor; removal
or revocation rejects it. Do not persist old evidence as a cursor implementation.
Where the underlying tool has no continuation, including the initial bounded
search window, disclose cutoff and offer explicit narrower/member queries instead
of inventing an apparently exhaustive Next page. Define canonical ordering for
union inventory/fact tools separately from search rank.

Citation identity includes index ID, backing incarnation, source-root identity,
logical path, source hash, native location and member publication. Never collapse
two `README.md` files, equal symbols or shared byte hashes across indexes. Optional
display grouping of overlapping source occurrences must preserve all origins and
declare its subset; it cannot remove an independent provenance chain. Per-index
totals may double-count shared sources, so label them summed memberships, not
unique files. Root aliases remain mapped to original document coordinates.

## Verification, profiles and sessions

Apply the [verification plan](plan-v190.md#fourth-research-area-verifying-returned-references-against-saved-sources)
after final bounded selection. Deduplicate checks only where physical source
identity, expected digest and allowed scope agree; retain a receipt association
for every index citation and permitted selected Project origin. A file checked through one grant does not authorize
another. Adopt the shared proposed enum style:
`verification: off | report | require` (default `report`, changed by the maintainer during P4) and
`member_availability: report | require` (default `report` for combined reads).
The controls are independent; X0/V0 publish their semantics together. The earlier
design's `failure_policy`/`require_all` names were never shipped and are replaced.
“All returned sources matched” says nothing about an unavailable index
or a relevant file not returned. Zero hits never earns a verified-corpus badge.

Lean/Rich replies preserve member identity, availability, verification and error
accounting. Index profiles remain independent per member; a combined Rich request
cannot generate missing facts or migrate indexes. Report differences without
silently reindexing. Session transcripts and exports retain the exact member set,
vector and dated receipts. Restoring a session checks current attachment/permission
and shows historical results as historical; it cannot regrant detached indexes.
Result cards display index name, path and permitted selected Project origins;
copies and JSON carry stable IDs and Project/membership revisions too.

## Native ownership and compatibility

Core owns Project/index/membership/grant/federated-response schemas and budget/error contracts.
Store owns bounded registry metadata, revisions, receipts and lifecycle fences.
Ingest retains source capture, ignore and extraction policy. Kernel owns the
deterministic merge/filter/budget policy through neutral scoped-read interfaces;
CLI supplies existing direct-worker client adapters. MCP invokes the shared read
handler and adds transport framing; Workbench and Explorer render complete native
results and submit reviewed management requests. If a policy needs shared client
composition, implement it in the owning existing module with neutral interfaces;
do not add a new crate or let workers open other collections arbitrarily. The
broker continues to handle metadata, not source passages or embeddings.

Keep legacy root/collection launcher paths and single-`project` inputs supported
through an explicit index-binding compatibility adapter. The new Project layer
must not reinterpret old IDs or silently broaden existing queries. The SQLite
upgrade below is a prerequisite, not a post-release cleanup. No version bump or
database mutation occurs in this research change.

Renaming is cheap metadata; root/profile changes expose configured versus
published revisions and existing durable jobs. Removing an index revokes new
access, drains owned work/readers under current lifecycle rules and marks affected
attachments unavailable; it never silently selects another index. Failed
registration/configuration keeps previous valid assignments readable under their
still-authorized scope. Dangling context rows need bounded cleanup without
deleting an index. Project, membership, grant and context metadata quotas prevent
unbounded growth. Saved Project defaults reference member IDs, never result caches.

## SQLite schema and upgrade path

### Proposed schema boundary

The inspected [registry owner](../../crates/airk-store/src/service/registry.rs)
accepts SQLite `user_version` 1 through 11 on the writable migration path and
2 through 11 on the read-only saved open path. Both check application/namespace
ownership as applicable; do not generalize writable acceptance to saved reads.
[Storage formats](../storage-formats.md) separate registry schema from public
`service-v1`/`index-v1` identities. This feature requires a new registry schema
version, assigned after reviewing the then-current tree; do not assume 12 will
still be free. Version every changed IPC/config/receipt format separately.

Use additive normalized metadata in the existing service SQLite database. Keep
the old SQL `projects` table as the legacy native registration owner until its
job/lease/lifecycle callers can be deliberately migrated. The new user-facing
Project is a different entity. Illustrative new tables (names are proposals):

| Table | Keys / principal fields | Constraints and ownership |
| --- | --- | --- |
| `index_definitions` | Stable `index_id`; legacy binding; display name; definition revision; lifecycle/tombstone state | Unique backing binding; do not duplicate collection ownership or copy source/extraction state into metadata |
| `query_projects` | Stable `project_id`; display name; revision; lifecycle state | Independent of index/root names; bounded metadata; no uniqueness requirement on display name |
| `project_index_members` | Composite primary key `(project_id, index_id)`; membership revision; default-query flag | Many-to-many join with foreign keys and indexes in both directions; repeated membership rejected |
| `project_member_scopes` | Project/index member key; stable root ID; normalized allowed prefix entries | Canonical bounded prefix sets; validate roots against the backing index and apply no widening normalization |
| Index policy/operation metadata | Index ID; canonical source identity; exact/subtree include/deny entries; policy epoch; split source/destination/stage mapping | M0 chooses existing registry/collection policy ownership; denies must be enforced by native collection reads/capture too. Reuse existing durable job/receipt tables, not a new operation store |
| `client_context_projects` | Context ID; Project ID; ordering/default-selection metadata; context revision | Many Projects per context; no source grants inferred from this preference table |
| `client_grants` and grant-member rows | Grant ID/revision; client/namespace; selected Project and membership revisions; resolved index/source scope; expiry/revocation state | Immutable scope snapshots or fenced revision updates; no plaintext source/query/evidence payloads |
| Migration mapping/state | Migration identity; legacy binding/context IDs; created IDs; stage; versioned outcome | Stable idempotency and bounded resumable editor imports; do not invent a second operation ledger |

Reuse existing receipt/intent tables for mutation recovery; add only the mapping
columns/tables genuinely needed. Use `PRAGMA foreign_keys=ON` on every applicable
connection, explicit uniqueness/check constraints and indexes for member lookup,
impact review, grant fences and reverse index-to-Project lookup. Validate actual
SQL/schema identity as existing code requires, not just `user_version`.

Project deletion may delete its membership rows transactionally but must never
cascade into `index_definitions`, collections, source roots, jobs or retained
objects. Index removal uses explicit lifecycle code: revoke grants, mark shared
memberships unavailable and drain/delete owned index data under existing rules.
Keep metadata tombstones or explicitly clear referencing rows in that operation;
never let a foreign-key cascade choose a physical deletion. Preserve the existing
registration/removal history and opt-out behavior.

Registry metadata changes should not require a collection rebuild or alter its
publication/token. If implementation discovers an index-format change is needed,
split it into a separately reviewed migration with its own compatibility, quota
and recovery design; do not hide it in this metadata upgrade.

### Upgrade stages

| Stage | Required work | Failure / recovery contract |
| --- | --- | --- |
| U0 — preflight | Inventory actual schema/application/namespace, registrations, contexts available for import, jobs and runtime owners. Validate old schema and capacity; prepare a bounded cost/compatibility review. | Unsupported, corrupt, foreign or oversized state fails explicitly before mutation; never initialize over it |
| U1 — exclusive migration admission | Use native installer/service ownership to drain old writers and incompatible clients/workers before changing shared metadata. Fence new admissions and honor accepted jobs/receipts. Reserve temporary disk/headroom for migration and backup. | Busy/unknown ownership blocks upgrade safely; old processes must not keep writing a DB after its version advances |
| U2 — consistent protected backup | Use SQLite's supported consistent backup mechanism after the ownership boundary, including committed WAL contents; validate backup identity/integrity and record its exact digest/location in existing upgrade metadata. | Never copy only a live `.db` file or delete WAL to force success. Backup stays in private owned state with bounded retention; never package or publish it |
| U3 — native schema transaction | In one bounded transaction add schema/constraints, backfill one index metadata row per existing registration with persisted ID mappings, validate counts/relationships and advance `user_version` only on success. | SQLite rollback leaves the old schema usable on interruption before commit; repeated startup detects committed state and does not duplicate IDs |
| U4 — Project/context import | Import known editor workspace/folder selections into reusable Project metadata and context associations using versioned idempotent native requests. Initially preserve exactly the old index scopes/defaults. Where no context exists, offer clearly labeled legacy one-index Projects instead of guessing workspace grouping. | Missing editor state is explicit pending import. No guessed multi-Project union, wider grant, automatic scan or inaccessible-index substitution |
| U5 — activate clients | Validate Project/member/grant revisions, run native single-index and multi-Project transport probes, then publish new catalogs/UI/connection configuration through guarded existing paths. | Unsupported old runtime/extension shows compatibility-required state; withhold new features instead of showing an apparently empty working library |
| U6 — settle and retain recovery evidence | Mark upgrade complete only after state validation and scoped probes; clean temporary migration inputs/backup only under explicit bounded retention policy. | Restart resumes the saved stage; an uncertain outcome is inspected by receipt rather than replaying grant/index mutations |

Trigger U0/U1 from the managed native runtime activation coordinator in CLI.
The new native service's registry owner runs U2/U3 through Store, under the saved
activation intent and exclusive migration admission before normal registry writes
or endpoint readiness. Extend existing verified managed-install predecessor drain;
do not claim the new migration phase already exists. Windows/Linux termination
and macOS graceful shutdown retain their existing qualification limits. Unknown
owners block migration. MCP reads and webviews never trigger an upgrade.

Separate the atomic database transaction from resumable editor imports. Bound
backfill work by existing registration limits and new membership limits; do not
hold a writer transaction waiting for VS Code input. Retain settings, pause/manual
and vector policies, aliases, source exclusions, quota/root ownership, jobs,
watcher watermarks, durable intents/outcomes, usage and removal history. Metadata
import alone must not enqueue reindexing. Reconcile registration changes against
the frozen migration revision before activation. Each context import has a stable
key so opening the same workspace twice cannot create duplicate Projects.

### Old clients, rollback and qualification

Old binaries capped at schema 11 will reject the upgraded registry. Treat this
as expected incompatibility: update saved-reader validators, installer probes,
worker/CLI version handshakes and extension minimum-runtime checks together.
Legacy *API calls* can remain supported by the new runtime, while old *executables*
must not open/mutate the new schema. Do not weaken unknown-schema rejection or
dual-write unversioned fields to keep old writers running.

The 1.9.0 extension adds the 1.9 forward runtime line under
[versioning policy](../versioning.md). After migration, 1.9→1.8 is a blocked
ordinary downgrade; using an older matching extension does not undo the database
upgrade. Refuse incompatible activation/setup/evidence access from a second older
VS Code build/profile at the native boundary, returning an upgrade-required
reason. Supporting UI explains it; an already shipped extension may show only
a generic compatibility error. Safety cannot depend on changing old UI code.
X6 must test actual installed skew. If the fallback release
defers X, do not migrate the registry or claim these migration restrictions were
triggered; retain single-index V2 receipts as specified in the main plan.

Before commit, rollback is transactional. After commit but before admitting new
writes, a reviewed native recovery can restore the validated backup under full
owner exclusion and restore the matching runtime/configuration; it must inspect
receipt state first. Once new Project/grant/registration/job writes are accepted,
blind backup restore would discard real user work and may replay old intents.
Disable automatic downgrade at that point; prefer forward repair with the new
runtime. Any later restore requires a separate preview of lost metadata/operations,
an owned current backup, receipt reconciliation and explicit user authorization.
Never restore collection directories from this metadata-only backup or delete
source files as compensation.

Migration tests must cover supported old-schema fixtures, latest schema, repeated
upgrade, fresh installs, inactive/missing sources, explicit old defaults, multiple
collections for one root, two Projects sharing an index, duplicate context imports,
foreign/corrupt/unknown/newer schemas, partial or absent extension state, full disk,
busy/WAL/backup failure, process termination before/after commit and each import,
pending jobs/receipts, old-runtime reopening, post-upgrade writes and safe rollback
refusal. Compare preserved source/index identities and settings byte-for-byte where
applicable, and validate foreign keys/counts/uniqueness after every recovery.
Exercise installed upgrade on qualified Windows/macOS/Linux paths separately.

## Superseded decisions and retained safeguards

| Earlier folder/workspace proposal | New decision |
| --- | --- |
| Folder equals project; automatic registration of newly opened repository roots | Project metadata above indexes; many-to-many membership; explicit source assignment and Project opening |
| One automatic workspace plan based only on open folders | Indexing screen manages Projects, shared index membership, source assignments and window access, including unopened sources |
| Always bind a contained folder to the parent index | Offer scoped reuse as the recommended choice, but allow explicit other indexes or reviewed separate registration |
| Multi-root endpoint assembled from folder roots | Endpoint grants one or more reviewed Projects and their resolved index scopes; folder order is irrelevant |
| Management's selected project determines all indexing navigation | New top-level Indexing library and window-access screen, with explicit targeted actions |
| One project selector for every query | Explicit one-or-more Project scope with optional index narrowing; legacy index-binding calls stay compatible |

Retain native overlap/root isolation, no-follow source checks, explicit scope
intersection, source read-only behavior, review revalidation, manual-config
preservation, parity and honest platform qualification. Defer automatic descendant
pruning, root relinking, cross-collection embedding ownership, physical index
merging and cross-host federation. Reviewed splitting and permanent trimming are
now M0–M2 scope, superseding their earlier deferral. Original research remains dated evidence;
it should not be rewritten to pretend this design already existed.

## Delivery plan and acceptance

| Package | Deliverable | Depends on | Gate |
| --- | --- | --- | --- |
| X0 | ADR 002, terminology/access rules, input catalog and limits; record current per-folder fixtures/limitations, including ambiguous library targets and one shared MCP definition | Compact-plan W0 correctness; design can start earlier | Reviewed scope/dependencies and scenarios 1–20 disposition from the main plan; current behavior recorded before X2 |
| X1 | New bounded source `relationships`, `repository_markers` and broad-root observations; catalogs/join/scopes/grants, include/deny policy and U0–U3 migration | X0, M0 design before schema freeze | Existing isolation preserved; metadata/transport parity, permanent-policy ownership, SQL/backup/backfill/recovery tests pass |
| X2 | New Explorer Indexing screen, Project CRUD/membership, source assignment and opening/closing Projects per window; replace incremental automatic setup | X1 | Shared index across Projects and windows, empty/container/nested-folder flows, stale review/trust/cancellation; no unrequested jobs |
| X3 | Enforced member/root/prefix grants and stable MCP/CLI connection assembly, including revocation | X1–X2 | Actual transports reject ungranted reads, stale cursors, cached accesses and alias expansion; another window is unaffected |
| X4 | One-or-more Project query selectors, authorized membership union, deduplicated index reads, publication vectors, merge/errors/cursors | X0, X3; Lean/Rich reconciled | Actual parity; shared indexes queried/scored once; correct per-Project scopes and attribution; legacy single-index compatibility |
| X5 | Workbench/Search/Code/Docs/parse consumers, cards/sessions/exports; extend V2's existing single-index receipt member fields to combined citations | X4, T1, V2; V1/V2 have no X dependency | No lost identity, subset/error/receipt data; follow-ups target correct member; receipt extension is additive |
| M0 | Split/trim contract: partitions, permanent denies, policy epochs, operation recovery and upgrade/storage format impact | X0; design feeds X1 schema freeze | ADR 002 and schema review cover shared-Project effects and no bypass across alias/rebuild/read paths |
| M1 | Native permanent trim, immediate fences, removal publication, pack reclamation and CLI/Indexing UI/status | M0, X1, X3; X2 for UI | Deny survives faults/rebuild/upgrade; every native/transport path excludes content; physical cleanup reported honestly |
| M2 | Split preparation, independent destination build, residual policy/publication and reviewed membership/grant cutover | M1, X2, X3 | Two complete disjoint partitions, inherited denies and exact resume; no premature grant, shared ownership or source mutation |
| X6 | U4–U6 upgrade, full migration/fault/resource/relevance and installed-editor qualification including M operations | X1–X5, M1–M2 | Proven upgrade/split/trim recovery, cleanup/rollback limits and observed platform/cost results |

Ship vertical slices with contract and transport coverage together. An index
library alone is an intermediate milestone, not completion of cross-project
queries. This expands the original 1.9.0 workspace work materially; estimate after
X0/X1 and reduce optional ranking work first if needed. Any later scope reduction
must explicitly state whether combined queries are still absent.

Minimum test scenarios include: one index in two Projects; two Projects in one
query; shared indexes deduplicated across different membership scopes; Project
deletion versus shared-index removal; one Project in two windows; three indexes
in one window; combined code/spec roots; unopened sources; empty/no-folder window;
saved/untitled/moved workspace contexts; overlapping parent/child assignments;
identical paths/symbols/content across indexes; renamed/deleted indexes; different
models/profiles and partial vectors; unavailable/busy members; large fan-out and
tiny budgets; publication changes mid-read/page; revocation and source alias
addition during queued/running requests; cancellation and service restart; manual
MCP conflicts; local/remote separation; cross-project verification mismatches;
malformed/stale grants and exported templates; storage/quota/migration failures.

Measure latency, peak memory, worker/channel counts, query embedding duplication,
source verification bytes and query quality as selected-member count grows.
Share query embeddings only through existing qualified model/pipeline identity
and admission mechanisms; never introduce cross-collection retained ownership
implicitly. Compare combined versus separate explicit queries at equal total
budgets and judge mixed code/document relevance separately. Run required repository
checks and actual native/transport tests; installed Windows/macOS/Linux evidence
remains separate. No performance improvement or platform pass is established by
this design-only analysis.

## Decisions to confirm during X0

The defaults above make the design actionable. Implementation review should set
qualified member/fan-out and metadata limits, fair budget allocation thresholds,
workspace-context save/copy UX, exact public selector/receipt names and legacy
binding presentation. These details must not weaken explicit user scope, silently
restore folder-driven registration or turn a failed member into a zero-hit success.
