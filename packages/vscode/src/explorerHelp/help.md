---
id: explorer.help
title: Using Explorer help
route: help
area: explorer
aliases: help, how do i, documentation
related_airk_help: overview, topics
---
# Using Explorer help

Help searches one offline catalog of Explorer topics and AIRK engine guidance. The same catalog is available to agents through airk_explorer_help_search/get and help resources. Engine guidance is generated from the native airk_help topic source at build time. It does not sample live state or require indexing.

## Steps

1. Search by purpose, role or source format, for example tester PDF, architect dependencies, writer spreadsheet or developer bug. Titles and aliases are ranked ahead of body matches; narrow the Area filter if needed.
2. Choose a role starting point, Choose indexing by purpose, Document types and extraction, Work with an AI agent, Share a reviewed snapshot or About AIRK.
3. Choose **Help** in the upper-right, then select current-screen guidance or Browse all help in the native VS Code quick pick. Overview's role workflow buttons and Management > Indexing's guidance buttons open the corresponding guides.
4. Use **Engine guide** to read the related engine topic offline. **Live reference** asks the connected engine for its actual version's help; it needs an authorized registered project connection.
5. Under Live reference type, choose MCP tool schema, CLI command help or Engine topic, enter its exact name and choose **Read live reference**. Examples are airk_search, collections extraction detail and indexing. This reads help only; it never executes that command or tool. Follow **Next page** for a long schema, or **First page** to restart it.
6. If live reference fails, choose **Back to** the bundled topic. A late reference reply cannot replace a different topic you selected.

## About and community

The header's **About** button opens Help > About AIRK: what AIRK is, Ioka credit, Munarium/VCP inspiration, the installed extension version, public source and website links, licensing, and the Ioka Discord community invitation. External links open only after a user click.

Topics for screens that are not built yet are labeled **planned** and describe the design, not current behavior.
