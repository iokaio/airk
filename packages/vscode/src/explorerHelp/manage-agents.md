---
id: management.agents
title: Agent connections
route: manage/agents
area: management
aliases: connect codex, connect claude, copilot, mcp setup, response mode, cli
related_airk_help: agent-setup, mcp
---
# Agent connections

**Help your agent discover and use AIRK** provides copyable project instructions and separate buttons for Claude Code's CLAUDE.md, Codex's AGENTS.md and GitHub Copilot's .github/copilot-instructions.md. Open **Instruction file help** for the review/save steps and guidance for other assistants. Existing files are preserved; missing files open as unsaved drafts.

Open this section from the tabs across the top of Management. Everyday controls remain in the Indexing tab: Search mode, Updates and Evidence detail.

Management > Agents connects coding agents to one AIRK MCP server for evidence, help and reviewed Explorer drafts.

## Steps

1. Choose **Connect a coding agent…** and the project that agent should query.
2. Review the single airk connection. New project configurations use it; trusted startup consolidates recognized earlier Retrieval and Explorer entries in supported project files. Other servers and disabled state are preserved. Custom conflicts and unsaved files require review.
3. **Check AIRK MCP connection** verifies the HTTPS connection and unified tool catalog. Restart the coding agent and complete its own trust and renamed-tool approvals.
4. Ask the agent to use status and capabilities, then search for a known phrase and cite its publication and source. Use diagnostics for runtime versions, jobs and diagnostics.explorer UI attachment; status stays lightweight.
5. In multi-root workspaces, projects lists the connection's configured bindings. Pass project explicitly; changing the UI dropdown does not redirect retrieval.

User-wide and client-private configuration/approval stores require that client's controls. A manual recognized VS Code configuration owns its policies; the extension does not register a duplicate connection. Read Work with an AI agent for draft validation, templates, proposals and shared snapshots.

Agents connect directly to the service's HTTPS address; no AIRK process runs per agent, and stdio entries are retired. Claude Code and Codex entries include a one-shot `airk runtime http-credential` helper that prints the token and exits, so agents started outside VS Code can authenticate.

After an installation replaced an earlier AIRK, an agent entry may still name a removed launcher, an unknown connection, a changed address, missing trust or a missing helper. This section lists those findings, together with the Explorer banner and status bar warning. **Reconnect** shows which existing index and entries it will use, and asks once. It never creates an index. If no index exists for the recorded folder, the result is `awaiting_index`: create one explicitly in **Indexing → Index library**, then reconnect. If several indexes match, apply the intended selection in **Indexing → Search scope** first.

Reconnect uses the matching member of the reviewed native selection, including combined selections, and grants access only to that index. It can make an existing index this window's search scope, enroll HTTPS and rewrite each entry in place, keeping other servers, comments and policies. Completed scope changes retain their outcomes even if a later connection step fails. Each entry reports a typed status, such as `complete` or `awaiting_host_reconnect` with the agent's own reconnect action. Restart or reconnect the agent to load it; AIRK never claims that a running agent reconnected. Unsaved edits, custom entries and ambiguous registrations are preserved and reported for review.

Codex, Claude Code, Copilot and other assistants can connect to the same project simultaneously. Each connection negotiates independently. Version 1.5.9 corrects discovery followed by a legacy handshake, which previously could deliver instructions but reject tool listing. A client's connected label alone does not prove tools are available: use **Check AIRK MCP connection** and verify an actual tool call. A failed freshness job is a separate indexing problem; it does not mean the MCP connection failed.

Management → Agents → **Check AIRK connection** shows its stages, elapsed time and result in the panel, with **Copy connection details**. It checks the selected project's current native registration, connects over HTTPS, lists the 29 unified tools and reads status without starting indexing. Failure details retain the failed stage and error output. A slow handshake can exceed an agent's own startup deadline even when this check eventually passes.

Ask an assistant to discover AIRK in its full tool catalog, including deferred tools, and call `status` before claiming the tools are unavailable. The extension's successful check does not establish that another agent loaded its configuration. Use that agent's MCP controls to check its server entry and collect the exact failure if access still fails. The copied verification prompt includes these steps.
