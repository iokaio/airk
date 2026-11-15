# Native inference observations

Owner: `../inference_provider.rs`; pure serialized observations do not prove
native ownership, barrier completion or permission to retire workers. The service
retains the actual native ownership and completion witnesses.

## Fixed resident reference accounting

ProviderStatus and CompletionObservation bound model_references_outstanding by
actual running_outstanding plus MAX_CACHED_MODEL_RESIDENT_SLOTS, exactly three
named caches: bundled MiniLM, one Arbiter Focus and one Arbiter Context session.
The addition saturates safely at the representable u64 bound. Queued plus running
still uses checked addition and must fit admitted_high_water. Callback and delivery
counts retain their original admitted-high-water bounds. Cached sessions are not
admitted queued/running jobs, so two idle optional residents remain truthful even
with no admitted work. The fixed slots grant no other model/session cache,
queue admission, resource allowance, deadline, memory or source authority.

Complete still requires closed admission and zero queued/running, callbacks,
deliveries and model references, plus the actual native barrier at the owner.
NativeFactoryNeverAllocated retains every original zero-provider requirement.
No observation can invent native completion, suppress live references or transfer
ownership. No field, wire version or persisted format changes.

Independent regressions cover two idle optional residents, all three cache slots,
two running owners plus three caches, excessive references, live references under
Complete/zero-provider, saturation and queued/count overflow. Existing admission
and identity/refusal tests remain applicable. Actual service tests must retain the
two-resident reference assertion and settle both stages and the execution lane.
