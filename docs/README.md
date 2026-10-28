# AIRK documentation

The [1.5.12 development record](release-1.5.12.md) tracks optional agent instructions with copy/open-file actions, responsive Explorer startup, shared runtime discovery, live reconnect checks, visible MCP diagnostics and agent configuration updates against current native registrations. The full inline agent setup guide remains proposed; previous release records retain their original validation scope.

Overview and Management share the [index dashboard](index-management.md): published content and semantic coverage first, followed by current source, semantic or cleanup work. Job counters and saved index totals remain distinct and update automatically.

The [1.5.10 candidate](release-1.5.10.md) adds local raw failure evidence and fixes cleanup continuation under large object inventories and quota pressure. Its validation record distinguishes the fresh Bookbuilder reproduction from the still-unavailable remote retained state.

[Storage formats](storage-formats.md) separates index/service compatibility identifiers from executable versions, describes fresh default paths and existing-location compatibility, and records the current cleanup and diagnostic evidence.

Current 1.5.12 uses [one unified MCP connection](unified-mcp.md), with 26 distinct tools, fixed project scope and guarded startup consolidation of recognized old server definitions. [Index management](index-management.md) documents progress, saved settings, recovery and diagnostics.

The [1.5.9 compatibility record](release-1.5.9.md) covers discovery/fallback and independent simultaneous agent connections. The [1.5.8 update lifecycle record](release-1.5.8.md) describes the single-action reload notice, explicit runtime installation stages and quiet success. [1.5.7](release-1.5.7.md) retains its original candidate validation.

Current development version: **1.5.12**. Management uses horizontal tabs, captured diagnostic report open/copy/save and full retained service log export. Completed runtime setup reopens quietly. Recovery checks the unfinished runtime marker, resumes outstanding source work after successful recovery and preserves failure budgets across new source hints; see the [1.5.7 development record](release-1.5.7.md), [index management](index-management.md) and [automatic indexing recovery](index-self-healing.md). The [original recovery proposal](index-self-healing-proposal.md), [1.5.6 record](release-1.5.6.md), [1.5.5 record](release-1.5.5.md), [published 1.5.4 record](release-1.5.4.md) and earlier records retain their original design, evidence and publication scope.

The 1.5.0 local Explorer candidate includes fourteen Assistance tools, paired project setup, a unified offline Help search, four role guides, purpose/format-based indexing guidance and About/community links. See [the extension guide](vscode-extension.md), [implementation receipts](explorer/code-review-action-plan.md), [role guidance](../packages/vscode/src/explorerHelp/role-developer.md), [indexing choices](../packages/vscode/src/explorerHelp/indexing-choices.md) and [document types](../packages/vscode/src/explorerHelp/document-types.md). These articles and the native engine guidance are also available directly in Explorer Help and through Assistance help tools.

The [1.4.4 release-tool repair](release-workflow.md#pinned-144-release-tool-repair) corrects macOS bundle verification and reuses the original artifacts verified by the successful routine CI run without rebuilding or relabeling native binaries. The original source commit and reviewed tooling commit are recorded separately.

The historical 1.4.4 follow-up corrects Unix cleanup ordering for naturally exited children while retaining exact child and descendant-exit proof. Routine CI keeps correctness/build/package checks, uses lightweight default features for focused Windows/macOS regressions, and bounds independent native fixture concurrency. Long workload and performance campaigns are available through explicit [CI qualification](ci-and-builds.md).

1.4.4 also creates staged Windows runtime directories with explicit current-user ownership and owner-only access, including nested payload directories, so elevated installations satisfy the same strict ownership checks. Installed macOS runtime validation, self-test and retirement read the complete app payload from its outer root. Cold service starts coordinate registration-metadata setup before source watching.

1.4.4 scales WAL/shared-memory allowances with admitted database capacity, preserves preceding-budget reads during recovery, and sizes growth using logical SQLite pages. See the [release record](release-1.4.4.md) and [reliability review, including Turso and other embedded engines](storage-reliability.md).

Installation and upgrades now display a VS Code progress notification with the current stage: version and payload checks, staging, native activation, agent connection updates, workspace setup and cleanup. Progress uses actual stages without an invented percentage. On Windows and Linux, an admitted upgrade hard-stops verified preceding AIRK processes and disconnects their clients; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

Successful runtime activation finishes quietly. A detected extension update offers **Reload to finish update**; external coding-agent sessions may need their own restart to load the new tool connection. Connection or cleanup issues produce a specific warning and details instead of a clean success message.

Trusted VSIX installation and **Install or Repair CLI** refresh recognized existing workspace AIRK connections in Codex (`.codex/config.toml`), Claude Code (`.mcp.json`) and GitHub Copilot (`.vscode/mcp.json`) to the installed stable launcher and registered service arguments. Other servers, environment and tool policies remain intact; missing entries do not enable an agent. Foreign/custom launches, unsupported TOML, unsaved edits and unsafe files remain for manual review with a visible warning. Restart connected agents after installation. Targeted Windows installation is covered by the local installer harness; broader platform qualification remains pending.

The current branch is synchronized at **1.5.12**, carrying forward the 1.5.0 addition of the [AIRK Explorer and Explorer Assistance](release-1.5.0.md); the paragraphs above and below about 1.4.x describe earlier releases. [Routine CI](ci-and-builds.md) runs the full workspace once on Linux, focused native regressions elsewhere, all platform builds/package smoke and a container smoke. Full platform/container suites and installed-runtime lifecycle campaigns are explicit qualification. 1.4.2 scales Balanced admission to one quarter of detected RAM (2–32 GiB), protects capacity for up to four project query workers, and raises Balanced source pacing to 64 MiB/s per job within a 256 MiB/s shared ceiling. Native watching supports 8,192 directories per project and 32,768 shared, with bounded incremental traversal and specific limit diagnostics. Existing manual resource overrides and storage quotas are preserved. These are admission policies, not hard OS memory or disk-I/O limits. Follow the [versioning process](versioning.md) from local candidates through CI and publication. Its checks run on Windows x64; earlier installed-editor and Linux receipts do not qualify this follow-up.

The [MCP contract](mcp.md) distinguishes raw unavailable diagnostics, bounded publication/readiness observations and typed exact-request retries. Qualification clients preserve these distinctions and exact submitted-query accounting.

The [MCP contract](mcp.md) also covers `help` (native usage topics, native parser-generated CLI help and current tool schemas) and `status.work` (scoped progress/jobs, bounded queues, watcher status and matched waits). The sixteen native evidence tools remain read-only; the unified catalog also advertises explicitly reviewed UI drafts.

The [MCP contract](mcp.md) documents `status.runtime`, including running frontend/worker/service versions, process and payload identities, build receipt metadata and explicit missing observations.

CLI installation removes previous terminal executables regardless of version and retires the previous native runtime and rollback reference after successful activation. Locked files report incomplete cleanup and retry on a later trusted activation or **Install or Repair CLI**. Sources, indexes and settings are preserved. See the [console installation contract](service-console.md).

Trusted VSIX activation now installs the native runtime and the `airk` terminal command, registers new workspace folders, prepares the bundled offline model and queues indexing with automatic watching. No setup links are required. Open a new VS Code terminal after installation; restart external terminal applications to refresh PATH. User `airk.cli.autoInstall: false` disables automatic installation and registration. Existing 1.4 registrations, selected index-data locations and watching preferences are preserved, including after a storage change. Compatible 1.4 runtime updates use the native fenced activation and schema-migration path; no fresh registration or reindex is requested for saved projects. Managed runtime ownership and explicit Stop remain authoritative. Use **Choose Index Storage Folder** or **Choose Native Automatic Watching** to change defaults later. Storage selection creates a reviewed binding and retains old index files for separate cleanup. Agent connection remains a separate explicit action.

**AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents**

The **1.4.4 release** corrects WAL admission and logical database sizing. Earlier 1.4.0/1.4.1/1.4.2 qualification remains historical. Start with the [historical 1.4.4 release record](release-1.4.4.md), [service plan](airkservice-architecture.md), [completion checklist](service-implementation-checklist.md), [project setup](service-project-setup.md) and [client adapters](service-client-adapters.md). Native Mac and the complete installed-platform/signing/policy matrix remain unqualified.

The 1.3.x/1.2.x release records and former direct-helper/window-scheduler guides are historical references. They do not describe the 1.4 default lifecycle or authorize opening legacy stores through the service. Native and extension source/lockfile versions are synchronized at 1.5.12; publication is a separate action.

The [Windows build/editor guide](windows-local-build.md) describes pair builds, stage-only acceptance without a VSIX, and real native registration/usage/response-mode checks in isolated editor profiles.

- [Architecture and code boundaries](architecture.md)
- [AIRK Explorer design inputs and approved 1.5.0 design](explorer/README.md): the planned workbench that replaces the Service Console, plus the separate Explorer Assistance MCP server. In development; not shipped.
- [AIRK 1.4.0 local service architecture and implementation plan](airkservice-architecture.md)
- [1.5.9 MCP compatibility record](release-1.5.9.md)
- [1.5.8 update and management record](release-1.5.8.md)
- [Earlier 1.5.6 development record](release-1.5.6.md)
- [Earlier 1.5.5 development record](release-1.5.5.md)
- [Published 1.5.4 release record](release-1.5.4.md)
- [1.5.3 local candidate record](release-1.5.3.md)
- [1.5.0 Explorer release record](release-1.5.0.md)
- [Previous 1.4.4 release record](release-1.4.4.md)
- [Historical 1.4.3 storage-recovery candidate](release-1.4.3.md)
- [Storage reliability and embedded engine review](storage-reliability.md)
- [Historical 1.4.2 resource-policy candidate](release-1.4.2.md)
- [Historical 1.4.1 upgrade candidate](release-1.4.1.md)
- [Versioning from local install through CI and publication](versioning.md)
- [Historical 1.4.0 candidate and qualification limits](release-1.4.0.md)
- [Offline service installation, resumable activation, managed payloads and startup ownership](service-installation.md) Shutdown ownership checks also cover damaged payload files, and initialization receipts prevent silent registry replacement after control loss.
- [Reviewed runtime upgrades, rollback, saved activation and version pins](service-runtime-transitions.md)
- [Mac signed service bundle, optional login ownership and qualification limits](service-macos-login.md)
- [Reviewed project setup, reserved storage and stable-launcher MCP bindings](service-project-setup.md)
- [Explicit legacy agent-entry review, startup removal and old derived-file retirement](service-legacy-transition.md)
- [Guarded project unregister and resumable selected index cleanup](service-project-retirement.md)
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
- [Native CPU capability detection, qualified model catalog and future INT8 selection](onnx-models.md#capabilities-and-qualified-model-catalog)
- [Durable native scheduling and crash recovery](native-scheduler.md)
- [MCP tool contracts](mcp.md)
- [1.3.1 independent project workers](release-1.3.1.md)
- [1.3.2 empty-index progressive publication fix and validation](release-1.3.2.md)
- [1.3.0 changes and qualification limits](release-1.3.0.md)
- [Historical 1.2.5 local candidate and dated validation](release-1.2.5.md)
- [MCP tool retirement in 1.3.0](unreleased-mcp-tool-retirement.md)
- [Request execution, bounded admission and verification costs](request-execution.md)
- [Code review findings and 1.1.4 implementation record](airkimprovements.md)
- [Source, parser and provider hardening](ingest-hardening.md)
- [Resumable inventory/fact discovery and compact outlines](discovery.md)
- [Coding-agent setup compatibility research](agent-integration.md)
- [Google Antigravity manual MCP setup](antigravity-mcp.md)
- [Coding-agent tool alignment and online documentation review](agent-tool-alignment.md)
- [Agent Lean index modes and MCP efficiency plan](mcp-prioritization.md)
- [1.2.4 diagnostics, structured contention retries, helper identity and bounded batch recovery](release-1.2.4.md)
- [1.2.3 Agent Lean default and indexing fix](release-1.2.3.md)
- [1.2.2 semantic indexing fix](release-1.2.2.md)
- [1.2.1 documentation update](release-1.2.1.md)
- [1.2.0 current-index, Agent Lean and compression behavior](release-1.2.0.md)
- [Workspace indexing coverage and clickable footer](index-coverage.md)
- [Native VS Code extension and limits](vscode-extension.md)
- [Extension listing content, agent integration copy and illustrations](extension-content.md)
- [1.0.0 stable release notes and publication checklist](release-1.0.0.md)
- [Sidebar PNG renderings](ui/README.md)
- [Usage & Savings PNG previews for 1.1.3](ui/usage-reporting.md)
- [October 17 combined release notes: 1.1.0–1.1.2](releases-2026-10-17.md)
- [1.1.3 implementation and qualification: Usage & Savings](release-1.1.3.md)
- [Historical 1.1.4 record: reliable usage comparisons](release-1.1.4.md)
- [Measuring AIRK's value: sessions, evaluation and overhead](usage-evaluation.md)
- [1.1.2 implementation and qualification: coding-agent connections](release-1.1.2.md)
- [1.1.1 implementation and qualification: dashboard, storage and recovery](release-1.1.1.md)
- [1.1.0 implementation and qualification: workstation sizing](release-1.1.0.md)
- [Historical 0.1.0 release review and test checklist](release-checklist-0.1.0.md)
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

Contributor guidance: [CONTRIBUTING.md](../CONTRIBUTING.md). Implemented, proposed and unqualified behavior must remain distinguishable.

Development helper recovery now preserves project workers and rebinds retained direct model sessions with bounded retries and explicit stop/start controls; see [shared embedding](service-embedding.md). Default adapter cutover and installed-platform qualification remain open.

Native project controls now expose cached worker identity, queued restart and startup-circuit state. Explicit restart drains the expected worker before replacement, preserves other projects and supports retained evidence when sources are unavailable. See [native service control](service-protocol.md). Default client cutover and installed-platform qualification remain open.

Development console code now presents cached project/work observations, guarded repair previews, exact request recovery and redacted diagnostic previews. Native global lifecycle uses durable instance-fenced stop/restart intent and an explicit-stop preference. Default adapter cutover, complete configuration and installed/editor qualification remain in progress. See [the console foundation and development receipts](service-console.md).

Initialized registry recovery now refuses missing/empty databases and missing required tables, preserving accepted request and fencing identity. See [durable registry ownership](service-jobs.md).

Installed namespace binding now independently pins the completed registry identity outside the control directory; losing both registry files refuses startup without creating another authority. See the [installation development contract](service-installation.md).

Saved bootstrap snapshots now read completed registry metadata without executing a damaged selected payload or migrating/pruning the registry. The console retains bounded cached project pages and logs while disconnected, labels sampled work explicitly, and can submit a saved-instance Stop through the independently verified bundled main executable. Native Stop rechecks the actual service identity; Restart/Start still require a verified complete payload. See [down-service behavior](service-console.md#down-service-and-damaged-payload-observation).

Development payload repair now requires an exact native preview and stopped owners, preserves the complete damaged slot, resumes interrupted copies and returns an idempotent completion receipt. The trusted console uses the matching approved offline bundle; Start remains explicit and uninstall retains the damaged originals. See [payload repair](service-installation.md#explicit-payload-repair).

The [Explorer code review and implementation follow-up](explorer/code-review-action-plan.md) records the chronological branch audit, corrective work, local validation and remaining qualification.

The [1.5.12 process changes](service-installation.md#process-lifetime-and-recognizable-names) cover client-owned MCP cleanup, distinct installed worker names and single-engine downloads.
