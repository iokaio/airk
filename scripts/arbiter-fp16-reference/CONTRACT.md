# Independent Arbiter FP16 reference and candidate graph proof

This maintained research owner integrates frozen independent half-reference R2 (7e1a41a657159fda574b866845deb319fb12f4ad9966ba80c8f4109b145b0f9e). The reference module exposes a library API. The Large CPU launcher below uses that API. Creating or testing this source authorizes no checkpoint read, model forward, native/GPU operation, export, training, qualification or activation.

The public candidate verifier requires the authenticated original full-FP32 graph (Focus 19b20ad4, Compact c79c6725, Large f8ba5d3e), its complete closed external-file pins, independently authenticated publisher configuration/reference, and the actual native-pinned candidate graph. It never imports the exporter or trusts its report. Original graph and external-file authentication is repeated after validation; caller retains the native source/file/process custody and original deadline. Candidate learned bytes are checked independently against publisher-derived RNE half/protected FP32 bytes after the complete graph proof succeeds. The private byte-only helper is not an acceptance API.

The independent structural gate derives learned roles from original parameter consumers/configuration and compares the complete output-reachable DAG. Only learned half storage/Gather/GEMM cast boundaries and the explicitly enumerated causal efficient-attention replacement may change. Every surviving original mathematical node, tensor Constant, attribute, edge, ABI and opset is compared; unknown, extra, disconnected or duplicated nodes/tensors, functions, subgraphs, alternate precision, changed protected paths and unknown transformation patterns refuse. Constant external storage may become identical inline bytes. Learned storage relocation is allowed only with exact shape/dtype, complete native file closure and the independent publisher byte comparison. Protected norm scales and Focus head remain FP32; Context sparse readout uses the same half embedding then FP32 rows, never a duplicate full embedding.

Context uses causal GQA with the original scale, Q/K post-RoPE, V projection, consecutive grouped heads, the exact original sink and all-live lengths T-1/T. GQA has no attention bias or past key/value inputs. Slots 1 and 2 produce the exact layer-local `present_key` and `present_value` values required by the pinned CUDA operator. These values have no consumers and the public graph output remains the original FP32 `scores`. Focus uses causal MHA with consecutive KV repetition, packed [live,pad,T,pad,T] mask and Where of original live queries to literal half zero before the FP32 attention output. All layout, shape/axis constants, Cast targets/inputs/outputs and consumers are checked. Original residual/RMSNorm/QK norm/RoPE/SiLU/gating/final readout and mask/position/candidate semantics remain exact under this declared precision recipe. No dense Softmax or quadratic mask computation may survive the replacement. This is a bounded supported transformation gate, not a general ONNX equivalence engine.

The default independent forward computes causal attention with FP32 softmax over half inputs, followed by a half output. Context also has an explicit CPU-only diagnostic via HalfReference.forward with context_flash_single_tile=True. It requires all-live input of at most 32 tokens and head width 128. It follows the pinned ORT CUDA FlashAttention single-key-tile arithmetic. Compute FP32 QK scores and row maxima, multiply the original FP32 scale by log2(e) with FP32 rounding, and form the max-shifted exp2 arguments with the rounded scaled maximum and a fused multiply-subtract. Sum the unrounded exponentials in FP32, round the same unnormalized exponentials to nearest-even FP16 for the PV products, accumulate the PV products in FP32 and divide by the FP32 sum before the final nearest-even FP16 output. Torch CPU arithmetic supplies the reference, and GPU instruction approximations and reduction order can differ.

The CPU layer-zero diagnostic calls HalfReference.layer0_operands(row) with
the caller-authenticated native input. It requires an all-live Context T32 row
and the Compact 16-query-head, eight-KV-head, head-width-128 geometry. It shares
the ordinary forward's embedding, input RMSNorm, half projections, Q/K RMSNorm,
RoPE and final narrowing. It returns contiguous FP16 CPU Q, K and V in
[1, heads, 32, 128] order before attention. The caller authenticates the original
publisher configuration and the frozen native input identity.

The layer-zero projection comparison calls
HalfReference.layer0_projection_operands(row, native_half_input) with the same
caller-authenticated Compact T32 row and a finite CPU FP16 [1, 32, 2048] capture
of the shared normalized and narrowed projection input. It returns the
independent shared input from embedding and RMSNorm, then raw K, V and Q from
the supplied captured input, in that order. The four contiguous CPU FP16 tensors
retain the native [1, 32, width] layout with widths 2048, 1024, 1024 and 2048.
Ordinary forward and this diagnostic use the same half projection weights and
linear operation. The caller retains the capture identity and comparison.

Original and candidate learned initializer byte proof, whole graph proof, independent half/original numerical comparisons, native strict CUDA placement, actual Flash/MEA dispatch and synchronized return, resource populations, original model deadlines, task/CAL and public admission are separate requirements. Source tests establish only the independent graph/rounding/reference mechanics on literal CPU fixtures. Existing numerical limits do not change. No exporter self-comparison, matching names/counts, metadata hash or requested provider option qualifies an execution.
Default namespace controls use a self-contained literal Scorer.text / ModuleList model with independent IEEE byte expectations and all embedding/norm/projection names. The genuine pinned publisher-wrapper witness remains only in frozen historical private R2 evidence. Maintained controls require declared Torch/ONNX dependencies, never an ignored campaign source path. Model header semantics and all graph roots are checked in addition to the complete DAG.

## Portable recipe graph proof

`portable_graph.py` proves a candidate graph exported with the exporter's `portable` recipe against the same authenticated originals. It applies the structural gate described above with a different policy. Learned roles come from the original parameter consumers and the publisher configuration. The expected graph is the authenticated original plus the policy written in the module. The complete output-reachable DAG, ABI, model header, opsets, initializer catalog, layout constants and retained value declarations must match it exactly. The module never imports the exporter or reads its report. `graph.py` is unchanged because execution owners pin its digest. It supplies the bounded primitives and the original authentication.

The portable policy keeps learned storage in binary16 and all arithmetic in FP32. Each learned projection weight passes through exactly one Cast to FP32, `/airk_g5/weight/{node}/weight32`, which feeds the weight operand of its original MatMul. The activation operand, the product and the product's value name stay as they are in the original. Token and candidate rows are gathered from the half embedding and only the gathered rows are cast to FP32, as in the default recipe. The attention replacement has the default layout nodes and constants and no Q, K, V or output casts. Context GroupQueryAttention keeps the original scale, causal=1, do_rotary=0, empty past-key and past-value slots and lengths T-1 and T. It writes the FP32 result to the original sink value at slot 0. Slots 1 and 2 are the private `present_key` and `present_value` values without consumers. Focus MultiHeadAttention keeps the consecutive KV repetition and the packed mask and writes `raw32`. Where then selects the literal FP32 `/airk_g5/float_zero` for padded queries into the original sink value. Protected norm scales and the Focus head remain FP32. The default recipe's `half_zero` is absent.

Before the DAG comparison, a precision census checks the recipe on the candidate alone. No Softmax node exists. No `to` or `dtype` attribute targets binary16 and no tensor attribute holds it. The binary16 initializers are exactly the learned half roles. Each half projection weight has one consumer, a Cast to FP32 whose only consumer is one MatMul weight operand. The half embedding feeds only the data input of Gather, and each gathered value has one consumer, a Cast to FP32. Each layer has one attention operator. Context has seven inputs with empty past slots and three outputs. Focus has five inputs with an empty bias slot and one output. These counts leave no slot for a rotary cache, attention bias or past state, and the present values have no consumer. The literal controls also require every refused candidate graph to refuse with the census disabled.

`verify_portable_files` takes the same reference, original graph, closed external-file pins and candidate identity as the default gate. It authenticates the original before and after the comparison. `verify_portable.py` runs it for Compact or Large. The runner checks its pinned publisher owner source, the reviewed digest of the original pin file and the complete publisher release before the comparison. It passes an empty candidate file closure, so a candidate Constant left in external storage refuses.

This proof covers graph structure only. Each module refuses the other recipe's graph. `verify_actual_half_parameters` and its learned-byte comparison accept only the default recipe, so this directory has no independent publisher byte comparison for a portable candidate's learned ranges. The proof opens no candidate weights and establishes no numerical result, provider placement, weight residency, execution or qualification. Focus is covered by the library and its literal controls. The runner accepts Context models only.

## Windows Large CPU reference

Run `python -I -B -S scripts/arbiter-fp16-reference/prepare_large.py` under
the checked human shell. Retain stdout in a fresh private campaign file.
This metadata read authenticates the complete Qwen3-4B publisher release,
both independent FP32 fixture/producer pairs, and the four Large native CUDA
FP32 input identities. The 128-token digest differs from Compact. The weight
and attention-tile byte counts describe allocations in the recipe. Measure
process and Job peaks during an admitted run.

After the Compact development attention checks pass and Large has its own
reviewed, completed baseline FP16 conversion, invoke the T32 reference under
the checked human shell. Keep the final task-quality decision separate.

```powershell
python -I -B -S scripts/arbiter-fp16-reference/run_large_t32.py `
  --plan $largePlan --plan-sha256 $largePlanSha `
  --conversion $largeConversion --conversion-sha256 $largeConversionSha `
  --owned-output $freshLargeReferenceDirectory
```

Supply canonical absolute paths for the private conversion plan, the actual
conversion execution receipt and a fresh direct child of the campaign root.
Review the SHA-256 digests before invocation. The launcher authenticates the
conversion-time G5 source group and JUnit, the source and conversion receipts, process
settlement, graph, sidecar and original FP32 input. The Windows owner starts
the pinned interpreter suspended under the original 32 GiB process and Job
limits and 600-second clock. The helper allows two process slots for the
interpreter shim and requires one observed process for this base-Python
launch. The worker authenticates all three original BF16 shards, then reads
one indexed parameter at a time as logical FP32 to derive the independent
weights and digests. The Large profile has 398 parameters and shares the
embedding with the readout. The worker records each shard boundary, whole-graph
verification and forward completion. It disables CUDA, observes the actual Job limits, checks
the Large graph and learned bytes independently and returns one eight-score
T32 vector. Runtime, ownership, refusal and settlement evidence stay private.
The development graph with promoted projections has a separate arithmetic
recipe. Wider forwards require their own owned execution and review.

Run `python -I -B -S scripts/arbiter-fp16-reference/test_prepare_large.py`
for metadata controls and `python -I -B -S scripts/arbiter-fp16-reference/test_large_launch.py`
for the CPU launcher controls. The Windows suspension test uses a tensor-free
child to check the actual caps and natural settlement. Use the provisioned
Torch environment to run `test_large_publisher.py` for the dense-versus-streamed
source census and complete literal forward comparison.

For Windows SM86 Context Large, retain the original all-FP32 softmax/PV baseline.
The distinct `baseline-half-sm86-causal-flash-single-tile` T32 recipe uses the
committed CPU operand computation in `arbiter-fp16-reference.py`. For T128,
T1024 and T8192, `large_flash.py` follows the causal 64-by-64 SM86 H128
kernel, scanning keys right to left. QK uses half operands and FP32 accumulation.
Compute base-two exponentials in FP32, retain the original denominator and
narrow the unnormalized exponent tile to FP16 for PV. Rescale the previous
FP32 numerator and four denominator lane sums at each tile boundary, combine
the four sums by XOR-2 then XOR-1, normalize once and narrow the result to FP16.
CPU matrix products and exponential instructions can round differently from
the device. The CPU recipe supports the frozen all-live inputs only. These
mathematical choices apply before inspecting any full-model score.

The independent two-tile operand controls in `test_large_flash.py` cover causal
key count, denominator precision, exponent narrowing and online rescaling.
The provider source authority is the CU15 configured ORT source inventory
`e66d54941f57a37882d099c32f2087b5f6b391f592655fca192b493c347c8b9c`.
Its `flash_fwd_launch_template.h`, `flash_fwd_kernel.h`, `softmax.h`,
`kernel_traits.h` and `utils.h` own the selected block traits, iteration,
narrowing and normalization. The linked CUTLASS SM80 C-layout definition in
`mma_traits_sm80.hpp` has SHA-256
`6c39df1b65facd9322bfdf4a74dfb0bf97f1694892ac9b43a61e1590191a364e`.
Qualify each model, width and native execution identity separately.

`compare_large_flash.py --per-slot` joins the fixed native Large T32 score
to the original FP32, simple CPU FP16 and single-tile CPU Flash score vectors.
It checks shared graph, weights, numerical input and both owned execution
receipts before reporting signed errors. `large_projection_control.py` checks
the CPU first-layer K, V and Q projection words at frozen T32 token positions
0, 15 and 31 against exact signed fixed-point dot products with ties-to-even
rounding. The input activation uses the same FP32 RMS norm and FP16 cast as
the reference. Each consumed external tensor is matched to the independent
publisher digest. A native intermediate capture is needed to localize any
CPU-versus-CUDA discrepancy.
### Large first-layer projection diagnosis

The Windows `probe_large.py projection-t32` mode captures the original shared FP16 input and three first-layer projection outputs from a synchronized 32-token run. `large_layer0_capture.py` verifies the CUDA producers, shared consumer edges, row indices, element words and unchanged eight final scores. `large_projection_control.py` takes the pinned independent Large reference, the original native observation (`--capture` and `--capture-sha256`), and a fresh `--owned-log`. It compares three complete rows with exact integer dot products rounded to FP16. With `--gpu-treatment` in the assigned exclusive GPU slot, it also uses `large_projection_gemm.py` to replay the original and widened products on the same operands. The receipt reports counts and maxima for each route, host and Job observations, and natural settlement. This test measures one layer and keeps complete model scoring as a separate run.

For the Windows Large protected Q/K/V T32 experiment, `../arbiter-fp16-execution/large_qkv_candidate.py` takes a fresh approved output directory and creates a graph referring to the original external weights. `run_large_t32.py` uses `large_qkv_reference.py` for the separate matching reference. It widens the original half operands for all 108 projections, accumulates in FP32, narrows the output once and retains the original single-tile attention recipe. `test_large_qkv_reference.py` checks the complete route and rejects reversed model order. This single-tile reference covers the stated short input. The full-model measured outcome is recorded in the campaign ledger.

For an original Context Large CAL input, `large_chronological_reference.py::score(independent, row, check, widen_qkv=...)` accepts the authenticated CPU model, the unchanged native compiled fields and an absolute-deadline callback. It selects the historical single-tile operation through 32 tokens and the original 64-column reverse tile reduction for longer inputs through 8,192 tokens. Partial final tiles mask future columns before the reduction. It preserves 32 query heads sharing eight KV heads, the native final decision position and one to eight original ordered card symbols. The original Context ABI requires all input tokens to be live. Legal disabled output options stay masked. The original reference owns the final readout and finite vector checks.

The caller must bind the actual graph, weights, executed optimizer and measured attention dispatch at the requested shape before issuing a matching-reference result. Widening denotes exactly the prior 108 Q/K/V projections and original half output boundaries. Further projection or attention changes require independently verified math. The operand controls open no publisher weights and give no qualification to the failed Windows Large QKV graph. Run `test_large_chronological_reference.py` in the verified, CPU-only Torch environment to check final partial tiles, scaled softmax, grouped heads, roster widths and native refusals.
