# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

The 1.4.5 branch also incorporates the native lifecycle and CI fixes from `origin/feat/airksvc`: bounded Unix child cleanup, owner-only staged Windows runtime directories, complete macOS payload-root validation and retirement, and a registration bootstrap gate before source watching. Routine CI retains correctness, build and package smoke checks; extended platform and lifecycle campaigns remain explicit qualification. Local qualification of this merged branch is still required.

**1.4.5: redb storage cutover.** Collections, lexical search, DiskANN records, publication, service registrations/jobs, usage/scheduler ledgers and private staging use redb. SQLite code and dependencies have been removed. Local qualification is in progress; see [validation and remaining limits](docs/release-1.4.5.md).

**Fresh setup and full indexing are required.** Previous SQLite indexes, registrations, pending jobs and usage are not imported. Native activation excludes the old service, workers and helpers before removing fixed AIRK-owned database files using ownership receipts. Selected project setup removes its recognized old database files under writer/read exclusion. Sources, unknown files, unrelated projects and separately stored settings are preserved. Trusted automatic setup repeats the saved workspace registration at its selected data path, prepares the offline model and queues indexing. Existing coding-agent sessions must reconnect.

Database files grow automatically within collection, root and job quotas. `airk_status` reports sampled logical lengths for `current.airk` and the collection usage ledger. **AIRK: Compact Index Storage…** opens a guarded selected-project preview, drains readers/writers, verifies the committed publication before and after compaction, and reports measured reclaimed bytes, including zero.

**Historical 1.4.4 behavior:** corrected a fixed WAL limit that could block indexing and repeatedly fail recovery despite free disk space. WAL and shared-memory allowances now scale with admitted database capacity; retained reads and pending replay inputs are preserved. SQLite growth admission includes uncheckpointed pages. See the [storage-recovery release record](docs/release-1.4.4.md). This source fix still requires installation and recovery verification in affected workspaces.

1.4.2 scales Balanced admission to one quarter of detected RAM (2–32 GiB), protects capacity for up to four project query workers, and raises Balanced source pacing to 64 MiB/s per job within a 256 MiB/s shared ceiling. Native watching supports 8,192 directories per project and 32,768 shared, with bounded incremental traversal and specific limit diagnostics. Existing manual resource overrides and storage quotas are preserved. These are admission policies, not hard OS memory or disk-I/O limits.

Installation and upgrades now display a VS Code progress notification with the current stage: version and payload checks, staging, native activation, agent connection updates, workspace setup and cleanup. Progress uses actual stages without an invented percentage. A connected-client preflight pauses the upgrade with a visible Retry Update action; success and failure remain explicit. VS Code owns the earlier VSIX installation UI, before AIRK can activate.

Installation now shows a completion notification with MCP Servers, Reload Window and Show Details actions. Copilot users can restart AIRK from MCP Servers; external coding-agent sessions and existing terminals need their own restart. Routine activation does not repeat the notice for the same runtime, while explicit Install or Repair CLI shows it again. Connection or cleanup issues produce a warning instead of a clean success message. The dashboard retains restart guidance, including the distinction between an editor reload and a full restart required before a rejected VSIX reinstall.

Trusted VSIX installation and **Install or Repair CLI** refresh recognized existing workspace AIRK connections in Codex (`.codex/config.toml`), Claude Code (`.mcp.json`) and GitHub Copilot (`.vscode/mcp.json`) to the installed stable launcher and registered service arguments. Other servers, environment and tool policies remain intact; missing entries do not enable an agent. Foreign/custom launches, unsupported TOML, unsaved edits and unsafe files remain for manual review with a visible warning. Restart connected agents after installation. Targeted Windows installation is covered by the local installer harness; broader platform qualification remains pending.

The preceding local candidate was **1.4.4**, superseding the 1.4.2 resource-policy candidate. Validation and remaining qualification limits are recorded in its release notes.

CLI installation removes previous extension-managed terminal executable copies regardless of version. It also retires the previous native rollback payload after successful activation, preserving the selected runtime, sources, indexes and settings. Locked files report incomplete cleanup; disconnect old clients and use **AIRK: Install or Repair CLI**, or allow the next trusted activation to retry. Close old terminal sessions completely; **Developer: Reload Window** can preserve their environment.

Trusted VSIX activation now installs the native runtime and the `airk` terminal command, registers new workspace folders, prepares the bundled offline model and queues indexing with automatic watching. No setup links are required. Open a new VS Code terminal after installation; restart external terminal applications to refresh PATH. User `airk.cli.autoInstall: false` disables automatic installation and registration. Compatible runtime updates preserve registered projects and saved choices. The 1.4.5 storage boundary requires fresh registration/full indexing, retains the selected data location, and enables native watching for that new registration. Managed runtime ownership and explicit Stop remain authoritative. Use **Choose Index Storage Folder** or **Choose Native Automatic Watching** to change defaults later. Storage selection creates a reviewed binding and retains old index files for separate cleanup. Agent connection remains a separate explicit action.

**Historical 1.4.4 release candidate.** See the [release record](docs/release-1.4.4.md). Earlier 1.4.0/1.4.1/1.4.2 qualification remains historical; this candidate requires its own validation. Its publication record remains historical; current publication is controlled by protected-main CI and the release workflows. [Routine CI](docs/ci-and-builds.md) runs the full workspace on Linux, focused Windows/macOS regressions and native/VSIX build and smoke gates; extended platform and lifecycle qualification is opt-in.

The [Windows build and editor harness](docs/windows-local-build.md) now uses the verified runtime pair and reviewed native registrations. An allowlisted development stage can be exercised before creating a VSIX; final artifact acceptance remains separate.

AIRK indexes local code and documents and serves bounded, cited evidence to coding agents through read-only MCP tools. It preserves original document locations and exact indexed revision identities. It provides native syntax facts and candidate relationships without claiming complete language understanding or resolved references. Original files remain outside derived storage.

## Native service ownership

A per-user service brokers authenticated connections and owns registration, durable work and resource policy. Retrieval flows directly between clients and project workers. Project workers also talk directly to the shared inference helper; the broker does not forward evidence, source text, model weights or embeddings.

The approved runtime pair contains two executables: a lightweight `airk` for native service/client/worker roles, and `airk-embed` for inference and embedded offline MiniLM assets. Compatible projects share verified model files and loaded sessions. Inference starts when needed; connecting a read-only client does not load a model. Native operation targets Windows, Linux and both Mac architectures. Windows/Linux x64 and private Windows editor evidence is recorded separately; native Mac and the complete installed-platform matrix remain unqualified.

The Mac layout uses a [GUI-less signed app bundle](docs/service-macos-login.md) containing those same two executables, with a macOS 13.0 deployment baseline and optional bundle-scoped login registration. Portable ownership/package checks pass; native SDK, signing, lifecycle and installed-platform qualification remain pending.

Each project has a canonical source, separate data root, collection name and opaque service binding. Registration reserves disjoint storage before creating it. Sources cannot overlap registered data, runtime state, control or bound logs. Native workers own indexing and source watching. Automatic watching explicitly continues after editor windows close; login startup is a separate opt-in choice.

## Start with a fresh 1.4 namespace

The [native management clients](docs/service-native-management.md) provide direct usage/settings reads, bounded exports and guarded project changes. Usage & Savings waits for native completion; imports bind the reviewed input bytes, and shared quota updates reject stale policies.

1.4 is a clean break. It does not read, convert or share live 1.3 indexes, journals, jobs or revision tokens. Old state and agent configurations remain for explicit review. Fresh indexing requires additional disk space. Known older workers can still compete for resources; review and stop their owned registrations separately.

In the extension, trusted activation sets up new workspace roots automatically. Use **AIRK: Index Now** for later explicit indexing and **AIRK: Choose Native Automatic Watching** to switch between manual and automatic watching. **AIRK Service Console** shows cached observations, readiness, jobs, sampled native wait reasons, independent source/helper state, project settings, guarded repairs and lifecycle controls. Queued jobs remain visible while paused and can be canceled explicitly. Start is explicit after a deliberate Stop; reconnect cannot clear that preference. The console also offers Terse/Full response mode, reviewed runtime upgrades/rollback and saved-request recovery.

Project settings offer [guarded unregister](docs/service-project-retirement.md), including paused projects. It stops new admission and drains the selected owner while preserving source files, settings and index data. The console's **Unregistered projects / cleanup** view separately previews and deletes selected owned index state, preserves saved settings and sources, and resumes interrupted cleanup before re-registration.

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

`airk_help` adds a 35-topic usage catalog, generated public CLI command help and current MCP tool schemas. `airk_status.work` independently reports project jobs, bounded queued work, cached phase/counters, source-watcher status and matched sampled waits. Help explains management options; these MCP tools do not change job state. All 16 tools remain read-only.

`airk_status` includes a runtime JSON bundle for the MCP frontend, index worker and native service: compiled versions, process identities, executable/payload hashes and available build receipt metadata. Missing observations remain explicit; see the [MCP contract](docs/mcp.md).

MCP provides bounded file discovery, outlines, symbols, code/document facts, imports, references, relationships, search, reads, context, capabilities and changes. **Terse is the default.** It sends the exact token once, uses shared source bindings and `items`, and avoids repeated static diagnostics. Full mode supplies detailed provenance, coverage and readiness for audits or surprising results; agents can request it per call. Compact navigation uses names, kinds, lines and signatures. Syntax call targets remain labeled candidates.

Results describe indexed evidence. `airk_changes` calls its validation `index-current`; this is not a live-file verification. Change baselines retain bounded metadata across up to 64 publications/24 hours, with explicit coverage floors under pressure. Reads retain usable prior generations during publication or disclose a finite retry hint. See [response modes](docs/mcp-response-modes.md) and the [MCP contract](docs/mcp.md).

Storage uses independent collection ownership, admitted publication/replacement space, bounded readers and conservative recovery. DiskANN is the selected disk-backed vector engine; bootstrap lexical/retained-snapshot adapters are not a substitute for that engine. Scoped repair preserves original sources and reports partial failures. See [publication](docs/service-publication.md), [storage ownership](docs/service-storage-ownership.md) and [maintenance plans](docs/service-maintenance.md).

## Development and records

Read [CONTRIBUTING.md](CONTRIBUTING.md), [architecture and dependency boundaries](docs/architecture.md) and the [documentation index](docs/README.md). The runtime builder stages a host-native executable pair without creating a VSIX. Formatting, Clippy, workspace tests, repository checks and exact artifact verification remain release gates. Installed-platform evidence is recorded separately from development fixtures.

The [1.3.2](docs/release-1.3.2.md), [1.3.1](docs/release-1.3.1.md), [1.3.0](docs/release-1.3.0.md) and earlier release notes are historical records. Their direct-helper/window-scheduler instructions do not describe 1.4 defaults. Existing public releases are separate from this unpushed development branch.

AIRK is [Apache-2.0 licensed](LICENSE). Preserve [NOTICE](NOTICE), [third-party notices](THIRD_PARTY_NOTICES.md), original SPDX headers and dependency license bundles. Commits use the authorized human contributor's DCO sign-off.
