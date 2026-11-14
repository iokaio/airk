# Private Query context partition

Owner: `airk_host::project_protocol::Query`; inert allocation types and semantics
belong to Core `combined_context` and its CONTRACT.md. Existing read-budget
conversion stays with `airk_host::read_attempt`.

Optional `context_partition` is appended after existing Query fields. Absent
ordinary allocations preserve exact retained bytes. Present null, unknown keys,
invalid schema/resource/minimum/ordinal shapes refuse during decode. Some never
falls back to None. Current matched runtime pairs only: old closed decoders
refuse Some before effects; no invented capability/version or state migration.

The field grants no authority. The typed consumer must match original attempt,
native selection digest and actual installed index, then enforce allocations
and the earliest deadline while retaining parent cancellation/retry/lifetime
guards. Missing metadata is ordinary only without an allocated combined parent.

The actual worker validates Some before storage, broker or backend admission:
only an evidence Answer with the original native attempt and actual installed
authority may carry it. The typed child budget must fit its allocation and
parent ceilings. Zero optional-token allocations use the public minimum token
placeholder with no private ordinal allowance, which cannot authorize scoring.
No allocated request executes as a default-budget ordinary query. Ordinary None
and administrative purpose restrictions persist.

The literal fixture `fixtures/split/i1/combined_context_allowance.json` and owning
tests cover absent exact bytes, Some/old strict zero-effect refusal, malformed
and null allocations, and foreign attempt/selection/index binding. Select
`test(project_protocol::combined_context_tests::)` and Supervisor
`test(combined_context_partition_is_refused_before_any_backend_admission)`.
This foundation does not qualify combined Answer execution, models or transports.
