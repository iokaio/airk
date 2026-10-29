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
3. **Check AIRK MCP connection** verifies the launcher and unified tool catalog. Restart the coding agent and complete its own trust and renamed-tool approvals.
4. Ask the agent to use status and capabilities, then search for a known phrase and cite its publication and source. Use diagnostics for runtime versions, jobs and diagnostics.explorer UI attachment; status stays lightweight.
5. In multi-root workspaces, projects lists the connection's configured bindings. Pass project explicitly; changing the UI dropdown does not redirect retrieval.

User-wide and client-private configuration/approval stores require that client's controls. A manual recognized VS Code configuration owns its policies; the extension does not register a duplicate connection. Read Work with an AI agent for draft validation, templates, proposals and shared snapshots.

After reinstalling AIRK with fresh storage, an older agent configuration may still reference the previous index. **Update existing AIRK connection** for Codex, or **Install** for supported JSON clients, replaces a recognized entry with the stable launcher and `--root SOURCE mcp`. Requests resolve the current active registration without persisting storage paths or internal project bindings. Setup checks the native registration first. Codex creation/update checks the saved connection automatically and distinguishes a successful save from a failed AIRK check; restart the agent separately to load it. Existing sessions retain their old executable and catalog until restarted or reconnected. Unsaved edits, custom launchers and ambiguous registrations are preserved and reported for review.

Codex, Claude Code, Copilot and other assistants can connect to the same project simultaneously. Each connection negotiates independently. Version 1.5.9 corrects discovery followed by a legacy handshake, which previously could deliver instructions but reject tool listing. A client's connected label alone does not prove tools are available: use **Check AIRK MCP connection** and verify an actual tool call. A failed freshness job is a separate indexing problem; it does not mean the MCP connection failed.

Management → Agents → **Check AIRK connection** shows its stages, elapsed time and result in the panel, with **Copy connection details**. It checks the selected project's current native registration, launches AIRK, lists the 27 unified tools and reads status without starting indexing. Failure details retain the failed stage and available launcher error output. A slow handshake can exceed an agent's own startup deadline even when this check eventually passes.

Ask an assistant to discover AIRK in its full tool catalog, including deferred tools, and call `status` before claiming the tools are unavailable. The extension's successful check does not establish that another agent loaded its configuration. Use that agent's MCP controls to check its server entry and collect the exact failure if access still fails. The copied verification prompt includes these steps.
