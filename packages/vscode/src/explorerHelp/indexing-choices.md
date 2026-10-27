---
id: indexing-choices
title: Choose indexing by purpose
route:
area: guidance
aliases: purpose, detail, agent lean, rich structure, search only, essential, balanced, detailed, vectors, indexing choices, developer tester architect writer
related_airk_help: indexing-choices, indexing, configuration, models, formats
---
# Choose indexing by purpose

Choose for the evidence you need, the source formats you actually have and the machine budget. Your Overview lens only reorders journeys; it never changes indexing settings. Existing project settings are preserved when opening or upgrading AIRK.

## Steps

1. Identify the purpose: exact text lookup, code investigation, requirements/testing, architecture analysis or writing. Identify source languages and Markdown, PDF, DOCX, XLSX, PPTX or scanned/image material. Ask the user when these are unknown; do not choose a richer index merely because a role is selected.
2. Check Overview and Management > Indexing. Inspect the project before changing it. An agent can use airk_status and airk_capabilities to check published detail, adapters, lexical readiness and independent semantic readiness.
3. In Management > Indexing, choose **Project settings… > Index detail**. The native picker uses essential, agent_lean, balanced and detailed. Review and approve a change; observe the resulting native job and new publication before assuming it took effect.
4. Choose vectors separately under **Vector search**. Review **Vector eligibility** when selected files should stay lexical-only. Adapter overrides, source scan limits, watching and storage quotas are separate settings.
5. Check representative files with Evidence > Inspector and Documents > Facts or Code > Symbols. Compare useful answers, retained coverage and costs before raising detail or budgets further.

## Evidence detail

- **Search Only (essential)**: text search and citations without rich code/document facts. Useful for known phrases, identifiers or text-only document lookup. It is a poor choice when the task requires requirement, acceptance, table or relationship inventories. Parsers and document conversion may still run.
- **Agent Lean (agent_lean)**: the new-collection default. A starting point for Developer, Tester and mixed code/document work: selected declarations, imports, local structure, unresolved call candidates and useful prose facts. Broad inventories and continuity fingerprints are omitted; absence of a fact is not proof that its source is absent.
- **Rich Structure (balanced)**: broader supported structure/framework/document inventories and declaration continuity, useful when lean evidence is insufficient for impact, architecture or document/table exploration. It costs additional extraction and retained space.
- **Detailed (detailed)**: advanced supported fact and continuity output, including document-unit tracking where implemented. It does not add complete language understanding, execute a compiler or guarantee resolved references. Use it for a demonstrated evidence need.

## Vectors and source selection

Lexical mode suits exact names, error messages, requirement IDs and quotations. Vector/hybrid modes can help conceptual questions across prose and code when an offline model and selected vectors are ready. More syntax detail does not guarantee faster or better vector inference; these are independent controls. Explicit vector failures must remain visible.

Agent Lean's default vector policy skips reviewed lockfile basenames and minified JS/CSS, retaining their lexical evidence. It does not automatically remove all generated, vendor or test directories. Explicit saved policies retain their semantics. Keep tests and specifications when they are evidence for your purpose; exclude irrelevant semantic work deliberately, rather than removing useful source evidence.

Native watching maintains observed source intent. Manual watching suits a deliberate snapshot or expensive source tree; the user then requests Index now. Pause, watching, service Stop and cancellation are different operations. Do not rebuild a healthy collection just to refresh ordinary source edits.

## Limits

Configured settings can differ from the published index until work succeeds. Job acceptance is not completion. Source freshness, lexical readiness and semantic coverage are independent. Never present unchanged source bytes, complete format coverage or successful semantics without observations. Use the Document types guide for extraction and OCR choices; exact native flags come from airk_help command selectors.
