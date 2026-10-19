# Collection-owned model objects

AIRK uses local models. The trusted extension provisions pinned weights,
tokenizer and license files on first setup; native-only installations provision
them explicitly. No weights are bundled in releases. The model directory is an input;
published generations use verified immutable copies owned by their collection.

## Storage and identity

New generations use manifest schema 3 and retrieval specification
`tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-5/inventory-2/model-objects-1`.
Their checksummed `model-objects.json` component contains a canonical version 1
`ModelObjectSet`, limited to 16 KiB. It records the complete embedding pipeline
digest, the existing model identity, and fixed asset names with raw SHA-256
digests and byte lengths.

Word-vector models require `model.json` and `vocabulary.json`. ONNX models
require `model.json`, `model.onnx`, `tokenizer.json`, and `LICENSE`. Arbitrary
asset paths and incomplete or additional asset sets are rejected. Existing
model format, tokenizer, graph, pooling, and input limits still apply.

Each asset is stored once per collection at
`objects/models/<raw-sha256>`. Generations reference those objects instead of
copying a `model/` directory. Equal weight bytes remain shared when only model
metadata changes. The changed complete pipeline identity still invalidates
embedding reuse. Collection boundaries remain independent; this is not a
machine-wide model cache.

A lexical-only generation has neither model references nor vector files. It
does not require the model-object directory to exist. Historical schema 2
generations continue to read their original inline `model/` assets. Their
manifests and revision inputs are not rewritten during reads.

## Publication and reads

Publication first verifies a bounded asset plan, retains its exact bytes under
the collection writer lock, and then performs inference from those immutable
objects. It checks both the existing model identity and the complete pipeline
digest against the finalized inventory. Source changes during capture, object
corruption, missing assets, and identity mismatches fail explicitly.

New bytes are charged before writes. Temporary copies use reservation-owned
staging so interrupted work can be recovered. Installation never overwrites an
existing digest object; an existing object must verify before reuse. Source
and object paths reject linked directory ancestors and nonregular assets.

The generation manifest authenticates the model-reference sidecar. Readers
hold the generation's canonical lease while opening referenced objects and
verify their lengths, hashes, and pipeline identity. ONNX and word-vector
loaders consume the verified assets directly without recreating a temporary
model directory. No live model-directory fallback is used for a shared-object
generation.

## Retention and relocation contract

Model assets are explicit `Model` object dependencies. Collection maintenance
must retain the union referenced by every surviving full generation, including
reader-protected generations and unfinished retirement work. A history-only
representation retains source and extraction payload dependencies; it does
not retain model assets for historical search, which is not provided.

Removal requires authenticated reference enumeration and verified owned
objects. Unknown files or corrupt reference metadata must not become guessed
deletion candidates. A retained old reader must remain usable after the active
model changes; its model becomes collectible only when no surviving full
generation owns it.

References contain digests rather than absolute paths. Explicit data-root
relocation copies and verifies the collection objects along with generation
metadata. The destination must pass the same reference checks before it is
used.

## Qualification scope

Original tiny word-vector fixtures exercise sharing, metadata-only changes,
corruption, quota failures, reader protection, lexical-only operation, legacy
inline compatibility, and relocation. These fixtures do not establish ONNX
model quality or qualify an arbitrary user-provided model. Platform and test
receipts belong in the implementation ledger after the corresponding local
gates complete.
