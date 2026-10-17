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

Scanning streams one source/extraction at a time into content-addressed storage. Eligible corpus totals include all source aliases and count successfully extracted files, including reused files. Unsupported candidates still consume discovery/read budgets. Oversized or unextractable individual files are reported as skipped. Exhausted collection/discovery/read budgets fail the whole pass and preserve the previous published generation. Collection-limit errors include measured totals, configured limits and the file that crossed the limit.

The compact catalog remains bounded to 512 MiB; extracted-object JSON to 256 MiB; retained sources to 256 MiB; facts and metadata retain independent bounds. Storage quotas and physical free-space admission remain separate and authoritative. A 2 GiB source allowance does **not** imply a 2 GiB index: text, postings, facts, original sources, retained generations and temporary publication files all consume storage. The default 4 GiB collection disk quota may be reached well before the source allowance. Inspect [storage quotas](storage-quotas.md) before raising them; automatic memory sizing never raises disk quotas.

Use an isolated data root for workload qualification. Inspect skipped files, measured peak RSS, disk use, retrieval quality and full-disk recovery before treating a larger permitted workload as supported performance.

The 512 MiB inventory ceiling includes full change-tracking metadata. Code-heavy repositories can exceed the former 128 MiB ceiling even with less than 100 MiB of source. Writers, retrieval readers and retained-catalog readers share this ceiling; AIRK does not discard continuity records to fit it. Decoded metadata, serialization and validation can consume substantially more memory than the encoded inventory. Inventory overflow still fails publication and retains the previous generation.
