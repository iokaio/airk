# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

1.4.4 corrects Unix cleanup after a child exits naturally. Cleanup confirms that the owned child has exited and its process group has drained before releasing it; transient native errors remain bounded by the existing deadline.

1.4.4 also creates staged Windows runtime directories with explicit current-user ownership and owner-only access, including nested payload directories, so elevated installations satisfy the same strict ownership checks. Installed macOS runtime validation, self-test and retirement read the complete app payload from its outer root. Cold service starts coordinate registration-metadata setup before source watching.

1.4.4 corrects a fixed WAL limit that could block indexing and repeatedly fail recovery despite free disk space. WAL and shared-memory allowances now scale with admitted database capacity; retained reads and pending replay inputs are preserved. SQLite growth admission includes uncheckpointed pages. See the [storage-recovery release record](https://github.com/iokaio/airk/blob/main/docs/release-1.4.4.md). This source fix still requires installation and recovery verification in affected workspaces.

1.4.2 scales Balanced admission to one quarter of detected RAM (2–32 GiB), protects capacity for up to four project query workers, and raises Balanced source pacing to 64 MiB/s per job within a 256 MiB/s shared ceiling. Native watching supports 8,192 directories per project and 32,768 shared, with bounded incremental traversal and specific limit diagnostics. Existing manual resource overrides and storage quotas are preserved. These are admission policies, not hard OS memory or disk-I/O limits.

Installation and upgrades now display a VS Code progress notification with the current stage: version and payload checks, staging, native activation, agent connection updates, workspace setup and cleanup. Progress uses actual stages without an invented percentage. A connected-client preflight pauses the upgrade with a visible Retry Update action; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

Installation now shows a completion notification with MCP Servers, Reload Window and Show Details actions. Copilot users can restart AIRK from MCP Servers; external coding-agent sessions and existing terminals need their own restart. Routine activation does not repeat the notice for the same runtime, while explicit Install or Repair CLI shows it again. Connection or cleanup issues produce a warning instead of a clean success message. The dashboard retains restart guidance, including the distinction between an editor reload and a full restart required before a rejected VSIX reinstall.

Trusted VSIX installation and **Install or Repair CLI** refresh recognized existing workspace AIRK connections in Codex (`.codex/config.toml`), Claude Code (`.mcp.json`) and GitHub Copilot (`.vscode/mcp.json`) to the installed stable launcher and registered service arguments. Other servers, environment and tool policies remain intact; missing entries do not enable an agent. Foreign/custom launches, unsupported TOML, unsaved edits and unsafe files remain for manual review with a visible warning. Restart connected agents after installation. Targeted Windows installation is covered by the local installer harness; broader platform qualification remains pending.

The current package version is **1.4.4**, superseding the 1.4.2 resource-policy candidate. Validation and remaining qualification limits are recorded in its release notes.

CLI installation removes previous extension-managed terminal executable copies regardless of version. It also retires the previous native rollback payload after successful activation, preserving the selected runtime, sources, indexes and settings. Locked files report incomplete cleanup; disconnect old clients and use **AIRK: Install or Repair CLI**, or allow the next trusted activation to retry. Close old terminal sessions completely; **Developer: Reload Window** can preserve their environment.

Trusted VSIX activation now installs the native runtime and the `airk` terminal command, registers new workspace folders, prepares the bundled offline model and queues indexing with automatic watching. No setup links are required. Open a new VS Code terminal after installation; restart external terminal applications to refresh PATH. User `airk.cli.autoInstall: false` disables automatic installation and registration. Existing 1.4 registrations, selected index-data locations and watching preferences are preserved, including after a storage change. Compatible 1.4 runtime updates use the native fenced activation and schema-migration path; no fresh registration or reindex is requested for saved projects. Managed runtime ownership and explicit Stop remain authoritative. Use **Choose Index Storage Folder** or **Choose Native Automatic Watching** to change defaults later. Storage selection creates a reviewed binding and retains old index files for separate cleanup. Agent connection remains a separate explicit action.

Managed installations support a verified lightweight client launcher in user metadata while leaving the approved provider payload read-only and preserving its lifecycle policy. See the [native installation contract](https://github.com/iokaio/airk/blob/main/docs/service-installation.md).

**1.4.4 native service and storage recovery.** See the [release record](https://github.com/iokaio/airk/blob/main/docs/release-1.4.4.md). Earlier 1.4.0/1.4.1/1.4.2 qualification remains historical; this candidate requires its own validation. Publication is controlled by protected-main CI and the release workflows.

Private editor acceptance now exercises reviewed native registrations and Full/Terse panel choices. Development-stage tests and installed-VSIX qualification are recorded separately; neither implies native Mac qualification.

AIRK gives coding agents bounded, cited evidence from local code and documents. Its read-only MCP tools preserve indexed revision identities and original document locations. A native service owns indexing and resources; agents retrieve evidence directly from project workers. The offline inference helper embeds MiniLM and shares compatible model sessions across projects. No cloud service or API key is required.

## First registration

The 16-tool MCP catalog includes `airk_help`: ask for its topic catalog, a topic such as jobs/troubleshooting, an exact CLI command path or another tool's schema. `airk_status.work` separately reports scoped jobs, progress, up to 16 queued jobs, watcher observations and matched waits. Missing/stale samples remain explicit. The agent can explain management choices; MCP never executes them.

Use `airk_status` in your coding agent to inspect the connected MCP frontend, index worker and native service versions, process identities, executable/payload hashes and available build metadata. These observations identify running processes separately from the extension version and index extractor version.

Trust the workspace to start automatic setup. Use **AIRK: Index Now** for later explicit indexing. Automatic native watching continues after editor windows close; change it to manual with **AIRK: Choose Native Automatic Watching**. Login startup is a separate choice.

1.4 uses a fresh namespace and rebuilds indexes from sources. It does not convert or share live 1.3 stores, jobs or tokens. Existing indexes, old registrations and agent settings remain for explicit review. Allow additional disk space for the runtime pair and fresh indexes. Sources stay separate from derived state.

## Service Console

Open **AIRK: Connect / Reconnect Service** for project and worker observations, accepted jobs, phase/readiness information, sampled native wait reasons, source/helper failures, project settings, resource presets and logs. Closed registered projects and paused queued jobs remain visible; queued cancellation is explicit. Native maintenance has guarded previews and separate approval for execution. Runtime upgrades and rollback review exact approved targets and preserve saved recovery intent.

A deliberate Stop stays stopped until **Start service**. Reconnecting, advertising MCP definitions or closing a window does not silently repeat work. Uncertain requests keep their exact saved key for explicit recovery.

Old startup has a separate review and removal confirmation. The verified new runtime checks saved and native OS ownership without executing the selected old command. Startup removal preserves sources, settings and old indexes. A separate [offline native CLI preview and deletion](https://github.com/iokaio/airk/blob/main/docs/service-legacy-transition.md) retires owner-verified 1.3 derived files while preserving sources, settings and usage history. Process candidates remain observations; unknown registrations are preserved. Mac loaded-registration proof is implemented; actual native removal qualification remains pending.

The runtime contains two executables: lightweight `airk` service/client/worker roles and `airk-embed` inference with offline model assets. Native ownership is independent of the extension directory. Windows, Linux and both Mac architectures are targets; installed-platform qualification is recorded separately and native Mac/editor acceptance remains pending.

The Mac payload contains those same two executables in a GUI-less app bundle, targeting macOS 13.0. Optional signed-bundle login preserves system consent and refuses unverifiable registrations. Native Mac signing/login and installed-editor qualification remain pending.

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
