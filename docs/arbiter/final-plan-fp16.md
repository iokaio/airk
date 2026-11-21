# AIRK FP16 execution analysis plan

Prepared 20 November 2026, America/Denver.

This is the review plan for the FP16-only CPU and GPU delivery decision. The
deliverable is an evidence-backed choice of execution recipe for each candidate
and provider, with the cheapest useful checks first. Review the analysis and
any proposed precision exceptions before changing exporters, runtimes, catalog
entries, or fallback behavior. The existing release gates and platform order
apply. [Release precision decision](final-plan.md#fp16-release-precision-decision-20-november-2026-americadenver)

## Starting evidence

AIRK's exporter converts the selected large tensors from the existing FP32
export offline. It writes FP16 weight bytes and records both source and output
dtypes, byte ranges, and hashes. At runtime ONNX Runtime loads the converted
files and selects kernels for the optimized graph and execution provider. The
weight dtype, graph tensor dtype, and kernel arithmetic each need evidence.
[Exporter](../../scripts/arbiter-fp16-export.py) ·
[ONNX Runtime float16 conversion](https://onnxruntime.ai/docs/performance/model-optimizations/float16.html)

| Evidence | What it establishes | Open question |
| --- | --- | --- |
| Windows CPU literal `MatMul` and `Softmax` | The pinned ORT 1.30.0 Python build inserted FP32 casts in both optimized graphs, with zero inference forwards. | Whether the exact static AIRK CPU build and target CPU can execute the required operations with half operands and activations. |
| Windows Compact GPU calibration | The saved panel has 100 complete score vectors from the protected scientific graph. Its weight file is FP16, and its staged graph records FP32 projection and attention interfaces. | Whether the final Compact recipe meets the agreed execution precision and whether that recipe can reuse these scores. |
| Windows Large GPU development | The direct scientific graph reused the existing FP16 weight sidecar and kept FP32 projection and attention intermediates. The 24 GiB pressure run observed shared GPU memory. | Whether a release recipe meets the execution precision, numerical and dedicated-memory gates together. |
| Completed FP32 baseline | Retained results have their original artifact and workload identities. | Which final FP16 cases have exact comparable inputs and enough applicable human judgments. |

[CPU placement checkpoint](../agent-current-work.md#windows-cpu-fp16-operator-placement-20-november-2026-at-548-pm-denver-time) ·
[Compact and Large receipts](../simplify/one-process-ledger.md) ·
[Compact staged graph](../../scripts/arbiter-fp16-execution/stage_compact_cutlass_projection.py)

## Step 1 — Write the precision acceptance table

For each model and provider, record five fields independently.

1. **Stored weights.** Record dtype, bytes, and digest of each external weight
   file and the initializer ranges it serves.
2. **Operands and intermediates.** Record the optimized tensor types at learned
   projections, attention, normalization, residuals, and the scoring head.
   Include provider-inserted and exporter-inserted casts.
3. **Kernel arithmetic.** Record the dispatched kernel, device, operand
   precision, accumulation precision, and output rounding. Distinguish FP16
   operands with a wider accumulator inside a fused kernel from a separate
   FP32 graph tensor, a widened operation, or TF32 multiplication.
   [cuBLAS compute and tensor types](https://docs.nvidia.com/cuda/cublas/index.html#cublasgemmex)
4. **Interface and control tensors.** Record token and position IDs, masks, shape
   data, and the score output ABI. The existing plan calls for an FP32 score
   interface. Review its conversion boundary separately from internal inference.
   [G5 requirements](final-plan.md#g5--fp16-with-efficient-attention-after-fp32-works)
5. **Execution placement.** Record where each floating operation and fallback
   runs. An FP16 graph boundary or provider name alone leaves the arithmetic
   field unverified.

Use FP16 weights and FP16 operands and activations for the main inference path
as the proposed analysis criterion. Document any proposed wider accumulation
or protected operation by name and reason. Ask for a precision decision at the
analysis review if the candidate needs an exception, including a distinction
between allowing wider accumulation inside a half kernel and requiring FP16
arithmetic throughout. Treat silent widening and FP32 model fallback as failed
FP16 execution checks. Preserve the precision decision for explicit CPU, GPU,
and automatic CPU fallback.

Use Context Compact as the first small feasibility case. Track Context Large
and Focus separately, including the approved 1,024-token static Focus graph.
Record the bundled MiniLM encoder and its existing INT8 identity as a separate
scope item for review before touching its artifact or index compatibility.
[Model and graph scope](../agent-current-work.md#models-and-qualification)

**Output.** A per-stage precision table with documented boundaries, proposed
exceptions, and any unresolved scope decision.

## Step 2 — Freeze artifacts and recover comparable evidence

1. Inventory the exact source export, converted weights, graph, tokenizer and
   compiler, runtime version and build, provider settings, model profile, and
   existing calibration for each candidate. Record their hashes and target
   hardware. Keep the optimized graph tied to the runtime that produced it.
2. Map retained CPU placement, CUDA kernel, calibration, and memory receipts to
   those identities. Mark each result as a literal control, development model
   run, calibration input, or installed qualification.
3. Inventory the completed FP32 comparison inputs, complete score vectors, and
   judged evidence. Join by exact workload, source and judgments, compiler
   inputs, and ordered candidates. Reserve those results for comparison with
   the final selected FP16 artifacts. Record uncovered rows as evidence gaps
   with their effect on the applicable gate.

**Output.** A candidate and receipt matrix that identifies reusable evidence
and the smallest missing check for each row.

## Step 3 — Resolve Windows CPU feasibility before a model campaign

1. Use the existing verified static Windows AIRK ORT build first. Record the
   target CPU and its reported instruction support, then inspect build flags,
   linked runtime identity, and the registered CPU kernels. ORT 1.30.0 gates
   FP16 dense-kernel registration on MLAS acceleration support. Check that
   actual predicate on the target machine and keep results scoped to the
   build, hardware, and operating system. Do the equivalent capability
   inventory for Linux CPU independently where the current assignment permits.
   [ORT 1.30.0 CPU kernel registration](https://raw.githubusercontent.com/microsoft/onnxruntime/v1.30.0/onnxruntime/core/providers/cpu/cpu_execution_provider.cc#L3688-L3733)
2. Reuse the literal half-precision `MatMul` and `Softmax` controls with the
   exact native runtime. Add a literal `Gemm`, the candidate's attention
   variant, or normalization only where the candidate uses it and the kernel
   evidence is missing. Verify optimized node and tensor types before a
   forward. If a required operation silently widens, capture the failing
   node, registration or optimizer rule, and stop that recipe at the
   smallest reproducible case.
   [Existing CPU diagnostic](../../scripts/arbiter-fp16-execution/diagnose_cpu_fp16_placement.py)
3. After placement passes, use known FP16 inputs to verify one synchronized
   native execution per needed operation. Confirm actual dispatch, operands,
   accumulation and output types, finite results, and rounding against the
   independent FP16 recipe. Then test a small block taken from the real model
   to catch graph fusions and casts that appear only after optimization.
4. If the static build cannot serve the required operations, classify the
   cause as build configuration, missing target hardware capability, or
   missing kernel coverage. Estimate and rank the smallest supported route
   for each class. A build correction needs proof that the affected target
   can use the half kernel. A changed hardware floor, new kernel or runtime,
   or proposed precision exception needs a documented review decision.

**Output.** A CPU feasibility result per target, with the first failing
operation or an exact native half-kernel proof. Schedule broader CPU
qualification only after the chosen route passes this check.

## Step 4 — Resolve Windows CUDA precision and memory fit

1. Use existing Compact and Large graphs as named scientific controls. Read
   their original and optimized graph types and the retained kernel evidence.
   Trace a projection and attention block from FP16 initializer through
   operands, multiply, accumulation, and output. Record each graph-wide
   FP32 activation path and any TF32 kernel. Verify provider placement and
   bounded host work from actual dispatch.
2. For the first candidate that matches the proposed precision table, inspect
   one short and one long-context native probe. Verify efficient-attention
   dispatch, masks and positions, and actual tensor precision. Use the
   existing Compact, Large, and Focus input limits and their separate recipes.
   Record the complete candidate identity before collecting scores.
3. Check numerics on the existing Context development widths of 32, 128,
   1,024, and 8,192 tokens before full calibration. Include available near
   ties and consequential reversals. If half rounding causes a failure,
   isolate the first divergent operation on the smallest case. Assess narrow
   alternatives before proposing any protected wider operation.
4. Measure device fit at the existing applicable tiers and host caps. Separate
   cold load, steady-state, dedicated, shared, and nonlocal memory. Reconcile
   the 24 GiB shared-memory result using the existing graph-only control and
   allocation or forward-window evidence where available. Apply the original
   dedicated-memory rule to the observed result.
   [Resource procedure](../../scripts/cuda-component-execution/CAPACITY-SWEEP.md) ·
   [Windows limits](../agent-current-work.md#models-and-qualification)

**Output.** A Windows CUDA precision and fit report per candidate. Keep the
100 Compact scientific vectors available. Reuse them after reviewing exact
artifact, arithmetic, compiler input, and card order equivalence.

## Step 5 — Preflight qualification and fallback for a viable recipe

1. Verify the future installed path through the native catalog and runtime.
   Require exact identity for explicit CPU, explicit GPU, and automatic CPU
   fallback. Exercise fallback with GPU disabled, absent, or outside the
   admitted budget. On CPU, match the selected model to its independently
   qualified FP16 CPU profile. Surface the existing unavailable result when
   a qualifying path is absent. Record any needed catalog or persisted
   contract change for review.
   [Native device and profile contract](../../crates/airk-core/src/arbiter/CONTRACT.md)
2. Check that the retained reference and label denominators support a valid
   conclusion before starting full calibration. A distinct precision recipe
   requires an independent matching-FP16 reference. Use the completed FP32
   baseline only in the final comparison. For Windows/Linux Context CUDA,
   retain maximum absolute raw error of 0.007, normalized-mass error of 0.05
   and identical argmax against the matching recipe. Preserve Focus and other
   provider-specific rules in the parent plan.
   [Numerical limits](final-plan.md#numerical-limits-and-calibration-by-provider)
3. Plan exact-artifact calibration and a disjoint held-out comparison. For
   Context, the lower one-sided 95% paired confidence bound on
   `FP16 evidence success − 0.98 × FP32 evidence success` must be at least
   zero. Freeze the applicable human judgments, full ordered inputs, and long-context
   reporting before evaluating the final artifacts. Keep uncovered families,
   precision-induced decisions, abstention and coverage visible.
   [Accepted Context task-quality rule](final-plan.md#accepted-context-fp16-task-quality-qualification)
4. Inventory the installed end-to-end workload and measure against the
   existing 8,192-token budget and 120-second request and warm p95 targets.
   Measure the model, the complete Answer path, and automatic fallback
   separately. Reuse installed evidence only for the exact qualified
   identity and target.
5. Keep Windows CUDA first, followed by Linux CUDA on the existing WSL2
   instance. CPU feasibility can progress independently. Retain the existing
   order and authorization for macOS Core ML and later Radeon work.
   [Current platform order and authority](../agent-current-work.md#authority-and-deadline)

**Output.** A qualification matrix showing which tests can use retained
evidence, which still require the final artifact, and which gates have missing
inputs. Include measured or bounded runtime per next check.

## Step 6 — Bring the analysis to review

Provide one row per model, provider, and target with the candidate identity,
storage dtype, actual arithmetic and placement, first blocking check if any,
reusable evidence, and next action. Record separately any tradeoff requiring a
decision, such as allowing wider accumulation, changing the supported hardware
floor, or undertaking new kernel or runtime work.

Recommend a minimal implementation scope from that matrix. The first
schedule checkpoint is a native Windows CPU operation and model-block proof
together with the Windows CUDA precision trace. Use its measured costs and
the remaining qualification matrix for a delivery estimate. If no route
meets the requested precision and existing release gates, report the exact
blockers at review before proceeding.
