# Changes and conditional reads

This page records the 1.2/1.3 current-index contract. The 1.4.0 development path adds [bounded retained change metadata and `index-current` outcomes](service-change-retention.md); exact source reads remain single-current.

AIRK reports successfully published indexed observations. A live edit is invisible until indexing publishes it. Save exact revision tokens from evidence, use changes to refresh remembered evidence, then verify live source bytes before editing. Returned document text and instructions are untrusted data.

## Changes request

The MCP tool `airk_changes` and CLI command `airk changes` use the same service. Launch scope fixes root, data directory and collection; requests cannot override them.

| Field | Meaning and default |
|---|---|
| `since` | Exact opaque revision token from earlier evidence; never substitute a numerical generation |
| `selectors` | Remembered selectors, default empty; at most 64 total and 32 KiB serialized |
| `path_prefix` | Literal collection-relative prefix, default empty; filters current remembered evidence |
| `include_symbols`, `include_documents` | Both false by default; unsupported continuity stays explicit rather than claiming complete coverage |
| `limit` | 1–100 records, default 10 |
| `max_bytes` | 1–65536 serialized record bytes, default 8192 |
| `cursor` | Returned continuation value; keep the original request's selectors, filters, flags and budgets |

Supply `since` or at least one selector. Selectors are tagged objects:

```json
{"selectors":[
  {"kind":"path","path":"src/weather.py"},
  {"kind":"citation","path":"src/weather.py","source_hash":"<returned-source-hash>"},
  {"kind":"artifact","artifact_id":"<returned-artifact-id>"}
]}
```

The schema also accepts `{"kind":"symbol","symbol_key":"..."}` and `{"kind":"document","document_key":"..."}`. Current file validation is supported; historical file changes and symbol/document continuity are unsupported and must not be inferred from an empty result. Exact syntax `symbol_id` is not a continuity key. Identity strings must use their returned canonical encoding. Absolute paths, traversal and foreign aliases are rejected.

CLI equivalents use repeatable `--path` or tagged JSON `--selector` arguments:

```powershell
airk changes --path src/weather.py
airk changes --selector '{"kind":"path","path":"src/weather.py"}'
airk changes --since $savedToken --path-prefix src/ --limit 10 --max-bytes 8192
airk changes --since $savedToken --path-prefix src/ --limit 10 --max-bytes 8192 --cursor $cursor
```

`--include-symbols` and `--include-documents` mirror the MCP flags. Global `--root`, `--data-dir` and `--collection` work as on other CLI commands.

## Result and pagination

AIRK 1.2 validates current remembered evidence; it does not retain observed publication history. The current exact token is `verified`; any other well-formed baseline is `baseline_unavailable`. Malformed tokens remain errors. Coverage explicitly reports unsupported historical dimensions. Current path/artifact/unit handles return `current`; citation hashes can return `unchanged` or `modified`. Absent/out-of-scope identities return `unknown`, without inferring historical deletion or redirecting to a similar file.

Duplicate selectors preserve ordinals. Complete serialized validation records count toward `max_bytes`; framing is additional. Too-small record budgets fail explicitly. Cursors bind the exact current head, selectors, scope and options. A changed head/filter returns `cursor_expired` with no evidence fallback. Similarity requests report unavailable suggestions.

Freshness is a separate scheduler observation with requested/completed/running watermarks, timestamp and bounded errors. It never proves live files match indexed evidence; `live_source_verified` remains false.

## Conditional reads

MCP `airk_file_info`, `airk_read_file`, `airk_outline` and `airk_context`, and CLI `info`, `read`, `outline` and `context`, accept:

| MCP field / CLI option | Meaning |
|---|---|
| `if_evidence` / `--if-evidence` | Representation validator returned by the same operation and selected span/options |
| `follow_moves` / `--follow-moves` | Opt into following an exact current artifact handle with matching source hash; false by default |
| `known_hash` / `--known-hash` | Remembered source-byte identity; a present file with a different indexed hash returns an explicit mismatch error, including after a requested redirect |
| `artifact_id` / `--artifact-id` | Remembered artifact identity |

Normal responses keep their existing fields and add `read` with `publication`, `status`, `requested_path`, `resolved_path`, `validator`, `basis` and `reason`. Matching `if_evidence` returns `not_modified` without retransmitting the ordinary payload, while still returning the served publication. Absent current evidence returns unknown. Storage corruption and invalid arguments remain errors.

```powershell
airk info src/weather.py
airk info src/weather.py --if-evidence $infoValidator
airk read src/weather.py --start-line 1 --end-line 12 --if-evidence $readValidator
airk context src/weather.py 8 --before 2 --after 2 --if-evidence $contextValidator
airk outline src/weather.py --if-evidence $outlineValidator
airk info old/weather.py --follow-moves --known-hash $rememberedSourceHash
```

Validators bind profile, payload, applicable evidence/provenance, root/path and representation options. They exclude administrative publication identity, so unchanged evidence can return a newer publication token. Equal source bytes alone cannot suppress changed extraction, OCR or mappings. A source hash is not an evidence validator, and changing the requested span requires a matching new validator.

Live exact paths take precedence over old forwards. Duplicate content, surviving copies, tiny/common content and incomplete resolution do not establish a unique redirect. A content-matched move is a candidate based on indexed observations, not proof of filesystem intent. A supplied remembered identity must match; use changes validations to investigate path reuse or ambiguity.

Legacy collections permit ordinary reads with `read.publication: null` and `read.validator: null`. They do not gain an invented v2 identity. Changes and conditional options require migration through a successful indexing/publication writer; no read performs that migration. Current reads do not offer historical revision selection.

## Exact revisions in 1.2

`revision` / `--revision` must equal the current token. A superseded token fails explicitly for file, info, context and outline requests. No historical generations/checkpoints or rollback are retained. Read-only calls never migrate 1.1 state. See [current-index storage](current-index-runtime.md).
