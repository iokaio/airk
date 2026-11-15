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
only an evidence Answer with a genuine Context allocation or a canonical
Search with Core's exact `is_ordinary_focus` allocation may carry it. Both
require the original native attempt and actual installed authority. A Focus-only
allocation additionally requires the installed selection's exact scope digest,
matching sole immutable index and live native clock/expiry before storage or
broker admission. Administrative purpose, native-read camouflage and crossing
Search/Answer purposes refuse. The typed child budget must fit its allocation
and parent ceilings. Zero optional-token allocations use the public minimum token
placeholder with no private ordinal allowance, which cannot authorize scoring.
No allocated request executes as a default-budget ordinary query. Ordinary None
and administrative purpose restrictions persist.

The literal fixture `fixtures/split/i1/combined_context_allowance.json` and owning
tests cover absent exact bytes, Some/old strict zero-effect refusal, malformed
and null allocations, and foreign attempt/selection/index binding. Select
`test(project_protocol::combined_context_tests::)` and Supervisor
`test(combined_context_partition_is_refused_before_any_backend_admission)`.
This foundation does not qualify combined Answer execution, models or transports.

Ordinary Focus applicability adds no field or grant. The actual Host resolves a
singleton W4/A2/Open roster before capturing the existing allowance: parent
calls/rounds/fanout one, resources calls one, at most 2048 tokens, 64 candidates
and 65536 receipt-metadata bytes, bounded by its actual original native frame.
Assessment ordinals are empty and cannot authorize Context work. Search
`max_bytes` remains its original text-only contract. Native canonical reply
packing intersects the optional metadata ceiling with actual remaining frame
space; transport frames and source bodies are unchanged. Multi-index and
explicit-subset requests without a real aggregate allocation remain unallocated
ordinary reads. No local per-index view manufactures singleton authority.

Focused selectors additionally include
`test(service_runtime::tools_call::singleton_focus::tests::)`,
`test(actual_singleton_focus_worker_)`,
`test(malformed_focus_allowances_never_fall_back_to_context_or_unallocated_search)`
and `test(singleton_focus_actual_outer_write_)`. These are native Host/worker
admission and delivery controls, not model or installed-transport qualification.
