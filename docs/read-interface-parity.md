# CLI, MCP and Workbench read parity

Audit dated November 1, 2026, for the 1.8.7 candidate. Equivalent read requests must retain complete native data through each interface. Workbench wraps reads in bounded plans; `airk tools call airk_<operation>` returns the native MCP result envelope. The tools shell uses that same call path and pretty-prints its text content. Dedicated CLI aliases below are convenience argument builders; their documented defaults can differ. Use `tools help` and `tools call --args-file` for the complete native input schema.

| Native operation | Workbench command | Dedicated CLI alias, where present |
|---|---|---|
| search | `/search` | `airk search` |
| symbols | `/symbols` | `airk symbol` |
| references | `/refs` | `airk refs` |
| read_file | `/read` | `airk read` |
| outline | `/outline` | `airk outline` |
| list_files | `/files` | `airk files` |
| file_info | `/info` | `airk info` |
| imports | `/imports` | `airk imports` |
| code_facts | `/code-facts` | `airk code-facts` |
| document_facts | `/facts` | `airk facts` |
| changes | `/changes` | `airk changes` |
| context | `/context` | `airk context` |
| relationships | `/relationships` | `airk relationships` |
| status | `/status` | `airk status` |
| diagnostics | `/diagnostics` | `airk tools call airk_diagnostics` |
| capabilities | `/capabilities` | `airk tools call airk_capabilities` |
| help | `/help` | `airk tools call airk_help` |

`airk parse --execute '/status'` and Workbench execute the same native `airk_parse` handler. `airk tools call airk_parse --args-file request.json` accepts an explicit bounded plan. This is a read-only composition layer, not an agent or management runner.

## Shared implementation and data

The kernel validates plan scope, revision, bindings, fan-out and budgets. The MCP adapter supplies a callback that invokes its canonical handlers inside the already admitted blocking lifetime. Inner reads use the refreshed outer kernel, one outer usage record and its execution permit. They cannot recursively invoke parse, management or transport operations. All 17 operations retain the MCP presentation, including `items` fact records, provenance/readiness and complete capabilities/help. Failed reads retain their complete native receipt under the same byte budgets.

The frontend and one-shot CLI share response observation code. Direct status/diagnostics and completed plan steps report configured `query_concurrency`, reusing the policy observation made for admission. Unified project selection and diagnostics editor observations are also retained inside plan steps. Fresh inner status reads identify their own observation time, zero cache age and `cached: false`; separately cached direct status calls report their actual captured time and age. These values need not match between separate requests.

Scope selection remains interface-specific: unified MCP's `project` is a connection-configured selector, while CLI root/data/collection options select a native registration. The unified endpoint adds its configured project identity and optional Explorer diagnostic observation to both direct and nested results. Headless native CLI calls do not fabricate an attached editor or a connection allowlist. Compare the same underlying collection and native arguments, retain those additional observations when present, and distinguish them from indexed evidence. Native usage accounting measures the worker response before frontend-only metadata is added.

Workbench requests full responses with explicit rich facts unless compact is chosen. Its forms and slash flags include every native read input apart from transport display controls. Conditional validators, artifact/continuity selectors, change selector arrays, extraction filters, paging/count options and detailed capabilities/help options remain available. Exact fact paths use the same native handler as prefix reads and remain scoped by the plan.

Structured views offer **Complete response** even when cited cards are present. Full JSON and Markdown copies/exports preserve the complete response. Selected-evidence exports deliberately remain selected subsets and identify source uncertainty. Output/payload limits still apply; truncation, omitted records and failures are explicit. Previously saved turns keep their original response; rerun a command to collect new fields.

## Regression evidence

- The native 17-operation table compares full MCP and executed Workbench results, checks the complete native input inventory, and verifies failed-read receipts. It excludes per-request IDs and cache observation timing from value equality.
- Bridge tests cover aliases, modes, all five capacity choices, live policy updates, identical JSON text/structured content, failed/unrelated results and payload/output bounds.
- The CLI session regression checks all 18 tool names through both response modes and all capacity choices, retaining arbitrary native fields, complete arguments and unchanged error receipts. Existing dedicated-alias tests deserialize their arguments against actual MCP schemas and check filter/matching/paging flags. The actual-process CLI/stdio regression also compares direct MCP status field presence with the nested Workbench result and requires capacity in dedicated CLI status and CLI-executed status plans.
- Extension regressions cover typed selector arrays and complete Markdown exports with evidence cards, alongside existing state, continuation, selection and host-boundary tests.

These local regressions do not establish installed-editor acceptance or cross-platform qualification. Current run outcomes and installer identity are in the [consolidated release record](releases/release-1.8.7.md).
