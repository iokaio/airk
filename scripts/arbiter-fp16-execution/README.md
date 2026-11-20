# Context CUDA graph and execution measurements

These tools stage Compact and Large candidate graphs from retained model assets and measure them on the campaign device. Model weights stay in the private campaign directories. Each run takes a pinned native executable, the build receipt that produced it, and the applicable graph, reference and input receipts.

For Compact, `probe_compact_cutlass.py` measures attention operands, graph placement, raw scores, warm timings and original calibration inputs. Its score receipts preserve both independent references and the winner for each complete vector. For Large, `large_cutlass_candidate.py` stages the compensated-attention graph with the original external weight file. `large_cutlass_operands.py` prepares the original layer-17 inputs, and `probe_large_cutlass.py` collects native measurements. The CPU reference tools are in the adjacent `arbiter-fp16-reference` directory.

Run a selected program with `--help` for its exact arguments. Use the checked campaign execution path and a fresh receipt destination on D:. See the [execution contract](CONTRACT.md) for graph identities, input requirements and run modes, and the [capacity sweep procedure](../cuda-component-execution/CAPACITY-SWEEP.md) for pressure admission.

Run `compact_width_drift.py` using isolated Python with bytecode disabled to summarize the four frozen eight-card Compact score receipts. Its JSON reports each width, reference top-two margins and matching and publisher raw and probability errors. The pooled statistics give each of the 32 scores one vote and use nearest-rank p95. Task-level evidence uses the complete Answer workload and its own judgments.

Both model probes use score_comparison.py to compare complete score vectors against the matching arithmetic and original publisher references. Run test_score_comparison.py with isolated Python to check ties, nonfinite values and the two raw-error limits.

Run large_width_results.py with the campaign directory and a fresh private JSON output path to reproduce the four Windows Large score comparisons. It reopens each original native log and reports the score vectors, per-width and pooled raw and normalized-mass differences, winners, native test elapsed time and observed memory peaks. The test elapsed time includes model loading and teardown. The 8192-token result remains separately identifiable.
