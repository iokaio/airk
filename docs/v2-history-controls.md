# Local history and checkpoint controls

> Historical 1.1 implementation record. AIRK 1.2 production uses the [single-current-index contract](current-index-runtime.md). Retained historical reads, checkpoint/retention administration and receipt-backed generations below are not current capabilities. Old primitive storage tests remain separate from production routing.


Exact historical evidence is available through `--revision TOKEN` on CLI file-info/read-file/context/outline requests and the corresponding MCP tools' `revision` field. Responses identify the served publication, current publication, historical status and original source root. Only retained committed bytes are read; a missing revision returns `history_unavailable`, and corrupt retained bytes fail verification. Historical search and move-following are unsupported. Representation validators continue to bind the selected evidence and response shape.

`airk_history` and `collections history available` list bounded receipt identities (at most 64 results/64 KiB), with cursors bound to the current head and request budget. Supplying an exact revision verifies its retained representation. Listing alone does not verify every historical component. `collections history show|set|preview|run` administers the separate opt-in policy; `run` requires `--apply`. Policy defaults keep up to three recent and twelve total historical revisions within 24 hourly buckets, 256 MiB and ten percent of the collection quota. Protected revisions can exceed targets and remain visible as pending work. Checkpoints are limited to four bounded labels and admitted against their retained dependency footprint.

Root pointer version 5 persists policy/checkpoint administration; explicit readers retain versions 3 and 4. Configuration does not reindex or publish source changes. Enabling history while an ordinary retirement intent is unfinished requires completing maintenance first. Lineage clearing publishes new completeness floors while preserving immutable prior publication bytes; protected copies may therefore retain prior lineage.

AIRK's extension controls show the selected collection, workspace root and storage directory with each result. **Show History Status** displays the persisted history policy, its bounded selection preview and retention protection, including unknown byte totals. History is opt-in and separate from searchable-generation retention and indexing mode.

**Choose History Policy** preserves the current policy's limits while enabling or disabling automatic history selection. Before saving, it displays the proposed policy and clearly labels the selection preview as the current policy's plan. Saving does not immediately remove existing history or checkpoints. **Run History Maintenance** previews the current plan, asks for confirmation, and runs one bounded pass. Review the result's pending and error fields; a completed publication does not imply completed cleanup.

**List Retained Publication Identities** pages through bounded receipt listings. A receipt identifies a committed publication; it does not alone verify the stored representation or provide historical search. **Create Checkpoint** chooses from the first bounded page and verifies that exact revision before reviewing a label and creating a pin. Checkpoints refer only to committed evidence, never unsaved or newly read live source files. **Remove Checkpoint** reviews the exact label/revision binding and releases that pin. It does not promise immediate evidence deletion.

**Preview Logical History Purge** accepts a JSON request containing explicit revision tokens, optional checkpoint removals with their expected revision tokens, and an explicit `clear_lineage` boolean. It displays the protected targets and exact plan before confirmation. Apply sends the same captured request and plan digest; stale plans fail rather than silently choosing a new target. Results can remain protected or pending, and errors remain visible.

```json
{"revisions":[],"checkpoints":[],"clear_lineage":false}
```

Purging is logical removal of eligible managed evidence, not secure erasure. Active and predecessor revisions, reader leases, checkpoints and in-progress conversions can keep evidence available. Removing a pin does not override other protection. Clearing lineage is a separate explicit publication action performed by the kernel; inspect its returned outcome. Original source files are untouched. Backups, previously relocated storage roots, external copies, filesystem snapshots and SSD remnants are outside this operation. Manage those separately if privacy requirements call for their removal.

A historical package can become readable before conversion finishes retiring its full searchable representation. Purge previews report conversion protection until that work completes. Continue bounded history maintenance and preview again before applying a purge; historical readability alone does not establish conversion completion.

The controls do not modify coding-agent settings, instructions, trust policy or approval configuration. They invoke the native helper for the selected collection and show its bounded results.
