# AIRK 1.9.0 research assessment and delivery plan

**Assessment date:** 2 November 2026. **Baseline:** AIRK 1.8.7.
**Status:** recommended scope and implementation gates, not implemented behavior or
a release announcement. **Implementation authorized:** 2 November 2026; follow the
[implementation ledger](implementation-v190.md) for actual progress, tests and
commits. The implementation
target is **1.9.0 on the existing `feat/finetuning` branch**, with tested, reviewed
commits for each accepted phase. This plan synthesizes **five primary research inputs**:
the three original notes, the fourth research area within this document, and
the separate cross-project feature design. Dated observations retain their limits.

**Working-version decision, 3 November 2026:** complete and manually test this
plan on **1.8.8** on the same branch. Promote to **1.9.0** after implementation and
manual acceptance, rebuilding and validating artifacts under the promoted version
before the CI/Marketplace handoff. Keep phase names, schema/grant/policy versions
and earlier tested payload identities independent from this numeric working
version. The original P0b 1.9.0 checkpoint remains dated history; this decision
does not authorize CI dispatch or publication.

P3 implementation decisions are captured in [ADR 002](../decisions/002-projects-indexes-and-query-scope.md), [ADR 003](../decisions/003-dated-saved-source-verification.md) and the executable [design contract pack](contracts-v190/README.md). These settle selector names, grants, persistent policy ownership, registry-12 upgrade stages, bounded receipt semantics and profile-family inputs. They enable no new runtime behavior. Enabled verification uses explicit resolved budgets; P4 local measurements and qualification limits are recorded in the ledger.

**P4 maintainer decision:** Report is the default for evidence reads and completed
response-level plans. This is a user-requested change from the earlier Off design;
context, retained profiles and reply detail do not choose the mode. Off explicitly
skips source checking; Require remains the strict option. The initial bounded
preset is eight distinct sources, 16 MiB of aggregate raw bytes and a cooperative
2,000-ms checking deadline, with independent caller overrides and explicit unchecked
outcomes. P4 measured this preset and passed local qualification as recorded in
the ledger. Metadata/help reads do
not acquire checks. Plans check final returned references once, deduplicated across
steps; explicit per-step choices use a declared steps scope. Preserve the explicit
Off path against frozen/preceding performance gates and separately record default
Report's intentional I/O, latency and reply-byte cost; do not describe that cost
as unchanged default behavior or silently fall back to Off.

Review in this order: [phase schedule](#implementation-path-on-the-current-branch),
[performance gates](#performance-protection-throughout-implementation),
[Indexing recommendations](#indexing-recommendations-and-agent-query-scope),
[decision map](#decision-map), then the detailed research and package contracts.

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

The [3 November audit](../issue-analysis/audit-report.md) and
[open-issue analysis](../issue-analysis/open-issue-analysis.md) add dated
operational evidence. Their assessment and implementation boundaries are woven
into [the remaining-phase remediation sequence](#operational-research-and-remaining-phase-sequence).

The [Shark Tank remediation draft](../reviews/shark-tank-todos.md), also dated
3 November and based on 1.8.7, supplies additional qualification proposals.
The [selective integration below](#selective-shark-tank-remediation) adopts useful
items within the remaining phases and records exclusions; its separate milestones
and blanket requirements do not replace this plan or current repository policy.

## Recommended direction

Make 1.9.0 a release about **intentional indexing and dependable, useful evidence**.
Protect the existing startup, retrieval and UI improvements before adding work to
their paths. Repair the reported compact-plan failure first, then deliver complete
read contracts, intentional Project/index management and useful scope guidance.
Keep retained-profile changes and relevance experiments separately measurable.

Recommended order for addressing the research as implementation work:

1. Preserve the frozen performance/quality baseline and compact-plan repairs.
   Keep native/extension identities synchronized at working version 1.8.8 during
   implementation and manual testing; promote to 1.9.0 after manual acceptance.
2. Separate reply detail from retention, establish missing-evidence vocabulary,
   and implement bounded single-index verification with Report as the default with complete interface parity.
   Settle the Project/index and split/trim contracts before freezing schemas.
3. Deliver safe metadata upgrade, the **Indexing** screen and enforced window
   grants, then expose combined queries only when their consumers are complete.
4. Add **Recommendations** for useful index subsets, combinations and reuse,
   with copyable agent-harness scope instructions. Deliver permanent trimming
   before splitting; connect recommendations to those reviewed operations.
5. Introduce versioned file-aware profiles and explicit refresh/requery. Evaluate
   ranking and diversity independently; include them only if measured gains
   justify their cost. Finish combined upgrade, recovery and release qualification.

The phase schedule below is the execution order. Existing W/E/X/M/T/V/S/G package
IDs remain traceable design workstreams, not separate parallel implementation
plans. New R packages own Indexing recommendations. This revision takes precedence
over the older delivery ordering in [cross-project indexing](cross-project-indexing.md);
reconcile that design and the ADRs during implementation before activating changes.

## Implementation path on the current branch

Complete one phase at a time. A phase begins from the preceding accepted commit,
keeps unrelated work intact, and ends with behavior, tests, documentation and
review evidence in a human DCO-signed commit. A substantial phase may have small
buildable internal commits, but its public capability must remain unavailable
until its complete native/CLI/MCP/Workbench contract passes. Do not count an
internal scaffold as a delivered phase or distribute intermediate gated builds.

### Ordered phases and acceptance checkpoints

The tests in this table are the distinctive acceptance cases, in addition to the
shared checks and performance gates below. This table defines planned acceptance;
the linked ledger records completion. A row alone asserts no passing tests.

| Phase / packages | Concrete outcome and commit boundary | Required evidence before acceptance |
| --- | --- | --- |
| P0a — E0 baseline | Record the clean starting commit, native/extension versions, build/model/configuration identities and current failures. Extend original fixtures and measurement tooling before changing behavior. | Reproduce the compact-plan issue; freeze representative relevance judgments and matched installed/native/UI performance cohorts, budgets and noise tolerance. Record unavailable cohorts instead of inventing a baseline. Commit fixtures and sanitized evidence. |
| P0b — candidate identity | The original accepted checkpoint synchronized 1.9.0 and added/tested the supported 1.9 forward runtime line. The 3 November maintainer decision temporarily sets the working version to 1.8.8. Keep manifests, local locks and generated catalogs synchronized; retain the original release/checkpoint identities. Neither numeric change enables a storage migration. | Check `release-version.py check --expected 1.8.8` while implementing/testing, then 1.9.0 after promotion; compile/check catalogs and installer compatibility. Preserve P0a's exact 1.8.7 controls and all dated 1.9.0 receipts. |
| P1 — W0 correctness | Repair compact shared bindings/locations and native nested-plan schema agreement. | Actual native/CLI/MCP compact and rich dependent plans, negative guard cases and bounded outputs; convert the captured invariants to meaningful regressions. Confirm ordinary reads do not gain extra queries or source reads. |
| P2a — T1 replies | Expose independent reply projection everywhere, preserving absent-argument compatibility and existing Workbench defaults. | Actual transport and consumer parity, conflicting-argument errors, copies/exports, equivalent useful evidence and unchanged default work/bytes apart from documented identity/timing. |
| P2b — T2a/G1 guidance | Adopt shared existing-observation reasons and bounded next-read guidance without changing storage profiles. | Unsupported, not-retained, pending, failed, stale, excluded and truncated fixtures; no full coverage scan or repeated diagnostics to explain an empty result. |
| P3 — X0/M0/V0 and T2b design gates | Complete ADR 002/003, source/workspace fixtures, profile-family inventory, permanent-policy/partition ownership, scope enums and receipt schemas. Reconcile the feature design, registry-versus-collection versions and recommendation inputs. T2b reporting implementation remains P11a. | Resolve every schema/authorization/performance decision needed by P4–P10; reviewed migration and fault matrices, bounded algorithms and budgets. No live migration or new profile selection. |
| P4 — V1/V2 verification | Deliver single-index source verification with Report as the default end to end, with future member identity slots. | All receipt states, source races, large-file budgets, unsaved-buffer limits and real transport/export parity. `off` adds no hashing; unchanged matches start no extraction, model, writer or indexing job. |
| P5 — X1 and U0–U6 safety foundation | Implement normalized catalogs, compatibility mappings, bounded overlap observations and the complete upgrade admission/backup/migration/import/readiness/recovery path in isolated fixtures. Design and guard collection policy formats separately. | Real predecessor/candidate activation and fault tests prove the safety-critical U0–U6 path, old-reader/writer refusal, idempotent imports, backup validity and no implicit rebuild. Keep production activation of the new schema disabled until P6 is ready; X6 later expands qualification, not basic safety. |
| P6 — operational remediation, then X2/X3 management and grants | Follow P6a–P6f below; deliver Indexing library/detail/source/Project flows, reviewed window associations and native grants/revocation. Activate the qualified upgrade and explicit-setup cutover together; retain complete legacy single-index retrieval. | Remediation receipts and local/actual-transport gates below, mapped workspace scenarios, direct CLI and actual agent scope tests, empty/untrusted/manual/remote cases, stale reviews and no automatic folder scans or regrant. Run unchanged P5 upgrade tests against the complete UI/connection path before enabling it; retain separately named installed/platform qualification limits. |
| P7 — X4/X5 combined retrieval | Expose multi-Project queries with optional index subsets, canonical prefix unions, unique-index execution and complete UI/parse/session/export/verification support in the same accepted phase. | Shared-index attribution and scoring, partial/failed members, cursor/publication/grant expiry, all input catalogs and actual transports. Compare one-index routing with the legacy path; prove unselected indexes are untouched and total budgets stay bounded. |
| P8 — R1 recommendations and harness guidance | Add bounded Recommendations, temporary scope selection and explicit saved defaults; copy live-capability-checked MCP instructions for one or several selected indexes. | All recommendation decision rows, stale/incomplete data, inaccessible members, narrow-to-broader user choices, no hidden queries/mutations and UI/CLI/MCP advisory parity. Smoke-test generated examples through actual transport fixtures. Splitting guidance explains pending capability until P10. |
| P9 — M1 permanent trim | Add reviewed permanent deny admission, all-read fences, removal publication and resumable reclamation with honest status. | Direct CLI and every read family reject blocked evidence, including graph endpoints/caches; restart/rebuild/alias/upgrade invariants, active-read races, quota failure and source-byte preservation. Empty-policy reads retain their fast path. |
| P10 — M2/R2 split and management advice | Add two independent partitions, staged destination build, reviewed Project cutover and resumable recovery; enable recommendation links to qualified split/trim previews. | Fail/cancel at every stage; inherit denies; no overlap/loss in effective partition rules, premature grants or shared-object ownership. Measure peak disk/CPU, query contention, cancellation and cleanup. No recommendation starts a job. |
| P11a — T2b effective-profile reporting | Implement the P3 inventory's bounded family/effective-profile read contract and complete native/CLI/MCP/Workbench parity before adding new profiles. | Distinguish unsupported, not retained and reply-omitted families on all legacy profiles; status/input/export parity and no extraction or corpus scan for reporting. |
| P11b — T3 retained profiles | Introduce file-aware Lean/Rich through explicit migration, one selected payload per file, independently of reply modes or ranking. | Cross profile/reply matrix; manifest/prose/table evidence, legacy policy continuity, no-op/edit indexing reuse, quota/fault recovery and measured storage/model costs. Leave unqualified adapter families on their documented behavior. |
| P12 — V3 refresh | Deliver explicit selected-reference refresh/requery through existing management admission, including selected member indexes where supported. | Full-scan fallback review, exact saved operation recovery, pause/manual/quota behavior, publication churn and bounded requery; never turn read verification into hidden indexing. |
| P13a / P13b — S1 / S2 conditional | Evaluate lexical ranking first, optional diversity in a separate commit/experiment. Retain current behavior if gates fail. | Frozen held-out nDCG@10/Recall@10, resource budgets, scope filtering before limits, stable ties, complete subsets/exports and any independently qualified lexical-format upgrade. No simultaneous model or semantic-unit change. |
| P14 — X6/Q0 final qualification | Review the combined diff, expand installed upgrade/platform/fault coverage and run the selected end-to-end resource/task campaigns. Finalize current docs and 1.9.0 release-candidate evidence. | No unexplained cumulative performance regression, scope leak, incomplete migration or missing parity; observed failures/skips and platform limits recorded. Release, push, PR, merge and publication remain separate authorized actions. |

P6 management can be implemented as inert library/review commits followed by the grant and
activation commit, but no intermediate build may migrate normal user state or
advertise the new hierarchy. Similarly, X4's implementation may precede X5
internally; **public combined reads activate only with X5 parity in P7**. The
single-index V2 receipt is complete at P4 and gains Project/member associations
in P7. These boundaries avoid a partially usable schema or API between phases.

The [selective Shark Tank work](#selective-shark-tank-remediation) adds targeted
qualification within these same boundaries: setup/ignore triage and risk review
with P6, scope/relevance/property cases with P7/P8, fault and partition cases with
P9–P11, and consolidated installed/resource/documentation evidence at P14.
Prepare any additional held-out labels before P13 experiments. Do not postpone
tests for an activating feature to P14 or turn optional broad campaigns into new
prerequisites for every phase.

### Operational research and remaining-phase sequence

**Assessment added 3 November 2026.** The audit reports installed **1.8.7 on
darwin-arm64**; the issue analysis diagnoses nine 1.8.7 reports against source
snapshot **`b72fbc8`**. They are valuable failure reproductions and hypotheses,
not qualification of the current 1.8.8 branch. Recheck each affected live path
before implementation and record whether the problem persists, changed or was
already resolved. Neither document changes accepted P0–P4 evidence or relaxes
P5's U0–U6 acceptance. A reproduced P5 safety failure blocks P5; otherwise these
are explicit remaining-phase work, with no P6 implementation in the P5 checkpoint.

The audit's strongest findings are the unusable copied signed macOS launcher and
different native signing provenance between Marketplace and GitHub. Its successful
secret/ignore, no-follow, source-preservation and document-location checks identify
behaviors to preserve, rather than proving a complete security or platform audit.
Its tool-count/help drift and unhelpful errors warrant small catalog/documentation
corrections. Include unknown-Project `QueryReturnedNoRows` in P6f's canonical
target-and-recovery errors, preserving scope/privacy and avoiding automatic
namespace creation or reset. Native watchers continuing after editor closure are an intentional
lifecycle that Indexing must explain, with explicit management controls. Observed
vector throughput and an Apple Silicon run do not establish model/performance
qualification. Fake-home state leakage warrants isolated installed-test roots,
not an unrequested change to production storage defaults.

The open-issue analysis usefully traces shared causes through native ownership,
helper availability, admission, transport and UI evidence. Several suggested
remedies need narrower contracts: restarting forever, querying status after every
turn, accepting a launcher symlink or draining unlimited input would introduce
new safety/performance risks. Use the following sequence instead. Each subphase
has a coherent source/test/docs commit boundary; substantial work may use inert
internal commits under the existing phase rules. Do not combine all nine issues
into one corrective commit or make repeated hosted pushes the debugging loop.

| Subphase / issues | Implementation and value | Minimum acceptance evidence and boundary |
| --- | --- | --- |
| P6a — installed launcher and release producer (#66, #67) | First choose and implement a standalone-safe signed launcher: prove an embedded Info.plist survives signing/copying, or use a separately signed small launcher. Preserve safe-file, alias, digest, ownership and payload-pair checks. Cover install, repair, managed activation, upgrade, shutdown and uninstall paths that execute copied binaries. Make both distribution channels consume one approved immutable signed native artifact set, preserving source/run/attempt/hash provenance. This removes an installation blocker and makes signing claims reproducible. | Native install/receipt/tamper and workflow-selection tests, isolated state roots, interrupted repair and rollback cases. On macOS, run the actual copied Developer ID signed/notarized executable, strict signature/OS checks, installed VS Code activation and agent connection; an in-bundle `--version` check is insufficient. Windows or ad-hoc fixtures cannot pass this signed-macOS gate. Unavailable credentials/platform remain explicitly unqualified and block release qualification; independent later source work may continue without claiming macOS readiness. No publication during this subphase. |
| P6b — helper outage diagnosis and recovery (#58) | Capture bounded process exit/signal, worker/grant renewal and recovery transitions before correcting the demonstrated cause. Replace lifetime exhaustion with bounded consecutive-outage recovery, measured healthy reset/cooldown and observable terminal/backoff states; preserve explicit Stop and manual/managed ownership. Repair stale worker membership without killing a healthy shared pool only where reproduced. This restores recoverability without a restart storm. | Fake-clock backoff/reset tests, repeated helper loss, grant replacement, transient versus terminal failures and explicit Stop across reconnect/restart. Exercise real native supervisor/worker paths. Preserve actual failure reasons; do not label every SIGKILL a signature failure or claim the original unexplained helper exits are fixed by changing a counter. No read-triggered helper spawning, model loading or new scheduler. |
| P6c — outage read contract (#58) | Put typed helper availability classification at the canonical owning boundary, preserving crate dependency direction. Permit Default search to return lexical evidence only when the saved DefaultSearchPolicy allows that degraded outcome; explicit vector/hybrid and RequireComplete retain their strict contract. Return complete served-mode, incomplete/availability, publication, coverage and retry information through every consumer. This keeps usable evidence available without overstating completeness. | Real native/CLI/MCP and Workbench/copy/export parity for saved policies, pending/ready/outage transitions, narrow typed transient errors and unrelated failures. Freeze field/error/retry schemas against current catalogs before implementation; do not blindly copy illustrative fields or retry delays from the research. Preserve grants, source-verification receipts, total budgets and original lexical evidence. No automatic replay of ambiguously accepted operations or per-read health probes; status may reuse bounded already-observed availability. |
| P6d — source admission fairness (#64) | Ensure an admissible due source update gets the next existing writer opportunity ahead of a vector continuation. Fix the owning admission predicate as well as loop ordering; use service-mode batch-boundary preemption and saved watermarks. Retain fairness across indexes, bounded vector turns and a single writer. This makes edits discoverable during long embedding backlogs. | Edit/add/delete/rename during a measured multi-turn backlog; freeze lexical-publication allowance at one existing turn plus bounded admission overhead. Test pause/manual/quota/backoff, blocked/no-op source work, repeated edits, cancellation and continued vector progress. Reuse existing freshness observations; no filesystem scan merely to report lag. Compare query latency/resource use under the same backlog and load, carrying these gates into P7 fan-out and P12 refresh. |
| P6e — bounded transport and truthful evidence (#63, #59) | Preserve the current stdio frame bound and distinguish oversized input from clean EOF. Prefer explicit stderr diagnostics and nonzero fail-closed termination unless the current transport can safely reject one bounded frame and continue with correct ordering. Fix Workbench's token-inequality-as-older comparison using authority/incarnation/generation/digest-aware observations; incomparable results get an honest different/unknown observation. This prevents silent disconnects and false stale warnings. | Actual process-pipe tests at/beyond the bound, chunked/no-newline/malformed input, bounded discard/cancellation and accepted-work/EOF behavior; diagnostics never contaminate protocol stdout. UI/controller tests cover older/newer/equal/inconsistent generations, missing/rebuilt identities and late responses after session/scope changes, backed by native publication receipts. Reuse returned publication/status or coalesced existing observation; no unconditional status request per turn, new source check or copy/export RPC. Preserve immutable turn evidence and Report verification semantics. |
| P6f — safe setup, trust and management activation (#61, #62, #65; X2/X3) | Reuse the reviewed prepare-registration/reserve/prepare-storage/register path for explicit new-index setup across supported native/CLI/editor entry points. Explain missing selected data-leaf recovery; do not recursively create arbitrary paths or mutate on reads. Distinguish Restricted Mode from disposed/revoked client authority, offer Manage Workspace Trust, and suppress unavailable native commands while retaining handler guards. Correct `files <PREFIX>` help from the canonical parser/catalog rather than adding an unnecessary flag. Then complete the original Indexing/grant/upgrade activation gate. | Actual CLI setup/replay, source preservation, occupied/unowned destinations, missing parents, aliases/races and platform permission boundaries. Test restricted/granted/revoked trust through installed command/controller paths, stale in-flight review and all new forms; never silently grant or start native work from informational help. Compare CLI/MCP input catalogs and actual help/filter behavior. Re-run unchanged P5 upgrade/owner/recovery tests through complete UI/connection paths before cutover. |

The maintainer's 3 November supply-chain addition belongs in P6a's producer
boundary: pin `cargo-deny` and `cargo-vet`, commit dependency/source/license
policies and audit imports, and integrate their checks with the existing local
and dependency CI workflow. Keep baseline exemptions visibly unreviewed; require
review for new/changed dependencies, including Git revision changes. Record tool,
database/import identities, actual findings and remaining audit gaps. Refresh
advisories/imports explicitly rather than treating a locked cached check as fresh
security evidence. Preserve native/model/npm provenance checks and explain the
reason for these additions in the release notes. These checks add no runtime work.

Before installed tests in P6a/P6f, inventory launcher, config, cache, logs, data,
managed-install and uninstall roots. Fixtures must isolate native paths as well as
extension fake-home paths and assert cleanup cannot escape owned destinations.
Keep test-only isolation separate from user-selected production defaults. Review
affected process/platform/unsafe boundaries and sibling callers when changing
them; a source-wide unsafe count alone establishes no safety result.

P6a starts first because package integrity must precede confidence in installed
tests; P6b supplies the failure observations/classification needed by P6c. P6d
must precede wider fan-out load. P6e establishes transport and publication semantics
before larger scope inputs. P6f finishes explicit setup and trust before any new
grant or live migration. Preserve this order for accepted commits; credentialed
platform proof may remain a clearly tracked release blocker while independent
source work proceeds. Required gates never turn into passes because they were
unavailable. Do not widen the phase into model replacement, a new scheduler,
unbounded recovery, weakened signatures or background all-library diagnostics.

Carry the foundations forward rather than treating these as isolated fixes:

| Remaining phase | Integrated regression/qualification obligation |
| --- | --- |
| P7 combined reads | Apply policy-qualified degradation per selected member; preserve typed partial/error/retry and verification data in all transports and exports. Compare per-index publication identities within authority; do not order a multi-index envelope as one scalar token or compare different selected index sets as older/newer. Test helper outage plus fresh source publication under fan-out without touching unselected members. |
| P8 recommendations/harness | Advise using observed source lag, semantic availability and selected/omitted members without new scans or health probes. Generate scope recipes and limits from current canonical capabilities, including bounded framing; eliminate manual tool-count drift. Explain per-user launcher/config portability, explicit lifecycle controls and strict versus degraded outcomes. Do not embed secrets or silently edit harness configuration. |
| P9/P10/P12 management | Keep trim/split/refresh admission under the same ownership, trust, pause and source-fairness rules. Surface source-pending versus semantic-pending honestly. Test outage/cancel/restart and grant changes without hiding incomplete cleanup or automatically replaying reviewed operations. |
| P14 and release handoff | Re-run the complete installed path using exact qualified artifacts on each declared platform, with signed macOS proof and both-channel provenance. Validate Marketplace native payloads/install manifests against the approved signed producer; document only authorized outer-package/signature transformations rather than assuming service-signed VSIX envelopes are byte-identical. At a separately authorized release, verify the actually published downloads and retain immutable producer identities. A missing signed-platform or channel proof blocks promotion/publication claims. |

Keep the original issue documents as dated reports. Future evidence should link
each issue to its reproduction, current-source diagnosis, corrective commit,
native/transport/installed tests and remaining platform limits. Start with local
fixtures and actual transports; use focused hosted diagnostics only for a concrete
unresolved platform/signing question, collecting the existing run before another
correction. No phase is complete solely because a renderer mock or workflow-schema
test passes. Continue the frozen baseline and preceding-phase performance gates;
measure outage/admission costs separately and retain healthy single-index fast paths.

### Selective Shark Tank remediation

**Planning assessment, 3 November 2026.** Use the review's stable `REQ-*` IDs for
traceability, without confusing its R/M/P area codes with this plan's package or
phase IDs. Its audit did not execute the proposed campaigns. Existing bootstrap
instructions, benchmark/quality/fault scripts, runtime-upgrade checks, installed
extension suites and P5 recovery tests already cover parts of the subject.
Verify each claimed gap against live source before implementing; extend the
owning harness rather than creating a competing runner. Record the search and
existing owner in the phase ledger. Preserve accepted P0–P5 evidence and the
P6a–P6f order; none of these proposals retroactively invalidates a completed phase.

Adopt the following work at the phase that can use and test it. Each row belongs
with that phase's behavior, documentation and validation commit; it is not a new
parallel programme or an instruction to start implementation during this edit.

| Review requirements | Placement and useful implementation | Acceptance and bounded scope |
| --- | --- | --- |
| REQ-G-1–G-6 — guardrails | Apply throughout P6–P14: reuse prior art, preserve dependency direction, source isolation, human DCO, parity, protected release boundaries and evidence provenance. Keep evaluation/model downloads explicit and outside product retrieval; send no query, source or activation data to an external service. | Existing task authorization and repository policy govern; the draft does not require repeated approval for already authorized work or prohibit existing installation downloads. Record UTC date, tested commit plus dirty/source digest where relevant, binary/model/corpus hashes, commands, hardware/OS, affinity, cache conditions and unavailable counters in existing receipts. Add only missing fields; do not rewrite historical receipts to imply new measurements. |
| REQ-A-1–A-3 — reproducible setup | During P6a/P6f, improve the contributor entry path and, where existing scripts leave a demonstrated gap, add a thin bootstrap wrapper around pinned provisioning/build/test commands. Provide prerequisite-only, provision/build and normal-check modes; make downloads explicit and support verified offline inputs. | Use current `CONTRIBUTING.md`, toolchain, Node, Python and nextest pins, not the draft's stale versions/counts. Missing prerequisites produce actionable errors rather than installing OS software, changing PATH globally or altering permissions. Report actual pass/skip/fail counts and elapsed time. Repeated setup reuses hash-verified inputs. Verify fresh-machine behavior only on exercised platforms; a wrapper cannot promise a green run or gate unrelated feature work until every VM is available. Retain 1.8.7 reproduction as optional historical evidence, separate from current 1.8.8 results. |
| REQ-B-1–B-2 — skipped-test triage | At P6 entry, inventory current Rust ignores and profile exclusions, distinguishing helper entry points, deliberately gated qualification and actual unresolved debt. Reuse this inventory for phase acceptance and P14 reporting. | Record commit, exact test IDs, reasons, required payload/platform and intended command. Include extension/Python skips when explaining overall qualification. Reconcile discoverable tests with the runner's skipped/excluded counts; `rg` attribute counts alone do not account for generated/conditional tests. Add reason validation through existing cheap checks if useful; do not relabel skips as passes, remove qualification gates or churn every historical reason merely to enforce a new spelling. Track debt in the ledger; opening external issues is a separately authorized action. |
| REQ-C-1–C-5, REQ-X-1 — coverage and risk review | Before changing P6 ownership/admission and P9/P10 publication/quota paths, obtain a bounded coverage report if supported, then review affected branches and caller coupling. Extend reports to extension/Python where useful. Target meaningful tests at refusal, rollback, recovery and stale-authority behavior. | Pin development-only tooling and retain measured module/branch scope, configuration, exclusions and unavailable instrumentation. Coverage guides review; it does not prove correctness. Record findings/severity and close relevant safety findings with behavior tests before activation. No broad Store refactor, mandatory percentage target or automatic baseline commit. Full-workspace coverage and possible delta gates remain optional proposals after a comparable baseline and CI-cost review. |
| REQ-F-1, F-3–F-4 — bounded generated tests | Add fixed-seed generated/property cases in owning crates as P6 grants/paths, P7 unions/cursors, P9 denies, P10 partitions and P11 profiles change. Cover arbitrary identifiers/UTF-8, path components, budgets, malformed metadata, revision fences and transactional non-mutation. | Derive properties from actual contracts. Chunkers may overlap or omit by policy, so require correct declared ranges, coverage/omissions and byte accounting rather than assuming disjoint lossless concatenation. Keep deterministic case counts bounded; retain minimal failures as ordinary regressions. Reuse current test facilities first; add a dev-only dependency only where it provides clear value. Real Windows/POSIX traversal and alias tests remain necessary. |
| REQ-F-2, REQ-S-4 — hostile input | During P6e transport, P7 parsing and P11 extraction changes, extend original adversarial fixtures for overlong frames/lines, invalid UTF-8 boundaries, mixed scripts, archives, mutation races and path escapes. Add focused opt-in fuzz targets only for actual exposed parsers/providers with remaining risk. | Use isolated owned fixtures, capped input/output/time/memory and process containment. Generate large/sparse/archive cases on demand within reserved capacity; never extract a bomb unrestricted or follow a test escape outside its fixture. Assert documented bounded outcomes, source preservation and no panic. No blanket 24-hour fuzz prerequisite or new fuzzing crate for unimplemented DiskANN surfaces; qualify actual page providers when they exist. Preserve discovered seeds and conventional regression tests. |
| REQ-R-1–R-6 — broader retrieval evidence | Extend E0's existing `check-retrieval-quality.py` owner at P7/P8 with shared-index attribution and useful subset cases, at P11 with profile comparisons, and before P13 with additional independently labelled held-out code/document queries. At P14, publish only the breadth actually measured. | Keep frozen original controls unchanged. Pin any added public corpus by revision/content digest and redistribution terms, fetch explicitly into ignored isolated storage, and publish only permitted sanitized fixtures. Freeze graded path/range judgments before observing candidate output; record authoring revision and independent spot checks when available. Report nDCG@10/Recall@10 by mode and query family, with MRR/Recall@5 only if useful and hand-checked. Separate literal ripgrep matching from conceptual retrieval; disclose ties, budgets, chunk/file ranking units and unsupported cases. No forced positive headline. Five corpora/300 blind queries remain a later broad-claim target, not a prerequisite for every feature commit; do not call post-hoc labels blind. |
| REQ-P-1–P-4 — resource and scale evidence | At P7, P10 and P14, extend existing scale, real-corpus and service benchmarks for selected-index fan-out, concurrent updates, peak split capacity and cleanup. Run an affordable representative size first, then 10k/100k entries only within explicit capacity/time budgets and supported capabilities. | Preserve this plan's frozen and preceding-phase gates; proposed generic 15% limits cannot replace them. Record p50/p95, failures/retries, ingest throughput, retained/peak disk, RSS and physical I/O only when actually measured. Distinguish discovered entries from eligible files, indexed artifacts and vectors. Publish a separate table per size/shape/platform; 100k synthetic entries alone cannot qualify arbitrary large repositories. A 1M campaign is optional and may remain not run. Add Criterion only for a demonstrated component-measurement gap, not a duplicate of end-to-end runners. |
| REQ-S-1–S-3, S-6–S-7 — durability, concurrency and upgrade | Extend `check-publication-faults.py`, `check-runtime-upgrade.py`, P5's catalog tests and existing service-load owners during P9/P10/P11 and P14. Cover quota/space refusal at publication, cancellation, cutover/restore fences, multi-client updates, process cleanup and retained predecessor compatibility. | Inject storage failures deterministically or use an explicitly bounded disposable volume; never fill a user's storage device. Verify the last admitted revision remains readable, no partial publication leaks, and recovery reports retained intent correctly. Test supported predecessor/candidate identities; unsupported downgrades must refuse, especially after the new-write barrier. Begin with finite load runs and explicit resource bounds; longer soak is optional qualification, not a mandatory 72-hour/nightly dependency. Process termination is crash evidence, not physical power-loss proof. Keep dated pass/fail/not-run receipts and treat relevant integrity/isolation failures as blockers. |
| REQ-S-5, S-8–S-9, REQ-X-2 — privacy and installed paths | Fold the review's second-account Unix storage/Windows Assistance checks and install → first index → cited result → upgrade → uninstall/residue cases into P6a/P6f and P14's existing installed matrix. Investigate the Intel cold-client failure with bounded focused reproduction; reuse the signed-channel work already assigned to P6a. | Prefer documented manual procedures when disposable-account testing requires privileges or unavailable tooling. Never create accounts, modify machine permissions, kill unrelated processes or purge user state as an incidental test action. Inventory all fixture roots and verify owned cleanup. Record exact installed/signed payloads, platform and failure history; unavailable second-account/editor/signing/Intel evidence remains unqualified and cannot support the corresponding release claim. Do not substitute a source mock for an installed result. |
| REQ-D-1–D-6, REQ-L-1/L-3 — useful documentation | With P8 guidance and P14 handoff, extend the existing documentation index, usage evaluation, benchmark/validation records and release workflow/runbook. Make install → first cited result → troubleshooting, contributor setup → architecture/parity and maintainer release → qualification paths easy to follow. | Walk each entry path in at most three links where practical; preserve historical records. Generate summary tables from receipts and link method/limits. README and Marketplace wording must agree with implemented behavior and observed measurements; no compulsory “X% better” sentence, implied regulated-environment certification or invented pricing. Rehearse the existing runbook without publishing and record friction; do not require a new duplicate document or a second maintainer to proceed with independent source work. |
| REQ-CI-1–CI-3 and G-3 — measurement cost | Keep the above campaigns local or explicitly selected qualification, reusing current change selection and artifact provenance. When CI work is separately undertaken, record actual wall-clock/billable cost and propose the smallest missing gate. | No automatic nightly four-platform bootstrap, coverage, fuzz, scale, soak or privacy matrix. No new schedule, dispatch, required release gate or automatic `main` baseline write is authorized by this planning edit. Preserve current lean CI and signing/promotion protections. A proposed +15% cost allowance is not an approved budget. |

**Deliberately excluded from the 1.9.0 implementation path:**

- Automatic coverage ratchets/floors and broad toolchain/test rewrites
  (REQ-C-3/C-4 as mandatory gates), exhaustive mutation campaigns (REQ-M-1) and
  the recurring all-platform programme (REQ-CI-1). Their cost and comparability
  are not established; targeted branch tests and the existing qualification
  gates provide a tractable first step. Future adoption requires measured value.
- A new exact-cosine implementation solely to measure an ANN engine not yet
  selected by the runnable path (part of REQ-R-6/P-1). Reuse the actual provider's
  diagnostics if available; defer additional baselines until they answer a
  demonstrated retrieval question. Commercial-agent measurements need separate
  access/data-sharing authorization; never send repository or private evaluation
  content to obtain a comparison automatically.
- Product activation telemetry/receipts (REQ-T-1), pricing/positioning changes,
  fixed release cadence, outreach, issue quotas, staffing and business kill
  criteria (REQ-L-2/L-4 and REQ-O-*). These are separate owner decisions, not
  dependencies of indexing correctness. This plan adds no activation database,
  new public read contract or telemetry collection, including local-only collection.
- Hazardous literal interpretations of fault/security acceptance: unrestricted
  disk filling, privileged account/ACL changes, unbounded archive expansion,
  automatic historical runtime installation or downgrade, and mandatory long
  fuzz/soak runs. The bounded fixture/manual alternatives above retain useful
  evidence without expanding AIRK's runtime architecture or risking user state.

Safety failures in an exercised affected path remain blockers; declining an
expansive campaign never permits an integrity/privacy failure to be ignored.
Unknowns must limit the published claim. Preserve the Shark Tank draft as dated
input; this reconciliation, current repository guidance and explicit maintainer
instructions govern implementation rather than importing its entire 90-day plan.

### Tests, review, commits and resuming work

For each phase, record its entry commit, acceptance cases, exact test commands,
pass/fail/skip counts, artifact identities, benchmark comparisons, known limits,
documentation changes and recovery boundary in a dated implementation ledger
linked from the future 1.9.0 candidate record. Record the resulting commit ID in
the next ledger update; do not attempt a self-referential commit hash. Keep raw
private corpora/indexes outside Git and publish only sanitized original fixtures
and evidence summaries. Update the ledger before moving to the next phase.

Run focused behavioral tests while developing. Before accepting a code phase,
run the formatting, Clippy, workspace tests, repository and whitespace checks
required by [CONTRIBUTING.md](../../CONTRIBUTING.md), plus affected extension,
Python, native-transport and installer checks. Provision the pinned offline model
and runtime inputs when required. UI mocks are supplementary. A missing tool or
skipped required gate is recorded as **unqualified**, never a pass; do not accept
the phase as complete until its required gate is resolved. Documentation-only
checkpoints use repository/link/whitespace checks without rerunning runtime suites.

Review the complete phase diff and relevant earlier changes before committing;
fix root causes and sibling paths, not just the last failing assertion. Use
`git commit -s` with the configured authorized contributor, verify its trailer,
and use a focused message such as `feat(indexing): add reviewed project grants`.
Preserve the user's branch and history; no per-phase branch/PR churn or automatic
push. Tests and documentation belong with their behavior. Large hosted matrices
are not a per-commit debugging loop; preserve the repository's routine CI policy
and request broad hosted qualification only for a concrete release need.

An ordinary code revert is sufficient only before new persistent state is
accepted. After a schema/policy change or split/trim cutover, follow the recorded
upgrade/operation recovery boundary: a Git revert cannot restore a database or
undo an exclusion safely. Use disposable fixtures for intermediate checkpoints,
not a user's active index as the migration test bed.

**Version discipline:** P0b's original 1.9.0 bump is historical; the current
working version is 1.8.8 until implementation and manual acceptance, followed by
a fresh 1.9.0 build and qualification. Do not bump for each phase. Local
development uses isolated
source/runtime test environments with exact commit/payload identities. Withhold
distribution of installable candidates until the chosen release gate. If changed
VSIXs must be distributed earlier, revisit the version schedule: existing
[versioning policy](../versioning.md) requires a new unused numeric patch for each
changed distributed candidate, not different bytes labeled 1.9.0. Never relabel
tested binaries or weaken the existing same-version replacement protection.

## Performance protection throughout implementation

Performance is an acceptance condition for every phase, not a late optimization
pass. P0a records the current 1.8.7 branch baseline and any differences from the
verified installed or published comparison build, if available, including
pre-existing failures. Compare each phase with both
that frozen baseline and the preceding accepted phase; a chain of small slowdowns
must not evade the cumulative gate. Historical results in
[the 1.8.2 performance report](../performance-improvements-1.8.2.md) identify work
to preserve, not universal speedup claims or numerical thresholds for 1.9.0.

### Baselines and budgets

Use the same source hashes, model/profile, budgets, build mode, concurrency,
machine/CPU limits and readiness state for matched comparisons. Separate cold
setup, first useful response and warm reads; installed launcher/editor observations
are distinct from isolated native measurements. Include unchanged scans, one-file
edits, indexing under query load, cancellation, timeouts and failures. Keep all
attempts and outliers. Choose sample counts, warmup, repetitions and ordering
before candidate runs; do not claim a stable p95/p99 from a tiny sample.
Keep unrelated builds and other measurement campaigns out of matched timing runs;
contention belongs only in the explicitly defined load cohort.

P0a must fill a budget sheet for each cohort: baseline median/tail latency,
throughput, failure rate, CPU, peak RSS, disk reads/writes, retained/peak disk,
response/IPC bytes and relevant work counters. Set numerical absolute and
relative allowances from observed noise **before** candidate measurements, with
a total-baseline ceiling as well as a per-phase ceiling. The default target is
no material regression for unchanged behavior. Correctness-required overhead
must be quantified and reviewed; never hide it by widening a timeout, dropping
evidence, lowering sample counts or rewriting the baseline. A missing/noisy
baseline remains an open gate for that path, not a fabricated improvement.

Record a metric as **unavailable/not instrumented** when the harness cannot
observe it; existing runners do not measure every field above. P0a must establish
the critical legacy startup/read/indexing controls and freeze the evaluation
method, without building every later feature's instrumentation first. Add the
missing phase-specific counters and freeze its limits before that phase's
candidate comparison. Unaffected work can proceed, but a missing required metric
cannot qualify the affected phase. For a new behavior with no 1.8.7 equivalent,
declare a same-work control or mark the comparison inapplicable; report its
absolute resource budget separately and still protect unchanged legacy paths.

| Cohort / contract | Measurements and deterministic protection |
| --- | --- |
| Legacy single-index reads, verification off | Cold/warm search, symbols, references, reads, plans and first status through actual CLI/MCP. No extra source hashing, model initialization, full catalog walk, coverage audit or writer work; grant/policy validation stays bounded. |
| Status and editor responsiveness | Preserve independent status admission/deadlines, fenced reuse of healthy connections and observations, scalar status/save updates, single delivery of full results and retained view state. Opening Recommendations or changing a picker must not trigger diagnostics, reindexing or repeat full history/result transfer. |
| Indexing and storage | No-op generation/reuse, localized edits, vector reuse, admission, cleanup and retention plateau. Project/member/name/default changes do not rebuild indexes, duplicate watchers or load models. Profile/source changes disclose their separate indexing cost. |
| Combined reads | Test 1, 2, 4 and the supported maximum unique indexes, including shared members and slow/unavailable ones. Aggregate byte/result/deadline budgets and bounded service admission apply; N members cannot create N unrestricted nested requests. Unselected members perform no evidence/model/source work. |
| Verification report/require | Measure distinct files and total raw bytes, with large documents and slow/remote sources separated. Charge hashing to explicit file/byte/time/concurrency limits; deduplicate within the request and keep the `off` path free of verification I/O. |
| Split/trim | Measure foreground read impact as well as job duration, peak storage and cleanup debt. Reuse service scheduling/quotas; bounded resumable turns, cancellation and persisted progress must not starve ordinary queries or status. |
| Recommendations | Bound rows, prefix comparisons, response bytes and runtime independently of corpus bytes. Use saved metadata/observations, revision-keyed caches and explicit refresh; no per-query advisor or background corpus scan. Large/incomplete libraries return partial advice with limits. |

Use deterministic work assertions in routine tests: unique member executions,
zero hashing when verification is off, zero model loads on metadata-only paths,
zero generation changes on no-op indexing, no duplicate response payloads and
no unselected-index access. Put noisy elapsed-time comparisons in controlled
campaigns, not brittle unit-test wall-clock assertions. Preserve current cache,
certificate, admission and invalidation safety; do not bypass them for a score.
Resolve grants once per bounded request and revalidate their revision fences;
do not scan every Project or every deny rule for every returned chunk. Compile or
index canonical prefix policies at policy revision, bound policy size and retain
an empty-policy fast path. Cache keys include service/registry, grant, membership,
policy and relevant publication revisions; revocation and exclusions always fence
stale entries. No cache entry becomes fresh source-verification authority.

Reuse existing measurement owners before adding a runner:

- `scripts/benchmark-mcp-investigation.py`: cold status-first, unprimed and reused
  task cohorts; extend its current concurrency choices to exercise the configured
  16/32/64 limits and selected-index fan-out, without claiming that coverage today.
- `scripts/benchmark-service.py`: synthetic multi-project/client and concurrent
  indexing waves; distinguish successful latency, rejection and retry outcomes.
- `scripts/benchmark-v2.py` and the existing vector performance target: reuse,
  no-op/edit work and model/cache behavior under qualified fixture ownership.
- `scripts/benchmark-real-corpora.py`: separate isolated-native corpus costs;
  it does not establish installed-launcher or editor latency.
- `scripts/benchmark-scale.py` and `scripts/benchmark-indexing.py`: inspect their
  current entry/ingest workloads before extending scale qualification; keep
  discovered-entry, indexed-file, resource and readiness measurements distinct.
- E0's original relevance corpus and existing UI/native transport fixtures:
  completed useful tasks, nDCG@10/Recall@10, exported completeness and IPC work.

First add only the missing observations needed for the active phase. Re-run its
affected cohorts and the cheap legacy single-index/status sentinel; run the wider
cumulative campaign at P7, P10 and P14. Keep routine CI lean and all failures
visible. An opt-in feature may consume additional work within its declared budget;
it must not impose that cost on users who leave it off or query a smaller scope.

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
  Sources, Project definitions/membership, **Recommendations** and a review drawer
  replace folder-driven setup. Keep service/storage/recovery operations in
  Management with targeted links. Preserve manual agent
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

Follow P3/P5/P6 for contracts, safe catalog/upgrade and complete Indexing/grant
activation; expose shared combined reads and consumers together at P7. R1/R2 add
recommendations and management links. X6 expands migration/fault/platform
qualification after the safety-critical path already works. An index picker alone
does not satisfy this feature. Prioritize it over optional ranking experiments;
any reduced release scope requires a maintainer decision.

#### Indexing recommendations and agent query scope

Add a visible **Recommendations** section in Indexing, with contextual links from
This window, Project and Index detail. Its purpose is to help the user decide
**what to index, what to reuse and what to query for a particular task**. It uses
local deterministic rules and user choices, without agent reasoning, a model,
external services or retained query/source text. Suggestions are advice; the
native review and authorization rules remain authoritative.

Show **Available to this connection** separately from **Selected for this query**.
A window may make several indexes available while a query selects just one or a
useful combination. Display the effective deduplicated index count, source/prefix
boundaries, selected Projects and omitted members. Selecting all is an explicit
choice with cost and coverage tradeoffs, never an automatic response to empty
results or an unavailable member. Recommendations must also offer **Keep this
layout** when splitting or narrowing has no demonstrated benefit.

| User situation / observed evidence | Recommendation and value | Tradeoff and next action |
| --- | --- | --- |
| An existing index covers the intended sources with suitable policy | Reuse it with reviewed Project membership/prefix scope; avoid another watcher, scan and embedding set | Verify actual roots, readiness and permitted scope. Offer **Review reuse**, not inferred ownership from folder names. |
| A task concerns one application or package | Select its index, or a supported prefix inside that index, as the starting scope | Lower fan-out and less unrelated evidence are expected, not guaranteed latency/quality gains. Show omitted dependencies/specs and offer deliberate widening. |
| Implementation depends on shared code or specifications | Select a purposeful combination, such as application + shared library + specifications, leaving unrelated indexes out | Show deduplicated members, readiness and total query limits. **Use this query scope** changes a selection, not grants or indexing policy. |
| One index contains distinct source groups with different lifecycle/profile needs or frequent narrow use | Offer a split preview to obtain independent policy/budgets and reusable memberships | Prefer existing scope filters if sufficient. Splitting costs a destination build, temporary space and future jobs; querying both may add overhead. Size alone is not a reason to split. |
| Two distinct indexes overlap the same sources | Review reuse or query only one where its profile/coverage suffices | They may intentionally differ. Membership removal is distinct from index deletion; no automatic merge, pruning or data removal. |
| A path should remain indexed but is irrelevant to this task | Narrow the query or change the Project's selected defaults | Do not recommend permanent denial merely to reduce a query's scope; other Projects may need the evidence. |
| The user explicitly identifies a path that should never be in this index | Offer **Review permanent removal** with all affected Projects and any other indexes covering it | Explain immediate exclusion, cleanup stages and separately reviewed unblock. It removes eligibility for every user of that index, not the source files. |
| Saved observations are missing, stale, partial or conflicting | Explain the limitation; offer bounded metadata refresh or manual selection | Do not claim savings, completeness or a safe split from unknown counts. Any deeper inventory is a separate disclosed management operation. |

Each card gives the observed reason, source/time of that observation, proposed
selection or management action, affected indexes/Projects, expected benefit and
cost/uncertainty. Distinguish **observed**, **estimated** and **unknown** values;
never manufacture a savings percentage or claim that fewer indexes necessarily
improve recall. A library index can be large yet appropriate; a split can make
combined work slower. Use existing bounded counts/root relationships and declared
policies to identify candidate source groups, not a new semantic clustering pass.

**Controls and persistence.** Provide **Use this query scope**, **Save as Project
default**, **Copy agent scope instructions**, **Review reuse/split/removal** and
**Dismiss** where applicable. Query selection is temporary; a saved Project
default is a separately reviewed metadata change affecting the displayed set of
users and future selections. Neither expands an active grant. Never silently
replace a restored session's explicit selection. A recommendation requiring a
new index, membership or wider grant opens its existing review flow; active and
pending connection scope stay visibly distinct. Revalidate IDs, revisions,
eligibility and grants before acting on a stale card. Unsupported actions remain
explanatory until their phase is qualified; R2 enables split/trim action links.

**Bounded implementation.** R1 adds one canonical native advisory read over
authorized metadata; CLI, Workbench and MCP expose the same bounded fields and
supported inputs. Registry management visibility and agent query grants are
different: an agent receives advice only about its permitted scope, never the
machine-wide library. The extension renders advice and generates copy text from
that result plus the advertised tool catalog; it does not implement an alternative
scope resolver. Reuse current aggregate observations if available, without adding
query logging or a database to infer intent. A user may state an intended source
group/task category; that choice is a filter, not permission to discover content.

Bound catalog pages, candidates, prefix comparisons, bytes and elapsed work.
Avoid all-pairs library comparisons; selected-candidate overlap checks can refine
a proposal during review. Key cached advice by service/catalog/membership/policy
revisions, authorized-scope/grant fingerprint, selection/filters and saved
observation version/expiry. Readiness can change without a new publication;
expired observations produce stale/unknown advice rather than a current claim.
Do not share advice across different authorized scopes. Show observation age;
grant revocation still invalidates authority immediately. No source hashes,
coverage scan, evidence query, model load or new
watcher is permitted merely to open the screen. Refresh uses existing observation
ownership, coalesces events and stops view-only work when hidden; it introduces
no recommendation polling loop. Scalar updates do not resend full catalogs or
retrieval results. Defer automatic scope benchmarking and learned recommendations;
E0/P7 can evaluate selected combinations in controlled contributor fixtures.

**Agent-harness instructions.** Copy a self-contained plain-text/Markdown block
with the chosen Project/index names and stable IDs, effective prefix restrictions,
generation time, scope revision/fingerprint and active-versus-pending connection
state. Include tested JSON examples for the actual installed catalog. Keep
configuration edits separate: copying must not write AGENTS.md, CLAUDE.md, client
configuration or sources, and copied text is not a transferable access grant.

The generated instructions teach the harness to:

1. Discover the current permitted scope and supported inputs, then confirm that
   the copied IDs still resolve. Discovery tool names come from the live catalog;
   proposed `query_scope` is not assumed to exist on today's server.
2. Send explicit `project_ids` **and** `index_ids` for the chosen subset where
   supported. Index IDs narrow the authorized union of the selected Projects;
   they never grant access independently. Use one combined request for a useful
   set rather than one request per Project that re-queries a shared index.
3. Preserve the originating index, exact publication, source hash/location and
   valid handle for follow-up reads. Same-named paths in different indexes are
   different evidence; a follow-up selects its explicit origin.
4. Inspect member outcomes, readiness, truncation, total budgets, continuations
   and verification receipts. `member_availability=require` and
   `verification=require` solve different problems; neither certifies whole-scope
   completeness. Continue only using returned valid continuations and typed retries.
5. Treat zero results as **no matches returned** under the selected publications,
   readiness, filters and budgets, not proof of absence or completeness even
   within that scope. A broader query is a deliberate, reported selection change
   within current grants; never silently select all indexes, drop failed members
   or request broader access.
6. On an old catalog, use the supported legacy single-index `project` binding and
   clearly report that combined selection is unavailable. Never send future
   fields to 1.8.7 or reinterpret legacy bindings as new Project IDs. Do not
   synthesize a merged result with lost provenance as a compatibility shortcut.

Illustrative future `search` arguments for **Payments** plus **Platform** Projects,
querying only **Payments code** and **Product specifications** (the shared
Platform code index remains omitted). These IDs are fictional and the fields are
proposals; the actual copy action substitutes validated IDs and only advertised
arguments:

```json
{
  "project_ids": ["payments-project", "platform-project"],
  "index_ids": ["payments-code-index", "specifications-index"],
  "query": "refund policy",
  "mode": "lexical",
  "limit": 12,
  "max_bytes": 8192,
  "member_availability": "report",
  "verification": "off"
}
```

Omitting `index_ids` may resolve every permitted member of the selected Projects;
the generated subset recipe must therefore retain it. An explicit empty or
invalid selection is a validation failure, never an alias for all indexes. Saved
defaults that are no longer permitted require review rather than fallback to a
larger scope. Discovery/capability failures produce unavailable instructions with
an actionable reason, not guessed valid IDs or inputs.

R1/P8 tests cover every decision row, missing observations, disjoint prefix grants,
duplicate shared memberships, partial pages, stale reviews, dismiss/default/session
behavior and large-library work bounds. Parse generated examples against actual
tool schemas and exercise canonical CLI/MCP paths; prove unselected/ungranted
indexes are untouched, same-path follow-ups retain origin, and copied text does
not leak unauthorized roots. R2/P10 adds split/trim previews, inherited exclusions,
affected-Project disclosure and post-operation scope regeneration. UI interaction,
keyboard/accessibility and narrow-layout tests supplement native/transport tests.

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
| Indexing recommendations | Always query all; opaque automatic optimizer; bounded explanatory advice | User-selected focused index or useful combination, reuse and reviewed split/trim suggestions | Clearer choices and lower unnecessary work without hidden coverage changes | Medium / stale metadata and authority | Explicit selected/omitted members, bounded metadata-only computation, current grants and no autonomous mutation |
| Agent scope instructions | Hardcoded examples; edit harness files automatically; capability-checked copy | Generate explicit Project/index subset recipes from active grants and canonical schemas | Agents use the intended scope and preserve provenance in follow-ups | Medium / catalog compatibility | Actual transport examples pass; old runtimes receive supported legacy guidance; no silent all-index fallback |
| SQLite upgrade | Rename old project rows in place; rebuild indexes; additive normalized metadata migration | New Project/index join plus compatibility mappings, U0–U6 upgrade path | Preserve existing data/settings while introducing the new hierarchy | Large / database and mixed-version safety | Consistent backup, atomic backfill/version, idempotent imports, crash recovery and post-write downgrade refusal |
| Split index | Copy entire databases; defer splitting; reviewed captured rebuild into two partitions | Retain original ID for the remainder and create an independent second index, with explicit membership cutover | Separate unrelated sources/budgets without losing Project control | Large / multi-index operation and peak disk | Disjoint complete partitions, inherited denies, staged destination, resumable fenced cutover and no premature grants |
| Trim and permanent path block | Delete current records only; source ignore edit; native index-owned deny plus cleanup | Durable deny first, then remove evidence and reclaim owned storage | Remove unwanted material and prevent future reintroduction | Large / policy enforcement and reclamation | Every capture/read path honors the deny across restarts/rebuilds/aliases; physical cleanup reported separately |
| Reply control | Change all defaults; preserve compatibility with explicit Lean/Rich selection | Preserve no-argument behavior; explicit Lean/Rich projection, with Lean recommended for agent/CLI setup and Rich for Workbench | Predictable detail without surprise byte/field changes | Medium / compatibility | No new argument on an existing Agent Lean collection retains its projection/encoding; actual full-result parity for explicit choices |
| Retained evidence | Rename old levels; file-aware Lean/Rich; dynamic enrichment | New versioned Lean/Rich profiles; keep Rich Structure as the legacy `balanced` label | Useful manifest/prose/table evidence without confusing old continuity semantics | Large / publication and migration | Legacy label/identity preserved until explicit migration; new Rich is a superset of new Lean |
| Rich storage | Selected payload; common core plus rich objects; second mutable index | Selected payload first | Simpler ownership and recovery | Medium / bounded change | Cost campaign demonstrates whether separate rich objects are warranted later |
| Lexical relevance | Stable-ID ranking; ranked exact matching; implicit relaxed matching | Evaluate ranked exact matching; stable IDs break ties | More useful candidates within existing budgets | Medium–large / index cost | Matched relevance gain, narrow-scope recall and authenticated update behavior |
| Diversity | Hidden display cap; explicit optional selection; always-on file cap | Explicit optional selection, judged separately | More distinct useful files when results repeat | Medium / recall tradeoff | Disclose subset; preserve concentrated-result option; no hidden export loss |
| Missing evidence | Empty results; new status subsystem; reuse canonical coverage plus actionable reasons | Extend existing observations only where necessary | Distinguish absence from unavailable evidence and choose a next step | Medium / shared contract | Separate unsupported, omitted, pending, failed, stale and truncated outcomes |
| Helper outage | Fail every search; unbounded restart; policy-qualified lexical degradation plus bounded recovery | Diagnose exits, recover per outage and honor strict versus Default policy | Useful evidence and understandable recovery without false completeness | Medium–large / process ownership and parity | P6b/P6c real supervisor and actual read transports; no per-read probes, model starts or ambiguous replay |
| Freshness and background progress | Compare unequal tokens; poll every turn; reuse causally valid observations and prioritize admissible source work | Authority-aware publication comparison and bounded existing writer admission | Truthful freshness labels and timely lexical edits during embedding | Medium / ordering and starvation | P6d/P6e backlog fairness, late-response and incomparable-publication tests; no extra scans or unconditional status RPC |
| Signed distribution | Different signing by channel; weakened launcher guards; one approved signed producer with standalone-safe launcher | Preserve integrity checks and qualify exact installed payloads on each channel | Working macOS installation and reproducible provenance | Large / credentials and installed-platform proof | P6a/P14 copied signed executable, real editor/agent path and published-payload verification; no Windows-only success claim |
| Certainty of returned references | Reindex every hit; metadata-only checks; hash distinct returned sources; re-extract every source | Bounded full-source hash verification, separate explicit refresh on mismatch | Evidence of saved-source agreement at a stated time, usable by people and agents | Medium–large / file races, scope and latency | Every referenced source accounted for; stable captured bytes match the exact result hash; failures cannot receive a verified label |
| Performance acceptance | End-only benchmark; per-phase relative checks; frozen baseline plus phase checks | Deterministic no-extra-work gates and matched resource/quality budgets against both references | Preserve responsive existing paths and expose new feature costs | Medium / measurement noise and cumulative drift | Freeze thresholds before candidate runs; keep failures and tail observations; no cost when the feature is off |
| Additional audit qualification | Import a new recurring assurance programme; ignore the review; integrate bounded tests into affected phases | Extend existing owners with targeted risk/property/fault and held-out evaluation cases; keep broad campaigns opt-in | Better failure detection and credible claims without unbounded delivery or CI cost | Medium / scope and instrumentation | Selective REQ mapping above, actual receipts, unchanged frozen gates and explicit unknowns; no hazardous live-state testing |
| Advanced optimization | Reranker/model swap; unit tuning; cross-collection cache | Defer; retain measured experiment backlog | Avoid new resource/lifecycle cost without demonstrated benefit | Large / qualification | Revisit only with baseline misses and measured task/resource gains |

## Work packages and review boundaries

Use the ordered same-branch phase commits above, with one behavior and its complete
native/interface contract per acceptance boundary. Do not expose native fields
and leave parity to a later cleanup. The package identifiers below describe
design dependencies; they are not an alternative order or a requirement for a
separate PR per package. Keep internal scaffolding inert until its public gate.

| Package / order | Deliverable and boundaries | Depends on | Completion evidence and value |
| --- | --- | --- | --- |
| W0 — first | Repair compact locations/aggregation/bindings and trace nested-plan schema mismatch. Convert the two-tier battery's 35 post-run invariants into appropriate deterministic regressions, retaining negative cases and the compact-plan task assertion. | None | Actual compact/rich native/CLI/MCP paths preserve citations, identity, guard failures, budgets and dependent reads; captured checks are not mistaken for passing workflow qualification. |
| E0 — start with W0 | Extend tooling with original versioned code/manifest/prose/table queries and graded expected ranges. Use nDCG@10 as the primary relevance metric and Recall@10 as the recall guard, scored separately for lexical/vector/hybrid at fixed budgets. | None | Frozen judgments, negative-query outcomes, bytes/expansions and cold/warm latency; synthetic harness cases are not the production relevance baseline. |
| P6a–P6f — operational foundation | Integrate dated issues #58, #59 and #61–#67 through the ordered remediation sequence above; share canonical ownership, setup, availability, admission, transport and evidence contracts with new management. | P5 accepted; P6b before P6c; all relevant source gates before X2/X3 activation | One coherent behavior/test/docs boundary at a time; actual native/transport evidence, isolated installed roots, unchanged performance gates and explicitly outstanding signed-platform qualification. |
| X0–X1 — replaces W1 | X0 records workspace fixtures/limitations and ADR 002. X1 adds bounded native `relationships` (same/contains/contained_by), `repository_markers`, broad-root observations, catalogs/join/scopes/grants and M0 metadata. Implement the complete safety-critical U0–U6 path at P5 before P6 activation. | W0 for consumers; X0 design can start earlier; M0 before X1 schema freeze | Preserve old isolation. Bounded observations and actual parity; SQL/backup/backfill/import/readiness/recovery/mixed-version gates pass before live migration, not first at X6. |
| X2–X3 — replaces W2/W3 setup | New Indexing screen, Project membership/source assignment, opening Projects per window and enforced grants/revocation. | X1 | No unrequested folder indexing; shared indexes reused; Project/member/window scope and actual transport tests pass. |
| X4 — combined reads | Multi-Project resolution, unique-index execution, qualified follow-ups, vectors/errors/cursors and canonical input catalogs. | X3/T1 for internal work; public activation with X5 at P7 | Shared membership cannot duplicate scoring, jobs or counts; attribution/scopes preserved. Internal implementation has no V2 dependency; public capability cannot precede complete parity. |
| X5 — combined consumers | UI/parse/session/copy/export integration; extend complete V2 single-index receipts to each member citation. | X4, T1, V2; joint P7 activation | Full data survives every consumer; member receipts extend the predesigned identity fields without changing their guarantee. |
| R1 — recommendations and agent scope | Bounded native advisory read, Indexing cards, explicit temporary/saved selections and capability-checked copied harness instructions. | X5, T2a, X0/M0 policy and advice contracts | Complete advisory parity, current authorized scope, useful subset examples and bounded work; no model, scan, auto-selection or mutation. |
| R2 — management recommendations | Connect split/trim recommendations to qualified previews and regenerate affected scope instructions after reviewed completion. | R1, M1, M2 | Shared-Project effects, costs/unknowns, inherited blocks and changed grants disclosed; preview advice cannot bypass native admission. |
| M0 — content-management contract | Design split partitions, persistent index-owned include/deny policy, operation receipts and shared-Project effects; include compatibility and schema work in X1/U stages. | X0 design; before X1 migration freeze | No alias/ignore/rebuild bypass; ADR 002 includes split/trim lifecycle and upgrade implications. |
| M1 — permanent trim | Native deny admission/fencing, removal publications, pack reclamation, status and Inspector/CLI UI actions. | M0, X1, X3; X2 for Explorer UI | Immediate exclusion across actual native/transport paths; policy survives failure/upgrade/rebuild and cleanup has honest states. |
| M2 — split index | Reviewed partition/destination preparation, capacity admission, independent indexing, residual deny/publication and membership cutover with durable recovery. | M1, X2, X3; existing native publication jobs | Crash-safe two-index result, no source mutation, no implicit shared objects or grant expansion; inherited exclusions and ownership verified. |
| X6 — feature qualification | Expand the already functioning U0–U6 upgrade/activation path with the complete fault/resource/relevance/installed-editor matrix including recommendations and split/trim. | X1–X5, M1–M2, R1–R2 | Mixed-version handling, operation recovery/cleanup, rollback limits and platform evidence; catalog-only delivery is insufficient. |
| T1 — independent replies | Explicit canonical Lean/Rich selection and legacy compatibility; preserve existing absent-argument defaults and encoding, including Agent Lean MCP/CLI reads. Workbench keeps its existing rich behavior. | W0 | Explicit-choice parity; regression comparing no-new-argument results modulo documented identity/timing; conflicting flags reject. No storage change. |
| T2a / T2b — availability and profiles | T2a defines shared availability/reason vocabulary using existing observations, without profile or registry changes. T2b inventories families and adds bounded effective-profile reporting before new profiles. | T2a: T1; T2b: T2a, with X contracts reconciled if selected | Unsupported/not retained/reply-omitted remain distinct; bounded status. T2a can ship independently of X/T3. |
| T3 — file-aware evidence | Implement Lean/Rich first for sampled code, manifests, Markdown and DOCX; publish under existing ownership. Separate migration/recovery review from adapter policy review. | T2b | Manifest facts survive Lean; Rich retains the core; pause/manual/vector settings survive; interruption/quota failure keeps the valid publication. Broad adapter qualification remains explicit. |
| V0–V2 — single-index verification | V0 produces ADR 003 and a receipt schema with index/member identity slots from day one; V1 implements hashing; V2 ships native/CLI/MCP/Workbench single-index receipts. | V0 design alongside E0; V1 after V0/W0; V2 after V1/T1; no X dependency | Match/mismatch/race/budget outcomes preserve existing index scopes. X5 later extends member associations additively. |
| V3 — explicit refresh | Refresh/requery through existing reviewed native service operations and qualify recovery. | V2; X5 only for the optional multi-index refresh path | No hidden mutation, unsafe replay or loop; readiness and publication transitions explicit. |
| S1 — ranking | Diagnose lexical candidate cutoffs; select and implement an authenticated local ranking design. Preserve AND semantics and metadata filters before limits. | E0; T3 for final matched-profile evaluation | Better held-out relevance within predeclared cost limits, stable ties and no guard regression. Delivers the clearest search-quality experiment. |
| S2 — optional diversity | Evaluate explicit file diversity independently of ranker changes. | E0 and S1 comparison baseline | Selected subset/truncation survive exports; concentrated results remain inspectable. |
| G1 — guidance | Consolidate missing-evidence reasons and inert next-read suggestions, identifier routing, paging and relationship caveats across interfaces. | T2a vocabulary only; no S1/S2/X/T3 dependency | Existing empty/unavailable/excluded/stale/truncated cases get correct next actions without ranking changes or unadvertised new inputs. |
| Q0 — release gate | Review combined behavior, run task/resource campaigns and document supported scope/migrations. | All selected release packages; X/V3 only when included | Complete task outcomes, parity, failure recovery and platform limits are recorded for the declared full or fallback scope before 1.9.0 claims. |

The dependency rule is **V2 delivers single-index receipts first; X5 adds per-member
citation associations later**. Neither V1 nor V2 depends on X. X4's internal work
depends on X3 and T1; public X4/X5 activation also requires V2. T2a/G1 remain
independent of X. These independent contracts permit smaller phases, not an
instruction to implement several phases concurrently. E0 runs throughout. Keep S1
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
- Recommendations use these contracts and never supply authority. Set limits,
  metadata observations, capability discovery and saved-default/session semantics
  before R1; distinguish query omission, membership removal and permanent denial.
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
`verification: off | report | require` (default `report`) and
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

The requested full target is W0, E0, X0–X6, M0–M2, R1–R2, T1, T2a/T2b, T3,
G1, V0–V3 and Q0. S1/S2 remain conditional on measured gains. Use the phase
schedule and performance gates; reassess estimates after P3/P5. This is the
default implementation scope. Reducing the requested Project/index, split/trim
or recommendation scope requires the maintainer's review, not an autonomous
fallback because the work is large.

**Contingency for maintainer decision, not the current target:** a minimum
fallback 1.9.0 could contain W0, E0, T1, T2a's existing-observation vocabulary,
G1, V0–V2 and Q0 for those packages. V2's member/index identity fields are present
from the start, with one member, so X5 stays additive. Retain existing index
policies; defer X0–X6, M0–M2 and R1–R2 delivery to 1.10 only by that decision,
with optional design/ADR work remaining inert. Also defer T2b/T3, V3 and S1/S2
unless independently qualified.
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

Finish any existing owned control-layout/default-index-directory relocation while
the registry still uses the legacy format, before admitting schema 12. Its legacy
rewriter replaces matching values throughout the registry and must not touch the
new immutable index backing identities. After schema 12, refuse that rewriter
before persisting a relocation intent or moving data; retain completed receipts.
The activation path must resume a pending catalog with its matching candidate
before normal readiness probes. General root relinking remains deferred; supporting
a later physical relocation of an active catalog requires a separately qualified
typed identity/mapping operation. This ordering preserves the existing legacy
upgrade route without allowing partial directory moves or predecessor rollback.

Reuse and extend existing managed-install ownership/drain: Windows/Linux have
verified predecessor termination, while macOS has graceful shutdown and requires
its own qualification. Unknown/undrained owners block migration. An activation
timeout must inspect the saved upgrade receipt rather than restart U3 blindly.

P5 implements and tests the complete operational U0–U6 safety path in isolated
state; P6 is the earliest normal-state activation boundary, after Indexing and
grant consumers are ready. U4's safe pending imports, U5's readiness decisions
and U6's bounded settlement/recovery cannot be deferred to final qualification.
X6 adds wider evidence for that working path. Feature gates are not bypasses for
unknown schemas or unsupported state; failed admission retains the old supported
selection or an explicit recovery state, never partial readiness.

Validate registry, collection-policy and operation-receipt versions independently
of the application version. Earlier development builds also labeled 1.9.0 must
refuse newer grant/deny/split semantics they cannot enforce. Bind test receipts
to exact runtime build identities and exercise predecessor development readers
as well as 1.8 readers. New policy tables may be prepared inertly, but do not admit
deny/partition policies until all native readers enforce them at P9/P10; an old
direct CLI cannot bypass that requirement. Do not relax managed activation rules
for intermediate same-version development builds.

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
| P0b | synchronized version manifests/lockfiles/catalogs, versioning and contributor version guidance, installer forward-line policy, candidate release record and accurate README/index/Marketplace status |
| P6a–P6f | service-embedding, service-freshness, service-project-setup, CLI help/reference, mcp, unified-mcp, read-interface-parity, Workbench freshness/trust/error help, signing/build/release provenance and installed-test isolation. Keep issue-analysis reports dated; record current evidence in the ledger/release record. Include selected Shark Tank setup/skip/risk findings in existing contributor and validation records; state channel/platform limits accurately in README and Marketplace copy. |
| X0 | ADR 002, vscode-extension current behavior/limitations, scenario map, architecture boundaries and cross-project design |
| X1 | collections, source-roots, storage-formats, service-project-setup, service-freshness, versioning, U-stage release/upgrade notes |
| X2 | vscode-extension, Explorer help and source-assignment/Project UX; extension README/Marketplace setup text |
| X3 | unified-mcp, mcp, cli-reference, agent-integration, source-roots, versioning and grants/revocation help |
| X4–X5 | unified-mcp, mcp, cli-reference, read-interface-parity, Workbench forms/session/export help and both developer guides |
| R1–R2 | Indexing Recommendations help, cross-project design, agent-integration, CLI/MCP developer guides, canonical input catalog, scope recipe examples, saved-default/session behavior and split/trim decision help |
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
| Intermediate phase exposes incomplete schema/API behavior | P5–P7 activation gates | Complete operational upgrade safety before migration, combined-read consumers before advertisement, exact format/build fences and isolated fixtures; never assume a code revert repairs persisted state |
| Old extension/build/profile reopens shared state | X3/X6, scenario 20 | Explicit version/schema fences, upgrade-required UX and no automatic old-runtime activation or grant expansion |
| Verification dominates latency on large PDFs/remote files | V0–V2, E0 | Full-source hashing with measured byte/time admission, bounded default Report mode, explicit unchecked outcomes; no metadata-only substitute or blanket speed claim |
| Lexical ranking needs FTS layout rebuild | S1 design gate | `detail=none`/`columnsize=0` may not provide chosen scoring inputs. Audit authenticated FTS schema and every publication/update path; any layout change needs a separately versioned collection lexical/index-v1 compatibility and rebuild/migration plan, quota/recovery/rollback tests and per-collection cost disclosure. It is not the X registry migration; keep current ranking if not qualified |
| Split or trim leaves leaked evidence or incomplete ownership transfer | M0–M2/X6 | Durable deny fences, independent destination ownership, peak quota reservation, shared-Project review and phase-by-phase fault tests; no success while partition, grant or cleanup state is unknown |
| New features undo current performance improvements | Every phase/E0/Q0 | Frozen 1.8.7 and previous-phase comparisons, deterministic work counters, controlled tail/resource campaigns and no unselected/disabled feature work; stop at an unexplained breached gate |
| Advice creates overscope, bad splits or a new background cost | R1/R2/P8–P10 | Bounded authorized metadata, explicit selected/omitted indexes, unknown estimates, schema-checked recipes and native reviews; no automatic management or all-index fallback |
| Helper recovery hides failure or creates restart/query churn | P6b/P6c, P7 | Diagnose actual exits, typed bounded recovery, explicit Stop, saved-policy semantics and complete partial/error data; no read-triggered startup, unconditional retries or strict-mode degradation |
| Vector backlog starves source edits or UI reports a fresh result as older | P6d/P6e, P7/P12 | Fair admissible source admission, bounded vector progress and causally scoped publication comparison; measure backlog latency and preserve incomparable/unknown labels without per-query polling |
| Signed macOS launcher fails or Marketplace differs from qualified native payloads | P6a/P14, release handoff | Standalone-safe signature design, copied installed-path/editor/agent tests, immutable signed producer and channel payload verification; credentials/platform omissions remain release blockers, never inferred passes |
| Expanded scope prevents a coherent release | Q0 after P3/P5 | Ask the maintainer to review the documented contingency; no silently dropped requested features, partial hierarchy or migration-only release |
| Assurance work becomes a second product or damages a test host | Selective Shark Tank mapping, each affected phase/P14 | Extend existing harnesses, cap fixture resources, preserve user/OS state, keep long campaigns opt-in and unknowns explicit; no new telemetry, automatic coverage commits or recurring broad CI matrix |

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
   `verification: off | report | require`, default `report`, as specified above.
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

**Recommendation:** deliver bounded read-only verification with Report as the default in 1.9.0 and
make explicit refresh/requery the recovery path. This adds useful certainty
without paying reindexing cost on every unchanged hit. Consider a default-on
policy only after the measured latency and verification coverage meet agreed
thresholds; retain a strict option for consumers that require every returned
reference to be checked. Defer universal certification, automatic read-triggered
indexing and whole-workspace freshness claims.

## Validation and release decisions

Set numerical quality and resource thresholds before experiments, after recording
the baseline, following the performance contract above. Gate each accepted phase
and the cumulative change; enabling an opt-in path is measured separately from
the legacy path with that feature off. Require no lost mandatory evidence,
scope/identity violation or hidden truncation. Compare indexing profiles independently from reply projections:
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
Recommendations acceptance includes useful one-index and combined-index subsets,
explicit omitted members, bounded advisory work, stale or inaccessible inputs,
safe saved defaults and actual-schema-valid copied MCP examples. Compare selected
subsets with all permitted indexes in original evaluation fixtures; record useful
evidence/recall and resource differences rather than asserting every narrower
query is better. Include concurrent management/query load and one selected index
in a large library to detect hidden all-library work.
Keep installed Windows, macOS and Linux qualification separate from unit fixtures;
unrun path/profile/editor scenarios remain unknown. Run the checks in
[CONTRIBUTING.md](../../CONTRIBUTING.md), keep routine CI lean and use broad
qualification only when explicitly requested. Review the complete diff and
validation evidence before corrective pushes; record whether review was independent.

Include the operational sequence's helper-outage, source-backlog, framing,
publication-ordering, setup/trust and signed-channel matrices in this campaign.
Their local regressions establish only the paths actually exercised. P6a's macOS
signed-copy/editor/agent gate and P14's immutable signed-producer/channel evidence
are required before claiming those distribution paths qualified; a successful
Windows P5 run cannot satisfy them. Publication verification occurs only after
separate authorization and checks the actual outputs, with honest failure/skip
records and no substitution of different same-version native bytes.

Use the selected Shark Tank requirements to deepen these same campaigns, not to
create a second release definition. Publish skip reasons, targeted coverage/risk
findings, frozen evaluation judgments, bounded hostile-input and space-failure
outcomes, and installed privacy/upgrade evidence in existing records. Report
unavailable metrics/platforms explicitly. Optional mutation, long-soak, million-
entry and broad scheduled campaigns remain outside required 1.9.0 acceptance;
any claim needing their evidence must be narrowed or withheld. Walk the updated
documentation entry paths and rehearse the release runbook without publication.

The original P0b checkpoint synchronized 1.9.0 after baseline capture. The
maintainer's 3 November decision now keeps **1.8.8** as the working version until
implementation and manual acceptance are complete; then promote to **1.9.0** and
rebuild before CI/release qualification. Before each phase commit and a later
feature/fix PR, verify the current native/extension locks and generated catalogs
agree. Do not repeat a version increment per phase or relabel earlier artifacts.
Follow the separate changed-candidate rule if distributing intermediate builds.
Update README, documentation index, release notes and
VSIX/Marketplace descriptions together when behavior lands, labeling unfinished
candidate capabilities accurately.
Keep these research notes dated and identifiable as proposals, and claim release
completion only after authorized merge/publication and verification succeed.

## Evidence boundary for this plan

This assessment reads the three original local research files, the new
cross-project feature design, current project guidance and the cited native/
extension verification and multi-project routing paths. The 3 November operational
integration reads both issue-analysis documents completely and spot-checks current
launcher, supervisor, trust and Workbench paths; it does not reproduce their
installed failures, settle the unexplained helper exits or qualify signed artifacts.
The selective Shark Tank integration reads its remediation draft and checks the
existing harness inventory against the remaining phases. It adopts no new audit
measurements, does not verify its broad absence claims, and runs no proposed
coverage, fuzz, scale, privacy or commercial comparison campaign during planning.
The phase refinement also
reviews current versioning/CI policy, the dated performance report, release
evidence and measurement harness capabilities;
it does not rerun the earlier live experiments, audit the Copilot implementation anew or
establish new performance/platform results. Their source-review and measured
findings are attributed above, while priorities, package boundaries and scope
decisions are recommendations made here. The earlier planning pass changed only
this plan and made no runtime, indexing-policy, registration, database, version
or Git-commit change. Subsequent implementation evidence belongs to the linked
ledger and candidate release record; it does not retroactively qualify research.
