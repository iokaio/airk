# AIRK CLI reference

<!-- Generated from airkctl's parser by `cargo test -p airkctl --lib cli_reference`; do not edit. Set AIRK_WRITE_CLI_REFERENCE=1 to regenerate. -->

Every public `airkctl` command and subcommand, exactly as `airkctl <command> --help` prints it. `airk` itself is the service and takes no commands. For concepts and workflows, see [Running AIRK headless](headless-cli.md) and [the CLI guide](cli.md).

## Contents

- [`airkctl`](#airkctl)
- [`airkctl admin`](#airkctl-admin)
- [`airkctl admin open`](#airkctl-admin-open)
- [`airkctl admin url`](#airkctl-admin-url)
- [`airkctl agents`](#airkctl-agents)
- [`airkctl agents connect`](#airkctl-agents-connect)
- [`airkctl agents credential`](#airkctl-agents-credential)
- [`airkctl agents default`](#airkctl-agents-default)
- [`airkctl agents endpoint`](#airkctl-agents-endpoint)
- [`airkctl agents info`](#airkctl-agents-info)
- [`airkctl agents launch`](#airkctl-agents-launch)
- [`airkctl agents list`](#airkctl-agents-list)
- [`airkctl agents remove`](#airkctl-agents-remove)
- [`airkctl agents repair`](#airkctl-agents-repair)
- [`airkctl agents revoke`](#airkctl-agents-revoke)
- [`airkctl agents verify`](#airkctl-agents-verify)
- [`airkctl changes`](#airkctl-changes)
- [`airkctl code-facts`](#airkctl-code-facts)
- [`airkctl configure`](#airkctl-configure)
- [`airkctl context`](#airkctl-context)
- [`airkctl explorer`](#airkctl-explorer)
- [`airkctl explorer draft-plan`](#airkctl-explorer-draft-plan)
- [`airkctl explorer draft-search`](#airkctl-explorer-draft-search)
- [`airkctl explorer draft-strategy`](#airkctl-explorer-draft-strategy)
- [`airkctl explorer get-handoff`](#airkctl-explorer-get-handoff)
- [`airkctl explorer get-template`](#airkctl-explorer-get-template)
- [`airkctl explorer list-templates`](#airkctl-explorer-list-templates)
- [`airkctl explorer proposal-status`](#airkctl-explorer-proposal-status)
- [`airkctl explorer suggest-view`](#airkctl-explorer-suggest-view)
- [`airkctl explorer validate-draft`](#airkctl-explorer-validate-draft)
- [`airkctl facts`](#airkctl-facts)
- [`airkctl files`](#airkctl-files)
- [`airkctl folders`](#airkctl-folders)
- [`airkctl imports`](#airkctl-imports)
- [`airkctl index`](#airkctl-index)
- [`airkctl info`](#airkctl-info)
- [`airkctl init`](#airkctl-init)
- [`airkctl inspect`](#airkctl-inspect)
- [`airkctl jobs`](#airkctl-jobs)
- [`airkctl jobs cancel`](#airkctl-jobs-cancel)
- [`airkctl jobs get`](#airkctl-jobs-get)
- [`airkctl jobs retry`](#airkctl-jobs-retry)
- [`airkctl library`](#airkctl-library)
- [`airkctl library change`](#airkctl-library-change)
- [`airkctl library describe`](#airkctl-library-describe)
- [`airkctl library list`](#airkctl-library-list)
- [`airkctl library rename`](#airkctl-library-rename)
- [`airkctl library retire`](#airkctl-library-retire)
- [`airkctl library set-create`](#airkctl-library-set-create)
- [`airkctl library set-delete`](#airkctl-library-set-delete)
- [`airkctl library set-rename`](#airkctl-library-set-rename)
- [`airkctl library setup`](#airkctl-library-setup)
- [`airkctl library setup-apply`](#airkctl-library-setup-apply)
- [`airkctl library show`](#airkctl-library-show)
- [`airkctl model-catalog`](#airkctl-model-catalog)
- [`airkctl outline`](#airkctl-outline)
- [`airkctl parse`](#airkctl-parse)
- [`airkctl pause`](#airkctl-pause)
- [`airkctl progress`](#airkctl-progress)
- [`airkctl projects`](#airkctl-projects)
- [`airkctl read`](#airkctl-read)
- [`airkctl refs`](#airkctl-refs)
- [`airkctl relationships`](#airkctl-relationships)
- [`airkctl resume`](#airkctl-resume)
- [`airkctl search`](#airkctl-search)
- [`airkctl service`](#airkctl-service)
- [`airkctl service abandon-lifecycle`](#airkctl-service-abandon-lifecycle)
- [`airkctl service health`](#airkctl-service-health)
- [`airkctl service inventory`](#airkctl-service-inventory)
- [`airkctl service issuer-rotation`](#airkctl-service-issuer-rotation)
- [`airkctl service login`](#airkctl-service-login)
- [`airkctl service logs`](#airkctl-service-logs)
- [`airkctl service prepare-configuration`](#airkctl-service-prepare-configuration)
- [`airkctl service prepare-registration`](#airkctl-service-prepare-registration)
- [`airkctl service prepare-storage`](#airkctl-service-prepare-storage)
- [`airkctl service purge`](#airkctl-service-purge)
- [`airkctl service purge-plan`](#airkctl-service-purge-plan)
- [`airkctl service record-error`](#airkctl-service-record-error)
- [`airkctl service register`](#airkctl-service-register)
- [`airkctl service requests`](#airkctl-service-requests)
- [`airkctl service requests forget`](#airkctl-service-requests-forget)
- [`airkctl service requests list`](#airkctl-service-requests-list)
- [`airkctl service requests prepare`](#airkctl-service-requests-prepare)
- [`airkctl service requests prepare-configuration`](#airkctl-service-requests-prepare-configuration)
- [`airkctl service requests retry`](#airkctl-service-requests-retry)
- [`airkctl service restart`](#airkctl-service-restart)
- [`airkctl service rpc`](#airkctl-service-rpc)
- [`airkctl service shutdown`](#airkctl-service-shutdown)
- [`airkctl service start`](#airkctl-service-start)
- [`airkctl service status`](#airkctl-service-status)
- [`airkctl service stop`](#airkctl-service-stop)
- [`airkctl service trust`](#airkctl-service-trust)
- [`airkctl service uninstall`](#airkctl-service-uninstall)
- [`airkctl session`](#airkctl-session)
- [`airkctl settings`](#airkctl-settings)
- [`airkctl setup-model`](#airkctl-setup-model)
- [`airkctl status`](#airkctl-status)
- [`airkctl symbol`](#airkctl-symbol)
- [`airkctl tools`](#airkctl-tools)
- [`airkctl tools call`](#airkctl-tools-call)
- [`airkctl tools help`](#airkctl-tools-help)
- [`airkctl tools list`](#airkctl-tools-list)
- [`airkctl tools shell`](#airkctl-tools-shell)
- [`airkctl usage`](#airkctl-usage)
- [`airkctl usage calibrate`](#airkctl-usage-calibrate)
- [`airkctl usage clear`](#airkctl-usage-clear)
- [`airkctl usage configure`](#airkctl-usage-configure)
- [`airkctl usage feedback`](#airkctl-usage-feedback)
- [`airkctl usage import`](#airkctl-usage-import)
- [`airkctl usage policy`](#airkctl-usage-policy)
- [`airkctl usage report`](#airkctl-usage-report)
- [`airkctl version`](#airkctl-version)
- [`airkctl watch`](#airkctl-watch)

## airkctl

```text
AIRK command-line client: reaches the AIRK service through its local control endpoint

Usage: airkctl [OPTIONS] <COMMAND>

Commands:
  version        Print the version document: airkctl, its contracts and the installation
  tools          The unified tools: list, help and call
  init           Acknowledge an existing reviewed index mapping and prepare its reserved storage
  model-catalog  Print the offline model catalog and the CPU choice for this host
  session        Serve one editor window's framed session (protocol 2) on stdin/stdout
  agents         Agent connections and configuration: endpoint, info, list, connect, repair, remove, verify, credential, launch and revoke
  admin          AIRK Admin in the browser: open it, or print a single-use sign-in URL
  explorer       Offline Explorer drafts, validation and templates; results go to a new --output-file
  settings       Read one project setting view; read-only
  configure      Apply a guarded project configuration draft from --file or --stdin (JSON with an "operation")
  usage          Local usage reporting for the selected project: report, policy, configure (mode), clear, feedback, import, calibrate
  index          Queue indexing for the project (full rebuilds need a reviewed maintenance preview)
  setup-model    Queue the explicit model setup job for the project
  inspect        Queue an inspection of the project's configuration and index
  pause          Pause work for the project
  resume         Resume work paused with `pause`
  watch          Turn native source watching on or off for the project (on keeps the index current after edits), or print its watch status
  progress       Print the project's live indexing progress: the running job's phase and counts, pending and queued jobs, and pause state
  jobs           Read, retry or cancel one job by its ID
  library        The index library: list, show and set up indexes (the admin pages' Index library)
  folders        List the folders in PATH (folders only, links never followed), or the starting list without one (M24: AIRK Admin's Browse dialog)
  projects       List the projects registered with the service (same fields as the MCP `projects` tool)
  service        The AIRK service: start, stop, status, logs, requests, trust and management
  parse          Plan, and with --execute run, a multi-step read-only evidence request
  changes        Report indexed changes since a revision token: added, modified and removed files, symbols and documents
  code-facts     Inspect code facts and signatures with exact default name matching and bounded continuation
  relationships  Follow indexed relationships of a symbol in --direction, optionally of one --kind, up to --hops levels
  status         Print the project's publication, freshness, coverage and job status
  search         Search indexed evidence; results carry file, line and revision citations
  files          List indexed files under an optional path prefix, with continuation cursors
  read           Read an indexed file's lines (--start-line/--end-line) from the published revision, with a citation
  info           Show what the index knows about one file: language, size, extraction method and parse status
  outline        Read one indexed file's structure in file order; this returns evidence, not an Explorer page proposal
  symbol         Find definitions by exact name by default; use --name-match for suffix/contains variants
  imports        List import and dependency statements found under a path prefix
  facts          List document facts (requirements, dates, decisions, features) extracted from indexed documents
  refs           Find exact-case textual identifier uses; these are candidates, not resolved language references
  context        Read lines around --line in a file (--before/--after), with a citation

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help

  -V, --version
          Print version
```

## airkctl admin

```text
AIRK Admin in the browser: open it, or print a single-use sign-in URL

Usage: airkctl admin [OPTIONS] <COMMAND>

Commands:
  open  Open AIRK Admin in this computer's default browser, signed in
  url   Print a single-use AIRK Admin sign-in URL (valid 60 seconds) for manual use

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl admin open

```text
Open AIRK Admin in this computer's default browser, signed in.

Needs a local graphical session (refused over SSH). The single-use sign-in link goes through an owner-only file, never a command line.

Usage: airkctl admin open [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --page <PAGE>
          The page to open

          [possible values: overview, index-library, activity, agents, storage, recovery, service, logs, usage, search, help]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --for-folder <FOR_FOLDER>
          Index library: prefill a new index for this folder

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --show-index <SHOW_INDEX>
          Index library: show this index (`idx_…`)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --show-set <SHOW_SET>
          Index library: show this index set (`prj_…`)

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl admin url

```text
Print a single-use AIRK Admin sign-in URL (valid 60 seconds) for manual use.

The URL is a secret: open it only in your own browser and never paste it into a page.

Usage: airkctl admin url [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --page <PAGE>
          The page to open

          [possible values: overview, index-library, activity, agents, storage, recovery, service, logs, usage, search, help]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --for-folder <FOR_FOLDER>
          Index library: prefill a new index for this folder

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --show-index <SHOW_INDEX>
          Index library: show this index (`idx_…`)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --show-set <SHOW_SET>
          Index library: show this index set (`prj_…`)

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl agents

```text
Agent connections and configuration: endpoint, info, list, connect, repair, remove, verify, credential, launch and revoke

Usage: airkctl agents [OPTIONS] <COMMAND>

Commands:
  endpoint    Read the owner-authenticated local HTTPS endpoint without enrolling a client
  info        Read an existing connection's verified references without issuing credentials
  credential  Print an existing, unrevoked connection's bearer header for an agent's header helper
  list        Classify AIRK's entries in agent configuration files; read-only
  connect     Write an AIRK entry for Claude Code, Codex or VS Code, enrolling a reviewed selection (--selection-file with a new --connection-id), a new open connection (--open) or reusing an existing connection
  repair      Rewrite AIRK entries whose endpoint or helper is stale; --connection-id replaces an unknown connection
  remove      Remove AIRK's entries (only those naming --connection-id, when given)
  verify      Probe the service's own MCP endpoint with a connection's stored credential
  launch      Start Claude Code, Codex or VS Code on this connection in the foreground (U91)
  default     Set an open connection's default scope and agent choice; the agent's next call uses it
  revoke      Revoke one local HTTPS connection while preserving its original receipt

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl agents connect

```text
Write an AIRK entry for Claude Code, Codex or VS Code, enrolling a reviewed selection (--selection-file with a new --connection-id), a new open connection (--open) or reusing an existing connection.

An open connection reads its default scope (`agents default` or the Explorer sets it); with agent choice on, the agent may name indexes and index sets per call.

Usage: airkctl agents connect [OPTIONS] --client <CLIENT>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --client <CLIENT>
          [possible values: claude, codex, vscode]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --connection-id <CONNECTION_ID>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --open
          Enroll a new open connection with a new id: no reviewed grant, agent choice off and an empty default scope until `agents default` sets one

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --selection-file <SELECTION_FILE>
          Owner-private reviewed selection to enroll as --connection-id

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --user
          Write the user-level file instead of the project's

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --project <PROJECT>
          The project folder (default: the current directory)

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --server-key <SERVER_KEY>
          [default: airk]

      --airkctl <AIRKCTL>
          The airkctl executable written credential helpers name (default: this airkctl)

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl agents credential

```text
Print an existing, unrevoked connection's bearer header for an agent's header helper.

Offline: reads only the owner-private credential files; never starts the service, enrolls, rotates or logs the token. `--format header-json` prints {"Authorization":"Bearer <token>"}.

Usage: airkctl agents credential [OPTIONS] --connection-id <CONNECTION_ID> --format <FORMAT>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --connection-id <CONNECTION_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --format <FORMAT>
          [possible values: header-json]

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl agents default

```text
Set an open connection's default scope and agent choice; the agent's next call uses it.

--index and --set take ids or exact names and replace the whole default scope; --clear empties it. Without --expected-revision the current defaults revision is read first. Prints {"schema":1,"connectionId":…,"open":{…}}.

Usage: airkctl agents default [OPTIONS] --connection-id <CONNECTION_ID>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --connection-id <CONNECTION_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --index <INDEXES>
          An index (`idx_…` or its exact name) in the default scope (repeatable)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --set <SETS>
          An index set (`prj_…` or its exact name) in the default scope (repeatable)

      --clear
          Empty the default scope

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --agent-choice <AGENT_CHOICE>
          Whether the agent may name indexes and sets itself and list them

          [possible values: off, on]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --expected-revision <EXPECTED_REVISION>
          Refuse unless the stored defaults are at this revision

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl agents endpoint

```text
Read the owner-authenticated local HTTPS endpoint without enrolling a client

Usage: airkctl agents endpoint [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl agents info

```text
Read an existing connection's verified references without issuing credentials

Usage: airkctl agents info [OPTIONS] --connection-id <CONNECTION_ID>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --connection-id <CONNECTION_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl agents launch

```text
Start Claude Code, Codex or VS Code on this connection in the foreground (U91).

The credential goes only into the client's environment and the AIRK issuer is added to its trust; existing CA bundles stay enabled. Exits with the client's status.

Usage: airkctl agents launch [OPTIONS] --client <CLIENT> --connection-id <CONNECTION_ID> [-- <ARGS>...]

Arguments:
  [ARGS]...
          Arguments for the client, after `--`

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --client <CLIENT>
          [possible values: claude, codex, vscode]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --connection-id <CONNECTION_ID>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --directory <DIRECTORY>
          The client's working folder (default: the current directory)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl agents list

```text
Classify AIRK's entries in agent configuration files; read-only.

With --connection-id, report only whether this installation owns that connection.

Usage: airkctl agents list [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --connection-id <CONNECTION_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --project <PROJECTS>
          A project folder to check (repeatable; default: the current directory)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --no-user
          Leave this user's agent configuration files out

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl agents remove

```text
Remove AIRK's entries (only those naming --connection-id, when given)

Usage: airkctl agents remove [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --connection-id <CONNECTION_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --project <PROJECTS>
          A project folder to check (repeatable; default: the current directory)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --no-user
          Leave this user's agent configuration files out

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl agents repair

```text
Rewrite AIRK entries whose endpoint or helper is stale; --connection-id replaces an unknown connection

Usage: airkctl agents repair [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --connection-id <CONNECTION_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --project <PROJECTS>
          A project folder to check (repeatable; default: the current directory)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --no-user
          Leave this user's agent configuration files out

      --airkctl <AIRKCTL>
          The airkctl executable written credential helpers name (default: this airkctl)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl agents revoke

```text
Revoke one local HTTPS connection while preserving its original receipt

Usage: airkctl agents revoke [OPTIONS] --connection-id <CONNECTION_ID>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --connection-id <CONNECTION_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl agents verify

```text
Probe the service's own MCP endpoint with a connection's stored credential

Usage: airkctl agents verify [OPTIONS] --connection-id <CONNECTION_ID>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --connection-id <CONNECTION_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl changes

```text
Report indexed changes since a revision token: added, modified and removed files, symbols and documents

Usage: airkctl changes [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --since <SINCE>


      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --selector <SELECTORS>
          Repeatable tagged JSON selector, e.g. {"kind":"path","path":"one.rs"}

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --path <PATHS>
          Remember an exact collection-relative indexed path

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --include-symbols


      --collection <COLLECTION>
          [default: default]

      --include-documents


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

## airkctl code-facts

```text
Inspect code facts and signatures with exact default name matching and bounded continuation

Usage: airkctl code-facts [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --name-match <NAME_MATCH>
          Default exact lookup; suffix/contains match simple or qualified names case-sensitively

          [possible values: exact, suffix, contains]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --include-count
          Request qualified match counts; a bounded partial scan can report an unknown total

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --compact [<COMPACT>]
          False includes rich facts and durable IDs; independent of response mode

          [possible values: true, false]

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --symbol-key <SYMBOL_KEY>
          Durable symbol key from indexed continuity evidence; excludes textual filters

      --collection <COLLECTION>
          [default: default]

      --name <NAME>


      --kind <KIND>


      --language <LANGUAGE>


      --path-prefix <PATH_PREFIX>
          [default: ""]

      --limit <LIMIT>
          [default: 10]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airkctl configure

```text
Apply a guarded project configuration draft from --file or --stdin (JSON with an "operation")

Usage: airkctl configure [OPTIONS]

Options:
      --file <FILE>


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --stdin


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl context

```text
Read lines around --line in a file (--before/--after), with a citation

Usage: airkctl context [OPTIONS] <PATH> <LINE>

Arguments:
  <PATH>


  <LINE>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --revision <REVISION>


      --if-evidence <IF_EVIDENCE>


      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --follow-moves


      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --known-hash <KNOWN_HASH>


      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --artifact-id <ARTIFACT_ID>


      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --before <BEFORE>
          [default: 5]

      --collection <COLLECTION>
          [default: default]

      --after <AFTER>
          [default: 5]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airkctl explorer

```text
Offline Explorer drafts, validation and templates; results go to a new --output-file

Usage: airkctl explorer [OPTIONS] <COMMAND>

Commands:
  draft-search     Write a proposed Explorer Search form (MCP draft_search) after schema validation
  draft-strategy   Write a proposed Explorer strategy (MCP draft_strategy)
  draft-plan       Write a proposed Explorer plan (MCP draft_plan)
  suggest-view     Write a proposed Explorer view (MCP suggest_view)
  validate-draft   Validate a draft without writing a proposal
  list-templates   List the bundled strategy templates
  get-template     Write one bundled template
  proposal-status  Report the status of a written draft
  get-handoff      Write a handoff a VS Code window shared through its session (M12)

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer draft-plan

```text
Write a proposed Explorer plan (MCP draft_plan)

Usage: airkctl explorer draft-plan [OPTIONS] --output-file <OUTPUT_FILE>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name

          [default: {}]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer draft-search

```text
Write a proposed Explorer Search form (MCP draft_search) after schema validation

Usage: airkctl explorer draft-search [OPTIONS] --output-file <OUTPUT_FILE>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name

          [default: {}]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer draft-strategy

```text
Write a proposed Explorer strategy (MCP draft_strategy)

Usage: airkctl explorer draft-strategy [OPTIONS] --output-file <OUTPUT_FILE>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name

          [default: {}]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer get-handoff

```text
Write a handoff a VS Code window shared through its session (M12)

Usage: airkctl explorer get-handoff [OPTIONS] --handoff-id <HANDOFF_ID> --output-file <OUTPUT_FILE>

Options:
      --handoff-id <HANDOFF_ID>
          The handoff id the window published

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the handoff (must not exist)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer get-template

```text
Write one bundled template

Usage: airkctl explorer get-template [OPTIONS] --output-file <OUTPUT_FILE>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name

          [default: {}]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer list-templates

```text
List the bundled strategy templates

Usage: airkctl explorer list-templates [OPTIONS] --output-file <OUTPUT_FILE>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name

          [default: {}]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer proposal-status

```text
Report the status of a written draft

Usage: airkctl explorer proposal-status [OPTIONS] --draft <DRAFT> --output-file <OUTPUT_FILE>

Options:
      --draft <DRAFT>
          The draft file (at most 1 MiB)

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the result (must not exist)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer suggest-view

```text
Write a proposed Explorer view (MCP suggest_view)

Usage: airkctl explorer suggest-view [OPTIONS] --output-file <OUTPUT_FILE>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name

          [default: {}]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl explorer validate-draft

```text
Validate a draft without writing a proposal

Usage: airkctl explorer validate-draft [OPTIONS] --output-file <OUTPUT_FILE>

Options:
      --args <ARGS>
          Tool arguments as one JSON object, exactly as for the MCP tool of the same name

          [default: {}]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --output-file <OUTPUT_FILE>
          New file for the result (must not exist). Results are never sent to an Explorer Inbox

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl facts

```text
List document facts (requirements, dates, decisions, features) extracted from indexed documents

Usage: airkctl facts [OPTIONS] [PATH_PREFIX]

Arguments:
  [PATH_PREFIX]
          [default: ""]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --compact [<COMPACT>]
          False includes rich facts and durable IDs; independent of response mode

          [possible values: true, false]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --document-key <DOCUMENT_KEY>
          Durable document key; excludes textual filters

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --name <NAME>


      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --kind <KIND>
          Exact document fact category, such as requirement, date, architecture or feature

      --collection <COLLECTION>
          [default: default]

      --limit <LIMIT>
          [default: 50]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airkctl files

```text
List indexed files under an optional path prefix, with continuation cursors

Usage: airkctl files [OPTIONS] [PREFIX]

Arguments:
  [PREFIX]
          [default: ""]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --limit <LIMIT>
          [default: 10]

      --max-bytes <MAX_BYTES>
          [default: 8192]

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl folders

```text
List the folders in PATH (folders only, links never followed), or the starting list without one (M24: AIRK Admin's Browse dialog)

Usage: airkctl folders [OPTIONS] [PATH]

Arguments:
  [PATH]
          An absolute folder path; omit it for drives, your home folder and the AIRK data location

Options:
      --hidden
          Include hidden folders

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --after <AFTER>
          Continue after this folder name (the previous page's `continuation`)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl imports

```text
List import and dependency statements found under a path prefix

Usage: airkctl imports [OPTIONS] [PATH_PREFIX]

Arguments:
  [PATH_PREFIX]
          [default: ""]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --compact [<COMPACT>]
          False includes rich facts and durable IDs; independent of response mode

          [possible values: true, false]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl index

```text
Queue indexing for the project (full rebuilds need a reviewed maintenance preview)

Usage: airkctl index [OPTIONS]

Options:
      --full


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl info

```text
Show what the index knows about one file: language, size, extraction method and parse status

Usage: airkctl info [OPTIONS] <PATH>

Arguments:
  <PATH>


Options:
      --include-profile-families
          Include eighteen metadata-only effective profile family observations for this file. Collection policy is unavailable under partial Project scopes. Reporting adds no source reads or profile migration; existing verification remains independent.

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --revision <REVISION>


      --if-evidence <IF_EVIDENCE>


      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --follow-moves


      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --known-hash <KNOWN_HASH>


      --artifact-id <ARTIFACT_ID>


      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl init

```text
Acknowledge an existing reviewed index mapping and prepare its reserved storage.

In an authority-1 catalog, --root/--data-dir/--collection must exactly match an existing ready index. New indexes use the reviewed index setup: `airkctl library setup` (or the admin pages' Index library); init creates no index or grant. Authority-zero services retain their legacy registration behavior. For a service on an explicit control directory (the `service-foreground` role), use `airkctl service register --runtime-dir <control>` instead.

Usage: airkctl init [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl inspect

```text
Queue an inspection of the project's configuration and index

Usage: airkctl inspect [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl jobs

```text
Read, retry or cancel one job by its ID

Usage: airkctl jobs [OPTIONS] <COMMAND>

Commands:
  get     Print one job's record
  retry   Retry a failed job
  cancel  Cancel a pending or running job

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl jobs cancel

```text
Cancel a pending or running job

Usage: airkctl jobs cancel [OPTIONS] <ID>

Arguments:
  <ID>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl jobs get

```text
Print one job's record

Usage: airkctl jobs get [OPTIONS] <ID>

Arguments:
  <ID>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl jobs retry

```text
Retry a failed job

Usage: airkctl jobs retry [OPTIONS] <ID>

Arguments:
  <ID>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library

```text
The index library: list, show and set up indexes (the admin pages' Index library)

Usage: airkctl library [OPTIONS] <COMMAND>

Commands:
  list         One catalog page (or every page with --all): indexes, index sets (projects), memberships, sources, grants or contexts
  show         One catalog entity: an index, an index set or a grant
  setup        Draft a new index for --root (and --data-dir) and print the service's review; --apply also creates it
  setup-apply  Apply a draft `setup` printed (the whole `catalog_index_setup_draft` line or its `draft`)
  change       Preview any catalog operation (JSON, as the admin pages send it); --yes applies the preview
  set-create   Create an index set; with --index, add each index with every enabled source root, whole (an empty set with the same name is reused instead of creating another)
  set-rename   Rename an index set
  set-delete   Delete an index set (its indexes stay); needs --confirm <set name>
  rename       Rename an index
  retire       Retire an index (stop tracking it); needs --confirm <index name>
  describe     Set or clear the description of an index or an index set; agents read it in list_indexes (without one, a description is generated from the name and paths)

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library change

```text
Preview any catalog operation (JSON, as the admin pages send it); --yes applies the preview

Usage: airkctl library change [OPTIONS] --operation <OPERATION>

Options:
      --operation <OPERATION>
          The `CatalogOperation` JSON, for example {"operation":"rename_index",…}

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --yes
          Apply the previewed change

      --confirm <CONFIRM>
          The target's current name (or a grant's or context's id), typed to confirm a delete, removal or retirement (U98)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --runtime-dir <RUNTIME_DIR>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library describe

```text
Set or clear the description of an index or an index set; agents read it in list_indexes (without one, a description is generated from the name and paths)

Usage: airkctl library describe [OPTIONS]

Options:
      --index <INDEX>
          The index (`idx_…`)

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --set <SET>
          The index set (`prj_…`)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --text <TEXT>
          The description: at most 1 KiB, newlines allowed, no other control characters

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --clear
          Remove the stored description

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --yes
          Apply the previewed change

      --confirm <CONFIRM>
          The target's current name (or a grant's or context's id), typed to confirm a delete, removal or retirement (U98)

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --runtime-dir <RUNTIME_DIR>


      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library list

```text
One catalog page (or every page with --all): indexes, index sets (projects), memberships, sources, grants or contexts

Usage: airkctl library list [OPTIONS]

Options:
      --kind <KIND>
          [default: indexes]
          [possible values: indexes, projects, memberships, sources, grants, contexts]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --index <INDEX>
          Limit memberships or sources to one index (`idx_…`)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --set <SET>
          Limit memberships to one index set (`prj_…`)

      --after <AFTER>
          A continuation cursor a previous page printed (JSON)

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --all
          Follow every continuation at the first page's revision

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library rename

```text
Rename an index

Usage: airkctl library rename [OPTIONS] --index <INDEX> --name <NAME>

Options:
      --index <INDEX>


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --name <NAME>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --yes
          Apply the previewed change

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --confirm <CONFIRM>
          The target's current name (or a grant's or context's id), typed to confirm a delete, removal or retirement (U98)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --runtime-dir <RUNTIME_DIR>


      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library retire

```text
Retire an index (stop tracking it); needs --confirm <index name>

Usage: airkctl library retire [OPTIONS] --index <INDEX>

Options:
      --index <INDEX>


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --yes
          Apply the previewed change

      --confirm <CONFIRM>
          The target's current name (or a grant's or context's id), typed to confirm a delete, removal or retirement (U98)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --runtime-dir <RUNTIME_DIR>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library set-create

```text
Create an index set; with --index, add each index with every enabled source root, whole (an empty set with the same name is reused instead of creating another)

Usage: airkctl library set-create [OPTIONS] --name <NAME>

Options:
      --name <NAME>


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --index <INDEXES>
          An index (`idx_…`) to add to the set (repeatable); the memberships apply with --yes

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --yes
          Apply the previewed change

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --confirm <CONFIRM>
          The target's current name (or a grant's or context's id), typed to confirm a delete, removal or retirement (U98)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --runtime-dir <RUNTIME_DIR>


      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library set-delete

```text
Delete an index set (its indexes stay); needs --confirm <set name>

Usage: airkctl library set-delete [OPTIONS] --set <SET>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --set <SET>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --yes
          Apply the previewed change

      --confirm <CONFIRM>
          The target's current name (or a grant's or context's id), typed to confirm a delete, removal or retirement (U98)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --runtime-dir <RUNTIME_DIR>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library set-rename

```text
Rename an index set

Usage: airkctl library set-rename [OPTIONS] --set <SET> --name <NAME>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --set <SET>


      --name <NAME>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --yes
          Apply the previewed change

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --confirm <CONFIRM>
          The target's current name (or a grant's or context's id), typed to confirm a delete, removal or retirement (U98)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --runtime-dir <RUNTIME_DIR>


      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library setup

```text
Draft a new index for --root (and --data-dir) and print the service's review; --apply also creates it

Usage: airkctl library setup [OPTIONS] --name <NAME>

Options:
      --name <NAME>
          The index's display name

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --index-collection <INDEX_COLLECTION>
          The index's collection (default: from the name, lowercase letters, digits and dashes)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --additional <ADDITIONAL>
          An additional source root as ALIAS=PATH (repeatable)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --watch <WATCH>
          Keep the index current after edits (native source watching)

          [default: on]
          [possible values: on, off]

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --revision <REVISION>
          Draft against this catalog revision (default: the current one)

      --apply
          Apply the draft right after printing it

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --runtime-dir <RUNTIME_DIR>


      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library setup-apply

```text
Apply a draft `setup` printed (the whole `catalog_index_setup_draft` line or its `draft`)

Usage: airkctl library setup-apply [OPTIONS] --draft-file <DRAFT_FILE>

Options:
      --draft-file <DRAFT_FILE>


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl library show

```text
One catalog entity: an index, an index set or a grant

Usage: airkctl library show [OPTIONS]

Options:
      --index <INDEX>


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --set <SET>


      --grant <GRANT>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --revision <REVISION>
          Refuse unless the catalog is at this revision

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --runtime-dir <RUNTIME_DIR>


      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl model-catalog

```text
Print the offline model catalog and the CPU choice for this host

Usage: airkctl model-catalog [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl outline

```text
Read one indexed file's structure in file order; this returns evidence, not an Explorer page proposal

Usage: airkctl outline [OPTIONS] <PATH>

Arguments:
  <PATH>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --compact [<COMPACT>]
          Return minimal names/kinds/lines/signatures with shared original file/provenance bindings

          [possible values: true, false]

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --revision <REVISION>


      --if-evidence <IF_EVIDENCE>


      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --follow-moves


      --collection <COLLECTION>
          [default: default]

      --known-hash <KNOWN_HASH>


      --artifact-id <ARTIFACT_ID>


      --limit <LIMIT>
          [default: 50]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airkctl parse

```text
Plan, and with --execute run, a multi-step read-only evidence request.

Give a natural-language request as the TEXT argument, or a structured plan with --request FILE ('-' for stdin). Without --execute it only prints the plan; same contract as the MCP `parse` tool. Never writes sources.

Usage: airkctl parse [OPTIONS] [TEXT]

Arguments:
  [TEXT]


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --project <PROJECT>
          Configured native project binding (alias for global --service-project)

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --execute


      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --request <REQUEST>
          Structured request file; '-' reads bounded JSON from stdin

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl pause

```text
Pause work for the project

Usage: airkctl pause [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl progress

```text
Print the project's live indexing progress: the running job's phase and counts, pending and queued jobs, and pause state

Usage: airkctl progress [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl projects

```text
List the projects registered with the service (same fields as the MCP `projects` tool)

Usage: airkctl projects [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl read

```text
Read an indexed file's lines (--start-line/--end-line) from the published revision, with a citation

Usage: airkctl read [OPTIONS] --start-line <START_LINE> --end-line <END_LINE> <PATH>

Arguments:
  <PATH>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --revision <REVISION>


      --if-evidence <IF_EVIDENCE>


      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --follow-moves


      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --known-hash <KNOWN_HASH>


      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --artifact-id <ARTIFACT_ID>


      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

      --start-line <START_LINE>


      --end-line <END_LINE>


      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airkctl refs

```text
Find exact-case textual identifier uses; these are candidates, not resolved language references

Usage: airkctl refs [OPTIONS] <NAME>

Arguments:
  <NAME>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --language <LANGUAGE>
          Exact indexed extraction-language label; not a filename-extension filter

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --evidence-kind <EVIDENCE_KIND>
          Indexed evidence classification; unknown includes unclassified extraction

          [possible values: code, document, configuration, unknown]

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --exclude-path-prefix <EXCLUDE_PATH_PREFIX>
          Exclude one literal collection-relative prefix from this query only

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --cursor <CURSOR>
          Continue one exact publication with identical identifier, filters and ordering

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --order <ORDER>
          Omit for the legacy response; paged index order uses stable IDs, with path/code_first alternatives

          [possible values: index, path, code_first]

      --collection <COLLECTION>
          [default: default]

      --include-files
          Summarize returned page records by file; bounded metadata is additional to the text-byte budget

      --include-count
          Request candidate-scoped counts; the whole-query total remains unknown

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --limit <LIMIT>
          [default: 10]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airkctl relationships

```text
Follow indexed relationships of a symbol in --direction, optionally of one --kind, up to --hops levels

Usage: airkctl relationships [OPTIONS] <SYMBOL>

Arguments:
  <SYMBOL>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --direction <DIRECTION>
          [default: incoming]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --kind <KIND>


      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --hops <HOPS>
          [default: 1]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --limit <LIMIT>
          [default: 10]

      --collection <COLLECTION>
          [default: default]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airkctl resume

```text
Resume work paused with `pause`

Usage: airkctl resume [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl search

```text
Search indexed evidence; results carry file, line and revision citations.

Without --mode the saved search policy applies. Eligible policies can return lexical matches during helper outages; execution reports the served mode and semantic incompleteness. Explicit vector/hybrid and saved RequireComplete refuse helper outages. Hybrid can still serve lexical evidence during a pending index upgrade, with that separate limitation disclosed. --mode lexical uses normalized AND terms: punctuation and quotation marks separate terms; quotes do not enable phrase matching. Lexical order is stable index identity, not relevance.

Usage: airkctl search [OPTIONS] <QUERY>

Arguments:
  <QUERY>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --language <LANGUAGE>
          Exact indexed extraction-language label; not a filename-extension filter

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --evidence-kind <EVIDENCE_KIND>
          Indexed evidence classification; unknown includes unclassified extraction

          [possible values: code, document, configuration, unknown]

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --exclude-path-prefix <EXCLUDE_PATH_PREFIX>
          Exclude one literal collection-relative prefix from this query only

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --mode <MODE>
          [possible values: lexical, vector, hybrid]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --semantic
          Use the configured local embedding model and disk-backed vector graph

      --collection <COLLECTION>
          [default: default]

      --path-prefix <PATH_PREFIX>


      --limit <LIMIT>
          [default: 10]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl service

```text
The AIRK service: start, stop, status, logs, requests, trust and management

Usage: airkctl service [OPTIONS] <COMMAND>

Commands:
  start                  Start the installed service (the install record's verified airk) and wait until it is healthy
  stop                   Stop the service and wait until its process exits; a saved Stop keeps automatic starts off
  restart                Stop the service, wait until its process exits, then start the installed service again
  uninstall              Remove this AIRK Service installation through its signed setup step: stop the service, remove login startup, the certificate trust it added, and the install pointer and record
  logs                   Read the service's diagnostic logs (recent from the running service; all, raw or stopped-service reads from the local files)
  trust                  This installation's HTTPS issuer in your OS trust store; works while the service is down
  issuer-rotation        The saved HTTPS issuer rotation and current endpoint (status only in this version)
  abandon-lifecycle      Mark a stuck stop or restart record abandoned (U80); a finished record is printed unchanged
  status                 Report the installation and whether the service is running, with its saved Stop and any unfinished stop or restart
  record-error           Record a bounded client error (JSON on stdin) without needing the service to run
  register               Register --root/--data-dir/--collection with the service at --runtime-dir. The data directory must exist
  prepare-registration   Print the canonical registration for --root/--data-dir/--collection, including its binding. Read-only
  prepare-configuration  Prepare a guarded configuration request for a project; submit it with `requests retry`
  prepare-storage        Create the reserved index storage for an already reserved project (owner-only on Unix)
  requests               Saved client requests: list, prepare, retry or forget by nonce
  rpc                    Send one raw management request (ControlMethod JSON) and print the service's answer
  inventory              The service's locations (program, data, state, cache, logs) with sizes (U100)
  purge-plan             The exact files a purge of the cache or the closed logs would delete now, and its plan digest
  purge                  Delete exactly the plan --plan-digest names; --confirm must be the location's path as `inventory` lists it
  login                  Login startup on Windows: status, install or uninstall (U101)
  health                 Check that the service for --runtime-dir is running and healthy; exits non-zero otherwise
  shutdown               Ask the service for --runtime-dir to drain its workers and exit

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service abandon-lifecycle

```text
Mark a stuck stop or restart record abandoned (U80); a finished record is printed unchanged

Usage: airkctl service abandon-lifecycle [OPTIONS] --request-id <REQUEST_ID>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --request-id <REQUEST_ID>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service health

```text
Check that the service for --runtime-dir is running and healthy; exits non-zero otherwise

Usage: airkctl service health [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service inventory

```text
The service's locations (program, data, state, cache, logs) with sizes (U100)

Usage: airkctl service inventory [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service issuer-rotation

```text
The saved HTTPS issuer rotation and current endpoint (status only in this version)

Usage: airkctl service issuer-rotation [OPTIONS] <ACTION>

Arguments:
  <ACTION>
          [possible values: status]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service login

```text
Login startup on Windows: status, install or uninstall (U101)

Usage: airkctl service login [OPTIONS] <ACTION>

Arguments:
  <ACTION>
          [possible values: status, install, uninstall]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service logs

```text
Read the service's diagnostic logs (recent from the running service; all, raw or stopped-service reads from the local files)

Usage: airkctl service logs [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --limit <LIMIT>
          [default: 32]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --all
          Export retained logs locally (200 MB high-water, FIFO purge to 100 MB)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --raw
          Open the separate local raw job-error ring (may contain paths/error values)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --output-file <OUTPUT_FILE>
          Capture raw retained diagnostics into a new local file without buffering stdout (U79)

      --format <FORMAT>
          [default: json]
          [possible values: json, text]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service prepare-configuration

```text
Prepare a guarded configuration request for a project; submit it with `requests retry`

Usage: airkctl service prepare-configuration [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>


      --project <PROJECT>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service prepare-registration

```text
Print the canonical registration for --root/--data-dir/--collection, including its binding. Read-only

Usage: airkctl service prepare-registration [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service prepare-storage

```text
Create the reserved index storage for an already reserved project (owner-only on Unix)

Usage: airkctl service prepare-storage [OPTIONS] --runtime-dir <RUNTIME_DIR> --project <PROJECT>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>


      --project <PROJECT>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service purge

```text
Delete exactly the plan --plan-digest names; --confirm must be the location's path as `inventory` lists it

Usage: airkctl service purge [OPTIONS] --kind <KIND> --plan-digest <PLAN_DIGEST>

Options:
      --kind <KIND>
          [possible values: cache, logs]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --plan-digest <PLAN_DIGEST>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --confirm <CONFIRM>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --runtime-dir <RUNTIME_DIR>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service purge-plan

```text
The exact files a purge of the cache or the closed logs would delete now, and its plan digest

Usage: airkctl service purge-plan [OPTIONS] --kind <KIND>

Options:
      --kind <KIND>
          [possible values: cache, logs]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service record-error

```text
Record a bounded client error (JSON on stdin) without needing the service to run

Usage: airkctl service record-error [OPTIONS] --operation <OPERATION>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --operation <OPERATION>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service register

```text
Register --root/--data-dir/--collection with the service at --runtime-dir. The data directory must exist

Usage: airkctl service register [OPTIONS] --runtime-dir <RUNTIME_DIR>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service requests

```text
Saved client requests: list, prepare, retry or forget by nonce

Usage: airkctl service requests [OPTIONS] <COMMAND>

Commands:
  prepare-configuration  Save a configuration request for a project from a draft --file, without submitting it
  prepare                Save a job request (--kind index, inspect, reconcile, vectors, model_setup, verify, recover or maintain)
  list                   List saved client requests and their outcomes
  retry                  Submit, or resubmit after an unknown outcome, a saved request by its nonce
  forget                 Discard a saved request record by its nonce

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service requests forget

```text
Discard a saved request record by its nonce

Usage: airkctl service requests forget [OPTIONS] <NONCE>

Arguments:
  <NONCE>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service requests list

```text
List saved client requests and their outcomes

Usage: airkctl service requests list [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service requests prepare

```text
Save a job request (--kind index, inspect, reconcile, vectors, model_setup, verify, recover or maintain)

Usage: airkctl service requests prepare [OPTIONS] --project <PROJECT>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --project <PROJECT>


      --kind <KIND>
          [default: index]
          [possible values: index, inspect, reconcile, vectors, model_setup, verify, recover, maintain]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service requests prepare-configuration

```text
Save a configuration request for a project from a draft --file, without submitting it

Usage: airkctl service requests prepare-configuration [OPTIONS] --project <PROJECT> --file <FILE>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --project <PROJECT>


      --file <FILE>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service requests retry

```text
Submit, or resubmit after an unknown outcome, a saved request by its nonce

Usage: airkctl service requests retry [OPTIONS] <NONCE>

Arguments:
  <NONCE>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service restart

```text
Stop the service, wait until its process exits, then start the installed service again

Usage: airkctl service restart [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service rpc

```text
Send one raw management request (ControlMethod JSON) and print the service's answer.

The metadata relay's filter applies: evidence credentials and private setup methods are never relayed. Transport success and the service's own status are reported separately.

Usage: airkctl service rpc [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --request-file <REQUEST_FILE>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --stdin
          Read bounded management JSON from stdin instead of a request file

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl service shutdown

```text
Ask the service for --runtime-dir to drain its workers and exit

Usage: airkctl service shutdown [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service start

```text
Start the installed service (the install record's verified airk) and wait until it is healthy

Usage: airkctl service start [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service status

```text
Report the installation and whether the service is running, with its saved Stop and any unfinished stop or restart

Usage: airkctl service status [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service stop

```text
Stop the service and wait until its process exits; a saved Stop keeps automatic starts off

Usage: airkctl service stop [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl service trust

```text
This installation's HTTPS issuer in your OS trust store; works while the service is down.

install and remove need --expected-ca-sha256, the reviewed issuer digest that inspect prints; the OS may ask you to confirm.

Usage: airkctl service trust [OPTIONS] <ACTION>

Arguments:
  <ACTION>
          [possible values: inspect, install, remove]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --runtime-dir <RUNTIME_DIR>
          The service's control directory (default: --service-runtime-dir, else the installation's)

      --expected-ca-sha256 <EXPECTED_CA_SHA256>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl service uninstall

```text
Remove this AIRK Service installation through its signed setup step: stop the service, remove login startup, the certificate trust it added, and the install pointer and record.

Index data, service state, cache and logs are kept unless --purge-indexes. airkctl hands the work to setup and exits; `airkctl service status` and the printed receipt report the outcome. On Windows, Settings > Apps > AIRK Service also removes the program files.

Usage: airkctl service uninstall [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --yes
          Confirm the uninstall without an interactive prompt

      --purge-indexes
          Also delete index data, service state, cache and logs

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl session

```text
Serve one editor window's framed session (protocol 2) on stdin/stdout

Usage: airkctl session [OPTIONS]

Options:
      --label <LABEL>
          The window label shown to agents (at most 128 bytes)

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --owner-watch
          Exit when the parent process (the editor's extension host) exits

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl settings

```text
Read one project setting view; read-only

Usage: airkctl settings [OPTIONS] <VIEW>

Arguments:
  <VIEW>
          [possible values: extraction_policy, vector_selection, vector_status, roots, scan_limits, quota_policy, quota_status]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --validate-file <VALIDATE_FILE>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl setup-model

```text
Queue the explicit model setup job for the project

Usage: airkctl setup-model [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl status

```text
Print the project's publication, freshness, coverage and job status

Usage: airkctl status [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl symbol

```text
Find definitions by exact name by default; use --name-match for suffix/contains variants

Usage: airkctl symbol [OPTIONS] <NAME>

Arguments:
  <NAME>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --verification <VERIFICATION>
          Dated saved-source checks; default Report, bounded to 8 sources, 16 MiB and 2000 ms. Off skips checking

          [possible values: off, report, require]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --verification-max-sources <VERIFICATION_MAX_SOURCES>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --verification-max-bytes <VERIFICATION_MAX_BYTES>


      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --verification-deadline-ms <VERIFICATION_DEADLINE_MS>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --reply-detail <REPLY_DETAIL>
          Lean/Rich reply projection; independent of retained indexing detail

          [possible values: lean, rich]

      --name-match <NAME_MATCH>
          Default exact lookup; suffix/contains match simple or qualified names case-sensitively

          [possible values: exact, suffix, contains]

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --include-count
          Request qualified match counts; a bounded partial scan can report an unknown total

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --compact [<COMPACT>]
          False includes rich facts and durable IDs; independent of response mode

          [possible values: true, false]

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --cursor <CURSOR>
          Continue the same revision-bound discovery with unchanged filters

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --language <LANGUAGE>


      --collection <COLLECTION>
          [default: default]

      --path-prefix <PATH_PREFIX>
          [default: ""]

      --limit <LIMIT>
          [default: 10]

      --max-bytes <MAX_BYTES>
          [default: 8192]

  -h, --help
          Print help
```

## airkctl tools

```text
The unified tools: list, help and call

Usage: airkctl tools [OPTIONS] <COMMAND>

Commands:
  list   List the connection's unified tools; --json includes complete schemas and annotations
  help   Show a tool's description, input schema, defaults and argument template
  call   Invoke an MCP tool and print its unchanged result envelope as JSON
  shell  Open a connection-bound interactive console with help and examples

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl tools call

```text
Invoke an MCP tool and print its unchanged result envelope as JSON.

--index and --set read whole indexes and index sets (ids or exact names) through the open read path an agent uses; --connection applies that open connection's default scope and agent choice, exactly as its MCP calls do. list_indexes always takes that path.

Usage: airkctl tools call [OPTIONS] <TOOL>

Arguments:
  <TOOL>


Options:
      --args <ARGS>
          JSON object; omit for {}. Maximum 64 KiB

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --args-file <ARGS_FILE>
          Read a UTF-8 JSON object from a regular file (maximum 64 KiB)

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --index <INDEXES>
          An index to read whole: `idx_…` or its exact name (repeatable, at most 32)

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --set <SETS>
          An index set to read as its member indexes: `prj_…` or its exact name (repeatable)

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --connection <CONNECTION>
          The open agent connection whose default scope and agent choice apply

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl tools help

```text
Show a tool's description, input schema, defaults and argument template

Usage: airkctl tools help [OPTIONS] <TOOL>

Arguments:
  <TOOL>


Options:
      --json


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --connection <CONNECTION>
          An open agent connection whose catalog to read

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl tools list

```text
List the connection's unified tools; --json includes complete schemas and annotations.

With --connection, the catalog an open agent connection sees: its scope selectors and list_indexes appear only when its agent choice is on.

Usage: airkctl tools list [OPTIONS]

Options:
      --json


      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --connection <CONNECTION>
          An open agent connection whose catalog to list

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help (see a summary with '-h')
```

## airkctl tools shell

```text
Open a connection-bound interactive console with help and examples

Usage: airkctl tools shell [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl usage

```text
Local usage reporting for the selected project: report, policy, configure (mode), clear, feedback, import, calibrate

Usage: airkctl usage [OPTIONS] <COMMAND>

Commands:
  report     Summarize recorded usage for the last --days (json, csv or markdown; optional price scenario file)
  policy     Show the project's usage recording mode and retention
  clear      Delete recorded usage for the project
  configure  Set the recording mode: disabled, aggregates_only or diagnostic_events
  feedback   Record whether one returned request was useful, partly useful or not useful
  import     Import a usage export; --preview shows what would change and --if-sha256 pins the file
  calibrate  Calibrate token estimates against a tokenizer and baseline/returned samples (advanced)

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl usage calibrate

```text
Calibrate token estimates against a tokenizer and baseline/returned samples (advanced)

Usage: airkctl usage calibrate [OPTIONS] --tokenizer <TOKENIZER> --baseline <BASELINE> --returned <RETURNED> --generation <GENERATION> --content-kind <CONTENT_KIND>

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --tokenizer <TOKENIZER>


      --baseline <BASELINE>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --returned <RETURNED>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --generation <GENERATION>


      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --content-kind <CONTENT_KIND>


      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --record


      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl usage clear

```text
Delete recorded usage for the project

Usage: airkctl usage clear [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl usage configure

```text
Set the recording mode: disabled, aggregates_only or diagnostic_events

Usage: airkctl usage configure [OPTIONS] --mode <MODE>

Options:
      --mode <MODE>
          [possible values: disabled, aggregates_only, diagnostic_events]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl usage feedback

```text
Record whether one returned request was useful, partly useful or not useful

Usage: airkctl usage feedback [OPTIONS] --usefulness <USEFULNESS> <REQUEST_ID>

Arguments:
  <REQUEST_ID>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --usefulness <USEFULNESS>
          [possible values: useful, partly_useful, not_useful, partly-useful, not-useful]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl usage import

```text
Import a usage export; --preview shows what would change and --if-sha256 pins the file

Usage: airkctl usage import [OPTIONS] <FILE>

Arguments:
  <FILE>


Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --preview


      --if-sha256 <IF_SHA256>


      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl usage policy

```text
Show the project's usage recording mode and retention

Usage: airkctl usage policy [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl usage report

```text
Summarize recorded usage for the last --days (json, csv or markdown; optional price scenario file)

Usage: airkctl usage report [OPTIONS]

Options:
      --days <DAYS>
          [default: 7]

      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --format <FORMAT>
          [default: json]
          [possible values: json, csv, markdown]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --price-scenario <PRICE_SCENARIO>


      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl version

```text
Print the version document: airkctl, its contracts and the installation

Usage: airkctl version [OPTIONS]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```

## airkctl watch

```text
Turn native source watching on or off for the project (on keeps the index current after edits), or print its watch status

Usage: airkctl watch [OPTIONS] <MODE>

Arguments:
  <MODE>
          [possible values: on, off, status]

Options:
      --output <OUTPUT>
          Result and error format: json writes one JSON document or error line

          [default: text]
          [possible values: json, text]

      --service-runtime-dir <SERVICE_RUNTIME_DIR>
          The AIRK service's control directory

      --service-project <SERVICE_PROJECT>
          A registered project binding

      --catalog-session <CATALOG_SESSION>
          Explicit reviewed single-index session JSON; preferences alone grant no access

      --combined-session <COMBINED_SESSION>
          Frozen reviewed multi-Project session JSON; indexes are executed once each

      --project-ids <PROJECT_IDS>
          Narrow a combined read to explicit Projects from its reviewed session

      --index-ids <INDEX_IDS>
          Narrow a combined read to member indexes; requires explicit Project scope

      --member-availability <MEMBER_AVAILABILITY>
          Report member failures or require every selected member to answer

          [possible values: report, require]

      --root <ROOT>
          The project folder (default: the current folder)

          [default: .]

      --data-dir <DATA_DIR>
          The project's data folder (default: the installation's data location)

      --collection <COLLECTION>
          [default: default]

  -h, --help
          Print help
```
