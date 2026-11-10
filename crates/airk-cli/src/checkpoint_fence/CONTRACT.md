# `airk_cli::checkpoint_fence` contract

**Owner:** `airk-cli`. **Introduced:** step S7.3 of the
[S6/S7 untangling plan](../../../../docs/simplify/s6-s7-plan.md), 9 November
2026.

## Purpose

`checkpoint_fence` is the leaf module for the read side of the native upgrade
admission fence, `native-upgrade-checkpoint.json` in a control directory. A
present, valid fence means new work admission is closed for one upgrade
operation.

Before S7.3, the fence lived only in `service_runtime::checkpoint`. The
project supervisor, the source supervisor, service management and the
installer imported the whole service runtime just to ask whether admission
was closed. They now depend on this leaf instead.

These stay in `service_runtime::checkpoint`: the admission `Coordinator`, the
fence writers (`prepare`, `arm_recovery`), `settle`, the immutable packet
writer and the checkpoint packets.

## Interface

| Item | Visibility | Contract |
|---|---|---|
| `admission_closed(control) -> Result<bool>` | `pub(crate)` | `true` when a valid fence exists. Never creates, repairs or removes it. |
| `fence_operation(control) -> Result<Option<Incarnation>>` | `pub(crate)` | The operation that owns a valid fence, read without changing it. |
| `fence(control) -> Result<Option<Fence>>` | `pub(crate)` | `None` when the file is absent. Otherwise reads at most 4,096 bytes and requires schema 1, `control` equal to the canonical control directory, and `service_instance` present exactly when `origin` is `running_service`. |
| `bytes(path, maximum) -> Result<Vec<u8>>` | `pub(crate)` | `install_files::safe_file`, then a read bounded by `maximum`. Also used by the packet writer. |
| `Fence`, its fields, `FenceOrigin`, `FENCE` | `pub(crate)` | The persisted fence. The writer builds it. |

## Owned state and invariants

- **Persisted format.** `Fence` is schema 1 JSON with fields `schema`,
  `control`, `operation_id`, `service_instance`, `origin`
  (`running_service` or `installer_after_verified_exit`), `execution_sha256`
  and `candidate_execution_sha256`, in that order. The installer reads fences
  written by earlier services, so the bytes are frozen. Changing them needs
  plan decision D8.
- **Read only.** Nothing here writes or deletes. Only `settle` in
  `service_runtime::checkpoint` removes a fence, and only for its own
  operation.
- **Fail closed.** An unreadable, oversized, redirected or inconsistent fence
  is an error, never "admission open". Callers that must not block on an error
  choose `unwrap_or(true)` themselves.
- **Error strings are observable.** They are kept byte-identical to the
  pre-S7.3 text.
- No statics, caches or clock. The `Coordinator` keeps its own cached copy.

## Dependencies (allowed edges)

- Allowed: `std`, `anyhow`, `serde`, `serde_json`, `airk_core` (identity
  types) and the `airk-cli` leaf `install_files`.
- **Forbidden:** `service_runtime` and every other `airk-cli` module.
  Re-measure with `python scripts/dev/simplify_metrics.py`.

## Conformance applicability

These are not registered read, control or job operations, so properties P1–P9
of the plan's §4.6 do not apply. Reason: a local persisted marker with no wire
format, request identity or read budget.

## Tests

T0, run from the repository root:

```console
cargo test -p airk-cli --no-default-features --lib checkpoint
```

The run must select the three `service_runtime::checkpoint::tests`, which
read the fence back through this module. Callers' suites also cover it:
`runtime_install::legacy_maintenance::tests` and
`runtime_install::activation::checkpoint::tests`. The tests stay where they
are, because they drive the staying writer.

## Entry points

- Source: `mod.rs`, beside this contract.
- The S7.3 call-site rewrite is reproducible with
  `python scripts/dev/s7_3_checkpoint_fence.py --check`.
