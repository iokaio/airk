# Offline INT8 transformer embeddings

AIRK 1.6.0 runs two pinned MiniLM INT8 exports through ONNX Runtime 1.22.1 CPU
inside `airk-embed`. Service, project workers and MCP frontends exclude inference
weights and engine dependencies. Compatible projects share the helper's model
session. No Python, network service or post-install download is needed.

The optimization reduces embedding memory and CPU cost and bundled weight size.
[Measurements and limitations](semantic-index-optimization.md) explain the choice.
FP32 weights, tract inference and precision fallbacks are removed. Floating-point
pooling, normalization and output vectors remain; INT8 describes quantized weights.

## Bundled profiles

Both exports use `sentence-transformers/all-MiniLM-L6-v2`, revision
`10dbd2f06a8baf40ad285b037edda610c1b9a57c`, Apache-2.0, WordPiece tokenization,
256-token context and 384-dimensional normalized output.

| Execution CPU | Profile | Weight bytes |
|---|---|---:|
| x64 without VNNI | `minilm-l6-v2-u8-v1` | 23,046,789 |
| x64 with AVX-VNNI or AVX-512-VNNI; ARM64 | `minilm-l6-v2-s8-v1` | 23,026,053 |

The signed export is byte-identical to the pinned upstream ARM64 export. Native
CPU detection selects the profile. Export names do not establish performance
qualification. `airk model-catalog` reports observations, selection and hashes
without opening an index. Both profiles ship in every platform installer.

## Offline build inputs

```console
python scripts/provision-local-model.py --destination target/local-vector-model --download
python scripts/provision-onnx-runtime.py --download
python scripts/build-service-runtime.py --destination artifacts/runtime/int8-local --profile release
```

These are explicit build-time downloads. Without `--download`, provisioning only
verifies existing inputs. Offline builders can transfer both profile directories
under `target/local-vector-model` and `target/local-onnx-runtime/<os>-<arch>`.
`AIRK_BUNDLED_MODEL_DIR` may select another model parent directory. Each profile
has `model.json`, `model.onnx`, `tokenizer.json` and `LICENSE`. Cargo never downloads
model assets or runtime libraries.

Rust build checks exact manifests, sizes and hashes before embedding both models.
Runtime staging and packaging execute `airk-embed --verify-bundled-models` against
the actual helper, rejecting missing, substituted, extra or FP32 profiles. CPU
libraries and their notices ship beside the helper under the complete installer
inventory. Copying only `airk.exe` is insufficient; use the complete runtime/VSIX.

Asset identity and provenance are recorded in the
[model catalog](../crates/airk-retrieval/src/model_catalog.json) and
[runtime catalog](../crates/airk-retrieval/src/ort_runtime_catalog.json).
Installed AIRK never contacts those source URLs.

## Inference and identity

One CPU session executes one text per forward pass, with CPU spinning disabled.
Dynamic quantization depends on activation ranges; batching different texts
changed vectors during measurement. Per-text execution preserves cache behavior
regardless of batch companions. UTF-8 input, WordPiece context and execution
admission are bounded, followed by masked mean pooling and L2 normalization.
Blank input has no vector. Altered assets, unsupported manifests, invalid tensor
shapes and nonfinite output fail explicitly. Schema-1 word-vector fixtures remain
separate from the shipped transformer; they are not a precision fallback.

Pipeline identity covers exact weights/tokenizer, runtime, platform, preprocessing
and pooling. Query and index vectors must use the same identity.

## Bounds and supported profile

New ONNX imports must match the exact bundled profile selected for this CPU,
including its preprocessing manifest. An obsolete, edited or mismatched profile
fails before the saved model changes. Read-only metadata inspection can still
describe earlier assets so they can be retired during upgrade.

Each forward pass uses one text, a UTF-8-safe 16 KiB prefix and at most 256 tokens.
Semantic units retain original citation members; the existing 64-member fan-out
bound can split a group. The 192 MiB inference execution reservation is an admission
estimate, not a measured resident-memory ceiling or a whole-service memory limit.

## Semantic upgrade

The service queues setup for registered projects once during registry upgrade,
including closed workspaces. Setup replaces old semantic graphs and embedding
caches from retained extraction evidence, then queues durable semantic rebuilding.
Text/fact evidence and sources are preserved. Paused projects wait for resume;
semantic opt-outs remain off. Old ONNX publications cannot supply new-pipeline
vectors. Text search remains available while semantic replacement is pending.
Overview and Management separate published coverage from current semantic work;
model setup completion does not mean every vector is ready.

## Native validation

After provisioning:

```console
cargo test -p airk-retrieval --all-features bundled_int8_is_normalized_bounded_and_batch_independent -- --ignored
```

For ignored cache/order tests, set `AIRK_TEST_MODEL_DIR` to the absolute path of
one profile directory, not its parent. Run native service and packaging checks
from [CONTRIBUTING](../CONTRIBUTING.md) against the exact runtime being packaged.
Windows validation does not qualify Mac/Linux, VNNI performance or ARM performance.

Prepared runtime inputs can live outside Cargo's target cache using
`AIRK_BUNDLED_RUNTIME_DIR`; its children are platform directories such as
`windows-x86_64`. The container uses this to preserve verified libraries when
BuildKit mounts its compiler cache. macOS packages require 13.3 or later, matching
the pinned inference library's declared minimum; oldest-OS qualification remains
separate from compilation on current runners.
