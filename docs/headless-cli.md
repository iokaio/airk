# Running AIRK headless

Everything AIRK indexes and answers is available without VS Code. This page maps each extension feature to its CLI equivalent and lists the places where the CLI still needs raw JSON. Run `airk --help` for the built-in guide, which covers how the service works, how to select a project, a headless quick start and how to learn the tools. `airk <command> --help` describes each command. The same help text is available to agents through `airk tools call airk_help`.

To experiment without any installation, build with `build.ps1` or `build.sh` at the repository root. Each build writes `builds/<UTC timestamp>-<platform>/` with the runtime and a README that walks through starting the service and registering a project.

## Feature map

| Extension feature | Headless equivalent |
|---|---|
| Install or repair the runtime | `airk runtime location`, `inspect` and `ensure` for the per-user installation, or `airk serve --runtime-dir <control>` from a local build |
| Add a project | Installed runtime: `airk --root <src> init`. Explicit service: `airk --root <src> --data-dir <data> --collection <name> service register --runtime-dir <control>` |
| Index, pause, resume, watch | `airk <scope> index`, `pause`, `resume`, `watch on`/`watch off`, `inspect` |
| Vector search setup | `airk <scope> setup-model`, then `airk <scope> configure --stdin` with `{"operation":"vectors","enabled":true}` |
| Project settings | `airk <scope> settings <view>` to read; `configure --file`/`--stdin` to change |
| Search and navigation | `search`, `files`, `read`, `info`, `outline`, `symbol`, `imports`, `facts`, `refs`, `context`, `changes`, `code-facts`, `relationships`, or any tool through `tools call` |
| Workbench evidence plans | `airk <scope> parse --text "<request>"` (plan only) and `--execute` |
| Agent connection | `airk <scope> setup --client claude` (also codex, vscode, cursor, gemini, cline, roo, junie, copilot-cli, opencode and others) prints the MCP configuration for an installed runtime; agents launch `airk … mcp` |
| Usage reports | `airk <scope> usage report`, `policy`, `configure`, `clear`, `feedback`, `import` |
| Management status and logs | `airk service health`, `service logs`, `service console` (JSON transport), `tools call airk_status` and `airk_diagnostics` |
| Remove a project's index | `airk <scope> configure --stdin` with `{"operation":"unregister"}` (sources untouched) |
| Review and remove AIRK data | `airk runtime inventory --install-dir <dir>`; `airk runtime purge --install-dir <dir> --select <id> [--confirm <digest>]` |
| Login startup | `airk runtime login --install-dir <dir> plan`, `status`, `install` or `uninstall` |
| Uninstall the runtime | `airk runtime uninstall --install-dir <dir>` |

`<scope>` is either `--root <src> [--data-dir …] [--collection …]` for the installed runtime, or `--service-runtime-dir <control> --service-project <binding> --root … --data-dir … --collection …` for a service you started.

## Editor-only features

These belong to the editor integration and have no headless meaning:

- the first-use setup review;
- the `airk.runtime.nativeExecution` gate;
- the Workbench and Explorer views, including saved Workbench sessions;
- Explorer Assistance drafts and the Suggestions Inbox;
- the editor's terminal-command integration and **Remove Terminal Command**;
- reporting the extension's uninstall cleanup result.

## Known gaps

- **Guarded maintenance** (rebuild, repair and verify previews with plans) has no dedicated command. The extension creates and executes plans through `service rpc` requests (`plan_create`, `plan_items`, `plan_execute`). Headless users can send the same JSON with `airk service rpc --runtime-dir <control> --stdin`, but there is no friendlier wrapper yet. `airk index --full` deliberately refuses and points to this path.
- **Durable request retries** use `airk service requests prepare|list|retry|forget`, which exposes the raw request model.

These gaps are recorded for a follow-up; the CLI is not claimed to be friendlier than the extension for them.
