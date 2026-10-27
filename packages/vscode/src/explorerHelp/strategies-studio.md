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

## Import and export

**Export…** saves the strategy as a .strategy.airk.json file through the save dialog. **Import…** validates a file exactly like a template before Studio shows it.

## Warnings

The token count is an estimate. Review the prompt before sharing it: repositories can contain confidential code or instructions planted in source comments.
