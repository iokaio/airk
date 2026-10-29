# AIRK processes and ownership

AIRK uses one elected service per user/machine installation and independent collections. That does not mean one executable process per VS Code window. Agents, Explorer, native workers and Windows launchers have different owners and lifetimes.

## What each process does

| Process or command | Purpose | Owner and lifetime |
|---|---|---|
| `airk-service` | Metadata broker, durable jobs, resource admission and connection grants. It does not perform project retrieval or load a separate model for every agent. | One elected instance for its installation/control namespace. Background monitoring or accepted work can keep it running after VS Code closes. |
| `airk-source` | Native source watching and reconciliation for the service's registered roots. | Shared service-owned source worker, rather than one watcher per agent. |
| `airk-index` | Opens a scoped project kernel/store and serves retrieval/index work. | Service-owned project worker; the number depends on admitted projects/work. It can retire independently of an MCP frontend. |
| `airk-embed` | Shared offline inference pool. Compatible project sessions reuse loaded model state. | Service-owned helper, not one model process per MCP connection. See [embedding ownership](service-embedding.md). |
| `airk … mcp` | One stdio MCP frontend: request IDs, response mode, client attribution and configured source/project scope. | Launched by an agent or Explorer for that connection. Separate connections have separate streams and protocol state. This frontend obtains service grants and talks directly to project workers. |
| `airk service console …` | Persistent private control transport for Explorer's management operations. | Extension-owned connection, separate from its evidence MCP connection. It is not another broker or project indexer. |
| `airk-launcher` | Resolves the selected installation/runtime and forwards arguments, stdio and exit status. It performs no indexing or inference. | Connection-owned forwarding process on Windows; Unix replaces it with the selected command. |

Role names are owned copies of the approved runtime executable, not separate independently installed engines. A Task Manager row named `airk.exe` needs its command line to distinguish MCP, console and short-lived CLI commands. Original source files remain outside process/runtime ownership.

## Why Windows can show two launchers for one connection

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

The universal binary is deliberately retained across upgrades; its pointer changes. Therefore, an older retained universal copy can continue to use the longer route even after the installation launcher is updated. The existing shortcut does not establish that every installed connection uses one launcher. Unix forwarding replaces the process at each hop, so this Windows process-count multiplication does not apply there.

Both launcher layers are not intrinsically required to execute an MCP request. They are required by this retained compatibility route. Reducing that overhead further needs a verified launcher-refresh/migration design that preserves active clients, unknown-file protection, selected-runtime verification and repair compatibility; killing or deleting the installed launchers is not that migration.

## One VS Code window can own several connections

Explorer has its own evidence MCP connection and management control transport. Codex and Claude each create their own MCP connections. Agent hosts may also retain connections for more than one conversation or session. A window count is therefore not a connection count.

AIRK cannot safely merge two independent stdio connections by matching their parent executable or source path: their request IDs, initialization, response preferences and lifetimes are separate. Conversely, a live agent-host process alone does **not** prove every connection it retains is still needed. Closing an individual session must close its MCP input; host-exit monitoring only detects the loss of the process that launched it. AIRK has no authoritative view of which conversation is visible in an agent's UI.

## Observed Windows snapshot, October 29, 2026

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

## What would establish a lifecycle problem

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
