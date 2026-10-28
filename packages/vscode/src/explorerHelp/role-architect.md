---
id: role-architect
title: Architect workflow
route:
area: roles
aliases: architect, architecture, dependency, boundaries, ADR, decision, impact, indexing purpose
related_airk_help: roles, indexing-choices, code, documents, freshness
---
# Architect workflow

Compare documented intent with written code structure and bounded dependency candidates. Agent Lean is a starting point; Rich Structure is useful when broader structural facts, framework cues or declaration continuity are required. Detailed is an advanced choice for a specific supported evidence need, not a promise of a complete semantic graph.

## Steps

1. Select the project's boundaries and relevant reference documents with the user. Additional source roots require reviewed registration; do not silently traverse sibling projects or assume collections are interchangeable.
2. Check status and capabilities, then Search for component names and Documents > Facts for architecture/design/decision statements. Keep dated historical decisions distinct from current implementation.
3. Use Code > Symbols, Outline, written imports and candidate Relationships to explore one boundary at a time. Follow bounded continuations and label unresolved calls or producer-dependent imported assertions.
4. Cross-check the strongest relationships with source excerpts and live files. Treat absence of an edge as incomplete evidence. For Markdown/Office diagrams or PDF figures, inspect the original visual material rather than assuming text extraction captured the design.
5. Use Review > Impact for changed declarations and candidate consumers, including old rename/deletion paths when still indexed. Require one consistent publication across evidence calls.
6. Use a Runnable strategy or prepare an explicit Planner task for the architecture question. Include boundary assumptions, alternatives, evidence gaps and validation steps in the handoff; the user approves changes separately.

## Indexing choices

Lexical search finds exact interfaces and decision IDs. Vector/hybrid can discover related prose when semantic coverage is available. More syntax detail and vectors are independent choices. Reassess cost and useful evidence before increasing detail, scan limits or vector eligibility across a large monorepo.
