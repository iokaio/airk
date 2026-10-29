# Semantic index optimization in 1.6.0

AIRK 1.6.0 replaces the FP32 MiniLM inference path with two bundled INT8 exports.
The purpose is to reduce the installer weight budget and the shared embedding
helper's memory and CPU cost, while keeping semantic search local and offline.
This is the default 1.6.0 implementation, not an opt-in experiment or a precision
setting users must manage. The candidate is being validated before publication.

The decision uses the measurements below: on the tested non-VNNI Windows CPU,
unsigned INT8 delivered about **1.7 times the native FP32 throughput** at about
**61% less peak helper memory**. Both INT8 weight files together total
46,072,842 bytes, about **49% less than the old single 90,405,214-byte FP32 model**.
These are component and raw-weight measurements, not an installer-size promise.
The packaged native runtime and compression also affect the final download.

Semantic search supplements exact text, symbols and facts. The small judged
query set retained unsigned INT8's top-five success rate, but embeddings and
rankings changed; the evidence does not establish universal quality parity.
This tradeoff favors a lighter local helper without making semantic search the
only source of evidence available to an agent.

## Runtime selection and offline delivery

| Execution CPU | Bundled MiniLM export |
| --- | --- |
| x64 without VNNI | Unsigned INT8 (`model_quint8_avx2.onnx`) |
| x64 with AVX-VNNI or AVX-512-VNNI | Signed INT8 (`model_qint8_avx512_vnni.onnx`) |
| ARM64, including Apple Silicon | Signed INT8, byte-identical to the pinned ARM64 export |

Native CPU feature detection selects the model. Both weight files, the tokenizer,
licenses and the platform's CPU runtime library travel in the installer. There
is no model download during installation, setup, rebuilding or queries. There
is no FP32 model or fallback in the build. Hash and inventory checks reject
stale or altered inputs. ARM64 and VNNI performance still need native receipts;
the upstream export names and matching hashes do not establish performance.

Each model call processes one text. Dynamic INT8 activation quantization changed
results when batch companions changed during measurement. B1 keeps an input's
vector independent of other inputs and preserves exact embedding-cache behavior.
Model hashes, tokenizer, preprocessing and runtime are part of pipeline identity.

## Upgrade contract

The 1.6.0 transition replaces prior semantic vectors and embedding caches and
schedules durable semantic rebuilding for registered projects, including closed
workspaces. Text/fact indexes and sources are preserved. Explicit semantic
opt-outs remain off; paused projects wait for resume. Rebuild work must survive
service/editor interruption, and Overview and Management report published text
coverage separately from semantic rebuilding. Old and new vectors are never
mixed. Migration and installer qualification are release gates, not inferred
from the component comparison.

## Measured baseline — 2026-10-28

The following is historical decision evidence against released 1.5.12. The old
FP32 model, manifest fixture and comparison execution path are not retained in
this branch or shipped in 1.6.0. Private corpus and query material is excluded
from the repository and installer.

## Candidates

All candidates use `sentence-transformers/all-MiniLM-L6-v2` at revision
`10dbd2f06a8baf40ad285b037edda610c1b9a57c`, the same tokenizer, 256-token context,
384 output dimensions, masked mean pooling and L2 normalization.

| Profile | Runtime | Model bytes | SHA-256 |
|---|---|---:|---|
| `native-fp32` | Released AIRK helper / tract | 90,405,214 | `6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452` |
| `ort-fp32` | ONNX Runtime CPU | 90,405,214 | Same FP32 asset |
| `ort-u8` | ONNX Runtime CPU | 23,046,789 | `b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21` |
| `ort-s8` | ONNX Runtime CPU | 23,026,053 | `4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474` |

The unsigned export is `onnx/model_quint8_avx2.onnx`; the signed export is
`onnx/model_qint8_avx512_vnni.onnx`. These are upstream quantization recipe names,
not proof of instruction dispatch or qualification on a particular CPU. See the
[pinned upstream files](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/tree/10dbd2f06a8baf40ad285b037edda610c1b9a57c/onnx),
[ONNX Runtime quantization guidance](https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html)
and [Sentence Transformers export guidance](https://sbert.net/docs/sentence_transformer/usage/efficiency.html#quantizing-onnx-models).

## Measurement scope and results

The host was a Windows x64 AMD Threadripper PRO 5975WX with AVX2 and no VNNI.
Each profile ran twice in fresh processes with one inference thread and one text
per forward pass. The same 2,048 passages came from 1,706 files; 1,941 passages
used the longest 256-token bucket. Ten locally reviewed queries had known relevant
file locations. Source hashes were unchanged after measurement.

| Profile | Passages/second | Peak working set (MiB) | Known-file top-five hits |
|---|---:|---:|---:|
| Released 1.5.12 native FP32 | 12.70–12.76 | 321.9 | 10/10 |
| ORT FP32 control | 17.02–17.23 | 214.3–215.7 | 10/10 |
| ORT unsigned INT8 | 21.50–21.59 | 123.0–126.2 | 10/10 |
| ORT signed INT8 | 24.89–25.01 | 121.9–123.3 | 9/10 |

All four had 8/10 top-one hits and 10/10 top-ten hits. Unsigned INT8 versus the
native baseline had mean passage cosine 0.9869, minimum 0.9688, mean top-ten
passage overlap 84%, and Jaccard 0.7296. Signed INT8 had mean cosine 0.9824 and
minimum 0.9576 on this non-VNNI host. This small sample supports the optimization
tradeoff; it does not establish identical rankings or general relevance parity.
ORT measurements included the Python harness, numpy and tokenizer memory; native
measurements used the released helper. No Python inference process ships in AIRK.

Repeated per-text outputs were identical within each profile. Changing batch
companions changed INT8 output, so production uses B1. Cross-profile vectors are
incompatible even when a small query set appears to work with mixed vectors.

Private input snapshots, query labels, vectors and raw reports are excluded from
source and installation packages. Historical measurements do not retain an FP32
model or runnable FP32 comparison path in this branch. Current reproduction uses
the INT8 native checks and offline packaging procedure in [model provisioning](onnx-models.md).
