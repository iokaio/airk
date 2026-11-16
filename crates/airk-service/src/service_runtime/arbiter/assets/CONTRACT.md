# Owned optional-model acquisition and shutdown

Owner: `airk-service::service_runtime::arbiter::assets`, with lifecycle admission
and service shutdown remaining in their existing owners. Decision: final-plan
Steps 3/6, recorded 15 November 2026 (America/Denver).

Explicit Provision runs its original asynchronous transport future on one joined
Tokio blocking owner using the existing runtime handle. Synchronous Store begin,
verified-cache lookup, partial restart, publication, final whole-asset hashing
and file writes therefore leave the service control executor available. Actual
reservation observers are retained in each stage before native warm admission,
with capacity derived from Kernel's `MAX_OPTIONAL_RESERVATIONS` (16). Only an
observed successful final release permits pruning; pending or unknown observers
are retained. Capacity pressure refuses new optional work instead of eviction. No new
runtime, process, provider, source, retry or clock is created. Both cached
rollback and continuation pin verification use the same joined boundary.

The captured profile, Store/ServiceLease, original part deadlines, redirect
counters, cancellation and source diagnostics survive until the actual operation
returns. The caller awaits the blocking join without selecting away on cancel.
Cancel never claims native completion or releases original custody early.
Import and Remove retain their existing joined blocking owners. The 900-second
minimum or size at 256 KiB/s part deadline, 30-second idle bound and four-redirect
limit remain unchanged. Reuse still authenticates every declared logical asset.

Service shutdown first fences new lifecycle admission and restoration, clears
both stages' pending intent and requests cancellation. It waits for observed
actual lifecycle settlement before dropping the Arbiter owner or advancing the
service lifecycle record. The wait registers its Notify observation before
checking state; no wake can be lost between check and await. Actual completion
notifies after publishing state. A stopping stage cannot start, replace a
cancellation token, restore an old choice or admit a cached continuation.

The current native lifecycle request has no embedded stop deadline. At entry
to service Draining, one absolute 30-second native-release observation cutoff is
captured, before ingress and connection drainage. Both stages, every actual
active/warming/retained target and every actual reservation observer share this
same cutoff; no stage or target refreshes it. This is a new explicit shutdown
observation contract using the existing native-release ceiling, not extra model
work time or an extension of read/Provision deadlines. A narrower existing
cutoff wins. Expiry retains Draining/unknown and never advances `finish_drain` or
logs successful `ServiceStopped`.

Before cancellation or release enqueue can remove a resident, shutdown captures
the actual native active/warming identities and the active resident's real
`OptionalRelease` observer, including an actual retained resident that differs
from the latest requested profile. Queue release acceptance and lifecycle
`busy=false` are insufficient: successful shutdown also observes actual profile
release, no remaining native stage work and the last reservation owner's drop.
An unavailable/racing charge observation or unresolved native Draining phase
refuses successful completion. No dropped future, canceled request or file
presence supplies that proof. The real ServiceLease remains owned throughout
blocking work, even after a caller's external stop observation expires.

Cheap regressions use observed event gates and the actual ServiceLease. They
prove control progress during synchronous verification, retained custody after
cancellation, settlement only after actual worker return, unchanged runtime
clock/deadline, lost-wakeup-safe shutdown and refusal of unknown native drains.
No fixed sleeps, retry, longer frame deadline or fabricated cleanup is allowed.
Actual loaded-model, ABI/resource, installed and platform evidence remains
separate from these ownership regressions.
