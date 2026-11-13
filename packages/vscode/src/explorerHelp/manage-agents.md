---
id: management.agents
title: Agent connections
route: manage/agents
area: management
aliases: connect codex, connect claude, copilot, mcp setup, response mode, cli, agent choice, open connection, default scope
related_airk_help: agent-setup, mcp
---
# Agent connections

Management › Agents connects coding agents to this window's folder. Choose the agent (Claude Code, Codex or VS Code / GitHub Copilot) and choose **Connect**. Connect creates an open connection for the agent (`airkctl agents connect --open`), writes the agent's configuration entry and sets the connection's **default scope** to this window's search scope (`airkctl agents default`). When no search scope is applied, choose one in Indexing › Search scope first.

An open connection admits the agent to AIRK; which indexes it reads is the default scope that AIRK Service stores for the connection. A connection connected here **follows this window's search scope**: whenever you apply another scope in Indexing › Search scope, or the window's index is chosen automatically, its default scope is updated. A change takes effect on the agent's next call; the agent does not reconnect. Clearing the window's search scope clears the default scope, and the agent's searches are refused until a scope is applied again. Turn **Follow this window's search scope** off to keep a connection's current default scope. If a scope cannot be passed on, the Agents screen says why; your scope change still applies.

**Let the agent choose indexes** is off by default:

- **Off**: the agent searches only its default scope. It cannot list or pick indexes, and a call that names an index or index set is refused.
- **On**: the agent can list every index and index set on this computer, with their descriptions and indexed folders, and search any of them. A call that names nothing still uses the default scope. Any agent with this setting on can read every index of your user account.

After the setting changes, a running agent may need to reload its tool list: in Claude Code run `/mcp`; restart Codex; in VS Code restart the AIRK server from MCP: List Servers if the tool list does not change. Descriptions are edited in Indexing › Search scope.

**Connected agents** lists the AIRK entries in this folder's agent configuration files (`airkctl agents list`) with their file, connection and status, the complete native report and, for each open connection, its default scope, Follow setting and choice switch. A connection created in AIRK Admin for a reviewed scope keeps the scope it was connected with and is managed in AIRK Admin. **Disconnect** removes the entries that name that connection (`airkctl agents remove --connection-id`); other servers and settings in the file are kept. An open connection that no remaining entry names is then revoked (`airkctl agents revoke`).

Choose **Open AIRK Admin** for advanced connection management. If the VS Code browser reports a certificate problem, choose Open in my browser.

## Steps

1. Apply a search scope in Indexing › Search scope.
2. In Management › Agents, choose the agent and **Connect**.
3. If the agent was already running when its entry was first written, load the entry: Claude Code with `/mcp` or a restart, Codex with a restart; VS Code starts the MCP server from the new entry. Connecting an agent that is already connected here only updates its default scope; no reconnect is needed.
4. Optionally turn on **Let the agent choose indexes** for that agent.
