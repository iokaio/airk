---
id: explorer.code.symbols
title: Find symbols and candidate relationships
route: code/symbols
area: explorer
aliases: definitions, go to definition, callers, callees, call graph
related_airk_help: code, tool-selection
---
# Find symbols and candidate relationships

Code › Symbols finds exact native definitions. With **Code facts** checked it returns symbol ids, which let you expand relationships.

## Steps

1. Enter an exact definition name, or leave it empty to list definitions.
2. Choose **Find**.
3. Choose **Relationships** on a result, pick incoming or outgoing and 1–3 hops, then **Show candidates**.

## Limits

- Definitions are written syntax (E2). Imported producer facts are labeled E4.
- Edges are syntax candidates. A written call is not a resolved binding, and the absence of an edge proves nothing.
