# 1.1.3 implementation and qualification: Usage & Savings

Development candidate on `feat/usage`. Native and extension manifests and their lockfiles are synchronized at 1.1.3. This change does not publish a release or change installed agent settings.

## Delivered behavior

| Work item | Implementation | Evidence boundary |
|---|---|---|
| Visible reporting | Collection Usage & Savings button, cached 30-day summary, 7/30-day report, daily table, exports, recording and clear controls | Reads the existing native ledger; no second accounting store |
| Understandable estimates | Plain-language headline, token explanation, full-file comparison, signed percentages, comparison coverage, missing data and expandable developer details | Smaller responses are not proof of lower provider consumption or bills |
| Session evidence | Content-free OpenAI Responses / Anthropic Messages counter adapters, normalized session template, import preview, bounded retained request links and duplicate-link rejection | Explicit final usage objects only; no automatic private log discovery or universal agent integration |
| Task evaluation | Matched-task packaging and repeated-pair comparison script, protocol, quality bins, signed token/time/tool-round differences and separate both-passed summaries | Evaluator attests comparable conditions; fixtures are not representative production evidence |
| Cost workflow | Report-local dated rates, cache assumptions, optional output price, exported scenario, separate session and context equivalents | Unknown cache splits and missing output measurements remain unpriced; no verified billing claim |
| Durable reporting | Atomic bounded native cumulative aggregate surviving daily expiry; clear/reset safety; independent enabled/disabled and multiworker overhead harness | Starts with newly recorded 1.1.3 activity; imported session details remain daily; recording remains partial |

The [user and evaluator guide](usage-evaluation.md) gives exact commands, supported counter schemas, evaluation protocol and limits. The [native accounting contract](usage-reporting.md) retains the collection/root, privacy and budget boundaries.

## Concurrent accounting correction

The initial candidate recorded only 218 of 1,000 successful calls across four independent Windows CLI workers (782 observations missing). Immediate lifecycle-lock failures discarded normal overlapping requests; opening an existing ledger also unnecessarily reserved storage and repeated schema setup.

Usage operations now wait up to two seconds for lifecycle-lock contention. Implicit writes retry shared-quota lock contention within a separate two-second admission/inventory budget. Existing-ledger opens only read and validate scope/incarnation. Collection deletion still acquires its guard without waiting. Quota limits, atomic deduplication, collection/root binding, disabled mode, and rejection of writers from before a clear/recreation remain enforced. Capacity, corruption and unrelated I/O failures are not retried as contention. Operating-system I/O and synchronous commits are not hard wall-time bounded.

A 500 ms intermediate budget passed a short process workload but exposed starvation during the broader Windows test run. The final two-second budget passed the concurrent regression and bounded-stuck-writer tests on both Windows and Linux. A CLI smoke now independently checks 40 successful calls across four processes against both daily and cumulative totals. The load harness's `--require-complete` option fails on any missing or extra observation.

| Windows release-helper workload | Serial calls recorded / submitted | Concurrent calls recorded / submitted | Concurrent calls missing |
|---|---:|---:|---:|
| Before fix, four workers | 1,000 / 1,000 | 218 / 1,000 | 782 |
| Final fix, four workers | 1,000 / 1,000 | 1,000 / 1,000 | 0 |
| Final fix, sixteen workers | 1,000 / 1,000 | 1,000 / 1,000 | 0 |

The final four-worker run began after builds, editor checks and other test workloads finished. Serial medians were 37.68 ms with recording and 19.46 ms disabled (18.22 ms difference). Concurrent calls had a 67.17 ms median and 153.57 ms maximum. All successful calls were recorded, and the final ledger was 163,840 bytes. Both final load runs passed `--require-complete`.

The sixteen-worker run had 197.93 ms median concurrent call time and 982.84 ms maximum, including process startup, retrieval, serialization and recording. Serial medians were 39.41 ms enabled and 20.17 ms disabled (19.24 ms difference). Tests and packaging overlapped that run, so those timings describe a loaded host. Its final ledger was 163,840 bytes. These are synthetic local CLI workloads, not persistent MCP latency, complete production accounting or provider savings. Historical loss remains unknown; reports continue to say partial.

## Validation record

- Windows workspace build, formatting, clippy with warnings denied, and the final complete workspace test run passed: **687 passed, 4 intentionally ignored**, plus doc-tests. The final store regression suite passed all 14 tests.
- Linux x64 Docker workspace qualification passed **694 nextest cases, 4 skipped**, plus doc-tests. That full run used the intermediate wait budget; after the final budget adjustment, the Linux store suite was rebuilt and all 14 tests passed again. This is local container evidence, not macOS qualification.
- Extension suite: **68 passed, 1 skipped**. The skipped old/new-helper upgrade fixture needs a previous helper. Tests use the real helper and cover allowlisted panel messages, stale actions, scope, cancellation, exports, escaping, signed estimates and missing measurements.
- Python contracts: **89 passed, 1 Linux-only test skipped on Windows**. Native usage-input smoke passed, including concurrent subprocess accounting, import preview, retained request links, replay, signed prices, unknown cache splits, disabled no-writes and clearing.
- Native ZIP and VSIX were rebuilt and package-verified. Dependency notices were generated for 466 packages; four previously documented dependencies without standalone legal text remain release-review items.
- Repository and whitespace checks passed. The [eight UI previews](ui/usage-reporting.md) remain renderings of the actual UI with fictional measurements.

## Installed VS Code acceptance

The final Windows VSIX passed all five scenarios in VS Code **1.142.0**, across seven isolated installed-editor windows: shared workspace, separate workspaces sharing storage, multi-root workspace, usage reporting, and untrusted workspace. The verified VSIX SHA-256 is `e381801f88dd5fb8ad8bbbfa1679a4948e236b98662ac4881c29cc4dcccab367`.

The reporting scenario exercised real webview buttons and VS Code dialogs: seven-day selection (also verified in the exported CSV metadata), CSV/Markdown exports, measurement file selection and preview/confirmation, input/cached-input/output prices and their removal, recording off/on, two collection reports, clear cancellation and confirmation, and stale-panel rejection after a storage change. Native readback verified the other collection's supplied measurement survived clearing, and source bytes and both indexes were preserved. Untrusted workspaces rejected indexing and reporting commands, exposed no MCP definitions and left storage untouched. Shared-window scenarios verified MCP reads during indexing and collection isolation.

The harness uses private profiles, fictional sources, VS Code's simple file dialogs and a loopback debugging endpoint. It does not alter the user's editor profile. OS-native dialogs, live trust-revocation/reload, remote hosts and other operating systems are outside this result. See [installed-editor qualification](windows-local-build.md#installed-extension-and-two-window-qualification) for reproducible commands and the optional `AIRK_EDITOR_SCENARIOS` selector.

The final unsigned local packages are under `artifacts/local/usage-1.1.3-ready/native/publish/` and `artifacts/local/usage-1.1.3-ready/vscode/publish/`. They are development candidates; nothing has been published or installed into the normal user profile.

## Remaining release qualification

- Representative agent tasks with matched sources, models, cache conditions and acceptance tests. Synthetic fixtures do not establish production time, token or billing reductions.
- Longer-duration workloads, full-disk and process-kill injection on supported systems, native macOS checks and persistent MCP overhead measurements. Accounting can still be dropped when bounds are exceeded or storage fails; the successful local runs are not a universal losslessness guarantee.
- Provider-specific cache-write pricing before adding those tiers. Current adapters deliberately leave affected input cost unpriced.

The concurrent-loss regression and installed-report acceptance blockers have been addressed within the tested workloads. Native and extension versions and their lockfiles remain synchronized at **1.1.3**; no additional increment is needed for this PR.
