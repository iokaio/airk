# AIRK 1.8.22 local VSIX candidate

**Unpublished Windows x64 manual-test candidate, 10 November 2026.**

1.8.22 corrects [1.8.19](release-1.8.19.md) and [1.8.20](release-1.8.20.md),
which broke the [One AIRK plan](../one-airk.md). After installing 1.8.20 the
maintainer saw two resident `airk.exe` processes. The replacement had also
deleted `airk-launcher.exe` while the workspace's Claude Code and Codex
entries still started it, and Explorer gave no warning. The correction is
recorded in the [release-readiness ledger](../simplify/release-readiness.md).
The user's decisions are at the top of [one-airk.md](../one-airk.md). The
implementation and its evidence are in the
[one-process ledger](../simplify/one-process-ledger.md).

**About 1.8.21.** A 1.8.21 VSIX of this same work was built on 10 November
2026 but never handed out. Its installed-editor check failed after a window
reload, so 1.8.22 corrects it:

- Short operations on an existing installation now wait up to 15 seconds for
  each other instead of refusing at once. These include legacy-cache and
  launcher inspection, HTTP connection lookup, predecessor-cleanup lookup and
  legacy-cache retirement.
- The failure was a read-only check refused with "another AIRK
  installer/launcher is active". A reloaded window and an agent check can run
  such operations at the same moment.
- Long installer operations (stage, activate, login registration) still refuse
  a second installer at once.

1.8.22 keeps the 1.8.19 rule that, until 1.9.0, an installer replaces an
earlier installation instead of upgrading it. The `airk-state/1` baseline
(1.8.16) is unchanged.

## One resident process

Only the service stays resident: one `airk.exe` per user and installation.
Stable, preview and custom installations are separate installations, and each
has its own service.

- **Workers run inside the service.** Project owners, cleanup owners and the
  source watcher are threads and tasks of the service, each project with its
  own runtime, lock and execution scope. Earlier releases started them as
  separate `airk` processes.
- **A stuck owner is quarantined; the service keeps running.** An
  in-process owner cannot be killed. A stopping owner first gets the former
  10-second stop period (5 seconds for the watcher), then is asked to drain at
  its next safe point. If it still has not exited 20 seconds later:
  - its thread keeps that project's lease, so no other owner can open the
    project's storage, and it refuses new work;
  - that project is blocked from starting a new owner. Status shows the
    existing `automatic_start_blocked: true`. The service log names the
    reason (`owner_quarantined_stop_deadline`, `…_work_stalled` or
    `…_query_deadline`). Status does not tell a quarantine apart from an
    exhausted restart circuit;
  - every other project keeps running;
  - an explicit service restart is the recovery. If the stuck thread exits on
    its own, it is reaped and the project returns to normal restart backoff.

  A query that overruns its 125-second deadline and work that the stall
  watchdog finds stuck quarantine the owner at once. A source-watcher pulse
  that goes stale stops the watcher the same way. An earlier draft of this work made
  the whole service exit with code 76 instead. Review found that this could
  crash-loop every project, so it was replaced before release.
- **Allowed temporary processes.** Each one exits when its job or owner ends:
  - one `airk service session` per VS Code window, described below;
  - `airk` commands the user types, and installer one-shots;
  - at most one short-lived PDF parser child for the whole service, in a
    Windows job or Unix process group with 512 MiB memory, 10 s CPU and 15 s
    wall-clock limits. A new parser is created for each document. Other
    documents wait in arrival order, up to 60 seconds. If the slot stays
    busy, that document keeps its previously indexed generation, is retried
    once in the same scan, and is then reported as `extraction_deferred` for
    a later scan. It is never recorded as failed or removed. A waiting
    document stops without starting a parser when its owner drains, is
    cancelled or stalls. On Linux and macOS the parser exits when the service
    dies;
  - a `tools shell` terminal the user opens.

`scripts/check-self-spawn.py` checks process starts:

- It fails when production code starts its own executable for any purpose
  other than the reviewed PDF parser and the listed legacy paths.
- It fails when production code adds any process start that is not on its
  reviewed list. That includes a start whose image path arrives through a
  parameter.

Hosted owners share one process, and three changes follow from that:

- **Usage spans.** A span that overlaps another owner's job, or a request
  with usage recording disabled, is reported with the existing
  `overlapped: true` and carries no counters, instead of attributing
  service-wide counters to one project. Source-watcher pulses are not
  counted as overlap.
- **Shared caches.** A cache whose lock was poisoned by a panic in one owner
  is discarded and rebuilt rather than failing every project.
- **Releasing a project's cache entries.** When a project or cleanup owner
  exits, its collection's cache entries and change-notification handles are
  released.

### One session per VS Code window

The extension no longer starts one `airk.exe` per operation. Each window runs
one `airk service session`. The session relays control requests and runs
allowlisted CLI commands in-process, with the same output as the one-shot
command. Responses can be up to 16 MiB, the former one-shot buffer.
`http-credential` and `runtime launch` are refused inside the session.

The session exits when its window's extension host ends. One exception is
documented: when its owner is lost while installer or other exclusive work has
already started, the session lets that work finish, but never past the
request's own deadline (at most 600 seconds). Read-only work is cancelled at
once. Cancellation is cooperative.

The extension still starts a separate one-shot when a different runtime build
is involved or when no window session exists. It also starts the uninstall
worker and the `tools shell` terminal.

## The service keeps running

- **Login startup on every install.** Login startup is registered on every
  installation, replacement and upgrade. Earlier releases registered it only
  on a fresh installation. The exception is a user who ran
  `airk runtime login uninstall`: that writes `startup-opt-out.json` in the
  installation folder, reported as `startup_manager.user_opted_out`, and an
  upgrade honors it. `runtime login install` clears the record. This is a new
  installation-folder file, not a change to the `airk-state/1` format.
- **No idle exit in HTTPS mode.** The service no longer stops when idle. The
  1.8.20 Explorer workaround that restarted an idle-exited service is removed.
  A narrower recovery remains for a service that no OS manager will restart,
  because registration was refused or is absent. If such a service ended
  without a Stop request, the next status check starts it once for that
  stopped instance. It never does so after an explicit Stop.
- **One service per installation.** A second `serve`, `managed-serve`,
  `foreground` or `host` start is refused with a `service_already_running:`
  message. The JSON error `code` is still `operation_failed`.
- **The CLI starts the service through the OS manager.** If login startup is
  registered and the service is stopped, a CLI command starts it through the
  OS manager and never spawns `serve` itself. Without a registration, the CLI
  starts the on-demand service, which then becomes the single service.

## Agents connect directly over HTTPS

Agents connect to the service at `https://127.0.0.1:<port>/mcp`. No process
is started for each agent, and AIRK no longer writes stdio MCP entries.

The bearer token is delivered as follows:

- **Claude Code and Codex.** Claude Code's `headersHelper` and Codex's
  `mcp_servers.<id>.http_headers_helper` run
  `airk runtime http-credential --install-dir <dir> --connection-id <id> --format header-json`.
  That one-shot prints `{"Authorization":"Bearer <token>"}` and exits. It
  reads only this installation's owner-private credential file. It never
  starts the service, enrolls, rotates or logs the token. Agents started
  outside VS Code therefore receive their token without an environment
  variable. When the runtime changes, the extension refreshes the helper
  commands that it owns.
- **VS Code / Copilot Chat.** VS Code gets the token through the extension's
  own MCP provider, or through the `airk.httpCredential` command input in
  `.vscode/mcp.json`.

## The replacement installer reconnects agents

The pre-1.9.0 replacement now follows one-airk.md §3, §9 and §10.3. The 1.8.19
statement that agent configurations outside VS Code are not edited is
reversed. That narrowing had been made during implementation without the
user's approval.

The replacement works in this order:

1. **Prove ownership** of every location first. A layout folder outside the
   default locations needs an AIRK owner record. An installation record that
   exists but cannot be read is refused, and nothing changes.
2. **Lock.** A `.<leaf>.replace-1.lock` beside the installation refuses a
   concurrent replacement before any side effect.
3. **Remove the owned login registration.**
4. **Retire the owned user CA** through its trust receipt.
5. **Stop, move aside and delete** the old folders. If a move fails partway,
   the moved folders are restored, newest first. If that rollback fails, every
   folder left aside is reported.

The plan and the result carry `client_references`: the executables, the old
endpoint and its CA that the removal breaks. Before `--apply`, the extension
saves the affected agent entries to a reconnect ledger
(`airk.reconnect.ledger.1`). It covers both project files and user-level
files. Nothing is written before that capture. The extension then retires the
replaced host artifacts. The extension still replaces a pre-1.9.0 installation
without a prompt, as the user decided for 1.8.19.

## Explorer warns about broken agent entries

Explorer checks agent entries in:

- project files: `.mcp.json`, `.codex/config.toml` and `.vscode/mcp.json`;
- user-level files: `~/.claude.json`, `~/.codex/config.toml` and the VS Code
  User `mcp.json`, but only entries that AIRK provably wrote;
- the reconnect ledger.

It warns once per set of findings when the window starts, and again after an
installation. Findings appear in an Explorer banner, the status bar and
Management → Agents. Each entry is classified as one of:

| Class | Meaning |
| --- | --- |
| `stdio_retired` | A stdio AIRK entry; stdio MCP is retired. |
| `stdio_missing_command` | A stdio entry whose absolute command no longer exists. |
| `https_unknown_connection` | An HTTPS entry naming a connection this installation does not know. |
| `https_endpoint_mismatch` | An HTTPS entry whose URL differs from the connection's endpoint. |
| `https_trust_missing` | The client is missing the issuer trust. |
| `https_helper_missing` | The credential helper's program is missing. This is repaired by the helper refresh, not a full Reconnect. |
| `ok` | No problem found. |

**Reconnect** (also **AIRK: Reconnect Coding Agents…**) shows what it will
change, including any index it will create, and asks once. For each recorded
folder, it then:

1. creates the index through the reviewed Indexing setup, or uses an existing
   one;
2. grants this window access. **Side effect:** this makes the index the
   window's search scope;
3. enrolls HTTPS;
4. rewrites each entry in place, keeping other servers, comments, disabled
   states and policies;
5. provisions trust and the token helper.

Each entry reports one of these statuses:

- `complete`;
- `awaiting_host_reconnect`, with the host's action, such as Claude Code `/mcp`
  → Reconnect or restarting Codex;
- `awaiting_index`: a step of the index chain was refused. Its native code is
  shown and no file changed;
- `blocked_unknown_owner`;
- `blocked_client_compatibility`;
- `recovery_required`.

AIRK never claims that a running agent reconnected.

## Verified so far

All results are from Windows. Receipts are in `artifacts/one-process/` and are
listed in the [one-process ledger](../simplify/one-process-ledger.md).

| Area | Receipt and result |
| --- | --- |
| Session protocol contract | `session-contract-r2`: 5 selected, 5 pass. The first run, r1, did not compile. |
| CLI output sink (golden capture) | 60 offline runs: 56 byte-identical. The other 4 (`runtime --help`, `service --help`, two runs each) differ only by the new `http-credential` and `session` rows. |
| `airk service session` | `efj-tests-r2`: 22 selected, 22 pass. `efj-clippy-r3`: pass. A real-binary test checks four in-session runs byte-equal to the one-shot output, refusal of `serve` and exit on EOF. |
| Session drain on owner loss | `owner-clippy-r1`: pass. `owner-tests-r1`: 19 selected, 19 pass. |
| Native review corrections (replacement order, rollback, lock, refusals, 16 MiB budget) | `review-clippy-r1`: pass. `review-tests-r1`: 96 selected, 96 pass. |
| Extension: one session per window | `ext-d-full-r2`: 1,379 selected, 1,358 pass, 0 fail, 21 skipped. This was tested against a fake child and the wire fixture, not the real binary. |
| Agents: warning and Reconnect | `t0-r3` (agentReconnect and agentConfigHealth): 31 selected, 31 pass. |
| Workers in the service (worktree) | `final-clippy-2`: clean. `final-cli-workers`: 220 run, 214 pass. `h4-parity`: 140 selected, 131 pass. The 15 failures also fail at unmodified HEAD (fixture location). |
| Integration | `integ-compile-r2`: pass. `integ-ext-full-r2` (full mocked extension suite): 1,382 selected, 1,361 pass, 0 fail, 21 skipped. |
| Quarantine, drain, PDF deferral, caches and usage (scoped Clippy and the combined native batch, main checkout) | `integ-clippy-r3`: pass. `integ-tests-r2`: 1,863 selected, 1,862 pass, 1 fail. The failure was a new test with a wrong premise; the rewritten test passes (`integ-t0-r1`: 13/13). The 15 earlier worker-test failures do not recur. |
| Install-lock wait (1.8.22) | `lockfix-tests-r1`: 36 selected, 36 pass. |
| Extension suite (pinned Node 24.15.0) and repository checks | `integ-ext-full-r5`: 1,382 selected, 1,361 pass, 0 fail. `integ-check-repo-r2`: pass. |
| 1.8.22 packages | Runtime `airk.exe` `479e1a6e…`; `airk-1.8.22-win32-x64.vsix` `01e342b3…`, 939 files. |
| Installed Explorer | `installed-explorer-r2`: pass. |
| Installed native editor | `installed-native-r2`: `same` and `different` pass; **`multi` failed** in the UI automation at the Help Quick Pick (see the ledger's root-cause note); `usage` and `untrusted` did not run. |
| No worker or stdio processes on a real runtime | Topology watches during the 1.8.21 and 1.8.22 journeys (`topology-watch-native-r1`, `-r2`) saw only the service, window sessions, the self-test child and one-shots. |

## Not yet verified

These items are **pending**:

- The installed `multi`, `usage` and `untrusted` editor journeys on 1.8.22.
- The remaining T2 process-topology checkpoints: idle, two windows, the
  service stopped while Explorer polls, PDFs, a window closed, a CLI burst,
  `airk serve` refused, and a CLI start through the OS manager.
- Replacement over 1.8.20, the warning and Reconnect in an installed editor.
- The journey harness does not unregister the login startup that a fresh
  fixture install now registers. Those tasks were removed by hand.
- The extension's session client against the real binary.
- Real-client checks:
  - whether Codex trusts the OS user certificate store outside VS Code;
  - which Windows shell runs the Claude Code and Codex header helpers.
- Linux and macOS. Nothing was built or tested there. The session's Unix
  stream-redirect code has not been compiled. Unix cleanup after an abrupt
  service death is not implemented. The macOS login adapter remains
  unqualified.

## Known limits

- A session request reports success-only exit status, and non-UTF-8 standard
  output is converted lossily.
- Cancellation is cooperative: cancelled work can continue briefly after it
  is answered `cancelled`.
- Single-instance enforcement is per control root. Stable, preview and custom
  installations are separate by design.
- Project owners still reach the service through in-process pipes. Direct
  in-memory calls (H7) are deferred and involve no extra process.
- The hidden worker role subcommands remain for their test callers (H10).

## Version

Native, extension, lockfiles and the template catalog are synchronized at
1.8.22 just before the installer build. Check them with
`python scripts/release-version.py check --expected 1.8.22`.
