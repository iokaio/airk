# Native Admin entry

The owner pipe alone calls `AdminPlane::native_*`. These methods share the
existing Admin session owner, operation table, confirmation, native-key custody,
A2 grants and complete I3 result projection. They create no HTTP peer or Origin
proof. Browser HTTP admission remains in `admin_contract`.

The Core `native_admin` contract owns secrets, chunks and size/deadline limits.
Only the Rust native client receives cookie/CSRF secrets. Neither an I2 frame nor
an editor message receives them. Each call receives the original absolute
same-host monotonic deadline, capped by the existing 120-second I2 ceiling.
Reads, queueing and retries do not renew it. The injected canonical dispatch
runs in process, avoiding recursive owner-pipe admission.
The direct dispatcher receives `NativeDispatchCheck` and calls it again after
canonical checkpoint admission, immediately before entering the operation.
The pure live-service callback also fences direct grant publication while its
session lock is held; it never reacquires that lock.
Session permits carry revocation and the original idle/absolute expiry. The
effect check and each buffered delivery recheck them without acquiring the
session lock. Expiry during dispatch or between chunks suppresses delivery
with `admin_session_invalid`; callers treat an ended session after dispatch
as an unknown outcome and never replay it automatically.

Before effects, two service-global response slots reserve bounded capacity.
At most two pending or buffered responses occupy these slots, each bounded by
the existing 32 MiB public response limit. Responses stream in 64 KiB chunks;
status, total and exact increasing offset remain fixed. A read after the final
chunk, a duplicate call, or an incorrect offset refuses. Completed and cancelled
request IDs remain tombstones until the original deadline; at most 128 IDs are
retained. Capacity exhaustion refuses before dispatch.
An RAII reservation is shared by the actual producer and buffered reply.
Cancel, expiry and session close cannot release the producer's capacity before
that producer has finished; late replies are discarded before collection.

Cancellation races with the atomic dispatch boundary. Before dispatch it
prevents entry; after dispatch it suppresses delivery and preserves native
mutation recovery under its original identity. It never claims rollback or
replays a mutation under another key. A cancellation that arrives before its
call retains a tombstone for the full I2 deadline ceiling. Session close revokes
only that session and its A2 grants, cancels its pending calls and frees its
buffers. Service end cancels all calls and ends every session. Cookie expiry,
restart, or abrupt native-client loss does not imply persistent recovery.

`native/tests.rs` covers bounded assembly, full existing size boundary, Unicode,
strict offsets, duplicate suppression, cancellation before/after dispatch,
session isolation, expiry during dispatch and between chunks, and no-effect
admission refusals. These injected-handler
tests do not establish real I1/I2 transport or installed-editor qualification.

Actual delivery retains the original ReadAttemptContext after body creation and
through the last native chunk's outer I1 write. A chunk builder is not a policy
linearization point. Native delivery-context lookup validates the same session,
request identity, original deadline and dispatch phase; missing custody refuses.
Known Answer results register the Core-owned exact delivery binding roster in
shared context custody before JSON/SSE/chunk projection. The guard captures
immutable native policy facts, the original narrowed deadline and cancellation;
it cannot capture the context/container and create a cycle.

HTTPS and native I1 writers execute the actual synchronous write poll under
Store policy read ownership, then credential/read-context cells and NativeGate
in that order. No lock is held across an await or Poll::Pending. Disable/profile
switch may not commit across an admitted byte cut. Once revoked, a buffered or
partially written response closes; it cannot become a successful fallback.
