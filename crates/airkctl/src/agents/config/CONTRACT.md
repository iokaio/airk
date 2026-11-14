# Agent configuration and retained-edit records

Owner: `airkctl::agents::config`. The invoking user's CLI owns configuration
writes; the service, extension and MCP do not. Existing connect/repair/remove
classification, unrelated-byte preservation, linked-file refusal and the 1 MiB
input/output bounds remain in force.

The I2 Entry and its Agent, Scope, Class and Transport data live in
`contract::agent_entry`, with their original decode/schema rules. This module
reexports those same types and retains classification, labels, findings and
editing behavior; contract generation has no dependency on these adapters.

## Retained existing-file publication

The approved [D8 decision](../../../../../docs/split/contracts.md#313-approved-d8-decision-retaining-displaced-agent-configuration)
selects retention of displaced objects. The contract landed before its writer.
`io`, `eligibility`, `retention` and its native adapter now consume the Rust
record owner. Existing-file edits use one retained publication; absent-file
creation keeps the original no-clobber behavior. The service never writes
agent files. There is no native restore/discard command or automatic cleanup.

The writer requires native identity,
privacy, durability, all source/read exclusions and actual Git/package
eligibility exclusions at the target location. Unsafe, unsupported or unproven
paths require explicit refusal/manual editing. Existing root privacy helpers,
this repository's ignore rules and relative `.airk` discovery exclusions alone
do not establish those prerequisites for arbitrary target parents or roots
explicitly selected inside an archive. Never modify user ignore files, adopt
an unmarked subtree or treat a same-user directory mode as an indexing fence.

Admission opens/pins the actual parent and checks handle-based privacy and
mutation policy. On Windows the parent and source file must already be
owner-private; no ACL is changed to qualify them. Write-capable source and
directory handles establish permissions before reading original bytes. Native
volume, Git/package and retention-root ownership, capacity and pending-state
checks also precede source reads. No-op edits may initialize the private
ownership marker but create no transaction or copy.

Source handles are released after capturing the immutable bounded snapshot;
they do not lease or lock the user's file while an edit is prepared. This is
required for ordinary Windows editor `MoveFileEx(REPLACE_EXISTING)` saves, which
refuse an open destination even with delete sharing. Publication still reopens
the live target and compares its full original identity, bytes and metadata
before staging; pinned parent/archive ownership and displaced-object proof are
unchanged.

Initial automatic eligibility requires an existing Git worktree with no tracked
archive entries, effective ignore of the marker and prospective transaction's
fixed artifact names, and explicit disjoint literal npm payload allowlists.
The configured package manager must be the independently probed npm 11.12.1
profile; pack/publish lifecycle hooks, bundled dependencies, workspaces,
unmodeled backends/options, wildcard/overlapping allowlists and source redirect
ambiguity refuse. Account for automatic main, browser and bin entries. Git
commands are bounded local observations without a shell; fsmonitor is disabled
and nonstandard Git environment overrides refuse. Revalidate the current Git
result and exact manifest bytes before retaining source-derived artifacts.
This proves the exercised ordinary configured backend at admission, not later
user overrides, forced staging or copying state into another payload.

The native adapter admits local NTFS, Linux ext-family and macOS APFS profiles
and requires successful directory flush. Only the observed Windows API profile
has preparation measurements here; Unix remains separately uncompiled and
unqualified until its platform checks. Unrecognized volumes refuse. NTFS
alternate streams currently require manual editing rather than silently
bypassing source/retained byte accounting.

The writer syncs prepared data and its prepared receipt, persists/syncs started,
attempts publication once, observes every role even on failure, and syncs data,
directories and the final observation. Windows uses zero ReplaceFile flags;
Unix uses descriptor-relative exchange/swap. Never fall back to overwrite
rename/copy-delete. Receipt updates use the owned private receipt object;
interrupted/incomplete metadata is manual evidence, never replay. Name binding
is checked before publication and before acknowledging success. Directory
flush API success is not claimed power-loss qualification.

Known early changes refuse before publication. Late in-place/editor changes
retain the actual displaced object and return a published-conflict result;
deletion remains absence. Typed local errors distinguish refusal, retained
conflict and uncertain outcome. Connect checks admission before enrollment;
later errors retain the selected connection identity in their message. Existing
repair/remove action shapes keep complete error strings and partial outcomes.

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

Under the owner-marker admission lock, count pending reservations with undiscarded
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
The original five `late_config_publication_*` failed diagnostic receipts remain
historical evidence of canonical-path loss. Their approved Option A expectation
change is a distinct test step: the same after-staging interleavings now require
the exact proposal at the target and exact displaced user edit in its original
private typed transaction, with owner/root/parent/native-object identity and
snapshot hashes checked. Deletion and absent-file no-clobber still refuse;
no-op publication retains the exact original object. Isolated fixtures satisfy
the actual private/Git/package admission policy. This never relabels the
original canonical-path failures or establishes unrun platform qualification.

T0 selector, for the primary Cargo owner:

```console
cargo nextest run --profile ci -p airkctl --lib --locked --timings -E 'test(retained_edit_) | test(config_retained_) | test(late_config_publication_)'
```

Run scoped Clippy before the test batch. Actual retained-object identity,
native failure phases, durable interruption, concurrent/open-writer behavior,
privacy, direct/root/auxiliary ingestion, public reads, Git/package eligibility
and caller conflict/unknown presentation require their identified native,
transport and platform evidence. Scoped tests do not establish a release.
