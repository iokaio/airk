---
id: role-tester
title: Tester workflow
route:
area: roles
aliases: tester, QA, quality, acceptance, requirements, regression, tests, test plan, indexing purpose
related_airk_help: roles, indexing-choices, documents, code, freshness
---
# Tester workflow

Combine implementation, tests and requirements evidence to prepare checks and identify gaps. Start with Agent Lean for code and useful prose facts. Consider Rich Structure for broader document/table inventories; Search Only is insufficient when you need structured requirement or acceptance candidates. Preserve test sources and specifications in the source scope even if some files are excluded from vectors.

## Steps

1. Check the selected project's publication and actual adapter coverage. Ask whether requirements are Markdown, DOCX, XLSX, text-layer PDF or scans; scanned requirements need an explicitly provisioned OCR workflow.
2. In Documents > Facts, inspect requirement, acceptance and table candidates. Keep exact source statements, IDs and original page/worksheet/row locations. In spreadsheets, formula caches are not recalculated results.
3. Use lexical Search for requirement IDs and test names, then Code > Symbols and References for implementation leads. Neither an identifier match nor an absent edge proves requirement coverage.
4. Use Review > Changes, Impact and Tests to collect changed evidence and candidate tests. Check staged/working-tree states, old rename/deletion paths, publication consistency and clipping. AIRK does not execute tests or certify coverage.
5. Build a Planner task containing the expected behavior, boundary/negative cases, data prerequisites and acceptance checks. Review and explicitly share a complete task snapshot or save the plan for handoff.
6. Run tests in the project's actual harness outside AIRK. Bring the report back to Planner's citation checker, inspect changed/missing/unchecked citations and manually mark Review, Needs work or Completed.

## Limits

A verified snapshot proves a saved-byte comparison, not correctness or test success. Citation validation is not behavioral verification. Missing evidence may reflect index detail, adapter limitations, ignored files or budgets; inspect those before declaring a test gap.
