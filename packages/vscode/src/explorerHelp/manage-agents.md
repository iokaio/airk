---
id: management.agents
title: Agent connections
route: manage/agents
area: management
aliases: connect codex, connect claude, copilot, mcp setup, response mode, cli, workspace default, scope, mcp roots, list_scopes
related_airk_help: agent-setup, mcp
---
# Agent connections

Agents connects coding agents to this window's folder. Choose the agent (Claude Code, Codex or VS Code / GitHub Copilot) and choose **Connect**. Connect writes AIRK's entry in that agent's project configuration (`airkctl agents connect`): only the MCP endpoint URL and the `X-AIRK-Response-Mode: terse` header. AIRK keeps your other servers and settings in the file. The line under the agent shows whether this folder has an entry for it and its status; **Remove** deletes this folder's AIRK entry for that agent.

The endpoint line shows the local MCP address (`http://127.0.0.1:PORT/mcp`) with **Copy**. The endpoint needs no token or credential: there is no connection to create, renew or revoke. It is plain HTTP on 127.0.0.1, with no certificate to trust; AIRK Admin › Agents shows manual configuration for each agent. An older `https://` entry is reported as pointing to an earlier address, and Reconnect rewrites it. Any local process that can reach 127.0.0.1 can read your indexes through it.

## Set up manually

**Set up manually** is always available for the selected agent. It shows the exact native target path and complete JSON or TOML entry. **Copy complete entry** copies those same bytes. Merge the AIRK entry into the named file while keeping your other servers and settings.

Automatic editing can be refused when AIRK cannot safely retain the complete old file and its recovery copy, including a folder other accounts can change. The manual section opens with an explanation; **Details** retains the typed refusal and original diagnostic. A concurrent edit or unknown outcome requires reviewing the current file before changing it again. No folder permissions are changed to make an automatic edit eligible.

A port change requires a fresh entry. Copy it from this view again, or run `airkctl agents endpoint`. For a selected target, use `airkctl agents manual --client claude --project PROJECT`; replace `claude` with `codex` or `vscode`, and use `--user` for a supported user-level target.

## Which indexes an agent reads

Agents choose the scope on each call. Read tools take `index_ids` and `index_set_ids` (ids or names, read as a union) and `workspace` (an absolute folder path). `list_scopes` lists the indexes and index sets with their descriptions, and the stored workspace defaults.

When a call names no indexes or index sets, AIRK uses the **workspace default** of the most specific stored folder that contains `workspace`. On Windows folder names compare without case and `/` equals `\`. Without a matching default the call is refused with `scope_required`.

The search scope you apply in Indexes › Search scope is the workspace default of every folder of this window (`airkctl scope default --workspace FOLDER`). Clearing the search scope clears it. Defaults are stored by AIRK Service and survive restarts and closed windows; the Agents screen shows the default that applies to each folder of this window. If a default cannot be stored, the Agents screen says why; your scope change still applies. `airkctl scope defaults` lists every stored default.

If the agent supports MCP roots, it does not need to pass `workspace`: AIRK uses the agent's first workspace root, so its project folder's default applies automatically.

## Steps

1. Optionally apply a search scope in Indexes › Search scope, so calls that name no indexes have a default.
2. In Agents, choose the agent and **Connect**.
3. If the agent was already running when its entry was written, load it: Claude Code with `/mcp` or a restart, Codex with a restart; VS Code starts the MCP server from the new entry. Changing the search scope later needs no reconnect: the new default applies to the agent's next call.

**Details** lists every AIRK entry in this folder's agent files with the complete native reports. An entry written by an earlier version names a credential helper or token that is no longer used; choose **Connect** again to replace it with the URL-only entry.
