# Split inventory: VS Code extension (P0.2)

Status: Phase 0 draft, 11 November 2026; source review only; authorizes no deletion.

The dated Phase 0 inventory below is retained as history. For the advanced
split's current shared-presentation ownership, compatibility obligations and
proposed pilot, use [§12](#12-p6a-shared-presentation-inventory--13-november-2026).
In particular, the historical command counts and blanket prohibition on
extension-assisted installation do not describe the current approved plan.

Scope: every extension surface in `packages/vscode` at 1.8.23 on `feat/split`
(122 production `.ts`/`.tsx` files, 29,721 lines, plus two webview stylesheets
and 43 help topics). Governing sections: [split plan](../split-plan.md) §4.4,
§7.3, §8, §10.3, §10.4 and decisions U1–U71.

Markers: **[V]** read in source or stated by the plan; **[I]** inference or
proposal that still needs review. Facts columns (lines, callers, today's
behavior) are [V] unless marked. A disposition is [V] only when a plan section
dictates it. A "remove" row is not deletion evidence: deletion still needs a
simplification-plan evidence class and the U61 independent review.

Admin page names are the §7.3 groups: Overview, Index library (and index sets),
Activity, Storage, Recovery, Service, Agents, Usage, Logs, Search and
evidence, Help. An "admin link" opens the page through `airkctl admin open`
(or `admin url`), so it is itself an `airkctl` one-shot and must fall back to
the §7.3 "After Stop" guidance when the service is not running.

No AIRK MCP evidence was used: the `airk` MCP server failed to connect in
this session; all findings come from local reads (`rg`/`grep`, file reads).

## 1. Commands

59 identifiers: 57 contributed in `package.json` plus 2 registered in
`serviceExtension.ts` (`commands` table, ~:3423–3459) but not contributed.
`COMMAND_ROUTES` (`explorerCommands.ts`) maps 17 of them to Explorer
Management routes; `NATIVE_COMMANDS` lists the 34 gated by
`airk.nativeCommandsAvailable`.

| # | Command | Today [V] | Disposition | Surviving owner or admin page |
|---|---|---|---|---|
| 1 | `airk.explorer.open` | Opens Explorer panel at a route | keep (feature) | Explorer |
| 2 | `airk.explorer.search` | Prompts and runs Explorer search | keep via airkctl session | session evidence read |
| 3 | `airk.explorer.showSuggestions` | Opens Suggestions Inbox | keep (feature) | Inbox; drafts arrive over session (U10) |
| 4 | `airk.installCli` | `repairInstallation()` → explicit `automaticInstall` | guidance-only (must not install) [V §8.5] | §8.3 "not installed" guidance, **Download AIRK Service** |
| 5 | `airk.enableNativeExecution` | Sets `airk.runtime.nativeExecution` | keep (feature) | gate for `airkctl` (`nativeGate.ts`) |
| 6 | `airk.setUp` | `repairInstallation()` | guidance-only (must not install) [V §8.5] | same as #4 |
| 7 | `airk.removeTerminalCommand` | Removes extension-written PATH/shell blocks | remove [V §8.5] | service installer removes old extension PATH/shell edits by receipt [V §9.3] |
| 8 | `airk.showLastUninstallResult` | Reads `<state>/extension-uninstall/last-result.json` | remove [I] | none: `vscode:uninstall` no longer runs native uninstall [V §8.5] |
| 9 | `airk.reviewData` | `runtime inventory` / `runtime purge` one-shots | admin link | **Storage** (`airkctl service inventory/purge` [V §6.2]) |
| 10 | `airk.openTools` | Terminal running `airk tools … shell` | keep via airkctl one-shot | editor terminal runs verified `airkctl tools shell` [V §4.3] |
| 11 | `airk.treeRefresh` | Refreshes fallback tree | keep via airkctl session [I] | status refresh |
| 12 | `airk.treeIndex` | Tree inline: enqueue index job | admin link | **Activity** |
| 13 | `airk.treeReindex` | Tree inline: repair `rebuild` | admin link | **Recovery** |
| 14 | `airk.treePause` | Tree inline: pause | admin link | **Activity** |
| 15 | `airk.treeRemove` | Route `#manage/storage` | admin link | **Storage** |
| 16 | `airk.menu` | Route `#overview` (Explorer overview) | keep (feature) | Explorer |
| 17 | `airk.index` | Enqueue index job, open `#manage/activity` | admin link [V §8.5] | **Activity** |
| 18 | `airk.reindex` | Repair `rebuild` preview, `#manage/repair` | admin link | **Recovery** |
| 19 | `airk.indexMode` | `configure_watch` quick pick | admin link | **Index library** (policies) [I] |
| 20 | `airk.pause` | `pause` rpc | admin link | **Activity** |
| 21 | `airk.resume` | `pause:false` rpc | admin link | **Activity** |
| 22 | `airk.chooseStorage` | Opens `#indexing/library` | admin link [V §8.5] | **Index library** |
| 23 | `airk.collections` | Route `#manage/projects` (alias → overview) | admin link | **Overview** |
| 24 | `airk.removeCollection` | Route `#manage/storage` | admin link | **Storage** |
| 25 | `airk.extractionPolicy` | Route `#manage/indexing` (alias → repair) | admin link | **Index library** [I] |
| 26 | `airk.ingestionLimits` | Route `#manage/indexing` | admin link | **Index library** [I] |
| 27 | `airk.configureIngestionLimits` | Route `#manage/indexing` | admin link | **Index library** [I] |
| 28 | `airk.configureExtractionPolicy` | Route `#manage/indexing` | admin link | **Index library** [I] |
| 29 | `airk.status` | `tools call airk_status` to output channel | keep via airkctl session | session read (`airkctl status` equivalent) |
| 30 | `airk.usage` | Usage webview panel | admin link | **Usage** |
| 31 | `airk.setupAgent` | TypeScript writers + HTTPS enrollment | keep via airkctl one-shot [V U11] | `airkctl agents connect` |
| 32 | `airk.reconnectAgents` | Health scan + rewrite | keep via airkctl one-shot [V U28, §8.4] | `airkctl agents list` + `agents repair` |
| 33 | `airk.verifyMcp` | MCP handshake over HTTPS (`probeMcp`) | keep via airkctl one-shot [I] | no named `airkctl` command yet (open question 3) |
| 34 | `airk.manageMcp` | Alias of `setupAgent` | admin link [I] | **Agents** |
| 35 | `airk.diagnose` | Repair `verify` preview | admin link | **Recovery** |
| 36 | `airk.logs` | Route `#manage/logs`, action `logs` | admin link | **Logs** |
| 37 | `airk.storageLimits` | Route `#manage/storage` | admin link [V §8.5] | **Storage** |
| 38 | `airk.manageVscodeMcp` | Runs `workbench.mcp.listServer` | keep (feature) | editor command |
| 39 | `airk.currentMaintenance` | Repair `auto` preview | admin link | **Recovery** |
| 40 | `airk.serviceConnect` | Console `connect`, `#manage/service` | keep via airkctl session | becomes **Check again**: re-verify I4/`airkctl`, reattach [V §8.1] |
| 41 | `airk.serviceStart` | Console `start` | keep via airkctl one-shot [V §2.3] | `airkctl service start` |
| 42 | `airk.serviceRestart` | Console `restart` | admin link [V §8.1, §8.5] | **Service** |
| 43 | `airk.serviceStop` | Console `stop` | admin link [V §8.1, §8.5] | **Service** |
| 44 | `airk.serviceLogs` | Console `logs` | admin link | **Logs** |
| 45 | `airk.serviceResources` | Console `resources` | admin link | **Service** |
| 46 | `airk.serviceDiagnostics` | Console `previewReport` | admin link | **Logs** (redacted report) |
| 47 | `airk.showExtensionOutput` | Shows output channel | keep (feature) | — |
| 48 | `airk.workbench.focus` | Focus Workbench | keep (feature) | — |
| 49 | `airk.workbench.newSession` | New session | keep (feature) | — |
| 50 | `airk.workbench.clearSessions` | Clear saved sessions | keep (feature) | — |
| 51 | `airk.workbench.searchSelection` | `/search` of editor selection | keep via airkctl session | session evidence read |
| 52 | `airk.manageWorkspaceTrust` | `workbench.trust.manage` | keep (feature) | — |
| 53 | `airk.trustHttpIssuer` | Reviews CurrentUser trust | admin link [V §8.5] | **Service** (`airkctl service http-trust`) |
| 54 | `airk.removeHttpIssuerTrust` | Removes owned trust | admin link [V §8.5] | **Service** |
| 55 | `airk.httpCredential` | Reads credential file, returns token for `${input:airkToken_…}` | keep via airkctl one-shot [V §6.5] | token one-shot; never via session |
| 56 | `airk.httpIssuerRotationStatus` | Reads rotation receipt | admin link [V §8.5] | **Service** |
| 57 | `airk.rotateHttpIssuer` | Rotation review + client trust bundles | admin link [V §8.5] | **Service** |
| 58 | `airk.retryUpdate` (not contributed) | Re-runs `automaticInstall` | remove [V §8.1 "every install, repair, update path"] | none |
| 59 | `airk.reloadUpdatedExtension` (not contributed; status bar) | Reloads window after VSIX replacement | keep (feature) [I] | `extensionUpdateLoader.ts` stays (§8.5) |

New commands Phase 6 must add [V §8.1, §8.3]: **Open AIRK Admin** (`airkctl
admin open`), **Download AIRK Service** (U59 link), **Installation
instructions**. **Start** and **Check again** reuse #41 and #40 [I].
`MANAGE_COMMANDS` and the sidebar's `commandUris` (`serviceExtension.ts`
~:3393) reference #4, #22, #19 and others and must be rewritten with them.

Per admin page (32 links): Overview 1, Index library 6, Activity 5,
Storage 4, Recovery 4, Service 7, Agents 1, Usage 1, Logs 3.

## 2. Settings

| Setting | Scope | Disposition | Note |
|---|---|---|---|
| `airk.mcp.responseMode` | machine | keep [I] | Sent as `X-AIRK-Response-Mode` in definitions and to `tools shell` |
| `airk.cli.autoInstall` | machine | remove [V §8.5] | No install path remains |
| `airk.cli.shellIntegration` | machine | remove [V §8.5] | Installer owns PATH (§9.5) |
| `airk.runtime.nativeExecution` | machine | keep (re-described) [V §8.3] | Now gates running `airkctl` |
| `airk.service.channel` | machine | remove [V §8.5] | One service per user (U17) |
| `airk.service.installDir` | machine | remove [V §8.5] | Location only through the U39 pointer; no editor setting |
| `airk.service.managedSource` | machine | remove [V §8.5] | No organization payloads (U15) |
| `airk.service.managedSha256` | machine | remove [V §8.5] | same |
| `airk.service.managedLifecycle` | machine | remove [V §8.5] | same |
| `airk.service.managedStateDir` | machine | remove [V §8.5] | same |
| `airk.explorer.assistance.enabled` | — | keep [V U10] | Controls session draft delivery instead of the window pipe |
| `airk.explorer.planner.directory` | resource | keep | Editor-only |
| `airk.explorer.suggestions.ttlHours` | — | keep | Editor-only |
| `airk.explorer.review.testPathPattern` | resource | keep | Editor-only |
| `airk.workbench.persistSessions` | window | keep | Editor-only |
| `airk.workbench.persistResults` | window | keep | Editor-only |
| `airk.mcp.registrationEnabled` | machine | keep [V §8.1] | VS Code MCP provider stays |
| `airk.storage.path` | machine | remove [V §8.5] | Data location chosen by installer (U39); relocation in admin **Storage** |

Keep 9, remove 9, migrate 0. No setting value is carried to the service: the
installer never reads editor settings [I].

## 3. Views, MCP providers, walkthrough, menus, package fields

| Surface | Today [V] | Disposition |
|---|---|---|
| View container `airk` (activity bar) | Hosts `airk.dashboard`, `airk.collectionsView` | keep |
| View container `airk-workbench` (secondary sidebar) | Hosts `airk.workbench` | keep |
| View `airk.dashboard` (webview "AIRK Explorer") | `ExplorerSidebar`, status + setup guidance | keep; content becomes §8.3 states [V] |
| View `airk.collectionsView` ("Workspace Projects", tree, `when: airk.explorerUnavailable`) | Fallback project tree with index/reindex/pause inline | keep as read-only status fallback; inline actions become admin links [I] |
| View `airk.workbench` (webview) | `WorkbenchView` | keep |
| MCP provider `airk.collections` | HTTPS definitions; resolves token by reading credential file | keep; definition from `airkctl`, token by one-shot [V §8.1, §6.5] |
| MCP provider `airk.explorerAssistance` ("retired") | `provideAssistanceDefinitions()` returns `[]`; resolve throws `legacy_mcp_definition_retired_restart_airk` | remove with its activation event [I]; verify VS Code drops cached definitions of an uncontributed provider |
| Activation events | `onStartupFinished`, `onMcpServerDefinitionProvider:airk.collections`, `…:airk.explorerAssistance` | keep first two; remove third |
| Walkthrough `airk.gettingStarted` step `indexStep` | Links `airk.chooseStorage` | rewrite: admin link to **Index library** |
| step `indexModeStep` | Links `airk.indexMode` | rewrite: admin link to **Index library** |
| step `setupAgentStep` | Links `airk.setupAgent` | keep (via `airkctl agents connect`) |
| step `verifyMcpStep` | Links `airk.verifyMcp` | keep (open question 3) |
| (new step) | — | add first step "Install AIRK Service" with the U59 link [I] |
| `menus.commandPalette` | 36 entries: 4 hidden (`when:false`: tree index/reindex/pause/remove), `httpCredential` hidden, 31 gated by the native enablement | keep gates for kept and admin-link commands (both run `airkctl`); drop entries of removed commands |
| `menus.view/title` | `explorer.open` on dashboard; `treeRefresh` on tree | keep |
| `menus.view/item/context` | `treeIndex`, `treeReindex`, `treePause` inline | admin links or dropped with the tree actions [I] |
| `scripts.vscode:uninstall` | `node ./out/uninstallHook.js` | keep; removes only editor-owned storage [V §8.5] |
| `files` | includes `native/**`, `airk-update.json` | drop `native/**` [V §4.4]; keep `airk-update.json` for the update loader [I] |
| `capabilities.untrustedWorkspaces.description` | "starts its native helper" | rewrite for `airkctl` |
| `extensionKind: ["workspace"]` | Runs on workspace host | keep [V §8.3 remote rule] |
| `scripts/build-explorer.cjs` | Reads `crates/airk-mcp/src/help/topics.rs`, `unified_explorer.json`, `explorer_templates.json` | stop reading `topics.rs`; consume service-owned catalogs with drift check [V §10.4] |

## 4. Process start sites

Roster: `PROCESS_STARTS` in `src/test/explorerBoundaries.test.ts` (:171–185),
13 reviewed sites, checked by an AST scan for `child_process` calls,
`runText`/`runSystemText` and `createTerminal`. A full `rg` for
`child_process`, `spawn`, `execFile`, `fork` and `createTerminal` in
production sources found no site outside the roster [V]. `mcpStdioSession.ts`
imports only a type and starts nothing itself; `explorerAssistance/main.ts`
is started by the native forwarder, not by the extension [V].

| Site [V] | What it starts [V] | Disposition | Surviving owner |
|---|---|---|---|
| `nativeSession.ts#launchSession:spawn` | `airk service session` (one per window) | replace (stays until the Phase 6 adapter replaces its last caller) | `src/airkctl/` session client starting `airkctl session` [V §6.2, §6.5] |
| `nativeSession.ts#windowlessOneShot:runText` | One temporary `airk` command without a window (uninstall worker) | remove [I] | none needed (uninstall no longer runs native work); any one-shot goes through `src/airkctl/` |
| `nativeSession.ts#differentBuildOneShot:runText` | One-shot against a different-build predecessor image | remove [I] | none: predecessors are the service installer's (U12, U13) |
| `serviceClient.ts#NativeControlTransport.constructor:spawn` | `airk service console --runtime-dir … --runtime-sha256 …` (different build or outside a window) | replace (stays until Phase 6) | `airkctl session` control requests; `airkctl` one-shot outside a window [V §6.2] |
| `native.ts#runText:runSystemText` | Gated one-shot primitive; carries `service rpc --stdin` (`serviceBootstrap.ts` :586–600), `service prepare-storage/prepare-configuration/legacy …/logs/record-error`, `runtime http-endpoint/http-issuer-rotation/inventory/purge` | replace (stays until Phase 6) | `src/airkctl/` one-shot runner; `service rpc` → `airkctl session` or `airkctl` one-shot [V §6.2] |
| `native.ts#runSystemText:execFile` | The `execFile` primitive | replace | `src/airkctl/` (the only spawning module, §4.4) |
| `native.ts#prepareHelper:runText` | Dead per roster text | remove (dead: roster states no production caller) | none |
| `native.ts#runJson:runText` | Dead per roster text | remove (same evidence) | none |
| `uninstallHook.ts#startUninstallWorker:spawn` | Node worker (`--airk-uninstall-worker`) to outlive the 5-second hook | remove unless editor-storage deletion needs it [I] | inline editor-storage cleanup in `vscode:uninstall` |
| `serviceExtension.ts#ServiceExtension.openTools:createTerminal` | Terminal: `airk tools shell` | keep, retargeted | editor terminal adapter running verified `airkctl tools shell` [V §4.3, §4.4] |
| `cliInstall.ts#installPath:runSystemText` | PowerShell for user PATH | remove [V §8.5, §2.3] | installer (§9.5) |
| `cliRemove.ts#removeShellIntegration:runSystemText` | PowerShell for user PATH | remove [V §9.3] | installer receipts |
| `privateDirectory.ts#run:execFile` | `whoami.exe`, `icacls.exe` (ACL write) | remove with its callers [I] | none; I4 ownership checks are new `src/airkctl/` code |

Expected roster after Phase 6 [I], a reviewed checked-in value per §4.4:
`src/airkctl/` session spawn (`airkctl session`), `src/airkctl/` one-shot
`execFile` (`airkctl …`), Windows Authenticode verification tool, macOS
`codesign --verify --strict`, a Windows owner/ACL read tool if Node cannot
prove "not writable by others" for the I4 pointer and record, and the editor
terminal for `airkctl tools shell`. Browser opening uses `vscode.env.openExternal`
(today `explorerController.ts` :766 for `ABOUT_LINKS`) and is not a process
start.

## 5. Source files

Columns: production importers from a relative-import scan (test importers
excluded) [V]; disposition (`stay`, `ui` = move to `packages/ui`, `admin` =
move to `packages/admin`, `re-home` = behavior re-homed in service + admin,
`airkctl` = replace with `src/airkctl/` facade or generated I2 types,
`remove`); the Phase 5 admin page group that must exist before removal.

### 5.1 Install and update

| File | Lines | Called by | Disposition | Admin page first |
|---|---|---|---|---|
| `serviceBootstrap.ts` | 935 | diagnosticLogs, extensionUninstall, serviceConsole, serviceConsoleController, serviceExtension, serviceWorkspace, uninstallHook | remove [V §10.3] (RuntimeHost: discover/ensure/install/transition, project review/registration, legacy startup, HTTP endpoint/rotation) | Overview (leftovers), Index library, Service |
| `native.ts` | 310 | cliInstall, cliRemove, nativeSession, serviceExtension | airkctl (one-shot runner); `probeMcp` (HTTPS) and dead primitives remove | — |
| `cliInstall.ts` | 279 | cliRemove, extensionUninstall, serviceExtension | remove [V §10.3] | — |
| `cliRemove.ts` | 152 | extensionUninstall, serviceExtension | remove [V §10.3] | — |
| `runtimeRebuild.ts` | 145 | serviceBootstrap, serviceConsole, serviceConsoleController, serviceExtension | remove [V §10.3] (staged runtime, clean break) | — |
| `extensionUninstall.ts` | 240 | serviceExtension, uninstallHook | stay, reduced to editor-owned storage [V §8.5] | — |
| `uninstallHook.ts` | 127 | `package.json` `vscode:uninstall` | stay, reduced [V §8.5] | — |
| `extensionUpdateLoader.ts` | 354 | extension, serviceExtension | stay; drops the native manifest entry [V §8.5] | — |
| `extension.ts` | 3 | entry point | stay | — |
| `nativeGate.ts` | 13 | explorerController, mcpStdioSession, native, nativeSession, serviceClient, serviceExtension | stay; gates `airkctl` [V §10.3] | — |

### 5.2 Transports and shared semantic rules

| File | Lines | Called by | Disposition | Admin page first |
|---|---|---|---|---|
| `nativeSession.ts` | 626 | serviceBootstrap, serviceExtension, usagePanel | airkctl (session client; port framing/limits, §6.5) [V §10.3] | — |
| `serviceClient.ts` | 699 | 18 modules (management, intents, workspace, uninstall, Explorer control) | airkctl facade for reads; management methods re-home [V §10.3] | all management pages |
| `serviceHttp.ts` | 268 | agentConfig, agentMigration, agentReconnect, explorerEvidence, httpIssuerRotation, mcpHttpSession, model, native, serviceBootstrap, serviceExtension | remove [V §10.3]; only recognizer patterns (credential input id, helper command form) move into `airkctl/agents/` | Agents, Service |
| `mcpHttpSession.ts` | 85 | explorerEvidence, native | remove (no network to the service, §4.4) | — |
| `mcpStdioSession.ts` | 136 | explorerEvidence, explorerReview, explorerStrategyRunner, mcpHttpSession, native | airkctl (evidence client interface becomes the session facade) | — |
| `privateDirectory.ts` | 55 | explorerAssistanceHost, privateIntentStore, serviceExtension | remove [V §10.3] | — |
| `readRetry.ts` | 166 | explorerEvidence, native | stay (semantic rule) [V §10.3] | — |
| `nativeOutcome.ts` | 45 | referenceRefresh, serviceClient, serviceConsoleController, serviceExtension, serviceIntents | stay (semantic rule) [V §10.3] | — |
| `catalogSession.ts` | 41 | model, readRetry, scopeSelection, serviceConsoleController, serviceHttp, serviceWorkspace, workbenchReferenceRefresh, workbenchView | airkctl (generated I2 type; mirrors Core `ScopedConnectRequest`) | — |

### 5.3 Agent configuration

| File | Lines | Called by | Disposition | Admin page first |
|---|---|---|---|---|
| `agentConfig.ts` | 238 | agentConfigHealth, agentConnections, agentMigration, agentSetup, extensionUninstall, globalAgentInventory, serviceExtension | stay, reduced to removal-only JSON recognizer [V §10.3] | Agents |
| `agentConfigHealth.ts` | 275 | explorerController, serviceExtension | airkctl (`agents list`); no-service subset folds into the recognizer, tested against Rust-emitted cases [V §6.4] | Agents |
| `agentConnections.ts` | 104 | agentSetup | stay (guide UI); `createCodexConfig` writer removed | — |
| `agentMigration.ts` | 356 | agentConfigHealth, agentReconnect, agentSetup, extensionUninstall, serviceExtension | remove (rewrites → `agents repair`; predecessor proof → installer receipt §6.4) | Agents |
| `agentReconnect.ts` | 254 | agentConfigHealth, serviceExtension | stay (reconnect record, pure decisions) [V §10.3] | — |
| `agentSetup.ts` | 222 | serviceExtension | airkctl (`agents connect`) | Agents |
| `agentToml.ts` | 133 | unifiedAgentConfig | stay, reduced to TOML recognizer for removal [I] | — |
| `unifiedAgentConfig.ts` | 256 | agentConfigHealth, agentMigration, agentReconnect, globalAgentInventory | airkctl (Rust writer); removal transform subset stays [I] | Agents |
| `globalAgentInventory.ts` | 107 | serviceExtension | stay as user-level recognizer for the no-service case [I] | — |
| `httpIssuerRotation.ts` | 162 | serviceBootstrap, serviceExtension | remove [V §10.3] | Service |
| `model.ts` | 196 | 15 modules | airkctl: entry/credential-helper builders move to Rust `agents`; `LaunchSpec` replaced by I2 selection; display types stay [I] | Agents |

### 5.4 Management host

| File | Lines | Called by | Disposition | Admin page first |
|---|---|---|---|---|
| `explorerManagement.ts` | 396 | explorerController | admin [V §10.3] | Overview, Activity, Storage, Recovery, Service, Agents, Usage, Logs |
| `explorerControl.ts` | 106 | explorerController, explorerManagement | re-home; status part replaced by session events [I] | Overview |
| `indexingCatalog.ts` | 645 | 20 modules incl. workbench, scope, serviceClient | re-home [V §10.3]: wire validators → Rust-generated; `CatalogView` → admin; scope reads stay | Index library |
| `indexingCatalogController.ts` | 496 | explorerControl, explorerController, localSearchScope | re-home; window-scope methods stay via session [I] | Index library |
| `indexingSplit.ts` | 144 | explorerWebview/indexingSplit, indexingCatalogController, intentAcceptedEvidence, serviceClient, serviceConsoleController, serviceIntents | re-home [V §10.3] | Index library |
| `serviceConsole.ts` | 346 | serviceExtension | re-home [V §10.3] | Overview, Activity, Service, Logs |
| `serviceConsoleController.ts` | 1,413 | serviceConsole | re-home [V §10.3] | all management pages |
| `serviceConsoleView.ts` | 126 | explorerControl, explorerManagement, serviceConsole, serviceConsoleController, serviceExtension | admin [V §10.3] | Service, Logs |
| `serviceDashboard.ts` | 27 | explorerSidebar | remove [V §10.3]; replaced by `status/` §8.3 guidance | — |
| `usagePanel.ts` | 174 | serviceExtension | admin [V §10.3] | Usage |
| `usageReport.ts` | 178 | explorerController, report, serviceExtension, usagePanel | admin [V §10.3] | Usage |
| `report.ts` | 60 | usagePanel | admin [I] | Usage |
| `diagnosticLogs.ts` | 22 | serviceConsole, serviceExtension | re-home [V §10.3] | Logs |
| `diagnosticCaptures.ts` | 44 | serviceConsole | re-home [V §10.3] | Logs |
| `serviceIntents.ts` | 283 | 8 modules | re-home (durable requests) [V §10.3] | all mutation pages |
| `privateIntentStore.ts` | 768 | serviceConsole, serviceExtension | re-home [V §10.3] | all mutation pages |
| `intentAcceptedEvidence.ts` | 352 | privateIntentStore, serviceIntents | re-home [V §10.3] | all mutation pages |
| `vectorModel.ts` | 103 | model | re-home (model setup) [V §10.3] | Index library [I] |
| `serviceWorkspace.ts` | 297 | serviceExtension | re-home (project review/registration) [I]; `nativeScopeLaunchSpec` → scope | Index library |
| `localSearchScope.ts` | 197 | serviceExtension | stay (window scope via session); `saveLocalIndexSet` → admin [V §8.1 last paragraph] | Index library and index sets |
| `scopeSelection.ts` | 165 | serviceExtension, serviceWorkspace, workbenchView | stay (window scope) [V §8.1] | — |

### 5.5 Explorer evidence host (stays; reads use the session facade, §10.3)

| File | Lines | Called by | Disposition |
|---|---|---|---|
| `explorerController.ts` | 1,178 | serviceExtension, workbenchView | stay; management/indexing routes leave |
| `explorerEvidence.ts` | 220 | 9 modules | stay; `McpHttpSession` + `httpMaterial` (:202) replaced by session |
| `explorerFacts.ts` | 203 | explorerController, explorerInspect, explorerReview, explorerStrategyRunner | stay |
| `explorerInspect.ts` | 185 | explorerController | stay |
| `explorerSearch.ts` | 205 | explorerController, explorerFacts, explorerInspect, explorerStrategyRunner | stay |
| `explorerReview.ts` | 308 | explorerController | stay (git review, editor-only §7.3) |
| `explorerSource.ts` | 55 | explorerControl, explorerController, explorerInspect, explorerReview, explorerWorkspaceIO | stay |
| `explorerStrategyRunner.ts` | 299 | explorerController | stay |
| `explorerStrategies.ts` | 548 | 13 modules (host, webview, Assistance) | stay [I] (Strategies are not a §7.3 page) |
| `explorerSuggestions.ts` | 227 | explorerAssistanceHost, explorerController, serviceExtension | stay (Inbox) |
| `explorerDrafts.ts` | 325 | 13 modules | stay (authoritative host validation); draft schema owner moves to the service with fixtures [I §6.2] |
| `explorerHandoffs.ts` | 39 | explorerController, serviceExtension | stay |
| `explorerPlanFiles.ts` | 49 | explorerWorkspaceIO | stay |
| `explorerWorkspaceIO.ts` | 361 | explorerController, workbenchView | stay |
| `explorerErrors.ts` | 87 | 18 modules | ui (shared error registry) [I] |
| `explorerFeatures.ts` | 20 | explorerAssistance/tools, explorerContracts, explorerController, explorerDrafts | ui [I] |
| `explorerRoutes.ts` | 84 | 12 modules | ui with capability flags; `manage` and library routes leave [I] |
| `explorerCommands.ts` | 39 | explorerController, serviceExtension | stay, rewritten (§1) |
| `explorerPanel.ts` | 159 | explorerController | stay (webview shell) |
| `explorerSidebar.ts` | 157 | serviceExtension | stay (§8.3 states) |
| `explorerStatus.ts` | 200 | explorerController, explorerManagement, explorerSidebar, webview app/manage/overview | ui [I] |
| `explorerContracts.ts` | 904 | 46 modules | split per §6 below; primary stay |
| `nativeMetadata.ts` | 178 | explorerEvidence, workbenchView | stay |
| `publications.ts` | 77 | explorerFacts, explorerStatus, workbenchContracts, workbenchView, workbenchWebview/main | ui [I] |
| `sharedCapabilityFields.ts` | 100 | nativeMetadata, workbenchView | stay |
| `sourceVerification.ts` | 182 | 11 modules incl. workbench webview | stay [I] |
| `referenceRefresh.ts` | 228 | intentAcceptedEvidence, serviceClient, serviceConsoleController, serviceIntents, workbenchReferenceRefresh, workbenchView | stay [I] (open question 1); journal recovery by service receipt |
| `serviceExtension.ts` | 3,487 | extensionUpdateLoader | stay; untangled in place into composition root, discovery/status, agents, MCP provider, views [V §10.3] |

### 5.6 Workbench host (stays; reads use the session facade)

| File | Lines | Called by | Disposition |
|---|---|---|---|
| `workbenchView.ts` | 554 | serviceExtension | stay |
| `workbenchContracts.ts` | 186 | 9 modules | stay |
| `workbenchRender.ts` | 361 | explorerSearch, nativeMetadata, sharedCapabilityFields, workbenchReferenceRefresh, workbenchView, workbenchWebview/main | stay |
| `workbenchSessions.ts` | 128 | workbenchView | stay |
| `workbenchStrategies.ts` | 88 | workbenchSessions, workbenchView | stay |
| `workbenchReferenceRefresh.ts` | 47 | explorerControl, serviceConsoleController, workbenchView | stay [I] (open question 1) |
| `workbenchWebview/main.tsx` | 227 | bundle entry | stay |

### 5.7 Explorer Assistance (replaced by session delivery, §10.3)

| File | Lines | Called by | Disposition | Must exist first |
|---|---|---|---|---|
| `explorerAssistance/main.ts` | 45 | bundle entry; started by `airk explorer-assistance` | remove | service-served Explorer tools + session draft delivery (Phases 3, 6) |
| `explorerAssistance/server.ts` | 114 | main | remove | same |
| `explorerAssistance/tools.ts` | 307 | main, server | remove; catalog owned by `airk-mcp` `unified_explorer.json` [V §10.4] | same |
| `explorerAssistance/attach.ts` | 84 | main, tools | remove (`node:net` to window pipe) | same |
| `explorerAssistance/help.ts` | 76 | main, server, tools | remove; help served from service-owned catalog | same |
| `explorerAssistance/helpCatalog.d.ts` | 6 | type stub | remove | same |
| `explorerAssistanceHost.ts` | 225 | explorerAssistanceLaunch, serviceExtension | remove (window pipe/socket host) | same |
| `explorerAssistanceLaunch.ts` | 42 | agentMigration, serviceExtension | remove (launch descriptor) | same |

### 5.8 Webviews

| File | Lines | Disposition | Admin page first |
|---|---|---|---|
| `explorerWebview/api.ts` | 71 | stay (postMessage adapter) | — |
| `explorerWebview/app.tsx` | 274 | stay (editor shell); shared composition → ui [I] | — |
| `explorerWebview/main.tsx` | 7 | stay | — |
| `explorerWebview/vscodeApi.d.ts` | 8 | stay | — |
| `explorerWebview/code.tsx` | 102 | ui [V §10.3] | — |
| `explorerWebview/components.tsx` | 104 | ui | — |
| `explorerWebview/docs.tsx` | 41 | ui | — |
| `explorerWebview/evidence.tsx` | 160 | ui | — |
| `explorerWebview/facts.tsx` | 99 | ui | — |
| `explorerWebview/format.ts` | 46 | ui | — |
| `explorerWebview/help.tsx` | 142 | ui (Help on both, §7.3) | — |
| `explorerWebview/helpCatalog.d.ts` | 6 | ui | — |
| `explorerWebview/notices.tsx` | 31 | ui | — |
| `explorerWebview/search.tsx` | 237 | ui | — |
| `explorerWebview/searchStore.ts` | 50 | ui | — |
| `explorerWebview/workspaces.tsx` | 112 | ui (route map with capability gating) [I] | — |
| `explorerWebview/fileBrowser.tsx` | 61 | stay (workspace files) | — |
| `explorerWebview/inbox.tsx` | 204 | stay (Inbox is editor-only, §7.3) | — |
| `explorerWebview/planner.tsx` | 238 | stay (plan files in workspace, §7.3) | — |
| `explorerWebview/review.tsx` | 136 | stay (git review, §7.3) | — |
| `explorerWebview/strategies.tsx` | 327 | stay [I] | — |
| `explorerWebview/strategyStore.ts` | 103 | stay [I] | — |
| `explorerWebview/overview.tsx` | 127 | stay; management cards → admin Overview [I] | Overview |
| `explorerWebview/indexing.tsx` | 278 | admin; the `window` (search scope) tab stays [I] | Index library and index sets |
| `explorerWebview/indexingSplit.tsx` | 90 | admin | Index library |
| `explorerWebview/manage.tsx` | 644 | admin [V §10.3] | Overview through Logs |
| `explorerWebview/usage.tsx` | 42 | admin | Usage |
| `explorerWebview/runtimeUpgrade.tsx` | 63 | remove (upgrade/rollback end, §2.3) | — |
| `explorerWebview/styles.css` | 406 | ui tokens + editor shell mapping (§7) | — |
| `workbenchWebview/style.css` | 47 | stay; tokenized (§7) | — |
| `src/explorerHelp/*.md` (43 topics, 1,199 lines) | — | ui; `manage-*` and `indexing-*` topics move with admin pages [I] | per topic |

### 5.9 File counts (122 `.ts`/`.tsx`)

| Disposition | Files | Lines |
|---|---|---|
| stay (incl. reduced) | 55 | 14,929 |
| ui | 17 | 1,598 |
| admin | 9 | 1,988 |
| re-home | 13 | 5,019 |
| airkctl | 9 | 2,761 |
| remove | 19 | 3,426 |
| **Total** | **122** | **29,721** |

Counts are by primary disposition; split files (`explorerContracts.ts`,
`indexingCatalog.ts`, `model.ts`, `localSearchScope.ts`, `app.tsx`,
`indexing.tsx`, `overview.tsx`, `serviceExtension.ts`) are counted once [I].

## 6. `explorerContracts.ts` exports (76) for contract step C9

Classes: **UI** = shared view state/events → `packages/ui`; **ED** =
editor-only, stays; **WIRE** = native wire contract → generated from the Rust
owner (I2/I3, U47) or checked against Rust fixtures; **MGMT** =
management/upgrade, retire or move to admin.

| # | Export (line) | Kind | Class | Note |
|---|---|---|---|---|
| 1 | `RuntimeUpgradeProgress` (3) | interface | MGMT | upgrade ends |
| 2 | `CapturedLifecycleResponse` (13) | interface | MGMT | lifecycle receipts → service |
| 3 | `AGENT_INSTRUCTION_FILES` (19) | const | ED | workspace instruction files |
| 4 | `AgentEntryStatus` (26) | type | WIRE | `airkctl agents list` classes (§6.4) |
| 5 | `AgentConfigFinding` (30) | interface | WIRE | `agents list` result |
| 6 | `AgentConfigHealthView` (49) | interface | WIRE | `agents list` per project |
| 7 | `AgentReconnectResult` (56) | interface | WIRE | `agents repair` result |
| 8 | `AgentConfigAlerts` (68) | interface | ED | window alert state |
| 9 | `AIRK_AGENT_INSTRUCTIONS` (77) | const | ED | text copied into workspace files |
| 10 | `MESSAGE_VERSION` (91) | const | UI | view-event protocol version |
| 11 | `ABOUT_LINKS` (93) | const | UI | Help on both shells |
| 12 | `ConnectionState` (107) | type | UI | |
| 13 | `IndexState` (108) | type | UI | derived display state |
| 14 | `FreshnessState` (110) | type | WIRE | mirrors `airk_status.freshness.state` [I] |
| 15 | `SemanticState` (111) | type | UI | derived |
| 16 | `EvidenceClass` (113) | type | UI | |
| 17 | `SearchMode` (114) | type | WIRE | native search mode values |
| 18 | `LiveCheck` (115) | type | UI | some values editor-only |
| 19 | `ProjectSummary` (118) | interface | MGMT | console project page |
| 20 | `StatusSummary` (142) | interface | UI | |
| 21 | `ConnectionCheck` (177) | interface | ED | agent MCP check |
| 22 | `ExplorerViewState` (182) | interface | UI | after removing `management`, `runtimeUpgrade`, `indexing` (MGMT) and moving `extensionUpdate`, `agentConfigAlerts`, `suggestions`, `connectionCheck` to an editor extension (ED) |
| 23 | `SuggestionCard` (221) | interface | ED | Inbox |
| 24 | `SuggestionsView` (235) | interface | ED | Inbox |
| 25 | `HitView` (238) | interface | UI | |
| 26 | `SearchResultView` (251) | interface | UI | |
| 27 | `SearchForm` (266) | interface | UI | |
| 28 | `SearchSessionView` (267) | interface | UI | |
| 29 | `PassageView` (277) | interface | UI | |
| 30 | `OpenSourceResult` (286) | interface | ED | opens an editor |
| 31 | `ProvenanceView` (289) | interface | UI | |
| 32 | `FactItemView` (291) | interface | UI | |
| 33 | `FactListView` (314) | interface | UI | |
| 34 | `appendFactPage` (333) | function | UI | pure |
| 35 | `InspectView` (343) | interface | UI | |
| 36 | `CitationStatus` (368) | type | UI | |
| 37 | `CitationResult` (369) | interface | UI | |
| 38 | `CitationCheckView` (378) | interface | UI | |
| 39 | `ChangeItemView` (388) | interface | UI | |
| 40 | `ChangesView` (389) | interface | UI | |
| 41 | `WorkspaceFileKind` (400) | type | ED | |
| 42 | `WorkspaceFilesView` (401) | interface | ED | |
| 43 | `FactRow` (414) | type | UI | also used by admin |
| 44 | `ProjectWorkView` (416) | interface | MGMT | |
| 45 | `ProjectHealthView` (435) | interface | MGMT | |
| 46 | `ManagedProject` (444) | interface | MGMT | |
| 47 | `PlanItemView` (472) | interface | MGMT | repair plans |
| 48 | `PlanView` (473) | interface | MGMT | |
| 49 | `PendingIntentView` (485) | interface | MGMT | durable requests → service |
| 50 | `ManagementView` (486) | interface | MGMT | |
| 51 | `MANAGE_ACTIONS` (525) | const | MGMT | replaced by the I3 operation table |
| 52 | `MANAGE_COMMANDS` (536) | const | MGMT | becomes admin links (§1) |
| 53 | `HelpTopic` (540) | interface | UI | |
| 54 | `HelpCatalog` (554) | interface | UI | `extensionVersion` → content version (§10.4) |
| 55 | `ASSISTANCE_SERVER_NAME` (557) | const | MGMT (retire) | Node server ends |
| 56 | `ASSISTANCE_TOOLS` (559) | const | WIRE | owned by `airk-mcp` catalog |
| 57 | `ASSISTANCE_HELP_URI` (562) | const | MGMT (retire) | |
| 58 | `WINDOW_SCHEMA` (569) | const | MGMT (retire) | window pipe descriptor |
| 59 | `WINDOW_DESCRIPTOR_LIMIT` (570) | const | MGMT (retire) | |
| 60 | `ASSISTANCE_IPC_LIMIT` (572) | const | MGMT (retire) | |
| 61 | `WindowDescriptor` (573) | interface | MGMT (retire) | holds endpoint + credential |
| 62 | `parseWindowDescriptor` (588) | function | MGMT (retire) | pipe/socket regexes |
| 63 | `AssistanceIpcRequest` (603) | type | MGMT (retire) | replaced by I2 session delivery |
| 64 | `AssistanceWindowStatus` (609) | interface | WIRE | becomes I2 window status payload [I] |
| 65 | `ExplorerHandoff` (619) | interface | WIRE | I2 `handoff_get` payload [I] |
| 66 | `AssistanceIpcReply` (630) | type | MGMT (retire) | replaced by I2 |
| 67 | `EngineHelpView` (638) | interface | UI | |
| 68 | `ParsedCitation` (642) | interface | UI | |
| 69 | `parseCitations` (646) | function | UI | pure |
| 70 | `WEBVIEW_MESSAGES` (679) | const | ED | editor request catalog; split below |
| 71 | `WebviewMessageType` (826) | type | ED | |
| 72 | `explorerRequestPolicy` (847) | function | ED | no-replay classification stays in the adapter |
| 73 | `WebviewMessage` (856) | type | ED | |
| 74 | `WebviewMessageOf` (857) | type | ED | |
| 75 | `parseWebviewMessage` (877) | function | ED | |
| 76 | `HostMessage` (894) | type | ED | carries UI state types |

Totals: UI 30, ED 16, WIRE 9, MGMT 21 (of which 9 retire with Assistance).

`WEBVIEW_MESSAGES` has 101 message types [V]. Proposed split for C9 [I]:
evidence/read actions (`search.*` 3, `evidence.*` 5, `code.*` 5, `docs.facts`,
`list.more`, `help.engine`, `help.choose`, `help.openExternal`, `nav.go`,
`ready`, `status.refresh`, `project.select`, `details.copy`) become
adapter-independent `packages/ui` events; `manage.*` 4, `usage.*` 2 and 36 of
38 `indexing.*` move to admin (I3); `indexing.scope.read/apply` stay for the
window scope; editor-only (`files.*`, `planner.*`, `recipes.*`, `strategy.*`,
`studio.save`, `review.*`, `suggestions.*`, `handoff.share`, `source.*`,
`instructions.open`, `agents.configHealth`, `clipboard.write`,
`workspace.manageTrust`, `workbench.toggle`, `extension.reload`,
`notice.dismiss`) stay. `indexing.context.read/preview/delete`,
`indexing.grant.draft/use/revoke` and `indexing.create` have host handlers
(`explorerController.ts` :659–667) and tests but no sender in
`explorerWebview/` [V by `rg`]: deletion candidates once a compile and the
boundary scan confirm no caller.

## 7. Webview CSS tokens

38 distinct `--vscode-*` names [V], 206 occurrences in `src`: 113 in
`explorerWebview/styles.css` and 58 in `workbenchWebview/style.css` (the plan's
"74" is the number of lines in those two stylesheets containing
`var(--vscode-…)`: 60 + 14), plus inline HTML in `agentConnections.ts` (8),
`explorerSidebar.ts` (11), `report.ts` (10) and `serviceDashboard.ts` (6).
`styles.css` already defines 11 `--airk-*` tokens (:6–16) and two
`.airk-themed` blocks (~:300–327) that override `--vscode-*` with fixed light
and dark palettes; those values are the starting point for the browser
shell's light/dark values [I].

| `--vscode-*` | Proposed `--airk-*` | | `--vscode-*` | Proposed `--airk-*` |
|---|---|---|---|---|
| `button-background` | `--airk-button-bg` | | `input-background` | `--airk-input-bg` |
| `button-border` | `--airk-button-border` | | `input-border` | `--airk-input-border` |
| `button-foreground` | `--airk-button-fg` | | `input-foreground` | `--airk-input-fg` |
| `button-hoverBackground` | `--airk-button-hover-bg` | | `list-activeSelectionBackground` | `--airk-selection-bg` |
| `button-secondaryBackground` | `--airk-button-secondary-bg` | | `list-activeSelectionForeground` | `--airk-selection-fg` |
| `button-secondaryHoverBackground` | `--airk-button-secondary-hover-bg` | | `list-hoverBackground` | `--airk-hover-bg` |
| `charts-yellow` | `--airk-highlight` | | `list-inactiveSelectionBackground` | `--airk-selection-inactive-bg` |
| `descriptionForeground` | `--airk-muted` (exists) | | `menu-background` | `--airk-menu-bg` |
| `editor-background` | `--airk-bg` | | `menu-foreground` | `--airk-menu-fg` |
| `editor-font-family` | `--airk-font-mono` | | `panel-border` | `--airk-border` (exists) |
| `editor-font-size` | `--airk-font-mono-size` | | `quickInput-background` | `--airk-overlay-bg` |
| `editorError-foreground` | `--airk-error` (exists) | | `sideBar-background` | `--airk-sidebar` (exists) |
| `editorWarning-foreground` | `--airk-warn` (exists) | | `testing-iconPassed` | `--airk-ok` (exists) |
| `editorWidget-background` | `--airk-surface` (exists) | | `textBlockQuote-background` | `--airk-quote-bg` |
| `errorForeground` | `--airk-error-fg` | | `textCodeBlock-background` | `--airk-code-bg` |
| `focusBorder` | `--airk-focus` (`--airk-accent` derives from it) | | `textLink-foreground` | `--airk-link` (`--airk-info` derives from it) |
| `font-family` | `--airk-font` | | `toolbar-hoverBackground` | `--airk-toolbar-hover-bg` |
| `font-size` | `--airk-font-size` | | `widget-border` | `--airk-widget-border` |
| `foreground` | `--airk-fg` | | `widget-shadow` | `--airk-shadow` |

Inline HTML in `report.ts` (→ admin) and `serviceDashboard.ts` (removed)
needs no editor mapping; `agentConnections.ts` and `explorerSidebar.ts` stay
editor-only and may keep `--vscode-*` [I].

## 8. Read-retry and outcome rules

| Rule | File | Shared Rust cases | Status |
|---|---|---|---|
| `singleRequiredMemberRetry` (combined single-member retry) | `readRetry.ts` :29 | Yes [V]: `fixtures/wire/read_retry_decisions.json` (32 cases, generator `read_retry_decisions.py`), asserted by Rust `airk_core::combined_reads::read_retry` (`read_retry.rs` ~:434) and by `src/test/wireFixtures.test.ts` | covered |
| `readRetryDelay`, `schema: airk.combined-read/1` branch | `readRetry.ts` :119 | Yes (delegates to the rule above) [V] | covered |
| `readRetryDelay`, `scoped_status_prepare_timeout` / `scoped_connection_unavailable` with `requested_scope` and the 9 accepted `native_control_response` codes | `readRetry.ts` | No shared fixture [I]; hand-written TS tests (`nativeReadRetry.test.ts`) and separate Rust tests (`crates/airk-cli/tests/native_read_retry.rs`) | gap → add Rust-emitted cases |
| `readRetryDelay`, top-level `status: "retry"` (`publication_busy`, `selection_busy`) | `readRetry.ts` | No [I]; the fixture's busy cases are nested in combined members | gap |
| `readRetryDelay`, top-level `direct_query_admission` / `direct_query_setup` | `readRetry.ts` | No [I]; same as above | gap |
| Integral-literal rule (`parseIntegralJson`: `25.0`, `2.5e1` refused) | `readRetry.ts` | TS-only test; fixture case `integral_float_delay` covers only the combined path [V] | gap for top-level envelopes |
| `ServiceError`, `retainNativeOutcome`, `retainNativeOutcomeDuring`, `retainUnvalidatedNativeReply`, `validateNativeReply`, `isLocalTrackingFailure`, `isNativeReplyValidationFailure` | `nativeOutcome.ts` (45 lines) | None [V: no fixture under `fixtures/wire/` names them] | gap → C8 |

After the split most `nativeOutcome.ts` consumers (serviceIntents,
serviceConsoleController, serviceClient) leave; `referenceRefresh.ts` and the
`airkctl` facade remain [I]. `fixtures/wire/session_frames.json` (schema
`airk.wire-fixture.session-frames/1`) is the existing shared basis for the
`airkctl session` framing [V].

## 9. CA, token, credential, endpoint and pipe readers (must disappear, §4.4)

| Caller [V] | What it touches | Disposition |
|---|---|---|
| `serviceHttp.ts` `httpMaterial` (:252) | Reads `ca.pem` and `credential-<id>.token` (owner-private) | remove |
| `serviceHttp.ts` `httpPublicTrustFile` (:250), `validateHttpConnection` (:177–185), `httpUrl` (:81, `https://127.0.0.1:<port>/mcp`), `validateEndpoint` | CA paths, loopback URL, endpoint descriptor | remove |
| `serviceHttp.ts` `publishNativeInput` (:32); callers `serviceExtension.ts` :901, :978, :1002, :1672 | Writes request files into the service control directory | remove |
| `mcpHttpSession.ts` | `node:https`, `node:tls`, `node:http`; bearer token to loopback `/mcp` | remove |
| `explorerEvidence.ts` :202 | `httpMaterial` → `McpHttpSession` for every Explorer/Workbench read | replace with session reads |
| `native.ts` :255 (`probeMcp`) | `httpMaterial` → `McpHttpSession` | remove (open question 3) |
| `serviceExtension.ts` :911–944 (`provisionHttpInfo`) | CA material; sets `NODE_EXTRA_CA_CERTS` / `CODEX_CA_CERTIFICATE` in the terminal environment | remove |
| `serviceExtension.ts` :1436–1446 | Watches control dir for `http-1` / `endpoint.json` | remove |
| `serviceExtension.ts` :1719, :1779, :1842, :1868, :2538 | `httpPublicTrustFile` reads (trust bundles, launch descriptor) | remove |
| `serviceExtension.ts` :1914, :2459, :2495, :2510 | `AIRK_MCP_TOKEN_*` and CA variables in `environmentVariableCollection` | remove |
| `serviceExtension.ts` :3109–3121 (`httpCredential`), :3147, :3178 (MCP provider) | Reads credential file, returns token | replace with `airkctl` token one-shot [V §6.5] |
| `serviceExtension.ts` :2125 | Reads `<state>/extension-uninstall/last-result.json` | remove (with #8) |
| `httpIssuerRotation.ts` :32–156 | Old/new CA files under `http-1`, trust bundles, CA variables | remove |
| `agentMigration.ts` :156 | Compares `ca_certificate_file` | remove |
| `agentReconnect.ts` :237 | Matches `NODE_EXTRA_CA_CERTS` / `CODEX_CA_CERTIFICATE` paths | reduce to recognizer data from `airkctl agents list` [I] |
| `model.ts` :118–156 | Builds `runtime http-credential` helper command, `${input:airkToken_…}`, endpoint URL | move to Rust `airkctl agents` |
| `serviceBootstrap.ts` :590, :766 | `runtime http-endpoint`, `runtime http-issuer-rotation` | remove |
| `explorerAssistanceHost.ts` :70–116 | Creates `\\.\pipe\airk-explorer-<hex>` / `airk-explorer.sock` (`node:net`) and a credential descriptor | remove |
| `explorerAssistance/attach.ts` | `node:net` connect with descriptor credential | remove |
| `explorerContracts.ts` :569–600 | `PIPE`/`SOCKET` regexes, `WindowDescriptor` with endpoint and credential | retire (§6) |

After Phase 6 the §4.4 boundary scan should find no `net`/`tls`/`http`/
`https`/`http2` import in extension host code outside tests (today 3 files:
`mcpHttpSession.ts`, `explorerAssistanceHost.ts`, `explorerAssistance/attach.ts`
[V]) and none of the references above.

## 10. Counts

| Surface | Disposition counts |
|---|---|
| Commands (59) | keep (feature) 11, keep via airkctl 11, admin link 32, guidance-only 2, remove 3 |
| Settings (18) | keep 9, remove 9, migrate 0 |
| Views and providers | views keep 3 (one reduced to read-only), containers keep 2, MCP providers keep 1 / remove 1, activation events keep 2 / remove 1, walkthrough steps keep 2 / rewrite 2 / add 1 |
| Process start sites (13) | replace 4, remove 8, keep 1 (retargeted terminal) |
| Source files (122) | stay 55, ui 17, admin 9, re-home 13, airkctl 9, remove 19 |
| `explorerContracts.ts` exports (76) | UI 30, ED 16, WIRE 9, MGMT 21 |
| `--vscode-*` names (38) | 7 map to existing `--airk-*` tokens, 31 new tokens |
| Retry/outcome rules (7 rows) | covered 2, gap 5 (`nativeOutcome.ts` entirely) |
| Credential/endpoint readers (20 rows) | remove or retire 16, replace 2, reduce/move 2 |

## 11. Open questions

1. **Workbench reference refresh** (`referenceRefresh.ts`,
   `workbenchReferenceRefresh.ts`) submits a `configure_project` mutation from
   the editor. U6 makes index configuration admin-only, while §8.1 keeps
   Workbench. Does reference refresh stay in Workbench through the session,
   or become an admin link?
2. **Window search scope and grants.** Today the window scope uses
   `indexing.scope.apply`, and host handlers exist for `indexing.grant.*`.
   §8.1 keeps scope selection in the extension; §8.1 and U6 make grant
   creation admin-only. Does selecting a window scope through the session
   issue a window grant (as the admin session gets one, §7.3), or must an
   admin-created grant already exist? This shapes I2 and C9.
3. **`airk.verifyMcp`.** It performs an MCP handshake over HTTPS. §6.4 names
   no `airkctl` command for such a probe. Phase 1 must either add one to I2
   or reduce the command to `agents list` health. This is a contract gap for
   Phase 1 review, not a user decision unless it changes a U decision.

## 12. P6a shared-presentation inventory — 13 November 2026

This is the reviewable P6a inventory for [codex-plan §7](../codex-plan.md#7-shared-adminexplorer-experience-in-vs-code-and-browser),
continuing this existing inventory rather than creating another execution ledger.
The execution ledger remains [one-process-ledger.md](../../simplify/one-process-ledger.md).
**No intentional workflow loss, source migration or duplicate deletion is approved
by this inventory.** Existing browser, Explorer and Workbench entry points remain
served until their named surviving owners qualify. P6b contract changes and pilot
implementation are separate reviewable changes.

Evidence: live source on `feat/advanced-split`, HEAD
`f43511506b0bed6436f97aaf35482da8a331da1e`, candidate 1.8.35, including the
working tree. This section is source inspection, not installed or cross-host
qualification. The retained `artifacts/advanced-split/p6a-ui-source-inventory.json`
records exact reviewed file bytes; subsequent edits require revalidation of their
affected rows. The [combined proposal](../codex-admin-extension-combined.md) and
[Admin review](../codex-admin-review.md) retain their original 1.8.35 source and
installed 1.8.34 evidence identities. Their concept images are not implemented UI.
The 107 capture IDs are assigned below; no earlier screenshot is fresh evidence
for this working tree.

### 12.1 Current route and owner map

Current Explorer has **28 addressable routes**, including five management handoffs;
Admin has **11 page destinations**. [explorerRoutes.ts](../../../packages/vscode/src/explorerRoutes.ts)
owns accepted Explorer routes/aliases; [workspaces.tsx](../../../packages/vscode/src/explorerWebview/workspaces.tsx)
owns the screen mapping. Explorer Overview is rendered by its app/overview modules,
outside `SCREENS`. [Admin app.tsx](../../../packages/admin/src/app.tsx) owns browser
hash navigation. The proposed top-level shared destinations are Overview, Search,
Indexes, Activity, Agents, Service and Help; grouping is presentation, not removal
of existing deep links or operations.

| Current Explorer route(s) | Current source owner under `packages/vscode/src/` | Browser equivalent now / required surviving destination |
| --- | --- | --- |
| `overview` | `explorerWebview/app.tsx`, `overview.tsx`, controller/status host | Admin Overview (`pages.tsx`); shared Overview must retain window binding, service/source/semantic distinctions and absent-service guidance. |
| `search/query`, `search/recipes` | `explorerWebview/search.tsx`, `searchStore.ts`, `explorerSearch.ts`, `explorerController.ts` | Admin Search is an explicit scope/tool runner, not Saved Searches parity. Shared Search preserves full query arguments, bounded results, citations and recipe persistence/import/export. |
| `code/symbols`, `code/references`, `code/outline` | `explorerWebview/code.tsx`, `explorerFacts.ts` | Tools may be callable in browser Search, but guided Code navigation is missing. Preserve under Search → evidence tools; textual reference limitations remain explicit. |
| `docs/facts` | `explorerWebview/docs.tsx`, `explorerFacts.ts` | Guided document facts remain to migrate; retain extraction/original-location distinctions. |
| `evidence/inspector`, `evidence/checker`, `evidence/changes` | `explorerWebview/evidence.tsx`, `explorerInspect.ts`, `explorerEvidence.ts` | Browser tool calls alone do not replace citation selection, validation, publication baselines, source checks or continuation. Preserve as guided evidence tools. |
| `review/changes`, `review/impact`, `review/tests` | `explorerWebview/review.tsx`, `explorerReview.ts` | Browser has no editor Git/dirty-buffer bridge. Preserve editor workflows; browser must support explicit reviewed inputs or disclose the unavailable host capability with a next step. Impact is bounded candidates; Tests does not execute tests. |
| `strategies/library`, `strategies/studio`, `strategies/planner` | `explorerWebview/strategies.tsx`, `planner.tsx`, `strategyStore.ts`; strategy/runner/workspace-file owners | No guided browser equivalent today. Preserve editing, parameters, snapshots, bounded run evidence, prompt compilation, plan dependencies, imports, exports, guarded save and manual handoffs. No autonomous orchestration. |
| `indexing/window`, `indexing/library`, `indexing/projects` | `explorerWebview/indexing.tsx`; `indexingCatalog.ts`, I2 window scope/agents owners | Admin Index library/set views exist; browser Search scope is separate A2 authority. Shared Indexes/scope presentation must preserve named unions, descriptions, save-set, Make readable, split outcomes and native review/receipt custody. |
| `manage/overview`, `manage/storage`, `manage/repair`, `manage/service`, `manage/logs` | `explorerWebview/adminLinks.tsx`; `airkctl/features/adminOpener.ts` | These are handoff cards to Admin, not independently implemented management screens. Shared Service/Activity can replace cards only after both hosts render the surviving workflows. Recovery retains its actual supported diagnostic role. |
| `manage/activity` | `explorerWebview/manageViews.tsx`, `airkctl/features/explorerHost.ts`, `liveStatus.ts` | Admin Activity (`activity.tsx`) has service job/receipt workflows. Preserve window session events as distinct from service events/logs, truncation and current-operation evidence. |
| `manage/usage` | `explorerWebview/manageViews.tsx`, `airkctl/features/explorerHost.ts` | Admin `usage.tsx` is richer (policy, settings, exports). First common pilot is JSON report; retain richer browser controls and editor link until their inputs/results migrate. |
| `manage/agents` | `explorerWebview/agents.tsx`, `airkctl/features/windowAgents.ts`, native agent CLI | Admin `agents.tsx` provides endpoint/trust inspection, grant-bound configuration examples and local terminal guidance. Preserve Explorer open-connection/default/choice/follow/disconnect and local config health/reconnect; browser cannot claim enrollment from a copied command. |
| `help` | `explorerWebview/help.tsx`, bundled `explorerHelp/`, `help.engine` host | Admin `help.tsx` calls native topics/tool catalog. Shared Help combines task navigation with one native content owner and explicit editor/browser applicability; keep offline bundled editor guidance. |

Legacy `manage/projects` → `manage/overview` and `manage/indexing` → `manage/repair`
aliases remain accepted until replacement links/bookmarks and restored route state
have explicit compatibility tests. All eleven Admin page IDs remain accepted:
`overview`, `index-library`, `activity`, `agents`, `storage`, `recovery`, `service`,
`logs`, `usage`, `search`, `help`. Index-library `root`, `index`, `set` prefills
remain navigation only; a link never authorizes mutation.

### 12.2 Commands and host entry points

The current [package contribution](../../../packages/vscode/package.json) and
[serviceExtension command table](../../../packages/vscode/src/serviceExtension.ts)
each contain these **20 commands**, not the historical 59 above. Keep their IDs,
palette/menu behavior and applicable gates; presentation consolidation does not
authorize removing them.

| Command IDs (`airk.` prefix) | Surviving duty / browser relationship |
| --- | --- |
| `explorer.open`, `explorer.search`, `explorer.showSuggestions` | Open shared editor presentation, carry bounded search input, retain reviewed window-bound suggestions. Browser uses its own routes/session and does not adopt a window inbox. |
| `openAdmin` | Preserve explicit host choice and page/prefill targeting. External browser uses a fresh local launch; editor choice must load bundled shared presentation, not frame the protected Admin URL. |
| `serviceStart`, `downloadService`, `installationInstructions`, `checkAgain`, `serviceConnect` | Keep installation discovery, start/check recovery and offline guidance in the small editor host. P7's explicit automated/manual/defer install choice is approved future work, not implemented by these existing link commands. |
| `enableNativeExecution`, `manageWorkspaceTrust`, `showExtensionOutput` | Keep native-execution opt-in, VS Code trust management and local diagnostics available as appropriate when service use is gated/unavailable. No browser impersonation of workspace trust. |
| `setupAgent`, `reconnectAgents`, `manageVscodeMcp`, `httpCredential` | Keep verified native agent operations, reviewed local file ownership/repair, real-client action and credential-input provider. Tokens/launch URLs do not enter shared page props or persisted UI state. Browser keeps explicit local-terminal instructions where I3 lacks the operation. |
| `workbench.focus`, `workbench.newSession`, `workbench.clearSessions`, `workbench.searchSelection` | Retain separate Workbench provider, sessions and editor-selection input. No inferred migration or browser substitute. |

Also preserve the `airk.dashboard` sidebar fallback, `airk.workbench` view provider,
`airk.collections` MCP definition provider, existing menu/walkthrough entry points,
and editor input-variable credential resolution. The old `COMMAND_ROUTES` catalog
still describes compatibility/internal routing; its presence does not establish
that all historical commands remain registered.

### 12.3 State, persisted entries and compatibility boundaries

| Current owner / entry | Lifetime and preservation obligation | Browser equivalent / migration rule |
| --- | --- | --- |
| `explorerWebview/api.ts` host `getState`/`setState`; app `route`, Studio/Planner drafts | VS Code webview restoration; `strategyStore.ts` owns `strategies.studio`/`planner`, draft revisions, edited prompts and saved-file identities. Preserve route aliases and user-edited content; stale run/saved bindings invalidate on selection change. | Browser route/hash and inert draft storage need an explicit versioned restoration decision before replacing these workflows. Do not persist grants, CSRF, review tokens or authority as draft state. |
| `explorerSearch.ts`/`searchStore.ts` search session | Host/window-session form, results, page and `selectionEpoch`; not a durable recipe library. Preserve form/page edits against late replies, complete native failures and result-owner identity. | Browser mounted Search state is not equivalent session restoration. Shared store binds each host's actual scope/session and invalidates late results. |
| Workspace Memento `airk.explorer.searchRecipes` | Controller validates up to 50 recipes; maintain saved recipes across replacement and preserve all schema fields. | Browser must provide validated import/export and a reviewed durable inert recipe library before claiming saved-recipe parity; no silent data loss or automatic cross-host sync. |
| Workspace Memento `airk.explorer.dismissedNotices` | Content-owned notice keys (bounded to 512); condition alerts remain visible as specified. | Preserve known dismissals in editor; browser may use a separate host preference store after review. Required warnings/errors cannot disappear because a screen is shared. |
| Workspace Memento `airk.windowScope.selection`, `airk.windowScope.userCleared` | Remembered selection only, never an active grant/lease. Re-resolve catalog/memberships on restore; explicit clear suppresses automatic replacement. | A2 browser session authority stays independent from W4 window authority. Remembered names/IDs are hints requiring fresh admission. |
| Workspace Memento `airk.agents.openConnections` | Workspace-connected open entries and follow-window preference; native service owns defaults/choice/credential authority. | Browser does not inherit window following or make its own selection an agent default without an explicit supported action. |
| `.airk/recipes`, `.airk/strategies`, configurable `airk.explorer.planner.directory` (default `.airk/plans`) | Existing guarded workspace import/save, path/link checks, overwrite review, hash/readback and unsaved-draft alternatives remain host-owned. Preserve `airk.explorer.search-recipe/1`, `strategy/1`, `studio/1`, `run/1`, `plan/1` formats. | Browser file selection/download can exchange validated files; it does not grant arbitrary workspace writes or prove a downloaded file was saved. Existing source/scratch privacy rules remain. |
| Workbench `<globalStorageUri>/workbench/<project>/` sessions and `shortcuts.json` | `workbenchSessions.ts` schema-1 inert sessions, revision lock, bounded turns/results, persisted form draft; `workbench.persistSessions`/`persistResults` deletion policy. Preserve separately; named selections are not automatically restored authority. | No browser Workbench or session migration is implemented/planned by the pilot. Existing copy/export/source-open/reference-refresh capabilities remain intact. |
| `airk.admin.openIn` | Remembered `external`/`vscode` choice owned by native extension configuration; host choice survives UI replacement. Current editor path still calls Simple Browser and is not rendered-display proof. | Browser has its own session; shared editor choice will target bundled local presentation. Preserve external default, explicit choice and failure guidance. |
| Browser `airk-admin-csrf:<service_instance>` sessionStorage mirror | Existing same-tab restoration only; remove on logout/observed401. Never promote it into shared props, URLs, generic storage or editor messages. | Browser adapter retains this I3 custody. VS Code adapter uses existing verified native session, not the browser credential. |
| R04 navigation custody in `indexLibrary.tsx`; R19 operation records | Exact browser session owns immutable intent, receipt handle, full results and pending/unknown admission. Navigation cannot mint replacement keys; settled-cell reclamation preserves pending/unknown custody. | Migration must retain owner identity and results through navigation; same rendering does not imply editor and browser share request authority. Browser reload/persisted recovery is not established by this in-memory custody. |

No format is changed by this inventory. Before any format or stored-session change,
retain fixture/schema identity or separately review the required contract. A
browser persistence design is a concrete P6b/P6c deliverable; lack of one blocks
retiring the editor's existing saved workflows.

### 12.4 Editor actions and the narrow bridge boundary

Existing [explorerContracts.ts](../../../packages/vscode/src/explorerContracts.ts)
message validation and [ExplorerPanel](../../../packages/vscode/src/explorerPanel.ts)
own the bundled editor boundary. Keep exhaustive allowlists, request/session/panel
generation and scope fences. Shared rendering code imports neither `vscode` nor
browser session credentials; adapters own effects. Both currently use Preact
10.29.8, which permits an initial shared presentation module without a dependency
upgrade or a new crate.

| Capability | Editor surviving owner | Browser behavior / constraint |
| --- | --- | --- |
| Open cited source | `explorerWorkspaceIO.openSource`, native verification plus dirty-buffer metadata | Copy original location/citation; open a supported external original document where available. Never reinterpret extracted PDF/Office line numbers as source-editor lines. |
| Active editor/selection and Git context | Workbench view and Explorer review/source host | Explicit supported inputs only; show unavailable capability with a useful next step. No fabricated workspace/Git inventory. |
| Clipboard/export/import | Host clipboard/save/open dialogs, guarded workspace file owners | Clipboard API/file input/download with observed success/failure and complete retained data. Download start is not proof of filesystem save. |
| Plans/Studio and instruction files | `explorerWorkspaceIO`, `explorerPlanFiles`, native config owner | Editor reviews and writes through existing guarded owners. Browser may export inert content and local instructions; no generic write-file or shell bridge. |
| Suggestions/drafts/handoffs | `explorerSuggestions`, `explorerDrafts`, `explorerHandoffs`, window publisher | Preserve captured window identity, expected target revision, expiry, review/apply-to-form and handoff receipt. Browser cannot silently bind to a window or execute suggested operations. |
| Scope/agent setup/start/recovery | Existing I2/window/agent/native host owners | A2/browser reads and supported I3 management only. Browser does not inherit W4 grants, local trust consent or agent-config ownership. |
| Help routes/approved links | Shared Help route parser and host-specific fixed-destination opener | Stable secret-free article/tool links; allowlisted navigation only. No raw `executeCommand`, executable path, shell string or arbitrary URL bridge. |

Keep `frame-ancestors 'none'` and X-Frame-Options DENY. Loading a service URL into
Simple Browser cannot implement the editor host. The shared editor bundle remains
extension-owned under its current restrictive CSP and local-resource roots; service
HTML/scripts do not receive privileged editor APIs. No direct renderer access to
the owner IPC or native executable is proposed.

### 12.5 Concrete Usage and Help pilot for P6b review

1. **Freeze before moving.** Retain complete equivalent native Usage JSON and Help
   topic/catalog outcomes, canonical input catalogs and current renderer behavior.
   Use the owning Rust `UsageDocumentRead`/Help contracts and generated fixtures;
   do not create TypeScript/Python wire mirrors. Structural movement preserves
   original assertions; improved UX follows as a distinguishable behavior step.
2. **Shared presentation owner.** Start with pure modules under the existing Admin
   source tree, consumed by both build entry points. Adapters supply a constrained
   read interface, host/scope label, known capability set, route and export effects.
   The pilot needs no new crate/package, remote script loading or arbitrary RPC.
   A later package extraction is not a prerequisite and requires its own review.
3. **Usage common subset.** Share the JSON report summary/details for one explicit
   index and days 1–365; keep full native document, recording limits, missing/wide
   values, UTC context and estimates. Browser uses `usage.document`; editor uses
   existing `work.usage` → verified `airkctl usage report --format json`. Admit the
   index through each host's owner; never silently borrow the other host's scope.
   Mark the pilot subset explicitly. Admin's Markdown/CSV export, policy, settings
   and settings-file validation/price-scenario inputs remain served by existing
   owners until full `UsageDocumentRead` parity qualifies.
4. **Help shell common subset.** Share task/article navigation, rendering, focus,
   continuation/error presentation and host applicability. Preserve bundled editor
   guidance when native execution/service access is unavailable. Native topics and
   tool schemas remain the content owners; retain Explorer evidence/strategy Help.
   Browser uses `search.call(help)`/`tools.catalog`; editor's current `help.engine`
   requires a project. No-project metadata Help and tool catalog need a separately
   reviewed narrow host read before claiming equivalent native reference behavior.
   Preserve all topic/command/tool selectors, optional/null schema semantics and
   loaded-section text on continuation failures. A metadata filter cannot be
   labeled full-content search.
5. **Contract delta before consumers.** Specify only missing read applicability,
   full supported Usage inputs beyond the pilot and host capability/navigation
   metadata. Existing C9 message validation remains exhaustive. Extend the Rust
   owner/generation path where native shapes are involved; define independent
   accepted/refused/unknown/expired/session-change cases. Do not add management,
   file writing, enrollment or execution to the read pilot.
6. **Qualification before retirement.** Compare full equivalent results through
   real I2 and I3 transports, normalizing only documented envelopes/identities and
   timing. Exercise no scope, explicit/single/multiple scope applicability,
   not-ready/refused/transport body loss, session/scope change, cancellation and
   late results. Run browser DOM/keyboard/history plus packaged VSIX rendering
   and embedded-asset checks; source presence/mock callbacks are insufficient.
   Inspect 1366×768 and 1440×1000, narrow editor/390-pixel Help, 200% zoom and
   high-contrast/keyboard states without clipping evidence. Present surviving-owner
   benefit, outstanding gaps and exact receipts before P6c or any deletion.

Pilot success means the same reviewed presentation works in both actual hosts.
It does not complete Search/Strategies/Agents/Workbench migration, the entire
107-row UX backlog, browser persistence or release/platform qualification.

### 12.6 Admin review coverage and current disposition

The complete source review/design input is [the Admin review's 107-row inventory](../codex-admin-review.md#complete-screenshot-and-design-inventory)
and `images/admin/capture-manifest.json`. The following map assigns every captured
ID once to a surviving workflow/content owner. **Assigned is not implemented or
qualified.** Per-row recommendations and designed titles remain in that original
review rather than being copied into another backlog. UX01–UX10 remain acceptance
criteria; R22/R23 component regressions do not establish whole-page migration,
screen-reader behavior or viewport acceptance.

| Capture ID (38 screen/form states) | Required shared destination | Surviving current owner / preservation duty |
| --- | --- | --- |
| `00-session-entry` | Browser session gate; editor unavailable/start guidance | `main.tsx`, `session.ts`, `app.tsx`; shared host entry; shared migration pending. |
| `01-overview` | Overview | Admin `pages.tsx`; shared Overview/window context; shared migration pending. |
| `02-index-library` | Indexes library | `indexLibrary.tsx`; native catalog; shared migration pending. |
| `03-activity` | Activity | `activity.tsx`; service jobs plus separate editor events; shared migration pending. |
| `04-agents` | Agents | Admin and Explorer agent/native owners; shared migration pending. |
| `05-storage` | Service → Storage | `pages.tsx`; bounded inventory coverage; shared migration pending. |
| `06-recovery` | Service → Recovery | `pages.tsx`; actual supported diagnostics; shared migration pending. |
| `07-service` | Service | `pages.tsx`; lifecycle/trust/startup owners; shared migration pending. |
| `08-logs` | Activity → Logs | `pages.tsx`; native bounded log owner; shared migration pending. |
| `09-usage` | Activity → Usage | `usage.tsx`; native Usage read owner; shared migration pending. |
| `10-search` | Search | `pages.tsx`; canonical scoped tools plus retained Explorer inputs; shared migration pending. |
| `11-help` | Help task landing | `help.tsx`; native and retained editor guidance; shared migration pending. |
| `12-index-detail` | Indexes → Index detail | `indexLibrary.tsx`; source/settings/policy/split ownership; shared migration pending. |
| `13-set-detail` | Indexes → Set detail | `indexLibrary.tsx`; native membership/delete owner; shared migration pending. |
| `14-folder-picker` | Shared folder dialog | `folderPicker.tsx`; native browse, DOM accessibility; shared migration pending. |
| `15-job-detail` | Activity → Job detail | `activity.tsx`; immutable job/receipt identity; shared migration pending. |
| `agents-claude-code` | Agents → Claude Code | `agents.tsx`; native enrollment and literal CA command owner; shared migration pending. |
| `agents-codex` | Agents → Codex | `agents.tsx`; native enrollment and literal CA command owner; shared migration pending. |
| `agents-vs-code-copilot` | Agents → VS Code/Copilot | `agents.tsx`; native trust and editor MCP integration; shared migration pending. |
| `agents-other-mcp-clients` | Agents → Other clients | `agents.tsx`; supported local terminal setup; shared migration pending. |
| `16-storage-loaded` | Service → Storage inventory | `pages.tsx`; exact covered bytes/locations and exclusions; shared migration pending. |
| `17-cache-review` | Service → Cache cleanup | `pages.tsx`; exact native plan/empty-plan/refusal; shared migration pending. |
| `18-logs-cleanup-review` | Service → Closed-log cleanup | `pages.tsx`; exact native plan and destructive review; shared migration pending. |
| `19-service-loaded` | Service → Status | `pages.tsx`; observed status age and full native evidence; shared migration pending. |
| `20-logs-loaded` | Activity → Logs results | `pages.tsx`; preserve refusal versus empty and coverage; shared migration pending. |
| `21-usage-report` | Activity → Usage report | `usage.tsx`; common JSON pilot plus complete report; shared migration pending. |
| `22-usage-export` | Activity → Usage export | `usage.tsx`; preserve original report inputs/formats; shared migration pending. |
| `23-index-settings` | Usage/Service → Settings reference | `usage.tsx`; native settings view/validation owner; shared migration pending. |
| `24-help-empty-filter` | Help search/filter empty state | `help.tsx`; preserve query/article and honest search label; shared migration pending. |
| `25-service-unreachable` | Unavailable/error state in each host | `api.ts`, `render.tsx`, session/host startup owners; shared migration pending. |
| `26-search-catalog` | Search → Named scope/catalog | `pages.tsx`; A2 owner distinct from window W4; shared migration pending. |
| `27-search-results` | Search → Bounded evidence | Canonical reads; shared citations/coverage/full export; shared migration pending. |
| `28-create-index` | Indexes → Create | `indexLibrary.tsx`; native draft/apply/recovery custody; shared migration pending. |
| `sources-remove_root` | Indexes → Remove source root | `indexLibrary.tsx`; native source-change review; shared migration pending. |
| `sources-trim_path` | Indexes → Trim source path | `indexLibrary.tsx`; native restrictive source review; shared migration pending. |
| `sources-unblock_path` | Indexes → Unblock source path | `indexLibrary.tsx`; native source-change review; shared migration pending. |
| `29-help-mobile` | Help narrow article layout | `help.tsx`; 390-pixel/focus/history acceptance; shared migration pending. |
| `30-signed-out` | Browser session end/editor host state | `session.ts`, `app.tsx`; no replay or secret persistence; shared migration pending. |

All **40 topic captures** map individually to shared Help → task/article, with
`crates/airk-mcp/src/help/` retaining native content ownership and the original
review's topic-by-topic leading task/applicability correction. Preserve full
content, command guidance and source citations; the Help shell pilot does not
certify those content corrections. Exact assigned IDs:

- `help-topic-overview`, `help-topic-workbench`, `help-topic-about`, `help-topic-explorer`, `help-topic-roles`.
- `help-topic-indexing-choices`, `help-topic-job-management`, `help-topic-index-health`, `help-topic-topics`, `help-topic-quickstart`.
- `help-topic-cli`, `help-topic-commands`, `help-topic-mcp`, `help-topic-tools`, `help-topic-tool-selection`.
- `help-topic-search`, `help-topic-indexing`, `help-topic-jobs`, `help-topic-freshness`, `help-topic-code`.
- `help-topic-documents`, `help-topic-formats`, `help-topic-models`, `help-topic-service`, `help-topic-resources`.
- `help-topic-installation`, `help-topic-upgrades`, `help-topic-configuration`, `help-topic-storage`, `help-topic-maintenance`.
- `help-topic-agent-setup`, `help-topic-request-recovery`, `help-topic-troubleshooting`, `help-topic-privacy`, `help-topic-limits`.
- `help-topic-usage`, `help-topic-versions`, `help-topic-architecture`, `help-topic-workflows`, `help-topic-glossary`.

All **29 tool captures** map individually to shared Help → full tool reference,
with the live Rust-owned tool catalog/schema and native semantic cases retaining
input/result authority. Required-first presentation must disclose its subset and
keep every optional/nullable input, continuation and error field. Preserve the
original review's per-tool interpretation limits. Exact assigned IDs:

- `help-tool-changes`, `help-tool-context`, `help-tool-file_info`, `help-tool-parse`, `help-tool-query_scope`.
- `help-tool-read_file`, `help-tool-search`, `help-tool-code_facts`, `help-tool-imports`, `help-tool-outline`.
- `help-tool-references`, `help-tool-relationships`, `help-tool-symbols`, `help-tool-document_facts`, `help-tool-capabilities`.
- `help-tool-diagnostics`, `help-tool-help`, `help-tool-status`, `help-tool-projects`, `help-tool-draft_search`.
- `help-tool-draft_strategy`, `help-tool-draft_plan`, `help-tool-list_templates`, `help-tool-get_template`, `help-tool-validate_draft`.
- `help-tool-proposal_status`, `help-tool-get_handoff`, `help-tool-suggest_view`, `help-tool-list_files`.

The 38 + 40 + 29 assignments exhaust the capture manifest. Their historical
captures and concept designs establish review coverage, not implementation,
release evidence or authorization to retire their current owners.

### 12.7 P6b local read-pilot contract for review

This contract-only preparation creates the already-planned `packages/ui/` source
directory. It currently contains no package manifest, dependency installation,
consumer, build wiring or persisted state. The proposed local presentation boundary
is [pilotContract.ts](../../../packages/ui/src/pilotContract.ts), with independent
[request/admission cases](../../../packages/ui/test/fixtures/pilot_cases.json).
It does not change native Rust, I1/I2/I3 wire formats, service capabilities or
existing C9 messages. Consumer adoption waits for primary review of this concrete
contract and its integration points.

| Decision | Exact pilot boundary |
| --- | --- |
| Requests | Local `v:1`, `type:pilot.read`, bounded `requestId`, nonnegative safe-integer `epoch`, and one exhaustive `read`. Usage is `kind:usage-json`, one 64-hex `binding`, integer `days:1..365`. Help is `kind:help-topic`, bundled topic ID or empty landing selector, `startLine:1..100000`, fixed native Help byte budget 32768. No URL, command, arbitrary CLI arguments, native method, file path or secret field is accepted. Tool/command reference selectors remain served by the existing Help owners outside this first subset. |
| Meaning of epoch | Host-owned view generation, not a grant, lease, native caller/key or proof of authority. The real adapter captures exact current session/panel/context and checks it before dispatch and after await. Increment generation on session/service/window-scope/selected-binding/trust/native-execution changes. A renderer cannot authorize a request by supplying another epoch or context object. |
| Capabilities/admission | Two bounded reads only. Editor adapter derives readiness from the verified compatible native session and enforces actual workspace trust/native-execution opt-in; Usage also equals the current admitted index binding. Browser adapter owns its actual I3 session and chosen Usage index; it does not consume window scope. Metadata Help can be admitted with no selected index, while still respecting each host's actual readiness/trust gates. Offline Help shell/local navigation needs no native read and grants no additional capability. |
| Full results | `PilotResult` imports the generated I2 `SessionOutcome` invoked member and I3 `Envelope`; complete native stdout, stderr, error and envelope/result fields stay intact. Generated payloads remain opaque native values, not handwritten DTO mirrors. Transport failure keeps observed HTTP status, and local pre-dispatch refusal has an explicit local code. Native read refusal is not relabeled as no data or successful completion. |
| Result ownership | `PilotObservation` retains its immutable independently captured request and `current`/`superseded` applicability. Late/superseded native responses remain available under their original context, never overwrite the active report/article or silently become current authority. Cancellation is read cancellation; the adapter never replays requests as a side effect of rendering, reattachment or error recovery. |
| Secret boundary | Read adapters use only fixed Usage JSON/Help operations. No browser CSRF, launch code, credential value, installation path authority or native caller/key enters renderer requests, props or persisted state. Actual native read/semantic cases must establish whole-result safety before bridge adoption; do not delete unknown fields to manufacture a safe result. A forbidden native response needs an owner correction, not renderer redaction by guesswork. |
| Persistence | First pilot adds **no durable state or schema**. Existing editor recipe/Memento/webview drafts, Workbench files, browser session custody and old rich Usage/Help views retain their owners. Pilot navigation, days and article selection are inert component/session state only; reconnect re-admits authority. Durable browser recipes/drafts remain a later explicit reviewed design, not a hidden localStorage/IndexedDB addition. |
| Bundling | Shared presentation must use each consuming bundle's single Preact 10.29.8 instance, including hooks. Package/dependency/lockfile, TypeScript root/import, bundler alias and artifact-input changes are a separate reviewed integration step. Service-delivered mutable JavaScript receives no editor privilege; browser CSP/frame denial remains intact. |

The independent local cases cover four valid requests, sixteen malformed/unknown
or privilege-passthrough requests, and nine admission outcomes: selected Usage in
either host; no index for metadata Help; missing/foreign index; untrusted host;
native execution off; disconnected session; changed generation. Parsed request
and nested read inputs are copied/frozen independently of the renderer's object.
These are local protocol tests, not native semantic/secret review or real transport
qualification. Consumer lifecycle/race/native cases are required in the next step.

The smallest proposed integration is explicit `pilot.read` validation/dispatch in
the editor's existing C9 owner and a new read-only native adapter module; shared
pure Usage-report/Help-shell modules; and browser adapter/Usage/Help consumers.
Changes need coordinated ownership of `explorerContracts.ts`, `explorerController.ts`
or its existing owning hook, editor build/TypeScript configuration, and browser
`usage.tsx`/`help.tsx`. Keep the pilot route opt-in until actual browser and packaged
editor rendering qualify. Rich existing controls and Workbench remain served.

At contract preparation, one transport detail remained to be independently pinned:
`airkctl usage report` writes the document directly, whereas I3 `usage.document`
retains it in a native response wrapper. Extract only that documented wrapper for
the common presentation while preserving both original complete outcomes. Native
Help likewise has a CLI content envelope and I3 tool-result wrapper. Compare real
equivalent observations and their documented wrappers before claiming parity;
the TypeScript contract alone does not establish those result semantics.

The initial read-pilot preparation included an unadopted
[native adapter draft](../../../packages/ui/src/nativePilot.ts),
[browser adapter](../../../packages/ui/src/browserPilot.ts) using the existing
I3 constructors, and a [pure presentation tree](../../../packages/ui/src/pilotPresentation.ts).
The tree has fixed tags/text children and accepts its rendering factory from the
host, avoiding a second Preact or hooks instance. It currently displays context,
applicability, explicit subset limits and the complete opaque outcome; readable
native document/article projection awaits the Rust-emitted semantic cases. No
existing Usage/Help consumer or legacy workflow is replaced by these modules.
Adapter tests use deliberately synthetic opaque payloads to establish fixed inputs,
whole-result retention, admission, cancellation, latest-read/session fences and
transport settlement; they do not qualify native result shapes or secret safety.

Source follow-up confirms `cli.rs::resolve_call` already sends unselected
`airk_help`/`help` without resolving a project or applying the window scope. The
draft uses only fixed `tools call airk_help --args <bounded Help arguments>
--output json` without root/project/catalog/combined flags; no new I2 option is
proposed. Actual native session/read cases still need to exercise that path.
The native Usage owner currently refuses periods beyond 30 days even though
the existing C9/local request catalog admits up to 365; accepted request syntax
is not a successful report. Preserve and test the native refusal; do not widen
its limit or label 31–365-day reports qualified.

#### Implementation follow-up — 2026-11-13

The pure C9 parser now lives in `packages/vscode/src/uiPilotContract.ts`; the
shared package reexports that single owner. The host consumes the sealed
`out/uiPilotRuntime.js` bundle, and both existing Usage/Help screens expose the
read preview without retiring their richer workflows. `pilotProjection.ts`
unwraps only the operation-specific wrappers established by the actual
Rust-emitted `fixtures/split/ui_pilot/native_reads.json`: I2 stdout contains the
Usage document or Help MCP result directly; I3 retains the corresponding native
`usage_document` or `tool_result` wrapper. Twelve independent cases compare the
full request bodies/arguments, projected values and complete original channels,
including native refusal and unknown fields. These are actual CLI formatter and
Admin-handler seams with injected canonical native packets, not installed
service/IPC/HTTP journeys.

The recorded decision is to preserve whole normal native outputs and prove
forbidden request-shape/capability refusals plus scans for the actual session
secrets in native tests. There is no guessed recursive response-field denylist,
field stripping or fixture-only safety label in production adapters. Public
schemas legitimately document fields such as `caller` and `request_key`.

Help generation now consumes the Rust-emitted
`crates/airk-mcp/src/help/content.json`, validates version and complete source
values, merges existing authored Explorer topics and emits the single
`packages/ui/generated/help-manifest.json`. The full manifest is also packaged at
`extension/out/help-manifest.json`; `channel:null` means content is neutral and
the artifact identity owns the actual channel. Both complete native selector
`helpRequestSchema` and public `tools[].inputSchema` survive, with their distinct
applicability. There is no second hand-maintained native schema/catalog.

Browser Help now searches all bundled full articles without a service session.
Both hosts use the shared Unicode-aware search scorer, Markdown block parser
and pure native-tool schema presenter. Required fields sort first; optionality
and nullability remain distinct, and complete tool definitions remain visible.
Failed browser continuation preserves loaded text and all section outcomes.
Actual Edge DOM checks at 390px with each host's source CSS exercise full-text
guide selection, article focus/scroll and complete tool JSON offline. These
controls do not establish installed-host or live transport qualification.

The currently emitted content is provisional until the native content owner's
corrections and added portable/trust/Arbiter guides are regenerated. The expected
final inventory retains all 40 original native IDs and 43 authored guides, adding
three native IDs for 86 merged topics. Final task-findability expectations must
name those tasks independently rather than derive assertions from the roster.

#### Shared management port decision — 2026-11-14

The primary native owner chose persistent I2 `AdminCall`/`AdminClose`, gated by
`admin-management/1`, rather than per-mutation CLI construction. Rust keeps the
service-created Admin cookie and CSRF entirely private to the native session and
owner I1 transport. The service uses the same canonical AdminPlane operation
table, admission, review/confirmation, key custody and outcome projection as I3;
there is no fake HTTP peer, arbitrary Control relay or service-delivered editor
JavaScript. Generated I2 outcomes remain the wire owner.

The frontend's local C9 request is a closed `admin.call` envelope: version,
requestId, host-owned generation, one existing Admin UI operation and a JSON
payload containing exactly `body`. The wrapper admits the complete existing I3
64 KiB body budget; its nine-byte JSON wrapper is transport overhead, not a
smaller body allowance. `admin.close` has no body, operation, credentials or
renderer-selected native identity. Unknown outer fields/operations and non-JSON
values are refused before dispatch. Native operation/body validation and typed
authority projection remain canonical native duties, without a guessed recursive
field denylist.

Before dispatch the host checks the captured private native session, actual
workspace trust/native-execution setting, compatible readiness, capability and
current generation. A renderer cannot supply readiness or authority. The adapter
uses a deadline/cancellation channel and never replays an uncertain write. A late
reply retains its full I3 outcome under its original context; it cannot reactivate
a closed or replaced UI lifetime. Response/frame sizing must retain full outcomes
and explicit refusal rather than truncation; the native owner is reconciling the
existing I3 32 MiB and I2 16 MiB response bounds before qualification.

The shared ApiPort returns the existing full `CallResult` with generated I3
envelopes. Native UI sessions have their own discriminant and an opaque in-memory
lifetime, with no fake CSRF and no browser token mirroring. Browser HTTP admission,
cookie, CSRF, launch-code and exact-session 401 behavior stay with their existing
owner. Native Admin scope stays independent of editor W4 scope and agent grants;
the existing Explorer Search/evidence workflows remain until separately migrated
and qualified. Closing a UI lifetime ends further admission synchronously and
calls its captured close operation once. No native authority or lifetime is
persisted in Memento, webview state, localStorage or sessionStorage.

Direct generated-key Job Retry and Service Stop/Restart now additionally retain
their immutable original action and full outcome in exact-session custody across
navigation. Synchronous duplicate admission and regeneration after unknown body
loss are blocked. The direct-action store admits at most 32 outstanding actions,
matching the existing native review bound; unknown custody is never evicted and
known unmounted cells are reclaimed. This is UI admission, not a change to native
capacity or a claim of receipt recovery when no receipt handle was received.

#### Implemented source adoption and custody — 2026-11-14 (qualification pending)

`packages/ui/src/management.ts` exposes the existing Admin presentation as one
implementation for both hosts; presentation files remain at their existing owner
while adoption qualifies. Explorer now mounts that implementation for Activity,
Usage, Agents, Logs, Service, Storage, Recovery and Index library. The prior editor
workflows remain available under **Retained editor workflows**; Search, editor
scope/index-set controls, Code, Documents, Evidence, Review and Strategies retain
their existing owners. This is source adoption, not a claim that every installed
editor or native transport has passed qualification.

The consuming editor bundles its single pinned Preact instance for all shared
components. Admin CSS is derived from the existing source inside a scoped rule;
browser bootstrap/launch/history capture is excluded from the native embedding.
The native lifetime survives presentation navigation. `admin.session` is a closed
C9 capture request carrying only the current epoch; it returns the public service
instance and same-child retained public preparation acknowledgments. It never
returns a cookie, CSRF, caller/key, executable or native control address. The
host rechecks trust, native execution, verified readiness, actual capability and
generation before capture/call. A captured context has a distinct immutable host
identity even when a later confirmed close opens another context in the same
child. Unknown close cannot reopen or rebind that context. `admin.ended` carries
only the matching epoch and observed 401, ending the renderer lifetime before a
broken/unfinished body can conceal authorization loss.

The generated I2 owner supplies `admin_call`, `admin_read`, `admin_cancel` and
`admin_close`, with `NativeAdminChunk` responses. Node assembles at most 32 MiB
using chunks of at most 64 KiB; existing frame limits are unchanged. Assembly
checks exact offsets, stable status/total, done-at-end, bounded bytes and complete
UTF-8 JSON. Cancellation targets the original initial request, including before
its first reply. Queueing consumes the original deadline; AdminCall is never
automatically replayed after busy, cancellation or loss. The typed native
session-ended-after-initial-chunk failure ends only the captured context, reports
observed 401 and preserves unknown effects without fabricating an I3 envelope.

The R17 generated mutation flow applies to the six native Admin rows
`jobs.retry`, `catalog.policy_resume`, `catalog.split_recover`,
`recovery.plan_create`, `service.stop` and `service.restart`. A full native
preparation acknowledgment is retained before `{prepared:handle}` can submit.
Recovery sends only `{recover:handle}` to the original operation. READY recovery
does not submit or indicate completion. Native Internal, unknown, missing or
uncached outcomes remain unresolved; only complete success or explicit Refused
can be acknowledged with `{acknowledged:handle}`. The full original result is
retained before ACK; ACK loss never replaces it or retries the mutation, and an
ACK400 restores the unresolved barrier.

Browser preparation custody uses a versioned same-tab journal bound to the public
service instance and existing browser cookie/UI lifetime. It retains at most 32
public immutable input bodies (each at most 64 KiB), opaque handles and complete
acknowledgments. Storage must succeed synchronously before submit. Restore is
read/recover-only and blocks new generated actions while original restored
outcomes remain unresolved. New exchange, logout and 401 clear it; late replies
from an old session cannot write or clear a replacement journal. This does not
claim power-loss, cross-tab or browser-restart durability. Native host custody is
memory-only for the captured I2 context; no capability is persisted in Memento or
renderer storage. R04 policy/split recovery uses the same-operation preparation
owner and retains its original method/session/receipt-order fences.

Shared Service now presents generated native Arbiter status/configuration and
revision-CAS controls: global default-off, explicit immutable index/task
enrollment, all returned profile availability/size/reasons, selected versus active
profile, provision/cancel/remove/recover. MiniLM remains separate. Workbench adds
the canonical `airk_answer` read with the native default bounded request and an
explicit full-question/custom-budget control (16,384 UTF-8 question bytes), preserving
existing client, authority, window-scope and explicit-selection fences. Its shared
presentation links cited paragraphs to exact retained evidence-record IDs, shows
gaps/work/omissions and keeps complete native outcomes in existing copies/exports.
The existing persisted turn label remains bounded to 4,096 characters; the complete
immutable question/budget stays in `turn.request`, request details and JSON/Markdown
exports, including failed attempts. Rerun preserves that full original input.
Within one native management epoch, outer Explorer navigation (including Help and
Overview) retains the same shared management instance and lazily mounted visited
pages. Embedded page selection is controlled by the outer route; it does not
remount draft/result owners. A new native epoch or UI session discards those owners.
Hidden pages suspend progress polling, including already queued timer callbacks;
their open folder modal closes and releases focus/inertness without submitting a
draft. Other pages are not eagerly mounted or read merely to retain a draft.
Picker reads bind to the exact original session and modal lifetime. Each awaited
start/path/parent reply is checked before any later request or state update;
closing, hiding or replacing the session cannot continue the chain under a new
authority.

Both JavaScript producers encode multiline native Help through escaped string
codegen (`template-literal` unsupported in the output policy). Native article,
CLI and schema strings retain their exact UTF-8 bytes, including meaningful
trailing spaces; generated JavaScript itself contains no trailing whitespace.
It does not guess source paths or treat unavailable/unqualified models as ready.

Current evidence includes scoped typechecks, retained session/Workbench cases,
Rust-emitted chunk semantic controls, actual browser DOM controls, the merged
1.9.0 Help manifest (43 native plus 43 authored topics), regenerated browser/editor
assets and exact Admin rebuild verification. The native session observer attaches
before mounted controls can issue requests, retaining immediate auth-ended notices.
Native/live I2/I3 journeys, installed editor behavior and
package/platform qualification remain separate required work. The native
readiness/profile installation and full source-open behavior retain their native
owners; internal evidence-record navigation is not a new source-open capability.
