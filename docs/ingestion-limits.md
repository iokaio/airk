# Ingestion limits

Collections without a saved policy use machine-aware sizing. AIRK detects physical RAM on Windows and macOS, and physical RAM constrained by readable standard cgroup memory limits on Linux. The eligible-source budget is **one eighth of that capacity, capped at 2 GiB**. With unavailable or invalid memory information it uses **512 MiB**. It uses total capacity, not fluctuating free RAM; it does not reserve memory or promise that indexing fits in that much RAM. Unusual cgroup mounts and other process-specific memory restrictions may not be detected.

| Limit | Automatic policy on a machine with at least 16 GiB RAM |
|---|---:|
| Raw source file | 256 MiB |
| Eligible source per collection | 2 GiB |
| Extracted text per collection | 4 GiB |
| Successfully extracted files | 100,000 |
| Visited non-ignored entries | 1,000,000 |
| Raw reads per index pass | 8 GiB |
| UTF-8 or extracted text per file | 32 MiB |

For 8 GiB RAM the source/extracted/read budgets are 1/2/4 GiB; for 4 GiB they are 512 MiB/1/2 GiB. The per-file admission limit never exceeds the source budget. These are admission limits, **not qualified workload sizes**. [Scale qualification](scale-qualification.md) records measurements.

## Index detail

Each collection selects how much derived evidence to retain. **Rich Structure (`balanced`) is the default for new collections.** Existing saved extraction policies retain their selected level until explicitly changed.

| Level | Retained native evidence | Deliberate omissions |
|---|---|---|
| Search Only (`essential`) | Searchable text, source paths/hashes, exact original locations | Structured facts, relationships and symbol/document-unit continuity |
| Rich Structure (`balanced`) | Essential plus adapter-supported declarations, signatures, imports, structural/configuration/document facts and declaration continuity | Individual call occurrences and document-unit continuity |
| Agent Lean (`agent_lean`) | Searchable text, exact locations, declarations/signatures/imports, selected local structure and document facts | Generic call occurrences, continuity fingerprints and broad document inventories |
| Detailed | The full existing adapter-supported fact/relationship and continuity output | Adapter limitations still apply; no claim of complete resolution |

In the sidebar, choose **Index detail → Evidence to retain** on the collection card. Review the explanation and choose **Save and index** or **Save for next index**. The card distinguishes the configured level from the published level while a change is pending. Changing detail preserves adapter-selection overrides and affects only that collection. The CLI equivalent is:

```console
airk collections extraction show
airk collections extraction detail agent_lean
airk collections extraction detail essential
airk collections extraction detail balanced
airk collections extraction detail detailed
```

Detail is included in extraction identity. An ordinary index pass re-extracts files whose level changed and publishes current evidence through the recovery journal. Progressive scheduled batches can become readable before the complete scan finishes; the panel reports the configured and applied levels separately. A failure retains previously committed evidence and reports the incomplete work. Increasing detail requires re-extraction; historical generations and history-retention settings are unavailable in 1.2. Explicit provider imports remain separately bound evidence. If retained OCR output prevents applying a changed level, run a full reindex to apply native extraction, then explicitly rerun OCR for those files. The full reindex may omit images with no native text; the original sources remain untouched. No OCR subprocess is launched implicitly.

All levels keep text and citation accuracy, including original document locations. Qualified common grammar adapters apply detail during their syntax walk: Search Only avoids rich code metadata and fingerprints, while Rich Structure avoids generic call evidence but keeps recognized framework cues. Search Only Markdown/plain-text extraction collects heading boundaries without classifying dates, quantities or requirements. PDF/PPTX use the same selected prose path after native text extraction; DOCX/XLSX and other unqualified producers retain full extraction followed by selection. [Extraction policy](extraction-policy.md#detail-aware-extraction) lists the qualified scope. Parsers still run within their existing source/node budgets; reduced detail does not promise proportional time savings.

Written definition and heading spans are captured independently of optional facts. They preserve vector-planning boundaries, written context and native/mapped locations even when Search Only retains no facts. File, parsing, memory-work and disk quotas remain independent. Fact and relationship responses include an `indexing` object naming the published level and deliberate omissions. An empty result at a reduced level must not be interpreted as proof that the source has no such facts.

### Execution and compression

`airk execution` reports CPU/memory admission and the storage-root slot policy. `AIRK_EXECUTION_MODE` selects `automatic`, `interactive` or `throughput`; existing worker settings remain bounded ceilings. These estimates do not enforce an RSS limit. `airk index --scheduled --progress` emits committed progress as JSON on stderr, preserving final stdout JSON. Discovery totals remain unknown until a complete inventory.

Optional retained-object compression is off by default. `airk compression set automatic` affects future immutable writes; `airk compression repack --max-files 32` rewrites a bounded batch through the recovery journal. Continue with the returned `next_after` using `--after`. See [the release notes](release-1.2.0.md#optional-lossless-retained-object-compression) for byte limits, exact evidence guarantees and accounting scope.

## Inspect and configure

In the extension, use **AIRK: Show Ingestion Limits** to see automatic versus configured policy, detected memory, the storage folder, disk quota and cached coverage measurements. **AIRK: Show Collection Status and Coverage** updates measurements; incomplete inventories are labeled partial. Coverage includes successfully extracted source/extracted byte totals and up to twenty largest eligible sources. Inspecting limits does not change storage or require indexing to finish.

Use **AIRK: Configure Ingestion Limits…** to restore automatic sizing or validate, review and import a JSON policy. The CLI equivalents are:

```console
airk collections limits describe
airk collections limits show
airk collections limits validate limits.json
airk collections limits set limits.json
airk collections limits reset
```

When indexing reaches a collection source, extracted-text, file-count, discovery or read budget, the extension offers **Review larger limit**. Enter a larger allowance within the displayed hard ceiling, then choose **Save limit and retry**. The confirmation explains: "Larger limits can increase indexing time, memory use and disk usage. Disk quotas and per-file extraction limits still apply; the previous index is retained if indexing fails." Values above normal machine sizing are labeled explicitly.

Approval saves only the selected budget for that collection and retries once. Dismissal leaves the policy unchanged; a second failure stops without another automatic prompt/retry loop. If another client changes the limits while approval is pending, saving fails and requires a fresh review. No override is offered for per-file extraction ceilings, disk quota failures or unrelated errors. CLI and unattended workers continue to enforce configured limits and never grant themselves an increase.

`show` returns only the effective policy; `describe` also returns the detected memory, automatic recommendation and whether a saved policy is active. `reset` removes the current collection's explicit override. Existing saved limits are never silently increased or replaced; an old 64 MiB policy stays in force until explicitly changed or reset. All changes apply on the next index; they do not publish or discard evidence.

An explicit policy equivalent to the largest automatic defaults is:

```json
{
  "file_bytes": 268435456,
  "corpus_bytes": 2147483648,
  "extracted_bytes": 4294967296,
  "files": 100000,
  "discovery_entries": 1000000,
  "read_bytes": 8589934592
}
```

All six settings are required; unknown fields are rejected. Explicit validation ceilings are 256 MiB/file, 64 GiB eligible source, 128 GiB extracted text, 1,000,000 files, 10,000,000 discovery entries and 128 GiB pass reads. The **2 GiB cap applies to automatic defaults**, not a deliberate larger policy. Memory detection does not override explicit settings.

## Extraction, storage and failure behavior

Raw source size and extraction cost have separate bounds. UTF-8 sources above 4 MiB and up to 32 MiB retain their complete text and original line locations, with `partial-text-only` status. They do not run syntax, structured-data, embedded-language or prose-fact extraction and do not claim symbols, relationships or continuity. Smaller files keep their normal adapters. Binary/non-UTF-8 content does not receive this fallback. Files beyond the text limit are explicitly skipped, never silently truncated.

PDF/Office sources can be admitted up to the raw file limit, but extracted text and individual OOXML parts remain bounded to 32 MiB. The existing page, slide, sheet, cell, aggregate XLSX XML, fact and nesting limits still apply. Explicit OCR keeps its own smaller provider/page/text budgets. Native PDF/Office parser calls are not forcibly preempted; byte ceilings are not hard CPU-time or RSS limits. Process-isolated native parsing remains a separate engineering task.

Scanning defaults to serial extraction and can admit a bounded batch of pure extraction jobs as described below. Protected source reads, hashing, profile/reuse decisions, binding and retention stay on the coordinator. Eligible corpus totals include all source aliases and count successfully extracted files, including reused files. Unsupported candidates still consume discovery/read budgets. Oversized or unextractable individual files are reported as skipped. Exhausted collection/discovery/read budgets fail the whole pass and preserve the previous published generation. Collection-limit errors include measured totals, configured limits and the file that crossed the limit.

The compact catalog remains bounded to 512 MiB; extracted-object JSON to 256 MiB; retained sources to 256 MiB; facts and metadata retain independent bounds. Storage quotas and physical free-space admission remain separate and authoritative. A 2 GiB source allowance does **not** imply a 2 GiB index: text, postings, facts, original sources, current retained objects and temporary publication files all consume storage. New collections default to a 100 GiB collection quota within a 500 GiB data-root quota. Existing explicit quotas remain in effect. Inspect [storage quotas](storage-quotas.md) before raising them; automatic memory sizing never raises disk quotas.

Use an isolated data root for workload qualification. Inspect skipped files, measured peak RSS, disk use, retrieval quality and full-disk recovery before treating a larger permitted workload as supported performance.

The 512 MiB inventory ceiling includes full change-tracking metadata. Code-heavy repositories can exceed the former 128 MiB ceiling even with less than 100 MiB of source. Writers, retrieval readers and retained-catalog readers share this ceiling; AIRK does not discard continuity records to fit it. Decoded metadata, serialization and validation can consume substantially more memory than the encoded inventory. Inventory overflow still fails publication and retains the previous generation.

## Bounded extraction workers

`AIRK_EXTRACT_WORKERS` requests 1–4 extraction workers; the default remains **1** pending workload qualification. Invalid values fail explicitly. Worker count is an execution setting and does not change extraction identity or evidence. PDF, DOCX, XLSX, PPTX and sources above the 4 MiB structured-input ceiling always run serially after pending work drains. Reuse and storage callbacks also run on the coordinator.

Worker admission uses a separate memory heuristic: one eighth of detected capacity, capped at 4 GiB, or 512 MiB when unknown. It allows 256 MiB for the coordinator's protected read and 580 MiB per admitted worker: two 256 MiB allowances for serialized result/validation work, 4 MiB of source and 64 MiB of parser allowance. The admitted count is at least one and never exceeds the request or four. With a request of four, 8 GiB RAM admits one worker, 16 GiB admits three, and 32 GiB or more admits four. This heuristic does not reserve memory and is **not an RSS cap**; decoded payloads and native allocations can exceed their encoded sizes. The collection-source sizing cap remains 2 GiB independently.

At most the admitted worker count of jobs can be queued, running or waiting for delivery together, with at most 4 MiB of source per worker. Results must pass the existing 256 MiB serialized-payload limit. The coordinator may additionally hold one source while draining a batch. Results, skips and retention callbacks follow the scan's captured discovery order, regardless of worker completion order; this is not a guarantee of filename sorting or identical discovery order across runs. An earlier fatal extraction/retention error takes precedence over a later coordinator error. Shutdown closes channels, discards queued work and joins in-progress workers; it does not forcibly preempt a parser.

Reports distinguish requested/admitted/spawned workers, parallel/serial jobs, in-flight source high water, the encoded-result allowance, coordinator source high water and time waiting for results. `extraction_work_us` sums wall time spent inside pure extraction jobs, excluding coordinator reads, hashing, reuse, binding and retention; it is not CPU time and can exceed elapsed indexing time when jobs overlap. The [optimization qualification ledger](indexingenhancements.md) records measured throughput and memory separately from these admission bounds.
