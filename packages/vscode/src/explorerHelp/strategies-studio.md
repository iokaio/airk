---
id: strategies.studio
title: Strategy Studio
route: strategies/studio
area: strategies
aliases: prompt compiler, evidence pipeline, run strategy, export strategy, import strategy
related_airk_help: workflows, search
---
# Strategy Studio

Strategies › Studio edits one strategy, runs it against one pinned AIRK publication, and compiles an editable prompt.

Steps come from a closed operator catalog: AIRK retrieval tools, a saved-file hash check and evidence transforms. There are no shell, write, network or recursion operators, and search steps are always lexical. Arguments can bind only ${params.*} and ${scope.*}. The first step is always airk.status, which pins the publication. Editing a core template creates a fork named user/NAME, and adding, removing or reordering a step clears the run.

## Steps

1. Write the objective and fill in the parameters and optional path scope.
2. Adjust the steps if needed; validation errors are listed and block Run.
3. Choose **Run**. The run pins the publication from the first status step and stops if the index is republished.
4. Read the outcome. A verified snapshot means every cited file's saved bytes matched the index; otherwise the run is partial and lists why (truncation, budgets, a step error or a live hash mismatch).
5. Choose the prompt profile: Portable includes the excerpts and assumes no AIRK; MCP-aware also asks the agent to re-check with AIRK if it has the tools.
6. Edit the prompt if needed, then **Copy prompt**. **Rebuild** asks before discarding edits.
7. Or choose **Share snapshot with agent**, review the confirmation and give the ten-minute token to the agent for airk_explorer_handoff_get. The full snapshot is capped at 24 KiB; edits, binding changes and observed evidence changes revoke the token. Read Share a reviewed snapshot for the full contract.

## Import and export

**Export…** saves the strategy as a .strategy.airk.json file through the save dialog. **Import…** validates a file exactly like a template before Studio shows it.

The import picker starts in .airk/strategies and can read another explicit file you select. Importing asks before replacing Studio work; edits or a project change while the picker/confirmation is open keep the current draft. The source file is never modified by import.

## Warnings

The token count is an estimate. Review the prompt before sharing it: repositories can contain confidential code or instructions planted in source comments.

Changing the strategy, objective, parameters or scope invalidates its run and generated prompt. A delayed result for an older draft or project is discarded. Edited prompts are retained with a stale warning and must be rebuilt before export. Evidence and schema budgets count serialized UTF-8 bytes, including metadata and escaping. Failed parallel steps settle their started reads before the run can finish.

## Saved-strategy library

Saved strategies lists .airk/strategies in the chosen workspace. Navigate folders and breadcrumbs, then Load or double-click a JSON file. Browse saved strategies remains available while editing. Replacement confirms and rechecks the current draft/project. The source file remains unchanged. Refresh and bounded paging are explicit; linked/redirected library paths are refused.
