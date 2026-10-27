---
id: explorer.search.query
title: Search indexed evidence
route: search/query
area: explorer
aliases: find text, lexical search, semantic search, hybrid search, vector search, grep the index
related_airk_help: search, tool-selection
---
# Search indexed evidence

Search runs `airk_search` against the selected project's published index. Every result is a citation with a path, line range and content hash.

## Steps

1. Enter a query. For **Semantic (by meaning)**, describe the concept or behavior you want to find; exact words need not appear. **Lexical (all words)** requires all query words to appear.
2. Choose a mode. **Hybrid (lexical + semantic)** combines both approaches. **Collection policy** follows the project's saved policy; the result says which mode AIRK applied. Semantic search uses the native vector mode, including in saved recipes.
3. Optionally set a path scope relative to the project, such as `src/auth/`.
4. Choose **Search**. Use **Expand** for more indexed lines, **Inspect** for the file record, or **Open source** to open the file.

Results display twenty at a time with **Previous** and **Next**. The limit controls how many results AIRK retrieves (up to 100); the byte budget can return fewer. Paging the retrieved results does not rerun the query. Your query/options, latest results and current page survive tab switches and closing/reopening Explorer in the same selected project and extension-host session. Changing the project or launcher binding, or restarting the extension host, clears them. These session results are not saved searches and are not written to workspace storage. A newer publication is labeled; run Search again when you want current results.

## Limits

- Lexical results are ordered by index identity, not relevance.
- Semantic and hybrid results are similarity candidates, not proof of meaning. Enable vectors in Management › Indexing to use these modes. Building, partial or failed semantic coverage can mean empty, unavailable or incomplete results; readiness appears alongside the results.
- "N returned · limit L · more may exist" means the limit cut the list; raise it or narrow the query.
- Open source compares the saved bytes with the indexed hash and warns when the file changed.

Page changes announce the retained result range. Previous/Next use the same controls as Review Changes.
