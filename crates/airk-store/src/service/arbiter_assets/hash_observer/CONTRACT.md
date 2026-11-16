# Test-support observation of actual optional-asset hashing

Owner: Store `service::arbiter_assets::hash_observer`. This module exists only
with explicit `test-support`; Store default features remain empty. Service's
existing test-support feature propagates it for the genuine external Rust
consumer in the private acquisition regression. No product wire, stored record,
digest, acquisition authority or production-default hash behavior changes.

A lexical thread-scoped observer sees Entered only after the first positive
native file read has actually updated the digest. It carries the exact Asset
and positive actual cumulative byte count. Returned is an RAII observation of
the actual hash scope return/unwind, including verification/I/O failure; it is
not proof that verification succeeded. Every hash result remains owned by the
existing native hash implementation. The separate complete Store result tells
success, original error, cancellation and phase-observation failure.

Callbacks are synchronous, nonblocking observations only: atomics and try-lock
channel delivery; no sleeps, waits/gates, I/O, configuration/source mutation or
result rewriting. The hash scope clones the observer before callbacks so no
thread-local borrow is held during notification. Nested scopes restore their
prior callback, including unwind, and callbacks never cross threads. Observer
panics are contained under normal Rust unwind semantics and cannot replace the
native I/O result or double-panic on hash-scope Drop. Production defaults contain
neither this observer nor callback branches.

The Service callback filters the exact current Large external-weight Asset,
records the native positive-read event and marks active only until Store's RAII
Returned event. Actual I1 Health and Cancel must complete while that real native
hash remains active. The callback has no gate or I/O. Receiver/receipt writes
occur outside the hash observer. The complete final Store observation receiver
lives outside fallible control flow and is recovered after every actual worker
join, including early Health/Cancel/Shutdown failure. Original Store I/O error
has precedence over phase lookup or missing/dropped observers. A successful I/O
with failed phase observation stays a distinct failure. These test observations
are private receipts, not product DTOs or ManagedStopComplete.

Small real-file tests independently pin event order/positive progress/exact
asset identity, actual hash-error retention, observer-panic containment, lexical
restoration and thread isolation. They do not replace the ignored16GB data run.
