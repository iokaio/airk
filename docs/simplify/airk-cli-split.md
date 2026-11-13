# Splitting `airk-cli` into crates

Status: proposed 12 November 2026 (user request: split `airk-cli` into the
pieces that make the most sense, before 1.8.31). This is structural work
(Track S, `docs/simplify-plan.md` §4.1 and the CLAUDE.md "Simplification and
structural changes" rules). Every step preserves behavior and is checked
against the same tests and golden outputs as the step before. The ledger
records each step.

## Why

`airk-cli` is one crate with 81 top-level modules and about 86,600
production lines. It sits at the end of the serial chain
`airk-core` → `airk-store` → `airk-kernel` → `airk-mcp` → `airk-cli`.

The recorded optimized builds compiled at most 2 crates at a time for about
737 of 863 seconds. Splitting the largest crate lets independent parts
compile in parallel, so a change to one part rebuilds less. Each part also
gets an owner, a contract and a test command it can run in isolation.

## Measured module graph (12 November, `crate::` references, tests excluded)

**One cycle** joins 18 modules, 50,354 lines (58% of the crate):

`runtime_install`, `project_supervisor`, `service_runtime`, `setup_role`,
`combined_frontend`, `catalog_admission`, `project_client`,
`service_management`, `http_gateway`, `unified_mcp`, `service_manager`,
`catalog_coordinator`, `universal_launcher`, `service_frontend`,
`runtime_metadata`, `http_backend`, `usage_client`,
`reference_refresh_control`.

There are two small cycles:

- `embedding_pool_process`, `unix_guardian`, `embedding_pool_bootstrap`;
- `runtime_pair`, `process_roles`.

The other 58 modules are acyclic.

A crate is extracted only when no cycle passes through it (§4.1, criterion
1), so the large cycle is broken in place first.

## Target crates

Each crate becomes a member of the workspace. Dependencies point downward
only.

| Crate | Modules (now in `airk-cli`) | About | May depend on |
|---|---|---|---|
| `airk-installer` | `runtime_install`, `service_manager`, `universal_launcher`, `install_layout`, `install_files`, `runtime_pair`, `process_roles`, `runtime_legacy_origin`, `legacy_startup`, `legacy_processes`, `user_paths`, `setup_role`, `standalone_setup`, `runtime_metadata` | 33k | core, store, process, ipc |
| `airk-inference-host` | `inference_engine`, `embedding_pool_process`, `embedding_pool_bootstrap`, `embedding_pool_client`, `embedding_client`, `unix_guardian`, `service_inference`, `inference_completion_ledger`, `bundled_assets` | 7.5k | core, kernel, retrieval |
| `airk-catalog-control` | `catalog_admission`, `catalog_coordinator`, `catalog_authority`, `catalog_*_control`, `catalog_index_setup`, `catalog_grant_draft`, `catalog_host_context`, `reference_refresh_control` | 8k | core, store, kernel |
| `airk-supervisor` | `project_supervisor`, `project_worker*`, `project_protocol`, `source_supervisor`, `source_worker`, `derived_worker` | 12k | core, store, kernel, mcp, catalog-control, inference-host |
| `airk-frontend` | `project_client`, `combined_frontend`, `unified_mcp`, `service_frontend`, `usage_client`, `session`, `explorer_assistance` | 9k | core, ipc, mcp, supervisor protocol types |
| `airk-https` | `http_endpoint`, `http_gateway`, `http_backend`, `admin_contract`, `admin_plane` | 9k | core, mcp, frontend |
| `airk-service` | `service_runtime`, `service_management`, `service_logs`, `request_journal`, `client_requests`, the remaining service modules | 9k | all of the above except installer |
| `airk-cli` (bin) | `main`, `lib.rs` role dispatcher, composition | small | all |

Module names stay the same inside each new crate. Callers change from
`crate::x` to `airk_<crate>::x`, and visibility widens only for real callers
in another crate.

## Edges that must be inverted before extraction

Each line below is an edge that points from a lower layer up into a higher
one. Each becomes an injected trait or callback owned by the lower crate,
or moves the shared type down. The count is the number of `crate::`
references.

**Installer reaching into the service and frontend:**

- `runtime_install` → `project_client` (2), `service_frontend` (1),
  `http_backend` (1), `service_management` (1)
- `service_frontend` → `runtime_install` (10): either the service frontend
  takes installer types as inputs, or those types move to the installer's
  public interface.
- `runtime_metadata` → `service_runtime` (4), `reference_refresh_control` (1)
- `setup_role` → `service_runtime` (1)

**Frontend reaching into the service:**

- `combined_frontend` → `service_runtime` (2), `runtime_install` (2)
- `usage_client` → `service_runtime` (1)
- `http_backend` → `runtime_install` (3)

**Catalog control reaching into the supervisor:**

- `catalog_coordinator` → `project_supervisor` (1)

**Same crate, so no change needed:** `runtime_install` ↔ `service_manager`
and `universal_launcher`, and `catalog_admission` ↔ `catalog_coordinator`.

## Step 2a: in-place moves (12 November, uncompiled until the primary's check)

Behavior-preserving moves only: code moved verbatim with its doc comments;
only paths, `use` lines and the visibility noted below changed. Back edges
reported by `module_graph.py --groups` went from 12 to 2. The large cycle
shrank from 18 modules (50,354 lines) to 11 (29,500), all of it closed by the
two remaining edges.

| Item | From | To | Notes |
|---|---|---|---|
| `request`, `request_for_version`, `request_for_instance`, `request_for_version_and_instance`, `request_scoped`, `prebaseline_metadata_method`, `request_initialized_probe` | `service_runtime` | new `control_client` (installer group) | `service_runtime` re-exports the four public functions (integration tests use them) and, under `cfg(test)`, the probe and `prebaseline_metadata_method` (now `pub(crate)`) for existing tests |
| `administrative_read` | `service_runtime` | `usage_client` | `service_runtime` re-exports it |
| `NativeCatalogCoordinator` alias | `catalog_coordinator` | `project_supervisor` | it names the supervisor's handle type |
| `NativeMissingSourceEvent` and its impl | `source_supervisor` | `catalog_source_change_control` | `observed` widened from private to `pub(crate)` for its one caller, `source_supervisor` |
| `bridge_installed` | `runtime_install` | `project_client` | no callers in the workspace; public path changes |
| `create_imported_backend`, `ImportedBackend` | `runtime_install::http_legacy` | new `http_backend::imported_legacy` | only `http_backend` constructs it |
| `console_page`, `console_page_role`, `observed` | `service_management` | new `registry_console` (catalog group) | `console_page_role` widened to `pub(crate)` |

In-crate callers now use the new paths. Group changes in
`airk-cli-groups.json`: `installer` now precedes `supervisor` (no edge joins
them in either direction), so `runtime_metadata`, which needs `project_worker`
and `runtime_install`, moved from `service` to `supervisor`;
`control_client` joined `installer` and `registry_console` joined `catalog`.

**Remaining, not a single small move.** Both remaining edges and one edge
the script does not count start in the `airk runtime …` dispatcher
(`runtime_install::commands::run_with`, 58 command arms) and its HTTP setup
handlers (`runtime_install::http_commands`):

- `commands.rs` `SourceScope` → `service_frontend::RootScope::new` and
  `metadata_before` (registry observation owned by the frontend);
- `http_commands::resolved_existing_connection` →
  `http_backend::validate` → `combined_frontend::validate_http_selection`;
- `commands.rs` → `service_runtime::serve` (three arms) and
  `managed_service::serve`; `runtime_install::activation` →
  `service_runtime::checkpoint` (packet read/digest). The script misses these
  because they go through `use crate::{…, service_runtime}`.

Removing them needs either moving the dispatcher and the HTTP setup handlers
above the service group (with `pub(crate)` widening of many private
`runtime_install` submodules) or hooks injected by the caller, and moving
`service_runtime::checkpoint` to a lower module. That is a design choice for
the next step, not a move.

## Step 2b: dispatcher out of the installer, test edges (12 November, uncompiled until the primary's check)

Behavior-preserving moves only; code and test bodies moved verbatim, with
only paths, `use` lines and the visibility noted below changed. Result: 0
back edges in `module_graph.py --groups` (was 3) and 0 in
`module_graph.py --include-tests --groups` (was 14), 0 unassigned modules.

| Item | From | To | Notes |
|---|---|---|---|
| `run`, `run_with`, `install_login_startup`, `connection_observation` (+ its tests) | `runtime_install::commands` | new `runtime_commands` (cli) | `require_login_registration_change` now private; `existing_http_connection` and `setup_http_connection` are re-exported here. Public paths `airk_cli::runtime_install::{run, run_with, existing_http_connection, setup_http_connection}` became `airk_cli::runtime_commands::…`; `main.rs` and `standalone_setup` updated |
| `managed_service` (+ test) | `runtime_install` | `runtime_commands::managed_service` | only the dispatcher calls it |
| `http_commands` (+ tests) | `runtime_install::http_commands` | new `runtime_http_commands` (https) | `service_runtime::agents` calls it, so it cannot sit in `cli`; `credential_header` `pub(super)` → `pub(crate)` |
| `Packet`, `packet_name`, `packet_digest`, `read_packet`, `immutable`, `settle`, `arm_recovery`, `MAX_PACKET` | `service_runtime::checkpoint` | new `checkpoint_packet` (os) | re-exported from `service_runtime::checkpoint` (`MAX_PACKET`, `arm_recovery` under `cfg(test)`); installer callers use `crate::checkpoint_packet`; `immutable`, `MAX_PACKET` widened to `pub(crate)`; `checkpoint_fence/CONTRACT.md` updated |
| `SourceCheckpointObservation`, `ExitBoundary`, `ReconcileReference` | `source_supervisor::checkpoint` | `checkpoint_packet` | the packet's persisted format; `source_supervisor::checkpoint` re-exports the observation |
| `NativeReadFailure`, `require_native_success` | `usage_client` | new `native_read_result` (os) | `usage_client` re-exports both (public paths unchanged); the guarded supervisor fixture now names `native_read_result` |

`runtime_install` widening for the moved dispatcher (`private`/`pub(super)` →
`pub(crate)`): modules `activation` (and `activation::rebuild`),
`http_cleanup`, `http_rotation`, `inventory`, `launcher_repair`, `layout`,
`legacy_caches`, `legacy_launchers`, `legacy_maintenance`, `lifecycle`,
`managed`, `payload_repair`, `replacement`, `retirement`, `snapshot`,
`staging`, `state_contract`, `upgrade_stop`, `tests` (cfg(test)); the
functions, structs, fields and consts they use, and the types in those
signatures (`Intent`, `Receipt`, `Settlement`, `lifecycle::Input`, `Managed`,
`Plan`, `http_rotation::Input`, `Quiescence`, `SelectedPreflight`,
`StartupAttempts`, `BootstrapObservation`, `Selection::schema`). The
installer no longer imports `service_runtime`; its tests call
`control_client::request*` directly (the paths `service_runtime` re-exported).

Test-only edges, each resolved by moving whole tests (bodies unchanged):

| Tests | From | To |
|---|---|---|
| `fresh_layout_…`, `selected_new_layout_…`, `xdg_absolute_policy_…`, `native_user_home_child_fixture`, `native_home_ignores_…` | `user_paths::tests` | `runtime_install::user_paths_tests` (re-exec name updated to `runtime_install::user_paths_tests::native_user_home_child_fixture`); `Channel::leaf`, `UserPaths::select_layout`, `xdg`, `tests::fixture_paths` widened to `pub(crate)` |
| `imported_http_union_…`, `imported_http_factory_is_lazy_…` | `catalog_host_context::legacy_binding_tests` | `http_backend::legacy_binding_tests` |
| `http_migration_retains_…`, `legacy_eligibility_observes_…` | same | `runtime_install::http_legacy::legacy_binding_tests` (fixtures `imported_fixture`, `insert_imported_fixture` widened) |
| `predecessor_exclusion_is_exact_…` | `service_inference::tests` | `catalog_admission::provider_exclusion_tests` |
| `more_than_128_native_restarts_…` | `service_inference::tests` | `project_supervisor::provider_restart_tests` (local-inference); `service_inference::tests` is now mounted only with `local-inference`, since every remaining test there needs it |
| `held_inference_keeps_native_status_…` | `inference_engine::pool::owner::tests` | `http_gateway::inference_owner_tests` (local-inference); widened `inference_engine::pool`, `pool::accounting`, `Work`, `State` (+ `sender`, `accounting`), `InferenceOwner::state`, `Accounting`, `ComputeCharge`, `DeliveryCharge`, `admit`, `pool::tests::{fixture, open}` |
| `login_registration_changes_refuse_…`, `standalone_adoption_commands_refuse_…` | `runtime_install::tests` | `runtime_commands::dispatch_tests` (`issuer_gate_fixture`, `unselected_fixture` widened) |

Group changes: `project_protocol`, `source_protocol`, `session_protocol`,
`embedding_pool_protocol` (wire types with no upward references),
`catalog_test_fixture` (a store-only fixture with no crate-module
references; its lowest user is `embedding_pool_process`), `checkpoint_packet`
and `native_read_result` joined `os`; `runtime_http_commands` joined
`https`; `runtime_commands` joined `cli`.

**Left for extraction.** The script counts text, so it accepts tests that
call `#[cfg(test)]` helpers in a lower group (`catalog_test_fixture`,
`user_paths::tests::fixture_paths`,
`catalog_host_context::legacy_binding_tests`, `runtime_install::tests`,
`inference_engine::pool::tests`). `cfg(test)` does not cross a crate
boundary, so extraction needs those helpers behind a test-support feature.
`docs/service-installation.md` still links to
`runtime_install/http_commands.rs`.

## Order (one step at a time; each compiles and passes before the next)

1. **Measurement script.** Commit the module-graph script
   (`scripts/dev/module_graph.py`) so every step can be re-measured.
2. **Invert the edges above in place**, one group per commit: installer,
   then frontend, then catalog. Behavior is unchanged. After each group,
   re-measure; the goal is no cycle crossing the target boundaries.
3. **Extract leaf crates first:**
   - `airk-inference-host` (once its small cycle is internal);
   - `airk-installer`;
   - `airk-catalog-control`.
4. **Extract the middle crates:** `airk-supervisor`, then `airk-frontend`,
   then `airk-https`.
5. **Last:** `airk-service` and the thin `airk-cli` binary.
6. **After each extraction:**
   - scoped Clippy in both configurations;
   - the affected crates' library tests;
   - the integration targets that start services;
   - `--timings` recorded.

   Large moves are generated by a committed script that a reviewer can
   re-run.

## Not changed

No wire shape, digest input, persisted format, CLI help text or behavior
changes. The binaries `airk` and `airkctl` and their names stay the same.
CI keeps `[profile.release]`.

## Step 3a: `airk-host` extracted (12 November 2026)

`python scripts/dev/extract_crate.py --group os` moved 26 modules (37 files) into `crates/airk-host`.
`airk-cli/src/lib.rs` re-exports each with its original visibility, so every path is unchanged.

**Build script split.** The embedded-model table moved to `airk-host/build.rs` (it feeds `bundled_assets`).
The macOS login helper and the admin-asset sealing stay in `airk-cli/build.rs`.

**Visibility.** `scripts/dev/widen_visibility.py` widened only the definitions the compiler reported
(E0603/E0616/E0624/E0451/E0364/E0365). Five more were done by hand:

- `native_command`'s grouped `pub(crate) use` from `airk-process`;
- `WorkerStderr::start`;
- two signature types, `Installed` and `Guard` (`private_interfaces`);
- `fs_fixture_sentinel`, now `#[cfg(test)]` (it is used only by a `#[test]` in its own module).

**Test helpers.** Items that `airk-cli`'s tests use became `cfg(any(test, feature = "test-support"))`, and
`airk-cli` enables `airk-host/test-support` as a dev-dependency only. The items:

- `user_paths::tests` and `xdg`;
- `service_configuration::input`;
- `native_command::{execute_native_program, execute_with_timeout}`;
- `native_process::{without_error_dialogs, attach_owned}` (the `local-inference` test path);
- `process_diagnostics`'s stderr capture.

**Orphan rule.** `ConfigurationReads` (in `airk-host`) can no longer be implemented for `Path` in `airk-cli`.
The implementation moved to a local `client_requests::ControlRoot(&Path)`.
`airk_cli::service_configuration` is now a facade: a glob re-export plus a local `prepare(control: &Path, …)`.
Every caller, including `tests/unregister_native.rs`, compiles unchanged.

**Evidence.**

- Clippy `-p airk-host -p airk-cli`: default and `local-inference` both pass.
- Library tests, compared by identity with `scripts/dev/compare_test_sets.py` against step 2b:
  - default: 1070 tests = 1010 `airk-cli` + 60 `airk-host`, missing 0, added 0, changed 0;
  - `local-inference`: 1109 tests, all identical.
- `airk-host`'s own tests run in 1.9 s.
- Integration targets compile (Clippy `--all-targets`); running them is deferred to the end of the split.
