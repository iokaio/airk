# `setup_role` contract (I4 writer; split Phase 7)

**Owner:** `airk-cli` (`crates/airk-cli/src/setup_role/`). **Introduced:**
split plan Phase 7 ([split plan](../../../../docs/split/split-plan.md)
§9.1–§9.6, §10.2; decisions U13–U16, U20, U30–U33, U36, U39, U40, U50, U78,
U82; [contracts](../../../../docs/split/contracts.md) §5.5–§5.7).

## Purpose

The private roles of the signed `airk` image that install, repair, upgrade
and remove an AIRK Service installation (U33: no third executable):

- `airk __airk_role setup --operation <absolute path> [--plan | --prepare]`
  consumes one `airk-setup-operation/1` file and runs the §5.6 transaction.
- `airk __airk_role service-login --install-id <32 hex>` is the entry the
  login registration starts.

## Interface

| Mode | Runs | Writes |
|---|---|---|
| `--plan` | Read-only disclosure (U13): predecessors found, items that will be left, location problems, the existing installation | `plan.json` and `plan.txt` beside the operation file; stdout = `plan.json`. No lock, no receipt; the operation is not consumed |
| `--prepare` | Before file placement: lock, location checks, predecessor retirement, service drain, wait for installed images to exit | Receipt (open); stdout `airk-setup-result/1` with `status: awaiting_placement` |
| (none) | Every remaining step | Receipt (closed); stdout `airk-setup-result/1` with `status: complete` |

Removal kinds (`uninstall`, `purge`) take no `--prepare`.

The payload inventory `payload-inventory.json` (`airk-service-payload/1`,
written by `scripts/build-local-service-installer.py`) lists every placed
file (`/`-separated program-relative path → `sha256`, `bytes`), the two
executables, the admin asset digests, the signing identity (or `null`),
version, channel and target. Its SHA-256 is the operation's
`payload_inventory_sha256`. With `--plan`/`--prepare` it is read beside the
operation file; otherwise from the program folder.

Admin asset digests (I4 `admin_assets`) frame every embedded asset in name
order with `airk_core::identity::framed_digest`: `inventory_sha256` over
`airk/admin-asset-inventory/1` and each name, SHA-256 and content type;
`bundle_sha256` over `airk/admin-asset-bundle/1` and each name and bytes
(`admin_plane::embedded_asset_digests`). This is the interim definition until
C7's `admin-assets.json` producer lands.

## Authority (before any state is opened)

1. The operation file is a regular, unlinked, owner-private file in a folder
   without links; at most 16 KiB; canonical `airk-setup-operation/1`.
2. First admission: within its lifetime (≤ 10 minutes) by the injected clock.
   Resume: the receipt `<state>/setup/<operation_id>.json` binds the exact
   operation bytes (`operation_sha256`); the operation is decoded against the
   admission time the receipt recorded. A closed receipt refuses a replay
   (`setup_operation_consumed`); other bytes are `setup_operation_changed`.
3. Own image: its SHA-256 equals the inventory's `airk`; in the complete
   mode it runs from `<program>/<executables.airk>`; for a removal with an
   installation at the same locations, it is that record's `airk`.
4. One operation per user at a time: the owner-private lock
   `install-pointer/install.lock` beside the fixed pointer
   (`installation_in_progress`).

Single use: the operation file (and its folder's inventory and plan copies)
is deleted when the receipt closes. A failure closes the receipt with
`failure`; an interruption leaves it open for a resume.

## Transaction (§5.6)

Pure in `plan.rs`, tested against `fixtures/split/i4/interruptions.json`.

Placing kinds (`install`, `repair`, `upgrade`): `lock` → `retire_predecessors`
→ `drain` → [`--prepare` stops] → `verify_placement` → `write_record` →
`write_pointer` → `register_login`? → `start`? → `apply_trust` → `close`.

Removing kinds: `lock` → `drain` → `unregister_login` → `remove_trust` →
`remove_pointer` → `remove_record` → `purge_data`? → `close`.

- `lock` validates locations every run. Placing kinds (U39/U50): absolute,
  below a top-level folder, no links, no nesting between the five locations,
  no overlap with any pre-split root (both channels and layout families of
  `user_paths`, the pre-split Inno `standalone` folder) or the pointer folder;
  each existing folder (or nearest existing ancestor) is owned by this user
  and not writable by others; each chosen folder is absent, empty, the
  existing installation's, or (program, complete mode) holds only inventory
  files, the record, the inventory and Inno's `unins###.{exe,dat,msg}`.
  Removing kinds: the operation's locations equal the installation's.
- `retire_predecessors` (U13/U14): every existing pre-split install root is
  planned and retired through the existing receipts-only
  `runtime_install::replace_installation` (operation identity
  `framed_digest("airk/split-setup-retirement/1", operation_id)`, candidate =
  the payload inventory digest). U13 acknowledgements are required when any
  root needs removal. A root that cannot be retired is a leftover with its
  reason; a leftover that can restart an old service is listed first. The
  pre-split Inno `standalone` folder is reported, not removed (U74 pending).
- `drain`: for the operation's and the existing record's control roots, a
  `Health` from this user's service whose reported image SHA-256 is the
  existing record's or the payload's `airk`, then `Shutdown` fenced to its
  instance and the exit of its process (60 s). Before placement or removal,
  every process running exactly an installed image (except this one) must
  exit by itself within 15 s (`installation_files_in_use`); nothing is
  terminated. Re-observed until `placed` (or `login_unregistered`) is durable.
- `verify_placement`: every inventory file matches size and SHA-256; version
  equals this `airk`; the placed `airkctl --version` reports the same
  version; admin digests equal the embedded bundle's; the inventory names a
  signing identity (`payload_unsigned` otherwise). Authenticode and
  `codesign` are not re-verified here.
- `write_record`: `install_id` is kept for the same program folder, otherwise
  derived from the operation (`framed_digest("airk/install-id/1",
  operation_id)`), so a resume rewrites the same record. Owner-private,
  temporary file + rename.
- `write_pointer`: last of the placement steps, only for a record the I4
  reader accepts; owner-only folder, temporary file + rename + folder sync.
- `register_login` (Windows Task Scheduler, Linux systemd user unit) through
  `ServiceManager::for_installation` (`airk-service-<hash>`, definition and
  receipt under `<state>/login/` on Windows): registered without starting.
  macOS records `login_unsupported`. A failure is recorded
  (`login_failed`), not raised.
- `start`: the installed `airkctl --output json service start` (U94, one
  start implementation). A failure is recorded and reported when the receipt
  closes (`service_not_healthy`, non-zero exit), never hidden.
- `apply_trust` after the start (the HTTPS issuer exists only after the
  service's first start; a recorded deviation from the §5.6 table order):
  `skip` → `trust_declined`; Linux → `trust_unsupported`; otherwise
  `http_endpoint::trust::manage(Install)` with the OS consent dialog; no
  issuer yet or a failure → `trust_declined` with the reason.
- Removals: owned login registration, owned trust (its receipt) and the
  pointer (only when it names this installation) and the record. Index data,
  state, cache and logs are deleted only by `purge` or `delete_indexes:
  purge` (U32; `ask` keeps); the state folder keeps `setup/`; a location
  containing the program or pointer folder is refused.

## Not implemented (reported, not hidden)

macOS SMAppService login and the `AIRK.app` layout (U40/U78); removing a
moved program folder; program-file removal for `airkctl service uninstall`
on macOS/Linux; U74 roster widening (Inno `standalone`, extension CLI copies
and PATH, 1.3 schedulers beyond what `replace_installation` covers); the
service consuming the record's `data`, `cache` and `logs` locations (U89).

## T0

```console
cargo test -p airk-cli --lib setup_role
cargo test -p airk-cli --lib role_admission
cargo test -p airk-core --lib install_record
```
