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
| Pause Auto-index / Resume | Persist native pause state and retain pending revisions; explicit indexing remains available |
| Manage Collections / Trim | Preview usage and reclaim plan; remove selected derived state |
| Choose Index Storage Folder | Select a new location; reindex there and retain prior collections without moving/deleting them |
| Show Extraction Policy / Configure Extraction Policy | Inspect configured versus published selection; validate a bounded JSON policy and confirm saving it for the next successful index |

The indexing-mode Quick Pick offers Manual, After edits settle, On save and When idle. Advanced timings remain separate settings. The footer shows active-folder coverage or scanning/indexing/failure/storage-budget state; the tooltip explains queued work and automatic pause. **AIRK: Show Local Usage Report** opens collection-scoped aggregate reports with explicit counterfactual-estimate labels.

Coverage, usage and retention commands open themed report panels with selected metrics as cards and tables. **Open as JSON** preserves the full native diagnostic output. The panel loads no workspace resources and accepts only the fixed JSON-open message; native reports remain observational.

The AIRK Activity Bar view lists active workspaces and indexed collections in the current storage folder. It shows indexing state, measured coverage when available, disk use and the last successful index time. Inline actions index or reindex an active collection, pause auto-indexing for all collections in this window, refresh the view, or remove an inactive indexed collection after the existing confirmation. The view reads native collection metadata; it does not traverse source files on repaint. Item actions take the selected collection, so they are hidden from the Command Palette.

The **Get Started with AIRK** walkthrough in VS Code's Getting Started page links four steps: trust and index the workspace (with optional storage selection), choose an indexing mode, set up a coding agent, and verify the native MCP connection. Its steps complete when the linked commands run; setup still previews configuration for the user to apply in the selected client. The walkthrough uses packaged local illustrations and does not load workspace content.

The [extraction-policy controls](extraction-policy.md) show the configured digest, published applied digest and pending-index state. Configuration captures a private copy of the selected JSON file, validates and previews it, then saves those same bytes only after confirmation. It offers an explicit **Index now** action afterward. Reading policy does not index; saving does not reinterpret existing evidence. The selected file and workspace/client settings are unchanged, and temporary input is removed on cancellation or failure. Existing native adapter capabilities remain the limit of each override.

[Workspace coverage](index-coverage.md) implements the current/eligible file percentage with a native, bounded inventory. Clicking the status item opens grouped AIRK controls, including **Index Now** for the active editor's folder; multi-root workspaces without an active match prompt for a folder. The tooltip shows stale/missing/extraction-failed/capacity counts and unsaved editor documents. Incomplete scans and new disk events show unknown coverage; zero eligibility never becomes 100%. **AIRK: Open Controls** is also a palette command. Its entries use the same names as the palette titles, with a one-line description of each command.

## Scheduler and space behavior

**Show Retention Status**, **Choose Retention Policy** and **Run Retention Maintenance** expose the native [searchable retention policy](retention.md). Policy changes show a preview and require confirmation; maintenance separately previews before a bounded apply. Status includes collection/root/storage, manual/automatic mode and origin, protected bytes, pending work and errors. Upgraded collections retain manual mode until selected otherwise. Existing **Trim Obsolete Index Generations** remains an active-only manual trim and may remove the policy predecessor. No history or checkpoint controls are advertised.

Retention observations refresh after indexing or explicit status actions and are cached for footer display. Repainting never runs cleanup. A failed observation does not mask a successful publication. Retention mode is independent of automatic indexing/pause and never rewrites client settings or tool approvals.

New collections default to automatic retention of active evidence plus one predecessor. Existing collections without a persisted policy remain manual. The effective mode and policy origin are visible in retention status; opening an upgraded workspace does not opt it into automatic cleanup.

Filesystem events, including ignore-rule changes, queue incremental work. Bursts coalesce by collection; rapid agent edits reset the quiet timer and a maximum batch delay bounds debounced/on-save waiting, subject to job completion/minimum intervals. Idle mode also waits for editor activity to settle and deliberately does not use the maximum batch delay to override idleness. On-save mode queues saved documents; external edits wait for Index Now unless an independently started native watch worker is running. Native ingestion remains authoritative for ignores. Editor jobs submit to the [durable native queue](native-scheduler.md), whose operating-system worker lock serializes indexing across hosts sharing a data root. An explicit `schedule run --watch` process drains requests and periodically reconciles registered roots without an editor; no system service is automatically installed.

Edits arriving during a build queue a subsequent pass. Repeated explicit clicks share the pass already handling that revision. Pause prevents automatic jobs and retains queued revisions; an in-progress writer finishes. Explicit indexing and first server startup for a missing collection can run while paused or in manual mode; reconnecting an indexed server retains its snapshot while edits settle. Closing a folder drops its editor queue while allowing submitted native work to finish. Failed jobs preserve native pending state without retry loops. The kernel preserves prior evidence on publication failure, including full rebuilds. Submitted/completed revision watermarks persist across helper exits; event notifications still in the editor's debounce buffer are reconciled by a fresh inventory after reopening.

Coverage refreshes are separate background jobs: one scan at a time after quiet/idle periods, after indexing, at startup and every five minutes. Storage/folder changes invalidate old bindings. Coverage refresh does not restart MCP clients or walk sources during status-bar repaint. Native scan limits and cache identity rules are documented in [workspace coverage](index-coverage.md).

The soft byte budget measures collection-state bytes, not source workspace size or helper-cache bytes. At the limit, automatic indexing blocks until resumed; manual indexing may exceed it. A successful batch can cross the soft limit. Removal uses native validated collection maintenance, protects collections open in this window and refuses active native readers. Users must close/disconnect the collection in other windows/agents first. There is no automatic collection deletion. **AIRK: Trim Obsolete Index Generations** first opens a reclaim preview, then requires confirmation; native generation pruning respects reader leases. Storage relocation preserves the previous collections and builds new state at the chosen location.

## Agent setup and trust

The extension registers a trusted collection-scoped VS Code MCP definition provider. Other agents require separate APIs/configuration. [The agent matrix](agent-integration.md) records the verified routes. Do not assume registering a server in VS Code also configures Claude Code, Codex or another installed agent.

Workspace trust gates native process execution/indexing and MCP definitions. Untrusted windows expose no servers and refuse indexing/setup/maintenance commands. [Workspace trust documentation](https://code.visualstudio.com/api/extension-guides/workspace-trust) provides the integration contract. This is product design, not a request to alter trust or approval settings in the user's current editor.

Helper bytes/version/platform are checked before use. A version/digest-specific per-user cache keeps generated agent launch paths valid after extension-directory replacement. Refresh generated configuration after helper upgrades. External agent settings are previewed, never overwritten or silently enabled. VS Code provider registration does not configure another agent. No extension runtime network service, telemetry or runtime npm dependency is added.

## Remaining acceptance

Test clean installation and helper startup on Windows/macOS/Linux; storage selection and relocation; multi-window collection isolation; 100 rapid edits producing bounded work; changes during indexing; pause/resume; disk-full recovery; collection removal preserving source; and actual MCP links for the target clients. Measure cold-start, CPU/RSS, disk growth, indexing delay and DiskANN recall. Keep runtime behavior in the Rust kernel so the extension remains replaceable.
