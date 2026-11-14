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
- `endpoint::require_private_directory_handle`,
  `require_private_metadata_handle` and `require_protected_parent_handle`
  check the exact already-open object, without resolving a named path.
  Unix uses effective UID and fstat mode (0700 directories, 0600 single-link
  metadata); protected parents permit root/user ownership and reject foreign
  write bits, with the existing root-owned sticky-directory exception.
  Windows queries owner/DACL through `GetSecurityInfo` on READ_CONTROL
  handles. Private objects require this user's owner and no allowed principals
  beyond this user, SYSTEM and Administrators. Protected parents use the
  existing `foreign_write_grant` rule and trusted-owner set. Unknown/null ACLs,
  unsupported ACEs, wrong object types, reparse objects and aliased metadata
  refuse. These functions neither open/traverse nor pin a path, change ACLs,
  establish native identity or prove publication/durability; the caller owns
  those boundaries. They add no record format or archive writer.
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

## Shared I4 filesystem reader

`install_reader::read_metadata` returns only the canonical Core
`InstallationImage` after the same bounded owner-private metadata checks. It
does not construct `Verified`, check executable hashes/signatures, authorize
startup or adopt nonempty roots. Explicit signed repair can inspect damaged
installed images without weakening ordinary `read`, which still performs both
actual image checks and C10. Native admission rechecks exact metadata/root
custody before any effect; candidate execution requires its independent full
payload and signature proof.

`install_reader` owns the bounded pointer/record/executable filesystem reader
formerly in `airkctl::install`. It depends only on Core contracts and the
existing endpoint privacy check. Platform user-folder discovery and signature
policy remain injected by callers; this module starts no process and opens no
registry/catalog state. `Verified`, `Discovery`, `read` and `owned_bytes` are
shared by the CLI and native installed-service entry point.

This extraction preserves decision/function bodies, error text, signature
callback ordering, canonical I4 decoder use and complete result fields. The
existing `airkctl::install::tests` identities/assertions remain unchanged and
exercise this owner through reexports. No recovery exception is added to the
ordinary I4 reader, no stored format changes, and no test fixture is rewritten.

```console
cargo nextest run --profile ci -p airkctl --no-default-features --lib --locked --timings -E 'test(install::tests::)'
```

The filtered run must report nonzero selected tests. This structural step
precedes the manual-start resolver correction and retains its limitations;
platform ownership and artifact execution require their native checks.

## T0

```console
cargo test -p airk-ipc
```


Rooted `private_record::{read_private_at,write_private_at}` use the caller's
OpenedDirectory. Policy is checked on the exact held directory and opened file;
creation is exclusive and owner-private. Bytes are bounded, synced, then
published within the original parent object and the directory is synced.
Windows retains ancestor name pins; Unix uses original dirfds. The API does
not itself authorize replacement of an existing logical record. Setup's
ActiveSetup fence owns that decision. `endpoint::create_root_at` initializes
an owner-private child through the same rooted foundation.

The external `endpoint::create_private_file_at` adapter initializes one exclusive
owner-private file through the pinned Process parent (Unix mode0600, Windows
explicit protected current-user DACL). R10 streams verified payload copies
through this handle; metadata writers retain their final strict policy check.


`install_reader::completed_inventory` is a read-only completed-private-setup
proof adapter. It accepts an exact Core I4 image, reads bounded canonical
settled Active/history and the matching successful original receipt, compares
the authority's native identity and exact candidate pair, then returns only
the original raw inventory digest. Fixed settled owner and archived history
are equivalent evidence; an unsettled owner alone grants none. This grants no
current-image/C10, mutation, process or cleanup authority. R10 requires this
proof and exact raw digest before copying full runtime inventory. Missing proof
preserves legacy artifacts and reports original/manual removal guidance.
