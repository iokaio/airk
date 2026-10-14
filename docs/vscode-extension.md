# Native VS Code extension preview

An independent thin native-helper extension is implemented under [packages/vscode](../packages/vscode). It wraps the Rust kernel and bundles the native helper in platform-specific VSIX packages. It is an unsigned preview, not a Marketplace release. [Windows local build/review instructions](windows-local-build.md) provide the single PowerShell command and an isolated editor test.

[Deployment and packaging](extension-deployment.md) adapts VCP's delivery practices; [CI](ci-and-builds.md) builds matching native and extension candidates on all three operating systems. Actual observed checks remain in [validation](validation.md).

## Runtime and distribution

A Node extension host can launch the native AIRK process. Workspace extensions run alongside workspace files; remote windows need the helper on that remote host, not a local path pretending to read remote source. A pure browser extension cannot directly host the current native stack. This follows the documented [extension host model](https://code.visualstudio.com/api/advanced-topics/extension-host).

Package platform-specific helpers for Windows, macOS and Linux. VS Code supports target-specific VSIX packages, including Windows/Linux x64 and ARM64 plus macOS x64 and ARM64. Bundle the right executable and dependency notices so users do not install Docker, Rust, C/C++ tools or Python. Extension updates must preserve a stable helper launch path for separately configured agents. Platform and macOS signing/notarization requirements still need packaging qualification. [Platform packaging documentation](https://code.visualstudio.com/api/working-with-extensions/publishing-extension) supports this delivery approach; the ease-of-use assessment is our inference.

## Storage and collection UI

Use machine-scoped settings for user-selected storage. A folder picker should allow another local drive; defaults can use the extension's per-user global storage. Keep the chosen absolute path out of shared repository configuration. Show its free space, total AIRK usage and each collection's current/reclaimable bytes, last index, queued work, root binding and extraction status.

Each workspace folder gets a stable collection ID. Multi-root workspaces initially register one MCP server per root; a collection spanning multiple roots requires the root inventory described in [collections](collections.md). Choosing a common data directory never authorizes cross-collection retrieval.

Implemented settings and commands:

| Control | Proposed default / behavior |
|---|---|
| airk.storage.path | Machine-scoped user-selected absolute folder; defaults to extension user storage |
| airk.index.mode | debounced; choices manual, onSave, debounced, idle |
| airk.index.quietMs | 2000 ms after the latest eligible change |
| airk.index.maxDelayMs | 30000 ms before offering/beginning a queued bounded batch |
| airk.index.minIntervalMs | Avoid repeated rebuilds during bursts |
| airk.index.idleMs | 10000 ms without editor activity in idle mode |
| airk.storage.maxBytes | User-selected soft total budget; never silently deletes active evidence |
| Index Now / Reindex Collection | Immediate queued incremental update / deliberate full rebuild |
| Pause Auto-index / Resume | Stop scheduling without losing pending changes |
| Manage Collections / Trim | Preview usage and reclaim plan; remove selected derived state |
| Choose Index Storage Folder | Select a new location; reindex there and retain prior collections without moving/deleting them |

Offer a single Quick Pick for Manual, After edits settle, On save and When idle. Expose advanced timings separately. A status item should distinguish current, queued, indexing, partially indexed, paused, capacity-limited and failed.

[Workspace coverage](index-coverage.md) adds the requested current/eligible file percentage and click-to-update footer to the delivery plan. It needs a native eligibility/freshness inventory; the existing scheduler-state footer cannot infer that percentage from snapshot counts.

## Scheduler and space behavior

Filesystem events, including ignore-rule changes, queue incremental work. Coalesce bursts by collection; rapid AI changes reset the quiet timer and a maximum batch delay bounds debounced/on-save waiting, subject to job completion/minimum intervals. Idle mode also waits for editor activity to settle and deliberately does not use the maximum batch delay to override idleness. On-save mode queues saved documents; external edits wait for Index Now. Native ingestion remains authoritative for ignores. Jobs run serially per extension host and native writer locks protect collections across hosts/windows. A machine-wide scheduling daemon is still future work.

Queue edits arriving during a native build for a subsequent pass. Pause prevents new automatic jobs and retains queued revisions; an in-progress writer finishes. Explicit indexing and a first server startup for a missing collection can run while paused or in manual mode; reconnecting an indexed server retains its snapshot while edits settle. Closing a folder drops its queued work while allowing an in-flight native write to finish. Failed jobs preserve pending state without retry loops. The native kernel preserves the prior snapshot on failure, including `index --full` rebuilds. Source digest-based extraction reuse exists; embedding reuse and persisted event watermarks do not.

The soft byte budget measures collection-state bytes, not source workspace size or helper-cache bytes. At the limit, automatic indexing blocks until resumed; manual indexing may exceed it. A successful batch can cross the soft limit. Removal uses native validated collection maintenance and protects collections open in this window. Users must close/disconnect the collection in other windows/agents first. There is no automatic collection deletion. Generation/cache pruning, hard quotas, transactional storage migration and machine-wide active-reader tracking remain planned.

## Agent setup and trust

The extension registers a trusted collection-scoped VS Code MCP definition provider. Other agents require separate APIs/configuration. [The agent matrix](agent-integration.md) records the verified routes. Do not assume registering a server in VS Code also configures Claude Code, Codex or another installed agent.

Workspace trust gates native process execution/indexing and MCP definitions. Untrusted windows expose no servers and refuse indexing/setup/maintenance commands. [Workspace trust documentation](https://code.visualstudio.com/api/extension-guides/workspace-trust) provides the integration contract. This is product design, not a request to alter trust or approval settings in the user's current editor.

Helper bytes/version/platform are checked before use. A version/digest-specific per-user cache keeps generated agent launch paths valid after extension-directory replacement. Refresh generated configuration after helper upgrades. External agent settings are previewed, never overwritten or silently enabled. VS Code provider registration does not configure another agent. No extension runtime network service, telemetry or runtime npm dependency is added.

## Remaining acceptance

Test clean installation and helper startup on Windows/macOS/Linux; storage selection and relocation; multi-window collection isolation; 100 rapid edits producing bounded work; changes during indexing; pause/resume; disk-full recovery; collection removal preserving source; and actual MCP links for the target clients. Measure cold-start, CPU/RSS, disk growth, indexing delay and DiskANN recall. Keep runtime behavior in the Rust kernel so the extension remains replaceable.
