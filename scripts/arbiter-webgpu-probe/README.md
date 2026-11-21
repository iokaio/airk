# Arbiter WebGPU probe

`smoke.py` runs a retained Context graph through an ONNX Runtime provider and
compares the scores with the original FP32 scores kept in a retained reference
receipt. It answers three early questions about the WebGPU route in the
[FP16 delivery plan](../../docs/arbiter/final-plan-fp16.md): whether the
provider starts and places the graph on the GPU, whether its scores agree
with the reference, and how its latency compares with CUDA on the same card.

It is a smoke test. It qualifies nothing, it measures one process on one
adapter, and its memory figure is whole-adapter usage from `nvidia-smi`, which
includes every other process on the card.

## Inputs

- `--model`: the ONNX graph. The six inputs are `input_ids`, `attention_mask`,
  `position_ids`, `decision_positions`, `candidate_positions` and
  `option_mask`, and the output is `scores`.
- `--external-data-dir`: the folder that holds the graph's `model.onnx.data`
  when it is not beside the graph.
- `--case`: one retained reference receipt per case, repeated as needed. The
  script reads `result.compiled` for the inputs and
  `result.original_binding.original_fp32` for the reference scores.
- `--fixture`: a retained native numeric fixture. Every row becomes a case,
  with `original_fp32_all_live` as its reference scores.
- `--inputs-only`: a retained reference receipt used for its inputs alone,
  for a model the receipt's scores do not belong to. The case records scores
  and timing and makes no comparison.
- `--provider`: `webgpu` (default) or `cpu`.
- `--option key=value`: a provider option, repeated as needed.
- `--session-config key=value`: a session configuration entry, repeated as
  needed.
- `--intra-threads`: the intra-op thread count, for CPU runs.
- `--warm`: warm forwards per case after the first one. The default is 3.
- `--no-profile`: skip the runtime profile and the placement count.
- `--output`: a fresh, empty directory.

Run it in an environment that has `onnxruntime`, `numpy` and, for WebGPU, the
`onnxruntime-ep-webgpu` plugin package. Record the wheel hashes you installed.

## Outputs

`smoke.json` in the output directory, plus the runtime profile unless
disabled. The receipt records the model and weight digests, the plugin
library digest and adapter, load time, per-case first and warm forward times,
scores, the maximum raw difference from the reference, winner agreement, the
kernels executed per provider and operator, and adapter memory before load,
after load and at peak. A one-line summary is printed.

## Use

```console
python -I -B scripts/arbiter-webgpu-probe/smoke.py --model <graph.onnx> --external-data-dir <weights-dir> --case <t32-receipt.json> --case <t8192-receipt.json> --output <fresh-dir>
```

Keep the output under the private campaign root. Do not commit it.

## Probe variants

`variant.py` derives a probe variant of a retained attention graph, to
measure what a change does to memory before the maintained exporter adopts
it. It is not the product graph.

- `--drop-attention-bias` removes the explicit bias input from every causal
  `GroupQueryAttention` node and prunes the nodes that only produced it. The
  kernel then applies causality itself. This is valid only for inputs with no
  padding, which is how Context inputs are compiled.
- `--half-weights` stores each large FP32 matrix weight and the embedding as
  FP16 behind a cast, copies every other tensor unchanged, and makes the
  variant folder self-contained. A non-finite source value or an overflow in
  the conversion is refused.

```console
python -I -B scripts/arbiter-webgpu-probe/variant.py --source <graph.onnx> --source-data <model.onnx.data> --output <fresh-dir> --drop-attention-bias --half-weights
```

The runtime expands half weights again at load unless two passes are
disabled. Pass this to `smoke.py` to keep them resident in half:

```console
--session-config "optimization.disable_specified_optimizers=ConstantFolding;FuseFp16InitializerToFp32NodeTransformer"
```

On WebGPU, `--option storageBufferCacheMode=simple` roughly halved the peak
in the first measurements. The default mode and `lazyRelease` were worse.
