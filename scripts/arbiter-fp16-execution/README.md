# Context CUDA graph and execution measurements

These tools stage Compact and Large candidate graphs from retained model assets and measure them on the campaign device. Model weights stay in the private campaign directories. Each run takes a pinned native executable, the build receipt that produced it, and the applicable graph, reference and input receipts.

For Compact, `probe_compact_cutlass.py` measures attention operands, graph placement, raw scores, warm timings and original calibration inputs. Its score receipts preserve both independent references and the winner for each complete vector. For Large, `large_cutlass_candidate.py` stages the compensated-attention graph with the original external weight file. `large_cutlass_operands.py` prepares the original layer-17 inputs, and `probe_large_cutlass.py` collects native measurements. The CPU reference tools are in the adjacent `arbiter-fp16-reference` directory.

Protected Compact R7 calibration uses `probe_compact_cutlass.py --mode cal
--cal-epoch r7-first56 --cal-phase N` for phases 0–6 after the first native
lifetime settles. Select `r7-full100` for phases 0–12 from the complete
capture and terminal receipts. Pass the SHA-256 values of
`r7-chronological-first56-r2/projection-receipt.json` and
`r7-chronological-native-first56-r1/native.json` with
`--cal-projection-receipt-sha256` and `--cal-compiler-sha256`. For the
terminal capture use the corresponding `full100` directories. Keep the
same-image graph receipt, the original 120-second clock, the 16 GiB host Job
and 24 GiB device arena. Use the [R7 chronological preparation](../arbiter/evaluation/selected_goal/cal_raw/README.md#r7-chronological-original-inputs)
to produce both exact-model compiler inputs. The scorer retains only this
phase's original R7 rows, native local operation ordinals and synchronized
raw vectors. Reference comparison, calibration fitting and product admission
require separate evidence.

For the settled R8 corpus, use `--cal-epoch r8-full100` and the retained R8
projection and compiler receipt SHA values. Use phases 7–12 to score the 44
rows left after verifying the exact same-input R7 prefix. R8 uses the existing
Compact scientific image and graph with the same 120-second, 16 GiB host and
24 GiB device limits. The R7 raw vectors preserve their original Job evidence.

`probe_large_cutlass.py --mode fixed` uses one warm-up and 20 measured T8192 calls. `--mode mixed` runs three cycles of T32, T128, T1024, T8192 and T32. Both require `--direct --width 8192`, the current image’s operand and graph receipts, and the exact previously measured native score words for every width. The timing journal measures synchronized model calls within one session and records cold open, per-call resource observations and release. For fixed20, `--separate-native-window` retains deny-write/delete handles after original input verification and records the prelaunch, native and post-verification clocks separately. The 120-second native clock includes cold open and every call. The receipt also gives the overall elapsed time against the original combined 120-second screen.

The Windows Large direct pressure study uses `protected_capacity_sweep.py --model large` with its own same-image operand, final typed graph and static Cargo receipts. The fit measures one original T8192 invocation with a 22 GiB process and Job limit. A fixed20 request requires a completed fit at that tier with verified dedicated-only residency, correct scores and natural process settlement. Each tier retains its raw WDDM samples and exact ballast observations.

Run a selected program with `--help` for its exact arguments. Use the checked campaign execution path and a fresh receipt destination on D:. See the [execution contract](CONTRACT.md) for graph identities, input requirements and run modes, and the [capacity sweep procedure](../cuda-component-execution/CAPACITY-SWEEP.md) for pressure admission.

Run `compact_width_drift.py` using isolated Python with bytecode disabled to summarize the four frozen eight-card Compact score receipts. Its JSON reports each width, reference top-two margins and matching and publisher raw and probability errors. The pooled statistics give each of the 32 scores one vote and use nearest-rank p95. Task-level evidence uses the complete Answer workload and its own judgments.

Both model probes use score_comparison.py to compare complete score vectors against the matching arithmetic and original publisher references. Run test_score_comparison.py with isolated Python to check ties, nonfinite values and the two raw-error limits.

Run large_width_results.py with the campaign directory and a fresh private JSON output path to reproduce the four Windows Large score comparisons. It reopens each original native log and reports the score vectors, per-width and pooled raw and normalized-mass differences, winners, native test elapsed time and observed memory peaks. The test elapsed time includes model loading and teardown. The 8192-token result remains separately identifiable.

## CPU FP16 operator placement

Run `python -I -B -S scripts/arbiter-fp16-execution/diagnose_cpu_fp16_placement.py
--output <fresh guarded campaign directory>` to inspect how the pinned Windows
ORT CPU provider loads literal FP16 MatMul and Softmax graphs. It saves the
optimized graphs and reports inserted casts. The controls contain no model
weights and execute zero forwards.
