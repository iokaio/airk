---
id: role-developer
title: Developer workflow
route:
area: roles
aliases: developer, programmer, bug, API, implementation, refactor, source code, indexing purpose
related_airk_help: roles, indexing-choices, code, workflows, freshness
---
# Developer workflow

Use AIRK to investigate bugs, explain a component or prepare an API change with bounded evidence. Start with Agent Lean for supported code syntax; move to Rich Structure only when broader structure or declaration continuity is needed. Choose vectors separately for conceptual discovery; exact identifiers and errors usually start with lexical search.

## Steps

1. Select the registered project and check Overview. An agent checks status and capabilities before assuming the publication, language support or semantics are ready.
2. Search the error message or symbol with a literal path scope. Use Code > Symbols, References and Outline to inspect declarations, written imports and candidate callers; References includes comments and strings.
3. Inspect the strongest hits and expand revision/hash-bound context. Read tests and specifications alongside implementation. A candidate edge is a lead, not a resolved call graph.
4. In Strategies > Library, choose a currently Runnable bug/API/component template. Agents discover the actual installed templates and contracts with list_templates and get_template; do not assume every template is runnable.
5. Run the evidence strategy, inspect partial/truncated outcomes and rebuild stale prompts. Share a complete Studio snapshot explicitly, or add tasks to Planner, save and hand off one task.
6. Before editing, the agent verifies live files against cited hashes/revisions. After changes, run the project's own checks outside AIRK, refresh indexing as appropriate and review changed evidence. AIRK never runs the coding agent or completes tasks automatically.

## Useful follow-up

Use Review > Changes, Impact and Tests for Git/index comparisons and test candidates. Follow Changes pagination before advancing a remembered baseline. Deleted or renamed evidence may no longer be indexed; unknown coverage remains unknown. Read Document types and extraction when the bug is governed by PDF/Office requirements or configuration files.
