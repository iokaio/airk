# MCP QA follow-up for the 1.8.9 working build

6 November 2026. Evaluation of the supplied 1.8.8 report (29 tools, approximately
110 calls, original publication generation 9430). Findings below distinguish
confirmed defects from intentional contracts. The corrections below are applied
to the unpublished 1.8.9 source. Windows completed its full native campaign,
passing assertion-only corrective checks, full extension suite and doctests;
the original full-run failures remain recorded. Mac Neo completed its full
campaign, including all four long native tests, and all 21 native corrective
checks passed. The Windows VSIX and all five selected installed-editor scenarios
passed. The final Mac extension suite passed 849 tests with 14 explicit skips;
Python and the editor-helper regressions passed, and cleanup was confirmed. The
[working-build record](../releases/release-1.8.9.md) owns the final
artifact identities and validation results.

Eligible installed runtimes upgrade automatically. Explorer then reads the
authenticated native capabilities without requiring a reconnect or library
refresh. Known read-only or pre-query contention gets bounded retries while the
panel is visible; a replacement runtime or window generation cannot reuse an old
observation. Upgrade stages, elapsed time and failures appear across Overview,
Indexing and Management, and the split panel explains its current availability.
These metadata reads grant no access and start no source indexing or split.

The full Mac run additionally reproduced a safe-retry mismatch: native
preparation reported catalog revision 12 for the original revision-7 selector,
with the same grant, index and Project selection. The client now accepts that
forward metadata clock while retaining all authority fields, original arguments
and the original deadline. Backward clocks, changed grants, submitted work and
unknown outcomes remain ineligible. The two-index diagnostics fixture now
declares its existing two-resident-project prerequisite. This concrete finding
does not prove the original reported outage had the same cause.

Installed-editor qualification also exposed an initial-window-owner race:
setup could be reviewed before automatic metadata established its owner, then
the later owner observation invalidated the review before Apply. Initial catalog
pages now await authenticated window ownership, and setup admission rejects an
unknown owner. Later owner changes still invalidate reviews; Apply is never
polled or replayed. The final Windows suite includes delayed-owner and stale-owner
regressions (859 passed, 4 skipped); final Mac and editor evidence is recorded in
the working-build record.

A second deterministic interleaving showed that an automatic retry could remain
pending after an explicit successful context read. Its later metadata clock then
cleared an active catalog view. A validated complete observation for the same
current owner now cancels that redundant retry. Invalid and retired-owner
observations remain rejected. The diagnostic editor harness records bounded
native and rendered observations before its driver deadline, without replaying
an uncertain mutation or changing valid results.

| Finding | Evaluation and applied implementation |
| --- | --- |
| 1. Generic direct-project outage | Reproduced a plain-text failure during investigation. Recognized native admission/retry receipts now survive preparation, shared status wrapping and transport boundaries. Unknown transport failures carry a structured code and explicit submission uncertainty. The original minute-long outage's underlying cause remains unestablished. |
| 2. Lexical punctuation/quotes | Token-AND behavior is intentional: punctuation and quotes do not select a phrase mode, and lexical candidates use stable-ID order. A larger bounded query returned both cited definitions. Snippets now prefer a contiguous query literal when present without changing candidates or ranking; help explains the query contract. |
| 3. Current changes verification | Exact current validation endpoints now retain current publication and source attribution. Selector-routing paths are not counted as extra source evidence. Historical endpoints remain historical. |
| 4. Scope/grant disagreement | Frozen single-index connections now disclose their installed scope/grant and frozen/imported qualification. They do not acquire current catalog discovery, invented names or wider authority. |
| 5. Invalid/open filters and empty cursors | Kind and language/profile labels remain open to retained producers. Empty or oversized labels get a typed field error. A complete empty selected query discloses its exact unobserved filters rather than declaring them globally unsupported. Language postings constrain discovery before unrelated definitions can produce an empty continuation. |
| 6. Unavailable strategy steps | Draft schemas and admission now exclude unavailable operators, including `host.gitDiff`. Installed preview templates and manual structural validation retain their separate purpose. No rejected draft reaches the Inbox. |
| 7. Error specificity | Unknown templates and proposals have distinct errors; unavailable handoffs are not mislabeled as expired proposals. Review and management errors identify the actual field, and nested parse errors preserve their path. Help rejects `detail` on native selectors with a `detail` field error; that option belongs to Explorer `query`/`topic_id` guidance. |
| 8. Missing schema bounds | Evidence schemas now expose the existing count, byte and hop bounds. Native context, CLI and Workbench share radii 0–100 and a five-line default; Workbench's former 40-line restriction and clamp are removed. Stored explicit choices remain valid. Help, whole-plan and verification budgets keep their separate limits. |
| 9. Error envelope shapes | Native and transport errors now preserve recognized receipts and scope observations in structured responses. Nested combined-member retry information remains explicit through its outer envelope. Lookalike error text does not create retry permission, and uncertain mutations are not replayed. |
| 10. Public tool names | Public descriptions and adapter-authored diagnostics use the advertised names and the actual `diagnostics.explorer.ui` location. User strategies, shared text and evidence remain unchanged; native help bodies retain executable CLI and compatibility names. |
| 11. Capability/verification cost | Terse capability replies summarize Workbench operation names and explicitly identify omitted schemas; `response_mode=full` retains the schemas. The pre-correction measurement was about 18.9 KB of schemas in a 29.6 KB reply. Whole saved-source hashing and complete receipts remain intentional; callers may explicitly select `verification=off`. |
| 12. Relationship truncation | Responses now disclose actual traversal, visited-symbol, count and byte limiting reasons with narrower follow-up guidance. There is no new graph cursor or completeness guarantee: a result count below the requested limit can still be truncated. |
| 13. Scoped diagnostics/provenance/status | Scoped runtime/work/freshness observations remain unavailable when their authority is absent. Independent retained producer/adapter/extractor identities and superseded scalar observation times/tokens are preserved. Documentation clarifies these intentional qualifications. |

The pre-QA 1.8.9 native source identity is
`8bcf41751e4cd4281fcc34e0ad649cae8b509ee4b4542488a19e83e2a0cfa462`
(973 inputs). Its Windows and Neo runs belong to that pre-QA source, not the
integrated implementation above. Initial Windows owner-process failures lacked the required
private fixture base. Several Neo protected-read failures used SSH's symlinked
`/var` temporary directory instead of its canonical `/private/var` spelling;
other Neo failures exposed nested transient read receipts under a terminal combined
envelope. The integrated correction retains those member retry receipts through
the outer response. Original failures remain identifiable separately from later
validation; protected-read assertions are preserved.

The supplied report's five Inbox drafts have not been applied or dismissed by
this work. Their presence does not authorize executing their contents.
