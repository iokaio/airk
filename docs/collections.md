# Collections and current storage

A collection binds a canonical owner root and explicit source aliases to one independent current index. See the [native publication contract](service-publication.md) for exact bounds, recovery and token semantics.

## Identity and isolation

The native layout is `<data-dir>/collections-1.4/<id>/incarnations/<identity>`, selected by an owned `active.json`. Collection identifiers remain stable local names; tools cannot switch their registered root or collection. Version 1.4.5 requires fresh registration and full indexing at the SQLite boundary. Cleanup removes only recognized old AIRK database files under verified ownership and exclusion; unknown files, sources and unrelated projects remain protected.

## Published components

`current-owner.json` bounds ownership and capacity; `current.airk` stores the authenticated map, graph records/proofs, head and transaction journal. The coherent format-150 head authenticates catalog, lexical and graph tables. Content-addressed objects hold exact source/extraction bytes, owned model assets and embedding packs. Authenticated current metadata/resources retain file identities, analysis and cache ownership. No generation inventory or old receipt is a production read authority.

## Retrieval and memory

Requests hold one shared gate through candidate selection and evidence rendering. Exact source reads decode only the selected retained artifact. Fact/relationship postings and graph records are authenticated with bounded reads; cursors bind the current token and filters. Scan preparation uses a bounded compact projection, not a persisted historical snapshot. Bounded authenticated reads enforce page, posting, byte and deadline limits; complete ownership/verification scans can still have corpus-dependent work.

## Multiple agents sharing one collection

Use the same source root, data directory and collection for all agents. Writers serialize. Healthy Prepared and Applying phases retain the prior coherent committed snapshot; later requests see the new head after atomic commit. Live readers can defer reclamation, retirement and compaction. Failed backend I/O can require readers to drain before reopening and recovery. Idle connections do not pin old evidence. Reissue expired exact-token/cursor requests against the current head after checking the reported state.

## Retention and removal

Only current evidence and durable pending work own retained objects. Publication attempts bounded cleanup; native maintenance repeats it and reports removed files/bytes and pending work. Use the trusted Service Console for selected-project maintenance. **AIRK: Compact Index Storage…** separately reduces the current database file under Ready/no-pending and reader/writer exclusion, verifying publication identity before and after. Historical retention, generation pruning, checkpoints and rollback are unsupported. Removal acquires writer, lifecycle, usage and current-request exclusion and deletes only the selected managed collection. Sources remain untouched.

## Relocating a data root

The native storage picker prepares a reviewed binding; it does not silently move an existing collection. Use [native project setup](service-project-setup.md) and inspect the selected source/data identity before indexing. The older [standalone relocation record](storage-relocation.md) is historical and does not authorize copying an active native database.
