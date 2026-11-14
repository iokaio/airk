# Offline INT8 transformer embeddings

The AIRK 1.9.0 build embeds two pinned trained MiniLM mixed-precision INT8
exports and statically links ONNX Runtime 1.30.0 CPU into the main executable. Inference runs inside the native
service under its actual service lease; compatible projects share its model
session. There is no separate semantic inference process or runtime-library DLL
payload. Project index/source workers retain their process isolation and request
inference through the service-owned provider. No Python, network service or
post-install model download is needed.

The optimization reduces embedding memory and CPU cost and bundled weight size.
[Measurements and limitations](semantic-index-optimization.md) explain the choice.
The trained exports retain FP32 embeddings and six sensitive feed-forward output
projections; 66.7% of linear parameters are quantized. Floating-point pooling,
normalization and output vectors remain. There is no precision fallback.

## Bundled profiles

Both trained exports derive from `sentence-transformers/all-MiniLM-L6-v2`,
revision `10dbd2f06a8baf40ad285b037edda610c1b9a57c`, with retained Apache-2.0
notices, WordPiece tokenization, 256-token context and 384-dimensional normalized
output. The native catalog binds the actual trained bytes, tokenizer,
quantization policy and runtime to a distinct embedding pipeline.

| Execution CPU | Profile | Weight bytes |
|---|---|---:|
| x64 without VNNI | `minilm-l6-v2-u8-v2` | 69,325,926 |
| x64 with AVX-VNNI or AVX-512-VNNI; ARM64 | `minilm-l6-v2-s8-v2` | 69,355,444 |

The signed export exactly recodes the unsigned weight values and uses a two-pass
integer activation calculation to avoid AVX2 saturation. Native CPU detection
selects the profile. Numerical admission and profile selection do not establish
retrieval or platform qualification; campaign results remain in the release
record. `airkctl model-catalog` reports selection and hashes without opening an
index. Both profiles are inputs to every platform installer.

Historical `*-v1` model identities and hashes remain compatibility metadata.
Their original runtime and pipeline are preserved when reading old collections.
Semantic search and incremental vector publication require an explicit full
rebuild for the new pipeline; retained lexical and source reads remain available.
Old vector bytes are never relabeled as trained-v2 vectors.

## Offline build inputs

The Windows entrypoint provisions and verifies these inputs explicitly:

```powershell
# Existing verified model/static ORT cache; no model/ORT downloads.
pwsh -NoProfile -File scripts/build-windows.ps1
# Initial pinned source/tools/model provisioning and static native build.
pwsh -NoProfile -File scripts/build-windows.ps1 -DownloadInputs
```

See [Windows local builds](windows-local-build.md) for custom caches, the exact
five-variable static environment, link-supplement receipts and package commands.
The canonical cross-platform producer is `scripts/provision-static-onnx-runtime.py`:
`--verify` checks an existing build, while explicit `--download --download-tools
--build --parallel 2` prepares pinned inputs and builds the complete `all re2`
archive closure. Its default cache is `target/static-onnx-runtime/<Rust-target>`.
Use `--environment-file <report>` and pass subsequent Cargo/runtime build commands
through `scripts/static-runtime-environment.py --environment-file <report> --
<command> <arguments>`. The helper re-verifies the saved report against actual
inputs and supplies only the approved static variables to the child process.
Cargo/npm dependency restoration remains separate from model/ORT provisioning.

Model provisioning uses `scripts/provision-local-model.py --destination
target/local-vector-model` with verified build inputs. The trained catalog URLs
are null: `--download` cannot substitute historical upstream weights. CI acquires
the authenticated, pinned build-input artifact through
`scripts/model-build-inputs.py`; installed clients do not download these weights. `AIRK_BUNDLED_MODEL_DIR` selects another verified model parent. Each
profile contains `model.json`, `model.onnx`, `tokenizer.json` and `LICENSE`.
Cargo never downloads model assets or ORT libraries. Offline caches must preserve
exact native archives, source/tool/build receipts and notices, not just copy a
platform DLL from an older installation.

Rust build checks exact manifests, sizes and hashes before embedding both models.
The current runtime builder executes the actual main image with
`--verify-bundled-models` and `--verify-model-smoke`, and checks its native imports
for an external ORT dependency. The model smoke runs fixed text twice with an
isolated actual service lease and requires 384 finite, repeat-identical values.
Schema-2 packages contain the main executable plus required metadata and notices;
use the complete verified runtime/VSIX for installation and upgrades.

`scripts/check-transformer-model.py` additionally exercises the installed service's
default, vector and hybrid retrieval, verifies its embedded pipeline identity and
in-process provider, and checks that default setup creates no extracted model
sets. Its custom-input rejection checks copy verified offline build inputs into
the test's own directory; they do not extract or alter the running default model.
The shared owned-fixture wrapper preserves state if native shutdown is uncertain.
These are gate requirements; actual candidate results belong in the release record.

Model identity and provenance remain in the
[model catalog](../crates/airk-retrieval/src/model_catalog.json). Static ORT source,
archive and dependency inputs are pinned in the
[static runtime lock](../scripts/static-onnx-runtime-lock.json); retained build
receipts bind the verified library/link closure to the packaged main image.
Installed AIRK never contacts those source URLs.

### Historical separate-helper builds

AIRK 1.6.0 through the predecessor architecture executed MiniLM in `airk-embed`,
with dynamically distributed ORT libraries beside it. Its
[runtime catalog](../crates/airk-retrieval/src/ort_runtime_catalog.json),
`provision-onnx-runtime.py`, `AIRK_BUNDLED_RUNTIME_DIR` and helper verification
commands describe that historical payload. They remain relevant to exact legacy
verification and migration, but cannot provision or qualify a 1.8.14 static
main-only package. Earlier measurement records retain their original architecture
and do not establish current memory or performance results.

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
Moving weights into the main image does not itself justify changing the model or
vector pipeline identity, discarding valid vectors, or relabeling accepted work.
The approved executable digest and service/provider generation are separate
execution identities and do change across replacement.

The service reports provider kind `in_process` and no separate inference
`process_id`; model pages are included in service RSS. Diagnostics retain provider
identity, admission state, queued/running work, callbacks, delivery and model
references. Zero counters alone do not prove safe retirement: native completion
barriers and retained completion evidence govern drain. Query and indexing work
share bounded admission; canceled callers do not make unfinished native work
cease to exist.

The persistent runtime contract advances to registry schema 13 for in-process
provider semantics. An incompatible predecessor must reject this state with
`runtime_state_contract_unsupported`; rollback requires a compatible runtime or
explicit validated snapshot restoration. Existing accepted requests retain their
original execution identity and require reconciliation, rather than being
silently approved under the replacement image.


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

When a semantic pipeline upgrade requires replacement, the service queues setup
for affected registered projects, including closed workspaces. Setup replaces
incompatible semantic graphs and embedding caches from retained extraction
evidence, then queues durable semantic rebuilding. The schema-13 execution
contract and move to in-process inference are not by themselves reasons to
rebuild vectors whose pipeline identity remains valid.
Text/fact evidence and sources are preserved. Paused projects wait for resume;
semantic opt-outs remain off. Old ONNX publications cannot supply new-pipeline
vectors. Text search remains available while semantic replacement is pending.
Overview and Management separate published coverage from current semantic work;
model setup completion does not mean every vector is ready.

## Native validation

Run the main-image bundled-model and model-smoke checks through the runtime
builder, then the native service and packaging checks from
[CONTRIBUTING](../CONTRIBUTING.md) against that exact sealed runtime. For a scoped
retrieval test, wrap Cargo with the verified static environment described above:

```console
python scripts/static-runtime-environment.py --environment-file <verified-report> -- cargo test -p airk-retrieval --all-features bundled_int8_is_normalized_bounded_and_batch_independent -- --ignored
```

For ignored cache/order tests, set `AIRK_TEST_MODEL_DIR` to the absolute path of
one profile directory, not its parent. Static archive verification, import checks,
model smoke, service/provider lifetime tests and installed upgrade acceptance are
distinct evidence; none alone qualifies the full deployment. Windows results do
not qualify macOS/Linux, VNNI performance or ARM performance. macOS packages target
13.3 or later; compilation on a current runner does not prove oldest-OS support.
