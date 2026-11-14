# Combined read contracts and retry decisions

Core owns the inert request/result types and pure decisions in
`combined_reads.rs` and this directory. Native admission, transport lifetime,
catalog expansion and active caller state remain with their service/client
owners. See [combined reads](../../../../docs/combined-reads.md) for the full
result, provenance, budget and delivery contract.

## Retry binding

`proposed_retry` and `validated_retry` validate the canonical metadata of one
failed required member: no successful sibling, publication, delivered ranking,
truncation, uncertain submission or mismatching inner framing permits replay.
This standalone validation does not establish the caller's current authority.

Callers considering replay also use the matching pure binding decision:

- `validated_retry_for_selection` takes the captured `WindowScopeSelection`,
  public `ScopeSelectionObservation` and original arguments. Owner,
  scope identity and catalog revision are exact; the public source revision
  must match the captured selection revision. Nonces and service identities
  stay private. A coherent stale outer/inner response cannot replace the
  captured source.
- `validated_retry_for_catalog` binds the original `ScopedConnectRequest`
  grant/revision, index and projects.
- `validated_retry_for_combined_catalog` binds the original
  `CombinedConnectRequest`. Absent captured index membership conservatively
  refuses automatic replay; the client never fabricates grant expansion from
  a result. Preserve the complete original refusal in that case.

Original arguments must request required members and obey the same selector
relationships as native execution. Explicit indexes are exact. Explicit
projects without indexes may resolve a subset of known permitted indexes;
observed projects may be pruned to a nonempty subset of requested projects.
Invalid, duplicated or widened original selectors refuse. Catalog metadata
clock handling in separate setup envelopes is unchanged; grant revisions and
selection epochs are never relaxed by this binding decision.

Automatic retry is conservatively unsupported for index-only selections with
no project/set identity and for catalog unions whose allowed index membership
was not captured. Such selectors may still be valid native read inputs; the
retry decision retains their original full refusal and does not label them
malformed, invent a project or infer an absent catalog expansion.

A failed binding has no fallback to standalone validation. Passing a decision
does not refresh authority, extend the original deadline, add retry credit or
permit mutation replay. Clients retain one original immutable request and
recheck their live session/selection before repeating it.

## T0

`cargo nextest run --profile ci -p airk-core --lib --locked --timings -E
'test(combined_reads::read_retry::tests::)'`

The independent shared cases are in
`fixtures/wire/read_retry_decisions.json` (standalone `cases` and caller
`binding_cases`). Python materializes reviewed expected values; Rust and
TypeScript validate them independently. Existing catalog result bytes,
persisted formats and digest inputs are unchanged.
