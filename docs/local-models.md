# Local vector models and retrieval

AIRK provides persistent lexical retrieval without a model. Vector and hybrid retrieval are optional and entirely local. A missing model is explicitly **not ready**, with no remote fallback or fabricated hash vectors.

The [CPU transformer adapter](onnx-models.md) executes deliberately provisioned ONNX models using tract and a local BERT WordPiece tokenizer. A pinned Apache-2.0 MiniLM recipe, real-model retrieval test and independent ONNX Runtime numerical comparison are provided. Schema 2 binds weights, tokenizer, license, context and pooling into one revision identity. The schema 1 word-vector adapter below remains compatible with existing assets.

The initial embedding adapter averages recognized pretrained word vectors and L2-normalizes the result. It then indexes/searches squared Euclidean distance with Microsoft's DiskANN algorithms and AIRK's disk file provider. This supports deliberately supplied word-vector models; it is not transformer inference, compiler resolution, or evidence of general semantic understanding. Word order, negation, programming-language meaning and unknown identifiers are important limitations. Measure quality for the selected vocabulary and corpus.

## Provision assets

Place `model.json` and `vocabulary.json` at `<data-dir>/models/local/`.

`vocabulary.json` maps lowercase tokens to equal-length arrays of finite float32-compatible values. Use a model you are authorized to use and redistribute; record its actual license. AIRK does not download a model or silently accept a license. This original test fixture illustrates format, not production quality:

~~~json
{"authenticate":[1.0,0.0],"login":[0.99,0.01],"invoice":[0.0,1.0]}
~~~

Hash the exact vocabulary bytes with SHA-256, then create `model.json`:

~~~json
{
  "schema": 1,
  "name": "your-reviewed-model-name",
  "license": "the-actual-model-license",
  "sha256": "replace-with-the-64-lowercase-hex-sha256-of-vocabulary-json",
  "dimensions": 2
}
~~~

On Windows, `Get-FileHash -Algorithm SHA256 vocabulary.json` prints the digest; lowercase its hex value. On macOS use `shasum -a 256 vocabulary.json`; on Linux use `sha256sum vocabulary.json`. Schema 1 fixes pooling, normalization and distance as described above. Unknown manifest fields are rejected. The manifest is capped at 16 KiB, vocabulary at 256 MiB and one million tokens, and dimensions at 1–2,048. Components must be finite with absolute magnitude at most 10^10. These are safety bounds, not recommended sizes or performance targets.

Tokenization splits on characters other than letters, numbers and underscores, then lowercases each token. Repeated words participate repeatedly in the mean. Unknown words are skipped. Chunks with no recognized words or a zero mean receive no vector and retain lexical retrieval. A query with no recognized model evidence fails explicitly.

~~~console
airk --data-dir /path/to/state --collection project-a index
airk --data-dir /path/to/state --collection project-a status
airk --data-dir /path/to/state --collection project-a search login --mode vector
airk --data-dir /path/to/state --collection project-a search login --mode hybrid
~~~

Ordinary indexing detects a model digest change, advances the revision and builds coherent components. Invalid/mismatched assets fail indexing while preserving the previous generation. Each generation contains verified copies of its model assets so a query cannot silently combine new embeddings with an old graph. Status reports `local_model_ready`, `indexed_vectors`, `lexical_backend` and `vector_backend`. A ready model with fewer vectors than chunks indicates vocabulary coverage gaps, not a fully embedded inventory.

## Query modes and budgets

Lexical mode is the default. Vector mode performs approximate DiskANN retrieval; `--semantic` remains an alias for vector mode. Hybrid mode uses reciprocal-rank fusion with constant 60 over bounded lexical and vector lists from one generation. MCP `airk_search` supports `mode` values `lexical`, `vector`, `hybrid`. Citation paths, hashes and original document locations are preserved.

The kernel caps hits, response text bytes and vector candidate work. Filtering bounded vector candidates by path can underfill results, and an incomplete scan sets `truncated`. Approximate retrieval does not prove other matching evidence is absent. Each result's generation identifies its source/model revision.

## Qualification

Automated tests use original synthetic vectors and compare reopened disk-backed search with a bounded exact distance oracle. The deterministic 200-vector fixture requires at least 90% recall@10. Tests cover nonfinite/dimension rejection, corruption, immutable reopen, model digest failure, lexical/vector citation binding and source rename behavior. Synthetic recall establishes algorithm/provider integration; it does not qualify a production model's semantic quality.

Before claiming model/corpus performance, record model source/license, exact asset digest/specification, held-out queries, relevance judgments, recall/ranking, cold/warm latency, process RSS, index bytes and file/physical-disk I/O separately. Cross-platform, power-loss and larger-than-memory claims require their own evidence. Index construction streams retained artifacts into the disk components, while the explicitly provisioned vocabulary remains memory-resident within its asset bound. Use schema 2 for [transformer/ONNX inference](onnx-models.md).
