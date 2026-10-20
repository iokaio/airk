# Resumable discovery

File lists, symbols, imports, code facts, document facts and outlines support an optional `cursor`. Save a response's exact publication token and pass its cursor to the same tool with the same filters. Continue until `truncated` is false. Cursors use deterministic keyset ordering and bind the collection, exact revision, filters and representation; changing any of these returns an explicit error. A cursor is a consistency selector, not an authorization token.

The default rich representation and existing arguments remain supported. Limits remain 1–100 records and 1–65536 bytes. `record_bytes` counts the complete serialized records, including their citations and metadata. Array punctuation, response metadata and cursor framing are additional. `limit` and `max_bytes` can change between pages without changing the selected evidence.

When a record does not fit after earlier records, the cursor continues after the last returned record. If the next record cannot fit an empty page, `required_record_bytes` reports its serialized size. Increase `max_bytes` and repeat the same input cursor; an initial blocked page has no cursor. No record is silently skipped. A rich record larger than the maximum response budget remains unavailable through that representation; compact outlines and bounded source reads provide alternative evidence.

```json
{"prefix":"crates/","limit":100,"max_bytes":16384}
```

Call `airk_list_files` again with the returned `cursor` and unchanged `prefix`. Additional roots retain their published `@alias/` path names and participate in the same ordering.

## Compact outlines

`airk_outline` accepts `compact: true`. Compact records appear in `outline`, while the default rich records remain in `facts`. Each compact record contains its name, kind, syntax kind, original citation, available byte span and durable continuity key. It omits signatures, documentation, producer detail and document metadata. Use the durable key with `airk_code_facts` or `airk_document_facts` for rich details; imported facts without a durable key remain queryable using their exact name and path.

```json
{"path":"crates/example/src/lib.rs","compact":true,"limit":100,"max_bytes":32768}
```

Conditional outline validators bind the representation, selection, page and response budget. A validator for one page or rich/compact representation cannot suppress a different page or representation. Compact citations retain source/document locations and do not establish semantic resolution.

## Publication and retention

Current file/fact discovery selects one verified publication per request. A new publication expires its earlier current cursor; restart discovery to enumerate the new revision. This deliberately prevents combining pages from different revisions, including concurrent publications.

Exact `revision` options also require the current head. Superseded tokens fail; no historical cursor continuation or fallback to live source is provided. Old 1.1 collections are isolated in their previous namespace.

Direct CLI discovery commands accept `--cursor`; `airk outline` also accepts `--compact`. The same options are available through `airk tools call` and the interactive tools console. Durable-key fact lookup is exclusive of cursor/textual filters; it is a lookup, not an inventory traversal.

## Implementation boundaries

Pure cursor/compact-record contracts live in `airk-core::discovery`. Kernel request routing holds the current gate while `airk-store::current` authenticates postings, artifact/continuity metadata and retained evidence. Each fact page limits candidates and read bytes independently of response bytes. Empty pages may still carry a progress cursor. Imported assertions require source/build/provenance binding.

Tests cover alias paths, complete keyset enumeration, compact outlines, record budgets, same-name/same-line ties, tampering, filter changes, current-token expiry and conditional validators. These establish bounded response behavior, not constant-cost corpus scans. See [current runtime bounds](current-index-runtime.md).
