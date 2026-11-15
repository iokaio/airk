# Semantic readiness and graph specification

Readiness uses the existing `SemanticReadiness` owner and `Unavailable` state;
this correction adds no wire fields or persisted format. A saved allocated
graph with an unsupported specification or record width cannot be advertised
as Ready, including when retained semantic-work metadata claims completion.
The native observation explains that index regeneration is required. It does
not start a rebuild, infer that one is queued, or modify historical evidence.

The owning Retrieval functions calculate the expected graph specification and
record width from the authenticated model descriptor. The metadata adapter
reads only the bounded, hash-verified model manifest (at most16KiB), not weights,
a model session, graph records or live source bytes. Existing runtime-upgrade
pending handling and unallocated bootstrap handling retain their order.

Unavailable observations retain the scoped model/selection identity, selected
artifact count and search policy. Completed counts are zero and unit totals and
ready timestamps remain unknown; historical graph counts are not current
coverage. Disabled semantic configurations do not acquire a model identity.

The focused regression exercises an actual current native WordVector Ready
index as a positive control, then the pure metadata projection with an
independently retained specification1 recipe and a malformed record width.
It checks existing Unavailable fields and a disabled-model control. It does not
forge an authenticated old store or establish transport/platform qualification.
