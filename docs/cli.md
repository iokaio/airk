# AIRK command-line guide

P7 adds explicit reviewed combined sessions, unique-index execution, shared
budgets and complete member-qualified results. [Combined-read contracts](combined-reads.md)
record selectors, discovery, publication vectors, plan verification and the
current qualification boundary. Legacy source bindings retain their one-index
meaning. Use the actual connection catalog for supported inputs.

The 1.8.8 working build supplies optional [evidence guidance](evidence-guidance.md)
from returned metadata. Advice is bounded and inert; CLI/MCP/Workbench and
complete/selected exports retain applicable reasons without additional reads.

The `airk` executable is the whole engine, not only a VS Code helper. This guide explains how the commands fit together. The [CLI reference](cli-reference.md) documents every public command and option exactly as `airk <command> --help` prints them; it is generated from the parser, and a test keeps it current. [Running AIRK headless](headless-cli.md) maps each VS Code feature to its command.

For each command's purpose, implementation, tests and guarded behavior, see the [CLI developer guide](cli-developer-guide.md). The [MCP developer guide](mcp-developer-guide.md) covers the corresponding agent interface. `airk tools list` exposes 19 native prefixed operations, including `query_scope`; unified MCP adds projects and nine Assistance operations. The terminal exposes those separately through `airk projects` and `airk explorer`, so the native catalog size is not a count of all CLI capabilities.

## Complete read results in 1.8.7

The 1.8.8 working build adds [Lean/Rich reply selection](reply-detail.md): use
`--reply-detail lean|rich` on read aliases or `reply_detail` in tools-call/shell
JSON. This is independent of retained indexing detail. Omitting it preserves
the inherited defaults described below; conflicts with legacy projection flags
reject instead of choosing precedence.

CLI tool calls, dedicated read commands and Workbench plans share the MCP read handlers. Full results retain provenance, publication, readiness, coverage, continuations and error/retry details. `airk tools call airk_status` returns its MCP envelope, while `airk parse --execute '/status'` places the complete status result under `steps[].result`, including `query_concurrency`. The shell pretty-prints the same tool content. Defaults can differ between convenience commands; use matching arguments and [the parity audit](read-interface-parity.md) when comparing results.

For explicit plans, put a JSON object in the `plan` field and pass the request through `airk tools call airk_parse --args-file request.json`. Do not encode the plan object as a string. Workbench's complete JSON/Markdown exports retain the full response; selected-evidence exports identify their selected subset.

MCP-compatible status results can report `status_timeout` with `index_error.stage: setup`, unknown readiness and null evidence. Later polls preserve that explicit setup-deadline outcome after already accepted work drains. Apply the existing bounded read-retry contract with the same scope; genuine setup failures and ordinary session cancellation retain their failure handling. See the [status contract](mcp.md).

## Per-file profile metadata — P11a

The unpublished **1.8.8 P11a working build** adds an explicit opt-in to FileInfo:

```sh
airk info src/example.rs --include-profile-families --verification off

airk tools call airk_file_info --args '{"path":"src/example.rs","include_profile_families":true,"verification":"off"}'
```

Inside `airk tools shell`, enter the same JSON directly, without shell quoting:

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

For a known type/function, use compact `airk symbol` first, scoped `airk refs` for textual uses, and `airk read`/`context` to inspect a selected location. `airk outline` reads one file's structure in source order; it does not propose an Explorer screen. Search remains useful for concepts and supplementary passages, with bounded same-publication definition pointers where available.

`symbol` and `code-facts` keep exact matching by default; `--name-match suffix|contains` explicitly expands simple/qualified names case-sensitively. `--include-count` requests a qualified count, whose total can remain unknown under bounded scanning. Search and refs accept indexed `--language`, `--evidence-kind code|document|configuration|unknown` and literal `--exclude-path-prefix`, applied before candidate ranking rather than to a tiny returned top-k.

References add `--order index|path|code_first`, `--include-files`, `--include-count`, `--cursor`, `--limit` and `--max-bytes`. New selectors opt into a paged response; omitted selectors preserve the preceding response contract. Continue with the same identifier, scope, filters and ordering. File summaries count matching lines returned on this page, not every occurrence or every matching file. A too-small byte budget can produce an empty page with required bytes and a resumable cursor; terminal scan/absolute-record limits instead remain explicit coverage gaps. See the [developer guide](cli-developer-guide.md#identifier-investigations-and-bounded-discovery) for the full contract and examples.

## Conventions

- Ordinary one-shot commands print JSON on stdout. Help, text/CSV/Markdown exports and interactive `tools shell` have their own output formats. MCP stdio and persistent service console are protocol streams; foreground service/host commands retain ownership until stopped.
- A failure exits non-zero and writes a message to stderr. Set `AIRK_ERROR_FORMAT=json` to get a structured error instead. Query commands can return a typed busy or retry refusal while workers start; repeat the same call after the reported delay.
- Source files are never modified. Commands that change AIRK state (registering, configuring, pausing, purging) say so in their help.
- Agents can read the same help through MCP: `airk tools call airk_help --args '{"command":"search"}'`.

## Services, projects and scope

One **service** owns a control directory: its registry, job queue and worker
processes. In the current catalog, an **index** has an independent native backing
registration; a query **Project** groups reviewed source/path memberships from
one or more indexes. A legacy lowercase **project** is the source/data/collection
registration with a 64-character hexadecimal **binding**. These identities are
not interchangeable.

There are two ways to reach a service:

- **Installed runtime** (what the VS Code extension sets up). Commands find the per-user installation automatically; `--service-install-dir` selects another one. Select a project with `--root <folder>`, plus `--data-dir` and `--collection` when they are not the defaults.
- **A service you start yourself** (headless, no installation). Run `airk serve --runtime-dir <control>`. Then add `--service-runtime-dir <control> --service-project <binding> --root … --data-dir … --collection …` to project commands.

## Reviewed index and Project setup in 1.8.8

The working build activates authority-1 catalogs through the qualified native
installer. New indexes require the catalog setup flow; `init` and `service register`
cannot bypass it. Query Project membership and evidence grants are separate from
creating an index. [Reviewed setup](service-project-setup.md) describes those
boundaries; [combined reads](combined-reads.md) describes evidence selection.

Headless administration uses the same native methods as the editor. Save one
method JSON value in a file and run:

```console
airk service rpc --runtime-dir <control> --request-file <method.json>
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

The following retained commands apply to authority-zero services. They are not
setup instructions for new indexes in the current authority-1 catalog. Indexes
and settings live in the data directory; the source folder is only read.

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

On macOS and Linux, keep the data directory private to your account (for example `mkdir -m 700`). AIRK creates new storage directories with mode 0700 and never changes the permissions of an existing one. If other accounts may be able to read a project's storage, `airk runtime inventory` reports a `storage_not_private` warning for it; registration is not refused. The source folder and the data directory must not contain each other. To remove a project's index and settings, leaving the sources untouched, run `airk <scope> configure --stdin` with `{"operation":"unregister"}`.

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
