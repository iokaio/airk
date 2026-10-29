# One AIRK MCP connection

Current AIRK exposes one server named `airk`, with 27 unique, unprefixed tools. Evidence tools answer from the configured index. Help explains AIRK. Draft tools prepare suggestions that wait for the user in Explorer. This removes duplicate status/help entry points and the need for an agent to choose between two AIRK servers.

Tool names are unique within this server; the client may add its own server namespace. Repeating `airk_` on every tool adds no distinction between reading evidence and proposing a form. Verbs and descriptions carry that distinction. Previously saved `airk_*` and `airk_explorer_*` calls remain callable compatibility aliases; they are omitted from discovery so agents see one catalog. Native `airk tools` retains its seventeen read-only aliases for existing terminal scripts and Explorer's internal evidence adapter.

| Tool | Description |
|---|---|
| `projects` | List only the bindings configured on this connection and its default. Does not enumerate or register every machine project. |
| `status` | Quick published counts, exact publication, freshness and semantic readiness. No editor or runtime/job probes. |
| `diagnostics` | Detailed coverage, runtime/process identities, job progress, queues, watcher/waits and optional editor attachment. May take longer; does not index or repair. |
| `capabilities` | Describe extraction adapters and supported evidence capabilities. |
| `search` | Search indexed code/documents and return bounded, cited lexical, vector or hybrid evidence. |
| `read_file` | Read an indexed line range with its revision and source identity. |
| `list_files` | List indexed paths, with bounded continuation. |
| `file_info` | Inspect indexed file metadata, extraction basis and hash. |
| `outline` | Read one indexed file's declaration or document outline. |
| `symbols` | Find native definition names and their indexed locations. |
| `imports` | Find selected import/use/include declarations. |
| `code_facts` | Retrieve supported syntax or producer-backed code facts and coverage. |
| `document_facts` | Retrieve structured document facts with original locations and extraction qualifiers. |
| `references` | Find exact-case textual identifier occurrences; does not claim resolved references. |
| `relationships` | Retrieve supported structural candidates or producer assertions with their basis. |
| `context` | Read bounded neighboring indexed lines. |
| `changes` | Validate remembered evidence or page through observed indexed changes. |
| `help` | Read native topics/CLI help/tool schemas, search bundled guidance with `query`, or read a guide with `topic_id`. Returns documentation. |
| `draft_search` | Propose an Explorer Search form; returns a receipt and never runs the query. |
| `draft_strategy` | Propose a Studio strategy using supported templates/operators; never executes it. |
| `draft_plan` | Propose Planner tasks and instructions; never saves the plan. |
| `suggest_view` | Propose Navigation, Review or Management guidance using `kind` and a typed `draft`; never runs a management action or test. |
| `list_templates` | Discover the installed strategy pack, with bounded paging and availability. |
| `get_template` | Read one installed template's parameters, defaults and contracts. |
| `validate_draft` | Validate a proposal without admitting it to the Inbox. |
| `proposal_status` | Read a proposal's review state; applied does not mean executed. |
| `get_handoff` | Read only the complete snapshot explicitly shared by the user through an expiring token. |

## Scope and availability

From 1.5.9, the stdio lifecycle accepts a legacy `initialize` fallback after a discovery probe without retaining a contradictory modern-only metadata requirement. Each connection negotiates independently: Codex, Claude Code, Copilot and other clients can address the same saved project with different supported handshake styles. Client names are diagnostic labels, not routing or compatibility switches. Modern-only requests retain their metadata checks; catalog discovery does not wait for the service, index or editor adapter. See [the 1.5.9 evidence record](release-1.5.9.md). Native protocol fixtures are distinct from qualification inside each assistant application.

New project-local connections retain a canonical source folder as their scope: `airk-launcher --root <source> mcp`. They do not persist a data path or internal project ID. Each request resolves that source's active registration, so rebuilding or restoring the index does not require regenerating the connection. Multiple active indexes for one source require an explicit `--index-collection`. Missing, removed and ambiguous registrations remain distinct; tools/help still discover normally. Additional source folders use repeated `--additional-root` with `--no-default-project`. Select a configured scope from `projects`; it is a stable connection selector, not the internal native binding. Explorer selection never redirects retrieval. Legacy explicitly pinned launches remain supported, and Explorer's internal evidence connection retains its registration and revision fences.

The native frontend owns stdio and direct evidence reads. The CLI composes an optional verified extension adapter for offline UI help, templates and window IPC. Kernel/store never depend on the editor. A missing, closed or invalid editor adapter cannot remove native tools or prevent evidence retrieval. Native help, catalog discovery and configured-project listing do not elect the service; the first native observation/query checks the selected runtime and service. Explicit Stop is respected. UI help requires the configured verified extension bundle, but no open window; UI drafts require a matching trusted window.

`status` keeps compact publication/readiness fields and adds the configured `project` scope. It has a two-second unified request budget; expiration reports readiness as unknown. `diagnostics` adds detailed native observations and `explorer`. Neither response mode nor include_status makes status run diagnostics. Each component retains its own availability and observation age. A successful status envelope can report unavailable evidence; callers must require `index_state: available`, the expected collection and a publication token before treating results as indexed evidence. Source publication and semantic coverage remain separate observations. Native usage accounting stays at the evidence boundary; optional editor calls and the added UI portion of composite status are not measured as native evidence bytes.

## Startup migration

After verified runtime selection and native registration, trusted startup and **Install or Repair CLI** consolidate recognized existing project configurations in one guarded save per file:

Version 1.5.11 checks saved editor bindings against the current native registry before generating these updates. An unambiguous replacement registration for the same source and collection repairs the editor reference; unavailable or ambiguous metadata cannot authorize a configuration rewrite. Explicit setup still revalidates immediately before writing. Already matching files are retained without repeated update messages.

- `.codex/config.toml`
- `.mcp.json`
- `.vscode/mcp.json`
- `.gemini/settings.json`
- `.agents/mcp_config.json`
- `.cursor/mcp.json`
- `.roo/mcp.json`
- `.junie/mcp/mcp.json`

For the current project, the old `airk-<collection>` and extension-owned `airk_explorer` definitions are removed and replaced with `airk`. Launchers, canonical roots, collection/binding and the extension descriptor must match this installation; old commands are never executed. New setup snippets also contain one entry. The retired VS Code Assistance provider withdraws its definitions instead of advertising a second server.

Migration preserves unrelated servers/settings, JSON comments, supported TOML comments, environment values and disabled state. If either old server was disabled, the unified connection remains disabled. Supported tool allow/deny/preapproval lists are translated; partially approved combined view tools require fresh approval rather than granting additional actions. Conflicting environments/policies, unknown launchers, unsupported TOML, unsafe/linked files or unsaved/concurrent edits preserve the file and produce a message for review. Missing configurations do not enable an agent. User-wide and client-private approval stores are not rewritten. Restart the agent and approve renamed tools through that client's controls.

In 1.5.11, opening an agent guide checks the selected source/collection against a complete, revision-bound native registration listing. An obsolete editor registration can be repaired only when there is one matching active native index; ambiguity, missing registrations and unavailable metadata remain explicit failures. This changes the editor reference, not native registrations or index contents. Setup rechecks before writing configuration.

After a fresh installation or a reviewed storage change, use the explicit **Update existing AIRK connection** action for Codex, or **Install** in the supported JSON-client guide. These actions can replace the storage path and project identity together for the same source, collection and recognized launcher. Recognized legacy installed launches migrate using their matching launcher, canonical source and collection even if local binding receipts have been erased. Obsolete data/binding selectors are removed together. Unknown commands, other source roots/collections and custom launch arguments remain for review. Root-based entries remain unchanged across binding/storage replacement. Merely replacing `service-1.4` with `index-v1` in an existing configuration is insufficient because storage participates in the native project identity. Codex saves retain the document's newline convention and read back the saved file. Both creation and update automatically probe AIRK; the guide distinguishes saved, already matching, and connection-check failure. This does not prove the external agent loaded its configuration or override its tool/disabled policies.

Index jobs, sources, publications and model settings are unchanged by connection migration. The service remains the sole owner of indexing and recovery. See [index management](index-management.md), [the evidence contract](mcp.md) and the [1.5.7 development record](release-1.5.7.md).

Successful lightweight status snapshots are cached in memory for less than five seconds per native client/project. The result includes the responding AIRK `version`, basic `health`, collection/scope, index counts and an exact publication token. Source and semantic states are summarized without pipeline/selection digests or detailed coverage metadata. `observed_unix_ms`, `cache_age_ms` and `cached` disclose the observation and cache age. Each cache hit rechecks active registration and registry incarnation; removed or recreated bindings cannot reuse the old cache. Unavailable/unknown/error results are not cached. Diagnostics bypasses and invalidates this cache. No background index work or periodic status polling is added to agent connections. Cached responses are snapshots, not live-source verification.

Cold installed-runtime verification is shared within each MCP session for both source-scoped and explicitly pinned connections. A quick-status timeout reports unknown readiness with null evidence and preserves that pending verification for later requests. A failed verification never becomes ready, and session shutdown cancels its pending task. This does not replay submitted evidence queries or weaken payload, control-root or health identity checks.
