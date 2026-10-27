---
id: explorer.review.impact
title: Change impact checklist
route: review/impact
area: explorer
aliases: impact, affected code, references, blast radius
related_airk_help: code
---
# Change impact checklist

Review › Impact builds a checklist from the declarations AIRK indexed in the changed files. For each declaration it counts exact-case textual references across the index, lists up to five referencing files and counts references in test paths.

This is a checklist, not a verdict: references are textual occurrences, not resolved callers, and declarations come from the indexed version of each file.

## Steps

1. Open Review › Impact after Review › Changes shows the files you expect.
2. Work through the list and tick each declaration once you have checked its references; ticks are not saved.
3. Use Code › References for the full reference list of one name.

At most 10 changed files and 20 declarations are examined, with up to 20 references each.


## Bounded review and test discovery

Review prioritizes source changes ahead of documentation and samples declarations across up to twenty changed/current-or-old paths. Tests includes directly changed test files as Git observations even when no indexed declaration is available. Textual-reference candidates retain indexed source locations; changed-file rows do not invent a line or source excerpt. Relevant test directories are discovered from changed test paths or bounded indexed file inventory, and test-scoped queries run before hit limits are applied.

The report identifies inspected paths, declaration samples, omitted pages and clipped test-directory discovery. An empty list is not proof of no impact or no tests. Earlier committed changes are outside this uncommitted-Git review. Deleted declarations already absent from the index cannot be recovered; no historical index is retained.
