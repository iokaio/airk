# Measuring AIRK's value

See the [Usage & Savings UI previews](ui/usage-reporting.md) for rendered examples with fictional measurements.

The **Usage & Savings** button on each collection opens a report for that collection. Start with **Smaller results**: it compares AIRK responses with the complete files they cite. The token estimate translates that difference into small pieces of text an AI would read. It is not a measurement of your agent's complete prompt or bill.

To answer whether AIRK helped complete work faster or with fewer tokens, supply session measurements and compare the same tasks with and without AIRK. AIRK does not run agents or inspect their private logs for this purpose.

## Bring your own session measurements

Use **Create measurement template** in the report, fill in the final session counters, save it, and choose **Import measurements**. The extension validates and displays a preview before recording the same captured bytes. Unknown values stay `null`; zero means a measured zero. A model revision digest identifies the exact model definition you used, not a prompt, task or source file.

Record a session once after it finishes. Keep its random `event_id` and original `unix_ms` when retrying an import. Do not repeatedly import cumulative snapshots under new IDs. Native replay protection is bounded; expired imports are rejected. Imports do not increment AIRK's own request count.

The optional `airk_request_ids` array can contain up to 100 `request_id` values from AIRK responses. The native ledger requires every ID to be retained in the selected collection and refuses IDs already linked by another session import. Unknown, dropped, expired and foreign IDs fail the entire import. Links establish an explicitly supplied association, not that AIRK caused the session's outcome. The ledger keeps bounded link membership and aggregate link counts, not session names or full session traces.

## Convert supported client counters

`scripts/usage-evidence.py adapt` converts an explicitly selected JSON array of final API usage objects into an AIRK import. It supports a bounded subset of OpenAI Responses and Anthropic Messages usage fields. These are API counter adapters, not automatic Codex or Claude Code transcript collectors. Other clients can supply the normalized session contract directly.

The mappings follow [OpenAI's prompt-cache accounting](https://developers.openai.com/api/docs/guides/prompt-caching) and [Anthropic's input-token accounting](https://platform.claude.com/docs/en/build-with-claude/prompt-caching). OpenAI input totals include cached input. Anthropic total input combines ordinary input, cache reads and cache creation. Output reasoning details are not added again to output totals.

Only supported numeric usage fields are accepted. Full responses, prompts, arbitrary metadata and unknown fields are rejected. Inputs are capped at 1 MiB and 1,000 final usage objects. Supply each completed request exactly once; do not mix stream deltas and cumulative totals. Missing required counts make that aggregate unknown. Positive or missing cache-write counts suppress the imported cache split because the current two-rate input scenario cannot price separate write tiers. Missing Anthropic cache-creation counts likewise prevent a complete input total. No rates are fetched automatically.

For an original fictional `counts.json`:

```json
[
  {"input_tokens": 1000, "input_tokens_details": {"cached_tokens": 200, "cache_write_tokens": 0}, "output_tokens": 100}
]
```

```console
python scripts/usage-evidence.py adapt --adapter openai-responses --input counts.json --model-revision-sha256 MODEL_DIGEST --event-id RANDOM_32_HEX_ID --unix-ms OBSERVATION_UTC_MS --client codex --elapsed-ms 12000 --tool-round-trips 3 > measurement.json
airk usage import measurement.json --preview
airk usage import measurement.json
```

Replace the uppercase placeholders with the actual identities/time. Use the same `--root`, `--data-dir` and `--collection` options as the agent. `--airk-request-id` is repeatable. The report keeps client, model/tokenizer and provenance categories separate. Imports remain self-reported even when converted from provider-shaped counters.

## Matched-task evaluation

1. Select a representative task set before running the comparison: file discovery, code investigation, document questions and multi-step edits. Keep the task set, source revision, acceptance checks and budgets fixed outside AIRK's ledger.
2. Run each task with and without AIRK using the same client/model revision and comparable tool access. Start independent sessions. Alternate or randomize run order, document cache conditions, and repeat trials. Measure indexing/setup separately and state how it is amortized.
3. Apply the same acceptance checks to both outputs. Record failures and incomplete tasks. Capture final input/output/cache counters, elapsed time and tool rounds where available. Do not infer missing values from AIRK response bytes.
4. Save each side as a normalized `SessionMeasurement` object (the `session` object from an adapted measurement). Package a pair with explicit attestation:

```console
python scripts/usage-evidence.py pair --with-airk with-session.json --without-airk without-session.json --with-result passed --without-result passed --attest-matched-task-and-model --event-id RANDOM_32_HEX_ID --unix-ms OBSERVATION_UTC_MS --client codex > pair.json
airk usage import pair.json --preview
airk usage import pair.json
```

5. Collect the resulting `paired_task` objects in one JSON array and compare repeated trials:

```console
python scripts/usage-evidence.py compare --input pairs.json > comparison.json
```

The comparison reports signed total/median/minimum/maximum differences and measured/missing pair counts for input tokens, output tokens, elapsed time and tool rounds. It reports all five quality outcomes and a separate both-passed subset. Different model revisions are rejected. Positive differences mean less work with AIRK. The both-passed subset can be biased by excluding failures; always publish quality outcomes too. A supplied attestation does not verify experimental conditions or causality.

## Time, money and local cost

**Set cost assumptions** accepts a model, currency, rate date, input rate, optional cached-input rate and optional output rate. Rates are per million tokens. The default prices supplied session measurements only; pricing the full-file comparison requires an explicit uncached-input assumption. Unknown cache splits are left unpriced. Missing output rates or counts remain unknown. Rates apply only to the open report and exported snapshot; they do not become an implicit global model setting.

Session cost equivalents are hypothetical amounts at the supplied rates. They exclude provider-specific fees, cache-write tiers and discounts not represented by the scenario. They do not verify a provider bill or model compatibility. Subscription spending may remain unchanged. AIRK's local indexing time, disk usage and process counters are shown under developer details; retrieval latency is not time saved on the complete task.

Matched-task cost differences are shown separately for uncached input, cached input and output. Each component requires complete counters for both sides of every pair in that group, and its applicable price. Positive amounts mean lower hypothetical cost with AIRK; negative amounts mean higher cost. Task-quality outcomes remain visible, including failed AIRK runs.

## Independent accounting-overhead measurement

```console
python scripts/benchmark-usage.py --binary target/release/airk.exe --trials 1000 --workers 4 --require-complete --output target/usage-overhead.json
```

The harness creates original synthetic files in its own temporary workspace. It alternates recording enabled/disabled, then runs concurrent independent workers. It records external wall time including process startup, serialized output and ledger flush, and independently compares submitted calls with recorded calls. It reports losses rather than assuming the ledger is complete. `--require-complete` also exits unsuccessfully if the recorded count differs from the number of submitted successful calls, preserving the JSON measurements for diagnosis. This is a local CLI workload, not persistent MCP performance or evidence of agent savings. Run it on each target platform and representative storage/workloads before making overhead claims. Full-disk, process-kill and long-duration load qualification remain separate release gates.

## Longer-term progress

The native `lifetime` report field is one bounded aggregate updated atomically with each new recorded AIRK observation. It survives daily retention, shares the existing ledger quota and preserves negative context reductions. It starts with observations recorded by 1.1.3; retained older daily buckets are not backfilled. It is partial native history, not “since installation” or lifetime agent-session savings. Imported measurements retain their existing daily window. Clear removes both daily and cumulative totals and invalidates old writers. Disabling pauses future updates without deleting existing totals.
