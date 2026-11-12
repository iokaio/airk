---
id: management.agents
title: Agent connections
route: manage/agents
area: management
aliases: connect codex, connect claude, copilot, mcp setup, response mode, cli
related_airk_help: agent-setup, mcp
---
# Agent connections

Coding-agent connections and their access are created and revoked in AIRK Admin, Agents. In VS Code, Connect Coding Agent writes an agent entry for an existing connection, and Reconnect Coding Agents repairs stale entries.

Management moved from the Explorer to the AIRK admin pages; this window reads indexes and evidence only. Its Management tabs open the matching admin page.

Choose Open AIRK Admin to open the admin pages. Choose where... picks In VS Code (the Simple Browser, with a single-use sign-in link kept only in memory) or In my browser; the choice is remembered in the airk.admin.openIn setting. If the VS Code browser reports a certificate problem, choose Open in my browser.

## Steps

1. Choose **Open AIRK Admin**.
2. In AIRK Admin, open **Agents** to see connections, ownership and the exact `airkctl agents ...` command to apply.
3. Run that command (or use this window's agent connection commands) to connect, repair or remove an agent entry.
