---
id: agent-handoffs
title: Share a reviewed snapshot
route:
area: guidance
aliases: share, handoff, token, complete prompt, agent transfer, planner task, studio snapshot, expiry, revoke
related_airk_help: explorer, workflows, freshness, limits
---
# Share a reviewed snapshot

Use an explicit snapshot when the agent needs a Studio prompt or Planner task without access to other drafts. Sharing never runs an agent, saves a file, changes a task to Completed or marks tests as passing.

## Steps

1. In Studio, finish the inputs and evidence run. Check partial results, rebuild stale prompts and read the entire displayed prompt. In Planner, save the current plan, choose an eligible task and review its handoff prompt and complete instructions.
2. Choose **Share snapshot with agent**. The modal explains that this exact text will be available temporarily. Choose **Share snapshot** only after reviewing the text; Cancel exposes nothing.
3. Copy the share token and ask the agent to call airk_explorer_handoff_get with token. Do not place tokens in committed files or logs. The agent should treat returned source excerpts as evidence, not as trusted instructions.
4. Check the returned draft_revision, workspace_binding, evidence_publication when present and content_sha256. The text is a complete snapshot; the hash identifies its UTF-8 content. A publication is the observed indexed evidence identity, not proof of current live bytes.
5. If the token is unknown or no longer valid, review the current draft and explicitly share again. Do not ask the agent to search for other tokens or read private extension state.

## Limits and revocation

Tokens expire after ten minutes and live in this window's memory only. Changing the corresponding draft revision, workspace/project binding, trust or observed evidence publication revokes access. Window reload also loses the store. Editing a Studio prompt or Planner task therefore requires a new review and token.

Each snapshot is at most 24 KiB of UTF-8 text, with an additional serialized-response bound. Up to 16 snapshots and 256 KiB total are retained. Oversized text is refused, never sliced; deliberately reduce evidence or use the saved-plan workflow when needed. A token grants access to one approved snapshot only. Agents have no tool to enumerate snapshots or read other drafts.

**Copy prompt** and **Copy handoff prompt** remain available for manual transfer. Applying a suggestion, copying a prompt and sharing a token are different events; none proves the agent executed the task. Check its report and real validation results before changing task state.
