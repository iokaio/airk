# AIRK 1.5.11 development record

October 28, 2026. Development continues on `feat/manageindices` after the local [1.5.10 candidate](release-1.5.10.md). Native and extension versions and their lockfiles are synchronized at 1.5.11. A Windows x64 installer has been built and verified locally for manual testing; nothing has been published. Earlier validation receipts apply to their original binaries.

## Agent configuration correction — implemented in source

A fresh index can coexist with a project-local agent file retained from an earlier installation. The old file contains both its old data root and old native project identity. Automatic migration intentionally requires a matching identity, but explicit setup reused that restriction and offered no successful route to replace the old binding. Its Windows editor save also compared generated LF text against an editor-normalized CRLF document, and unchanged updates reported success without checking the connection.

Agent setup now resolves the selected source/collection against all pages of current native registration metadata, with revision and capacity bounds. It repairs an obsolete editor reference only when the service has exactly one matching active registration, preserves a still-valid selection, and refuses ambiguous or unavailable results. Setup revalidates before configuration writes. Explicit Codex updates and supported JSON installation can replace the data root and native identity together for the same source, collection and recognized launcher; background migration remains strict. Configuration changes preserve other settings and guards for redirected files, unsaved edits and concurrent changes. No native project registration or indexing operation is performed by this correction.

Codex saves preserve Windows newlines, read back the disk file, and automatically test AIRK. The guide distinguishes changed from already matching configurations and reports connection failures while retaining successful saves. Errors are also surfaced through VS Code. The update button is available when Explorer assistance is disabled. The existing separate guide now shows progress while resolving its project; the complete inline design below remains a proposal.

Earlier validation of the configuration correction passed 353 extension tests with 12 skipped. That receipt precedes the startup changes below.

## Responsive startup and connection diagnostics — implemented in source

Extension activation registers the UI and commands and returns before awaiting native startup. Explorer can render while discovery is deliberately blocked; a shared startup banner shows the current step and elapsed time. Opening a Management route also reveals the panel before connecting. Closing the window fences late startup completions.

Within one extension host, concurrent discovery requests share one operation and receive isolated observations. The verified bundle self-test runs once per host. A completed installation checks authenticated service health and the exact runtime version/payload before reusing the running service, without another native `ensure`. Explicit Stop remains authoritative. Missing/mismatched health, managed lifecycles and unfinished transitions use the existing native lifecycle path and per-installation locking. Completed startup reuses its verified inspection to publish registrations, rather than repeating discovery just to finish activation. Definitions reuse current workspace bindings, with invalidation when folders or runtime configuration change.

This removes repeated work and the UI dependency on native verification. It does **not** add a persistent integrity cache or skip the initial full payload check on each new extension host. Native launcher verification is unchanged. Absolute startup time and simultaneous real editor windows on other platforms remain unqualified.

Startup validates each saved source/collection against current native registrations before migrating agent files. This prevents a stale editor receipt from overwriting a manually corrected connection after an index storage replacement. Recognized exact configurations remain unchanged; other client settings, trust and approval policies are preserved.

Management → Agents now describes the unified 26-tool endpoint. Check AIRK connection uses the selected project, immediately publishes preparation, then reports handshake, tool-list and status stages. Duplicate clicks share one operation; failures and successes remain visible with duration and Copy connection details. Errors retain the failed protocol method and bounded launcher stderr. Slow successful handshakes explicitly warn that an agent may have a shorter startup deadline. Checks enqueue no indexing jobs.

A passing check establishes only the extension's own MCP connection. It does not establish that another client loaded its configuration. The verification prompt now asks assistants to discover deferred tools and call `status` before declaring AIRK unavailable. Both Codex and Claude Code were subsequently reported working on the user's other machine with the corrected 1.5.10 configuration; that report does not qualify the 1.5.11 candidate.

## Published content and current work — implemented in source

Overview and Management → Indexing now share an **In your index** card followed by current work. The card reports published files, text chunks, vectors and completed/selected semantic files. A file-coverage bar does not imply overall completion or time remaining. Source discovery and pending source updates remain separate. One complete, dated MCP snapshot is used without mixing counts from other publication revisions; cached console metadata remains the fallback before it arrives.

Vector jobs show semantic units, committed files and batches, rather than leading with zero source capture/extraction counts. Semantic preparation explains retained inputs and batch publication. Cleanup is an explicit highlighted stage and retains published content above it. Attempt time is labeled, unknown counts remain unknown, completed semantic units may include retained work, and failed attempts say failed. Routine running banners are folded into work; failures, waits and recovery actions remain visible. Copy details includes the index snapshot and semantic work counters. Overview moves project identities and model details into a secondary disclosure.

The remote 1.5.10 status supplied during this work reported complete discovery of 8,158 files, 65,404 chunks, 9,805 published vectors and 160 of 8,156 semantic files completed. Its current vector job had published one batch and was not stalled. This established misleading UI counters, not a failed fresh index. This change does not claim to diagnose or improve the reported semantic computation cost.

## Local validation

Windows x64 source checks passed: Rust formatting, workspace Clippy with all targets/features and warnings denied, and the full workspace test suite (1,430 passed, 17 ignored, including doctest groups). The script suite passed 177 tests with four skipped. The extension compile/typechecks and tests passed 366 with 12 skipped before supplying a built native runtime. The extension suite then passed 376 with two skipped against the exact 1.5.11 release runtime: POSIX startup and the separate old/new helper replacement fixture were not run. Repository checks, synchronized-version verification and whitespace checks passed; AGENTS.md and CLAUDE.md remain byte-identical.

Regression coverage includes activation and Explorer rendering while native discovery is deliberately blocked, closing during startup, healthy service reuse without another ensure, explicit Stop, shared discovery with isolated observations, late configuration changes, semantic preparation/cleanup counters, failed versus unfinished attempts, publication consistency, and copyable diagnostic snapshots. A deliberate review pass of the combined diff was performed without delegation. Desktop and narrow rendered webview fixtures were inspected in a headless browser; these fixtures are not installed-editor acceptance or measurements on the other machine.

The built runtime passed the real offline model/helper and persistent DiskANN qualification, including default/vector/hybrid retrieval, opt-out and re-enable, invalid-input rejection, revision identity, installed-file immutability and source preservation. Native archive smoke tests and VSIX inventory, version and payload checks passed. The final VSIX was also opened to verify its 1.5.11 extension/runtime metadata and new dashboard/startup code. No cross-platform, installed-editor acceptance or remote-machine startup latency claim follows from these local tests.

Local installer receipt:

- File: `artifacts/local/20261028-144538-1.5.11-713c4a0b/vscode/publish/airk-1.5.11-win32-x64.vsix`
- Size: 119,998,485 bytes.
- SHA-256: `2846091799180aacefbcbf3c2244926432851e9f252bfebdb42a9a93a30b9639`.
- The adjacent `manifest.json` and `SHA256SUMS` bind the packaged extension to its native candidate. `vector-check.json` in the build directory records real-model qualification. This is an unsigned local candidate from the working branch, not a Marketplace release.

## Agent connection UX proposal — not implemented

The original agent picker awaited `AgentSetupHost.chooseSpec()` silently before creating a separate guide panel. The correction above adds visible progress and avoids unnecessary discovery for an already loaded workspace. The Agents overview and check feedback are implemented, but replacing the separate setup guide with the complete inline flow below remains proposed.

Proposed behavior:

- Keep the supported-agent picker. Selecting an agent immediately shows a setup card inside Management → Agents. Commands invoked elsewhere navigate to this same view immediately, before native work begins.
- Bind setup to the project selected in Explorer. Render its cached identity immediately and validate it before writing configuration. Project changes, lost trust and cancellation invalidate pending results; late completion must never open a panel or change the new selection.
- Show explicit stages such as “Checking Claude Code configuration…” and “Checking the AIRK connection…”, elapsed time and a stop-waiting action. Update the card automatically, with a target of showing the selected agent within 200 ms before slow work. Use one bounded operation deadline; explain slow or failed checks in the card and provide retry and copyable details. Do not start a new attempt automatically after an uncertain configuration write.
- Inspect configuration first and offer one primary action: “Connect Claude Code”, “Update AIRK connection”, or “Check connection” when its configuration already matches. Preserve other servers, policies and unsaved edits. Conflicts get a specific review action; unsupported host/client combinations get manual guidance in the same card.
- After an explicit connect/update action, save the configuration and automatically test the same configured MCP endpoint. Configuration saved, AIRK responding, and the external agent having loaded AIRK remain distinct observations. Show exact client-specific next steps without claiming the extension can observe another agent's session.
- Report indexing readiness separately. An index that is still being built must not prevent configuring the agent or listing MCP tools; explain whether search results are available while indexing continues.
- Keep paths, collection IDs, JSON/TOML, raw diagnostics and response-mode preferences in secondary details. Editor tabs, configuration files and external MCP controls open only from clearly labeled user actions. Use the same unified tool description throughout.
- Reuse already validated installation observations with invalidation on runtime selection, workspace/binding, trust or configuration changes. Opening the setup UI must not await the full runtime diagnostic sequence or index completion. Revalidate the binding before mutations and preserve current ownership/verification checks.

Acceptance should cover a deliberately delayed runtime response, existing/correct configuration, first setup, conflicts and unsaved files, cancellation with a late result, project switching, duplicate clicks, service unavailable, configuration saved but verification failed, and distinct Claude Code/Codex/Copilot next steps. No delayed automatic panel opening, invented progress percentage, or unsupported “agent connected” success state is acceptable.

The complete inline UI and its acceptance scenarios remain future implementation work; the configuration correction above has its own tests.
