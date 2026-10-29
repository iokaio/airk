# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

**Version 1.6.0 — offline semantic index optimization.** Two bundled INT8 MiniLM exports replace the previous FP32 model. Native CPU detection chooses unsigned INT8 on x64 without VNNI and signed INT8 on VNNI x64 and ARM64. Installation and rebuilding require no model download. Upgrade rebuilds semantic vectors and caches while preserving text/fact evidence, pause and semantic opt-out settings. See the [1.6.0 record](https://github.com/iokaio/airk/blob/main/docs/release-1.6.0.md) and [optimization rationale](https://github.com/iokaio/airk/blob/main/docs/semantic-index-optimization.md).

The following management and agent improvements were introduced in 1.5.12 and remain available.

Management → Agents now offers **Copy instructions** and separate buttons to open Claude Code (`CLAUDE.md`), Codex (`AGENTS.md`) and GitHub Copilot (`.github/copilot-instructions.md`) project instructions. The suggestion teaches agents to discover deferred AIRK tools, confirm status, use focused evidence and verify live files. Existing files stay unchanged; missing files open as unsaved drafts for review and saving. The bundled **Project instructions for AI agents** help topic also covers other assistants.

Version **1.5.12** includes the index-management and startup improvements described below. Explorer opens while runtime checks continue in the background. Healthy completed installations reconnect without running native startup again, and simultaneous discovery requests share work. Agent configuration uses current native registrations. The Agents panel displays connection-check stages, elapsed time and copyable results for the unified 26-tool connection. The full inline agent setup guide remains a proposal. See the [1.5.12 release record](https://github.com/iokaio/airk/blob/main/docs/release-1.5.12.md).

AIRK 1.5.12 also ties MCP process lifetime to its client and gives installed service, index and source workers distinct executable names. Role copies are created locally; downloads still include one main engine. Live agents keep independent MCP connections, while background indexing remains service-owned.

Overview and Management now lead with published files, text chunks, vectors and semantic file coverage. Current work appears separately with counters appropriate to source indexing, semantic batches or cleanup. The display updates automatically; starting another attempt does not replace saved index counts with zeros.

The 1.5.10 candidate adds **Open raw diagnostic log** and **Save raw diagnostic log…**, continued cleanup of obsolete index objects, explicit failed repair results, and storage-format identities independent of the AIRK release version. Management Storage reports the format and cleanup progress. Existing owned storage locations remain compatible; fresh installations use separate runtime, index-v1 data and service-v1 state defaults. See the [candidate record](https://github.com/iokaio/airk/blob/main/docs/release-1.5.10.md).

Version 1.5.9 fixes MCP discovery followed by a legacy handshake: a delayed discovery response no longer leaves a successfully initialized connection rejecting its tool requests. Protocol state stays per connection, so assistants can share one project using different handshake styles. Modern requests still require valid metadata. See the [1.5.9 compatibility record](https://github.com/iokaio/airk/blob/main/docs/release-1.5.9.md).

One **AIRK MCP connection** now exposes 26 unique tools without repeated `airk_` prefixes: evidence reads, documentation and reviewed UI drafts. `search` returns indexed evidence; `draft_search` prepares a form for user review. `status` reports index health separately from Explorer attachment, and multi-root connections require an explicit configured project. Trusted startup replaces recognized earlier Retrieval and Explorer definitions together with one `airk` entry. See the [unified catalog and migration contract](https://github.com/iokaio/airk/blob/main/docs/unified-mcp.md).

Failed jobs observed by the service now produce retained `job_failed` events with the exact job and safe failure code. Index preparation and discovery preserve stage and recognized native causes. Older generic failures cannot be reconstructed; unknown causes remain explicit.

**Management features introduced in 1.5.12.** Management uses consistent horizontal tabs for Indexing, Activity, Storage, Recovery, Service, Agents, Usage and Logs. Indexing contains a health banner, current progress, published search availability, and three everyday settings: search mode, updates and evidence detail. Semantic enablement verifies/prepares the offline model before saving the policy and requests source and semantic work. Registration records native setup durably, so closing the editor does not strand a newly registered project before its initial index requests. See the [1.5.8 development record](https://github.com/iokaio/airk/blob/main/docs/release-1.5.8.md).

Queued repair requests whose plan has finished or lost its owner are settled without replaying old approvals, so they cannot indefinitely block otherwise eligible source work. Overview and Management update automatically while visible. Management exposes plan gates, confirms manual diagnostic status checks, separates old counters from queued work, and provides Copy details for troubleshooting panels.

Updates distinguish the installed extension, the running interface and native runtime activation. A detected pending extension update presents one **Reload to finish update** action and retains that action in Explorer and the status bar. Runtime progress says Installing or Updating AIRK runtime; successful activation finishes quietly and project indexing remains in Management. VS Code controls extension replacement, so an already running version without these changes retains its earlier messages until reloaded.

Indexing recovery now belongs to the native service: accepted search/detail settings finish without the editor, temporary source/storage failures wait for restored conditions, and worker/helper retry budgets survive service restarts. Version 1.5.7 corrects a completed-bootstrap marker that repeatedly requested recovery, resumes outstanding source work after successful recovery, and prevents source hints from replenishing repeated failure budgets. Pause, Cancel, manual updates, custom models and guarded cleanup remain authoritative. Management can open/copy/save a captured diagnostic report and export the full retained service log ring. Verified completed installations reopen quietly without repeating terminal installation or runtime cleanup.

Studio **Save** writes the complete draft inputs to `.airk/strategies/<name>.studio.airk.json` in the selected local project without a folder picker. It preserves the objective, parameters, path scope, steps, prompt profile and edited prompt under a `user/<name>` definition. Save again to update it, or change the name for a copy; outside edits are guarded. Browse saved strategies restores these inputs, with evidence recollected after loading. Accepting a suggestion does not save automatically. **Export…** continues to save only a strategy definition.

Management separates current work from published evidence. Captured files, newly extracted payloads, publications, phase time and last advancement remain distinct; unknown totals stay unknown. Each Management section has its own tab; inactive sections are unmounted, and project selection persists across tabs. Existing Management URLs open the corresponding tab. Removal preserves source files and settings; deleting retained index data requires a separate preview and confirmation.

**Published 1.5.4:** Installer upgrades stop verified previous AIRK processes, including older versions and Windows clients holding the launcher open. Existing watcher metadata migrates at service startup; shutdown ownership checks read saved metadata without migration and preserve the actual failure cause. See the [1.5.4 development record](https://github.com/iokaio/airk/blob/main/docs/release-1.5.4.md). Earlier candidate and validation records retain their original scope. This release also carries the previously unpublished 1.5.3 semantic preparation fix, Semantic/Hybrid search controls, missing-source project removal and bounded watcher directory diagnostics.

Explorer review fixes in this 1.5.0 local candidate guard plan saves, keep evidence and drafts bound to their project/publication, settle failed parallel reads, refuse expired proposals and preserve complete task instructions. Remote plan saving uses an untitled draft and the editor Save command. Platform qualification is recorded separately in the 1.5.0 release notes.

Explorer help, template discovery, dry-run validation, reviewed proposals and shared snapshots are available through the unified AIRK connection. Help searches offline UI and engine guidance for supported workflows, indexing choices and document types. Planner can load Markdown as unverified drafts or complete plan JSON. Friendly failures release waiting controls and explain recovery without replaying actions.

**1.5.0 adds the AIRK Explorer**, an editor workbench that replaces the Service Console: search and inspect indexed evidence, review Git changes against the index, build evidence strategies and agent plans, and manage project indexing and the native service. The original 1.5.0 **Explorer Assistance** server answered how-to questions about the Explorer and delivered inert drafts to a Suggestions Inbox; it never calls AIRK Retrieval, runs commands or writes files. That release used a separate connection; 1.5.7 consolidates it into the unified endpoint. External agents reach AIRK through the stable launcher, so no system Node is needed. See the [1.5.0 release record](https://github.com/iokaio/airk/blob/main/docs/release-1.5.0.md) and the [extension guide](https://github.com/iokaio/airk/blob/main/docs/vscode-extension.md).

Open it with **AIRK: Open Explorer** or the AIRK activity bar. **Connect a Coding Agent** configures one AIRK endpoint; VS Code offers one workspace definition. The old separate Assistance provider withdraws its entries.

Historical 1.4.4 release: corrects Unix cleanup after a child exits naturally. Cleanup confirms that the owned child has exited and its process group has drained before releasing it; transient native errors remain bounded by the existing deadline.

1.4.4 also creates staged Windows runtime directories with explicit current-user ownership and owner-only access, including nested payload directories, so elevated installations satisfy the same strict ownership checks. Installed macOS runtime validation, self-test and retirement read the complete app payload from its outer root. Cold service starts coordinate registration-metadata setup before source watching.

1.4.4 corrects a fixed WAL limit that could block indexing and repeatedly fail recovery despite free disk space. WAL and shared-memory allowances now scale with admitted database capacity; retained reads and pending replay inputs are preserved. SQLite growth admission includes uncheckpointed pages. See the [storage-recovery release record](https://github.com/iokaio/airk/blob/main/docs/release-1.4.4.md). This source fix still requires installation and recovery verification in affected workspaces.

1.4.2 scales Balanced admission to one quarter of detected RAM (2–32 GiB), protects capacity for up to four project query workers, and raises Balanced source pacing to 64 MiB/s per job within a 256 MiB/s shared ceiling. Native watching supports 8,192 directories per project and 32,768 shared, with bounded incremental traversal and specific limit diagnostics. Existing manual resource overrides and storage quotas are preserved. These are admission policies, not hard OS memory or disk-I/O limits.

Installation and upgrades now display a VS Code progress notification with the current stage: version and payload checks, staging, native activation, agent connection updates, workspace setup and cleanup. Progress uses actual stages without an invented percentage. On Windows and Linux, an admitted upgrade hard-stops verified preceding AIRK processes and disconnects their clients; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

Successful runtime activation finishes quietly. A detected extension update offers **Reload to finish update**; external coding-agent sessions may need their own restart to load the new tool connection. Connection or cleanup issues produce a specific warning and details instead of a clean success message.

Trusted startup and **Install or Repair CLI** migrate recognized project-local agent connections to one stable-launcher endpoint. Codex, Claude Code, Copilot, Gemini, Antigravity, Cursor, Roo and Junie files are supported. Other servers, environment values and disabled state are preserved; conflicts, unsafe files and unsaved edits remain for review. Restart the coding agent and approve the renamed tools after migration.

The current development package version is **1.5.12**. The published **1.5.4** release and earlier candidate records retain their original validation scope.

CLI installation removes previous extension-managed terminal executable copies regardless of version. It also retires the previous native rollback payload after successful activation, preserving the selected runtime, sources, indexes and settings. Locked files report incomplete cleanup; disconnect old clients and use **AIRK: Install or Repair CLI**, or allow the next trusted activation to retry. Close old terminal sessions completely; **Developer: Reload Window** can preserve their environment.

Trusted VSIX activation now installs the native runtime and the `airk` terminal command, registers new workspace folders, prepares the bundled offline model and queues indexing with automatic watching. No setup links are required. Open a new VS Code terminal after installation; restart external terminal applications to refresh PATH. User `airk.cli.autoInstall: false` disables automatic installation and registration. Existing 1.4 registrations, selected index-data locations and watching preferences are preserved, including after a storage change. Compatible 1.4 runtime updates use the native fenced activation and schema-migration path; no fresh registration or reindex is requested for saved projects. Managed runtime ownership and explicit Stop remain authoritative. Use **Choose Index Storage Folder** or **Choose Native Automatic Watching** to change defaults later. Storage selection creates a reviewed binding and retains old index files for separate cleanup. Agent connection remains a separate explicit action.

Managed installations support a verified lightweight client launcher in user metadata while leaving the approved provider payload read-only and preserving its lifecycle policy. See the [native installation contract](https://github.com/iokaio/airk/blob/main/docs/service-installation.md).

**1.4.4 native service and storage recovery.** See the [release record](https://github.com/iokaio/airk/blob/main/docs/release-1.4.4.md). Earlier 1.4.0/1.4.1/1.4.2 qualification remains historical; this candidate requires its own validation. Publication is controlled by protected-main CI and the release workflows. [Routine CI](https://github.com/iokaio/airk/blob/main/docs/ci-and-builds.md) runs the full workspace on Linux, focused Windows/macOS regressions and all native/VSIX build and smoke gates; full platform and multi-project lifecycle qualification is opt-in.

Private editor acceptance now exercises reviewed native registrations and Full/Terse panel choices. Development-stage tests and installed-VSIX qualification are recorded separately; neither implies native Mac qualification.

AIRK gives coding agents bounded, cited evidence from local code and documents. Its read-only MCP tools preserve indexed revision identities and original document locations. A native service owns indexing and resources; agents retrieve evidence directly from project workers. The offline inference helper embeds MiniLM and shares compatible model sessions across projects. No cloud service or API key is required.

## First registration

The unified 26-tool catalog includes `help` for usage topics, CLI command help and public tool schemas. `status.work` separately reports scoped jobs, progress, bounded queues, watcher observations and matched waits. Missing/stale samples remain explicit. Draft tools can propose management guidance; the user performs management actions.

Use `airk_status` in your coding agent to inspect the connected MCP frontend, index worker and native service versions, process identities, executable/payload hashes and available build metadata. These observations identify running processes separately from the extension version and index extractor version.

Trust the workspace to start automatic setup. Use **AIRK: Index Now** for later explicit indexing. Automatic native watching continues after editor windows close; change it to manual with **AIRK: Choose Native Automatic Watching**. Login startup is a separate choice.

1.4 uses a fresh namespace and rebuilds indexes from sources. It does not convert or share live 1.3 stores, jobs or tokens. Existing indexes, old registrations and agent settings remain for explicit review. Allow additional disk space for the runtime pair and fresh indexes. Sources stay separate from derived state.

## Service Console

Open **AIRK: Connect / Reconnect Service** for project and worker observations, accepted jobs, phase/readiness information, sampled native wait reasons, source/helper failures, project settings, resource presets and logs. Closed registered projects and paused queued jobs remain visible; queued cancellation is explicit. Native maintenance has guarded previews and separate approval for execution. Runtime upgrades and rollback review exact approved targets and preserve saved recovery intent.

A deliberate Stop stays stopped until **Start service**. Reconnecting, advertising MCP definitions or closing a window does not silently repeat work. Uncertain requests keep their exact saved key for explicit recovery.

Old startup has a separate review and removal confirmation. The verified new runtime checks saved and native OS ownership without executing the selected old command. Startup removal preserves sources, settings and old indexes. A separate [offline native CLI preview and deletion](https://github.com/iokaio/airk/blob/main/docs/service-legacy-transition.md) retires owner-verified 1.3 derived files while preserving sources, settings and usage history. Process candidates remain observations; unknown registrations are preserved. Mac loaded-registration proof is implemented; actual native removal qualification remains pending.

The runtime contains two executables: lightweight `airk` service/client/worker roles and `airk-embed` inference with offline model assets. Native ownership is independent of the extension directory. Windows, Linux and both Mac architectures are targets; installed-platform qualification is recorded separately and native Mac/editor acceptance remains pending.

The Mac payload contains those same two executables in a GUI-less app bundle, targeting macOS 13.3. Optional signed-bundle login preserves system consent and refuses unverifiable registrations. Native Mac signing/login and installed-editor qualification remain pending.

## Coding agents and response mode

Use **AIRK: Connect Coding Agent**. The guide supplies stable-launcher arguments for the reviewed collection and preserves unrelated settings. Recognized old project JSON entries have an explicit diff and approved replacement with fresh file/trust checks. TOML and other formats require manual review. In remote windows, configure the agent on the same execution host as its sources.

**Terse is the default.** The Service Console offers **Full** for provenance audits, parser/coverage limitations, readiness diagnosis and surprising results. Agents can request `response_mode: full` for one call. Compact navigation keeps names, kinds, lines and signatures; fact tools share bindings and consistently use `items`.

MCP verification checks the bounded native connection; it does not prove a coding agent has enabled that server. Results describe indexed evidence. `index-current` does not mean current live files. Syntax references and method-name matches remain candidates.

## Commands and settings

**Usage & Savings** reads the collection directly and supports JSON, CSV, Markdown and explicit price assumptions. Recording, clear, import and feedback actions use guarded durable native jobs and wait for completion. Imported measurements bind the bytes shown in their preview. Source/extraction limits and shared storage quotas are available through the native project configuration view.

Index Now accepts durable native work. Rebuild and maintenance commands open guarded repair previews. The tools terminal and status use the direct project client. The CLI PATH command points to the approved stable launcher and preserves existing entries. Project settings and automatic watching are native service controls, replacing the former window-owned indexing modes and direct-store maintenance paths.

Project settings also offer approved **Unregister project**, including paused projects. It stops new connections and automatic work, drains the owned worker and preserves sources, settings and index data. **Unregistered projects / cleanup** separately previews selected owned index state and requires explicit deletion approval. Native cleanup preserves sources and saved settings, refuses unknown files/links, and resumes interrupted removal before re-registration. Shared models and usage history remain available.

The settings page and walkthrough describe native manual/automatic watching. Retired editor idle/save timers and soft storage-budget settings are removed. Native project settings can reset a source-scan override to automatic limits; native quotas continue to apply to manual and automatic work.

Only user/machine settings select installation policy and storage preferences. A workspace cannot nominate an executable, approval digest or service endpoint. Changing a storage preference does not move an existing index: a new native binding requires review. The previous collection remains independently registered.

## Scope and records

AIRK indexes supported local source/document formats and preserves original citations. It does not claim complete language understanding, resolved references or live-file verification. Models and calibration inputs stay local; usage measurements are content-free, while diagnostic reports redact paths by default.

Read the [service plan](https://github.com/iokaio/airk/blob/main/docs/airkservice-architecture.md), [client adapter contract](https://github.com/iokaio/airk/blob/main/docs/service-client-adapters.md), [installation](https://github.com/iokaio/airk/blob/main/docs/service-installation.md) and [response modes](https://github.com/iokaio/airk/blob/main/docs/mcp-response-modes.md). The 1.3 release notes and their direct-helper instructions are historical. This development branch is not a Marketplace publication.
