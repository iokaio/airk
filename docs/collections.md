# Collections, storage and DiskANN

One user/machine may search many unrelated workspaces through one deployed AIRK container. Each workspace's MCP configuration fixes its collection, mounted root and data directory. Native deployment uses the same layout without Docker.

## Implemented isolation

IDs are 1–64 lowercase ASCII letters/digits/hyphens/underscores; path separators, case aliases and reserved filesystem names are rejected. The kernel accesses only that collection's snapshot and checks persisted collection/root identities. Retrieval envelopes identify their collection. MCP tool arguments cannot select another collection or read arbitrary live filesystem paths.

~~~text
<data-root>/
  collections/
    project-a/
      index.sqlite3    # source revision payload, exact facts and relationships
      writer.lock      # only while a writer is active, or after a crash
    project-b/
      index.sqlite3
~~~

Native setup can point all projects to a user-chosen drive with --data-dir. In Docker the shared volume is mounted at /data and each source at a distinct path under /workspaces. Never reuse a collection ID for a different host project. Canonical container paths cannot prove host-workspace identity after changing mounts; managed registration/root aliases are needed before automatic migration.

One collection currently has one canonical root. Multi-root workspace sets will need a versioned root inventory and source IDs including root aliases. Do not merge identical relative paths from different roots before that exists.

## Multiple agents sharing one collection

Claude Code, Codex, Copilot and other local clients can query the **same** index concurrently. Configure their launches with the same canonical root, data directory and collection ID; their client-side server names need not match. Collection identity belongs to the workspace, not to the agent or MCP connection. Do not create an index per agent. Each stdio connection has its own worker process reading the common published snapshot; Docker workers all run inside the one deployed container.

Reads do not acquire the writer lock. One collection writer builds/publishes at a time; a concurrent writer receives a locked error rather than competing to publish. Each retrieval operation loads one coherent snapshot and reports its generation. A request overlapping publication may finish on the previous generation; subsequent requests reopen the published snapshot and refresh worker-local lexical caches. A multi-call investigation is not a transaction: compare generations/hashes when combining evidence across calls. MCP initialization only connects a reader and does not reindex or take ownership of the collection.

The real-binary concurrency test runs two MCP processes against one index, verifies reads with a held writer lock, queries during a third indexing process and checks both readers see the new generation without reconnecting. This validates the bounded scaffold on the tested platform; it does not establish large-repository throughput or a shared memory cache. Disconnect every agent/window using a collection before removing its derived state. Machine-wide reader leases and enforced safe pruning are planned for final storage.

## Memory and disk design

The working scaffold persists derived revisions and normalized exact facts/relationships in embedded SQLite WAL transactions and creates worker-local in-memory lexical maps. Read transactions pin one revision during writer publication; legacy JSON migrates on the next successful index. A standalone MemoryStore retains the snapshot contract for embedding. [SQLite contracts](code-facts-and-relationships.md) describe binding, migration, caps and optional SCIP state.

The selected final tier has persistent lexical/exact facts, local embeddings and **disk-backed DiskANN**. DiskANN is a composable algorithm using a provider that owns graph/vector access. Microsoft's current repository documents memory and disk providers separately. An index saved to disk and fully reconstructed into RAM is insufficient for this target. [DiskANN upstream](https://github.com/microsoft/DiskANN) describes that provider boundary.

The inspected Munarium adapter at the revision in [the inspiration record](inspiration.md) implements in-memory access and restores its provider from sealed bytes. AIRK will implement/qualify an independent disk provider with bounded page/neighbor/vector caches, stable IDs and local file storage. Do not claim the existing inspected adapter already solves large-than-memory retrieval.

## Final layout and publication — planned

~~~text
<data-root>/
  models/<verified-spec>/                 # local immutable model assets
  collections/<id>/
    collection.json                      # explicit root inventory and policy
    sources/<content-hash>/               # retained normalized/source content
    generations/<generation>/
      manifest.json                      # schema, source, parser and model IDs
      lexical/                           # persistent Tantivy component
      facts/                             # SQLite native/imported facts and relationships
      vectors/                           # DiskANN graph, vectors and ID maps
    active.json                          # one coherent published generation
    pending/                             # bounded indexing intents/checkpoints
    cache/                               # disposable per-collection work/cache
~~~

Choose an actual DiskANN package/provider version through Windows/macOS/Linux qualification. Record model digest/dimensions/normalization/distance function, provider revision, tokenizer/parser/chunker revisions and source inventory hash. Build privately, verify/reopen all components, then atomically publish one manifest. Queries pin one generation. Never fuse mismatched lexical/vector/source versions.

Collection-specific indexes/caches must be selected before candidate lookup, not just filtered after search. Disk reads and RSS must be measured independently. Use a bounded exact-vector oracle for recall tests, not an unlabelled substitute for DiskANN. Missing model/index state is not-ready, with no remote fallback.

## Space lifecycle

The implemented CLI lists collection usage and removes a named validated managed collection, leaving sources intact. [Operations](operations.md) provides commands. Automatic pruning, compaction and migration are future work. Planned maintenance first removes unpinned obsolete generations and disposable caches, then explicitly selected obsolete collections. Active generations and referenced model assets remain protected. Show reclaimable bytes and a concrete plan before destructive extension actions. If active data alone exceeds a budget, pause new indexing and surface the capacity issue rather than silently erase evidence.
