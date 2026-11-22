# Whole-selection context allocations

Owner: `airk_core::combined_context`. Execution and authority remain with the
native frontend, worker and local Kernel. This is the reviewed option B;
no persisted state, existing digest domain or public Answer argument changes.

The shared `context_records` module owns the literal `Resources`,
`PartitionAllowance`, and `ContextDeliveryBinding` records and validators.
This module re-exports those types and continues to own selection accounting
and combined result materialization. Existing service and public encodings
use those same Rust fields and representations.

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

The Kernel compositor materializes each complete native envelope before deriving
ContextAction ranking-count metadata. Text-only, structured-only and duplicated
text/structured envelopes use the same canonical body and preserve all retained
native receipts. The count describes ranked evidence already returned under the
original calls/candidates/bytes allocation; it does not enlarge any read budget.
Ordinary non-Context operation count refusals remain unchanged. A transport
representation cannot turn a multi-hit context packet into a zero-hit count.

Context delivery byte admission reserves the serialized native wrapper, worst-width
closed scalar/publication/scope/summary metadata and exact final-delivery receipt
syntax before dispatch. The remaining canonical body allowance is divided by
three: escaped JSON text is at most twice the canonical UTF-8 JSON bytes, plus
one complete structured representation and the measured empty envelope syntax.
Current Context envelopes have one text content block and optional structured
body/error flag; unreserved envelope extensions or disagreeing representations
fail closed while full native custody remains private. Native response body
metadata is included in the local packing limit, and cannot be reclaimed as
unused body allowance by another partition. Work's existing accounting meaning
and original public budgets stay unchanged.

Context composition does not perform optional common-header factoring. Ranking
retains a bounded prefix of references into the complete native members; when
metadata would exceed the admitted canonical cap, only that prefix shrinks and
truncated is set. Full member bodies, original initial receipts, source locations
and native packet assessment/omission semantics are retained. Rank selection is
final before the whole saved-source verification receipt is produced; verification
never certifies an earlier projection that is later changed. The existing minimum
verification receipt capacity plus exact attachment/failure syntax is reserved
before dispatch; the actual remaining capacity is bounded by the existing receipt
maximum. Report may disclose bounded proof coverage and Require refuses honestly.
No result is replayed, no source is silently trimmed, and no cap is enlarged.

A native failed envelope may intentionally pair an opaque human-readable text
message with complete structured failure details. This existing error shape is
retained unchanged and charged as the complete opaque envelope, rather than
reinterpreting display text as JSON or ignoring its structured payload. Nominal
success and parseable JSON representations still must agree; unreserved envelope
fields continue to refuse. Opaque failure does not establish zero/known work or
permit replay.

The same native envelope byte owner accepts both pre-materialized text-only
frontend results and materialized Kernel results. With no structuredContent,
canonical body bytes come from parsing the native JSON text; the enclosing tool
envelope is never compared to that body. When both representations exist they
must agree exactly. Structured-only results use their own complete native body.

## Answer /3 (`airk.combined-context/2`, 22 November 2026)

The combined Answer /3 path lives in `combined_context/answer.rs`, beside the
/2 records above, which stay until the Answer /2 path is deleted. Its
recorded contract decision is in the Core arbiter contract, "Combined Answer
/3 allowance". In short: each member gets an L1 `PartitionAllowanceV3` whose
card and Focus ordinals are the prefixes `1..=k`, with `LoopResources`
(calls, input tokens, candidates, bytes, Focus pairs); `initial_allocations_v3`
shares a BudgetV3 without minting; a member's use (`LoopUse`) is counted from
its reply's work and decision log and the unused counts are handed to the
next member; unknown model work holds the whole allowance and withdraws model
work from later members; `CombinedContextSummaryV2` has no known-violation
outcome (a /3 reply is validated under its clamped budget). The wrapper's
`context` field is `CombinedContext` (/2 or /1). Delivery bindings of a /3
reply come from `model_work.focus_profile` (Focus) and the reply's
application profile when card calls were made (Context). Core selector:
`test(combined_context::answer::tests::)`.
