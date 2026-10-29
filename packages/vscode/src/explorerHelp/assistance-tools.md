---
id: assistance-tools
title: Work with an AI agent
route:
area: guidance
aliases: AI agent, MCP, explorer assistance tools, template discovery, validation, proposals, navigation, setup, codex, claude, copilot
related_airk_help: explorer, roles, agent-setup, mcp, workflows
---
# Work with an AI agent

AIRK 1.6.4 includes the runtime, project cleanup and diagnostics fixes from the unpublished 1.6.1–1.6.3 candidates. After upgrading, reconnect each agent so it loads the installed runtime and 27-tool catalog.

Connect one MCP server named airk for a registered project. Its 27 tools distinguish indexed evidence (search, read_file, symbols), documentation (help), quick readiness (status), detailed troubleshooting (diagnostics), and reviewed UI drafts (draft_search, draft_strategy, draft_plan, suggest_view). Native evidence remains available when Explorer is closed. Each agent loads its own configuration and completes its trust prompts.

## Steps

1. In Management > Agents, choose **Connect a coding agent…**. New configurations contain one airk entry. Startup and Install or Repair CLI replace recognized earlier Retrieval and Explorer entries together, preserving unrelated settings and disabled state. Custom conflicts and unsaved files require review.
2. Restart the agent after migration, enable the single AIRK server, and approve renamed tools in that client's controls. **Check AIRK MCP connection** verifies the unified catalog; the agent must still verify its own loaded configuration.
3. Call status and capabilities before using indexed evidence. status returns lightweight published readiness; use diagnostics for runtime identities, jobs and diagnostics.explorer attachment details. Use help with query to discover UI/engine guides and topic_id to read one; help with topic or command explains native usage.
4. For multiple configured roots, call projects and pass the matching project binding. Evidence never follows the Explorer dropdown. Select the agent's configured project in Explorer before sending drafts.
5. Search returns cited evidence now. draft_search only queues a form in Suggestions. Review proposal_status and wait for the user to apply it; applied never means executed.

## Templates and validation

list_templates pages through the installed pack. get_template returns the selected template's exact parameters, defaults and operator contracts; Preview does not mean runnable. validate_draft checks the corresponding proposal envelope without putting it in Suggestions. proposal_status reports review state and fixed reasons without returning the draft body.

## Navigation proposals

suggest_view with kind navigation and the proposal envelope in draft can propose an implemented screen. Code Symbols/References accept name and path_prefix; Documents Facts also accepts kind; Checker accepts text; indexed Changes accepts since. Other routes accept no filters. **Open screen** fills supported forms for the user to submit; it never runs a prefilled query or approves management work.

## Sharing back to the agent

The agent cannot read arbitrary Studio or Planner drafts. The user reviews the displayed text and chooses **Share snapshot with agent**, confirms the modal, then gives the expiring token to the agent. get_handoff returns only that complete snapshot and its provenance. See Share a reviewed snapshot for limits and revocation.

## Troubleshooting

UI_NOT_ATTACHED means no matching trusted window; open the project in VS Code and enable Assistance. AMBIGUOUS_TARGET means more than one live window matches the agent's working directory; keep a single intended window for that project. HOST_UNAVAILABLE can follow a closed/reloaded host. STALE_BINDING requires fresh status and a newly reviewed proposal. PROPOSAL_CONFLICT requires reviewing the current draft; never overwrite the user's work automatically. RATE_LIMITED requires reviewing/discarding existing suggestions or waiting.

The attachment uses the configured project's canonical root to find a matching trusted window. Check the current selected project before proposing work in a multi-root window. Configure remote agents on the remote extension host; local-agent connectivity is not established by copying remote paths.

New agent entries use the stable launcher and `--root SOURCE mcp`. Each request resolves the current active registration for that source; it does not register or index the project. Missing, removed and ambiguous registrations are distinct outcomes. Use `--index-collection` only to disambiguate multiple registered indexes. Restart or reconnect existing agent sessions after an AIRK upgrade: an already running MCP process retains its previous executable and tool catalog.

A status timeout reports unknown readiness. Use diagnostics to inspect runtime and work separately. Cached status includes its observation age and never proves that live source bytes match the publication.
