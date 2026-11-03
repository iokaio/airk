# CLI developer guide

Current command behavior for the **AIRK 1.9.0 development candidate**, inheriting the 1.8.7 read contracts. This guide explains why each public command exists, how it behaves and where developers should follow its implementation. The generated [CLI reference](cli-reference.md) remains authoritative for every flag, default and complete parser help; [CLI workflows](cli.md) and [headless feature mapping](headless-cli.md) provide task-oriented walkthroughs. Current candidate qualification is tracked in the [1.9.0 record](releases/release-1.9.0.md); [1.8.7 evidence](releases/release-1.8.7.md) remains dated history.

**Related developer reference: [MCP developer guide](mcp-developer-guide.md)** explains every unified MCP tool, inputs, examples, limitations and implementation/test ownership.

## Why 18 native tools and 28 MCP tools?

These counts describe different catalogs, not the size of the CLI. `airk tools list` exposes **18 prefixed native tools** for retrieval, parsing, readiness/diagnostics and help. The unified agent MCP endpoint composes those operations with **projects plus nine Explorer Assistance operations**, yielding **28 unprefixed tools**. The full terminal CLI additionally has project, indexing, configuration, service, runtime and usage commands.

| Unified MCP capability | Terminal entrypoint | Scope or workflow difference |
|---|---|---|
| 18 native operations | `airk tools list/help/call/shell`, e.g. `airk tools call airk_search` | Prefixed native names; direct commands such as `airk search` provide convenient flags. |
| `projects` | `airk projects` | MCP lists connection-configured bindings; CLI lists the selected service registry. |
| `list_templates`, `get_template` | `airk explorer list-templates/get-template` | CLI writes results to an explicit output file. |
| `draft_search`, `draft_strategy`, `draft_plan`, `suggest_view`, `validate_draft` | Corresponding hyphenated `airk explorer` commands | Ordinary CLI proposals are offline files; MCP can propose to an attached trusted Explorer for user review. Neither executes the proposed work. |
| `proposal_status` | `airk explorer proposal-status` | MCP reads an admitted proposal receipt; CLI reads a CLI-produced draft file. |
| `get_handoff` | `airk explorer get-handoff` | Requires a user-created expiring share token from a trusted window. |

The public parser reference has 120 command sections including root and command groups. This guide also includes the explicitly implemented `airk tools help` command, which the reference walker omits as a help-named child. These are documentation entry counts, not 121 independent evidence tools.

## Developer contracts and examples

In 1.9.0, the 15 native read aliases expose `--reply-detail lean|rich`.
Tools-call/shell use the native `reply_detail` JSON field. The selection is
independent of retained indexing detail; omitted arguments preserve legacy
defaults. See [reply detail](reply-detail.md) and the regenerated parser reference
for supported flags, conflicts, parse defaults and Workbench equivalents.

Equivalent CLI, MCP and Workbench reads share the canonical handlers and retain complete result data. `tools call` emits the MCP envelope; `tools shell` pretty-prints its text payload; dedicated aliases build arguments with their documented defaults. Direct status/diagnostics report `query_concurrency` at the result's top level; executed parse plans retain it inside each completed status/diagnostics step. [The parity audit](read-interface-parity.md) records all 17 read mappings, supported inputs and the actual-process regressions. Separate requests retain their own request IDs, observation times and cache ages.

- Select the canonical source root, separate index data directory and collection/binding. A UI dropdown does not redirect a native query. Keep index/control/log storage outside source roots.
- Published evidence is an indexed snapshot. Preserve publication tokens, hashes, validators, original locations, continuation filters and byte budgets. Verify live files separately before editing.
- Ordinary queries print JSON; failures exit nonzero with diagnostics on stderr (`AIRK_ERROR_FORMAT=json` requests structured errors). Help/text/CSV/Markdown output, interactive tools shell, MCP stdio, persistent service console and foreground owners have their documented output/lifetime contracts.
- Source files remain read-only. Management commands can change owned runtime, registry, settings, jobs, indexes and logs. Even evidence connections can prepare the installed service and maintain caches/accounting metadata.
- Honor exact typed retry hints only for the eligible read. Unknown submission outcomes require the saved durable request/nonce contract; do not submit a new operation identity to guess whether a mutation succeeded.
- A status setup deadline retains its explicit expiry for later polls after accepted setup work drains. Keep `status_timeout` stage `setup` as unknown readiness with null evidence; generic cancellation text and genuine verification failures are not retry receipts. See the [MCP status contract](mcp.md).
- The invocation shapes below come from parser help. Angle-bracket arguments are placeholders, not runnable approved tokens. Obtain guards/digests from the appropriate reviewed preview; do not fabricate them.

For an already registered project, run these separately, supplying the same root/data/collection selection when needed:

```console
airk status
airk files --prefix crates/
airk search "publication" --mode lexical
airk parse "Find the definition of Owner"
airk tools list --json
airk tools help airk_search --json
```

parse prepares by default; inspect its plan before explicitly using --execute. Queue indexing separately with airk index, then observe its receipt/status until readiness is reported. For a guarded management change, prepare/preview â†’ review exact identities and bytes â†’ submit/confirm once â†’ reconcile the same saved receipt.

## Identifier investigations and bounded discovery

For a known type or function, start compact with `airk symbol`, inspect exact textual uses with scoped `airk refs`, and expand one chosen definition using `airk read` or `airk context`. Use `airk outline` for one unfamiliar file's structure in source order. It returns evidence; the similarly named Explorer navigation route is a UI suggestion. `airk search` complements these lookups for concepts, documentation and code passages.

```console
airk symbol Session --name-match suffix --path-prefix crates/ --limit 30 --include-count
airk code-facts --name Owner --path-prefix crates/ --compact false
airk refs Owner --language rust --order path --include-files --include-count
airk search "ownership policy" --mode hybrid --evidence-kind document
```

These examples are invocation shapes, not claims about this repository's result counts. Search and refs accept exact indexed `--language`, `--evidence-kind code|document|configuration|unknown`, and a literal `--exclude-path-prefix`. Classification follows saved extraction metadata; mixed or unclassified extraction remains unknown. These query filters neither change saved indexing nor bypass scope authorization. Both search branches filter before top-k selection, with independent bounded candidate-discovery coverage.

Symbol/code-facts matching stays exact when `--name-match` is omitted. Explicit suffix/contains compares simple or qualified names case-sensitively; opaque symbol IDs are not substring targets. `--include-count` requests qualified count metadata. Exact totals appear only when bounded work establishes them; otherwise inspect lower bound, completeness and count scope. Durable `--symbol-key` lookup excludes these matching/count options.

References retain the preceding selection/response contract unless a new filter, cursor, order or summary/count option selects the paged contract. Paged `index` order is deterministic stable-ID order. Use the returned cursor with unchanged identifier, filters, ordering and representation; count/text budgets may increase within supported limits when a complete record needs more space. `--include-files` summarizes returned-page records in the native `count_unit`, not every matching file or every identifier occurrence. Summary metadata is additional to the text budget and capped at 16 rows/4 KiB. UTF-8 matches reconstruct complete original lines; extracted non-UTF-8 fragments remain explicitly qualified. A cursor cannot silently advance to a different publication. `--order code_first` is a presentation choice, not compiler-resolved impact analysis.

Reference totals remain unknown for the whole query. A complete bounded scan can count only `verified_lexical_candidates`; partial count scope is `page_scan`. `page.coverage` states that token-boundary coverage is incomplete: an identifier split across indexed chunk tokenization may be missing from postings, even when zero candidates match. Candidate exhaustion is not proof of no uses. Use live verification or additional scoped source reads when completeness matters, and inspect line/fragment units and terminal coverage limits before interpreting counts.

For a relationship investigation, a returned type/impl ID may provide bounded associated-method targets when retained owner/scope evidence supports them. Follow written targets such as `Owner::new` for incoming call candidates; a struct with no incoming edges does not prove no uses. Duplicate names and incomplete extraction remain qualified. Keep exact publication tokens, original locations and citations when copying CLI output; terse receipt suppression is not a replacement for that evidence identity.

## Command index

- [airk](#airk)
- [airk bridge](#airk-bridge)
- [airk changes](#airk-changes)
- [airk code-facts](#airk-code-facts)
- [airk configure](#airk-configure)
- [airk context](#airk-context)
- [airk explorer](#airk-explorer)
- [airk explorer draft-plan](#airk-explorer-draft-plan)
- [airk explorer draft-search](#airk-explorer-draft-search)
- [airk explorer draft-strategy](#airk-explorer-draft-strategy)
- [airk explorer get-handoff](#airk-explorer-get-handoff)
- [airk explorer get-template](#airk-explorer-get-template)
- [airk explorer list-templates](#airk-explorer-list-templates)
- [airk explorer proposal-status](#airk-explorer-proposal-status)
- [airk explorer suggest-view](#airk-explorer-suggest-view)
- [airk explorer validate-draft](#airk-explorer-validate-draft)
- [airk facts](#airk-facts)
- [airk files](#airk-files)
- [airk host](#airk-host)
- [airk imports](#airk-imports)
- [airk index](#airk-index)
- [airk info](#airk-info)
- [airk init](#airk-init)
- [airk inspect](#airk-inspect)
- [airk mcp](#airk-mcp)
- [airk model-catalog](#airk-model-catalog)
- [airk outline](#airk-outline)
- [airk parse](#airk-parse)
- [airk pause](#airk-pause)
- [airk projects](#airk-projects)
- [airk read](#airk-read)
- [airk refs](#airk-refs)
- [airk relationships](#airk-relationships)
- [airk resume](#airk-resume)
- [airk runtime](#airk-runtime)
- [airk runtime abandon-lifecycle](#airk-runtime-abandon-lifecycle)
- [airk runtime abort-activation](#airk-runtime-abort-activation)
- [airk runtime abort-launcher-repair](#airk-runtime-abort-launcher-repair)
- [airk runtime abort-layout](#airk-runtime-abort-layout)
- [airk runtime abort-stage](#airk-runtime-abort-stage)
- [airk runtime activate](#airk-runtime-activate)
- [airk runtime container-host](#airk-runtime-container-host)
- [airk runtime diagnose](#airk-runtime-diagnose)
- [airk runtime ensure](#airk-runtime-ensure)
- [airk runtime foreground](#airk-runtime-foreground)
- [airk runtime inspect](#airk-runtime-inspect)
- [airk runtime inspect-candidate](#airk-runtime-inspect-candidate)
- [airk runtime inventory](#airk-runtime-inventory)
- [airk runtime launch](#airk-runtime-launch)
- [airk runtime lifecycle](#airk-runtime-lifecycle)
- [airk runtime location](#airk-runtime-location)
- [airk runtime login](#airk-runtime-login)
- [airk runtime login install](#airk-runtime-login-install)
- [airk runtime login plan](#airk-runtime-login-plan)
- [airk runtime login start](#airk-runtime-login-start)
- [airk runtime login status](#airk-runtime-login-status)
- [airk runtime login stop](#airk-runtime-login-stop)
- [airk runtime login uninstall](#airk-runtime-login-uninstall)
- [airk runtime managed](#airk-runtime-managed)
- [airk runtime prepare-client](#airk-runtime-prepare-client)
- [airk runtime purge](#airk-runtime-purge)
- [airk runtime remove](#airk-runtime-remove)
- [airk runtime repair-launcher](#airk-runtime-repair-launcher)
- [airk runtime repair-payload](#airk-runtime-repair-payload)
- [airk runtime reset-startup](#airk-runtime-reset-startup)
- [airk runtime retire-previous](#airk-runtime-retire-previous)
- [airk runtime rollback](#airk-runtime-rollback)
- [airk runtime self-test](#airk-runtime-self-test)
- [airk runtime snapshot](#airk-runtime-snapshot)
- [airk runtime source-scope](#airk-runtime-source-scope)
- [airk runtime stage](#airk-runtime-stage)
- [airk runtime start](#airk-runtime-start)
- [airk runtime uninstall](#airk-runtime-uninstall)
- [airk search](#airk-search)
- [airk serve](#airk-serve)
- [airk service](#airk-service)
- [airk service console](#airk-service-console)
- [airk service health](#airk-service-health)
- [airk service legacy](#airk-service-legacy)
- [airk service legacy derived-receipts](#airk-service-legacy-derived-receipts)
- [airk service legacy forget](#airk-service-legacy-forget)
- [airk service legacy forget-derived](#airk-service-legacy-forget-derived)
- [airk service legacy list](#airk-service-legacy-list)
- [airk service legacy preview-derived](#airk-service-legacy-preview-derived)
- [airk service legacy remove-derived](#airk-service-legacy-remove-derived)
- [airk service legacy remove-startup](#airk-service-legacy-remove-startup)
- [airk service legacy review](#airk-service-legacy-review)
- [airk service legacy show](#airk-service-legacy-show)
- [airk service logs](#airk-service-logs)
- [airk service prepare-configuration](#airk-service-prepare-configuration)
- [airk service prepare-registration](#airk-service-prepare-registration)
- [airk service prepare-storage](#airk-service-prepare-storage)
- [airk service record-error](#airk-service-record-error)
- [airk service register](#airk-service-register)
- [airk service requests](#airk-service-requests)
- [airk service requests forget](#airk-service-requests-forget)
- [airk service requests list](#airk-service-requests-list)
- [airk service requests prepare](#airk-service-requests-prepare)
- [airk service requests prepare-configuration](#airk-service-requests-prepare-configuration)
- [airk service requests retry](#airk-service-requests-retry)
- [airk service rpc](#airk-service-rpc)
- [airk service shutdown](#airk-service-shutdown)
- [airk settings](#airk-settings)
- [airk setup](#airk-setup)
- [airk setup-model](#airk-setup-model)
- [airk status](#airk-status)
- [airk symbol](#airk-symbol)
- [airk tools](#airk-tools)
- [airk tools call](#airk-tools-call)
- [airk tools help](#airk-tools-help)
- [airk tools list](#airk-tools-list)
- [airk tools shell](#airk-tools-shell)
- [airk usage](#airk-usage)
- [airk usage calibrate](#airk-usage-calibrate)
- [airk usage clear](#airk-usage-clear)
- [airk usage configure](#airk-usage-configure)
- [airk usage feedback](#airk-usage-feedback)
- [airk usage import](#airk-usage-import)
- [airk usage policy](#airk-usage-policy)
- [airk usage report](#airk-usage-report)
- [airk watch](#airk-watch)

## airk

**Purpose:** Artifact Indexing and Retrieval Kernel: local, read-only code and document retrieval through a native service. Run `airk --help` (not -h) for the full guide.

**Behavior and developer use:** Choose a subcommand. Use --help for the full built-in guide; -h is the shorter synopsis. Root options select the project and transport rather than running a query themselves.

```text
Usage: airk [OPTIONS] <COMMAND>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk).

## airk bridge

**Purpose:** Internal read-only adapter used by trusted launchers

**Behavior and developer use:** Trusted launcher adapter, not the general agent endpoint. Its explicit control directory and project keep the connection bound to the reviewed native scope.

```text
Usage: airk bridge [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-bridge).

## airk changes

**Purpose:** Report indexed changes since a revision token: added, modified and removed files, symbols and documents

**Behavior and developer use:** Consume all continuation pages before advancing the retained revision baseline. index-current describes the publication, not a live-file check; unavailable retained history is an explicit gap.

```text
Usage: airk changes [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-changes).

## airk code-facts

**Purpose:** Inspect code facts and signatures with exact default name matching and bounded continuation.

**Behavior and developer use:** Use after compact discovery when signatures, scopes, IDs or call candidates matter. --name-match exact|suffix|contains and --include-count share the qualified discovery contract; every explicit matching mode, including exact, needs --name. --symbol-key retains exact identity and excludes textual matching/count flags. Compactness, byte budget and cursor remain independent of response mode.

```text
Usage: airk code-facts [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-code-facts).

## airk configure

**Purpose:** Apply a guarded project configuration draft from --file or --stdin (JSON with an "operation").

**Behavior and developer use:** Reads a guarded JSON operation and applies it through native configuration ownership. Review the draft and its current guards; do not manufacture digests or replay uncertain changes with a new identity.

```text
Usage: airk configure [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-configure).

## airk context

**Purpose:** Read lines around --line in a file (--before/--after), with a citation

**Behavior and developer use:** Use a known indexed path and line with bounded surrounding lines; it avoids retrieving the whole file and retains source citations.

```text
Usage: airk context [OPTIONS] <PATH> <LINE>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-context).

## airk explorer

**Purpose:** Explorer Assistance tools without an editor; every result is written to --output.

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airk explorer [OPTIONS] <COMMAND>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer).

## airk explorer draft-plan

**Purpose:** Write proposed Planner tasks (MCP draft_plan) after schema validation

**Behavior and developer use:** Validates the bundled Assistance schema and writes a fresh --output file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airk explorer draft-plan [OPTIONS] --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-draft-plan).

## airk explorer draft-search

**Purpose:** Write a proposed Explorer Search form (MCP draft_search) after schema validation

**Behavior and developer use:** Validates the bundled Assistance schema and writes a fresh --output file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airk explorer draft-search [OPTIONS] --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-draft-search).

## airk explorer draft-strategy

**Purpose:** Write a proposed Studio strategy (MCP draft_strategy) after schema validation

**Behavior and developer use:** Validates the bundled Assistance schema and writes a fresh --output file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airk explorer draft-strategy [OPTIONS] --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-draft-strategy).

## airk explorer get-handoff

**Purpose:** Read a handoff the user shared from a running Explorer window (MCP get_handoff).

**Behavior and developer use:** Reads only a user-explicit shared token from a trusted running window, then writes the chosen output file. Tokens expire and are not authorization to read arbitrary editor state.

```text
Usage: airk explorer get-handoff [OPTIONS] --token <TOKEN> --explorer-descriptor <EXPLORER_DESCRIPTOR> --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-get-handoff).

## airk explorer get-template

**Purpose:** Read one strategy template with its operators (MCP get_template; --args {"template_id":â€¦})

**Behavior and developer use:** Validates the bundled Assistance schema and writes a fresh --output file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airk explorer get-template [OPTIONS] --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-get-template).

## airk explorer list-templates

**Purpose:** List the bundled strategy templates (MCP list_templates; optional offset, limit, max_bytes)

**Behavior and developer use:** Validates the bundled Assistance schema and writes a fresh --output file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airk explorer list-templates [OPTIONS] --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-list-templates).

## airk explorer proposal-status

**Purpose:** Report the status of a draft file written by this CLI (it is never admitted to an Inbox)

**Behavior and developer use:** Inspects a CLI draft file, never a live Suggestions Inbox receipt. It cannot establish that a user applied or executed a proposal.

```text
Usage: airk explorer proposal-status [OPTIONS] --draft <DRAFT> --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-proposal-status).

## airk explorer suggest-view

**Purpose:** Write a Navigation, Review or Management suggestion (MCP suggest_view; --args {"kind":â€¦,"draft":â€¦})

**Behavior and developer use:** Validates the bundled Assistance schema and writes a fresh --output file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airk explorer suggest-view [OPTIONS] --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-suggest-view).

## airk explorer validate-draft

**Purpose:** Validate a draft without writing it as a proposal (MCP validate_draft; --args {"kind":â€¦,"draft":â€¦})

**Behavior and developer use:** Validates the bundled Assistance schema and writes a fresh --output file. Ordinary offline drafts do not enter a live Explorer Inbox or execute a search, strategy or task; validation errors still produce a diagnostic result and a nonzero exit.

```text
Usage: airk explorer validate-draft [OPTIONS] --output <OUTPUT>
```

**Implementation:** [explorer_offline.rs](../crates/airk-mcp/src/explorer_offline.rs). **Regression/contract evidence:** [explorer_assistance.rs](../crates/airk-cli/tests/explorer_assistance.rs). Full options: [parser reference](cli-reference.md#airk-explorer-validate-draft).

## airk facts

**Purpose:** List document facts (requirements, dates, decisions, features) extracted from indexed documents

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airk facts [OPTIONS] [PATH_PREFIX]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-facts).

## airk files

**Purpose:** List indexed files under --prefix, with continuation cursors

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airk files [OPTIONS] [PREFIX]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-files).

## airk host

**Purpose:** Run the installed service under an explicit container/user supervisor

**Behavior and developer use:** Runs the installed service under explicit external supervision. The invoking supervisor owns process lifetime; it is separate from optional OS login registration.

```text
Usage: airk host [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-host).

## airk imports

**Purpose:** List import and dependency statements found under a path prefix

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airk imports [OPTIONS] [PATH_PREFIX]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-imports).

## airk index

**Purpose:** Queue indexing of the project's sources and print the job receipt. Follow it with `tools call airk_status`.

**Behavior and developer use:** Submits a durable job and returns its receipt. Acceptance is not readiness; follow status/job observations instead of assuming this call waits for the complete index.

```text
Usage: airk index [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-index).

## airk info

**Purpose:** Show what the index knows about one file: language, size, extraction method and parse status

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airk info [OPTIONS] <PATH>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-info).

## airk init

**Purpose:** Register --root as a project of the installed runtime and prepare its index storage.

**Behavior and developer use:** Use for first-time headless registration. It prepares separate owned index storage; it does not copy or modify the source tree.

```text
Usage: airk init [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-init).

## airk inspect

**Purpose:** Queue a bounded inspection job that refreshes coverage and readiness without reindexing

**Behavior and developer use:** Queues bounded coverage/readiness inspection without requesting a full source reindex. Its job receipt has the same acceptance-versus-completion distinction.

```text
Usage: airk inspect [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-inspect).

## airk mcp

**Purpose:** Serve the read-only MCP protocol on stdin/stdout for one agent connection (this is what agents launch)

**Behavior and developer use:** Long-running JSON-RPC stdio endpoint for agents. Stdout is protocol-only; stderr holds diagnostics. It composes the unified catalog rather than printing one JSON result.

```text
Usage: airk mcp [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-mcp).

## airk model-catalog

**Purpose:** Print the offline model catalog and the CPU-based model choice for this machine; opens no project state

**Behavior and developer use:** Reads bundled model metadata and CPU selection without opening a project or loading an inference session.

```text
Usage: airk model-catalog [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-model-catalog).

## airk outline

**Purpose:** Outline one file's structure: symbols, headings and sections with line ranges

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airk outline [OPTIONS] <PATH>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-outline).

## airk parse

**Purpose:** Plan, and with --execute run, a multi-step read-only evidence request.

**Behavior and developer use:** Prepare mode returns a bounded plan for review. Only --execute requests execution. Finite parsed or explicitly confirmed plans use native evidence operations; learned proposals remain review-only.

Explicit request files can also select `mode: execute`. In 1.9.0 compact symbol
discovery supports cited aggregation and dependent file/location reads through
the same consumer as MCP and Workbench. Nested plan/recipe/budget/output inputs
are objects. See [compact binding and symbol-ID lookup rules](read-interface-parity.md#regression-evidence)
before composing dependent requests. `tools call` and the tools shell retain
the same native results and validation.

```text
Usage: airk parse [OPTIONS] [TEXT]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-parse).

## airk pause

**Purpose:** Pause automatic and queued work for the project; existing evidence stays queryable

**Behavior and developer use:** Pauses project work while published evidence remains readable. It does not stop the shared service or other registered projects.

```text
Usage: airk pause [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-pause).

## airk projects

**Purpose:** List the projects registered with the service (same fields as the MCP `projects` tool).

**Behavior and developer use:** Enumerates the selected native service registry. Unified MCP projects instead lists only bindings configured on that MCP connection; do not equate their visibility.

```text
Usage: airk projects [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-projects).

## airk read

**Purpose:** Read an indexed file's lines (--start-line/--end-line) from the published revision, with a citation

**Behavior and developer use:** Returns indexed text, not current filesystem contents. Retain its publication, hash, citation and original location, then verify live bytes separately before editing.

```text
Usage: airk read [OPTIONS] --start-line <START_LINE> --end-line <END_LINE> <PATH>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-read).

## airk refs

**Purpose:** Find exact-case textual identifier uses; these are candidates, not resolved language references.

**Behavior and developer use:** Returns matches verified within lexical candidates. Optional --language/--evidence-kind/--exclude-path-prefix scopes candidates; --cursor, --order index|path|code_first, --include-files and --include-count select bounded paging and qualified metadata. Repeat identifier, filters and ordering on continuation. Summaries cover this page; exact candidate-scoped counts are not whole-query totals because token seams can hide identifiers. UTF-8 lines are reconstructed; non-UTF-8 fragments retain explicit completeness/unit qualifiers. A record that fits the supported maximum but not the current text budget reports required bytes and a resumable cursor; records over 64 KiB and terminal scan/deduplication limits instead report incomplete coverage without usable continuation. An empty page never proves no uses by itself.

```text
Usage: airk refs [OPTIONS] <NAME>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-refs).

## airk relationships

**Purpose:** Follow indexed relationships of a symbol in --direction, optionally of one --kind, up to --hops levels

**Behavior and developer use:** Traverses bounded indexed structural relationships. Direction, kind and hop limits select evidence; candidate edges do not establish fully resolved language semantics.

```text
Usage: airk relationships [OPTIONS] <SYMBOL>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-relationships).

## airk resume

**Purpose:** Resume work paused with `pause`

**Behavior and developer use:** Explicitly resumes the selected project, preserving shared-service and other-project isolation.

```text
Usage: airk resume [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-resume).

## airk runtime

**Purpose:** Installed-runtime lifecycle: location, inspect, ensure, start, login, uninstall, inventory, purge and repair.

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airk runtime [OPTIONS] <COMMAND>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime).

## airk runtime abandon-lifecycle

**Purpose:** Abandon one saved lifecycle recovery intent; preserve every running process and keep automatic starts disabled

**Behavior and developer use:** Abandons only the saved recovery intent. Running processes are preserved and automatic starts remain disabled.

```text
Usage: airk runtime abandon-lifecycle [OPTIONS] --install-dir <INSTALL_DIR> --request-id <REQUEST_ID>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-abandon-lifecycle).

## airk runtime abort-activation

**Purpose:** Restore the preceding selection after an unfinished activation; preserve runtime payloads

**Behavior and developer use:** Recovers the preceding selection for an unfinished activation. A committed selection cannot be undone by a stale abort record.

```text
Usage: airk runtime abort-activation [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-abort-activation).

## airk runtime abort-launcher-repair

**Purpose:** Abort an owned unfinished launcher copy; an already replaced image must be finalized

**Behavior and developer use:** Aborts an owned interrupted copy; if replacement already happened, finalization is required instead.

```text
Usage: airk runtime abort-launcher-repair [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-abort-launcher-repair).

## airk runtime abort-layout

**Purpose:** Abandon an unfinished state-layout claim; preserve all state directories and their contents

**Behavior and developer use:** Abandons the unfinished state-layout claim without deleting any state directory or its contents.

```text
Usage: airk runtime abort-layout [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-abort-layout).

## airk runtime abort-stage

**Purpose:** Abort an ownership-proven interrupted copy; preserve promoted runtimes and unknown files

**Behavior and developer use:** Aborts only the ownership-proven interrupted copy; promoted payloads and unknown files remain intact.

```text
Usage: airk runtime abort-stage [OPTIONS] --install-dir <INSTALL_DIR> --runtime-sha256 <RUNTIME_SHA256>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-abort-stage).

## airk runtime activate

**Purpose:** Drain the selected service and select a staged runtime only after native health succeeds

**Behavior and developer use:** Drains the observed service, fences selection and checks native health before committing activation. Preserve saved transaction receipts for interrupted recovery.

```text
Usage: airk runtime activate [OPTIONS] --install-dir <INSTALL_DIR> --runtime-sha256 <RUNTIME_SHA256>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-activate).

## airk runtime container-host

**Purpose:** Run one externally supervised Linux container service from an approved read-only image pair

**Behavior and developer use:** Dispatches through native installed-runtime ownership, selection and lifecycle guards. Use exact returned identities, receipts and confirmation digests for state changes; location/inspection commands observe rather than authorize a later mutation.

```text
Usage: airk runtime container-host [OPTIONS] --source <SOURCE> --approved-sha256 <APPROVED_SHA256>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-container-host).

## airk runtime diagnose

**Purpose:** Bounded bootstrap diagnostics work even when runtime selection or the service fails

**Behavior and developer use:** Dispatches through native installed-runtime ownership, selection and lifecycle guards. Use exact returned identities, receipts and confirmation digests for state changes; location/inspection commands observe rather than authorize a later mutation.

```text
Usage: airk runtime diagnose [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-diagnose).

## airk runtime ensure

**Purpose:** Launch the selected service on demand, without registering login startup

**Behavior and developer use:** On-demand start honors a deliberate Stop and does not install login startup. Use explicit start to resume a stopped installation.

```text
Usage: airk runtime ensure [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-ensure).

## airk runtime foreground

**Purpose:** Run the selected service under an explicit external/user startup manager

**Behavior and developer use:** Dispatches through native installed-runtime ownership, selection and lifecycle guards. Use exact returned identities, receipts and confirmation digests for state changes; location/inspection commands observe rather than authorize a later mutation.

```text
Usage: airk runtime foreground [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-foreground).

## airk runtime inspect

**Purpose:** Inspect verified selection without starting a process or opening a project

**Behavior and developer use:** Verifies saved runtime selection without starting an owner or opening a collection. Use snapshot when damaged/untrusted payloads must not be executed or trusted.

```text
Usage: airk runtime inspect [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-inspect).

## airk runtime inspect-candidate

**Purpose:** Verify one immutable approved candidate without selecting or executing it

**Behavior and developer use:** Validates an immutable candidate before selection. Verification does not authorize activation or execute its service.

```text
Usage: airk runtime inspect-candidate [OPTIONS] --install-dir <INSTALL_DIR> --runtime-sha256 <RUNTIME_SHA256>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-inspect-candidate).

## airk runtime inventory

**Purpose:** Read-only inventory of AIRK-owned per-user data and recorded project index roots

**Behavior and developer use:** Dispatches through native installed-runtime ownership, selection and lifecycle guards. Use exact returned identities, receipts and confirmation digests for state changes; location/inspection commands observe rather than authorize a later mutation.

```text
Usage: airk runtime inventory [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-inventory).

## airk runtime launch

**Purpose:** Stable launcher: forward arguments and stdio to the selected verified executable

**Behavior and developer use:** Stable forwarding entrypoint preserves arguments and stdio while resolving the selected verified executable. Treat it as transport, not an evidence operation.

```text
Usage: airk runtime launch [OPTIONS] --install-dir <INSTALL_DIR> -- <ARGS>...
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-launch).

## airk runtime lifecycle

**Purpose:** Stop/restart the observed instance using one bounded durable request read from stdin

**Behavior and developer use:** Reads one bounded durable stop/restart request from stdin. Instance identity and saved outcome prevent a stale request from controlling a replacement owner.

```text
Usage: airk runtime lifecycle [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-lifecycle).

## airk runtime location

**Purpose:** Resolve the native user's stable-channel install location; no workspace settings or writes

**Behavior and developer use:** Dispatches through native installed-runtime ownership, selection and lifecycle guards. Use exact returned identities, receipts and confirmation digests for state changes; location/inspection commands observe rather than authorize a later mutation.

```text
Usage: airk runtime location [OPTIONS]
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-location).

## airk runtime login

**Purpose:** Optional login registration; never required by on-demand operation

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airk runtime login [OPTIONS] --install-dir <INSTALL_DIR> <COMMAND>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-login).

## airk runtime login install

**Purpose:** Register the service to start at login for this user

**Behavior and developer use:** Uses the platform startup manager for this user. Login registration is optional and separate from on-demand operation; stop affects the registered service, while uninstall removes registration.

```text
Usage: airk runtime login --install-dir <INSTALL_DIR> install [OPTIONS]
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-login-install).

## airk runtime login plan

**Purpose:** Show what login startup registration would create; changes nothing

**Behavior and developer use:** Uses the platform startup manager for this user. Login registration is optional and separate from on-demand operation; stop affects the registered service, while uninstall removes registration.

```text
Usage: airk runtime login --install-dir <INSTALL_DIR> plan [OPTIONS]
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-login-plan).

## airk runtime login start

**Purpose:** Start the registered login service now

**Behavior and developer use:** Uses the platform startup manager for this user. Login registration is optional and separate from on-demand operation; stop affects the registered service, while uninstall removes registration.

```text
Usage: airk runtime login --install-dir <INSTALL_DIR> start [OPTIONS]
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-login-start).

## airk runtime login status

**Purpose:** Report whether login startup is registered, loaded and running

**Behavior and developer use:** Uses the platform startup manager for this user. Login registration is optional and separate from on-demand operation; stop affects the registered service, while uninstall removes registration.

```text
Usage: airk runtime login --install-dir <INSTALL_DIR> status [OPTIONS]
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-login-status).

## airk runtime login stop

**Purpose:** Stop the registered login service

**Behavior and developer use:** Uses the platform startup manager for this user. Login registration is optional and separate from on-demand operation; stop affects the registered service, while uninstall removes registration.

```text
Usage: airk runtime login --install-dir <INSTALL_DIR> stop [OPTIONS]
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-login-stop).

## airk runtime login uninstall

**Purpose:** Remove the login startup registration

**Behavior and developer use:** Uses the platform startup manager for this user. Login registration is optional and separate from on-demand operation; stop affects the registered service, while uninstall removes registration.

```text
Usage: airk runtime login --install-dir <INSTALL_DIR> uninstall [OPTIONS]
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-login-uninstall).

## airk runtime managed

**Purpose:** Pin an approved organization-provided payload without copying or modifying it

**Behavior and developer use:** Pins a verified organization-provided read-only payload. AIRK does not overwrite the provider-owned source payload.

```text
Usage: airk runtime managed [OPTIONS] --source <SOURCE> --install-dir <INSTALL_DIR> --approved-sha256 <APPROVED_SHA256> --verification-basis <VERIFICATION_BASIS>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-managed).

## airk runtime prepare-client

**Purpose:** Create an owned lightweight client launcher, including for a read-only managed payload

**Behavior and developer use:** Creates an owned lightweight launcher while leaving an approved managed payload read-only.

```text
Usage: airk runtime prepare-client [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-prepare-client).

## airk runtime purge

**Purpose:** Preview, then with `--confirm <plan_digest>` delete, the contents of selected owned log/cache locations. Project indexes are removed only by unregistering projects

**Behavior and developer use:** First preview owned cache/log cleanup, then confirm the returned plan digest. It cannot remove project indexes; use guarded project unregister for that.

```text
Usage: airk runtime purge [OPTIONS] --install-dir <INSTALL_DIR> --select <SELECT>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-purge).

## airk runtime remove

**Purpose:** Remove one unused owned runtime; preserve active/rollback selections and unknown files

**Behavior and developer use:** Removes only an unused owned runtime slot. Active and rollback selections and unknown files remain protected.

```text
Usage: airk runtime remove [OPTIONS] --install-dir <INSTALL_DIR> --runtime-sha256 <RUNTIME_SHA256>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-remove).

## airk runtime repair-launcher

**Purpose:** Repair an absent or known older launcher from the selected approved payload; requires stopped owners

**Behavior and developer use:** Requires stopped owners and a known absent/older owned launcher. Resume/finalize the same saved repair rather than overwriting an unknown image.

```text
Usage: airk runtime repair-launcher [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-repair-launcher).

## airk runtime repair-payload

**Purpose:** Preview or explicitly repair the selected payload, preserving the complete damaged slot

**Behavior and developer use:** Preview and explicitly repair an owned damaged slot; preserve its complete original. Selection and ownership checks still fence recovery.

```text
Usage: airk runtime repair-payload [OPTIONS] --install-dir <INSTALL_DIR> --source <SOURCE> --approved-sha256 <APPROVED_SHA256>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-repair-payload).

## airk runtime reset-startup

**Purpose:** Explicitly clear the on-demand crash circuit after diagnosing a startup failure

**Behavior and developer use:** Explicitly resets the on-demand crash circuit after investigation. It does not explain or repair the underlying startup failure.

```text
Usage: airk runtime reset-startup [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-reset-startup).

## airk runtime retire-previous

**Purpose:** Retire the previous rollback payload after a successful replacement; preserve the active runtime and project data

**Behavior and developer use:** Explicitly retires the rollback payload after successful replacement. Review that loss of rollback independently of index or source retention.

```text
Usage: airk runtime retire-previous [OPTIONS] --install-dir <INSTALL_DIR> --expected-active <EXPECTED_ACTIVE>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-retire-previous).

## airk runtime rollback

**Purpose:** Restore the preceding verified selection using the same drain and health transaction

**Behavior and developer use:** Uses the same guarded drain/health transaction for the preceding selection; it is not a blind file replacement.

```text
Usage: airk runtime rollback [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-rollback).

## airk runtime self-test

**Purpose:** Bounded executable/paired-helper self-test, without collection access or inference

**Behavior and developer use:** Checks the executable/helper pair with a bounded self-test. It does not open collections, run inference or qualify a complete installed workflow.

```text
Usage: airk runtime self-test [OPTIONS]
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-self-test).

## airk runtime snapshot

**Purpose:** Read saved bootstrap/project metadata without executing or trusting selected payload files

**Behavior and developer use:** Reads bootstrap/project metadata without executing or trusting the selected payload. This supports diagnosis when ordinary verified selection is unavailable.

```text
Usage: airk runtime snapshot [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-snapshot).

## airk runtime source-scope

**Purpose:** Read one exact saved source scope, without electing a service or opening an index

**Behavior and developer use:** Reads one exact saved source binding without electing a service or opening an index. Do not substitute a UI-selected project.

```text
Usage: airk runtime source-scope [OPTIONS] --install-dir <INSTALL_DIR> --expected-control <EXPECTED_CONTROL> --expected-runtime <EXPECTED_RUNTIME> --source-root <SOURCE_ROOT>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-source-scope).

## airk runtime stage

**Purpose:** Stage an offline payload with an independently approved install-manifest digest

**Behavior and developer use:** Copies a candidate only against an independently approved install-manifest digest. Never derive approval by trusting the same unverified input being staged.

```text
Usage: airk runtime stage [OPTIONS] --source <SOURCE> --install-dir <INSTALL_DIR> --approved-sha256 <APPROVED_SHA256> --verification-basis <VERIFICATION_BASIS>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-stage).

## airk runtime start

**Purpose:** Explicitly start a deliberately stopped service; automatic ensure honors that stop

**Behavior and developer use:** Explicit user start can clear deliberate Stop. Distinguish this authorization from passive ensure/reconnect.

```text
Usage: airk runtime start [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-start).

## airk runtime uninstall

**Purpose:** Stop/unregister this service and retire owned runtime payloads; preserve project data

**Behavior and developer use:** Stops/unregisters this service and removes owned runtime payloads while preserving project data. This differs from unregistering a project and deleting its index.

```text
Usage: airk runtime uninstall [OPTIONS] --install-dir <INSTALL_DIR>
```

**Implementation:** [runtime_install_commands.rs](../crates/airk-cli/src/runtime_install_commands.rs). **Regression/contract evidence:** [runtime_install_tests.rs](../crates/airk-cli/src/runtime_install_tests.rs). Full options: [parser reference](cli-reference.md#airk-runtime-uninstall).

## airk search

**Purpose:** Search indexed evidence; results carry file, line and revision citations.

**Behavior and developer use:** Use for concepts and supplementary discovery; named definitions usually begin with symbol/code-facts. --language and --evidence-kind filter indexed extraction before lexical/vector candidate selection; --exclude-path-prefix is literal query scope. Preserve mode/readiness policy, lexical AND semantics, partial coverage and exact citations. Bounded definition pointers can aid the next lookup without changing ranking. Filters do not change indexing policy.

```text
Usage: airk search [OPTIONS] <QUERY>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-search).

## airk serve

**Purpose:** Run the AIRK service in the foreground for a private control directory (created if missing).

**Behavior and developer use:** Owns a foreground service in an explicit private control directory. This isolated route bypasses installed launcher preparation; benchmark results here do not measure installed startup.

```text
Usage: airk serve [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-serve).

## airk service

**Purpose:** Bounded native metadata and request recovery; never a project writer

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airk service [OPTIONS] <COMMAND>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service).

## airk service console

**Purpose:** Persistent JSON management transport on stdin/stdout (used by the VS Code extension)

**Behavior and developer use:** Persistent JSON management transport used by the extension. This is not the MCP evidence protocol and is not a one-result terminal query.

```text
Usage: airk service console [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-console).

## airk service health

**Purpose:** Check that the service for --runtime-dir is running and healthy; exits non-zero otherwise

**Behavior and developer use:** Targets the explicit control directory and native metadata/request protocol. Preserve service, project and durable request identities; management responses are not indexed evidence.

```text
Usage: airk service health [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-health).

## airk service legacy

**Purpose:** Review and retire pre-1.4 startup entries and derived state (explicit, previewed)

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airk service legacy [OPTIONS] <COMMAND>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy).

## airk service legacy derived-receipts

**Purpose:** List captured old-derived cleanup scopes for explicit recovery

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy derived-receipts [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-derived-receipts).

## airk service legacy forget

**Purpose:** Forget only a completed local receipt; never stop a process or repeat work

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy forget [OPTIONS] --runtime-dir <RUNTIME_DIR> --approval <APPROVAL>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-forget).

## airk service legacy forget-derived

**Purpose:** Forget one approved old-derived cleanup scope after review

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy forget-derived [OPTIONS] --runtime-dir <RUNTIME_DIR> --approval <APPROVAL>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-forget-derived).

## airk service legacy list

**Purpose:** List bounded saved removal identities/phases, including interrupted work

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy list [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-list).

## airk service legacy preview-derived

**Purpose:** Preview known owned 1.3 derived files without opening an old database

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy preview-derived [OPTIONS] --runtime-dir <RUNTIME_DIR> --root <ROOT> --data-dir <DATA_DIR> --collection <COLLECTION>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-preview-derived).

## airk service legacy remove-derived

**Purpose:** Delete one bounded turn of explicitly reviewed old derived files

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy remove-derived [OPTIONS] --runtime-dir <RUNTIME_DIR> --root <ROOT> --data-dir <DATA_DIR> --collection <COLLECTION> --approval <APPROVAL>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-remove-derived).

## airk service legacy remove-startup

**Purpose:** Remove only reviewed old login startup; preserve sources/settings/indexes

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy remove-startup [OPTIONS] --runtime-dir <RUNTIME_DIR> --root <ROOT> --data-dir <DATA_DIR> --collection <COLLECTION> --executable <EXECUTABLE> --approval <APPROVAL>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-remove-startup).

## airk service legacy review

**Purpose:** Read saved ownership and OS state; do not start, stop or convert anything

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy review [OPTIONS] --runtime-dir <RUNTIME_DIR> --root <ROOT> --data-dir <DATA_DIR> --collection <COLLECTION> --executable <EXECUTABLE>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-review).

## airk service legacy show

**Purpose:** Show one saved removal identity and its phase

**Behavior and developer use:** Works only on explicit pre-1.4 cleanup scopes/receipts. Review saved ownership and preview before removal; retain unknown files, sources and unrelated state. Forgetting bookkeeping is separate from stopping/removing anything.

```text
Usage: airk service legacy show [OPTIONS] --runtime-dir <RUNTIME_DIR> --approval <APPROVAL>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-legacy-show).

## airk service logs

**Purpose:** Print recent service log records (--limit), export all retained logs (--all) or the raw job-error ring (--raw)

**Behavior and developer use:** Exports the finite retained service/raw error rings. A larger export cannot reconstruct logs already evicted; treat paths and diagnostics as private when sharing.

```text
Usage: airk service logs [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-logs).

## airk service prepare-configuration

**Purpose:** Prepare a guarded configuration request for a project; submit it with `requests retry`

**Behavior and developer use:** Prepares a guarded durable configuration request; submission is a separate requests retry step using the saved nonce.

```text
Usage: airk service prepare-configuration [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-prepare-configuration).

## airk service prepare-registration

**Purpose:** Print the canonical registration for --root/--data-dir/--collection, including its binding. Read-only

**Behavior and developer use:** Computes canonical source/data/collection identity read-only. Review the binding before reserving or creating storage.

```text
Usage: airk service prepare-registration [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-prepare-registration).

## airk service prepare-storage

**Purpose:** Create the reserved index storage for an already reserved project (owner-only on Unix)

**Behavior and developer use:** Creates reserved index storage only for an already reserved project, with private ownership on Unix. Never use a source path as index storage.

```text
Usage: airk service prepare-storage [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-prepare-storage).

## airk service record-error

**Purpose:** Record a bounded local extension/setup error without starting the service

**Behavior and developer use:** Records bounded local setup/extension diagnostics without starting the service. Keep source contents and credentials out of error messages.

```text
Usage: airk service record-error [OPTIONS] --runtime-dir <RUNTIME_DIR> --operation <OPERATION>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-record-error).

## airk service register

**Purpose:** Register --root/--data-dir/--collection with the service at --runtime-dir. The data directory must exist

**Behavior and developer use:** Registers the canonical binding with the explicit service; data storage must already exist. init combines the normal installed registration/storage workflow.

```text
Usage: airk service register [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-register).

## airk service requests

**Purpose:** Prepare, list, retry or forget durable client requests (jobs and configuration) for a control directory

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airk service requests [OPTIONS] --runtime-dir <RUNTIME_DIR> <COMMAND>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-requests).

## airk service requests forget

**Purpose:** Discard a saved request record by its nonce

**Behavior and developer use:** Forgets local request bookkeeping. It does not cancel work the service already accepted.

```text
Usage: airk service requests --runtime-dir <RUNTIME_DIR> forget [OPTIONS] <NONCE>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-requests-forget).

## airk service requests list

**Purpose:** List saved client requests and their outcomes

**Behavior and developer use:** Targets the explicit control directory and native metadata/request protocol. Preserve service, project and durable request identities; management responses are not indexed evidence.

```text
Usage: airk service requests --runtime-dir <RUNTIME_DIR> list [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-requests-list).

## airk service requests prepare

**Purpose:** Save a job request (--kind index, inspect, reconcile, vectors, model_setup, verify, recover or maintain)

**Behavior and developer use:** Saves a job intent without submitting it. Keep the returned nonce to reconcile acceptance with the same durable identity.

```text
Usage: airk service requests --runtime-dir <RUNTIME_DIR> prepare [OPTIONS] --project <PROJECT>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-requests-prepare).

## airk service requests prepare-configuration

**Purpose:** Save a configuration request for a project from a draft --file, without submitting it

**Behavior and developer use:** Saves the complete guarded configuration intent without submitting; a later retry must use that exact saved request.

```text
Usage: airk service requests --runtime-dir <RUNTIME_DIR> prepare-configuration [OPTIONS] --project <PROJECT> --file <FILE>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-requests-prepare-configuration).

## airk service requests retry

**Purpose:** Submit, or resubmit after an unknown outcome, a saved request by its nonce

**Behavior and developer use:** Submits/reconciles the same saved nonce after an unknown outcome. This idempotent management contract does not authorize replay of arbitrary MCP evidence plans.

```text
Usage: airk service requests --runtime-dir <RUNTIME_DIR> retry [OPTIONS] <NONCE>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-requests-retry).

## airk service rpc

**Purpose:** Send one raw management request (JSON from --request-file or --stdin) and print the response

**Behavior and developer use:** Low-level bounded management JSON escape hatch. Prefer typed higher-level commands and retain instance/request guards when using it.

In 1.8.5, `resources` and `configure_resources` expose the same concurrent evidence setting as Management → Service: **4, 8, 16 (default), 32 or 64**. Set `configuration.query_concurrency` while preserving the other resource fields. See the [PowerShell read/update example and propagation contract](service-resources.md#cli-access). There is no dedicated concurrency flag.

```text
Usage: airk service rpc [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-rpc).

## airk service shutdown

**Purpose:** Ask the service for --runtime-dir to drain its workers and exit

**Behavior and developer use:** Requests drain of the selected service and workers, affecting all its projects. Project pause has narrower scope.

```text
Usage: airk service shutdown [OPTIONS] --runtime-dir <RUNTIME_DIR>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [service_runtime.rs](../crates/airk-cli/tests/service_runtime.rs). Full options: [parser reference](cli-reference.md#airk-service-shutdown).

## airk settings

**Purpose:** Read one project setting view; read-only.

**Behavior and developer use:** The public parser selects the native project and dispatches to its owning service/kernel adapter. Evidence responses retain indexed revision identity and bounded extraction coverage; they do not establish live-file freshness or complete language semantics.

```text
Usage: airk settings [OPTIONS] <VIEW>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-settings).

## airk setup

**Purpose:** Print the MCP client configuration (command and arguments) for an already registered project

**Behavior and developer use:** Prints client configuration for an already registered project and its verified launcher. Review/copy the output into the agent configuration; this command does not edit client files.

```text
Usage: airk setup [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-setup).

## airk setup-model

**Purpose:** Prepare the bundled offline embedding model for vector search; keeps an existing or custom selection

**Behavior and developer use:** Prepares the bundled offline embedding model through native job ownership and preserves an existing/custom selection. It is not a cloud model download or an evidence query.

```text
Usage: airk setup-model [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-setup-model).

## airk status

A completed status query that overlaps ordinary scalar-cache invalidation returns its original snapshot successfully with `status_observation.superseded: true`, `reason: scalar_invalidation` and an explanatory message. Original publication and observation age are retained. The superseded snapshot does not repopulate the current cache or acknowledge an obsolete status receipt; the query is not replayed. Project/namespace changes, authenticated worker replacement, expired source authorization and shutdown retain their existing rejection behavior. Cached reuse has its five-second limit; configured evidence capacity has one additional reserved status slot.

**Purpose:** Print the project's publication, freshness, coverage and job status

**Behavior and developer use:** Reads project publication/coverage and job observations. It differs from lightweight MCP status, whose scope excludes detailed job and process probes.

```text
Usage: airk status [OPTIONS]
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-status).

## airk symbol

**Purpose:** Find definitions by exact name by default; use --name-match for suffix/contains variants.

**Behavior and developer use:** Start compact, scope with --language/--path-prefix, and use --limit/--max-bytes before requesting rich detail. --name-match suffix|contains compares simple or qualified names case-sensitively; omitted mode keeps exact lookup. --include-count reports qualified count precision; bounded partial pages do not claim a full total. Matching and representation bind the cursor.

```text
Usage: airk symbol [OPTIONS] <NAME>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-symbol).

## airk tools

**Purpose:** List, describe or call the read-only MCP tools from the terminal.

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airk tools [OPTIONS] <COMMAND>
```

**Implementation:** [tools.rs](../crates/airk-cli/src/tools.rs). **Regression/contract evidence:** [tools.rs](../crates/airk-cli/tests/tools.rs). Full options: [parser reference](cli-reference.md#airk-tools).

## airk tools call

**Purpose:** Invoke an MCP tool and print its unchanged result envelope as JSON

**Behavior and developer use:** Accepts one JSON object through --args or --args-file (maximum 64 KiB) and returns the native tool envelope. Use prefixed names such as airk_search.

```text
Usage: airk tools call [OPTIONS] <TOOL>
```

**Implementation:** [tools.rs](../crates/airk-cli/src/tools.rs). **Regression/contract evidence:** [tools.rs](../crates/airk-cli/tests/tools.rs). Full options: [parser reference](cli-reference.md#airk-tools-call).

## airk tools help

**Purpose:** Describe a native tool's schema, defaults and argument template without opening a project.

**Behavior and developer use:** Works offline without a registration or index. Use it to obtain the exact schema before constructing tools call arguments.

```text
Usage: airk tools help [OPTIONS] <TOOL>
```

**Implementation:** [tools.rs](../crates/airk-cli/src/tools.rs). **Regression/contract evidence:** [tools.rs](../crates/airk-cli/tests/tools.rs). Full options: [parser reference](cli-reference.md#airk-tools) (use the tools section and built-in help for tools help).

## airk tools list

**Purpose:** List the 18 native prefixed tools; --json includes schemas. Unified MCP adds projects and nine Explorer Assistance tools.

**Behavior and developer use:** Lists 18 native prefixed tool schemas. It does not list all 28 unified MCP tools; projects and Explorer Assistance have separate CLI commands.

```text
Usage: airk tools list [OPTIONS]
```

**Implementation:** [tools.rs](../crates/airk-cli/src/tools.rs). **Regression/contract evidence:** [tools.rs](../crates/airk-cli/tests/tools.rs). Full options: [parser reference](cli-reference.md#airk-tools-list).

## airk tools shell

**Purpose:** Open a collection-bound interactive console with help and examples

**Behavior and developer use:** Interactive collection-bound tool console with help/examples. Its session lifetime and prompts differ from one-shot JSON commands.

```text
Usage: airk tools shell [OPTIONS]
```

**Implementation:** [tools.rs](../crates/airk-cli/src/tools.rs). **Regression/contract evidence:** [tools.rs](../crates/airk-cli/tests/tools.rs). Full options: [parser reference](cli-reference.md#airk-tools-shell).

## airk usage

**Purpose:** Local usage reporting for the selected project: report, policy, configure (mode), clear, feedback, import, calibrate

**Behavior and developer use:** Command group: choose a child listed below or run --help. The group itself does not run all child operations.

```text
Usage: airk usage [OPTIONS] <COMMAND>
```

**Implementation:** [usage_client.rs](../crates/airk-cli/src/usage_client.rs). **Regression/contract evidence:** [usage_native.rs](../crates/airk-cli/tests/usage_native.rs). Full options: [parser reference](cli-reference.md#airk-usage).

## airk usage calibrate

**Purpose:** Calibrate token estimates against a tokenizer and baseline/returned samples (advanced)

**Behavior and developer use:** Compares local tokenizer and sample estimates. It does not contact a provider or prove actual billed savings.

```text
Usage: airk usage calibrate [OPTIONS] --tokenizer <TOKENIZER> --baseline <BASELINE> --returned <RETURNED> --generation <GENERATION> --content-kind <CONTENT_KIND>
```

**Implementation:** [usage_client.rs](../crates/airk-cli/src/usage_client.rs). **Regression/contract evidence:** [usage_native.rs](../crates/airk-cli/tests/usage_native.rs). Full options: [parser reference](cli-reference.md#airk-usage-calibrate).

## airk usage clear

**Purpose:** Delete recorded usage for the project

**Behavior and developer use:** Deletes this project's recorded usage; source/index evidence remains separate.

```text
Usage: airk usage clear [OPTIONS]
```

**Implementation:** [usage_client.rs](../crates/airk-cli/src/usage_client.rs). **Regression/contract evidence:** [usage_native.rs](../crates/airk-cli/tests/usage_native.rs). Full options: [parser reference](cli-reference.md#airk-usage-clear).

## airk usage configure

**Purpose:** Set the recording mode: disabled, aggregates_only or diagnostic_events

**Behavior and developer use:** Changes local recording mode among disabled, aggregates_only and diagnostic_events. Review privacy and retention before enabling detailed events.

```text
Usage: airk usage configure [OPTIONS] --mode <MODE>
```

**Implementation:** [usage_client.rs](../crates/airk-cli/src/usage_client.rs). **Regression/contract evidence:** [usage_native.rs](../crates/airk-cli/tests/usage_native.rs). Full options: [parser reference](cli-reference.md#airk-usage-configure).

## airk usage feedback

**Purpose:** Record whether one returned request was useful, partly useful or not useful

**Behavior and developer use:** Uses the selected project's native local usage/settings adapter. Usage measurements and retention are independent of index readiness and never authorize source changes.

```text
Usage: airk usage feedback [OPTIONS] --usefulness <USEFULNESS> <REQUEST_ID>
```

**Implementation:** [usage_client.rs](../crates/airk-cli/src/usage_client.rs). **Regression/contract evidence:** [usage_native.rs](../crates/airk-cli/tests/usage_native.rs). Full options: [parser reference](cli-reference.md#airk-usage-feedback).

## airk usage import

**Purpose:** Import a usage export; --preview shows what would change and --if-sha256 pins the file

**Behavior and developer use:** Preview first and pin the reviewed bytes with --if-sha256 when importing. Import mutates local usage records, not evidence or source files.

```text
Usage: airk usage import [OPTIONS] <FILE>
```

**Implementation:** [usage_client.rs](../crates/airk-cli/src/usage_client.rs). **Regression/contract evidence:** [usage_native.rs](../crates/airk-cli/tests/usage_native.rs). Full options: [parser reference](cli-reference.md#airk-usage-import).

## airk usage policy

**Purpose:** Show the project's usage recording mode and retention

**Behavior and developer use:** Uses the selected project's native local usage/settings adapter. Usage measurements and retention are independent of index readiness and never authorize source changes.

```text
Usage: airk usage policy [OPTIONS]
```

**Implementation:** [usage_client.rs](../crates/airk-cli/src/usage_client.rs). **Regression/contract evidence:** [usage_native.rs](../crates/airk-cli/tests/usage_native.rs). Full options: [parser reference](cli-reference.md#airk-usage-policy).

## airk usage report

**Purpose:** Summarize recorded usage for the last --days (json, csv or markdown; optional price scenario file)

**Behavior and developer use:** Reports recorded estimates with optional price scenarios. Estimates are not provider invoices or measured money saved.

```text
Usage: airk usage report [OPTIONS]
```

**Implementation:** [usage_client.rs](../crates/airk-cli/src/usage_client.rs). **Regression/contract evidence:** [usage_native.rs](../crates/airk-cli/tests/usage_native.rs). Full options: [parser reference](cli-reference.md#airk-usage-report).

## airk watch

**Purpose:** Turn native source watching on or off for the project (on keeps the index current after edits)

**Behavior and developer use:** Changes native source watching for this project. On keeps watching independently of editor lifetime; it does not register OS login startup.

```text
Usage: airk watch [OPTIONS] <MODE>
```

**Implementation:** [main.rs](../crates/airk-cli/src/main.rs). **Regression/contract evidence:** [cli_commands.rs](../crates/airk-cli/tests/cli_commands.rs). Full options: [parser reference](cli-reference.md#airk-watch).

## Changing a command safely

Read the parser/dispatcher and owning module before editing; the links above are entrypoints, not a claim that every behavior lives in main.rs. Runtime dispatch leads to staging, activation, layout, managed-payload, repair and retirement modules; their unit tests sit beside those implementations. Evidence dispatch leads to the shared [MCP native handlers](../crates/airk-mcp/src/lib.rs), Kernel queries and Store. The CLI owns transport, scheduling and lifecycle; it does not move service ownership into the editor or make Kernel/Store depend on UI adapters.

For help changes, regenerate docs/cli-reference.md with the existing parser-reference test workflow and review its diff. Keep this guide, the [MCP developer guide](mcp-developer-guide.md), public schemas, [CLI workflows](cli.md), release notes and README links consistent. Run the relevant command/contract tests and repository checks; link evidence for the exact platform tested and report unavailable qualification honestly.

The [CLI, MCP and Workbench parity audit](read-interface-parity.md) maps all read operations and records complete-data, observation, budget and export contracts for 1.8.7.
