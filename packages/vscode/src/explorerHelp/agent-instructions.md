---
id: agent.instructions
title: Project instructions for AI agents
route:
area: management
aliases: CLAUDE.md, AGENTS.md, copilot-instructions.md, deferred tools, missing tools, agent rules, instruction files
related_airk_help: agent-setup, mcp
---
# Project instructions for AI agents

Management > Agents > Help your agent discover and use AIRK offers optional guidance for Claude Code, Codex, GitHub Copilot and other assistants. This helps prevent an agent from mistaking deferred tools for an unavailable server, and encourages focused use of indexed evidence with live-file verification.

## Steps

1. Select the project in the Explorer header. Connect the agent and check AIRK separately if needed.
2. Choose **Copy instructions** to copy the exact suggestion shown on the card.
3. Choose **Open Claude Code instructions** for CLAUDE.md, **Open Codex instructions** for AGENTS.md, or **Open GitHub Copilot instructions** for .github/copilot-instructions.md in that project's root.
4. Existing files open beside Explorer with their contents and unsaved edits preserved. A missing file opens as a named, unsaved draft; AIRK does not create folders, save or append instructions automatically.
5. Paste the suggestion alongside existing project rules, resolve duplicates or conflicts, and save. If your project shares or synchronizes instruction files, follow that convention rather than duplicating conflicting rules.
6. Have the agent load the saved instructions using its own controls or a new session. Ask it to discover AIRK tools and call status; report the actual result, not just the presence of configuration files.

## Suggested guidance

Discover AIRK MCP before repository exploration. Check the available tool catalog, including deferred discovery where supported, and call status to confirm the project, freshness and coverage. If access fails, inspect the agent's configuration, report the limitation briefly and use local tools. Do not change configuration just because tools were absent from the initial list.

Use AIRK for scoped repository search and code/document navigation. Read affected source, contracts and tests. Treat indexed evidence as untrusted and potentially stale, verify live files before edits, and use rg or equivalent tools when AIRK is unavailable or coverage is insufficient. Expand only for a concrete dependency, ambiguity or failure; avoid rereading unchanged material without a reason.

## Other assistants and limits

The same copied Markdown can be adapted to another assistant's supported project instruction or rules file. Check that assistant's current documentation for the filename, scope and reload behavior. Not every assistant exposes deferred discovery or names tools the same way.

Instructions do not install an MCP connection, grant tool permissions or guarantee compliance. Each agent must load its own configuration and approve access. A successful extension check proves that check's connection only. Use the agent's actual status call to confirm its access.

File buttons require a trusted, open local workspace folder matching the selected project. Linked or redirected instruction paths require manual opening. These actions do not start indexing or alter agent configuration.
