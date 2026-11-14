# Source admission and private retained agent edits

Owners: `source_scope.rs`, discovery/scan in `lib.rs`, coverage `inventory.rs`
and protected capture in `source_read.rs`. Core owns the shared
[live reservation predicates](../../../airk-core/src/source_policy/CONTRACT.md).
The approved private archive boundary is documented in
[D8 §3.13](../../../../docs/split/contracts.md#313-approved-d8-decision-retaining-displaced-agent-configuration).

Source roots with a reserved `.airk` ancestor refuse before loading ignore
controls or emitting payloads, including explicitly selected roots and aliases
rebased inside the archive. Native metadata canonicalization supplements the
lexical predicate. Coverage inventory and all scans share this admission.
The direct selected-source adapter returns false for reserved roots and
propagates unavailable root boundaries; it never silently treats unknown
metadata as permission to read. Ordinary descendant `.airk` exclusions remain
unreincludeable; similarly named neighboring source folders remain eligible.

Explicit administrative extraction, direct protected input and saved-source
verification reject `.airk/agent-edits` and every descendant/version even when
the caller or permanent policy would otherwise permit the path. This private
archive fence is narrower than source-root exclusion: unrelated legacy `.airk`
assets remain available through already authorized administrative inputs.

Authorization still precedes metadata resolution in `capture_input`. Resolution
is observed solely for exclusion, and never substitutes a newly resolved
spelling for authorized no-follow capture. Repeat the fence after opening and
pinning the existing source traversal, before reading source bytes. Windows
existing ancestor handles exclude delete sharing; Unix traversal keeps openat
directory handles and retains the existing post-read/reopen identity checks.
All naming/probe failures refuse or return an unchecked verification state.
A confirmed private archive returns `OutOfScope`, no hash and zero bytes read.

The original ten Windows diagnostic assertions remain in
`tests/retained_archive_exclusion.rs`; Unix additionally observes a real
canonical symlink alias. Additive tests cover exact `before`, `prepared` and
`receipt.json` roles, authorization precedence, and a Windows dot-name alias
whose canonical equivalence is first observed on that host. No sleeps/retries,
real user config copies or archive writer consumers are introduced.

T0 selectors, for the primary Cargo owner after scoped Clippy:

```console
cargo nextest run --profile ci -p airk-core --lib --locked --timings -E 'test(live_reserved_source_paths_)'
cargo nextest run --profile ci -p airk-ingest --test retained_archive_exclusion --locked --timings -E 'test(retained_archive_)'
```

These gates establish exercised source/capture admission only. Store/Kernel
historical/public-result fencing, actual transport comparisons, all platform
native aliases/filesystems, handle-private archive creation and Git/package
admission remain separate obligations. This change neither enables archives
nor alters source policy formats, digests, user ignore files or publication.
