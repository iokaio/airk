# CLI developer guide

AIRK supplies optional [evidence guidance](evidence-guidance.md)
from returned metadata. Advice is bounded and inert; CLI/MCP/Workbench and
complete/selected exports retain applicable reasons without additional reads.

Current command behavior after split Phase 4 ([split plan](split/split-plan.md) §5–§6): `airkctl` is the command-line client and `airk` is the service only (see [the `airk` executable](#the-airk-executable)). This guide explains why each public command exists, how it behaves and where developers should follow its implementation. The generated [CLI reference](cli-reference.md) remains authoritative for every flag, default and complete parser help; [CLI workflows](cli.md) and [headless feature mapping](headless-cli.md) provide task-oriented walkthroughs. Current implementation and qualification limits are tracked in the [1.8.14 record](releases/release-1.8.14.md); [1.8.8](releases/release-1.8.8.md) and [1.8.7 evidence](releases/release-1.8.7.md) remain dated history.

**Related developer reference: [MCP developer guide](mcp-developer-guide.md)** explains every unified MCP tool, inputs, examples, limitations and implementation/test ownership.

## Native and unified catalogs

`airkctl tools list --json` describes the selected connection. CLI
`tools list/help/call/shell` and public HTTPS share
the 29-tool unified catalog, public aliases and argument validation. Its 19 native
evidence/help/plan/scope operations are a backend subset; `projects` and nine
Assistance operations retain their distinct connection/proposal authority. A
reviewed `--combined-session` adds Project/index selectors to eligible reads.
A legacy connection's `query_scope` reports `configuration_required`. Use the
actual catalog rather than treating a tool count as a capability check.
Scope calls and the tools shell resolve the configured native authority; an
absent installation is an error, not an empty legacy scope. A fresh reviewed
grant requires its explicit session rather than inferring access from a root
or registration. Tool lists and help are served by the running service.

Use `--project-ids` and optional `--index-ids` to narrow a combined convenience
read, or pass the same fields through `tools call`/shell. `--member-availability`
is independent of source verification. The session JSON selects a reviewed grant;
IDs, saved defaults and the Workbench dropdown cannot grant access. Complete
member results, qualified ranking, publication vectors and typed failures survive
CLI and MCP output. See [combined reads](combined-reads.md).

| Unified MCP capability | Unified terminal entrypoint | Separate convenience or administrative command |
|---|---|---|
| Native operations | `airkctl tools call search`, or the same public name in `tools help`/shell | Direct commands such as `airkctl search` provide convenient flags. Historical `airk_<operation>` aliases remain accepted. |
| `projects` | `airkctl tools call projects` lists connection-configured bindings | `airkctl projects` lists the selected service registry and is a distinct administrative read. |
| `list_templates`, `get_template` | `airkctl tools call list_templates` or `airkctl tools call get_template` | `airkctl explorer list-templates/get-template` writes results to a new `--output-file`. |
| `draft_search`, `draft_strategy`, `draft_plan`, `suggest_view`, `validate_draft` | `airkctl tools call <public-name>` retains the same connection/proposal authority as MCP | Corresponding hyphenated `airkctl explorer` commands produce offline files. Connected proposals require an attached trusted Explorer and user review; neither route executes the proposed work. |
| `proposal_status` | `airkctl tools call proposal_status` reads an admitted proposal receipt | `airkctl explorer proposal-status` reads a CLI-produced draft file. |
| `get_handoff` | `airkctl tools call get_handoff` | `airkctl explorer get-handoff` reads a handoff a VS Code window shared through its session, by `--handoff-id`. |

The public parser reference enumerates commands and command groups. This guide also includes the explicitly implemented `airkctl tools help` command, which the reference walker omits as a help-named child. Command sections are not independent evidence tools.

## Developer contracts and examples

In 1.8.8, the 15 native read aliases expose `--reply-detail lean|rich`.
Tools-call/shell use the native `reply_detail` JSON field. The selection is
independent of retained indexing detail; omitted arguments preserve legacy
defaults. See [reply detail](reply-detail.md) and the regenerated parser reference
for supported flags, conflicts, parse defaults and Workbench equivalents.

Equivalent CLI, MCP and Workbench reads share the canonical handlers and retain complete result data. `tools call` emits the MCP envelope; `tools shell` pretty-prints its text payload; dedicated aliases build arguments with their documented defaults. Direct status/diagnostics report `query_concurrency` at the result's top level; executed parse plans retain it inside each completed status/diagnostics step. [The parity audit](read-interface-parity.md) records all 17 read mappings, supported inputs and the actual-process regressions. Separate requests retain their own request IDs, observation times and cache ages.

- Select the canonical source root, separate index data directory and collection/binding. A UI dropdown does not redirect a native query. Keep index/control/log storage outside source roots.
- Published evidence is an indexed snapshot. Preserve publication tokens, hashes, validators, original locations, continuation filters and byte budgets. Verify live files separately before editing.
- Ordinary queries print JSON; failures exit nonzero with diagnostics on stderr (`--output json` selects JSON error lines). Help/text/CSV/Markdown output, the interactive tools shell and the window session (`airkctl session`) have their documented output/lifetime contracts.
- Source files remain read-only. Management commands can change owned registry, settings, jobs, indexes, logs and agent configuration entries. The service also maintains caches/accounting metadata.
- Honor exact typed retry hints only for the eligible read. Unknown submission outcomes require the saved durable request/nonce contract; do not submit a new operation identity to guess whether a mutation succeeded.
- A status setup deadline retains its explicit expiry for later polls after accepted setup work drains. Keep `status_timeout` stage `setup` as unknown readiness with null evidence; generic cancellation text and genuine verification failures are not retry receipts. See the [MCP status contract](mcp.md).
- The invocation shapes below come from parser help. Angle-bracket arguments are placeholders, not runnable approved tokens. Obtain guards/digests from the appropriate reviewed preview; do not fabricate them.

For an already registered project, run these separately, supplying the same root/data/collection selection when needed:

```console
airkctl status
airkctl files crates/
airkctl search "publication" --mode lexical
airkctl parse "Find the definition of Owner"
airkctl tools list --json
airkctl tools help airk_search --json
```

parse prepares by default; inspect its plan before explicitly using --execute. Queue indexing separately with airkctl index, then observe its receipt/status until readiness is reported. For a guarded management change, prepare/preview → review exact identities and bytes → submit/confirm once → reconcile the same saved receipt.

## Identifier investigations and bounded discovery

For a known type or function, start compact with `airkctl symbol`, inspect exact textual uses with scoped `airkctl refs`, and expand one chosen definition using `airkctl read` or `airkctl context`. Use `airkctl outline` for one unfamiliar file's structure in source order. It returns evidence; the similarly named Explorer navigation route is a UI suggestion. `airkctl search` complements these lookups for concepts, documentation and code passages.

```console
airkctl symbol Session --name-match suffix --path-prefix crates/ --limit 30 --include-count
airkctl code-facts --name Owner --path-prefix crates/ --compact false
airkctl refs Owner --language rust --order path --include-files --include-count
airkctl search "ownership policy" --mode hybrid --evidence-kind document
```

These examples are invocation shapes, not claims about this repository's result counts. Search and refs accept exact indexed `--language`, `--evidence-kind code|document|configuration|unknown`, and a literal `--exclude-path-prefix`. Classification follows saved extraction metadata; mixed or unclassified extraction remains unknown. These query filters neither change saved indexing nor bypass scope authorization. Both search branches filter before top-k selection, with independent bounded candidate-discovery coverage.

Symbol/code-facts matching stays exact when `--name-match` is omitted. Explicit suffix/contains compares simple or qualified names case-sensitively; opaque symbol IDs are not substring targets. `--include-count` requests qualified count metadata. Exact totals appear only when bounded work establishes them; otherwise inspect lower bound, completeness and count scope. Durable `--symbol-key` lookup excludes these matching/count options.

References retain the preceding selection/response contract unless a new filter, cursor, order or summary/count option selects the paged contract. Paged `index` order is deterministic stable-ID order. Use the returned cursor with unchanged identifier, filters, ordering and representation; count/text budgets may increase within supported limits when a complete record needs more space. `--include-files` summarizes returned-page records in the native `count_unit`, not every matching file or every identifier occurrence. Summary metadata is additional to the text budget and capped at 16 rows/4 KiB. UTF-8 matches reconstruct complete original lines; extracted non-UTF-8 fragments remain explicitly qualified. A cursor cannot silently advance to a different publication. `--order code_first` is a presentation choice, not compiler-resolved impact analysis.

Reference totals remain unknown for the whole query. A complete bounded scan can count only `verified_lexical_candidates`; partial count scope is `page_scan`. `page.coverage` states that token-boundary coverage is incomplete: an identifier split across indexed chunk tokenization may be missing from postings, even when zero candidates match. Candidate exhaustion is not proof of no uses. Use live verification or additional scoped source reads when completeness matters, and inspect line/fragment units and terminal coverage limits before interpreting counts.

For a relationship investigation, a returned type/impl ID may provide bounded associated-method targets when retained owner/scope evidence supports them. Follow written targets such as `Owner::new` for incoming call candidates; a struct with no incoming edges does not prove no uses. Duplicate names and incomplete extraction remain qualified. Keep exact publication tokens, original locations and citations when copying CLI output; terse receipt suppression is not a replacement for that evidence identity.

## Command index

- [airkctl](#airkctl)
- [airkctl agents](#airkctl-agents)
- [airkctl agents connect](#airkctl-agents-connect)
- [airkctl agents credential](#airkctl-agents-credential)
- [airkctl agents endpoint](#airkctl-agents-endpoint)
- [airkctl agents info](#airkctl-agents-info)
- [airkctl agents launch](#airkctl-agents-launch)
- [airkctl agents list](#airkctl-agents-list)
- [airkctl agents remove](#airkctl-agents-remove)
- [airkctl agents repair](#airkctl-agents-repair)
- [airkctl agents revoke](#airkctl-agents-revoke)
- [airkctl agents verify](#airkctl-agents-verify)
- [airkctl changes](#airkctl-changes)
- [airkctl code-facts](#airkctl-code-facts)
- [airkctl configure](#airkctl-configure)
- [airkctl context](#airkctl-context)
- [airkctl explorer](#airkctl-explorer)
- [airkctl explorer draft-plan](#airkctl-explorer-draft-plan)
- [airkctl explorer draft-search](#airkctl-explorer-draft-search)
- [airkctl explorer draft-strategy](#airkctl-explorer-draft-strategy)
- [airkctl explorer get-handoff](#airkctl-explorer-get-handoff)
- [airkctl explorer get-template](#airkctl-explorer-get-template)
- [airkctl explorer list-templates](#airkctl-explorer-list-templates)
- [airkctl explorer proposal-status](#airkctl-explorer-proposal-status)
- [airkctl explorer suggest-view](#airkctl-explorer-suggest-view)
- [airkctl explorer validate-draft](#airkctl-explorer-validate-draft)
- [airkctl facts](#airkctl-facts)
- [airkctl files](#airkctl-files)
- [airkctl imports](#airkctl-imports)
- [airkctl index](#airkctl-index)
- [airkctl info](#airkctl-info)
- [airkctl init](#airkctl-init)
- [airkctl inspect](#airkctl-inspect)
- [airkctl jobs](#airkctl-jobs)
- [airkctl model-catalog](#airkctl-model-catalog)
- [airkctl outline](#airkctl-outline)
- [airkctl parse](#airkctl-parse)
- [airkctl pause](#airkctl-pause)
- [airkctl progress](#airkctl-progress)
- [airkctl projects](#airkctl-projects)
- [airkctl read](#airkctl-read)
- [airkctl refs](#airkctl-refs)
- [airkctl relationships](#airkctl-relationships)
- [airkctl resume](#airkctl-resume)
- [airkctl search](#airkctl-search)
- [airkctl service](#airkctl-service)
- [airkctl service abandon-lifecycle](#airkctl-service-abandon-lifecycle)
- [airkctl service health](#airkctl-service-health)
- [airkctl service issuer-rotation](#airkctl-service-issuer-rotation)
- [airkctl service logs](#airkctl-service-logs)
- [airkctl service prepare-configuration](#airkctl-service-prepare-configuration)
- [airkctl service prepare-registration](#airkctl-service-prepare-registration)
- [airkctl service prepare-storage](#airkctl-service-prepare-storage)
- [airkctl service record-error](#airkctl-service-record-error)
- [airkctl service register](#airkctl-service-register)
- [airkctl service requests](#airkctl-service-requests)
- [airkctl service requests forget](#airkctl-service-requests-forget)
- [airkctl service requests list](#airkctl-service-requests-list)
- [airkctl service requests prepare](#airkctl-service-requests-prepare)
- [airkctl service requests prepare-configuration](#airkctl-service-requests-prepare-configuration)
- [airkctl service requests retry](#airkctl-service-requests-retry)
- [airkctl service restart](#airkctl-service-restart)
- [airkctl service rpc](#airkctl-service-rpc)
- [airkctl service shutdown](#airkctl-service-shutdown)
- [airkctl service start](#airkctl-service-start)
- [airkctl service status](#airkctl-service-status)
- [airkctl service stop](#airkctl-service-stop)
- [airkctl service trust](#airkctl-service-trust)
- [airkctl session](#airkctl-session)
- [airkctl settings](#airkctl-settings)
- [airkctl setup-model](#airkctl-setup-model)
- [airkctl status](#airkctl-status)
- [airkctl symbol](#airkctl-symbol)
- [airkctl tools](#airkctl-tools)
- [airkctl tools call](#airkctl-tools-call)
- [airkctl tools help](#airkctl-tools-help)
- [airkctl tools list](#airkctl-tools-list)
- [airkctl tools shell](#airkctl-tools-shell)
- [airkctl usage](#airkctl-usage)
- [airkctl usage calibrate](#airkctl-usage-calibrate)
- [airkctl usage clear](#airkctl-usage-clear)
- [airkctl usage configure](#airkctl-usage-configure)
- [airkctl usage feedback](#airkctl-usage-feedback)
- [airkctl usage import](#airkctl-usage-import)
- [airkctl usage policy](#airkctl-usage-policy)
- [airkctl usage report](#airkctl-usage-report)
- [airkctl version](#airkctl-version)
- [airkctl watch](#airkctl-watch)
- [The `airk` executable](#the-airk-executable)

## airkctl

**Purpose:** AIRK command-line client: reaches the AIRK service through its local control endpoint.

**Behavior and developer use:** Choose a subcommand. Global options select the project and transport rather than running a query themselves: `--root`, `--data-dir`, `--collection`, `--service-runtime-dir`, `--service-project`, `--catalog-session` and `--combined-session` (with `--project-ids`, `--index-ids` and `--member-availability` for combined reads). `--output json` selects JSON results and JSON error lines.

```text
Usage: airkctl [OPTIONS] <COMMAND>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). **Tests:** [cli/tests.rs](../crates/airkctl/src/cli/tests.rs). Full options: [parser reference](cli-reference.md#airkctl).

## airkctl agents

**Purpose:** Agent connections and configuration: endpoint, info, list, connect, repair, remove, verify, credential, launch and revoke.

**Behavior and developer use:** Command group: choose a child listed below or run --help. These replace the pre-split `runtime http-*` commands and the enrollment part of `setup`. In every child, --runtime-dir defaults to --service-runtime-dir, else the installation's control directory.

```text
Usage: airkctl agents [OPTIONS] <COMMAND>
```

**Implementation:** [agents.rs](../crates/airkctl/src/agents.rs). Full options: [parser reference](cli-reference.md#airkctl-agents).

## airkctl agents connect

**Purpose:** Write an AIRK entry for Claude Code, Codex or VS Code, enrolling a reviewed selection (--selection-file with a new --connection-id) or reusing an existing connection.

**Behavior and developer use:** Writes the project's file, or the user-level file with --user. Credential helpers in the written entry name this airkctl, or --airkctl. Only AIRK's own entry is touched.

```text
Usage: airkctl agents connect [OPTIONS] --client <CLIENT> --connection-id <CONNECTION_ID>
```

**Implementation:** [manage.rs](../crates/airkctl/src/agents/manage.rs). **Tests:** [agents/config/tests.rs](../crates/airkctl/src/agents/config/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-connect).

## airkctl agents credential

**Purpose:** Print an existing, unrevoked connection's bearer header for an agent's header helper.

**Behavior and developer use:** Offline: reads only the owner-private credential files; never starts the service, enrolls, rotates or logs the token. `--format header-json` prints `{"Authorization":"Bearer <token>"}`. Entries written by `agents connect` name this command as their helper.

```text
Usage: airkctl agents credential [OPTIONS] --connection-id <CONNECTION_ID> --format <FORMAT>
```

**Implementation:** [agents.rs](../crates/airkctl/src/agents.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-credential).

## airkctl agents endpoint

**Purpose:** Read the owner-authenticated local HTTPS endpoint without enrolling a client.

**Behavior and developer use:** Prints the document `runtime http-endpoint` printed. It issues no credential.

```text
Usage: airkctl agents endpoint [OPTIONS]
```

**Implementation:** [agents.rs](../crates/airkctl/src/agents.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-endpoint).

## airkctl agents info

**Purpose:** Read an existing connection's verified references without issuing credentials.

**Behavior and developer use:** Prints the document `runtime http-connection-info` printed for --connection-id.

```text
Usage: airkctl agents info [OPTIONS] --connection-id <CONNECTION_ID>
```

**Implementation:** [agents.rs](../crates/airkctl/src/agents.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-info).

## airkctl agents launch

**Purpose:** Start Claude Code, Codex or VS Code on this connection in the foreground.

**Behavior and developer use:** The credential goes only into the client's environment and the AIRK issuer is added to its trust; existing CA bundles stay enabled. Arguments after `--` go to the client. Exits with the client's status.

```text
Usage: airkctl agents launch [OPTIONS] --client <CLIENT> --connection-id <CONNECTION_ID> [-- <ARGS>...]
```

**Implementation:** [launch.rs](../crates/airkctl/src/launch.rs). **Tests:** [launch/tests.rs](../crates/airkctl/src/launch/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-launch).

## airkctl agents list

**Purpose:** Classify AIRK's entries in agent configuration files; read-only.

**Behavior and developer use:** Checks the given --project folders (default: the current directory) and, unless --no-user, this user's files. With --connection-id it reports only whether this installation owns that connection.

```text
Usage: airkctl agents list [OPTIONS]
```

**Implementation:** [manage.rs](../crates/airkctl/src/agents/manage.rs). **Tests:** [agents/config/tests.rs](../crates/airkctl/src/agents/config/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-list).

## airkctl agents remove

**Purpose:** Remove AIRK's entries (only those naming --connection-id, when given).

**Behavior and developer use:** Uses the same project/user file selection as `agents list`. It edits configuration files only; `agents revoke` revokes a connection.

```text
Usage: airkctl agents remove [OPTIONS]
```

**Implementation:** [manage.rs](../crates/airkctl/src/agents/manage.rs). **Tests:** [agents/config/tests.rs](../crates/airkctl/src/agents/config/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-remove).

## airkctl agents repair

**Purpose:** Rewrite AIRK entries whose endpoint or helper is stale; --connection-id replaces an unknown connection.

**Behavior and developer use:** Uses the same project/user file selection as `agents list`. Only recognized AIRK entries are rewritten.

```text
Usage: airkctl agents repair [OPTIONS]
```

**Implementation:** [manage.rs](../crates/airkctl/src/agents/manage.rs). **Tests:** [agents/config/tests.rs](../crates/airkctl/src/agents/config/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-repair).

## airkctl agents revoke

**Purpose:** Revoke one local HTTPS connection while preserving its original receipt.

**Behavior and developer use:** Prints the document `runtime http-revoke` printed. Configuration entries naming the connection remain until `agents remove` removes them.

```text
Usage: airkctl agents revoke [OPTIONS] --connection-id <CONNECTION_ID>
```

**Implementation:** [agents.rs](../crates/airkctl/src/agents.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-revoke).

## airkctl agents verify

**Purpose:** Probe the service's own MCP endpoint with a connection's stored credential.

**Behavior and developer use:** Checks one --connection-id against the running service.

```text
Usage: airkctl agents verify [OPTIONS] --connection-id <CONNECTION_ID>
```

**Implementation:** [agents.rs](../crates/airkctl/src/agents.rs). Full options: [parser reference](cli-reference.md#airkctl-agents-verify).

## airkctl changes

**Purpose:** Report indexed changes since a revision token: added, modified and removed files, symbols and documents

**Behavior and developer use:** Consume all continuation pages before advancing the retained revision baseline. index-current describes the publication, not a live-file check; unavailable retained history is an explicit gap.

```text
Usage: airkctl changes [OPTIONS]
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-changes).

## airkctl code-facts

**Purpose:** Inspect code facts and signatures with exact default name matching and bounded continuation.

**Behavior and developer use:** Use after compact discovery when signatures, scopes, IDs or call candidates matter. --name-match exact|suffix|contains and --include-count share the qualified discovery contract; every explicit matching mode, including exact, needs --name. --symbol-key retains exact identity and excludes textual matching/count flags. Compactness, byte budget and cursor remain independent of response mode.

```text
Usage: airkctl code-facts [OPTIONS]
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-code-facts).

## airkctl configure

**Purpose:** Apply a guarded project configuration draft from --file or --stdin (JSON with an "operation").

**Behavior and developer use:** Reads a guarded JSON operation and applies it through native configuration ownership. Review the draft and its current guards; do not manufacture digests or replay uncertain changes with a new identity.

```text
Usage: airkctl configure [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-configure).

## airkctl context

**Purpose:** Read lines around --line in a file (--before/--after), with a citation

**Behavior and developer use:** Use a known indexed path and line with bounded surrounding lines; it avoids retrieving the whole file and retains source citations.

```text
Usage: airkctl context [OPTIONS] <PATH> <LINE>
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-context).

## airkctl explorer

**Purpose:** Offline Explorer drafts, validation and templates; results go to a new --output-file.

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations. `airkctl` sends each operation to the running service, which runs it.

```text
Usage: airkctl explorer [OPTIONS] <COMMAND>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer).

## airkctl explorer draft-plan

**Purpose:** Write proposed Planner tasks (MCP draft_plan) after schema validation

**Behavior and developer use:** Validates the bundled Assistance schema and writes a new --output-file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airkctl explorer draft-plan [OPTIONS] --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-draft-plan).

## airkctl explorer draft-search

**Purpose:** Write a proposed Explorer Search form (MCP draft_search) after schema validation

**Behavior and developer use:** Validates the bundled Assistance schema and writes a new --output-file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airkctl explorer draft-search [OPTIONS] --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-draft-search).

## airkctl explorer draft-strategy

**Purpose:** Write a proposed Studio strategy (MCP draft_strategy) after schema validation

**Behavior and developer use:** Validates the bundled Assistance schema and writes a new --output-file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airkctl explorer draft-strategy [OPTIONS] --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-draft-strategy).

## airkctl explorer get-handoff

**Purpose:** Write a handoff a VS Code window shared through its session (MCP get_handoff).

**Behavior and developer use:** Reads only the handoff a window published under --handoff-id through its `airkctl session`, then writes a new --output-file. A handoff id is not authorization to read other editor state.

```text
Usage: airkctl explorer get-handoff [OPTIONS] --handoff-id <HANDOFF_ID> --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-get-handoff).

## airkctl explorer get-template

**Purpose:** Read one strategy template with its operators (MCP get_template; --args {"template_id":â€¦})

**Behavior and developer use:** Validates the bundled Assistance schema and writes a new --output-file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airkctl explorer get-template [OPTIONS] --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-get-template).

## airkctl explorer list-templates

**Purpose:** List the bundled strategy templates (MCP list_templates; optional offset, limit, max_bytes)

**Behavior and developer use:** Validates the bundled Assistance schema and writes a new --output-file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airkctl explorer list-templates [OPTIONS] --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-list-templates).

## airkctl explorer proposal-status

**Purpose:** Report the status of a draft file written by this CLI (it is never admitted to an Inbox)

**Behavior and developer use:** Inspects a CLI draft file, never a live Suggestions Inbox receipt. It cannot establish that a user applied or executed a proposal.

```text
Usage: airkctl explorer proposal-status [OPTIONS] --draft <DRAFT> --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-proposal-status).

## airkctl explorer suggest-view

**Purpose:** Write a Navigation, Review or Management suggestion (MCP suggest_view; --args {"kind":â€¦,"draft":â€¦})

**Behavior and developer use:** Validates the bundled Assistance schema and writes a new --output-file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airkctl explorer suggest-view [OPTIONS] --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-suggest-view).

## airkctl explorer validate-draft

**Purpose:** Validate a draft without writing it as a proposal (MCP validate_draft; --args {"kind":â€¦,"draft":â€¦})

**Behavior and developer use:** Validates the bundled Assistance schema and writes a new --output-file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airkctl explorer validate-draft [OPTIONS] --output-file <OUTPUT_FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs); the service runs it with [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). Full options: [parser reference](cli-reference.md#airkctl-explorer-validate-draft).

## airkctl facts

**Purpose:** List document facts (requirements, dates, decisions, features) extracted from indexed documents

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airkctl facts [OPTIONS] [PATH_PREFIX]
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-facts).

## airkctl files

**Purpose:** List indexed files under an optional path prefix, with continuation cursors

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airkctl files [OPTIONS] [PREFIX]
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-files).

## airkctl imports

**Purpose:** List import and dependency statements found under a path prefix

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airkctl imports [OPTIONS] [PATH_PREFIX]
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-imports).

## airkctl index

**Purpose:** Queue indexing of the project's sources and print the job receipt. Follow it with `tools call airk_status`. `--full` needs a reviewed maintenance preview.

**Behavior and developer use:** Submits a durable job and returns its receipt. Acceptance is not readiness; follow status/job observations instead of assuming this call waits for the complete index.

```text
Usage: airkctl index [OPTIONS]
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-index).

## airkctl info

**Purpose:** Show what the index knows about one file: language, size, extraction method and parse status

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airkctl info [OPTIONS] <PATH>
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-info).

## airkctl init

**Purpose:** Acknowledge an existing reviewed index mapping and prepare its reserved storage.

**Behavior and developer use:** In a current authority-1 catalog, the exact root, data directory and collection must already identify a ready reviewed index. `init` acknowledges that registration and prepares its reserved storage; it creates no index or grant and refuses a fresh unmapped root before creating storage. Use the [reviewed IndexSetup draft/apply/receipt journey](cli.md#reviewed-index-and-project-setup-in-188) through `service rpc` for a new index. Authority-zero services retain their explicitly historical registration behavior. Source files remain unchanged.

```text
Usage: airkctl init [OPTIONS]
```

**Implementation:** [registration.rs](../crates/airkctl/src/registration.rs). Full options: [parser reference](cli-reference.md#airkctl-init).

## airkctl inspect

**Purpose:** Queue a bounded inspection job that refreshes coverage and readiness without reindexing

**Behavior and developer use:** Queues bounded coverage/readiness inspection without requesting a full source reindex. Its job receipt has the same acceptance-versus-completion distinction.

```text
Usage: airkctl inspect [OPTIONS]
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-inspect).

## airkctl jobs

**Purpose:** Read, retry or cancel one job by its ID.

**Behavior and developer use:** `jobs get ID` prints the job record (`Job`); `jobs retry ID` retries a failed job with a fresh request key (`Retry`; not a saved client request, so a lost reply is an unknown outcome to check with `jobs get`); `jobs cancel ID` cancels a pending or running job (`Cancel`). The Explorer's job actions run these commands through the editor session (U103).

```text
Usage: airkctl jobs [OPTIONS] <COMMAND>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs), [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-jobs).

## airkctl model-catalog

**Purpose:** Print the offline model catalog and the CPU choice for this host

**Behavior and developer use:** Served by the running service from bundled model metadata and its CPU selection.

```text
Usage: airkctl model-catalog [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-model-catalog).

## airkctl outline

**Purpose:** Outline one file's structure: symbols, headings and sections with line ranges

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airkctl outline [OPTIONS] <PATH>
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-outline).

## airkctl parse

**Purpose:** Plan, and with --execute run, a multi-step read-only evidence request.

**Behavior and developer use:** Prepare mode returns a bounded plan for review. Only --execute requests execution. Finite parsed or explicitly confirmed plans use native evidence operations; learned proposals remain review-only.

Explicit request files can also select `mode: execute`. In 1.8.8 compact symbol
discovery supports cited aggregation and dependent file/location reads through
the same consumer as MCP and Workbench. Nested plan/recipe/budget/output inputs
are objects. See [compact binding and symbol-ID lookup rules](read-interface-parity.md#regression-evidence)
before composing dependent requests. `tools call` and the tools shell retain
the same native results and validation.

```text
Usage: airkctl parse [OPTIONS] [TEXT]
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-parse).

## airkctl pause

**Purpose:** Pause automatic and queued work for the project; existing evidence stays queryable

**Behavior and developer use:** Pauses project work while published evidence remains readable. It does not stop the shared service or other registered projects.

```text
Usage: airkctl pause [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-pause).

## airkctl progress

**Purpose:** Print the project's live indexing progress: the running job's phase and counts, pending and queued jobs, and pause state.

**Behavior and developer use:** Prints the service's `Progress` observation unchanged: the running job's phase, captured/extracted/prepared/committed counts, publications, elapsed times, stalled and paused flags, the pending and queued jobs and the project's pause state. The admin pages (`indexes.progress`) and the Explorer read the same observation. It needs no read authority, so it works while an index is being built or before it belongs to an index set.

```text
Usage: airkctl progress [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-progress).

## airkctl projects

**Purpose:** List the projects registered with the service (same fields as the MCP `projects` tool).

**Behavior and developer use:** Enumerates the selected service registry (--runtime-dir, else --service-runtime-dir, else the installation's). Unified MCP projects instead lists only bindings configured on that MCP connection; do not equate their visibility.

```text
Usage: airkctl projects [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-projects).

## airkctl read

**Purpose:** Read an indexed file's lines (--start-line/--end-line) from the published revision, with a citation

**Behavior and developer use:** Returns indexed text, not current filesystem contents. Retain its publication, hash, citation and original location, then verify live bytes separately before editing.

```text
Usage: airkctl read [OPTIONS] --start-line <START_LINE> --end-line <END_LINE> <PATH>
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-read).

## airkctl refs

**Purpose:** Find exact-case textual identifier uses; these are candidates, not resolved language references.

**Behavior and developer use:** Returns matches verified within lexical candidates. Optional --language/--evidence-kind/--exclude-path-prefix scopes candidates; --cursor, --order index|path|code_first, --include-files and --include-count select bounded paging and qualified metadata. Repeat identifier, filters and ordering on continuation. Summaries cover this page; exact candidate-scoped counts are not whole-query totals because token seams can hide identifiers. UTF-8 lines are reconstructed; non-UTF-8 fragments retain explicit completeness/unit qualifiers. A record that fits the supported maximum but not the current text budget reports required bytes and a resumable cursor; records over 64 KiB and terminal scan/deduplication limits instead report incomplete coverage without usable continuation. An empty page never proves no uses by itself.

```text
Usage: airkctl refs [OPTIONS] <NAME>
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-refs).

## airkctl relationships

**Purpose:** Follow indexed relationships of a symbol in --direction, optionally of one --kind, up to --hops levels

**Behavior and developer use:** Traverses bounded indexed structural relationships. Direction, kind and hop limits select evidence; candidate edges do not establish fully resolved language semantics.

```text
Usage: airkctl relationships [OPTIONS] <SYMBOL>
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-relationships).

## airkctl resume

**Purpose:** Resume work paused with `pause`

**Behavior and developer use:** Explicitly resumes the selected project, preserving shared-service and other-project isolation.

```text
Usage: airkctl resume [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-resume).

## airkctl search

**Purpose:** Search indexed evidence; results carry file, line and revision citations.

**Behavior and developer use:** Use for concepts and supplementary discovery; named definitions usually begin with symbol/code-facts. --language and --evidence-kind filter indexed extraction before lexical/vector candidate selection; --exclude-path-prefix is literal query scope. Preserve mode/readiness policy, lexical AND semantics, partial coverage and exact citations. Bounded definition pointers can aid the next lookup without changing ranking. Filters do not change indexing policy.

```text
Usage: airkctl search [OPTIONS] <QUERY>
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-search).

## airkctl service

**Purpose:** The AIRK service: start, stop, restart, status, health, logs, trust, registration, saved requests and raw management requests

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airkctl service [OPTIONS] <COMMAND>
```

**Implementation:** [service.rs](../crates/airkctl/src/service.rs). Full options: [parser reference](cli-reference.md#airkctl-service).

## airkctl service abandon-lifecycle

**Purpose:** Mark a stuck stop or restart record abandoned; a finished record is printed unchanged.

**Behavior and developer use:** Takes the record's --request-id. Replaces `runtime abandon-lifecycle`.

```text
Usage: airkctl service abandon-lifecycle [OPTIONS] --request-id <REQUEST_ID>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-service-abandon-lifecycle).

## airkctl service health

**Purpose:** Check that the service for --runtime-dir (default: --service-runtime-dir) is running and healthy; exits non-zero otherwise

**Behavior and developer use:** Targets the selected control directory and native metadata/request protocol. Preserve service, project and durable request identities; management responses are not indexed evidence.

```text
Usage: airkctl service health [OPTIONS]
```

**Implementation:** [service.rs](../crates/airkctl/src/service.rs). Full options: [parser reference](cli-reference.md#airkctl-service-health).

## airkctl service issuer-rotation

**Purpose:** The saved HTTPS issuer rotation and current endpoint (status only in this version).

**Behavior and developer use:** `status` prints what the status action of `runtime http-issuer-rotation` printed, read from the running service. Other rotation actions are not served yet.

```text
Usage: airkctl service issuer-rotation [OPTIONS] <ACTION>
```

**Implementation:** [rotation.rs](../crates/airkctl/src/rotation.rs). Full options: [parser reference](cli-reference.md#airkctl-service-issuer-rotation).

## airkctl service logs

**Purpose:** Read the service's diagnostic logs: recent records from the running service (--limit); all retained logs (--all), the raw job-error ring (--raw) and stopped-service reads come from the local files

**Behavior and developer use:** Exports the finite retained service/raw error rings. A larger export cannot reconstruct logs already evicted; treat paths and diagnostics as private when sharing. `--output-file` captures raw retained diagnostics into a new local file.

```text
Usage: airkctl service logs [OPTIONS]
```

**Implementation:** [logs.rs](../crates/airkctl/src/logs.rs). Full options: [parser reference](cli-reference.md#airkctl-service-logs).

## airkctl service prepare-configuration

**Purpose:** Prepare a guarded configuration request for a project; submit it with `requests retry`

**Behavior and developer use:** Prepares a guarded durable configuration request; submission is a separate requests retry step using the saved nonce.

```text
Usage: airkctl service prepare-configuration [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>
```

**Implementation:** [registration.rs](../crates/airkctl/src/registration.rs). Full options: [parser reference](cli-reference.md#airkctl-service-prepare-configuration).

## airkctl service prepare-registration

**Purpose:** Print the canonical registration for --root/--data-dir/--collection, including its binding. Read-only

**Behavior and developer use:** Computes canonical source/data/collection identity read-only. Review the binding before reserving or creating storage.

```text
Usage: airkctl service prepare-registration [OPTIONS]
```

**Implementation:** [registration.rs](../crates/airkctl/src/registration.rs). Full options: [parser reference](cli-reference.md#airkctl-service-prepare-registration).

## airkctl service prepare-storage

**Purpose:** Create the reserved index storage for an already reserved project (owner-only on Unix)

**Behavior and developer use:** Creates reserved index storage only for an already reserved project, with private ownership on Unix. Never use a source path as index storage.

```text
Usage: airkctl service prepare-storage [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>
```

**Implementation:** [registration.rs](../crates/airkctl/src/registration.rs). Full options: [parser reference](cli-reference.md#airkctl-service-prepare-storage).

## airkctl service record-error

**Purpose:** Record a bounded client error (JSON on stdin) without needing the service to run

**Behavior and developer use:** The running service records it; while it is down, the verified `airk` records it through its private record-error role. Keep source contents and credentials out of error messages.

```text
Usage: airkctl service record-error [OPTIONS] --operation <OPERATION>
```

**Implementation:** [record.rs](../crates/airkctl/src/record.rs). Full options: [parser reference](cli-reference.md#airkctl-service-record-error).

## airkctl service register

**Purpose:** Register --root/--data-dir/--collection with the service at --runtime-dir. The data directory must exist

**Behavior and developer use:** Registers the canonical binding with the explicit service; data storage must already exist. init combines the normal installed registration/storage workflow.

```text
Usage: airkctl service register [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [registration.rs](../crates/airkctl/src/registration.rs). Full options: [parser reference](cli-reference.md#airkctl-service-register).

## airkctl service requests

**Purpose:** Prepare, list, retry or forget durable client requests (jobs and configuration) for the selected control directory

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airkctl service requests [OPTIONS] <COMMAND>
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-service-requests).

## airkctl service requests forget

**Purpose:** Discard a saved request record by its nonce

**Behavior and developer use:** Forgets local request bookkeeping. It does not cancel work the service already accepted.

```text
Usage: airkctl service requests forget [OPTIONS] <NONCE>
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-service-requests-forget).

## airkctl service requests list

**Purpose:** List saved client requests and their outcomes

**Behavior and developer use:** Targets the selected control directory and native metadata/request protocol. Preserve service, project and durable request identities; management responses are not indexed evidence.

```text
Usage: airkctl service requests list [OPTIONS]
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-service-requests-list).

## airkctl service requests prepare

**Purpose:** Save a job request (--kind index, inspect, reconcile, vectors, model_setup, verify, recover or maintain)

**Behavior and developer use:** Saves a job intent without submitting it. Keep the returned nonce to reconcile acceptance with the same durable identity.

```text
Usage: airkctl service requests prepare [OPTIONS] --project <PROJECT>
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-service-requests-prepare).

## airkctl service requests prepare-configuration

**Purpose:** Save a configuration request for a project from a draft --file, without submitting it

**Behavior and developer use:** Saves the complete guarded configuration intent without submitting; a later retry must use that exact saved request.

```text
Usage: airkctl service requests prepare-configuration [OPTIONS] --project <PROJECT> --file <FILE>
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-service-requests-prepare-configuration).

## airkctl service requests retry

**Purpose:** Submit, or resubmit after an unknown outcome, a saved request by its nonce

**Behavior and developer use:** Submits/reconciles the same saved nonce after an unknown outcome. This idempotent management contract does not authorize replay of arbitrary MCP evidence plans.

```text
Usage: airkctl service requests retry [OPTIONS] <NONCE>
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-service-requests-retry).

## airkctl service restart

**Purpose:** Stop the service, wait until its process exits, then start the installed service again.

**Behavior and developer use:** Replaces the restart request of `runtime lifecycle`. --runtime-dir defaults to --service-runtime-dir, else the installation's control directory.

```text
Usage: airkctl service restart [OPTIONS]
```

**Implementation:** [lifecycle.rs](../crates/airkctl/src/lifecycle.rs). Full options: [parser reference](cli-reference.md#airkctl-service-restart).

## airkctl service rpc

**Purpose:** Send one raw management request (ControlMethod JSON from --request-file or --stdin) and print the service's answer

**Behavior and developer use:** Low-level bounded management JSON escape hatch. Prefer typed higher-level commands and retain instance/request guards when using it. Evidence credentials and private setup methods are never relayed; transport success and the service's own status are reported separately.

In 1.8.5, `resources` and `configure_resources` expose the same concurrent evidence setting as Management → Service: **4, 8, 16 (default), 32 or 64**. Set `configuration.query_concurrency` while preserving the other resource fields. See the [PowerShell read/update example and propagation contract](service-resources.md#cli-access). There is no dedicated concurrency flag.

```text
Usage: airkctl service rpc [OPTIONS]
```

**Implementation:** [service.rs](../crates/airkctl/src/service.rs). Full options: [parser reference](cli-reference.md#airkctl-service-rpc).

## airkctl service shutdown

**Purpose:** Ask the service for --runtime-dir (default: --service-runtime-dir) to drain its workers and exit

**Behavior and developer use:** Requests drain of the selected service and workers, affecting all its projects. Project pause has narrower scope.

```text
Usage: airkctl service shutdown [OPTIONS]
```

**Implementation:** [service.rs](../crates/airkctl/src/service.rs). Full options: [parser reference](cli-reference.md#airkctl-service-shutdown).

## airkctl service start

**Purpose:** Start the installed service (the install record's verified airk) and wait until it is healthy.

**Behavior and developer use:** An explicit start clears a saved Stop. --runtime-dir defaults to --service-runtime-dir, else the installation's control directory.

```text
Usage: airkctl service start [OPTIONS]
```

**Implementation:** [start.rs](../crates/airkctl/src/start.rs). Full options: [parser reference](cli-reference.md#airkctl-service-start).

## airkctl service status

**Purpose:** Report the installation and whether the service is running, with its saved Stop and any unfinished stop or restart.

**Behavior and developer use:** Replaces `runtime inspect` and `runtime diagnose`; for a service that will not start, read its logs with `service logs`. --runtime-dir defaults to --service-runtime-dir, else the installation's control directory.

```text
Usage: airkctl service status [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-service-status).

## airkctl service stop

**Purpose:** Stop the service and wait until its process exits; a saved Stop keeps automatic starts off.

**Behavior and developer use:** Replaces the stop request of `runtime lifecycle`. It affects every project the service hosts. --runtime-dir defaults to --service-runtime-dir, else the installation's control directory.

```text
Usage: airkctl service stop [OPTIONS]
```

**Implementation:** [lifecycle.rs](../crates/airkctl/src/lifecycle.rs). Full options: [parser reference](cli-reference.md#airkctl-service-stop).

## airkctl service trust

**Purpose:** This installation's HTTPS issuer in your OS trust store; works while the service is down.

**Behavior and developer use:** `inspect` prints the reviewed issuer digest; `install` and `remove` need it as --expected-ca-sha256, and the OS may ask you to confirm. Replaces `runtime http-trust`.

```text
Usage: airkctl service trust [OPTIONS] <ACTION>
```

**Implementation:** [trust.rs](../crates/airkctl/src/trust.rs). Full options: [parser reference](cli-reference.md#airkctl-service-trust).

## airkctl session

**Purpose:** Serve one editor window's framed session (protocol 2) on stdin/stdout.

**Behavior and developer use:** The per-window helper the VS Code extension starts; not for interactive use. Frames are length-prefixed JSON; `hello` comes first, requests in flight are bounded, and an oversized reply is refused rather than truncated. `--label` names the window for agents; `--owner-watch` exits when the parent process exits.

```text
Usage: airkctl session [OPTIONS]
```

**Implementation:** [session.rs](../crates/airkctl/src/cli/session.rs). **Tests:** [session/tests.rs](../crates/airkctl/src/cli/session/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-session).

## airkctl settings

**Purpose:** Read one project setting view; read-only.

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airkctl settings [OPTIONS] <VIEW>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-settings).

## airkctl setup-model

**Purpose:** Prepare the bundled offline embedding model for vector search; keeps an existing or custom selection

**Behavior and developer use:** Prepares the bundled offline embedding model through native job ownership and preserves an existing/custom selection. It is not a cloud model download or an evidence query.

```text
Usage: airkctl setup-model [OPTIONS]
```

**Implementation:** [journal.rs](../crates/airkctl/src/journal.rs). Full options: [parser reference](cli-reference.md#airkctl-setup-model).

## airkctl status

A completed status query that overlaps ordinary scalar-cache invalidation returns its original snapshot successfully with `status_observation.superseded: true`, `reason: scalar_invalidation` and an explanatory message. Original publication and observation age are retained. The superseded snapshot does not repopulate the current cache or acknowledge an obsolete status receipt; the query is not replayed. Project/namespace changes, authenticated worker replacement, expired source authorization and shutdown retain their existing rejection behavior. Cached reuse has its five-second limit; configured evidence capacity has one additional reserved status slot.

**Purpose:** Print the project's publication, freshness, coverage and job status

**Behavior and developer use:** Reads project publication/coverage and job observations. It differs from lightweight MCP status, whose scope excludes detailed job and process probes.

```text
Usage: airkctl status [OPTIONS]
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-status).

## airkctl symbol

**Purpose:** Find definitions by exact name by default; use --name-match for suffix/contains variants.

**Behavior and developer use:** Start compact, scope with --language/--path-prefix, and use --limit/--max-bytes before requesting rich detail. --name-match suffix|contains compares simple or qualified names case-sensitively; omitted mode keeps exact lookup. --include-count reports qualified count precision; bounded partial pages do not claim a full total. Matching and representation bind the cursor.

```text
Usage: airkctl symbol [OPTIONS] <NAME>
```

**Implementation:** [evidence.rs](../crates/airkctl/src/evidence.rs). Full options: [parser reference](cli-reference.md#airkctl-symbol).

## airkctl tools

**Purpose:** List, describe or call unified evidence and Explorer proposal tools.

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airkctl tools [OPTIONS] <COMMAND>
```

**Implementation:** [tools.rs](../crates/airkctl/src/tools.rs). **Tests:** [tools/tests.rs](../crates/airkctl/src/tools/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-tools).

## airkctl tools call

**Purpose:** Invoke an MCP tool and print its unchanged result envelope as JSON

**Behavior and developer use:** Accepts one JSON object through --args or --args-file (maximum 64 KiB) and returns the unchanged unified tool envelope. Use public names such as `search`; historical native aliases such as `airk_search` remain accepted. Proposal tools retain their connection and user-review requirements and do not execute proposed work.

```text
Usage: airkctl tools call [OPTIONS] <TOOL>
```

**Implementation:** [tools.rs](../crates/airkctl/src/tools.rs). **Tests:** [tools/tests.rs](../crates/airkctl/src/tools/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-tools-call).

## airkctl tools help

**Purpose:** Show a tool's description, input schema, defaults and argument template.

**Behavior and developer use:** The catalog is served by the running service; without one the command fails with `service_unavailable`. Use it to obtain the exact schema before constructing tools call arguments.

```text
Usage: airkctl tools help [OPTIONS] <TOOL>
```

**Implementation:** [tools.rs](../crates/airkctl/src/tools.rs). **Tests:** [tools/tests.rs](../crates/airkctl/src/tools/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-tools-help).

## airkctl tools list

**Purpose:** List the connection's unified tools; `--json` includes complete schemas and annotations.

**Behavior and developer use:** Served by the running service. Lists the same 29 public definitions as MCP for the selected session, including combined selector and discovery inputs when configured. The 19 native operations are an internal subset. Assistance tools are in this unified catalog; separate `airkctl explorer` commands retain their documented offline-file or trusted-window workflows.

```text
Usage: airkctl tools list [OPTIONS]
```

**Implementation:** [tools.rs](../crates/airkctl/src/tools.rs). **Tests:** [tools/tests.rs](../crates/airkctl/src/tools/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-tools-list).

## airkctl tools shell

**Purpose:** Open a connection-bound interactive console with help and examples

**Behavior and developer use:** Interactive console using the selected connection's unified tool catalog and authority, with help/examples. Its session lifetime and prompts differ from one-shot JSON commands.

```text
Usage: airkctl tools shell [OPTIONS]
```

**Implementation:** [shell.rs](../crates/airkctl/src/cli/shell.rs). Full options: [parser reference](cli-reference.md#airkctl-tools-shell).

## airkctl usage

**Purpose:** Local usage reporting for the selected project: report, policy, configure (mode), clear, feedback, import, calibrate

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airkctl usage [OPTIONS] <COMMAND>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-usage).

## airkctl usage calibrate

**Purpose:** Calibrate token estimates against a tokenizer and baseline/returned samples (advanced)

**Behavior and developer use:** Compares local tokenizer and sample estimates. It does not contact a provider or prove actual billed savings.

```text
Usage: airkctl usage calibrate [OPTIONS] --tokenizer <TOKENIZER> --baseline <BASELINE> --returned <RETURNED> --generation <GENERATION> --content-kind <CONTENT_KIND>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-usage-calibrate).

## airkctl usage clear

**Purpose:** Delete recorded usage for the project

**Behavior and developer use:** Deletes this project's recorded usage; source/index evidence remains separate.

```text
Usage: airkctl usage clear [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-usage-clear).

## airkctl usage configure

**Purpose:** Set the recording mode: disabled, aggregates_only or diagnostic_events

**Behavior and developer use:** Changes local recording mode among disabled, aggregates_only and diagnostic_events. Review privacy and retention before enabling detailed events.

```text
Usage: airkctl usage configure [OPTIONS] --mode <MODE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-usage-configure).

## airkctl usage feedback

**Purpose:** Record whether one returned request was useful, partly useful or not useful

**Behavior and developer use:** Uses the selected project's native local usage/settings adapter. Usage measurements and retention are independent of index readiness and never authorize source changes.

```text
Usage: airkctl usage feedback [OPTIONS] --usefulness <USEFULNESS> <REQUEST_ID>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-usage-feedback).

## airkctl usage import

**Purpose:** Import a usage export; --preview shows what would change and --if-sha256 pins the file

**Behavior and developer use:** Preview first and pin the reviewed bytes with --if-sha256 when importing. Import mutates local usage records, not evidence or source files.

```text
Usage: airkctl usage import [OPTIONS] <FILE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-usage-import).

## airkctl usage policy

**Purpose:** Show the project's usage recording mode and retention

**Behavior and developer use:** Uses the selected project's native local usage/settings adapter. Usage measurements and retention are independent of index readiness and never authorize source changes.

```text
Usage: airkctl usage policy [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-usage-policy).

## airkctl usage report

**Purpose:** Summarize recorded usage for the last --days (json, csv or markdown; optional price scenario file)

**Behavior and developer use:** Reports recorded estimates with optional price scenarios. Estimates are not provider invoices or measured money saved.

```text
Usage: airkctl usage report [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-usage-report).

## airkctl version

**Purpose:** Print the version document: airkctl, its contracts and the installation.

**Behavior and developer use:** Answers locally in text or JSON (`--output json`); it does not need the service.

```text
Usage: airkctl version [OPTIONS]
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). **Tests:** [cli/tests.rs](../crates/airkctl/src/cli/tests.rs). Full options: [parser reference](cli-reference.md#airkctl-version).

## airkctl watch

**Purpose:** Turn native source watching on or off for the project (on keeps the index current after edits), or print its watch status.

**Behavior and developer use:** `on`/`off` change native source watching for this project. On keeps watching independently of editor lifetime; it does not register OS login startup. `status` prints the service's `Watch` observation unchanged.

```text
Usage: airkctl watch [OPTIONS] <MODE>
```

**Implementation:** [cli.rs](../crates/airkctl/src/cli.rs). Full options: [parser reference](cli-reference.md#airkctl-watch).

## The `airk` executable

`airk` is the service only. With no arguments it starts the installed service detached, through the `airkctl` installed beside it, or reports that the service is already running. With any user-style arguments it says that `airk` takes no commands and that `airkctl` is the CLI, reports the running state and starts nothing.

`airk serve --runtime-dir` became the private role `airk __airk_role service-foreground --runtime-dir <dir>`, for development, tests and external supervisors; it is not a user interface. As an interim state (U93, not shippable until Phase 7), the pre-split `runtime …`, `setup`, `uninstall`, `host` and `service legacy …` operations are reachable only as `airk __airk_role setup-interim <arguments>` until the Phase 7 setup role and installer replace them. Retired: `mcp`, `bridge`, `service session`, `service console`, `explorer-assistance`, `runtime launch` and the universal launcher.

Renamed in `airkctl`: `runtime http-*` became `agents …` and `service trust`/`service issuer-rotation`; `setup` enrollment became `agents connect`; `runtime lifecycle` became `service stop`/`service restart`; `runtime inspect` and `runtime diagnose` became `service status`.

## Changing a command safely

Read the parser/dispatcher in [cli.rs](../crates/airkctl/src/cli.rs) and the owning module before editing; the links above are entrypoints, not a claim that every behavior lives in one file. `airkctl` reaches the service over its local control endpoint; the evidence reads are generated into [evidence.rs](../crates/airkctl/src/evidence.rs) by a committed port script, so edit the script's inputs rather than that file. Evidence requests reach the shared [MCP native handlers](../crates/airk-mcp/src/lib.rs), Kernel queries and Store inside the service. Unit tests sit beside the airkctl modules (for example [cli/tests.rs](../crates/airkctl/src/cli/tests.rs)); process tests that drive `airkctl` against a foreground service are under [crates/airk-cli/tests](../crates/airk-cli/tests). The CLI does not open project state or move service ownership into the editor, and Kernel/Store do not depend on UI adapters.

For help changes, regenerate docs/cli-reference.md with the existing parser-reference test workflow and review its diff. Keep this guide, the [MCP developer guide](mcp-developer-guide.md), public schemas, [CLI workflows](cli.md), release notes and README links consistent. Run the relevant command/contract tests and repository checks; link evidence for the exact platform tested and report unavailable qualification honestly.

The [CLI, MCP and Workbench parity audit](read-interface-parity.md) maps all read operations and records complete-data, observation, budget and export contracts for 1.8.7.
