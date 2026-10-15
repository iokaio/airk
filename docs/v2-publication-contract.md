# V2 publication and migration

The authoritative inventory is `InventoryV2`, version 3, with version 2 read compatibility. It binds canonical source roots, effective extraction profiles, path-free payload digests, artifact handles, file and continuity stamps, producer assertions and the lineage digest. Version 3 also binds the applied extraction policy and complete embedding pipeline descriptor. The schema-4 `Snapshot` is a compatibility projection for existing bounded consumers; its serialization does not define the v2 revision. The storage engine format remains 2; inventory and engine versions are independent.

The root SQLite pointer and immutable exact-fact catalog use independently checked version 3. Legacy root versions 1–2 and compact inventory version 1 remain readable. Legacy digest verification uses the original encoded bytes and original engine framing. A successful v2 root transaction raises the root version, causing older writers to fail closed. Merely reading a collection does not migrate it.

The immutable catalog's continuity extension has its own schema version 1 and indexed key/path projection. Older catalogs without that extension return no continuity record. Incomplete or unknown extensions fail closed. Public fact keys are bound during catalog construction; imported assertions retain their producer identity.

Optional `embedding-cache.json` is a manifest-authenticated component containing sorted references to exact cache objects used by that publication. A cache object's own checksum does not authorize reuse: a prior committed reference must also authenticate its bytes. Retention traces these references from every surviving generation, including reader-protected generations. Full rebuilds bypass lookup and verify recomputed values against existing immutable objects. No-vector outcomes are counted explicitly and are not cached.

All kernel writers use the same finalizer: initialization, indexing, full reindexing, SCIP import, LSP/Ctags and OCR. The finalizer allocates a random full-width incarnation on first publication and advances the committed parent sequence. It preserves same-location artifact handles and independently updates content/evidence stamps. Bound evidence includes incarnation, artifact identity, original root/path, payload, effective profile and the producer domain. Copy/move reconciliation and detailed change events are later phases; phase 2 records explicit unavailable coverage rather than claiming a complete delta.

Publication proceeds under the collection writer lock and quota reservation:

1. Verify retained inputs, convert legacy output without changing interpretation, and retain source/payload objects.
2. Encode lineage and canonical inventory, then derive the revision token from inventory bytes and the retrieval specification. Neither input contains the final manifest checksum or its own revision.
3. Build private lexical/vector components and the exact-fact catalog. Close the catalog and write lineage before enumerating the final component checksums.
4. Write the receipt-bound manifest, sync and reopen the prepared components, then rename the generation into immutable storage.
5. Compare the expected parent and atomically commit the active pointer and retained publication receipt. Preparation, quota or parent conflicts leave the previous committed revision authoritative.

The final manifest has a 128 KiB bound and at most 1,024 component entries. Store and retrieval verify it against a trusted receipt; an orphan generation cannot authorize itself. Catalog admission reserves space for the final manifest and lineage as well as the pointer transaction.

Native extraction uses effective profile selection before verified cache lookup. Full rebuilds bypass reuse. Conversion separates local syntax identities from emitted fact records, preserving suppressed owners and shared framework identities. Binding reconstructs public IDs from typed local identities; it never rewrites arbitrary string prefixes. OCR retains its exact operation provenance without rerunning a producer during migration. A source or interpretation mismatch fails or requires extraction under the selected profile.

Metadata reads cache a leased catalog by its exact committed token and use indexed file rows. Warm file metadata does not decode the inventory or payload. Source reads continue to verify the requested retained object. Immutable catalog leases preserve earlier evidence across subsequent publications.

Mixed-format pruning and relocation recognize both inventories. Receipt retirement follows removal of the last protected representation; cleanup failure is reported separately. Automatic retention defaults, historical packages and checkpoints are not enabled by this migration. Native platform and installed-provider qualification remain distinct from local fixture results; see the [execution ledger](architecture-v2-ledger.md).
