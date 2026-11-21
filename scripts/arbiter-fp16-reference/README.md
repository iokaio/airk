# Independent Context numerical references

The CPU tools in this directory generate matching-recipe score vectors from the original model weights and native-compiled request inputs. They retain the original publisher FP32 scores separately. Each result identifies the input, model, math recipe and bounded worker that produced it. For original Large TRAIN publisher comparisons, `large_train_case.py` retains the embedding and final normalization and streams one authenticated FP32 decoder layer at a time through the original publisher forward. `check_large_publisher_chunks.py` compares BF16 promotion and two complete small-model forwards against stock, fully resident PyTorch. `large_train_screen.py CAMPAIGN DESTINATION` freezes the original four-case TRAIN roster and its top-two margin selector before scoring. The `screen` phase of `run_large_t32.py` runs a T32 positive control followed by exactly those four publisher rows under the same 22 GiB and 600-second limits. These files support the [native execution tools](../arbiter-fp16-execution/README.md).

For Large, `run_large_t32.py` verifies the conversion plan and model closure before invoking the selected CPU recipe. `large_body_reference.py` retains the intermediate half roundings. `large_weight_only_reference.py` uses FP32 inputs and outputs for the 252 dense projections with half-stored weights and compensated attention. `large_roundback_panel.py` isolates the first layer's measured projection boundaries on the original operands.

Use the pinned CPU Torch environment and the checked same-user runner. See the [reference contract](CONTRACT.md) for original inputs, budgets and supported modes. Keep model assets and score receipts in the private campaign directory.

For the exposed TRAIN_Q543 question, large_train_case.py reopens the original seven ordered options and the native 1,588-token input. Run the publisher phase of run_large_t32.py first, then supply its sealed execution receipt to the matching phase. The publisher checks the known 32-token result before scoring this development question.

For 8192 tokens, run_large_t32.py can compute a contiguous layer range per bounded CPU Job. large_layer_job.py verifies the previous residual and the original graph and weights. large_direct_layers.py applies the same dense projections and attention math in model order. The 1024-token control uses layers 0 through 17 and 18 through 35, then compares all eight final score words to the uninterrupted result. Keep the per-layer hashes, timings and original execution receipts. A finished 8192-token CPU panel still needs an independent native CUDA comparison.

## Portable graph proof

`portable_graph.py` is the independent graph proof for exports made with the FP16 exporter's `portable` recipe, which keeps binary16 learned storage and computes projections and attention in FP32. `graph.py` remains the proof for the default recipe. The [reference contract](CONTRACT.md#portable-recipe-graph-proof) states the policy and its limits.

For Compact or Large, run the proof under the checked same-user shell with an interpreter that provides `onnx`:

```powershell
python -I -B scripts/arbiter-fp16-reference/verify_portable.py `
  --model compact `
  --original $originalGraph `
  --original-pins $originalPins --original-pins-sha256 $originalPinsSha `
  --graph $portableGraph --graph-sha256 $portableGraphSha `
  --publisher $publisherAssetRoot
```

`--original` is the authenticated original FP32 `model.onnx` with its external data beside it. `--original-pins` is a JSON map from each external file name to its `sha256` and `bytes`, and the run requires that file's reviewed digest. `--graph` is the portable `model.onnx` with its reviewed digest. `--publisher` is the publisher asset root that holds `research-asset-pins.json`. The run hashes the original external file twice and the publisher release once. It opens no candidate weights and writes no files. On success it prints one JSON line with the input identities, the three verifier source digests and the proof. A refusal exits nonzero and names the failed check. Retain stdout in the private campaign directory.

Run `python -I -B scripts/arbiter-fp16-reference/test_portable_graph.py` for the literal Context and Focus controls. They need `onnx` and use no exporter, model, Torch or GPU.
