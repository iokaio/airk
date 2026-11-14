# Agent configuration and retained-edit records

Owner: `airkctl::agents::config`. The invoking user's CLI owns configuration
writes; the service, extension and MCP do not. Existing connect/repair/remove
classification, unrelated-byte preservation, linked-file refusal and the 1 MiB
input/output bounds remain in force.

## Contract-first retention step

The approved [D8 decision](../../../../../docs/split/contracts.md#313-approved-d8-decision-retaining-displaced-agent-configuration)
selects retention of displaced objects. `edit_record.rs` is presently mounted
only in test builds: Rust owns its executable record validation, while no
production reader, writer, recovery command or archive directory is enabled.
Passing record tests does not close the existing late-publication defect.

Before a writer consumes this contract, separately prove native identity,
privacy, durability, all source/read exclusions and actual Git/package
eligibility exclusions at the target location. Unsafe, unsupported or unproven
paths require explicit refusal/manual editing. Existing root privacy helpers,
this repository's ignore rules and relative `.airk` discovery exclusions alone
do not establish those prerequisites for arbitrary target parents or roots
explicitly selected inside an archive. Never modify user ignore files, adopt
an unmarked subtree or treat a same-user directory mode as an indexing fence.

## Versioned private bytes

The reserved layout is
`<target parent>/.airk/agent-edits/v1/<transaction-id>/`. A marker at the owned
`v1` root uses schema `airk.agent-config-edit-root/1`, the current owner, root
directory identity and target parent identity. Its fixed name is `owner.json`.
Never alter an existing `.airk` owner, permissions or unrelated contents.

The transaction owns only fixed names: `receipt.json`, `prepared`, and on
Windows `before`. Receipt schema `airk.agent-config-edit/1` binds the transaction
ID, owner, root/parent identities, exact target basename, operation, original
and prepared object identities/lengths/SHA-256, primitive, state, sequence and
optional post-publication observations. No record accepts a restore/delete
path. Every struct and tagged variant rejects unknown fields; required fields,
canonical hex, duplicate fields, unsupported schema/enum values, mismatched
platforms, different volumes and unsafe basenames refuse. Records use compact
UTF-8 JSON in Rust declaration order, without whitespace or trailing newline.
Both marker and receipt input/encoding bounds are 64 KiB. Independent fixtures
pin exact bytes; changing them requires a recorded format decision.

Native identities are tagged `unix` (`device`, `inode`, unsigned 64-bit values)
or `windows` (`volume_serial`, unsigned 64-bit value, and `file_id`, 128 bits in
canonical lowercase byte-order hex). Windows file IDs represent native
`FILE_ID_128.Identifier` bytes in their returned order, not a path or digest.
Owners are a Unix unsigned 32-bit UID or canonical Windows SID. Identity
allocation and OS qualification are future consumers, not validation claims.

Original/prepared snapshots are distinct regular-file object identities on
the root volume, each at most 1 MiB, with complete SHA-256. Post-observations
are `absent`, `complete` with a bounded snapshot, or `incomplete` with optional
identity/length, bounded observed-byte count and an explicit reason:
`oversized`, `changing`, or `unreadable`. Incomplete observations cannot carry
a complete hash. Oversized observations retain the actual observed length;
an open writer may exceed admission limits and its object must remain intact.
The optional unsigned 32-bit native error preserves a Win32 error or positive
Unix errno, interpreted using the recorded primitive. Error codes alone never
establish publication or lack of effects.

## States and reconciliation

- `prepared`, sequence 0: no post-observations; the future writer must sync
  proposal bytes and this receipt before advancing.
- `publication_started`, sequence 1: no post-observations; persist/sync this
  state before a single native publication attempt.
- `published_retained`, sequence at least 2: target matches prepared snapshot
  and displaced object matches reviewed original at this observation.
- `conflict_retained`, sequence at least 2: target matches prepared snapshot,
  while an identified displaced object differs or is incompletely observed.
  Proposed bytes may already be published; never claim nothing was written.
- `outcome_unknown`, sequence at least 2: required post-observations record
  uncertainty without inferring success, rollback or replay.
- `refused_retained`, sequence at least 2: staged proposal remains, target is
  not the prepared object and Windows has no retained `before` object. These
  are necessary consistency checks; the native owner must still prove refusal.

Windows records `replace_file`, with the displaced object at `before`; Unix
records `rename_exchange` (Linux) or `rename_swap` (macOS), with the displaced
object at `prepared` and no `before` artifact. Publication observations use
these fixed roles. Receipt successors preserve every immutable lineage field
and increment sequence exactly once. Prepared can advance only to started;
started/unknown can advance to an observed state. Published/conflict/refused
can later record conflict/uncertainty, accommodating open-writer changes.
No successor returns to publication admission or authorizes another attempt.

Recovery first validates the entire marker/receipt and exact caller-observed
owner, root, parent, transaction and basename binding. A valid record is still
only evidence: separately reopen/revalidate the live objects. Missing, foreign,
changed or incomplete artifacts never authorize an inferred restore, replay
or deletion. This step adds no recovery filesystem actions. Human review and
explicit OS-level discard remain the only approved restoration/discard workflow;
there is no automatic restore, retention expiry, sweep or uninstall cleanup.

## Admission

Under one future admission lock, count pending reservations with undiscarded
transactions. Refuse a seventeenth transaction. Reserve 2 MiB plus 128 KiB
metadata headroom, require that much observed available space and refuse if
observed contents plus existing reservations plus this reservation exceeds
64 MiB. Overflow refuses. These are admission checks, not a hard disk cap
against external writers. No eviction, truncation or automatic cleanup is
permitted. The pure contract predicate creates no files or locks.

## Evidence and remaining gates

`fixtures/split/agents/config_edits/` contains independently specified Unix
and Windows markers, all six states, native 1177 uncertainty, macOS swap,
retained open-writer growth, deletion refusal, transition, changed-binding,
malformed and count/byte/disk-bound cases. Tests compare exact encoding and
strict validation; they exercise no OS publication and qualify no platform.
The earlier five `late_config_publication_*` observations remain unchanged.

T0 selector, for the primary Cargo owner:

```console
cargo nextest run --profile ci -p airkctl --lib --locked --timings -E 'test(retained_edit_)'
```

Run scoped Clippy before the test batch. Actual retained-object identity,
native failure phases, durable interruption, concurrent/open-writer behavior,
privacy, direct/root/auxiliary ingestion, public reads, Git/package eligibility
and caller conflict/unknown presentation remain separate prerequisite evidence.
