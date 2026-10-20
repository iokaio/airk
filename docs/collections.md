# Collections and current storage

A collection binds a canonical owner root and explicit source aliases to one independent current index. See the [1.2 runtime contract](current-index-runtime.md) for exact bounds, recovery and token semantics.

## Identity and isolation

The layout is `<data-dir>/collections-1.2/<id>`. Collection identifiers remain stable lowercase local names; tools cannot switch their launch-time root or collection. Old `<data-dir>/collections/` state is left untouched. Per-collection settings start fresh in 1.2.

## Published components

`current-owner.json` bounds ownership and capacity; `current.sqlite3` stores the authenticated map, graph records/proofs, head and transaction journal. The head selects `lexical-<sequence>`. Content-addressed objects hold exact source/extraction bytes, owned model assets and embedding packs. Authenticated current metadata/resources retain file identities, analysis and cache ownership. No generation inventory or old receipt is a production read authority.

## Retrieval and memory

Requests hold one shared gate through candidate selection and evidence rendering. Exact source reads decode only the selected retained artifact. Fact/relationship postings and graph records are authenticated with bounded reads; cursors bind the current token and filters. Scan preparation uses a bounded compact projection, not a persisted historical snapshot. Tantivy manifest verification and ownership scans still have corpus-dependent work.

## Multiple agents sharing one collection

Use the same source root, data directory and collection for all agents. Writers serialize; active requests can defer publication. Applying returns busy/unavailable until completion or recovery. Idle connections do not pin old evidence. Reissue expired exact-token/cursor requests against the current head after checking the reported state.

## Retention and removal

Only current evidence and durable pending work own retained objects. Publication attempts bounded cleanup; `airk collections maintain` repeats it and reports removed files/bytes and pending work. Historical retention, generation pruning, checkpoints and rollback are unsupported. Removal acquires writer, lifecycle, usage and current-request exclusion and deletes only the selected managed collection. Sources remain untouched.

## Relocating a data root

Use `airk --data-dir OLD storage plan NEW` to preview an explicit move and `storage relocate NEW` to perform it. The destination must be a new directory with sufficient quota headroom. Active workers, writers, usage transactions or active current requests prevent relocation. The service verifies file hashes before activation, keeps old managed payloads, and records recoverable markers so ordinary clients cannot reopen retired or incomplete roots. `airk --data-dir OLD storage recover` resumes the recorded operation. Update client configuration explicitly after success; the storage picker does not silently migrate collections. See [relocation and recovery](storage-relocation.md) for boundaries and retained-copy capacity planning.
