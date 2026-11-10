# `airk_store::store_fs` contract

**Owner:** `airk-store`. **Introduced:** step S7.5 of the
[S6/S7 untangling plan](../../../../docs/simplify/s6-s7-plan.md), 9 November
2026.

## Purpose

`store_fs` is the leaf module for the Store's filesystem and lock layer:

- **ownership files:** the empty, process-lifetime lock files that own a
  service namespace, project, collection manifest or relocation;
- **the setup directory handle:** `SetupDirectory`, a retained directory
  object whose every mutation is relative to that object, never to a logical
  path. It has a Unix (`openat`/`renameat2`/`renameatx_np`) and a Windows
  (`NtCreateFile`, HANDLE-relative) implementation;
- **`sync_parent`:** durable directory-entry changes on Unix;
- **`canonical_location`:** canonical data locations without creating
  directories or accepting traversal.

Before S7.5, these lived in `service`, `maintenance`, `service::data_scope`
and `service::registry::catalog::setup::storage`. Every Store module that
locked a file, synced a directory or opened a setup directory therefore
depended on one of those hubs. `capacity`, `durability` and `roots` left the
Store cycle as a result. In the same step, `QuotaPressure` and `PressureScope`
moved from `quota` down into `capacity`, which `quota` re-exports.

## Interface

The module is private to the crate. Items are `pub(crate)` unless noted.

| Item | Contract |
|---|---|
| `acquire_file(root, name)`, `acquire_file_for(.., timeout)` | Creates if missing, then takes an exclusive lock; waits up to `timeout` in 10 ms steps, then fails with "another AIRK process owns service resource". |
| `acquire_existing_file`, `acquire_existing_file_for` | The same, but never creates the file. |
| `acquire_shared_file(root, name)` | Creates if missing and takes a shared lock without waiting. |
| `open_existing_ownership_file(root, name)` | Opens without locking or creating. |
| `OwnershipFile(File, AtomicBool)` | Unlocks on drop unless the flag marks a guardian pin, which must be released by last close. Both fields are `pub(crate)` because `service` sets the flag. |
| `sync_parent(path)` | Unix: `fsync` of the directory. Elsewhere: no-op. |
| `canonical_location(path)` | `pub`; also re-exported as `service::data_scope::canonical_location` for airk-kernel. Absolute, no `..`, at most 4,096 bytes; canonicalizes the existing prefix and appends the missing suffix. |
| `exists(path)` | `symlink_metadata` presence; a dangling link counts as present. |
| `SetupDirectory`, `SetupFileMode`, `SetupEntries`, `file_identity` | The descriptor-anchored handle. `SetupDirectory::file` is `pub(crate)` because setup storage passes the retained object on. |

Test-only: on Windows, `native_file_identity`, `nt_result` and
`SetupDirectory::publish_with_hook` are `pub(crate)` so the setup storage
regressions, which stay in
`service::registry::catalog::setup::storage::windows::tests`, can reach them.

## Owned state and invariants

- **Ownership files are empty.** Opening refuses a non-empty, linked,
  aliased or (on Windows) reparse-point file. Nothing here writes bytes into
  them. On Unix they are created `0600` with `O_NOFOLLOW`.
- **Never delete a lock inode.** Releasing ownership unlocks and closes; it
  never removes the file.
- **Descriptor-relative mutation.** `SetupDirectory` resolves only single
  child names against a retained directory and refuses `.`, `..`, separators
  and NUL. Publication is atomic no-replace where the platform provides it;
  Unix replacement is a name operation under the caller's exclusion, which the
  method docs state.
- **Error strings are observable.** They are kept byte-identical to the
  pre-S7.5 text.
- No statics. The only clock use is `Instant` in the bounded lock wait.

## Dependencies (allowed edges)

- Allowed: `std`, `anyhow`, `libc` (Unix), `windows_sys` (Windows),
  `capacity` (free-space observation for `SetupDirectory::capacity`) and
  `managed_collection` (alias refusal for ownership files).
- **Forbidden:** `service`, `quota`, `maintenance` and every other Store
  module. Re-measure with `python scripts/dev/simplify_metrics.py`.

## Conformance applicability

These are not registered read, control or job operations, so properties P1–P9
of the plan's §4.6 do not apply. Reason: a local filesystem layer with no wire
format, request identity or read budget.

## Tests

T0, run from the repository root:

```console
cargo test -p airk-store --no-default-features --lib -- service::tests service::existing_owner_tests service::collection::tests service::registry::catalog::setup::storage
```

The tests stay with the code that drives them, so their identities did not
change in S7.5. The run must select the ownership-file tests in
`service::tests` and `service::existing_owner_tests`, the manifest-lock tests
in `service::collection::tests`, and the setup storage regressions: the
Windows ones in `...::storage::windows::tests` on Windows, and the Unix ones in
`...::storage::tests` on Unix.

## Entry points

- Source: `mod.rs` (ownership files, `sync_parent`, `canonical_location`),
  `setup_directory.rs` (the handle and its Unix implementation) and
  `setup_directory/windows.rs` (the Windows implementation), beside this
  contract.
- The S7.5 call-site rewrite is reproducible with
  `python scripts/dev/s7_5_store_fs.py --check`.
