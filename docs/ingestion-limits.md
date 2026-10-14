# Ingestion limits

Limits are a persisted collection policy, separate from disk quota. Inspect them with `airk collections limits show`; replace them explicitly with `airk collections limits set limits.json`. The JSON object has these defaults:

```json
{
  "file_bytes": 16777216,
  "corpus_bytes": 67108864,
  "extracted_bytes": 134217728,
  "files": 10000,
  "discovery_entries": 100000,
  "read_bytes": 268435456
}
```

Every setting is required and unknown fields are rejected. The per-file ceiling is 16 MiB; configured corpus/extracted ceilings are 64/128 GiB, file count 1,000,000, discovery 10,000,000 and pass reads 128 GiB. These are validation ceilings, **not qualified workload sizes**. The default corpus/file-count limits remain conservative. [Scale qualification](scale-qualification.md) records the larger profiles actually measured.

Scanning streams one source/extraction at a time into content-addressed retained storage. Corpus totals include all source aliases. Oversized individual files are reported as skipped; exhausted corpus/discovery/read budgets fail the pass and keep the previous published generation. Unsupported candidates still consume discovery/read budgets. Changing policy does not publish or discard an existing generation. Coverage caches bind the policy and show capacity-blocked evidence after a fresh inventory.

The 128 MiB compact catalog, 64 MiB extracted-object serialization, 4 MiB extracted-document text, per-format expansion/fact limits, provider budgets, logical quotas, free-space admission and bounded ANN construction still apply. Native parser calls are not forcibly preempted. More permitted source bytes do not imply constant memory, unrestricted document expansion or complete indexing. OCR is explicit, retains its original source and still uses format/provider bounds; a subsequent changed-source or full reindex can replace that explicit extraction.

Use an isolated data root for workload qualification. Never raise limits as a substitute for inspecting skipped files, measured peak RSS, disk consumption, response latency, retrieval quality and full-disk recovery.
