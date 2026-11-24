# Indexing performance plan

**Status, 24 November 2026:** analysis complete, eight decisions recorded, design
sketched, and all ten questions of section 6 answered by the user. The user
said "Start P0.2" the same day. Step P0 is closed (section 2.6). The user
then directed: "Just move on and get the new implementation working and
tested." The new store is not implemented yet; work continues at P1 and P3.

This plan replaces the way AIRK stores and builds an index. It was written by
the primary agent (Claude Code) on the user's direction of 24 November 2026:
"current indexing is way too slow and my thinking is we're doing things that
might have been sourced from Munarium roots that provide no real value to us
for AIRK." On the same day the user stopped the Codex session and wrote: "We
will do this work first before returning to what it was doing."

Terms used here:

- **Base pass**: the part of indexing that reads sources and publishes text,
  symbols and the lexical index. Search by words and symbols works after it.
- **Vectors pass**: the part that computes an embedding vector for each
  semantic unit of text and stores it. Search by meaning works after it.
- **Unit**: one span of text that gets one vector (today about 256 tokens).
- **Old store**: the index format in the source today (`current.sqlite3` plus
  one object file per source and per extraction).
- **Plain store**: the format this plan proposes (section 5).

## 1. Summary

Indexing is slow for two separate reasons.

1. **The base pass runs at 3 to 5 files a second** because the old store is
   built to be tamper-evident. Each file's derived data is written as separate
   objects with one disk sync each, read back and verified about nine times,
   and every key change goes through a Merkle tree kept in SQLite that is
   computed three times for each publication.
2. **Vectors are built at about 20 a second** because inference runs on one
   thread with one text per call, and each vector insert reads over a hundred
   Merkle-proved records from SQLite.

The development machine has 32 cores, 128 GB of memory and an RTX A6000. The
vectors job was measured using about two cores.

Munarium and VCP contributed the shape: one artifact object per file, hash
provenance, generation-style publication and DiskANN. AIRK then hardened that
shape far beyond either project, and the hardening is where the time goes.

The user decided on 24 November 2026 that tamper-evidence of AIRK's own index
files is not a requirement (decision D1). That removes the reason for the
slow structures. The plan is to replace the old store with ordinary SQLite
tables and a flat vector file, to read and parse on all cores, and to run
inference on many processor threads and, where allowed, on the graphics card
(Q2, Q9, Q10). Lexical results become ranked in the same change (Q4).

**A correction to the first report.** I estimated that parallel inference
alone would make vectors 10 to 15 times faster. That was wrong. Graph
insertion in the old store takes 19.2 ms for each vector on one thread, which
caps the vectors pass near 52 a second whatever the inference speed. Parallel
inference on the old store gives about 2.5 times. The larger gain needs the
vector storage change as well. The answer to Q1 in section 6 settles the
order: the session pool is built as a component and is not shipped on the old
store.

## 2. Evidence

### 2.1 Measured speed

All figures come from receipts already in the repository. No new measurement
was run for this plan.

| What | Measured | Rate |
|---|---|---|
| munarium base pass: 1,023 files, 5,380 chunks | 360 to 372 s | 2.8 files/s |
| bookbuilder-v2 base pass: 8,116 files, 65,236 chunks | 1,675 to 1,720 s | 4.8 files/s |
| Vectors, 1,200-file sample, 21,796 vectors, before the 22 November fixes | 1,939 s | 11 vectors/s |
| The same sample after the fixes | 1,066 s | 20 vectors/s |
| Graph insertion on that sample | 41 ms, then 19.2 ms for each vector | 122 proved record reads for each insert after the fix |
| Live service vectors | 19,867 to 65,858 an hour | 5.5 to 18 vectors/s |
| MiniLM INT8 on this processor, one thread, one text per call | 21.5 passages/s | the inference ceiling today |
| 1,000 files of 1 KiB each, cold, vectors off, debug build | 75 s | 13 files/s |

Sources: the [ledger](../simplify/one-process-ledger.md#L15813) (line 15813
onward), [00-setup.md section 2.3](../arbiter/answer-evaluation/00-setup.md),
[semantic-index-optimization.md](../semantic-index-optimization.md) and the
[October speed review](../history/indexing-speed-review-2026-10-21.md).

Three readings of these figures:

- I read "index pass" in the ledger as the base pass. 40,536 vectors in 366 s
  would exceed the inference ceiling, so the pass cannot include them.
- The 1,066 s vectors run is almost exactly 21,796 vectors at 21.5 a second.
  One-thread inference is the limit today, with graph insertion (52 a second)
  just behind it.
- A file yields 18 to 40 vectors, about 8 for each chunk. By that ratio
  bookbuilder-v2 needs roughly 500,000 vectors, about seven hours at today's
  rate. This is my projection, not a measurement.

### 2.2 What one file costs today

**Scan** ([airk-ingest lib.rs](../../crates/airk-ingest/src/lib.rs#L925)).

- One coordinator thread reads every file and computes its SHA-256, whether
  or not the file changed. Modification times are never trusted.
- Only extraction runs on worker threads, and results are taken back in
  source order. The kernel's own plan caps extraction at four workers
  ([execution.rs](../../crates/airk-kernel/src/execution.rs#L57)). The service
  grants thread counts separately; I have not traced that path.
- The coordinator then serializes the payload to JSON three times, hashes it,
  compresses it, and writes a copy of the source and the payload as two
  separate files, each with its own disk sync and rename
  ([blobs.rs](../../crates/airk-store/src/blobs.rs#L825)).
- For plain text the payload also holds the whole text, so the text is stored
  twice.

**Publication**, for each batch of 32 to 4,096 files
([update.rs](../../crates/airk-kernel/src/current/update.rs#L535),
[journal.rs](../../crates/airk-store/src/current/journal.rs#L1999)).

- The payload just written is read back, hashed, decompressed and parsed
  about nine times for each file, by my count: vector planning, metadata,
  postings, the lexical dry run (twice), journal prepare, mark-applying, and
  the lexical replay at commit (twice).
- Every symbol name, kind, qualified name and edge becomes a key whose value
  is a JSON set of paths
  ([postings.rs](../../crates/airk-store/src/current/postings.rs#L97)).
- Every key lives in a 16-level Merkle tree with 65,536 buckets. One write
  reads the whole bucket, does 16 SQL lookups and 17 hashes, and encodes and
  decodes the bucket again
  ([map.rs](../../crates/airk-store/src/current/map.rs#L546)). The whole set
  of changes is computed three times: plan, prepare and commit.
- The lexical index is applied in a transaction that is rolled back to take a
  digest, then applied again at commit
  ([fts.rs](../../crates/airk-store/src/current/journal/fts.rs#L216)).
- Each publication is three SQL commits: prepared, applying and ready.

**Vectors pass**
([enrichment.rs](../../crates/airk-kernel/src/current/enrichment.rs#L1043)).

- Publications hold 32 files each, with at most 8 in a service turn, and each
  goes through the same journal.
- The complete list of pending files is read, sorted and rewritten as JSON on
  every batch, so that cost grows with the square of the file count.
- Inference uses one session, one thread and one text per call. The worker
  count passed in is ignored
  ([onnx.rs](../../crates/airk-retrieval/src/onnx.rs#L220)).
- DiskANN stores each node as a SQLite row, and every read verifies a 32-node
  Merkle proof
  ([graph.rs](../../crates/airk-store/src/current/graph.rs#L174)).

### 2.3 Where the time goes

**Vectors pass (measured).** One-thread inference at 21.5 a second is the
limit. Graph insertion at 52 a second is the next limit. The number of
vectors for each file multiplies both.

**Base pass (from reading the code).** This was my ranking before step P0
measured it. Section 2.6 corrects the order: the Merkle map is first, at
three quarters of the time. In my judgement the order was:

1. the repeated read-and-verify passes over stored payloads;
2. one disk sync for each object on Windows;
3. the triple Merkle computation;
4. the double lexical application;
5. the serial coordinator.

The October
review considered and declined removing the sync for each object, the double
application and the quota grants for each publication, each on integrity
grounds. Decision D1 removes those grounds.

**Ceilings in the old store.** An index is capped at 100,000 files
([runtime.rs](../../crates/airk-kernel/src/current/runtime.rs#L181)) and a map
bucket at 128 entries.

**A quality finding.** Lexical search is not ranked. The FTS5 table is created
with `detail=none` and `columnsize=0`, terms are joined with AND, and results
come back in the order of a hash
([lexical.rs](../../crates/airk-store/src/current/lexical.rs#L29)). Question
Q4 covers when to change that.

### 2.4 Where the design came from

[inspiration.md](../inspiration.md) and
[ADR 001](../decisions/001-local-retrieval-kernel.md) record what was taken as
ideas. No code was copied.

| Origin | What it contributed | Cost today |
|---|---|---|
| Munarium | One independently usable artifact file for each source file; path-and-hash provenance; DiskANN with verification | Objects and disk syncs for each file; a hash on every use; the choice of graph engine |
| VCP | Chunk records; generation publication and recovery | Layout built around chunks; the publication journal |
| AIRK itself | Merkle-authenticated map and graph in SQLite; prepare-then-replay; the lexical seal; quota grants for each publication; bit-identical vectors (one text per call) | Most of the measured time |

I did not read the Munarium repository for this plan. The attribution rests on
those two records.

### 2.5 Limits of this evidence

- The speed estimates for the plain store come from general experience with
  SQLite, FTS5, flat vector scans and ONNX Runtime, not from AIRK runs.
- The count of nine passes and the ranking in section 2.3 are from reading
  code.
- I have not traced the service's shared inference helper
  (`EmbeddingModel::Remote`), the service thread grants, or the query-time
  cost in depth.
- The AIRK MCP `status` call timed out for both live indexes during the
  analysis, so the analysis used local source reading.

### 2.6 Baseline measured for this plan (step P0, 24 November 2026)

**How it was measured.** The kernel's component harness (the
`vector_performance` test binary) was built from this branch with stage
timers (`airk-core/stage-timing`), in the development profile at optimization
level 2, as the evaluation images were. The sources are fresh `git archive`
exports of munarium (`9948345d`) and bookbuilder (`238b22a7`), whose archive
digests equal those of the earlier evaluation exports. The two
credential-setting scripts that the evaluation indexes denied were removed
from the private bookbuilder copy, unread. Each operation ran in its own
process on the development machine through
`scripts/indexing-plan/run-baseline.ps1`. Receipts are in the private folder
`artifacts/indexing-plan/p0-baseline-r1`.

The harness indexes through `Kernel::index`: one extraction worker, and one
publication for the whole scan. The service publishes in several growing
batches, so its times differ; it took 360 to 372 s for the same 1,023
munarium files.

| Operation | Source | Wall | Rate |
|---|---|---:|---:|
| Cold base pass | munarium: 1,023 files indexed, 5,380 chunks, 30 MB | 263.4 s | 3.9 files/s |
| Rescan, nothing changed | munarium | 1.8 s | |
| One edited file: index, then a lexical search | munarium | 2.8 s, then 0.06 s | |
| Cold base pass | bookbuilder-v2: 8,116 files indexed, 65,236 chunks, 91 MB | 1,048.3 s | 7.7 files/s |
| Rescan, nothing changed | bookbuilder-v2 | 14.3 s | |

The bookbuilder pass has the same shape as the munarium pass below:
publication is 83% of it and the Merkle map 64% (670 s), with 173 map keys
written for each file. Its index occupies 1.6 GB for 91 MB of indexed
source.

**The vectors run was stopped.** It had run for 17 minutes when the user
wrote, on 24 November 2026: "I know we're going to dramatically improve the
performance. I don't really care about measuring it exactly." and "I know
that indexing both of those repos took hours and hours last time. That's a
good enough measurement. Just move on and get the new implementation working
and tested." No vectors figure was taken in this step; the earlier receipts
of section 2.1 stand. Step P7.2 is reduced accordingly.

**Where the munarium cold base pass goes** (263.4 s; parts overlap slightly
and do not sum to the total):

| Part | Seconds | Share |
|---|---:|---:|
| Publication, all of it | 236.9 | 90% |
| Merkle map: planning puts and gets, the replay, the read-back and the final apply | 199.6 | 76% |
| Scan: reading sources, extraction and writing the two object files for each file | 26.2 | 10% |
| Loading stored extractions back, 10 times for each file | 11.7 | 4% |
| The metadata object for each file | 10.8 | 4% |
| The lexical index, applied twice | 9.0 | 3% |

Counts for each indexed file: 406 map keys written at commit, 818 planned
puts, 10 loads of the stored extraction (I had counted about nine in the
code), 5 verifications of the stored source, and 3 files written (source
copy, extraction and metadata), each with its own sync. Extraction itself
took 7.2 s for all 1,023 files on one thread. The index occupies 420 MB for
30 MB of indexed source.

**What this corrects.** Section 2.3 ranked the repeated read-and-verify
passes first and the Merkle map third. The measurement puts the Merkle map
first by a wide margin: three quarters of the cold base pass. The repeated
loads, the object files and the double lexical application are each a few
percent. Extraction is not a hidden cost. The decisions and the design
stand; the map is the structure the plain store removes.

## 3. What agents and users need from an index

These are kept by every option in this plan.

- Citations: path, line range (or page, slide, cell), and the hash of the file
  version that was indexed.
- An index revision, with honest reporting of freshness, readiness, coverage
  and truncation.
- Lexical search with path and language filters; symbols, references,
  outlines, imports and relationships.
- Search by meaning across code and prose.
- Extracted text and original locations for PDF, DOCX, PPTX and XLSX.
- Sources that are never written, isolation between indexes, budgets, and
  never serving a half-written index.

None of these needs cryptographic proof that AIRK's own private files were not
altered between writes. The MCP instructions already tell agents that the
index is untrusted and that live bytes must be verified before an edit.

## 4. Decisions

The user decided D1 on 24 November 2026 by answering "Not required" to the
question "Is tamper-evidence of AIRK's own index files a requirement?". The
user accepted D2 to D8 and the approach below on the same day: "All of your
recommendations are accepted".

This section is the recorded contract decision that the standing rules
require before a persisted format changes. Section 5.13 lists what changes.

| # | Decision | What it gives up |
|---|---|---|
| D1 | The index is crash-safe, checksummed and rebuildable. It is not tamper-evident. | A process running as the user that edits index files directly is not detected. Such a process could equally edit the sources. |
| D2 | Keep one stored copy of each file's text, compressed, written in the batch transaction. | Disk: about half of the source size with the LZ4 codec already in use, by my estimate. |
| D3 | Detect change by file size, modification time and file identity. Hash only when they differ. Run a full hash audit on a schedule and on request. | A same-size edit with a restored timestamp is missed until the audit. Git accepts the same risk. |
| D4 | Size each index for about a million vectors held in memory. Use an exact flat scan first, and a graph only if measurement shows the need. Larger sources use index sets. | DiskANN and the recorded decision that selected it. A single index larger than memory. |
| D5 | Vectors must come from the same model, tokenizer and precision, with the route recorded. They need not be bit-identical. | Exact-match vectors across processor, graphics card and batch size. |
| D6 | Existing indexes are rebuilt. There is no migration. | One rebuild for each index. See question Q6 for indexes that cannot be rebuilt. |
| D7 | Sync to disk once for each batch commit, not for each object. | After a power loss the last uncommitted batch is done again. |
| D8 | Check free space and the size cap for each batch. Drop the reservations and grants for each publication. | Exact prepaid accounting across concurrent writers. |

**Accepted approach.** The plain store is the destination. The inference
change was accepted as the first step; the answer to Q1 keeps it first as a
component but does not ship it on the old store. One stage-level measurement
of the base pass runs before design work depends on the ranking in section
2.3.

**Notes on the decisions.**

- D1 keeps the SHA-256 of each source file, for citations, staleness and
  reuse. It keeps atomic publication. It keeps authentication of model
  weights when they are installed, because those files come from outside.
- D2 withdraws the idea of reading plain text live from the workspace. A
  citation stays readable when a file changes during a task. My earlier
  estimate of "a quarter to a third" assumed a stronger codec than the one in
  use; the figure in the table corrects it.
- D4 leans on index sets, which exist today. At 18 to 40 vectors for each
  file, a million vectors is roughly 25,000 to 55,000 files.
- D5's "precision" means the stored weights. By the answer to Q9 one FP16
  file serves both routes: the graphics card computes in FP16 and the
  processor in FP32 from the same weights. Their vectors differ in low bits
  and share one index.
- D6 is supported by the standing assignment text, "Current-version index
  regeneration is authorized when upgrade cannot preserve the current
  contract" ([agent-current-work.md](../agent-current-work.md)). The user's
  acceptance on 24 November settles it for this plan.

## 5. Design sketch: the plain store

This section says how the plain store would work. Names of files, tables and
columns are proposals. Step P1 fixes them in a contract before any code.

### 5.1 Files on disk

Each index keeps its present folder, owner header and filesystem ownership
guard. Inside it:

| File | Holds |
|---|---|
| `index.sqlite3` (with its WAL files) | Everything except vectors |
| `vectors-<generation>.f32` | One fixed-width row of 32-bit floats for each distinct embedding input |

There are no object files for sources, extractions, metadata or resources.

SQLite runs in WAL mode with `synchronous=FULL`, as the service database does
today ([database.rs](../../crates/airk-store/src/current/database.rs#L524)).
One commit is one disk sync (D7).

### 5.2 Tables

```sql
-- One row per setting: format, index identity, configuration, extractor
-- version, model identity, sequence, committed vector rows, scan state.
CREATE TABLE meta(key TEXT PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;

CREATE TABLE files(
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,        -- logical path; '@alias/...' for added roots
  source_hash BLOB NOT NULL,        -- SHA-256 of the source bytes (citations)
  source_bytes INTEGER NOT NULL,
  stat_size INTEGER, stat_mtime_ns INTEGER, stat_file_id BLOB,  -- D3
  hash_checked_ns INTEGER,          -- last time the bytes were hashed
  language TEXT, extraction_method TEXT, parse_status TEXT,
  profile_id BLOB NOT NULL,         -- extraction profile identity
  artifact_id BLOB NOT NULL,        -- stable handle kept across edits
  first_seq INTEGER, path_seq INTEGER, content_seq INTEGER, evidence_seq INTEGER,
  indexed_lines INTEGER,
  vector_state INTEGER NOT NULL     -- not selected, pending, done or failed
);

-- D2: the extracted text, stored once. For plain text this is the source.
CREATE TABLE file_text(
  file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
  raw_bytes INTEGER NOT NULL, checksum INTEGER NOT NULL, body BLOB NOT NULL);

-- Citation units, as today: at most 8 KiB or 160 lines each.
CREATE TABLE chunks(
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,
  stable_id BLOB NOT NULL,          -- today's lexical document identity
  byte_start INTEGER NOT NULL, byte_end INTEGER NOT NULL,
  start_line INTEGER NOT NULL, end_line INTEGER NOT NULL,
  location BLOB,                    -- page, slide, cell or region; NULL for lines
  UNIQUE(file_id, ordinal));

-- Lexical index over AIRK's own normalized terms, keyed by chunk id.
CREATE VIRTUAL TABLE fts USING fts5(terms, content='', contentless_delete=1,
  tokenize='ascii tokenchars ''_''');

-- One row per native fact: definition, call, import, heading and so on.
CREATE TABLE facts(
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,         -- native order, kept for outlines
  kind TEXT NOT NULL, name TEXT NOT NULL,
  qualified_name TEXT, symbol_id TEXT, owner_id TEXT, target TEXT,
  start_line INTEGER, end_line INTEGER,
  detail BLOB);                     -- remaining fields
CREATE INDEX facts_name ON facts(name);
CREATE INDEX facts_qualified ON facts(qualified_name);
CREATE INDEX facts_symbol ON facts(symbol_id);
CREATE INDEX facts_kind ON facts(kind, file_id);
CREATE INDEX facts_owner ON facts(owner_id);
CREATE INDEX facts_target ON facts(target);

-- Semantic units. Identical inputs share one vector row.
CREATE TABLE units(
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,
  embedding_key BLOB NOT NULL,      -- hash of model identity and input text
  input_spans BLOB NOT NULL,        -- context and body spans in file_text
  members BLOB NOT NULL);           -- chunk spans this unit cites
CREATE INDEX units_key ON units(embedding_key);

CREATE TABLE vector_rows(
  embedding_key BLOB PRIMARY KEY,
  row INTEGER NOT NULL UNIQUE,      -- position in the vector file
  block_checksum INTEGER) WITHOUT ROWID;

-- Bounded log behind the changes tool, and stable unit handles.
CREATE TABLE changes(seq INTEGER, path TEXT, kind TEXT, detail BLOB);
CREATE TABLE continuity(file_id INTEGER REFERENCES files(id) ON DELETE CASCADE,
  unit_handle BLOB, descriptor BLOB);
```

What these replace: the Merkle map and its fourteen key domains, postings,
the native and query projections, text ranges, the vector map, the pending
journal tables, and the pending-work list. Imported analysis (for example
SCIP) gets its own two tables in the same style; P1 specifies them.

**A simpler realization to settle in P1.2.** Today's read handlers take a
file's whole extraction (text, chunks, facts, regions, relationships and
continuity) as one typed value, which is an existing Core contract. The
plain store can keep that value as one compressed row for each file and use
`chunks`, `fts`, `facts` and `units` as thin lookup tables that point into
it, with only the columns the queries filter on. Reads then load one row
where they load one object file today, the handlers' inputs do not change,
and step P5 shrinks to the lookups. For plain text the conversion already
requires the extraction's text to equal the source bytes
([legacy.rs](../../crates/airk-ingest/src/adapters/legacy.rs#L113)), so the
text is still stored once. P1.2 chooses between this and the fully
relational tables above.

Open design checks, settled by a small measurement in P3 and not by the user:
the FTS5 `detail` level, which must support BM25 ranking (Q4) at an
acceptable index size, and whether composite regions such as notebook cells
need their own text table.

### 5.3 Vector file

- A header (format, dimensions, model pipeline identity) followed by rows of
  normalized 32-bit floats. For MiniLM a row is 1,536 bytes; a million rows
  are 1.5 GB.
- The file only grows. `meta` records how many rows are committed. Rows past
  that count are ignored on open and overwritten by the next append.
- A row whose last unit is deleted is marked dead in memory from
  `vector_rows`. When more than about a third are dead, the writer copies the
  live rows to a new generation file and switches to it in one transaction.
- Search is an exact scan: one dot product for each live row, on several
  threads, keeping the best results. A path or scope filter becomes a list of
  rows first, as the exact subset search does today. By my estimate a scan of
  a million rows takes tens of milliseconds on several threads; P3 measures
  it. If it misses the latency target, the fallback is an 8-bit copy for the
  first pass or an in-memory graph (follow-up F3).
- A full rebuild of text keeps `vector_rows`. A unit whose input is unchanged
  finds its vector by `embedding_key` and is not embedded again. A model
  change starts a new generation file.

### 5.4 Indexing a batch (base pass)

1. **Walk** the sources with the present ignore, policy and budget rules, in
   the present priority order (documentation and manifests first).
2. **Check** each entry against its `files` row (section 5.5). An unchanged
   file costs one row lookup.
3. **Read and parse on all cores.** Each worker reads one file with the
   present protected read, computes its SHA-256, selects the profile,
   extracts text, chunks and facts, computes the lexical terms for each
   chunk, plans the semantic units with the model's tokenizer, and compresses
   the text. Nothing is written by a worker.
4. **Write on one thread.** The writer takes finished files in batches (for
   example 2,000 files, 64 MiB or two seconds, whichever comes first). One
   transaction deletes the old rows of each changed file and inserts the new
   ones, records the change, and raises the sequence. One commit, one sync.
5. **Reconcile deletions** in a final transaction when the walk is complete
   and the roots are still reachable, with the present rules for what counts
   as observed absence.

Readers see each batch as soon as it commits. A crash loses at most the
uncommitted batch, and the next scan picks those files up again because
their rows are missing or stale.

### 5.5 Change detection

- A file whose size, modification time and file identity match its row is
  unchanged. It is not read.
- Otherwise the file is read and hashed. If the hash matches, only the stat
  columns are updated. If not, the file is extracted and replaced.
- A file modified within the timestamp resolution of the last scan is always
  hashed, which is the rule Git uses for the same problem.
- The audit reads and hashes every file and is run on a schedule, on request,
  and by Verify. Watcher and editor hints keep working as today; they name
  the paths to check first.

### 5.6 Embedding

- One model file serves both routes (Q9): MiniLM weights stored as FP16. The
  processor computes in FP32 from those weights and the graphics card
  computes in FP16. Vectors from both routes share one index, and a query can
  be embedded on either route. The two INT8 files retire, so the model
  identity changes and every vector is computed again. D6 rebuilds every
  index in any case.
- On the processor the model holds a pool of sessions, each on one thread.
- On the graphics card the model runs in batches through the CUDA runtime
  that the Arbiter models already use, under the present device admission.
  Embedding on the card has its own setting, which allows it by default
  (Q10). When the card is turned off, absent, refused by admission or fails,
  embedding uses the processor pool and the status names the route used.
- Pending work is `files.vector_state` and the units without a row in
  `vector_rows`. There is no pending list to rewrite.
- A vector batch is: embed, append rows to the vector file, sync that file,
  then one small transaction that inserts into `vector_rows`, marks finished
  files, records the committed row count and raises the sequence.
- Base batches and vector batches are both short transactions on the one
  writer, so they interleave. A newly edited file no longer waits behind a
  long vectors turn (issue #64).

### 5.7 Reads: what each tool uses

By my reading of the tool descriptions and the Store methods. P1 confirms
each row against the live handlers.

| Tool | Reads in the plain store |
|---|---|
| `status`, `diagnostics`, `capabilities` | `meta`; counts from `files` by `vector_state`; scan state |
| `search` (lexical) | `fts` ranked by BM25, then `chunks` and `files`; snippet cut from `file_text` |
| `search` (vector, hybrid) | the query vector; a scan of the vector file; `vector_rows`, `units`, `chunks`, `files`; fusion stays in Kernel |
| `symbols`, `code_facts`, `document_facts`, `imports` | `facts` by name, kind, language and path, paged by file and ordinal |
| `outline` | `facts` for one file in ordinal order |
| `relationships` | `facts` by owner and target; the imported tables |
| `references` | `fts` for candidates, then positions found in `file_text` |
| `read_file`, `context` | `files`, then lines cut from `file_text` |
| `file_info`, `list_files` | `files` |
| `changes` | `changes`, `continuity` and the sequence columns of `files` |
| `answer`, `parse`, and the draft and template tools | compose the reads above or do not read index contents |

Combined reads over several indexes, index sets, scopes and rank fusion sit
above the Store and do not change.

### 5.8 Revision, readiness and coverage

- The revision is the index incarnation plus a sequence that rises with each
  commit. The token keeps its present wire shape. Its digest becomes a hash
  of the incarnation, sequence and format, so it identifies a publication but
  no longer commits to content.
- Discovery readiness comes from the scan state in `meta`. Semantic readiness
  comes from counts of `vector_state`. Both keep their present fields.
- Truncation, coverage and saved-source verification keep their present
  meaning and fields.

### 5.9 Crash safety, corruption and Verify

- Atomicity is the SQLite transaction. There is no prepare, apply or replay.
- Each stored text carries a checksum, checked when it is read. Vector rows
  are covered by a checksum for each block of rows, checked by Verify.
- Verify runs SQLite's integrity check, the FTS5 integrity check, the text
  and vector checksums, cross-table counts, and on request the hash audit of
  live sources.
- Any failure marks the index as needing a rebuild, with a typed error. It is
  never presented as an empty index. A rebuild reads the sources again.

### 5.10 Concurrency

- One writer for each index, under the present writer lock and execution
  lease.
- Readers use WAL snapshots. A read is no longer refused while a publication
  is being applied.
- A reader holding an old snapshot may still have the previous vector
  generation file open. That file is removed at a later open, not at once.
- The gate that excludes readers during removal or relocation of an index
  folder stays.

### 5.11 Limits and disk checks

- The 100,000-file figure stays as a configurable limit. It is no longer a
  structural ceiling, because the scan no longer loads the whole inventory
  into memory.
- Before each batch the writer checks free space on the volume against a
  floor plus the batch's estimated growth, and the index's size against its
  cap (D8). Model assets, relocation and the service catalog keep their own
  accounting; this plan does not touch them.

### 5.12 What is retired

Each item is deleted only with the evidence the standing rules require: no
caller after a clean compile and Clippy.

- The Merkle map and graph, proofs, overlays and digests.
- The pending journal with its prepared and applying phases, captured-input
  digests and replay.
- The lexical dry run, seal and witness cache.
- Object files for sources, extractions, metadata and resources, and their
  reclamation.
- Postings, the native and query projections, text ranges and the vector map.
- The DiskANN provider and the mutable disk index.
- Reservations and grants for each publication.
- The embedding cache packs, replaced by `vector_rows`.

### 5.13 Contract changes this plan makes

1. A new persisted index format with its own identifier. The old format is
   not read by the new version.
2. The inputs of the revision token's digest change (section 5.8).
3. The graph specification identity disappears. The vector file has its own
   format identity.
4. Reads are served during publication.
5. Verify changes meaning (section 5.9). "Reclaim unreferenced index objects"
   has nothing left to do.
6. Counters in index reports that describe proofs, graph reads and cache
   packs report zero.
7. Lexical results are ordered by BM25 relevance in place of hash order (Q4).
   A result must still contain every query term. The `search` tool
   description and the help text change to say so.
8. The embedding model identity changes from the two INT8 profiles to one
   profile with FP16 weights (Q9).
9. A new service setting allows embedding on the graphics card and is on by
   default (Q10).

**One wire shape addition.** The setting of item 9 is added to the service
configuration and to the Admin and extension surfaces through the
schema-to-TypeScript generation. Every other tool schema and the I1, I2 and
I3 contracts stay as they are.

## 6. Questions and answers

The user answered all ten on 24 November 2026. Each entry keeps the options
that were offered, my recommendation and the answer. Where the answer differs
from my recommendation, the entry says what follows from it.

### Q1. Order of work after the correction

The session pool alone gives about 2.5 times on vectors while the old store
is in place (section 1).

- **A (recommended).** Build and test the session pool first as a component,
  because the plain store needs it. Do not qualify or ship it on the old
  store. Go straight to the plain store.
- **B.** Ship the pool on the old store first as an interim release. This
  costs one qualification and installer cycle on code that is about to be
  replaced.

**Answer, 24 November 2026: A.** The pool is built and tested in P2 and first
ships with the plain store.

### Q2. Embedding on the graphics card

- **A (recommended).** After this plan. This plan uses the processor pool
  only. By my estimate that reaches 200 vectors a second or more on this
  machine, which is about 40 minutes for an index the size of bookbuilder-v2
  and 3 minutes for munarium.
- **B.** In this plan. It needs a MiniLM export that is not INT8, a CUDA path
  for embeddings, and either a second pipeline whose vectors cannot mix with
  the processor's or a change of precision on the processor. It adds a
  qualification on each platform.

**Answer, 24 November 2026: B, in this plan.** This differs from my
recommendation. It adds steps P2.5 to P2.9 and questions Q9 and Q10.
Follow-up F1 shrinks to the routes that are not CUDA.

### Q3. Number of vectors for each file

- **A (recommended).** Keep today's unit planner unchanged in this plan.
  Evaluate fewer, larger or more selective units afterwards with the Answer
  evaluation harness, as its own change.
- **B.** Change the units now, for example one unit for each symbol or
  section. Fewer vectors, but a speed change and a quality change would land
  together and could not be told apart.

**Answer, 24 November 2026: A.** The unit planner is unchanged. Follow-up F2
stays.

### Q4. Lexical result order

- **A (recommended).** The plain store keeps today's result order, so old and
  new results can be compared one for one. The index is built able to rank.
  Ranking by BM25 is switched on in the next step with its own measurement.
- **B.** Switch to BM25 ranking in the same change.

**Answer, 24 November 2026: B, in the same change.** This differs from my
recommendation. By my reading the answer changes the order only: a result
must still contain every query term, and BM25 orders those results, with
today's identity as the tie-break. Old and new lexical results are then
compared as complete sets and not in order (P1.3, P5.1), and the ranked order
gets its own tests.

### Q5. Original bytes of binary documents

D2 keeps one copy of each file's text. For PDF, DOCX, XLSX and PPTX the text
is the extraction, not the file.

- **A (recommended).** Store the extracted text and locations only.
- **B.** Also keep the original bytes. This allows extraction to be repeated
  without the source file, and costs disk and write time.

**Answer, 24 November 2026: A.** Extracted text and locations only.

### Q6. Indexes that cannot be rebuilt

D6 rebuilds every index from its sources. An index whose source folder is
gone cannot be rebuilt. The same holds for "legacy saved evidence" that an
earlier release adopted; by my reading of
[legacy_adoption.rs](../../crates/airk-kernel/src/legacy_adoption.rs) adoption
is already unsupported and only its observation remains.

- **A (recommended).** Such an index is reported as "needs rebuild; source
  unavailable". Its old files stay on disk untouched until the user removes
  them. The new version cannot read them; an older installed version can.
- **B.** Write a one-time importer from the old format. This is large work
  and keeps the old read code alive.

**Answer, 24 November 2026: A.** The index is reported as needing a rebuild
with its source unavailable, and its old files are left in place.

### Q7. Branch and commits

- **A (recommended).** A new branch from the head of `feat/arbiter-final`,
  named `feat/index-plain-store`, with pushes of that branch to `origin`
  authorized. This plan is its first commit. The paused Arbiter work and
  Codex's uncommitted files stay as they are.
- **B.** Stay on `feat/arbiter-final`.

**Answer, 24 November 2026: A.** Branch `feat/index-plain-store` from the head
of `feat/arbiter-final`, with pushes of that branch to `origin` authorized.
No pull request, merge, publication or deployment is authorized.

### Q8. Records for other sessions

`docs/agent-current-work.md` and the "Current assignment" paragraph of
`CLAUDE.md` and `AGENTS.md` still name the Answer evaluation as the active
plan.

- **A (recommended).** Update all three and add a ledger entry, so Codex and
  later sessions resume this work.
- **B.** Leave them. Only this plan records the change of direction.

**Answer, 24 November 2026: A.** Done with the commit that adds this plan.
Codex had already recorded the pause of the Answer campaign in the assignment
file and the ledger on the same day.

### Q9. One model file or two (follows from Q2)

A query must be embedded by the same model as the index it searches, and a
processor fallback must always exist.

- **A (recommended).** One file: MiniLM weights stored as FP16. The graphics
  card computes in FP16 and the processor in FP32 from the same weights,
  which is the recipe already chosen for the Arbiter models. Vectors from
  both routes share one index. The two INT8 files retire, which reverses
  "MiniLM INT8 stays" of 20 November. On the processor this is about 20%
  slower for each thread than INT8 (17.0 against 21.5 passages a second in
  the October measurement) and uses more memory for each session.
- **B.** Two pipelines: INT8 for the processor and an FP16 file for the card.
  An index belongs to the pipeline that built it, and three weight files
  ship.

**Answer, 24 November 2026: A.**

### Q10. The setting for embedding on the graphics card (follows from Q2)

- **A (recommended).** Embedding follows the existing service-wide "Allow GPU
  use" setting, which is off by default.
- **B.** A separate setting for embeddings alone.

**Answer, 24 November 2026: B.** In the user's words: "Separate setting.
'allow' by default." This differs from my recommendation. Embedding on the
card has its own setting and is allowed unless the user turns it off. The
Arbiter setting keeps its own default of off. By my reading the platforms in
this plan are Windows CUDA and Linux CUDA, as in the present campaign; the
user has not confirmed that.

## 7. Implementation plan

### 7.1 Rules for execution

- The standing rules in `CLAUDE.md` apply: verified identity before writes,
  one Cargo owner for each target, contract before consumers, mechanical
  restructuring kept apart from behaviour changes, deletion only with
  evidence, and receipts for expensive runs.
- One primary deliverable is in integration at a time. The steps below are
  that sequence. P2 is the only step that can run beside another.
- Delegation is limited: at most two implementation agents at once, on
  disjoint files and separate Cargo targets, and only where a step names
  them.
- Platform order follows the present campaign: Windows first, then Linux.
  macOS waits with the rest of the macOS work.
- Each step ends with its checks run, its result in the ledger and one
  signed-off commit.

### 7.2 Targets

These are my estimates of what the design should reach on the development
machine. They are guides and not gates: on 24 November 2026 the user said an
exact measurement is not wanted. P7 runs each source once on the new store
to confirm it is far faster, and reports a result that is not.

| Measure | Today | Target |
|---|---|---|
| Base pass, bookbuilder-v2 (8,116 files) | 1,675 to 1,720 s | 60 s or less |
| Base pass, munarium (1,023 files) | 360 to 372 s | 15 s or less |
| Rescan with nothing changed, bookbuilder-v2 | every file read and hashed; not measured | 5 s or less |
| One edited file to searchable by words | 6.5 s on a 1,000-file fixture (debug build) | 1 s or less |
| Vectors on the processor (FP16 weights, FP32 arithmetic) | 20 a second | 150 a second or more |
| Vectors on the graphics card | none | 1,500 a second or more |
| Vector search over a million rows | not measured | measured in P3; a budget is set there |
| Reads during publication | refused while applying | served |

On an eight-core machine the processor figure scales down with the cores; by
my estimate three to five times today's rate. The graphics-card figure is my
estimate for the A6000 and is the least certain number in this table.

### 7.3 Steps

**P0. Records and baseline.**

| Part | Work | Done when |
|---|---|---|
| P0.1 | Commit this plan on the branch from Q7. Apply the answer to Q8. Link the plan from the documentation index. | Commit exists; repository and whitespace checks pass |
| P0.2 | Add stage timers to the old base pass behind the test-support feature: walk, read and hash, extract, retain, and each publication stage. | Timers compile out of product builds; scoped Clippy clean |
| P0.3 | Index fresh read-only exports of munarium and bookbuilder-v2 into a scratch root on D:, with the disk budget recorded first. Record base-pass stage times, unchanged rescan, vectors rate, query latencies and index size. | Receipts in the private campaign folder; a "today" table added to this plan |
| P0.4 | Read `scripts/benchmark-indexing.py` and extend it, or write a successor, so the same measures can be repeated in P7. | Script tests pass; the two runs of P0.3 are reproducible from it |

P0 is closed. P0.1 is done with the commit that adds this plan. P0.2 and
P0.4 are done: the timers are behind the `airk-core/stage-timing` feature and
the scripts are in `scripts/indexing-plan`. P0.3 ran the base-pass
operations of section 2.6, which corrected the ranking of section 2.3, and
was stopped before its vectors and search samples on the user's word.

**P1. Seam inventory and contract.** No behaviour changes.

| Part | Work | Done when |
|---|---|---|
| P1.1 | List every Store function that Kernel, the Store service code and tests call on an index, and classify each as kept (and which table serves it), retired, or rewritten. Section 9 holds the first cut from the census. | Every function of section 9 is confirmed against its callers |
| P1.2 | Write the contract for the plain store: schema, files, transaction rules, revision identity, recovery, limits, Verify. Give it a format identifier distinct from `airk-state/1`. | `CONTRACT.md` beside the new module, reviewed |
| P1.3 | Build the parity harness. It runs every canonical read handler on the same sources through the old store and the new, and compares full results. Only the differences listed in section 5.13 are normalized. Lexical and hybrid search are compared as complete result sets, because their order changes (Q4). | Harness runs old against old with zero differences |
| P1.4 | Freeze expected results from the old store for the parity corpus, so they stay independent once the old code is gone. | Fixtures committed; they hold no private source |

**P2. Embedding: session pool, FP16 model and graphics card.** Independent of
the store. Nothing from P2 ships on the old store (Q1).

| Part | Work | Done when |
|---|---|---|
| P2.1 | Read the shared inference helper path and the service thread grants. Record what changes there. | Notes in this plan |
| P2.2 | Give the ONNX model a pool of one-thread sessions. Spread the texts of a batch across them and restore input order. Keep one text per call. | Vectors byte-equal to the single-session path on the real-model oracle |
| P2.3 | Raise the inference worker limit from 8 and derive the default from cores and memory. Keep pause, cancel and memory admission. | Decision tests with injected limits pass |
| P2.4 | Measure with `vector_performance`. | Inference rate recorded for 1, 4, 8 and 16 sessions |
| P2.5 | Export MiniLM with FP16 weights from the pinned upstream revision with the maintained exporter. Record its hash, manifest, licence and catalog entry. New setup stops selecting the INT8 profiles. | The catalog authenticates the file; an independent check reproduces the FP32 original's outputs within a stated tolerance |
| P2.6 | The processor route: FP32 arithmetic from the FP16 weights, on the session pool. | Vectors match the reference within the tolerance; rate and memory recorded for 1, 4, 8 and 16 sessions |
| P2.7 | The graphics-card route: batched FP16 inference through the present CUDA runtime and device admission. Windows first, then Linux. | Vectors match the processor route within the tolerance; rate and memory held recorded; the card is released between jobs |
| P2.8 | The new setting (Q10), allowed by default, added to the service configuration through the schema generation, with its Admin and extension surfaces. Route choice, fallback to the processor, and status that names the route used. | Decision tests for a card that is allowed, turned off, absent, refused by admission and failing; the generated contract check passes |
| P2.9 | Compare retrieval between today's INT8 vectors and the FP16 vectors on the reviewed query set and the parity corpus. | Result recorded; any loss is reported to the user before P7 |

Notes for P2.1, from reading on 24 November 2026:

- In the service, embedding goes through `RemoteModel`, which sends each
  batch of at most 32 texts with a worker count to the shared inference
  owner. The ONNX model ignores the count and runs one session on one thread
  ([onnx.rs](../../crates/airk-retrieval/src/onnx.rs#L413)).
- A service work grant allows 1 to 4 extraction threads and 1 to 4 inference
  threads, and the resource configuration accepts an inference override of
  1 to 4
  ([service_resources.rs](../../crates/airk-core/src/service_resources.rs#L194)).
  The pool needs these limits raised.
- Outside the service the kernel's own plan caps extraction at 4 workers and
  inference at 8. Its memory rule reduced extraction to one worker in the
  baseline run on this machine.
- `ResourceConfiguration` in the same file holds `allow_gpu`. The embedding
  setting of Q10 joins it there.
- Still to read: the session host in `airk-inference`
  (`inference_engine/pool`, `embedding_pool_client.rs`) and the supervisor's
  sizing of grants.

**P3. Store: schema, writer and vector file.**

| Part | Work | Done when |
|---|---|---|
| P3.1 | New module in `airk-store` for the plain store: create, open, owner header, WAL settings, format check. | Opens and refuses foreign or old formats with typed errors |
| P3.2 | The batch writer: replace and delete files with their text, chunks, lexical rows, facts, units and change records in one transaction. | Kill-during-commit tests leave either the old or the new batch, never a mix |
| P3.3 | The vector file: append, commit, open, liveness, compaction, exact scan with a scalar path and a vector-instruction path. | Scan equals a naive reference on random data; a crash between append and commit loses nothing committed |
| P3.4 | Settle the two design checks of section 5.2 and the scan latency budget by measurement. | Results recorded in the contract |
| P3.5 | The free-space and size-cap check (D8). | Refusal tests with an injected volume observer |

**P4. Kernel: scan and embedding pipeline.**

| Part | Work | Done when |
|---|---|---|
| P4.1 | The parallel scan of section 5.4, with the present source policy, ignore rules, budgets, progress events, pause and cancel. | Same inventory as the old scan on the parity corpus |
| P4.2 | Change detection (D3) with the audit and the timestamp rule. | Tests cover unchanged, touched, same-size edit found by audit, and edits inside the timestamp resolution |
| P4.3 | Deletion reconciliation and move detection. | Old move and deletion tests pass against the new store |
| P4.4 | The embedding stage of section 5.6 on the routes from P2, with reuse by `embedding_key`. | Semantic readiness reaches ready; a rebuild with unchanged text embeds nothing |
| P4.5 | Hints from the watcher and editor; interleaving of base and vector batches. | An edit during a vectors pass is searchable by words within the target |

**P5. Reads.**

| Part | Work | Done when |
|---|---|---|
| P5.1 | Lexical, vector and hybrid search with snippets, filters and scopes. Lexical results are ranked by BM25 with every query term required (Q4). | Complete result sets equal the old store's; ranked-order tests on known-item queries pass; differences in hybrid results are recorded |
| P5.2 | Symbols, facts, outline, imports, relationships and references. | Parity on the corpus |
| P5.3 | File reads, context, file information and listings. | Parity on the corpus |
| P5.4 | Changes, stable handles, move following and conditional reads. | Parity on the corpus |
| P5.5 | Status, readiness, coverage and diagnostics. | Parity apart from the counters of section 5.13 |
| P5.6 | The per-surface selections of [read-interface-parity.md](../read-interface-parity.md) for CLI, MCP, Admin and Workbench through real transports. | Recorded per surface |

**P6. Service integration.**

| Part | Work | Done when |
|---|---|---|
| P6.1 | Index, vectors, rebuild, maintain and split jobs on the plain store. | Job tests pass |
| P6.2 | Verify as in section 5.9, with its persisted progress. | Injected corruption of each kind is found |
| P6.3 | Usage and size reporting, relocation, removal and reset with the new files. | Existing lifecycle tests pass |
| P6.4 | Detection of an old-format index and the rebuild job (D6), following the answer to Q6. | An old fixture index is rebuilt; an index without sources reports as Q6 says |
| P6.5 | The Answer evaluation's private search switch carried onto the plain store. | Its tests pass |

**P7. Switch, measure, delete, document.**

| Part | Work | Done when |
|---|---|---|
| P7.1 | Full parity run on the corpus, munarium and bookbuilder-v2. | Zero unexplained differences |
| P7.2 | One cold base pass and one rescan of each source on the new store, and one vectors run of munarium. | The times recorded beside those of section 2.6; no exact comparison is required |
| P7.3 | Make the plain store the only store. | Integration preflight passes on a matched pair |
| P7.4 | Delete the code of section 5.12 with evidence. | Clean compile and Clippy with no caller left |
| P7.5 | Update the architecture guide, the index runtime document, contracts, help content and release notes. Bump the version when an installer is built. | Final workspace gate, doctests and repository checks pass |

### 7.4 Checks for each kind of step

From the [validation matrix](../../CONTRIBUTING.md#agent-validation-matrix):

- Rust in one crate: format, `python scripts/dev-check.py -p <crate>`, focused
  Nextest runs with the selected count reported. No `cargo check` before
  Clippy.
- A new persisted format: the contract and fixtures land before consumers
  (P1 before P3).
- Reads: full-result comparisons through canonical handlers and real
  transports (P5.6).
- Persistence: cheap failure and recovery tests first (P3.2, P3.3), then the
  real process checks (P7.3).
- A release claim: the final workspace gate, doctests, and the runtime and
  package checks. Scoped passes do not replace them.

### 7.5 Risks

| Risk | Handling |
|---|---|
| Extraction itself is slower than assumed, so the base pass misses its target | P0 measures it before design work depends on it |
| The exact vector scan is too slow at a million rows | P3.4 measures it; follow-up F3 is the fallback |
| A read behaviour of the old store is missed | The parity harness compares full results for every handler |
| The work grows because the seam is wide: 122 Store functions have call sites outside Store, 117 of them in Kernel (section 9) | P1 sizes it before P3; the step list is revised then |
| A second store lives beside the first for too long | P7.4 deletes the old one in the same campaign |
| Disk use during the work | Each run records its budget first, under the standing disk rules |
| The FP16 processor route is slower or heavier than the October figures suggest | P2.6 measures it before the INT8 files are retired |
| Embedding on the card competes with the Arbiter models for its memory | P2.7 uses the present device admission and records the memory held |
| Ranked lexical order changes hybrid results in ways the parity run cannot judge | P5.1 records the differences; the Answer evaluation starts from a new baseline afterwards |

No completion date is given. Sizes relative to each other: P0 is small; P1,
P2 and P6 are medium; P3, P4 and P5 are large.

## 8. Follow-ups outside this plan

| # | Follow-up | Depends on |
|---|---|---|
| F1 | Graphics-card routes other than CUDA (Radeon, macOS) | The Arbiter work on those routes |
| F2 | Fewer or larger semantic units | Q3 |
| F3 | An 8-bit vector copy or an in-memory graph | P3.4 showing the need |
| F4 | A substring index for code | Measurement after P7 |
| F5 | A static embedding tier for instant vectors | A model decision |

## 9. Seam inventory (step P1.1, first cut)

This is the list of Store functions the plain store must serve, replace or
retire. It comes from `scripts/indexing-plan/seam_census.py`, which reads
source text, and from my reading of the names. P1.1 confirms each row
against its callers before P3 starts.

**Census, 24 November 2026.** The reader, the writer and the file store have
319 public functions between them: 102 on the reader, 37 on the writer, 178
on the file store and 2 on both reader and writer. Of these, 122 have
textual call sites outside Store: 45 on the reader, 19 on the writer and 59
on the file store (one is counted for both reader and writer). Kernel calls
117 of the 122. A textual count also matches a same-named function of
another type; 29 of the 122 names are shared in that way.

**Reader functions called outside Store (45).**

| Family | Functions | In the plain store |
|---|---|---|
| Revision | `head`, `token`, `require_token`, `has_prepared_update` | The sequence in `meta`. There is no prepared state, so `has_prepared_update` is retired |
| Configuration and source policy | `collection_state`, `source_policy_digest`, `attach_source_policy`, `attach_source_policy_fence` | `meta`; the policy fence is kept |
| Read scope | `attach_read_scope_fence`, `require_path`, `restricts_paths`, `selected_scope_restricts_paths`, `scope_bound_cursor_identity`, `read_scope_fingerprint` | Kept as they are, as path filters over `files` |
| Files | `artifact`, `captured_input`, `artifact_path`, `lookup_paths`, `lookup_paths_filtered_checked`, `file_metadata`, `file_metadata_bytes`, `file_scalars`, `unit_owner` | `files` and `continuity` |
| Lexical | `lexical_search`, `lexical_search_paths`, `lexical_search_ranked_any`, `lexical_reference_page`, `verify_lexical`, `verify_lexical_integrity` | `fts` and `chunks`; the two verify functions move into Verify |
| Facts and relationships | `native_facts_page_with_options`, `native_relationships`, `analysis` | `facts` and the imported tables |
| Text | `native_line_projection`, `text_range_projection` | `file_text` and `chunks` |
| Vectors | `vector_binding`, `path_occurrences`, `occurrence_hit`, `graph_candidate_pointers`, `map_get` | `units`, `vector_rows` and the vector file |
| Changes | `recent_changes_since` | `changes` |
| Resources | `resource`, `resource_reference` | Rows in `meta`; pending semantic work becomes `files.vector_state` |
| Integrity mode and cleanup | `private_query_eligible`, `observe_private_integrity`, `reclamation_plan` | Retired |

**Writer functions called outside Store (19).** `initialize`,
`initialize_collection` and `install_initial_lexical_manifest` become
creation of an empty index. `prepare_artifacts_with_optional_projections`,
`prepare`, `mark_applying`, `commit`, `abort_prepared`, `pending`, `phase`,
`has_object_captures`, `prepare_repack`, `verify_current_lexical` and
`verify_pending_fts` are replaced by one batch transaction (section 5.4) and
one vector commit (section 5.6). `reclaim_objects` is retired with the object
files. `reconcile_operation_funding`,
`reconcile_operation_funding_preserving` and `release_orphan_grants` are
retired with the grants (D8). `head` is shared with the reader.

**File-store functions called outside Store (59).**

| Family | Functions | In the plain store |
|---|---|---|
| Stored objects | `check_retained_present`, `load_extraction_payload`, `load_retained`, `load_retained_bounded`, `load_retrieval_artifact`, `read_retained_source`, `verify_retained_source`, `retain_artifact_with_budget_owned`, `retain_payload_encoded_owned`, `retain_payload_with_budget_owned`, `load_legacy_saved_artifact` | Reads of `file_text`, `chunks` and `facts`; writes join the batch. Legacy saved evidence is retired (Q6) |
| Metadata and resources | `prepare_current_metadata_input`, `retain_current_metadata_encoded_owned`, `retain_current_metadata_scalars_prepared_owned`, `prepare_native_fact_projection`, `read_current_resource`, `retain_current_resource_owned` | Retired; their content is rows written by the batch |
| Open and lifecycle | `current_scope`, `current_read`, `current_build_read`, `current_write`, `current_database_capacity`, `runtime_bootstrap`, `write_runtime_bootstrap`, `finish_runtime_bootstrap`, `rebuild_obsolete_index`, `reclaim_current_objects`, `pin_service_objects` | Rewritten: open, create and old-format detection |
| Funding | `close_abandoned_operation_reservations`, `takeover_operation_funding_for_cleanup`, `takeover_operation_funding_for_retry`, `with_operation_funding` | Retired with the grants (D8), after checking what else uses them |
| Settings outside the index format | `compression_mode`, `configure_compression`, `extraction_policy_bytes`, `configure_extraction_policy_bytes`, `vector_policy_bytes`, `configure_vector_policy_bytes`, `shared_model_default`, `shared_model_runtime`, `shared_model_source`, `embedded_default_reference`, `embedded_model_binding`, `installed_model`, `validate_model_reference`, `with_embedded_service_models`, `with_service_models`, `service_incarnation`, `managed_collection_directory`, `invalidate_owned_state_before_mutation`, `with_split_access`, `directory`, `lock`, `new` | Kept |
| Shared names to check | `catalog`, `facts`, `facts_page`, `relationships`, `receipt` | Older-format reads or other types of the same name; P1.1 settles each |

## 10. Document history

| Date | Change |
|---|---|
| 24 November 2026 | First version: analysis, decisions D1 to D8, design sketch, open questions Q1 to Q8, implementation plan |
| 24 November 2026 | Step P0: baseline of the base pass measured and section 2.3's ranking corrected (section 2.6); the vectors measurement stopped on the user's word; targets made guides; seam inventory (section 9) and notes for P2.1 added |
| 24 November 2026 | The user's answers to Q1 to Q10 recorded. Q2, Q4 and Q10 differ from my recommendations: embedding on the graphics card, BM25 ranking and a separate embedding setting join the plan. Steps P2.5 to P2.9 added; targets, risks and section 5.13 revised |
