# Local CPU transformer embeddings

Schema 2 executes a real BERT-family ONNX encoder using [tract 0.23.8](https://docs.rs/crate/tract-onnx/0.23.8), compiled into the native binary. It uses a local Hugging Face WordPiece tokenizer, attention-mask mean pooling and L2 normalization. Runtime installation requires no Python, ONNX Runtime DLLs, cloud inference or network connection. Unsupported graphs and tokenizers fail explicitly. [Schema 1 word-vector assets](local-models.md) remain compatible.

## Deliberate provisioning

The initial reproducible model is [sentence-transformers/all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2), revision `10dbd2f06a8baf40ad285b037edda610c1b9a57c`. Its model card declares Apache-2.0 and describes a 384-dimensional English sentence/short-paragraph encoder with a 256-word-piece context. It can support document and natural-language/code-comment matching; this does not establish compiler resolution or broad code understanding.

~~~console
python scripts/provision-local-model.py --destination /path/to/state/models/local --download
airk --data-dir /path/to/state index
airk --data-dir /path/to/state search "recover access after forgetting a login secret" --mode vector
airk --data-dir /path/to/state search "recover access after forgetting a login secret" --mode hybrid
~~~

The provisioning script is a separate operator/developer tool. Without `--download`, it only verifies existing local files. It checks exact sizes and SHA-256 digests, refuses mismatched existing files, and generates `model.json`. Weights are excluded from source control and release packages. The license declaration comes from the model card; the included license file is the standard Apache-2.0 text.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| model.onnx | 90,405,214 | `6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452` |
| tokenizer.json | 466,247 | `be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037` |
| LICENSE | 11,358 | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` |

The manifest contains schema `2`, `name`, `license`, `source`, `revision`, the three asset hashes (`model_sha256`, `tokenizer_sha256`, `license_sha256`), `dimensions`, `max_tokens`, `pooling`, `input_ids`, `attention_mask`, optional `token_type_ids`, and `output`. The pinned model uses dimensions `384`, context `256`, pooling `attention-mask-mean-l2`, the conventional input names, and output `last_hidden_state`. Unknown manifest fields are rejected.

The identity hashes the complete canonical manifest plus AIRK's runtime specification. Tokenizer, context, provenance/license or weight changes therefore create a new revision. Inputs must be int64 `[1, tokens]`; output must be float32 `[1, tokens, dimensions]`. Padding is masked during pooling. Generation model copies are charged to the publication quota along with other index components. Readers lazily cache verified inference plans; lexical operations do not initialize transformer execution.

## Bounds and supported profile

Inference handles one input at a time, taking at most the first 16 KiB of UTF-8 text and then the declared 8–512 tokens. Citations and lexical text remain intact; a vector represents only that prefix. Model files are capped at 256 MiB, tokenizer JSON at 4 MiB, dimensions at 2,048, and vocabulary at 100,000 tokens.

The adapter accepts the supported BERT operator profile and WordPiece/BertNormalizer/BertPreTokenizer pipeline with bounded special-token processing. It rejects external tensor files, custom domains, control flow, random operators, oversized constants and nonstatic intermediate shapes. Individual declared intermediate tensors are capped at 128 MiB and cumulative declared sizes at 1 GiB. These validation limits are not a process-RSS guarantee: model loading, compiler transformations, allocator overhead and caches also consume memory. Use trusted, licensed assets. Native inference is not forcibly interrupted by a wall-clock deadline.

## Reproducible qualification

Original code/document passages and fixed relevance judgments are included in the example. Run the native adapter and compare its embeddings against an independent CPU engine:

~~~console
python scripts/provision-local-model.py --destination target/model-qualification/minilm --download
cargo run -p airk-retrieval --example qualify_model --locked -- target/model-qualification/minilm > target/model-qualification/native-result.json
python -m pip --isolated install --target target/model-qualification/oracle-python --no-user onnxruntime==1.23.2 tokenizers==0.23.1
python scripts/qualify-local-model.py --model-dir target/model-qualification/minilm --native-result target/model-qualification/native-result.json --oracle-python target/model-qualification/oracle-python
~~~

Python packages are qualification tools only. The oracle uses [ONNX Runtime CPU execution](https://onnxruntime.ai/docs/get-started/with-python.html), the verified model/tokenizer, masked pooling and normalization. It rejects component differences above `1e-5` or cosine similarity below `0.99999`.

The local Windows x64 debug run retrieved the expected passage for all six fixed queries from eight original documents. Fourteen embeddings matched ONNX Runtime 1.23.2: maximum component difference `2.2665e-7`, minimum cosine similarity `0.999999995`. Initial plan loading took 8.3 seconds; the six following query embeddings took 0.92–1.19 seconds each. These debug timings describe one machine and are not release-performance targets. The small fixture establishes numerical adapter correctness and limited retrieval behavior, not production-corpus semantic quality.

Set `AIRK_TEST_ONNX` to the provisioned directory and run `cargo test -p airk-retrieval --test onnx_model -- --ignored` for the deliberate persistent Tantivy/DiskANN integration test. Ordinary tests validate asset identities, bounds, external-data rejection and graph shape/operator limits without fetching weights.

`python scripts/check-transformer-model.py --binary target/release/airk --model-dir target/model-qualification/minilm --report target/model-qualification/release-cli.json` exercises the public CLI, retained model copies, vector/hybrid queries, invalid-asset rollback and context-bound model updates. Use `airk.exe` on Windows. The optional report captures direct process CPU, peak RSS and I/O using the [scale harness measurement conventions](scale-qualification.md); each query starts a fresh process and loads its own model plan.

The 2026-10-14 Windows x64 optimized CLI run passed all six lifecycle checks. Its binary SHA-256 was `bdfec036c89d09e3a69e505127621ad2450b1c64db7515aa9d538664635a4df4`. On the original two-document fixture, initial indexing took 4.797 seconds wall time and 4.234 seconds process CPU, peaking at 323.1 MiB RSS. Fresh-process vector/hybrid queries took 0.734–0.750 seconds wall time and approximately 0.703 seconds CPU, peaking at 319.4–319.5 MiB RSS. Those query times include verification and inference-plan loading; they do not measure an already-loaded MCP worker. Operating-system caches were uncontrolled. The tiny fixture and one Windows machine do not establish production-corpus quality or cross-platform performance.

For a production corpus, add held-out queries and relevance judgments from authorized material. Record asset identities, recall/ranking, cold/warm latency, process RSS, logical index bytes and application versus physical I/O separately. Different languages, long inputs, platforms and model upgrades require their own evidence; the pinned MiniLM result does not qualify arbitrary exports.
