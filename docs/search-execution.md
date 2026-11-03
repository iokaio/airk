# Search execution and helper availability

The 1.8.8 P6c working implementation distinguishes the search that ran from
indexed readiness and live-source verification. Qualification is recorded in the
[implementation ledger](research/implementation-v190.md).

Search replies retain an `execution` observation with `version: 1`, the original
`requested_mode`, actual `served_mode`, `incomplete` and an optional
`incomplete_reason`. Reasons include `helper_unavailable`,
`semantic_coverage_incomplete` and `semantic_upgrade_pending`. An empty result can
still be incomplete. Byte/count truncation, indexed semantic/discovery readiness
and source-verification receipts remain separate observations.

Default searches can retain their already-collected lexical results after a
typed embedding-helper availability failure when the saved PartialHybrid or
LexicalWhilePending policy permits it. RequireComplete and explicit vector or
hybrid requests fail strictly for that outage. The existing explicit Hybrid
lexical exception during an ONNX semantic upgrade remains separately disclosed.
Saved semantic opt-out preserves the original Default request and does not imply
an outage. Invalid protocol, permissions and model errors do not become lexical
successes merely because they happened during embedding.

Strict helper failures retain one bounded native receipt with
`error.code: helper_unavailable`, `retryable: false` and
`action: retry_later_without_replay`, plus the requested mode and known selected
publication/readiness. This receipt does not authorize automatic replay. Search
projection adds no health probe, model load, helper start or source scan.

CLI commands/tool calls/shell, MCP and Workbench retain these observations.
Terse MCP still compacts ordinary search replies. Workbench details and full
exports preserve the native metadata; explicitly selected exports disclose their
subset. Plans report semantic incompleteness independently of clipping, including
evidence-only output that intentionally omits raw step results.

[Source verification](source-verification.md) defaults to Report and supports
explicit Off and Require. A verified citation establishes the bounded source
check recorded in its receipt; it does not establish complete semantic retrieval.
Require can refuse stale evidence even when lexical fallback itself is allowed.
