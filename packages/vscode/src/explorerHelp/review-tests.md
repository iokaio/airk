---
id: explorer.review.tests
title: Declared test candidates
route: review/tests
area: explorer
aliases: tests, test candidates, affected tests
related_airk_help: code
---
# Declared test candidates

Review › Tests lists lines in test paths that mention a declaration from the changed files. They are declared test candidates: the Explorer never runs tests, and this is not coverage.

Test paths are chosen by the airk.explorer.review.testPathPattern setting, a regular expression over collection-relative paths. The default matches test, tests, __tests__ and spec folders and files named like name.test.ts, name.spec.js or name_test.go.

## Steps

1. Open Review › Tests after reviewing the changes.
2. Run the listed tests with your own tools if they look relevant.
3. Adjust airk.explorer.review.testPathPattern if your tests live elsewhere, then choose **Refresh**.


## Bounded review and test discovery

Review prioritizes source changes ahead of documentation and samples declarations across up to twenty changed/current-or-old paths. Tests includes directly changed test files as Git observations even when no indexed declaration is available. Textual-reference candidates retain indexed source locations; changed-file rows do not invent a line or source excerpt. Relevant test directories are discovered from changed test paths or bounded indexed file inventory, and test-scoped queries run before hit limits are applied.

The report identifies inspected paths, declaration samples, omitted pages and clipped test-directory discovery. An empty list is not proof of no impact or no tests. Earlier committed changes are outside this uncommitted-Git review. Deleted declarations already absent from the index cannot be recovered; no historical index is retained.
