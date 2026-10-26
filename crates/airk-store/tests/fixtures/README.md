# Storage engine comparison fixtures

`lexical-fts5-3.53.3.json` contains results captured on October 26, 2026
from the pinned SQLite 3.53.3 FTS5 adapter. The corpus is fictional and generated
by `current::kv::lexical::tests::replacements_deletions_and_retained_queries_match_fts5_candidates_and_citations`.
The fixture records each query, literal prefix, limit, stable IDs, original
citation JSON and the `limit + 1` completeness flag for five mutation batches.

Before freezing these results, the test ran both adapters and asserted exact
agreement for every case. It now tests redb against the captured results without
linking or executing a SQL oracle. The historical source adapter is preserved
in Git at checkpoint `25dfeb5` in `src/current/lexical.rs`; it is not a runtime
fallback. The source tree still contains other SQL users pending conversion.

These fixtures establish logical compatibility for the generated cases. They
do not establish performance, full capacity recovery or platform qualification.

`catalog-map-roots.json` freezes the empty root and all 192 roots in the catalog
test's eight generated mutation batches. The SQL and redb adapters agreed on
every root before capture; the final contents are also checked against an
independent ordered in-memory map. This fixture preserves catalog digest bytes
without requiring a compiled SQL oracle in the new adapter test.
