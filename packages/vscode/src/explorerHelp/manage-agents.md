---
id: management.agents
title: Agent connections
route: manage/agents
area: management
aliases: connect codex, connect claude, copilot, mcp setup, response mode, cli
related_airk_help: agent-setup, mcp
---
# Agent connections

Open this section from the tabs across the top of Management. Everyday controls remain in the Indexing tab: Search mode, Updates and Evidence detail.

Management > Agents connects coding agents to one AIRK MCP server for evidence, help and reviewed Explorer drafts.

## Steps

1. Choose **Connect a coding agent…** and the project that agent should query.
2. Review the single airk connection. New project configurations use it; trusted startup consolidates recognized earlier Retrieval and Explorer entries in supported project files. Other servers and disabled state are preserved. Custom conflicts and unsaved files require review.
3. **Check AIRK MCP connection** verifies the launcher and unified tool catalog. Restart the coding agent and complete its own trust and renamed-tool approvals.
4. Ask the agent to use status and capabilities, then search for a known phrase and cite its publication and source. status.explorer reports UI attachment separately.
5. In multi-root workspaces, projects lists the connection's configured bindings. Pass project explicitly; changing the UI dropdown does not redirect retrieval.

User-wide and client-private configuration/approval stores require that client's controls. A manual recognized VS Code configuration owns its policies; the extension does not register a duplicate connection. Read Work with an AI agent for draft validation, templates, proposals and shared snapshots.

Codex, Claude Code, Copilot and other assistants can connect to the same project simultaneously. Each connection negotiates independently. Version 1.5.9 corrects discovery followed by a legacy handshake, which previously could deliver instructions but reject tool listing. A client's connected label alone does not prove tools are available: use **Check AIRK MCP connection** and verify an actual tool call. A failed freshness job is a separate indexing problem; it does not mean the MCP connection failed.
