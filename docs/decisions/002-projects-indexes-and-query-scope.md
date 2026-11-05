# ADR 002: Projects, indexes and many-to-many query scope

Date: 2 November 2026.

Status: implementation design for the approved 1.9.0 plan. Runtime activation
remains gated by P5/P6/P7/P9/P10 acceptance. This record does not assert a human
approval, installed migration or completed qualification.

## Context

Folder-driven registrations conflate an editor context, an index and query
authority. Users need reusable indexes, several indexes in a Project, several
Projects in a window/query, and one index shared by Projects. Splitting and
permanent trimming must preserve that organization without duplicating authority.

The live P2b tree still has registry schema 11: writable open recognizes 1–11;
saved read recognizes 2–11. Both retain application/namespace/schema checks.
The registry is bounded to 32 MiB and 1,024 registrations. Existing collection
ownership, source roots, jobs and publications remain independently protected.
See [registry](../../crates/airk-store/src/service_registry.rs),
[service contracts](../../crates/airk-core/src/service.rs),
[collection ownership](../../crates/airk-store/src/service_collection.rs) and
[the feature design](../research/cross-project-indexing.md).

## Decision

### Identity and ownership

Use **Project → indexes → source assignments**, with many-to-many Project/index
membership. Core owns typed contracts; Store owns durable metadata/ownership;
Kernel owns scope, budgets and composition; CLI owns process/transport adapters;
the extension owns review and presentation. Add no external database or reasoning
service. Native administrative metadata and agent-visible discovery are separate.

New opaque IDs use an existing random 128-bit identity with a typed prefix:
`prj_`, `idx_`, `src_`, `ctx_` and `grt_`, followed by 32 lowercase hex digits.
These identify query Projects, indexes, canonical source assignments, contexts and
reviewed access snapshots respectively. IDs are not bearer credentials. Names
are editable/nonunique; IDs and expected revisions determine identity.

Keep the existing 64-hex native `ProjectBinding` as a **legacy index binding**.
Keep SQL `projects` as its existing registration/job/lease owner. A new index row
has one immutable backing registration/collection mapping. Renames do not change
source identity, publication or storage. Data-root relocation retains its existing
owned migration/identity rules; general root relinking and physical merging remain
outside this feature.

### Canonical scope and defaults

Retain legacy `project` with its exact one-index meaning. New retrieval controls
are **`project_ids`** (1–32 new Project IDs) and optional **`index_ids`** (1–32).
Reject mixed legacy/new selectors, empty arrays, duplicate IDs, unknown/ungranted
IDs and an index outside the selected Project union before fan-out. An index-only
new selector is invalid: it needs Project scope. No wildcard or all-library mode.

An explicit `project_ids` request without `index_ids` means all permitted members
of those selected Projects. Saved UI/session defaults are expanded into explicit
selectors before execution. Agent defaults must be explicitly configured; an
omitted argument cannot broaden a legacy connection. Freeze active connection/
session choices: saving workspace or Project defaults affects future selection,
not other active windows. Changing a prepared selection requires re-preparation.

Represent authority as canonical **source-root ID + exact/subtree path** atoms.
Subtree containment uses path components; a grant for `src` cannot include
`src-other`. A subtree with empty path means that root; an exact empty path is
invalid. Stored paths have no leading/trailing separator, empty component, dot/
dot-dot component, backslash, NUL or stream/drive colon. Preserve platform-native
identity/case rules; do not invent portable Unicode case folding. Existing query
`path_prefix` retains its literal filtering semantics, intersected with authority.
Disjoint atoms remain disjoint; never replace them with a common ancestor.

For selected Projects A/B sharing index I, union only I's permitted scopes from
selected memberships, intersect the connection grant/current source assignments/
request filters, then query I **once**. Attribute a hit only to selected memberships
that permit its actual source. Other Projects containing I confer no authority.

### Grants, discovery and revocation

A context association is a preference, not permission. Store local versioned
context choices privately, bound to host/service namespace and folder/workspace
identity. Empty/untitled contexts receive opaque local IDs. Save-as/copy/reuse is
explicit. Opening/reordering folders starts no registration, scan or scope union.
Manual MCP configuration is not overwritten. Local/SSH/WSL/container authorities
remain separate; 1.9.0 does not federate services.

Use a durable reviewed access snapshot (`grt_…`, revision) and the existing short
worker connection grant separately. Retain the current **30-second maximum**
worker-grant TTL. Renew only the same or narrower reviewed scope using existing
connection ownership; do not add a polling subsystem to justify reads. Scope
expansion needs review/reconnect or explicit renewal. A legacy configuration can
use an imported one-index compatibility snapshot, preserving its original roots
and restrictions; it cannot acquire new Project members/aliases implicitly.

**3 November 2026 P6f format amendment.** Import one-index compatibility
snapshots only from the validated P5 backfill plan at owned cutover, after the
unchanged zero-grant readiness proof. Preserve original source identities,
aliases and restrictions; context choices and open folders confer no permission.
Imports use the same scoped native admission, revocation and delivery fences as
reviewed grants, while new Project discovery remains legacy/configuration-required.
Missing, changed or revoked imports are never regenerated from current folders,
memberships or configuration. Unknown original source metadata blocks the whole
promotion pending explicit recovery; primary-root-only access is not guessed.

Authority contract 1 selects a distinct, fully validated schema-12 DDL shape.
Authority 0 and P5 retain their exact earlier shape and zero-grant stages. The
owned promotion atomically seals imported snapshots and advances the format,
leaving final activation to the native cutover owner. Imported and reviewed
grants each admit at most 1,024 records, including revoked tombstones, within the
unchanged 32 MiB registry bound. Maxima across all dimensions need not fit
simultaneously. Imported provenance and reserved IDs cannot be supplied through
the user-reviewed grant input. This amendment is an implementation contract,
not a claim that production activation or platform qualification has passed.

Add a separately versioned, required scoped-grant payload to protected native IPC
when P6 activates. Existing `ConnectionGrant` rejects unknown fields, so old peers
must refuse that payload. Do not globally renumber unrelated control methods or
weaken old validators. Review snapshot identity is not the secret IPC bearer
token. Debug/logs expose digests/revisions, not credentials or source scope text.

Track presentation/definition revision separately from authorization scope epoch
and saved-default revision. Renaming does not force reindex/regrant. Membership,
source or policy narrowing fences new admission/cached delivery immediately;
unknown authority fails closed. Revocation is not reported applied until owned
workers have installed the fence and affected in-flight delivery is discarded or
drained. Check scope again at delivery. Already delivered/exported data cannot be
recalled. Old policy snapshots can restrict a grant, never expand it after unblock.

Keep `projects` discovery as the legacy configured-binding catalog. Add versioned
**`query_scope`** discovery of only granted Projects/indexes/defaults/revisions,
with paged `projects`, `indexes` and `sources` views. Legacy-only connections get
an explicit configuration-required/legacy result for new Project scope; never a
machine-wide catalog. Detailed administrative library reads remain CLI/Indexing
management operations. The contract pack specifies inputs and bounds.

### Composition, budgets and result identity

The initial upper bound is **32 unique indexes** per query. Start qualified
composition with **fan-out one**, sharing the outer admission/deadline/budgets;
P7 may qualify a higher bounded fan-out before enabling it. Child work must not
multiply each member's concurrency or hold every parent slot while waiting for
another slot. The legacy one-index route retains its existing implementation.

Use equal-weight rank contribution **`1 / (61 + zero_based_local_rank)`**, policy
`airk.rank-fusion.rrf-equal/1`, with stable member/citation/local-order ties. Never
compare uncalibrated embedding/SQLite scores across indexes or call this a
probability/global-corpus top-k. A shared index supplies one candidate list and
one contribution, not one per Project. Distinct indexes remain distinct origins
even when their bytes/paths match. Diversity remains an independent P13 option.

Apply authorization/prefix unions/metadata filters before branch candidate limits.
Share count/text/time/source-verification budgets deterministically in canonical
index-ID order; publish allocation/cutoffs. Do not silently omit an unqueried
member or refill indefinitely. Explicit search mode applies to every member;
omitted mode discloses each saved policy, with no hidden vector/model fallback.

`member_availability: report | require` is independent of verification. Report
preserves usable data and all Project/member failures; require gives a typed
non-success when a requested member cannot answer. A readable retained index can
have unavailable live sources. Empty Projects and unavailable indexes are not
successful zero-hit members. A changed authority snapshot expires the response;
report mode cannot authorize delivery through revoked scope.

Return a publication vector/observation interval, not a global atomic token.
Qualified evidence identifies index, backing incarnation, canonical source root,
logical path/hash/native location and member publication. Shared-index attribution
is precise; summed index counts are not unique-source counts. Composite cursors
bind the full vector, scope/grant/Project/member revisions, filters, projection,
budgets and merge positions. A changed member expires the cursor. Search has no
invented continuation. Member-specific changes/validators keep their own identity.

File info/read/context/imports/outline/relationships/changes require one originating
index in new scope. Several Projects may authorize it. Union search, symbols,
code/document facts, references and files retain member-qualified results. Status/
diagnostics/capabilities aggregate bounded observations. Parse/recipes must keep
qualified bindings and request selection on ambiguity, with complete sessions,
cards, copies, exports and [verification](003-dated-saved-source-verification.md).

### Permanent policy and split ownership

Do not store permanent blocks solely in the registry or inside deletable derived
data. Existing [removal](../../crates/airk-store/src/service_project_removal.rs)
purges collection settings, including ordinary extraction policy. Add a bounded
**native per-index policy ledger** under the owned data root, outside collection
incarnations. P9 implements it as `source-policy.json` beside the stable collection
owner: the native registry supplies admitted policy authority, and Store retains
the enforcement copy with its exact identity and operation receipt. Neither copy
can independently widen the other's scope. Physical collection removal preserves
the enforcement copy, stable owner and index tombstone. Ordinary re-registration
of retired backing refuses; reactivation would require a separately qualified
reconciliation preserving policy and identity. Missing required policy fails closed. Deliberate loss
of all private AIRK state is not a recoverable-policy or OS-sandbox guarantee.

Bind policy to the stable logical collection ownership identity (including
relocation's original identity), index ID and canonical source-root mappings.
Alias spelling/removal/readdition does not change canonical source identity or
discard a deny. Policy version 1 contains positive exact/subtree includes and
permanent denies; deny wins over includes/ignore negations. Null includes means
all configured roots; an empty include list means no eligible sources. Defaults
stay on the empty-policy fast path. Relaxation is a separate reviewed operation;
existing access snapshots cannot automatically gain the relaxed scope.

Before the first policy is active, extend the required managed collection owner
contract with a versioned policy capability/reference. Exclude this new mutable
reference from the original ownership digest so existing publication identity
does not change; authenticate/fence policy identity separately. The current Owner
rejects unknown fields, and actual predecessor read/rebuild/write/purge refusal
must pass before activation. Re-creation consults the surviving ledger before
creating an owner. Unsupported standalone collection layouts cannot enable
trim/split until the same guarantees are implemented; registered service indexes
are the initial management surface. This is separate from registry schema 12 and
does not change lexical schema 126/140 or claim a collection rebuild is required.

Block first, then publish removal and reclaim under existing jobs/leases/quotas.
All capture/hash/model and retained read families—including graph endpoints,
conditional/cached reads, locators and verification—observe policy before use and
at delivery. A path/filter implementation unable to enforce it returns unavailable.
Blocking, current-index removal and physical cleanup have separate outcomes.
Cached sessions/exports/backups are not secure erasure; cleanup is explicit.

Split retains the **original index/primary-root identity for the remainder** and
creates **one** independent destination. Compile selection and remainder into
disjoint complete predicates under the reviewed original policy; exact files do
not become subtree grants. Inherit existing denies. Build through captured-source
indexing with independent objects, not SQLite copying or shared packs. Positive
selection prunes discovery before capture; no hidden whole-root scan.

Reserve destination/temporary/rewrite/cleanup peak capacity. Destination remains
staging/ungranted, including through legacy access, until validated. Then durably
fence transferred paths in the source, publish its remainder and perform reviewed
Project/grant cutover. No cross-database atomicity is promised. Before fencing,
failure preserves original access; afterward retain restrictive fences and resume
by receipt. Preserve manual/pause/watch policy and queued work; temporary drains
must not overwrite a concurrent user policy change. Other Projects keep the
original remainder unless the review explicitly adds the destination. Subsequent
policy changes need fresh impact review, not an implied perpetual partition claim.

### Upgrade and activation

Reserve **registry schema 12** against the inspected schema-11 tree; do not reuse
it for unrelated work. Add normalized index definitions, query Projects,
members/scopes, context import mappings, reviewed grants and migration mappings.
Retain old registration/job/lease ownership. Validate full schema identity,
application/namespace, foreign keys, bounds and indexes on all applicable opens.
Use expected revisions, checked counters and reverse membership indexes. Deleting
a Project can remove its joins, never cascade into index/source data or jobs.

U0–U6 are mandatory: preflight; exclusive activation/drain admission; consistent
protected SQLite backup; atomic additive schema/backfill; idempotent bounded
context imports; scoped readiness/client activation; durable settlement/recovery.
Integrate CLI activation's saved intent/owner drain with Store migration. Query,
webview and saved-reader opens cannot initiate migration. Backfill one stable
index mapping per registration without opening source contents or reindexing.
Known legacy contexts preserve exact choices; missing context means pending import
or a labeled one-index legacy Project, never guessed grouping.

Probe actual predecessor/candidate behavior and every fault boundary in P5.
Before new writes, validated recovery may restore under exclusive ownership.
After accepted post-upgrade writes, automatic backup restore/downgrade is refused;
forward repair preserves user work. Backups include committed SQLite/WAL state,
stay private and never restore collection directories. Retain settings, jobs,
outcomes, source watermarks, aliases, pause/manual policies and removal history.
P5 kept production activation disabled. P6f's 1.8.8 working candidate enables only
qualified native authority-1 Genesis/upgrade activation with reviewed management
and grant enforcement; its acceptance evidence and remaining gates are in the
[implementation ledger](../research/implementation-v190.md#p6f--catalog-management-and-scoped-native-activation).
Public combined reads remain **disabled until complete P7**. Old executables reject newer schema;
new legacy API adapters preserve one-index semantics without weakening fences.

Complete the existing owned control-layout/default-storage relocation while the
registry is still in its supported legacy format, before catalog admission. The
historical data-root rewriter replaces matching values across registry tables;
it cannot rewrite immutable catalog backing identities. Schema-12 calls must
refuse before creating a relocation intent or moving data. Already completed
relocation receipts remain valid. A later catalog-aware relocation implementation
would need its own typed identity/mapping transaction and qualification; general
root relinking remains outside this feature. P6 activation must preserve this
ordering and resume a pending catalog with its matching candidate before normal
readiness probes, rather than starting a predecessor or relocating partial state.

### Dated implementation clarification — 3 November 2026

Source observations are explicit bounded reads. They must not add filesystem
inspection or a registry-wide comparison to routine status, including Full status,
or change the legacy configured-binding `projects` catalog. Native administrative
library observations and configured/granted agent observations are separate:
the native administrative read may explicitly select the library; scoped adapters
must supply their authorized bindings and source prefixes. An agent-supplied ID
or an omitted selection is not authority. Actual CLI/MCP/Workbench contracts and
transport tests remain required before exposing equivalent reads.

Saved-root comparisons describe registry metadata rather than live filesystem
identity. Legacy watcher caches cannot prove all configured aliases are known;
missing schema-12 mappings or source imports cannot become a reassuring absence
of overlap. Continuations bind the complete bounded metadata snapshot and scope.
Root/alias metadata and response bytes have separate limits; disclose both row
and byte omissions. No other collection or source is opened for these comparisons.

Run observations for explicit setup/detail requests, not once per library row or
on each Recommendations/status refresh. Batch consumers must share the outer
metadata/time budget and reuse already observed inputs; they cannot multiply the
per-root allowance or trigger repository inspections as an implicit background
cost. Qualify any new default observation work separately before enabling it.

Repository hints inspect only the selected root and an explicitly requested
bounded sample of direct children. The tiny `.git` marker-file capture uses the
existing no-follow source reader; report its target without following it. Markers
and conventional/environment-based broad-root hints grant no indexing permission
and do not prove repository understanding or root safety. Marker-file reads must
join the source-prefix/policy audit before agent exposure and P9 enforcement;
they cannot provide a route around a source restriction. Native observation
primitives implemented during P5 do not themselves enable an Indexing screen,
new agent capability or production migration.

## Alternatives

Folder-equals-index preserves the current confusion and duplicate work. Copying
indexes per Project multiplies storage/models/jobs. Rootless merged storage or
cross-collection object sharing adds ownership/cleanup risk without evidence.
Client/UI federation creates a second query implementation and incomplete parity.
Registry-only bans cannot protect direct retained reads; deletable sidecars cannot
make blocks permanent. Reindexing as verification changes the evidence being
checked. None is selected.

## Consequences and qualification

The metadata and upgrade work is substantial. One index retains one owner,
watcher, model policy and publication stream despite many memberships. Narrow
selection reduces unnecessary work, with honest partial failures and independent
origins. No source writes, cross-service federation, automatic all-index fallback
or resolved cross-project edges are introduced.

[Contract schemas/fixtures](../research/contracts-v190/README.md),
[profile inventory](../research/profile-family-inventory-v190.md) and the
[phase ledger](../research/implementation-v190.md) define the remaining evidence.
P3 checks design consistency only. Migration, denial/old-reader safety, actual
transport parity, resource/latency and installed-platform behavior require their
later implementation gates.
