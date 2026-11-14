# Private native read-budget conversion

Owner: `airk-host::read_attempt`. Wire types remain Core `ReadAttemptBudget`.
The Process monotonic clock contract supplies the same platform basis as
`std::time::Instant`; conversion cannot safely mix Darwin CLOCK_MONOTONIC
elapsed time with Instant's CLOCK_UPTIME_RAW elapsed time.

Encoding samples the native counter before Instant. Decoding samples Instant
before the native counter. Sampling and transport time consume the original
read and optional native-settings retry budgets. Repeated delivery of the same
frame never restarts either budget. Existing caps, clock/schema checks, request
identity and remote-admission semantics remain unchanged. Private sampled
helpers permit deterministic nanosecond tests without sleeping or broadening
the public API.

The feature-version gate preserves strict legacy query serialization. Existing
native service/version, worker/grant and same-boot fences still apply; the gate
alone does not authorize mixed-version runtime pairs. There is no new wire,
persisted format or digest input.
