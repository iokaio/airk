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
- No filesystem, ownership, link or signature check lives here; readers
  (`airkctl`, the setup role, the extension) do those and share the fixtures.

## Separate private setup recovery metadata

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
  install ID, while a fresh or moved installation uses the operation ID.
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
