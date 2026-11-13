# AIRK documentation

**Advanced split source checkpoint (13 November 2026).** P0 restores extracted
component test ownership and the `airkctl` one-shot command stack. The first P1
correction fences automatic scope switching/reselection and retains both
revocation obligations of an uncertain lease renewal. Exact-entry removal has a
prepared native contract; its handler is not served yet. Candidate version
remains 1.8.35; earlier installer/VSIX receipts retain their original
source identities. Progress and checks: [execution ledger](simplify/one-process-ledger.md).

**Split Phase 4 (unpublished, non-shippable interim).** `airk` is the service
only; `airk` with no arguments starts it in the background or says it is already
running. `airkctl` is the command-line client, with every surviving command
under the same names and arguments. The pre-split `runtime …`, `setup`,
`uninstall`, `host` and `service legacy …` operations are reachable only as
`airk __airk_role setup-interim <arguments>` until Phase 7. The installer and
extension are being reworked under the [split plan](split/split-plan.md)
(Phases 6–7). See [headless use](headless-cli.md) and the generated
[CLI reference](cli-reference.md).

**Stopped source checkpoint (10 November 2026).** Manual installation of the
1.8.23 candidate reported an HTTP cleanup refusal. The native eligibility fix
is committed but untested. No installer was rebuilt for that fix. Installer
repair, builds and qualification are stopped at the user's direction; retained
artifact receipts describe their original inputs.

**1.8.35 split candidate (13 November 2026).** Manifests and lockfiles are synchronized at 1.8.35 for the local Windows AIRK Service installer and VSIX. The Explorer's Management › Agents connects Claude Code, Codex or VS Code / GitHub Copilot with one click through an **open** connection whose default scope is the window's search scope and follows it; agent choice is off by default, so an agent reads only, or narrows within, its assigned scope (with choice on it can list and name any index, `list_indexes`). Search scope is the union of the checked index sets and indexes, indexes and index sets have descriptions, multi-index reads take `limit_per_index` and read their indexes concurrently, the Workbench can read named indexes and sets, and `airkctl` gains `agents connect --open`, `agents default`, `tools call --index/--set/--connection` and `library describe` (U104–U109). See the [1.8.35 record](releases/release-1.8.35.md). The 1.8.34 and earlier text below is dated history.

**1.8.34 split candidate (13 November 2026).** Manifests and lockfiles are synchronized at 1.8.34 for the local Windows AIRK Service installer and VSIX. A new index is readable at once (setup creates an index set named after it; older indexes get **Make readable** in AIRK Admin), AIRK Admin and the Explorer show live indexing progress, and the Explorer offers Index now, Pause/Resume, watching on/off, Retry and Cancel, usage and recent activity (U103). See the [1.8.34 record](releases/release-1.8.34.md). The 1.8.33 and earlier text below is dated history.

**1.8.33 split candidate (13 November 2026).** Manifests and lockfiles are synchronized at 1.8.33 for the local Windows AIRK Service installer and VSIX. Index setup works on a new installation (the installer creates its data and cache folders; refused setups say why), path fields have a **Browse…** folder dialog served by the service, and AIRK Admin's Agents, Usage, Activity and Help pages are readable, with manual MCP configuration for each agent. See the [1.8.33 record](releases/release-1.8.33.md). The 1.8.32 and earlier text below is dated history.

**1.8.32 split candidate (13 November 2026).** Manifests and lockfiles are synchronized at 1.8.32 for the local Windows AIRK Service installer and VSIX. 1.8.31 never created its index catalog: the service now creates it on its first start, so the Index library, AIRK Admin and index creation work on a new installation; `airkctl` commands also find the installed service without `--service-runtime-dir`. See the [1.8.32 record](releases/release-1.8.32.md). The 1.8.31, 1.8.30, 1.8.29, 1.8.27, 1.8.26, 1.8.25 and 1.8.24 text below is dated history.

**1.8.31 split candidate (12 November 2026).** Manifests and lockfiles are synchronized at 1.8.31 for the local Windows AIRK Service installer and VSIX that complete index creation: the admin Index library and Usage pages, `airkctl library` and service management commands, typed confirmation for destructive steps, Storage cleanup and Windows sign-in startup, window search scopes, and a read-only Explorer Indexing view with **Open AIRK Admin**; see the [1.8.31 record](releases/release-1.8.31.md). The 1.8.30, 1.8.29, 1.8.27, 1.8.26, 1.8.25 and 1.8.24 text below is dated history.

**1.8.30 split candidate (12 November 2026).** Manifests and lockfiles are synchronized at 1.8.30 for the rebuilt local Windows AIRK Service installer and VSIX (upgrades with VS Code open, setup's process scan, stdio agent-entry cleanup, downloads link); see the [1.8.30 record](releases/release-1.8.30.md). The 1.8.29, 1.8.27, 1.8.26, 1.8.25 and 1.8.24 text below is dated history.

**1.8.29 split candidate (12 November 2026).** Manifests and lockfiles are synchronized at 1.8.29 for the rebuilt local Windows AIRK Service installer and VSIX (stuck pre-split retirement taken over, installer progress page); see the [1.8.29 record](releases/release-1.8.29.md). 1.8.28 was not used; the 1.8.27, 1.8.26, 1.8.25 and 1.8.24 text below is dated history.

**1.8.27 split candidate (12 November 2026).** Manifests and lockfiles are synchronized at 1.8.27 for the rebuilt local Windows AIRK Service installer and VSIX (Management version line); see the [1.8.27 record](releases/release-1.8.27.md). The 1.8.26, 1.8.25 and 1.8.24 text below is dated history.

**1.8.26 split candidate (12 November 2026).** Manifests and lockfiles are synchronized at 1.8.26 for the rebuilt local Windows AIRK Service installer and VSIX (old-extension guard, Windows upgrade process scan); see the [1.8.26 record](releases/release-1.8.26.md). The 1.8.25 and 1.8.24 text below is dated history.

**1.8.25 split candidate (12 November 2026).** Manifests and lockfiles are synchronized at 1.8.25 for the rebuilt local Windows AIRK Service installer (pre-split retirement fix); see the [1.8.25 record](releases/release-1.8.25.md). The 1.8.24 text below is dated history.

**1.8.24 first split candidate (12 November 2026).** Manifests and lockfiles are synchronized at 1.8.24 for the local Windows AIRK Service installer and UI-only VSIX of the split; see the [1.8.24 record](releases/release-1.8.24.md). The 1.8.23 text below is dated history.

**1.8.23 Windows debug manual-test candidate:** the
[manual-test task ledger](simplify/one-process-ledger.md#manual-test-installer-correction-10-november-2026)
records the 1.8.22 Reconnect failure and the authorized repair. Reconnect must
not create indexes. Installations through 1.9.0 inclusive start empty after
verified owned uninstall; installing the same build also requires full uninstall.
Ordinary editor reopening only reconnects. The previous 1.8.22 artifact failed
this policy and retains its original evidence.

The [1.8.23 working record](releases/release-1.8.23.md) describes the correction
and its remaining gates. The Windows debug runtime, native ZIP and VSIX have been built and package-verified. Native/extension source manifests and lockfiles are synchronized at
1.8.23; scoped native and extension checks and package verification passed. Full release qualification remains pending.
The [one-process ledger](simplify/one-process-ledger.md) is the authoritative
implementation and evidence record. The
[release-readiness ledger](simplify/release-readiness.md) retains earlier
candidate outcomes. Installed-editor acceptance and
Linux/macOS qualification of 1.8.23 are pending. The earlier
[1.8.16 stop record](research/installer-stop-2026-11-09.md) retains its failed
installed acceptance. Merge and publication remain separate actions.

**One resident process (inherited from 1.8.22):**

- **The service is the only resident process.** Exactly one `airk.exe` stays
  resident per user and installation. Project owners, cleanup owners and the
  source watcher run inside it. A stuck owner is quarantined: its project
  reports it and other projects keep running until an explicit service
  restart.
- **Temporary processes.** These are one window helper per VS Code window
  (`airkctl session` after the split), CLI and installer one-shots, at most one PDF parser child and a
  user-opened `tools shell`.
- **Agents.** They connect directly to `https://127.0.0.1:<port>/mcp`. Claude
  Code and Codex receive their token from the one-shot
  `airkctl agents credential` helper.
- **Startup.** Login startup is registered on every install unless the user
  opted out, and the service never exits when idle.
- **Replacement.** Installation through 1.9.0 inclusive captures affected agent entries.
  Explorer then warns about broken entries and offers a one-confirmation
  **Reconnect**.

See [process ownership](process-model.md),
[agent setup](agent-integration.md#reconnecting-after-a-replacement) and
[installation](service-installation.md#replacing-an-installation-through-190).

**Future concept, not current behavior:** [AIRK Arbiter](arbiter/concept.md)
is a 10 November 2026 technical plan for an opt-in GPU decision tier. Nothing
in it is implemented. Its proposed sidecar process is not part of the current
process model.

**Simplification, 9 November 2026:** the [simplification plan](simplify-plan.md)
separates release stabilization (Track R) from restructuring (Track S).
Accepted simplification work through S7 is substantially complete; see the
[S1 review record](simplify/s1-review.md). It covers the
[compatibility inventory](simplify/compatibility-inventory.md), the
[contract-generation pilot](simplify/contract-generation-pilot.md) and the first
component contract,
[`install_files`](../crates/airk-host/src/install_files/CONTRACT.md). Those
structural steps changed no version, wire format or persisted state. Candidate
source versions are synchronized at 1.8.23 for the
installer build; the 1.8.16 `airk-state/1` baseline remains unchanged.

**Simplification hand-off (10 November 2026):** [docs/simplify/handoff-2026-11-10.md](simplify/handoff-2026-11-10.md)
summarizes the `feat/simplify` restructuring, its verification and the open
decisions.

**Documentation history:** dated reviews, superseded plans, phase logs, spikes
and experiments moved to [docs/history](history/README.md) in step D6a of the
simplification plan. They are kept unchanged apart from link updates and are
not current guidance. `python scripts/dev/d6_archive_docs.py --check` verifies
the moves, links and generated index.

The [1.8.16 working record](releases/release-1.8.16.md) retains the historical
8 November rebuild policy, superseded by the 10 November empty-install decision.
Its persisted `airk-state/1` format baseline remains 1.8.16. New installations
through 1.9.0 inclusive fully uninstall the owned predecessor and wait for
explicit index creation; source files remain untouched. The earlier rebuild
evidence does not qualify the new installer policy.

Historical r26/r29 packaging, checkpoint-overflow corrections and pre-baseline
migration experiments remain in the [1.8.14 record](releases/release-1.8.14.md)
and [fixture matrix](testing/fixture-compatibility.md). The authentic two-index
1.8.11 to 1.8.16 native reset has passed; installed-editor and broader release
qualification remain separate.

The [clean-break and rebuild contract](upgrade-rebuild-fallback.md) replaces the
optional fallback and historical index migration requirements. Native shutdown,
fresh-state admission, interrupted-reset recovery, client reconnection and exact
owned-artifact cleanup remain mandatory. Dated predecessor migration experiments
retain their results; they do not define the current pre-baseline upgrade path.

[Standalone CLI onboarding](agent-integration.md#standalone-onboarding-without-vs-code)
is implemented on this branch. Scoped CLI tests pass; actual client launch and
installed acceptance remain pending. The agent
guide and [local HTTPS API](local-https-api.md#obtain-a-connection) distinguish
reviewed scope/grant creation, native protected credentials, standalone process
environment/trust and the separate VS Code credential resolver.
Standalone Windows/macOS/Linux installer source installs only the CLI/native
server, with no VSIX or Code actions. Signing and published-artifact verification
remain pending; the extension is a separate optional distribution.
The [standalone Windows installer](../installers/windows/README.md) adds only
its literal verified runtime directory to user `PATH` after native setup succeeds.
Raw entries, registry type and manual paths are preserved; receipt-owned removal
follows confirmed successful native uninstall. This is separate from VSIX
terminal integration. Its 13 static/mocked checks do not execute Inno or registry
round-trips; ISCC was unavailable for this local check; real compilation/signing remain unqualified.

The [1.8.16 record](releases/release-1.8.16.md) retains passing native reset,
Windows extension and Mac component scopes separately from failed installed
editor runs. R38 mocked/native-enabled extension suites passed, but its UI run
failed on a sequence-16 response budget. R39's corrected budget and completeness
checks passed scoped regressions and actual one-/two-index native, CLI and
HTTP-MCP parity. The fresh R39 runtime and authentic two-index reset passed,
but all three R39 installed-editor attempts failed. The latest run stopped
on a definition-read cancellation before enrollment. No R39 Neo batch ran;
no public-release acceptance is implied.
Reopen terminals for installed CLI PATH changes; activate an old loaded
extension once with **Restart Extensions**.

The authorized follow-up to the completion plan's two-failure stop corrected the
shared native model-preparation fixture. Four cheap regressions, scoped Clippy,
separate actual worker/model setup and held/checkpoint preflights passed using
the unchanged original r26 debug runtime. Installed/platform qualification is
incomplete. The [8 November stop and repair note](research/one-airk-completion-stop-2026-11-08.md)
retains both original failures, the correction and unrun gates.

**1.8.23 builds on the unpublished One AIRK implementation; its Windows debug candidate is built and package-verified.** The [One AIRK implementation](one-airk.md) supplies the single-executable HTTPS runtime. The [1.8.23 working record](releases/release-1.8.23.md) tracks this correction. The [1.8.22 record](releases/release-1.8.22.md) retains its failed manual-test evidence; [1.8.20](releases/release-1.8.20.md), [1.8.19](releases/release-1.8.19.md) and [1.8.18](releases/release-1.8.18.md) are dated history. The 1.8.16 and 1.8.14 records retain historical implementation evidence. Earlier binaries keep their original versions and hashes.

[One AIRK implementation plan](one-airk.md) targets the **1.8.16+** development line with a single native
executable, embedded MiniLM, authenticated loopback MCP, automatic alternate-port
selection, compatible-line checkpointed upgrades and a clean break from older installations. It
defines local certificate trust, macOS bootstrap/management, a documented HTTPS
API for local tools and retained CLI/Workbench/MCP parity. The
[implementation ledger](research/one-airk-implementation.md) records source
integration, passed checks and outstanding native/platform gates separately.

**Explorer notices and updates:** dismiss update, connection and action-outcome notices with **×**. Dismissals are saved for the workspace and stay hidden across screens, Explorer reopen and window reload. Once the new loader is running, later detected AIRK extension replacements reload AIRK automatically and reopen its single Explorer panel when it was open. For the first transition from an older loaded extension, use **Restart Extensions** in VS Code’s Extensions pane to activate the new version. Installed-editor acceptance is recorded separately.

**Inherited 1.8.12 behavior:** Indexing has three tabs: **Search scope** selects local indexes and index sets with checkboxes; **Index library** creates and manages indexes, including **Delete index**; **Index sets** organizes reusable search groups. Choose indexes or sets and apply the selection; access details are handled internally. See the [historical 1.8.12 working-build record](releases/release-1.8.12.md) for behavior and validation. The earlier 1.8.11, 1.8.10 and 1.8.9 records retain their dated evidence.

The [MCP QA follow-up](reviews/mcp-qa-1.8.9.md) tracks the supplied 1.8.8
observations, confirmed defects, intentional contracts and current qualification.

Opening a trusted folder does not register or index it. Use **Indexing > Index library** to create an index with reviewed sources, storage and watching. In **Search scope**, check the indexes or index sets to search and apply the selection. Evidence reads and source verification do not start indexing.

Current [credential lifecycle corrections](local-https-api.md) reclaim bounded
active-ledger capacity after revocation and proven Explorer attachment retirement,
preserving active manual credentials, immutable ownership and ID-reuse fences.
The [all-fifteen client classification](agent-integration.md) separates implemented
serializers from actual connecting-process qualification. Current validation and
remaining platform gates are in the working-build record.

**Setup and uninstall:** Successful setup or reconnection clears obsolete failure warnings. Current setup failures remain visible, and uninstall removes empty shell startup files that AIRK created. See [setup and uninstall guidance](vscode-extension.md) and the [branch review](reviews/branch-v190-review-2026-11-06.md).

**Missing sources and storage:** the working source adds [reviewed index
retirement and cleanup](service-project-retirement.md) for removed files or folders.
The separate explicit legacy-index upgrade recovery and its **AIRK: Review
unavailable legacy upgrade index retirement** command were removed on
9 November 2026 with the in-place catalog upgrade; an older installation takes
the [clean break](service-installation.md#unavailable-legacy-index-during-upgrade)
instead. Healthy indexes keep their independent evidence; old grants are never silently narrowed. Post-P14
recovery has selected Windows native/process and Mac portable validation. Full
workspace and final installer qualification remain separate; see the working-build
record for current results and earlier failures.

| Current guide | Scope |
| --- | --- |
| [Local HTTPS API](local-https-api.md) | Unpublished 1.8.14 developer contract: discovery, local CA trust, scoped credentials, canonical JSON and MCP routes, concurrency and recovery |
| [Source verification and explicit reference refresh](source-verification.md) | Dated read-only verification; separately reviewed P12 refresh/requery and its limits |
| [Retained indexing profiles](code-indexing-options.md) | Explicit Lean/Rich migration, legacy meanings and independent replies |
| [Per-file family reporting](profile-family-reporting.md) | Actual applied profile and bounded availability observations |
| [Reviewed index splitting](index-splitting.md) | Independent destination, restrictive cutover and exact cleanup/recovery |
| [Permanent source exclusions](source-policy.md) | Native enforcement, removal publications and separate reclamation |
| [Combined Project reads](combined-reads.md) | Unique-index composition, scope and complete member results |
| [Recommendations](recommendations.md) | Bounded metadata advice and separately reviewed defaults |
| [Guarded index retirement](service-project-retirement.md) | Missing files/storage, current-reference cleanup and independent indexes |
| [Unavailable legacy index during the clean break](service-installation.md#unavailable-legacy-index-during-upgrade) | Reported unavailable sources; the explicit retirement commands were removed on 9 November 2026 |

The [1.8.8 working-build record](releases/release-1.8.8.md) and
[implementation ledger](research/implementation-v190.md) retain exact source/runtime
identities, historical failures and platform limits. [1.9.0](releases/release-1.9.0.md)
is the planned promotion target after implementation and manual acceptance.

The dated [1.8.7 external audit](issue-analysis/audit-report.md) and
[open-issue analysis](issue-analysis/open-issue-analysis.md) retain their reported
platform findings and source checkpoints. They do not qualify the current working
build or establish that the reported release issues have been fixed.

P1 repairs compact plan bindings/aggregation and nested object schemas; the
[interface contract](read-interface-parity.md#regression-evidence) records original
locations, explicit rich symbol-ID lookups, guards and transport/export coverage.

[Lean/Rich replies](reply-detail.md) explains independent per-read
projection, legacy defaults/flags, Workbench choices, CLI/MCP examples and limits.

[P3 design contracts](research/contracts-v190/README.md), [ADR 002](decisions/002-projects-indexes-and-query-scope.md), [ADR 003](decisions/003-dated-saved-source-verification.md) and the [profile-family inventory](research/profile-family-inventory-v190.md) define scope, ownership, upgrade and verification boundaries. Native activation is implemented for this local working build; final release and broader platform qualification remain separate from the selected local P14 results.

[Evidence guidance](evidence-guidance.md) describes bounded existing-observation
reasons, empty-result limits and inert next reads across CLI/MCP/Workbench.

**Inherited 1.8.7 behavior:** CLI and Workbench reads use the MCP handlers and retain their full results, including configured `query_concurrency` in status/diagnostics. Cold status preserves explicit setup-timeout receipts across later polls. This release consolidates all unpublished 1.8.0-1.8.6 storage, startup, retrieval, concurrency and Workbench improvements. See the [consolidated release notes](releases/release-1.8.7.md) for current behavior and dated validation.

Unreleased developer workflow: [native build reuse and Darwin failure diagnostics](ci-and-builds.md#native-build-reuse-and-darwin-failure-diagnostics) describes separate build/validation jobs, the complete-attempt release rule and focused repetitions. The historical Intel failure was corrected by the `0dbd2f0...` source and followed by green ARM/Intel main runs; those dated results do not qualify the current working tree. Current CI qualification remains pending. The 1.8.23 source manifests and lockfiles are synchronized; Windows debug compilation and packaging passed; CI and release qualification remain pending.

Developer references: [MCP tools](mcp-developer-guide.md) covers all 29 public tools with inputs, examples, evidence limits and implementation/test links. [CLI commands](cli-developer-guide.md) covers each command, its purpose, native ownership and guarded workflows. CLI `tools list/help/call/shell` and public MCP share the 29-tool unified catalog; the 19 native evidence/help/plan/scope operations are an internal subset. Separate administrative and offline Explorer commands retain their own authority and workflow.

The [CLI/MCP/Workbench parity audit](read-interface-parity.md) maps every native read, complete response/input contracts, conditional/continuity options and full versus selected exports. [Installed MCP checks](history/mcp-improvements-test-results.md#installed-187-acceptance--november-1-2026) record the 78-check Windows campaign, its permitted retry and the remaining navigation limits. Earlier 20-scenario investigations remain explicitly historical.

[Release records](releases/README.md) retains the consolidated 1.8.7 notes and dated development candidates. The [release workflow](release-workflow.md) describes exact-main CI, native signing, Marketplace promotion and published-output verification.

[Performance improvements and causal analysis](performance-improvements-1.8.2.md) retains the earlier installed-responsiveness measurements and separate native regressions. Those unmatched samples are not a controlled 1.8.7 speedup claim. [Workbench implementation](workbench/implementation.md) describes retained session/tool choices, query reveal, reading position, complete exports and local evidence parsing.

**1.7.6 stable release (published 2026-10-31):** [review 02 follow-up: setup review, native execution control, uninstall cleanup, data purge and advisory gate](releases/release-1.7.6.md). Published 1.7.3 fixed [Marketplace image links and release promotion](releases/release-1.7.3.md); published 1.7.2 introduced the [Workbench and Explorer interface](releases/release-1.7.2.md).

The [repository review response](reviews/reporeview01-response.md) records parser/dependency fixes, privacy guidance and the agreed deferral of existing-storage permission enforcement. The [review 02 response](reviews/review02-response.md) maps the later [review 02 drafts](reviews/review02/README.md) to their 1.7.6 outcomes. See also [release verification](release-verification.md) and [dependency advisories](dependency-advisories.md). The [CLI guide](cli.md) and generated [CLI reference](cli-reference.md) document every command. [Running AIRK headless](headless-cli.md) maps every extension feature to the CLI; `build.ps1`/`build.sh` produce an installation-free local build under `builds/`.

Workbench opens once after successful trusted setup. Its compact header provides history, settings and new-session icons; the composer’s **+** menu holds scope, recipes and tool forms. Explorer’s **Workbench** toolbar button, immediately left of Help, toggles secondary-sidebar visibility and shows a pressed state while visible. The project selector is available inside Overview and Management; its selection remains shared across Explorer screens. VS Code retains control of sidebar tab order. For the initial transition from older AIRK code, use **Restart Extensions** in VS Code’s Extensions pane to activate the new loader.

[Experiment with parser training in a fork](parser-training.md) explains the public role inventory, exact compressed corpora, verify/restore commands, whole-family split regeneration, offline fitting/evaluation and suggestion-only asset review. The [data catalog](../tools/parser-train/data/README.md) preserves historical inputs, hashes and source attribution.

**Workbench introduced in 1.7.2:** [implementation and validation](workbench/implementation.md), [target design and linked mockups](workbench/design.md). Direct evidence workflows and the agent-facing `parse` tool share one bounded native runner. The development catalog has 29 tools, including grant-bound scope discovery; the dated 1.6.4 record remains preserved.

**Version 1.6.4 — runtime upgrades, project cleanup and responsive diagnostics.** This public release consolidates the unpublished 1.6.1–1.6.3 branch work. Supported runtimes upgrade before connection checks; uninstall removes owned executables and recorded agent entries while preserving index data. Trusted open folders register and index automatically, subject to saved opt-out, pause and manual-update settings. Removing a project drains its workers and deletes its AIRK index and saved settings while preserving source files.

See [process ownership](process-model.md) for the current one-resident-process model, what a normal process snapshot looks like, and the dated launcher and multi-process history.

The [startup and tool optimization plan](history/optimizations.md) records the 1.7.6 status-timeout reproduction, cache/read inventories and return-preparation reviews of all 28 public MCP tools. Its phased execution ledger targets **1.8.0** on `feat/optimizations`, with local test/review/commit gates and AIRK/BigDotNetProject corpus comparisons. The target private-owner model moves deep integrity checks to publication and bounded maintenance; a final human-review section covers data reduction. Consult the ledger for implemented work; proposed targets are not performance guarantees.

The historical 1.6.4 catalog exposed 27 tools; the current catalog has 29. In the unpublished 1.8.14 implementation, CLI `tools list/help/call/shell` and public HTTPS share this catalog. `status` is a lightweight readiness check; `diagnostics` provides detailed runtime, job and watcher observations. Cold MCP verification survives individual status timeouts, so later polls reuse the same authenticated setup. CLI request-epoch preparation uses a bounded wait for explicit busy/uncertain registry responses and retains failure codes. macOS inference shutdown releases the owned ONNX environment after sessions drain, before native library cleanup. Agent connections follow their configured source folders across index replacement. Explorer bounds status polling, keeps local log actions responsive and reconciles uncertain requests internally without replaying them. Installation migrates registered historical default index roots to `index-v1`, preserving publications and settings; custom roots stay in place. See the [1.6.4 release notes](releases/release-1.6.4.md).

The [1.6.0 implementation record](releases/release-1.6.0.md) replaces FP32 inference with two bundled offline INT8 models, selects a model for the CPU, and rebuilds semantic coverage while preserving text and facts. See the [optimization rationale and measurements](semantic-index-optimization.md) and [build inputs and inference contract](onnx-models.md).

The [Guidance implementation plan](explorer/guidance-plan.md) is retained for future evaluation and implementation. Guidance tools, artifact management and the proposed workspace are not part of 1.6.4. The [Workbench implementation plan and mockups](workbench/design.md) also describe future work, not 1.6.4 features.

The [1.5.12 release record](releases/release-1.5.12.md) tracks optional agent instructions with copy/open-file actions, responsive Explorer startup, shared runtime discovery, live reconnect checks, visible MCP diagnostics and agent configuration updates against current native registrations. It also covers client-owned MCP process cleanup and recognizable installed worker names. The full inline agent setup guide remains proposed; previous release records retain their original validation scope.

Overview and Management share the [index dashboard](index-management.md): published content and semantic coverage first, followed by current source, semantic or cleanup work. Job counters and saved index totals remain distinct and update automatically.

The [1.5.10 candidate](releases/release-1.5.10.md) adds local raw failure evidence and fixes cleanup continuation under large object inventories and quota pressure. Its validation record distinguishes the fresh BigDotNetProject reproduction from the still-unavailable remote retained state.

[Storage formats](storage-formats.md) separates index/service compatibility identifiers from executable versions, describes fresh default paths and existing-location compatibility, and records the current cleanup and diagnostic evidence.

Published 1.6.4 uses [one unified MCP connection](unified-mcp.md), with 27 distinct tools, fixed project scope and guarded startup consolidation of recognized old server definitions. [Index management](index-management.md) documents progress, saved settings, recovery and diagnostics.

The [1.5.9 compatibility record](releases/release-1.5.9.md) covers discovery/fallback and independent simultaneous agent connections. The [1.5.8 update lifecycle record](releases/release-1.5.8.md) describes the single-action reload notice, explicit runtime installation stages and quiet success. [1.5.7](releases/release-1.5.7.md) retains its original candidate validation.

Published release: **1.7.6**. Management uses horizontal tabs, captured diagnostic report open/copy/save and full retained service log export. Completed runtime setup reopens quietly. Recovery checks the unfinished runtime marker, resumes outstanding source work after successful recovery and preserves failure budgets across new source hints; see the [1.5.7 development record](releases/release-1.5.7.md), [index management](index-management.md) and [automatic indexing recovery](index-self-healing.md). The [original recovery proposal](history/index-self-healing-proposal.md), [1.5.6 record](releases/release-1.5.6.md), [1.5.5 record](releases/release-1.5.5.md), [published 1.5.4 record](releases/release-1.5.4.md) and earlier records retain their original design, evidence and publication scope.

The 1.5.0 local Explorer candidate includes fourteen Assistance tools, paired project setup, a unified offline Help search, four role guides, purpose/format-based indexing guidance and About/community links. See [the extension guide](vscode-extension.md), [implementation receipts](explorer/code-review-action-plan.md), [role guidance](../packages/vscode/src/explorerHelp/role-developer.md), [indexing choices](../packages/vscode/src/explorerHelp/indexing-choices.md) and [document types](../packages/vscode/src/explorerHelp/document-types.md). These articles and the native engine guidance are also available directly in Explorer Help and through Assistance help tools.

The [1.4.4 release-tool repair](release-workflow.md#pinned-144-release-tool-repair) corrects macOS bundle verification and reuses the original artifacts verified by the successful routine CI run without rebuilding or relabeling native binaries. The original source commit and reviewed tooling commit are recorded separately.

The historical 1.4.4 follow-up corrects Unix cleanup ordering for naturally exited children while retaining exact child and descendant-exit proof. Routine CI keeps correctness/build/package checks, uses lightweight default features for focused Windows/macOS regressions, and bounds independent native fixture concurrency. Long workload and performance campaigns are available through explicit [CI qualification](ci-and-builds.md).

1.4.4 also creates staged Windows runtime directories with explicit current-user ownership and owner-only access, including nested payload directories, so elevated installations satisfy the same strict ownership checks. Installed macOS runtime validation, self-test and retirement read the complete app payload from its outer root. Cold service starts coordinate registration-metadata setup before source watching.

1.4.4 scales WAL/shared-memory allowances with admitted database capacity, preserves preceding-budget reads during recovery, and sizes growth using logical SQLite pages. See the [release record](releases/release-1.4.4.md) and [reliability review, including Turso and other embedded engines](history/storage-reliability.md).

Installation and upgrades now display a VS Code progress notification with the current stage: version and payload checks, staging, native activation, agent connection updates, workspace setup and cleanup. Progress uses actual stages without an invented percentage. On Windows and Linux, an admitted upgrade hard-stops verified preceding AIRK processes and disconnects their clients; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

Successful runtime activation finishes quietly. Once its loader is active, AIRK handles later detected extension replacements automatically; external coding-agent sessions may need their own restart to load the new tool connection. Connection or cleanup issues produce a specific warning and details instead of a clean success message.

**Historical pre-baseline behavior:** The 1.8.14 working installer and **Install or Repair CLI** migrate recognized owned workspace connections for the supported Codex (`.codex/config.toml`), Claude Code (`.mcp.json`) and VS Code/Copilot Chat (`.vscode/mcp.json`) adapters to the authenticated native HTTPS endpoint. After complete known configuration migration, native ownership checks retire old launcher images, their processes and eligible approved superseded runtime executables. That historical migration preserved selected runtime, sources, indexes and settings. That earlier pre-baseline rebuild is superseded by the current empty-install policy through 1.9.0 inclusive. New connections use HTTPS directly. Other servers, disabled settings and tool policies remain intact; missing entries do not enable an agent. Unsupported clients, unproven/custom declarations, ambiguous scopes, unsaved edits, unsafe files or blocked cleanup leave installation explicitly incomplete, rather than reporting clean success. Actual installed-client/platform qualification remains separate; see [agent setup](agent-integration.md).

The current branch prepares **1.8.23 as the next local candidate**, retaining **1.8.16 as the persisted format baseline** and building on the unpublished One AIRK implementation. Source manifests and lockfiles are synchronized at 1.8.23; the Windows debug native package and VSIX are built and package-verified. Public **1.9.0** remains a future release target. The paragraphs above and below about 1.4.x describe earlier releases. [Routine CI](ci-and-builds.md) runs the full workspace once on Linux, focused native regressions elsewhere, all platform builds/package smoke and a container smoke. Full platform/container suites and installed-runtime lifecycle campaigns are explicit qualification. Historical 1.4.2 introduced Balanced admission at one quarter of detected RAM (2–32 GiB), protection for up to four project query workers, and source pacing of 64 MiB/s per job within a 256 MiB/s shared ceiling. Native watching supports 8,192 directories per project and 32,768 shared, with bounded incremental traversal and specific limit diagnostics. These are admission policies, not hard OS memory or disk-I/O limits. Follow the [versioning process](versioning.md) and current working record for qualification and publication evidence.

The [MCP contract](mcp.md) distinguishes raw unavailable diagnostics, bounded publication/readiness observations and typed exact-request retries. Qualification clients preserve these distinctions and exact submitted-query accounting.

The [MCP contract](mcp.md) also covers `help` (native usage topics, native parser-generated CLI help and current tool schemas) and `diagnostics.work` (scoped progress/jobs, bounded queues, watcher status and matched waits). The seventeen native read-only tools remain read-only; the unified catalog also advertises explicitly reviewed UI drafts.

The [MCP contract](mcp.md) documents `diagnostics.runtime`, including running frontend/worker/service versions, process and payload identities, build receipt metadata and explicit missing observations.

**Historical compatible-line cleanup:** Successful CLI installation removes obsolete terminal executables and eligible approved nonselected runtime payloads regardless of version, after activation and complete known-client configuration migration. Locked files report incomplete cleanup; a later trusted activation or **Install or Repair CLI** resumes the exact transaction. The selected runtime, sources, indexes and settings are preserved. See the [console installation contract](service-console.md).

Trusted activation can install or reconnect the approved runtime and terminal
command, then reconcile existing native registrations. It does not register opened
folders, start a new index or grant evidence access. New source/storage/watching
choices require [reviewed index setup](service-project-setup.md). Choose local indexes
and index sets with **Search scope** checkboxes and apply the selection; the host
handles native access details internally. `airk.cli.autoInstall` controls
automatic runtime setup, not indexing authority. Existing accepted work, explicit
Stop, pause and watching preferences remain native. Open a new terminal after PATH
integration; external terminal applications may need restarting.

**AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents**

The **1.4.4 release** corrects WAL admission and logical database sizing. Earlier 1.4.0/1.4.1/1.4.2 qualification remains historical. Start with the [historical 1.4.4 release record](releases/release-1.4.4.md), [service plan](history/airkservice-architecture.md), [completion checklist](history/service-implementation-checklist.md), [project setup](service-project-setup.md) and [client adapters](service-client-adapters.md). That historical record did not qualify native Mac or the complete installed-platform/signing/policy matrix; current P6 evidence is recorded in the implementation ledger.

The 1.3.x/1.2.x release records and former direct-helper/window-scheduler guides are historical references. They do not describe the 1.4 default lifecycle or authorize opening legacy stores through the service. Native/extension source and lockfile versions are synchronized at **1.8.23**; the Windows debug candidate is built and package-verified. Earlier tested builds retain their original identity. Packaging, manual acceptance, later promotion and publication require their own evidence.

The [local Rust iteration guide](../CONTRIBUTING.md#local-rust-iteration) covers scoped Clippy, persistent targets, platform-specific development incremental settings and timing reports; full qualification remains required.

The [Windows build/editor guide](windows-local-build.md) describes pair builds, stage-only acceptance without a VSIX, and real native registration/usage/response-mode checks in isolated editor profiles.

- [1.6.4 public release: upgrades, removal, MCP status and migration](releases/release-1.6.4.md)
- [Historical unpublished 1.6.1–1.6.3 candidate evidence](releases/release-1.6.3.md)
- [Architecture and code boundaries](architecture.md)
- [AIRK Explorer design inputs and approved 1.5.0 design](explorer/README.md): historical design records for the shipped workbench. Current native evidence and optional Explorer assistance share the [unified MCP endpoint](unified-mcp.md).
- [AIRK 1.4.0 local service architecture and implementation plan](history/airkservice-architecture.md)
- [1.5.9 MCP compatibility record](releases/release-1.5.9.md)
- [1.5.8 update and management record](releases/release-1.5.8.md)
- [Earlier 1.5.6 development record](releases/release-1.5.6.md)
- [Earlier 1.5.5 development record](releases/release-1.5.5.md)
- [Published 1.5.4 release record](releases/release-1.5.4.md)
- [1.5.3 local candidate record](releases/release-1.5.3.md)
- [1.5.0 Explorer release record](releases/release-1.5.0.md)
- [Previous 1.4.4 release record](releases/release-1.4.4.md)
- [Historical 1.4.3 storage-recovery candidate](releases/release-1.4.3.md)
- [Storage reliability and embedded engine review](history/storage-reliability.md)
- [Historical 1.4.2 resource-policy candidate](releases/release-1.4.2.md)
- [Historical 1.4.1 upgrade candidate](releases/release-1.4.1.md)
- [Versioning from local install through CI and publication](versioning.md)
- [Historical 1.4.0 candidate and qualification limits](releases/release-1.4.0.md)
- [Offline service installation, resumable activation, managed payloads and startup ownership](service-installation.md) Shutdown ownership checks also cover damaged payload files, and initialization receipts prevent silent registry replacement after control loss.
- [Reviewed runtime upgrades, rollback, saved activation and version pins](service-runtime-transitions.md)
- [Mac signed service bundle, optional login ownership and qualification limits](history/service-macos-login.md)
- [Reviewed project setup, reserved storage and stable-launcher MCP bindings](service-project-setup.md)
- [Explicit legacy agent-entry review, startup removal and old derived-file retirement](service-legacy-transition.md)
- [Project removal, automatic cleanup and historical retirement contracts](service-project-retirement.md)
- [Direct service client adapters and durable CLI request recovery](service-client-adapters.md)
- [Direct usage/settings reads, scan-limit reset and guarded project management](service-native-management.md)
- [Default-cutover development qualification and remaining gates](history/service-cutover-qualification.md)
- [Service implementation completion checklist](history/service-implementation-checklist.md)
- [Complete service-plan requirements review and qualification boundaries](history/service-requirements-review.md)
- [Service publication, admitted sidecar lifetimes and bounded readers](service-publication.md)
- [Service worker ownership, source/semantic phases and durable continuations](service-jobs.md)
- [Guarded service project configuration, bounded results and uncertain-outcome recovery](service-configuration.md)
- [Service job progress, phase timing and stall handling](service-progress.md)
- [Retained service events, bounded paging and explicit retry](service-events.md)
- [Cached project inspection and maintenance guards](service-inspection.md)
- [Durable service maintenance plans](service-maintenance.md): guarded previews, frozen scope, approved execution and partial-failure receipts.
- [Vector regeneration from retained evidence](service-vector-regeneration.md): forced recomputation, bounded turns and atomic citation/node replacement.
- [Bounded process logs and down-service diagnostics](service-logging.md). Native placement is bound to install/state ownership and source isolation.
- [Native service brokerage and direct query protocol](service-protocol.md)
- [Service resource admission, presets and observations](service-resources.md), including one shared per-worker materialization budget through client cancellation.
- [Service provider lifetime, surviving Unix guardians and durable crash recovery](service-provider-lifetime.md)
- [Native process ownership and provider cleanup](service-processes.md)
- [MCP response modes, minimal navigation and fact result keys](mcp-response-modes.md)
- [Indexed call candidates, read retries and physical repacks](service-retrieval.md)
- [Retained indexed changes and `index-current` validation](service-change-retention.md)
- [Service storage ownership](service-storage-ownership.md) — separate 1.4 quota, staging, lifecycle and recovery ownership, with one durable registry authority per data root.
- [Fenced cached service source freshness](service-freshness.md) Includes unavailable-source reasons and retained reads.
- [Service-owned saved-source watching and reconciliation](service-source-watching.md)
- [Shared embedding runtime, global offline model/calibration inputs and resource bounds](service-embedding.md)
- [October 22 service publication mechanism spike](history/service-publication-spike.md)
- [Historical architecture v2 implementation plan](history/architecture-v2.md)
- [Proposed change tracing and bounded development indexes](history/change-tracing-plan.md)
- [Decision 001: focused local retrieval](decisions/001-local-retrieval-kernel.md)
- [Design inspiration and independent implementation](inspiration.md)
- [Implemented format support](formats.md)
- [Ingestion limits, machine sizing and index detail](ingestion-limits.md)
- [Per-collection extraction policy](extraction-policy.md)
- [Measured scale and performance qualification](scale-qualification.md)
- [October 21 indexing feedback review, implementation and measurement](history/indexing-speed-review-2026-10-21.md)
- [October 21 follow-up: durable rebuilds, exact completion reports and guarded lexical cache](history/indexing-follow-up-2026-10-21.md)
- [Historical 1.2 indexing optimization and qualification record](history/indexingenhancements.md)
- [Current-index sharding, Tantivy-to-FTS5 rationale and storage alternatives](history/index-sharding-review.md)
- [Document requirements, dates and design indexing](document-indexing.md)
- [Code-indexing recommendations and optional semantic-provider review](code-indexing-options.md)
- [Implemented SQLite code facts, relationships and SCIP imports](code-facts-and-relationships.md)
- [Collections and selected DiskANN architecture](collections.md)
- [Shared storage quotas and reservation limits](storage-quotas.md)
- [Current-object cleanup and retired history controls](retention.md)
- [Historical v2 design: content and checkpoints, not available in 1.2](history/v2-history-controls.md)
- [Transactional storage relocation and recovery](storage-relocation.md)
- [Additional source folders and revision-bound aliases](source-roots.md)
- [Explicit local model assets and vector/hybrid retrieval](local-models.md)
- [Native CPU capability detection and pinned INT8 model profiles](onnx-models.md#bundled-profiles)
- [Semantic index optimization: rationale, offline models and measured tradeoffs](semantic-index-optimization.md)
- [Durable native scheduling and crash recovery](native-scheduler.md)
- [MCP tool contracts](mcp.md)
- [1.3.1 independent project workers](releases/release-1.3.1.md)
- [1.3.2 empty-index progressive publication fix and validation](releases/release-1.3.2.md)
- [1.3.0 changes and qualification limits](releases/release-1.3.0.md)
- [Historical 1.2.5 local candidate and dated validation](releases/release-1.2.5.md)
- [MCP tool retirement in 1.3.0](history/unreleased-mcp-tool-retirement.md)
- [Request execution, bounded admission and verification costs](request-execution.md)
- [Code review findings and 1.1.4 implementation record](history/airkimprovements.md)
- [Source, parser and provider hardening](ingest-hardening.md)
- [Resumable inventory/fact discovery and compact outlines](discovery.md)
- [Coding-agent setup compatibility research](agent-integration.md)
- [AIRK and Copilot search: implementation comparison and lessons](research/copilot-search-analysis.md)
- [Two-tier indexing and reply detail: live MCP evaluation and implementation options](research/two-tier-indexing.md)
- [VS Code folders, workspaces and AIRK project identity — 2 November 2026 research and proposal](research/vs-code-folders-workspaces.md)
- [AIRK 1.9.0 research assessments, decisions and delivery plan — proposal, 2 November 2026](research/plan-v190.md)
- [AIRK 1.9.0 implementation ledger — phase status, baseline identities and observed validation](research/implementation-v190.md)
- [Projects, shared indexes and cross-project queries — feature design and upgrade path, 2 November 2026](research/cross-project-indexing.md)
- [Google Antigravity manual MCP setup](antigravity-mcp.md)
- [Coding-agent tool alignment and online documentation review](history/agent-tool-alignment.md)
- [Agent Lean index modes and MCP efficiency plan](history/mcp-prioritization.md)
- [2026-11-01 agent-side MCP investigation: query log, admission, ranking and tool-description recommendations](history/mcp-recommendations.md)
- [1.2.4 diagnostics, structured contention retries, helper identity and bounded batch recovery](releases/release-1.2.4.md)
- [1.2.3 Agent Lean default and indexing fix](releases/release-1.2.3.md)
- [1.2.2 semantic indexing fix](releases/release-1.2.2.md)
- [1.2.1 documentation update](releases/release-1.2.1.md)
- [1.2.0 current-index, Agent Lean and compression behavior](releases/release-1.2.0.md)
- [Workspace indexing coverage and clickable footer](index-coverage.md)
- [Native VS Code extension and limits](vscode-extension.md)
- [Extension listing content, agent integration copy and illustrations](history/extension-content.md)
- [1.0.0 stable release notes and publication checklist](releases/release-1.0.0.md)
- [Sidebar PNG renderings](ui/README.md)
- [Usage & Savings PNG previews for 1.1.3](ui/usage-reporting.md)
- [October 17 combined release notes: 1.1.0–1.1.2](releases/releases-2026-10-17.md)
- [1.1.3 implementation and qualification: Usage & Savings](releases/release-1.1.3.md)
- [Historical 1.1.4 record: reliable usage comparisons](releases/release-1.1.4.md)
- [Measuring AIRK's value: sessions, evaluation and overhead](usage-evaluation.md)
- [1.1.2 implementation and qualification: coding-agent connections](releases/release-1.1.2.md)
- [1.1.1 implementation and qualification: dashboard, storage and recovery](releases/release-1.1.1.md)
- [1.1.0 implementation and qualification: workstation sizing](releases/release-1.1.0.md)
- [Historical 0.1.0 release review and test checklist](releases/release-checklist-0.1.0.md)
- [0.1.0 implementation review and remaining roadmap work](history/review-0.1.0.md)
- [Main branch protection and manual extension publication](release-workflow.md)
- [Build and review Windows app/extension locally](windows-local-build.md)
- [Actual Explorer hot-update, singleton and dismissal qualification](reviews/explorer-hot-update-qualification.md)
- [Extension deployment adapted from VCP](extension-deployment.md)
- [GitHub CI, native artifacts, credentials and spending controls](ci-and-builds.md)
- [Operations and recovery](operations.md)
- [Native macOS build and release instructions](macos-build.md)
- [Implementation roadmap](roadmap.md)
- [Language popularity and remaining native adapter gaps — 27 October 2026 research](history/language-gaps.html)
- [Observed validation and historical receipts](history/validation.md)
- [Local usage, performance and estimated context reduction](usage-reporting.md)

Contributor guidance and offline-first bootstrap: [CONTRIBUTING.md](../CONTRIBUTING.md).
The [ignored-test/profile-exclusion inventory](testing/ignore-triage.md) separates
helper entry points, gated qualification and actual runner evidence. Implemented,
proposed and unqualified behavior must remain distinguishable.

Development helper recovery now preserves project workers and rebinds retained direct model sessions with bounded retries and explicit stop/start controls; see [shared embedding](service-embedding.md). Default adapter cutover and installed-platform qualification remain open.

Native project controls now expose cached worker identity, queued restart and startup-circuit state. Explicit restart drains the expected worker before replacement, preserves other projects and supports retained evidence when sources are unavailable. See [native service control](service-protocol.md). Default client cutover and installed-platform qualification remain open.

Development console code now presents cached project/work observations, guarded repair previews, exact request recovery and redacted diagnostic previews. Native global lifecycle uses durable instance-fenced stop/restart intent and an explicit-stop preference. Default adapter cutover, complete configuration and installed/editor qualification remain in progress. See [the console foundation and development receipts](service-console.md).

Initialized registry recovery now refuses missing/empty databases and missing required tables, preserving accepted request and fencing identity. See [durable registry ownership](service-jobs.md).

Installed namespace binding now independently pins the completed registry identity outside the control directory; losing both registry files refuses startup without creating another authority. See the [installation development contract](service-installation.md).

Saved bootstrap snapshots now read completed registry metadata without executing a damaged selected payload or migrating/pruning the registry. The console retains bounded cached project pages and logs while disconnected, labels sampled work explicitly, and can submit a saved-instance Stop through the independently verified bundled main executable. Native Stop rechecks the actual service identity; Restart/Start still require a verified complete payload. See [down-service behavior](service-console.md#down-service-and-damaged-payload-observation).

Development payload repair now requires an exact native preview and stopped owners, preserves the complete damaged slot, resumes interrupted copies and returns an idempotent completion receipt. The trusted console uses the matching approved offline bundle; Start remains explicit and uninstall retains the damaged originals. See [payload repair](service-installation.md#explicit-payload-repair).

The [Explorer code review and implementation follow-up](explorer/code-review-action-plan.md) records the chronological branch audit, corrective work, local validation and remaining qualification.

The [1.5.12 process changes](service-installation.md#process-lifetime-and-recognizable-names) cover client-owned MCP cleanup, distinct installed worker names and single-engine downloads.
- [Admin pages](admin-pages.md): the local management web interface (`airkctl admin open`), its sign-in and what this version serves.
