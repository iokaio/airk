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
