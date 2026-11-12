# Resumable discovery

File lists, symbols, imports, code facts, document facts and outlines support an optional `cursor`. Save a response's exact publication token and pass its cursor to the same tool with the same filters. Continue until `truncated` is false. Cursors use deterministic keyset ordering and bind the collection, exact revision, filters and representation; changing any of these returns an explicit error. A cursor is a consistency selector, not an authorization token.

Existing representations and arguments remain supported. Symbols/imports/outlines
default to minimal navigation; code/document discovery keeps its applied-profile
legacy default. Explicit [Lean/Rich reply detail](reply-detail.md) selects the
available projection independently of retention. Limits remain 1–100 records and
1–65536 bytes. `record_bytes` counts complete serialized records and new bindings;
array punctuation, response metadata and cursor framing are additional. `limit`
and `max_bytes` can change between pages without changing the selected evidence.

When a record does not fit after earlier records, the cursor continues after the last returned record. If the next record cannot fit an empty page, `required_record_bytes` reports its serialized size. Increase `max_bytes` and repeat the same input cursor; an initial blocked page has no cursor. No record is silently skipped. A rich record larger than the maximum response budget remains unavailable through that representation; compact outlines and bounded source reads provide alternative evidence.

```json
{"prefix":"crates/","limit":100,"max_bytes":16384}
```

Call `airk_list_files` again with the returned `cursor` and unchanged `prefix`. Additional roots retain their published `@alias/` path names and participate in the same ordering.

## Compact outlines

CLI/MCP results normalize discovery records under `items`; internal producer
`facts`/`outline` members are not extra public result arrays. Minimal
symbols/imports/outlines retain names, kinds, line ranges, available signatures,
original document locations and applicable truncation/table observations. Their
page-local `files`/`provenance` tables bind hashes and producer/coverage metadata.
Consumers must resolve those bindings; never treat a numeric file index as a
path or invent omitted symbol IDs. Code/document facts retain their established
compact projection, including available citations, IDs, heading context and
formula/cache qualifiers. `compact:false` or `reply_detail:rich` requests retained
rich details. Durable-key lookups retain their existing rich representation;
imported facts without such a key remain queryable by exact name and path.

```json
{"path":"crates/example/src/lib.rs","compact":true,"limit":100,"max_bytes":32768}
```

Conditional outline validators bind the representation, selection, page and response budget. A validator for one page or rich/compact representation cannot suppress a different page or representation. Compact citations retain source/document locations and do not establish semantic resolution.

## Publication and retention

Current file/fact discovery selects one verified publication per request. A new publication expires its earlier current cursor; restart discovery to enumerate the new revision. This deliberately prevents combining pages from different revisions, including concurrent publications.

Exact `revision` options also require the current head. Superseded tokens fail; no historical cursor continuation or fallback to live source is provided. Old 1.1 collections are isolated in their previous namespace.

Direct CLI discovery commands accept `--cursor`; `airkctl outline` also accepts `--compact`. The same options are available through `airkctl tools call` and the interactive tools console. Durable-key fact lookup is exclusive of cursor/textual filters; it is a lookup, not an inventory traversal.

CLI read aliases also accept `--reply-detail lean|rich`. A projection change
invalidates a continuation; equivalent legacy flags preserve the same effective
representation. Workbench forms/slash commands expose the same choice. Matching
old/new controls are valid and contradictory flags reject.

## Implementation boundaries

Pure cursor/compact-record contracts live in `airk-core::discovery`. Kernel request routing holds the current gate while `airk-store::current` authenticates postings, artifact/continuity metadata and retained evidence. Each fact page limits candidates and read bytes independently of response bytes. Empty pages may still carry a progress cursor. Imported assertions require source/build/provenance binding.

Tests cover alias paths, complete keyset enumeration, compact outlines, record budgets, same-name/same-line ties, tampering, filter changes, current-token expiry and conditional validators. These establish bounded response behavior, not constant-cost corpus scans. See [current runtime bounds](current-index-runtime.md).
