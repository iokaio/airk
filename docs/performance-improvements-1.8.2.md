# AIRK 1.8.2: installed responsiveness and performance analysis

Measured November 1, 2026, on Windows 11 x64. The owner approved the installed 1.8.2 product before this final comparison.

**The main result is a more responsive installed experience: the owner reports quicker loading and previously slow or timed-out requests returning promptly. Five new sessions through the actual configured installed launcher all returned an available first status, with no status timeout.** The median time through initialization to confirmed first status was **2.286 seconds**. The retained installed 1.7.6 example took approximately **5.5 seconds**, including a timed-out first status and a second request. These are observations from different campaigns, not a controlled speedup percentage.

The first new session took **7.311 seconds**, including **5.083 seconds** to initialize. That outlier remains part of the five-session result. The evidence supports improved typical installed responsiveness and successful first-status handling in this sample; it does not promise that every cold launch is faster or that timeouts are impossible.

The separate matched corpus campaign measures indexing and retrieval through an explicit private service. It does not reproduce the installed launcher/editor startup path. It found native regressions and incomplete large-corpus readiness. Those findings remain release limitations; they do not negate the installed responsiveness observations.

## Installed startup and usable evidence: the primary responsiveness measures

The five sessions used the exact command and arguments in the workspace MCP configuration: the installed launcher, configured source root and existing Explorer descriptor. Each created a fresh MCP frontend against the existing service/index, initialized, requested status without retries or diagnostic warm-up, requested inventory, lexical search, a two-line README excerpt, vector and hybrid search, then status again. Only those measurement frontends were closed; the installed service and sources were preserved.

| Fresh frontend | Initialize ms | First status ms | Through confirmed first status ms | First status |
|---|---:|---:|---:|---|
| 1 | 5,083.088 | 2,228.206 | 7,311.351 | available |
| 2 | 1,251.821 | 1,002.729 | 2,254.588 | available |
| 3 | 1,277.487 | 1,007.297 | 2,284.817 | available |
| 4 | 1,294.899 | 996.297 | 2,291.226 | available |
| 5 | 1,276.938 | 1,008.717 | 2,285.687 | available |

All **25 evidence probes** returned nonempty usable evidence, and all five final status observations were available. Descriptive medians below include every successful probe in this five-session sample; they are not matched before/after percentiles.

| Installed operation | Median ms | Usable / requested |
|---|---:|---:|
| Inventory | 191.501 | 5 / 5 |
| Lexical | 207.795 | 5 / 5 |
| Excerpt | 164.705 | 5 / 5 |
| Vector | 1,030.466 | 5 / 5 |
| Hybrid | 3,587.349 | 5 / 5 |

The retained 1.7.6 installed observations include initialize at 2,243 ms, first status at 2,014 ms returning `status_timeout` with unknown readiness, and a second status at 1,234 ms returning an available publication. Immediate subsequent cached statuses took 4/3/3 ms. Another session initialized in 2,234 ms and returned diagnostics in 2,149 ms. See the [original installed observations](optimizations.md#measured-installed-windows-observations). Those immediate cache hits are a different workload from status after five evidence queries.

**Limits:** the earlier installed evidence contains individual samples, not a 30-session distribution. Its corpus had 1,303 files/4,507 chunks/26,980 vectors; the new installed sample had 1,389/5,188/32,505. Native validation was running during the new sample. Service state, source revisions, background load and filesystem caches were not controlled between campaigns. Fresh frontend does not mean cold filesystem. Actual editor window-load, connected-banner and first-painted-result times were not instrumented; the owner's accepted normal-editor experience is manual evidence, not a timed UI benchmark.

## Workbench responsiveness and everyday use

Workbench improvements are part of the release's main installed-experience story, alongside launcher/status behavior. They reduce avoidable preparation and transfer work and improve how returned evidence is read and copied.

| Change | User-visible benefit | Evidence / measurement boundary |
|---|---|---|
| Readiness independent of saved history | A slow history scan need not prevent preparing the selected project's connection and tools. | Implemented host lifetime/controller behavior; installed UI elapsed time was not instrumented. |
| Reuse the selected connection across New/Clear | Starting a transcript avoids discarding and rebuilding a healthy selected transport. | Lifecycle/identity tests; no universal reconnect speedup percentage. |
| Save avoids a duplicate session scan | Saving does less repeated local history work. | Implementation and session tests; storage latency not benchmarked. |
| Scalar status/save updates | Background updates avoid resending full result bodies while exports retain complete values. | Over 95% smaller status messages in the retained 200-turn fixture; byte counts, not elapsed UI speed. |
| Query-first reveal and retained reading position | New pending/completed/failed replies show the question; manual/nested reading and sidebar-tab position remain stable. | Compiled UI fixtures and owner-approved installed experience; no timed rendering claim. |
| Linked Select all N and Copy selected | Top/bottom controls select and copy the returned set, including unexpanded or filtered selections under the documented rules. | Functional keyboard/filter/copy/budget fixtures; no extra page is fetched. |
| Conversational evidence requests | Supported developer, writer, architect and QA phrasing prepares bounded native evidence plans while preserving identifiers. | 476-case inventory plus 48 known role regressions; unsupported/compound requests retain clarification and explicit review. |

See the [Workbench implementation](workbench/implementation.md), [release notes](releases/release-1.8.2.md) and [UI preparation/reuse record](optimizations.md#f16af17-ui-preparation-and-workbench-reuse). These usability and work-reduction improvements are separate from kernel throughput.

## Why responsiveness improved without a general native speedup

1. **The original timeout included the wrong work in one short deadline.** Installed verification, registration resolution and brokerage could consume the two-second status allowance before scalar index observation completed. The current [unified status path](../crates/airk-mcp/src/unified.rs) separates a bounded ten-second cold preparation allowance from the two-second established observation allowance. The first new installed status took 2.228 seconds and still returned evidence. Avoiding that false timeout improves reliability even when preparation itself is not faster. This does not remove the finite deadlines or turn unknown readiness into available evidence.

2. **Installed startup and UI preparation were explicit optimization targets.** [Runtime verification](../crates/airk-cli/src/runtime_install.rs) reuses verified identity within its fenced authority window; [runtime setup](../crates/airk-cli/src/runtime_setup.rs) and [frontend observation ownership](../crates/airk-cli/src/status_observation.rs) avoid duplicate/abandoned work. Static discovery can finish before nonessential setup. Explorer publishes verified service health independently of optional management details, and eligible selected projects can appear before unrelated workspace preparation finishes. These changes directly address waiting to connect and seeing a usable interface. Their individual contributions to the observed seconds have not been profiled.

3. **Some improvements reduce transfer work rather than kernel execution time.** Explorer sends each full successful Search result once instead of twice: **50% fewer full-result deliveries**. In the retained **200-turn Workbench fixture**, scalar status messages use less than one twentieth of full-state message bytes: **over 95% smaller status messages**. Workbench also retains its selected connection across New/Clear and avoids a duplicate session scan on Save. These measured work reductions explain a narrower gain; they are not elapsed search-speed percentages. See [Search delivery](optimizations.md#f08ab-prompt-ui-feedback) and [Workbench transfer evidence](optimizations.md#f16af17-ui-preparation-and-workbench-reuse).

4. **The corpus campaign bypasses important optimized paths.** Its [session constructor](../scripts/benchmark-real-corpora.py) launches the verified engine directly with an explicit isolated service/project binding from [the fixture](../scripts/check-service-clients.py). It does not traverse the normal installed launcher or measure editor setup and painting. A tens-of-milliseconds isolated status comparison therefore cannot establish whether the original installed timeout problem was fixed. Treating that campaign as the overall verdict was the wrong measurement emphasis.

5. **Native indexing and reads retain expensive work, and additional safeguards have costs.** Graph/vector/occurrence verification remains strict. Scanning, manifest verification, ownership checks, publication and semantic inference still scale with corpus size. The new storage representations add production, certification, retention and maintenance work. Their individual runtime cost was not counted here, so the 4% indexing regression cannot honestly be assigned to one of them. Query-vector/result reuse, shared worker scalar observations, accounting-reader reuse and other F06b/F07/F12b/F15/F18/F19 scopes remain deferred; this release does not complete the whole optimization plan.

6. **The warm-status regression has a concrete investigation target, not a proven timing attribution.** In the explicit pinned route, [status materialization](../crates/airk-cli/src/status_observation.rs) authenticates pinned scope before consulting the response cache. [Scope verification](../crates/airk-cli/src/project_client.rs) now checks the namespace/revision stamp on both sides of the selected registration read, including fencing generation; the older path checked registration and namespace. That is additional source-confirmed work on the cache-hit path. Counters/profiling are needed to determine how much of the observed extra latency comes from it, registry contention or other work. Do not weaken scope/revision checks to obtain a faster number.

The source supports those mechanisms and remaining costs. It does **not** establish the root cause of every native read error, the graceful-stop timeout, or the share of wall time spent in each stage. The campaign stores redacted error classifications, so exact causes cannot be recovered from those receipts. A follow-up should instrument existing request boundaries for preparation, scope verification, cache hits, selected reads, accounting, serialization and UI arrival, then reproduce specific failures on isolated state. Repeated broad CI runs are not a substitute for that profiling.

## Matched native corpus comparison: secondary throughput and retrieval measures

Both release pairs used identical captured inputs and native default policies, bundled semantic models, disabled watchers, sequential corpora, 120-second command allowances and an unchanged one-hour initial readiness allowance per corpus. Requested cohorts were 30 fresh frontends on a running service and 30 after one healthy restart, followed by cancellation and unchanged reconciliation. Each session requested full responses, one-result inventory/search probes with 4,096-byte budgets and a two-line README excerpt. Query: `configuration`. No hidden successful retry replaced the first response.

The 1.8.2 pair ran first, then 1.7.6. Windows 11/AMD64, 64 logical CPUs and Python 3.13.7 were recorded. No affinity, antivirus or background-load controls and no filesystem-cache eviction were used. Python/extension validation briefly overlapped early 1.8.2 indexing; later native validation began after both corpus campaigns. Physical I/O, query-work counters, modified-source incremental performance and isolated audit interference were not measured. This is a single-host observation, not causal platform qualification.

| Corpus | Git-visible files / bytes | Indexed files / chunks | Final vectors, 1.7.6 / 1.8.2 |
|---|---:|---:|---:|
| AIRK | 1,460 / 58,204,170 | 1,389 / 5,188 | 32,505 / 32,505 |
| BigDotNetProject | 8,250 / 153,987,315 | 8,123 / 65,392 | 33,381 / 27,685 |

AIRK reached full semantic readiness in **50m 7.517s** on 1.7.6 and **52m 6.324s** on 1.8.2: **3.95% longer** in this observation. Both selected/completed 1,387 artifacts. BigDotNetProject reached only **1,024/8,121** selected semantic artifacts on 1.7.6 and **768/8,121** on 1.8.2 before the deadline. Both source jobs succeeded; neither full two-corpus campaign completed. BigDotNetProject's running-service, reopen, cancellation and reconciliation cohorts were not reached in either run.

### AIRK running-service cohort

Read percentiles include usable responses only; native failures are counted separately and never replaced. Each operation requested 30 observations. First/warm-status percentiles include all returned status observations; none failed in these two AIRK cohorts.

| Operation | 1.7.6 p50 / p95 ms | 1.8.2 p50 / p95 ms | Usable 1.7.6 / 1.8.2 | Native errors 1.7.6 / 1.8.2 |
|---|---:|---:|---:|---:|
| Initialize | 46.369 / 50.686 | 46.455 / 52.097 | 30 / 30 | 0 / 0 |
| First Status | 43.373 / 46.150 | 74.587 / 144.934 | 30 / 30 | 0 / 0 |
| Warm Status | 1.837 / 2.533 | 72.392 / 142.034 | 30 / 30 | 0 / 0 |
| Inventory | 74.937 / 78.442 | 124.258 / 131.975 | 30 / 26 | 0 / 4 |
| Lexical | 68.704 / 70.729 | 138.658 / 154.713 | 30 / 27 | 0 / 3 |
| Excerpt | 49.349 / 52.002 | 149.653 / 236.552 | 30 / 26 | 0 / 4 |
| Vector | 415.006 / 422.878 | 472.553 / 568.374 | 30 / 29 | 0 / 1 |
| Hybrid | 1,406.491 / 1,424.913 | 1,464.770 / 1,570.117 | 30 / 28 | 0 / 2 |

### AIRK reopen cohort

One healthy service restart, followed by 30 fresh frontends. The first session after restart is included rather than silently warmed away.

| Operation | 1.7.6 p50 / p95 ms | 1.8.2 p50 / p95 ms | Usable 1.7.6 / 1.8.2 | Native errors 1.7.6 / 1.8.2 |
|---|---:|---:|---:|---:|
| Initialize | 45.437 / 49.170 | 45.164 / 52.124 | 30 / 30 | 0 / 0 |
| First Status | 43.343 / 74.661 | 57.047 / 59.781 | 30 / 30 | 0 / 0 |
| Warm Status | 1.848 / 2.449 | 54.224 / 56.405 | 30 / 30 | 0 / 0 |
| Inventory | 76.745 / 78.991 | 91.991 / 94.968 | 30 / 30 | 0 / 0 |
| Lexical | 69.946 / 72.414 | 82.945 / 90.548 | 30 / 30 | 0 / 0 |
| Excerpt | 49.560 / 52.259 | 62.400 / 65.732 | 30 / 30 | 0 / 0 |
| Vector | 417.458 / 430.323 | 430.197 / 494.570 | 30 / 30 | 0 / 0 |
| Hybrid | 1,407.471 / 1,419.508 | 1,421.484 / 1,437.700 | 30 / 30 | 0 / 0 |

The 1.8.2 running-service cohort retained **14 native read errors out of 150 probes**: four inventory, three lexical, four excerpt, one vector and two hybrid. The corresponding 1.7.6 cohort had zero. Both reopen cohorts returned 150/150 usable probes. The near-equal reopen hybrid latency and slower native status/inventory/lexical/excerpt results support no general native retrieval speedup claim.

### Initial availability, failures and lifecycle

| Corpus / version | First inventory / lexical / excerpt seconds | First vector / hybrid seconds | Failed or unavailable status observations |
|---|---:|---:|---:|
| AIRK / 1.7.6 | 4.164 / 4.199 / 4.232 | 428.256 / 4.380 | 1 |
| BigDotNetProject / 1.7.6 | 4.297 / 4.332 / 4.370 | 1102.335 / 4.522 | 10 |
| AIRK / 1.8.2 | 7.952 / 7.991 / 8.031 | 479.553 / 8.188 | 4 |
| BigDotNetProject / 1.8.2 | 7.428 / 7.461 / 7.495 | 1570.325 / 7.008 | 22 |

First-usable times start at initial index submission and preserve prior errors, empty results and typed no-content observations. Hybrid can return lexical evidence under partial-hybrid policy; first hybrid evidence does not mean semantic readiness. Initial native-error counts remain in the raw reports:

- 1.7.6 AIRK: inventory 0, lexical 0, excerpt 0, vector 7, hybrid 6.
- 1.7.6 BigDotNetProject: inventory 2, lexical 2, excerpt 2, vector 8, hybrid 7.
- 1.8.2 AIRK: inventory 0, lexical 0, excerpt 0, vector 15, hybrid 12.
- 1.8.2 BigDotNetProject: inventory 2, lexical 2, excerpt 2, vector 12, hybrid 11.

AIRK cancellation won in both runs; unchanged reconcile succeeded in both. Both campaigns exited **1 / `campaign_deadline`** during BigDotNetProject readiness. Both retained **failed graceful shutdown waits**, followed by successful owned-process reaping. Subsequent checks found no processes using either benchmark runtime and no remaining index fixture directories. Successful reaping does not relabel graceful shutdown as passing.

## Input, runtime and receipt identity

Before documentation edits, a separate post-run identity check verified that both complete Git-visible source inventories were unchanged and identical across runtime runs. AIRK was a deliberately recorded dirty documentation checkout at `a10254ff49a4905ec35944cd414bba67c59f7a36`; BigDotNetProject was clean at `238b22a722ceb78447d72e39585fd6a7d4d3dd0c`. Public aliases replace private corpus names and paths; original ignored receipts remain unmodified.

- AIRK source identity: `d8119c7b791d4baef98a4b3d490ed0b7650db89fcd9dd6ac3d6a6ebdd7c9f62a`.
- BigDotNetProject source identity: `586b5d59b95022719d5ff855af751b455899acaa48c77045734d71fa0a8e0bc7`.

| Runtime / receipt | 1.7.6 SHA-256 | 1.8.2 SHA-256 |
|---|---|---|
| Install manifest | `04c57b54ad8ecd8a1b78675eea4a01ad4beafae5e626c24a2b84f6ecdb3b87ec` | `6b3942fa9ebd168746281e2ceec8c604913545c10afe95a32ab99fe0cdab8fa7` |
| Main executable | `480c210542c763a7a2ed6cfe4b0260fdbe9d8cd86e381ee7627be995a08df5d5` | `726725f91deaba56ddde11a4caa8eeda93e9bfbfb3ed272de0ba7c4b6077a04c` |
| Embedding helper | `1b8f825976218bcad3f29fd9c420b6b5b42d9510a23d2ad1e0d66e13ce3f7d88` | `5865bbcd62dccb8210313f49a29b40b80964e5610dd9230e300b61e83442a58b` |
| Raw corpus report | `30fad6c6601c7b0a1f2dc9797984c433f6f923f07540f84ac5b14af58187d811` | `150546c7b7b733f941860273b556656274aa33245730e98f8a708c356ea144ba` |

Installed-route receipt SHA-256: `a04c992d31011cdd823eacc59f4f3e74a6d23a5e732898ab8dab59a080a8de92`. Source-verification receipt SHA-256: `0c07e4b06e12a31365f118262c4abb25386c83fef2aacb060daa4b835a7c26f5`.

Both runtime inventories and compiled versions were verified against the retained release-package manifests before execution. Main executables used the lightweight release profile with separate bundled inference helpers. AIRK reported the same semantic pipeline digest in both runs: `734e4a977c38b719e77143aa25bf053eb6099061ca6a8313bb4bb588dafbc4e5`; selection digest `7ab264a290ad80a8286857cb10d699e5e812ddc75c7672d953d32b0caf048fe9`, default `partial_hybrid` policy. No complete BigDotNetProject post-readiness diagnostic identity was collected.

Raw receipts and measurement scripts remain ignored under `artifacts/performance-1.8.2-20261101/`; private source/excerpts/index state are not publication artifacts. To repeat the corpus methodology, use the [runner](../scripts/benchmark-real-corpora.py) with `--runtime-dir`, externally verified `--runtime-sha256`, `--airk`, `--BigDotNetProject`, `--semantic`, `--excerpt-path README.md`, `--samples 30`, `--command-timeout 120` and `--job-timeout 3600`. Runtime, state and report paths must be outside both source roots, with temporary state. A new checkout is a new corpus identity; do not treat it as the captured input automatically.

See the [release record](releases/release-1.8.2.md) for product changes and validation, and the [optimization ledger](optimizations.md) for adopted/deferred scopes and historical evidence. Installed responsiveness, native corpus qualification, correctness checks and publication provenance are separate claims.
