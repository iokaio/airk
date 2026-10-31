# AIRK CLI reference

<!-- Generated from the native parser by `cargo test -p airk-cli --bin airk cli_reference`; do not edit. Set AIRK_WRITE_CLI_REFERENCE=1 to regenerate. -->

Every public command and subcommand, exactly as `airk <command> --help` prints it. For concepts, workflows and the feature map, see [Running AIRK headless](headless-cli.md) and [the CLI guide](cli.md).

## Contents

- [`airk`](#airk)
- [`airk bridge`](#airk-bridge)
- [`airk changes`](#airk-changes)
- [`airk code-facts`](#airk-code-facts)
- [`airk configure`](#airk-configure)
- [`airk context`](#airk-context)
- [`airk explorer`](#airk-explorer)
- [`airk explorer draft-plan`](#airk-explorer-draft-plan)
- [`airk explorer draft-search`](#airk-explorer-draft-search)
- [`airk explorer draft-strategy`](#airk-explorer-draft-strategy)
- [`airk explorer get-handoff`](#airk-explorer-get-handoff)
- [`airk explorer get-template`](#airk-explorer-get-template)
- [`airk explorer list-templates`](#airk-explorer-list-templates)
- [`airk explorer proposal-status`](#airk-explorer-proposal-status)
- [`airk explorer suggest-view`](#airk-explorer-suggest-view)
- [`airk explorer validate-draft`](#airk-explorer-validate-draft)
- [`airk facts`](#airk-facts)
- [`airk files`](#airk-files)
- [`airk host`](#airk-host)
- [`airk imports`](#airk-imports)
- [`airk index`](#airk-index)
- [`airk info`](#airk-info)
- [`airk init`](#airk-init)
- [`airk inspect`](#airk-inspect)
- [`airk mcp`](#airk-mcp)
- [`airk model-catalog`](#airk-model-catalog)
- [`airk outline`](#airk-outline)
- [`airk parse`](#airk-parse)
- [`airk pause`](#airk-pause)
- [`airk projects`](#airk-projects)
- [`airk read`](#airk-read)
- [`airk refs`](#airk-refs)
- [`airk relationships`](#airk-relationships)
- [`airk resume`](#airk-resume)
- [`airk runtime`](#airk-runtime)
- [`airk runtime abandon-lifecycle`](#airk-runtime-abandon-lifecycle)
- [`airk runtime abort-activation`](#airk-runtime-abort-activation)
- [`airk runtime abort-launcher-repair`](#airk-runtime-abort-launcher-repair)
- [`airk runtime abort-layout`](#airk-runtime-abort-layout)
- [`airk runtime abort-stage`](#airk-runtime-abort-stage)
- [`airk runtime activate`](#airk-runtime-activate)
- [`airk runtime container-host`](#airk-runtime-container-host)
- [`airk runtime diagnose`](#airk-runtime-diagnose)
- [`airk runtime ensure`](#airk-runtime-ensure)
- [`airk runtime foreground`](#airk-runtime-foreground)
- [`airk runtime inspect`](#airk-runtime-inspect)
- [`airk runtime inspect-candidate`](#airk-runtime-inspect-candidate)
- [`airk runtime inventory`](#airk-runtime-inventory)
- [`airk runtime launch`](#airk-runtime-launch)
- [`airk runtime lifecycle`](#airk-runtime-lifecycle)
- [`airk runtime location`](#airk-runtime-location)
- [`airk runtime login`](#airk-runtime-login)
- [`airk runtime login install`](#airk-runtime-login-install)
- [`airk runtime login plan`](#airk-runtime-login-plan)
- [`airk runtime login start`](#airk-runtime-login-start)
- [`airk runtime login status`](#airk-runtime-login-status)
- [`airk runtime login stop`](#airk-runtime-login-stop)
- [`airk runtime login uninstall`](#airk-runtime-login-uninstall)
- [`airk runtime managed`](#airk-runtime-managed)
- [`airk runtime prepare-client`](#airk-runtime-prepare-client)
- [`airk runtime purge`](#airk-runtime-purge)
- [`airk runtime remove`](#airk-runtime-remove)
- [`airk runtime repair-launcher`](#airk-runtime-repair-launcher)
- [`airk runtime repair-payload`](#airk-runtime-repair-payload)
- [`airk runtime reset-startup`](#airk-runtime-reset-startup)
- [`airk runtime retire-previous`](#airk-runtime-retire-previous)
- [`airk runtime rollback`](#airk-runtime-rollback)
- [`airk runtime self-test`](#airk-runtime-self-test)
- [`airk runtime snapshot`](#airk-runtime-snapshot)
- [`airk runtime stage`](#airk-runtime-stage)
- [`airk runtime start`](#airk-runtime-start)
- [`airk runtime uninstall`](#airk-runtime-uninstall)
- [`airk search`](#airk-search)
- [`airk serve`](#airk-serve)
- [`airk service`](#airk-service)
- [`airk service console`](#airk-service-console)
- [`airk service health`](#airk-service-health)
- [`airk service legacy`](#airk-service-legacy)
- [`airk service legacy derived-receipts`](#airk-service-legacy-derived-receipts)
- [`airk service legacy forget`](#airk-service-legacy-forget)
- [`airk service legacy forget-derived`](#airk-service-legacy-forget-derived)
- [`airk service legacy list`](#airk-service-legacy-list)
- [`airk service legacy preview-derived`](#airk-service-legacy-preview-derived)
- [`airk service legacy remove-derived`](#airk-service-legacy-remove-derived)
- [`airk service legacy remove-startup`](#airk-service-legacy-remove-startup)
- [`airk service legacy review`](#airk-service-legacy-review)
- [`airk service legacy show`](#airk-service-legacy-show)
- [`airk service logs`](#airk-service-logs)
- [`airk service prepare-configuration`](#airk-service-prepare-configuration)
- [`airk service prepare-registration`](#airk-service-prepare-registration)
- [`airk service prepare-storage`](#airk-service-prepare-storage)
- [`airk service record-error`](#airk-service-record-error)
- [`airk service register`](#airk-service-register)
- [`airk service requests`](#airk-service-requests)
- [`airk service requests forget`](#airk-service-requests-forget)
- [`airk service requests list`](#airk-service-requests-list)
- [`airk service requests prepare`](#airk-service-requests-prepare)
- [`airk service requests prepare-configuration`](#airk-service-requests-prepare-configuration)
- [`airk service requests retry`](#airk-service-requests-retry)
- [`airk service rpc`](#airk-service-rpc)
- [`airk service shutdown`](#airk-service-shutdown)
- [`airk settings`](#airk-settings)
- [`airk setup`](#airk-setup)
- [`airk setup-model`](#airk-setup-model)
- [`airk status`](#airk-status)
- [`airk symbol`](#airk-symbol)
- [`airk tools`](#airk-tools)
- [`airk tools call`](#airk-tools-call)
- [`airk tools list`](#airk-tools-list)
- [`airk tools shell`](#airk-tools-shell)
- [`airk usage`](#airk-usage)
- [`airk usage calibrate`](#airk-usage-calibrate)
- [`airk usage clear`](#airk-usage-clear)
- [`airk usage configure`](#airk-usage-configure)
- [`airk usage feedback`](#airk-usage-feedback)
- [`airk usage import`](#airk-usage-import)
- [`airk usage policy`](#airk-usage-policy)
- [`airk usage report`](#airk-usage-report)
- [`airk watch`](#airk-watch)

## airk

```text
Artifact Indexing and Retrieval Kernel: local, read-only code and document retrieval through a native service. Run `airk --help` (not -h) for the full guide.

Usage: airk [OPTIONS] <COMMAND>

Commands:
  parse          Plan, and with --execute run, a multi-step read-only evidence request
  settings       Read one project setting view; read-only
  configure      Apply a guarded project configuration draft from --file or --stdin (JSON with an "operation")
  usage          Local usage reporting for the selected project: report, policy, configure (mode), clear, feedback, import, calibrate
  runtime        Installed-runtime lifecycle: location, inspect, ensure, start, login, uninstall, inventory, purge and repair
  bridge         Internal read-only adapter used by trusted launchers
  serve          Run the AIRK service in the foreground for a private control directory (created if missing)
  model-catalog  Print the offline model catalog and the CPU-based model choice for this machine; opens no project state
  tools          List, describe or call the read-only MCP tools from the terminal
  changes        Report indexed changes since a revision token: added, modified and removed files, symbols and documents
  code-facts     List code facts (definitions, signatures, durable symbol keys) for a symbol or path, with continuation cursors
  relationships  Follow indexed relationships of a symbol in --direction, optionally of one --kind, up to --hops levels
  setup          Print the MCP client configuration (command and arguments) for an already registered project
  init           Register --root as a project of the installed runtime and prepare its index storage
  status         Print the project's publication, freshness, coverage and job status
  search         Search indexed evidence; results carry file, line and revision citations
  files          List indexed files under --prefix, with continuation cursors
  read           Read an indexed file's lines (--start-line/--end-line) from the published revision, with a citation
  mcp            Serve the read-only MCP protocol on stdin/stdout for one agent connection (this is what agents launch)
  info           Show what the index knows about one file: language, size, extraction method and parse status
  outline        Outline one file's structure: symbols, headings and sections with line ranges
  symbol         Find symbol definitions by name, optionally filtered by language or path prefix
  imports        List import and dependency statements found under a path prefix
  facts          List document facts (requirements, dates, decisions, features) extracted from indexed documents
  refs           Find indexed references to a name under a path prefix
  context        Read lines around --line in a file (--before/--after), with a citation
  index          Queue indexing of the project's sources and print the job receipt. Follow it with `tools call airk_status`
  setup-model    Prepare the bundled offline embedding model for vector search; keeps an existing or custom selection
  inspect        Queue a bounded inspection job that refreshes coverage and readiness without reindexing
  pause          Pause automatic and queued work for the project; existing evidence stays queryable
  resume         Resume work paused with `pause`
  watch          Turn native source watching on or off for the project (on keeps the index current after edits)
  projects       List the projects registered with the service (same fields as the MCP `projects` tool)
  explorer       Explorer Assistance tools without an editor; every result is written to --output
  host           Run the installed service under an explicit container/user supervisor
  service        Bounded native metadata and request recovery; never a project writer
  help           Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

HOW AIRK WORKS
  AIRK indexes local source code and documents and answers bounded, cited,
  read-only queries. One native service per control directory owns a registry
  of projects, a durable job queue and resource admission. The service starts
  project workers (indexing and queries), a source watcher and the embedding
  helper (airk-embed) itself; you start only the service, CLI commands and MCP
  frontends. Every command prints JSON on stdout. Errors exit non-zero with a
  message on stderr; set AIRK_ERROR_FORMAT=json for a structured error.

SELECTING A SERVICE AND PROJECT
  A project is (--root, --data-dir, --collection). Its canonical form has a
  64-hex "binding". Commands reach the service in one of two ways:
    Installed runtime (what the VS Code extension sets up):
      airk --root <src> [--data-dir <index dir>] [--collection <name>] <command>
      (--service-install-dir <dir> selects a non-default installation)
    Explicit service started by you (headless, no installation):
      airk --service-runtime-dir <control> --service-project <binding> \
           --root <src> --data-dir <index dir> --collection <name> <command>

HEADLESS QUICK START (no installation)
  airk serve --runtime-dir <control>                 # terminal 1: the service
  airk service health --runtime-dir <control>         # wait until this succeeds
  airk --root <src> --data-dir <data> --collection demo service prepare-registration
                                                      # read-only; prints "binding"
  airk --root <src> --data-dir <data> --collection demo service register --runtime-dir <control>
  airk <scope> index                                  # queue indexing; prints a job receipt
  airk <scope> tools call airk_status --args '{}'     # job, coverage and readiness
  airk <scope> search "words to find" --mode lexical # cited search; vectors need setup-model
  airk <scope> watch on                               # keep the index current
  airk <scope> mcp                                    # stdio MCP server for an agent
  airk service shutdown --runtime-dir <control>
  where <scope> is the explicit-service flag set above. On macOS/Linux the
  data directory should be private to your account (mode 0700).

LEARNING THE TOOLS
  airk tools list [--json]         every MCP tool (also available as commands)
  airk tools help <tool>           description, input schema and an argument template
  airk tools call airk_help        the engine's own topic guide
  airk <command> --help            arguments of any command below

PROJECTS AND EXPLORER ASSISTANCE
  airk projects [--runtime-dir <control>]           registered projects and their bindings
  airk explorer <tool> --args '<json>' --output <new file>
                                                   draft/template/validation tools; results go
                                                   to the file, never to an Explorer Inbox

MANAGEMENT WITHOUT VS CODE
  pause / resume / watch on|off / inspect / setup-model   project work
  settings <view> / configure --stdin                      read or change project settings
                                                           ({"operation":"unregister"} removes a project's index)
  usage report|policy|configure                            local usage reporting
  service console|logs|rpc|requests                        service metadata, logs and raw requests
  runtime location|inventory|purge|uninstall|...           installed-runtime lifecycle and data review
```

## airk bridge

```text
Internal read-only adapter used by trusted launchers

Usage: airk bridge [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --project <PROJECT>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk changes

```text
Report indexed changes since a revision token: added, modified and removed files, symbols and documents

Usage: airk changes [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --since <SINCE>
          

      --data-dir <DATA_DIR>
          

      --selector <SELECTORS>
          Repeatable tagged JSON selector, e.g. {"kind":"path","path":"one.rs"}

      --collection <COLLECTION>
          [default: default]

      --path <PATHS>
          Remember an exact collection-relative indexed path

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --include-symbols
          

      --include-documents
          

      --service-project <SERVICE_PROJECT>
          

      --include-suggestions
          Bounded similarity/directory hints; never follow or promote identity

      --limit <LIMIT>
          [default: 10]

      --max-bytes <MAX_BYTES>
          [default: 8192]

      --cursor <CURSOR>
          

  -h, --help
          Print help
```

## airk code-facts

```text
List code facts (definitions, signatures, durable symbol keys) for a symbol or path, with continuation cursors

Usage: airk code-facts [OPTIONS]

Options:
      --compact [<COMPACT>]
          False includes rich facts and durable IDs; independent of response mode
          
          [possible values: true, false]

      --root <ROOT>
          [default: .]

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --symbol-key <SYMBOL_KEY>
          Durable symbol key from indexed continuity evidence; excludes textual filters

      --name <NAME>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --kind <KIND>
          

      --language <LANGUAGE>
          

      --service-project <SERVICE_PROJECT>
          

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --limit <LIMIT>
          [default: 10]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airk configure

```text
Apply a guarded project configuration draft from --file or --stdin (JSON with an "operation").

Examples: {"operation":"unregister"} deletes the project's index and saved settings (sources untouched); {"operation":"vectors","enabled":true} turns on semantic search. Prints the accepted job receipt.

Usage: airk configure [OPTIONS]

Options:
      --file <FILE>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --stdin
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk context

```text
Read lines around --line in a file (--before/--after), with a citation

Usage: airk context [OPTIONS] <PATH> <LINE>

Arguments:
  <PATH>
          

  <LINE>
          

Options:
      --revision <REVISION>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --if-evidence <IF_EVIDENCE>
          

      --collection <COLLECTION>
          [default: default]

      --follow-moves
          

      --known-hash <KNOWN_HASH>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --artifact-id <ARTIFACT_ID>
          

      --before <BEFORE>
          [default: 5]

      --service-project <SERVICE_PROJECT>
          

      --after <AFTER>
          [default: 5]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airk explorer

```text
Explorer Assistance tools without an editor; every result is written to --output.

Drafts are checked against the same schemas as the MCP tools and written to a new file for review. Nothing is sent to an Explorer Inbox or executed. get-handoff alone needs a running editor.

Usage: airk explorer [OPTIONS] <COMMAND>

Commands:
  draft-search     Write a proposed Explorer Search form (MCP draft_search) after schema validation
  draft-strategy   Write a proposed Studio strategy (MCP draft_strategy) after schema validation
  draft-plan       Write proposed Planner tasks (MCP draft_plan) after schema validation
  suggest-view     Write a Navigation, Review or Management suggestion (MCP suggest_view; --args {"kind":…,"draft":…})
  validate-draft   Validate a draft without writing it as a proposal (MCP validate_draft; --args {"kind":…,"draft":…})
  list-templates   List the bundled strategy templates (MCP list_templates; optional offset, limit, max_bytes)
  get-template     Read one strategy template with its operators (MCP get_template; --args {"template_id":…})
  proposal-status  Report the status of a draft file written by this CLI (it is never admitted to an Inbox)
  get-handoff      Read a handoff the user shared from a running Explorer window (MCP get_handoff)
  help             Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk explorer draft-plan

```text
Write proposed Planner tasks (MCP draft_plan) after schema validation

Usage: airk explorer draft-plan [OPTIONS] --output <OUTPUT>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name
          
          [default: {}]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --output <OUTPUT>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk explorer draft-search

```text
Write a proposed Explorer Search form (MCP draft_search) after schema validation

Usage: airk explorer draft-search [OPTIONS] --output <OUTPUT>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name
          
          [default: {}]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --output <OUTPUT>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk explorer draft-strategy

```text
Write a proposed Studio strategy (MCP draft_strategy) after schema validation

Usage: airk explorer draft-strategy [OPTIONS] --output <OUTPUT>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name
          
          [default: {}]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --output <OUTPUT>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk explorer get-handoff

```text
Read a handoff the user shared from a running Explorer window (MCP get_handoff).

Handoff tokens exist only in that window, so this needs its launch descriptor (<VS Code global storage>/iokaio.airk/explorer-assistance/launch-1.json).

Usage: airk explorer get-handoff [OPTIONS] --token <TOKEN> --explorer-descriptor <EXPLORER_DESCRIPTOR> --output <OUTPUT>

Options:
      --root <ROOT>
          [default: .]

      --token <TOKEN>
          The ex-share-… token the user copied from Explorer

      --data-dir <DATA_DIR>
          

      --explorer-descriptor <EXPLORER_DESCRIPTOR>
          

      --collection <COLLECTION>
          [default: default]

      --output <OUTPUT>
          New file for the result (must not exist)

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk explorer get-template

```text
Read one strategy template with its operators (MCP get_template; --args {"template_id":…})

Usage: airk explorer get-template [OPTIONS] --output <OUTPUT>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name
          
          [default: {}]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --output <OUTPUT>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk explorer list-templates

```text
List the bundled strategy templates (MCP list_templates; optional offset, limit, max_bytes)

Usage: airk explorer list-templates [OPTIONS] --output <OUTPUT>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name
          
          [default: {}]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --output <OUTPUT>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk explorer proposal-status

```text
Report the status of a draft file written by this CLI (it is never admitted to an Inbox)

Usage: airk explorer proposal-status [OPTIONS] --draft <DRAFT> --output <OUTPUT>

Options:
      --draft <DRAFT>
          Draft file written by draft-search, draft-strategy, draft-plan or suggest-view

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --output <OUTPUT>
          New file for the result (must not exist)

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk explorer suggest-view

```text
Write a Navigation, Review or Management suggestion (MCP suggest_view; --args {"kind":…,"draft":…})

Usage: airk explorer suggest-view [OPTIONS] --output <OUTPUT>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name
          
          [default: {}]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --output <OUTPUT>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk explorer validate-draft

```text
Validate a draft without writing it as a proposal (MCP validate_draft; --args {"kind":…,"draft":…})

Usage: airk explorer validate-draft [OPTIONS] --output <OUTPUT>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name
          
          [default: {}]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --output <OUTPUT>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk facts

```text
List document facts (requirements, dates, decisions, features) extracted from indexed documents

Usage: airk facts [OPTIONS] [PATH_PREFIX]

Arguments:
  [PATH_PREFIX]
          [default: ""]

Options:
      --compact [<COMPACT>]
          False includes rich facts and durable IDs; independent of response mode
          
          [possible values: true, false]

      --root <ROOT>
          [default: .]

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --document-key <DOCUMENT_KEY>
          Durable document key; excludes textual filters

      --name <NAME>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --kind <KIND>
          Exact document fact category, such as requirement, date, architecture or feature

      --limit <LIMIT>
          [default: 50]

      --service-project <SERVICE_PROJECT>
          

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airk files

```text
List indexed files under --prefix, with continuation cursors

Usage: airk files [OPTIONS] [PREFIX]

Arguments:
  [PREFIX]
          [default: ""]

Options:
      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --limit <LIMIT>
          [default: 10]

      --collection <COLLECTION>
          [default: default]

      --max-bytes <MAX_BYTES>
          [default: 8192]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk host

```text
Run the installed service under an explicit container/user supervisor

Usage: airk host [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk imports

```text
List import and dependency statements found under a path prefix

Usage: airk imports [OPTIONS] [PATH_PREFIX]

Arguments:
  [PATH_PREFIX]
          [default: ""]

Options:
      --compact [<COMPACT>]
          False includes rich facts and durable IDs; independent of response mode
          
          [possible values: true, false]

      --root <ROOT>
          [default: .]

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk index

```text
Queue indexing of the project's sources and print the job receipt. Follow it with `tools call airk_status`.

Full rebuilds (--full) need a guarded maintenance preview through the service instead.

Usage: airk index [OPTIONS]

Options:
      --full
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk info

```text
Show what the index knows about one file: language, size, extraction method and parse status

Usage: airk info [OPTIONS] <PATH>

Arguments:
  <PATH>
          

Options:
      --revision <REVISION>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --if-evidence <IF_EVIDENCE>
          

      --collection <COLLECTION>
          [default: default]

      --follow-moves
          

      --known-hash <KNOWN_HASH>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --artifact-id <ARTIFACT_ID>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk init

```text
Register --root as a project of the installed runtime and prepare its index storage.

For a service you started with `airk serve`, use `airk service register --runtime-dir <control>` instead.

Usage: airk init [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk inspect

```text
Queue a bounded inspection job that refreshes coverage and readiness without reindexing

Usage: airk inspect [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk mcp

```text
Serve the read-only MCP protocol on stdin/stdout for one agent connection (this is what agents launch)

Usage: airk mcp [OPTIONS]

Options:
      --explorer-descriptor <EXPLORER_DESCRIPTOR>
          Optional verified Explorer adapter; native retrieval works without it

      --root <ROOT>
          [default: .]

      --additional-project <ADDITIONAL_PROJECT>
          Additional explicitly configured project bindings in this same service

      --data-dir <DATA_DIR>
          

      --additional-root <ADDITIONAL_ROOT>
          Additional source folders; registrations are resolved on each request

      --collection <COLLECTION>
          [default: default]

      --index-collection <INDEX_COLLECTION>
          Choose a collection only when this source has multiple active indexes

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --no-default-project
          Require a project argument for scoped tools in multi-root workspaces

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk model-catalog

```text
Print the offline model catalog and the CPU-based model choice for this machine; opens no project state

Usage: airk model-catalog [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk outline

```text
Outline one file's structure: symbols, headings and sections with line ranges

Usage: airk outline [OPTIONS] <PATH>

Arguments:
  <PATH>
          

Options:
      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --root <ROOT>
          [default: .]

      --compact [<COMPACT>]
          Return minimal names/kinds/lines/signatures with shared original file/provenance bindings
          
          [possible values: true, false]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --revision <REVISION>
          

      --if-evidence <IF_EVIDENCE>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --follow-moves
          

      --known-hash <KNOWN_HASH>
          

      --service-project <SERVICE_PROJECT>
          

      --artifact-id <ARTIFACT_ID>
          

      --limit <LIMIT>
          [default: 50]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airk parse

```text
Plan, and with --execute run, a multi-step read-only evidence request.

Give a natural-language request as the TEXT argument, or a structured plan with --request FILE ('-' for stdin). Without --execute it only prints the plan; same contract as the MCP `parse` tool. Never writes sources.

Usage: airk parse [OPTIONS] [TEXT]

Arguments:
  [TEXT]
          

Options:
      --project <PROJECT>
          Configured native project binding (alias for global --service-project)

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --execute
          

      --collection <COLLECTION>
          [default: default]

      --request <REQUEST>
          Structured request file; '-' reads bounded JSON from stdin

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk pause

```text
Pause automatic and queued work for the project; existing evidence stays queryable

Usage: airk pause [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk projects

```text
List the projects registered with the service (same fields as the MCP `projects` tool).

Uses the installed runtime, or the service you started when --runtime-dir names its control directory.

Usage: airk projects [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          Control directory of a service started with `airk serve`

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk read

```text
Read an indexed file's lines (--start-line/--end-line) from the published revision, with a citation

Usage: airk read [OPTIONS] --start-line <START_LINE> --end-line <END_LINE> <PATH>

Arguments:
  <PATH>
          

Options:
      --revision <REVISION>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --if-evidence <IF_EVIDENCE>
          

      --collection <COLLECTION>
          [default: default]

      --follow-moves
          

      --known-hash <KNOWN_HASH>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --artifact-id <ARTIFACT_ID>
          

      --service-project <SERVICE_PROJECT>
          

      --start-line <START_LINE>
          

      --end-line <END_LINE>
          

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airk refs

```text
Find indexed references to a name under a path prefix

Usage: airk refs [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --path-prefix <PATH_PREFIX>
          [default: ""]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk relationships

```text
Follow indexed relationships of a symbol in --direction, optionally of one --kind, up to --hops levels

Usage: airk relationships [OPTIONS] <SYMBOL>

Arguments:
  <SYMBOL>
          

Options:
      --direction <DIRECTION>
          [default: incoming]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --kind <KIND>
          

      --collection <COLLECTION>
          [default: default]

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --hops <HOPS>
          [default: 1]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --limit <LIMIT>
          [default: 10]

      --max-bytes <MAX_BYTES>
          [default: 8192]

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk resume

```text
Resume work paused with `pause`

Usage: airk resume [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime

```text
Installed-runtime lifecycle: location, inspect, ensure, start, login, uninstall, inventory, purge and repair.

The VS Code extension uses these; headless experiments can instead run `airk serve` from a local build.

Usage: airk runtime [OPTIONS] <COMMAND>

Commands:
  container-host         Run one externally supervised Linux container service from an approved read-only image pair
  location               Resolve the native user's stable-channel install location; no workspace settings or writes
  login                  Optional login registration; never required by on-demand operation
  foreground             Run the selected service under an explicit external/user startup manager
  stage                  Stage an offline payload with an independently approved install-manifest digest
  abort-layout           Abandon an unfinished state-layout claim; preserve all state directories and their contents
  abort-activation       Restore the preceding selection after an unfinished activation; preserve runtime payloads
  managed                Pin an approved organization-provided payload without copying or modifying it
  prepare-client         Create an owned lightweight client launcher, including for a read-only managed payload
  activate               Drain the selected service and select a staged runtime only after native health succeeds
  inspect-candidate      Verify one immutable approved candidate without selecting or executing it
  ensure                 Launch the selected service on demand, without registering login startup
  start                  Explicitly start a deliberately stopped service; automatic ensure honors that stop
  lifecycle              Stop/restart the observed instance using one bounded durable request read from stdin
  abandon-lifecycle      Abandon one saved lifecycle recovery intent; preserve every running process and keep automatic starts disabled
  inspect                Inspect verified selection without starting a process or opening a project
  inventory              Read-only inventory of AIRK-owned per-user data and recorded project index roots
  purge                  Preview, then with `--confirm <plan_digest>` delete, the contents of selected owned log/cache locations. Project indexes are removed only by unregistering projects
  snapshot               Read saved bootstrap/project metadata without executing or trusting selected payload files
  repair-payload         Preview or explicitly repair the selected payload, preserving the complete damaged slot
  remove                 Remove one unused owned runtime; preserve active/rollback selections and unknown files
  retire-previous        Retire the previous rollback payload after a successful replacement; preserve the active runtime and project data
  abort-stage            Abort an ownership-proven interrupted copy; preserve promoted runtimes and unknown files
  repair-launcher        Repair an absent or known older launcher from the selected approved payload; requires stopped owners
  abort-launcher-repair  Abort an owned unfinished launcher copy; an already replaced image must be finalized
  uninstall              Stop/unregister this service and retire owned runtime payloads; preserve project data
  diagnose               Bounded bootstrap diagnostics work even when runtime selection or the service fails
  reset-startup          Explicitly clear the on-demand crash circuit after diagnosing a startup failure
  rollback               Restore the preceding verified selection using the same drain and health transaction
  launch                 Stable launcher: forward arguments and stdio to the selected verified executable
  self-test              Bounded executable/paired-helper self-test, without collection access or inference
  help                   Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk runtime abandon-lifecycle

```text
Abandon one saved lifecycle recovery intent; preserve every running process and keep automatic starts disabled

Usage: airk runtime abandon-lifecycle [OPTIONS] --install-dir <INSTALL_DIR> --request-id <REQUEST_ID>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --request-id <REQUEST_ID>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime abort-activation

```text
Restore the preceding selection after an unfinished activation; preserve runtime payloads

Usage: airk runtime abort-activation [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --expected-candidate <EXPECTED_CANDIDATE>
          

      --collection <COLLECTION>
          [default: default]

      --expected-active <EXPECTED_ACTIVE>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime abort-launcher-repair

```text
Abort an owned unfinished launcher copy; an already replaced image must be finalized

Usage: airk runtime abort-launcher-repair [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime abort-layout

```text
Abandon an unfinished state-layout claim; preserve all state directories and their contents

Usage: airk runtime abort-layout [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime abort-stage

```text
Abort an ownership-proven interrupted copy; preserve promoted runtimes and unknown files

Usage: airk runtime abort-stage [OPTIONS] --install-dir <INSTALL_DIR> --runtime-sha256 <RUNTIME_SHA256>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --runtime-sha256 <RUNTIME_SHA256>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime activate

```text
Drain the selected service and select a staged runtime only after native health succeeds

Usage: airk runtime activate [OPTIONS] --install-dir <INSTALL_DIR> --runtime-sha256 <RUNTIME_SHA256>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --runtime-sha256 <RUNTIME_SHA256>
          

      --collection <COLLECTION>
          [default: default]

      --expected-active <EXPECTED_ACTIVE>
          Refuse if the reviewed preceding selection changed; exact completed replay does not start a service

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime container-host

```text
Run one externally supervised Linux container service from an approved read-only image pair

Usage: airk runtime container-host [OPTIONS] --source <SOURCE> --approved-sha256 <APPROVED_SHA256>

Options:
      --root <ROOT>
          [default: .]

      --source <SOURCE>
          

      --approved-sha256 <APPROVED_SHA256>
          

      --data-dir <DATA_DIR>
          

      --channel <CHANNEL>
          [default: stable]
          [possible values: stable, preview]

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime diagnose

```text
Bounded bootstrap diagnostics work even when runtime selection or the service fails

Usage: airk runtime diagnose [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime ensure

```text
Launch the selected service on demand, without registering login startup

Usage: airk runtime ensure [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime foreground

```text
Run the selected service under an explicit external/user startup manager

Usage: airk runtime foreground [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime inspect

```text
Inspect verified selection without starting a process or opening a project

Usage: airk runtime inspect [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime inspect-candidate

```text
Verify one immutable approved candidate without selecting or executing it

Usage: airk runtime inspect-candidate [OPTIONS] --install-dir <INSTALL_DIR> --runtime-sha256 <RUNTIME_SHA256>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --runtime-sha256 <RUNTIME_SHA256>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime inventory

```text
Read-only inventory of AIRK-owned per-user data and recorded project index roots

Usage: airk runtime inventory [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime launch

```text
Stable launcher: forward arguments and stdio to the selected verified executable

Usage: airk runtime launch [OPTIONS] --install-dir <INSTALL_DIR> -- <ARGS>...

Arguments:
  <ARGS>...
          

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime lifecycle

```text
Stop/restart the observed instance using one bounded durable request read from stdin

Usage: airk runtime lifecycle [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime location

```text
Resolve the native user's stable-channel install location; no workspace settings or writes

Usage: airk runtime location [OPTIONS]

Options:
      --channel <CHANNEL>
          [default: stable]
          [possible values: stable, preview]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime login

```text
Optional login registration; never required by on-demand operation

Usage: airk runtime login [OPTIONS] --install-dir <INSTALL_DIR> <COMMAND>

Commands:
  plan       Show what login startup registration would create; changes nothing
  status     Report whether login startup is registered, loaded and running
  install    Register the service to start at login for this user
  start      Start the registered login service now
  stop       Stop the registered login service
  uninstall  Remove the login startup registration
  help       Print this message or the help of the given subcommand(s)

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime login install

```text
Register the service to start at login for this user

Usage: airk runtime login --install-dir <INSTALL_DIR> install [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime login plan

```text
Show what login startup registration would create; changes nothing

Usage: airk runtime login --install-dir <INSTALL_DIR> plan [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime login start

```text
Start the registered login service now

Usage: airk runtime login --install-dir <INSTALL_DIR> start [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime login status

```text
Report whether login startup is registered, loaded and running

Usage: airk runtime login --install-dir <INSTALL_DIR> status [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime login stop

```text
Stop the registered login service

Usage: airk runtime login --install-dir <INSTALL_DIR> stop [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime login uninstall

```text
Remove the login startup registration

Usage: airk runtime login --install-dir <INSTALL_DIR> uninstall [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime managed

```text
Pin an approved organization-provided payload without copying or modifying it

Usage: airk runtime managed [OPTIONS] --source <SOURCE> --install-dir <INSTALL_DIR> --approved-sha256 <APPROVED_SHA256> --verification-basis <VERIFICATION_BASIS>

Options:
      --root <ROOT>
          [default: .]

      --source <SOURCE>
          

      --data-dir <DATA_DIR>
          

      --install-dir <INSTALL_DIR>
          

      --approved-sha256 <APPROVED_SHA256>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --verification-basis <VERIFICATION_BASIS>
          [possible values: trusted-distribution, organization-digest]

      --lifecycle <LIFECYCLE>
          [default: external]
          [possible values: external, user-on-demand]

      --service-project <SERVICE_PROJECT>
          

      --state-dir <STATE_DIR>
          

      --channel <CHANNEL>
          [default: stable]
          [possible values: stable, preview]

  -h, --help
          Print help
```

## airk runtime prepare-client

```text
Create an owned lightweight client launcher, including for a read-only managed payload

Usage: airk runtime prepare-client [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime purge

```text
Preview, then with `--confirm <plan_digest>` delete, the contents of selected owned log/cache locations. Project indexes are removed only by unregistering projects

Usage: airk runtime purge [OPTIONS] --install-dir <INSTALL_DIR> --select <SELECT>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --select <SELECT>
          

      --collection <COLLECTION>
          [default: default]

      --confirm <CONFIRM>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime remove

```text
Remove one unused owned runtime; preserve active/rollback selections and unknown files

Usage: airk runtime remove [OPTIONS] --install-dir <INSTALL_DIR> --runtime-sha256 <RUNTIME_SHA256>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --runtime-sha256 <RUNTIME_SHA256>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime repair-launcher

```text
Repair an absent or known older launcher from the selected approved payload; requires stopped owners

Usage: airk runtime repair-launcher [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime repair-payload

```text
Preview or explicitly repair the selected payload, preserving the complete damaged slot

Usage: airk runtime repair-payload [OPTIONS] --install-dir <INSTALL_DIR> --source <SOURCE> --approved-sha256 <APPROVED_SHA256>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --source <SOURCE>
          

      --approved-sha256 <APPROVED_SHA256>
          

      --collection <COLLECTION>
          [default: default]

      --preview <PREVIEW>
          Omit for read-only preview; supply the reviewed preview digest to apply

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime reset-startup

```text
Explicitly clear the on-demand crash circuit after diagnosing a startup failure

Usage: airk runtime reset-startup [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime retire-previous

```text
Retire the previous rollback payload after a successful replacement; preserve the active runtime and project data

Usage: airk runtime retire-previous [OPTIONS] --install-dir <INSTALL_DIR> --expected-active <EXPECTED_ACTIVE>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --expected-active <EXPECTED_ACTIVE>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime rollback

```text
Restore the preceding verified selection using the same drain and health transaction

Usage: airk runtime rollback [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime self-test

```text
Bounded executable/paired-helper self-test, without collection access or inference

Usage: airk runtime self-test [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime snapshot

```text
Read saved bootstrap/project metadata without executing or trusting selected payload files

Usage: airk runtime snapshot [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --after <AFTER>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --revision <REVISION>
          

      --limit <LIMIT>
          [default: 16]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime stage

```text
Stage an offline payload with an independently approved install-manifest digest

Usage: airk runtime stage [OPTIONS] --source <SOURCE> --install-dir <INSTALL_DIR> --approved-sha256 <APPROVED_SHA256> --verification-basis <VERIFICATION_BASIS>

Options:
      --root <ROOT>
          [default: .]

      --source <SOURCE>
          

      --data-dir <DATA_DIR>
          

      --install-dir <INSTALL_DIR>
          

      --approved-sha256 <APPROVED_SHA256>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --verification-basis <VERIFICATION_BASIS>
          [possible values: trusted-distribution, organization-digest]

      --state-dir <STATE_DIR>
          Writable service state; defaults to native state for a native install, otherwise this install root

      --channel <CHANNEL>
          [default: stable]
          [possible values: stable, preview]

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime start

```text
Explicitly start a deliberately stopped service; automatic ensure honors that stop

Usage: airk runtime start [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk runtime uninstall

```text
Stop/unregister this service and retire owned runtime payloads; preserve project data

Usage: airk runtime uninstall [OPTIONS] --install-dir <INSTALL_DIR>

Options:
      --install-dir <INSTALL_DIR>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk search

```text
Search indexed evidence; results carry file, line and revision citations.

Without --mode the project's search policy applies, which uses vectors when they are enabled; that fails until the embedding model is set up (`setup-model`). --mode lexical (all words must appear) always works on a published index; vector and hybrid need ready vectors.

Usage: airk search [OPTIONS] <QUERY>

Arguments:
  <QUERY>
          

Options:
      --mode <MODE>
          [possible values: lexical, vector, hybrid]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --semantic
          Use the configured local embedding model and disk-backed vector graph

      --collection <COLLECTION>
          [default: default]

      --path-prefix <PATH_PREFIX>
          

      --limit <LIMIT>
          [default: 10]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --max-bytes <MAX_BYTES>
          [default: 8192]

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk serve

```text
Run the AIRK service in the foreground for a private control directory (created if missing).

It owns the project registry, job queue and worker processes. Stop it with `airk service shutdown`.

Usage: airk serve [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --idle-exit-seconds <IDLE_EXIT_SECONDS>
          On-demand grace after all clients/workers and runnable jobs are gone

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk service

```text
Bounded native metadata and request recovery; never a project writer

Usage: airk service [OPTIONS] <COMMAND>

Commands:
  legacy                 Review and retire pre-1.4 startup entries and derived state (explicit, previewed)
  requests               Prepare, list, retry or forget durable client requests (jobs and configuration) for a control directory
  prepare-registration   Print the canonical registration for --root/--data-dir/--collection, including its binding. Read-only
  prepare-storage        Create the reserved index storage for an already reserved project (owner-only on Unix)
  prepare-configuration  Prepare a guarded configuration request for a project; submit it with `requests retry`
  console                Persistent JSON management transport on stdin/stdout (used by the VS Code extension)
  logs                   Print recent service log records (--limit), export all retained logs (--all) or the raw job-error ring (--raw)
  record-error           Record a bounded local extension/setup error without starting the service
  register               Register --root/--data-dir/--collection with the service at --runtime-dir. The data directory must exist
  rpc                    Send one raw management request (JSON from --request-file or --stdin) and print the response
  health                 Check that the service for --runtime-dir is running and healthy; exits non-zero otherwise
  shutdown               Ask the service for --runtime-dir to drain its workers and exit
  help                   Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service console

```text
Persistent JSON management transport on stdin/stdout (used by the VS Code extension)

Usage: airk service console [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --runtime-sha256 <RUNTIME_SHA256>
          Pin the approved complete payload identity supplied by the trusted launcher

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service health

```text
Check that the service for --runtime-dir is running and healthy; exits non-zero otherwise

Usage: airk service health [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy

```text
Review and retire pre-1.4 startup entries and derived state (explicit, previewed)

Usage: airk service legacy [OPTIONS] <COMMAND>

Commands:
  derived-receipts  List captured old-derived cleanup scopes for explicit recovery
  forget-derived    Forget one approved old-derived cleanup scope after review
  preview-derived   Preview known owned 1.3 derived files without opening an old database
  remove-derived    Delete one bounded turn of explicitly reviewed old derived files
  review            Read saved ownership and OS state; do not start, stop or convert anything
  remove-startup    Remove only reviewed old login startup; preserve sources/settings/indexes
  list              List bounded saved removal identities/phases, including interrupted work
  show              Show one saved removal identity and its phase
  forget            Forget only a completed local receipt; never stop a process or repeat work
  help              Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy derived-receipts

```text
List captured old-derived cleanup scopes for explicit recovery

Usage: airk service legacy derived-receipts [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy forget

```text
Forget only a completed local receipt; never stop a process or repeat work

Usage: airk service legacy forget [OPTIONS] --runtime-dir <RUNTIME_DIR> --approval <APPROVAL>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --approval <APPROVAL>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy forget-derived

```text
Forget one approved old-derived cleanup scope after review

Usage: airk service legacy forget-derived [OPTIONS] --runtime-dir <RUNTIME_DIR> --approval <APPROVAL>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --approval <APPROVAL>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy list

```text
List bounded saved removal identities/phases, including interrupted work

Usage: airk service legacy list [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy preview-derived

```text
Preview known owned 1.3 derived files without opening an old database

Usage: airk service legacy preview-derived [OPTIONS] --runtime-dir <RUNTIME_DIR> --root <ROOT> --data-dir <DATA_DIR> --collection <COLLECTION>

Options:
      --runtime-dir <RUNTIME_DIR>
          

      --root <ROOT>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy remove-derived

```text
Delete one bounded turn of explicitly reviewed old derived files

Usage: airk service legacy remove-derived [OPTIONS] --runtime-dir <RUNTIME_DIR> --root <ROOT> --data-dir <DATA_DIR> --collection <COLLECTION> --approval <APPROVAL>

Options:
      --runtime-dir <RUNTIME_DIR>
          

      --root <ROOT>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --approval <APPROVAL>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy remove-startup

```text
Remove only reviewed old login startup; preserve sources/settings/indexes

Usage: airk service legacy remove-startup [OPTIONS] --runtime-dir <RUNTIME_DIR> --root <ROOT> --data-dir <DATA_DIR> --collection <COLLECTION> --executable <EXECUTABLE> --approval <APPROVAL>

Options:
      --runtime-dir <RUNTIME_DIR>
          

      --root <ROOT>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --executable <EXECUTABLE>
          

      --approval <APPROVAL>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy review

```text
Read saved ownership and OS state; do not start, stop or convert anything

Usage: airk service legacy review [OPTIONS] --runtime-dir <RUNTIME_DIR> --root <ROOT> --data-dir <DATA_DIR> --collection <COLLECTION> --executable <EXECUTABLE>

Options:
      --runtime-dir <RUNTIME_DIR>
          

      --root <ROOT>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --executable <EXECUTABLE>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service legacy show

```text
Show one saved removal identity and its phase

Usage: airk service legacy show [OPTIONS] --runtime-dir <RUNTIME_DIR> --approval <APPROVAL>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --approval <APPROVAL>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service logs

```text
Print recent service log records (--limit), export all retained logs (--all) or the raw job-error ring (--raw)

Usage: airk service logs [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --limit <LIMIT>
          [default: 32]

      --all
          Export retained logs locally (200 MB high-water, FIFO purge to 100 MB)

      --collection <COLLECTION>
          [default: default]

      --raw
          Open the separate local raw job-error ring (may contain paths/error values)

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --output <OUTPUT>
          Capture raw retained diagnostics into a new local file without buffering stdout

      --format <FORMAT>
          [default: json]
          [possible values: json, text]

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service prepare-configuration

```text
Prepare a guarded configuration request for a project; submit it with `requests retry`

Usage: airk service prepare-configuration [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --project <PROJECT>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service prepare-registration

```text
Print the canonical registration for --root/--data-dir/--collection, including its binding. Read-only

Usage: airk service prepare-registration [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service prepare-storage

```text
Create the reserved index storage for an already reserved project (owner-only on Unix)

Usage: airk service prepare-storage [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --project <PROJECT>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service record-error

```text
Record a bounded local extension/setup error without starting the service

Usage: airk service record-error [OPTIONS] --runtime-dir <RUNTIME_DIR> --operation <OPERATION>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --operation <OPERATION>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service register

```text
Register --root/--data-dir/--collection with the service at --runtime-dir. The data directory must exist

Usage: airk service register [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service requests

```text
Prepare, list, retry or forget durable client requests (jobs and configuration) for a control directory

Usage: airk service requests [OPTIONS] --runtime-dir <RUNTIME_DIR> <COMMAND>

Commands:
  prepare-configuration  Save a configuration request for a project from a draft --file, without submitting it
  prepare                Save a job request (--kind index, inspect, reconcile, vectors, model_setup, verify, recover or maintain)
  list                   List saved client requests and their outcomes
  retry                  Submit, or resubmit after an unknown outcome, a saved request by its nonce
  forget                 Discard a saved request record by its nonce
  help                   Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service requests forget

```text
Discard a saved request record by its nonce

Usage: airk service requests --runtime-dir <RUNTIME_DIR> forget [OPTIONS] <NONCE>

Arguments:
  <NONCE>
          

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service requests list

```text
List saved client requests and their outcomes

Usage: airk service requests --runtime-dir <RUNTIME_DIR> list [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service requests prepare

```text
Save a job request (--kind index, inspect, reconcile, vectors, model_setup, verify, recover or maintain)

Usage: airk service requests --runtime-dir <RUNTIME_DIR> prepare [OPTIONS] --project <PROJECT>

Options:
      --project <PROJECT>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --kind <KIND>
          [default: index]
          [possible values: index, inspect, reconcile, vectors, model_setup, verify, recover, maintain]

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service requests prepare-configuration

```text
Save a configuration request for a project from a draft --file, without submitting it

Usage: airk service requests --runtime-dir <RUNTIME_DIR> prepare-configuration [OPTIONS] --project <PROJECT> --file <FILE>

Options:
      --project <PROJECT>
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --file <FILE>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service requests retry

```text
Submit, or resubmit after an unknown outcome, a saved request by its nonce

Usage: airk service requests --runtime-dir <RUNTIME_DIR> retry [OPTIONS] <NONCE>

Arguments:
  <NONCE>
          

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service rpc

```text
Send one raw management request (JSON from --request-file or --stdin) and print the response

Usage: airk service rpc [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --request-file <REQUEST_FILE>
          

      --collection <COLLECTION>
          [default: default]

      --stdin
          Read bounded management JSON from stdin instead of a request file

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk service shutdown

```text
Ask the service for --runtime-dir to drain its workers and exit

Usage: airk service shutdown [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --root <ROOT>
          [default: .]

      --runtime-dir <RUNTIME_DIR>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk settings

```text
Read one project setting view; read-only.

Views: extraction_policy, vector_selection, vector_status, roots, scan_limits, quota_policy and quota_status. --validate-file checks a configuration draft without applying it.

Usage: airk settings [OPTIONS] <VIEW>

Arguments:
  <VIEW>
          [possible values: extraction_policy, vector_selection, vector_status, roots, scan_limits, quota_policy, quota_status]

Options:
      --root <ROOT>
          [default: .]

      --validate-file <VALIDATE_FILE>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk setup

```text
Print the MCP client configuration (command and arguments) for an already registered project

Usage: airk setup [OPTIONS]

Options:
      --client <CLIENT>
          [default: claude]
          [possible values: claude, codex, vscode, cursor, gemini, cline, roo, junie, copilot-cli, opencode, opencode-v2, kilo]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk setup-model

```text
Prepare the bundled offline embedding model for vector search; keeps an existing or custom selection

Usage: airk setup-model [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk status

```text
Print the project's publication, freshness, coverage and job status

Usage: airk status [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk symbol

```text
Find symbol definitions by name, optionally filtered by language or path prefix

Usage: airk symbol [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --compact [<COMPACT>]
          False includes rich facts and durable IDs; independent of response mode
          
          [possible values: true, false]

      --root <ROOT>
          [default: .]

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --language <LANGUAGE>
          

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk tools

```text
List, describe or call the read-only MCP tools from the terminal.

`tools list --json`, `tools help <tool>` and `tools call <tool> --args '<json>'`. Start here to learn every query an agent can make.

Usage: airk tools [OPTIONS] <COMMAND>

Commands:
  list   List the same tools advertised to MCP clients; --json includes their schemas
  help   Show a tool's description, input schema, defaults and argument template
  call   Invoke an MCP tool and print its unchanged result envelope as JSON
  shell  Open a collection-bound interactive console with help and examples

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help (see a summary with '-h')
```

## airk tools call

```text
Invoke an MCP tool and print its unchanged result envelope as JSON

Usage: airk tools call [OPTIONS] <TOOL>

Arguments:
  <TOOL>
          

Options:
      --args <ARGS>
          JSON object; omit for {}. Maximum 64 KiB

      --root <ROOT>
          [default: .]

      --args-file <ARGS_FILE>
          Read a UTF-8 JSON object from a regular file (maximum 64 KiB)

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help

Examples (add --root, --data-dir and --collection for your workspace):
  airk tools call airk_status
  airk tools call airk_search --args-file query.json
Use 'airk tools help airk_search' for the JSON format. Tool errors exit nonzero.
```

## airk tools list

```text
List the same tools advertised to MCP clients; --json includes their schemas

Usage: airk tools list [OPTIONS]

Options:
      --json
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk tools shell

```text
Open a collection-bound interactive console with help and examples

Usage: airk tools shell [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk usage

```text
Local usage reporting for the selected project: report, policy, configure (mode), clear, feedback, import, calibrate

Usage: airk usage [OPTIONS] <COMMAND>

Commands:
  report     Summarize recorded usage for the last --days (json, csv or markdown; optional price scenario file)
  policy     Show the project's usage recording mode and retention
  clear      Delete recorded usage for the project
  configure  Set the recording mode: disabled, aggregates_only or diagnostic_events
  feedback   Record whether one returned request was useful, partly useful or not useful
  import     Import a usage export; --preview shows what would change and --if-sha256 pins the file
  calibrate  Calibrate token estimates against a tokenizer and baseline/returned samples (advanced)
  help       Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk usage calibrate

```text
Calibrate token estimates against a tokenizer and baseline/returned samples (advanced)

Usage: airk usage calibrate [OPTIONS] --tokenizer <TOKENIZER> --baseline <BASELINE> --returned <RETURNED> --generation <GENERATION> --content-kind <CONTENT_KIND>

Options:
      --root <ROOT>
          [default: .]

      --tokenizer <TOKENIZER>
          

      --baseline <BASELINE>
          

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --returned <RETURNED>
          

      --generation <GENERATION>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --content-kind <CONTENT_KIND>
          

      --record
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk usage clear

```text
Delete recorded usage for the project

Usage: airk usage clear [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk usage configure

```text
Set the recording mode: disabled, aggregates_only or diagnostic_events

Usage: airk usage configure [OPTIONS] --mode <MODE>

Options:
      --mode <MODE>
          [possible values: disabled, aggregates_only, diagnostic_events]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk usage feedback

```text
Record whether one returned request was useful, partly useful or not useful

Usage: airk usage feedback [OPTIONS] --usefulness <USEFULNESS> <REQUEST_ID>

Arguments:
  <REQUEST_ID>
          

Options:
      --root <ROOT>
          [default: .]

      --usefulness <USEFULNESS>
          [possible values: useful, partly_useful, not_useful, partly-useful, not-useful]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk usage import

```text
Import a usage export; --preview shows what would change and --if-sha256 pins the file

Usage: airk usage import [OPTIONS] <FILE>

Arguments:
  <FILE>
          

Options:
      --preview
          

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --if-sha256 <IF_SHA256>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk usage policy

```text
Show the project's usage recording mode and retention

Usage: airk usage policy [OPTIONS]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk usage report

```text
Summarize recorded usage for the last --days (json, csv or markdown; optional price scenario file)

Usage: airk usage report [OPTIONS]

Options:
      --days <DAYS>
          [default: 7]

      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --format <FORMAT>
          [default: json]
          [possible values: json, csv, markdown]

      --collection <COLLECTION>
          [default: default]

      --price-scenario <PRICE_SCENARIO>
          

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```

## airk watch

```text
Turn native source watching on or off for the project (on keeps the index current after edits)

Usage: airk watch [OPTIONS] <MODE>

Arguments:
  <MODE>
          [possible values: on, off]

Options:
      --root <ROOT>
          [default: .]

      --data-dir <DATA_DIR>
          

      --collection <COLLECTION>
          [default: default]

      --service-install-dir <SERVICE_INSTALL_DIR>
          

      --service-project <SERVICE_PROJECT>
          

  -h, --help
          Print help
```
