# Searchable revision retention

Retention manages derived AIRK state within one collection. It never removes source files. The policy keeps active evidence plus one committed predecessor; live reader leases may protect more. Historical read packages and named checkpoints are not supported by this policy.

## Inspect before changing policy

```powershell
airk collections retention show
airk collections retention preview
airk collections retention run
```

All three commands above are observational. `run` without `--apply` is a preview, not cleanup or recovery. Global `--root`, `--data-dir` and `--collection` choose the scope as on other commands.

Status reports effective policy and its digest/origin, active token, logical/protected bytes, bounded protected revision lists, pending intents/bytes, last maintenance receipt and quota pressure where known. Unknown byte totals are null, not zero. Check list truncation flags. An absent maintenance receipt means the last maintenance outcome is unknown, not that cleanup succeeded.

Verified older generations appear separately as `legacy_protected` in status and `legacy_candidates` in previews, using their original revision digest rather than an invented v2 token. The first v2 publication conservatively protects all legacy generations; later v2 ancestry can make them eligible, while external reader leases still protect them. Explicit active-only legacy pruning can retire them earlier. Only recognized legacy manifests with verifiable inventory/identity qualify; older formats without an inventory fail closed and are not claimed as fully supported mixed-format cleanup.

Policy origins distinguish `existing_manual`, `new_collection_default` and `explicit`. New collections start with automatic retention, keeping active evidence plus one predecessor. Upgraded collections without an explicit policy remain manual until an operator changes the policy. Inspect the effective mode when creating or opening a collection; automatic indexing and automatic retention are separate settings.

## Policy and explicit maintenance

The supported policy is:

```json
{"version":1,"mode":"manual","searchable_predecessors":1}
```

Choose `automatic` to enable bounded postpublication maintenance. Version 1 supports exactly one searchable predecessor. Policy files/inline input are bounded to 64 KiB and reject unknown fields or unsupported values.

```powershell
airk collections retention preview --file retention-policy.json
airk collections retention set retention-policy.json
airk collections retention preview --json '{"version":1,"mode":"automatic","searchable_predecessors":1}'
airk collections retention set --json '{"version":1,"mode":"automatic","searchable_predecessors":1}'
airk collections retention run --apply
```

Previewing a proposed policy does not persist it. Setting policy returns a digest-bound receipt and does not immediately delete generations. `run --apply` performs one bounded policy maintenance pass, including in manual mode. It rechecks current protection before removal; a preview is not a promise that every listed candidate can still be reclaimed. A busy reader can leave pending work. Inspect the result receipt/state and run another explicit pass when appropriate.

Default passes allow 256 MiB of removal and a cooperative 1000 ms time budget. A generation larger than that removal allowance can remain pending indefinitely; repeated default passes do not enlarge the allowance. Explicitly raise the pass budget when needed, without changing retention policy or relaxing reader protection:

```powershell
airk collections retention run --max-removal-bytes 1073741824 --max-duration-ms 10000 --max-metadata-bytes 33554432
airk collections retention run --apply --max-removal-bytes 1073741824 --max-duration-ms 10000 --max-metadata-bytes 33554432
```

The same supplied budgets apply to preview and apply. Removal allowance is 1 byte through 64 GiB; elapsed-time allowance is 1 through 60000 ms, checked cooperatively rather than as an OS deadline. `--max-metadata-bytes` permits 16 KiB through 64 MiB, default 16 MiB. Increase it when verifying surviving inventories leaves GC pending despite sufficient removal/time allowance. Other scan/candidate bounds remain in force. Extension maintenance uses default budgets; use these CLI controls for a larger bounded pass when large components or surviving metadata leave it pending.

Automatic mode performs bounded maintenance after successful publications. A running native scheduled worker can continue pending automatic maintenance while idle, one collection per pass; its `maintained` outcome is separate from indexing success/failure. Invalid pending-maintenance metadata yields a bounded `maintenance_failed` outcome rather than stopping the watch loop or marking an indexing job failed. No read or footer update drives that cleanup. A cleanup failure does not undo or turn a publication into a failure; inspect retention status separately. On typed quota pressure, automatic publication may clean eligible state and retry once with fresh reservations. Manual mode never silently prunes under quota pressure. Corruption, parser failures and arbitrary I/O failures do not authorize cleanup retries or automatic external-provider reruns.

The existing `collections prune [--apply]` command remains a separate explicit manual trim. It protects the active revision and readers, but can remove the policy's predecessor. Use retention maintenance when you want the configured keep set preserved. Neither operation removes another collection's state under this collection's policy.

## Extension controls

**Show Retention Status** displays collection/root/storage, effective mode/origin, protected state, pending work and errors. **Choose Retention Policy** shows a prospective preview before confirmation and saves precisely that policy. **Run Retention Maintenance** previews before confirmation and performs one bounded pass. The older **Trim Obsolete Index Generations** remains the separate manual prune operation.

The footer uses cached status refreshed after indexing or explicit status/control actions. Repainting it never performs cleanup. Retention controls do not overwrite coding-agent configuration, tool approvals or workspace source files. A failed status refresh does not turn successful indexing into failure. Workspace trust remains required to launch the native helper.

## Bounds, recovery and privacy

Active/searchable dependencies, protected readers, pending removal files, staging and metadata all count against quota. Retention targets are not a physical disk-space guarantee; protected evidence may keep usage above target. Status includes required/available bytes where measured, and bounded error details for blocked or failed maintenance.

Interrupted retirement is recoverable only through validated intents bound to the collection's incarnation/revision. Unknown directories, links and malformed or foreign markers are not general cleanup targets. Object GC stops if surviving dependency roots cannot be verified.

Deletion reclaims logical AIRK files, not SSD blocks, backups, old relocated roots or external copies. A source hash mentioned in lineage does not keep an old source object alive, but paths/hashes can remain as change metadata after old content is removed. Active indexed secrets remain until the sources change and a successful publication replaces that evidence. No secure-erasure or complete purge claim follows from retention success.
