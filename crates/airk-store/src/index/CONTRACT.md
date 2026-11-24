# Plain index store (`airk-index/1`)

Owner: Store, `crates/airk-store/src/index/`. Decided by the user on
24 November 2026: [indexing performance plan](../../../../docs/indexing/performance-plan.md),
decisions D1 to D8 and section 5. This module is the engine. Kernel does not
use it yet; plan steps P4 to P6 connect it.

## Guarantees

- **Crash-safe, not tamper-evident (D1).** A batch is one SQLite transaction
  in WAL mode with `synchronous=FULL`: after a crash a reader sees either the
  previous publication or the whole batch, never part of it (D7). Nothing
  proves stored bytes against deliberate change. Stored payloads carry their
  SHA-256 and are checked when read; damage is an error that asks for a
  rebuild, never an empty result.
- **Snapshots.** An `IndexReader` is one read transaction. It keeps the
  publication it opened on, unaffected by later commits, until dropped.
  Readers are not refused while a batch commits.
- **One writer.** The engine does not lock. The caller holds the existing
  collection writer lock around `IndexWriter` use.
- **Format check.** A database without the `airk-index/1` application id and
  user version is refused with `IncompatibleIndex`. `create` never overwrites.
  Old indexes are rebuilt, not migrated (D6, Q6).
- **Opaque records.** The file record, metadata, citations, member spans,
  resources and change records are the caller's bytes, returned unchanged.

## Files

| File | Holds |
|---|---|
| `index.sqlite3` | `meta`, `files`, `payloads`, `chunks`, `fts` (contentless FTS5), `selectors`, `handles`, `units`, `vector_rows`, `resources`, `changes` |
| `vectors-<generation>.f32` | 64-byte header (magic, dimensions, pipeline digest) and little-endian `f32` rows |

`meta` holds `format`, `sequence` and, with vectors on, `vector_dimensions`,
`vector_pipeline`, `vector_generation` and `vector_rows` (committed rows).

## Publication rules

- `apply(Batch)`: paths strictly increasing and valid. Every changed path's
  rows are removed first, then upserts inserted, so a handle or payload can
  move to an earlier path in one batch. A payload no file references is
  deleted. The sequence rises by one when files, resources or a change
  record change; stat refreshes alone do not publish.
- Lexical: chunk terms are the caller's normalized, prefixed terms. A query
  matches chunks containing every term; `ranked` orders by BM25 (lower is
  better) then stable id, unranked by stable id.
- Vectors: `configure_vectors` with a new pipeline starts an empty
  generation, deletes units and rows, and returns files with vectors to
  pending; `None` turns vectors off. `commit_vectors` writes new rows past the
  committed count, syncs the file, then commits units, row keys, file states
  and the new row count in one transaction. Rows past the committed count are
  ignored and overwritten. A file whose source hash changed or that is no
  longer pending is skipped as stale. Identical inputs share one row.
- Search is exact: one dot product per row (vectors are unit length),
  best first, ties to the lower row; an optional row subset filters it.
- Retired generation files are removed when a writer opens or reconfigures;
  a reader of an old snapshot may briefly fail to load them.

## Limits

Payload 64 MiB raw; file record 256 KiB; metadata and change record 16 MiB;
resource 64 MiB; path 8 KiB; 8,192 chunks, 65,536 selectors and the
extraction chunk bound of units per file; 4,096 dimensions; 4,096 retained
change sequences; pages of at most 4,096 paths; at most 10,000 lexical hits.

## Not yet

Compaction of dead vector rows; the free-space and size-cap check (D8) is
the caller's; Verify runs `integrity_check` and the FTS5 integrity check
only. Tests: `index::tests` (14), including rollback of a failed batch,
snapshot isolation, payload reclamation, stale vector planning, leftover
rows and format refusal.
