---
name: airk-tools
description: Choose AIRK retrieval tools for code or document work that needs evidence from a configured index, from a single declaration to a change spanning several files.
---

When the task already names a sufficient local example, go directly there. Otherwise choose the tool that answers the open question.

- **Behavior across files** — Use `gather_context` with the task and the evidence you need: the rule that controls the behavior, where it is applied, and an existing example that exercises it. Compare the returned passages with those needs. If one is missing, follow the relevant citation or make a targeted search before editing.
- **Concept or phrase** — Use `search_passages` when you know the behavior or wording but lack a dependable identifier. Use `path_prefix` when you know the area. Read a cited range if the excerpt leaves a question open.
- **Declaration** — Use `find_definition` with the exact written name. Narrow ambiguous matches by path or language. Read the declaration’s source range when you need its implementation.
- **Occurrences** — Use `find_identifier_occurrences` for an exact-case identifier. Hits are matching lines and may include declarations, comments, strings, or unrelated namesakes. Inspect candidate lines before treating them as callers. For an exhaustive search, follow the cursor until the scan reports completion.
- **Directory known** — Use `list_indexed_files` with a literal path prefix to find candidate paths. For a known file, skip the listing.
- **File known** — Use `outline_file` to locate imports, definitions, or document headings by line. If you already have the lines, skip the outline.
- **Range known** — Use `read_indexed_range` with the exact path and inclusive, one-based line range. For a citation, pass its publication token as `revision`. If the publication was superseded, obtain a fresh citation. In an extracted document, keep a range within one page, block, or slide.

The connection supplies the search scope. If AIRK returns `scope_required`, tell the user which connection needs a scope. Keep the originating path, location, and publication with cited evidence. Respect the response budget and any reported omissions or unavailable sources. Follow a continuation when the missing portion matters to the answer.
