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
