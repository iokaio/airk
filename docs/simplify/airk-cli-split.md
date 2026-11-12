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
