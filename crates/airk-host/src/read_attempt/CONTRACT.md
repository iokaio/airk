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

The whole Answer owner captures the already narrowed native frame once before
member work. An unchanged opaque Instant deadline reuses that exact request ID,
clock and absolute expiry; separate member samples only reduce remaining time.
A genuinely narrower child may shorten expiry, never enlarge its captured
ceiling. Cancellation, expiry, foreign clocks and identity rebinding refuse.
This preserves the service ledger's exact cross-member parent identity without
renewing time at a worker hop.

## Same-process lexical repair timing

For a standalone M3 Search with no carried parent, the service may capture
arrival and the default allowance with the same monotonic sample. Only a
selected singleton lexical Search can bind the provenance to an existing
request ID. The frontend retains a bounded, process-local offer keyed by
the actual emitted read-attempt scalars, selected worker and complete
redeemed scope. The in-process worker may claim it once after its ordinary
read admission. A child process or an unmatched, dropped, canceled, expired,
carried or composed attempt cannot claim it. Nothing is added to the wire,
persisted state, or digest inputs.

Dropping the offer cancels the repair observation but leaves the normal
read context and accepted capacity custody intact. A claim can only
shorten its repair deadline to the decoded worker deadline. This supplies
timing provenance, with no authority to repair or assertion that any
measured repair fits. Existing delivery, cancellation, and reap limits apply.

The feature-version gate preserves strict legacy query serialization. Existing
native service/version, worker/grant and same-boot fences still apply; the gate
alone does not authorize mixed-version runtime pairs. There is no new wire,
persisted format or digest input.
