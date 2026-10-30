# AIRK command-line guide

The `airk` executable is the whole engine, not only a VS Code helper. This guide explains how the commands fit together. The [CLI reference](cli-reference.md) documents every public command and option exactly as `airk <command> --help` prints them; it is generated from the parser, and a test keeps it current. [Running AIRK headless](headless-cli.md) maps each VS Code feature to its command.

## Conventions

- Commands print one JSON document on stdout. Human-oriented exceptions are `airk --help`, `tools list` without `--json`, and `service logs --format text`.
- A failure exits non-zero and writes a message to stderr. Set `AIRK_ERROR_FORMAT=json` to get a structured error instead. Query commands can return a typed busy or retry refusal while workers start; repeat the same call after the reported delay.
- Source files are never modified. Commands that change AIRK state (registering, configuring, pausing, purging) say so in their help.
- Agents can read the same help through MCP: `airk tools call airk_help --args '{"command":"search"}'`.

## Services, projects and scope

One **service** owns a control directory: its project registry, job queue and worker processes. A **project** is a source folder (`--root`), an index directory (`--data-dir`) and a collection name (`--collection`). Its canonical form has a 64-character hexadecimal **binding**.

There are two ways to reach a service:

- **Installed runtime** (what the VS Code extension sets up). Commands find the per-user installation automatically; `--service-install-dir` selects another one. Select a project with `--root <folder>`, plus `--data-dir` and `--collection` when they are not the defaults.
- **A service you start yourself** (headless, no installation). Run `airk serve --runtime-dir <control>`. Then add `--service-runtime-dir <control> --service-project <binding> --root … --data-dir … --collection …` to project commands.

## Creating a project for any folder

You can register any folder you can read as a project. Indexes and settings live in the data directory; the source folder is only read.

```text
# A service you started:
airk --root /path/to/folder --data-dir /path/to/index --collection mydocs service prepare-registration   # read-only: prints the binding
airk --root /path/to/folder --data-dir /path/to/index --collection mydocs service register --runtime-dir <control>
airk projects --runtime-dir <control>

# The installed runtime (the default data directory is used when --data-dir is omitted):
airk --root /path/to/folder init
airk projects
```

Then queue indexing with `airk <scope> index`. Run `airk <scope> watch on` to keep the index current, and check progress with `airk <scope> tools call airk_status --args '{}'`.

On macOS and Linux an existing data directory must be private to your account (for example `mkdir -m 700`). A directory that other accounts can reach is refused with `storage_not_private`, and AIRK never changes its permissions. The source folder and the data directory must not contain each other. To remove a project's index and settings, leaving the sources untouched, run `airk <scope> configure --stdin` with `{"operation":"unregister"}`.

## Command families

| Family | Commands | Purpose |
|---|---|---|
| Service | `serve`, `service health`, `service shutdown`, `service register`, `service prepare-registration`, `service logs`, `service console`, `service rpc`, `service requests …`, `service legacy …` | Run and manage a service and its registry |
| Projects | `projects`, `init`, `index`, `inspect`, `pause`, `resume`, `watch`, `setup-model`, `settings`, `configure`, `status` | Register, index and configure projects |
| Evidence | `search`, `files`, `read`, `info`, `outline`, `symbol`, `imports`, `facts`, `refs`, `context`, `changes`, `code-facts`, `relationships`, `parse` | Read-only, cited queries; `parse` plans or runs multi-step requests |
| MCP tools | `tools list`, `tools help`, `tools call`, `tools shell`, `mcp` | The same tools agents use; `mcp` serves them over stdio |
| Explorer Assistance | `explorer draft-search`, `draft-strategy`, `draft-plan`, `suggest-view`, `validate-draft`, `list-templates`, `get-template`, `proposal-status`, `get-handoff` | Editor proposals produced offline into files (see below) |
| Agents | `setup --client <name>` | Print an MCP client configuration for an installed runtime |
| Usage | `usage report`, `policy`, `configure`, `clear`, `feedback`, `import`, `calibrate` | Local usage reporting |
| Runtime | `runtime location`, `inspect`, `ensure`, `start`, `login …`, `inventory`, `purge`, `uninstall`, and staging/repair commands | Installed-runtime lifecycle and data review |

## Explorer Assistance from the CLI

In VS Code these tools send proposals to an open Explorer window's Suggestions Inbox. The CLI versions never do that: every result goes to the file named by the required `--output`, which must not already exist and is created owner-only on Unix. `--args` takes the same JSON object as the MCP tool of the same name.

- `draft-search`, `draft-strategy`, `draft-plan` and `suggest-view` check the draft against the same generated schemas the MCP server embeds, then write it with `valid`, `errors`, `admitted: false` and `executed: false`. An invalid draft is still written, and the command exits non-zero. The editor's additional semantic and live checks are not run.
- `validate-draft` writes the validation result only.
- `list-templates` and `get-template` read the bundled strategy pack. It is generated from the extension source and checked for drift at build time.
- `proposal-status --draft <file>` reports on a draft file written by the CLI. Such drafts are never admitted to an Inbox.
- `get-handoff` reads a snapshot the user shared from a running Explorer window. Handoff tokens exist only in that window, so it needs that window's `--explorer-descriptor` (`…/iokaio.airk/explorer-assistance/launch-1.json` in VS Code's global storage). A missing descriptor fails without creating the output file.

## Tests

`crates/airk-cli/tests/cli_commands.rs` runs `--help` for every public command. It also exercises the query, management and Explorer commands against a private headless service. For commands that need an installed runtime, a login manager, legacy state or an editor, it names the native test or qualification script that covers them, and it fails if any command in the reference has no listed test.
