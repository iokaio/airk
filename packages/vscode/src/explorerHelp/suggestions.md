---
id: explorer.suggestions
title: Agent suggestions inbox
route:
area: explorer
aliases: inbox, agent drafts, proposals, suggest search, suggest strategy, suggest plan
related_airk_help: workflows
---
# Agent suggestions inbox

Coding agents connected to Explorer Assistance can propose a Search form, a Studio strategy, a Planner plan, a Review tab to look at, or management guidance that opens one Management screen. Each proposal waits in this window's Suggestions inbox; nothing is applied, run or saved until you act. Open it with the **Suggestions** button in the Explorer header, the count in the AIRK sidebar, or **AIRK: Show Agent Suggestions**.

Each card shows the agent's self-reported name (not authenticated), when it arrived, where it would go, its status and any warnings, such as evidence that cites an older publication. Cards with the same destination and title collapse into one with a count, and at most 20 are shown.

## Steps

1. Choose **Review** to read the draft as text.
2. Choose **Apply to draft**. A search fills the Search form without running it; a strategy goes to Studio and a plan to Planner. Review and management suggestions show **Open screen** instead: it opens the screen and runs nothing, and every action there asks for your confirmation.
3. If Studio has work, the Planner already has a plan, or the target changed since the agent wrote the draft, compare the fields and choose to replace, merge or append, or cancel.
4. Or choose **Defer** or **Reject**. **Discard all** rejects every open draft for this window.

Drafts live in memory for this window only and expire after airk.explorer.suggestions.ttlHours (default 24). Clicking a card never applies it.
