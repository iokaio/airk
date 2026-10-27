---
id: strategies.planner
title: Strategy Planner
route: strategies/planner
area: strategies
aliases: plan, tasks, handoff, save plan, dependencies, completion
related_airk_help: workflows
---
# Strategy Planner

Strategies › Planner combines Studio snapshots and manual tasks into one Markdown plan with YAML front matter (schema airk.explorer.plan/1). A coding agent follows it one task at a time; the Explorer never runs the agent.

Task states are Draft, Ready, Exported, Review, Needs work and Completed. Blocked is derived from incomplete predecessors and Stale from evidence on an older publication; neither can be chosen by hand. Copying a handoff prompt marks a task Exported, never Completed, and Completed asks for confirmation.

## Steps

1. Choose **New plan**, or **Add to Planner** in Studio to add the current strategy and its prompt as a task.
2. Add manual tasks, edit instructions and tick dependencies. Cycles are refused.
3. Choose **Save in workspace…**. The save dialog opens in the airk.explorer.planner.directory folder (default .airk/plans) and is the consent to write; the file must stay inside the workspace folder, unsaved editor copies are never overwritten and replacing an existing file asks first.
4. After the save is confirmed, pick a task and choose **Copy handoff prompt**.
5. When the agent reports back, paste the report into **Completion check**, review the citation results and set the task to Review, Needs work or Completed.

**Open as draft** opens the Markdown as an untitled document without saving it. Editing the plan after saving requires saving again before the next handoff.
