# One AIRK MCP connection

The 1.5.7 development branch exposes one server named `airk`, with 26 unique, unprefixed tools. Evidence tools answer from the configured index. Help explains AIRK. Draft tools prepare suggestions that wait for the user in Explorer. This removes duplicate status/help entry points and the need for an agent to choose between two AIRK servers.

Tool names are unique within this server; the client may add its own server namespace. Repeating `airk_` on every tool adds no distinction between reading evidence and proposing a form. Verbs and descriptions carry that distinction. Previously saved `airk_*` and `airk_explorer_*` calls remain callable compatibility aliases; they are omitted from discovery so agents see one catalog. Native `airk tools` retains its sixteen evidence aliases for existing terminal scripts and Explorer's internal evidence adapter.

| Tool | Description |
|---|---|
| `projects` | List only the bindings configured on this connection and its default. Does not enumerate or register every machine project. |
| `status` | Observe published availability, freshness, semantics and current work, with Explorer attachment reported separately. Does not index or repair. |
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

A project-local connection retains its registered binding as the default. The VS Code provider offers one workspace connection; multiple configured roots require an explicit `project` binding from `projects`. Additional projects must be explicitly supplied as registered bindings of the same service. Changing the Explorer dropdown never redirects evidence queries. A UI request also checks the selected project at admission, so a selection change between status and proposal cannot put a draft into another project's view.

The native frontend owns stdio and direct evidence reads. The CLI composes an optional verified extension adapter for offline UI help, templates and window IPC. Kernel/store never depend on the editor. A missing, closed or invalid editor adapter cannot remove native tools or prevent evidence retrieval. Native help, catalog discovery and configured-project listing do not elect the service; the first native observation/query checks the selected runtime and service. Explicit Stop is respected. UI help requires the configured verified extension bundle, but no open window; UI drafts require a matching trusted window.

`status` keeps native root fields and adds `project` and `explorer`. Each component retains its own availability and observation age. A successful status envelope can report unavailable evidence; callers must require `index_state: available`, the expected collection and a publication token before treating results as indexed evidence. Source publication and semantic coverage remain separate observations. Native usage accounting stays at the evidence boundary; optional editor calls and the added UI portion of composite status are not measured as native evidence bytes.

## Startup migration

After verified runtime selection and native registration, trusted startup and **Install or Repair CLI** consolidate recognized existing project configurations in one guarded save per file:

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

Index jobs, sources, publications and model settings are unchanged by connection migration. The service remains the sole owner of indexing and recovery. See [index management](index-management.md), [the evidence contract](mcp.md) and the [1.5.7 development record](release-1.5.7.md).
