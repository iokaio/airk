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
