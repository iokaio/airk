# AIRK — Artifact Indexing and Retrieval Kernel, the Local MCP Power Tool for Coding Agents

**1.4.0 development.** This working tree now uses the native service for extension and CLI operation. The complete implementation, container/package cutover, testing and review are still in progress. No final 1.4 VSIX has been built. See the [implementation plan](docs/airkservice-architecture.md), [completion checklist](docs/service-implementation-checklist.md) and [development release record](docs/release-1.4.0.md).

The [Windows build and editor harness](docs/windows-local-build.md) now uses the verified runtime pair and reviewed native registrations. An allowlisted development stage can be exercised before creating a VSIX; final artifact acceptance remains separate.

AIRK indexes local code and documents and serves bounded, cited evidence to coding agents through read-only MCP tools. It preserves original document locations and exact indexed revision identities. It provides native syntax facts and candidate relationships without claiming complete language understanding or resolved references. Original files remain outside derived storage.

## Native service ownership

A per-user service brokers authenticated connections and owns registration, durable work and resource policy. Retrieval flows directly between clients and project workers. Project workers also talk directly to the shared inference helper; the broker does not forward evidence, source text, model weights or embeddings.

The approved runtime pair contains two executables: a lightweight `airk` for native service/client/worker roles, and `airk-embed` for inference and embedded offline MiniLM assets. Compatible projects share verified model files and loaded sessions. Inference starts when needed; connecting a read-only client does not load a model. Native operation targets Windows, Linux and both Mac architectures. Windows/Linux development evidence is recorded in the component contracts; native Mac and installed-editor qualification remain pending.

The Mac layout uses a [GUI-less signed app bundle](docs/service-macos-login.md) containing those same two executables, with a macOS 13.0 deployment baseline and optional bundle-scoped login registration. Portable ownership/package checks pass; native SDK, signing, lifecycle and installed-platform qualification remain pending.

Each project has a canonical source, separate data root, collection name and opaque service binding. Registration reserves disjoint storage before creating it. Sources cannot overlap registered data, runtime state, control or bound logs. Native workers own indexing and source watching. Automatic watching explicitly continues after editor windows close; login startup is a separate opt-in choice.

## Start with a fresh 1.4 namespace

The [native management clients](docs/service-native-management.md) provide direct usage/settings reads, bounded exports and guarded project changes. Usage & Savings waits for native completion; imports bind the reviewed input bytes, and shared quota updates reject stale policies.

1.4 is a clean break. It does not read, convert or share live 1.3 indexes, journals, jobs or revision tokens. Old state and agent configurations remain for explicit review. Fresh indexing requires additional disk space. Known older workers can still compete for resources; review and stop their owned registrations separately.

In the extension, use **AIRK: Index Now** to review a trusted root and choose manual or automatic native watching. **AIRK Service Console** shows cached observations, readiness, jobs, independent source/helper state, project settings, guarded repairs and lifecycle controls. Start is explicit after a deliberate Stop; reconnect cannot clear that preference. The console also offers Terse/Full response mode, reviewed runtime upgrades/rollback and saved-request recovery.

Project settings offer [guarded unregister](docs/service-project-retirement.md), including paused projects. It stops new admission and drains the selected owner while preserving source files, settings and index data. The console's **Unregistered projects / cleanup** view separately previews and deletes selected owned index state, preserves saved settings and sources, and resumes interrupted cleanup before re-registration.

Native project settings can reset scan-limit overrides to automatic limits. Native quotas apply to manual and automatic work; retired editor timer/soft-budget settings no longer appear. Workspace tree actions use the clicked project, and native usage counts performed source, semantic and replacement work independently of job acceptance.

The MCP provider advertises only reviewed bindings. **Connect Coding Agent** creates a stable-launcher configuration on request and preserves unrelated client settings. Recognized legacy JSON entries have an [explicit diff and guarded replacement](docs/service-legacy-transition.md); TOML and other agent formats use manual review. Agent connection is separate from the extension's bounded read-only MCP verification.

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

MCP provides bounded file discovery, outlines, symbols, code/document facts, imports, references, relationships, search, reads, context, capabilities and changes. **Terse is the default.** It sends the exact token once, uses shared source bindings and `items`, and avoids repeated static diagnostics. Full mode supplies detailed provenance, coverage and readiness for audits or surprising results; agents can request it per call. Compact navigation uses names, kinds, lines and signatures. Syntax call targets remain labeled candidates.

Results describe indexed evidence. `airk_changes` calls its validation `index-current`; this is not a live-file verification. Change baselines retain bounded metadata across up to 64 publications/24 hours, with explicit coverage floors under pressure. Reads retain usable prior generations during publication or disclose a finite retry hint. See [response modes](docs/mcp-response-modes.md) and the [MCP contract](docs/mcp.md).

Storage uses independent collection ownership, admitted publication/replacement space, bounded readers and conservative recovery. DiskANN is the selected disk-backed vector engine; bootstrap lexical/retained-snapshot adapters are not a substitute for that engine. Scoped repair preserves original sources and reports partial failures. See [publication](docs/service-publication.md), [storage ownership](docs/service-storage-ownership.md) and [maintenance plans](docs/service-maintenance.md).

## Development and records

Read [CONTRIBUTING.md](CONTRIBUTING.md), [architecture and dependency boundaries](docs/architecture.md) and the [documentation index](docs/README.md). The runtime builder stages a host-native executable pair without creating a VSIX. Formatting, Clippy, workspace tests, repository checks and exact artifact qualification remain release gates. Installed-platform evidence is recorded separately from development fixtures.

The [1.3.2](docs/release-1.3.2.md), [1.3.1](docs/release-1.3.1.md), [1.3.0](docs/release-1.3.0.md) and earlier release notes are historical records. Their direct-helper/window-scheduler instructions do not describe 1.4 defaults. Existing public releases are separate from this unpushed development branch.

AIRK is [Apache-2.0 licensed](LICENSE). Preserve [NOTICE](NOTICE), [third-party notices](THIRD_PARTY_NOTICES.md), original SPDX headers and dependency license bundles. Commits use the authorized human contributor's DCO sign-off.
