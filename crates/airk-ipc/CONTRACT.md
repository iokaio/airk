# `airk-ipc` contract (I1 transport)

**Owner:** the `airk-ipc` crate (split plan Phase 2, U27). **Introduced:**
group G1 as the `airk-cli` module `ipc` and extracted as a crate in group G6,
11 November 2026 ([split plan](../../docs/split/split-plan.md) §10.1–§10.2;
[dependency inventory](../../docs/split/inventory/dependencies.md) A4).

## Purpose

The same-user local transport between `airk` and its clients:

- `endpoint`: owner-only control roots and endpoints (Windows named pipes,
  Unix sockets), peer identity checks before any message is read, bounded
  message reads and writes, and `require_owner_private` for a file this user
  wrote (Windows: owner and ACL; Unix: regular single-link file owned by the
  effective user without group or other bits; U84), and on Windows
  `require_not_writable_by_others` for a location's parent folder (owner the
  user, SYSTEM or Administrators; no write-type allowed entry for another
  account, by the pure `foreign_write_grant` rule; U96). Platform code lives in
  `endpoint/windows.rs` and `endpoint/unix.rs`, selected by `cfg`.
- `private_record` (split U90): owner-private bounded record files under an
  owner-only root, `read_private`/`write_private` (atomic replace or
  no-clobber), `private_file`, the regular-unlinked-file check
  `require_regular_unlinked`, and an exclusive owner-private lock file
  (`lock`). Moved from `airk-cli` `http_endpoint` by
  `scripts/dev/split_u90b_cert_trust.py` so `airkctl`'s offline trust
  (U75) reads and writes the HTTPS records with the service's checks; the
  error messages and on-disk bytes are unchanged.
- `frame`: the 4-byte big-endian length-prefixed JSON codec shared by the
  control, query and session protocols.
- `control`: one framed I1 control request per connection with bounded
  deadlines, the `registry_busy` retry window and the Health identity check.
  The request is written and the response read under the method's own
  frame budgets (`ControlMethod::request_frame_budget` and
  `response_frame_budget`, contracts §2.8; 64 KiB unless the method declares
  more). `read_message_sized` returns the payload length so the service can
  hold a decoded request to its method's budget. The service's baseline-gate policy stays in
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
