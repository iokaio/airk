# Storage quotas

AIRK enforces reservations for application-written logical bytes in one canonical data root. The default policy allows 500 GiB for the root and 100 GiB for each collection. Temporary publication files, content-addressed source/extraction objects, models, retained generations and database journals all consume capacity. A generation with a live reader still counts; a quota never authorizes deleting it.

## Inspect and configure

~~~console
airk --data-dir /path/to/state --collection project-a collections quota status
airk --data-dir /path/to/state --collection project-a collections quota policy
airk --data-dir /path/to/state --collection project-a collections quota set quota-policy.json
~~~

`set` also accepts bounded `--json` input and an optional `--expected-json` policy, compared while holding the quota ledger lock to reject concurrent edits. The extension uses this comparison when editing limits.

`set` replaces the complete shared policy, including overrides, from a JSON file of at most 64 KiB. Values are integer bytes:

~~~json
{
  "root_bytes": 536870912000,
  "default_collection_bytes": 107374182400,
  "collections": { "project-a": 214748364800 }
}
~~~

Status distinguishes existing `used_bytes` from outstanding `reserved_bytes`, with corresponding collection totals. A policy cannot be lowered below existing bytes plus active reservations. Canonicalization makes alternate paths to the same root use the same ledger. Collection IDs retain their normal validation rules.

`physical_capacity` separately reports the operating system's current total available to the caller, filesystem free bytes and caller-available bytes. Windows uses `GetDiskFreeSpaceExW`; Unix uses `statvfs`. OS user quotas may make caller-available bytes smaller than filesystem free space. AIRK leaves 16 MiB of physical safety headroom and reports the remaining `available_growth_bytes`. An absent-directory probe resolves its nearest existing ancestor without creating the requested directory.

Private catalog admission also leaves 10% of observed growth available for filesystem activity between planning and the fresh pre-write capacity check. The catalog cap is the smaller of this physical allowance and the remaining logical reservation, with room retained for publishing the active pointer. This margin is not a reservation against other programs; subsequent allocation can still fail safely.

## Admission and publication

[Retention](retention.md) and quota are separate policies. Manual retention never silently prunes to admit a publication. Automatic mode first attempts eligible cleanup when shared or collection headroom is smaller than twice the last generation’s observed size plus 16 MiB. This estimate is a planning hint, not a prediction of arbitrary future edits. Per-write reservations still stop growth at the configured bound. A private SQLite catalog that reaches its page cap reports typed quota pressure. Automatic mode permits one typed quota-pressure cleanup/retry using a fresh reservation; protected evidence and other collections may still block admission. Status exposes available/required/protected bytes where known. Ordinary reads and status refreshes do not clean up, and cleanup failure after commitment does not invalidate the published revision.

The native ledger serializes admission across processes. A reservation retains its entire allowance until the last handle closes. Writers charge before increasing a blob or graph file, copying a model, writing Tantivy data, or publishing metadata. Atomic replacements charge their temporary payload too. Private SQLite catalogs reserve a fixed page budget before creation; page growth and journal-changing operations cannot escape that allocation. Their journal is disabled only because unpublished catalogs are disposable. The active revision pointer retains transactional publication and a reserved database/journal allowance.

Publication reserves available capacity under the configured root and collection policy; it has no separate fixed attempt or SQLite catalog file-size ceiling. After other generation components are written, the private fact catalog's page allowance uses the smaller of remaining reserved bytes and observed physical growth capacity, leaving room for the active pointer's main database, WAL and shared-memory files. SQLite's representable page-count limit and the separate [bounded ingestion/metadata contracts](ingestion-limits.md) still apply. This admission accounts for repeated citations, indexed keys and derived relationships without estimating database size from extracted JSON. Physical capacity is a point-in-time observation, so external writes can still cause a safe publication failure.

Reservations are conservative: bytes already written can also remain covered by an outstanding reservation, deleted build intermediates do not refund charges, and a concurrent publisher may be refused even if its final compact result would fit. Root services such as the scheduler reserve shared capacity without inventing a collection identity. Usage recording uses fail-fast admission and a short soft deadline; a busy or exhausted ledger drops optional accounting rather than turning retrieval into an indexing operation.

Physical admission checks the small ledger write, then each requested growth charge before the corresponding write. It does not require free space equal to the root quota or the publisher's entire logical reservation. SQLite/journal charges can intentionally include conservative upper bounds. These checks are point-in-time observations, not physical reservations: another process, another data root or an external program can consume space after a check. An OS write failure still aborts publication and preserves the previous active revision. Directories mounted from a different filesystem inside managed state are refused because a root-only free-space observation would not describe their writes; use a separate data root for another volume.

If allocation fails, the active generation is unchanged. New unreferenced blobs may remain until explicit pruning. Quota-owned private staging is removed on normal guard release or recovered after process death using operating-system leases. Recovery only removes known reservation-owned staging; retained generations and source workspaces are preserved. Retry after pruning obsolete state or deliberately changing the policy. Repeated blind retries do not increase available capacity.

Before an immutable generation leaves private staging, its reservation records the expected active head and prepared receipt in a bounded, atomically installed journal. If the process dies before committing the pointer, recovery authenticates that receipt, checks the unchanged head and obtains an exclusive generation lease before detaching the unpublished directory into owned staging for removal. A committed receipt preserves the generation. Corruption, a changed head or a live reader prevents deletion. Interrupted removal resumes within staging. Existing unjournaled directories are never adopted or deleted by this recovery path; this qualifies process termination, not power-loss durability.

## Exact boundary and limitations

This is an application logical-byte quota, not an operating-system filesystem quota or a machine-wide physical disk guarantee. It does not account for allocation-unit slack, filesystem metadata, compression, copy-on-write history or external programs writing files. Separate configured data roots have separate policies. Existing files count by length, so manually placing model assets inside a managed root reduces later admission capacity.

The small `.quota` control ledger is separately bounded rather than charged recursively: at most 1,024 reservations, 1 KiB per record, zero-length lease files and a 64 KiB policy. Lock files and other bounded administration metadata have their own limits. Inventory refuses linked/nonregular state and stops after one million entries. Normal ledger lock acquisition retries for at most two seconds. Best-effort usage admission checks a deadline between filesystem operations; an individual operating-system I/O call cannot be preempted. These are not hard wall-clock guarantees.

Managed current-version writers participate in this protocol; raw filesystem changes and older binaries cannot be made safe by an in-process reservation API. Protect the data directory accordingly. Tests cover shared root/collection admission, concurrent growth, sparse-write refusal, SQLite page exhaustion, root-only service allocations, nonblocking optional admission, native capacity observation, missing-directory read-only probes and child-process crash cleanup. Full-disk races, hostile filesystem, power-loss and platform qualification remain release work.

See [collection retention](collections.md#retention-and-removal) and [transactional relocation](storage-relocation.md).

In the sidebar, **Storage limits…** edits the selected collection and shared root limits in GiB, preserving other collection overrides. **Generations and cleanup…** selects manual/automatic retention and one through nine completed generations. **Reclaim eligible space…** previews a bounded retention pass. Lowering quotas may first run automatic retention for the selected collection; saving still refuses limits below live allocations. Other collections, readers or protected history can require a larger limit or separate cleanup. No disk space is preallocated. Existing explicitly saved quotas remain unchanged on upgrade.

Failed capacity admission retains the published index. The extension stops automatic retries for that collection until Resume or Index now is selected, rather than retrying on every file change. Physical disk-space checks remain independent of logical quotas.
