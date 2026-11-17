# Open MCP session status projection

Owner: `airk-mcp::response`; HTTP attaches one `OpenStatusSession` to each
actual backend session. The built-in local service connection is shared and
must not own this memory. U110 open reads may recreate their native workers
for each call; that worker lifetime cannot define the public session's status
observation lifetime.

The projection reuses the canonical status field digest/normalization rules.
It retains one current validated single-index scope fingerprint and fixed-field
status digests, with no response/evidence cache. Switching scope resets memory.
Collection/incarnation binding uses the full publication envelope or the
incarnation parsed from its exact retained publication token. Timestamps alone
do not constitute a state transition; failure text and all other substantive
status fields remain part of the digest.

Only unchanged top-level freshness, readiness, discovery observation and
indexing fields may be omitted. Full, include_status and explicit status,
diagnostics, capabilities and changes repeat them. Error envelopes retain
status. Complete source-verification facts, read_scope, scope_observation,
provenance and arbitrary nested evidence/user objects remain unchanged.
Text and structured representations are changed together only when both
already agree. Unrecognized representations or invalid scope metadata are
preserved. Receipt-bound combined-read bodies are never transformed.

This is per-session display state only: no wire shape, digest input, persisted
format, read budget, source authority, retry or cancellation rule changes.
Canonical unit tests compare complete expected results. HTTP backend tests
prove per-factory-session isolation with injected hooks; the actual CLI HTTPS
terse-status regression and real-transport parity suite remain separate gates.


## G4 scoped device diagnostics (additive read contract)

Canonical airk_diagnostics may carry scoped_device_status, schema
airk.arbiter.scoped-device-status/1. Its exact ScopeBinding and per-stage
IndexApplication distinguish this index's eligibility from complete native
DeviceStageStatus residency/selection observations. No configuration document or
global enrollment list is serialized. Typed unavailable uses revision0 and no
invented stage/model state. Observations are descriptive, never execution or
qualification capabilities. Scalar status and Search/Answer schemas are unchanged.

The native read-only ArbiterDeviceRead method carries actual worker/scope, pinned
publication and original ReadAttemptBudget; response uses the same typed name.
Service repeats actual scope gates and original monotonic budget before/after;
Kernel repeats actual scope/publication fences. Only strict current4 reads and
metadata snapshots are permitted: no migration/CAS/rebind, model/device load or
probe, downloads or native continuation. Default absent controller/original
attempt omits the optional observation. Native absent/retained/corrupt4 yields
truthful unavailable, never fake disabled/current configuration.

Combined diagnostics keeps the complete observation under each corresponding
member.result; it never fuses authorities or global enrollment state. Complete
copy/export retains precision, native fallback reasons and all variant metadata.
The additional observation is bounded256KiB inside unchanged original reply/frame
ceilings. No retries or replacement clock are introduced.
