# Whole Answer saved-source verification

The existing combined frontend owns the complete selected Answer. Core owns
VerificationControls, VerificationBudget, VerificationAllocation and receipts;
this module introduces no native wire shape, state format or authority.

A combined Answer is never a job of the service's Answer job table. Its job
options beside the request are bounded as on the single-index path
(`AnswerJobOptions::validate`). `fresh` holds, since a combined Answer always
runs afresh. `invocation_key`, `wait_ms` and `interactive` cannot be honoured
here, so a call that names one is refused with the typed, non-submitted
`invalid_native_arguments` before any member is queried; no option is
silently ignored.

Parse the actual AnswerArgs once and preserve its requested Report, Require or
Off semantics. Native Answer children and their canonical inner reads perform
no independent saved-source checks. A private Answer-reader marker covers
standalone Answer too; Parse's explicit per-step verification is unchanged.
Standalone Answer retains its existing outer verifier. Combined Answer reuses
the existing combined VerifyProjection owner on the complete final projection.

That owner partitions the original source count and raw-source byte budget via
SharedBudgets and VerificationAllocation, and opens one verification window
bounded by the original read context. Member deadlines cannot refresh it.
Zero source/byte allocations remain explicit exhausted outcomes, never public
defaults. Every returned reference, including attempted evidence in failed
members, stays in the final projection and its native verification inventory.

The original whole output cap includes verification and final delivery metadata.
The composer reserves the actual final delivery attachment first. Verification
uses only exact remaining canonical bytes minus JSON attachment syntax and the
Require failure overlay, bounded by the existing 32768-byte native receipt cap.
The 1024-byte minimum is admission to that existing verifier, not a promise that
all source rows fit. Missing capacity or uncertain native observation reports
unperformed/unknown checks; Require fails. No budget is raised, native evidence
trimmed, or successful source claim invented. Full native member and outer
unknown fields remain retained; rendering updates the matching text/structured
copies together. The original final delivery fences remain mandatory.

The original one-Context-slot byte admission remains unchanged when stages are
off. An eligible combined child reserves its actual additional Focus binding
before NN and final packing inside the existing child allocation. The frontend
collects both stage facts with Core's canonical packet extractor, checks body
plus extra Focus metadata against that same allocation, and never reclaims
those metadata bytes for another member. No frame or native Work/Resources
budget meaning changes. Unknown Focus work holds the entire partition and its
assessment ordinals, just as unknown Context work does; it is never known zero
work or spare optional capacity.

## A member publication that moved (23 November 2026, change-log entry 67)

When a member's source projection is refused because a newer publication
replaced the member's evidence publication under the same authority (the
native read's typed `publication_busy`):
- a combined Answer member, whose loop may have spent model tokens, becomes
  `publication_advanced` (`retryable: false`, `query_replay_safe: false`),
  not `source_projection_expired`;
- an ordinary combined member read (`read_member`) is read again when its
  result carries no Focus or Context delivery binding, at most three reads;
  with such a binding it is `publication_advanced`; still moving after the
  third read it is the retryable `publication_busy`.
Every other projection failure is still `source_projection_expired`.
Test: `combined_frontend::tests::moved_member_publication_is_classified_by_its_typed_native_receipt`
(the classification; the member loop itself is not exercised by a test
that races a publication).