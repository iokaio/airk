# Current search: roster and private evaluation switch

Owner: Kernel (`current/search.rs`, `search/v2.rs`). Store owns the lexical
readers it calls. No wire field, persisted format, chunking or stored vector
is changed by either entry below.

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

## Private evaluation switch `search_v2` (22 November 2026)

The switch exists only in test-support builds (`v2::active()` is `false` in
a product build). It is read once per process from
`AIRK_ARBITER_EVALUATION_SEARCH` (`search_v2`, or `none`/unset for off; any
other value fails every search) or bound by `bind_evaluation_search`, and a
later binding must agree. When the variable turns the switch on, the process
writes `AIRK_EVALUATION_SEARCH bound=search_v2` once to stderr.

When the switch is off, or the mode is not Hybrid, or the query has fewer
than `PROSE_WORDS` (8) words, search takes the product path unchanged. The
test `switch_off_hybrid_search_is_the_product_search` holds this byte for
byte. When the switch is on and the hybrid query is prose:

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
