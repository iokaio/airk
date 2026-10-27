---
id: strategies.library
title: Strategy Library
route: strategies/library
area: strategies
aliases: templates, blueprints, core pack, runnable, preview
related_airk_help: workflows
---
# Strategy Library

Strategies › Library lists the 25 templates of the AIRK core pack in six categories, each versioned as airk.core/NAME@1.0.0 (for example airk.core/bug@1.0.0). A strategy gathers bounded, cited evidence from AIRK and compiles a prompt; the Explorer never runs an agent.

Three templates are Runnable: Investigate & fix a bug, Change an API contract and Explain a component. The others are labeled Preview with their actual operator/prerequisite reason. The presence of Review does not automatically make every Git-related template runnable. A Preview template can still be opened in Studio to edit its steps or compile a prompt without evidence.

Agents use airk_explorer_templates_list (following next_offset) and airk_explorer_template_get for this installed pack's exact parameters, defaults, steps, budgets and operator contracts. Availability is reported per template; agents must not invent template IDs or assume Preview means executable.

## Steps

1. Filter by category or text.
2. Choose **Open in Studio** (or **Preview in Studio**).
3. If Studio already has work, confirm **Replace Studio work**; otherwise nothing changes.

Browsing the Library never replaces Studio work silently.
