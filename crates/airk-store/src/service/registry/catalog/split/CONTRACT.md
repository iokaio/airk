# Store split publication contract

`catalog::split` owns accepted split intent, phase receipts, participant fences and atomic membership publication. Core `catalog_split` owns the request/receipt types and digest rules. No query or preview advances work.

Policy contracts 2 and 3 preserve their existing physical layouts and exact request bytes. Grouped splits require policy contract 4, installed only in the accepted split transaction. Contract 4 uses the complete existing policy3 physical layout plus marker 4; layout validation accepts that exact shape and a supported marker, while predecessor binaries refuse marker 4. Accepted older requests are resumed without adding a grouping or changing their digest domain.

New reviews include Core grouping mode Create or ExistingNamedSet. Create uses an operation-derived fresh identity. ExistingNamedSet requires one exact-name set, the exact full original-index scope and pinned set identity/definition/scope plus original membership revision. Unrelated, restricted or ambiguous same-name sets refuse. Both reservation and membership publication recheck the original index definition/name and all grouping pins.

For ExistingNamedSet, the matching legacy project effect must retain the original and omit destination_scope. That no-op effect remains in the affected-project census so normal source membership fencing is preserved; only grouping owns its destination addition. No explicit effect may remove or independently add either grouped membership. All other project effects retain their reviewed semantics.

The existing immediate membership transaction enables the validated destination, applies unrelated reviewed project effects, creates/reuses the grouping, confirms both memberships, and advances the original split receipt atomically. Failure rolls all metadata back. Replay uses the original request key/operation; it does not infer authority from an existing same-name set. Grouped membership receipts use their new Core-documented digest domain. Neither grouping nor set deletion creates grants, rewrites selections/defaults or changes member lifecycle.

DeleteProject remains the only set-deletion owner. It removes the set and its associations/preferences, preserving member definitions, source rows/files, stored publications, configuration, work/watch ownership and other memberships. Native grouping matrix tests cover signed/checked SQLite revisions, rollback, original-key replay, conflicts, existing-set scope preservation, empty/stale deletion and active-work preservation. Supervisor split qualification drives the full native split/fence/publication/funding pipeline and checks grouping plus member evidence before/after real fenced set deletion; running those selectors is required before claiming that outcome.

A claimed split worker settles a committed plain index under the collection
writer and the participant's saved funding phase. Settlement checks the
collection scope and incarnation and measures actual retained bytes. The
plain store has no separate current journal grant. Legacy journal settlement
still requires its own Ready state and recovery allowance. Canceled cleanup
preserves the pinned owner across interruption and accounts for disposal only
after the database, sidecars and vector files are absent.

## Finished-split pruning (record cleanups of 23 November 2026, item 2)

At most 64 split operations are retained (`index_split_capacity`), and before
change-log entry 68 part 4 nothing removed one. `finalization::prune_finished_splits`
now deletes a split once it is finished for good: complete with its final
funding acknowledgment and no attention, or canceled before its fence with its
cleanup done (and its final plan, when it ran one, acknowledged). Neither can
be reopened. A split with an active participant or a
pending or running job is kept. Children are deleted before the operation,
because every split table refers to it with `ON DELETE RESTRICT`: extraction
policy, recoveries, jobs, history, its inactive participant rows, and its final
plan, error, acknowledgment and slot receipts. The newest 8 finished splits
are kept. Pruning runs in the registry's request-epoch pass
(`catalog::record_pruning`) once 48 splits are retained, and the epoch rotates,
so a pruned split's request and recovery keys expire. Test:
`split::tests::more_than_64_splits_succeed_and_an_unfinished_split_survives`.