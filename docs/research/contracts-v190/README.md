# 1.9.0 design contract pack

Date: 2 November 2026. P3 design acceptance only. These original synthetic
fixtures specify future behavior; they do not prove migration, source verification,
policy enforcement, transport parity or installed-platform performance.

Read [ADR 002](../../decisions/002-projects-indexes-and-query-scope.md),
[ADR 003](../../decisions/003-dated-saved-source-verification.md),
[the feature design](../cross-project-indexing.md),
[profile inventory](../profile-family-inventory-v190.md) and
[phase ledger](../implementation-v190.md) together. ADRs settle implementation
choices where earlier research is illustrative. The approved phase order remains
unchanged. No new feature is enabled by installing these documents.

## Wire contracts and activation

| Artifact | Meaning | Implementation gate |
| --- | --- | --- |
| [query-controls.schema.json](query-controls.schema.json) | Closed selector/control fragment to merge into each applicable native tool schema; not a replacement for tool-specific inputs | P4 verification, P7 combined scope |
| [scope-discovery.schema.json](scope-discovery.schema.json) | Granted-only `query_scope` views, independently paged Projects, indexes and sources | P6 native grants/management; P7 public retrieval catalog |
| [source-policy.schema.json](source-policy.schema.json) | Versioned positive predicates and persistent denies, separate from registry version | P5 format/refusal fixtures; P9 enforcement, P10 partition |
| [verification-receipt.schema.json](verification-receipt.schema.json) | Dated full saved-byte observations, reference associations and explicit incomplete accounting | P4 single index; P7 member identities |
| [fixtures.json](fixtures.json) | Positive/negative wire examples, shared-index authority graph, partition, 24 workspace outcomes and U0–U6 faults | Design checks now; real handlers in their owning phases |

Run `pwsh -NoProfile -File scripts/check-v190-design.ps1` from the repository
root. Schema validation is supplemented by semantic fixtures: JSON Schema cannot
establish filesystem ownership, scope intersection, consistent timestamps, count
arithmetic or actual old-reader refusal. All native lengths must also be enforced
in UTF-8 bytes before allocation, independently of JSON Schema character lengths.
Canonical native contracts drive CLI/MCP catalogs and Workbench controls; UI-only
validation is insufficient. Reject unknown enum values and conflicting inputs.

The query controls use `project_ids` and optional `index_ids`. A legacy `project`
cannot be mixed with either. A new request without an index subset means all
permitted members of its explicit Projects. UI/session defaults first resolve to
explicit selectors, rather than changing that meaning. Grant/scope checks precede
any child work. Deduplicate shared indexes, not independent identical sources.
The fixture selects A/B and excludes C: `shared/api` and `shared/impl` stay
disjoint, and each hit is attributed only to its authorizing selected membership.

Discovery returns only the connection's reviewed grant. `projects` takes neither
ID; `indexes` requires one Project ID; `sources` requires Project and index IDs.
Defaults are preferences within permitted members. Each response includes the
scope/grant revision, observation, truncation and continuation identity. Tokens
bind view, arguments and revisions. The legacy `projects` catalog keeps its
existing meaning. Administrative library enumeration is a separate native read.

| Read family | New-scope execution | Required retained identity |
| --- | --- | --- |
| Search, symbols, code/document facts, references, files | Unique-index union under total budget | Index, publication, root, source and precise selected Project associations |
| File info/read/context/imports/outline/relationships/changes | One originating index required | Same origin, validators and endpoint provenance; ambiguity asks for selection |
| Status/diagnostics/capabilities | Bounded member observations | Per-member outcomes/readiness, never one invented atomic publication |
| Parse/recipes and conditional reads | Qualified bindings, no guessed file origin | Full step/session/export data and unchanged typed retries |

Start with at most 32 Projects/unique indexes and fan-out one. Use deterministic
canonical index-ID allocation of the shared outer count/text/time/verification
budgets and disclose child cutoffs. RRF policy is `airk.rank-fusion.rrf-equal/1`,
contribution `1/(61 + zero_based_local_rank)`, stable index/citation/local-order
ties. One index keeps the legacy route. Never compare raw scores between indexes.
Report/require member availability remains independent of report/require source
verification. No all-library fallback, implicit model work or silent member drop.

## Registry schema 12 and migration ownership

Reserve schema 12 against accepted P2b's schema 11. Preserve registry application
ID, 32-MiB file bound, 1,024 legacy registration bound, namespace ownership and
all existing jobs/leases/settings/removal outcomes. New catalogs use checked
counters and expected revisions, not wall-clock ordering. Bound each catalog to
1,024 definitions/contexts/grants and each Project to 32 memberships; at most 128
scope atoms per membership and 17 configured source roots per index. Reject
capacity excess explicitly; never evict a reviewed grant or policy silently.
P5 must test actual serialized size at these limits against the registry ceiling.

| Normalized metadata | Ownership and foreign-key rule |
| --- | --- |
| Index definition | Stable index ID, immutable backing binding/namespace/collection identity, tombstone, revisions and policy reference/expected epoch |
| Active registration mapping | Optional FK to legacy `projects`, `ON DELETE SET NULL`; immutable backing mapping survives purge outside that FK |
| Query Project | Stable Project ID, editable name, definition/scope/default revisions |
| Membership and scope atoms | FKs to query Project/index definition; reverse index for impact checks; deleting a Project removes joins only |
| Context/import mapping | Host/context identity, exact selected Projects/index defaults and idempotency key; unknown legacy context remains pending |
| Reviewed access snapshot | Stable grant ID/revision, frozen authority atoms and epochs; private bearer token remains separate |
| Upgrade/operation receipt | Stage, owner, expected schema/digests, backup identity and accepted-new-write barrier; no source contents |

An immutable backing FK must not prevent existing registration purge or cascade
index/policy deletion. No Project deletion deletes source/index data. Settle or
block an existing data-root relocation before U0; do not mix its identity changes
with catalog activation. P5 fixes concrete SQL/indexes and validates full schema
shape on every applicable open, not merely `user_version`.

| Stage | Required operation | Fault and recovery boundary |
| --- | --- | --- |
| U0 preflight | Validate owner, known schema/table shape, bounds, disk headroom, runtime/client capabilities and no active relocation | Foreign/newer/corrupt input refuses without mutation |
| U1 exclusive activation | Persist intent, stop new admission, fence/drain owned workers and jobs under existing installer ownership | Unknown owner blocks; accepted work/receipts survive; no process guessing |
| U2 backup | Consistent SQLite backup with identity, digest, schema, namespace/owner evidence and bounded capacity | Ordinary saved/writable opens still reject WAL/SHM overlays. Only an explicit owned migration adapter may validate/checkpoint or back up recognized committed WAL after drain; never delete or ignore it |
| U3 additive transaction | Create normalized tables, map each legacy registration to one stable index; no source opens/rebuilds | Crash before commit rolls back; after commit resumes stable IDs, no duplicate rows |
| U4 imports | Bounded idempotent exact context imports | Duplicate import reuses IDs; missing editor state is pending, never guessed grouping |
| U5 readiness | Probe actual predecessor refusal and candidate compatibility, complete UI/grant path | Probe failure permits only owned validated recovery before new writes; no half-enabled production schema |
| U6 settlement | Record activation and first accepted post-upgrade write barrier | After new writes, reject automatic restore/downgrade; forward repair preserves them |

Production activation remains disabled through P5 and is enabled only with P6.
Combined public retrieval remains disabled until P7. Probe actual predecessor
executables as readers/writers, not just mocked version integers. Backups remain
private, preserve committed registry state and never restore collection directories.
Document restore eligibility and operator recovery receipts; do not blindly reuse
the existing runtime launcher's rollback after new metadata writes.

## Permanent policy, trim and split

Store owns the native policy ledger outside deletable collection incarnations:
`source-policies-v1/<namespace>/<collection>/`. Registry references/epochs are
fences, not a second rule authority. Bind stable original ownership identity and
index/root IDs. Preserve ledger/tombstone on purge/re-registration; missing
required ledger fails closed. The collection owner gains a separately validated
policy capability/ref that old executables reject. Exclude that mutable field
from the original owner digest so publication identity is unchanged. Test real
old read/rebuild/write/purge refusal before policy activation. Unsupported direct
standalone layouts cannot enable these operations until equally protected.

Null positive includes mean configured roots; empty includes mean no sources.
Exact and subtree predicates use components, never arbitrary prefix expansion.
Deny wins before capture/hash/model work and in every retained read, cache,
conditional locator, graph endpoint and delivery path. A source alias is not new
identity or permission. Narrowing first fences admission and cached/in-flight
delivery, then awaits worker acknowledgment before reporting applied. Unblock is
a separate reviewed relaxation and cannot widen old grants. No secure-erasure or
recovery-after-total-private-state-loss guarantee is made.

Trim stages are preview/impact/capacity, persist restrictive policy, acknowledge
fences, publish removal, reclaim, settle. Failure after deny never rolls authority
back merely to make a job succeed. Report blocked/current-removal/cleanup states
separately. Rebuilds and re-registration consult policy before discovery.

Split leaves the original index as remainder and creates one independent
destination. Compile selected predicate and its complement under the old policy;
inherit denies into both and preserve exact-file semantics for future paths.
The fixture proves no overlap/loss, excludes private paths and prevents
`src/a.rs.backup` from inheriting an exact-file selection. Bound include/deny
expansion before admission. No whole-root capture to discover a selected subtree.

Stages: reviewed plan/revisions/affected Projects; reserve peak destination plus
temporary/rewrite/cleanup capacity; capture/build independent staging destination;
validate publications; persist source transfer deny and acknowledge fences;
publish remainder; reviewed membership/grant cutover; reclaim and settle. Until
cutover, destination is ungranted even through legacy access. Before source deny,
failure leaves original access; afterward retain the deny and resume by receipt.
No cross-database atomicity or shared objects/packs are assumed. Preserve concurrent
manual/pause policy revisions. Projects not reviewed for destination retain only
the original remainder. Later relaxations require a new impact review.

## Verification safety and presentation

Off is the existing fast path: no source lookup/hash/placeholder. Enabled calls
require the full explicit flat budget tuple. Safety ceilings are 256 sources,
1 GiB aggregate bytes and 30 seconds cooperative deadline. These are rejection
ceilings, not default presets or latency promises. P4 measures real native capture
before publishing client presets. Initial stream buffer is 64 KiB, concurrency one.
Charge attempted bytes; a raced/partial read is unchecked, never matched.

Inventory every typed returned file reference before checking, at most 4,096
associations. Reserve at most 32 KiB serialized receipt metadata within the outer
reply budget; do not add an unbounded receipt after truncating the original reply.
If it cannot fit, expose exact/lower-bound counts, omitted rows and incomplete
accounting. Never certify a hidden subset. Original results/qualifiers remain;
require yields typed `verification_required_unmet` with complete attempted evidence
and receipt, not a generic error string. References already intentionally absent
from a selected projection are not invented; every actually returned reference,
including supplemental pointers and conditional targets, is accounted for.

Source rows link to a response-local origin and every reference association;
selected copies materialize/rebind those links. Counts are exact only when known.
Checked = matched + mismatched. Unchecked includes missing/unreadable/out-of-scope,
unsupported, raced, expired and budget-limited sources. Null denominator means
unknown, not zero. Known unresolvable file references are unsupported; unresolved
written language assertions remain qualified assertions rather than imaginary
file locators. Zero references is not applicable. Shared memberships never cause
duplicate hashing or broaden attribution. Revalidate original publication/scope
at completion; do not patch old snippets with a newer hash.

CLI prints structured non-success before nonzero exit; MCP returns structured
error evidence; Workbench/parse store the complete typed outcome. Global parse
verification inventories final output once, deduplicating steps. Initially reject
enabled global plus enabled per-step modes if their independent budgets cannot
be represented without duplicate hashing. Prepared digests bind those controls.
Unsaved-buffer observations remain separate. Historical receipts keep their dates;
saved-byte equality never removes extraction, relevance, coverage or unresolved
reference qualifiers, and never changes collection-wide live verification status.

## Workspace, recommendations and later acceptance

Fixtures 1–20 map the original folder/workspace scenarios; 21–24 add empty,
untitled, simultaneous-window and quiet saved-context reopen cases. All forbid
automatic registration, scope expansion or source scanning on open. Case-specific
behavior is recorded in `workspace_scenarios`; P6/P14 exercise real host paths.
Untrusted windows launch no native process. Local/remote authorities do not union.
Display rename/reordering does not restart MCP. Unknown moved roots do not relink.
Manual configuration is preserved. Saving defaults does not widen active grants.

P8 recommendations consume only bounded already-observed metadata: membership/
scope revisions, saved counts and precision, overlap identity proof, pipeline/
storage readiness and dated observed query-cost cohorts. Missing data yields
unknown, not a scan. Evaluate at most 32 visible candidate indexes and eight
advisory rows per request; disclose pagination/incomplete library coverage. Never
infer a measured speedup from size alone. Explain one index versus an explicit
combination, overlap, split/trim capability and expected tradeoffs. Actions open
review or temporary selection; saving defaults is separate. Generated harness
instructions use the live `query_scope` catalog and exact `project_ids`/`index_ids`,
show narrowed source scope, handle typed failures and never fall back to all
indexes. No credentials/private source contents enter copied instructions.

P4 proves source receipts/races/budgets/actual transports and exports. P5 proves
migration/fault safety with production activation disabled. P6 proves complete
management/grants and workspace cutover; P7 proves combined reads, identity and
parity. P9/P10 prove all-read denial and crash-safe partition/resource bounds.
P11a implements the legacy family inventory without scans; P11b separately
migrates new profiles. Each phase retains the frozen and preceding performance
gates. Design-fixture success is never substituted for those runtime tests.
