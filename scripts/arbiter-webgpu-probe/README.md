# Arbiter WebGPU probe

`smoke.py` runs a retained Context graph through an ONNX Runtime provider and
compares the scores with the original FP32 scores kept in a retained reference
receipt. It answers three early questions about the WebGPU route in the
[Arbiter plan](../../docs/arbiter/final-plan-fp16.md#10-engineering-state-carried-forward): whether the
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
- `--pad-to`, `--pad-token-id` and `--unpadded-too`: see
  [Padding to a length ladder](#padding-to-a-length-ladder). A `--case` or
  `--inputs-only` path may end in `@N` to pad that one case to exactly `N`
  tokens.
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
Padding also needs `onnx`.

When the WebGPU provider fails to start, for example on an unknown value of a
provider option, the runtime opens the session on the CPU provider instead and
reports only a warning. `smoke.py` and `panel.py` refuse a WebGPU run whose
session did not bind the plugin, so such a run leaves no receipt.

## Outputs

`smoke.json` in the output directory, plus the runtime profile unless
disabled. The receipt records the model and weight digests, the plugin
library digest and adapter, load time, per-case first and warm forward times,
scores, the maximum raw difference from the reference, winner agreement, the
kernels executed per provider and operator, and adapter memory before load,
after load and at peak. Each case also records adapter memory after it and
the peak during it, which counts the reading taken after it. A one-line
summary is printed.

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
In that mode the provider keeps a separate set of buffers for every token
count it has seen and never releases one, so memory grows with the number of
distinct input lengths. Padding bounds that number.

## Padding to a length ladder

`smoke.py` and `panel.py` can right-pad every input to one of a few fixed
token counts, so that a session sees only those lengths.

- `--pad-to N`, repeated as needed, lists the lengths. Each case or row is
  padded to the smallest listed length that is at least its token count. One
  that is longer than every listed length is refused before the session
  opens.
- `--pad-token-id` is the token id written into the pad positions. It is
  required when anything is padded. The retained `tokenizer.json` files
  declare no padding entry, so the id is not read from them. The
  measurements so far used 151643, the `<|endoftext|>` entry of those files.
- In `smoke.py`, a `--case` or `--inputs-only` path that ends in `@N` pads
  that case to exactly `N` tokens instead of its ladder length. Giving the
  same receipt twice this way puts one input on two rungs of a ladder.
- `--unpadded-too`, in `smoke.py`, runs each padded case unpadded first, with
  the same number of forwards, and records the unpadded times and scores and
  the largest difference between the padded and unpadded scores. The extra
  input length adds its own buffers, so leave it out of a memory measurement.

A pad position takes the pad id and an attention mask of zero, and its
position id continues the sequence. The decision position and the candidates
are unchanged, so the graph still scores the original last token.

Padding is applied only to a graph whose structure supports it, which the
scripts read with `onnx` before the session opens: the attention mask feeds
nothing but `Shape` nodes, and every attention node is a causal
`GroupQueryAttention` with no past state and no explicit bias. Then the
mask's values are never read and no position sees a later one. Any other
graph is refused. This is a structural reading, not a proof, so compare
padded scores with unpadded ones on each provider as well.

The receipt gains a `padding` block with the lengths, the pad id, the mask
value, the position rule and the graph reading. Each case or row keeps its
original token count in `tokens` and adds `padded_tokens`. `smoke.py` also
records `padded_by`, which is `ladder` or `exact`. `panel.py` adds the number
of rows at each padded length. Bands and reference checks use the original
token count and input digest, so a padded receipt is a valid `--reference`
for the same rows.

```console
python -I -B scripts/arbiter-webgpu-probe/smoke.py --model <graph.onnx> --case <t32-receipt.json> --case <t1588-receipt.json> --case <t1588-receipt.json>@4096 --pad-to 2048 --pad-to 8192 --pad-token-id 151643 --output <fresh-dir>
```

## Panel drift

`panel.py` scores every row of a compiled panel on one provider and measures
how far the scores are from named reference scores. It was written for the
drift check of the earlier FP16 delivery plan, which compared a portable
graph with reference scores across a frozen panel. That acceptance approach
was retired on 21 November 2026. The script remains a numerical comparison
tool for any panel of compiled inputs. It is a development measurement. It
applies no limit and qualifies nothing.

Inputs:

- `--model`, `--provider`, `--option`, `--session-config` and
  `--intra-threads`, as for `smoke.py`. Provider options apply to WebGPU
  only.
- `--weights`: a weight file the graph reads, repeated as needed, so that the
  receipt records its digest.
- `--panel`: the compiled rows. Each row has an `ordinal`, an `id` and
  `compiled` with the six inputs. A row that masks an option is refused.
- `--reference NAME=PATH`: a named reference score set, repeated as needed.
  The file is a retained raw panel or an earlier `panel.json` of this script.
  The first one is the primary reference. A reference row must carry the
  panel row's id and, where it records one, the row's input digest.
- `--comparison`: a retained paired CPU comparison, repeated as needed. Each
  one adds its rows to the `matching_cpu` and `publisher_cpu` sets. Its
  candidate scores must equal the primary reference's scores, which is what
  ties it to a panel row.
- `--band-edge`: the inclusive upper token bound of a band, repeated in
  increasing order. The default bands end at 32, 128, 1,024, 2,048 and 8,192
  tokens.
- `--margin`: a winner margin to count rows against, repeated as needed. The
  defaults are 0.002 and 0.014.
- `--first` and `--count`: a slice of the panel, for a short trial.
- `--warm`: warm forwards per row after the first one. The default is 0. The
  scores come from the last forward.
- `--pad-to` and `--pad-token-id`: see
  [Padding to a length ladder](#padding-to-a-length-ladder).
- `--output`: a fresh, empty directory.

Outputs: `panel.json` in the output directory. One line per row goes to
standard error while the run proceeds, and a one-line summary is printed at
the end. The receipt records the digests of the graph, the weights, the panel
and every reference file. For each row it holds the token count, the forward
times, the scores, the winner and its margin over the second score. For each
reference set the row has, it holds the reference scores, the maximum
absolute difference, winner agreement, the reference's top-two margin and
whether that margin is below twice the difference. With warm forwards it also
holds the largest difference between forwards of the same row.

The summary gives, for each reference set, the maximum, median and
nearest-rank 95th percentile of the per-row differences, every row whose
winner changed, every row whose reference margin is below twice its
difference, the same figures by token band, and the count of rows at each
margin. The margin counts are a proxy for the cost of a guard band. No fitted
threshold enters them.

Timing covers the session load, every forward and the whole run, with the
spread of the first forwards and, when there are any, of the warm forwards.
On WebGPU the receipt adds whole-adapter memory at baseline, after load,
after every row and at peak, which shows whether memory grows as token counts
vary.

```console
python -I -B scripts/arbiter-webgpu-probe/panel.py --model <graph.onnx> --weights <weights.bin> --panel <compact.json> --reference calibrated=<raw-panel.json> --comparison <paired-comparison.json> --provider cpu --intra-threads 8 --output <fresh-dir>
```

## Tests

`test_smoke.py`, `test_panel.py` and `test_variant.py` cover the pure
helpers, including padding, the graph reading and the shared session set-up.
They need no model, provider or device, and their expected values are written
by hand.

```console
python -I -B -m unittest discover -s scripts/arbiter-webgpu-probe -p "test_*.py"
```
