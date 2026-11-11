# Split inventory: airk commands (P0.1)

Status: Phase 0 draft, 11 November 2026; source review only (no build or test
was run); authorizes no deletion. Every `retire` row still needs the U61
independent review against its evidence class before any code is removed.
Decisions U1–U71 (split-plan §2.1) are taken as final.

## Sources and conventions

- Visible list: the 137 leaf headings of `docs/cli-reference.md` (146 `## airk`
  headings = root + 8 groups + 137 leaves) [V]. Hidden list: `#[command(hide = true)]`
  variants in `crates/airk-cli/src/main.rs` and `runtime_install.rs`, plus the pre-Clap
  argv dispatch in `main.rs:1271–1356` [V].
- `tools help` is a visible Clap subcommand but is omitted from the reference and
  from the coverage tables, because both filter any path segment named `help`
  (`main.rs:3320`, `tests/cli_commands.rs:1934`) [V]. It is listed with the hidden rows.
- Columns *Today*, *Local work*, *Sess* and *Coverage* are [V] (read in source)
  unless marked [I]. *Disposition* cites split-plan §6.2/§5.3/§10.2 where a row
  there names it; otherwise it is a proposal and marked [I].
- **Sess** = `service session` admission today (`session_commands.rs:102`):
  `C` concurrent, `X` exclusive, `—` refused.
- **Coverage**: `H` = in `HERE` (`tests/cli_commands.rs:1744`); `E: file · needle` =
  `ELSEWHERE` entry (`tests/cli_commands.rs:1325`); `none` = not in either table.
- **Evidence classes** (CLAUDE.md): `e1` not compiled; `e2` no caller after a
  clean compile and Clippy (applies once the named callers are removed);
  `e3` reviewed compatibility-inventory row naming a surviving owner whose
  tests pass.
- **Control methods** `M1`–`M24` are listed in (b); names are placeholders for Phase 1.

### Local-work codes

| Code | Meaning |
|---|---|
| INS | `runtime_install::installed_control` (`runtime_install.rs:745`): selection preflight, install lock, may **start** the service. `INSm` = `installed_metadata_control` (`:795`), metadata only |
| RES | Project resolution `service_frontend::resolve`/`saved` (`service_frontend.rs:396`, `:444`): opens the registry (`ServiceRegistry::open_saved`) and, without `--service-project`, runs kernel `planned_registration` (`airk-kernel/src/service_projects.rs:230`) |
| REG | Opens the service registry (`service-1.4.sqlite3`) in the CLI process |
| KRN | `airk-kernel` planning code in process |
| MCP | `airk-mcp` code in process (Unified router, tool catalog, help, Explorer offline) |
| JRN | Writes the client-request journal `<control>/client-requests/` (`client_requests.rs:213`) |
| FS | Other local file reads/writes (named in the row) |
| OS | OS login manager, user trust store or process control |
| STG | Staged-install engine (selection, slots, activation, payload, launcher) |
| PIPE | Control or project pipe requests only |

**A-std** (shared by every evidence read): INS + RES; `Unified` routing and argument
validation in process (`tools.rs:435`, MCP); reads over the project pipe
(`project_client.rs:1162/:1199`). With `--catalog-session`: `CatalogDetail` over PIPE
(`service_frontend.rs:287`). With `--combined-session`: REG + host context
(`combined_frontend.rs:104`, `:306`).

## Inventory

### A. Evidence reads

All dispatch through `query()` (`main.rs:2300`) to `tools::run_remote_selected`
(`tools.rs:391`) or `run_backend` (`:454`). Disposition for the family: §6.2
"kept as they are"; the local work in A-std moves to shared M1 (resolution) and
M3 (tool call). No command-specific method.

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `parse` | visible | Plan/execute a multi-step read request (`airk_parse`) | A-std; FS reads `--request` file/stdin ≤32 KiB (`main.rs:2345`) | `airkctl` | — | H | `--project` aliases `--service-project` |
| `search` | visible | Indexed search (`airk_search`) | A-std | `airkctl` | — | E: this file · `headless_workflow` | |
| `files` | visible | List indexed files (`airk_list_files`) | A-std | `airkctl` | — | H | |
| `read` | visible | Read indexed lines (`airk_read_file`) | A-std | `airkctl` | — | H | |
| `info` | visible | File info (`airk_file_info`) | A-std | `airkctl` | — | H | |
| `outline` | visible | File structure (`airk_outline`) | A-std | `airkctl` | — | H | |
| `symbol` | visible | Definitions by name | A-std | `airkctl` | — | H | |
| `imports` | visible | Import statements | A-std | `airkctl` | — | H | |
| `facts` | visible | Document facts | A-std | `airkctl` | — | H | |
| `refs` | visible | Textual identifier uses | A-std | `airkctl` | — | H | |
| `context` | visible | Lines around a line | A-std | `airkctl` | — | H | |
| `changes` | visible | Indexed changes since a revision | A-std | `airkctl` | — | H | |
| `code-facts` | visible | Code facts and signatures | A-std | `airkctl` | — | H | |
| `relationships` | visible | Symbol relationships | A-std | `airkctl` | — | H | |
| `status` | visible | Publication/freshness/job status (`airk_status`) | A-std; REG status-cache scope (`project_client.rs:756`) | `airkctl` | — | H | |

### B. Project, index and job commands

§6.2: "kept; local work moves into the service".

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `settings` | visible | Read a settings view or validate a draft file | INS, RES; paged native reads + page reassembly (`usage_client.rs:142`, `:232`, `airk_kernel::usage_page`) | `airkctl` | — | H | `--validate-file` path is sent to the service, not read locally. Reassembly is pure; P0 DTO inventory places it (Core) |
| `configure` | visible | Apply a configuration draft (file/stdin) | INS, RES; JRN; Inspect job + `Job` poll (`main.rs:2013`); KRN `prepare_input` via `service_configuration::prepare` | `airkctl` + M5–M9 | — | H | Contract change §5.2 (journal) |
| `index` | visible | Queue indexing job | INS, RES, JRN (`main.rs:1997`); `--full` refused locally (`:1650`) | `airkctl` + M5, M7, M8 | — | H | stderr names `airk service requests` |
| `setup-model` | visible | Queue `model_setup` job | as `index` | `airkctl` + M5, M7, M8 | — | E: scripts/check-service-clients.py · `setup-model` | |
| `inspect` | visible | Queue inspection job | as `index` | `airkctl` + M5, M7, M8 | — | H | |
| `pause` | visible | Pause project work (`Pause`) | INS, RES, PIPE | `airkctl` | — | H | |
| `resume` | visible | Resume (`Pause{paused:false}`) | INS, RES, PIPE | `airkctl` | — | H | |
| `watch` | visible | Source watching on/off (`ConfigureWatch`) | INS, RES, PIPE | `airkctl` | — | H | |
| `projects` | visible | List registered projects (`Projects` pages) | INS (or `--runtime-dir`), PIPE (`main.rs:970`) | `airkctl` | — | H | |
| `model-catalog` | visible | Offline model catalog and CPU choice | KRN `inspect_model_catalog`; bundled manifest (`main.rs:1526`) | `airkctl` + M10 | — | H | No service today |
| `init` | visible | Acknowledge an existing mapping, reserve and prepare storage | INS; KRN `service_registration::prepare`; PIPE `ReserveProject`; REG + FS create storage (`main.rs:1956`) | `airkctl` + M1, M4 [I] | — | E: this file · `installed_init_acknowledges_only_existing_reviewed_mapping` | Not named in §6.2; see Q6 |

### C. Usage

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `usage report` | visible | Usage summary (json/csv/markdown) | INS, RES; paged native reads + reassembly (`usage_client.rs:114`) | `airkctl` | C | H | `--price-scenario` path read by service |
| `usage policy` | visible | Recording mode and retention | INS, RES; PIPE `NativeRead` (`main.rs:1794`) | `airkctl` | C | H | |
| `usage clear` | visible | Delete recorded usage | via `configure()`: JRN, KRN, PIPE | `airkctl` + M5–M9 | X | H | |
| `usage configure` | visible | Set recording mode (alias `set`) | as `usage clear` | `airkctl` + M5–M9 | X | H | Keep alias `set` |
| `usage feedback` | visible | Record request usefulness | as `usage clear` | `airkctl` + M5–M9 | X | H | |
| `usage import` | visible | Import a usage export | `--preview`: FS read ≤64 KiB + Core validation (`main.rs:1817`); otherwise as `usage clear` | `airkctl` + M5–M9 | C (preview) / X | E: scripts/check-usage-inputs.py · `import` | Preview needs only Core types |
| `usage calibrate` | visible | Calibrate token estimates | as `usage clear` (paths read by service) | `airkctl` + M5–M9 | X | E: scripts/check-tokenizer-calibration.py · `calibrate` | |

### D. Tools

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `tools list` | visible | Unified tool catalog | MCP `airk_mcp::unified::tools` (`tools.rs:225`); no service | `airkctl` + M2 | — | H | §5.2 "served by the service from `airk-mcp`" |
| `tools call` | visible | Invoke one tool, print envelope | A-std; `airk_help`/`help` answered in process (`tools.rs:235`) | `airkctl` (M1, M3) | C | H | Help tool moves into M3 |
| `tools shell` | visible | Interactive connection-bound console | A-std + catalog | `airkctl` + M2, M3 | — | E: crates/airk-cli/tests/tools.rs · `shell` | Interactive; console stays in `airkctl` |

### E. Explorer (offline drafts)

All run `explorer_command` (`main.rs:893`) and write a new owner-only file
(`write_new`, `main.rs:874`). §6.2: "kept; served by the service". `airkctl`
keeps writing the `--output` file as the invoking user.

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `explorer draft-search` | visible | Validate and write a search draft | MCP `explorer_offline::draft` (`airk-mcp/src/explorer_offline.rs:187`); FS output | `airkctl` + M11 | — | H | |
| `explorer draft-strategy` | visible | Strategy draft | as above | `airkctl` + M11 | — | H | |
| `explorer draft-plan` | visible | Planner draft | as above | `airkctl` + M11 | — | H | |
| `explorer suggest-view` | visible | View suggestion | as above | `airkctl` + M11 | — | H | |
| `explorer validate-draft` | visible | Validate without writing a proposal | MCP `validate_draft` (`:234`); FS | `airkctl` + M11 | — | H | |
| `explorer list-templates` | visible | Bundled strategy templates | MCP `list_templates` (`:256`); FS | `airkctl` + M11 | — | H | |
| `explorer get-template` | visible | One template | MCP `get_template` (`:298`); FS | `airkctl` + M11 | — | H | |
| `explorer proposal-status` | visible | Status of a written draft | FS read ≤1 MiB; MCP `proposal_status` (`:329`) | `airkctl` + M11 | — | H | |
| `explorer get-handoff` | visible | Read a handoff from a running window | Connects to the window's Assistance pipe via descriptor (`unified_mcp.rs:1049`) | `airkctl` + M12 [I] | — | H (failure path) | U10 forbids service→window connection; Q7 |

### F. Setup, uninstall and transports

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `setup` | visible | Guided install, index selection, agent MCP setup, optional client launch | STG `install_payload` (`standalone_setup.rs:135`); enrollment (`:578`, `:749`) over PIPE; config rendering (`main.rs:2836`); OS launch of Claude/Codex | `airkctl (renamed: agents connect)` for enrollment; `--runtime-source` path → setup role (§6.2) | — | E: crates/airk-cli/tests/configuration_native.rs · `"setup"` | One leaf, two owners; `--launch` Q8 |
| `uninstall` | visible | Uninstall per-user service and runtime files | STG/OS `standalone_setup::uninstall` (`:276`) | `airkctl (renamed: service uninstall)` → setup role (U20) | — | E: this file · `standalone_uninstall_without_confirmation_preserves_the_explicit_installation` | |
| `mcp` | visible | Historical stdio MCP | Refused on single-executable installs (`runtime_pair.rs:240`, `main.rs:1361`) | retire (e3; owner: service HTTPS `/mcp`) §5.3 | — | E: crates/airk-cli/tests/service_runtime.rs · `assert_retired_stdio_roles` | |
| `bridge` | visible | Historical read-only adapter | as `mcp` | retire (e3; owner: HTTPS `/mcp`) §5.3 | — | E: crates/airk-cli/tests/service_runtime.rs · `bridge` | |
| `serve` | visible | Foreground service on an explicit control dir | Runs the service (`service_runtime.rs:506`) | `airk private role` (service) [I] | — | E: crates/airk-cli/tests/service_runtime.rs · `supervised_jobs_and_direct_queries_preserve_grants_and_stop_on_service_loss` | Heavily used by tests; Q2 |
| `host` | visible | Installed service under an external supervisor | = `runtime foreground` (`main.rs:1637`) | `airk private role` [I] | — | E: crates/airk-cli/src/runtime_install/commands.rs · `Host` | Q2 |

### G. Service metadata, requests and legacy

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `service health` | visible | Health check (`Health`) | PIPE | `airkctl` (§6.2) | — | H | `--runtime-dir` Q3 |
| `service shutdown` | visible | Drain and exit (`Shutdown`) | PIPE | `airkctl` (§6.2) | — | H | `service stop` is new |
| `service logs` | visible | Recent/all/raw logs, optional file capture | FS reads control logs (`service_logs.rs:496`, `:505`, `raw.rs:162`, `:168`), renders text (`:579`), FS writes `--output` | `airkctl` + M13 (§6.2) | C | H | Existing `Logs{limit}` may cover `--limit` [I]; offline read when down (§6.3) |
| `service register` | visible | Register root/data/collection | KRN `service_projects::registration` (`service_projects.rs:194`); PIPE `Register` | `airkctl` + M4 (§6.2) | — | H | |
| `service prepare-registration` | visible | Print canonical registration | KRN only (`service_registration.rs:7`); no service | `airkctl` + M4 [I] | C | H | |
| `service prepare-storage` | visible | Create reserved storage | REG + FS create dirs (`service_registration.rs:13`) | `airkctl` + M4 [I] | X | E: crates/airk-cli/tests/configuration_native.rs · `prepare-storage` | §5.2 row 2 |
| `service prepare-configuration` | visible | Prepare guarded configuration request | PIPE `Inspection` + KRN `prepare_input` (`service_configuration.rs:175`) | `airkctl` + M9 [I] | X | E: crates/airk-cli/tests/configuration_native.rs · `prepare-configuration` | |
| `service record-error` | visible | Record bounded client error without starting service | FS append raw ring (`raw.rs:122`) | `airkctl` + M14 [I] | X | E: packages/vscode/src/test/diagnosticLogs.test.ts · `record-error` | Service-down case Q4 |
| `service rpc` | visible | Raw `ControlMethod` JSON (refuses Connect*) | PIPE | retire (e3; owner: `airkctl` one-shots + admin pages) §5.3/§6.2 | C | E: scripts/check-service-clients.py · `"rpc"` | Q1 |
| `service console` | visible | Persistent JSON management transport | PIPE relay (`console_client.rs:164`) | retire (e3; owner: `airkctl session`) §5.3 | — | E: crates/airk-cli/tests/service_runtime.rs · `console` | |
| `service session` | visible | Framed per-window helper | Runs allowlisted commands in process (`session_commands.rs:16`) | retire (e3; owner: `airkctl session`) §5.3/§10.2 | — | H | Allowlist logic moves with it |
| `service requests prepare` | visible | Save a job request | REG namespace + JRN (`client_requests.rs:258`) | `airkctl` + M5 (§6.2) | — | H | §5.2 journal contract |
| `service requests prepare-configuration` | visible | Save a configuration request | JRN + M9 work (`:279`) | `airkctl` + M5 | — | E: crates/airk-cli/tests/configuration_native.rs · `prepare-configuration` | |
| `service requests list` | visible | List saved requests | REG + JRN read (`:301`) | `airkctl` + M6 | — | H | |
| `service requests retry` | visible | Submit/resubmit by nonce | JRN + PIPE (`:328`) | `airkctl` + M7 | — | H | |
| `service requests forget` | visible | Discard a saved request | JRN delete (`:354`) | `airkctl` + M8 | — | H | |
| `service legacy review` | visible | Review pre-1.4 login startup | FS + OS observation (`legacy_startup.rs:460`) | setup role (retirement, §9.3) [I] | C | E: crates/airk-cli/tests/legacy_native.rs · `review` | |
| `service legacy remove-startup` | visible | Remove reviewed pre-1.4 startup | OS + FS journal (`:463`) | setup role (retirement) [I] | X | E: packages/vscode/src/test/serviceBootstrap.test.ts · `remove-startup` | |
| `service legacy preview-derived` | visible | Preview 1.3 derived files | FS (`airk_store::current::legacy_retirement::preview`) | setup role (retirement) [I] | — | E: crates/airk-cli/tests/legacy_native.rs · `preview-derived` | |
| `service legacy remove-derived` | visible | Delete reviewed 1.3 derived files | FS delete | setup role (retirement) [I] | — | E: crates/airk-cli/tests/legacy_native.rs · `remove-derived` | |
| `service legacy derived-receipts` | visible | List derived cleanup scopes | FS receipts | retire (e3; owner: setup retirement receipts) [I] | — | E: crates/airk-cli/tests/legacy_native.rs · `derived-receipts` | |
| `service legacy forget-derived` | visible | Forget a derived scope | FS | retire (e3; owner: setup retirement receipts) [I] | — | E: crates/airk-cli/src/legacy_startup.rs · `ForgetDerived` | |
| `service legacy list` | visible | List removal receipts | FS journal | retire (e3; owner: setup retirement receipts) [I] | C | E: crates/airk-cli/tests/legacy_native.rs · `list` | |
| `service legacy show` | visible | Show one removal receipt | FS journal | retire (e3; owner: setup retirement receipts) [I] | C | E: crates/airk-cli/src/legacy_startup.rs · `Show` | |
| `service legacy forget` | visible | Forget a completed receipt | FS journal | retire (e3; owner: setup retirement receipts) [I] | — | E: crates/airk-cli/src/legacy_startup/tests.rs · `.forget(approval)` | |

### H. Runtime: HTTPS connections and trust

Bodies in `runtime_install/commands.rs:11` → `http_commands.rs` unless noted.

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `runtime http-connection` | visible | Enroll a saved selection | INS; FS selection file; PIPE `HttpEnroll` (`:357`) | `airkctl (renamed: agents connect)` §6.2 | X | E: this file · `native_http_commands_preserve_private_selection_and_revocation` | Existing method; `airkctl` writes agent config (§6.4) |
| `runtime http-connection-info` | visible | Existing connection's references | INS; PIPE `HttpEndpoint`; FS owner-private token + `CredentialStore` (`:156`) | `airkctl (renamed: agents info)` [I] + M15 | C | E: same | |
| `runtime http-credential` | visible | Bearer header for agent helper | FS owner-private token + `CredentialStore::authenticate`; no service (`:16`) | `airkctl (renamed: agents credential)` §6.2/§6.3 exception 3 | — | H | Needs token verification without the service crate (DTO inventory) |
| `runtime http-endpoint` | visible | Read HTTPS endpoint | INS; PIPE `HttpEndpoint` (`:139`) | `airkctl (renamed: agents endpoint)` [I] | C | E: same | |
| `runtime http-revoke` | visible | Revoke one connection | INS; PIPE `HttpRevoke` (`:416`) | `airkctl (renamed: agents revoke)` §6.2 | — | E: same | |
| `runtime lookup-http-connection` | visible | Typed connection ownership lookup | Install lock, STG selection, PIPE, FS `CredentialStore` (`:224`); `--purpose predecessor-cleanup` → `http_cleanup::lookup` (`http_cleanup.rs:134`) | `airkctl (renamed: agents list)` §6.2 + M16; cleanup purpose → setup role [I] | C | E: same | |
| `runtime http-trust` | visible | Inspect/install/remove issuer in user trust store | INSm; FS identity; OS trust store (`:476`, `http_endpoint/trust.rs:361`) | `airkctl (renamed: service trust)` §6.2 + M17; setup role at install | X | E: same | Q9 |
| `runtime http-issuer-rotation` | visible | Stage/commit/finalize issuer rotation | INSm; install + rotation locks; FS `http-1`; quiescence path when TLS cannot start (`http_rotation.rs:87`) | `airkctl (renamed: service issuer-rotation)` §6.2 + M18 | X | E: same | Q4 |
| `runtime http-legacy-connection` | visible | Migrate owned predecessor stdio config | INS; REG `http_legacy::resolve_claim`; PIPE `HttpEnroll` (`:432`) | retire (e3; owner: U12/U13 removal + `airkctl agents repair/remove`) [I] | X | E: same | |
| `runtime http-legacy-union-connection` | visible | Migrate multi-project declaration | as above (`:556`) | retire (e3; same owner) [I] | X | E: same | |

### I. Runtime: lifecycle, login, storage and discovery

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `runtime start` | visible | Explicit start; clears saved Stop | STG selection; OS process start (`lifecycle.rs:139`) | `airkctl (renamed: service start)` §6.2/§6.3 exception 2 | X | E: scripts/check-service-install.py · `start` | No control method |
| `runtime ensure` | visible | On-demand start honoring Stop; publishes universal launcher | STG; OS start (`runtime_install.rs:2483`) | retire (e3; owner: `airkctl` automatic start §6.3) [I] | X | E: scripts/check-managed-runtime.py · `ensure` | |
| `runtime lifecycle` | visible | Durable stop/restart request from stdin | REG lifecycle records; OS (`lifecycle.rs:180`) | `airkctl (renamed: service stop/restart)` §6.2 + M19 | X | E: crates/airk-cli/src/runtime_install/managed/tests.rs · `lifecycle` | Q5 |
| `runtime abandon-lifecycle` | visible | Abandon a saved lifecycle recovery intent | REG write (`lifecycle.rs:86`) | `airkctl (renamed: service abandon-lifecycle)` [I] + M20 | X | E: crates/airk-cli/src/runtime_install/lifecycle.rs · `abandon` | Q4 |
| `runtime reset-startup` | visible | Clear on-demand crash circuit | FS `startup-<sha>.json` (`commands.rs:783`) | retire (e3; owner: `airkctl service start`) [I] | — | E: scripts/check-service-install.py · `reset-startup` | Q4/Q6 |
| `runtime login plan` | visible | Show login registration plan | OS manager (`service_manager.rs:283`) | `airkctl (renamed: service login plan)` §6.2 + M21 | C | E: crates/airk-cli/src/runtime_install.rs · `Plan` | |
| `runtime login status` | visible | Login registration state | OS (`service_manager.rs:459`) | `airkctl (renamed: service login status)` + M21 | C | E: scripts/check-macos-direct-runtime.py · login status | |
| `runtime login install` | visible | Register login startup | Install lock; drain; OS (`commands.rs:872`) | `airkctl (renamed: service login install)` + M21; setup role at install (§5.2) | X | E: scripts/check-macos-direct-runtime.py · login install | |
| `runtime login start` | visible | Start registered login service | OS (`service_manager.rs:561`) | `airkctl (renamed: service login start)` + M21 | X | E: same script · login start | |
| `runtime login stop` | visible | Stop login service | OS (`runtime_install.rs:2634`) | `airkctl (renamed: service login stop)` + M21 | X | E: same script · login stop | Q5 |
| `runtime login uninstall` | visible | Remove registration; record opt-out | OS + FS opt-out | `airkctl (renamed: service login uninstall)` + M21 | X | E: same script · login uninstall | Q5 |
| `runtime inventory` | visible | AIRK-owned per-user data and index roots | FS read (`inventory.rs:110`) | `airkctl (renamed: service inventory)` §6.2 + M22 | C | E: crates/airk-cli/src/runtime_install/inventory.rs · `fn measurement_is_bounded` | |
| `runtime purge` | visible | Preview/confirm deletion of owned log/cache contents | FS delete (`inventory.rs:267`) | `airkctl (renamed: service purge)` §6.2 + M23 | X | E: crates/airk-cli/src/runtime_install/inventory.rs · `fn purge_contents_keeps` | |
| `runtime location` | visible | Resolve install location and user paths | FS discovery only (`commands.rs:284`) | retire (e3; owner: install pointer read by `airkctl`, U39/§6.3) [I] | C | E: packages/vscode/src/test/extensionUninstall.test.ts · `location` | Q6 |
| `runtime inspect` | visible | Verify selection, lifecycle, registry incarnation | STG; REG (`commands.rs:546`) | retire (e3; owner: `airkctl service status` + install record) [I] | C | E: scripts/check-service-install.py · `inspect` | Q6 |
| `runtime diagnose` | visible | Bootstrap diagnostics when start fails | STG records + FS logs (`commands.rs:721`) | retire (e3; owner: `airkctl service status` + offline `service logs`) [I] | C | E: scripts/check-service-install.py · `diagnose` | Q6 |
| `runtime snapshot` | visible | Saved bootstrap/project metadata | REG/FS read (`snapshot.rs:64`) | retire (e3; owner: `airkctl projects` + admin pages) [I] | C | E: crates/airk-cli/tests/service_runtime.rs · `snapshot` | Q6 |
| `runtime self-test` | visible | Executable/bundled-model self-test | FS bundled inventory (`commands.rs:846`) | `airk private role` (build/installer verification) [I] | C | E: crates/airk-cli/tests/bundled_model.rs · `self-test` | Q6 |
| `runtime source-scope` | visible | Map a source folder to its saved index | Install lock; REG root-scope read (`commands.rs:177`) | service control method M1 [I] | C | E: crates/airk-cli/src/runtime_install/commands.rs · `source_scope_authenticates_…` | No direct `airkctl` command proposed |
| `runtime foreground` | visible | Run selected service under external manager | STG; process forwarding (`commands.rs:329`) | `airk private role` [I] | — | E: crates/airk-cli/src/runtime_install/commands.rs · `Foreground` | Q2 |
| `runtime container-host` | visible | Linux container service from approved image | STG `managed::configure` + serve (`commands.rs:254`) | `airk private role` [I] | — | E: scripts/check-container.py · `container` | Depends on managed policy (U15); Q2 |
| `runtime uninstall` | visible | Stop/unregister, retire payloads | STG/OS (`retirement::uninstall`) | setup role (U20) §6.2 | X | E: packages/vscode/src/test/extensionUninstall.test.ts · `uninstall` | |

### J. Runtime: staged installation engine

U15 drops slots, rollback and organization-managed payloads; §10.2 removes the
engine "with evidence once no caller remains". Every row: retire (e2, after
the extension `serviceBootstrap.ts` and script callers are removed in Phases 6–7).

| Command | Vis | Today | Local work | Disposition | Sess | Coverage |
|---|---|---|---|---|---|---|
| `runtime stage` | visible | Stage offline payload | STG | retire (e2) §6.2 | X | E: crates/airk-cli/src/runtime_install/staging/tests.rs · `stage` |
| `runtime activate` | visible | Drain and select staged runtime | STG, OS | retire (e2) §6.2 | X | E: scripts/check-runtime-activation.py · `activate` |
| `runtime abort-activation` | visible | Restore preceding selection | STG, OS | retire (e2) | X | E: scripts/check-runtime-activation.py · `abort-activation` |
| `runtime abort-layout` | visible | Abandon state-layout claim | STG | retire (e2) | — | E: crates/airk-cli/src/runtime_install/layout/tests.rs · `failed_or_abandoned_claim_…` |
| `runtime abort-stage` | visible | Abort interrupted copy | STG | retire (e2) | — | E: crates/airk-cli/src/runtime_install/staging.rs · `abort` |
| `runtime repair-launcher` | visible | Repair launcher image | STG, OS | retire (e2) | — | E: scripts/check-service-install.py · `repair-launcher` |
| `runtime abort-launcher-repair` | visible | Abort launcher copy | STG | retire (e2) | — | E: crates/airk-cli/src/runtime_install/launcher_repair.rs · `abort` |
| `runtime repair-payload` | visible | Preview/repair selected payload | STG | retire (e2) | X | E: crates/airk-cli/src/runtime_install/payload_repair.rs · `fn ` |
| `runtime inspect-candidate` | visible | Verify candidate payload | STG | retire (e2) | C | E: packages/vscode/src/test/serviceBootstrap.test.ts · `inspect-candidate` |
| `runtime managed` | visible | Pin organization payload | STG | retire (e2; U15) §6.2 | X | E: crates/airk-cli/src/runtime_install/managed/tests.rs · `managed` |
| `runtime prepare-client` | visible | Create client launcher | STG | retire (e2) | X | E: scripts/check-container.py · `prepare-client` |
| `runtime remove` | visible | Remove unused runtime | STG | retire (e2) | — | E: scripts/check-runtime-activation.py · `remove` |
| `runtime retire-previous` | visible | Remove superseded payloads | STG | retire (e2) §6.2 | X | E: scripts/check-service-install.py · `retire-previous` |
| `runtime rollback` | visible | Restore preceding selection | STG | retire (e2) | — | E: crates/airk-cli/src/runtime_install/activation.rs · `rollback` |
| `runtime launch` | visible | Stable launcher forwarding | Refused on single-executable (`commands.rs:807`) | retire (e3; owner: HTTPS `/mcp`) §5.3 | — | E: crates/airk-cli/tests/service_runtime.rs · `launch` |

### K. Runtime: predecessor retirement and refused upgrade paths

| Command | Vis | Today | Local work | Disposition | Sess | Coverage | Notes |
|---|---|---|---|---|---|---|---|
| `runtime replace-installation` | visible | Plan/apply removal of earlier state | FS/OS (`replacement.rs`) | setup role (retirement, §9.3/§10.2) | X | E: crates/airk-cli/src/runtime_install/replacement/tests.rs · `fn replacement_removes_only_…` | Widened to §9.3 roster |
| `runtime inspect-legacy-caches` | visible | Inventory old helper caches | FS | setup role (retirement) [I] | C | E: this file · `legacy_runtime_commands_require_an_existing_owned_installation` | |
| `runtime prepare-legacy-caches` | visible | Freeze cache inventory | FS | setup role (retirement) [I] | X | E: same | |
| `runtime reprepare-legacy-caches` | visible | Re-review cache inventory | FS | setup role (retirement) [I] | X | E: same | |
| `runtime retire-legacy-caches` | visible | Remove inventoried caches | FS, OS | setup role (retirement) [I] | X | E: same | |
| `runtime inspect-legacy-launchers` | visible | Verify old launcher lineage | FS | setup role (retirement) [I] | C | E: same | |
| `runtime retire-legacy-launchers` | visible | Remove old launchers | FS, OS | setup role (retirement) [I] | X | E: same | |
| `runtime adopt-legacy-index` | visible | Refused (`standalone_adoption_unsupported`) | none (`commands.rs:677`) | retire (e3; owner: U13 removal/clean break) | — | E: same | Refusal stub |
| `runtime adopt-legacy-indexes` | visible | Refused | none | retire (e3; same) | — | E: same | Refusal stub |
| `runtime legacy-retirement-candidates` | visible | Refused (`catalog_upgrade_unsupported`) | none (`commands.rs:166`) | retire (e3; owner: clean break) | — | E: crates/airk-cli/tests/catalog_upgrade.rs | Refusal stub |
| `runtime legacy-retirement-review` | visible | Refused | none | retire (e3; same) | — | E: same | |
| `runtime legacy-retirement-authorize` | visible | Refused | none | retire (e3; same) | — | E: same | |
| `runtime legacy-retirement-receipt` | visible | Refused | none | retire (e3; same) | — | E: same | |
| `runtime legacy-retirement-resume` | visible | Refused | none | retire (e3; same) | — | E: same | |

### L. Hidden commands, omitted visible command and pre-Clap entries

| Entry | Kind | Today | Local work | Disposition | Coverage / caller | Notes |
|---|---|---|---|---|---|---|
| `tools help` | visible, omitted from reference | Tool description, schema, template | MCP catalog | `airkctl` + M2 | crates/airk-cli/tests/tools.rs (`tools help …`) | Not in HERE/ELSEWHERE (filter) |
| `dump-vocab` | hidden, feature `parser-training` | Offline intent-corpus extraction | Kernel/store on explicit data dir | `airk private role` (developer build only) [I] | none found [I] | Not in release builds |
| `project-worker` | hidden | Project worker process | Worker | `airk private role` | spawned `project_supervisor.rs:547` | |
| `derived-worker` | hidden | Derived worker process | Worker | `airk private role` | spawned `project_supervisor.rs:547` | |
| `source-worker` | hidden | Source watcher as a process | Worker (`source_worker.rs:73`) | retire (e3; owner: in-service watcher thread `source_supervisor.rs:432`) [I] | no process spawner found by grep [I] | Q10 |
| `process-guardian` | hidden (+ pre-Clap argv `main.rs:1310`) | Unix provider guardian | OS | `airk private role` | spawned `unix_guardian.rs:135` | Clap variant only refuses (`main.rs:1505`) |
| `explorer-assistance` | hidden | Window-hosted Assistance MCP adapter | FS descriptor, stdio | retire (e3; owner: service-side Assistance queue) §10.2 | packages/vscode `explorerAssistanceLaunch.ts` | |
| `runtime managed-serve` | hidden | Service entry for the login manager (Windows/Linux) | Service | `airk private role` (login-startup service) §5.1 | `service_manager.rs:358`; scripts/check-service-install.py | Name tied to staged layout [I] |
| `runtime login-serve` | hidden | macOS direct login service entry | Service | `airk private role` §5.1 | `service_manager_macos.m`; scripts/check-macos-direct-runtime.py | |
| `runtime login-foreground` | hidden | macOS login foreground entry | Service | `airk private role` §5.1 | same | |
| `runtime mac-login` | hidden | SMAppService status/register/unregister child | OS (`runtime_macos_login.rs:323`) | `airk private role` [I] | `service_manager_macos.rs:153` | Used by setup and M21 |
| `runtime catalog-authority-probe` | hidden | Installer-only Genesis authority observation | REG (`commands.rs:824`) | setup role [I] | `activation/catalog_upgrade/authority.rs:250`; tests/catalog_upgrade.rs | Q11 |
| `runtime catalog-upgrade-probe` | hidden | Refused | none | retire (e3; owner: clean break) | tests/catalog_upgrade.rs | |
| `runtime catalog-upgrade-migrate` | hidden | Refused | none | retire (e3; same) | tests/catalog_upgrade.rs | |
| `--verify-model-smoke` | pre-Clap (`main.rs:1272`) | Bundled-model smoke inference | Inference | `airk private role` (build verification) | scripts/build-service-runtime.py:225; check-service-clients.py:303 | |
| `--verify-bundled-models` | pre-Clap (`main.rs:1284`) | Bundled-model inventory check | FS | `airk private role` (build verification) | scripts/build-service-runtime.py:43 | |
| `shared-process-guardian` | pre-Clap (`main.rs:1296`) | Shared guardian; refused on single-executable | none | retire (e3; owner: in-service inference) | `unix_guardian/shared.rs:215` | |
| `__airk_pdf_worker` | pre-Clap (`airk-ingest/src/pdf_worker.rs:26`) | PDF parser child | Parser | `airk private role` §5.1 | not checked | |
| universal launcher | pre-Clap by executable name + pointer (`universal_launcher.rs:164`) | Forward to named installation | STG | retire (e3) §5.3/§10.2 | not checked | |
| `airk-launcher` executable name | dispatch (`main.rs:1441`) | Forward to `runtime launch` | STG | retire (e3) §5.3 | not checked | |
| `--version`, `--help`, `help` | Clap built-ins | Version and help | none | `airkctl version` (new, I2) and `airkctl` help; `airk` prints the §5.1 message | seeded goldens `version`, `help` | Not counted below |

## (a) Counts per disposition

| Disposition | Visible (137) | Hidden/omitted/pre-Clap (20) | Total |
|---|---|---|---|
| `airkctl` (same; only shared M1/M3) | 25 | 0 | 25 |
| `airkctl` + command-specific control method | 33 | 1 | 34 |
| `airkctl (renamed: …)` (with or without method) | 21 | 0 | 21 |
| service control method only (no command) | 1 | 0 | 1 |
| setup role | 12 | 1 | 13 |
| `airk private role` | 5 | 11 | 16 |
| retire | 40 (e2: 14; e3: 26) | 7 (e3) | 47 |
| **Total** | **137** | **20** | **157** |

Renamed split: 7 to `airkctl agents …` (6 `runtime http-*`/lookup rows plus
`setup` → `agents connect`, whose install path goes to the setup role) and 14 to
`airkctl service …` (13 `runtime` rows plus root `uninstall` → `service uninstall`).
Retire by family: F 2, G 8, H 2, I 6, J 15, K 7; hidden 7.

## (b) New control methods implied

Each handler must wrap the named existing function; none reimplements it.
Methods already present and reused unchanged: `Health`, `Shutdown`, `Register`,
`ReserveProject`, `Projects`, `Pause`, `ConfigureWatch`, `Enqueue`,
`ConfigureProject`, `Job`, `Inspection`, `RequestEpoch`, `RequestReceipt`,
`NativeRead`, `ConnectScoped`, `CatalogDetail`, `HttpEndpoint`, `HttpEnroll`,
`HttpRevoke`, `Logs` (78 `ControlMethod` variants today, `airk-core/src/service.rs:123–427`) [V].

| # | Placeholder | Wraps (file:line) | Used by |
|---|---|---|---|
| M1 | `ResolveProject` (binding, root/data/collection or root scope) | `service_frontend::resolve` `service_frontend.rs:396`, `saved` `:444`, `resolve_mcp` `:352`, `RootScope::metadata_before` `:233`; `runtime_install::default_project_data` `runtime_install.rs:727`; kernel `planned_registration` `service_projects.rs:230` | All project commands; `runtime source-scope`; `init`; `service prepare-storage` (with M4 storage step) |
| M2 | `ToolsCatalog` | `tools::inspect_catalog` `tools.rs:225`; `airk_mcp::unified::tools` `airk-mcp/src/unified.rs:152` | `tools list/help/shell` |
| M3 | `ToolsCall` | `tools::run_remote_selected` `tools.rs:391` (with `public_project_backend` `:435`), `tools::run_backend` `:454`, `combined_frontend::resolve` `combined_frontend.rs:306`, `airk_mcp::help::call` `airk-mcp/src/help.rs:273`; status cache `project_client.rs:756` | Family A, `tools call/shell`. Should share the HTTP gateway's tool route [I] |
| M4 | `PlanRegistration` and `PrepareStorage` (two methods) | `service_registration::prepare` `service_registration.rs:7`, `service_projects::registration` `service_projects.rs:194`; `service_registration::prepare_storage` `service_registration.rs:13` | `service prepare-registration`, `service register`, `service prepare-storage`, `init` |
| M5 | `ClientRequestPrepare` (job or configuration) | `client_requests::prepare` `client_requests.rs:258`, `prepare_configuration` `:279` | `index`, `inspect`, `setup-model`, `configure`, `usage` writes, `service requests prepare*` |
| M6 | `ClientRequestList` | `client_requests::list` `:301` | `service requests list` |
| M7 | `ClientRequestSubmit` | `client_requests::submit` `:328` | job commands, `service requests retry` |
| M8 | `ClientRequestForget` | `client_requests::forget` `:354` | job commands, `service requests forget` |
| M9 | `PrepareConfiguration` | `service_configuration::prepare` `service_configuration.rs:175` | `configure`, `usage` writes, `service prepare-configuration` |
| M10 | `ModelCatalog` | `airk_retrieval::model_catalog::inspect_catalog` `airk-retrieval/src/model_catalog.rs:260`; `bundled_assets::manifest` `bundled_assets.rs:72` | `model-catalog` |
| M11 | `ExplorerOffline` (tool + arguments) | `explorer_offline::draft` `airk-mcp/src/explorer_offline.rs:187`, `validate_draft` `:234`, `list_templates` `:256`, `get_template` `:298`, `proposal_status` `:329` | `explorer` (8 commands) |
| M12 | `ExplorerHandoff` (conditional, Q7) | `unified_mcp::call_explorer` `unified_mcp.rs:1049` | `explorer get-handoff` |
| M13 | `LogsExport` (all / raw; `Logs` covers recent [I]) | `service_logs::recent` `service_logs.rs:496`, `export` `:505`, `render` `:579`, `raw::export` `service_logs/raw.rs:162`, `raw::export_to` `:168` | `service logs` |
| M14 | `RecordClientError` | `service_logs::record_client_error` `service_logs/raw.rs:122` | `service record-error` |
| M15 | `HttpConnectionInfo` | `http_commands::existing_connection` `runtime_install/http_commands.rs:156` | `agents info` |
| M16 | `HttpConnectionOwnership` | `http_commands::lookup_connection` `http_commands.rs:224` (without staged-selection checks) | `agents list` |
| M17 | `HttpTrust` | `http_commands::trust` `http_commands.rs:476`; `http_endpoint::trust::manage` `http_endpoint/trust.rs:361` | `service trust` |
| M18 | `HttpIssuerRotation` | `http_rotation::run` `runtime_install/http_rotation.rs:87` | `service issuer-rotation` |
| M19 | `Lifecycle` (stop/restart) | `lifecycle::run` `runtime_install/lifecycle.rs:180` | `service stop/restart` |
| M20 | `AbandonLifecycle` | `lifecycle::abandon` `lifecycle.rs:86` | `service abandon-lifecycle` |
| M21 | `LoginStartup` (plan/status/install/start/stop/uninstall) | `commands.rs:288–328`, `install_login_startup` `commands.rs:872`, `stop_login` `runtime_install.rs:2634`, `ServiceManager::plan/status/start` `service_manager.rs:283/459/561` | `service login …` |
| M22 | `StorageInventory` | `inventory::inventory` `runtime_install/inventory.rs:110` | `service inventory` |
| M23 | `StoragePurge` | `inventory::purge` `inventory.rs:267` | `service purge` |

Count: 24 new methods (M4 is two; M12 conditional on Q7). Not methods
(stay in `airkctl` under §6.3 or Core): token read for the credential helper,
detached start, offline log read, uninstall hand-off, and the pure page
reassembly in `usage_client.rs:232` (Core placement decided by the P0 DTO
inventory, because `airkctl` cannot link `airk-kernel`/`airk-mcp`, U27).
Input for that DTO inventory [V]: today's client paths use
`airk_mcp::local::{NativeRead, SettingsView, ReportFormat}`,
`airk_mcp::read_attempt::ReadAttemptContext`, `airk_kernel::usage_page` and
`http_endpoint::CredentialStore` (credential helper).

## (c) Golden capture input for P0.3

The seeded `fixtures/cli-golden/cases.json` already holds 30 offline cases
(versions, help, several `runtime`/`service` metadata reads, `tools list/help`,
`model-catalog`, `usage import --preview`) [V]. Normalize the `airk` program
name in help and in stderr guidance (`main.rs:2005`, `:2049`, `tools.rs:252`)
before comparing `airkctl` output.

| Class | Commands (current `airk`) |
|---|---|
| Offline, no install or service | `--version`, `--help`/group/leaf help; `tools list`, `tools help <every tool>`, `tools call airk_help`; `explorer` draft-search/draft-strategy/draft-plan/suggest-view/validate-draft/list-templates/get-template/proposal-status; `explorer get-handoff` (missing-descriptor failure); `model-catalog`; `service prepare-registration`; `runtime location`, `runtime self-test`; refusal stubs (`adopt-legacy-*`, `legacy-retirement-*`, `catalog-upgrade-*`) |
| Offline with control/install fixtures (no live service) | `service logs` (json/text/all/raw), `service record-error`, `service requests list/forget`, `service legacy list/show/derived-receipts/preview-derived`, `runtime http-credential`, `runtime source-scope`, `runtime inventory`, `runtime purge` (preview), `runtime inspect/snapshot/diagnose`, `runtime replace-installation` (plan); `mcp`/`bridge`/`runtime launch` refusals (single-executable fixture); `index --full` refusal; failure paths of every A/B/C command (no installation) |
| Live service required | Family A (15) and `tools call`; `settings`, `configure`, `index`, `setup-model`, `inspect`, `pause`, `resume`, `watch`, `projects`, `init`; all `usage` except `import --preview`; `service health/shutdown/register/prepare-configuration/prepare-storage/rpc`, `service requests prepare/prepare-configuration/retry`; `runtime http-connection/http-connection-info/http-endpoint/http-revoke/lookup-http-connection/http-trust inspect/http-issuer-rotation status`; `runtime lifecycle`, `runtime start` |
| OS-dependent, T2 only | `runtime login *`, `http-trust install/remove`, `service legacy review/remove-startup`, `uninstall`, `runtime uninstall` |
| Not golden-captured (long-lived, interactive or retired engine) | `serve`, `host`, `runtime foreground/container-host/managed-serve/login-serve/login-foreground`, workers, guardians, `service session/console` (covered by framed tests), `tools shell` (scripted stdin only), `setup`, family J |

Reproduction through `airkctl` needs a live service for every row that gains a
control method, including the rows captured offline today (`tools list/help`,
`explorer`, `model-catalog`, `service prepare-registration`).

## (d) Open questions

| # | Question |
|---|---|
| Q1 | `service rpc` is session-admitted (C) and is the only CLI route the code points to for catalog IndexSetup, source-mutation drafts and full-rebuild maintenance (`client_requests.rs:322`, `main.rs:442`, `main.rs:1650`). §6.2 replaces it with "one-shots" without naming them. Which control methods get `airkctl` one-shots, which stay admin-page-only (U6), and does a raw developer path remain? |
| Q2 | External-supervisor and development service entries (`serve --runtime-dir`, `host`, `runtime foreground`, `runtime container-host`) are not among §5.1's private roles. Do they survive as private roles? `container-host` depends on the managed-policy code that U15 drops. |
| Q3 | Control-directory selectors (`--runtime-dir`, `--service-install-dir`, hidden `--service-runtime-dir`, `--data-dir`) let commands target any control directory, which tests and `serve` rely on. U39 says `airkctl` finds the installation only through the pointer, while §6.2 keeps arguments. Which selectors does `airkctl` keep? |
| Q4 | Operations that today work with the service down: `service record-error`, `runtime abandon-lifecycle`, `runtime reset-startup`, issuer rotation recovery when an expired issuer prevents TLS startup (`http_rotation.rs`, quiescent path), `login status`. §6.3 allows only offline log reads. Which need a setup-role or offline path? |
| Q5 | Stop, restart and `login stop/uninstall` as control methods: today `lifecycle::run` and `stop_login` run in the CLI process. Which process owns relaunch on restart, and the observation that the old service exited? |
| Q6 | Commands without a §6.2 row, proposed here [I]: `runtime location/inspect/diagnose/snapshot/reset-startup/self-test/source-scope/http-endpoint/http-connection-info`, `init`, `service prepare-*`, `service record-error`, `service legacy` receipt commands. Confirm or correct before Phase 3. |
| Q7 | `explorer get-handoff` reads a token that exists only in the window; U10 forbids the service connecting to the window. Is it served through the window's `airkctl session` (window publishes handoffs to the service), or retired? |
| Q8 | `setup --launch` (starts Claude Code/Codex with credentials in the child environment) and `--client` values other than Claude/Codex/VS Code have no §6.4 equivalent. Add `airkctl agents launch`, or retire? |
| Q9 | `http-trust install/remove` changes the CurrentUser Root store, which raises an OS consent dialog for the calling process. Phase 1 must confirm the resident service process (login-started) can raise and own that dialog on Windows and macOS, or the operation runs in `airkctl`/setup instead. |
| Q10 | `source-worker`: the watcher now runs as an in-service thread (`source_supervisor.rs:432`); no process spawner of `source-worker` was found by text search. Confirm with a reviewed e3 row before deletion. |
| Q11 | `runtime catalog-authority-probe` is called by fresh activation (`authority.rs:250`). Does the fixed-layout setup role still create Genesis through this probe, or does the service create it on first start? |
