# `airk_core::read_limits` contract

**Owner:** `airk-core`. **Introduced:** step S7.4 of the
[S6/S7 untangling plan](../../../../docs/simplify/s6-s7-plan.md), 9 November
2026.

## Purpose

`read_limits` is the leaf module for the read limits that combined reads,
saved-byte verification and workbench plans all check:

| Constant | Value | Bounds |
|---|---|---|
| `MAX_SOURCES` | 256 | sources one verification may read |
| `MAX_BYTES` | 1,073,741,824 | bytes one verification may read |
| `MAX_DEADLINE_MS` | 30,000 | one verification's deadline |
| `MAX_STEP_REPORTS` | 72 (`6 * 12`) | step reports in one workbench result |

Before S7.4, the first three lived in `verification` and the last in
`workbench`. `combined_reads` imported `verification`, and `verification`
imported `workbench`, only for these numbers, which kept all three in the
large Core cycle.

## Interface

The module itself is private. The constants are `pub` and re-exported
unchanged from their original paths: `airk_core::verification::{MAX_SOURCES,
MAX_BYTES, MAX_DEADLINE_MS}` and `airk_core::workbench::MAX_STEP_REPORTS`.
Other crates and tests keep using those paths. Inside Core, production code
names `crate::read_limits::<constant>`.

## Owned state and invariants

- **The values are contract.** Allocations above them are refused. Changing a
  value is a behavior and contract change, not a structural one.
- Under plan decision D5 ("fixtures first"), these limits are candidates for
  the reviewed `fixtures/wire` set. S7.4 did not add that fixture, because it
  kept `fixtures/wire` byte-identical. Adding it is its own reviewed contract
  step.
- No statics, clock or I/O.

## Dependencies (allowed edges)

The separate [combined context allocation owner](../combined_context/CONTRACT.md)
defines additive nested Answer reservations and global assessment ordinals.
It consumes existing budgets; none of this leaf's constants change.

None. Every other Core module may depend on this leaf; it depends on nothing.

## Conformance applicability

Constants, not operations, so properties P1–P9 of the plan's §4.6 do not
apply.

## Tests

T0, run from the repository root:

```console
cargo test -p airk-core --lib verification
```

The run must select the verification tests, including the step-report bound
cases that use `workbench::MAX_STEP_REPORTS`. `combined_reads` allocation
tests and `crates/airk-cli/tests/combined_reads.rs` check the other limits
through their re-exported paths.

## Entry points

- Source: `mod.rs`, beside this contract.
- The S7.4 call-site rewrite is reproducible with
  `python scripts/dev/s7_4_core_leaves.py --check`.
