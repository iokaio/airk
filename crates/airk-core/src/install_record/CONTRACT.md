# `airk_core::install_record` contract (I4)

**Owner:** `airk-core`. **Introduced:** split plan step C6, 11 November 2026
([split plan](../../../../docs/split/split-plan.md) §4.2 I4, §9.5–§9.6;
[contracts](../../../../docs/split/contracts.md) §5).

## Purpose

Pure definitions and pure checks for the files that describe an AIRK Service
installation:

- `InstallPointer` (`airk-install-pointer/1`): the fixed per-user pointer to
  the chosen program folder (U39), written last; at most 4 KiB.
- `InstallRecord` (`airk-install-record/1`): the record in the program folder,
  naming both executables with digests, the admin asset digests, the chosen
  locations, signing facts and the installing operation; at most 64 KiB.
- `SetupOperation` (`airk-setup-operation/1`): one owner-private request the
  setup role consumes once, valid for at most ten minutes; at most 16 KiB.
- `SetupPhase`: the durable phases of a setup transaction, including the
  uninstall and purge removals and `login_unsupported` (split Phase 7).
- `SetupReceipt` (`airk-setup-receipt/1`): the setup role's receipt
  `<state>/setup/<operation_id>.json`, at most 256 KiB. It binds the
  operation bytes (`operation_sha256`), records each phase as a separate
  step with an outcome code and bounded detail, lists retired items and
  leftovers (U14), and carries a failure and a `closed` flag. Written by the
  setup role (`airk-cli` `setup_role`); read by `airkctl` and later
  installers.
- `ReplacementNotice` (`airk-install-replacing/1`, 1.8.30): `replacing.json`
  beside the pointer, at most 4 KiB, lifetime at most ten minutes, decoded
  against the caller's clock. The setup role writes it before it stops an
  installed service it will replace or remove, keeps it through file
  placement and removes it when the operation completes or fails. While one
  decodes, the extension ends its `airkctl session`, runs no `airkctl` and
  reconnects after it is gone (so an upgrade does not need the editor
  closed); an expired or undecodable notice is not in force.

## Interface and invariants

- Persisted bytes are canonical: one compact JSON line, fields in declaration
  order, then a newline. A decoder accepts only canonical bytes.
- Unknown fields and other schemas are refused. Every refusal maps to
  `installation_unverified` (I2 exit 7) in readers.
- A record must match the pointer's `record_sha256`, `install_id`, `version`
  and program folder; locations are absolute; executable paths stay inside the
  program folder (`/`-separated, no `..`, no absolute path, no backslash).
- `SetupOperation::decode` takes the caller's clock (`now_unix_ms`).
- Portable setup uses the exact `requested_by: "airkctl-portable"`
  discriminator and `installed_by.installer: "portable"`. This additive
  installer value retains the existing I4 schemas, field order and all prior
  fixture bytes; older closed readers refuse the new value. Portable requests
  place an installation with login startup disabled, trust skipped and no
  automatic start. Generic `airkctl` requests do not imply portable ownership.
  Adapters additionally prove the full approved payload inventory and reject
  every extra program file before adopting an already extracted portable
  candidate. Portable ownership never grants Inno, PATH or login ownership.
- No filesystem, ownership, link or signature check lives here; readers
  (`airkctl`, the setup role, the extension) do those and share the fixtures.

## Separate private setup recovery metadata

### Inno original-operation handoff custody

`inno_handoff::InnoHandoff` is a separate private
`airk-inno-handoff/1` record at `<program>/wrapper-handoff.json`, at most
16 KiB. It binds `install_id`, original installing `operation_id`, exact I4
`version`, the closed capability `original-operation/1`, and two existing
file-fact shapes (`path`, `sha256`, `bytes`): `uninstaller` and `data`.
Their exact relative names are `unins###.exe` and matching `unins###.dat`.
It is valid only for that exact validated I4 image with Installer::Inno.
Unknown fields/capabilities, noncanonical bytes, alternate streams/traversal,
zero-sized files, another basename or mismatched I4 identities refuse.

This record does not authenticate itself or grant launch authority. The
native seal-only owner writes it only after the original placing operation
has a successful closed main receipt, a settled Active owner, verified I4
pair/root continuity and the actual C10 uninstaller signature. It observes
the finalized EXE and DAT after Inno has saved/closed its uninstall log.
Inno stores compiled Pascal uninstall code in DAT; the EXE signature/hash
alone never establishes original-operation capability. The CLI additionally
checks owner privacy/nofollow native objects, current actual EXE/DAT hashes,
unique uninstaller selection and C10 before launch. Missing legacy evidence
refuses automated handoff; no version-only or EXE-only fallback is allowed.

The wrapper must qualify its pinned compiler's post-install/finalized-log
timing and compare the sealed files after process exit. Pure Core fixtures
prove shape/binding refusal, not successful Inno removal or log durability.

### Existing payload inventory Rust owner

`payload_inventory::{InventoryFile, InventoryExecutables, PayloadInventory}`
owns the existing `airk-service-payload/1` deserialize shape, structurally moved
from the service setup adapter for CLI portable/removal recovery reuse. Field
names/order, unknown-field refusal and actual raw inventory bytes/digests do
not change. `PayloadInventory::decode` bounds raw bytes to 256 KiB, verifies
their pinned SHA-256 before parsing, caps files at 4096, refuses unsafe or
reserved relative file names and requires both executable entries. It does
not canonicalize or reserialize the inventory before hashing. Filesystem
regular-file, nofollow, ownership, actual image/payload and signatures remain
adapter checks; this pure decoder neither executes nor authorizes adoption.

The approved P2 R06/R08 contract adds `setup_recovery`; it changes no existing
I4 type, field order, digest or decoder. These are private setup records, never
public evidence or a browser/CLI wire shape. Their canonical bytes follow the
same compact declaration-order JSON plus newline rule. Unknown fields,
versions, oversized input and incomplete identity bindings refuse. Each file
has a 256 KiB raw-byte bound before parsing.

- `PublicationReceipt` (`airk-install-publication/1`) is durably written at
  `<state>/setup/publication/<operation_id>.json` before either publication
  write. It binds the exact placing operation bytes and pinned payload
  inventory, the prior validated I4 pair (or absence), and the exact candidate
  pair including timestamps. The candidate matches operation locations and
  installing operation; a repair/upgrade at the same program root keeps its
  install ID. A fresh or moved installation retains the existing writer's
  derivation: the first 16 bytes of `framed_digest(b"airk/install-id/1",
  &[operation_id.as_bytes()])`. The operation ID itself is not the install ID.
- Under the installation lock, only exact prior pointer/prior record,
  prior pointer/candidate record, or candidate pointer/candidate record bytes
  classify as the three possible publication positions. A fresh installation
  starts with both files absent. A moved program additionally requires the
  original program's record to remain exact. A third state, missing formerly
  present file, foreign record or pointer-before-record ordering refuses.
  Classification never authorizes a write: the adapter first validates the
  receipt against the exact admitted operation and verifies every candidate
  field against the actual pinned payload inventory. A valid I4 pair alone
  cannot prove that payload match. Ordinary
  I4 reads remain strict and never consult this recovery exception.
- The writer retains the publication receipt and artifacts after interruption
  or failure. It may retire that operation's publication metadata only after
  the matching main setup receipt closes successfully and its exact candidate
  pair is observed. Never reconstruct a new timestamp, adopt a third record or
  replay a different operation to hide interrupted publication.
- `RetainedInstallation` (`airk-install-retained/1`) lives beside the fixed
  pointer at `retained/<install_id>.json`. Before pointer/record removal, it
  binds the canonical keep-data uninstall operation, its original validated
  I4 pair and four root observations for data, state, cache and logs. It does
  not make the old pointer active. Missing roots stay missing. Present roots
  use an opened native directory identity: Windows volume serial plus a
  lowercase 128-bit file ID, or Unix device/inode. Identity platforms must
  match the original installation's signing platform.
- A retained record admits no reuse by itself. Reuse additionally requires
  the matching successful closed main uninstall receipt, exact operation
  digest/ID/expiry, proven pointer removal then record removal then terminal
  completion, no purge
  phase, and live native observations of the exact roots. Failed, incomplete,
  foreign, purged or path-replaced roots refuse; same paths or owner-private
  permissions alone do not establish continuity. This supports keep-data
  reinstall without adopting arbitrary nonempty folders. Its fresh install ID
  does not change retained index/catalog identities or persisted state bytes.
- Adapters verify owner-private, unlinked files/directories, native root
  identities, current permissions, platform signatures and installation-lock
  ownership before consuming either record. No receipt permits source writes,
  arbitrary-path restoration, changed-root deletion or automatic rollback.
  A root identity is an observation, not authorization. Removal/purge recovery
  must retain the same original ownership proof until every effect is settled.

These definitions and independent fixtures land before service writer/read
consumers. `fixtures/split/i4/setup_recovery.json` covers exact bytes, ordered
publication states, fresh/moved layouts, malformed/foreign identities and
successful versus incomplete/failed/purged removal. Its retained SHA-256 of
`i4_cases.json` proves that the preexisting I4 fixture bytes are unchanged.
Pure contract checks do not qualify filesystem durability, platform ownership,
interrupted installation or retained-data journeys.

### Active setup ownership through wrapper placement

The separately versioned `setup_recovery::active::ActiveSetup`
(`airk-active-setup/1`, at most 256 KiB) lives at
`<fixed pointer parent>/active-setup.json`. It closes the gap between the
native prepare process releasing its lock and the wrapper finishing placement.
It binds the full canonical operation, its digest and original admission time,
the native identity of that authority directory, sequence, target and roots.
Placing targets reuse `PublicationReceipt`; removing targets retain the
original validated I4 pair. Expiry alone never releases this logical fence.

Removal admission callers may carry the existing canonical pointer and record
bytes at the closed names `expected-current.json` and
`expected-install-record.json` beside their private operation. These remain
the existing I4 formats, not new schemas. Before minting a removal owner, the
native adapter compares the full snapshots with the exact current I4 image
under its installation lock. The snapshot alone grants no effect authority.

- Every setup, service-start, login/OS-manager, installed-image admission and
  recovery path must consult the same owner under its appropriate native lock.
  Another operation refuses while it is unsettled, even if its lifetime expired
  or its original process exited. The same exact operation may reconcile its
  existing state; it does not get a fresh key, payload or admission time.
- Stages progress `admitted` → `prepared` → `pair_published` (placing) or
  `removal_applied` (removing) → `settled`. `prepared` owns the entire interval
  in which a wrapper might place files; it is not proof that placement stopped.
  A sequence increases by exactly one, and bound operation, target, authority
  root, prior snapshots and existing root identities never change.
- Selected roots capture program/data/state/cache/logs. Prior snapshots also
  capture the original roots when placing over an existing installation,
  including moved program locations. A root present at admission keeps its
  exact native identity. An absent root is not permission to adopt a directory
  that later appears. Native-created roots bind an exact fresh private staging
  object ID, its parent ID and a nonce before canonical publication; staging
  names derive from the closed root role, operation ID and nonce, never a
  receipt-supplied arbitrary path. Creation evidence can be added only while
  admitted, before the native prepare handoff. Unknown or interrupted creation
  without bound evidence remains a preserved refusal requiring reconciliation.
- Replacing payload files inside the same program root does not replace its
  directory identity. A directory-root replacement additionally prebinds the
  old identity, a fresh staged identity/parent/nonce and exact inventory digest.
  It cannot affect data/state/cache/log root identities. If the staged root
  becomes canonical, the displaced original program root must remain observed
  under its bound identity. A third root is never adopted. Both staged and
  original facts must be reverified before any publication or cleanup.
- The pure live-root comparison checks supplied handle observations against
  selected/prior bindings and any displaced program identity. It does not
  perform filesystem, ownership or signature checks, verify a payload inventory,
  prove process exit, or authorize a rename, purge or rollback. Adapters must
  establish all those facts and reobserve them at the owning fence boundaries.
- Candidate startup is eligible only at `pair_published`, for the exact
  recorded candidate image/install ID when the operation requests start.
  Adapters still check the full pair/inventory, live root continuity, saved Stop
  and role admission. Ordinary starts, OS startup and recovery must not bypass
  an unknown, unverified or mismatched active transaction. A private stage tag
  by itself never grants authority to start an executable.
- Settlement requires the matching closed successful main receipt, ordered
  publication/removal phases and terminal completion, plus actual final native
  pair/removal/root evidence. Explicit purges require reset acknowledgment and
  their own successful phase (before record publication for a reset install,
  after record removal for uninstall). Failure, absent receipts, unknown image
  exit or a stage tag alone never permit clearing the active owner. Preserve
  the original receipt/artifacts and reconcile rather than replay or guess.

The contract-only cases in `fixtures/split/i4/setup_active.json` are independent
inputs for competing prepares, interruption positions, root replacement,
prebound moved/staged program roots, startup refusal and terminal settlement.
No active owner writer, wrapper handoff, startup adapter or automatic cleanup
is implemented by this definition. Existing I4 fixture identities remain
unchanged. Recovery candidates use the existing `airk/install-id/1` framed
derivation for a fresh install; the initial recovery prototype's mistaken
operation-ID equality is corrected before any production recovery writer ships.

```console
cargo nextest run --profile ci -p airk-core --no-default-features --lib --locked --timings -E 'test(install_record::setup_recovery::tests::) | test(install_record::tests::)'
```

Report a nonzero selected count. The later service/adapter step must add real
filesystem interruption, foreign-state refusal and retained-root recovery
tests before platform/artifact qualification.

## Fixtures and T0

`fixtures/split/i4/i4_cases.json`, generated by `fixtures/split/i4/i4_cases.py`
(`--check` exits 1 on drift).

```console
cargo test -p airk-core --lib install_record
```
