# Final SessionState graph metadata collector

Maintained source; no build, live core replacement, policy admission or
qualification. Existing placement-v1 export and all strict/log bounds survive.
The new same-core export is airk_ort_collect_final_graph_metadata_v1. It uses
initialized SessionState through its owning translation unit, never external
opaque-pointer inspection or a public OrtApi table change. Rust owns JSON schema
airk.private.ort-final-graph-metadata/1 and complete/partial evidence.

Limits: encoded metadata JSON <=8MiB within the original16MiB native result;
65536 nodes,262144 values,524288 ordered edges;4096 UTF8 bytes/string;rank16;
64 elements/typed attribute array. Small immutable constants permit at most64
INT32/INT64/BOOL elements from CPU device memory only, with total payload<=1MiB
within the overall encoded budget. Large/GPU/unknown/unsupported values are
explicitly unread. No weight copy, GraphProto materialization, device transfer,
forward, optimizer rerun, or numeric/qualification claim. Required unread data
blocks dependent policy; it does not turn a complete inventory into a permit.

Graphs follow the existing deterministic final SessionState traversal (depth32,
graphs4096); node IDs are final Node::Index within graph; values are final
SessionState OrtValueNameIdxMap IDs. Edges retain ordered explicit, implicit,
output and graph-boundary slots, including absent optional slots. All inputs,
outputs and implicit bindings are emitted. Non-root outer bindings are marked
unresolved rather than guessed from names. Full node identities include EP,
name/domain/operator/version. Consumers derive from the complete edge inventory.
Types preserve tensor element/rank/dim-value/dim-symbol/unknown, and unsupported
non-tensor types explicitly. Attributes retain type and scalar/array values;
unsupported tensor/sparse/graph attributes remain explicitly unread (graph
scopes are independently traversed). Constants come from final SessionState
immutable initializer entries, never from assumed original graph positions.

C ABI records carry only fixed-width scalars and bounded borrowed string spans
valid during a synchronous callback. No C++ object pointers cross the boundary.
Every callback may cancel; exceptions, invalid input, exceeded caps, missing
IDs/child states, inconsistent counts or cancellation preserve partial evidence
and forbid complete=true. Rust checks UTF8, record sequencing, unique graph/node/
value IDs, all referenced endpoints, ordered slots, aggregate limits and exact
encoded JSON bytes. Final node name/domain/operator/provider rows must match the
existing placement-v1 full map as a multiset with graph scope; metadata is not
accepted merely because counts agree. Original deadline applies before/during/
after the collector. Pure fixtures cover malformed/order/duplicate/unknown IDs,
changed EP/roster, CPU vs GPU constants, cap/cancel/error and unknown outer scopes.

The retained R1 complete placement map is a finding only: its selected logs
truncated and its diagnostic did not pass. No cap or assertion is relaxed.

Prepasses retain pointers/references without additional unbounded metadata
string copies. The closed registered provider-name accessor returns a bounded
CUDA/CPU provider string by value; this inherent owning API copy is retained. Per-node attribute count<=64. Cancellation-only zero records check
the original deadline during bounded value/attribute/child prepasses and do not
change evidence counts. Ordered metadata records are serialized and charged in
Rust before append, reserving4096 bytes for the envelope; final exact encoded
size is checked again. Invalid UTF8 attribute bytes are refused, never decoded
lossily. Graph-only diagnostic is a separate mode and exposes selected-log
incompleteness without changing the old placement diagnostic overflow refusal.


The C++ patch is reviewed source, not executed evidence. Rust validation and
diagnostic integration requirements above are acceptance requirements; their
implementation and runtime verification are tracked separately in the execution ledger.
