# Current source admission and public catalog projection

Owner: Store. Live source admission excludes reserved `.airk` roots independently
of whether a permanent source policy is present. Persisted source definitions,
schema identities and digest inputs are unchanged.

Before exposing a current reader, authenticate and freeze its published primary
root and alias map. Reserved primary or alias roots refuse the whole reader,
including status/count metadata. Candidate checks use that immutable map without
SQL reentry. Ordinary published aliases retain their previous behavior.

Public typed catalog pages/details refuse historical reserved source/backing
roots and retained agent-edit backing data. Internal `item`/`optional_item`
readers remain available to native mutation and recovery. Refusal preserves the
stored rows; it does not sanitize, rewrite or fabricate an empty public result.

Regressions cover new admission, historical primary/alias publication, direct
candidates, public page/detail and the service's actual catalog listing caller.
Ordinary `.airk/data` backing assets remain supported. Windows passes do not
qualify Unix native aliases or other platform transports.

## Graph update proof-cache bound (22 November 2026)

The update-local base-proof cache in `graph.rs` is bounded only by its
existing 8 MiB of authenticated record and proof payload, about 2,900 proofs
of 384-dimension records. The former 256-entry cap is removed. Dirty
afterimages still take precedence, recovery still opens a fresh cache, and
no persisted byte changes. The test in `graph/tests.rs` fails with the
256-entry cap restored. This is an evaluation indexer fix, not yet adopted
for the product.

The private evaluation search switch also reads through
`lexical::ranked::lexical_search_ranked_any`. It reads FTS5 doclists and
authenticated document rows only, writes nothing and adds no index state.
Product search never calls it (see the Kernel `current/search/CONTRACT.md`).

## Reader open and WAL-index recovery (23 November 2026)

A read-only current-database open (`database::open_reader_budget`, used by
every current reader) still rejects contention at once: its connection has
`busy_timeout(0)`, and a BUSY or LOCKED result becomes the typed, retryable
`CurrentReadContention` (`publication_busy`, `repeat_same_request`). The one
exception is `SQLITE_BUSY_RECOVERY` (extended code 261): another connection
is rebuilding the WAL index from the bounded WAL, which holds no
publication. The open is then repeated every 5 ms for at most 500 ms, and
after that the last error is returned as before. No snapshot exists before
the open returns, so no caller work is repeated. Ordinary BUSY, LOCKED,
`SQLITE_BUSY_SNAPSHOT`, corruption and I/O errors never wait. The wait
ignores the caller's deadline; it is bounded by its own 500 ms.
Service work under `admission::with_work_admission` keeps its own repeat
around this open. Decision tests with an injected clock:
`database/recovery_tests.rs`.

## Source policy fence: not frozen versus reserved (23 November 2026)

`CurrentRead::require_source_policy_live` (change-log entry 67) keeps two
cases apart. A reader whose source roots are not frozen yet is the existing
typed `SourcePolicyReadUnavailable` (`source_policy_unavailable`), as
`attach_source_policy` already reported it. A frozen projection with a
reserved primary or alias root is still `ScopeReadRefused`
(`scoped_read_unavailable`). Before, both were the refusal. Test:
`read::scope_tests::unfrozen_sources_are_policy_unavailable_and_reserved_roots_refuse`.

### Caller-bounded retained original bytes

An admitted current reader can cap the original UTF-8 source load before
opening its retained object. The bounded path uses the same authenticated
object decoder, source hash, length checks, and scope admission as the
existing original-source read. The default exposes the same maximum as
before. The bound does not grant live source access or change on-disk
representations.
