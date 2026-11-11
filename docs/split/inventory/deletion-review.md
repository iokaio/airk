# Split inventory: U61 deletion review

Status: Independent U61 review, 11 November 2026; source review only (grep and
reads at `feat/split` 565a7a90; `python scripts/check-self-spawn.py` run, exit 0,
"6 reviewed production site(s), 5 legacy pending deletion"; no build, Clippy or
test was run). The AIRK MCP server failed to connect; all evidence is from local
reads. The reviewer did not write the inventories.

Verdicts:

- **CONFIRMED**: the evidence class holds now. The row authorizes deletion (U61)
  of exactly the named items, in a reviewed behavior step (not a structural step)
  that also removes the listed refusal tests or dead-code tests, followed by a clean
  compile and Clippy of the affected crate or package.
- **CONDITIONAL**: deletion is allowed only after the named later step lands and
  the named owner tests pass. Until then the row authorizes nothing.
- **REJECTED**: live duty, a live caller with no step that removes it, a
  contradiction with a U decision, or insufficient evidence. The row must be
  rewritten and reviewed again.

Common abbreviations: `SB` = `packages/vscode/src/serviceBootstrap.ts` (the
extension's native one-shot host, 935 lines, removed in Phase 6 per §10.3);
"Phase 6 removes SB" means its last importer is gone and the extension compiles.
"Scripts" means the §10.5.2 rewrite or retirement of the named check scripts.
Rust paths are under `crates/airk-cli/src/` unless prefixed.

Rule applied throughout: retiring a CLI leaf never authorizes deleting the
function it wraps when a kept module, the service, or a planned `airkctl`/setup
owner still needs that function. Each row names what stays.

## 1. `commands.md` (47 `retire` rows)

### F. Stdio transports

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| `mcp` | CONFIRMED (e3) | Refused before work: `main.rs:1361` → `runtime_pair::require_legacy_stdio` (`runtime_pair.rs:240-246`). `single_executable` returns true for an uninstalled development image and for every single-executable manifest (`runtime_pair.rs:208-235`), so every current build refuses. Refusal test `tests/service_runtime.rs:2255` `assert_retired_stdio_roles`. No production caller: the extension opens stdio only through an injected test transport (`explorerEvidence.ts:195,204`); `scripts/check-container.py:316` `check_stdio` runs only in the historical-image (non-`modern`) branch (`:627`). | Owner: service HTTPS `/mcp`; tests `crates/airk-cli/tests/mcp.rs` (real HTTPS MCP lifecycle) must pass. Delete the leaf and the stdio entry points `unified_mcp::serve`, `serve_combined`, `serve_scoped`, `serve_roots` (`main.rs:1537-1630`). **Keep** the rest of `unified_mcp.rs`: `with_runtime_readiness`, `http_attachment`, `http_project_attachments`, `call_explorer` are called by `http_backend.rs:138,152`, `combined_frontend.rs:333`, `main.rs:947`. Land in or after the §5.1 invocation step so a stale agent entry still gets a typed answer; update the `stdio_mcp_retired` text (`runtime_pair.rs:243`). Remove `service_runtime.rs:2259` and `cli_commands.rs:1422-1437` refusal cases in the same step. |
| `bridge` | CONFIRMED (e3) | Same refusal path (`main.rs:1361`, dispatch `:1508`); same refusal test. No extension or script caller other than the topology classifier (`check-process-topology.py:59`, which stays to classify old processes). | Same owner, tests and sequencing as `mcp`. |

### G. Service management and `service legacy`

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| `service rpc` | CONDITIONAL | Live callers: `SB:586,595,600` (health and arbitrary methods); scripts `check-service-clients.py`, `check-service-install.py`, `check-container.py`, `check-runtime-upgrade.py`, `check-service-console.py`, `check-service-pool.py`, `check-service-source-fairness.py`, `benchmark-mcp-investigation.py`. Commands Q1 (still open, not answered by U72-U78): `rpc` is the only CLI route to IndexSetup, source-mutation drafts and full-rebuild maintenance (`client_requests.rs:322`, `main.rs:442`, `:1650`). | After (1) Phase 1 I2 answers Q1 by naming the `airkctl` one-shot or admin page for every method `rpc` reaches, (2) Phase 3 `airkctl` implements them with parity tests, (3) Phase 6 removes SB, (4) scripts are rewritten. |
| `service console` | CONDITIONAL | `serviceClient.ts` `NativeControlTransport` (spawned with `service console`, `SB:613`); scripts `check-client-lifetime.py`, `check-service-clients.py`, `check-service-pool.py`, `check-storage-lifecycle.py`, `check-retrieval-quality.py`. | Owner `airkctl session` (Phase 3) with `fixtures/wire/session_frames.json` cases passing; Phase 6 removes `serviceClient.ts` console transport; scripts rewritten. |
| `service session` | CONDITIONAL | `nativeSession.ts#launchSession:spawn` is the per-window session today. | Owner `airkctl session` (Phase 3). The allowlist logic in `session_commands.rs` moves with it (§10.2); it is not deleted. Phase 6 replaces `nativeSession.ts`. |
| `service legacy derived-receipts` | CONDITIONAL | No extension or script caller; only `tests/legacy_native.rs:51,89`. Body is `airk_store::current::legacy_retirement::receipts` (`legacy_startup.rs:477`). U74(4) makes 1.3 derived-data removal a retirement duty, and these receipts are its resume record. | Leaf after Phase 7 retirement implements U74(4) with resume over these receipts (or a reviewed Q9 answer supersedes them). The store `receipts` reader stays until then. |
| `service legacy forget-derived` | CONDITIONAL | Only `legacy_startup.rs:480`; no external caller. | Same as `derived-receipts`. |
| `service legacy list` | CONDITIONAL | Production caller `SB:805`. Body `Journal::saved(..).list()` (`legacy_startup.rs:512`) reads the 1.3 scheduler removal journal that predecessors A3 resumes. | After Phase 6 removes SB and Phase 7 retirement owns the journal readers (they move, not delete). |
| `service legacy show` | CONDITIONAL | Production caller `SB:813`. | Same as `list`. |
| `service legacy forget` | CONDITIONAL | No external caller; `Journal::locked(..).forget` (`legacy_startup.rs:521`). | After Phase 7 retirement settles 1.3 journals (A3) and Q9 is answered. |

### H. Predecessor HTTPS migration

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| `runtime http-legacy-connection` | CONDITIONAL | Production caller `SB:688`; scripts `check-runtime-upgrade.py`, `setup-local-test.py` (+ `tests/test_local_test_setup.py`). The stdio refusal text names it as the migration path (`runtime_pair.rs:243`). | Leaf after Phase 6 removes SB and Phase 3/4 `airkctl agents repair/remove` exist (U12, U28). The leaf only; `runtime_install/http_legacy.rs` is **not** deletable with it (see §3 row `http_legacy.rs`). |
| `runtime http-legacy-union-connection` | CONDITIONAL | Production caller `SB:699`. | Same as above. |

### I. Lifecycle and discovery

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| `runtime ensure` | CONDITIONAL | `SB:438`, `serviceConsole.ts`, `serviceConsoleController.ts`; scripts `check-service-install.py`, `check-managed-runtime.py`, `check-runtime-activation.py`, `check-runtime-upgrade.py`, `check-service-clients.py`, `check-service-console.py`, `check-macos-direct-runtime.py`. | Owner `airkctl` automatic start (§6.3), Phase 3. Keep `ensure_running` (the Stop-honoring start logic the owner wraps); delete only the leaf and its universal-launcher publish branch (`runtime_install/commands.rs:470-486`). |
| `runtime reset-startup` | **REJECTED** | U75 (11 November) keeps "resetting login startup" as an `airkctl` offline recovery operation; it answers commands Q4, whose list names `runtime reset-startup`. The `retire` disposition predates U75. Caller `scripts/check-service-install.py`. Body reads staged `selected()` and `managed::read` (`commands.rs:783-799`). | Re-disposition to `airkctl` offline recovery (U75) with a fixed-layout implementation and an I2 entry. Not a deletion row. |
| `runtime location` | CONDITIONAL | `extensionUninstall.ts`, `uninstallHook.ts`, `SB:368`; `check-container.py:526,576`; `check-process-topology.py`, `check-service-clients.py`. | After the I4 pointer contract (Phase 1), `airkctl` pointer read (Phase 3), Phase 6 removal of the extension callers, and the §10.5.2 container rewrite. `discover_user_paths`/`UserPaths` stay for retirement. |
| `runtime inspect` | CONDITIONAL | `SB:375,440,459,575`; `check-container.py:633` (asserts `managed_policy`), `check-service-install.py`, `check-managed-runtime.py`, `check-runtime-upgrade.py`, `check-service-console.py`, `scripts/dev/s7_2_install_layout.py`. | Owner `airkctl service status` + I4 install record (Phases 1, 3, 7); container rewrite; Phase 6. |
| `runtime diagnose` | CONDITIONAL | `SB:322`; `check-runtime-activation.py`, `check-runtime-upgrade.py`, `check-service-install.py`. | Owner `airkctl service status` + offline `service logs` (§6.3, U75); Phase 6; scripts. |
| `runtime snapshot` | CONDITIONAL | `SB:378,617`; `serviceConsole.ts`, `serviceConsoleController.ts`, `serviceExtension.ts`; `check-runtime-upgrade.py`. | Owner `airkctl projects` + admin pages (Phases 3, 5); Phase 6. |

### J. Staged installation engine

Common condition for all 14 `e2` rows. The inventory preamble names only the
extension and script callers. Compiled Rust callers also remain, so e2 cannot
hold until **all** of these land:

1. Phase 7 setup role replaces `standalone_setup::install_payload`, which calls
   `stage` and `activate` (`standalone_setup.rs:207,246,261`) and is run by every
   installer today (`installers/windows/airk.iss:39,245`,
   `installers/linux/install.sh.in:29`, `installers/macos/Install AIRK.command.in:38`).
2. The §10.5.2 container rewrite removes `runtime container-host`, which calls
   `managed::configure` (`runtime_install/commands.rs:254-283`).
3. Kept retirement modules stop calling, or move, the staged readers they use
   today: `managed::require_unmanaged` (`replacement.rs:220`,
   `legacy_launchers.rs:447`, `runtime_macos_login.rs:256`),
   `activation::require_settled` and `activation::rebuild::*`
   (`legacy_launchers.rs:91,444,448`, `legacy_caches.rs:335,432`,
   `retirement/superseded.rs:73,156,191,439`, `runtime_macos_login.rs:38`),
   `staging::pending` (`superseded.rs:361`), `payload_repair::{preserved,require_settled}`
   and `launcher_repair::require_settled` (`legacy_launchers.rs:449-450`,
   `legacy_caches.rs:336,433`, `superseded.rs:364`). This needs the predecessors
   Q9 answer on interrupted journals.
4. Phase 6 removes SB, and the scripts are rewritten.

| Row | Verdict | Row-specific callers (besides 1-4) | Condition or surviving owner |
|---|---|---|---|
| `runtime stage` | CONDITIONAL (e2) | `SB:430,883`; `check-service-install.py`, `check-runtime-activation.py`, `check-macos-*`, `check-runtime-upgrade.py` | 1-4 |
| `runtime activate` | CONDITIONAL (e2) | `SB:432,887`; `check-runtime-activation.py`, `check-service-install.py`, `check-managed-runtime.py`, `check-macos-*`, `check-runtime-upgrade.py` | 1-4, and Q11 (whether fixed-layout setup still creates Genesis through `catalog-authority-probe`, `activation/catalog_upgrade/authority.rs:250`) |
| `runtime abort-activation` | CONDITIONAL (e2) | `SB:904`; `check-runtime-activation.py`, `check-runtime-upgrade.py` | 1-4 |
| `runtime abort-layout` | CONDITIONAL (e2) | No external caller; a pre-split layout claim is interrupted state (predecessors A11) | 1-4 and Q9 |
| `runtime abort-stage` | CONDITIONAL (e2) | No external caller; interrupted staging copy (A11) | 1-4 and Q9 |
| `runtime repair-launcher` | CONDITIONAL (e2) | `check-service-install.py:470-490`, `check-managed-runtime.py` | 1-4 |
| `runtime abort-launcher-repair` | CONDITIONAL (e2) | No external caller; `launcher-repair.json` is A11 state | 1-4 and Q9 |
| `runtime repair-payload` | CONDITIONAL (e2) | `SB:629` | 1-4 |
| `runtime inspect-candidate` | CONDITIONAL (e2) | `SB:859` | 1-4 |
| `runtime managed` | CONDITIONAL (e2) | `SB:426`; `check-managed-runtime.py` | 1-4. `managed.rs` readers stay for U73 (see §3) |
| `runtime prepare-client` | CONDITIONAL (e2) | `SB:439`; `check-container.py:640` (historical branch), `check-service-install.py` | 1-4 |
| `runtime remove` | CONDITIONAL (e2) | `check-runtime-activation.py:147` | 1-4 |
| `runtime retire-previous` | CONDITIONAL (e2) | `SB:484`, `serviceExtension.ts`; `check-service-install.py`, `check-runtime-upgrade.py`, `setup-local-test.py`, `check-service-frontend.cjs` | 1-4 |
| `runtime rollback` | CONDITIONAL (e2) | `check-service-install.py:27,36,563`; SB `runtimeTransition('rollback')` | 1-4 |
| `runtime launch` | CONFIRMED (e3) | Refused when the selected runtime is a single executable (`commands.rs:807-810` → `require_legacy_stdio`). Its only Rust caller is the `airk-launcher` name dispatch (`main.rs:1441-1455`, also retiring). Script uses are in historical-build branches (`check-service-install.py:465,496` under the non-`modern` path; `check-runtime-upgrade.py:1248,1736` `old_command`). The extension only builds these arguments as agent-entry data (`model.ts:39-50,115`) and recognizes them (`agentMigration.ts:71`). | Owner HTTPS `/mcp` (`tests/mcp.rs`). Delete together with the `airk-launcher` name dispatch. The recognition patterns must survive in the extension's removal-only recognizer (see the `agentMigration.ts` row in §2). |

### K. Refusal stubs

All 7 bodies only refuse (`runtime_install/commands.rs:166-176`, `:677-679`). A
`git grep` of `crates`, `packages`, `scripts` and `installers` finds no production
caller. The remaining references are refusal tests (`tests/catalog_upgrade.rs:278-365`,
`tests/cli_commands.rs:928-960,1367-1372,1611-1633`, `runtime_install/tests.rs:2247-2254`),
an extension negative test (`serviceBootstrap.test.ts:1131`) and
`docs/cli-reference.md`.

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| `runtime adopt-legacy-index` | CONFIRMED (e3) | As above | Owner: clean break (U13, U30). Remove the refusal tests in the same behavior step. Keep `legacy_maintenance` settlement (see §3). |
| `runtime adopt-legacy-indexes` | CONFIRMED (e3) | As above | Same |
| `runtime legacy-retirement-candidates` | CONFIRMED (e3) | As above | Same. Keep `CATALOG_UPGRADE_UNSUPPORTED`; it is still used by `authority.rs:258` and `catalog_upgrade.rs:61` |
| `runtime legacy-retirement-review` | CONFIRMED (e3) | As above | Same |
| `runtime legacy-retirement-authorize` | CONFIRMED (e3) | As above | Same |
| `runtime legacy-retirement-receipt` | CONFIRMED (e3) | As above | Same |
| `runtime legacy-retirement-resume` | CONFIRMED (e3) | As above | Same |

### L. Hidden and pre-Clap entries

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| `source-worker` | CONFIRMED (e3; e2 once the variant is gone) | `source_worker::run` is reached only from `Command::SourceWorker` (`main.rs:1504`). The watcher runs as an in-service thread calling `source_worker::serve` (`source_supervisor.rs:421-447`). `git grep` finds no spawner in `crates`, `packages` or `scripts`. `check-process-topology.py:55` forbids the role, and `check-self-spawn.py` `ROLE_ARG` would flag a new spawner. | Owner: in-service watcher. Tests: `source_supervisor.rs` `tests` (`:1590`) and `retry_tests`, `source_worker` `policy_tests` and `missing_source_tests`, airk-kernel `source_watch`. Delete the variant, `main.rs:1504`, the `session_commands.rs:232` arm and `source_worker::run`. Keep `serve` and `source_protocol`. Update `docs/process-model.md:29` and `docs/service-source-watching.md:19`, which still describe a process. |
| `explorer-assistance` | CONDITIONAL | Extension entries start it (`model.ts:110-116`, `explorerAssistanceLaunch.ts`). `explorer_assistance.rs` is also used by `http_endpoint/store.rs:487,546,577` and `service_runtime/http.rs:244` (HTTPS enrollment with an Explorer descriptor). | After Phase 3 adds the service-side Assistance queue (U10) and Phase 6 adds session delivery, and the HTTPS descriptor users are removed. |
| `runtime catalog-upgrade-probe` | CONFIRMED (e3) | Refusal-only (`commands.rs:171-176`); only `tests/catalog_upgrade.rs:284,302` | Owner: clean break. Keep the `CatalogAuthorityProbe { native_host: Some(_) }` refusal arm and the live `native_host: None` probe (`commands.rs:812-830`, called by `authority.rs:250`). |
| `runtime catalog-upgrade-migrate` | CONFIRMED (e3) | Refusal-only; only `tests/catalog_upgrade.rs:278,301` | Same |
| `shared-process-guardian` | CONDITIONAL | Pre-Clap entry refuses on single executables (`main.rs:1296-1308` → `runtime_pair.rs:248`). Compiled production callers remain: `embedding_pool_process/owned.rs:80` → `unix_guardian::shared::spawn` (`shared.rs:215`), `embedding_pool_process/shared.rs:134`, `embedding_pool_bootstrap.rs:196`. `check-self-spawn.py` lists it as LEGACY_PENDING "removed with the process roles (step H10)". | e2 after the paired legacy inference pool (`embedding_pool_process` owned/shared paths, `embedding_client.rs:68`) is deleted under its own reviewed row. Owner: in-service inference. |
| universal launcher (by executable name and pointer) | CONDITIONAL | Dispatch `main.rs:1345-1350`. `publish_for_default` is still called by `runtime ensure` for a non-single selection (`commands.rs:477`). | Delete `is_universal`, `run` and publish together with the `runtime ensure` row. **Keep** the retirement readers (`Pointer`, `resolve`, `retirement_image_for/at`, `retirement_digest_for`, `retire_for/at`, `publication_lock`) used by `legacy_launchers.rs:25,120,226,461-485` and predecessors A2/A4. Owner tests: `universal_launcher.rs` retirement tests (predecessors §6). |
| `airk-launcher` executable name | CONFIRMED (e3) | Forwards only to `runtime launch` (`main.rs:1441-1455`), which refuses on single-executable selections. | Delete together with `runtime launch`. Owner HTTPS `/mcp`. |

## 2. `extension.md` (rows marked remove or retire)

None of the 19 files can go before Phase 6. Each still has production
importers (listed in extension.md §5, re-checked by spot `grep`).

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| Cmd #7 `airk.removeTerminalCommand` | CONDITIONAL | Only the TS removes extension PATH/shell blocks today (`cliRemove.ts`); the native port is new code (predecessors A7) | Phase 7 native A7 port lands with Rust fixtures that reproduce `cliRemove.test.ts`, `cliInstall.test.ts:116-221` and `extensionUninstall.test.ts`, then Phase 6 |
| Cmd #8 `airk.showLastUninstallResult` | CONDITIONAL | Reads `last-result.json` written by the native uninstall worker (`uninstallHook.ts`) | Phase 6 removes native uninstall from `vscode:uninstall` (§8.5). The receipts become retirement input (A8, U74(3)) |
| Cmd #58 `airk.retryUpdate` | CONDITIONAL | Re-runs `automaticInstall` | Phase 6 removes `automaticInstall` (SB) |
| Setting `airk.cli.autoInstall` | CONDITIONAL | Read by `serviceDashboard.ts`, `serviceExtension.ts` | Phase 6 removes readers |
| Setting `airk.cli.shellIntegration` | CONDITIONAL | `serviceExtension.ts` (`shellScope`) | Phase 6 |
| Setting `airk.service.channel` | CONDITIONAL | `serviceConsole.ts` | Phase 6 |
| Setting `airk.service.installDir` | CONDITIONAL | `serviceConsole.ts`, `serviceExtension.ts` (configuration reads) | Phase 6. First confirm the removal-only recognizer (U12) can prove old entries that name a custom install root without this setting |
| Settings `airk.service.managedSource`, `managedSha256`, `managedLifecycle`, `managedStateDir` (4 rows) | CONDITIONAL | `serviceConsole.ts` | Phase 6 (U15, U73 native side) |
| Setting `airk.storage.path` | CONDITIONAL | `serviceExtension.ts:2093`, `cliRemove.ts` | Phase 6 and the A7 port |
| MCP provider `airk.explorerAssistance` | CONDITIONAL | Registered at `serviceExtension.ts:3462`. Resolve throws the `legacy_mcp_definition_retired_restart_airk` guidance for cached definitions | The inventory's own unverified premise (VS Code drops cached definitions of an uncontributed provider) must be observed in an installed editor first |
| Activation event `onMcpServerDefinitionProvider:airk.explorerAssistance` | CONDITIONAL | `package.json:73` | With the provider row |
| `files`: drop `native/**` | CONDITIONAL | Bundled native is verified by SB (`:209-251`) and sealed by `extensionUpdateLoader.ts` | Phase 6 removes SB and the native manifest entry (§8.5) |
| Start `nativeSession.ts#windowlessOneShot` | CONDITIONAL | Called from `nativeSession.ts:612` | Phase 6 (uninstall no longer runs native work) |
| Start `nativeSession.ts#differentBuildOneShot` | CONDITIONAL | `nativeSession.ts:614` | Phase 6 |
| Start `native.ts#prepareHelper:runText` | CONFIRMED (e2) | No production caller (`grep`: only `native.test.ts:380-391` and a comment in `cliInstall.ts:19`). The boundary test enforces this (`explorerBoundaries.test.ts:181,248`) | Delete the function and its tests in `native.test.ts`, and the roster entry. Retirement A6 reads helper caches natively; it does not need this creator |
| Start `native.ts#runJson:runText` | CONFIRMED (e2) | No production caller (only `native.test.ts:346,384`); enforced by `explorerBoundaries.test.ts:182,248` | Delete with its tests and the roster entry |
| Start `uninstallHook.ts#startUninstallWorker` | CONDITIONAL | `uninstallHook.ts:91-124`; inventory itself says "unless editor-storage deletion needs it" | Phase 6 shows the reduced editor-storage cleanup fits the hook's 5-second limit without a worker |
| Start `cliInstall.ts#installPath` | CONDITIONAL | With `cliInstall.ts` | See file row |
| Start `cliRemove.ts#removeShellIntegration` | CONDITIONAL | With `cliRemove.ts` | See file row |
| Start `privateDirectory.ts#run` | CONDITIONAL | With `privateDirectory.ts` | See file row |
| `serviceBootstrap.ts` | CONDITIONAL | 7 importers (`diagnosticLogs`, `extensionUninstall`, `serviceConsole`, `serviceConsoleController`, `serviceExtension`, `serviceWorkspace`, `uninstallHook`) | Phase 6 `src/airkctl/` replaces every native call it carries (see §1 rows) |
| `cliInstall.ts` | CONDITIONAL | Importers `cliRemove`, `extensionUninstall`, `serviceExtension`. Its tests are the only reference cases for A6 CLI copies and A7 blocks (U74(2)) | Phase 7 native A6/A7 port lands with Rust fixtures reproducing `cliInstall.test.ts:34-83,116-221`; then Phase 6 |
| `cliRemove.ts` | CONDITIONAL | Importers `extensionUninstall`, `serviceExtension`; reference for A7 | Same, with `cliRemove.test.ts` cases |
| `runtimeRebuild.ts` | CONDITIONAL | Importers SB, `serviceConsole`, `serviceConsoleController`, `serviceExtension` | Phase 6 |
| `serviceHttp.ts` | CONDITIONAL | 10 importers | Phase 6. Credential-input id and helper-command recognizer patterns move into `airkctl/agents/` first |
| `mcpHttpSession.ts` | CONDITIONAL | `explorerEvidence.ts:202`, `native.ts` `probeMcp` | Phase 6 session reads, and extension Q3 (`airk.verifyMcp`) answered in I2 |
| `privateDirectory.ts` | CONDITIONAL | `explorerAssistanceHost`, `privateIntentStore` (re-home), `serviceExtension` | Phase 6; the I4 ownership check may need its own reviewed Windows ACL read (§4 roster) |
| `agentMigration.ts` | CONDITIONAL | 5 importers. Holds the only recognition of old stdio entries: launcher path `:23`, `runtime launch … --` `:71`, `--root X mcp` and pinned forms `:83-91` | Move these patterns into the removal-only recognizer (`agentConfig.ts`, U12) with tests first; then Phase 6 |
| `httpIssuerRotation.ts` | CONDITIONAL | `SB`, `serviceExtension.ts:3063,3070` (updates owned issuer bundles in the environment collection) | Phase 6, with the environment-collection cleanup kept (see the last row) |
| `serviceDashboard.ts` | CONDITIONAL | `explorerSidebar.ts` | Phase 6 `status/` (§8.3) |
| `explorerAssistance/main.ts`, `server.ts`, `tools.ts`, `attach.ts`, `help.ts`, `helpCatalog.d.ts` (6 rows) | CONDITIONAL | Started by `airk explorer-assistance` from agent entries | Phase 3 service Assistance queue and Phase 6 session delivery (U10), together with the Rust `explorer-assistance` row |
| `explorerAssistanceHost.ts` | CONDITIONAL | `explorerAssistanceLaunch`, `serviceExtension` | Same |
| `explorerAssistanceLaunch.ts` | CONDITIONAL | `agentMigration`, `serviceExtension` | Same |
| `explorerWebview/runtimeUpgrade.tsx` | CONDITIONAL | Renders `RuntimeUpgradeProgress` from `ExplorerViewState` | Contract step C9 removes the field, and Phase 6 removes its host senders |
| §6 Assistance exports marked MGMT (retire): `ASSISTANCE_SERVER_NAME`, `ASSISTANCE_HELP_URI`, `WINDOW_SCHEMA`, `WINDOW_DESCRIPTOR_LIMIT`, `ASSISTANCE_IPC_LIMIT`, `WindowDescriptor`, `parseWindowDescriptor`, `AssistanceIpcRequest`, `AssistanceIpcReply` | CONDITIONAL | Used by the Assistance host, server and descriptor parsing | With the Assistance rows |
| §6 `WEBVIEW_MESSAGES` `indexing.create`, `indexing.context.read/preview/delete`, `indexing.grant.draft/use/revoke` (7 types) | CONFIRMED (no producer) | The only sender of Explorer webview messages is `explorerWebview/`, and `grep` finds none of these types there. The handlers are only reached through the switch at `explorerController.ts:656-667` | Delete the 7 message entries (`explorerContracts.ts:735-747,833`), their switch cases and the controller wrappers that then have no caller (confirm with `tsc`). **Keep** the host `ServiceExtension.useGrant` and `prepareWindowContext` (`serviceExtension.ts:523,616,646`): they serve the window search scope that U77 keeps |
| §9 reader and writer rows marked remove/retire (CA, token, endpoint, pipe, request-file and launch-descriptor readers in `serviceHttp.ts`, `mcpHttpSession.ts`, `native.ts:255`, `serviceExtension.ts` :911-944, :1436-1446, :1719-:2538 reads, `httpIssuerRotation.ts`, `agentMigration.ts:156`, SB :590/:766, `explorerAssistanceHost.ts`, `attach.ts`, `explorerContracts.ts:569-600`) | CONDITIONAL | All live in the current transport | Phase 6 `airkctl` session and one-shots (§4.4 boundary scan passes); Assistance rows for the pipe readers |
| §9 `serviceExtension.ts` :1914, :2459, :2495, :2510 (`AIRK_MCP_TOKEN_*`, `NODE_EXTRA_CA_CERTS`, `CODEX_CA_CERTIFICATE`, PATH in `environmentVariableCollection`) | **REJECTED** as "remove" | The split extension keeps the same id and is the only actor that can clear mutators this extension persisted (`cliInstall.ts:261-268` PATH prepend; token and CA entries; detection at `:2459`; cleanup at `:1758-1834`). The native installer cannot reach them (predecessors A7, Q7). Q7 is still open; no U decision assigns this cleanup | Writers may go in Phase 6. A reviewed clearing path for AIRK-owned variables must stay (or be added) until Q7 is decided. Rewrite the row |

## 3. `predecessors.md` §7 and §8

### §7 Retirement keep list

| Row | Verdict | Evidence | Note |
|---|---|---|---|
| `replacement.rs`, `replacement/operation.rs` | KEEP (agree) | A1/A9/A10 owner | Calls `managed::require_unmanaged` (`replacement.rs:220`); U73 replaces this with remove-if-proven |
| `upgrade_stop.rs`, `runtime_upgrade_stop_macos.rs` | KEEP (agree) | Shutdown proof | — |
| `legacy_processes.rs` | KEEP (agree) | Zero-match audit `replacement.rs:1026-1034` | — |
| `legacy_launchers.rs` | KEEP (agree) | `image_exclusion` | Depends on staged readers (`:91,444-450`); see §1 J item 3 |
| `retirement/superseded.rs`, `absent_optional_process_aliases` | KEEP (agree) | Image inventory | Depends on `activation::rebuild`, `staging::pending`, `payload_repair::preserved` |
| `universal_launcher.rs` readers | KEEP (agree) | `legacy_launchers.rs:25-485` | `run`/publish go with `ensure` (§1 L) |
| `user_paths.rs` `historical`, `owned_layout_locations`, `canonical_location`, `configuration_base`, `home` | KEEP (incomplete) | — | Add `select_layout` (`:405-436`, the newer `runtime[-preview]` family) and `owned_install_locations` (`:274-286`). §1 of the same inventory relies on both |
| `install_layout/` readers | KEEP (agree) | — | — |
| `install_files/` | KEEP (agree) | — | — |
| `service_manager*` readers and removers | KEEP (incomplete) | — | Name the U72 path explicitly: `service_manager_macos.rs:143-164,300-337` (`invoke` of the old binary's `runtime mac-login --action unregister`), with the new signature and digest check against the install receipt |
| `runtime_macos_login.rs` | KEEP (agree) | — | Calls `activation::startup` and `managed::require_unmanaged` (`:38,256`) |
| `legacy_startup.rs`, `journal.rs`, store `current/legacy_retirement.rs` | KEEP (agree) | "pending Q10" is stale: U74(4) makes this a live duty | — |
| `runtime_legacy_origin.rs` readers, `legacy_caches.rs` | KEEP (agree) | — | `legacy_caches.rs:375` uses `legacy_maintenance::Settlement`; add it, or decide it under Q9 |
| `trust.rs` `remove_owned`, `service_logs::has_saved_owner`, store readers | KEEP (agree) | — | `retire_owned_trust` has no test; add one before Phase 7 relies on it |

### §7 Staged engine delete list

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| `staging.rs` | CONDITIONAL | Callers `standalone_setup.rs:207,246`, `managed.rs`, `payload_repair.rs`, `superseded.rs:361`, `catalog_upgrade.rs`, `commands.rs` | §1 J items 1-4 |
| `activation.rs` and `selected.json` writers | CONDITIONAL | `standalone_setup.rs:261`; called by 20+ files, including kept `legacy_launchers`, `legacy_caches`, `superseded`, `runtime_macos_login` | §1 J items 1-4, plus Q11 (Genesis through `catalog-authority-probe`, `authority.rs:250`). The `activation::rebuild` readers move to retirement or are settled by Q9 |
| `managed.rs` | **REJECTED** as whole-file deletion | U73: a proven organization-managed pre-split root is removed. That needs the `managed-runtime.json` / `managed-detach.json` readers now used at `replacement.rs:219-255`, `legacy_launchers.rs:447`, `retirement.rs`, `runtime_macos_login.rs:256`. `managed::configure` is also called by `runtime container-host` (`commands.rs:271`) | Split the row: readers → retirement (keep); writers and pinning → CONDITIONAL on the §10.5.2 container rewrite and Phase 7 |
| `managed_service.rs` | CONDITIONAL | Only `runtime_install/commands.rs` | After the `runtime managed` and container-host leaves go |
| `payload_repair.rs` | CONDITIONAL | `require_settled` and `preserved` are called by kept modules (`legacy_launchers.rs:449`, `legacy_caches.rs:336,433`, `superseded.rs:364`, `retirement.rs`) | §1 J item 3 and Q9 |
| `launcher_repair.rs` | CONDITIONAL | `legacy_launchers.rs:450`, `retirement.rs`, `lifecycle.rs` | §1 J item 3 and Q9 |
| `retirement.rs` `retire_previous`, `remove`, `uninstall` slot logic | CONDITIONAL | `runtime uninstall` and root `uninstall` (Inno runs `uninstall --yes`, `airk.iss:278`); SB `retire-previous` | Phase 7 setup uninstall (U20, U32) replaces them; readers stay |
| `layout.rs` successor writer | CONDITIONAL | `layout/successor.rs`, activation | Phase 7; Q9 for abandoned claims |
| `clean_break_cleanup.rs` | CONDITIONAL | `activation/rebuild.rs`; journal `clean-break-cleanup-<sha>.json` is A11 state | Q9 decided, then with activation |
| `clean_break_background.rs` | **REJECTED** | Live caller in the running service loop: `service_management.rs:1346` `clean_break_background::Continuation::new`. No split step names its removal | Needs its own compatibility row: what pre-split state it continues, why U13 makes that unreachable, and which service step removes the call. Then e2 |
| `runtime_rebuild*` | **REJECTED** (row wrong) | No Rust module of that name exists (`ls runtime_install/`). The Rust rebuild code is `activation/rebuild.rs`, whose readers are called by kept modules (`legacy_launchers.rs:91,444`, `superseded.rs:73,156,191,439`, `authority.rs:15,158,205`). The extension's `runtimeRebuild.ts` is covered in §2 | Rewrite the row to name `activation/rebuild.rs` and split its readers from the writer |
| `legacy_maintenance.rs` | CONDITIONAL | Called by kept `legacy_caches.rs:375` and by `runtime_install.rs:2794-2800`, `activation.rs:851-852`, `commands.rs:304,878`; settles interrupted adoption (A11) | Q9 decided; `legacy_caches` decoupled |
| `http_legacy.rs` | **REJECTED** | Live HTTPS backend callers: `http_backend.rs:80,87,291` serve persisted `HttpSelection::ImportedLegacy` / `ImportedLegacyUnion` connections (also `http_endpoint/store.rs:535`). This is not staged-engine-only code; removing the variants changes a persisted credential/connection format | Needs U26 (a schema-identity proof against retained fixtures, or a user-approved new state contract) and a contract step that removes the variants. The two `http-legacy-*` CLI leaves can still go earlier (§1 H) |
| `data_migration.rs` | CONDITIONAL | Only `activation.rs:685-806` | With `activation.rs` |
| `replacement.rs` `decide` gate (`REPLACEMENT_THROUGH`) | CONDITIONAL | A behavior change, not a deletion | Phase 7 behavior step with reviewed new expectations for the four whole-plan-refusal tests (predecessors §2) |

### §8 Appendix D rows

None of these rows authorizes deletion yet. The surviving owner (the Phase 7
retirement module) does not exist. Its widened tests (both channels, per-item
continue-and-report) are listed in predecessors §6 "Gaps" as not yet written.

| Row | Verdict | Evidence | Condition or surviving owner |
|---|---|---|---|
| A1 roots | CONDITIONAL | `replacement.rs`, `operation.rs`, `upgrade_stop.rs` live | Retirement exists, with tests for both channels and both families. Fixture (f) changes under U73; (g) is "left and reported" under U74 |
| A2 custom roots | CONDITIONAL | Enumeration is new code | Retirement, with definition-parsing tests |
| A3 login registrations | CONDITIONAL | Live in `service_manager*`, `legacy_startup*` | Retirement. "Q1 open" is stale: U72 decided it (verified old-binary unregister only) |
| A4 universal launcher folder | CONDITIONAL | `universal_launcher.rs` | Retirement; U74(3) remnants rule |
| A5 Inno `standalone` | CONDITIONAL | Pascal only (`airk.iss`) | Native port; U74(1) proof; Q4 is decided |
| A6 editor caches and CLI copies | CONDITIONAL | `runtime_legacy_origin.rs`, `legacy_caches.rs`; TS `cliInstall.ts` | Native port of CLI-copy removal under U74(2) before the TS goes |
| A7 PATH and shell edits | CONDITIONAL | TS only | Native port with Rust fixtures reproducing the TS tests |
| A9 custom index data | CONDITIONAL | `replacement.rs:331-437` | Retirement |
| A10 trust | CONDITIONAL | `trust.rs`, `replacement.rs:933-971` | Retirement, plus a `retire_owned_trust` test |
| A11 interrupted pre-split operations | **REJECTED** | Status "Open (Q9)"; no surviving-owner semantics | Q9 must be decided before any code serving A11 is deleted (`operation.rs` journals, `activation/rebuild.rs`, `clean_break_*`, `legacy_maintenance.rs`, `payload_repair`/`launcher_repair` journals) |

## 4. Counts

| Source | CONFIRMED | CONDITIONAL | REJECTED | KEEP (agree or incomplete) | Rows |
|---|---|---|---|---|---|
| `commands.md` retire rows | 14 | 32 | 1 | — | 47 |
| `extension.md` remove/retire rows | 3 | 42 | 1 | — | 46 |
| `predecessors.md` §7 delete list | 0 | 11 | 4 | — | 15 |
| `predecessors.md` §7 keep list | — | — | — | 14 (2 incomplete) | 14 |
| `predecessors.md` §8 Appendix D | 0 | 9 | 1 | — | 10 |
| **Total deletion rows** | **17** | **94** | **7** | — | **118** |

The 14 confirmed `commands.md` rows are `mcp`, `bridge`, `runtime launch`, the
`airk-launcher` name dispatch, `source-worker`, the 7 K refusal stubs and the 2
`catalog-upgrade-*` stubs. The 3 confirmed extension rows are `prepareHelper`,
`runJson` and the 7 orphan `WEBVIEW_MESSAGES` types (counted as one row).

## 5. Inventory text that is wrong or stale

1. `commands.md` `runtime reset-startup` is marked `retire`. U75 keeps
   "resetting login startup" as an `airkctl` offline operation.
2. `commands.md` J preamble lists only extension and script callers. It omits the
   compiled Rust callers: `standalone_setup::install_payload`, which every
   installer runs; `container-host` → `managed::configure`; and the kept retirement
   modules that call staged readers.
3. `commands.md` L says `project-worker`/`derived-worker` are "spawned
   `project_supervisor.rs:547`". That site is `#[cfg(test)] spawn_image`
   (`:532-547`); production never spawns them. `process-guardian` is listed as a
   private role, but `check-self-spawn.py` marks `unix_guardian::spawn` as legacy
   pending deletion (H10). Neither is a §5.1 role.
4. Split-plan §5.3/§10.2 and the `mcp` row treat `unified_mcp.rs` as one stdio
   module. It also serves HTTPS (`http_backend.rs:138,152`), combined reads
   (`combined_frontend.rs:333`) and `explorer get-handoff` (`main.rs:947`); only
   the stdio `serve*` entry points retire.
5. `commands.md` `explorer-assistance` names only the extension caller.
   `explorer_assistance.rs` is also used by `http_endpoint/store.rs` and
   `service_runtime/http.rs`.
6. `commands.md` `service legacy list/show` omit their production caller
   (`serviceBootstrap.ts:805,813`).
7. `predecessors.md` §7 lists `runtime_rebuild*`. No such Rust module exists; the
   code is `activation/rebuild.rs`.
8. `predecessors.md` §7 lists `managed.rs` for deletion, against U73 and the
   `replacement.rs:220` caller. It lists `http_legacy.rs` and
   `clean_break_background.rs` for deletion although the live service calls them.
9. `predecessors.md` §7 keep list omits `user_paths::select_layout`,
   `owned_install_locations`, the U72 macOS unregister invoker, and the staged
   readers that kept modules call (`activation::rebuild`, `staging::pending`,
   `payload_repair::preserved`, the `*_settled` guards, `legacy_maintenance`).
10. `predecessors.md` §8 still shows Q1, Q4, Q5, Q6 and Q10 as open. U72-U74
    decided them. §8 also has no row for A8 (extension uninstall receipts).
11. `extension.md` §9 marks the `environmentVariableCollection` rows "remove". It
    does not keep the cleanup duty for values that earlier versions persisted
    (predecessors Q7, still open).
12. `docs/process-model.md:29` and `docs/service-source-watching.md:19` describe a
    supervised `source-worker` process. The watcher is an in-service thread.
