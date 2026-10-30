# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

**Version 1.6.4 — runtime upgrades, project cleanup and responsive diagnostics.** This public release consolidates the unpublished 1.6.1–1.6.3 branch work. Supported runtimes upgrade before connection checks; uninstall removes owned executables and recorded agent entries while preserving index data. Trusted open folders register and index automatically, subject to saved opt-out, pause and manual-update settings. Removing a project drains its workers and deletes its AIRK index and saved settings while preserving source files.

One MCP connection exposes 27 tools. `status` is a lightweight readiness check; `diagnostics` provides detailed runtime, job and watcher observations. Cold MCP verification survives individual status timeouts, so later polls reuse the same authenticated setup. CLI request-epoch preparation uses a bounded wait for explicit busy/uncertain registry responses and retains failure codes. macOS inference shutdown releases the owned ONNX environment after sessions drain, before native library cleanup. Connection checks keep unknown readiness distinct from a confirmed unavailable index. Agent connections follow their configured source folders across index replacement. Explorer bounds status polling, keeps local log actions responsive and reconciles uncertain requests internally without replaying them. Installation migrates registered historical default index roots to `index-v1`, preserving publications and settings; custom roots stay in place. See the [1.6.4 release notes](docs/release-1.6.4.md). [Process ownership and counts](docs/process-model.md) explains the shared workers, per-connection frontends and retained Windows launcher layers.

**Introduced in 1.6.0 — offline semantic index optimization.** Two bundled INT8 MiniLM exports replace the previous FP32 model. Native CPU detection chooses unsigned INT8 on x64 without VNNI and signed INT8 on VNNI x64 and ARM64. Installation and rebuilding require no model download. Upgrade rebuilds semantic vectors and caches while preserving text/fact evidence, pause and semantic opt-out settings. See the [1.6.0 record](docs/release-1.6.0.md) and [optimization rationale](docs/semantic-index-optimization.md).

The following management and agent improvements were introduced in 1.5.12 and remain available.

Management → Agents now offers **Copy instructions** and separate buttons to open Claude Code (`CLAUDE.md`), Codex (`AGENTS.md`) and GitHub Copilot (`.github/copilot-instructions.md`) project instructions. The suggestion teaches agents to discover deferred AIRK tools, confirm status, use focused evidence and verify live files. Existing files stay unchanged; missing files open as unsaved drafts for review and saving. The bundled **Project instructions for AI agents** help topic also covers other assistants.

Version **1.5.12** includes the index-management and startup improvements described below. Explorer opens while runtime checks continue in the background. Healthy completed installations reconnect without running native startup again, and simultaneous discovery requests share work. Agent configuration uses current native registrations. The Agents panel displays connection-check stages, elapsed time and copyable connection results. The full inline agent setup guide remains a proposal. See the [1.5.12 release record](docs/release-1.5.12.md).

AIRK 1.5.12 also ties MCP process lifetime to its client and gives installed service, index and source workers distinct executable names. Role copies are created locally; downloads still include one main engine. Live agents keep independent MCP connections, while background indexing remains service-owned.

Overview and Management now lead with published files, text chunks, vectors and semantic file coverage. Current work appears separately with counters appropriate to source indexing, semantic batches or cleanup. The display updates automatically; starting another attempt does not replace saved index counts with zeros.

The [1.5.10 candidate](docs/release-1.5.10.md) adds local raw job-error logs, continued cleanup of obsolete index objects, explicit failed repair results, and [storage format identifiers](docs/storage-formats.md) independent of application versions. Existing owned storage locations remain compatible; fresh installations use `runtime`, `index-v1` data and `service-v1` state defaults.

Version 1.5.9 fixes MCP discovery followed by a legacy handshake: a delayed discovery response no longer leaves a successfully initialized connection rejecting its tool requests. Protocol state stays per connection, so assistants can share one project using different handshake styles. Modern requests still require valid metadata. See the [1.5.9 compatibility record](docs/release-1.5.9.md).

Failed jobs observed by the service now produce retained `job_failed` events with the exact job and safe failure code. Index preparation and discovery preserve stage and recognized native causes. Older generic failures cannot be reconstructed; unknown causes remain explicit.

**Management features introduced in 1.5.12.** Management uses consistent horizontal tabs for Indexing, Activity, Storage, Recovery, Service, Agents, Usage and Logs. Indexing contains a health banner, current progress, published search availability, and three everyday settings: search mode, updates and evidence detail. Semantic enablement verifies/prepares the offline model before saving the policy and requests source and semantic work. Registration records native setup durably, so closing the editor does not strand a newly registered project before its initial index requests. See the [1.5.8 development record](docs/release-1.5.8.md).

Queued repair requests whose plan has finished or lost its owner are settled without replaying old approvals, so they cannot indefinitely block otherwise eligible source work. Overview and Management update automatically while visible. Management exposes plan gates, confirms manual diagnostic status checks, separates old counters from queued work, and provides Copy details for troubleshooting panels.

Updates distinguish the installed extension, the running interface and native runtime activation. A detected pending extension update presents one **Reload to finish update** action and retains that action in Explorer and the status bar. Runtime progress says Installing or Updating AIRK runtime; successful activation finishes quietly and project indexing remains in Management. VS Code controls extension replacement, so an already running version without these changes retains its earlier messages until reloaded.

Indexing recovery now belongs to the native service: accepted search/detail settings finish without the editor, temporary source/storage failures wait for restored conditions, and worker/helper retry budgets survive service restarts. Version 1.5.7 corrects a completed-bootstrap marker that repeatedly requested recovery, resumes outstanding source work after successful recovery, and prevents source hints from replenishing repeated failure budgets. Pause, Cancel, manual updates, custom models and guarded cleanup remain authoritative. Management can open/copy/save a captured diagnostic report and export the full retained service log ring. Verified completed installations reopen quietly without repeating terminal installation or runtime cleanup.

Studio **Save** writes the complete draft inputs to `.airk/strategies/<name>.studio.airk.json` in the selected local project without a folder picker. It preserves the objective, parameters, path scope, steps, prompt profile and edited prompt under a `user/<name>` definition. Save again to update it, or change the name for a copy; outside edits are guarded. Browse saved strategies restores these inputs, with evidence recollected after loading. Accepting a suggestion does not save automatically. **Export…** continues to save only a strategy definition.

Management separates current work from published evidence. Captured files, newly extracted payloads, publications, phase time and last advancement remain distinct; unknown totals stay unknown. Each Management section has its own tab; inactive sections are unmounted, and project selection persists across tabs. Existing Management URLs open the corresponding tab. In 1.6.4, removal preserves source files and automatically deletes AIRK index data and saved project settings after owned workers drain. Only opaque identifiers and a removal timestamp remain in native history; incomplete cleanup resumes automatically.

**Published 1.5.4:** Installer upgrades stop verified previous AIRK processes, including older versions and Windows clients holding the launcher open. Existing watcher metadata migrates at service startup; shutdown ownership checks read saved metadata without migration and preserve the actual failure cause. See the [1.5.4 development record](docs/release-1.5.4.md). Earlier candidate and validation records retain their original scope. This release also carries the previously unpublished 1.5.3 semantic preparation fix, Semantic/Hybrid search controls, missing-source project removal and bounded watcher directory diagnostics.

Explorer review fixes in this 1.5.0 local candidate guard plan saves, keep evidence and drafts bound to their project/publication, settle failed parallel reads, refuse expired proposals and preserve complete task instructions. Remote plan saving uses an untitled draft and the editor Save command. Platform qualification is recorded separately in the 1.5.0 release notes.

One **AIRK MCP connection** now exposes 27 unique tools without repeated `airk_` prefixes: evidence reads, documentation and reviewed UI drafts. `search` returns indexed evidence; `draft_search` prepares a form for user review. `status` reports index readiness; `diagnostics` reports detailed work and Explorer attachment, and multi-root connections require an explicit configured project. Trusted startup replaces recognized earlier Retrieval and Explorer definitions together with one `airk` entry. See the [unified catalog and migration contract](docs/unified-mcp.md).

**1.5.0 adds the AIRK Explorer**, an editor workbench that replaces the Service Console: search and inspect indexed evidence, review Git changes against the index, build evidence strategies and agent plans, and manage project indexing and the native service. The original 1.5.0 **Explorer Assistance** server answered how-to questions about the Explorer and delivered inert drafts to a Suggestions Inbox; it never calls AIRK Retrieval, runs commands or writes files. That release used a separate connection; 1.5.7 consolidates it into the unified endpoint. External agents reach AIRK through the stable launcher, so no system Node is needed. See the [1.5.0 release record](docs/release-1.5.0.md) and the [extension guide](docs/vscode-extension.md).

Historical 1.4.4 release: the GitHub release verifier now follows the same macOS app-bundle layout as native packaging and signing. A narrowly pinned release-tool repair reuses the already-tested 1.4.4 binaries; its original source and CI identity remain unchanged. See the [release record](docs/release-1.4.4.md).

1.4.4 corrects Unix cleanup after a child exits naturally. Cleanup confirms that the owned child has exited and its process group has drained before releasing it; transient native errors remain bounded by the existing deadline.

1.4.4 also creates staged Windows runtime directories with explicit current-user ownership and owner-only access, including nested payload directories, so elevated installations satisfy the same strict ownership checks. Installed macOS runtime validation, self-test and retirement read the complete app payload from its outer root. Cold service starts coordinate registration-metadata setup before source watching.

1.4.4 corrects a fixed WAL limit that could block indexing and repeatedly fail recovery despite free disk space. WAL and shared-memory allowances now scale with admitted database capacity; retained reads and pending replay inputs are preserved. SQLite growth admission includes uncheckpointed pages. See the [storage-recovery release record](docs/release-1.4.4.md). This source fix still requires installation and recovery verification in affected workspaces.

1.4.2 scales Balanced admission to one quarter of detected RAM (2–32 GiB), protects capacity for up to four project query workers, and raises Balanced source pacing to 64 MiB/s per job within a 256 MiB/s shared ceiling. Native watching supports 8,192 directories per project and 32,768 shared, with bounded incremental traversal and specific limit diagnostics. Existing manual resource overrides and storage quotas are preserved. These are admission policies, not hard OS memory or disk-I/O limits.

Installation and upgrades now display a VS Code progress notification with the current stage: version and payload checks, staging, native activation, agent connection updates, workspace setup and cleanup. Progress uses actual stages without an invented percentage. On Windows and Linux, an admitted upgrade hard-stops verified preceding AIRK processes and disconnects their clients; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

Successful runtime activation finishes quietly. A detected extension update offers **Reload to finish update**; external coding-agent sessions may need their own restart to load the new tool connection. Connection or cleanup issues produce a specific warning and details instead of a clean success message.

Trusted startup and **Install or Repair CLI** migrate recognized project-local agent connections to one stable-launcher endpoint. Codex, Claude Code, Copilot, Gemini, Antigravity, Cursor, Roo and Junie files are supported. Other servers, environment values and disabled state are preserved; conflicts, unsafe files and unsaved edits remain for review. Restart the coding agent and approve the renamed tools after migration.

The current development package version is **1.6.4**. The earlier 1.5.0 candidate superseded 1.4.4; its [release notes](docs/release-1.5.0.md) retain their original validation scope.

CLI installation removes previous extension-managed terminal executable copies regardless of version. It also retires the previous native rollback payload after successful activation, preserving the selected runtime, sources, indexes and settings. Locked files report incomplete cleanup; disconnect old clients and use **AIRK: Install or Repair CLI**, or allow the next trusted activation to retry. Close old terminal sessions completely; **Developer: Reload Window** can preserve their environment.

Trusted VSIX activation now installs the native runtime and the `airk` terminal command, registers new workspace folders, prepares the bundled offline model and queues indexing with automatic watching. No setup links are required. Open a new VS Code terminal after installation; restart external terminal applications to refresh PATH. User `airk.cli.autoInstall: false` disables automatic installation and registration. Existing 1.4 registrations, selected index-data locations and watching preferences are preserved, including after a storage change. Compatible 1.4 runtime updates use the native fenced activation and schema-migration path; no fresh registration or reindex is requested for saved projects. Managed runtime ownership and explicit Stop remain authoritative. Use **Choose Index Storage Folder** or **Choose Native Automatic Watching** to change defaults later. Storage selection creates a reviewed binding and retains old index files for separate cleanup. Agent connection remains a separate explicit action.

**1.5.0 AIRK Explorer and Explorer Assistance.** See the [release record](docs/release-1.5.0.md). The previous **1.4.4 native service and storage recovery** is recorded in its [release record](docs/release-1.4.4.md). Earlier 1.4.0/1.4.1/1.4.2 qualification remains historical; this candidate requires its own validation. Publication is controlled by protected-main CI and the release workflows. [Routine CI](docs/ci-and-builds.md) runs the full workspace on Linux, focused Windows/macOS regressions and all native/VSIX build and smoke gates; full platform and multi-project lifecycle qualification is opt-in.

The [Windows build and editor harness](docs/windows-local-build.md) now uses the verified runtime pair and reviewed native registrations. An allowlisted development stage can be exercised before creating a VSIX; final artifact acceptance remains separate.

AIRK indexes local code and documents and serves bounded, cited evidence to coding agents through read-only MCP tools. It preserves original document locations and exact indexed revision identities. It provides native syntax facts and candidate relationships without claiming complete language understanding or resolved references. Original files remain outside derived storage.

## Native service ownership

A per-user service brokers authenticated connections and owns registration, durable work and resource policy. Retrieval flows directly between clients and project workers. Project workers also talk directly to the shared inference helper; the broker does not forward evidence, source text, model weights or embeddings.

The approved runtime pair contains two executables: a lightweight `airk` for native service/client/worker roles, and `airk-embed` for inference and embedded offline MiniLM assets. Compatible projects share verified model files and loaded sessions. Inference starts when needed; connecting a read-only client does not load a model. Native operation targets Windows, Linux and both Mac architectures. Windows/Linux x64 and private Windows editor evidence is recorded separately; native Mac and the complete installed-platform matrix remain unqualified.

The Mac layout uses a [GUI-less signed app bundle](docs/service-macos-login.md) containing those same two executables, with a macOS 13.3 deployment baseline and optional bundle-scoped login registration. The bundled inference library sets that minimum; its license notices reside in bundle resources. Native signing, lifecycle and installed-platform qualification are recorded separately from portable ownership/package checks.

Each project has a canonical source, separate data root, collection name and opaque service binding. Registration reserves disjoint storage before creating it. Sources cannot overlap registered data, runtime state, control or bound logs. Native workers own indexing and source watching. Automatic watching explicitly continues after editor windows close; login startup is a separate opt-in choice.

## Start with a fresh 1.4 namespace

The [native management clients](docs/service-native-management.md) provide direct usage/settings reads, bounded exports and guarded project changes. Usage & Savings waits for native completion; imports bind the reviewed input bytes, and shared quota updates reject stale policies.

1.4 is a clean break. It does not read, convert or share live 1.3 indexes, journals, jobs or revision tokens. Old state and agent configurations remain for explicit review. Fresh indexing requires additional disk space. Known older workers can still compete for resources; review and stop their owned registrations separately.

In the extension, trusted activation sets up new workspace roots automatically. Use **AIRK: Index Now** for later explicit indexing and **AIRK: Choose Native Automatic Watching** to switch between manual and automatic watching. **AIRK Explorer > Management** shows cached observations, readiness, jobs, sampled native wait reasons, independent source/helper state, project settings, guarded repairs and lifecycle controls. Queued jobs remain visible while paused and can be canceled explicitly. Start is explicit after a deliberate Stop; reconnect cannot clear that preference. Management also offers Terse/Full response mode, reviewed runtime upgrades/rollback and saved-request recovery.

Project settings offer [project removal](docs/service-project-retirement.md), including paused projects. Removal stops admission and source watching, drains the selected owner and automatically deletes its index and saved settings. Source files are untouched. A minimal opaque identity/time record keeps explicit removal authoritative; explicit Add Project can register again after cleanup.

Native project settings can reset scan-limit overrides to automatic limits. Native quotas apply to manual and automatic work; retired editor timer/soft-budget settings no longer appear. Workspace tree actions use the clicked project, and native usage counts performed source, semantic and replacement work independently of job acceptance.

The MCP provider advertises only reviewed bindings. **Connect Coding Agent** creates a stable-launcher configuration on request and preserves unrelated client settings. Recognized legacy JSON entries have an [explicit diff and guarded replacement](docs/service-legacy-transition.md); Custom TOML and other agent formats use manual review; recognized existing local Codex, Claude Code and Copilot connections are refreshed during installation. Agent connection is separate from the extension's bounded read-only MCP verification.

The console separately reviews old login startup and offers ownership-verified removal with its own approval and resumable receipts. Review never executes the old command. Startup removal preserves sources, settings and indexes; candidate process matches do not authorize stopping unrelated workers. A separate [offline CLI preview and deletion](docs/service-legacy-transition.md) retires owner-verified 1.3 derived files while preserving sources, settings and usage history. Mac loaded-registration proof is implemented; actual native removal qualification remains pending.

Approved managed payloads can remain read-only. Their client launcher copies only the lightweight main into user metadata; the provider retains control of its payload and external service lifecycle. Development test evidence and remaining gates are recorded in [cutover qualification](docs/service-cutover-qualification.md).

## Native CLI

Install/select an independently approved offline runtime pair using the [installation contract](docs/service-installation.md). A manifest's own checksum is not executable approval. The stable launcher remains outside the versioned extension directory and follows the approved selection.

```console
airk --service-install-dir <installation> --root <source> --data-dir <data> --collection project-a init
airk --service-install-dir <installation> --root <source> --data-dir <data> --collection project-a index
airk --service-install-dir <installation> --root <source> --data-dir <data> --collection project-a tools call airk_status
airk --service-install-dir <installation> --root <source> --data-dir <data> --collection project-a setup --client codex
```

`init` registers roots and prepares reserved storage. `index` reports durable job acceptance; completion and semantic readiness are separate. CLI requests save their exact key before submission and support explicit recovery after a lost acknowledgment. Read aliases and `tools shell` use the direct project transport. A saved opaque binding supports retained reads when sources are unavailable. Missing or incompatible service state fails explicitly; there is no local-store fallback.

The old window scheduler and public direct-store maintenance commands are retired. Full rebuilds require a guarded native preview and separate approval. Advanced native management is available through bounded typed metadata RPC and the service console. See [client adapters and request recovery](docs/service-client-adapters.md) and [project setup](docs/service-project-setup.md).

## Indexed evidence and response size

`help` exposes native usage topics, generated public CLI command help, public tool schemas and optional Explorer guidance. `diagnostics.work` independently reports jobs, bounded queues, cached progress, watcher status and sampled waits. The unified catalog has 27 tools; its seventeen native evidence/help tools remain read-only. UI draft tools queue proposals for user review and never change index jobs.

`airk_status` includes a runtime JSON bundle for the MCP frontend, index worker and native service: compiled versions, process identities, executable/payload hashes and available build receipt metadata. Missing observations remain explicit; see the [MCP contract](docs/mcp.md).

MCP provides bounded file discovery, outlines, symbols, code/document facts, imports, references, relationships, search, reads, context, capabilities and changes. **Terse is the default.** It sends the exact token once, uses shared source bindings and `items`, and avoids repeated static diagnostics. Full mode supplies detailed provenance, coverage and readiness for audits or surprising results; agents can request it per call. Compact navigation uses names, kinds, lines and signatures. Syntax call targets remain labeled candidates.

Results describe indexed evidence. `airk_changes` calls its validation `index-current`; this is not a live-file verification. Change baselines retain bounded metadata across up to 64 publications/24 hours, with explicit coverage floors under pressure. Reads retain usable prior generations during publication or disclose a finite retry hint. See [response modes](docs/mcp-response-modes.md) and the [MCP contract](docs/mcp.md).

Storage uses independent collection ownership, admitted publication/replacement space, bounded readers and conservative recovery. DiskANN is the selected disk-backed vector engine; bootstrap lexical/retained-snapshot adapters are not a substitute for that engine. Scoped repair preserves original sources and reports partial failures. See [publication](docs/service-publication.md), [storage ownership](docs/service-storage-ownership.md) and [maintenance plans](docs/service-maintenance.md).

## Development and records

Read [CONTRIBUTING.md](CONTRIBUTING.md), [architecture and dependency boundaries](docs/architecture.md) and the [documentation index](docs/README.md). The runtime builder stages a host-native executable pair without creating a VSIX. Formatting, Clippy, workspace tests, repository checks and exact artifact verification remain release gates. Installed-platform evidence is recorded separately from development fixtures.

[Semantic index optimization](docs/semantic-index-optimization.md) explains the
1.6.0 switch to two bundled INT8 MiniLM exports: lower helper memory and CPU cost,
smaller weight assets, offline delivery, CPU selection and semantic-only rebuilding.

The [1.3.2](docs/release-1.3.2.md), [1.3.1](docs/release-1.3.1.md), [1.3.0](docs/release-1.3.0.md) and earlier release notes are historical records. Their direct-helper/window-scheduler instructions do not describe 1.4 defaults. Existing public releases are separate from this source candidate; publication requires successful exact-main CI and the release workflows.

AIRK is [Apache-2.0 licensed](LICENSE). Preserve [NOTICE](NOTICE), [third-party notices](THIRD_PARTY_NOTICES.md), original SPDX headers and dependency license bundles. Commits use the authorized human contributor's DCO sign-off.
