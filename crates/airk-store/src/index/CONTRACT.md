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

## Mapping of the current index (integration contract, 24 November 2026)

How the existing Store types map onto the engine. The reader side
(`CurrentRead` and its callers) and the writer side (`CurrentUpdater`, the
scan and enrichment) are built against this section; a change to it is made
here first.

- **Location.** `index.sqlite3` and the vector files live in the collection
  directory (`FileStore::directory()`), beside the existing owner header and
  gates, which stay. A directory with `current.sqlite3` and no
  `index.sqlite3` holds an old index: it is not read and is rebuilt through
  the existing full-rebuild owner (`rebuild_obsolete_index`).
- **meta keys** (besides the engine's own), written through `Batch::meta` in
  the batch's transaction and read with `IndexReader::meta`; a batch may not
  write an engine key. Integers in the digests below are 8-byte big-endian:
  - `scope`: canonical JSON of `CurrentScope` (root, collection, incarnation).
  - `collection_state`: canonical JSON of `CollectionState`, rewritten by the
    writer in the same transaction whenever configuration or counts change.
    It must round-trip byte-exact (Kernel hashes the configuration).
  - `lexical_commit`: 32 bytes, the `framed_digest(b"airk-index/lexical/1",
    [incarnation, sequence])` of the publication that last changed lexical
    rows. Readers report it as `CurrentHead::lexical_commit` and as every
    lexical candidate's `manifest_digest`.
- **Head.** A reader synthesizes `CurrentHead { version: 2, scope, sequence,
  artifact_root: framed_digest(b"airk-index/root/1", [incarnation,
  sequence]), lexical_sequence: sequence, lexical_commit, graph_head }`, where
  `graph_head` is `GraphHead::empty` over a graph scope equal to the index
  scope (graph instance = incarnation, specification = the model pipeline
  digest or `current-lexical-only/1`, record bytes as today), with
  `live_nodes` set to the committed vector rows still referenced by units.
  `CurrentHead::validate` is relaxed so this passes; the token digest becomes
  `framed_digest(b"current-index-head/3", [incarnation, sequence, format])`.
- **files.**
  - `record`: the exact bytes of today's `CapturedInput` (compact `Artifact`
    with `retained` set and `postings_version`).
  - `metadata`: `serde_json` bytes of the `InventoryFile`.
  - `payload`: `retained.object` (the payload digest).
  - `source_hash`: `content_hash`.
  - `language`: `artifact.language`.
- **payloads.** The JSON `ExtractionPayload` bytes that today form the
  "artifacts" object, keyed by the same digest. No source copy is stored.
  The source bytes of a file whose `extraction_method` is `utf8` are the
  payload's `text`. For other files the source is not retained (Q5):
  `read_retained_source` is unavailable, and `verify_retained_source`
  checks only that the stored record's hash matches `files.source_hash`.
- **chunks.** One row per chunk of the artifact rebuilt with
  `airk_core::binding::bind_payload`, in order. `stable_id` is
  `lexical::stable_id(path, ordinal)`. `citation` is the `serde_json` bytes
  of the `Citation` that lexical search returns today. `terms` is today's
  encoding: `airk_core::lexical::terms(format!("{path} {}", chunk.text))`
  mapped through `lexical::token` and joined with spaces. A lexical query
  maps its terms through the same `token`.
- **selectors.** Exactly the posting selectors of `postings::selectors`
  except `Files` and `Language` (which come from `files`): fact name, fact
  kind, edge source and edge target, with the terminal-call rule for
  `postings_version == 1`. `terminal_call_postings_complete` is true.
- **handles.** `Artifact`: the 16 bytes of `InventoryFile.state.artifact_id`.
  `Unit`: the canonical JSON of each continuity key in
  `InventoryFile.continuity`.
- **resources.** The same keys as today's resource map (`semantic-work`,
  `semantic-policy`, `scan-work`, `analysis`, `legacy-adoption`,
  `source-controls/1`, and so on), body = the bytes today's resource object
  holds. `embedding-cache` is not written. A source measurement for usage
  is the resource `source-measurement/<path>`; when that key exceeds the
  engine's 256-byte key bound it is `source-measurement/#<digest>` with the
  digest `framed_digest(b"airk-index/source-measurement/1", [path])`
  (`plain_writer::source_measurement_key`).
- **changes.** The writer records no change history yet; readers serve the
  absent-history view.
- **Accelerators and integrity.** `private_query_eligible` is false; every
  projection is `None`; lexical seals, Merkle proofs, grants, the journal and
  object reclamation have no counterpart.

## Not yet

Compaction of dead vector rows; the free-space and size-cap check (D8) is
the caller's; Verify runs `integrity_check` and the FTS5 integrity check
only. Tests: `index::tests` (15), including rollback of a failed batch,
snapshot isolation, payload reclamation, stale vector planning, leftover
rows, format refusal and caller meta. The current types' writer is
`current::plain_writer`; its end-to-end tests are
`airk-kernel/tests/plain_writer.rs`.
