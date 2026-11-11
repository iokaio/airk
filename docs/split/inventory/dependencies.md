# Split inventory: dependencies, listener and paths (P0.5, P0.7)

Status: Phase 0 draft, 11 November 2026; source review only (no build, test or
Cargo/npm command was run; the static `scripts/dev/boundary_check.py` and its
module-graph library were read and executed read-only). Authorizes no move or
deletion. Decisions U1–U71 (split-plan §2.1) are taken as final.

Serves [split-plan.md](../split-plan.md) §4.1–4.2, §6.1, §7.1, §9.5, §10.1–10.2,
U26, U27, U36, U39, and [simplify-plan.md](../../simplify-plan.md) §5.2 S1.4.
Companion: [predecessors.md](predecessors.md) §1 (pre-split path map).

## Conventions

- **[V]** verified by reading the cited code (or by running the read-only
  module-graph script). **[I]** inferred; needs confirmation.
- Rust paths are relative to `crates/airk-cli/src/` unless prefixed. "core",
  "kernel", "store", "mcp" mean `airk_core`, `airk_kernel`, `airk_store`,
  `airk_mcp`. Line numbers are at `caab8ec9`.
- Classes: **(1)** stays service-side; **(2)** pure type → `airk-core` in
  contract step C1 (no kernel, registry, filesystem or handler behavior);
  **(3)** transport → `airk-ipc`; **(4)** needs splitting (how is stated);
  **(5)** already in core — only the import path changes.
- AIRK MCP was unavailable (server connection closed); all evidence comes from
  local reads and `rg`.

---

# Part A — Dependency inventory (P0.5)

## A0. Blocking finding: the module-cycle ratchet already fails at HEAD

`python scripts/dev/boundary_check.py` reports **`module_cycles: FAIL`** [V]:
`http_gateway`, `project_worker` and `reference_refresh_control` joined one
merged 20-module cycle. The baseline's 14-module cycle and its
`{catalog_admission, catalog_coordinator, project_supervisor}` cycle have merged.

- The closing edge is `project_supervisor.rs:380`
  `crate::project_worker::serve(...)` (in-process owner, added in `fe607425`).
  Before that commit `project_supervisor.rs` did not reference `project_worker`
  [V by `git show`].
- The path back into the 14-cycle is `project_worker.rs:369-370`
  (`runtime_metadata::{bundle, work}`) and `project_worker.rs:819`
  (`runtime_install::rebuild_observation_for_control`) [V].
- Consequence: Phase 2 cannot use the ratchet as its "re-measure" evidence
  until the gate is green again. Group **G5** below restores it without
  changing the baseline. The alternative, a reviewed baseline update that
  accepts the merged cycle, would weaken a ratchet and needs the user's decision.

## A1. Cycle membership of the cluster (current tree, `boundary_check` graph)

| Module | Lines | Baseline cycle | Current cycle | Production edges to other `airk-cli` modules [V] |
|---|---|---|---|---|
| `local_ipc.rs` (+`_unix`, `_windows`, `_windows_job`, `_windows_path_tests`) | 378+118+481+198+83 | none | none | `service_ipc` |
| `service_ipc.rs` | 165 | none | none | — |
| `client_owner.rs` | 287 | none | none | — (std/OS only) |
| `session_protocol.rs` (+`tests.rs`) | 136 | none | none | `service_ipc` (doc link and tests only) |
| `console_client.rs` | 246 | 14-cycle | 20-cycle | `runtime_pair`, `service_ipc`, `service_runtime` |
| `project_client.rs` (+`status_observation`, `read_lifetime`) | 2465+1589+84 | 14-cycle | 20-cycle | `local_ipc`, `project_protocol`, `read_attempt`, `runtime_metadata`, `service_ipc`, `service_resource_policy`, `service_runtime`, `usage_client` |
| `project_protocol.rs` | 323 | none | none | `embedding_pool_protocol`, `service_logs` |
| `usage_client.rs` | 435 | 14-cycle | 20-cycle | `project_client`, `service_runtime` |
| `client_requests.rs` | 530 | 2-cycle with `service_configuration` | same | `console_client`, `install_files`, `install_layout`, `service_configuration` |
| `service_frontend.rs` | 964 | 14-cycle | 20-cycle | `console_client`, `install_layout`, `project_client`, `runtime_install`, `service_runtime`, `user_paths` |
| `service_registration.rs` | 216 | none | none | `install_layout` |
| `read_attempt.rs` (+`tests.rs`) | 85 | none | none | — |
| `runtime_metadata.rs` | 596 | 14-cycle | 20-cycle | `process_roles`, `runtime_pair`, `service_runtime` |
| `service_runtime.rs` control client (`:730-875`) | ~145 | 14-cycle | 20-cycle | `local_ipc`, `runtime_install` |

Inbound edges that keep the clients in the cycle [V]: `service_runtime` →
`console_client` (`administrative_read`, `service_runtime.rs:51`), →
`project_client` (`:66`), → `usage_client` (`:62,67,70`), → `runtime_metadata`
(`:355-356`). `combined_frontend`, `http_backend`, `unified_mcp`,
`runtime_install` → `project_client`; `http_backend`, `runtime_install`,
`unified_mcp` → `service_frontend`.

## A2. Symbol inventory

Production code unless marked *(test)*. Every core-only use (`airk_core::…`,
std, serde, tokio, sha2, windows-sys, libc) is omitted; those are already
allowed for `airk-ipc`.

### Transport files (`local_ipc*.rs`, `service_ipc.rs`)

| Symbol | Where | Class | Note |
|---|---|---|---|
| `store::service::ServiceLease` | `local_ipc.rs:129` `prepare_control_endpoint`, `:202` `retire_endpoint_namespace` | 4 | Only `lease.root()` is read [V]. Ipc takes `&Path` (`prepare_control_socket(root)`); the lease-taking wrapper stays service-side. The caller is `service_runtime.rs:530` [V]. |
| `cleanup_predecessor_endpoints`, `predecessor_child_role`, `remove_exited_endpoint` (`q-`/`e-` child roles, 64-entry pass) | `local_ipc.rs:156-245` | 4 | The socket-entry primitive is ipc. Child-role recognition and exit-proof policy stay with the supervisor (1). Caller: `project_supervisor.rs`. |
| `with_current_user_job_security`, `require_job_user` (named-job ACLs) | `local_ipc_windows_job.rs:28…`; re-exported `local_ipc.rs:18-20` | 1 | Used only by the embedding pool (`embedding_pool_process/shared.rs:243,291`, `embedding_pool_bootstrap.rs:277`) [V]. Move them to a service-side Windows job module. Ipc exposes `pub(crate)`→`pub` SID/descriptor helpers (`Allocation`, `sid_string`, `descriptor`) only if the service needs them after extraction. |
| `prepare_root`, `require_existing_root`, `create_root`, `require_owner_private`, `process_user`, `current_user`, `caller_identity`, `PeerRejected`, `endpoint`, `connect`, `Listener`, `Stream`, `read_message*`, `write_message*` | `local_ipc.rs`, platform files | 3 | Owner-only roots and peer checks are also needed by `airkctl` (U39 pointer and token-file ownership). `prepare_root` has 26 integration-test uses via `airk_cli::local_ipc` [V]; keep a `pub use` shim until those tests move. |
| `crate::owner_acl` shim | `lib.rs:35-38` (Windows) | 3 | Re-export of `require_owner_private`; user `explorer_assistance.rs:242`. |
| `service_ipc::{read_frame, write_frame, write_frame_limited, MAX_CODEC_BYTES}` | `service_ipc.rs` | 3 | Generic length-prefixed JSON codec; 14 module users across the service [V]. |

### `client_owner.rs`

| Symbol | Where | Class | Note |
|---|---|---|---|
| (none outside std/OS) | — | 4 | Owner-loss watch is session lifetime policy, not transport: it moves to `airkctl` with the session (Phase 3). Its service-side callers (`runtime_install`, `session`, `unified_mcp`, `universal_launcher`) retire with stdio MCP and the launcher (§5.3). |

### `session_protocol.rs`

| Symbol | Where | Class | Note |
|---|---|---|---|
| `crate::service_ipc` framing (doc link); `MAX_CODEC_BYTES` *(test)* | `:11`; `tests.rs:74-75` | 4 | I2 types and limits (`SessionRequest/Method/Response/Outcome/Failure`, `MAX_*`) → `airkctl` (Phase 3). The codec stays in ipc. Fixture `fixtures/wire/session_frames.json`. |

### `console_client.rs`

| Symbol | Where | Class | Note |
|---|---|---|---|
| `service_runtime::request`, `request_for_instance` | `:3,125,142` | 3 | Via the ipc control client (G2). These are the cycle edges. |
| `runtime_pair::current_executable_sha256`, `digest_file` | `:160,166` | 4 | `observation.executable_sha256 != engine` (`:131`) compares the service image digest with **this process's** digest. Under the split `airkctl` is another image, so this check would always fail. Replace it with the I1 exact pair (version and protocol) plus the I4 record's `airk` digest. Pure SHA-256 of a file → ipc helper. |
| `service_ipc::{read_frame, write_frame}` | `:173,189,191` | 3 | `run` (`service console`) retires (§6.2); the frame use is generic. |
| `relay`, `request`, `metadata_only` filter | `:150-161` | 4 | The metadata-only filter is I2 session policy → `airkctl`. `request` stays a service-internal helper until its users move (`client_requests`, `service_frontend`, `standalone_setup`). |

### `project_client.rs` and submodules

| Symbol | Where | Class | Note |
|---|---|---|---|
| `service_runtime::request` | `:249,304` | 3 | Health and Connect brokerage. |
| `local_ipc::*`, `project_protocol::{Query, Accepted, role, QueryResult}`, `service_ipc::write_frame` | `:3-6,347-371`; `read_lifetime.rs:12-49` | 3 / 2 | Transport is (3). Client query frames are (2) only if Option B is chosen (A3). |
| `mcp::remote::scoped_setup_failure` | `:47` | 4 | Builds the MCP error envelope. Keep it service-side (Option A), or move a pure JSON builder to core (Option B). |
| `mcp::response::error_result` (returns `rmcp` type) | `:111` | 4 | Same as above. |
| `mcp::response::StatusReceipt` | `:131,549-564`; `status_observation.rs:49-489`; `protocol` `:171,180` | 2 | Pure serde struct of six `Sha256Digest` (`mcp/src/response.rs:374`) [V]. Move to core; mcp re-exports. Wire bytes must stay identical. |
| `usage_client::NativeReadFailure::from_response` | `:149` | 3 | Pure JSON error wrapper (`usage_client.rs:10-24`) [V]. It is the `project_client` ↔ `usage_client` back edge. |
| `read_attempt::supported`, `encode` | `:386,504` | 4 | See `read_attempt.rs`. |
| `mcp::read_attempt::ReadAttemptContext` (`new`, type) | `:423-910`; `status_observation.rs:17,622,773`; `read_lifetime.rs:11` | 4 | Runtime object (`CancellationToken`, guards), not a DTO [V `mcp/src/read_attempt.rs:31`]. A client needs only deadline and retry deadline → `ReadAttemptBudget` (core). The context stays mcp/service. |
| `runtime_metadata::observe_process` | `:439,963` | 4 | Frontend identity. A pure `RuntimeProcess` builder can be shared; the build-receipt read (`runtime_metadata.rs:56-100`, staged `airk-install.json`) is install-layout specific (1). |
| `runtime_metadata::unavailable` | `:1075` | 1 | Status rendering for the MCP status tool. |
| `mcp::local::NativeRead` (incl. `SourceProjection`, `VerifyProjection`) | `:455-474` | 5 | Re-export of `core::native_reads` (`mcp/src/local.rs:12`) [V]. |
| `kernel::service_broker::new_instance_id` | `:506` | 4 | `getrandom` → `Incarnation` (`kernel/src/service_broker.rs:107`) [V]. Add a fresh-identity helper in **ipc** (it may depend on `getrandom`), not in core: no OS behavior enters Core. |
| `mcp::query_capacity::{QueryCapacity, QueryPermit}` | `:568-596` | 1 | Channel-pool admission for the service backend. |
| `mcp::unified::{success, failure}` | `:752,929,1248-1302`; `status_observation.rs:598,909` | 1 | MCP envelope. |
| `store::…::ServiceRegistry::open_saved_before`, `SavedScopeExpired` | `:762,767` (`status_scope`) | 1 | The client opens the SQLite registry to fence its status cache [V]. `airkctl` may never do this (§4.1). |
| `store::…::source_scope_retryable` | `status_observation.rs:753` | 1 | Downcasts `rusqlite` errors. |
| `mcp::response::typed_error_result` | `status_observation.rs:757` | 1 | |
| `mcp::remote::{RemoteTools, ReadyFuture, RemoteFuture, serve}` | `:776-1203` | 1 | The `Backend` MCP adapter. The service HTTPS path uses it: `http_backend.rs:281` `scoped_tools`, `combined_frontend.rs:341,495`, `unified_mcp.rs:518-1205` [V]. |
| `service_resource_policy::query_concurrency` | `:784,991` | 1 | Reads service resource configuration. |
| `mcp::help::call`, `mcp::StatusArgs` | `:919,924` | 1 | `local_help` path of the backend. |
| `kernel::service_broker::Broker::new`, `store::quota::QuotaContention`, `ServiceRegistry`, `service_resource_policy::save` | `:1809,2121,2317`; `status_observation.rs:1122,1353`; `shared_authority_ipc_tests.rs:10,145,701` *(test)* | 1 | Test peers. |

### `project_protocol.rs`

| Symbol | Where | Class | Note |
|---|---|---|---|
| `kernel::scoped_reads::ScopedReadAuthority` | `:36,42` (`Command::InstallScope`, `RenewScopeLease`) | 1 | Private supervisor→worker control. |
| `crate::embedding_pool_protocol::Access` | `:73,79` | 1 | |
| `crate::service_logs::JobFailure` | `:320` | 1 | |
| `mcp::local::NativeRead` | `:169` (`Query.native`) | 5 | |
| `mcp::response::StatusReceipt` | `:171,180` | 2 | |
| `Query`, `QueryResult`, `SelectionReadAdmission`, `Accepted`, `role()` | `:158-200` | 4 | The file splits along the I1 boundary. `Init`, `Command`, `Reply`, `Work*` and `WorkerCheckpointObservation` stay service-private (1). The client query frames go to core only under Option B. |

### `usage_client.rs`

| Symbol | Where | Class | Note |
|---|---|---|---|
| `kernel::usage_page::{Page, assemble}`; `scoped_page` *(test)* | `:38,293,315`; `:397` | 2 | `usage_page.rs` (332 lines) uses only serde, sha2 and core types [V]. Move `Page/Record/Part/page/scoped_page/assemble` to core; kernel re-exports. |
| `mcp::local::{ReportFormat, SettingsView, NativeRead::*}` | `:100-236` | 5 | |
| `mcp::read_attempt::ReadAttemptContext::new` | `:259` | 4 | Becomes a `ReadAttemptBudget` deadline. |
| `project_client::Connection::connect_scoped` | `:240,269` | 4 | Selected-scope reads. Option A needs a scoped variant of `ControlMethod::NativeRead` (today it takes `project` only, `core/src/service.rs:164`) — a Phase 1 addition. |
| `service_runtime::administrative_read` | `:284` | 4 | `administrative_read` (`service_runtime.rs:45-76`) is client code (console request, then `Connection` fallback) inside the service runtime. Move it to `usage_client` (G4). |

### `read_attempt.rs`

| Symbol | Where | Class | Note |
|---|---|---|---|
| `mcp::read_attempt::ReadAttemptContext` | `:7,22-58` (`encode`), `:60-73` (`decode`) | 4 | `encode(deadline, retry_deadline, request_id)` with `airk_process::monotonic::now_ns` (`:27`) → ipc (pure + process). `decode` builds the mcp context on the worker (1). `supported` (`:75`, ≥ 1.8.14 worker gate) is pre-split compatibility (1). |
| `project_protocol::{Query, QueryResult}`, `mcp::{NativeRead, StatusReceipt}` | `tests.rs:11-76` *(test)* | — | Follow their owners. |

### `runtime_metadata.rs`

| Symbol | Where | Class | Note |
|---|---|---|---|
| `process_roles::main_image`, `runtime_pair::payload_root`, `current_executable_sha256` | `:60,62,106` | 4 | `build` reads the staged payload receipt. That is staged-engine identity (1; retire with it). The `RuntimeProcess` fields (version, protocol, pid, os, arch) are a pure builder that can be shared. |
| `service_runtime::request`, `request_for_version_and_instance` | `:165,221,391,398` | 3 | `bundle`, `unavailable` → ipc control client. This removes `runtime_metadata` from the cycle. |
| `bundle`, `work`, `work_observation`, `unavailable` | `:158-426` | 1 | Status rendering for worker and MCP. |

### `client_requests.rs`, `service_registration.rs`, `service_frontend.rs`

| Symbol | Where | Class | Note |
|---|---|---|---|
| `store::…::ServiceRegistry::open_saved`, `open` *(test)* | `client_requests.rs:89,222`; `:479` | 1 | Journal namespace (§5.2: the journal moves into the service; contract first). |
| `install_files::safe_file`, `install_layout::validate_control_root` | `client_requests.rs:88,98,110,192` | 1 | |
| `kernel::service_broker::new_instance_id` | `client_requests.rs:270-296` | 4 | ipc helper (see above). |
| `service_configuration::{Draft, prepare}` ↔ `client_requests::request_epoch` | `client_requests.rs:282,285`; `service_configuration.rs:184` | 1 | Its own 2-cycle; not on the ipc path. |
| `console_client::request` | `client_requests.rs:22,337` | 1 | |
| `refusal_guidance` | `client_requests.rs:322` | 4 | Pure text → `airkctl` rendering. |
| `kernel::service_projects::{planned_registration, validate_planned, validate, registration}` | `service_registration.rs:8,22,75`; `client_requests.rs:489` *(test)* | 1 | Control method (§5.2). |
| `store::service::data_scope::privacy_warning` *(test)*; `ServiceRegistry::open_saved/open` | `service_registration.rs:15,139-200` | 1 | |
| `user_paths::canonical_location` | `service_frontend.rs:66` | 1 | |
| `store::…::{ServiceRegistry, open_saved_before, open_saved, SavedScopeExpired, SavedScopeStamp, SavedScopeResolution}` | `service_frontend.rs:23-243,451` | 1 | Root/selection resolution (control method, §5.2). |
| `runtime_install::{caller_install_root, installed_metadata_control, installed_control, default_project_data}` | `service_frontend.rs:270-412` | 1 | Staged-layout discovery; replaced by the I4 pointer in `airkctl`. |
| `project_client::SourceFence`, `console_client::request`, `service_runtime::request` | `service_frontend.rs:22,192,310,430` | 1 | |
| `kernel::service_projects::planned_registration` | `service_frontend.rs:455` | 1 | |
| `RootScope`, `RootResolution`, `RootObservation` (types) | `service_frontend.rs:9-44` | 4 | Used by `unified_mcp` and `main.rs` [V]. Only the result shapes cross I1; define those in core when the resolve control method lands. |

### Control request path (`service_runtime.rs:730-875`)

| Symbol | Where | Class | Note |
|---|---|---|---|
| `request`, `request_for_version`, `request_for_instance`, `request_for_version_and_instance`, `request_scoped` | `:730-786` | 3 | Thin wrappers over the ipc control client. |
| `registry_busy_retry` (≤6 retries, 3 s window), `request_once` (5/10/20/30 s deadlines, Health identity check) | `:808-871` | 3 | Typed I1 control client. Record in `CONTRACT.md` that the `registry_busy` retry is part of the I1 contract. Its fixed `sleep_until` stays as is in the structural move; an injected clock is a follow-up. Tests `service_runtime/registry_retry_tests.rs` (151 lines) move with it. |
| `prebaseline_metadata_method` | `:755-768` | 4 | Belongs with the gate below. |
| `runtime_install::require_baseline_control` → `state_contract::require_control` (`runtime_install/state_contract.rs:168`) → `install_layout::installation_for_control`, `read` (`ServiceRegistry::open_saved(control)?.incarnation()`, `:59`), `required` (managed/activation selection, `:68-88`) | `:779` | 4 | Client-side baseline gate, a pre-split staged-install duty. For a split fixed layout, `installation_for_control` finds no pre-split installation binding, so the gate passes [I]. Options in A4 G3. Every client that calls the gate stays in the cycle (the gate lives in `runtime_install`). |
| `request_initialized_probe`, `runtime_install::InitializedProbe` | `:788-806` | 1 | Installer-only (staged engine). |
| `administrative_read` | `:45-76` | 4 | Moves to `usage_client` (G4). |

## A3. What `airkctl` would need from `airk-mcp` today, and how to avoid it

| Need (today's CLI) | Evidence | Avoided by |
|---|---|---|
| Tool catalog `unified::tools()`, `local::tools`, `remote::Tool` | `tools.rs:3,15,23,135,222` | Served by the service over I1: a catalog method (§5.2 "Tools catalog and help … served by the service") |
| Tool help `help::call` | `tools.rs:142,236`; `project_client.rs:919` | Served by the service |
| **CLI help inside MCP help:** `help::install_cli_help(cli_help_catalog)` builds the MCP `airk_help command=` answer from the CLI parser in the same process | `main.rs:1341`; `mcp/src/help.rs:15-20` | Reverse dependency. After the split `airk` has no parser. Emit the `airkctl` help catalog as a reviewed, generated JSON data file with a drift check. airk-mcp embeds the data (not `include!` of Rust). Otherwise `airk_help command=…` loses content (parity regression). |
| Unified envelope and wrappers `unified::{Unified::new, combined, Project, success, failure}`, `remote::call_observed`, `response::ResponseMode::from_environment` | `tools.rs:29-70,436-461` | Every evidence read (`search` … `changes`, `main.rs:1880,1932` → `tools::run_remote_selected`) runs this wrapper today [V]. Option A: one service-side I1 tool-call method returns the exact unified result. This is also what MCP HTTPS runs (parity). |
| Explorer offline drafts and templates `explorer_offline::{draft, validate_draft, list_templates, get_template, proposal_status}` + `explorer_templates.json` | `main.rs:894` | Served by the service (§5.2) |
| Argument structs `SearchArgs` … `ChangesArgs`, `StatusArgs` | `mcp/src/lib.rs:191…`; `main.rs:2937-3074` *(test)* | `airkctl` forwards JSON arguments. Parity tests compare with service-emitted schemas from the catalog method. Core already has `read_arguments`. |
| `StatusReceipt`, `ReadAttemptContext`, `NativeRead`/`ReportFormat`/`SettingsView`, `usage_page` | A2 | Class 2, runtime-only or 5. Under Option A only `usage_page` and `ReadAttemptBudget` (already core) reach `airkctl`. |

**Recommendation: Option A.** The query client (`Connection`, `Backend`,
status cache, channel pool) stays service-side. It is already the service's
own HTTPS/MCP path. `airkctl` sends reads through new I1 methods (tool call,
scoped native read, catalog, help, explorer). Without this, C1 must move the
client query frames, the error-envelope builders and the status receipt into
core, and `airkctl` would still need the unified wrapper and help from
`airk-mcp` (Option B). That violates U27. `airk-ipc` then carries only the
transport, owner checks, framing, the typed control client, the read-budget
encoder and the fresh-identity helper.

## A4. Ordered untangling groups (Phase 2; one small commit each)

Simulated with the repository's own module-graph code; real re-measurement
after each group is still required [V for simulation only].

| # | Content | Track/kind | Files / lines (est.) | Cycle outcome |
|---|---|---|---|---|
| **G1** | Create `src/ipc/` (`mod.rs` facade, `CONTRACT.md`). `git mv` the transport files: `local_ipc.rs`→`ipc/endpoint.rs`, `local_ipc_unix.rs`→`ipc/unix.rs`, `local_ipc_windows.rs`→`ipc/windows.rs`, `local_ipc_windows_job.rs`→`ipc/windows/job.rs`, `local_ipc_windows_path_tests.rs`→`ipc/windows/path_tests.rs`, `service_ipc.rs`→`ipc/frame.rs`. Use plain `#[cfg(windows)] mod windows;` / `#[cfg(unix)] mod unix;` so no `#[path]` remains. Delete the two `module_honesty` rows (`local_ipc_windows_job.rs`, `local_ipc_windows_path_tests.rs`) from `boundary-exceptions.json`. Keep `pub mod local_ipc` / `service_ipc` / `owner_acl` as inline `pub use` shims in `lib.rs`. Use a committed move script. | Structural | 11 files; ~200 lines plus ~1,420 renamed | None (not cycle members) |
| **G1b** | Narrow ipc. Move named-job security out to a service module. Change `prepare_control_endpoint`/`retire_endpoint_namespace` to take `&Path`; keep the lease wrappers at the caller. Move the `q-`/`e-` predecessor-child policy beside `project_supervisor`. | Structural | ~6 files / ~350 | None |
| **G2** | `ipc/control.rs`: `request_once`, `registry_busy_retry`, Health identity check; move `registry_retry_tests.rs`. `service_runtime::request*` become wrappers (gate + ipc). Update `CONTRACT.md`. | Structural | ~5 / ~400 | None yet (wrappers still in `service_runtime`) |
| **G3** | **Baseline-gate ownership.** Recommended: a reviewed behavior step under U24. The service control dispatcher refuses non-prebaseline methods when `require_baseline_control` fails, with the same `clean_break_required` code. Remove the client-side call. Add a regression and a compatibility-inventory row. Alternative: an injected preflight hook in `ipc::control`, installed by the composition roots; an unset hook would silently skip the gate in library tests. Then point `console_client`, `project_client`, `runtime_metadata` and `usage_client` at `ipc::control`. | Behavior (contract) then structural | ~6 / ~250 | Simulated: `console_client` and `runtime_metadata` leave. `{project_client, usage_client}` stays a 2-cycle (a subset of the baseline; passes). |
| **G4** | Move `NativeReadFailure`/`require_native_success` → `ipc/read_failure.rs`. Move `administrative_read` → `usage_client` (callers `main.rs:1807`, `service_configuration.rs:106,207`). Make `read_attempt::encode` take `(deadline, retry_deadline)` and move it to `ipc/read_budget.rs`. Add `ipc::fresh_incarnation()` (`getrandom`); client callers stop calling `kernel::new_instance_id`. | Structural | ~12 / ~300 | Simulated: `project_client`, `usage_client` leave. |
| **G5** | Restore the ratchet (A0). `service_runtime` injects `project_worker::serve` into `Supervisor::start` (removes `project_supervisor → project_worker`). It also passes the rebuild-observation function into the worker init (removes `project_worker → runtime_install`). These are hooks, not new branches. | Structural | ~4 / ~100 | Simulated after G2–G5: cycles `{catalog_admission, catalog_coordinator, project_supervisor}` (= baseline), 10-module remainder of the 14-cycle, `client_requests`↔`service_configuration`, plus the two unchanged small cycles — all subsets; gate green. |
| **C1** (Phase 1, airk-core, before any consumer) | Move `kernel::usage_page` to core (kernel re-exports). Fix `StatusReceipt` (mcp re-export) only if any non-service consumer remains under Option A; otherwise defer. Add I1 methods: tool call, scoped `NativeRead`, catalog/help, explorer, resolve/registration (§5.2), each with independent fixtures. | Contract | ~8 / ~600 incl. fixtures | — |
| **G6** | Extract the crate. `crates/airk-ipc` gets `ipc/*` by committed script. Rewrite `crate::ipc::` → `airk_ipc::` by script. Update the workspace manifest and lockfile. `boundary-exceptions.json` crate edges: `airk-ipc: [airk-core, airk-process]`; `airk-cli` adds `airk-ipc`. Add SPDX headers and notices (`dependency-audit.py`, `license-notices.py`). Update `docs/architecture.md`. Prove the T0 command runs without the process stack (§10.1 criterion 4). | Structural | ~35 files (call sites scripted) / ~250 hand-written | — |

Rules applied:

- No cycle member enters `ipc/` before it leaves the cycle. G1 moves only
  non-members. `project_client`, `console_client`, `usage_client` and
  `runtime_metadata` never move into ipc: their transport pieces are extracted
  instead, and the remainder stays in the service or goes to `airkctl` in
  Phase 3.
- **`check-self-spawn.py` STARTS keys:** no cluster file appears in `ALLOWED`,
  `LEGACY_PENDING` or `STARTS` [V], so G1–G6 change no key. Phase 3 adds keys
  for `airkctl service start` (detached `airk`) and the U20 uninstall launcher.
  Those starts reuse `native_process::spawn_detached` (`STARTS`) and
  `runtime_install::start_with_observation_clock` (`STARTS`). They cannot be
  copied into `airkctl`, so moving the `native_process` primitives into
  `airk-process` is a needed decision: §10.1 lists `airk-process` as
  "unchanged".
- Final extraction edges: `airk-ipc → {airk-core, airk-process}`;
  `airkctl → {airk-core, airk-process, airk-ipc}`; `airk-cli` (later
  `airk-service`) adds `airk-ipc`. Third-party crates in `airk-ipc`: `tokio`,
  `serde`, `serde_json`, `sha2`, `anyhow`, `getrandom`, `windows-sys`,
  `libc` [I].
- `client_requests`, `service_registration`, `service_frontend` and the
  `Backend` half of `project_client` stay in the service behind the §5.2 control
  methods. The journal handoff is a contract (§5.2), not a move.

---

# Part B — Listener and path facts (P0.7)

## B1. HTTPS listener start on an empty catalog

**Today [V]:**

1. `serve_with_idle` (`service_runtime.rs:505`) builds `catalog_gate` at
   `:549-596` from `ServiceRegistry::open(root, instance)?.catalog_state()`.
2. `catalog_state` returns `None` when the registry `user_version < 12`
   (`store/src/service/registry/catalog.rs:588-596`). Plain `ServiceRegistry::open`
   creates and migrates the registry only up to version 11
   (`store/src/service/registry.rs:550-890`, `background/audit.rs:53`).
   Version 12 is written by Genesis (`catalog/genesis.rs:694`), by a catalog
   operation and by data relocation. Fresh Genesis activation sets
   `authority_contract=1` (`catalog/genesis/activation.rs:541`) and
   `activated=1` (`:632`).
3. The gate is `None` for: no catalog (`:551-554`, sets `legacy_connections`);
   `authority_contract == 0` (`:568-575`); any non-`AdmissionPending` open
   error (`:594`). It is `Some(NativeCatalogAdmission)` for an activated
   authority-1 catalog (`:576-588`), with no condition on the index count.
4. `http::Owner::start` returns `Ok(None)` without a gate (`service_runtime/http.rs:48-50`).
   The service then runs with the control pipe only and no HTTPS listener
   (`service_runtime.rs:644-653`).

**Fresh pre-split install with an empty catalog:** the staged installer runs
fresh Genesis before the first service probe (`runtime_install/activation.rs:674-678`
→ `catalog_upgrade::activate` → `authority::activate_fresh`, `catalog_upgrade/authority.rs:10`).
The catalog is therefore authority 1 and activated with zero indexes, the gate
is `Some`, and the listener starts [V for the code path; I: confirm with a T0
test that serves a fresh Genesis root with no index].

**A control root without Genesis** (a development `serve` on an arbitrary root
today, or a split install whose setup does not run Genesis) gets no listener [V].
The staged engine owns Genesis today and is removed by the split (§5.3). The
setup role, or the service on first start, must take over fresh Genesis.
Otherwise `/admin` never starts. This duty needs a compatibility-inventory row.

**Admission today [V]:** `Gateway::handle_on_connection` (`http_gateway.rs:291-320`)
→ `dispatch` (`:349`):

- Host/authority check `:358-371`.
- **`Origin` header refused** `:372-374`.
- Exactly one `Authorization` header `:377-383`; `Bearer` token ≤ 103 bytes
  `:384-394`.
- Ledger authentication `:395-440`; delivery fence; checkpoint admit on POST
  `:470-484`.
- Grant validation through `validate` (which builds `http_backend::factory_attached`).

Only then is `HttpReadService::handle_with_context`
(`mcp/src/remote/http.rs:434`) called; it routes `/mcp` (`:502`),
`/api/v1/tools` (`:527`) and `/api/v1/tools/<name>` (`:547`).
`HttpReadService::new` (`:349`) requires a literal `127.0.0.1:<port>`
authority. Every path, including any future `/admin`, therefore needs a bearer
token and is refused if it carries `Origin`, which browsers send.

**Minimal change for always-on `/admin` (proposal [I]):**

1. Run fresh Genesis in setup (or on first start) for split installs, so the
   gate is normally `Some`. This alone makes `Owner::start` bind on an empty
   catalog.
2. In `Gateway::handle_on_connection`, keep the Host check common. Route
   `/admin/…` to the admin dispatcher **before** the `Origin` refusal and the
   bearer extraction. The admin plane applies its own §7.2 rules (session,
   anti-forgery, exact `Origin`). Leave `dispatch` unchanged for `/mcp` and
   `/api/v1`, so their admission is byte-for-byte unchanged.
3. For a damaged registry or legacy catalog (gate `None`): let `Owner::start`
   bind in admin-only mode instead of returning `None`. Make
   `NativeAdmission`/`native_gate` optional (`http_gateway.rs:57-62,96,127,223,287,301`).
   `/mcp` and `/api/v1` then answer `503` with a new typed code before any
   credential work.
   `http_endpoint::installation_binding(&control, clock.namespace)` (`http.rs:56`)
   and the descriptor's `namespace` need a value without a catalog. Use the
   registry incarnation, or publish no MCP descriptor in this mode. This is a
   Phase 1 contract question under U24/U25.

## B2. §9.5 default paths versus every pre-split root (U36)

Pre-split roots (full map with citations: [predecessors.md](predecessors.md) §1).
Additions from this review:

| Item | Windows | macOS | Linux | Source |
|---|---|---|---|---|
| Install, launcher, settings, data, state, cache, logs (both channels, both families) | under `%LOCALAPPDATA%\AIRK\` and `%APPDATA%\AIRK\` | under `~/Library/Application Support/AIRK/`, `~/Library/Caches/AIRK/`, `~/Library/Logs/AIRK/` (`logs-v1[-preview]` for the newer family) | `~/.local/lib/airk/`, `$XDG_{CONFIG,DATA,STATE,CACHE}_HOME/airk/` | `user_paths.rs:257-270,344-436` [V] |
| Inno `standalone` + `runtime\`, `notices\`, `user-path-owner.txt`, user PATH entry `…\standalone\runtime` | `%LOCALAPPDATA%\AIRK\standalone` | — | — | `airk.iss:14,35-36,44,52` [V] |
| Inno AppId `{87B12FF2-FF1A-4E65-AD6C-53CB538E6303}`, Start menu `AIRK Setup` | `{userprograms}\AIRK Setup` | — | — | `airk.iss:9,39` [V] |
| Login definitions | `%APPDATA%\AIRK\services\airk-<hash>.xml` | `~/Library/LaunchAgents/airk-<hash>.plist`; bundle plists `io.iokaio.airk.service.{stable,preview}.plist` | `$XDG_CONFIG_HOME/systemd/user/airk-<hash>.service` | `service_manager.rs:238-249`; `runtime_macos_login.rs:290-291` [V] |
| 1.3 scheduler receipts | `%APPDATA%\AIRK\airk\services\` | `…/Application Support/AIRK/airk/services/` | `$XDG_CONFIG_HOME/airk/services/` | predecessors.md §1 |
| Extension | `<editor user>/globalStorage/iokaio.airk/{cli/bin, service-cli/bin, interface-cache, helpers}`; shell-profile PATH edits (`~/.profile`, `.bashrc`, zsh, fish `conf.d/airk-path.fish`) on macOS/Linux | same | same | `cliInstall.ts:21,143-162,246`; `extensionUpdateLoader.ts:183` [V] |
| Pre-split role image names | `airk-service[.exe]`, `airk-launcher`, `airk-index`, `airk-source` (payload files, not folders) | same | same | `process_roles.rs:19-22`; `runtime_install.rs:1491-1500` [V] |

**Collision and nesting results:**

| §9.5 default | Result |
|---|---|
| Windows `%LOCALAPPDATA%\Programs\AIRK\{bin\airkctl.exe, service\airk.exe, notices\}` | No collision or nesting: it is in a different subtree from `%LOCALAPPDATA%\AIRK` [V]. **Inno risk [I]:** reusing AppId `{87B12FF2…}` makes Inno treat the split as an upgrade. `UsePreviousAppDir` defaults to yes, so it would propose `…\AIRK\standalone` and take over the old uninstall entry. Give the split installer a **new AppId**. The old entry is retired by receipt (predecessors A5). The Start menu name `AIRK Admin` ≠ `AIRK Setup` [V]. The pre-split PATH entry `…\standalone\runtime` (containing `airk.exe`) can coexist until retirement; split `bin\` holds no `airk`. |
| Linux `~/.local/lib/airk-service/{bin/airkctl, service/airk, notices/}` | Sibling of `~/.local/lib/airk/`; no nesting [V]. `airk-service` was a pre-split executable name, never a folder [V]. Image-name matching must stay forbidden (§9.6 exact-image ownership). |
| macOS `AIRK.app` directly in `~/Library/Application Support/AIRK/` | No name collision [V]. It is **nested** in the pre-split parent that holds `service-1.4`, `runtime`, `launcher`, `configuration/`, `data/`, `state/` and `airk/services/` [V], so the plan's "outside every pre-split root" holds only for leaf roots. Two consequences. The U50 rule "chosen folder empty or owned" cannot apply to that non-empty parent. The I4 record "beside the bundle" would lie loose among pre-split items, and the name `airk-install.json` is the pre-split payload manifest (`runtime_metadata.rs:63`). **Proposal:** program folder `~/Library/Application Support/AIRK/program/` containing `AIRK.app` and `install-record.json`. This is still "under" the approved parent, but needs the primary agent's or the user's confirmation as a refinement of the U36 wording. |

**Proposed new folder names (U26)** — "under the existing per-user AIRK roots".
The names are checked against `user_paths.rs` leaves, predecessors.md
leftovers (`*.removed-*`, `.<leaf>.replacement-1`, `.<leaf>.replace-1.lock`,
`standalone`, `airk/services`) and Git history (`git log --all -S`: 0 path uses
of `join("indexes")`, `join("program")`, `install-pointer`, `install-record`) [V].
Defaults follow the U39 grouping: data + state together, cache + logs together.

| Role | Windows | macOS | Linux (XDG respected as today) |
|---|---|---|---|
| Index data | `%LOCALAPPDATA%\AIRK\data\indexes` | `~/Library/Application Support/AIRK/data/indexes` | `$XDG_DATA_HOME\|~/.local/share/airk/indexes` |
| Service state (control) | `%LOCALAPPDATA%\AIRK\state\service` | `~/Library/Application Support/AIRK/state/service` | `$XDG_STATE_HOME\|~/.local/state/airk/service` |
| Cache | `%LOCALAPPDATA%\AIRK\cache\service` | `~/Library/Caches/AIRK/service` | `$XDG_CACHE_HOME\|~/.cache/airk/service` |
| Logs | `%LOCALAPPDATA%\AIRK\logs\service` | `~/Library/Logs/AIRK/service` | `$XDG_STATE_HOME\|~/.local/state/airk/logs/service` |
| Install pointer (U39, fixed, owner-only directory + file) | `%LOCALAPPDATA%\AIRK\install-pointer\current.json` | `~/Library/Application Support/AIRK/install-pointer/current.json` | `~/.config/airk/install-pointer/current.json` |

Notes:

- No proposed leaf equals or contains any pre-split leaf. `service` is not a
  pre-split leaf; the pre-split leaves are `service-1.4[-preview]` and
  `service-v1[-preview]` [V]. Logs move out of `<state>\logs`, so the U39
  cache-and-logs location can be chosen separately. `current.json` is used
  pre-split only as `current.jsonl` inside `logs-1.4/` (`service_logs.rs:24`),
  in a different folder [V].
- The pointer must not depend on chosen locations, and must not depend on
  inherited `HOME`/`XDG_*`/`LOCALAPPDATA`. Native code resolves the known folder
  or the passwd home (`user_paths.rs:109-176,223`; test
  `native_home_ignores_inherited_workspace_home_overrides`, `:649`) [V]. On
  Linux the pointer therefore ignores `XDG_CONFIG_HOME`.
- **Extension caveat [I]:** Node's `process.env.LOCALAPPDATA` and
  `os.homedir()` follow the inherited environment, which native discovery
  deliberately ignores. I4 must specify how the extension resolves the same
  fixed path, for example by refusing a pointer that fails owner-only and
  self-identity checks, before it runs `airkctl`.
- Retirement must never remove the shared parents (`%LOCALAPPDATA%\AIRK`,
  `…/Application Support/AIRK`, `data`, `state`, `cache`, `logs`), which now
  hold split folders too (consistent with predecessors.md §1).
- `preview` channel: U17 allows one service per user, so no `-preview`
  variants are proposed.

## Open items for the primary agent

1. A0/G5: restore the ratchet by hook injection (recommended) or by a reviewed
   baseline update (user decision).
2. G3: own the baseline gate service-side (behavior step under U24) or with an
   injected hook.
3. Option A versus Option B (A3). Option A needs these Phase 1 I1 methods: tool
   call, scoped native read, catalog/help, explorer, and resolve/registration.
4. The CLI help catalog becomes generated data consumed by `airk-mcp` (A3).
5. Move the `native_process` start primitives into `airk-process` for
   `airkctl service start` (§10.1 lists `airk-process` as "unchanged").
6. Fresh Genesis ownership moves to setup or first start (B1); the admin-only
   listener mode and its namespace binding (B1.3).
7. macOS `program/` subfolder and record name (B2). New Inno AppId (B2).
