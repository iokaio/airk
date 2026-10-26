---
id: explorer.search.query
title: Search indexed evidence
route: search/query
area: explorer
aliases: find text, lexical search, hybrid search, vector search, grep the index
related_airk_help: search, tool-selection
---
# Search indexed evidence

Search runs `airk_search` against the selected project's published index. Every result is a citation with a path, line range and content hash.

## Steps

1. Enter the words to find. Lexical search requires all words to appear.
2. Choose a mode. **Collection policy** follows the project's saved policy; the result says which mode AIRK applied.
3. Optionally set a path scope relative to the project, such as `src/auth/`.
4. Choose **Search**. Use **Expand** for more indexed lines, **Inspect** for the file record, or **Open source** to open the file.

## Limits

- Lexical results are ordered by index identity, not relevance.
- Vector and hybrid results are similarity candidates, not proof of meaning.
- "N returned · limit L · more may exist" means the limit cut the list; raise it or narrow the query.
- Open source compares the saved bytes with the indexed hash and warns when the file changed.
