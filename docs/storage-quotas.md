# Storage quotas

AIRK enforces reservations for application-written logical bytes in one canonical data root. The default policy allows 500 GiB for the root and 256 GiB for each collection. Temporary publication files, content-addressed source/extraction objects, models, current databases and recovery journals all consume capacity. Current and pending ownership remains protected; a quota never authorizes deleting referenced evidence.

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
  "default_collection_bytes": 274877906944,
  "collections": { "project-a": 214748364800 }
}
~~~

Status distinguishes existing `used_bytes` from outstanding `reserved_bytes`, with corresponding collection totals. A policy cannot be lowered below existing bytes plus active reservations. Canonicalization makes alternate paths to the same root use the same ledger. Collection IDs retain their normal validation rules.

`physical_capacity` separately reports the operating system's current total available to the caller, filesystem free bytes and caller-available bytes. Windows uses `GetDiskFreeSpaceExW`; Unix uses `statvfs`. OS user quotas may make caller-available bytes smaller than filesystem free space. AIRK leaves 16 MiB of physical safety headroom and reports the remaining `available_growth_bytes`. An absent-directory probe resolves its nearest existing ancestor without creating the requested directory.

Private catalog admission also leaves 10% of observed growth available for filesystem activity between planning and the fresh pre-write capacity check. The catalog cap is the smaller of this physical allowance and the remaining logical reservation, with room retained for publishing the active pointer. This margin is not a reservation against other programs; subsequent allocation can still fail safely.

## Admission and publication

Managed writers reserve logical growth under the `.quota-1.2` ledger. Current SQL growth reserves the larger database capacity and its rollback allowance before syncing the owner header. Private lexical/graph preparation and retained objects are charged before writes. Old state, shared model assets and other files below the data root still consume logical root capacity; the new namespace does not free them.

Reservations are conservative. Written bytes may also remain covered by a live reservation, and discarded staging does not refund charges within that reservation. Physical capacity checks are point-in-time observations, not disk reservations. Other programs can still fill the device.

Before Applying, failed admission preserves the previous readable head. During Applying, a failed write leaves the collection unavailable until exact captured recovery completes. Durable replay allowances survive process exit; ordinary uncommitted staging is reclaimed under ownership checks. Current objects are reclaimed after complete ownership proof, never by guessing from filenames alone. `collections maintain` repeats bounded cleanup and does not advance the token.

## Exact boundary and limitations

This is an application logical-byte quota, not an operating-system filesystem quota or a machine-wide physical disk guarantee. It does not account for allocation-unit slack, filesystem metadata, compression, copy-on-write history or external programs writing files. Separate configured data roots have separate policies. Existing files count by length, so manually placing model assets inside a managed root reduces later admission capacity.

The small `.quota-1.2` control ledger is separately bounded rather than charged recursively: at most 1,024 reservations, 1 KiB per record, zero-length lease files and a 64 KiB policy. Lock files and other bounded administration metadata have their own limits. Inventory refuses linked/nonregular state and stops after one million entries. Normal ledger lock acquisition retries for at most two seconds. In 1.5.7 native service work installs a cancellation-aware admission budget of up to thirty seconds with progress wait observations; optional accounting retains its own shorter deadline. Typed quota contention preserves the underlying native lock cause for existing bounded admission callers. Best-effort usage admission checks a deadline between filesystem operations; an individual operating-system I/O call cannot be preempted. These are not hard wall-clock guarantees.

Managed current-version writers participate in this protocol; raw filesystem changes and older binaries cannot be made safe by an in-process reservation API. Protect the data directory accordingly. Tests cover shared root/collection admission, concurrent growth, sparse-write refusal, SQLite page exhaustion, root-only service allocations, nonblocking optional admission, native capacity observation, missing-directory read-only probes and child-process crash cleanup. Full-disk races, hostile filesystem, power-loss and platform qualification remain release work.

See [collection retention](collections.md#retention-and-removal) and [transactional relocation](storage-relocation.md).

In the sidebar, **Storage limits…** edits collection/shared limits while preserving other 1.2 collection overrides. **Reclaim Unreferenced Index Objects** runs current cleanup. Lowering a quota may first clean the selected collection; saving still refuses limits below live allocations. Historical retention controls are unavailable. No disk space is preallocated.

Failed capacity admission retains the published index. The extension stops automatic retries for that collection until Resume or Index now is selected, rather than retrying on every file change. Physical disk-space checks remain independent of logical quotas.
