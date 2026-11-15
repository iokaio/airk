# Replacement removal and interrupted predecessor transactions

Owner: `airk-installer::runtime_install::replacement`.

Decision recorded 15 November 2026 before dependent implementation. Existing
replacement admission, expected-predecessor, source/read-only preservation,
receipt and directory/image identity rules remain unchanged. Exact-handle
predecessor termination remains the existing owner, including unresponsive
predecessors; this change adds no new graceful-shutdown authority path.

After verified process termination and executable image exclusion, the initial
unfrozen path and pending-operation resume recover an existing rollback journal
through Store's `recover_existing_after_stop`. Before that call the installer
acquires the actual native ServiceLease for the exact admitted control root and
uses the original expected namespace. Store alone performs SQLite rollback;
`open_saved` remains read-only. The lease stays held through complete read-only
roster recapture and is released before directory moves. The recaptured roster
must equal the admitted source/data/control/namespace roster before freezing or
removing any location. Recovery/lease/namespace refusal leaves all source and
owned location names untouched and retains the pending operation for diagnosis.
Only interrupted private derived registry bytes may be rolled back. No source
file is written; no new schema, wire shape or receipt/digest format is added.

Focused regressions use a real SQLite-produced spilled uncommitted transaction
image, preserve the native readonly refusal, and exercise initial and resume
recovery plus live-owner refusal before any rename/purge callback.

Before any original SQLite recovery, Store recovers bounded private copies of
both exact DB/journal byte images and runs the same existing-schema and actual
SQL namespace validation there. A foreign database beneath an otherwise matching
owner refuses with original DB/journal bytes unchanged. Actual file IDs and
bounded bytes are rechecked against retained handles/path objects before the
original recovery under the still-held ServiceLease. Preview scratch is owned
by the private native control root and is never persisted as registry authority.
