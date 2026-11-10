# `airk_core::service_records` contract

**Owner:** `airk-core`. **Introduced:** step S7.4 of the
[S6/S7 untangling plan](../../../../docs/simplify/s6-s7-plan.md), 9 November
2026.

## Purpose

`service_records` is the leaf module for the service vocabulary that Core's
catalog, console, progress, maintenance and resource contracts share with the
service protocol:

- project identity: `ProjectBinding`, `ProjectRegistration`,
  `RegisteredProject` and `MAX_PROJECT_REGISTRATION_BYTES`;
- request identity: `RequestKey`;
- worker identity: `WorkerIdentity`;
- job records: `JobKind`, `JobState`, `JobOutcome` and `JobRecord`;
- maintenance: `MaintenanceObservation`.

Before S7.4, these lived in `service` next to `ControlMethod` and
`ControlResponse`, which name almost every Core contract. Each Core module
that only needed a request key or a project binding therefore joined the
large Core module cycle. Those modules now depend on this leaf.

## Interface

The module itself is private. Every item is `pub` and re-exported unchanged
from `airk_core::service`, which stays the public path for other crates,
tests and documentation. Inside Core, production code names
`crate::service_records::<item>`.

| Item | Contract |
|---|---|
| `ProjectRegistration::new(root, data_root, collection)` | Computes `binding` with framed digest `airk-service-project/1` and validates. |
| `ProjectRegistration::validate()` | Paths non-empty, at most 4,096 bytes and free of control characters; collection 1–128 bytes of `[A-Za-z0-9._-]`, not `.` or `..`; binding matches; JSON at most `MAX_PROJECT_REGISTRATION_BYTES` (20,480). |
| `JobKind::code()` | The stable `snake_case` code of each kind. |
| `MaintenanceObservation::validate()` | At most 8 pending incarnations and 4,096 removed files; a successful observation carries a pending count. |
| The other types | Plain serde records. |

## Owned state and invariants

- **Wire and persisted formats.** These types appear in control requests and
  responses, registry rows and receipts (for example `RequestKey` inside
  `fixtures/wire/catalog_receipt.json`). Field names, order, serde tags and the
  `airk-service-project/1` digest input are frozen. Changing them needs plan
  decision D8.
- **No module path in any format.** serde output does not include module
  paths, and no `type_name` or schema title in the workspace names these
  types, so the move left every byte unchanged.
- No statics, clock or I/O.

## Dependencies (allowed edges)

- Allowed: `identity`, `anyhow`, `serde` and `serde_json`.
- **Forbidden:** `service` and every other Core module. Adding one would put
  the leaf back in the Core cycle. Re-measure with
  `python scripts/dev/simplify_metrics.py`.

## Conformance applicability

The items are vocabulary used by registered operations, not operations
themselves, so properties P1–P9 of the plan's §4.6 do not apply here. The
operations that carry them keep their own conformance entries.

## Tests

T0, run from the repository root:

```console
cargo test -p airk-core --lib wire_fixture
```

The run must select the wire fixture tests, which check the reviewed
`fixtures/wire/catalog_receipt.json` bytes, including its `RequestKey`. The
registration, job and maintenance types are covered by their consumers'
suites, for example `catalog_setup`, `service_console`, `rebuild` and the
store registry tests.

## Entry points

- Source: `mod.rs`, beside this contract.
- The S7.4 call-site rewrite is reproducible with
  `python scripts/dev/s7_4_core_leaves.py --check`.
