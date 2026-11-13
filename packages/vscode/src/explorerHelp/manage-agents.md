---
id: management.agents
title: Agent connections
route: manage/agents
area: management
aliases: connect codex, connect claude, copilot, mcp setup, response mode, cli
related_airk_help: agent-setup, mcp
---
# Agent connections

Management › Agents connects coding agents to this window's folder. Choose the agent (Claude Code, Codex or VS Code / GitHub Copilot) and choose **Connect**; the agent reads the indexes in this window's search scope. When no search scope is applied, choose one in Indexing › Search scope first.

Connecting an agent to the search scope is being completed: in this version Connect says it is coming in the next step and changes nothing. Until then, connect agents in AIRK Admin › Agents.

**Connected agents** lists the AIRK entries in this folder's agent configuration files (`airkctl agents list`) with their file, connection and status, and the complete native report. **Disconnect** removes the entries that name that connection (`airkctl agents remove --connection-id`); other servers and settings in the file are kept.

Choose **Open AIRK Admin** for advanced connection management. If the VS Code browser reports a certificate problem, choose Open in my browser.

## Steps

1. Apply a search scope in Indexing › Search scope.
2. In Management › Agents, choose the agent and **Connect**.
3. Reconnect the running agent: Claude Code with `/mcp` or a restart, Codex with a restart; VS Code restarts the MCP server when its configuration changes.
