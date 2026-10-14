# Storage quotas

AIRK enforces reservations for application-written logical bytes in one canonical data root. The default policy allows 20 GiB for the root and 4 GiB for each collection. Temporary publication files, content-addressed source/extraction objects, models, retained generations and database journals all consume capacity. A generation with a live reader still counts; a quota never authorizes deleting it.

## Inspect and configure

~~~console
airk --data-dir /path/to/state --collection project-a collections quota status
airk --data-dir /path/to/state --collection project-a collections quota policy
airk --data-dir /path/to/state --collection project-a collections quota set quota-policy.json
~~~

`set` replaces the complete shared policy, including overrides, from a JSON file of at most 64 KiB. Values are integer bytes:

~~~json
{
  "root_bytes": 21474836480,
  "default_collection_bytes": 4294967296,
  "collections": { "project-a": 8589934592 }
}
~~~

Status distinguishes existing `used_bytes` from outstanding `reserved_bytes`, with corresponding collection totals. A policy cannot be lowered below existing bytes plus active reservations. Canonicalization makes alternate paths to the same root use the same ledger. Collection IDs retain their normal validation rules.

## Admission and publication

The native ledger serializes admission across processes. A reservation retains its entire allowance until the last handle closes. Writers charge before increasing a blob or graph file, copying a model, writing Tantivy data, or publishing metadata. Atomic replacements charge their temporary payload too. Private SQLite catalogs reserve a fixed page budget before creation; page growth and journal-changing operations cannot escape that allocation. Their journal is disabled only because unpublished catalogs are disposable. The active revision pointer retains transactional publication and a reserved database/journal allowance.

Publication requests available root/collection capacity, capped at 4 GiB per attempt. This is conservative: bytes already written can also remain covered by an outstanding reservation, deleted build intermediates do not refund charges, and a concurrent publisher may be refused even if its final compact result would fit. Root services such as the scheduler reserve shared capacity without inventing a collection identity. Usage recording uses fail-fast admission and a short soft deadline; a busy or exhausted ledger drops optional accounting rather than turning retrieval into an indexing operation.

If allocation fails, the active generation is unchanged. New unreferenced blobs may remain until explicit pruning. Quota-owned private staging is removed on normal guard release or recovered after process death using operating-system leases. Recovery only removes known reservation-owned staging; retained generations and source workspaces are preserved. Retry after pruning obsolete state or deliberately changing the policy. Repeated blind retries do not increase available capacity.

## Exact boundary and limitations

This is an application logical-byte quota, not an operating-system filesystem quota or a machine-wide physical disk guarantee. It does not account for allocation-unit slack, filesystem metadata, compression, copy-on-write history or external programs writing files. Separate configured data roots have separate policies. Existing files count by length, so manually placing model assets inside a managed root reduces later admission capacity.

The small `.quota` control ledger is separately bounded rather than charged recursively: at most 1,024 reservations, 1 KiB per record, zero-length lease files and a 64 KiB policy. Lock files and other bounded administration metadata have their own limits. Inventory refuses linked/nonregular state and stops after one million entries. Normal ledger lock acquisition retries for at most two seconds. Best-effort usage admission checks a deadline between filesystem operations; an individual operating-system I/O call cannot be preempted. These are not hard wall-clock guarantees.

Managed current-version writers participate in this protocol; raw filesystem changes and older binaries cannot be made safe by an in-process reservation API. Protect the data directory accordingly. Tests cover shared root/collection admission, concurrent growth, sparse-write refusal, SQLite page exhaustion, root-only service allocations, nonblocking optional admission and child-process crash cleanup. Full-disk, hostile filesystem, power-loss and platform qualification remain release work.

See [collection retention](collections.md#retention-and-removal) and [transactional relocation](storage-relocation.md).
