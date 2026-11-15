# Whole Answer saved-source verification

The existing combined frontend owns the complete selected Answer. Core owns
VerificationControls, VerificationBudget, VerificationAllocation and receipts;
this module introduces no native wire shape, state format or authority.

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
