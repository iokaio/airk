# `ipc` contract (I1 transport)

**Owner:** `airk-cli` until the `airk-ipc` crate is extracted (split plan
Phase 2, group G6; U27). **Introduced:** group G1, 11 November 2026
([split plan](../../../../docs/split/split-plan.md) §10.1–§10.2;
[dependency inventory](../../../../docs/split/inventory/dependencies.md) A4).

## Purpose

The same-user local transport between `airk` and its clients:

- `endpoint`: owner-only control roots and endpoints (Windows named pipes,
  Unix sockets), peer identity checks before any message is read, bounded
  message reads and writes. Platform code lives in `endpoint/windows.rs` and
  `endpoint/unix.rs`, selected by `cfg`.
- `frame`: the 4-byte big-endian length-prefixed JSON codec shared by the
  control, query and session protocols.

## Interface

Unchanged by G1. The old public paths `airk_cli::local_ipc` and
`airk_cli::service_ipc` re-export `ipc::endpoint` and `ipc::frame`; callers
move to the new paths in later groups.

## Invariants

- Transport only: no application policy, no registry, store, kernel or MCP
  dependency. Groups G1b–G4 move out the remaining service-only pieces
  (embedding-pool job security, lease-taking endpoint helpers, predecessor
  child-role policy) before extraction.
- Authentication (owner and peer checks) happens before deserializing a
  message.
- This module is in no top-level module cycle
  (`python scripts/dev/boundary_check.py`).

## T0

```console
cargo test -p airk-cli --lib --no-default-features --features local-inference ipc::
```
