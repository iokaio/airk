# Running AIRK headless

P7 adds explicit reviewed combined sessions, unique-index execution, shared
budgets and complete member-qualified results. [Combined-read contracts](combined-reads.md)
record selectors, discovery, publication vectors, plan verification and the
current qualification boundary. Legacy source bindings retain their one-index
meaning. Use the actual connection catalog for supported inputs.

Since split Phase 4, `airk` is the service only and `airkctl` is the command-line client. Run `airk` with no arguments to start the installed service in the background, or to see that it is already running. The installer and extension are being reworked under the [split plan](split/split-plan.md) (Phases 6–7).

Everything AIRK indexes and answers is available without VS Code. This page maps each extension feature to its CLI equivalent and lists the places where the CLI still needs raw JSON. See the [CLI guide](cli.md) and the generated [CLI reference](cli-reference.md), which documents every command. Run `airkctl --help` for the command list and `airkctl <command> --help` for each command. The same help text is available to agents through `airkctl tools call airk_help`.

To experiment without any installation, build with `build.ps1` or `build.sh` at the repository root. Each build writes `builds/<UTC timestamp>-<platform>/` with the runtime and a README that walks through starting the service and registering a project.

For command purpose, implementation ownership, effects and catalog differences, use the [CLI developer guide](cli-developer-guide.md). The companion [MCP developer guide](mcp-developer-guide.md) explains the agent tool interface and links back to CLI commands.

## Feature map

| Extension feature | Headless equivalent |
|---|---|
| Start, stop and inspect the service | `airk` (no arguments) or `airkctl service start`; `airkctl service stop`, `restart` and `status`. From a local build, the private role `airk __airk_role service-foreground --runtime-dir <control>` runs a foreground service for development and tests |
| Set up a new index and query Project | Use `service rpc` with native catalog draft, review, apply and receipt methods; see [current headless setup](cli.md#reviewed-index-and-project-setup-in-188). Legacy `init`/`service register` cannot create new indexes in an authority-1 catalog. |
| List projects | `airkctl projects` (installed runtime) or `airkctl projects --runtime-dir <control>` |
| Explorer Assistance tools | `airkctl explorer draft-search\|draft-strategy\|draft-plan\|suggest-view\|validate-draft\|list-templates\|get-template\|proposal-status\|get-handoff --output-file <new file>`: results go to the file, never to an Inbox (see the [CLI guide](cli.md#explorer-assistance-from-the-cli)) |
| Index, pause, resume, watch | `airkctl <scope> index`, `pause`, `resume`, `watch on`/`watch off`, `inspect` |
| Vector search setup | `airkctl <scope> setup-model`, then `airkctl <scope> configure --stdin` with `{"operation":"vectors","enabled":true}` |
| Project settings | `airkctl <scope> settings <view>` to read; `configure --file`/`--stdin` to change |
| Search and navigation | `search`, `files`, `read`, `info`, `outline`, `symbol`, `imports`, `facts`, `refs`, `context`, `changes`, `code-facts`, `relationships`, or any tool through `tools call` |
| Workbench evidence plans | `airkctl <scope> parse "<request>"` (plan only) and `--execute` |
| Agent connection | `airkctl agents connect --client claude` (or `codex`, `vscode`) with `--connection-id` and, for a new connection, `--selection-file <reviewed selection>` writes AIRK's entry; `agents list`, `repair`, `remove`, `verify`, `launch` and `revoke` manage connections. Agents connect over HTTPS; the stdio `mcp` command is retired |
| Usage reports | `airkctl <scope> usage report`, `policy`, `configure`, `clear`, `feedback`, `import` |
| Management status and logs | `airkctl service status`, `service health`, `service logs`, `service rpc`, `tools call airk_status` and `airk_diagnostics` |
| Retire an index | Prepare `catalog_source_mutation_draft` with `change: {"operation":"retire_index"}`, then explicitly apply its exact native request and preview digest through `catalog_apply`; inspect the exact receipt. [Retirement and physical cleanup](service-project-retirement.md) are separate. |
| Review and remove AIRK data, login startup, uninstall | Not yet `airkctl` commands. Until the Phase 7 setup role and installer replace them, the pre-split `runtime inventory`, `runtime purge`, `runtime login …` and `runtime uninstall` are reachable only as `airk __airk_role setup-interim runtime …` (non-shippable interim, U93) |

For current evidence reads, `<scope>` can be `--catalog-session <reviewed-session-json>`
or `--combined-session <reviewed-session-json>`, with `--service-runtime-dir <control>`
for an explicitly selected service. Sessions bind the reviewed grant and explicit
Project/index selection; paths and saved defaults grant no access. Do not combine
these session flags with legacy root/data/collection/project selectors. Imported
legacy connections retain their qualified one-index binding. [Current setup](cli.md#reviewed-index-and-project-setup-in-188)
describes the separate management flow; management/index-job commands use their
native backing registration rather than an evidence-session flag.

## Editor-only features

Headless reads and executed plans retain the same complete native results as MCP and Workbench. Direct status/diagnostics include configured `query_concurrency`; completed plan steps carry that field inside their result. CLI `tools call` preserves the MCP envelope and the interactive tools shell renders its text content. See [read interface parity](read-interface-parity.md) for all operations, input defaults, scoped observations and result/export limits.

Status replies can carry the explicit `status_timeout` setup stage with unknown readiness and null evidence. Later polls retain that deadline outcome after accepted setup work settles. Genuine setup failures and ordinary session cancellation keep their failure behavior; follow only the bounded typed read-retry contract with the original scope.

These belong to the editor integration and have no headless meaning:

- the first-use setup review;
- the `airk.runtime.nativeExecution` gate;
- the Workbench and Explorer views, including saved Workbench sessions;
- the Suggestions Inbox itself (CLI drafts are written to files instead);
- the editor's terminal-command integration and **Remove Terminal Command**;
- reporting the extension's uninstall cleanup result.

## Known gaps

- **Guarded maintenance** (rebuild, repair and verify previews with plans) has no dedicated command. The extension creates and executes plans through `service rpc` requests (`plan_create`, `plan_items`, `plan_execute`). Headless users can send the same JSON with `airkctl service rpc --runtime-dir <control> --stdin`, but there is no friendlier wrapper yet. `airkctl index --full` deliberately refuses and points to this path.
- **Durable request retries** use `airkctl service requests prepare|list|retry|forget`, which exposes the raw request model.
- **Current catalog setup and retirement** use `service rpc` with the native
  catalog contracts; there is no dedicated friendly CLI wizard. A successful
  metadata transport exit is not proof of application success: inspect the
  returned native `status`, complete receipt and readiness/cleanup observations.

These gaps are recorded for a follow-up; the CLI is not claimed to be friendlier than the extension for them.
