# `airk-ipc` contract (I1 transport)

**Owner:** the `airk-ipc` crate (split plan Phase 2, U27). **Introduced:**
group G1 as the `airk-cli` module `ipc` and extracted as a crate in group G6,
11 November 2026 ([split plan](../../docs/split/split-plan.md) §10.1–§10.2;
[dependency inventory](../../docs/split/inventory/dependencies.md) A4).

## Purpose

The same-user local transport between `airk` and its clients:

- `endpoint`: owner-only control roots and endpoints (Windows named pipes,
  Unix sockets), peer identity checks before any message is read, bounded
  message reads and writes. Platform code lives in `endpoint/windows.rs` and
  `endpoint/unix.rs`, selected by `cfg`.
- `frame`: the 4-byte big-endian length-prefixed JSON codec shared by the
  control, query and session protocols.
- `control`: one framed I1 control request per connection with bounded
  deadlines, the `registry_busy` retry window and the Health identity check.
  The response is read under the method's own frame budget
  (`ControlMethod::response_frame_budget`, contracts §2.8; 64 KiB unless the
  method declares more). The service's baseline-gate policy stays in
  `airk-cli` (group G2).

## Interface

`airk-cli` re-exports the crate as `ipc` and keeps the old paths
`airk_cli::local_ipc` and `airk_cli::service_ipc`, so its callers and tests
are unchanged. Items `airk-cli` calls are `pub`; everything else keeps its
narrower visibility.

## Invariants

- Transport only: depends on `airk-core` alone (edge gate in
  `scripts/dev/boundary-exceptions.json`); no registry, store, kernel or MCP
  dependency. The endpoint helpers take paths, not service leases (group G1b).
- Two legacy pieces remain until their only callers retire with the process
  roles (step H10): Windows named-job security (`endpoint/windows/job.rs`, used
  by the paired embedding pool) and the `q-`/`e-` predecessor child-endpoint
  cleanup in `endpoint.rs`. Delete them with those callers.
- Authentication (owner and peer checks) happens before deserializing a
  message.
- `cargo test -p airk-ipc` builds and runs without the service stack.

## T0

```console
cargo test -p airk-ipc
```
