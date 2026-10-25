# Native project management clients

Current 1.4 development guidance. Final packaging, platform qualification and release review remain open in the [completion checklist](service-implementation-checklist.md). No service VSIX has been built.

Reports and settings travel directly to the registered project worker. The broker carries identity, admission and job metadata. The MCP inventory remains sixteen read-only evidence tools; native management reads add no hidden tools or usage observations for panel refreshes.

Usage & Savings shows a pending-operation state and disables report controls until the native action finishes. Recording choices translate the panel's CLI spellings to the native enum, including daily totals and diagnostics. Real Windows stage acceptance covers seven-day CSV/Markdown exports, reviewed imports, supplied price assumptions, recording off/on, two-collection isolation, canceled/confirmed clearing and rejection after reviewed storage rebinding.

## Read clients

Use the approved stable launcher with the source/data/collection arguments for an existing registration. A saved opaque binding also supports retained reads when the source is unavailable.

```text
airk ... usage report --days 7
airk ... usage report --days 30 --format csv
airk ... usage report --days 7 --format markdown
airk ... usage report --price-scenario /absolute/prices.json
airk ... usage policy
airk ... settings extraction_policy
airk ... settings vector_selection
airk ... settings vector_status
airk ... settings roots
airk ... settings scan_limits
airk ... settings quota_policy
airk ... settings quota_status
airk ... settings extraction_policy --validate-file /absolute/policy.json
```

Validation accepts selected extraction, vector-selection, scan-limit and quota-policy files without applying them. Fact/read aliases use the MCP result envelope. Navigation is compact by default; `--compact false` requests IDs and rich facts independently of response mode.

Exact project-bound pages preserve the existing 64 KiB direct-frame allowance: at most 48 KiB per page, 16 KiB per record, 4 MiB per reconstructed snapshot, 4,096 records and 256 client pages. UTF-8-safe text chunks support exports. The client verifies scope, counts, byte counts and the final JSON digest. Changes during pagination require a new read; partial reports are not emitted as complete results.

Usage retains collection-scoped content-free history, longer-term totals, failure/unknown labels and explicit price assumptions. Normalized client-family labels are informational, never authentication claims. Each outer tool call records once. Brokerage, inner worker calls and report reads do not duplicate it. Estimates and supplied measurements remain distinct from provider-billed tokens and money.

Native source and semantic turns record their actual numeric work once under the native scheduler client. Job acceptance and waiting clients add no duplicate index observation. Deferred admission adds none; extraction failures remain failures. Source paths, skipped-file text and model inputs do not enter the usage ledger. Full replacement accounting remains a separate qualification item.

## Guarded writes

`configure --file <draft.json>` requests and waits for inspection, prepares exact incarnation/settings guards, and saves the request before submission. Acceptance identifies a durable job; it does not claim completion. The console displays its outcome and saved result separately.

```json
{"operation":"extraction_policy","path":"/absolute/policy.json"}
```

```json
{"operation":"scan_limits","path":"/absolute/limits.json"}
```

```json
{"operation":"quota_policy","path":"/absolute/quota.json"}
```

Control requests contain canonical file selectors and SHA-256, without file bodies. Extraction policies are bounded to 128 KiB, scan limits and usage imports to 64 KiB, and quota/price files to 16 KiB. Changed inputs fail before application. Results retain the reserved 16 KiB configuration-result space.

Extraction/detail/vector-selection/scan-limit changes request a native source pass. Quotas affect every collection sharing the data root. Their guard additionally binds the inspected quota-policy digest; the store compares that policy under its quota lock. Another project's intervening change cannot be overwritten by an older prepared request. Quota changes do not delete evidence, relocate data or replay failed jobs. Legacy quota state stays separate.

The Usage & Savings panel uses guarded jobs for recording, clear, import and feedback. It waits for completion before refreshing. Closing the panel, losing trust or changing its binding prevents further dispatch. Uncertain acknowledgements remain in the extension intent journal; read-only refresh never replays them.

Imports bind the bytes shown in their structure preview to the submitted request. Native execution checks retention and collection-local request links. Changed files need another preview; original files remain under user control. Calibration retains explicit tokenizer/sample selection and the shared native tokenizer provider. Sample bodies are absent from control IPC and usage records.

## Explicit recovery

The bounded [CLI journal](service-client-adapters.md) accepts enqueue and guarded configuration intents. Preparation needs an existing inspection and remains separate from submission:

```text
airk service requests --runtime-dir <control> prepare-configuration --project <binding> --file <draft.json>
airk service requests --runtime-dir <control> list
airk service requests --runtime-dir <control> retry <nonce>
airk service requests --runtime-dir <control> forget <nonce>
```

Exact replay returns the original job: repeating an acknowledged clear cannot erase later observations. Forgetting does not cancel native work. Unknown outcomes retain caller, epoch, nonce, guards and file selectors.

Windows development checks cover direct reporting/client accounting, clear replay, policy previews/publication, stale shared-quota rejection, preserved source/legacy bytes, bounded reconstruction and trusted panel recovery. Linux, paired-artifact and installed-editor evidence must be refreshed for the final source snapshot before the VSIX gate opens.
