# AIRK 1.2.3

Version 1.2.3 makes **Agent Lean** the default evidence level for new collections in the native CLI and VS Code extension. MCP capabilities advertise the same default. No manual selection is needed after a fresh installation or when adding a new collection.

Agent Lean retains exact text, citations, declarations, signatures, imports, selected local structure and useful document facts. It omits generic call occurrences, continuity fingerprints and broad document inventories. Rich Structure, Search Only and Detailed remain available.

The goal is to **start lean and fast, then retain more evidence only when a task needs it**. Agent Lean avoids generating and retaining optional facts that ordinary coding and document discovery often do not need. This is a product goal, not a measured universal speedup: local embedding inference and graph construction still take time.

| Choose | When it is useful |
| --- | --- |
| **Agent Lean (default)** | Everyday code navigation, declarations and signatures, useful imports, document requirements, semantic discovery and exact cited reads. |
| **Rich Structure** | Broader structural/configuration/document facts or declaration fingerprints and continuity tracking are needed, for example when examining a large refactor across revisions. |
| **Detailed** | Individual call occurrences, richer relationship evidence or document-unit continuity are needed for a deeper investigation. These remain adapter-supported syntax evidence, not guaranteed resolved call graphs. |
| **Search Only** | The workflow needs searchable text and exact citations without structured facts or continuity tracking. Vector search remains separately configurable. |

Choose **Index detail → Evidence to retain** on a collection, then **Save and index** to publish the new level. A richer level can increase extraction work, retained data and response size; use it for a concrete need. The published level remains visible while the change is pending.

Semantic search remains enabled with the same local model. Without an explicit vector policy, Agent Lean excludes reviewed lockfile basenames and minified JS/CSS from vector indexing while preserving lexical evidence. Explicit vector policies retain their meaning. The change does not establish a new semantic relevance or indexing-speed guarantee.

Existing collections retain explicitly saved extraction policies. Collections without a saved policy retain their published detail, including the previous Rich Structure default. Legacy policies with omitted detail continue to mean Detailed. Users can switch existing collections to Agent Lean through **Index detail → Evidence to retain**, then index to publish the change.

## Progressive indexing reliability

A timed or byte-limited progressive publication could reclaim a newly retained file before it entered the next batch. The next publication then failed with `publish progressive base batch: prepare current metadata: retained object missing`. This could happen on a fresh installation; wiping state did not fix the ownership bug. The same cleanup could discard superseded objects still needed for reuse later in the active scan.

The collection writer now protects these objects for the scan's lifetime, including intermediate semantic publications. Final publication and later maintenance reclaim unused objects after the scan releases ownership. Existing quota limits, committed evidence validation and journal recovery remain enforced. Regression coverage exercises timed and size-triggered flushes, prior-input reuse ownership, nested capture guards and cleanup after release; a scheduled scan also exercises compression, semantic enrichment and reopening.

After updating the extension and bundled helper to 1.2.3, use **Index now** to retry an affected partial collection. A fresh wipe is not required for this bug: previously committed files remain valid. On 1.2.2, **Reindex** (`index --full` with the affected collection's root/data directory) bypasses progressive publication as a workaround. Semantic readiness now explicitly says it covers committed files when discovery is incomplete; it does not claim complete repository coverage.

Native and extension versions and their lockfiles are synchronized at 1.2.3. Storage format and model identity are unchanged. This default is a product decision; outstanding paired-agent task and broader performance qualification remain outstanding. Historical 1.2.0–1.2.2 release and qualification records retain their original defaults. Publication remains a separate release step.
