# Local vector models and retrieval

AIRK enables vector indexing and hybrid search by default for each collection. The trusted extension prepares the pinned MiniLM model before indexing; the panel shows setup progress and failures. Turn **Vector search** off in the panel to opt out. The saved setting applies to all CLI/MCP connections for that collection. A missing model is explicitly **not ready**; it does not silently disable vectors or use remote inference.

Native-only installations can provision the [pinned model](onnx-models.md) with the supplied script before indexing. Native source indexing without assets can retain lexical evidence, but default hybrid queries remain not ready until a model-bearing generation is published. Explicit `--mode lexical` remains available for individual queries. MCP reads never download files.

The panel installs verified local inputs with `airk collections vectors install <directory>` under native quota reservations. It uses `airk collections vectors show` and `airk collections vectors set --enabled false` (or `true`) to save the collection setting under the native writer lock. Opt-out immediately selects lexical default queries and rejects vector/hybrid queries; the following index omits the model and DiskANN graph. Re-enabling requires indexing again if vectors were removed. Old leased/retained generations and shared input model files are preserved until their normal lifecycle permits cleanup.

The [CPU transformer adapter](onnx-models.md) executes deliberately provisioned ONNX models using tract and a local BERT WordPiece tokenizer. A pinned Apache-2.0 MiniLM recipe, real-model retrieval test and independent ONNX Runtime numerical comparison are provided. Schema 2 binds weights, tokenizer, license, context and pooling into one revision identity. The schema 1 word-vector adapter below remains compatible with existing assets.

The initial embedding adapter averages recognized pretrained word vectors and L2-normalizes the result. It then indexes/searches squared Euclidean distance with Microsoft's DiskANN algorithms and AIRK's disk file provider. This supports deliberately supplied word-vector models; it is not transformer inference, compiler resolution, or evidence of general semantic understanding. Word order, negation, programming-language meaning and unknown identifiers are important limitations. Measure quality for the selected vocabulary and corpus.

## Provision assets

Place `model.json` and `vocabulary.json` at `<data-dir>/models/local/`.

`vocabulary.json` maps lowercase tokens to equal-length arrays of finite float32-compatible values. Use a model you are authorized to use and redistribute; record its actual license. Custom assets remain an explicit operator input; automatic extension setup uses only the pinned Apache-2.0 MiniLM recipe. This original test fixture illustrates format, not production quality:

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

Indexing reuses vectors only for identical chunk input under the complete embedding pipeline descriptor, including model assets/metadata, tokenizer and Unicode tables, pooling, normalization and runtime identity. Cache bytes must match a reference authenticated by a committed publication. Chunk locations and file paths are rebound separately, so an unchanged declaration can reuse inference after a line shift or compatible file move. Full reindex bypasses reuse. Reports separate cache hits, misses, inference calls and no-vector outcomes; no-vector results are deliberately retried. Retention protects cache dependencies of surviving publications and reclaims unreferenced objects. This reuse does not change semantic quality or qualify a real ONNX model.

## Query modes and budgets

Omitting the mode selects hybrid when vectors are enabled, and lexical after the collection opts out. Vector mode performs approximate DiskANN retrieval; `--semantic` remains an alias for vector mode. Hybrid mode uses reciprocal-rank fusion with constant 60 over bounded lexical and vector lists from one generation. MCP `airk_search` supports `mode` values `lexical`, `vector`, `hybrid`. Citation paths, hashes and original document locations are preserved.

The kernel caps hits, response text bytes and vector candidate work. Filtering bounded vector candidates by path can underfill results, and an incomplete scan sets `truncated`. Approximate retrieval does not prove other matching evidence is absent. Each result's generation identifies its source/model revision.

## Qualification

Automated tests use original synthetic vectors and compare reopened disk-backed search with a bounded exact distance oracle. The deterministic 200-vector fixture requires at least 90% recall@10. Tests cover nonfinite/dimension rejection, corruption, immutable reopen, model digest failure, lexical/vector citation binding and source rename behavior. Synthetic recall establishes algorithm/provider integration; it does not qualify a production model's semantic quality.

Before claiming model/corpus performance, record model source/license, exact asset digest/specification, held-out queries, relevance judgments, recall/ranking, cold/warm latency, process RSS, index bytes and file/physical-disk I/O separately. Cross-platform, power-loss and larger-than-memory claims require their own evidence. Index construction streams retained artifacts into the disk components, while the explicitly provisioned vocabulary remains memory-resident within its asset bound. Use schema 2 for [transformer/ONNX inference](onnx-models.md).
