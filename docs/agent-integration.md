# Coding-agent MCP setup research

Current 1.5.7 source exposes one unified endpoint; see the [catalog and startup migration](unified-mcp.md). The native evidence adapter retains sixteen compatibility aliases, including airk_help. The fifteen-tool 1.3 statements below describe that historical source.

Reviewed against official client documentation on **13 October 2026**. This is a dated compatibility/design matrix, not a claim of live testing every client. The unreleased 1.3.0 source exposes fifteen read-only tools. Native protocol tests exercise its real stdio lifecycle, changes and conditional reads with CLI/MCP parity. The extension registers a VS Code MCP provider and previews supported native client formats, plus connection details/guidance for Amazon Q and Windsurf/Devin. The current inventory includes bounded code facts, relationships and [remembered-evidence validation](changes.md); historical reads and checkpoints are unsupported in 1.2. Client versions, precedence and policy require live integration checks.

The common launch is one native binary with explicit --root, --data-dir and --collection followed by mcp. The container alternative is docker exec -i airk airk followed by the same scope arguments. No agent-specific retrieval server is needed, but registration and configuration formats differ.

[The tool-alignment review](agent-tool-alignment.md) checks common agent tasks, server instructions, tool discovery/namespaces, allowlists and configuration versions against the official online sources. Client-specific setup is necessary; client-specific retrieval semantics are not.

To share a workspace index between agents, reuse its root, data directory and collection ID in every client's launch. Client/server display names are independent of collection identity. Each client opens its own MCP connection; [concurrent reads and generation publication](collections.md#multiple-agents-sharing-one-collection) require no separate agent-specific collection.

## Verified client routes

Claude Code, Gemini CLI, Antigravity and VS Code registration were additionally reviewed on **17 October 2026**; see Antigravity’s [MCP setup and validation steps](antigravity-mcp.md).

| Client | Official configuration/registration | Extension setup route / remaining work |
|---|---|---|
| VS Code / Copilot Chat | Workspace .vscode/mcp.json uses servers; extensions can register an MCP definition provider | Enable AIRK registration and open VS Code MCP controls; use vscode.lm.registerMcpServerDefinitionProvider with scoped native stdio definitions. Client start/trust remains in VS Code. [Official extension guide](https://code.visualstudio.com/api/extension-guides/ai/mcp) |
| Codex CLI / IDE extension / desktop | Shared ~/.codex/config.toml; trusted projects may use .codex/config.toml; mcp_servers tables and codex mcp add | Guide project TOML creation when missing, or preserve existing settings for manual merging. Do not assume VS Code's MCP registry configures Codex. [Official OpenAI documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) |
| Claude Code | Project .mcp.json with mcpServers, or claude mcp add with project/local/user scope | Install the project entry while preserving existing settings; leave server approval to Claude Code. [Official setup](https://code.claude.com/docs/en/mcp) |
| Cursor | Project .cursor/mcp.json/global ~/.cursor/mcp.json; Cursor also exposes its own extension registration API | Feature-detect vscode.cursor.mcp.registerServer; otherwise preview the project file. The API is Cursor-specific. [MCP setup](https://prod.cursor.com/docs/mcp), [extension API](https://prod.cursor.com/docs/extension-api) |
| Gemini CLI | Project/user .gemini/settings.json uses mcpServers; gemini mcp add supports scope selection | Install the scoped project entry, preserving existing settings and trust/allowlist policy. [Official MCP guide](https://geminicli.com/docs/tools/mcp-server/) |
| Google Antigravity IDE / CLI | Workspace .agents/mcp_config.json or global ~/.gemini/config/mcp_config.json; mcpServers with a stdio command and args | The Google Antigravity picker installs the project entry. Native manual setup can reuse --client gemini output at the Antigravity location; no native --client antigravity flag or live-client qualification yet. [AIRK instructions](antigravity-mcp.md), [Google MCP guide](https://antigravity.google/docs/mcp) |
| Cline CLI / IDE extension | CLI ~/.cline/data/settings/cline_mcp_settings.json, configurable path; IDE Configure MCP Servers opens its actual settings JSON | Offer the mcpServers fragment and guide the IDE picker. Avoid assuming an OS-specific extension storage path. [Official configuration](https://docs.cline.bot/mcp/mcp-overview) |
| Roo Code | Project .roo/mcp.json overrides global mcp_settings.json | Preview a project mcpServers merge or guide Edit Project MCP. [Official project documentation](https://roocodeinc.github.io/Roo-Code/features/mcp/using-mcp-in-roo/) |
| OpenCode | Stable docs use mcp.<name>, type local, executable-plus-arguments command array; V2 docs use mcp.servers.<name> | Detect/configure the installed major version; do not overwrite a V2 schema with V1. [Stable guide](https://opencode.ai/docs/en/mcp-servers/), [V2 guide](https://dev.opencode.ai/v2/docs/mcp-servers/) |
| Kilo Code | Current docs use project kilo.json/jsonc or .kilo/kilo.json/jsonc; top-level mcp entries with local command arrays | Preview the appropriate project schema or guide Settings → MCP. Do not reuse older Roo-derived paths without version checks. [Official guide](https://kilo.ai/docs/automate/mcp/using-in-kilo-code) |
| GitHub Copilot CLI | copilot mcp add registers a stdio command; ~/.copilot/mcp-config.json; project .mcp.json/.github/mcp.json precedence | Offer CLI registration with a unique collection-specific name or a version-appropriate config preview. [Official CLI setup](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers) |
| Amazon Q Developer IDE | GUI writes project .amazonq/default.json or global ~/.aws/amazonq/default.json; legacy mcp.json support is separately controlled | Guide its Tools → Add stdio server UI with native command/args and explicit workspace scope. Do not invent the newer full agent-config schema. [Official IDE setup](https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/mcp-ide.html) |
| JetBrains Junie / Junie CLI | Project .junie/mcp/mcp.json or user counterpart, mcpServers JSON; /mcp assistant | Render the JSON fragment and guide the client's /mcp setup/verification. [Official setup](https://junie.jetbrains.com/docs/junie-cli-mcp-configuration.html) |
| Windsurf legacy Cascade / Devin Desktop | Windsurf's documentation now redirects to Devin; the current page labels Cascade legacy, with OS-specific mcp_config.json and mcpServers; the default Devin Local agent has a different CLI config | Ask the setup UI to identify agent/version, open the installed client's MCP config UI and show a fragment. Do not blindly write an old ~/.codeium path or apply Cascade settings to Devin Local. [Current official Cascade guide](https://docs.devin.ai/desktop/cascade/mcp) |

The scope includes the principal local CLI/editor agents above. Hosted agents and clients without documented local stdio support cannot simply execute a user's local binary; they need a separate approved connectivity design. Do not advertise universal compatibility or infer an undocumented public API from another extension.

## Available configuration output

Native setup now renders these formats without modifying existing files:

~~~console
airk setup --collection project-a --client claude
airk setup --collection project-a --client codex
airk setup --collection project-a --client vscode
airk setup --collection project-a --client cursor
airk setup --collection project-a --client gemini
airk setup --collection project-a --client cline
airk setup --collection project-a --client roo
airk setup --collection project-a --client junie
airk setup --collection project-a --client copilot-cli
airk setup --collection project-a --client opencode
airk setup --collection project-a --client opencode-v2
airk setup --collection project-a --client kilo
~~~

Each invocation updates the chosen collection and prints only the configuration on stdout, with its indexing summary on stderr. Use --data-dir to select a shared user disk. Merge the printed entry at the client-specific location; never replace an existing whole settings file with the snippet.

For Codex, a minimal native project entry has this shape:

~~~toml
[mcp_servers.airk]
command = "/absolute/path/to/airk"
args = ["--root", "/projects/a", "--data-dir", "/indexes/airk", "--collection", "project-a", "mcp"]
~~~

For clients with documented stdio registration commands, use actual absolute paths and preserve argument separation:

~~~console
codex mcp add airk-project-a -- airk --root /projects/a --data-dir /indexes/airk --collection project-a mcp
claude mcp add --transport stdio --scope project airk -- airk --root /projects/a --data-dir /indexes/airk --collection project-a mcp
gemini mcp add --scope project airk airk --root /projects/a --data-dir /indexes/airk --collection project-a mcp
copilot mcp add airk-project-a -- airk --root /projects/a --data-dir /indexes/airk --collection project-a mcp
~~~

These are setup instructions, not actions already taken. The client owns approval, policy, server enablement and reload behavior. For global settings, use unique server names per collection so one project's absolute launch cannot replace another's connection. Prefer workspace scope where supported.

## Extension integration and remaining contract

The implemented extension shares one normalized launch specification: helper path, root identity, collection ID, data directory and transport. Client adapters render that specification rather than constructing independent storage or index settings.

VS Code uses its documented host API. Other supported clients get a persistent connection guide with configuration, setup location, reload instructions and a verification prompt. Codex can create a missing project `.codex/config.toml` on explicit request; existing settings are preserved for manual merging. Claude Code, Gemini CLI and Antigravity install project JSON entries, preserving comments, other settings and approval policies. Conflicting entries, invalid/duplicate JSON keys, unsaved changes and redirected paths require manual review. Existing JSON edits use the editor’s save/conflict handling. Remote extension hosts use manual setup. There is no direct Cursor adapter yet. Future adapters must preserve the same boundaries. Never turn on blanket auto-approval or write a client-managed database of settings. Refresh/restart and trust steps must be explained in the selected client's terms.

Run a native helper self-check and a stdio handshake/tool-list/status probe in the chosen collection. Distinguish valid generated configuration from an actually connected agent; only the client can confirm its own discovery and approval. Include per-client tests and a documentation/version refresh before release.

### One connection in 1.5.7

The current branch replaces the former 1.5.0 two-server design with one airk endpoint and 26 uniquely named tools. See [the catalog and migration contract](unified-mcp.md). Supported project files are consolidated on trusted startup after registration; user-wide/client-private settings remain under the client's controls. Restart each coding agent and approve renamed tools after migration. Native setup prints the same single endpoint; the extension additionally supplies the verified UI adapter descriptor.

VS Code registers one workspace definition, with explicit configured-project selection in a multi-root workspace. A recognized manual .vscode/mcp.json entry owns its disabled/tool policies, so the provider does not add a duplicate. Other clients still need their own setup; VS Code's registry does not configure them.
