# `airk_cli::install_layout` contract

**Owner:** `airk-cli`. **Introduced:** step S7.2 of the
[S6/S7 untangling plan](../../../../docs/simplify/s6-s7-plan.md), 9 November
2026.

## Purpose

`install_layout` is the leaf module that reads, validates and records the
durable binding between an installation, its service state and its control
directory:

- the installation's layout receipt (`runtime-layout.json`, the `Layout` type);
- the state ownership claim (`airk-state-1.4.*`);
- the control initialization receipt (`airk-control-1.4.*`);
- the registry namespace pin (`airk-registry-1.4.*`, `registry_binding`);
- the read side of the recovery handoff receipt
  (`rebuild-layout-<operation>.json`, `successor_receipt`).

Before S7.2, all of this lived in `runtime_install::layout`. Every service,
catalog, HTTP and log module that checked a control directory imported the
installer hub just to call `validate_control_root` or `log_parent`. Those
callers now depend on this leaf instead.

These stay in `runtime_install::layout`: `planned_state`, the `initialize*`
functions, `prepare_managed_control`, `control()` and the successor writer
(`admit_successor`, `preflight_clean_break`, `successor_handoff_started`).
The baseline gates (`require_baseline_*`) stay in `runtime_install`.

## Interface

All items are `pub(crate)`; there are no callers outside `airk-cli`.

| Item | Contract |
|---|---|
| `validate_control_root(control)` | Validates the control binding, then a managed origin's settled genesis, then a settled data relocation. Never creates state. |
| `validate_control(control)` | Control binding validation only. Must run before any directory creation. |
| `record_control_root(lease)`, `record_control(lease)` | Under the singleton lease, records the control receipt and begins the registry pin. Idempotent; resumes only an owned prefix. |
| `record_registry_root(lease, namespace)`, `record_registry(..)` | Completes the registry pin with the opened namespace. |
| `log_parent(control)` | The verified log directory for a control namespace; the control directory itself in raw development mode. The log modules reach it through `service_locations::log_parent`, which first applies an installation the service role adopted (U89, the record's `locations.logs`). |
| `saved_control`, `initialization_control`, `retained_parameters`, `retained_private_paths`, `managed_origin`, `require_managed_ready`, `installation_for_control`, `validate_catalog_control`, `issuer_rotation_control`, `inspect`, `abort` | Installer reads of the claimed layout. `abort` removes only an unready `runtime-layout.json`. |
| `Layout`, its fields, `read`, `owner`, `claimed`, `claim` | The layout receipt. `claim` writes the state ownership receipt. |
| `LAYOUT`, `OWNER`, `CLAIM`, `OWNER_PART`, `CONTROL_OWNER`, `CONTROL_PART`, `MAX_LAYOUT` | Receipt names and the 32 KiB metadata bound. |
| `publish_new`, `preflight_layout`, `control_binding`, `require_control` | Shared with the staying initialization code. |
| `registry_binding::{PIN, PART}` | Registry pin receipt names. |
| `successor_receipt::{Transition, read, receipt_name, successor_state, operation_for, require_private_union, MAX_PRESERVED_PATHS}` | The recovery receipt and its validation, shared with the staying successor writer. |

Several names are `pub(crate)` only because the staying tests in
`runtime_install::layout::tests` name them. Prefer the functions above to the
constants in new code.

## Owned state and invariants

- **Persisted formats.** `Layout` (schema 1), the registry pin `Binding`
  (schema 1, framed digest `airk/installed-registry-binding/1`) and
  `Transition` (schema 1, framed digest `airk/rebuild-layout/1`) are
  `airk-state/1` persisted formats. Field names, order and digest inputs are
  frozen. Changing them needs plan decision D8.
- **No fallback.** An interrupted or foreign claim, pin or receipt is
  preserved and reported. It never falls back to another ledger, control
  directory or log location.
- **Exclusive publication.** New receipts are written to a `.part` file and
  published with a no-replace rename (`publish_new`). Only a validated
  predecessor pin is replaced.
- **Error strings are observable.** They are kept byte-identical to the
  pre-S7.2 text.
- No statics and no clock. Each call observes the filesystem and returns.

## Dependencies (allowed edges)

- Allowed: `std`, `anyhow`, `serde`, `serde_json`, `airk_core`, `airk_store`,
  `airk_kernel` (new namespace identity only), `libc` and `windows_sys`
  (exclusive rename), and the `airk-cli` leaves `install_files`, `local_ipc`
  and `user_paths`.
- **Forbidden:** `runtime_install` and every other `airk-cli` module. In
  particular the successor writer, which needs the activation permit, must
  not move here. Re-measure with `python scripts/dev/simplify_metrics.py`.

## Conformance applicability

These are not registered read, control or job operations, so properties P1–P9
of the plan's §4.6 do not apply. Reason: installer-local receipts with no wire
format, request identity or read budget.

## Tests

T0, run from the repository root:

```console
cargo test -p airk-cli --no-default-features --lib runtime_install::layout::
```

The run must select at least 29 tests on Windows and 31 on Unix (one
Unix-only case in each of `layout::tests` and `layout::successor::tests`). The tests stay in `runtime_install::layout`,
because they drive the staying initialization code and some re-run fixtures by
exact name (`runtime_install::layout::tests::state_owner_partial_child_fixture`).

## Entry points

- Source: `mod.rs`, `registry_binding.rs` and `successor_receipt.rs`, beside
  this contract.
- The S7.2 call-site rewrite is reproducible with
  `python scripts/dev/s7_2_install_layout.py --check`.
