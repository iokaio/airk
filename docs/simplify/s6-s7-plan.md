# S6/S7 untangling plan and embed-pool decision record

Status: planning record, 9 November 2026, branch `feat/simplify`. The
predictions below come from replaying edge removals against
`scripts/dev/simplify_metrics.py`'s module graph. They are not measurements:
re-measure after each group, and record each step's actual result here.
Measured while the S2 P5/adoption deletion was in progress:

| Crate | Modules | Edges | Largest cycle | Modules in cycles |
|---|---|---|---|---|
| airk-cli | 65 | 277 | 34 | 38 |
| airk-core | 71 | 240 | 31 | 39 |
| airk-store | 29 | 112 | 21 | 21 |

S7.1, S7.2 and S7.3 depend on each other, in that order. S7.4 and S7.5 are
independent of them. Each step is a structural move: no behavior change, no
assertion edits, and test identities and outcomes identical. The boundary
check's cycle baseline (`scripts/dev/boundary-exceptions.json`) shrinks in
the same commit, as a reviewed diff.

## S7.1 CLI os: `native_command` leaf

- Move `windows_argument` (now in `service_manager.rs`) into
  `native_process`, its only user. Putting it in the new leaf would create a
  new 2-cycle.
- The new `native_command` module holds `windows_tool`, `Invocation`,
  `Output`, `execute`, `execute_with_timeout`, `execute_native_program` and
  `execute_native_observation`. Its callers are `http_endpoint`,
  `runtime_macos_login` and `service_manager`.
- This removes `native_process→service_manager` and
  `http_endpoint→service_manager`.
- Predicted: largest CLI cycle 34→28.
- The code is gated to Windows and macOS. Run scoped Clippy on macOS and
  Linux before anything else on those hosts.
- **Measured:** largest CLI cycle 34→28, modules in cycles 38→35, edges
  277→279, top-level modules 65→66. `explorer_assistance`, `native_process`
  and `service_inference` left the cycle. `embedding_pool_bootstrap`,
  `embedding_pool_process` and `unix_guardian` split off into their own
  3-cycle. `windows_argument` also stays used by `service_manager`, which now
  imports it from `native_process`. `MAX_CONFIG` moved with
  `execute_with_timeout`, which bounds input by it; `service_manager` imports
  it. The `manager_command` tests stay in `service_manager::tests`. Contract:
  [`native_command/CONTRACT.md`](../../crates/airk-cli/src/native_command/CONTRACT.md).

## S7.2 CLI layout: `install_layout` leaf

- Move the control-binding core of `runtime_install/layout.rs` into the new
  leaf: the constants, `Layout`, publication, the control
  validation/recording functions, the `registry_binding` child, and the
  `*_root` facades.
- These stay in `runtime_install`: initialization, `successor`, `control()`
  and the baseline gates.
- `read_bounded` gets one owner (`install_files`, whose `CONTRACT.md` is
  updated).
- Persisted receipts (`airk-state-1.4.*`, `Layout`) get exact-byte golden
  captures before the move. Commit a re-runnable move script with `--check`.
- Predicted: `runtime_install` fan-in ≤10; largest cycle 28→21.

## S7.3 Supervisor and catalog control: `checkpoint_fence` read leaf

- Move the read side of `service_runtime/checkpoint.rs` (fence constant,
  types, `fence()`, `admission_closed`, `fence_operation`, bounded reader).
  The writer and `settle` stay.
- Its callers are `project_supervisor`, `service_management`,
  `source_supervisor` and `runtime_install/activation`.
- The fence JSON is persisted (schema 1): capture its exact bytes.
- It only pays off after S7.1 and S7.2: predicted 21→14. Alone it does
  nothing (34→34).

## S7.4 (S6 in place) Core: `read_limits` and `service_records`

- `read_limits` gets the verification byte, deadline and source limits and
  the workbench step-report limit, re-exported from their old paths.
- `service_records`, which depends only on `identity`, gets the shared
  service vocabulary (request key, project registration, worker identity,
  job records, maintenance observation). `service` re-exports it, so other
  crates and the wire are unchanged.
- Check that no serde `type_name` or schema title depends on a module path.
  `fixtures/wire/` bytes must remain identical.
- Under D5, the moved limits join the reviewed fixtures; no mirror is
  generated.
- Predicted: largest Core cycle 31→19.

## S7.5 Store filesystem and lock layer: `store_fs` leaf

- Move the setup-directory handle and file modes, `sync_parent`, the
  ownership-file helpers and `canonical_location` into `store_fs`.
- Move `QuotaPressure` and `PressureScope` down into `capacity`.
- Predicted: largest Store cycle 21→18.
- It contains Unix file-descriptor code: run Clippy on macOS first. It is the
  prerequisite for a later quota/index-store step.

## Deferred, with reasons

- **HTTPS split.** The largest cycle stays at 21, because `http_backend` is
  still tied in through `combined_frontend`, `unified_mcp` and
  `project_client`.
- **`control_client` leaf.** It needs the baseline gate's dependency on
  `runtime_install::state_contract` redesigned first, which is a behavior
  question. Keeping that edge makes the cycle worse (14→15).
- **Read-api, residual installer, quota/index-store.** These come after S7.5.

## §4.1 crate extraction

No candidate meets the criteria today. Every CLI cluster sits in the
34-module cycle. Store and Core candidates sit in their crates' largest
cycles. Only `install_files` has a `CONTRACT.md`; it has no consumer outside
airk-cli (criterion 3), and its T0 time is unmeasured (criterion 4). No
extraction is proposed.

## Embed-pool `include!` and inventory question Q9

Question Q9 in the compatibility inventory asks whether producing historical
fixtures still needs the `airk-embed` helper target. The evidence:

- The workspace version is 1.8.16. The runtime and container builders build
  the helper only for versions below 1.8.14
  (`scripts/build-service-runtime.py`, `scripts/build-service-container.py`).
- `scripts/check-container.py` asserts the helper is absent. The extension's
  bootstrap and `scripts/local-build.py` refuse it.
- `runtime_pair::verify_helper` requires a pair at the current version, and
  no current pair can be built. So the helper-spawn path has no producer.
- Historical runtimes and fixtures come from tagged sources and their own
  build scripts, not from this tree's `embedding-worker` feature.
- The only remaining consumers are tests gated on `embedding-worker`, which
  copy `CARGO_BIN_EXE_airk-embed` into test payloads.

**Answer: no.** The helper is a P9 *producer*, not a consumer duty (see
inventory row `embed_pool_legacy_*`). S4's remaining item, "replace the
embed-pool `include!` scheme", is therefore completed by deletion, not
restructuring:

- Remove the `embedding-worker` feature, the `airk-embed` binary, the
  guarded-provider fixture target and the helper-only sources and tests.
- Turn the remaining single-instantiation `include!` sources into ordinary
  modules.
- Keep the P9 *consumer* duties: `payload_root`, the legacy launchers and
  `quiesce_for_maintenance`.
- Retiring the no-`local-inference` helper-spawn path is a separate,
  compiler-evidenced step.

The checks for this change are the plan's "embed-pool or feature changes"
boundary checks: every remaining feature combination, the runtime builder's
sealed-main/model/import verification, the affected process tests, and a
test-identity comparison in which only `embedding-worker` tests may
disappear.
