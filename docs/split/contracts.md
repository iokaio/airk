# Split Phase 1 contract drafts (P0.6)

Status: Phase 0 draft, 11 November 2026; source review only; lands in Phase 1 steps.

No build, test, Cargo or npm command was run. This draft authorizes no code,
deletion or wire change; each section lands as its own reviewed Phase 1 step
(C1–C11, §11) with independent fixtures and no consumer change. U1–U78
([split plan](split-plan.md) §2.1) and the ledger row "P0 decisions" are final.
Inputs: the [commands](inventory/commands.md), [dependencies](inventory/dependencies.md),
[extension](inventory/extension.md) and [build](inventory/build.md) inventories,
the current contract sources and the [generation pilot](../simplify/contract-generation-pilot.md).

## 0. Conventions and step map

- **[V]** verified in code at `565a7a90` (file:line cited, or cited by an
  inventory that verified it). **[I]** inferred; needs confirmation. **[P]**
  proposal of this draft, to be reviewed in its landing step.
- Rust paths are relative to `crates/airk-cli/src/` unless prefixed; `airk-cli`
  becomes `airk-service` by U34. "plan §n" is the split plan; bare §n is this file.
- "Owner pipe" = the owner-only named pipe (Windows) or peer-checked Unix
  socket of I1 [V `local_ipc*.rs`]. Only the same OS user can connect.
- Every contract section starts with an **inventory row** (split plan §4.2).
  "Fixtures-only" means D5 applies (no type generation); only I2 and I3 run the
  U47 generation pilot.
- Generated output is never its own expected value. Operation sets,
  compatibility expectations and decision cases are reviewed, checked-in
  fixtures whose expectations are written independently of the code under test
  (as `fixtures/wire/read_retry_decisions.py` does today [V]).

| Step | Contract | Section | Depends on |
|---|---|---|---|
| C1 | Pure Core types moved from kernel/service client | §1 | — |
| C2 | I1 method and response variants, typed `not_yet_served` arms | §2.2–2.3 | C1 |
| C3 | I1 request journal, receipts, window plane, events, launch code | §2.4–2.8 | C2 |
| C4 | I2 `airkctl` command, output, exit, version, session v2 (+U47 pilot) | §3 | C3 |
| C5 | I3 admin HTTP contract (+U47); **gated on the security review** | §4 | C3, C4 |
| C6 | I4 install pointer and record, setup operation, role admission | §5 | C4 |
| C7 | Artifact receipts and release catalog | §9 | C6 |
| C8 | Rust-emitted cases for `nativeOutcome.ts` and `readRetryDelay` gaps | §10 | C4 |
| C9 | UI view-state and adapter contract | §6 | C4, C5 |
| C10 | Signing identity file | §7 | — |
| C11 | Catalog ownership reversal | §8 | C2 |

## 1. C1 — Pure Core types moved from kernel and the service client

| Inventory | |
|---|---|
| Owner | `airk-core` (new leaf modules; previous owners re-export) |
| Consumers | Service (`usage_client`, `service_frontend`, `airk-kernel`); later `airkctl` and `airk-ipc` |
| Surfaces | Rust library only; serialized forms appear inside I1 responses |
| Authorization | None (vocabulary, not operations; conformance P1–P9 not applicable, as in `service_records/CONTRACT.md` [V]) |
| Version / unknown fields | Field names, order, serde tags unchanged; `deny_unknown_fields` kept where present today |
| Limits | Existing ones move unchanged (for example `usage_page::page(.., max_bytes)` [V `airk-kernel/src/usage_page.rs:94`]) |
| Cancellation / retry | Not applicable (pure) |
| Fixtures | `fixtures/wire/usage_page.json` (exact bytes of `Page`, `Record`, `Part` and `assemble` vectors); `fixtures/wire/root_resolution.json` |
| Generation | Fixtures-only (D5; outside the U47 pilot) |
| T0 | `cargo test -p airk-core --lib wire_fixture` (must select the new cases) |
| Lands in | C1, as a structural move: kernel/service keep `pub use` re-exports, so no consumer changes |

Option A is decided (P0 decisions): the query client, `Backend`, status cache
and channel pool stay in the service, and `airkctl` reads through I1. C1
therefore moves only what crosses I1 or what `airkctl` must run itself.

| Type or function | From | To | Reason | Mark |
|---|---|---|---|---|
| `usage_page::{Part, Record, Page, page, scoped_page, assemble}` | `airk-kernel/src/usage_page.rs` (332 lines; serde, sha2, core only) | `airk_core::usage_page`; kernel `pub use` | `airkctl` reassembles paged settings/usage reads (`usage_client.rs:38,293,315`); it cannot link the kernel (U27) | [V] source; [P] destination |
| `RootScope`, `RootResolution`, `RootObservation` result shapes | `service_frontend.rs:9-44` | `airk_core::service_resolution` | Become the M1 response; resolution behavior stays in the service | [V] location; [I] exact field set |
| Credential record parse/verify (pure part of `http_endpoint::CredentialStore::authenticate`) | `http_endpoint` | `airk_core::service_http` | Agent credential helper (plan §6.3 exception 3) runs in `airkctl` without the service crate; the file read stays an `airkctl` adapter | [I] needs a read of the store to separate pure from I/O |
| Already in Core, import path only | `NativeRead`, `SettingsView`, `ReportFormat` (re-exported by `airk_mcp::local`), `ReadAttemptBudget` | — | Class 5 | [V] |
| Not Core | `NativeReadFailure`, `fresh_incarnation()` → `airk-ipc` (Phase 2 G4); `ReadAttemptContext` stays MCP; `refusal_guidance` text → `airkctl` rendering; `StatusReceipt` deferred (no non-service consumer under Option A) | — | No OS behavior or runtime objects in Core | [V] per dependencies.md A2/A3 |

Golden rule for C1: the moved serializations reproduce `fixtures/wire/usage_page.json`
byte for byte, and the existing `catalog_receipt.json` and `session_frames.json`
tests still select and pass.

## 2. I1 additions — local control protocol

### 2.1 Inventory row

| Inventory | |
|---|---|
| Owner | `airk-core::service` (`ControlMethod`, `ControlResponse`, request/response records); transport `airk-ipc` after Phase 2 |
| Consumers | `airkctl` (one-shots and session), the admin dispatcher (in-process, same handlers), `airk` composition root |
| Surfaces | Owner pipe only. The operation table (§4.3) decides which methods the session relay and `/admin` may reach |
| Authorization | Same OS user by owner/peer check [V `require_owner_private`, `caller_identity`]; per-method rules below; checkpoint admission unchanged (`runtime_upgrade_in_progress`, `service_runtime.rs:157-185` [V]) |
| Version | `PROTOCOL_VERSION = 1` kept [P]: additions are new variants. `airk`/`airkctl` keep exact pair equality (plan §4.2): `Health.version` equals the client version and `Health.executable_sha256` equals the I4 record's `airk` digest, checked before any mutation |
| Unknown fields | `deny_unknown_fields` on every new record and enum, as today [V `service.rs:122,430,503`]. Unknown variant → request refused before dispatch |
| Limits | One request per connection, request read deadline 5 s [V `service_runtime.rs:138`]; default frame ≤ `MAX_FRAME_BYTES` 65,536 both ways [V `service.rs:14`, `local_ipc.rs:257,291`]; per-method raised budgets in §2.8 |
| Deadlines | Client deadlines 5/10/20/30 s and `registry_busy` retry ≤ 6 within 3 s [V dependencies.md A2, `service_runtime.rs:808-871`]; long reads carry `ReadAttemptBudget` |
| Cancellation | Dropping the connection cancels delivery only; accepted work keeps its admission until it finishes (plan §4.5) |
| Retry / recovery | Reads: existing read-retry rules only. Mutations: request keys and receipts (§2.4); never replayed by a transport |
| Fixtures | `fixtures/split/i1/control_methods.json` (one request and response per new variant, malformed cases), `fixtures/split/i1/not_yet_served.json`, `fixtures/split/i1/request_recovery.json`, `fixtures/split/i1/window_plane.json`, `fixtures/split/i1/admin_launch_code.json` |
| Generation | Fixtures-only (D5). I1 is not in the U47 pilot |
| Lands in | C2 (variants + refusal arms), C3 (journal, window plane, events, launch code) |

### 2.2 New `ControlMethod` / `ControlResponse` variants (C2)

Method names are those in [commands](inventory/commands.md) (b), finalized
[P]. "Wraps" cites the existing function the handler calls unchanged [V at
`caab8ec9`]. Responses whose existing function returns `serde_json::Value`
carry it as `result: Value`; its shape is pinned by the CLI goldens
(`fixtures/cli-golden/`), not by new types. Admission column: **R** read,
**M** mutation, **X** exclusive with installer-class work (the session's
current classes, `session_commands.rs:102` [V]).

| # | Method { request fields } | Response { fields } | Wraps | Adm. | Idempotency |
|---|---|---|---|---|---|
| M1 [3c.1: binding and registration; 3c.4c: catalog session, served; amended] | `ResolveProject { target: Binding{project} \| Registration{root, data_root, collection} \| Catalog{request: ScopedConnectRequest} }` (absolute paths; `airkctl` supplies `data_root`, by default the install record's `locations.data`, U39) | `Resolved { registration }` | `service_frontend::saved` :444 (read-only registry lookup; `planned_registration` for a registration); `service_frontend::resolve_scoped` :287 for a catalog session (the `CatalogDetail` read and its readiness checks; the index's backing project, as the CLI resolved it before `tools shell`; a session that does not resolve answers `RegistrationRefused { message }` with the CLI's error chain, since the reason is the user's next step); the service never starts or elects another. `RootScope{root}` and the `observation`/`planned` fields land with their first caller, `runtime source-scope`'s consumer (Phase 6; no `airkctl` command, commands.md); `init` needs only M4 (3c.4) | R | Read |
| M2 [landed: `airk_core::service`, fixture `fixtures/split/i1/control_methods.json`] | `ToolsCatalog { detail: List{schemas: bool} \| Help{tool}, combined?: CombinedConnectRequest }` (3a.2d: a combined session's catalog, served after the session resolves) | `ToolsCatalog { document: String }`: the exact pretty JSON text `tools list --json` / `tools help --json` printed, carried as text so a client keeps the help document's field order (list documents are key-sorted values); an unknown tool → `unknown_tool` | `tools::inspect_catalog` `tools.rs:225`; `airk_mcp::unified::tools` | R | Read |
| M3 [contract landed 3a.2a: `airk_core::service`, group `tools_call` in `fixtures/split/i1/control_methods.json`] | `ToolsCall { tool, arguments: Map, selection: Unselected \| Project{binding} \| Catalog{request: ScopedConnectRequest} \| Combined{request: CombinedConnectRequest, project_ids?, index_ids?, member_availability?} (3a.2d; the service adds the narrowing defaults to the arguments and refuses a conflicting argument): Terse \| Full, usage_client }`. Without a catalog session the help tool is answered in process before any resolution, as `main.rs:1536` does (`Unselected` serves only it; another tool fails `project binding required`). `Window{..}` with W4 (3f), `Admin` (a unit selection, `"admin"`; the calling admin session's A2 grant, resolved only by the admin plane; on the pipe `admin_scope_not_granted`; amended Phase 5b). No `read_budget`: the service runs the CLI's own call path, whose per-exchange `ReadAttemptBudget` and selection-admission retry stay inside `project_client` unchanged | `ToolResult { result: Value }` (the complete unified envelope the CLI printed) or `ToolCallFailed { message }` (the CLI's `operation_failed` message, full error chain ≤ 4,096 characters) | `tools::run_remote_selected` :391 with `public_project_backend` :435, `RemoteSession::call` (response-mode insertion, `remote::call_observed`); `Catalog` via `service_frontend::resolve_scoped` :287; status cache `project_client.rs:756` | R | Read; read-retry rules unchanged |
| N1 [contract landed 3c.5c-1; amended] | `UsageDocument { project, catalog?: ScopedConnectRequest, read: Settings{view} \| ValidateSettingsFile{view, path} \| Report{days, format} \| ReportFile{days, format, price_scenario_path} \| Policy }`: one complete document instead of per-page scoped reads, so the CLI's pager and reassembly (`usage_client::collect_selected`) run once, in the service, unchanged, and `airkctl` mirrors nothing; paths absolute | `UsageDocument { document }` (the value `airk` printed; a CSV or Markdown report is a string) or `UsageReadFailed { message, native? }` (error chain ≤ 4,096 characters; `native` is a failed native read's complete result, which `airk` printed on stdout before its error line) | `usage_client::{settings_selected, validate_settings_file_selected, render_selected, render_file_selected}` and the `UsagePolicy` read (`main.rs:1799`) | R | Read |
| M4a [served 3c.4b; amended] | `PlanRegistration { root, data_root, collection, storage: Planned \| Existing }` (absolute paths; `airkctl` supplies `data_root` as for M1, U39; `Planned` for `init`/`service prepare-registration`, `Existing` for `service register`, as the CLI chose) | `RegistrationPlan { registration }`, or `RegistrationRefused { message }` (full error chain ≤ 4,096 characters, as the CLI's `operation_failed` line) | `service_registration::prepare` :7 (`planned_registration`); `service_projects::registration` | R | Read |
| M4b [served 3c.4b; amended] | `PrepareStorage { project: ProjectBinding }` (the binding the CLI's `--project` named; the registry supplies the reservation) | `StoragePrepared { registration }` or `RegistrationRefused { message }`; no `created` flag (the canonical function does not observe it) | `service_registration::prepare_storage` :13 | M, X | Idempotent: a repeat finds the storage and answers the same registration |
| M5 [served 3c.3b] | `ClientRequestPrepare { caller, key: RequestKey, request: ControlMethod (Enqueue \| ConfigureProject only) }` | `ClientRequestPrepared { nonce, project, kind, duplicate: bool }` | `request_journal::prepare_keyed` over `persist_locked` (journal owner `request_journal.rs`; epoch acquisition and key generation move to the client, §2.4) | M | Keyed (§2.4) |
| M6 [served 3c.3b] | `ClientRequestList {}` | `ClientRequests { pending: [Pending{nonce, project, kind}], capacity }` | `request_journal::list` | R | Read |
| M7 [served 3c.3b] | `ClientRequestSubmit { nonce }` | existing `Job { job }` or `Failure { code }` | `request_journal::saved` + `check_receipt` (crossed-receipt check kept) | M | Keyed by the saved key |
| M8 [served 3c.3b] | `ClientRequestForget { nonce }` | `ClientRequestForgotten { nonce, existed }` | `request_journal::forget_existed` | M | Absent → `existed:false` |
| M9 [served 3c.5b; amended] | `PrepareConfiguration { project, draft: ConfigurationDraft }`: the typed `configure --file` draft, now owned by `airk_core::service_configuration` (moved from `airk-cli`'s `Draft` by `scripts/dev/split_3c5a_configuration_draft.py`, same JSON), so `airkctl` builds the `usage` drafts without a mirror; paths are absolute | `ConfigurationPrepared { request: ConfigurationRequest }` or `ConfigurationRefused { message }` (the CLI's error chain, ≤ 4,096 characters) | `service_configuration::prepare` :175 | R | No effect |
| M10 [served 3c.5d; amended] | `ModelCatalog {}` (the command takes no CPU argument; the kernel inspects the host) | `ModelCatalog { document: Value }` (≈4.5 KiB; the default frame), or `UsageReadFailed { message }` with the error chain | `airk_retrieval::model_catalog::inspect_catalog` :260; `bundled_assets::manifest` :72 | R | Read |
| M11 [served 3d; amended] | `ExplorerOffline { operation: DraftSearch \| DraftStrategy \| DraftPlan \| SuggestView \| ValidateDraft \| ListTemplates \| GetTemplate \| ProposalStatus, arguments: Map, document?: Value, project?: ProjectBinding }` (`document` is the draft `proposal-status` read; `project` the `--service-project` a draft falls back to; the service supplies the clock) | `ExplorerOffline { result }` or `ExplorerOfflineFailed { message }` (error chain ≤ 4,096 characters); `airkctl` writes the result to the new `--output-file` (U79) as `airk` wrote `--output` | `airk_mcp::explorer_offline::{draft :187, validate_draft :234, list_templates :256, get_template :298, proposal_status :329}` | R | Pure |
| M12 | `ExplorerHandoff { handoff_id }` | `ExplorerHandoff { handoff: Value }` | Today `unified_mcp::call_explorer` :1049 (window pipe); after C3 reads the W8 store (P0 decision: handoffs flow through the window session) | R | Read |
| M13 [amended U88: not added] | — (recent pages use the existing `Logs { limit }`) | `Logs { page }` | `service_logs::recent` :496 for recent; `--all`/`--raw` stay local reads in `airkctl` through `airk_core::service_logs` (U86), never over IPC | R | Read |
| M14 [amended 3b.2: the CLI's own inputs] | `RecordClientError { operation, input }` (`input` is the `{code, detail}` JSON text, ≤ 16 KiB; request budget 128 KiB for escaping) | `ClientErrorRecorded` | `service_logs::record_client_error` `raw.rs:122`, unchanged; while the service is down `airkctl` runs the private role `record-error`, which appends through the same writer (U75). No nonce/dedup: the canonical function has none | M | Append |
| M15a [served 3e.2; added] | `AgentEndpoint {}` (`agents endpoint`) | `AgentEndpoint { endpoint }` or `AgentRefused { message }` | `http_commands::endpoint` :139 (the service's own `Health` and `HttpEndpoint`, then `check_endpoint` against the installation binding), split as `endpoint_at(control)` | R | Read |
| M15 [served 3e.2; amended] | `AgentConnectionInfo { connection_id }` (`agents info`) | `AgentConnectionInfo { connection }` (no token) or `AgentRefused { message }` | `http_commands::existing_connection` :156, split as `existing_connection_at(control, id)` | R | Read |
| M16 [served 3e.4; amended] | `AgentConnectionOwnership { connection_id }` (`agents list`; the predecessor-cleanup purpose stays with the setup role) | `AgentConnectionOwnership { result: Value }` (the CLI's reply without `active`) or `AgentRefused { message }` | `http_commands::lookup_connection` :250, split as `lookup_connection_at(control, id)` without the selected-runtime and staged-selection checks | R | Read |
| M17 [amended 3e.5: not added] | — (`service trust inspect\|install\|remove` run locally) | — | `http_commands::trust`: all three actions read only `<control>/http-1` and the OS trust store, so `airkctl` runs them offline (U75, §3.9) with the same receipt engine and OS adapter, moved to `airk_process::cert_trust` and `airk_ipc::private_record` by `scripts/dev/split_u90b_cert_trust.py` (U90). Differences from `airk runtime http-trust`: the namespace is the endpoint descriptor's own (not the registry incarnation), install no longer requires live `Health`, and the active-issuer guard checks the owner-only control root and the saved rotation phase instead of the install layout and the full rotation record | — | — |
| M18 [contract 3e.5: `Status` only; amended] | `HttpIssuerRotation { action: Status }` (`service issuer-rotation status`) | `HttpIssuerRotation { result: Value }` or `HttpIssuerRotationRefused { message }` | `http_endpoint::rotation::output` (moved from `http_rotation.rs` by `scripts/dev/split_u90b_cert_trust.py`), the `--action status` document. Not served: `Prepare` (install lock, managed policy, pending activation, settled lifecycle and selected-runtime checks need the install record, Phase 7) and `Commit` (the service would stop and restart itself; needs the U85 restart path); `TrustNew`, `Abort` and `Finalize` change OS trust and stay local to the CLI (U75) but need `TlsIdentity` and the rotation record, which `airkctl` cannot link yet | R | Read |
| M19 [split-native, U85; lands 3b.1e] | `Lifecycle { action: Stop \| Restart, caller: Incarnation, key: RequestKey }`; the key comes from `RequestEpoch` as every keyed mutation's does | `LifecycleAccepted { record: LifecycleRecord, service_instance, process_id }` | The existing lifecycle ledger (`airk-store` `begin_lifecycle`/`advance_lifecycle`, unchanged format) with `LifecycleRequest { caller, key, action, expected_instance: this instance, runtime: this executable's digest, manager: false }`; the drain path advances Draining → Complete (Stop) or → Starting (Restart); the next admitted start completes a pending record, including one left by a crashed predecessor. `airkctl` waits for `process_id` to exit, then Restart runs `service start`. OS-manager ownership and forced stop: Phase 7 | M, X | Keyed by `(caller, key)`; a replay returns the same record |
| M20 [served 3e-m20; amended, U80] | `AbandonLifecycle { id: Sha256Digest }` | `LifecycleAbandoned { record }` | `lifecycle::abandon` :86's rule on the control-root ledger (`airk-store` `lifecycle_by_id`, `advance_lifecycle`, unchanged format); a record this instance is draining is refused `lifecycle_request_in_progress`, an unknown id `lifecycle_request_unknown` | M | Already terminal → same record [V :91-96] |
| M21 | (U101) `LoginStartup { action: Status \| Install \| Uninstall }` | `LoginStartup { login: LoginStartupStatus }` (Windows; elsewhere `supported: false`) | `commands.rs:288-328`, `install_login_startup` :872, `stop_login` `runtime_install.rs:2634`, `ServiceManager::plan/status/start` | R / M, X | Install/uninstall converge [I] |
| M22 | (U100) `StorageInventory {}` | `StorageInventory { inventory }`: the install record's five locations in order, sizes, reviewed purge rule each (`airk_core::storage_admin`) | `inventory::inventory` :110 | R | Read |
| M23 | (U100) `StoragePurgePlan { kind }` → `StoragePurgePlan { plan, plan_digest }`; `StoragePurge { kind, plan_digest }` → `StoragePurged { receipt }` | Cache contents or closed logs only; the purge applies exactly the current plan the digest names; planned files that stay are listed | `inventory::purge` :267 | M, X | Apply only with the preview digest |
| V1 [served 3e.9] | `AgentConnectionVerify { connection_id }` | `AgentConnectionVerified { result: {schema, connection_id, url, verified, http_status?, protocol_version?, server?, detail?} }` or `AgentRefused { message }` | `http_commands::verify_connection_at`: one MCP `initialize` over HTTPS pinned to the installation's public issuer with the stored credential, then `DELETE` of that session; the token never leaves the service (replaces the extension's HTTPS `verifyMcp`) | R | Read |

The window plane (W1–W8) and the launch code (A1) are C3 (§2.5–2.7).
Existing methods reused unchanged: `Health`, `Shutdown`, `Register`,
`ReserveProject`, `Projects`, `Pause`, `ConfigureWatch`, `Enqueue`,
`ConfigureProject`, `Job`, `Inspection`, `RequestEpoch`, `RequestReceipt`,
`NativeRead`, `CatalogDetail` and the rest of the catalog family, `Events`,
`HttpEndpoint`, `HttpEnroll`, `HttpRevoke`, `Logs` [V `service.rs:123-427`, 78 variants].

### 2.3 How C2 lands variants before handlers

1. One reviewed commit adds every variant of §2.2 and §2.5–2.7 (and its
   responses) to `airk-core::service`; `control_methods.json` pins one request
   and one response byte string per variant plus malformed cases (unknown
   field or variant, uppercase identity, oversize frame).
2. The service dispatch `match request.method` (`service_runtime.rs:198` [V])
   gets one **explicit arm per new variant** returning
   `Failure { code: "not_yet_served" }`. No wildcard arm, so the compiler still
   reports a missing handler and each later handler replaces exactly one arm.
3. The exhaustive console relay filter (`console_client.rs:39-109` [V]) refuses
   every new variant explicitly (`console_method_not_reviewed` [P]).
4. `not_yet_served.json` lists every unserved variant; a test sends each to an
   in-process service. A handler step moves its row to its own fixture.
5. I2 maps `not_yet_served` to exit 5; `airkctl version` omits the capability
   until the handler lands (§3.5).

### 2.4 Request journal, acceptance and receipt recovery (C3)

**Today [V].** The CLI gets `(namespace, epoch)` from `RequestEpoch`
(`client_requests.rs:20-73`), mints `caller` and `nonce`, and persists
`Saved { schema: 1, control, namespace, request }` as
`<control>/client-requests/<nonce>.json` (lock file, ≤ 64 entries of ≤ 64 KiB,
`persist_noclobber` + directory sync, `:213-257`); submit re-reads the exact
bytes (`:328-352`). The registry makes keys idempotent: the same
`(namespace, epoch, nonce, caller)` returns the accepted job, other arguments
under it are `KeyConflict`, a stale namespace/epoch is `KeyExpired`
(`airk-store/src/service/registry/jobs.rs:636-729`); `RequestReceipt` returns
`{ job, expired, removed_project }` (`service.rs:302,750`).

**Contract [P].** The journal file format, path, lock, limits and validation
stay byte-for-byte (it already lives in the service's control directory, so
no `airk-state/1` format changes); only the writer moves into the service
behind M5–M8.

1. The client (`airkctl`, the session, or the admin handler on the browser's
   behalf) obtains `(namespace, epoch)` with `RequestEpoch` and generates
   `caller` and `nonce` with `airk_ipc::fresh_incarnation()`. The browser never
   holds a key: the admin handler generates it server-side (§4.1, "no durable
   requests in the browser").
2. **Acceptance** = M5 has durably journaled the exact request. The service
   records acceptance before any effect; M5 itself has no effect on indexes.
3. **Submission** = M7 by nonce; the registry key rules above apply.
4. **Receipt lookup** = `RequestReceipt { caller, key }` (existing) and M6.

| Case | What the client observes | Outcome | Required client action |
|---|---|---|---|
| Disconnect before acceptance (M5 not durable) | M5 reply missing; M6 lacks the nonce | Nothing accepted; no effect | While alive, resend the **same** M5. `key_expired` then allows a new epoch and key, because nothing was accepted |
| Disconnect after durable acceptance (M5 reply lost) | Resent M5 answers `duplicate:true`, same nonce | Journaled, not submitted | Submit with M7 |
| Disconnect after M7 sent, before reply | `RequestReceipt` → `job: Some` / `None, expired:false` | Accepted / not yet accepted | `None`: resend M7 with the same nonce (key-idempotent). Never mint a new key |
| After completion | `RequestReceipt` → terminal `JobRecord` | Completed | Display; optionally M8 |
| Client death | Journal entry survives in the service | Prepared or accepted | Any surface (CLI, session, admin) lists (M6) and retries/forgets by nonce |
| Service restart | Journal and `service_requests` are durable; registry namespace persists; service instance changes | As before the restart | Reconnect, receipt lookup; a reconnect never replays M5 with a new key |
| Duplicate key, same arguments | M5 `duplicate:true`; `Enqueue` returns the same job [V] | Same request | None |
| Same key, different arguments | M5 `client_request_key_conflict`; `Enqueue`/`ConfigureProject` `KeyConflict` [V wire spelling, `service_management.rs:500` formats `JobError` with `Debug`] | Refused; the original is unchanged | Fail closed; show the original by receipt |
| Receipt pruned / epoch rotated | `RequestReceipt.expired:true`; submit `KeyExpired` [V spelling as above] | **Unknown** | Report unknown; never replay |
| Journal full (64) | M5 `client_request_journal_full` | Refused | Inspect and forget (`airkctl service requests …`) |

Rules fixed by `request_recovery.json` [P]:

- **M5 order.** The identical saved request under the nonce → `duplicate:true`;
  the nonce with other bytes → `client_request_key_conflict`; then a namespace
  or epoch that is not current → `client_request_key_expired`; then capacity →
  `client_request_journal_full`. M5's `caller` and `key` must equal those of
  its inner request, else `client_request_key_mismatch`. The registry's own
  rule is the same shape: an accepted key is found before the epoch check
  (`jobs.rs:638-660`), so an accepted key survives a later rotation.
- **New keys.** A client mints a new key for the same intent only right after
  a definitive M5 refusal that proves nothing was journaled
  (`client_request_key_expired`, `client_request_journal_full`). Every other
  loss (missing reply, death, restart, `registry_response_unknown_retry_with_key`,
  `service_instance_changed`) keeps the key.
- **Recovery by nonce.** `RequestReceipt` needs `caller` and `key`, which only
  the minting client holds; M6 entries stay `{nonce, project, kind}`. Another
  surface (after client death) recovers by resending M7 for the nonce: the
  exact saved key returns the accepted job, accepts a never-submitted one under
  a current epoch, or answers `KeyExpired`.
- **`KeyExpired` on a journaled entry is unknown**, not "never accepted": the
  unchanged journal format records no submission, and a pruned receipt also
  answers `KeyExpired`.
- **Restart.** Requests carry `expected_instance`; a stale one is refused with
  `service_instance_changed` before dispatch [V `service_runtime.rs:143-196`].
  The client re-reads `Health` and recovers by receipt or M6/M7.

UI states (I2, I3, extension): `submitted` (sent, no answer), `accepted`
(journaled or receipt with pending/running job), `completed` (terminal
outcome), `refused` (typed code, nothing changed), `unknown` (deadline after
send or expired receipt). Fixture `fixtures/split/i1/request_recovery.json`
holds each row as a scripted transcript (requests, injected disconnect point,
expected responses and final state) driven against an in-process service with
an injected clock.

### 2.5 Window plane: session identity, scope grant, Assistance, handoffs (C3)

[contract landed 3f.1: `airk_core::service` W1–W8 and M12 variants, `WindowLease` (redacting `Debug`), `WindowScopeSelection`, `AssistanceWindow`, `AssistanceDraft`, `ToolSelection::Window`; every pipe frame of `window_plane.json` is checked by `wire_fixture_tests::window_plane_pipe_frames_are_exact_i1_frames`.]

A window identity is owned by the service, not claimed by the extension. The
lease is a secret held only in `airkctl session` memory; it never appears in a
session frame, on stdout or in logs [P].

| # | Method { request } | Response { fields } | Rules |
|---|---|---|---|
| W1 | `WindowOpen { label (≤128 B), owner_pid }` | `WindowOpened { window_id, lease, ttl_ms, service_instance }` | ≤ 64 windows per service; TTL 30 s, renewed every 10 s (injected clock) |
| W2 | `WindowRenew { window_id, lease }` | `WindowRenewed { ttl_ms }` | Known window, wrong lease → `window_lease_invalid`; unknown id (closed, expired or from another instance) → `window_unknown` [P] |
| W3 | `WindowClose { window_id, lease }` | `WindowClosed` | Drops queue, handoffs and scope grant |
| W4 | `WindowScopeGrant { window_id, lease, selection: { namespace, expected_catalog_revision, index_ids, project_ids } }` [P, replaces `Catalog{ScopedConnectRequest} \| Combined{..}`] | `WindowScopeGranted { scope_identity, catalog_revision, expires_at_ms }` | U77: selects existing indexes only; never creates or changes indexes, index sets or agent grants; carries no catalog `GrantId` (Q6 closed by the ledger row "P0.6": an in-memory grant bound to the lease). Unknown index or stale revision → `scoped_read_unavailable` [V code]. A new grant replaces the old one. Revoked by close, lease expiry or restart, never re-granted automatically. Reads use M3 `selection: Window{..}`; without a grant → `window_scope_not_granted` [P]; no bearer leaves the service |
| W4b | `WindowScopeRelease { window_id, lease }` [P] | `WindowScopeReleased` | Idempotent; session frame `scope_release` |
| W5 | `AssistanceWindows {}` | `AssistanceWindows { windows: [{window_id, label, accepting, queued}] }` | Readable by owner pipe and by `/mcp` Explorer tools with a valid bearer [P] |
| W6 | `AssistanceSubmit { window_id, kind, draft: Value, submitter: ClientFamily }` | `AssistanceQueued { draft_id, queued }` | Per window ≤ 16 drafts and ≤ 1 MiB; draft ≤ 256 KiB; full → `assistance_queue_full` (never drops silently) |
| W7 | `AssistanceDrain { window_id, lease, ack_through: Option<u64>, limit ≤ 8 }` | `AssistanceDrained { drafts: [{sequence, draft_id, kind, draft}], next }` | Only the owning lease drains; ack removes |
| W8 | `HandoffPublish { window_id, lease, handoff: Value, ttl_ms ≤ 600000 }` | `HandoffPublished { handoff_id, expires_at_ms }` | ≤ 32 per window, ≤ 64 KiB each; read by M12 |

All W state is in memory, scoped to `service_instance`; a restart loses it,
`window_unknown` tells agents to list windows again, and sessions re-open and
emit `resync` (§3.7). `window_plane.json` covers open, renew, expiry under a
manual clock, foreign lease, queue full, drain/ack, cross-window drain,
restart and grant revocation.

Rules fixed by `window_plane.json` [P]:

- Lease: 256 random bits as 64 lowercase hex. It appears only on pipe frames
  between the owning session and the service. The session `control` relay
  refuses W1–W4b, W7 and W8 by method (`session_command_refused`) before the
  pipe; W5 is relayed. `owner_pid` is diagnostic only; liveness is renewal.
- A window is valid while `now < last renewal + ttl_ms`; expiry acts as W3.
  Over 64 windows → `window_capacity`; label over 128 bytes → `window_label_invalid`.
- Assistance: sizes are compact JSON bytes; a draft over 256 KiB →
  `assistance_draft_oversize`; count or byte bound → `assistance_queue_full`.
  `accepting` = open and not full. Sequences start at 1 per window and are
  never reused. W7 is non-destructive: it returns the oldest unacknowledged
  drafts (≤ `limit`, else `assistance_drain_limit_invalid`) and `next` = the
  first sequence not returned, or null. `ack_through` removes and is
  idempotent; beyond the highest assigned sequence → `assistance_ack_ahead`.
  At most `limit` drafts are unacknowledged in delivery, so acknowledgement is
  the back-pressure. The session suppresses repeats by `sequence`; after an
  overflow `resync` it re-drains from the last acknowledgement, so no draft is lost.
- Handoffs: `ttl_ms` 1..=600000 (else `handoff_ttl_invalid`), over 64 KiB →
  `handoff_oversize`, over 32 → `handoff_capacity`. M12 reads do not consume;
  at `expires_at_ms`, after close or restart → `handoff_unknown`. The editor
  publishes with a session frame `handoff_publish { handoff, ttl_ms }` →
  `handoff { handoff_id, expires_at_ms }`; it and `scope_release` →
  `scope_released` are §3.7 rows to add with the I2 window step.

**W4 served (amendment, selection authority step S1b,
`docs/split/selection-authority-spec.md`; U99).** The service answers W4 and
reads under it. The catalog check runs with the windows lock released; the
window and its grant generation are re-checked before the grant is stored
(window closed meanwhile → `window_unknown`; a concurrent grant or release
won → `scoped_read_unavailable`). Each stored grant draws a fresh in-memory
nonce registered live with the scoped-read admission gate; close, release,
replace, lease expiry (swept on every window-plane method and on every M3
`selection: Window`) and service end revoke it after the lock is released.
M3 `selection: Window{window_id, lease}` sweeps, resolves the grant, refuses a
selection without a project (`scoped_read_unavailable`), runs the full
catalog check again with the lock released (U99 Q3) and requires the stored
scope identity and catalog revision exactly (any catalog change →
`scoped_read_unavailable`; the grant is never refreshed by the service). The
read connects in process (no `ControlMethod` carries the selection
authority), runs the same handlers as a catalog-grant read and answers the
same `ToolResult`; only `scope_observation` names the selection instead of a
reviewed grant. A successful read renews the window (U99 Q5). Refusals are
`ControlResponse::Failure` with the existing codes; failure bodies echo only
the selection and its scope identity, never the nonce, the lease or the
service instance. A selection of more than one index reads through the
combined fan-out (spec §3.6) that `selection: Combined` uses: one in-process
selection connection per selected index (each admitted at exactly the
grant's revision), the same budgets, member reports, continuations and
retry data, and a final delivery receipt issued in process under the gate's
issue ticket with the nonce still live. Its selection observation carries
`selection` instead of `grant_id`/`grant_revision`, its query-scope pages
carry `"mode": "selected_scope"` and `"grant": null`, and its combined
authority identity uses the domain `airk/combined-selection-authority/1`.
An index of the selection without a non-empty membership of a selected
project refuses the whole read (`scoped_read_unavailable`).

### 2.6 Event subscription basis (C3)

No new I1 method: sessions and the admin stream poll the existing `Events`
(`EventPage { retained_prefix_lost, next, tail }`, errors
`event_cursor_expired`/`event_scope_changed` [V `service_events.rs`]) and
`Health`; buffering and resynchronization are I2 (§3.7) and I3 (§4.8) rules.

### 2.7 Admin launch code (C3)

| # | Method | Response | Rules [P] |
|---|---|---|---|
| A1 | `AdminLaunchCode {}` | `AdminLaunchCode { code, port, expires_in_ms }` | Owner pipe only; never relayed by the session, never over HTTP. 256-bit random code (base64url); the service stores only its SHA-256; TTL 60 s (injected clock); ≤ 8 outstanding, oldest evicted; consumed atomically by §4.5; `Debug` redacts it; never logged |

Rules fixed by `admin_launch_code.json` [P]: the code is base64url without
padding (43 characters); the stored digest is SHA-256 of those 43 ASCII bytes,
so a non-canonical spelling of the same bits is a different, unknown code.
Valid while `now < issued + 60000`. Unknown, expired, used, evicted, padded,
short or empty codes all get `admin_launch_code_invalid`. A1 takes no fields
(an unknown field is refused before dispatch). Neither the code nor its digest
appears in logs, `Debug` output or errors; the code appears only in the A1
reply, in the one-shot `airkctl admin url|open` URL fragment and in the §4.5
exchange body.

**A2 (amendment, Phase 5b; §4.6, U77).** `AdminScopeGrant { selection:
WindowScopeSelection }` → `AdminScopeGranted { scope_identity,
catalog_revision, expires_at_ms }` [contract landed: `airk_core::service`,
group `admin_scope_grant` in `fixtures/split/i1/control_methods.json`; M3
case `admin_selection`]. The W4 selection without a lease: served only by
the service's admin plane, which checks it with the catalog check W4 shares
(`service_runtime::scope_grant`: existing, non-retired indexes; the current
catalog revision exactly, an older one is stale; with projects selected,
every index a member of one; refusal `scoped_read_unavailable`) and binds the
grant in memory to the calling session and service instance. `expires_at_ms`
is the session's current expiry; logout, expiry, eviction and restart drop
the grant, and a failed check leaves the previous one. The pipe has no admin
session: `AdminScopeGrant` → `admin_session_required`; the metadata relay
refuses it (`console_method_not_reviewed`) and `airkctl session`'s `control`
relay refuses it by method. Reads under a W4 grant are served (§2.5
amendment, step S1b). Each stored A2 grant now carries an in-memory
selection authority (step S2a): a fresh nonce registered live with the
scoped-read admission gate inside the admin plane's lock, revoked after that
lock is released on replacement, logout, expiry, eviction, a store whose
session ended, and service end. Reads under an A2 grant are served (step
S2b): the admin plane's injected `SelectionReader` runs the same
`selection_read` path W4 uses, in process, with the session's selection ref.

### 2.8 Per-method byte budgets [P]

| Method | Request | Response | Reason |
|---|---|---|---|
| default | 64 KiB | 64 KiB | `MAX_FRAME_BYTES` [V] |
| M3 `ToolsCall` [amended 3a.2a] | 128 KiB (arguments ≤ 64 KiB as the CLI bounded them, plus selection and re-encoding room) | 4 MiB | The control frame reader refuses budgets above 4 MiB [V `airk-ipc` `endpoint.rs`], the public tool envelope is bounded to 1 MiB (`PUBLIC_TOOL_ENVELOPE_BYTES` [V `remote.rs:25`]) and the HTTPS canonical response limit is configurable to at most 4 MiB [V `remote/http.rs:61`]; the earlier 16 MiB (`MAX_CODEC_BYTES`) belongs to the session codec, not this transport. A larger result fails the frame write and is never truncated |
| N1 | 64 KiB | 4 MiB (`MAX_USAGE_DOCUMENT_RESPONSE_BYTES`, 3c.5c-1b): the `airk-ipc` frame ceiling | The CLI reassembled ≤ 8 MiB of records; a document above 4 MiB encoded is refused (`invalid native frame length`). Recorded gap; raising the ceiling is a user decision |
| M2 | 64 KiB | 1 MiB | Catalog with schemas |
| M11 | 1 MiB + 64 KiB | 1 MiB | `proposal-status` reads ≤ 1 MiB today [V commands.md E] |
| M13 | — | — | Not added (U88): recent pages use `Logs`, full and raw exports stay local |
| W6 | 320 KiB | 64 KiB | Draft ≤ 256 KiB |
| W7 | 64 KiB | 4 MiB (the frame ceiling; 3f.1) | At most eight 256 KiB drafts per drain |
| W8, M12 | 128 KiB / 64 KiB (W8 request), 64 KiB / 128 KiB (M12 response) (3f.1) | A 64 KiB handoff and its fields |

## 3. I2 — `airkctl` command contract

### 3.1 Inventory row

| Inventory | |
|---|---|
| Owner | `crates/airkctl`, module `src/contract/` (arguments, output, exit classes, version, session messages, capability catalog). C4 creates the crate with this module and a binary that serves only `--version` and `version` and answers every other command with exit 5 [P] |
| Consumers | Extension `src/airkctl/` facade (generated types), scripts, users, CI harnesses |
| Surfaces | Process arguments, stdout/stderr, exit status; `airkctl session` frames on stdin/stdout |
| Authorization | Runs as the invoking user; reaches the service only over I1 plus the §3.10 exceptions; the session refuses token-returning and pipe-only commands (§3.8) |
| Version | `i2_contract` integer, starting at 1; session `protocol` 2 (v1 stays with `airk service session` until it retires) |
| Unknown fields | Version document: consumers ignore unknown fields (forward compatibility of the handshake). Session frames, error lines and every other I2 record: `deny_unknown_fields` |
| Limits | Session: request frame 192 KiB, stdin 64 KiB, 64 args × 32 KiB, response 16 MiB, stderr tail 64 KiB, 8 in flight [V `session_protocol.rs:21-39`]; invoke deadline ≤ 120 s [P] (the 600 s installer exception ends, plan §6.5); event buffer 256 events / 1 MiB, separate from reply budgets [P] |
| Cancellation | Session `cancel {target}`; owner loss cancels read-only work and exits; accepted mutations continue under service ownership |
| Retry / recovery | Reads: native read-retry rules (C8 cases). Mutations: §2.4 receipts; the session never replays |
| Fixtures | `fixtures/split/i2/version.json`, `compatibility_matrix.json`, `exit_codes.json`, `error_lines.json`, `offline_operations.json`, `session_sequences.json`; `fixtures/wire/airkctl_session_frames.json`; `fixtures/cli-golden/` cases for every surviving command |
| Generation | U47 pilot: `schemars` JSON Schema → TypeScript, checked-in output, drift check (§3.11) |
| Lands in | C4 |

### 3.2 Argument conventions [P]

- Surviving commands keep their current names, arguments and output (plan §6.2);
  renames are those in commands.md (`runtime http-*` → `agents …`,
  `runtime …` → `service …`). New: `version`, `admin open|url`,
  `service start|stop|restart|status`, `agents launch|verify`, `service rpc`
  kept (P0 decisions).
- Global `--output <json|text>`. With `json`, stdout carries exactly one JSON
  document or the command's existing JSON stream. **Conflict:** ten
  commands already use `--output <FILE>` (nine `explorer` commands,
  `service logs` [V `docs/cli-reference.md:763-1180,6564`]); see Q1.
- Installation is found only through the I4 pointer. `--runtime-dir` and
  `--service-install-dir` stay as explicit user-supplied flags (P0 decision);
  the extension never passes them.
- Commands needing a running service start it automatically (§3.10 exception 2)
  unless a saved explicit Stop exists; then they exit 3 with guidance.

### 3.3 Output envelope [P]

Goldens require today's bytes, so I2 does not wrap native results:

| Stream | Content |
|---|---|
| stdout | The complete native result exactly as today (for a failed native read too, as `report_failure` prints it [V `main.rs:1263`]). With `--output json` on a text-default command: the structured value the text was rendered from. Never reduced |
| stderr, last line, `--output json` | `{"airkctl_error":1,"class":…,"code":…,"retryable":…,"retry_after_ms"?:…,"message":…,"native"?:{complete native failure or response}}`, message ≤ 4,096 chars; extends today's `AIRK_ERROR_FORMAT=json` line (`{"airk_error":1,"code","retryable","message"}` [V `main.rs:1256`]) under a new key |
| exit status | §3.4 |
| session `invoked` | `{ exit_code, class, stdout, stderr, error? }` — the same three channels in one frame |

### 3.4 Exit codes [P]

| Code | Class | Meaning | Mutation possible? |
|---|---|---|---|
| 0 | `success` | Completed | Yes, as requested |
| 1 | `refused` | The service answered a typed refusal (`Failure{code}`), or a native read failed with its complete result on stdout (today's 1 [V `main.rs:1247`]) | No |
| 2 | `usage` | Argument error (clap's existing 2) | No |
| 3 | `service_missing` | Not installed, not running and not startable, or saved explicit Stop | No |
| 4 | `incompatible_pair` | `airk`/`airkctl` version, protocol or record digest mismatch, or an incomplete installation | No (checked before any mutation) |
| 5 | `unsupported_capability` | `not_yet_served`, or an I2 contract/capability the caller requires is absent | No |
| 6 | `outcome_unknown` | Deadline or disconnect after a mutation was sent; recover by receipt | Maybe |
| 7 | `installation_unverified` | Pointer/record ownership, link or integrity check failed | No |
| 8 | `internal` | Bug or I/O failure inside `airkctl` | Unknown → treated as 6 by callers that mutate |

`fixtures/split/i2/exit_codes.json` maps native responses and failure points
to codes; C8's `native_outcomes.json` cases also drive it.

### 3.5 `airkctl version --output json` [P]

Local only: reads the I4 pointer and record, never starts or contacts the
service, so it works when the service is down.

```json
{"schema": "airkctl-version/1", "product": "AIRK Service", "version": "1.9.0", "channel": "stable",
 "i2_contract": 1, "i2_contracts_supported": [1], "i1_protocol": 1,
 "capabilities": ["session/2", "events/1", "journal/1", "tools/1", "explorer/1", "window-scope/1",
   "assistance/1", "handoff/1", "agents/1", "agents-launch/1", "agents-verify/1", "admin-launch/1",
   "offline-recovery/1"],
 "executable_sha256": "<64 hex>",
 "installation": {"state": "complete", "airk_version": "1.9.0", "pair": "exact", "install_id": "<32 hex>"}}
```

`airkctl --version` prints `airkctl <version>` (build receipt check
[V build.md A.1]). Capabilities are the reviewed list of served operations of
this exact pair; an unserved handler's capability is absent.

### 3.6 Compatibility matrix (`fixtures/split/i2/compatibility_matrix.json`)

The extension decides from contract version and capabilities, never from
product version ordering (plan §4.2). Every row is a reviewed expectation.

| # | Extension supports / requires | `airkctl` reports | Pair | Expected |
|---|---|---|---|---|
| 1 | I2 {1}; requires `session/2`, `tools/1` | 1.9.1, I2 1, superset of caps | exact | Compatible (product versions differ) |
| 2 | I2 {1} | I2 2 only | exact | Incompatible: unsupported contract version; both versions shown; no session started |
| 3 | I2 {1, 2} | I2 1 | exact | Compatible on 1 |
| 4 | Requires `window-scope/1` | caps lack it | exact | Incompatible: missing required capability |
| 5 | Optional `assistance/1` | caps lack it | exact | Compatible; Assistance disabled with reason |
| 6 | — | unknown capability strings | exact | Ignored |
| 7 | — | I2 1; record names `airk` 1.9.0, `airkctl` 1.9.1 | mismatch | Every service command exits 4 before any I1 mutation; the test asserts the fake service received no mutating method and no journal entry |
| 8 | — | `Health.executable_sha256` ≠ record | mismatch | Exit 4 before mutation |
| 9 | — | malformed or oversized version document | — | `installation_unverified` state; do not execute further |

### 3.7 Session v2 framing and events [P]

Framing stays: 4-byte big-endian length, then JSON, one codec
(`service_ipc` → `airk-ipc` [V `service_ipc.rs:30-71`]). Replies keep the v1
shape `{id, outcome}`; events are `{event: {...}}` (disjoint keys).

| Request | Reply | Notes |
|---|---|---|
| `hello { protocol: 2, client: {name, version}, i2_contracts: [..] }` | `hello { protocol, version, i2_contract, capabilities, executable_sha256, service_instance?, window_id? }` | First request; anything else first → `session_protocol_mismatch` [V v1 rule] |
| `invoke { argv, stdin?, deadline_ms }` | `invoked { exit_code, class, stdout, stderr, error? }` | Allowlist per §3.8 |
| `control { method }` | `control { response }` | Metadata relay; no `runtime_dir` (pointer); `Connect`/`ConnectScoped` refused `console_brokerage_forbidden` [V `console_client.rs:41`] |
| `cancel { target }` | `cancelled` | v1 semantics [V `session.rs:13-22`] |
| `subscribe { topics: [status, jobs, assistance], cursor? }` / `unsubscribe` | `subscribed { cursor }` | Cursor `{service_instance, window_id, native: EventCursor}`; another instance/window → `event_cursor_scope_mismatch` |
| `scope_select { selection: {namespace, expected_catalog_revision, index_ids, project_ids} }` | `scope { scope_identity, catalog_revision, expires_at_ms }` | W4 (U77); no catalog grant id |
| `scope_release {}` | `scope_released` | W4b; idempotent |
| `assistance_ack { through }` | `acknowledged` | W7 |
| `handoff_publish { handoff, ttl_ms }` | `handoff { handoff_id, expires_at_ms }` | W8 |
| Any window-plane frame refused by the service | `refused { code, retryable }` | For example `window_scope_not_granted`, `scoped_read_unavailable`, `assistance_ack_ahead`, `handoff_oversize`; nothing changed |

Event kinds: `status`, `job`, `assistance_draft`, `resync{reason: overflow | native_gap | service_restarted}`, `closing`. Overflow drops the buffer
and emits one `resync`, never a gap disguised as history. A service restart
(new `Health.instance`) re-opens the window (new `window_id`), drops the
Assistance queue and scope grant, fails in-flight reads with
`service_restarted` (retryable), and leaves accepted mutations to receipts.
Deadline and owner-loss tests use the injected `Timing`/clock seam
[V `session.rs:51-66`] and observed events, no sleeps.

Window-scope reads (amendment, selection authority S1b; U99 Q4). While a W4
grant is held, an `invoke` whose read names no explicit selection calls M3
with `selection: Window{..}` fenced to the session's instance; the lease
reaches the in-process command only through the `cli::Io` window seam, never
an argument or output. When that read is refused `scoped_read_unavailable`
and the catalog (read with `CatalogDetail` of the first selected index) is
at another revision in the same namespace, the session re-selects once: the
same indexes and projects at the current revision (W4). If that is granted it
retries the read once and returns the retry's outcome unchanged, refused or
not; otherwise it returns the first outcome. It never re-selects twice for
one read.

### 3.8 Session admission and the token rule

- The session never returns bearer tokens, connection grants, launch codes or
  window leases. `agents credential`, `admin url` and `service uninstall` are
  refused with `session_command_refused` [P]; they are one-shots only.
- `Connect`/`ConnectScoped`, upgrade checkpoints, `HttpEnroll` and A1 are
  refused on the `control` relay.
- Fixture `airkctl_session_frames.json` asserts these refusals and that no
  frame in any sequence contains a 64-hex token field.

### 3.9 Offline recovery operations (U75)

Run by `airkctl` while the service is down. None opens indexes, registries or
stores; each first verifies the pointer and record (§5).

| Operation | Command [P] | Owner-private inputs | Effect | Today [V] |
|---|---|---|---|---|
| Record an error | `service record-error` | Raw job-error ring under the logs location | Bounded append (M14 when running) | `service_logs/raw.rs:122` |
| Abandon a stuck lifecycle record | `service abandon-lifecycle` | Lifecycle record — **in the registry today**, see Q2 | Mark abandoned | `lifecycle.rs:86` opens `ServiceRegistry` |
| Reset login startup | `service reset-startup` | `startup-<sha>.json` crash-circuit file | Delete | `commands.rs:783` |
| Recover from an expired issuer | `service issuer-rotation recover` | `http-1/` identity, rotation lock; refuses if the service answers `Health` | Quiescent rotation | `http_rotation.rs:87` [V 3e.5: it does read the registry (`saved_endpoint` opens `ServiceRegistry::open_saved` for the namespace), takes the install lock and `quiescent(install)` exclusion, and stages bundles with `TlsIdentity`; not served yet] |
| Read login status | `service login status` | OS login manager, login receipt | Read | `service_manager.rs:459` |
| Add/remove AIRK certificate trust [3e.5] | `service trust inspect\|install\|remove` | Endpoint descriptor, public issuer certificate, trust ownership receipt and its lock, issuer-rotation lock and record | OS trust store with its consent dialog | `airk_process::cert_trust` (moved from `http_endpoint/trust.rs`, U90) |

The crate that owns these OS adapters under U27's edge limit is Q3.

### 3.10 Uses that are not pipe requests (plan §6.3, five exceptions)

| # | Exception | Inputs | Contract rule [P] |
|---|---|---|---|
| 1 | Read the install pointer and record | §5 files | Owner, link and size checks before parsing |
| 2 | Start `airk` detached | Record's `airk` path and digest | Verified image; owner-private start request (§5.6); explicit vs automatic mode decided by the service, so `airkctl` never reads the registry's Stop flag |
| 3 | Credential helper reads the token file | Owner-private token file | Never starts the service or enrolls; `agents launch` reuses this path to place the credential in the client's environment (P0 decision) |
| 4 | Read logs while the service is down | Log location from the record | Bounded reads |
| 5 | Delegate `service uninstall` | Setup operation file (§5.5) | One-shot only; hand off, then exit |

### 3.11 Generation pilot (U47) and drift check [P]

- `schemars` derives on `src/contract/` types with hand-written format
  annotations for identities and digests (rules in `try_from` newtypes are
  invisible to generators [V pilot]).
- `cargo test -p airkctl --lib contract_schema` compares the rendered schema
  with `packages/vscode/src/generated/i2.schema.json` and, on mismatch, writes
  the observation to a temporary file and fails (never rewrites);
  `python scripts/dev/contract_types.py i2 --check|--write` renders `i2.ts`.
- Pilot scope: version document, error line, exit classes, session v2 frames;
  widened only if no false types appear (u64, `Option` presence, pilot W2/W4).
  Semantic rules stay hand-written and fixture-tested.

## 4. I3 — Admin HTTP contract

**This section is the written input to the independent security review
(U38, amended by U55). No admin code is written before that review; a high or
critical finding stops for the user.**

### 4.1 Inventory row

| Inventory | |
|---|---|
| Owner | Service admin module beside `http_gateway*` in `airk-cli` (sessions, anti-forgery, embedded assets, admin dispatch, operation table); shared result types keep their native owners |
| Consumers | `packages/admin` browser adapter (generated I3 types), `packages/ui` through that adapter |
| Surfaces | `https://127.0.0.1:<port>/admin/` and `/admin/api/v1/` on the existing listener; served even without a catalog gate (P0 decision; Genesis on first start) |
| Authorization | Launch code → session cookie → anti-forgery token, plus Host/Origin/Fetch-Metadata rules (§4.5–4.7); per-operation rule in the table |
| Version | Path version `v1`; envelope `schema` field; breaking change → `v2` |
| Unknown fields | Request bodies `deny_unknown_fields`; responses carry complete native results |
| Limits | Request body ≤ 64 KiB (search ≤ 64 KiB); response ≤ 32 MiB (`OUTPUT_BYTES` [V `http_gateway.rs:67`]); existing connection caps 128/handshakes 32 [V :64-65]; streams §4.8 |
| Cancellation | Client abort cancels delivery only; accepted work continues and is recovered by receipt |
| Retry / recovery | Mutations use the service journal with a server-generated key (§2.4); the page shows submitted/accepted/completed/refused/unknown |
| Fixtures | `fixtures/split/i3/admin_operations.json`, `admission_cases.json` (+ independent generator `admission_cases.py`), `envelopes.json`, `headers.json`, `launch_exchange.json` |
| Generation | U47 pilot: `cargo test -p airk-cli --lib admin_contract_schema` → `packages/contracts-gen/i3.schema.json`; `python scripts/dev/contract_types.py i3 --check\|--write` → `i3.ts` |
| Lands in | C5 (specification and fixtures); handlers in Phase 5 |

### 4.2 Routes [P]

| Route | Method | Session | Purpose |
|---|---|---|---|
| `/admin` | GET | no | 308 to `/admin/` |
| `/admin/` | GET | no | Static shell; without a session it shows only "Open with `airkctl admin open`" and makes no data call |
| `/admin/launch` | GET | no | Static entry page for the code exchange (§4.5) |
| `/admin/assets/<name>` | GET | no | Embedded files looked up by exact name in the sealed inventory; no filesystem path, so no traversal |
| `/admin/api/v1/session` | POST | no (code) | Exchange code → cookie + anti-forgery token |
| `/admin/api/v1/session/logout` | POST | yes | End session |
| `/admin/api/v1/call/<operation>` | POST | yes | Every read and mutation in the operation table |
| `/admin/api/v1/events` | POST | yes | Bounded streamed NDJSON (fetch stream, so the anti-forgery header can be sent) |

Any other path under `/admin` → 404. Path rules: ASCII only; refuse `..`,
`%2e`, `%2f`, `%5c`, backslash, `//`, trailing dot segments, query on API
routes.

### 4.3 Operation table schema

One checked-in Rust table in the admin module, compared with the reviewed
`fixtures/split/i3/admin_operations.json` [P]:

| Column | Meaning |
|---|---|
| `operation` | Stable id, for example `service.stop`, `search.call`, `storage.purge` |
| `method` | The `ControlMethod` variant or canonical read handler it calls (one handler; no browser copy) |
| `plane` | `management` \| `evidence` (evidence never queues behind the metadata actor, plan §4.5) |
| `surfaces` | Subset of `pipe`, `airkctl`, `session`, `admin` |
| `authorization` | `owner_pipe` \| `window_lease` \| `admin_session_csrf` |
| `effect` | `read` \| `mutation` \| `destructive` (destructive = native draft → preview → apply-with-digest + typed confirmation) |
| `idempotency` | `read` \| `request_key` \| `operation_digest` \| `natural` |
| `conformance` | Applicable simplification-plan §4.6 properties P1–P9 |
| `never_over_http` | true → must not appear under `/admin` |

Tests: every `ControlMethod` variant appears exactly once (exhaustive match,
not a registry walk); every `admin` row has a handler and a conformance entry;
no `never_over_http` row has a route.

#### 4.3a Amendments (Phase 5b, admin mutations and search)

- **Rows whose `method` names no `ControlMethod`** are served through one
  reviewed mapping in the admin module (`admin_plane::operations::MAPPINGS`,
  tested against a hand-written copy): `HttpConnectionInfo` →
  `AgentConnectionInfo` (M15); `HttpConnectionOwnership` →
  `AgentConnectionOwnership` (M16); `LogsExport` → `Logs`;
  `HttpTrust.Inspect` → the read-only trust inspection `airk runtime
  http-trust --action inspect` ran, through the same callers, in the service
  (body exactly `{"http_trust":{"action":"inspect"}}`; failure 409
  `operation_failed` with `native: {message}`); `StorageInventory`,
  `StoragePurge` and `LoginStartup.*` → `not_yet_served` (Phase 7: no service
  handler; login startup is offline in `airkctl`, U75). Every other admin
  row's `method` is a `ControlMethod` variant (checked by the same test).
- **Keys.** For `Enqueue`, `ConfigureProject`, `Retry`, `PlanCreate`,
  `CatalogPolicyResume` and `Lifecycle` the admin handler reads
  `RequestEpoch` and generates `caller` and `nonce`; a body naming `caller`
  or `key` is 400 `admin_malformed_request`. `Enqueue` and
  `ConfigureProject` go through M5 → M7 → M8 as `airkctl` sends them; the
  other keyed methods carry their key directly. A lost reply is resent once
  with the same key or nonce; a second loss is 503
  `service_request_outcome_unknown` (`retryable: false`).
- **`request` in the success envelope** is present on every successful
  mutation: the key's nonce for keyed methods, the named journal nonce for
  `requests.submit`/`requests.forget`, and otherwise a server-generated nonce
  that names the admin request only (no journal entry). `state` is
  `accepted` for a job without an outcome or `LifecycleAccepted`, else
  `completed`; a refusal carries no `request`.
- **Lifecycle (U85).** `service.stop`/`service.restart` answer the complete
  `LifecycleAccepted` record; the service then exits, ending every admin
  session. A restart is completed by the client that starts the service; the
  result carries no added note.
- **Destructive rows** answered `not_yet_served` until U98 (§4.3b) defined
  the typed confirmation.
- **Search** (`search.call`) accepts only the `unselected` (help) and `admin`
  selections from the browser; any other is 400.

#### 4.3b Amendment (U98, 12 November 2026): request identities and typed confirmation

Two optional columns, both in the reviewed fixture:

| Column | Meaning |
|---|---|
| `request_key` | `{mode, caller?, key}`. Paths are dotted field names inside the method's own object, 1–4 lowercase segments. |
| `confirmation` | Present exactly on `destructive` rows: `catalog_target`, `split_original_index`, `plan_id` or `storage_target`. |

- **`generate`** (keyed mutations and the draft/preview reads:
  `catalog.preview`, `catalog.index_setup_draft`, `catalog.split_draft`,
  `catalog.grant_draft`, `catalog.source_draft`, `catalog.split_recover`).
  - A body that names a field at a rule path is 400
    `admin_malformed_request`.
  - The service reads `RequestEpoch`, fills a fresh caller (where the row
    names one) and key, and keeps them in the admin session under a new
    review token. A session holds at most `MAX_REVIEWS_PER_SESSION` (32)
    tokens; the oldest goes first.
  - In the native result, every object whose `key` equals the generated key
    loses that `key` and the `caller` beside it. The service keeps that
    caller under the same token. These request identities are the only
    fields removed: they are transport identities in the sense of the parity
    rule.
  - The success envelope carries `review: <token>`.
- **`review`** (`catalog.apply`, `catalog.index_setup_apply`,
  `catalog.split_apply` and the receipts).
  - The body is a `ReviewedCall` `{method, review, confirm_text?}`. The method
    omits the rule paths.
  - The service fills the identity the token holds. An unknown token, or one
    belonging to another session, is 400 `admin_review_unknown`: run the
    review again.
  - Tokens are not consumed. A repeated apply carries the same key, and the
    native key idempotency answers it; receipts stay readable for the
    session's life. Logout, expiry and restart drop them.
- **Typed confirmation.**
  - A destructive row needs `confirm_text` of at most 256 bytes, except a
    `catalog.apply` whose operation creates, renames or adds.
  - The service reads the target's current display name and compares it
    exactly; a mismatch is 400 `admin_confirmation_mismatch` and nothing is
    sent. Targets:
    - `catalog_target`: the index or index set (`CatalogDetail` at the
      request's expected revision) or the grant id;
    - `split_original_index`: the original index's name;
    - `plan_id`: the first 12 hexadecimal characters of the plan id;
    - `storage_target`: the location path as the inventory lists it.
  - The native digest check still decides whether the reviewed draft is
    current.
  - `airkctl` takes the same text as `--confirm <name>` and applies the same
    comparison before it sends.
- **New rows:**
  - `catalog.source_draft`, `catalog.source_observation`,
    `catalog.pending_source`, `catalog.source_resolve`;
  - `catalog.grant_draft`;
  - `catalog.split_source_state`, `catalog.split_receipt`,
    `catalog.split_recover`, `catalog.split_recovery_receipt`;
  - `catalog.policy_resume_receipt`;
  - `usage.document`.

### 4.4 Envelopes [P]

- Success: `{"schema":"airk-admin/1","ok":true,"operation":…,"result":<complete native result>,"request"?:{nonce,state},"review"?:<token>}` (`review` per §4.3b).
- Failure: `{"schema":"airk-admin/1","ok":false,"error":{"code","class","retryable","retry_after_ms"?,"native"?:<complete native response>}}`.
- HTTP status: 200 ok; 409 native refusal; 400 malformed; 401 no or expired
  session; 403 host/origin/fetch-site/anti-forgery; 404 unknown route or
  operation; 413 too large; 503 draining or `not_yet_served`; 500 internal.
  The body is always the envelope (no HTML error pages).

### 4.5 Launch-code exchange [P, for review]

1. `airkctl admin open|url` calls A1 over the owner pipe and builds
   `https://127.0.0.1:<port>/admin/launch#c=<code>`. The code is in the
   **fragment**, so it never enters a request line, `Referer` or server log.
   Alternative for the review: `?code=` with an immediate 303 (simpler, but the
   code reaches the request line and history until replaced).
   Since U97 the fragment may continue `&route=<encoded page route>`, a page
   id and optional prefill query (`index-library?root=<folder>`,
   `?index=<idx_…>`, `?set=<prj_…>`), built by `airkctl admin open|url --page
   … [--for-folder …|--show-index …|--show-set …]`. A route only selects a
   page and prefills fields; it never starts an action.
2. The entry page (static; no session; `script-src 'self'`) reads
   `location.hash`, calls `history.replaceState` to drop the code (keeping
   only a validated page route, `/admin/#<route>`), and POSTs
   `{"code":…}` to `/admin/api/v1/session` with `credentials: same-origin`.
3. The server requires exact Host, exact `Origin`, `Sec-Fetch-Site: same-origin`,
   `Content-Type: application/json`, body ≤ 1 KiB. It removes the code's hash
   with one atomic compare-and-remove. Unknown, expired and already used codes
   get the same 401 `admin_launch_code_invalid` (no oracle).
4. Success: create a session, `Set-Cookie`, reply `{csrf, idle_expires_in_ms,
   absolute_expires_in_ms, service_instance}`; the page navigates to `/admin/`.
5. Replay → 401. A restart invalidates all codes and sessions (memory only).
6. First navigation without a session loads only static files; every API call
   returns 401. The entry page being allowed never allows a management call.
7. The extension's **Open AIRK Admin** runs `airkctl admin url` as a one-shot
   and opens it with the editor's adapter; the code passes through extension
   memory once (single use, 60 s). The review decides whether that is
   acceptable or `airkctl admin open` must open the browser itself.

### 4.6 Session, anti-forgery and admin read grant [P]

- Cookie `__Host-airk_admin=<256-bit random>; Secure; HttpOnly; SameSite=Strict; Path=/`,
  no `Domain`, no `Max-Age`. Idle expiry 30 min, absolute 12 h, ≤ 16 sessions
  (oldest evicted), bound to the service instance, in memory.
- Anti-forgery token: returned in the session-exchange body, kept in page
  memory, sent as `X-AIRK-CSRF` on every API POST, compared in constant time
  with the session's value.
- Planes stay separate: any `Authorization` header on `/admin` → 401
  `admin_bearer_refused`; an admin cookie on `/mcp` or `/api/v1` →
  `cookie_on_bearer_plane`; their existing `Origin` refusal and bearer rules
  stay byte-for-byte [V `http_gateway.rs:358-394`].
- Admin read grant: the browser's index selection becomes a W4-style grant
  bound to the session id and instance; never sent to the browser; revoked at
  logout, expiry or restart. Q6 applies. Landed as A2 (§2.7 amendment):
  the grant is checked and stored, and reads under it are served through the
  in-memory selection admission (S2).
- Review item: cookies are not port-scoped, so any other HTTPS server on
  `127.0.0.1` the browser visits receives this cookie. The review decides
  whether the anti-forgery token requirement on every API call is sufficient.

### 4.7 Host, Origin and Fetch-Metadata rules [P]

| Request class | Host | `Origin` | `Sec-Fetch-Site` | Cookie | CSRF | Header missing |
|---|---|---|---|---|---|---|
| Static GET (`/admin/`, `/admin/launch`, assets) | exact `127.0.0.1:<port>` | absent or exact | `none` (top-level navigation launched by the OS opener) or `same-origin`; `same-site`/`cross-site` → 403 | not required | no | `Sec-Fetch-Site` missing → allowed (static, no data) |
| Session exchange POST | exact | required, exact | `same-origin` | — | — (code is the proof) | 403 |
| API and events POST | exact | required, exact | `same-origin` | required | required | 403 (fail closed) |

No CORS headers are ever sent; preflights get 403. `Sec-Fetch-Mode`/`Dest`
are recorded in fixtures but not required (`navigate`/`document` for pages,
`cors`/`empty` for API) [P].

### 4.8 Response headers and the event stream [P]

| Header | Value |
|---|---|
| `Content-Security-Policy` | `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'` |
| `X-Content-Type-Options` | `nosniff` |
| `Referrer-Policy` | `no-referrer` |
| `Cache-Control` | `no-store` on API, launch page and shell; assets immutable by digest name |
| `Cross-Origin-Opener-Policy` / `Cross-Origin-Resource-Policy` | `same-origin` / `same-origin` |
| `Permissions-Policy` | All features disabled |

Indexed evidence is rendered as text data, never as HTML (U25 review list).
Events: ≤ 2 streams per session, ≤ 16 total; buffer 256 events / 1 MiB; the
server closes each stream after 120 s (existing `STREAM_SECONDS` [V
`http_gateway.rs:69`]); reconnect with the last cursor; `resync` semantics as
§3.7, cursor scoped to the service instance and session.

### 4.9 Never over HTTP

Bearer tokens and credential files; TLS and issuer private keys;
`ConnectionGrant` (`Connect`, `ConnectScoped`); upgrade checkpoints
(`PrepareUpgradeCheckpoint`, `ObserveUpgradeCheckpoint`, `UpgradeRecoveryProbe`,
`ReleaseUpgradeAdmission`); `HttpEnroll` (credential-creating; `airkctl agents
connect` only, the Agents page shows the command); A1; W1–W3, W7–W8 leases;
uninstall and the setup role; certificate trust install/remove (OS dialog);
`service rpc` raw relay.

### 4.10 Items the security review must close

Launch-code placement (fragment vs query) and extension pass-through (§4.5.7);
history/referrer exposure; atomic consumption, expiry and replay; first
navigation; logout and invalidation; forged and missing headers; cross-plane
credentials; port-agnostic cookies (§4.6); path normalization and asset
lookup; untrusted document rendering; multi-user machines (other local users
can reach the loopback port); remote-workspace port forwarding (plan §8.3).
Test unauthenticated navigation separately from authenticated API calls and
add a real-browser check (U56 pinned-issuer client).

### 4.11 Security review disposition (U38/U55, 11 November 2026)

The independent review [security-review-i3.md](security-review-i3.md) found
0 critical, 2 high, 5 medium, 9 low and 6 informational items, with the verdict
"No high or critical finding remains if the recommended changes are applied."
None of the fixes changes a U decision, so under U55 they are adopted and C5
proceeds:

- **Normative amendments to §4:** the review's "Recommended specification
  text" for H1 (same-user peer admission on the exchange and on every
  `/admin/api` request, launch delivery through an owner-only redirect file
  instead of process arguments, fail closed where neither is qualified), H2
  (no opening, forwarding or `asExternalUri` of an admin URL from a remote
  extension host; `airkctl admin open` refuses without a local graphical
  session), M1 (one document for launch and shell; the token stays in page
  memory and no endpoint returns it), M2 (independent token, a fresh cookie per
  exchange, exactly one cookie of the name, one uniform 401, hashed storage
  never logged), M3 (Trusted Types, no HTML sinks, sanitizing Markdown with a
  scheme allowlist, attachment-only exports, visible bidi and control
  characters, hostile-document fixture), M4 (split issuers carry critical name
  constraints for 127.0.0.1, qualified per browser before trust is offered;
  issuer key stored apart from the leaf key) and M5 (pages never ask for a code
  or URL; `admin url` warns; preferred-port and issuer-fingerprint check before
  opening; documented per-user Linux NSS trust). Where that text and earlier
  §4 text differ, the review text governs.
- **Low and informational items** are C5 implementation checklist entries,
  each with a regression test.
- The real-browser check of §4.10 remains required at Phase 5.

## 5. I4 — Install pointer, install record, setup operation, role admission

### 5.1 Inventory row

| Inventory | |
|---|---|
| Owner | Setup/install contract in `airk-cli` (setup role); pure record types in `airk_core::install_record` [P] |
| Consumers | Setup role (writer), `airkctl` (reader, §3.10.1), the extension discovery (reader, plan §8.2), later installers (U50) |
| Surfaces | Files only |
| Authorization | Owner = current user; not writable by others (Windows: owner user or Administrators, ACEs limited to the user, SYSTEM, Administrators, as `require_owner_private` [V `local_ipc_windows.rs:282-330`]; Unix: uid match, mode without group/other write); regular single-link file, no reparse point or symlink in any component (`safe_file` [V `install_files/CONTRACT.md`]). Chosen locations (U39, U96): an existing location folder passes the same owner-private rule; on Windows the existing parent of a location that does not exist yet need only be not writable by others (`airk_ipc::endpoint::require_not_writable_by_others`: owner the user, SYSTEM or Administrators; no write-type allowed entry for another account; read-only grants accepted) |
| Version | `schema` string per file; unknown major → refuse (exit 7) |
| Unknown fields | `deny_unknown_fields` (readers fail closed) |
| Limits | Pointer ≤ 4 KiB; record ≤ 64 KiB; setup operation ≤ 16 KiB |
| Cancellation / recovery | §5.6 transaction; resume from the operation receipt |
| Fixtures | `fixtures/split/i4/pointer/{valid,invalid-*}.json`, `record/{valid,invalid-*}.json`, `setup_operation.json`, `transaction_states.json`, `interruptions.json`, `role_admission.json`; exact bytes (persisted format) |
| Generation | Fixtures-only (D5). The extension validates with a hand-written reader tested against these fixtures |
| Lands in | C6 |

### 5.2 Pointer path (fixed per user; ignores inherited overrides)

| Platform | Path [P, from dependencies.md B2] | Base resolution |
|---|---|---|
| Windows | `%LOCALAPPDATA%\AIRK\install-pointer\current.json` | Native: Known Folder `LocalAppData` (not the environment) [V approach `user_paths.rs:109-176`] |
| macOS | `~/Library/Application Support/AIRK/install-pointer/current.json` | passwd home, not `HOME` [V `native_home_ignores_inherited_workspace_home_overrides`, `user_paths.rs:649`] |
| Linux | `~/.config/airk/install-pointer/current.json` | passwd home; ignores `XDG_CONFIG_HOME` |

Extension resolution [P]: `os.userInfo().homedir` (passwd on Unix, the token
profile on Windows), never `os.homedir()`/`HOME`/`LOCALAPPDATA`; Windows Known
Folder redirection is Q5.

### 5.3 Schemas [P]

Pointer, written last (after the record), atomically (temporary file +
rename + directory sync):

```json
{"schema":"airk-install-pointer/1","install_id":"<32 hex>","program":"<absolute program folder>",
 "record":"install-record.json","record_sha256":"<64 hex>","version":"1.9.0","written_unix_ms":0}
```

Record, in the program folder (macOS: `…/AIRK/program/install-record.json`
beside `AIRK.app`, U78):

| Field | Content |
|---|---|
| `schema` | `airk-install-record/1` |
| `install_id`, `version`, `channel`, `target` | Identity of this installation |
| `contracts` | `{ "i1_protocol": 1, "i2_contract": 1, "i3_admin": 1, "i4_record": 1, "state": "airk-state/1" }` |
| `executables` | `{ "airkctl": {path, sha256, bytes}, "airk": {path, sha256, bytes} }`, paths relative to the program folder (plan §9.5 layout) |
| `admin_assets` | `{ inventory_sha256, bundle_sha256 }` (C7) |
| `locations` | Absolute `program`, `data`, `state`, `cache`, `logs` (U39) |
| `signing` | Windows `{publisher, identity_eku}`; macOS `{team_id}`; Linux `{signed:false, integrity:"record-sha256"}` (values from C10) |
| `installed_by` | `{ operation_id, installer: inno \| dmg \| app-first-launch \| install-sh, installed_unix_ms }` |

The record is written only after the complete pair and asset inventory pass
signature or integrity checks and both images report the same version
(plan §9.6.3). Readiness of login startup, trust and the service is recorded in the
setup receipt, not in I4, so one action's success cannot hide another's failure.

### 5.4 Reader checks (`airkctl` and extension, same fixtures)

1. Pointer and its folder: owner, not writable by others, no link/reparse,
   size bound, schema. 2. Record: same checks, `record_sha256` matches the
   pointer, `install_id` matches. 3. Executables: path inside `program`, no
   link, digest matches; Windows Authenticode and macOS `codesign --verify
   --strict` against C10 (Linux: digest only, documented weaker). 4. Any
   mismatch → `installation_unverified` (exit 7) or, while an operation
   receipt is open, `installation_incomplete` (exit 4).

### 5.5 Setup operation file [P]

`airk-setup-operation/1`, owner-only, in a fresh owner-only directory; consumed
once by the setup role:
`{schema, operation_id, kind: install|repair|upgrade|uninstall|purge,
created_unix_ms, expires_unix_ms (≤ 10 min), requested_by, acknowledgements:
{predecessor_removal, index_reset}, locations, payload_inventory_sha256,
options: {login_startup, trust: ask|skip, start, delete_indexes: ask|keep|purge}}`.
Unattended installs set acknowledgements by explicit flags (plan §9.1.1).

### 5.6 Transaction states and interruption fixtures (plan §9.6)

Setup receipt `state/service/setup/<operation_id>.json` records each phase
separately [P]:

| # | Phase | Durable boundary | Interruption fixture expectation |
|---|---|---|---|
| 1 | `locked` | Installation operation lock held | Concurrent installer or `airkctl service start` → `installation_in_progress` |
| 2 | `predecessors_retired` | Retirement receipt with leftovers | Resume continues retirement; never resets the new installation |
| 3 | `service_drained` | Exact-image exit evidence for `airk` and open `airkctl` sessions | Resume re-observes; image-name match never authorizes termination |
| 4 | `placed` | Pair + assets verified in place | Old pointer now mismatches files → readers report incomplete (fail closed) |
| 5 | `record_written` | Record durable | Resume writes the pointer |
| 6 | `pointer_written` | Pointer durable | Installation discoverable |
| 7 | `login_registered` / `login_failed` | Login receipt | Reported separately |
| 8 | `trust_applied` / `trust_declined` / `trust_unsupported` | Trust receipt | Reported separately |
| 9 | `started` → `healthy` | `Health` with exact pair | Readiness failure reported, not hidden |
| 10 | `complete` | Receipt closed | — |

**Replacement notice (1.8.30).** Before phase 3 stops an installed service
it will replace or remove, setup writes `airk-install-replacing/1`
(`replacing.json` beside the pointer: `{schema, operation_id, program,
created_unix_ms, expires_unix_ms (≤ 10 min)}`, canonical, owner-only; fixtures
`replacement_notices` in `fixtures/split/i4/i4_cases.json`). It stays through
placement and is removed when the receipt closes (complete or failed); an
expired notice is not in force. While one is in force the extension ends its
`airkctl session`, runs no `airkctl` and shows "AIRK Service is being
updated"; after it is gone the extension verifies the installation again and
reconnects. This is how phase 3 drains editor sessions, so an upgrade does
not need the editor closed. An extension older than 1.8.30 does not read the
notice and still holds `airkctl` (`installation_files_in_use`).

Upgrades replace files in place (fixed layout); changing the program folder
writes the new copy, switches the pointer, then removes the receipt-owned old
folder. Fixtures also cover same-build repair, compatible upgrade, rejected
mismatch, running windows and locked files.

### 5.7 Role admission (plan §5.1) [P]

Private roles are a pre-Clap selector `__airk_role <name>`; a bare selector
without its authority exits non-zero with `role_admission_refused` and starts
nothing.

| Role | Started by | Authority checked before any state is opened |
|---|---|---|
| `service-login` | Task Scheduler task / SMAppService agent / systemd unit | `--install-id` equals the pointer's; own image path and digest equal the record's `airk`; the login receipt names this registration. No dependency on an installer parent |
| `service-detached` | `airk` with no arguments, or `airkctl service start` | `--start-request <nonce>`: owner-private file in `state/service/start/`, ≤ 30 s old, consumed atomically; `mode: explicit` clears a saved Stop, `automatic` honors it (exits `service_explicitly_stopped`) |
| `service-foreground` [served, Phase 4] | Development, tests and an external supervisor (was `airk serve`) | `--runtime-dir <absolute>` `[--idle-exit-seconds 1..3600]`: the service's own admission as `serve` had it (owner-only control root for the current user, single-instance lease); reads no installation |
| `pdf-child` (today `__airk_pdf_worker`, admission-free [V `airk-ingest/src/pdf_worker.rs:26`]) | The service | stdin/stdout are pipes, not a console; first frame `{schema:"airk-pdf-child/1", parent_instance, nonce}` within 5 s; opens no file |
| `setup` | Installer wrapper or `airkctl service uninstall` | `--operation <path>` passes §5.5 checks; own signature or pinned digest verified; single use |
| Others kept as private roles (P0 decision) | Workers, guardian, foreground/container entries, build verification | Each gets a row in `role_admission.json` with its authority |

## 6. C9 — UI view-state and adapter contract

| Inventory | |
|---|---|
| Owner | `packages/ui` (shared view state, events, display components); editor-only types stay in `packages/vscode` |
| Consumers | Extension webview shell (postMessage adapter + I2 host facade); admin shell (I3 adapter + event stream) |
| Surfaces | TypeScript types and a message catalog; no transport |
| Authorization | None in UI; adapters call native operations whose authorization is native |
| Version / unknown fields | `MESSAGE_VERSION` (export #10 [V]) per message catalog; unknown message types refused by `parseWebviewMessage`-style parsers in each adapter |
| Limits | Message size bounds stay in the adapters (existing host checks) |
| Cancellation / retry | No-replay classification (`explorerRequestPolicy`, #72) stays in the extension adapter; UI never retries |
| Fixtures | `fixtures/split/ui/view_messages.json` (message catalog split: UI / editor / admin), `fixtures/split/ui/capabilities.json` |
| Generation | None: TypeScript owns UI-only state (plan §4.2). Native result shapes inside view state are not redefined; WIRE items come from I2/I3 generation or are checked against Rust fixtures |
| Lands in | C9 |

**Moves to `packages/ui` (30 UI exports of `explorerContracts.ts` [V
extension.md §6]):** `MESSAGE_VERSION`, `ABOUT_LINKS`, `ConnectionState`,
`IndexState`, `SemanticState`, `EvidenceClass`, `LiveCheck`, `StatusSummary`,
`ExplorerViewState` (minus MGMT and ED members), `HitView`,
`SearchResultView`, `SearchForm`, `SearchSessionView`, `PassageView`,
`ProvenanceView`, `FactItemView`, `FactListView`, `appendFactPage`,
`InspectView`, `CitationStatus`, `CitationResult`, `CitationCheckView`,
`ChangeItemView`, `ChangesView`, `FactRow`, `HelpTopic`, `HelpCatalog`
(`extensionVersion` → `contentVersion`, C11), `EngineHelpView`,
`ParsedCitation`, `parseCitations`.

**WIRE (9), not hand-copied:** `AgentEntryStatus`, `AgentConfigFinding`,
`AgentConfigHealthView`, `AgentReconnectResult` (I2 `agents list/repair`,
generated); `AssistanceWindowStatus`, `ExplorerHandoff` (I2 W5/M12 payloads);
`FreshnessState`, `SearchMode` (native values, checked against Rust fixtures);
`ASSISTANCE_TOOLS` (airk-mcp catalog, C11). ED (16) stay; MGMT (21) leave with
management or retire with Assistance.

**Capability hooks [P].** `interface ViewHost { request(type, payload):
Promise<Reply>; subscribe(listener): Unsubscribe; capabilities: Capabilities }`.
`Capabilities` flags, absent in the browser: `openSource`, `revealInWorkspace`,
`workspaceFiles`, `planner`, `gitReview`, `suggestionsInbox`, `handoffShare`,
`agentConfigHealth`, `workspaceTrust`, `extensionReload`. Both shells provide
`copyToClipboard`, `openExternal` (through their adapters), `scopeSelect`
(window via I2 W4; admin via its session grant). UI hides a control whose
capability is absent; it never branches on the shell name.

**Message catalog split** (`WEBVIEW_MESSAGES`, 101 types [V]): evidence and
read actions (`search.*`, `evidence.*`, `code.*`, `docs.facts`, `list.more`,
`help.*`, `nav.go`, `ready`, `status.refresh`, `project.select`,
`details.copy`) → UI events; `manage.*`, `usage.*` and 36 `indexing.*` → admin;
`indexing.scope.read/apply` stay (window scope); editor-only types stay in the
extension (extension.md §6 [V]).

**Boundary checks:** `packages/ui` imports no `vscode`, `node:*`, `fetch`,
`EventSource`, IPC or adapter module (resolved bundle inputs, plan §4.4).

## 7. C10 — Signing identity file

| Inventory | |
|---|---|
| Owner | `scripts/signing-identity.json`, loaded by one owner (`sign-native.py`); others import the loader |
| Consumers | `sign-native.py`, `ci-macos-sign.py`, `package-standalone-*.py`, `check-macos-direct-runtime.py`, `github-release.py`, installers (as defines), extension build (plan §8.2) |
| Surfaces | Repository file; public identifiers only (no secrets, user names or absolute paths) |
| Authorization | Repository review |
| Version / unknown fields | `schema: airk-signing-identity/1`; loader refuses unknown fields |
| Fixtures | Unit test: the loaded `AZURE_SIGNING` equals a reviewed literal with today's five keys (independent expectation); contract test that workflow action inputs equal the file |
| Generation | None (data file, read by each consumer) |
| Lands in | C10 |

Schema (values in [build](inventory/build.md) B.2; exclusion names copied from
the verified probe, not from the draft):

| Key | Fields |
|---|---|
| `windows` | `publisher`, `identity_eku`, `artifact_signing: {endpoint, account, certificate_profile, file_digest, timestamp_url, timestamp_digest, credential, excluded_credentials[]}`, `local_client: {package, version, nupkg_sha256, dlib, dlib_sha256, dlib_publisher}` |
| `macos` | `team_id`, `identity`, `certificate_sha256`, `local: {keychain, notary_profile, unlock_item}` (home-relative `~/` paths only) |

Hosted receipts keep `AZURE_SIGNING` byte-identical; local receipts use the
`…-local/1` schemas (C7).

## 8. C11 — Catalog ownership reversal

| Inventory | |
|---|---|
| Owner | `airk-mcp`: `unified_explorer.json`, `explorer_templates.json`, help topics; `airkctl` command tree for the `airk_help command=` catalog |
| Consumers | Service (serves M2/M11, `/mcp`), extension build (generated copy), admin Help page |
| Surfaces | Checked-in JSON data embedded with `include_str!` (data, not Rust source) |
| Version | Each file carries `content_version` (integer, bumped on content change) instead of the extension version |
| Fixtures | The data files themselves are reviewed; drift tests compare generated copies |
| Generation / drift | See below; catalogs are data, not I2/I3 types, so outside the U47 pilot |
| Lands in | C11a (owner side) and C11b (first consumer switch, outside Phase 1's "consumers untouched" rule; lands with Phase 3/6 consumers) |

Today [V]: `build-explorer.cjs` reads `crates/airk-mcp/src/help/topics.rs`
(:30, :41) and, with `--write-mcp-catalog`, writes `unified_explorer.json` and
`explorer_templates.json` (:110-117) including `extension_version`; the CLI
help in MCP comes from the running parser (`install_cli_help`, `main.rs:1341`).

| Item | After the split [P] | Drift check |
|---|---|---|
| `unified_explorer.json`, `explorer_templates.json` | Hand-maintained source in `airk-mcp`; `content_version` replaces `extension_version` (served by `explorer_offline.rs:47` [V], so the explorer goldens change in a recorded, reviewed step) | `packages/vscode/src/generated/explorer_catalog.json` copied by `node scripts/sync-catalogs.cjs --check` |
| Help topics | `airk-mcp` emits `help/topics.json` from `topics.rs` | `cargo test -p airk-mcp --lib help_topics_json` (writes observation on mismatch); extension consumes the JSON, not `topics.rs` |
| `airk_help command=…` catalog | Generated data `airk-mcp/src/help/cli_commands.json`, produced from the current `airk` parser in C11a and from the `airkctl` tree from Phase 3; `airk-mcp` embeds it, so `airk` needs no parser | `cargo test -p airkctl --lib help_catalog` (Phase 3+; before that `-p airk-cli`) |
| `build-explorer.cjs` | Stops reading `topics.rs` and stops writing native catalogs (C11b) | Boundary test: no native build step reads `packages/vscode` (plan §4.4) |

## 9. C7 — Artifact receipts and release catalog

| Inventory | |
|---|---|
| Owner | `scripts/split_receipts.py` (C7: pure validators for every split receipt schema; existing pre-split decoders stay unchanged and refuse the new schemas). Consumers in Phases 5 and 7 (`build-service-runtime.py`, packagers, `ci-candidates.py`, `github-release.py`) call it |
| Consumers | `package-native.py`, `ci-runtime.py`, `sign-native.py`, `package-standalone-*.py`, `package-extension.py`, release workflows, `verify-published-release.py`, the I4 record writer |
| Version | Every changed receipt bumps its schema; pre-split decoders stay unchanged and version-gated; no pre-split receipt qualifies a split payload |
| Fixtures | `fixtures/split/receipts/{pair_metadata_3.json, build_receipt_2.json, admin_assets_1.json, native_candidate_3.json, ci_runtime_2.json, vsix_universal.json, release_catalog.json, *-local.json}`; Python tests with independent expectations |
| Generation | Fixtures-only |
| Lands in | C7 (contract and fixtures), consumers in Phases 5 and 7 |

| Receipt | Today [V build.md] | Split [P] |
|---|---|---|
| Pair metadata `airk-runtime.json` | schema 2, `airk_sha256` | schema 3: `airk_sha256`, `airkctl_sha256`, `version`, `i1_protocol`, `i2_contract`, `admin_inventory_sha256`; exact key set |
| Build receipt `airk-build.json` | schema 1, ≤ 16 KiB | schema 2: adds `admin_assets {bundle_sha256, inventory_sha256, node, npm, locks}` and an `airkctl` import report; bound rechecked |
| Admin inventory `admin-assets.json` | none | `airk-admin-assets/1`: `files[{name, sha256, bytes, content_type}]`, `bundle_sha256`, toolchain; produced by one `scripts/build-admin-assets.py` |
| Native candidate | `airk-native-candidate/2` (`vsix_target`) | `/3`: both image hashes, admin inventory, no `vsix_target` |
| CI runtime seal | `airk-ci-runtime/1` | `/2`: binds both executables and the admin digest; `restore` re-verifies against `source_identity()` including frontend inputs |
| VSIX | per-target | universal (U29): `{target:"universal", i2_contracts:[1], required_capabilities, ui_inventory_sha256}`; no native entry |
| Release catalog | 8 artifacts, 12 inputs, 16 assets | 4 native targets + 1 universal VSIX + installers; recounted from the reviewed fixture, not from code |
| Local signing | — | `airk-native-signing-local/1`, `airk-service-pair-signing-local/1`, `airk-standalone-installer-local/1`; no paths [V build.md B] |

## 10. C8 — Rust-emitted cases for `nativeOutcome.ts` and `readRetryDelay`

| Inventory | |
|---|---|
| Owner | Native emitters and decision rules (Rust); fixtures in `fixtures/wire/` with independent Python expectation generators |
| Consumers | `packages/vscode/src/readRetry.ts`, `nativeOutcome.ts` (semantic rules stay hand-written, §10.3 of the plan); I2 exit-class mapper (§3.4) |
| Fixtures | `fixtures/wire/read_retry_envelopes.json` + `read_retry_envelopes.py`; `fixtures/wire/native_outcomes.json` + `native_outcomes.py` |
| Generation | None (semantic rules; U47 excludes them) |
| T0 | `python fixtures/wire/read_retry_envelopes.py --check`; `cargo test -p airk-cli --test native_read_retry`; `node --test packages/vscode/out/test/wireFixtures.test.js` |
| Lands in | C8 |

**Read-retry gaps** (combined path already covered by 32 cases [V]):

| Branch of `readRetryDelay` [V `readRetry.ts:119-166`] | Cases to add |
|---|---|
| `scoped_status_prepare_timeout` / `scoped_connection_unavailable` with `requested_scope` | Each of the 9 accepted `native_control_response` codes; a 10th code refused; `native_control_response` present on the timeout code refused; scope clock backwards refused, forward accepted; changed grant refused; `retry_after_ms` 0, 1, 1000, 1001, `25.0` |
| Top-level `status: "retry"` (`publication_busy`, `selection_busy`) | Valid; wrong `action`; `retryable:false`; other code; delay bounds; integral-float literal |
| Top-level `direct_query_admission` / `direct_query_setup` | `fresh_brokerage_required` true only for setup; `reason` ≠ `busy`; `query_submitted:true`; delay bounds; integral-float literal |

The envelopes are produced by the Rust code that emits them (for example
`airk_mcp::remote::scoped_setup_failure` and the direct-query refusals [I:
locate each emitter]); the Rust test checks the emitter output equals the
fixture bytes and the native decision (`crates/airk-cli/tests/native_read_retry.rs`
[V extension.md §8]) equals the expectation; the TypeScript test runs
`readRetryDelay` over the same cases.

**Native outcomes** (`nativeOutcome.ts`, 45 lines, no fixture today [V]):
cases are native replies with an expected class — `accepted` (`Job`,
`CatalogApplied`), `refused` (`Failure{code}`), `unknown`
(`CatalogOutcomeUnknown`, `SourceChangeResolutionOutcomeUnknown`,
`registry_response_unknown_retry_with_key`, transport loss after send),
`unvalidated` (well-formed JSON failing the consumer validator). Rules under
test: the complete reply is retained; `outcomeUnknown` is true for
`unknown`/`unvalidated`; no class authorizes another write. A pure Core
classifier [P] emits the classes; the I2 exit mapper (1 vs 6) uses the same
fixture.

## 11. Ordered landing list

One commit each, `git commit -s`, consumers untouched unless stated; each
commit carries its fixtures, its T0 selection with a non-zero selected count,
and contract docs (`CONTRACT.md` where a module gains one).

1. **C10** signing identity file + loader + literal test (no consumer reads it yet).
2. **C1** Core moves (`usage_page`, resolution shapes, credential record parse) with re-exports; `usage_page.json`, `root_resolution.json`.
3. **C2** I1 variants M1–M23, N1, V1 (and W1–W8, A1 types) with explicit `not_yet_served` arms and relay refusal arms; `control_methods.json`, `not_yet_served.json`.
4. **C3a** Journal ownership contract (M5–M8 semantics, recovery matrix) as fixtures and transcripts; `request_recovery.json`.
5. **C3b** Window plane, Assistance queue, handoffs, admin launch code contracts; `window_plane.json`, `admin_launch_code.json`.
6. **C8** Read-retry envelope and native-outcome cases with Python generators; Rust and TypeScript tests over existing code.
7. **C4a** `crates/airkctl` with `src/contract/` (version, exit classes, error line, session v2 types), `--version`/`version` only; U27 edge gate entries; `version.json`, `exit_codes.json`, `error_lines.json`, `compatibility_matrix.json`, `airkctl_session_frames.json`, `session_sequences.json`, `offline_operations.json`.
8. **C4b** U47 I2 pilot: schemars derives, `contract_schema` test, `scripts/dev/contract_types.py`, checked-in `i2.schema.json`/`i2.ts`; pilot result recorded.
9. **C6** I4 record/pointer/setup-operation types and fixtures, transaction states, role admission table.
10. **C7** Receipt and catalog schemas with fixtures (`build-service-runtime.py` decoder additions only, version-gated).
11. **C11a** Catalog content versions, `topics.json`, `cli_commands.json` and their drift tests (C11b consumer switch lands later).
12. **C5** I3 specification, operation-table fixture, admission/header/envelope cases, I3 generation — **after** the U38/U55 review of §4 has no open high or critical finding.
13. **C9** `packages/ui` view-state and capability contract with the message split fixture (requires C4 and C5 types).

## 12. Open questions

Only genuine gaps found by this review; none reopens a U decision.

| # | Question | Proposal |
|---|---|---|
| Q1 | plan §6.2 says every command accepts `--output json` and surviving commands keep their arguments, but ten commands already use `--output <FILE>` (nine `explorer` commands, `service logs`) [V `docs/cli-reference.md`]. Overloading one flag with a format or a path is ambiguous for a file named `json`. | Global `--output json\|text`; rename the file flag to `--output-file` on those ten commands, with a compatibility-inventory row and reviewed golden updates. Needs the user's choice because it changes kept argument names. |
| Q2 | U75 makes abandoning a stuck lifecycle record an offline `airkctl` operation that never opens registries, but lifecycle records and the explicit-Stop flag live in the service registry today (`lifecycle.rs:60-100` opens `ServiceRegistry`) [V]. | Keep lifecycle intent for split installs in an owner-private `state/service/lifecycle/` record written by `airkctl` and the service (a new file in new state folders, not a change to `airk-state/1` registry rows), and let the service role decide explicit vs automatic start (§5.7). Confirm before C6, since it adds a persisted file. |
| Q3 | Offline OS operations (trust store, login manager status, quiescent issuer rotation) must run in `airkctl`, whose AIRK edges are only Core, `airk-process` and `airk-ipc` (U27); that code lives in `airk-cli` today. | Move these thin OS adapters into `airk-process` (as the P0 decision already does for process-start primitives) with reviewed boundary entries; alternative: `airkctl` starts a private `airk` maintenance role, which would add a process kind to plan §4.3. |
| Q4 | Admin-page **Restart** (plan §7.3): the P0 decision makes restart "`airkctl` observes exit, then starts", but a browser has no `airkctl`. | The service records a restart intent and exits with a distinguished status that the login manager restarts; when no login registration exists, the page offers Stop plus the start guidance. Needs a login-manager behavior check per platform. | **Closed (C5):** the operation table admits `service.restart` on the admin surface; its Phase 5 handler restarts through the OS login manager when login startup is registered and otherwise refuses with the start guidance.
| Q5 | Windows: the pointer lives under the `LocalAppData` Known Folder, which can be redirected; Node has no Known Folder API, and the extension must not trust `LOCALAPPDATA`. | Extension derives `<token profile>\AppData\Local` from `os.userInfo().homedir`; if the Known Folder is redirected (detected with a reviewed read-only `reg.exe query` of `User Shell Folders`), it reads the redirected path, still applying all owner checks. |
| Q6 | U77 window grants and the admin session grant must not create agent grants, but authority-contract-1 workers require a catalog `GrantId` in `ScopedReadRef` [V `service.rs:840-848`]. | An ephemeral, in-memory broker grant kind bound to the window lease or admin session; if it needs a persisted catalog row, that is an `airk-state/1` change needing a schema-identity proof or user approval before C3b. |
