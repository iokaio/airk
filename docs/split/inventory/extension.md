# Split inventory: VS Code extension (P0.2)

Status: Phase 0 draft, 11 November 2026; source review only; authorizes no deletion.

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
