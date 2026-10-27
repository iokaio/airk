---
id: management.agents
title: Agent connections
route: manage/agents
area: management
aliases: connect codex, connect claude, copilot, mcp setup, response mode, cli
related_airk_help: agent-setup, mcp
---
# Agent connections

Management › Agents connects coding agents to AIRK Retrieval, the 16 read-only tools scoped to one project, and Explorer Assistance, the separate 14-tool help and UX connection.

## Steps

1. **Connect a coding agent…** picks the agent and writes or shows its configuration.
2. Check the guide's two labeled connections. Supported new project configurations include both. Existing recognized Retrieval-only project configurations gain Assistance during Install or Repair CLI; the Codex guide can update an existing recognized entry explicitly. Custom conflicts, disabled settings and unsaved editor files are preserved.
3. **Check AIRK MCP connection** starts the Retrieval launcher and lists its tools. **Check Explorer Assistance** verifies its launcher, bundled version, tool inventory and help answer separately.
4. Restart the agent session so it loads both connections. Send the separate verification prompts in that agent; a helper check does not establish that the agent loaded its config.

Read Work with an AI agent for template discovery, side-effect-free validation, proposal status, navigation and shared handoffs. Assistance attaches to the trusted window containing the agent's working directory; inspect its current project/binding before proposing work in a multi-root workspace.

## Limits

Helper verification proves the launcher works. Whether an agent loaded it is checked in that agent.

## Terse or Full responses

**Terse** is the default for routine searches and navigation. It keeps focused evidence and citations while omitting repeated metadata, so more of the agent's context budget remains for its task. **Full** adds detailed provenance, readiness and coverage information for missing or surprising results, freshness problems, parser limitations and evidence audits. Full produces larger responses; it does not make the index more complete.

An agent can request response_mode: full for one call. Reconnect its MCP session after changing the default.
