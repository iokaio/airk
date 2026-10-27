---
id: explorer.review.tests
title: Declared test candidates
route: review/tests
area: explorer
aliases: tests, test candidates, affected tests
related_airk_help: code
---
# Declared test candidates

Review › Tests helps choose tests to inspect or run for uncommitted Git changes. **Changed test files** come directly from Git, including unindexed tests. **Indexed test references** show test passages that mention sampled declarations from changed files, with the declaration name, path, indexed line and excerpt as the reason to investigate. The Explorer never runs these tests or measures coverage; a textual mention is not a resolved dependency.

Refresh rechecks the current change set and index. Unchanged inputs should yield the same candidates. This is a change-focused checklist, not a test-run history or a complete project test inventory. Use your project's test runner to learn execution outcomes.

Test paths are chosen by the airk.explorer.review.testPathPattern setting, a regular expression over collection-relative paths. The default matches test, tests, __tests__ and spec folders and files named like name.test.ts, name.spec.js or name_test.go.

## Steps

1. Open Review › Tests after reviewing the changes.
2. Run the listed tests with your own tools if they look relevant.
3. Adjust airk.explorer.review.testPathPattern if your tests live elsewhere, then choose **Refresh**.


## Bounded review and test discovery

Review prioritizes source changes ahead of documentation and samples declarations across up to twenty changed/current-or-old paths. Tests includes directly changed test files as Git observations even when no indexed declaration is available. Textual-reference candidates retain indexed source locations; changed-file rows do not invent a line or source excerpt. Relevant test directories are discovered from changed test paths or bounded indexed file inventory, and test-scoped queries run before hit limits are applied.

The report identifies inspected paths, declaration samples, omitted pages and clipped test-directory discovery. An empty list is not proof of no impact or no tests. Earlier committed changes are outside this uncommitted-Git review. Deleted declarations already absent from the index cannot be recovered; no historical index is retained.

The candidate report shows at most 100 entries, prioritizing changed test files and disclosing any omitted entries. Review Changes can page through the full changed-file list. The candidate report samples evidence; it is not a complete inventory of tests.

Reports use one publication. If that identity changes, the complete report restarts once within a shared 90-second budget, discarding the first attempt. Repeated publication changes remain an explicit unavailable result; wait for indexing to settle and refresh. Missing identities, integrity errors and other tool failures are not retried as publication changes. Changed test files remain visible when indexed analysis fails, with the failure and unknown declaration count shown explicitly. The report never presents mixed-publication references or labels unavailable analysis as passing. Discovery uses native pages of at most 100 files and retains its documented sampling limits.
