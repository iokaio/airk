# AIRK command-line guide

P7 adds explicit reviewed combined sessions, unique-index execution, shared
budgets and complete member-qualified results. [Combined-read contracts](combined-reads.md)
record selectors, discovery, publication vectors, plan verification and the
current qualification boundary. Legacy source bindings retain their one-index
meaning. Use the actual connection catalog for supported inputs.

The 1.8.8 working build supplies optional [evidence guidance](evidence-guidance.md)
from returned metadata. Advice is bounded and inert; CLI/MCP/Workbench and
complete/selected exports retain applicable reasons without additional reads.

Since split Phase 4, `airk` is the service only and `airkctl` is the command-line client. `airk` with no arguments starts the installed service in the background (through the `airkctl` installed beside it) or says it is already running; with any other arguments it says that `airk` takes no commands and reports whether the service is running. This guide explains how the `airkctl` commands fit together. The [CLI reference](cli-reference.md) documents every public command and option exactly as `airkctl <command> --help` prints them; it is generated from the parser, and a test keeps it current. [Running AIRK headless](headless-cli.md) maps each VS Code feature to its command.

For each command's purpose, implementation, tests and guarded behavior, see the [CLI developer guide](cli-developer-guide.md). The [MCP developer guide](mcp-developer-guide.md) covers the corresponding agent interface. In the unpublished 1.8.14 implementation, `rk tools list`, `tools help`, `tools call` and `tools shell` use the public HTTPS tool catalog and its shared alias and argument validation. Historical `airk_<operation>` aliases remain accepted. The selected native backend retains the same reviewed scope; public routing neither grants access nor attaches an unrelated Explorer window. Scoped diagnostics explicitly report unavailable process/work context where exposing project-wide information would exceed that scope. Native management commands provide their separately authorized lifecycle observations.

## Run commands with rk

The 1.9.0 installer offers `rk` as the default shortcut for the canonical
`airkctl` executable. For example, `rk service status` and
`airkctl service status` run the same CLI operation with the same arguments,
output and exit status. Help and executable identities may still say `airkctl`.
If `rk` already exists anywhere on PATH, interactive setup offers a different
alias name or no alias. Unattended setup automatically selects the first
available name: `rk`, then `rk1`, `rk2`, and so on. The installed
`airkctl-aliases.txt` records the selected alias. Windows uses a `.cmd` launcher;
macOS and Linux use a shell launcher. Examples below use the default `rk`;
substitute the selected name, or use `airkctl` when you chose no alias. The
canonical `airkctl` remains available with every choice. Earlier payloads may
only provide `airkctl`.

For an explicit choice, the Linux installer and macOS `Install AIRK.command`
accept `--cli-alias NAME` or `--no-cli-alias`. The Windows installer accepts
`/CLIALIAS=name` or `/NOCLIALIAS=1`; `/AUTOCLIALIAS=1` explicitly selects its
normal automatic choice. Choose only one alias option. An explicitly requested
occupied name refuses installation. Recovery uses the original saved choice:
do not add alias options to `--resume` (Unix) or `/AIRKOPERATION` (Windows).

After installation, open a new terminal. The following examples assume the
installation's command directory is on that terminal's `PATH`:

| Platform | Default installed shortcut | Example |
|---|---|---|
| Windows (PowerShell or Command Prompt) | `rk.cmd` beside `airkctl.exe` in the installed `bin` directory | `rk --help` |
| macOS | `~/.local/bin/rk` forwards to the installed `airkctl` | `rk --help` |
| Linux | `~/.local/bin/rk` forwards to the installed `airkctl` | `rk --help` |

On Windows, the installer uses its owned `bin` PATH integration. On macOS and
Linux, setup does not edit your shell profile. If `~/.local/bin` is not already
on `PATH`, invoke `"$HOME/.local/bin/rk" --help` directly, or add it for the
current shell:

```sh
export PATH="$HOME/.local/bin:$PATH"
rk --help
rk service status
```

To make that PATH change persistent, add the export to the startup file used by
your shell. Setup preserves existing commands when it selects another alias
or you choose no alias.

Portable packages do not register PATH. From the extracted package's root:

```powershell
.\rk.cmd --help
.\rk.cmd service status
```

```sh
# macOS or Linux
./rk --help
./rk service status
```

The portable shortcut selects its packaged matching client, including the app
bundle layout on macOS. You can also invoke the canonical client directly:
`.\runtime\airkctl.exe --help` on Windows, `./runtime/airkctl --help` on Linux,
or `./runtime/AIRK.app/Contents/MacOS/airkctl --help` on macOS. CLI help works
without starting the service; service commands still need their normal setup
and runtime prerequisites. See [native installation](service-installation.md)
for setup, portable registration and recovery.

## Complete read results in 1.8.7

The 1.8.8 working build adds [Lean/Rich reply selection](reply-detail.md): use
`--reply-detail lean|rich` on read aliases or `reply_detail` in tools-call/shell
JSON. This is independent of retained indexing detail. Omitting it preserves
the inherited defaults described below; conflicts with legacy projection flags
reject instead of choosing precedence.

CLI tool calls, dedicated read commands and Workbench plans share the MCP read handlers. Full results retain provenance, publication, readiness, coverage, continuations and error/retry details. `rk tools call airk_status` returns its MCP envelope, while `rk parse --execute '/status'` places the complete status result under `steps[].result`, including `query_concurrency`. The shell pretty-prints the same tool content. Defaults can differ between convenience commands; use matching arguments and [the parity audit](read-interface-parity.md) when comparing results.

For explicit plans, put a JSON object in the `plan` field and pass the request through `rk tools call airk_parse --args-file request.json`. Do not encode the plan object as a string. Workbench's complete JSON/Markdown exports retain the full response; selected-evidence exports identify their selected subset.

MCP-compatible status results can report `status_timeout` with `index_error.stage: setup`, unknown readiness and null evidence. Later polls preserve that explicit setup-deadline outcome after already accepted work drains. Apply the existing bounded read-retry contract with the same scope; genuine setup failures and ordinary session cancellation retain their failure handling. See the [status contract](mcp.md).

## Per-file profile metadata — P11a

The unpublished **1.8.8 P11a working build** adds an explicit opt-in to FileInfo:

```sh
rk info src/example.rs --include-profile-families --verification off

rk tools call airk_file_info --args '{"path":"src/example.rs","include_profile_families":true,"verification":"off"}'
```

Inside `rk tools shell`, enter the same JSON directly, without shell quoting:

```text
call airk_file_info {"path":"src/example.rs","include_profile_families":true,"verification":"off"}
```

MCP uses boolean `include_profile_families` on `airk_file_info` or its public
`file_info` alias. This is a FileInfo option, not an option for `read`, `outline`
or other path-taking commands. Omit the CLI flag to retain the existing
representation and validator. In tools/MCP JSON, omission or boolean `false`
preserves compatibility; a string such as `"true"` is invalid.

The report binds the exact applied file profile/publication and distinguishes
configured and published collection policy. Restricted Project callers receive
explicit unavailable policy observations. Reporting uses saved metadata; it does
not select a new profile, reindex, invoke producers or reopen originals. The
examples independently turn source verification Off; its default and controls
remain unchanged. Full/Terse encoding and Lean/Rich replies preserve the report.
A legacy validator cannot suppress a newly opted report; repeat an opted validator
with the same representation controls.

The console prints a banner, prompts and `Result:` before canonical JSON content;
it is not a JSON-only stream. One-shot tools calls retain the MCP envelope and
error flag. See the [profile-family guide](profile-family-reporting.md) for the
18-family observations and [qualification evidence and limits](research/implementation-v190.md#p11a--local-phase-checkpoint-6-november-2026).
P11a qualification and primary review passed. P11a is accepted for unpublished local development; the working version remains 1.8.8.

## Identifier navigation

For a known type/function, use compact `rk symbol` first, scoped `rk refs` for textual uses, and `rk read`/`context` to inspect a selected location. `rk outline` reads one file's structure in source order; it does not propose an Explorer screen. Search remains useful for concepts and supplementary passages, with bounded same-publication definition pointers where available.

`symbol` and `code-facts` keep exact matching by default; `--name-match suffix|contains` explicitly expands simple/qualified names case-sensitively. `--include-count` requests a qualified count, whose total can remain unknown under bounded scanning. `code-facts --path <file> --start-line N --end-line M` returns the facts whose own lines overlap that inclusive range (the definition containing a line, the calls inside a body); `--minimal` returns small rows with each fact's enclosing scope (`owner`) and states the file's path, content hash and provenance once. Search and refs accept indexed `--language`, `--evidence-kind code|document|configuration|unknown` and literal `--exclude-path-prefix`, applied before candidate ranking rather than to a tiny returned top-k.

References add `--order index|path|code_first`, `--include-files`, `--include-count`, `--cursor`, `--limit` and `--max-bytes`. New selectors opt into a paged response; omitted selectors preserve the preceding response contract. Continue with the same identifier, scope, filters and ordering. File summaries count matching lines returned on this page, not every occurrence or every matching file. A too-small byte budget can produce an empty page with required bytes and a resumable cursor; terminal scan/absolute-record limits instead remain explicit coverage gaps. See the [developer guide](cli-developer-guide.md#identifier-investigations-and-bounded-discovery) for the full contract and examples.

## Conventions

- Ordinary one-shot commands print JSON on stdout. Help, text/CSV/Markdown exports and interactive `tools shell` have their own output formats. `airkctl session` is a framed protocol stream for one editor window.
- A failure exits non-zero and writes a message to stderr. Add `--output json` to get JSON results and a structured JSON error line instead. Query commands can return a typed busy or retry refusal while workers start; repeat the same call after the reported delay.
- Source files are never modified. Commands that change AIRK state (registering, configuring, pausing, purging) say so in their help.
- Agents can read the same help through MCP: `rk tools call airk_help --args '{"command":"search"}'`.

## Services, projects and scope

One **service** owns a control directory: its registry, job queue and worker
processes. In the current catalog, an **index** has an independent native backing
registration; a query **Project** groups reviewed source/path memberships from
one or more indexes. A legacy lowercase **project** is the source/data/collection
registration with a 64-character hexadecimal **binding**. These identities are
not interchangeable.

There are two ways to reach a service:

- **Installed service.** `airkctl` finds the installation from its install record. Start the service with `airk` (no arguments) or `rk service start`; `rk service status` and `rk service stop` report and stop it. Select a project with `--root <folder>`, plus `--data-dir` and `--collection` when they are not the defaults.
- **A service you start yourself** (development, tests, external supervisors). The private role `airk __airk_role service-foreground --runtime-dir <control>` runs the service in the foreground; it replaces `airk serve --runtime-dir` and is not a user interface. Then add `--service-runtime-dir <control> --service-project <binding> --root … --data-dir … --collection …` to project commands.

## Reviewed index and Project setup in 1.8.8

The working build activates authority-1 catalogs through the qualified native
installer. New indexes require the catalog setup flow; `init` and `service register`
cannot bypass it. Query Project membership and evidence grants are separate from
creating an index. [Reviewed setup](service-project-setup.md) describes those
boundaries; [combined reads](combined-reads.md) describes evidence selection.

Headless administration uses the same native methods as the editor. Save one
method JSON value in a file and run:

```console
rk service rpc --runtime-dir <control> --request-file <method.json>
```

For example, this read-only method lists a bounded first page of indexes:

```json
{"catalog_page":{"request":{"kind":"indexes","index_id":null,"project_id":null,"after":null,"expected_catalog_revision":null,"limit":16,"max_bytes":16384}}}
```

Use the returned revision and continuation for later pages; do not infer a
complete library from one page. Current setup follows these separate steps:

1. Read `"request_epoch"` through `service rpc` and retain its namespace/epoch.
   Choose one new 32-hex nonce for the request key. Submit
   `catalog_index_setup_draft` with a `selection` containing that key, the current
   `expected_catalog_revision`, `name`, `primary_root`, `data_root`, `collection`,
   `additional_roots` (an array of alias/path objects) and `setup_watch`.
2. Inspect the returned complete native `draft`, canonical registration and
   storage action. Only after review, send
   `{"catalog_index_setup_apply":{"draft":<exact returned draft>}}`.
   Keep the original draft and its request caller/key for
   `catalog_index_setup_receipt`; uncertain outcomes do not permit a new key.
3. Create a query Project and its memberships using `catalog_preview` and
   explicit `catalog_apply` of the same request/preview digest. Use
   `catalog_host_context` and `catalog_grant_draft` for a reviewed grant, then
   explicitly apply that grant. The complete method and request shapes are in
   [native metadata methods](../crates/airk-core/src/service.rs),
   [index setup](../crates/airk-core/src/catalog_setup.rs) and
   [catalog operations](../crates/airk-core/src/catalog_management.rs).
4. Select that grant explicitly with `--catalog-session <reviewed-session-json>`
   for one index or `--combined-session <reviewed-session-json>` for a Project
   selection. Do not mix these flags with root/data/collection/project selectors.
   Native validation still checks the grant, catalog revision and complete scope.
   Management and indexing jobs use the native backing registration separately.

These are raw metadata APIs: inspect the native `status` and complete receipt,
not just the CLI exit code. A ready index, a saved grant and a currently authorized
evidence session are distinct outcomes. Missing/expired receipts are unknown;
never regenerate request identities to replay an uncertain mutation.

For removal, request `catalog_source_mutation_draft` with the current index ID,
catalog revision and request key, and `change: {"operation":"retire_index"}`.
Review and explicitly apply its exact `request` and `preview_digest` through
`catalog_apply`, then read `catalog_receipt` using the original caller/key.
Retirement removes current references while preserving frozen grants; physical
cleanup has its own proof. See [guarded index retirement](service-project-retirement.md).
The legacy `configure`/`unregister` path does not replace this catalog review.

## Legacy authority-zero registration examples

For current authority-1 indexes, source additions/removals and index retirement
require a reviewed `CatalogSourceMutationDraft` through `service rpc`, followed
by its exact apply and fenced receipt. Legacy `configure` operations such as
`unregister` refuse with `source_change_review_required`; retrying that request
does not replace review. Ordinary watch preferences remain separate from this
source-authority boundary.

The following retained commands apply to authority-zero services. They are not
setup instructions for new indexes in the current authority-1 catalog. Indexes
and settings live in the data directory; the source folder is only read.

```text
# A service you started:
rk --root /path/to/folder --data-dir /path/to/index --collection mydocs service prepare-registration   # read-only: prints the binding
rk --root /path/to/folder --data-dir /path/to/index --collection mydocs service register --runtime-dir <control>
rk projects --runtime-dir <control>

# The installed service (the default data directory is used when --data-dir is omitted):
rk --root /path/to/folder init
rk projects
```

Then queue indexing with `rk <scope> index`. Run `rk <scope> watch on` to keep the index current, and check progress with `rk <scope> tools call airk_status --args '{}'`.

On macOS and Linux, keep the data directory private to your account (for example `mkdir -m 700`). AIRK creates new storage directories with mode 0700 and never changes the permissions of an existing one. If other accounts may be able to read a project's storage, the pre-split `runtime inventory` reports a `storage_not_private` warning for it (reachable only through the interim setup role until its `airkctl` replacement); registration is not refused. The source folder and the data directory must not contain each other. To remove a project's index and settings, leaving the sources untouched, run `rk <scope> configure --stdin` with `{"operation":"unregister"}`.

## Command families

Current MCP clients use the service's authenticated HTTPS endpoint.
`rk agents connect` enrolls a reviewed selection and writes AIRK's entry for
Codex, Claude Code or VS Code; `agents list`, `repair`, `remove` and `verify`
classify, fix, remove and probe those entries. Writing an entry does not prove
that a client connected. See [native agent setup](agent-integration.md#native-setup-output).
The `mcp` and `bridge` stdio commands are retired.
After complete known-client migration, current installation must finish native
legacy-launcher cleanup and retire all approved superseded executable versions,
including later modern-to-modern updates. Preserve the selected runtime and all
source/index state; blocked cleanup leaves a resumable incomplete transaction.

| Family | Commands | Purpose |
|---|---|---|
| Service | `service start`, `stop`, `restart`, `status`, `health`, `shutdown`, `logs`, `abandon-lifecycle`, `register`, `prepare-registration`, `prepare-configuration`, `prepare-storage`, `record-error`, `rpc`, `requests …` | Run and manage the service and its registry |
| Projects | `projects`, `init`, `index`, `inspect`, `pause`, `resume`, `watch`, `setup-model`, `settings`, `configure`, `status` | Register, index and configure projects |
| Evidence | `search`, `files`, `read`, `info`, `outline`, `symbol`, `imports`, `facts`, `refs`, `context`, `changes`, `code-facts`, `relationships`, `parse` | Read-only, cited queries; `parse` plans or runs multi-step requests |
| MCP tools | `tools list`, `tools help`, `tools call`, `tools shell` | Canonical tool discovery and reads from the terminal; current agent transport is native HTTPS |
| Explorer Assistance | `explorer draft-search`, `draft-strategy`, `draft-plan`, `suggest-view`, `validate-draft`, `list-templates`, `get-template`, `proposal-status`, `get-handoff` | Editor proposals written into files (see below) |
| Agents | `agents endpoint`, `info`, `list`, `connect`, `repair`, `remove`, `verify`, `credential`, `launch`, `revoke` | Agent connections and their configuration entries for Codex, Claude Code and VS Code |
| Other | `version`, `model-catalog`, `session` | Version document, model catalog, and the per-window framed session |
| Usage | `usage report`, `policy`, `configure`, `clear`, `feedback`, `import`, `calibrate` | Local usage reporting |

The pre-split `runtime …`, `setup`, `uninstall`, `host` and `service legacy …` operations are not `airkctl` commands. Until the Phase 7 setup role and installer replace them (U93, a non-shippable interim), they are reachable only as `airk __airk_role setup-interim <arguments>`. Some were renamed: `runtime http-*` became `airkctl agents …` (`runtime http-trust` and `http-issuer-rotation`, and their later `airkctl service trust|issuer-rotation`, were removed with loopback TLS by local trust plan C on 23 November 2026), `setup` enrollment became `agents connect`, `runtime lifecycle` became `service stop|restart`, and `runtime inspect|diagnose` became `service status`. `mcp`, `bridge`, `service session`, `service console`, `explorer-assistance` and `runtime launch` are retired.

## Explorer Assistance from the CLI

In VS Code these tools send proposals to an open Explorer window's Suggestions Inbox. The CLI versions never do that: every result goes to the file named by the required `--output-file`, which must not already exist and is created owner-only on Unix. `--args` takes the same JSON object as the MCP tool of the same name.

- `draft-search`, `draft-strategy`, `draft-plan` and `suggest-view` check the draft against the same generated schemas the MCP server embeds, then write it with `valid`, `errors`, `admitted: false` and `executed: false`. An invalid draft is still written, and the command exits non-zero. The editor's additional semantic and live checks are not run.
- `validate-draft` writes the validation result only.
- `list-templates` and `get-template` read the bundled strategy pack. It is generated from the extension source and checked for drift at build time.
- `proposal-status --draft <file>` reports on a draft file written by the CLI. Such drafts are never admitted to an Inbox.
- `get-handoff --handoff-id <id>` writes a handoff that a VS Code window shared through its session.

## Tests

`crates/airk-cli/tests/cli_commands.rs` exercises the query, management and Explorer commands through `airkctl` against a private service started with the `service-foreground` role. For commands that need an installed runtime, a login manager, legacy state or an editor, it names the native test or qualification script that covers them, and it fails if any command in the reference has no listed test.
