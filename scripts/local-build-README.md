# AIRK {{VERSION}} local build ({{PLATFORM}})

Built {{BUILT}} from commit `{{COMMIT}}`{{DIRTY}} with the `{{PROFILE}}` profile by `build.ps1` or `build.sh`.

This build is for advanced users who fork AIRK and want to experiment with the native engine from a terminal, without VS Code. It runs directly from this folder. It does **not** install anything: no PATH or shell-profile edits, no login startup, no per-user runtime installation and no VS Code extension. Delete the folder to remove it. The binaries are unsigned local builds for this platform only; they are not a release.

## What is here

| Path | Purpose |
|---|---|
| `{{BIN}}` | The AIRK engine. Every process below is this executable started with a different command. |
| `{{EMBED}}` | The offline embedding helper. The service starts it when vector search needs it; never start it yourself. |
| `runtime/` | The complete runtime: ONNX Runtime libraries, the bundled offline model, `airk-install.json` (file digests), `airk-build.json` (build receipt) and license notices. Keep the files together. |

## Processes, by name

| Process | Started by | How |
|---|---|---|
| Service (`airk serve`) | You | `airk serve --runtime-dir <control>`. It is the metadata broker, job queue and resource admission for one private control directory, and keeps running until you shut it down. |
| Source worker | The service | Watches registered roots when watching is on. Started automatically. |
| Project workers | The service | Index and answer queries for one registered project each. Started automatically. |
| Embedding helper (`airk-embed`) | The service | Shared offline inference for vector search. Started automatically. |
| MCP frontend (`airk mcp`) | You or an agent | One stdio MCP server per connection; it talks to the service and project workers. |
| CLI commands | You | Short-lived clients such as `service health`, `index`, `search`, `status` and `tools call`. |

Never start the hidden worker commands (`project-worker`, `source-worker`, `derived-worker`, `process-guardian`) by hand; the service launches and authenticates them.

## Set up a test environment

Use a throwaway folder outside any repository. The commands below use PowerShell on Windows and a POSIX shell on macOS/Linux. Replace `<build>` with this folder's absolute path.

{{SETUP}}

Notes:

- On macOS and Linux, keep the data directory private to your account (for example `mkdir -m 700`). AIRK never changes the permissions of an existing directory; `airk runtime inventory` reports storage other accounts may read as `storage_not_private`.
- `service prepare-registration` is read-only and prints the canonical registration, including the project `binding`. `service register` stores it in the service registry. Run `index` to build the first index; `watch on` keeps it current from source changes.
- `airk --help` and `airk <command> --help` describe every command. `airk tools list --json` and `airk tools help <tool>` describe the MCP tools, and `airk tools call airk_help` returns the engine's own guide.
- `index` returns a job receipt. Job state and coverage are visible through `airk_status` and `airk_diagnostics` (`tools call`).
- Add `.airkignore` files to exclude sources. Common credential files (`.npmrc`, `.netrc`, SSH private keys, kubeconfig, Terraform state and others) are skipped by default; AIRK is not a secret scanner.

## Inspect and clean up

- `airk runtime inventory --install-dir <dir>` and `airk runtime purge` act on a real per-user installation; this local build has none, so delete the test folder instead.
- Service logs: `airk service logs --runtime-dir <control> --limit 32 --format json`.
- Stop the service with `airk service shutdown --runtime-dir <control>`, then delete the test folder. Deleting it removes the control state, indexes and logs of this experiment.

## Limits

This is the same engine the extension installs, but it is started without the installer's verified-selection, upgrade and login-startup layers. The installed lifecycle (runtime staging, activation, launchers and uninstall) is not exercised here. Results from this build do not qualify an installed release or other platforms.
