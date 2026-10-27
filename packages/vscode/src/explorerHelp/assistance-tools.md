---
id: assistance-tools
title: Work with an AI agent
route:
area: guidance
aliases: AI agent, MCP, explorer assistance tools, template discovery, validation, proposals, navigation, setup, codex, claude, copilot
related_airk_help: explorer, roles, agent-setup, mcp, workflows
---
# Work with an AI agent

Connect two MCP servers for a registered project: AIRK Retrieval (airk-collection, 16 read-only evidence tools) and Explorer Assistance (airk_explorer, 14 help/UX tools). Each agent must load its own configuration and complete its normal trust prompts. A working VS Code MCP connection does not configure another agent.

## Steps

1. In Management > Agents, choose **Connect a coding agent…** for the project. New supported configurations include both connections. For an existing recognized project Retrieval entry, Install or Repair CLI adds the missing Assistance entry; Codex's guide also offers **Add or update Explorer Assistance beside existing AIRK Retrieval**. Custom conflicts, unsupported TOML and unsaved files remain for manual review. Disabling Assistance prevents automatic backfill.
2. Restart the agent. Use the guide's separate checks for AIRK MCP and Explorer Assistance, then send the supplied verification prompts to the agent. Launcher checks cannot prove the agent loaded them.
3. Ask the agent to call airk_explorer_ui_status. It reports the attached window, active route, opaque binding fingerprint/generation, target draft revisions and blockers. A closed Explorer or unavailable draft state can require opening it before preparing revision-bound proposals. No source paths or credentials are returned.
4. Ask airk_explorer_help_search for your role, purpose or source formats, then airk_explorer_help_get for the returned topic. Engine guides are bundled offline; use AIRK Retrieval airk_help for the running binary's exact command flags and tool schemas.
5. For strategy work, discover the installed templates with airk_explorer_templates_list, follow next_offset, then read airk_explorer_template_get. Preserve runnable/preview limitations, parameters, operator contracts and budgets.
6. Use airk_explorer_validate_draft with kind and the exact suggest-call arguments. Schema checks can work offline; live_checked=false means binding/revision applicability was not checked. Validation never reserves a key, consumes an Inbox rate slot or adds a proposal.
7. Submit a Search, Strategy, Plan, Review, Management or Navigation proposal. Reuse its idempotency key only for the identical body; use the fresh binding and applicable target revision from status. The user reviews it in Suggestions.
8. Read airk_explorer_proposal_status for the machine-readable reason and admitted/current/applied target revisions. Applied means copied into an Explorer draft or an approved screen transition; executed remains false. An expired/conflicted/rejected proposal does not authorize a retry with different contents under the same key.

## Navigation proposals

airk_explorer_suggest_navigation can propose an implemented screen. Code Symbols/References accept name and path_prefix; Documents Facts also accepts kind; Checker accepts text; indexed Changes accepts since. Other routes accept no filters. **Open screen** fills supported forms for the user to submit; it never runs a prefilled query or approves management work.

## Sharing back to the agent

The agent cannot read arbitrary Studio or Planner drafts. The user reviews the displayed text and chooses **Share snapshot with agent**, confirms the modal, then gives the expiring token to the agent. airk_explorer_handoff_get returns only that complete snapshot and its provenance. See Share a reviewed snapshot for limits and revocation.

## Troubleshooting

UI_NOT_ATTACHED means no matching trusted window; open the project in VS Code and enable Assistance. AMBIGUOUS_TARGET means more than one live window matches the agent's working directory; keep a single intended window for that project. HOST_UNAVAILABLE can follow a closed/reloaded host. STALE_BINDING requires fresh status and a newly reviewed proposal. PROPOSAL_CONFLICT requires reviewing the current draft; never overwrite the user's work automatically. RATE_LIMITED requires reviewing/discarding existing suggestions or waiting.

The attachment follows the agent's working directory into this window's workspace folders. Check the current selected project before proposing work in a multi-root window. Configure remote agents on the remote extension host; local-agent connectivity is not established by copying remote paths.
