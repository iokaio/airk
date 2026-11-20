# Candle versus ONNX Runtime for AIRK

*20 November 2026 · Source and evidence review*

Keep ONNX Runtime (ORT) for the current FP16 release campaign. Candle is worth testing as an optional backend for **Context Compact on Windows CUDA**, using AIRK's existing compiler and a native Candle scorer. That experiment tests the main benefit of switching: direct control of precision, attention and memory in Rust. Removing ORT entirely would also require replacing the bundled MiniLM pipeline and carrying the approved Core ML and Radeon paths through separate implementations. No Candle performance or quality measurements were collected for this review.

## What AIRK uses today

- **Bundled semantic retrieval.** AIRK ships two pinned MiniLM-L6-v2 variants with original unsigned and signed INT8 graphs, platform selection, per-text dynamic activation semantics, attention-mask mean pooling and L2 normalization. The embedding identity includes runtime, OS and architecture, along with the model, tokenizer, preprocessing and pooling. A different runtime therefore creates a different vector pipeline even if the vector dimension stays the same. Existing semantic indexes and caches need the current upgrade and regeneration path or an explicitly approved compatibility decision. [MiniLM catalog contract](../../crates/airk-retrieval/src/model_catalog/CONTRACT.md) · [Embedding identity owner](../../crates/airk-retrieval/src/model.rs)
- **Optional Arbiter.** Focus is a Qwen3-0.6B pair reranker with the current 1,024-token static pair target and a sparse no/yes readout. Context Compact and Large use the selected Qwen3-1.7B and 4B weights, with a batch-one causal input and up to eight ordered card logits at the decision position. The campaign exercises Context at 32, 128, 1,024 and 8,192 tokens. AIRK's Rust compilers own tokenization, escaping, positions, masks and readout rows. The model runtime consumes their output. [Compiler contract](../../crates/airk-retrieval/src/arbiter/compiler/CONTRACT.md) · [Current assignment](../agent-current-work.md)
- **Runtime and delivery.** The workspace pins the Rust `ort` binding at `2.0.0-rc.10` and native ORT at 1.30.0. The service owns one static core and loads authenticated optional CUDA providers under separate Windows PE and Linux ELF policies. It verifies actual graph placement, cold and warm resource claims, synchronizes device work and retains the live service lease through teardown. Auto uses an eligible GPU variant or a qualified fitting CPU variant of the same model. The approved sequence continues from Windows and Linux CUDA to native macOS Core ML, then the planned Radeon experiments. [Workspace pin](../../Cargo.toml) · [Native build lock](../../scripts/static-onnx-runtime-lock.json) · [CUDA runtime contract](../../crates/airk-retrieval/src/arbiter/runtime/cuda/CONTRACT.md) · [Provider plan](../arbiter/final-plan.md#verified-starting-point-and-backend-order)

The existing Windows engineering baseline is concrete. Compact and Large passed the matching raw limit at all four development widths. Their warm 8,192-token model-forward p95s were 1.7721 and 4.793535 seconds on the A6000. The scientific results leave dedicated residency, calibrated Answer, paired held-out evidence and installed release qualification open. Use the exact paired inputs and separate latency measures when comparing a new backend. [Windows development and resume checkpoints](../agent-current-work.md#extension-restart-checkpoint-20-november-2026-at-1354-utc)

## Where Candle fits

Hugging Face Candle provides Rust tensors, neural-network layers and a collection of transformer implementations. The published `candle-core` version inspected is 0.11.1. The upstream source exposes CPU, optional CUDA and optional Apple Metal, with optional MKL and Accelerate integrations. The reviewed manifest exposes CUDA through `cudarc` and Candle kernels, and FlashAttention through a separate CUDA feature. These are implementation and dependency choices that need their own pinned toolchain and distribution review. [Published crate](https://docs.rs/crate/candle-core/0.11.1) · [Core feature manifest](https://github.com/huggingface/candle/blob/main/candle-core/Cargo.toml) · [FlashAttention build](https://github.com/huggingface/candle/blob/main/candle-flash-attn/build.rs)

| Area | Current ONNX Runtime strategy | Native Candle strategy |
| --- | --- | --- |
| Model representation | Reuses pinned graphs, external weights, schemas and ORT graph optimizers. Vendor-specific attention nodes still need provider-specific lowering and evidence. | Loads weights into explicit Rust models, for example from pinned SafeTensors. AIRK would own the forward, architecture variants, sparse scoring heads and precision boundaries. |
| CUDA and long context | Existing CUTLASS efficient-attention development evidence and actual final SessionState placement checks. Static core, provider and toolchain custody are substantial. | Direct control of device transfers, casts, allocations and kernel dispatch. Upstream Qwen3 provides useful building blocks, including a feature-gated CUDA FlashAttention path. Its dense branch still constructs scores and probabilities, so dispatch must be verified at 8,192 tokens. |
| CPU | Existing MiniLM INT8 path, Qwen CPU profiles, cold/warm observations and per-model admission. | A smaller dependency boundary becomes possible after ORT is removed from every in-process user. Matching the current U8/S8 activation math and CPU thread and memory behavior requires its own implementation and evidence. |
| Other accelerators | ORT documents Core ML, DirectML and AMD MIGraphX, and marks its ROCm EP deprecated. AIRK's planned providers still need their own qualified artifacts. | Metal offers a separate Apple GPU path. It would require a new qualification and a plan decision if it replaced Core ML. Candle's upstream core feature roster exposes no DirectML, Core ML or ROCm backend. Radeon would need another runtime or a new backend. |
| Diagnostics and delivery | Existing loader, manifest and receipt machinery can observe provider partitioning and admit exact builds. Native packaging and optimizer interactions add work. | Rust-level traces can make model operations easier to audit. AIRK would have to measure actual kernels and transfers, control driver and library discovery, ship exact native dependencies and qualify install, restart and fallback. CUDA still brings native build and driver dependencies. |

Sources for the backend comparison include the [Candle Qwen3 implementation](https://github.com/huggingface/candle/blob/main/candle-transformers/src/models/qwen3.rs), [Candle transformer features](https://github.com/huggingface/candle/blob/main/candle-transformers/Cargo.toml), [ORT provider roster](https://onnxruntime.ai/docs/execution-providers/), [ORT CUDA options](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html), [ORT Core ML](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html) and [ORT DirectML](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html).

Candle also has a `candle-onnx` evaluator. The inspected implementation directly interprets a protobuf graph and constructs initializers from embedded tensor data on CPU. That code gives no basis for assuming compatibility with AIRK's external-weight format, signed and unsigned INT8 operators, contrib attention or ORT placement and optimizer evidence. If explored, first inventory the exact graph operators, dtypes and storage and test a real artifact. A native Candle forward is the better experiment for testing the claimed benefits of explicit execution. [Candle evaluator source](https://github.com/huggingface/candle/blob/main/candle-onnx/src/eval.rs)

## Work required

1. **Add an internal scorer path.** Keep Candle behind the native Retrieval boundary. Reuse the existing compiled inputs, ordered output type, service lease and resource admission rather than duplicating scoring in CLI, Workbench or MCP. Pin Candle crates, CPU features, CUDA toolchain and GPU architectures. Use authenticated local weights and config, with each tensor's name, shape, dtype and bytes checked against its source. If production selection needs a new native runtime identity, make the contract decision first. The current Core GPU backend values are bound to ORT runtime identifiers, so a Candle CUDA execution must not claim the ORT CUDA identity. [Arbiter runtime owner](../../crates/airk-retrieval/src/arbiter/runtime.rs) · [Core device contract owner](../../crates/airk-core/src/arbiter/device.rs)
2. **Implement the exact model mathematics.** For Compact, start from the approved original weights, retain the native token/mask/position and card inputs, score only the original output rows, and clear or avoid request-spanning KV state. Candle's stock causal-LM head projects the final hidden state to vocabulary logits. AIRK requires a sparse head and its reviewed arithmetic. Port the protected half-storage and FP32 compute boundaries first, or treat different boundaries as a distinct candidate with an independent reference. Qualify causal masking, GQA head grouping, RoPE, normalization and accumulation explicitly. At 32 heads and 8,192 tokens a single dense attention score tensor is 4 GiB in FP16 or 8 GiB in FP32. The candidate needs fused or equivalently bounded attention without that intermediate. Large and Focus then need separate implementations and evidence. Focus must preserve the left-padded pair mask and the exact no/yes readout. [Qwen3 source](https://github.com/huggingface/candle/blob/main/candle-transformers/src/models/qwen3.rs) · [AIRK FP16 plan](../arbiter/final-plan.md#fp16-implementation-and-validation-sequence)
3. **Rebuild runtime evidence around Candle.** Authenticate the actual model, executable, dependencies, adapter and driver. Trace which kernels ran, dtypes, synchronization, host/device transfers and bounded allocations in the trusted service. Retain service ownership, resource reservations, deadlines, cancellation and completion, and prove fallback and GPU opt-out under the same conditions as the ORT arm. Keep Windows and Linux package and execution identities separate. For a full replacement, implement the equivalent MiniLM pipeline on each supported CPU architecture, exercise old-index detection and resumable regeneration, then resolve Core ML and Radeon coverage before removing ORT from packaging. [Service runtime contract](../../crates/airk-inference/src/inference_engine/pool/arbiter/CONTRACT.md) · [MiniLM catalog and regeneration contract](../../crates/airk-retrieval/src/model_catalog/CONTRACT.md)
4. **Make the comparison decisive.** First run the exact four Context widths, the close-call controls and independent matching-recipe and original-FP32 references. Record finite complete vectors, maximum raw error, mass and winner differences under the approved comparison rules. Then use one cold load and 20 unprofiled warm samples, actual host/device peak and fit under pressure, and matched installed Answer p95 and fallback. Measure build time, distributed and installed bytes, and loaded dependencies separately. Proceed to exact-artifact calibration and the held-out 98% paired judged-evidence gate only after those prerequisites pass. Take the ORT baseline from the same host and workload, and include all failed requests in the comparison. [Numerical and paired task acceptance](../arbiter/final-plan.md#accepted-context-fp16-task-quality-qualification) · [Developer machine fit](../arbiter/final-plan.md#developer-memory-fit-and-shipping-auto)

A Compact pilot can show whether Candle reduces the cost of the CUDA path while meeting the current quality, fit and latency gates. A win there would support a backend-specific adoption. Full migration has a separate decision point once the default embedding pipeline, other platform paths, installer and recovery costs are measured.

## Alternative Approach

### Arbiter on Candle: implementation and comparison plan

Nov 20, 2026 · @Tyler Jensen

#### Summary

Build a second Arbiter execution backend on Candle behind the existing `Scorer` facade, on a separate branch, and score it against the ONNX Runtime (ORT) results from the finished FP16 campaign on the same frozen inputs and the same machines.

- **Stays on ORT:** the bundled MiniLM INT8 embedder, CPU only. This plan does not touch it.
- **Moves in the spike:** Arbiter Focus (Qwen3-Reranker-0.6B) and Context Compact and Large (Qwen3-1.7B and 4B), on CPU and CUDA, Windows x64 and Linux x86\_64.
- **Decision it feeds:** adopt Candle for Arbiter, keep ORT, or stop. It is a comparison, not a commitment to switch.

The plan is based on a read of `feat/arbiter-final` at `afb0245`. Nothing here has been built or run.

#### Scope and constraints

**Entry condition.** The Windows and Linux CUDA FP16 campaign is closed, pass or recorded blocker, and its receipts are frozen. Branch from that commit (suggested name `spike/arbiter-candle`). Local commits only, same as the current branch rules.

**In scope**

- The three selected Qwen models, FP32 and FP16, on CPU and CUDA.
- The same compiler, tokenizer, `CompiledInput`, budgets, deadlines and device reservation as today.
- Spike profiles that are unqualified and evaluation-only, admitted through the existing `EvaluationCatalog` and `for_qualification` paths.

**Out of scope**

- MiniLM, macOS, Radeon, and any release qualification.
- Changes to wire shapes, digest inputs, persisted state or the release catalog.
- New processes, sidecars or a runtime found through an ambient DLL path.

**Repo rules that shape the design**

- No new crate without approval, so the backend is a feature-gated module inside `airk-retrieval` (suggested feature `arbiter-candle`).
- Mechanical restructuring and behavior changes go in separate commits.
- Model assets are read once as verified bytes. Native loading never reopens a path.

**Decisions needed from you before starting**

- [ ] Approve `candle-core` and `candle-nn` as workspace dependencies, pinned to an exact revision.
- [ ] Accept a CUDA toolkit as a build-time input for the `gpu-cuda` variant of the spike.
- [ ] Confirm the spike avoids Intel MKL, so the CPU numbers reflect a pure-Rust build.
- [ ] Set a time box for the spike.

#### Target design

Callers keep using `Scorer::open`, `compile` and `score_with_deadline`. Only what sits behind the session changes.

| Concern | Today on ORT | Candle spike |
| --- | --- | --- |
| Entry point | `Scorer` owns an ORT `Session` | Same `Scorer`, with an internal backend enum: ORT or Candle |
| Weights | Derived ONNX graphs plus external data, 44 parts on one GitHub prerelease | Publisher safetensors, hash-checked and loaded from the verified buffer |
| Model code | Inside the exported graph | Qwen3 forward pass in Rust: embedding, RMSNorm, grouped-query attention with RoPE, MLP |
| Readout | Baked into the graph: Focus `logits[1,2]`, Context `scores[1,N]` | Rust functions that implement the readout in the two export contracts |
| Input shapes | Focus graph is static `[1, 512]` | Dynamic; the profile's token ceiling is the only limit |
| FP16 | Separate exported graph with protected FP32 nodes | One dtype policy in code that mirrors the FP16 export contract |
| Long-context attention | ORT's fused CUDA attention kernels | Attention computed in query-row tiles, so no full `[heads, T, T]` tensor exists |
| Device custody | `CudaDevice` reservation, deadlines, settlement, poison handling | Kept as is |
| Placement proof | `placement.rs`, graph metadata and strict-placement gates | Not needed: every tensor is created on the named device |
| Native libraries | CUDA-enabled ORT, cuDNN, two provider DLLs under the held-file loader | The NVIDIA libraries Candle's CUDA layer loads, under the same loader rules |
| Runtime identity | `onnxruntime-cpu/1.30.0` | A new engine string and separate spike profile manifests |

The compiler, task policies, calibration assets, budgets and the service-side admission code are unchanged.

#### Implementation steps

Each step ends with a check that must pass before the next one starts. CPU FP32 comes first because it is the cheapest place to prove the model code is right.

1. **Dependency review.** Pin the Candle revision, run it through `deny.toml` and the supply-chain checks, and add its notices.
   - Exit: the workspace builds with the new feature off and nothing else changes.
2. **Backend seam.** Move the ORT session behind an internal backend enum inside `arbiter/runtime.rs`. This is a mechanical commit with no behavior change.
   - Exit: existing Arbiter tests pass unchanged with the feature off and on.
3. **Weight loading.** Add spike profile manifests that name the publisher safetensors with size and SHA-256. Load tensors from the verified in-memory bytes.
   - Exit: every expected tensor is found with the right shape and dtype for all three models.
4. **Qwen3 forward on CPU, FP32.** Write the forward pass at batch one. Use the Candle `qwen3.rs` revision already pinned in your research inventory as a reference, under the repo's attribution rules.
   - Exit: per-layer outputs match ORT CPU FP32 on the frozen 32-token case.
5. **Readouts.** Implement the Focus yes/no readout and the Context single-choice readout from the export contracts, including the option mask.
   - Exit: final scores match the retained ORT FP32 references on every frozen case within the 0.001 raw-score limit.
6. **Tiled attention.** Compute attention in query-row blocks so memory stays bounded at 8,192 tokens.
   - Exit: tiled and untiled results agree at 128 and 1,024 tokens, and the 8,192-token case runs inside the host cap.
7. **CUDA FP32.** Bind the Candle device through the existing `CudaDevice` reservation and deadline code. Put the NVIDIA libraries it loads under the held-file loader.
   - Exit: Compact and Large pass the four approved widths against the FP32 references on the A6000.
8. **FP16 policy.** Store projection and embedding weights as half. Keep residual, RMSNorm, RoPE, activation and readout in FP32, as the export contract does.
   - Exit: the run completes and settles at all widths, and parity is recorded against the same limits the campaign used.
9. **Linux build.** Build and run the same code on the WSL2 instance.
   - Exit: steps 7 and 8 repeat on Linux CUDA.
10. **Harness adapter.** Let the existing qualification harness select the backend and write the backend identity into each receipt.
    - Exit: one command produces ORT and Candle receipts for the same frozen input.

Prefix KV reuse and GGUF-quantized weights are follow-on experiments. Run them only after the like-for-like numbers exist, because both change model or execution identity.

#### Comparison test steps

The comparison is only fair if both backends see identical inputs on identical hardware, so the first two steps fix the baseline before any Candle number is taken.

1. **Freeze the ORT baseline.** Tag the campaign's final commit. List the receipts to compare against, with their input digests: Focus, Compact and Large, FP32 and FP16, CPU and CUDA, Windows and Linux.
2. **Record the test bed.** Capture GPU, driver, CUDA toolkit, OS build, Candle revision and the spike commit. Use the A6000 workstation and the same WSL2 instance as the campaign.
3. **CPU numerical parity.** Run Candle CPU FP32 against the ORT CPU FP32 references on the frozen cases: `diagnostic-all-live-32`, `all-live-1` at 128 tokens, `native-all-live-1024` and `native-all-live-8192`.
4. **Windows CUDA, FP32 then FP16.** Run one process at a time with exclusive use of the GPU, at the approved widths of 32, 128, 1,024 and 8,192 tokens. Keep the campaign's host caps: 32 GiB for Compact, 48 GiB for Large.
5. **Linux CUDA, FP16.** Repeat step 4 on WSL2 for the precision the campaign qualified there.
6. **CPU fallback.** Time Focus and Compact on CPU at the admitted thread fit, on both backends. This matters because automatic CPU fallback is a product requirement.
7. **Cold and warm timing.** For every configuration, take cold load in fresh processes and repeated warm scores in one loaded session. Use the same repetition counts the campaign used, and report p50 and p95.
8. **Composed path.** Run the composed Answer scenarios through the service with the Candle backend selected. Compare end-to-end latency with the campaign's figures.
9. **Judged agreement.** Score the campaign's judged cohort on both backends and compute paired agreement.
10. **Engineering measures.** Record clean build time, executable size, the count of native libraries shipped or loaded, download bytes per model, and lines of code removed if ORT's CUDA path were deleted.
11. **Write up.** Put ORT and Candle side by side for every row in the next section, with receipts under their own identities. No Candle result is relabelled as qualification evidence.

#### Comparative data and decision criteria

Candle is worth adopting only if it matches ORT on correctness and stays close on speed and memory. The numerical bars are the ones already in `final-plan.md`. The two marked "proposed" are my suggestions and need your approval.

| Measure | Recorded for each backend | Bar for Candle |
| --- | --- | --- |
| Raw score error against the FP32 reference | Maximum absolute error per frozen case and width | 0.001 or less for qualification; 0.010 is the development screen |
| Normalized mass drift | Against the matching reference and original FP32 | 0.05 or less |
| Winner and order | Top choice and full order per case; top-two margin | Same winner on every case; margins of 0.020 or less flagged as near ties |
| Judged agreement | Paired agreement on the judged cohort | 98% or more |
| Warm latency | p50 and p95 per model, precision and width | Proposed: within 15% of ORT |
| End-to-end Answer latency | p95 through the service | 120 seconds or less |
| Cold load | Seconds from open to first score | Reported; no bar |
| Peak device memory | Per model and width | Inside the campaign's device budget for that model |
| Peak host memory | Working set per process | Inside the 32 GiB Compact and 48 GiB Large caps |
| CPU fallback latency | Focus and Compact on CPU | Proposed: within 25% of ORT |
| Engineering | Build time, binary size, native libraries, download bytes | Reported; no bar |

**Reading the result**

- **Adopt:** Candle meets every numerical bar and both proposed latency bars on Windows and Linux CUDA and on CPU.
- **Keep ORT:** Candle misses numerical parity, or CPU fallback is much slower with no fix in sight.
- **Informative either way:** if Candle FP16 misses 0.001 by a margin similar to ORT's, the limit is probably a property of half-precision arithmetic on cuBLAS, not of ORT's attention kernel. That would bear on the gate itself.

#### Risks and open questions

Radeon is the largest risk: Candle's GPU backends are CUDA and Metal, so adopting it removes the DirectML and ROCm stages in `final-plan.md` unless ORT is kept for them. Keeping both would mean two Arbiter backends to qualify.

| Risk | Why it matters | How the spike answers it |
| --- | --- | --- |
| CPU fallback speed | Candle has no graph optimizer, and its CPU speed on these models is unmeasured | Test step 6, early, before CUDA work |
| FP16 parity fails again | CPU and cuBLAS round differently whatever the framework | Implementation step 8, judged by the same limits |
| Memory at 8,192 tokens | Plain attention would materialize the score tensor the plan avoids | Implementation step 6; Candle's flash-attention kernels are a fallback on Linux |
| Windows CUDA build | Candle's CUDA layer needs a toolkit at build time, and its runtime library loading must meet the no-ambient-path rule | Implementation step 7; unverified until tried |
| Evidence reset | Every existing receipt is bound to an ORT graph identity | Accepted cost; the spike produces new receipts, not replacements |
| Two runtimes in one binary | MiniLM keeps ORT CPU, so the static ORT build remains | Measure binary size and build time in test step 10 |
| Readout fidelity | The exact readout is defined by the export contracts, which I have not traced line by line | Implementation step 5 gates on the retained references |

**Open questions**

- Do the publisher's licenses and your notice rules allow loading their safetensors directly, with Hugging Face as the primary source?
- Should a failed CPU fallback result end the spike, or is a CUDA-only Candle backend with ORT on CPU acceptable?
- Is the Radeon opt-in important enough that a Candle result could not change the decision anyway? If so, the spike is not worth running.
