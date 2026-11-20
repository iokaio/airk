# Context CUDA graph and execution measurements

These tools stage Compact and Large candidate graphs from retained model assets and measure them on the campaign device. Model weights stay in the private campaign directories. Each run takes a pinned native executable, the build receipt that produced it, and the applicable graph, reference and input receipts.

For Compact, `probe_compact_cutlass.py` measures attention operands, graph placement, raw scores, warm timings and original calibration inputs. Its score receipts preserve both independent references and the winner for each complete vector. For Large, `large_cutlass_candidate.py` stages the compensated-attention graph with the original external weight file. `large_cutlass_operands.py` prepares the original layer-17 inputs, and `probe_large_cutlass.py` collects native measurements. The CPU reference tools are in the adjacent `arbiter-fp16-reference` directory.

Run a selected program with `--help` for its exact arguments. Use the checked campaign execution path and a fresh receipt destination on D:. See the [execution contract](CONTRACT.md) for graph identities, input requirements and run modes, and the [capacity sweep procedure](../cuda-component-execution/CAPACITY-SWEEP.md) for pressure admission.
