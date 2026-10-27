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
