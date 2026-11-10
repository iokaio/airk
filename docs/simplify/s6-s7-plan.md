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
- **Deviation (approved):** `Layout::read` and `retained_private_paths` call
  the successor receipt reader, so leaving all of `successor` in
  `runtime_install` would have put the leaf back in the cycle (replayed: 29).
  `successor` was split by read and write. The receipt reader (`Transition`,
  `read`, `validate_binding`, `preserved_private_paths` and their helpers)
  moved to `install_layout::successor_receipt`. The writer and admission
  (`successor_handoff_started`, `preflight_clean_break`, `prepare`,
  `capture_private_paths`, `apply`, `admit_successor`) stay. `sync_directory`
  moved to `install_files` next to `read_bounded`. The
  `explorer_assistance` and `universal_launcher` bounded readers stay
  separate, because their error text differs. The duplicate
  `runtime_install::log_parent` facade was removed in favor of the leaf's.
- **Measured:** largest CLI cycle 28→21, modules in cycles 35→28, edges
  279→288, top-level modules 66→67, `runtime_install` fan-in 10 modules.
  Exact-byte goldens of `Layout`, the state and control owner receipts, the
  registry pin and the `Transition` receipt (with their framed digests) were
  identical before and after. SHA-256 of the captured bytes:

  | Golden | SHA-256 |
  |---|---|
  | `Layout` pending | `abdabaeb19ae82f128f84c9d0e6f03ac6756553b4d6c49ffdd01e45ee0c2d85f` |
  | `Layout` ready, and the owner receipts for pending and ready | `e278deac79ec518627e0885fc35b3897176c9f9f24dab5dd4759092c7f003195` |
  | `Layout` managed preview | `75d54b654be7b6787953da9c996ef231090d21917cab933fe7807b28abf9567e` |
  | owner receipt, managed preview | `241b4afdca472f39a43e166f874892eea9610a5d5e47b9efcfbff8ead0601704` |
  | `Layout` and owner receipt without logs | `efa1685abd5ee3f0d3b3e0a80df52d4807320293d83f1f2bf7bfe011c823300d` |
  | receipt name constants and `MAX_LAYOUT` | `7b1a70d4f31af76c3775dd45b1139838c63fda5ea41a93e2dd372ff0c124af8c` |
  | registry pin: none / ready / managed | `de4a20bb…bec62` / `16d416f9…4651` / `a8148c2d…c25c3` |
  | registry pin digests: none / ready / managed | `124e2970…ece9e` / `d68252ba…89c8` / `7d975f8d…21d7` |
  | `rebuild-layout` `Transition` receipt | `a427e893bcdf3e100b269cae2c378c8550c0294628e0b84a11c13637c74ea1f2` |
  | `Transition` framed digest (`airk/rebuild-layout/1`) | `9eb837f974f52b6bcf9aa83838a191b1248c33ca003918677fc8e7036b744812` |

  The tests stayed in `runtime_install::layout`.
  Call sites: `python scripts/dev/s7_2_install_layout.py --check`. Contract:
  [`install_layout/CONTRACT.md`](../../crates/airk-cli/src/install_layout/CONTRACT.md).

## S7.3 Supervisor and catalog control: `checkpoint_fence` read leaf

- Move the read side of `service_runtime/checkpoint.rs` (fence constant,
  types, `fence()`, `admission_closed`, `fence_operation`, bounded reader).
  The writer and `settle` stay.
- Its callers are `project_supervisor`, `service_management`,
  `source_supervisor` and `runtime_install/activation`.
- The fence JSON is persisted (schema 1): capture its exact bytes.
- It only pays off after S7.1 and S7.2: predicted 21→14. Alone it does
  nothing (34→34).
- **Measured:** largest CLI cycle 21→14, modules in cycles 28→24, edges
  288→291, top-level modules 67→68. `catalog_source_change_control`,
  `http_gateway`, `reference_refresh_control` and `source_supervisor` left
  the cycle; `catalog_admission`, `catalog_coordinator` and
  `project_supervisor` split off into their own 3-cycle. The read and write
  sides were not mutually dependent: the writer imports `Fence`,
  `FenceOrigin`, `FENCE`, `fence` and the bounded reader `bytes` from the
  leaf, and the leaf depends only on `install_files`. The callers in
  `runtime_install/legacy_maintenance` (`fence_operation`) also moved to the
  leaf. The tests stayed in `service_runtime::checkpoint`. Call sites:
  `python scripts/dev/s7_3_checkpoint_fence.py --check`. Contract:
  [`checkpoint_fence/CONTRACT.md`](../../crates/airk-cli/src/checkpoint_fence/CONTRACT.md).
  Exact fence bytes were identical before and after. The written fence is
  hashed with its temporary control path replaced by `"<control>"`:

  | Golden | SHA-256 |
  |---|---|
  | Fence, `installer_after_verified_exit` | `d142db07d73a2188ea4503972b78d7e481acf4e13e102c30b0af3bc6ae74aa6c` |
  | Fence, `running_service` | `416ececd27612f56c71582c17a2e8d39d3fe9b312667c51e88f1a281635ac8a0` |
  | Fence written by the writer path | `f11bd78c1471e855e06c28a37e486d77579992474472075dc2fad99374fb734f` |
  | Read-back observations (`admission_closed`, `fence_operation`, instance, round trip) | `3baf8f562bba4bd6f8c5c9b50bc9c4b808a1183e00b587feb31a19ee5c51e0db` |
  | Fence file name | `180ed65db3e4b75b98394ca960da9dc21c68fdd2dbfdfa74c3e3f04f93f58e1d` |

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
- **Measured:** largest Core cycle 31→20, modules in cycles 39→28, edges
  240→244, top-level modules 71→73. `index_catalog`, `inference_provider`,
  `service_checkpoint`, `service_console`, `service_events`,
  `service_inspection`, `service_maintenance`, `service_progress`,
  `service_resources`, `source_policy` and `workbench` left the cycle. The
  remaining gap to the prediction is `catalog_refresh`, which also names
  `service::CombinedConnectRequest`. That type depends on `index_catalog` and
  `scoped_reads`, so it is not identity-only vocabulary and stayed in
  `service`; removing that edge alone would give 19.
- Moved to `service_records` (private module, `pub` items re-exported from
  `service`): `ProjectBinding`, `MAX_PROJECT_REGISTRATION_BYTES`,
  `ProjectRegistration`, `RegisteredProject`, `JobKind`, `JobState`,
  `RequestKey`, `JobOutcome`, `JobRecord`, `WorkerIdentity` and
  `MaintenanceObservation`. `ProjectPage`, `JobError` and the connection types
  stay in `service`. Moved to `read_limits` (private module, re-exported from
  `verification` and `workbench`): `MAX_SOURCES`, `MAX_BYTES`,
  `MAX_DEADLINE_MS` and `MAX_STEP_REPORTS`. Only Core production code was
  rewritten (39 paths); tests and other crates keep the old paths. Call sites:
  `python scripts/dev/s7_4_core_leaves.py --check`.
- No module path reaches any format: there is no `std::any::type_name` or
  `module_path!` use for these types, and the only `schemars` derives are
  airk-mcp's own parameter types. `fixtures/wire/` is byte-identical.
- **Not done:** D5 "the moved limits join the reviewed fixtures" would add a
  new `fixtures/wire` file, which conflicts with keeping `fixtures/wire`
  byte-identical in a structural step. It is left as its own contract step.
- Goldens (exact serde bytes of the moved types and the limit values, through
  the public `service`, `verification` and `workbench` paths), identical before
  and after:

  | Golden | SHA-256 |
  |---|---|
  | `ProjectRegistration` | `c61c051dfa6b0959132ca6e2eab4f0d891f7cd1af5d3539ff3a717f44a4e48ed` |
  | `RegisteredProject` | `22c80acce97a9b76770bdec0a68639a8e3d6684185211e52a7f892824f6e9112` |
  | `JobKind` values and codes | `67986bd4b631bf3db088a516dcabf51053579862b5f87a1d657474435e86a810` |
  | `JobState` values | `9e98b82e0fbbe7976e952cef464e95916735219d70f66a3c6488562fcadec849` |
  | `RequestKey` | `709fb244e604126ee5eb267765ab707b6ea487c81899396550c0173b75c30560` |
  | `JobOutcome` values | `4d32bb4918c64e2aa6483d5c121677b5f222d19760eeb7e6c47ad13b78ea6001` |
  | `JobRecord` | `e27eac47f147d7f534b9608935c771e0a118e3337dce11a7b44212055443af30` |
  | `WorkerIdentity` | `12f09911dbb1b2b93127edd30f3fead43faf388833b0a876171c6355fb207e05` |
  | `MaintenanceObservation` | `74c8182b0b60db59da74476b787fcbfb63e3a824064ef24cba99bf3ee5f55dfe` |
  | Limit values | `65a480cc6e414b1b2e8f110f3e41df9a6483c9fe8ab897d48159ee344d0f1964` |
  | `fixtures/wire/catalog_receipt.json` | `5a4823fc4aad4e887d11189194f3721190c92891f44c41cf8ca14789d7694224` |
  | `fixtures/wire/read_retry_decisions.json` | `1700dbf2a419b68f808fc42b8145817d11b57d72f56b97c71d9dcb7adf21506f` |

## S7.5 Store filesystem and lock layer: `store_fs` leaf

- Move the setup-directory handle and file modes, `sync_parent`, the
  ownership-file helpers and `canonical_location` into `store_fs`.
- Move `QuotaPressure` and `PressureScope` down into `capacity`.
- Predicted: largest Store cycle 21→18.
- It contains Unix file-descriptor code: run Clippy on macOS first. It is the
  prerequisite for a later quota/index-store step.
- **Measured:** largest Store cycle 21→18, modules in cycles 21→18, edges
  112→117, top-level modules 29→30. `capacity`, `durability` and `roots` left
  the cycle. `store_fs` depends only on `capacity` and `managed_collection`.
- Moved: the ownership-file helpers from `service`, `sync_parent` from
  `maintenance`, `canonical_location` with its `exists` helper from
  `service::data_scope` (re-exported there for airk-kernel), and the setup
  directory handle (`SetupDirectory`, `SetupFileMode`, `SetupEntries`,
  `file_identity`, the Unix implementation and the Windows HANDLE
  implementation) from `service::registry::catalog::setup::storage`.
  `QuotaPressure` and `PressureScope` moved into `capacity`; `quota`
  re-exports them, so `airk_store::quota::QuotaPressure` is unchanged. The old
  owners import what they still use, so their submodules are unchanged.
  Call sites: `python scripts/dev/s7_5_store_fs.py --check` (125 paths).
- The Windows setup regressions also drive setup storage internals, so they
  stay at `...::setup::storage::windows::tests`. The old
  `service_catalog_setup_storage_windows.rs` is now only their test host, and
  `native_file_identity`, `nt_result` and `publish_with_hook` became
  `pub(crate)` for them.
- Goldens: ownership files are empty, so no ownership-file bytes exist to
  capture, and the moved code encodes no setup-file bytes. Exact bytes of the
  `QuotaPressure` serde form and Display text, the storage pressure codes, and
  observed ownership, `canonical_location` and `SetupDirectory` behavior were
  identical before and after:

  | Golden | SHA-256 |
  |---|---|
  | `QuotaPressure` JSON, all three scopes | `6ff11b5cf983f7c879cddd25032ca243598e746d90fddce114bce7a8df208a7d` |
  | `QuotaPressure` Display text | `923972ab4ecf203a2878ab92ae3d4690746e91c9184f8b60bd3aa5ca2132b7a5` |
  | Pressure code and downcast | `4092912c89f68fc5586865233f4818b1928aa4b3d1d92c2054ea6685c0bd0cd0` |
  | Ownership-file observations (zero length, lock refusals) | `4c500f3ceef93e5ff8b822f6259ba6bb603b1b1d30b858c65e92c9dbb7a22419` |
  | `canonical_location` (temporary root replaced by `<root>`) | `6dda2e50568553506ea2159813d5046b8a6a9e943eb3a5fea1e7b4f1ec021026` |
  | `SetupDirectory` observations (Windows) | `8da4759f5eb65d39da1cfad3b5858f10d9e2d503f6412bcca2d543272596a4ba` |

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
