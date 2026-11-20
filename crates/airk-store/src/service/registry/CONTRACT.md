# Existing service registry observations and stopped-owner recovery

Owner: `airk-store::service::registry`.

Decision recorded 15 November 2026 before dependent replacement behavior.
`open_saved` remains read-only: no initialization, migration, recovery, receipt
pruning, or service ownership acquisition. A hot SQLite rollback journal may
therefore refuse inspection with `SQLITE_READONLY_ROLLBACK`.

`recover_existing_after_stop(&ServiceLease, expected_namespace)` is a separate
narrow mutation boundary. The caller proves the predecessor process/image
exclusion first and retains the actual exclusive native ServiceLease for the
exact control root. The completed native owner and expected namespace must
match before SQLite opens anything for writing. Existing database/journal
objects must be bounded, regular and unaliased. The existing SQLite owner opens
without CREATE, performs only SQLite's interrupted-transaction rollback, and
validates the existing schema/namespace. It returns no registry/write handle;
it never initializes, migrates, prunes receipts, mutates lifecycle state, or
opens source/index content. An absent journal validates through `open_saved`
without changing database bytes. Missing/foreign inputs refuse.

The caller must recapture and compare its complete read-only ownership/source
roster after recovery before removal. Recovery is not authorization to remove
unreviewed locations or to reuse an unproved pending operation. No wire shape,
digest input or persisted format changes. The owning regressions are in
`stopped_recovery_tests.rs`; installer replacement regressions additionally
exercise the process-excluded recovery ordering and refusal before moves.

Before any original SQLite recovery, Store recovers bounded private copies of
both exact DB/journal byte images and runs the same existing-schema and actual
SQL namespace validation there. A foreign database beneath an otherwise matching
owner refuses with original DB/journal bytes unchanged. Actual file IDs and
bounded bytes are rechecked against retained handles/path objects before the
original recovery under the still-held ServiceLease. Preview scratch is owned
by the private native control root and is never persisted as registry authority.

A retained canceled job vetoes automatic semantic setup for its own project. The registry checks the worker lease and project epoch before exposing that veto. Its immediate enqueue transaction checks cancellation again alongside the watch, active job and engine/settings signature gates.
