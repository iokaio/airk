# AIRK documentation

**Explorer notices and updates:** dismiss update, connection and action-outcome notices with **×**. Dismissals are saved for the workspace and stay hidden across screens, Explorer reopen and window reload. Once the new loader is running, later detected AIRK extension replacements reload AIRK automatically and reopen its single Explorer panel when it was open. For the first transition from an older loaded extension, use **Restart Extensions** in VS Code’s Extensions pane to activate the new version. Installed-editor acceptance is recorded separately.

**1.8.12 is an unpublished manual-test working build.** Indexing has three tabs: **Search scope** selects local indexes and index sets with checkboxes; **Index library** creates and manages indexes, including **Delete index**; **Index sets** organizes reusable search groups. Choose indexes or sets and apply the selection; access details are handled internally. See the [1.8.12 working-build record](releases/release-1.8.12.md) for behavior and validation. The earlier 1.8.11, 1.8.10 and 1.8.9 records retain their dated evidence.

The [MCP QA follow-up](reviews/mcp-qa-1.8.9.md) tracks the supplied 1.8.8
observations, confirmed defects, intentional contracts and current qualification.

Opening a trusted folder does not register or index it. Use **Indexing > Index library** to create an index with reviewed sources, storage and watching. In **Search scope**, check the indexes or index sets to search and apply the selection. Evidence reads and source verification do not start indexing.

**Setup and uninstall:** Successful setup or reconnection clears obsolete failure warnings. Current setup failures remain visible, and uninstall removes empty shell startup files that AIRK created. See [setup and uninstall guidance](vscode-extension.md) and the [branch review](reviews/branch-v190-review-2026-11-06.md).

**Missing sources and storage:** the working source adds [reviewed index
retirement and cleanup](service-project-retirement.md) for removed files or folders and separate
[unavailable legacy-index upgrade recovery](service-installation.md#unavailable-legacy-index-during-upgrade). Healthy indexes keep
their independent evidence; old grants are never silently narrowed. Post-P14
recovery has selected Windows native/process and Mac portable validation. Full
workspace and final installer qualification remain separate; see the working-build
record for current results and earlier failures.

| Current guide | Scope |
| --- | --- |
| [Source verification and explicit reference refresh](source-verification.md) | Dated read-only verification; separately reviewed P12 refresh/requery and its limits |
| [Retained indexing profiles](code-indexing-options.md) | Explicit Lean/Rich migration, legacy meanings and independent replies |
| [Per-file family reporting](profile-family-reporting.md) | Actual applied profile and bounded availability observations |
| [Reviewed index splitting](index-splitting.md) | Independent destination, restrictive cutover and exact cleanup/recovery |
| [Permanent source exclusions](source-policy.md) | Native enforcement, removal publications and separate reclamation |
| [Combined Project reads](combined-reads.md) | Unique-index composition, scope and complete member results |
| [Recommendations](recommendations.md) | Bounded metadata advice and separately reviewed defaults |
| [Guarded index retirement](service-project-retirement.md) | Missing files/storage, current-reference cleanup and independent indexes |
| [Unavailable legacy-index upgrade](service-installation.md#unavailable-legacy-index-during-upgrade) | Explicit same-candidate retirement before activation; complete native receipts |

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

Unreleased developer workflow: [native build reuse and Darwin failure diagnostics](ci-and-builds.md#native-build-reuse-and-darwin-failure-diagnostics) describes the separate build/validation jobs, complete-attempt release rule, focused repetitions and remaining Intel failure. Product versions remain synchronized at 1.8.12 for manual testing before later 1.9.0 promotion.

Developer references: [MCP tools](mcp-developer-guide.md) covers all 29 public tools with inputs, examples, evidence limits and implementation/test links. [CLI commands](cli-developer-guide.md) covers each command, its purpose, native ownership and guarded workflows. Both explain why `airk tools list` has 19 native tools while unified MCP has 29, and map the remaining capabilities to separate terminal commands.

The [CLI/MCP/Workbench parity audit](read-interface-parity.md) maps every native read, complete response/input contracts, conditional/continuity options and full versus selected exports. [Installed MCP checks](mcp-improvements-test-results.md#installed-187-acceptance--november-1-2026) record the 78-check Windows campaign, its permitted retry and the remaining navigation limits. Earlier 20-scenario investigations remain explicitly historical.

[Release records](releases/README.md) retains the consolidated 1.8.7 notes and dated development candidates. The [release workflow](release-workflow.md) describes exact-main CI, native signing, Marketplace promotion and published-output verification.

[Performance improvements and causal analysis](performance-improvements-1.8.2.md) retains the earlier installed-responsiveness measurements and separate native regressions. Those unmatched samples are not a controlled 1.8.7 speedup claim. [Workbench implementation](workbench/implementation.md) describes retained session/tool choices, query reveal, reading position, complete exports and local evidence parsing.

**1.7.6 stable release (published 2026-10-31):** [review 02 follow-up: setup review, native execution control, uninstall cleanup, data purge and advisory gate](releases/release-1.7.6.md). Published 1.7.3 fixed [Marketplace image links and release promotion](releases/release-1.7.3.md); published 1.7.2 introduced the [Workbench and Explorer interface](releases/release-1.7.2.md).

The [repository review response](reviews/reporeview01-response.md) records parser/dependency fixes, privacy guidance and the agreed deferral of existing-storage permission enforcement. The [review 02 response](reviews/review02-response.md) maps the later [review 02 drafts](reviews/review02/README.md) to their 1.7.6 outcomes. See also [release verification](release-verification.md) and [dependency advisories](dependency-advisories.md). The [CLI guide](cli.md) and generated [CLI reference](cli-reference.md) document every command. [Running AIRK headless](headless-cli.md) maps every extension feature to the CLI; `build.ps1`/`build.sh` produce an installation-free local build under `builds/`.

Workbench opens once after successful trusted setup. Its compact header provides history, settings and new-session icons; the composer’s **+** menu holds scope, recipes and tool forms. Explorer’s **Workbench** toolbar button, immediately left of Help, toggles secondary-sidebar visibility and shows a pressed state while visible. The project selector is available inside Overview and Management; its selection remains shared across Explorer screens. VS Code retains control of sidebar tab order. For the initial transition from older AIRK code, use **Restart Extensions** in VS Code’s Extensions pane to activate the new loader.

[Experiment with parser training in a fork](parser-training.md) explains the public role inventory, exact compressed corpora, verify/restore commands, whole-family split regeneration, offline fitting/evaluation and suggestion-only asset review. The [data catalog](../tools/parser-train/data/README.md) preserves historical inputs, hashes and source attribution.

**Workbench introduced in 1.7.2:** [implementation and validation](workbench/implementation.md), [target design and linked mockups](workbench/design.md). Direct evidence workflows and the agent-facing `parse` tool share one bounded native runner. The development catalog has 29 tools, including grant-bound scope discovery; the dated 1.6.4 record remains preserved.

**Version 1.6.4 — runtime upgrades, project cleanup and responsive diagnostics.** This public release consolidates the unpublished 1.6.1–1.6.3 branch work. Supported runtimes upgrade before connection checks; uninstall removes owned executables and recorded agent entries while preserving index data. Trusted open folders register and index automatically, subject to saved opt-out, pause and manual-update settings. Removing a project drains its workers and deletes its AIRK index and saved settings while preserving source files.

See [process counts, launcher overhead and ownership](process-model.md) for why one editor window can show multiple AIRK processes and how to distinguish live connections from lifecycle failures.

The [startup and tool optimization plan](optimizations.md) records the 1.7.6 status-timeout reproduction, cache/read inventories and return-preparation reviews of all 28 public MCP tools. Its phased execution ledger targets **1.8.0** on `feat/optimizations`, with local test/review/commit gates and AIRK/BigDotNetProject corpus comparisons. The target private-owner model moves deep integrity checks to publication and bounded maintenance; a final human-review section covers data reduction. Consult the ledger for implemented work; proposed targets are not performance guarantees.

The historical 1.6.4 catalog exposed 27 tools; the current catalog has 28. `status` is a lightweight readiness check; `diagnostics` provides detailed runtime, job and watcher observations. Cold MCP verification survives individual status timeouts, so later polls reuse the same authenticated setup. CLI request-epoch preparation uses a bounded wait for explicit busy/uncertain registry responses and retains failure codes. macOS inference shutdown releases the owned ONNX environment after sessions drain, before native library cleanup. Agent connections follow their configured source folders across index replacement. Explorer bounds status polling, keeps local log actions responsive and reconciles uncertain requests internally without replaying them. Installation migrates registered historical default index roots to `index-v1`, preserving publications and settings; custom roots stay in place. See the [1.6.4 release notes](releases/release-1.6.4.md).

The [1.6.0 implementation record](releases/release-1.6.0.md) replaces FP32 inference with two bundled offline INT8 models, selects a model for the CPU, and rebuilds semantic coverage while preserving text and facts. See the [optimization rationale and measurements](semantic-index-optimization.md) and [build inputs and inference contract](onnx-models.md).

The [Guidance implementation plan](explorer/guidance-plan.md) is retained for future evaluation and implementation. Guidance tools, artifact management and the proposed workspace are not part of 1.6.4. The [Workbench implementation plan and mockups](workbench/design.md) also describe future work, not 1.6.4 features.

The [1.5.12 release record](releases/release-1.5.12.md) tracks optional agent instructions with copy/open-file actions, responsive Explorer startup, shared runtime discovery, live reconnect checks, visible MCP diagnostics and agent configuration updates against current native registrations. It also covers client-owned MCP process cleanup and recognizable installed worker names. The full inline agent setup guide remains proposed; previous release records retain their original validation scope.

Overview and Management share the [index dashboard](index-management.md): published content and semantic coverage first, followed by current source, semantic or cleanup work. Job counters and saved index totals remain distinct and update automatically.

The [1.5.10 candidate](releases/release-1.5.10.md) adds local raw failure evidence and fixes cleanup continuation under large object inventories and quota pressure. Its validation record distinguishes the fresh BigDotNetProject reproduction from the still-unavailable remote retained state.

[Storage formats](storage-formats.md) separates index/service compatibility identifiers from executable versions, describes fresh default paths and existing-location compatibility, and records the current cleanup and diagnostic evidence.

Published 1.6.4 uses [one unified MCP connection](unified-mcp.md), with 27 distinct tools, fixed project scope and guarded startup consolidation of recognized old server definitions. [Index management](index-management.md) documents progress, saved settings, recovery and diagnostics.

The [1.5.9 compatibility record](releases/release-1.5.9.md) covers discovery/fallback and independent simultaneous agent connections. The [1.5.8 update lifecycle record](releases/release-1.5.8.md) describes the single-action reload notice, explicit runtime installation stages and quiet success. [1.5.7](releases/release-1.5.7.md) retains its original candidate validation.

Published release: **1.7.6**. Management uses horizontal tabs, captured diagnostic report open/copy/save and full retained service log export. Completed runtime setup reopens quietly. Recovery checks the unfinished runtime marker, resumes outstanding source work after successful recovery and preserves failure budgets across new source hints; see the [1.5.7 development record](releases/release-1.5.7.md), [index management](index-management.md) and [automatic indexing recovery](index-self-healing.md). The [original recovery proposal](index-self-healing-proposal.md), [1.5.6 record](releases/release-1.5.6.md), [1.5.5 record](releases/release-1.5.5.md), [published 1.5.4 record](releases/release-1.5.4.md) and earlier records retain their original design, evidence and publication scope.

The 1.5.0 local Explorer candidate includes fourteen Assistance tools, paired project setup, a unified offline Help search, four role guides, purpose/format-based indexing guidance and About/community links. See [the extension guide](vscode-extension.md), [implementation receipts](explorer/code-review-action-plan.md), [role guidance](../packages/vscode/src/explorerHelp/role-developer.md), [indexing choices](../packages/vscode/src/explorerHelp/indexing-choices.md) and [document types](../packages/vscode/src/explorerHelp/document-types.md). These articles and the native engine guidance are also available directly in Explorer Help and through Assistance help tools.

The [1.4.4 release-tool repair](release-workflow.md#pinned-144-release-tool-repair) corrects macOS bundle verification and reuses the original artifacts verified by the successful routine CI run without rebuilding or relabeling native binaries. The original source commit and reviewed tooling commit are recorded separately.

The historical 1.4.4 follow-up corrects Unix cleanup ordering for naturally exited children while retaining exact child and descendant-exit proof. Routine CI keeps correctness/build/package checks, uses lightweight default features for focused Windows/macOS regressions, and bounds independent native fixture concurrency. Long workload and performance campaigns are available through explicit [CI qualification](ci-and-builds.md).

1.4.4 also creates staged Windows runtime directories with explicit current-user ownership and owner-only access, including nested payload directories, so elevated installations satisfy the same strict ownership checks. Installed macOS runtime validation, self-test and retirement read the complete app payload from its outer root. Cold service starts coordinate registration-metadata setup before source watching.

1.4.4 scales WAL/shared-memory allowances with admitted database capacity, preserves preceding-budget reads during recovery, and sizes growth using logical SQLite pages. See the [release record](releases/release-1.4.4.md) and [reliability review, including Turso and other embedded engines](storage-reliability.md).

Installation and upgrades now display a VS Code progress notification with the current stage: version and payload checks, staging, native activation, agent connection updates, workspace setup and cleanup. Progress uses actual stages without an invented percentage. On Windows and Linux, an admitted upgrade hard-stops verified preceding AIRK processes and disconnects their clients; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

Successful runtime activation finishes quietly. Once its loader is active, AIRK handles later detected extension replacements automatically; external coding-agent sessions may need their own restart to load the new tool connection. Connection or cleanup issues produce a specific warning and details instead of a clean success message.

Trusted VSIX installation and **Install or Repair CLI** refresh recognized existing workspace AIRK connections in Codex (`.codex/config.toml`), Claude Code (`.mcp.json`) and GitHub Copilot (`.vscode/mcp.json`) to the installed stable launcher and registered service arguments. Other servers, environment and tool policies remain intact; missing entries do not enable an agent. Foreign/custom launches, unsupported TOML, unsaved edits and unsafe files remain for manual review with a visible warning. Restart connected agents after installation. Targeted Windows installation is covered by the local installer harness; broader platform qualification remains pending.

The current branch uses the **1.8.12 working version**, with **1.9.0** reserved for the planned promotion, carrying forward the 1.5.0 addition of the [AIRK Explorer and Explorer Assistance](releases/release-1.5.0.md); the paragraphs above and below about 1.4.x describe earlier releases. [Routine CI](ci-and-builds.md) runs the full workspace once on Linux, focused native regressions elsewhere, all platform builds/package smoke and a container smoke. Full platform/container suites and installed-runtime lifecycle campaigns are explicit qualification. 1.4.2 scales Balanced admission to one quarter of detected RAM (2–32 GiB), protects capacity for up to four project query workers, and raises Balanced source pacing to 64 MiB/s per job within a 256 MiB/s shared ceiling. Native watching supports 8,192 directories per project and 32,768 shared, with bounded incremental traversal and specific limit diagnostics. Existing manual resource overrides and storage quotas are preserved. These are admission policies, not hard OS memory or disk-I/O limits. Follow the [versioning process](versioning.md) from local candidates through CI and publication. Its checks run on Windows x64; earlier installed-editor and Linux receipts do not qualify this follow-up.

The [MCP contract](mcp.md) distinguishes raw unavailable diagnostics, bounded publication/readiness observations and typed exact-request retries. Qualification clients preserve these distinctions and exact submitted-query accounting.

The [MCP contract](mcp.md) also covers `help` (native usage topics, native parser-generated CLI help and current tool schemas) and `diagnostics.work` (scoped progress/jobs, bounded queues, watcher status and matched waits). The seventeen native read-only tools remain read-only; the unified catalog also advertises explicitly reviewed UI drafts.

The [MCP contract](mcp.md) documents `diagnostics.runtime`, including running frontend/worker/service versions, process and payload identities, build receipt metadata and explicit missing observations.

CLI installation removes previous terminal executables regardless of version and retires the previous native runtime and rollback reference after successful activation. Locked files report incomplete cleanup and retry on a later trusted activation or **Install or Repair CLI**. Sources, indexes and settings are preserved. See the [console installation contract](service-console.md).

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

The **1.4.4 release** corrects WAL admission and logical database sizing. Earlier 1.4.0/1.4.1/1.4.2 qualification remains historical. Start with the [historical 1.4.4 release record](releases/release-1.4.4.md), [service plan](airkservice-architecture.md), [completion checklist](service-implementation-checklist.md), [project setup](service-project-setup.md) and [client adapters](service-client-adapters.md). That historical record did not qualify native Mac or the complete installed-platform/signing/policy matrix; current P6 evidence is recorded in the implementation ledger.

The 1.3.x/1.2.x release records and former direct-helper/window-scheduler guides are historical references. They do not describe the 1.4 default lifecycle or authorize opening legacy stores through the service. Native and extension source/lockfile versions are synchronized at the **1.8.12 manual-test working build**. Promotion to the planned **1.9.0** follows implementation and manual acceptance; publication is verified separately.

The [local Rust iteration guide](../CONTRIBUTING.md#local-rust-iteration) covers scoped Clippy, persistent targets, platform-specific development incremental settings and timing reports; full qualification remains required.

The [Windows build/editor guide](windows-local-build.md) describes pair builds, stage-only acceptance without a VSIX, and real native registration/usage/response-mode checks in isolated editor profiles.

- [1.6.4 public release: upgrades, removal, MCP status and migration](releases/release-1.6.4.md)
- [Historical unpublished 1.6.1–1.6.3 candidate evidence](releases/release-1.6.3.md)
- [Architecture and code boundaries](architecture.md)
- [AIRK Explorer design inputs and approved 1.5.0 design](explorer/README.md): historical design records for the shipped workbench. Current native evidence and optional Explorer assistance share the [unified MCP endpoint](unified-mcp.md).
- [AIRK 1.4.0 local service architecture and implementation plan](airkservice-architecture.md)
- [1.5.9 MCP compatibility record](releases/release-1.5.9.md)
- [1.5.8 update and management record](releases/release-1.5.8.md)
- [Earlier 1.5.6 development record](releases/release-1.5.6.md)
- [Earlier 1.5.5 development record](releases/release-1.5.5.md)
- [Published 1.5.4 release record](releases/release-1.5.4.md)
- [1.5.3 local candidate record](releases/release-1.5.3.md)
- [1.5.0 Explorer release record](releases/release-1.5.0.md)
- [Previous 1.4.4 release record](releases/release-1.4.4.md)
- [Historical 1.4.3 storage-recovery candidate](releases/release-1.4.3.md)
- [Storage reliability and embedded engine review](storage-reliability.md)
- [Historical 1.4.2 resource-policy candidate](releases/release-1.4.2.md)
- [Historical 1.4.1 upgrade candidate](releases/release-1.4.1.md)
- [Versioning from local install through CI and publication](versioning.md)
- [Historical 1.4.0 candidate and qualification limits](releases/release-1.4.0.md)
- [Offline service installation, resumable activation, managed payloads and startup ownership](service-installation.md) Shutdown ownership checks also cover damaged payload files, and initialization receipts prevent silent registry replacement after control loss.
- [Reviewed runtime upgrades, rollback, saved activation and version pins](service-runtime-transitions.md)
- [Mac signed service bundle, optional login ownership and qualification limits](service-macos-login.md)
- [Reviewed project setup, reserved storage and stable-launcher MCP bindings](service-project-setup.md)
- [Explicit legacy agent-entry review, startup removal and old derived-file retirement](service-legacy-transition.md)
- [Project removal, automatic cleanup and historical retirement contracts](service-project-retirement.md)
- [Direct service client adapters and durable CLI request recovery](service-client-adapters.md)
- [Direct usage/settings reads, scan-limit reset and guarded project management](service-native-management.md)
- [Default-cutover development qualification and remaining gates](service-cutover-qualification.md)
- [Service implementation completion checklist](service-implementation-checklist.md)
- [Complete service-plan requirements review and qualification boundaries](service-requirements-review.md)
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
- [October 22 service publication mechanism spike](service-publication-spike.md)
- [Historical architecture v2 implementation plan](architecture-v2.md)
- [Proposed change tracing and bounded development indexes](change-tracing-plan.md)
- [Decision 001: focused local retrieval](decisions/001-local-retrieval-kernel.md)
- [Design inspiration and independent implementation](inspiration.md)
- [Implemented format support](formats.md)
- [Ingestion limits, machine sizing and index detail](ingestion-limits.md)
- [Per-collection extraction policy](extraction-policy.md)
- [Measured scale and performance qualification](scale-qualification.md)
- [October 21 indexing feedback review, implementation and measurement](indexing-speed-review-2026-10-21.md)
- [October 21 follow-up: durable rebuilds, exact completion reports and guarded lexical cache](indexing-follow-up-2026-10-21.md)
- [Historical 1.2 indexing optimization and qualification record](indexingenhancements.md)
- [Current-index sharding, Tantivy-to-FTS5 rationale and storage alternatives](index-sharding-review.md)
- [Document requirements, dates and design indexing](document-indexing.md)
- [Code-indexing recommendations and optional semantic-provider review](code-indexing-options.md)
- [Implemented SQLite code facts, relationships and SCIP imports](code-facts-and-relationships.md)
- [Collections and selected DiskANN architecture](collections.md)
- [Shared storage quotas and reservation limits](storage-quotas.md)
- [Current-object cleanup and retired history controls](retention.md)
- [Historical v2 design: content and checkpoints, not available in 1.2](v2-history-controls.md)
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
- [MCP tool retirement in 1.3.0](unreleased-mcp-tool-retirement.md)
- [Request execution, bounded admission and verification costs](request-execution.md)
- [Code review findings and 1.1.4 implementation record](airkimprovements.md)
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
- [Coding-agent tool alignment and online documentation review](agent-tool-alignment.md)
- [Agent Lean index modes and MCP efficiency plan](mcp-prioritization.md)
- [2026-11-01 agent-side MCP investigation: query log, admission, ranking and tool-description recommendations](mcp-recommendations.md)
- [1.2.4 diagnostics, structured contention retries, helper identity and bounded batch recovery](releases/release-1.2.4.md)
- [1.2.3 Agent Lean default and indexing fix](releases/release-1.2.3.md)
- [1.2.2 semantic indexing fix](releases/release-1.2.2.md)
- [1.2.1 documentation update](releases/release-1.2.1.md)
- [1.2.0 current-index, Agent Lean and compression behavior](releases/release-1.2.0.md)
- [Workspace indexing coverage and clickable footer](index-coverage.md)
- [Native VS Code extension and limits](vscode-extension.md)
- [Extension listing content, agent integration copy and illustrations](extension-content.md)
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
- [0.1.0 implementation review and remaining roadmap work](review-0.1.0.md)
- [Main branch protection and manual extension publication](release-workflow.md)
- [Build and review Windows app/extension locally](windows-local-build.md)
- [Extension deployment adapted from VCP](extension-deployment.md)
- [GitHub CI, native artifacts, credentials and spending controls](ci-and-builds.md)
- [Operations and recovery](operations.md)
- [Native macOS build and release instructions](macos-build.md)
- [Implementation roadmap](roadmap.md)
- [Language popularity and remaining native adapter gaps — 27 October 2026 research](language-gaps.html)
- [Observed validation and historical receipts](validation.md)
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
