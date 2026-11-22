# Arbiter FP16 export tools

Purpose: convert authenticated FP32 Arbiter graphs to the G5 FP16 recipe and
prepare the Focus and Context CUDA probe inputs of plan 12.5.2 (L5). The
exporter's rules are in [CONTRACT.md](CONTRACT.md). Nothing here runs a model,
touches a GPU or qualifies a route. Run every tool with `python -I` as the
verified human, passing a `--python-site` that holds onnx and numpy where asked.

| File | Inputs | Outputs | Use |
| --- | --- | --- | --- |
| `../arbiter-fp16-export.py` | Pinned FP32 source graph and its external-file pins. | `model.onnx`, `weights.bin` and a report. | The exporter library. Keys: `focus` (static 512), `focus1024` (weight8 source, refused by the census), `focus1024fp32`, `compact`, `large`. |
| `focus_widen.py` | `--source-dir`: the FP32 static-512 Focus export 19b20ad4 with its receipt. | Fresh directory: `model.onnx` (794fc0b2…, static 1024), one `model.onnx.data` sidecar, `widen-receipt.json`. | Applies the static-shape edits of `scripts/arbiter/export/qwen/focus1024/export_graph.py` to the FP32 source. Learned bytes are copied unchanged and repacked into one sidecar. |
| `focus_static.py` | `census`: the static-1024 Focus graph. `export --key focus\|focus1024\|focus1024fp32`: a source graph plus pins from a receipt. | `census`: a JSON report. `export`: a fresh export directory with `export-manifest.private.json`. | The census reports refusals with topology facts. The export manifest has the same shape as `scripts/export-arbiter-fp16.py`. |
| `l5a_inputs.py` | `--campaign R`. | Fresh directory: pinned CPU source profiles and asset maps for Compact, Large and Focus 512. | Gives the CUDA preparers byte-identical CPU sources. |
| `l5a_replay_compare.py` | `l5a_replay` receipts (`--replay NAME=PATH`), CPU reference batches (`--cpu-ref`), and `--pair CAND:REF`. | A comparison JSON. | Per pair: raw and mass error, winner agreement and flips, with near ties marked. Limits are those of CONTRACT.md. |
| `test_exporter.py` | None. | unittest results. | Literal exporter controls: `python -m unittest discover -s scripts/arbiter-fp16-export`. |

The Rust selectors these tools feed are in
`crates/airk-retrieval/tests/cuda_l5a_diagnostics.rs`. Their contract is in
`crates/airk-retrieval/src/arbiter/runtime/cuda/CONTRACT.md`, section
"L5 evaluation diagnostics and protected admission".
