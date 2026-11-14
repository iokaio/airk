# Whole-selection context allocations

Owner: `airk_core::combined_context`. Execution and authority remain with the
native frontend, worker and local Kernel. This is the reviewed option B;
no persisted state, existing digest domain or public Answer argument changes.

One original attempt, selected-scope digest and full canonical immutable-index
roster own every partition. A `PartitionAllowance` binds those facts without
granting authority. Consumers must still check the actual installed worker,
grant, publication, original read-attempt clock/deadline and cancellation.
Missing metadata cannot downgrade an allocated parent to singleton defaults.

Calls, model input tokens, retained candidate units and bytes are additive.
Initial quotient/remainder reservations follow canonical member order, admitting
only members with at least one call/candidate and 1024 bytes. Other authorized
members remain explicitly unqueried. Literal independent examples cover two
members/eight calls/2048 tokens/three assessments, four members/three assessments
and more members than calls. Assessment ordinals are globally unique
1..min(3, requested rounds), assigned in execution order. Empty allowance means
zero optional tokens while deterministic reads remain possible. Public rounds
stays 1..4. Existing read-depth/fanout maxima retain their maximum/peak meaning;
the first consumer runs one member packet at a time. No per-member budget reset.

`CombinedContextSummary` supplements complete combined member bodies, origins,
publication vector, ranking, partial/error/retry and delivery metadata. Validation
requires the exact expected member roster. Known work fits its allocation and
uses only allocated ordinals; a completed assessment requires positive actual
input tokens. Unknown work holds the entire allocation and ordinals: no zero
work, reclaim or replay inference. Known plus held resources use checked sums
within the original budget. A `KnownViolation` instead retains exact observed
failed resources which MUST exceed that partition's original allocation. Only
explicit AccountingViolation termination permits the checked known/held sum to
show that observed overrun; it grants no extra capacity, reclaim or replay.
All later members must be Unqueried(AccountingViolation). The consumer must
prove the complete native /2 packet is a failed Budget/Unassessed outcome before
creating this report; it preserves that original body and emits a nonretryable
accounting-violation error. Ordinary Known or successful Complete cannot use the
exception, nor can an under-cap observation claim it.
Unknown/unqueried cannot claim Complete; Complete
means known completion/accounting, not evidence sufficiency. Local /2 coverage
or clarification never suppresses another member or invents global model truth.

Unused resources may be reassigned only after known completion to an unstarted
member, never by invoking a prior member again. This pure allocator performs no
reclaim/execution. The native owner must charge exact final wrapper bytes and
retain producer/settlement lifetime for uncertain work.

Local Resources.bytes is the maximum of native Work.bytes and EXACT typed /2
packet JSON bytes. Later MCP/transport headers stay complete outside that local
allocation. The complete combined wrapper INCLUDING delivery metadata must
still fit the original WHOLE parent max_bytes and existing canonical/escaped
wire limits. Transport overhead failure is a nonreplayable delivery-budget
refusal, not NN work violation; no Covered rewrite or metadata trimming.

ContextDeliveryBinding is an inert expected native ScopeBinding/profile pair.
The summary's delivery_bindings list is required, including explicit empty
when validated native bodies contain no actual model receipt. Profile is
required nullable: null records known absence, never a wildcard. The extractor
materializes complete members and verifies the exact actual model receipt
roster, allocated globally unique ordinals and observed accounting. Known or
KnownViolation reports cannot omit their full native packet. Synthetic
Unavailable without advice, unknown work or tokens does not claim model work.
It confers no authority. The native final-delivery owner checks current policy
at its actual delivery linearization cut under the original attempt context;
standalone reobservation is not atomic disable/switch suppression proof.
Private transient result custody may retain bounded full native bodies for the
attempt lifetime, without a Context Arc cycle or new persisted/recovery format.
Qualification observers separately retain actual full artifacts; this is not a
durable or retrievable recovery receipt.

Fixtures: `fixtures/split/i1/combined_context_allowance.json`. Core selector:
`test(combined_context::tests::)`. Host owns strict wire conformance. No generated
consumer mirror or execution/model/transport qualification is claimed here.
