# AIRK processes and ownership

## Current process model (1.8.22)

> **Split Phase 4 (non-shippable interim).** The one-resident-process rule is
> unchanged. `airk` is now the service only: `airk` with no arguments, or
> `airkctl service start`, starts it detached as a private role. `airkctl` is
> the CLI, so typed commands and the Claude Code and Codex credential helpers
> (`airkctl agents credential`) are `airkctl` one-shots. The per-window helper
> becomes `airkctl session`; on this branch the extension still starts the
> retired `airk service session` until split Phase 6. The table below keeps the
> 1.8.22 names. See the [split plan](split/split-plan.md) §4.3 and §5.

**Exactly one `airk.exe` stays resident per user and installation: the
service.** This was decided by the user on 10 November 2026 (see the top of
[one-airk.md](one-airk.md)). The implementation and its Windows evidence are
in the [one-process ledger](simplify/one-process-ledger.md) and the
[1.8.22 record](releases/release-1.8.22.md). Linux, macOS and the installed
process-topology check are still pending.

Stable, preview and custom installations are separate installations. Each has
its own service, so a machine with two installations has two services.

### What runs where

| Process | Purpose | Owner and lifetime |
|---|---|---|
| The service (`airk __airk_role service-login` from login startup, or started on demand by `airk` with no arguments or `airkctl service start`) | HTTPS MCP endpoint, metadata broker, durable jobs, resource admission, connection grants and shared MiniLM inference. It also hosts every project owner, cleanup owner and the source watcher. | One per installation. It never exits when idle in HTTPS mode, keeps running after VS Code closes and starts again at sign-in. A second `serve`, `managed-serve`, `foreground` or `host` start is refused with a `service_already_running:` message (JSON `code` `operation_failed`). |
| `airkctl session` | The window's native helper. It relays control requests and runs allowlisted CLI commands in-process, so a window starts one process instead of one per operation. | One per VS Code window. It exits when its window's extension host ends (see the exception below). |
| `airkctl` and installer one-shots | Commands that a user types, and installer steps. Claude Code and Codex credential helpers (`airkctl agents credential … --format header-json`) are one-shots too. | Exit when the command finishes. |
| PDF parser child | Isolated extraction of one PDF document. | At most one for the whole service, described below. |
| `airkctl tools shell` | Interactive terminal tool. | Opened and closed by the user. |

Project owners, cleanup owners and the source watcher run on service threads.
Each project owner has its own runtime, lock and execution scope. The
per-project query endpoint keeps its name and protocol and is served by the
service process. Earlier releases ran these as separate `airk-index`,
`airk-source` or `project-worker`/`derived-worker`/`source-worker` processes.
The service no longer starts them, and project owners never start a Unix
`process-guardian`.

**Agents start no AIRK process.** They connect directly to the service at
`https://127.0.0.1:<port>/mcp`. There is no launcher, stdio frontend or proxy
per agent. See [agent setup](agent-integration.md).

### The PDF parser

The only `airk` child the service creates is the short-lived PDF parser.

- It runs in a Windows job or Unix process group, limited to 512 MiB memory,
  10 s CPU and 15 s wall-clock time.
- A new parser is created and torn down for each document.
- At most one parser is alive in the whole service. The next document waits
  in arrival order until the previous parser has exited and been reaped.
- A document waits up to 60 seconds for the slot.
- **If the slot stays busy**, the document is deferred rather than failed:
  - It keeps its previously indexed generation, if the root binding is
    unchanged, and is retried once later in the same scan.
  - If the slot is still busy then, it is reported with the skip code
    `extraction_deferred`, and a later scan, which sees it as stale or missing,
    extracts it again.
  - A deferred document is never recorded as failed or as removed.
  - Provider-analysis inventory verification refuses a scan that left a
    document deferred.
- **A waiting document stops within 50 ms, without starting a parser, when its
  owner drains, is cancelled or stalls.** The scan stops at that document and
  settles nothing for it. The job reports the owner's own drain, cancellation
  or stall.
- **On Linux and macOS the parser exits when the service dies.**
  - Linux: `PR_SET_PDEATHSIG` plus a parent re-check.
  - macOS: a kqueue process-exit watch.
  - Windows: the job object's kill-on-close.

`scripts/check-self-spawn.py` fails if production code starts its own
executable for any other reason, or adds any process start, including one
whose image path arrives through a parameter, that is not on its reviewed
list.

### A stuck owner is quarantined

An owner running inside the service cannot be killed. When it is stopped:

1. it gets the former stop period: 10 seconds for a project or cleanup owner,
   5 seconds for the watcher;
2. it is then asked to drain at its next safe point, which it observes in
   accepted work and equally during startup, activation and reads;
3. if it still has not exited 20 seconds later (30 seconds in all; 25 for
   the watcher), the owner is **quarantined**.

Stopping never blocks the supervisor: stopping owners are polled on its
regular turn.

**A quarantined owner:**

- keeps its thread and its project lease, so no second owner can open that
  project's storage;
- refuses new queries, grants, jobs and source changes;
- leaves its native state unsettled for recovery, as a kill did.

**Its project is blocked from starting a new owner.**

- Status uses existing fields: the project's worker shows
  `automatic_start_blocked: true`, `identity: null` and `restarting: false`.
  A quarantined watcher shows `blocked: true`, `predecessor_draining: true`
  and `available: false`.
- Status does not tell a quarantine apart from an exhausted restart circuit.
- The service log names the reason, in a `worker_failed` record whose code is
  one of:
  - `owner_quarantined_stop_deadline`;
  - `owner_quarantined_work_stalled`;
  - `owner_quarantined_query_deadline`.
  For the watcher it is a `source_worker_failed` record with
  `watcher_quarantined_stop_deadline`.
- A project restart request returns `project_owner_quarantined`.

**Every other project keeps running.** The recovery is an explicit service
restart.

**If the stuck thread later exits on its own**, it is reaped, its lease is
released, a `worker_stopped` record with `owner_quarantine_released` is
written, and the project returns to the normal restart backoff.

Two other conditions quarantine the owner itself, at once:

- a query that overruns its 125-second deadline;
- accepted work that stopped advancing and did not drain after cancellation
  (the stall watchdog).

A source-watcher pulse that goes stale stops the watcher as above.

A child-process owner, kept only for test fixtures, still exits with codes
75 and 76 in these cases.

An earlier draft made the whole service exit with code 76 so that the OS
manager restarted it. That was replaced because one stuck project could
crash-loop all projects.

### The window session's exception

On owner loss (the extension host exited), the session accepts nothing more
and cancels read-only work. It lets installer or other exclusive work that has
already started finish, but never past that request's own deadline, at most
600 seconds. Then it exits even if the work has not stopped. This is a
documented exception to "the session exits with its window". Cancellation is
cooperative.

The extension starts a separate one-shot only when a different runtime build
is involved or when no window session exists. It also starts the uninstall
worker and the `tools shell` terminal.

### Inspecting processes on Windows

Inspect names together with command lines, executable paths, creation times
and parent identities:

```powershell
Get-CimInstance Win32_Process |
    Where-Object Name -Like 'airk*.exe' |
    Select-Object Name, ProcessId, ParentProcessId, CreationDate, ExecutablePath, CommandLine
```

An expected snapshot has:

- one service row per installation;
- one `service session` row per open VS Code window;
- at most one PDF parser row;
- any short-lived command or `tools shell` that a person started.

Correlate parent creation times as well as PIDs, because Windows reuses PIDs.
A detached service legitimately outlives its original parent. These would
indicate a lifecycle problem:

- a session that remains after its window closed and its drain deadline
  passed;
- a second service for the same installation;
- more than one PDF parser alive at once;
- a project reported as quarantined (a stuck owner); restart the service
  explicitly;
- a resident role process (`airk-index`, `airk-source`, a launcher or an MCP
  frontend) started by a 1.8.22 runtime.

## History: before 1.8.22

The rest of this page describes earlier releases. It is kept as a dated record
and is not current behavior.

> **Note, 9 November 2026:** since 1.8.14 AIRK is a single executable. Offline
> MiniLM inference runs inside the service, and the separate `airk-embed`
> helper described below is neither built nor started.

> **Note, 10 November 2026:** workers moved into the service. That change is
> now described in the current section above.

Before 1.8.22, AIRK used one elected service per user/machine installation and independent collections. That did not mean one executable process per VS Code window. Agents, Explorer, native workers and Windows launchers had different owners and lifetimes.

### What each process did (before 1.8.22)

| Process or command | Purpose | Owner and lifetime |
|---|---|---|
| `airk-service` | Metadata broker, durable jobs, resource admission and connection grants. It does not perform project retrieval or load a separate model for every agent. | One elected instance for its installation/control namespace. Background monitoring or accepted work can keep it running after VS Code closes. |
| `airk-source` | Native source watching and reconciliation for the service's registered roots. | Shared service-owned source worker, rather than one watcher per agent. |
| `airk-index` | Opens a scoped project kernel/store and serves retrieval/index work. | Service-owned project worker; the number depends on admitted projects/work. It can retire independently of an MCP frontend. |
| `airk-embed` | Shared offline inference pool. Compatible project sessions reuse loaded model state. | Service-owned helper, not one model process per MCP connection. See [embedding ownership](service-embedding.md). |
| `airk … mcp` | One stdio MCP frontend: request IDs, response mode, client attribution and configured source/project scope. | Launched by an agent or Explorer for that connection. Separate connections have separate streams and protocol state. This frontend obtains service grants and talks directly to project workers. |
| `airk service console …` | Persistent private control transport for Explorer's management operations. | Extension-owned connection, separate from its evidence MCP connection. It is not another broker or project indexer. |
| `airk-launcher` | Resolves the selected installation/runtime and forwards arguments, stdio and exit status. It performs no indexing or inference. | Connection-owned forwarding process on Windows; Unix replaces it with the selected command. |

On Windows/Linux and historical Mac layouts, role names are owned copies of the approved runtime executable. From the macOS 1.8.8 layout onward, service/source/index roles execute the sealed main inside `AIRK.app`; command arguments identify the role, and no role copies are created. Only the separately signed launcher image is copied outside that bundle. A Task Manager row named `airk.exe` needs its command line to distinguish MCP, console and short-lived CLI commands. Original source files remain outside process/runtime ownership.

### Why Windows can show two launchers for one connection

The compatibility path is:

```mermaid
flowchart LR
    C[Agent or Explorer] --> U[Version-free universal launcher]
    U --> L[Installation stable launcher]
    L --> M[Selected engine: MCP frontend]
    M --> S[Shared service and scoped workers]
```

The universal launcher gives agent configurations a path that survives installation/version changes. Its authenticated pointer selects an installation. The installation launcher understands that installation's current launch/repair contract and selects the approved runtime.

Windows cannot use Unix `exec` to replace the launcher. The current forwarding adapter waits for its child, carries its exit code, and places descendants in kill-on-close Job Objects. Consequently, this compatibility route leaves **two waiting launchers plus one MCP frontend** visible for the connection. These launchers are compatibility/containment overhead, not two additional services or model loads. See [the forwarding adapter](../crates/airk-cli/src/native_process.rs) and [universal routing](../crates/airk-cli/src/universal_launcher.rs).

The current universal router skips the installation-launcher hop when both verified launcher binaries are byte-identical, the arguments permit direct routing, and a valid selected runtime is available. That route has **one waiting Windows launcher plus the frontend**. Unknown/different launcher implementations and repair paths retain delegation; removing the hop unconditionally would bypass the selected launcher's compatibility behavior.

The Windows/Linux universal binary is deliberately retained across upgrades; its pointer changes. Therefore, an older retained universal copy can continue to use the longer route even after the installation launcher is updated. Mac publication refreshes a recognized image from the approved standalone launcher, preserving unknown bytes and coordinating publishers/retirements with one directory lock. The existing shortcut does not establish that every installed connection uses one launcher. Unix forwarding replaces the process at each hop, so this Windows process-count multiplication does not apply there.

Both launcher layers are not intrinsically required to execute an MCP request. They are required by this retained compatibility route. Reducing that overhead further needs a verified launcher-refresh/migration design that preserves active clients, unknown-file protection, selected-runtime verification and repair compatibility; killing or deleting the installed launchers is not that migration.

### One VS Code window can own several connections

Explorer has its own evidence MCP connection and management control transport. Codex and Claude each create their own MCP connections. Agent hosts may also retain connections for more than one conversation or session. A window count is therefore not a connection count.

AIRK cannot safely merge two independent stdio connections by matching their parent executable or source path: their request IDs, initialization, response preferences and lifetimes are separate. Conversely, a live agent-host process alone does **not** prove every connection it retains is still needed. Closing an individual session must close its MCP input; host-exit monitoring only detects the loss of the process that launched it. AIRK has no authoritative view of which conversation is visible in an agent's UI.

### Observed Windows snapshot, October 29, 2026

The captured installed runtime was **1.6.3**, with selected inventory identity `fb3f5590226735bd9459043b507f447ca64faa15f9bde020c009b4cbf2c56147`. Its universal and installation-launcher SHA-256 values differed. All five MCP connections used the longer Windows route.

| Observed owner/role | Connections | AIRK processes |
|---|---:|---:|
| VS Code extension evidence MCP | 1 | 3 |
| Codex host evidence MCP | 2 | 6 |
| Two Claude host processes, one MCP each | 2 | 6 |
| Extension management console | 1 | 1 |
| Shared service, source worker and embedding helper | — | 3 |
| **Total observed** | **5 MCP connections plus control** | **19** |

Ten rows were waiting launchers, five were MCP frontends, one was the console, and three were shared service roles. These processes started between 12:05 and 12:35 local time and used the installed runtime, not the later isolated validation fixtures. The snapshot accounts for the rows; it does not prove that both Codex sessions and both Claude hosts were still needed. That requires observing the owning clients' session closure/reconnection behavior. No client was terminated based only on its name or this count.

### What would have established a lifecycle problem (before 1.8.22)

- A connection's launcher/frontend chain remains after its input closes or its authenticated owner exits.
- Repeated connect/disconnect cycles leave a growing count after the old connections drain.
- More than one elected service claims the same control namespace, or inference helpers appear without matching service ownership.
- A process uses a retired runtime after an authorized upgrade has completed its owned-client drain.

A detached service can legitimately outlive its original parent. Enabled monitoring, jobs and other live clients also explain continued service/worker ownership. The [installation lifecycle](service-installation.md) defines idle exit and guarded shutdown; do not infer orphaning from a missing service parent alone.

On Windows, inspect names together with command lines, executable paths, creation times and parent identities:

```powershell
Get-CimInstance Win32_Process |
    Where-Object Name -Like 'airk*.exe' |
    Select-Object Name, ProcessId, ParentProcessId, CreationDate, ExecutablePath, CommandLine
```

Correlate parent creation times as well as PIDs because Windows reuses PIDs. Native diagnostics provide service/worker identity separately from quick index status. The [installed lifecycle fixture](../scripts/check-service-install.py) and [client lifetime checks](../scripts/check-client-lifetime.py) test EOF, abrupt owner loss, another live session's preservation and shared-service survival in isolated installations. They do not certify that an external agent closes every obsolete conversation connection.
