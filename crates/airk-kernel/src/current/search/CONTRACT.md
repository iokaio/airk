# Current search: roster and private evaluation switch

Owner: Kernel (`current/search.rs`, `search/v2.rs`). Store owns the lexical
readers it calls. No wire field, persisted format, chunking or stored vector
is changed by either entry below.

## Plain-store vector search (24 November 2026)

The vector branch ranks the selected pipeline's committed rows by exact
dot product. Path scope and prefix apply before ranking. Deleted and
unreferenced rows are excluded. Each accepted occurrence binds to its
retained artifact and extraction before materializing a citation.
The branch reports incomplete results when its candidate or evidence
budget truncates the selected scope.

## Permanent denies and the scoped path roster (22 November 2026)

Before this fix, `filtered_paths` built a bounded path roster whenever the
read restricted paths. That included every read under a permanent
source-policy deny. The roster holds at most `MAX_SCOPED_PATHS` (4,096)
paths in alphabetical order. Every search of an index with any such deny was
therefore limited to its first 4,096 paths, and this was reported only as
`truncated`. Now only a selected scope that omits sources of the publication
(`selected_scope_restricts_paths`) or an explicit filter builds a roster.

A deny is still enforced by its removal publication, because reads are
admitted only once the deny is applied, and by the per-pointer and
per-artifact path checks of every branch. The search is query-time only and
needs no reindex. The regression test
`permanent_deny_searches_the_whole_retained_corpus_not_a_first_path_page`
(`tests/source_policy.rs`) indexes more than 4,096 paths before the target.
It requires vector, hybrid and lexical search to find a retained late path
after a deny and never return the denied file.

## Ranked lexical candidates for prose

Product hybrid Search adds a ranked keyword stream for prose queries of
at least eight words. Named identifiers lead the ranked term groups.
The stream uses the existing 100-candidate bound and verifies path
scope before the cut. The same ranking pass combines the candidates with the
existing conjunctive lexical branch and exact vector branch. Vector
query text, branch depth, fusion weighting, output limits and snippet
selection keep their ordinary policy. Short, lexical-only and vector-only
requests keep their existing candidate selection.

Test-support builds can pin the original baseline (none or unset), the
keyword branch alone (ranked_keywords), the identifier branch alone
(ranked_identifiers), or both streams (ranked_terms). These profiles
use the same query and publication scope and require no index changes.

## Focused semantic queries

For hybrid Search, clear independent questions, enumerations and imperative
clauses can contribute up to three focused queries in addition to the original.
All use the saved index's embedding pipeline and share the same scoped vector
matrix. The original query keeps at least half of the existing ranked row and
passage budgets. Merge by interleaved rank, with the original first, and
deduplicate occurrences before submitting one semantic branch to fusion.
The original request still drives snippet selection and Focus. If the query
has no confident split, or an auxiliary embedding is unavailable, use the
original semantic ranking with its full budget. Scope and integrity failures
still propagate. The test-support profile is semantic_subqueries.

## Fusion comparison

Prose hybrid Search keeps the same candidate branches and passage
budgets while ranking. The comparison profiles can reduce the keyword
stream's reciprocal-rank weight to 0.9, defer a file's hits after its
first two, or apply both. File deferral runs after duplicate semantic
occurrences are removed and preserves order in each group. Deferred
hits remain available after other files' leading hits.
Product Search applies file deferral with equal branch votes. The baseline
profile keeps equal votes and its original file order. Short queries
and lexical fallback, lexical-only or vector-only searches keep their order.

## Exact structural candidates

The private structural profile can include up to two excerpts from complete, unambiguous
native definition lookups. The navigation lookup and excerpt read use
the same current reader, request path prefix and Search filters. Each
excerpt is at most seven original lines from its native definition and
must fit its snippet budget in full. It competes as a ranked branch in
the same hit and byte budgets and keeps its verified original citation.
Search drops identical citations within this result. Ambiguous and
incomplete definitions remain navigation hints. These excerpts use the
same final Focus path as lexical and semantic hits. The test-support
profile is structural_definitions. Textual and syntax-only references
do not supply resolved one-hop targets.

## Contextual passage selection

Product Search uses the contextual passage policy. The private keyword_passages profile tests the same policy with its ranked candidates and result limit.
In prose hybrid snippet requests it selects a window around a ranked keyword
anchor and keeps up to a third of the window for preceding context. This only
changes native line-based windows. Their citations use the original source
line coordinates for the delivered text. When the total byte budget is tight,
each result is bounded by its share of the remaining budget as well as the
per-snippet cap. Full-chunk requests keep their existing materialization.
Focus receives the original query and the chosen excerpts.

## Private evaluation switch `search_v2` (22 November 2026)

The historical Search v2 profile is selectable only in test-support builds. It is read once per process from
`AIRK_ARBITER_EVALUATION_SEARCH` (`search_v2`, or `none`/unset for off; any
other value fails every search) or bound by `bind_evaluation_search`, and a
later binding must agree. When the variable turns the switch on, the process
writes `AIRK_EVALUATION_SEARCH bound=search_v2` once to stderr.

When the historical profile is selected and the hybrid query is prose:

- the instruction is reduced to up to 20 key words, with named identifiers
  first;
- two ranked any-term lexical branches are added (key words, and named words
  when they differ). They use Store's `lexical_search_ranked_any`: summed IDF
  over at most 24 groups, a group matching more than 50,000 rows is dropped,
  and path prefix, selector and read scope apply before the limit;
- vector and ranked branches take 300 candidates; the conjunctive branch
  stays at 100;
- the fused score is weighted by branch, then by a file-kind prior;
- a file's hits after its first 3 are deferred behind other files' hits;
- snippets are focused on the key words.

Materialization re-checks that a ranked chunk's own terms satisfy one of its
groups. The cross-index rank fusion of combined reads is unchanged.

## Wider shortlist evaluation

The private `wide_shortlist` profile starts with the contextual passage
policy and the same scoped branches. For prose hybrid snippet requests
with a limit of ten it materializes up to 40 distinct passages from the
fused order. It drops identical citations and native line passages
sharing at least four fifths of the shorter range on the same source
revision. Mapped locations require exact citation equality.

Keep the first six passages by fused rank. Up to two deeper semantic
passages from distinct new files can enter the remaining four slots.
Fill open slots with new files first and retain fused order among
selected hits. Use the final ten for per-hit byte allocation. Stop
gathering extra passages when the evidence read budget is exhausted.
Scope and source-integrity failures propagate. Focus continues to
receive the selected response through its normal Search path. The
test-support trace records shortlist citations and selected indexes.

The private `wide_cutoff` profile evaluates a second selection from that
same shortlist. It holds the first eight hits and selects a band of
four from the rest using query terms underrepresented in the held
excerpts. The first two candidates in that band are the deterministic
choices. On the scoped Focus path, score the four candidates with the
ordinary Host grant and native Focus validator. Substitute the two
highest scores only when all four were successfully scored. Otherwise
keep the deterministic choices. The candidate score frame includes a
reservation for the eight held results and navigation metadata.

A complete Focus receipt describes all four candidates, including the
two omitted from the response. Keep that receipt in the private
`AIRK_SEARCH_CUTOFF` trace and verify its exact projection against the
returned response. The experiment does not change the public Focus
receipt contract.
