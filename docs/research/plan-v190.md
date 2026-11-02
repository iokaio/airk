# AIRK 1.9.0 research assessment and delivery plan

**Assessment date:** 2 November 2026. **Baseline:** AIRK 1.8.7.
**Status:** recommended scope and implementation gates, not implemented behavior or
a release announcement. This plan synthesizes **five primary research inputs**:
the three original notes, the fourth research area within this document, and
the separate cross-project feature design. Dated observations retain their limits.

| Input | Assessment / location |
| --- | --- |
| 1. Copilot search comparison | [Original note](copilot-search-analysis.md) and assessment below |
| 2. Two-tier indexing | [Original note](two-tier-indexing.md) and assessment below |
| 3. Folders/workspaces | [Original note](vs-code-folders-workspaces.md) and revised assessment below |
| 4. Returned-reference verification | [Fourth research area in this plan](#fourth-research-area-verifying-returned-references-against-saved-sources) |
| 5. Projects/indexes and combined queries | [Feature design](cross-project-indexing.md), summarized within folders/workspaces below |

The [1 November MCP investigation](../mcp-recommendations.md) is supplementary
evidence; its implementation backlog is reconciled below rather than treated as
a competing delivery plan.

## Recommended direction

Make 1.9.0 a release about **intentional indexing and dependable, useful evidence**.
Repair the reported compact-plan failure first. Then prevent accidental duplicate
and container indexing, separate reply detail from retained evidence, and improve
ranking against a reproducible baseline. These changes reinforce one another:
avoiding duplicate collections reduces unnecessary work, file-aware retention
preserves useful facts, and ranking helps callers find those facts within budgets.

Recommended order for addressing the research as implementation work:

1. [Two-tier indexing](two-tier-indexing.md), correctness slice only: compact
   evidence consumers and nested-plan schema agreement.
2. [User-managed indexes and cross-project queries](cross-project-indexing.md),
   superseding the folder-driven setup design: native index definitions, explicit
   source assignments, reusable Projects above indexes, the new Explorer Indexing
   screen and queries scoped to one or more Projects. Include the SQLite schema
   upgrade and client migration as required feature work. Retain the original
   [folder/workspace scenarios](vs-code-folders-workspaces.md) as acceptance inputs.
3. Return to **two-tier indexing** for independent replies, family availability,
   versioned file-aware profiles and explicit migration.
4. Add **returned-reference source verification**, described below: one bounded
   native check shared across interfaces, with explicit refresh when files differ.
5. [Copilot search comparison](copilot-search-analysis.md), measured ranking and
   diversity improvements, with consistent missing-evidence guidance.

Start the search evaluation baseline alongside the first correctness work; it
does not need to wait for workspace changes. This order prioritizes demonstrated
task failure and unwanted resource use before speculative relevance improvements.
It is deliberately a sequence of slices across the papers rather than completing
every proposal in one paper before opening the next.

## Assessment of the five research inputs

### Two-tier indexing and reply detail

**Assessment:** the strongest direct task-level evidence of the three original notes.
The Windows 1.8.7 battery recorded 97 calls across 17 tools on an Agent Lean
collection. It found useful definitions, signatures, text and DOCX rows, but also
a valid compact-discovery plan whose dependent step was skipped while the rich
equivalent completed. Shared file bindings were lost by a consumer; increasing
index detail is not the remedy. The plan schema's object/string discrepancy also
needs tracing across the native schema and connector boundary.

The paper usefully separates three decisions: evidence retained during indexing,
record projection during a read, and metadata encoding/repetition. Its Cargo
manifest example shows why a global fact-kind allowlist can discard an entire
useful family. A richer reply cannot recover unretained facts. Conversely, larger
replies need not improve search breadth: full chunks admitted fewer hits in one
matched budget example.

**Value:** reliable chained investigations; smaller replies with equivalent
meaning; useful package/dependency evidence in Lean; fewer unexplained empty
results; clearer cost and migration choices. Reported 16.1–38.0% matched-envelope
reductions motivate evaluation, but establish neither token savings nor lower
completed-task cost. The broken compact plan's apparent reduction is not savings.

**Limits and recommendation:** one Lean-index project on Windows is not a paired
Lean/Rich indexing benchmark or general harness qualification. Adopt the paper's
consumer repair, then independent replies, then file-aware profiles. Start with
one selected payload per file/profile under the existing publication. Defer
separate rich objects and on-demand enrichment jobs until measured costs justify
their integrity, retention and recovery complexity.

### VS Code folders, workspaces and project identity

**Assessment:** the clearest account of avoidable operational cost and confusing
setup behavior. Its 20 scenarios connect per-folder automatic registration to
overlapping collections, broad container indexes, accumulated watchers and fragile
multi-root connections. Saving a folder as a workspace preserves indexing;
window-keyed saved searches and settings are separate issues. That distinction
prevents solving the wrong problem.

Native source-to-source observations remain necessary, but the new
[cross-project feature design](cross-project-indexing.md) supersedes the proposed
automatic per-folder registration/workspace plan. Users define persistent named
indexes and choose which source roots belong to each, independently of VS Code
workspaces. **Project → Index → source folders** becomes the hierarchy: Projects
are reusable metadata above indexes, with a many-to-many Project–Index join.
Each trusted window opens one or more Projects; retrieval selects one or more of
those Projects and resolves their permitted indexes, including same-host sources
outside open folders. Shared indexes are stored, watched and queried once.
Repository markers remain bounded suggestions. Opening a folder must not silently create an index or start
watching; existing saved indexing policies remain authoritative.

**Value:** fewer duplicate scans, embeddings and watchers; predictable budgets;
more intentional source selection; stable multi-index agent access; reuse of a
parent collection when opening a package subfolder; and searches spanning code,
specifications and shared repositories. These are expected benefits,
not measured resource reductions from this source review.

**Limits and recommendation:** platform path aliases, profile state and several
installed-editor transitions remain unverified. The proposal also needs a scope
decision: a hard connection-prefix intersection cannot silently permit references
outside that prefix. For 1.9.0, enforce the prefix on every evidence-bearing read
and report restricted coverage. Whole-project expansion must use an explicit,
separately authorized selection; treat a soft default search filter as a different
contract. Add a top-level **Indexing** screen for the index library, source
assignments, Project membership and window access; assemble connections from
reviewed Project grants rather than folder order. A combined query retains
selected Project/membership revisions and per-index publications,
coverage, verification and errors; it never implies an atomic global snapshot.
Defer root relinking, shared embedding ownership and automatic pruning
of registered descendants. Each introduces lifecycle or identity risk beyond
the explicit setup and bounded query composition required here.

#### Revised folders/workspaces plan: explicit indexes and combined queries

The separate [feature document](cross-project-indexing.md) specifies the data model,
Explorer screen, first-use flows, grants/revocation, query contracts, ranking,
budgets, continuations, errors, migration, ownership and acceptance scenarios.
Its X0–X6 packages replace this plan's original W1–W3 packages. W0's compact-plan
repair remains first; T1–T3, V0–V3 and relevance work remain applicable.

- **Define Projects above indexes:** native Project records specify available
  indexes, allowed subsets and query defaults. Many Projects may share one index;
  many indexes may belong to one Project. Index IDs map to existing collections
  without rewriting identity; explicit roots/aliases define each index's contents.
  Removing membership or deleting a Project never deletes the shared index.
- **Choose window access:** open/close one or more reviewed same-host Projects,
  save window associations and select one or more Projects per retrieval query.
  Resolve memberships to a deduplicated index union with optional index narrowing.
  Opening Projects does not start indexing; a new member/alias cannot silently
  expand existing grants. Empty, untitled, saved and remote windows have explicit
  flows. Remote/local services remain separate in 1.9.0.
- **Manage index contents:** add reviewed **Split index** and **Remove path and
  block permanently** actions. Splitting partitions one index into a retained
  original plus a new independent index; trimming persists an index-level deny
  and removes the path's evidence. These affect every Project sharing that index,
  preserve source files and require durable jobs, publication fences and recovery.
  See the management contract below and the synchronized feature design.
- **Provide the Indexing screen:** This window, Index library, Index detail,
  Sources, Project definitions/membership and a review drawer replace folder-driven
  setup. Keep service/storage/
  recovery operations in Management with targeted links. Preserve manual agent
  configurations and show pending connection updates separately from active scope.
- **Support multi-Project reads:** new `project_ids` selects one or more metadata
  Projects; optional `index_ids` narrows their permitted index union. Shared indexes
  execute once over the authorized prefix union, with correct Project attribution
  and no duplicate ranking boost. Keep qualified single-index file follow-ups,
  publication vectors, total budgets and per-member errors. Legacy `project`
  identifies an index binding through compatibility, not a new Project ID.
- **Upgrade SQLite safely:** add normalized Project/index catalogs and many-to-many
  membership/scope/grant tables in the existing registry. Keep old registration
  rows and index storage identities. Include native migration admission, consistent
  protected backup, transactional version/backfill, idempotent editor imports,
  mixed-version refusal, recovery and rollback limits (U0–U6 in the feature design).
- **Integrate certainty and parity:** V0–V3 receipts bind to each member citation;
  verifying returned files cannot certify unavailable indexes. Every CLI/MCP/
  Workbench consumer, parse binding, session, copy and export retains member identity
  and complete readiness/continuation/retry data. Test actual transports.

Implement catalog/review and the Indexing screen first (X0–X2), enforce active
connection scopes next (X3), then shared combined reads and consumers (X4–X5),
and complete migration/fault/platform qualification (X6). An index picker alone
does not satisfy this feature. This is a material expansion of 1.9.0 scope;
estimate after X0/X1, prioritize it over optional ranking experiments, and report
any reduced release scope explicitly.

#### Index management: split and permanent path removal

These are requested full-scope features, replacing the earlier deferral of index
splitting. They are native reviewed management operations, exposed in Explorer
**Indexing → Index detail → Sources** and equivalent CLI management commands.
Evidence MCP remains read-only; its responses expose resulting scope/availability
and operation status without triggering mutations.

**Split into two indexes.** The user selects source roots, directories or exact
files to move to a named second index. The existing index retains its ID and the
remainder; the new index gets independent identity, publication, quota, settings
and jobs. Preview both source partitions, expected work/storage and affected
Projects. Include/exclude selectors must partition the effective eligible source
set without overlap or unexplained loss; reject an empty side. Subtree selectors
apply to future eligible descendants, while exact-file selectors stay exact.
Existing blocked paths remain blocked in every resulting index that covers their
original source location. Splitting cannot resurrect previously removed evidence.

Implement versioned native inclusion/deny predicates where existing whole-root
aliases cannot express a partition. Preserve the original permanent primary-root
binding, even when only part of that tree remains eligible. Do not rename root
identity or merge/copy SQLite databases. Build the new index with existing captured
extraction/publication mechanisms and independent object ownership; initially
re-extract/re-embed rather than assuming safe cross-collection object transfer.
Reserve peak capacity for the original, new index and cleanup/rollback allowance.
Freshness/readiness still describe actual per-index observations; no simultaneous
filesystem snapshot is promised.

The split preview includes a Project membership plan: retain the original only,
attach both resulting indexes, or assign the new one to chosen Projects. Require
reviewed scopes for all affected memberships; do not silently expand grants or
replace the original index in every Project. Existing connections must renew if
needed. A Project that does not gain the new index is told that the moved paths
will no longer be available through its original membership.

Stage the destination as unpublished to ordinary clients until its base evidence
and the required readiness policy validate. Then durably fence the moved paths
in the original, publish the residual state and activate the reviewed membership
plan. Use a resumable operation receipt across these steps; collection publication
and registry changes are not one SQLite transaction. Before cutover, failure leaves
the original intact and the staged destination ungranted. After cutover begins,
failures preserve deny fences and report pending/failed completion, resuming exact
recorded steps without replay or silently undoing exclusions. Do not announce
success until both final partitions, grants and cleanup outcomes are accounted for.

**Trim and permanently block a path.** Select an exact file or directory subtree
inside the index's authorized source roots. Preview affected artifacts, vectors,
facts, relationships, root assignments and Projects, with explicit unknown counts.
On acceptance persist an index-owned deny rule and advance its policy revision
before admitting further evidence from that path. The deny has no expiry and
wins over include rules, negated ignore rules, rebuild/repair, re-registration,
profile changes, watches, explicit indexing and any include-ignored convenience.
Renaming an alias or removing/readding the same canonical source cannot bypass it.
Only a separately reviewed **Unblock path** policy change can remove the rule;
ordinary indexing actions never do so. Blocking a path in index A does not block
it in another index B; the review names other indexes covering it.

Apply the policy in discovery/capture, hashing/verification, watchers, extraction,
semantic work, all retained reads and all retrieval tools, including exact paths,
relationship endpoints, locators, conditional replies, caches and continuation.
Fence stale grants/cursors/publications and accepted responses before delivery
when exclusion is observed. If a tool cannot safely filter a transitional view,
return typed unavailable rather than expose removed evidence. Never present a
partial cleanup as proof that no stored bytes remain.

Under existing service leases and quota/reclamation ownership, publish removal
of text/fact/posting/vector/relationship references and rewrite shared packs where
needed before reclaiming obsolete objects. Report **blocked immediately**,
**removal pending**, **removed from current index** and **physical cleanup pending/
complete** separately. Failure/restart leaves the deny active and resumes cleanup.
Source files stay untouched. Already delivered results, exported files and retained
upgrade backups cannot be recalled; offer separate local session/cache cleanup
where supported and disclose retained copies. This is not a secure-erasure claim.

Persist canonical source-root identity plus normalized source-relative path and
exact-file/subtree kind, not just an editable alias string. Revalidate containment,
platform case/path forms and source binding. Root rebinding, split and upgrade
must carry forward or explicitly reconcile deny rules; never drop an unmappable
rule silently. Existing native policy/store owners must enforce the same rule
even for direct CLI access; an extension setting or `.airkignore` edit alone is
insufficient and would write into the source tree.

M0–M2 below deliver this work. M0's schema/version design feeds X1/U0–U3 so the
upgrade includes deny-policy ownership, partition predicates and split receipts.
Older runtimes must refuse unsupported policy formats instead of reading excluded
content. Qualification covers same-size edits, new descendants, alias remove/readd,
Unicode/case/links, full-root blocks, shared vectors, exclusion during active
reads/jobs, blocked endpoint leakage, source churn, quota failure, cancellation,
crash at each split phase, upgrade/rebuild preservation and affected Projects.
Reject an unsplittable/empty partition explicitly; never weaken isolation to finish.

### AIRK and Copilot search comparison

**Assessment:** a useful architectural comparison and experiment backlog, with
appropriately limited claims. The pinned, selected Copilot client snapshot cannot
establish current hosted behavior or comparative retrieval quality. Its most
actionable AIRK finding is local: normalized AND lexical candidates ordered by
stable ID can omit relevant candidates before hybrid fusion and give identity
order a relevance-like vote. Optional diversity and stage-level diagnostics are
also concrete ideas that fit AIRK's scope.

**Value:** better evidence near the top of bounded results; broader file coverage
when repetition consumes the budget; clearer explanations of missing results;
an evaluation framework that separates candidate misses from selection and
presentation problems. Keep the existing semantic-unit/citation distinction,
local execution and DiskANN direction.

**Limits and recommendation:** ranking mechanisms do not prove answer quality.
Build original relevance judgments before changing defaults. Test lexical ranking
first, preserving explicit AND matching unless an alternative is selected. FTS5
BM25 is a design option, not a drop-in fix for the current `detail=none` and
`columnsize=0` storage. Evaluate optional diversity separately. Defer model
replacement, local reranking and unsaved-buffer overlays; exclude cloud retrieval,
generative selection and agent orchestration from this release.

## Decision map

Priority reflects demonstrated failure, breadth of benefit, dependencies and
state/contract risk. Effort is relative engineering scope, not a calendar estimate.
Every choice must preserve source identity, bounded reads and interface parity.

| Decision | Options considered | Recommended choice for 1.9.0 | User value | Effort / risk | Evidence or decision gate |
| --- | --- | --- | --- | --- | --- |
| Compact-plan reliability | Force rich replies; raise index detail; repair shared-binding consumers | Repair consumers first | Dependable discovery-to-read workflows | Small–medium / correctness | Compact/rich plans produce equivalent useful dependent evidence through real transports |
| Project/index organization | Folder equals index; copied indexes per Project; many-to-many metadata | Project above Index above source folders; shared membership, explicit assignment | Reuse one index across Projects/windows without duplicate jobs/storage | Large / lifecycle and migration | Project deletion/removal never deletes shared data; index operations disclose affected Projects |
| Subfolder opening | New collection; parent with soft filter; scoped reuse or another explicit attachment | Offer scoped reuse without forcing it; allow reviewed alternatives | Reuse indexing while keeping user control | Large / authorization boundary | All reads/handles/exports honor granted root/prefix intersections |
| Container handling | Automatically index whole tree; recursive discovery; explicit separate/combined assignments | Review selected sources in the new Indexing screen | Understandable source boundaries and shared-versus-separate budgets | Medium / setup admission | No silent discovery or registration; aliases/provider limits disclosed |
| Window access and connections | Derive from folders; expose machine catalog; grants for selected Projects | One or more open Projects per window, stable native grants | Reusable scope across windows including unopened same-host sources | Large / scope and configuration | Project/member/alias changes, independent windows and manual-entry scenarios tested |
| Cross-project queries | One Project only; client loops; canonical union across Projects | Explicit one-or-more Project scope, optional index narrowing, deduplicated execution | Investigate code/specs across Projects while reusing shared indexes | Large / budgets, ranking and partial failure | Shared-index scopes/attribution and scoring correct; actual parity; complete errors/vectors/cursors |
| SQLite upgrade | Rename old project rows in place; rebuild indexes; additive normalized metadata migration | New Project/index join plus compatibility mappings, U0–U6 upgrade path | Preserve existing data/settings while introducing the new hierarchy | Large / database and mixed-version safety | Consistent backup, atomic backfill/version, idempotent imports, crash recovery and post-write downgrade refusal |
| Split index | Copy entire databases; defer splitting; reviewed captured rebuild into two partitions | Retain original ID for the remainder and create an independent second index, with explicit membership cutover | Separate unrelated sources/budgets without losing Project control | Large / multi-index operation and peak disk | Disjoint complete partitions, inherited denies, staged destination, resumable fenced cutover and no premature grants |
| Trim and permanent path block | Delete current records only; source ignore edit; native index-owned deny plus cleanup | Durable deny first, then remove evidence and reclaim owned storage | Remove unwanted material and prevent future reintroduction | Large / policy enforcement and reclamation | Every capture/read path honors the deny across restarts/rebuilds/aliases; physical cleanup reported separately |
| Reply control | Change all defaults; preserve compatibility with explicit Lean/Rich selection | Preserve no-argument behavior; explicit Lean/Rich projection, with Lean recommended for agent/CLI setup and Rich for Workbench | Predictable detail without surprise byte/field changes | Medium / compatibility | No new argument on an existing Agent Lean collection retains its projection/encoding; actual full-result parity for explicit choices |
| Retained evidence | Rename old levels; file-aware Lean/Rich; dynamic enrichment | New versioned Lean/Rich profiles; keep Rich Structure as the legacy `balanced` label | Useful manifest/prose/table evidence without confusing old continuity semantics | Large / publication and migration | Legacy label/identity preserved until explicit migration; new Rich is a superset of new Lean |
| Rich storage | Selected payload; common core plus rich objects; second mutable index | Selected payload first | Simpler ownership and recovery | Medium / bounded change | Cost campaign demonstrates whether separate rich objects are warranted later |
| Lexical relevance | Stable-ID ranking; ranked exact matching; implicit relaxed matching | Evaluate ranked exact matching; stable IDs break ties | More useful candidates within existing budgets | Medium–large / index cost | Matched relevance gain, narrow-scope recall and authenticated update behavior |
| Diversity | Hidden display cap; explicit optional selection; always-on file cap | Explicit optional selection, judged separately | More distinct useful files when results repeat | Medium / recall tradeoff | Disclose subset; preserve concentrated-result option; no hidden export loss |
| Missing evidence | Empty results; new status subsystem; reuse canonical coverage plus actionable reasons | Extend existing observations only where necessary | Distinguish absence from unavailable evidence and choose a next step | Medium / shared contract | Separate unsupported, omitted, pending, failed, stale and truncated outcomes |
| Certainty of returned references | Reindex every hit; metadata-only checks; hash distinct returned sources; re-extract every source | Bounded full-source hash verification, separate explicit refresh on mismatch | Evidence of saved-source agreement at a stated time, usable by people and agents | Medium–large / file races, scope and latency | Every referenced source accounted for; stable captured bytes match the exact result hash; failures cannot receive a verified label |
| Advanced optimization | Reranker/model swap; unit tuning; cross-collection cache | Defer; retain measured experiment backlog | Avoid new resource/lifecycle cost without demonstrated benefit | Large / qualification | Revisit only with baseline misses and measured task/resource gains |

## Work packages and review boundaries

Use separate PRs with one behavior and its complete native/interface contract per
review. Do not land native fields and leave parity to a later cleanup PR. The
package identifiers below describe dependencies, not a requirement to create one
large PR per row; split extensive packages by the indicated acceptance boundary.

| Package / order | Deliverable and boundaries | Depends on | Completion evidence and value |
| --- | --- | --- | --- |
| W0 — first | Repair compact locations/aggregation/bindings and trace nested-plan schema mismatch. Convert the two-tier battery's 35 post-run invariants into appropriate deterministic regressions, retaining negative cases and the compact-plan task assertion. | None | Actual compact/rich native/CLI/MCP paths preserve citations, identity, guard failures, budgets and dependent reads; captured checks are not mistaken for passing workflow qualification. |
| E0 — start with W0 | Extend tooling with original versioned code/manifest/prose/table queries and graded expected ranges. Use nDCG@10 as the primary relevance metric and Recall@10 as the recall guard, scored separately for lexical/vector/hybrid at fixed budgets. | None | Frozen judgments, negative-query outcomes, bytes/expansions and cold/warm latency; synthetic harness cases are not the production relevance baseline. |
| X0–X1 — replaces W1 | X0 records current workspace fixtures/limitations and produces ADR 002 for Project above Index. X1 implements new bounded native `relationships` (same/contains/contained_by), `repository_markers` and broad-root observations with parity, catalogs/join/scopes/grants and U0–U3 migration including M0 policy metadata. | W0 for consumers; X0 design can start earlier; M0 before X1 schema freeze | Source-to-source observations are new work; preserve old isolation. Metadata bounds, permanent-policy ownership, stale review, SQL/backup/backfill/recovery and actual transport parity pass. |
| X2–X3 — replaces W2/W3 setup | New Indexing screen, Project membership/source assignment, opening Projects per window and enforced grants/revocation. | X1 | No unrequested folder indexing; shared indexes reused; Project/member/window scope and actual transport tests pass. |
| X4 — combined reads | Multi-Project resolution, unique-index execution, qualified follow-ups, vectors/errors/cursors and canonical input catalogs. | X3, T1; no dependency on V2/X5 | Actual parity; shared membership cannot duplicate scoring, jobs or counts; per-Project attribution and scopes preserved. |
| X5 — combined consumers | UI/parse/session/copy/export integration; extend already shipped V2 single-index receipts to each member citation. | X4, T1, V2 | Full data survives every consumer; member receipts extend the predesigned identity fields without changing their guarantee. |
| M0 — content-management contract | Design split partitions, persistent index-owned include/deny policy, operation receipts and shared-Project effects; include compatibility and schema work in X1/U stages. | X0 design; before X1 migration freeze | No alias/ignore/rebuild bypass; ADR 002 includes split/trim lifecycle and upgrade implications. |
| M1 — permanent trim | Native deny admission/fencing, removal publications, pack reclamation, status and Inspector/CLI UI actions. | M0, X1, X3; X2 for Explorer UI | Immediate exclusion across actual native/transport paths; policy survives failure/upgrade/rebuild and cleanup has honest states. |
| M2 — split index | Reviewed partition/destination preparation, capacity admission, independent indexing, residual deny/publication and membership cutover with durable recovery. | M1, X2, X3; existing native publication jobs | Crash-safe two-index result, no source mutation, no implicit shared objects or grant expansion; inherited exclusions and ownership verified. |
| X6 — feature qualification | U4–U6 context/client migration and activation; complete upgrade/fault/resource/relevance/installed-editor matrix including split/trim. | X1–X5, M1–M2 | Mixed-version handling, operation recovery/cleanup, rollback limits and explicit platform evidence; catalog-only delivery is insufficient. |
| T1 — independent replies | Explicit canonical Lean/Rich selection and legacy compatibility; preserve existing absent-argument defaults and encoding, including Agent Lean MCP/CLI reads. Workbench keeps its existing rich behavior. | W0 | Explicit-choice parity; regression comparing no-new-argument results modulo documented identity/timing; conflicting flags reject. No storage change. |
| T2a / T2b — availability and profiles | T2a defines shared availability/reason vocabulary using existing observations, without profile or registry changes. T2b inventories families and adds bounded effective-profile reporting before new profiles. | T2a: T1; T2b: T2a, with X contracts reconciled if selected | Unsupported/not retained/reply-omitted remain distinct; bounded status. T2a can ship independently of X/T3. |
| T3 — file-aware evidence | Implement Lean/Rich first for sampled code, manifests, Markdown and DOCX; publish under existing ownership. Separate migration/recovery review from adapter policy review. | T2b | Manifest facts survive Lean; Rich retains the core; pause/manual/vector settings survive; interruption/quota failure keeps the valid publication. Broad adapter qualification remains explicit. |
| V0–V2 — single-index verification | V0 produces ADR 003 and a receipt schema with index/member identity slots from day one; V1 implements hashing; V2 ships native/CLI/MCP/Workbench single-index receipts. | V0 design alongside E0; V1 after V0/W0; V2 after V1/T1; no X dependency | Match/mismatch/race/budget outcomes preserve existing index scopes. X5 later extends member associations additively. |
| V3 — explicit refresh | Refresh/requery through existing reviewed native service operations and qualify recovery. | V2; X5 only for the optional multi-index refresh path | No hidden mutation, unsafe replay or loop; readiness and publication transitions explicit. |
| S1 — ranking | Diagnose lexical candidate cutoffs; select and implement an authenticated local ranking design. Preserve AND semantics and metadata filters before limits. | E0; T3 for final matched-profile evaluation | Better held-out relevance within predeclared cost limits, stable ties and no guard regression. Delivers the clearest search-quality experiment. |
| S2 — optional diversity | Evaluate explicit file diversity independently of ranker changes. | E0 and S1 comparison baseline | Selected subset/truncation survive exports; concentrated results remain inspectable. |
| G1 — guidance | Consolidate missing-evidence reasons and inert next-read suggestions, identifier routing, paging and relationship caveats across interfaces. | T2a vocabulary only; no S1/S2/X/T3 dependency | Existing empty/unavailable/excluded/stale/truncated cases get correct next actions without ranking changes or unadvertised new inputs. |
| Q0 — release gate | Review combined behavior, run task/resource campaigns and document supported scope/migrations. | All selected release packages; X/V3 only when included | Complete task outcomes, parity, failure recovery and platform limits are recorded for the declared full or fallback scope before 1.9.0 claims. |

The dependency rule is **V2 ships single-index receipts first; X5 adds per-member
citation associations later**. Neither V1 nor V2 depends on X. X4 depends on X3
and T1; X5 depends on X4 and V2. T2a/G1 also remain independent of X. X1/X2 can
proceed alongside T1 after W0/X0. E0 runs throughout. Keep S1
and diversity as separate experiments so an improvement or regression is
attributable. Do not bundle a new embedding model or semantic-unit change into
the ranking comparison. Reuse current tooling where suitable, including
`scripts/check-retrieval-quality.py`, before adding another evaluation runner.
That script currently supplies original static vectors and nine fixed synthetic
judgments: extend its transport/receipt harness, but build a separate frozen
representative corpus with the actual pinned model for E0. Freeze graded relevance
labels and nDCG@10/Recall@10 computation, then set numeric improvement, recall and
resource thresholds before candidate runs; keep held-out queries out of tuning.

### Contract decisions to settle before implementation

- Define reply-detail compatibility and conflicting-argument errors once in the
  canonical contract. Rich must not imply full chunks, more hits or larger budgets.
- Preserve existing default bytes/fields when no new reply argument is supplied,
  apart from documented observation identity/timing. Recommend explicit Lean for
  new agent/CLI configurations; do not silently change existing Agent Lean users.
- Reserve **Rich** for the new versioned profile. Keep **Rich Structure (legacy
  balanced)** identifiable in compatibility UI/help, retaining `balanced` semantics;
  it is not an alias for the new Rich profile. Detailed remains a distinct legacy
  policy. T2b/T3 update labels/catalogs and migration review together, including
  [the earlier efficiency plan](../mcp-prioritization.md).
- Define family availability before profile migration. Retain `essential`,
  `agent_lean`, `balanced` and `detailed` with their original semantics until an
  explicit migration; do not silently equate Balanced and Detailed continuity.
- Registration review must distinguish exact-root reuse from new overlapping
  registration, and revalidate the observation before admission. Marker scan
  exhaustion is unknown classification, not evidence that a folder is safe.
- Project definitions, many-to-many index membership, source assignments,
  window grants and one-or-more Project query selection are
  separate contracts. Adopt the supersession map in the cross-project feature
  document; do not implement the original W1–W3 automatic setup alongside it.
- Separate new Project IDs from legacy native `project` index bindings. Schema
  migration, saved-reader/IPC validators and compatibility adapters must ship
  together; do not rename SQL ownership rows or weaken unknown-version rejection.
- Keep connection scope distinct from a request filter. Apply the enforced
  intersection centrally, including explicit paths, locators and follow-up reads;
  prevent UI selection or relationship expansion from bypassing it.
- Keep ownership per [architecture](../architecture.md): Core contracts, Ingest
  adapter policy, Store objects/registry/publication, Kernel selection/budgets,
  CLI process/transport adapters and extension review/presentation. Use existing
  jobs, receipts, leases and quotas; introduce no parallel index authority.

### Shared policy names and decision records

Adopt one enum style across the two independent controls: proposed
`verification: off | report | require` (default `off`) and
`member_availability: report | require` (default `report` for explicitly selected
combined reads). `verification=require` demands checks for all returned sources;
`member_availability=require` demands outcomes from all selected members. Neither
implies complete corpus coverage. `off` applies only to verification. Retire this
design's earlier `failure_policy`/`require_all` spellings before implementation;
they are not shipped compatibility arguments. X0/V0 publish the pair together in
schemas, help, errors and acceptance fixtures.

X0 must produce **ADR 002: Projects, indexes and many-to-many query scope**;
V0 must produce **ADR 003: dated saved-source verification** under
`docs/decisions/`, following the existing ADR format. Include alternatives,
invariants, migration/compatibility and consequences. These are required future
decision records, not approvals invented by this planning edit.

## Scope control and deferred value

The full target is W0, E0, X0–X6, M0–M2, T1, T2a/T2b, T3, G1, V0–V3 and Q0.
S1/S2 remain conditional on measured gains. Preserve the recommended sequence;
set the release cut after X0/X1 estimates and upgrade evidence rather than treating
every research idea as an unconditional release blocker.

**Minimum fallback 1.9.0:** W0, E0, T1, T2a's existing-observation vocabulary,
G1, V0–V2 and Q0 for those packages. V2's member/index identity fields are present
from the start, with one member, so X5 stays additive. Retain existing index
policies; defer X0–X6 and M0–M2 delivery to 1.10 if necessary, with optional design/ADR work
remaining inert. Also defer T2b/T3, V3 and S1/S2 unless independently qualified.
The fallback has **no Project registry schema migration, new index hierarchy or
cross-project queries**. Do not apply U0–U3 just to reserve future tables, and do
not advertise a picker as cross-project support. Any unrelated schema change
would need its own explicit justification; none is planned for this fallback.

G1 does not wait for richer indexing or ranking: unavailable detail is described
using existing observations and the T2a vocabulary. Review the chosen scope and
its release notes before distributing a candidate. Deferral is a declared cut,
not evidence that the omitted feature was completed.

V2 uses existing native binding/collection/incarnation identity for its single
member; optional future Project associations remain absent, rather than requiring
new persistent IDs or the X registry schema. This keeps its receipt schema ready
for X5 while preserving the fallback's no-registry-migration boundary.

### Folder/workspace scenarios mapped to delivery

Numbers refer to the original 20-case table in
[folders/workspaces research](vs-code-folders-workspaces.md). X0 records current
extension behavior before X2 changes it, including cases 3–7, 12 and 18. The
incorrect per-root MCP-server guidance and current limitations are corrected in
the extension guide now; fixture implementation remains X0 work.

| Case | Situation | Owning package / disposition |
| --- | --- | --- |
| 1 | Open a repository root | X2 explicit Project/index setup and reuse; preserve existing saved policy |
| 2 | Save a single-folder workspace | X2 context association/import; saved-search/workspaceState continuity migration deferred |
| 3 | Add sibling folder / untitled multi-root | X0 baseline; X2 explicit setup; X3 stable permitted scope |
| 4 | Open a repository container | X1 new native observations; X2 reviewed source assignments, no automatic broad scan |
| 5 | Container already indexed, open child | X1 overlap observation; X2 offers reuse or reviewed separate index |
| 6 | Open a repository subfolder | X2 reuse choice; X3 enforces prefix scope |
| 7 | Parent and child open together | X1/X2 overlap review and X3 scoped grants; no duplicate work by default |
| 8 | Junction/symlink/subst/mapped/UNC aliases | X1 canonical identity fixtures; X6 installed platform qualification; uncertain cases remain unknown |
| 9 | Clones and worktrees | X2 intentional separate indexes or reused existing memberships; shared embedding ownership deferred |
| 10 | Submodules / nested repositories | Existing discovery/ignore behavior unchanged; X0/X6 record dedicated boundary tests; no new repository inference |
| 11 | Remove folder / close window | X2 distinguishes closing Project, membership removal and index removal; no automatic pause/delete; inactivity signal deferred |
| 12 | Reorder/rename workspace folders | X0 baseline; X2 display names; X3 canonical connection assembly |
| 13 | Move/rename repository on disk | X2 missing-root explanation; automatic relinking remains deferred |
| 14 | WSL/SSH/container windows | X2 storage-location explanation; X3 separate host grants; X6 remote qualification |
| 15 | Workspace trust changes | X2 review cancellation and X3 admission fences; preserve whole-window trust behavior |
| 16 | files.exclude/search.exclude | G1/current docs explain index ignore policy; preview-to-airkignore conversion deferred |
| 17 | Window settings inside folder settings | G1/current docs explain multi-root limits; changing setting scope deferred |
| 18 | One manual MCP definition suppresses provider | X0 baseline; X3 per-member conflict handling and no unrelated suppression |
| 19 | Agent launched beside saved workspace file | X3 explicit previewed multi-Project config at chosen location; X2 removes ambiguous `folders[0]` library fallback and requires target selection |
| 20 | Another VS Code build/profile | X1 upgrade/version fences; X3 grants and opt-out preservation; X6 mixed-extension/runtime tests; never regrant by profile change |

### Reconcile the 1 November MCP implementation backlog

The [earlier implementation plan](../mcp-recommendations.md#implementation-plan)
retains its dated observations. Its old four-request discussion is not current
capacity guidance: 1.8.7 exposes configurable 4/8/16/32/64 concurrency. Audit the
live catalog/callers before treating any proposed item as missing work.

| Earlier item | Disposition in this plan |
| --- | --- |
| Replay ledger, distinct count units and task costs | E0 extends the original Session case, preserving errors, query counts, matching-line versus occurrence distinctions and typed candidate labels |
| Identifier-aware ranking / definition hints | S1 evaluates existing hints first, then one lexical/identifier experiment against E0; no duplicate boost project or silent query rewriting |
| language/evidence-kind/exclude-prefix filters, suffix/contains names, reference paging/summaries/count precision | Already represented in the 1.8.7 catalog; W0/T1 retain actual-handler/transport regressions and G1 documents correct routing. New defects require reproductions, not reimplementation |
| Struct-to-method/caller navigation | G1 documents current supported targets/limits; broader type associations deferred pending source-bound ownership evidence and false-association fixtures |
| Brief admission queueing / setup waits | Preserve current finite queues and typed retry contracts; E0 records installed-path contention, X3/X4 audit admission under fan-out. Additional queue redesign deferred unless a current reproduced failure justifies a focused fix |
| Terse receipts and connection-only encoding | T1 preserves independently useful identity/exports; connection-only publication/source references remain deferred |
| Diversity and actionable missing evidence | S2 owns optional selection; G1 owns guidance and cannot be held by S1/S2 |

### Upgrade execution and version-line policy

For the full X delivery, extend the **managed native runtime activation path**:
the selected 1.9 CLI installer coordinates U0 preflight and U1 predecessor drain;
the new native service's registry owner performs U2/U3 backup/migration through
Store under the recorded upgrade intent and exclusive migration admission before
normal registry writes/readiness are enabled. CLI owns process effects; Store owns
SQL/backup validation. This is required implementation work, not a claim that
today's activation already has those stages. MCP reads and editor webviews never
initiate migration. Explicit native setup owns fresh-registry initialization.

Reuse and extend existing managed-install ownership/drain: Windows/Linux have
verified predecessor termination, while macOS has graceful shutdown and requires
its own qualification. Unknown/undrained owners block migration. An activation
timeout must inspect the saved upgrade receipt rather than restart U3 blindly.

The 1.9.0 extension must add **1.9** to supported forward runtime lines and stage
upgrades from supported older installs according to [versioning](../versioning.md).
After the Project registry migration, 1.9→1.8 is a blocked ordinary downgrade;
a matching older extension alone cannot make the newer database readable. Only
the reviewed U-stage recovery boundary permits restoring a compatible backup
before new writes; after new writes, prefer forward repair and require explicit
loss/receipt review for any restore. No automatic downgrade or schema spoofing.

A second VS Code build/profile still on 1.8 must be refused incompatible access
at the native version/schema boundary and cannot reactivate an old writer or
silently create another registry. Return an **upgrade required** compatibility
reason; supporting UI withdraws incompatible actions and explains it. An already
shipped older extension may display a generic compatibility error instead, so
safety must not depend on retrofitting its UI. Test actual old-extension startup,
external agent reconnect and pending jobs together in X6.
For the fallback without X, keep the existing registry schema and ordinary
supported-line rules; do not claim the Project migration downgrade restriction
has been triggered merely by changing the application version.

### Documentation impact by package

Every implementation commit/push/PR cycle must check README, documentation index,
the current release record and extension/Marketplace descriptions against the
implemented slice. The table adds concrete targets; it does not advertise future
features today. Create `docs/releases/release-1.9.0.md` as a candidate record when
implementation begins, with observed validation and chosen release cut. Marketplace
text includes `packages/vscode/README.md` and the package manifest's `description`
and setup copy, synchronized with native help.

| Package | Required affected documentation |
| --- | --- |
| W0 | Two-tier research follow-up; read-interface-parity, unified-mcp, mcp, CLI/MCP developer guides and release record; distinguish captured invariants from executed regressions |
| E0 | validation and relevance/performance evidence; check-retrieval-quality harness instructions; MCP investigation reconciliation |
| X0 | ADR 002, vscode-extension current behavior/limitations, scenario map, architecture boundaries and cross-project design |
| X1 | collections, source-roots, storage-formats, service-project-setup, service-freshness, versioning, U-stage release/upgrade notes |
| X2 | vscode-extension, Explorer help and source-assignment/Project UX; extension README/Marketplace setup text |
| X3 | unified-mcp, mcp, cli-reference, agent-integration, source-roots, versioning and grants/revocation help |
| X4–X5 | unified-mcp, mcp, cli-reference, read-interface-parity, Workbench forms/session/export help and both developer guides |
| M0–M2 | collections, source-roots, extraction-policy, storage-formats, service-project-setup, index-coverage, read-interface-parity, Indexing/CLI help and shared-Project split/trim/cleanup/recovery documentation |
| X6 | versioning, storage-formats, upgrade recovery/qualification, release record, README/index and extension/Marketplace limits |
| T1 | mcp, unified-mcp, cli-reference, read-interface-parity, Workbench defaults and MCP response-mode compatibility |
| T2a / T2b / T3 | extraction-policy, mcp-prioritization, document/code fact guides, profile capability/migration help; legacy Rich Structure distinction |
| G1 | index-coverage, service-freshness, CLI/MCP developer guides, tool descriptions and Explorer/Workbench next-action help |
| V0–V2 | ADR 003, index-coverage, service-freshness, mcp, unified-mcp, cli-reference, read-interface-parity, source verification and export/help wording |
| V3 | service-project-setup, index-coverage, native reviewed refresh/receipt and recovery docs; per-member refresh only when X5 lands |
| S1 / S2 | retrieval/ranking evidence, lexical storage/format and rebuild guidance if needed, read-interface-parity and selection/truncation help |
| Q0 | All current behavior/version surfaces above, dated release evidence, unresolved qualification and explicitly deferred features |

Names without paths refer to the corresponding files under `docs/`; ADR and new
release-record paths are future deliverables. Update `docs/architecture.md` when
component responsibilities change, preserving dependency direction.

### Risk register

| Risk | Owner / gate | Mitigation and release cut |
| --- | --- | --- |
| Registry upgrade damages installed metadata or replays pending work | X1/X6, U0–U6 | Protected consistent backup, owner drain, atomic backfill, receipt recovery and fault tests; defer X rather than ship an unqualified migration |
| Old extension/build/profile reopens shared state | X3/X6, scenario 20 | Explicit version/schema fences, upgrade-required UX and no automatic old-runtime activation or grant expansion |
| Verification dominates latency on large PDFs/remote files | V0–V2, E0 | Full-source hashing with measured byte/time admission, opt-in mode, explicit unchecked outcomes; no metadata-only substitute or blanket speed claim |
| Lexical ranking needs FTS layout rebuild | S1 design gate | `detail=none`/`columnsize=0` may not provide chosen scoring inputs. Audit authenticated FTS schema and every publication/update path; any layout change needs a separately versioned collection lexical/index-v1 compatibility and rebuild/migration plan, quota/recovery/rollback tests and per-collection cost disclosure. It is not the X registry migration; keep current ranking if not qualified |
| Split or trim leaves leaked evidence or incomplete ownership transfer | M0–M2/X6 | Durable deny fences, independent destination ownership, peak quota reservation, shared-Project review and phase-by-phase fault tests; no success while partition, grant or cleanup state is unknown |
| Expanded scope prevents a coherent release | Q0 after X0/X1 | Use the explicit fallback above; no hidden partial hierarchy or migration-only release |

| Deferred element | Potential value | Why it needs a later decision |
| --- | --- | --- |
| Registered-descendant pruning in combined projects | Less duplicate work in intentionally combined layouts | Registration/removal would change parent coverage, ignore behavior and watcher scope; needs reconciliation design |
| Root relinking after moves | Reuse retained work and explain missing sources | Changes source binding and publication identity; metadata resemblance is insufficient authorization |
| Cross-collection embedding reuse | Reduce repeated inference for clones/worktrees | Shared ownership, privacy, quotas and deletion must be designed first |
| Lazy rich enrichment / separate rich objects | Avoid rich cost on rarely investigated files | Adds captured-source, receipt, integrity and reclamation complexity without current cost evidence |
| Semantic-unit tuning and local reranking | Improve difficult relevance cases | Requires held-out gains that justify latency, memory, model and indexing cost |
| Unsaved editor evidence | Answer from active edits | Needs distinct opt-in source/version identity; cannot be mixed into saved-source publications |
| Per-project saved searches, inactivity observations and settings ergonomics | Better continuity and cleanup guidance | Useful follow-up after setup correctness; avoid automatic pausing/removal or content retention |
| Wider format/profile defaults | Consistent Lean/Rich experience for PDF, spreadsheets, slides and composite hosts | Current live battery did not qualify these families; preserve adapter caveats and coordinates |

## Fourth research area: verifying returned references against saved sources

**Question:** can AIRK query the index, perform a fast index pass on every
referenced file, and certify the result so the unverified warning can be removed?

**Assessment:** AIRK can provide a substantially stronger, testable guarantee:
**the saved source bytes checked during this response matched the indexed source
identity used for these references**. Recommend implementing this for 1.9.0.
The normal successful path should validate bytes without updating the index.
Running an indexing job for every returned file adds publication and model work
that unchanged bytes do not require, and a changed file cannot retroactively
validate evidence produced from its earlier contents.

This is a source-agreement guarantee, with a time and scope. It does not certify
that a search found every relevant file, that a syntax relationship is resolved,
that a requirement is true, or that extracted PDF/OCR/table content is accurate.
Re-extracting with the same parser is not independent evidence of parser accuracy.
The UI should use **Saved source verified**, rather than an unqualified
**Certified reliable** label.

### Existing implementation and the gap

The following paths were inspected in the live worktree for this addition;
associated tests were read, not executed as a new runtime qualification:

- [Conditional reads](../../crates/airk-kernel/src/current/conditional.rs) check
  requested revision and known hash against retained indexed identity and bind
  representation validators. They do not compare the current on-disk file.
- [Coverage](../../crates/airk-kernel/src/coverage.rs) and its
  [documented contract](../index-coverage.md) already compare full source hashes,
  account for extraction/version/root binding, and reject invalid comparisons.
  A workspace inventory is broader and more expensive than checking just the
  returned references. Its cached measurement is not a fresh per-result receipt.
- [Path-scoped indexing](../../crates/airk-kernel/src/current/indexing.rs) supports
  `index_paths`. Its [tests](../../crates/airk-kernel/src/current/indexing/tests.rs)
  cover requested-path updates, alias roots, removal and untouched neighbors.
  Configuration/directory/ignore hints can cause full reconciliation; configured
  model preparation and publication also occur in this path. It is not a
  guaranteed cheap hash-only operation and is not a public verified-query contract.
- [Explorer source checks](../../packages/vscode/src/explorerSource.ts) hash saved
  bytes, while [Inspector/Checker](../../packages/vscode/src/explorerInspect.ts)
  expose matching, changed, unsaved and unchecked states. The Checker caps distinct
  paths at 25. These extension helpers are useful precedent, but need native
  streaming, race-aware capture and canonical transport parity before becoming
  the proposed shared guarantee.
- [Service freshness](../../crates/airk-store/src/service_freshness.rs) deliberately
  emits `live_source_verified: false`. A successful indexing attempt is not a
  live-source verification receipt. Preserve that collection-wide meaning;
  verifying a result subset must not set a global all-files boolean to true.

### What verification would and would not establish

| Dimension | Evidence required | Claim allowed after this feature |
| --- | --- | --- |
| Retained evidence integrity | Existing authenticated index/payload/location checks | Response is bound to its indexed source and publication; verification does not relax these checks |
| Saved-source agreement | Full raw-source digest equals the hash attached to the exact result, with bounded capture/race checks | This referenced saved source matched during the stated observation interval |
| Extraction provenance | Effective extractor/profile/provider identities and existing quality/omission data | Interpretation and producer limits remain visible; byte equality does not prove extraction correctness |
| Search completeness/ranking | Whole eligible-corpus coverage and separate relevance evaluation | Returned-file verification alone makes no completeness or ranking guarantee, including for zero hits |
| Editor state | Separate host observation of unsaved edits | Disk agreement can coexist with unsaved edits; native CLI/MCP cannot claim to have checked every editor buffer |
| Continued freshness | A new check or a guarded edit operation at point of use | A receipt is a past observation; it cannot promise no later write or authorize an edit without current guards |

### Proposed query–verify workflow

1. Execute the normal scoped query and retain its exact publication, source
   hashes, locations, extraction identities, selection and budgets. Verification
   is an explicit per-query/session choice, independent of Lean/Rich detail.
   Existing snapshot reads remain available. Use the proposed shared contract
   `verification: off | report | require`, default `off`, as specified above.
2. Enumerate every distinct source referenced by the returned evidence, including
   supplemental definition pointers and relationship endpoints where present.
   Deduplicate by collection/root binding, logical source and expected digest,
   rather than hashing once per chunk. An unresolved endpoint or unsupported
   reference type must remain explicitly unverified; never silently omit it.
3. Resolve only through the current authorized native roots and enforced
   connection-prefix scope. Reuse source containment/eligibility rules and require
   source selection/configuration to remain valid. A stale indexed path is not
   permission to open a now-excluded file. Recheck registered alias/root identity;
   reject path traversal, symlink/reparse escape and redirected root bindings.
4. Open each allowed source read-only with a bounded, platform-qualified capture
   routine. Stream and hash the entire raw file with the indexed digest algorithm;
   size and mtime can reject obvious changes but cannot prove equality. Compare
   handle identity/size/change observations before and after reading and confirm
   the path still identifies that object. Detectable replacement or writes give
   `changed_during_check`, never a pass. Ordinary filesystems cannot provide a
   globally atomic multi-file snapshot, and metadata fences do not exclude every
   concurrent or adversarial writer; document this observation boundary honestly.
5. Match the captured digest to the source hash in the original response. For an
   unchanged file, reuse authenticated retained extraction: no parsing, embedding,
   writer lock or publication is needed. Bind a new verification receipt to that
   response's identity and evidence selection, rather than to whichever head is
   current later. Validate the current-only publication fence before completion;
   superseded observations require a bounded caller retry, not historical fallback.
6. Return per-source states and an aggregate accounting for all references. In
   `report` mode, keep indexed evidence and clearly mark mismatches/unknowns.
   In `require` mode, return a typed non-success result when any required source
   cannot be verified; do not present the remainder as a complete certified query.
   Preserve pending/partial/error details and the attempted publication. Zero
   references means `not_applicable`, not “all sources verified.”
7. If a mismatch is found, offer **Refresh referenced files and run again** as
   a separate explicit native management operation. Reuse existing service jobs,
   admission, receipts, quota and recovery. Check whether selected-path updates
   are actually safe; if a full scan is required, disclose it before admission.
   Do not turn an evidence read into a hidden index mutation or override pause,
   manual-update, provider or vector policies.
8. After an authorized refresh, run the original query again against one new
   publication and verify every reference in the new result, including newly
   selected files. Never attach new hashes to old snippets or combine revisions.
   Semantic readiness may still be partial: wait within an explicit budget or
   report the policy used; do not silently substitute lexical mode. Bound the
   refresh/requery cycle and report continuing churn instead of looping.

Checking returned files cannot detect a new matching file elsewhere or a stale
file excluded before top-k. Selected refresh therefore improves the reliability
of supplied evidence without establishing a current whole-workspace search. A
future whole-scope freshness mode would need its own inventory and cost contract.

### Receipt, warnings and cost contract

Use a versioned native receipt with stable index/member identity fields from the
first single-index V2 delivery, plus collection/root and enforced-scope identity,
publication and result-selection digest, expected/observed source hashes,
extractor/profile binding, per-file observation start/end times, method, state
and reason. Include requested/checked/matched/mismatched/unchecked counts, bytes
read and limits hit. Suggested states distinguish `matched`, `mismatch`, `missing`,
`unreadable`, `out_of_scope`, `changed_during_check`, `budget_exhausted`,
`publication_expired` and `unsupported`. Mark identifiers/fields as proposals
until the canonical schema and threat model have been reviewed.

Preserve receipts in Lean/Rich, Terse/Full, parse aggregates, cards, copies and
exports. `not_modified` of an indexed representation does not renew a source
verification timestamp; a verification request still needs a new source check.
Historical session exports retain their check times and must never render as
currently verified on reopening. Serialized receipts are observations, not
transferable authority or signatures from an independent certification body.

For references whose checks passed, replace the corresponding freshness warning
with **“Saved source verified at [time] against indexed revision [revision].”**
For mixed results use **“N of M referenced sources verified”** and expose reasons
beside affected evidence. Do not remove scope/coverage, extraction, unresolved
reference or relevance qualifications. The Workbench readiness panel's
collection-wide unverified statement needs wording that distinguishes it from
verified result subsets. Agent-facing descriptions must teach consumers to use
the receipt, while requiring a live recheck before later edits.

The fast path is proportional to the total bytes of distinct referenced files,
not snippet length. A small hit in a large PDF still requires hashing that PDF.
Apply explicit file-count, per-file/aggregate-byte, concurrency, memory and time
budgets under existing request admission; avoid unbounded nested fan-out. Check
deadlines between reads and document filesystem calls that cannot be forcibly
interrupted. Cancellation must stop additional admission and release handles.
Report oversized, unavailable remote or slow files as unchecked, never inferred
matches. Avoid freshness caches based only on metadata or watcher silence;
within-request deduplication is safe, while later requests need new reads for
the proposed fresh-check guarantee. Benchmark before selecting default limits
or enabling verification by default.

### Implementation packages and acceptance gates

| Package | Work and owning components | Acceptance evidence |
| --- | --- | --- |
| V0 — contract and baseline | Produce ADR 003. Core owns states, receipts with member-identity fields, selection binding and verification report/require semantics; jointly publish policy enum names with X0. Inventory capture logic and establish byte/latency baselines. | Source equality, integrity, extraction accuracy and completeness remain distinct. No X implementation dependency; set resource thresholds before defaults change. |
| V1 — native read-only verifier | Ingest owns safe source resolution/capture adapters; Kernel owns scoped result enumeration, deduplication and budgets; Store supplies authenticated expected identities and publication fences. CLI owns necessary host/transport integration. | Native fixtures cover every outcome, complete reference accounting, no writes/jobs and no privilege or source-scope expansion. Add no independent database, index or scheduler. |
| V2 — single-index parity and presentation | Ship native single-index receipts across MCP/aliases, CLI tools/shell/convenience reads, parse/recipes, Workbench and exports. Reuse the native Inspector/Checker path. Depends on V1/T1, never X4/X5; X5 later extends the existing member fields to multiple citations. | Actual transport/handler parity, honest dated receipts and compact binding associations. Only applicable freshness warnings are replaced; single-index verification can ship if X is deferred. |
| V3 — refresh and qualification | Reuse the explicit selected-path service operation where safe, with reviewed fallback to broader work; reconnect query results to their new publication and verify again. Measure code, manifests, large documents, remote sources and mixed outcomes. | Correct receipts across cancellation, pause/manual mode, quota failure, service/worker replacement, publication churn and partial vectors; no hidden refresh or infinite retry. Publish observed latency/byte costs and platform qualification limits. |

Regression cases must include same-size/same-mtime content edits, deletion,
rename/replacement during hashing, symlink/alias changes, root redirection,
permission failures, scope narrowing, changed ignores/configuration, duplicate
chunks, cross-file edges, zero hits, budget exhaustion and unsaved editor text.
Exercise concurrent publication and changed files during refresh/requery. For
PDF/DOCX/PPTX/table/OCR evidence, verify raw source identity while retaining
native extracted coordinates and producer caveats. Imported assertions need
their existing dependency/build validity checks; a matching source file cannot
renew stale external analysis. Require negative tests proving that no mismatch,
unchecked reference or stale receipt receives the verified label.

**Recommendation:** deliver bounded, opt-in read-only verification in 1.9.0 and
make explicit refresh/requery the recovery path. This adds useful certainty
without paying reindexing cost on every unchanged hit. Consider a default-on
policy only after the measured latency and verification coverage meet agreed
thresholds; retain a strict option for consumers that require every returned
reference to be checked. Defer universal certification, automatic read-triggered
indexing and whole-workspace freshness claims.

## Validation and release decisions

Set numerical quality and resource thresholds before experiments, after recording
the baseline. Require no lost mandatory evidence, scope/identity violation or
hidden truncation. Compare indexing profiles independently from reply projections:
cross Lean/Rich indexes with Lean/Rich replies on identical source/model inputs.
Measure cold/no-op/edit indexing, CPU, peak memory, retained/physical bytes, read
latency and expansion calls; reply bytes alone are insufficient. For completed
agent tasks, include patch correctness and missing-evidence recovery, keeping
model, prompt, revision and permissions fixed and recording unavailable results.

Exercise canonical native handlers and actual CLI/MCP transports, including
convenience commands, shell, aliases and parse/recipes. Audit Workbench forms,
structured details, session restoration and JSON/Markdown copies/exports. Compare
full equivalent results and input catalogs, excluding only documented transport
identities/timing. Renderer fixtures supplement this evidence; they cannot replace
it. Preserve revision rejection, source read-only behavior, original coordinates,
filters before candidate limits, quota failures and typed retries.

Workspace acceptance covers single folders, saved and untitled multi-root windows,
parent/child roots, containers, manual MCP entries, remote windows and path aliases.
Extend it with the cross-project feature matrix: one index shared by two Projects,
queries spanning several Projects, differing membership prefix scopes and correct
deduplication/attribution; also one index shared by two windows,
unopened sources, explicit combined roots versus federated queries, grant revocation,
identical paths across members, partial vectors, failed members, vector/cursor
expiry, verification and total-budget limits. Preserve single-index compatibility.
The SQLite U0–U6 campaign must cover protected backup, old-schema fixtures,
transaction interruption, duplicate context imports, pending jobs/receipts,
old-runtime refusal and safe rollback boundaries before this feature ships.
Include split/trim policies and resumable receipts in that campaign. Test every
mutation boundary and permanent-deny enforcement across reindex, repair, alias
changes, all evidence transports and shared-Project queries; verify sources are
unchanged and distinguish logical removal from physical reclamation.
Keep installed Windows, macOS and Linux qualification separate from unit fixtures;
unrun path/profile/editor scenarios remain unknown. Run the checks in
[CONTRIBUTING.md](../../CONTRIBUTING.md), keep routine CI lean and use broad
qualification only when explicitly requested. Review the complete diff and
validation evidence before corrective pushes; record whether review was independent.

Before feature/fix PR creation, remind the maintainer to increment the patch/build
number unless already requested or completed for that PR. The eventual 1.9.0
version change must synchronize native/extension versions and lockfiles; this
planning document does not perform that bump. Update README, documentation index,
release notes and VSIX/Marketplace descriptions together when behavior lands.
Keep these research notes dated and identifiable as proposals, and claim release
completion only after authorized merge/publication and verification succeed.

## Evidence boundary for this plan

This assessment reads the three original local research files, the new
cross-project feature design, current project guidance and the cited native/
extension verification and multi-project routing paths;
it does not rerun the earlier live experiments, audit the Copilot implementation anew or
establish new performance/platform results. Their source-review and measured
findings are attributed above, while priorities, package boundaries and scope
decisions are recommendations made here. No runtime, indexing policy, source
registration or version is changed by creating this file.
