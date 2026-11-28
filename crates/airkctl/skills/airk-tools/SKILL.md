---
name: airk-tools
description: Use when AIRK retrieval tools such as `gather_context` are connected and code or document work needs cited evidence from the index, from a single declaration to a change spanning several files.
---

# AIRK tools

If the task gives you the exact file and lines you need, read them. Otherwise pick the tool that answers the open question. A task that spans several places starts with `gather_context`, even when it names a symbol.

## Choose by open question

- **Task across several places:** use `gather_context`.
  - Pass the whole task as `task`.
  - Pass `needs` as a list, one statement per thing you must learn (up to 8). For a code change these are often the rule that controls the behavior, where it is applied, and an existing example or test that exercises it.
  - Pass exact strings from the task (identifiers, routes, quoted text) as `anchors`.
  - Set `deliverable` to `code_change`, `test`, `document`, or `analysis`.
  - The reply maps each need to passages and lists what was not found. For a need that was not found, call again with narrower needs, follow a citation, or make a targeted search before editing.
- **Concept or behavior, no dependable name:** use `search_passages`. Add `path_prefix` when you know the area. Read a cited range if the excerpt leaves a question open.
  - An empty result reports the mode it served. If that mode is lexical, every query word had to match and quotes did not make a phrase. Retry with two or three distinctive terms before concluding the thing is absent.
- **Declaration:** use `find_definition` with the exact written name.
  - Narrow ambiguous matches with `path`, `path_prefix`, or `language`.
  - If the exact name misses, retry with `name_match` set to `suffix` or `contains`.
  - It matches written names. It does not resolve imports or aliases.
  - Read the declaration's range when you need its implementation.
- **Where an identifier is written:** use `find_identifier_occurrences` with the exact-case identifier.
  - Hits are matching lines. They include declarations, comments, strings, and unrelated namesakes, so inspect a line before treating it as a caller.
  - A use written under another name is not matched.
  - For an exhaustive scan of that spelling, follow `cursor` until the reply reports the scan complete.
- **Directory known, file unknown:** use `list_indexed_files` with `prefix`, a literal path prefix and not a glob. This tool's parameter is `prefix`, not `path_prefix`. For a known file, skip the listing.
- **File known, lines unknown:** use `outline_file` with the exact path to locate imports, definitions, or document headings by line. If you already have the lines, skip the outline.
- **Lines known:** use `read_indexed_range` with the exact `path` and an inclusive, one-based `start_line` and `end_line`.
  - When following a citation, pass its publication token as `revision`.
  - A superseded publication is refused. Obtain a fresh citation; do not retry the old token.
  - In an extracted document (PDF, DOCX, PPTX), keep the range within one page, block, or slide.

## Handling replies

- **Scope:** the connection supplies the search scope. On `scope_required`, relay the message's remedy to the user and stop. Do not retry or work around it.
- **Citations:** keep the originating path, location, and publication with every piece of evidence you cite. A `{ref:publication}` reference is valid only within the reply that carries it.
- **Truncation and omissions:** replies are bounded by `max_bytes` (default 8192, maximum 65536). When a reply reports truncation, omissions, or unavailable sources, narrow the request or raise `max_bytes`, and follow a continuation when the missing part matters to the answer.
- **Retries:** retry only when the reply gives a typed retry. Never replay a call whose outcome is uncertain.
