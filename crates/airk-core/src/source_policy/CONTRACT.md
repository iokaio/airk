# Source policy: live reservation predicates

`source_policy.rs` owns the pure live predicates `source_root_is_reserved` and
`retained_agent_edit_path`. This addition changes no serialized source policy,
root type, digest framing, schema identity or retained-fixture validation.
`validate_root_path` remains syntax-only so historical state remains readable
for diagnostics and recovery.

`source_root_is_reserved` recognizes any ASCII-insensitive `.airk` component
in the complete source root. Selecting a deeper root cannot erase that private
ancestor. `retained_agent_edit_path` recognizes consecutive `.airk` and
`agent-edits` components, including that root and all descendants/versions.
It fences explicit administrative capture independently of source policy,
while unrelated legacy `.airk` administrative inputs keep their existing
explicit authorization behavior. Similar names do not match.

Windows drive/UNC/verbatim path strings split both native slash spellings;
POSIX strings split `/`, preserving literal Unix backslashes. These are lexical
reservation checks, not OS identity/canonicalization or authorization proof.
Native callers must preserve admission, canonicalize metadata as appropriate,
retain no-follow traversal, and refuse unknown native boundaries. Never use
the helpers to follow a redirect or substitute a newly resolved capture path.

The independent component cases live in the separate `source_policy/tests.rs`
test module. The per-platform ingestion and actual public-read tests belong
to their consumers; passing pure cases does not qualify native path aliases,
archive creation, privacy, Git/package exclusions or OS publication.
