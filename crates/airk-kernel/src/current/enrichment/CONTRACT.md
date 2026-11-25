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

## Plain-store semantic publication (24 November 2026)

The current index drains bounded work through the collection writer. It
plans semantic units from retained extraction payloads and checks each
artifact, model and selection against the current collection snapshot.
Identical embedding keys share a vector row. New rows are synced before
publishing units, semantic progress and carried source-control bindings in
one transaction. A reader can see partial coverage between committed batches.
A yielded, paused or failed batch leaves its uncommitted work pending.

## Active-model replacement at the same runtime

The current original MiniLM catalog restores exact publisher graph bytes under
new v3 profile identities and the unchanged current ORT1.30 provider/options.
The former checkpoint515… trained-v2 candidate is not a qualified baseline.

The shared upgrade-pending predicate checks complete active-catalog manifest
admission as well as runtime identity. Both active precisions remain recognized;
new setup still enforces CPU-selected product defaults. A retired same-runtime
manifest cannot advertise Ready or enter semantic search/incremental enrichment.
Bounded authenticated model metadata is sufficient; readiness never opens
weights/tokenizer/graph or reads live sources. The exact actual-owner evaluation
lease remains limited to its normal thread-local sealed manifest.

Only genuine io::NotFound for a captured shared model with a verified matching
runtime owner and sealed graph/tokenizer/license closure that cannot be active
may become Pending when old embedded metadata is absent. Missing possibly-active
metadata, malformed/hash-changed content, aliases, foreign scope and other errors
remain errors. This fallback grants no inference authority or success coverage.

Service ModelSetup calls upgrade_service_semantics/replace_semantic_state:
retained text/facts/locations survive one atomic new graph/cache publication;
forced vector recomputation resumes in existing bounded service turns. Sources
remain read-only, disabled policy remains disabled, and retired vectors/cache
keys cannot mix with the new pipeline. When retained state is unavailable, the
existing reviewed full-rebuild owner handles replacement; no automatic destructive
fallback is added. Backend availability, current data coverage and release/platform
qualification are independent facts. CAL/A6 requires the actual selected new
pipeline and observed full readiness before capture or fitting.

## Pipelined semantic batches and multi-batch service turns (22 November 2026)

These are indexer speed fixes, included in the Answer evaluation images and
not yet adopted for the product. They change no stored bytes. On the
evaluation's samples the graph records, lexical documents, artifact and
source objects and embedding packs were identical with and without them.

- **Pipelining (`update.rs`).** Each bounded batch is selected first. The
  keys that the previous batch will still insert count as registered, and
  the registry and membership limits count them. The batch is then embedded
  on the compute-owner thread while a helper thread inserts the previous
  batch into the graph. The helper carries the worker's owner bindings and
  grant allowance, so its checkpoints observe cancel, stall, expiry and pause.
  Registry choices, vectors, insertion order and occurrences equal the
  sequential order's. The membership work limit can trip up to one batch
  earlier than a sequential count.
- **Multi-batch turns (`service_enrichment.rs`, `enrichment.rs`).** A service
  vectors turn publishes up to 8 bounded 32-artifact batches, each its own
  atomic publication. It starts no further batch after a 120-second soft
  deadline, in place of 1 batch and 30 seconds. A paused turn ends at a batch
  boundary (`paused_at_boundary`), releases its pause wait and returns
  admission instead of parking with its writer and model. Object reclamation
  is deferred to the next turn when the turn yields or pauses. A source hint
  still yields before model admission or after the current batch. Before
  product adoption, this needs a recorded decision that weighs the longer
  unhinted turn against issue #64.
