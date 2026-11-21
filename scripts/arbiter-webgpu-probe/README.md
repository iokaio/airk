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
- `--provider`: `webgpu` (default) or `cpu`.
- `--option key=value`: a provider option, repeated as needed.
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
