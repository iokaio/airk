---
id: assistance-tools
title: Work with an AI agent
route:
area: guidance
aliases: AI agent, MCP, explorer assistance tools, template discovery, validation, proposals, navigation, setup, codex, claude, copilot, list_scopes, workspace default, mcp roots
related_airk_help: explorer, roles, agent-setup, mcp, workflows, arbiter
---
# Work with an AI agent

AIRK agents connect directly to the native service over loopback HTTPS; the endpoint needs no token or credential. VS Code is an optional setup interface; closing Explorer does not proxy or end a standalone agent connection. Inspect the running catalog rather than relying on a fixed tool count. Reconnect an agent after changing its saved configuration or upgrading the service when its tool catalog needs refreshing.

Connect one MCP server named airk; agents choose which indexes they read on each call. Its tools distinguish indexed evidence (search, read_file, symbols), documentation (help), quick readiness (status), detailed troubleshooting (diagnostics), and reviewed UI drafts (draft_search, draft_strategy, draft_plan, suggest_view). Native evidence remains available when Explorer is closed. Each agent loads its own configuration and completes its trust prompts.

## Steps

1. In Agents choose the agent and **Connect**. This writes the agent's entry with only the endpoint URL and the X-AIRK-Response-Mode header. The native writer preserves unrelated servers, comments, disabled states and client policies. Changed files, unsafe ownership and unsaved edits produce a refusal with recovery information; they are not permission to overwrite the file.
2. Load the changed entry in the agent and complete that client's trust controls. Claude Code may need its MCP reconnect action, Codex a restart, and VS Code a server restart. A native configuration report does not prove that a running external agent loaded it.
3. Call status and capabilities before using indexed evidence. status returns lightweight published readiness; use diagnostics for runtime identities, jobs and diagnostics.explorer attachment details. Use help with query to discover UI/engine guides and topic_id to read one; help with topic or command explains native usage.
4. Scope is a parameter of every read: index_ids and index_set_ids (ids or names, a union) and workspace (an absolute folder path). list_scopes lists the indexes and index sets with their descriptions and the stored workspace defaults. A call that names no indexes uses the workspace default of the most specific stored folder containing workspace; the search scope applied in Indexes > Search scope is the workspace default of every folder of that window. Without a default the call is refused with scope_required. When the agent supports MCP roots and passes no workspace, its first workspace root is used automatically. Results report which scope was used and retain each index's full provenance and failures.
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

New entries use the discovered HTTPS endpoint and AIRK's local certificate trust; no token or credential helper is written. Stdio MCP and the stable launcher are retired. Reads never create an index. Use airkctl agents --help, airkctl scope --help and the actual native reports for headless setup and repair. A changed workspace default applies to later calls; a client may need to refresh its cached tools list after an upgrade.

A status timeout reports unknown readiness. Cold installed-runtime verification stays shared within the MCP session, so later polls do not restart it; verification failures remain errors. Use diagnostics to inspect runtime and work separately. Cached status includes its observation age and never proves that live source bytes match the publication.

## Questions and contextual evidence

The `answer` tool gathers a bounded cited evidence packet for a question; `context` expands neighboring indexed lines at a specific location. The assistant can use the packet's organized source-backed evidence, assessments, gaps and stop reason when composing its response. These are read-only operations within the scope the call names or its workspace default.

The approved optional design has independent Focus ranking plus Arbiter gathering (Qwen3-1.7B; the Large model was retired). Focus can assist ordinary retrieval with Context off; neither stage widens the scope a call names. Availability depends on actual implemented controls, verified profiles, explicit stage/index enrollment and current resources. Inspect native Arbiter Help and status; a selected or downloaded model does not establish active readiness or completed model qualification.
