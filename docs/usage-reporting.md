# Local usage, performance and estimated savings

AIRK will collect local aggregate counters by default, as requested. The first code structures are implemented: [core observation/policy contracts](../crates/airk-core/src/usage.rs), [kernel estimation and report helpers](../crates/airk-kernel/src/usage.rs), and [calculation/isolation tests](../crates/airk-kernel/tests/usage.rs). Persistent recording, live CLI/MCP hooks, report commands and extension views are planned below. The current app does not collect usage or advertise a measured savings total yet.

The tracker measures AIRK's work and delivered context. It does not observe an agent's complete prompt, provider token accounting, cached-input discounts, model responses, task completion or bill. Estimated context reduction and hypothetical monetary savings must remain visibly different from those observations.

[Workspace indexing coverage](index-coverage.md) is a separate planned measure: current indexed files as a percentage of eligible workspace files, stale/pending counts and a clickable Index Now footer. Inventory completeness must remain visible; coverage is neither retrieval quality nor estimated savings.

## Ownership and code placement

| Owner | Structure / planned responsibility |
|---|---|
| airk-core::usage | Versioned content-free observations, collection/root scope, recording policy, recorder interface and explicit usefulness feedback |
| airk-kernel::usage | Request-span coordination, comparable-baseline construction, token estimates and bounded reporting; estimation/report helpers exist now |
| airk-store::usage — planned | Bounded local aggregates, writer shards, deduplication, rollups, retention, deletion and coverage metadata |
| airk-ingest / airk-retrieval — planned hooks | Normalized source byte counts, format/language buckets, index reuse and stage/cache counters |
| airk-cli / airk-mcp — planned hooks | One observation per outer request, serialized response bytes and normalized client family; future report output |
| packages/vscode — planned UI | Collection summary, history charts, indexing/space trends, recording controls and selected-collection export |

~~~mermaid
flowchart LR
  A[CLI or MCP request] --> K[Planned kernel request span]
  K --> R[Existing indexing and retrieval]
  R --> O[Content-free observation contracts]
  O --> S[Planned bounded local aggregates]
  S --> P[Kernel report and estimate helpers]
  P --> C[Planned CLI and extension reports]
~~~

Keep modules in the existing crates. Usage data is derived state under the chosen AIRK data directory, including the container volume. No cloud analytics, external database, provider credential or network connection is required. MCP reports stay bound to the worker's collection/root; workspace tools must never select another collection's history.

## Measurements and useful interpretations

| Area | Measurements | Interpretation |
|---|---|---|
| Activity | Calls by operation, collection, normalized client, document kind and later language; hourly/daily counts | Which tools/formats are used and when |
| Results | Nonempty/empty/error/not-ready/cancelled/capacity-limited counts, result counts and truncation | Scope, extraction coverage and budget problems; nonempty is not proof of relevance |
| Latency | Monotonic request duration, cold/warm cache labels, later stage timings | Slow tools, cold-start penalties and cache behavior |
| Context | Serialized UTF-8 response bytes including citations/metadata, plus comparable baseline bytes | Delivered context relative to an explicitly named alternative |
| Indexing | Changed/reused/removed/skipped files, processed source bytes, duration and last/peak index footprint | Extraction reuse, churn and disk growth; reuse does not imply source hashing/I/O was avoided |
| Scheduler | Received/coalesced events, launched batches, queue delay, pending revisions and budget/lock blocks | Excessive indexing or stale evidence during rapid edits |
| Feedback — optional | Useful/partly useful/not useful against an opaque request ID | Explicit usefulness evidence; absent feedback is unknown |
| Accounting health | Drops, flush failures, duplicates, retained coverage and estimator version | Whether the report window is complete enough to interpret |

The current report helper supplies operation/client/document-kind counts, outcome/truncation/cache totals, latency buckets, index-work totals and comparison coverage. Language profiles, scheduler metrics, feedback rollups, disk-growth history and persistent recording health are later implementations. Latency boundaries are 1 ms, 5 ms, 20 ms, 100 ms, 500 ms, 2 s and 16 s, followed by an overflow bucket. Bucket-derived percentiles must be approximate; exact percentile calculation is not implemented. Cache hit ratios use known warm/cold observations only and show unknown/not-applicable coverage separately.

## Honest token estimates

Measure response size after serialization at the CLI/MCP boundary. Current text budgets exclude some framing and cannot substitute for response measurements. The contract distinguishes logical result JSON from MCP tool-result JSON; neither automatically equals the text an agent sends to its model.

Keep three baseline categories separate:

1. **Full returned sources:** normalized full text of the unique sources contributing evidence, rendered with comparable framing from the same generation. This is a counterfactual full-source context scenario, not a claim that the agent would otherwise read every file.
2. **Observed alternative:** a comparable alternative-tool result captured by an explicitly integrated client or evaluation harness. It does not establish causal provider-billing savings.
3. **User supplied:** an explicitly configured scenario with provenance, never presented as observed workflow behavior.

Do not use repository size as the baseline. Deduplicate contributing sources within a request. PDF/DOCX/PPTX use extracted text, not compressed file size or images; SVG uses textual content rather than rendered pixels. Baselines and responses must match generation and representation. Missing/mismatched comparisons are excluded and counted. Failed/empty results, indexing and administrative tools receive no token-reduction credit. Nested helpers and replayed request IDs must not count twice.

The initial method is a UTF-8-byte heuristic: four nominal bytes per token, with two-to-six-byte sensitivity assumptions. These are adjustable scenario assumptions, not model-specific accuracy guarantees or a statistical confidence interval. Code, Unicode, natural language and JSON tokenize differently. Future local tokenizer adapters must record their exact tokenizer/model revision, apply the same tokenizer to both sides and count transient text without retaining it.

```text
nominal baseline tokens = ceil(baseline UTF-8 bytes / nominal bytes per token)
nominal returned tokens = ceil(serialized response UTF-8 bytes / nominal bytes per token)
estimated context reduction = nominal baseline tokens - nominal returned tokens
```

For a fictional 48,000-byte baseline and 4,800-byte response, the nominal reduction is 10,800 tokens and the broad assumption range is 5,600–23,200. An 80-byte source compared with a 400-byte JSON result instead yields **80 additional nominal tokens**. Preserve signed differences rather than clamping expansions to zero. Estimate per request before aggregation to retain per-call rounding.

Stratify comparisons by baseline and estimator version. Show compared/eligible calls, missing/mismatched baselines, excluded calls, replay duplicates, dropped records and retained coverage beside totals. Unknown drops/retention are unknown, not zero. Repeated deliveries do not establish unique task-level savings; cross-call overlap and cached agent context require an optional explicit client/session integration.

## Monetary scenarios and user value

Currency estimates remain unavailable unless the user supplies a model/input-price scenario, currency, rate date and treatment of cached input. A conditional scenario can use `estimated input-token reduction / 1,000,000 × supplied effective input rate`, labeled a hypothetical input-cost equivalent. Do not fetch prices silently, invent a default rate, mix models, count output-token savings or promise a lower bill. The current helper returns no bill-savings value and marks provider token usage as unobserved.

Actual bill/task comparisons need optional client-supplied usage and paired evaluations controlling model, task, success criteria and prompt history. Keep them separate from the local estimate. Expose indexing time, CPU/I/O when measurable, memory and disk growth too; smaller context or faster retrieval alone does not prove a net time/cost benefit.

## Recording, scope and retention

The default `UsagePolicy` is aggregates-only: 30-day aggregate retention, a 1 MiB collection cap and a 32 MiB data-root cap. These are planned enforced limits; no durable recorder is attached yet. Users can disable recording. Detailed content-free diagnostic events are separately opt-in with a short 24-hour allowance. The schema contains no query strings/hashes, symbol names, paths, source text, prompts, arguments, raw errors, credentials or free-form notes. Map client/format/language labels to bounded categories rather than arbitrary input strings. Client labels never authorize anything.

Scope combines collection ID and root-binding digest. Rebound/recreated collections must not inherit unrelated history. Container mounts need the managed identity discussed in [collections](collections.md); hashing a mount path cannot prove host-project identity. Place diagnostics within managed collection state so storage selection, usage inspection and explicit collection removal also govern those bytes.

The first durable adapter should flush aggregate batches in short-lived CLI workers and periodically in MCP workers. Each worker owns a bounded shard; a compactor merges daily aggregates with event/sequence deduplication and atomic publication. Do not take the index writer lock per observation, create a file per request, rewrite full history synchronously or persist detailed events by default. Coordinate registration/removal so late writers cannot recreate a deleted collection. Lifecycle tests must pass before persistence is enabled.

Recording is best-effort: failure never fails retrieval, exceeds a small recording-time budget or starts retry loops. Preserve observable drop/flush-failure counters. Disabled mode performs no usage-state reads/writes. Bound dimensions, buffers, record sizes and retention; prune diagnostics/obsolete aggregates before consuming index capacity. Exports must stream bounded aggregates, safely encode wide totals for JavaScript and avoid estimating their own report output as saved context.

## Delivery sequence

| Increment | Implementation | Acceptance |
|---|---|---|
| 1 — implemented foundation | Core contracts/default policy, signed heuristic and scoped bounded summaries | Sensitivity/overflow, expansion, failed/empty/missing/mismatched baselines, root/window isolation, replay deduplication and index work separate from tokens |
| 2 — local recorder | Store adapter, aggregate-only flush/rollups, caps/retention/coverage and deletion integration | Multiple workers, crash/partial writes, full disk, late deletion writes, disabled mode, no content capture and no retrieval failure |
| 3 — live hooks | Kernel request spans, adapter serialization measurement, indexing/retrieval/scheduler counters and comparable baselines | No nested/setup double counts; mixed document/framing/generation fixtures; bounded overhead and no extra unbounded source reads |
| 4 — reports and controls | Planned `airk usage report`, selected-collection JSON/CSV/Markdown export and extension Usage and Savings view | Explicit empty/disabled/partial windows, root/date filters, signed estimates, wide-number-safe exports and source-preserving history clearing |
| 5 — calibration/client evidence | Local tokenizer, explicit feedback, opt-in session/alternative-tool observations and optional price scenarios | Held-out format/language cases, reproducible provenance, usefulness separate from hit counts and billing claims supported by actual evidence |

A future read-only MCP summary tool is optional and must use the worker's collection. Adding it requires updating capabilities, protocol tests and extension probes; the current fourteen-tool inventory is unchanged. Owner-wide reports remain CLI/extension maintenance actions. None of these planned commands/settings are currently advertised as available.
