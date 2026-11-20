# Arbiter evaluation source

Maintained evaluation and independent reference recipes live here. Private inputs, models, execution plans and receipts remain under artifacts. Each component README and contract defines its supported entry points and required fresh source seals.

## Private Windows Compact FP32 Answer comparison

Run the paired fixture on an idle Windows host from the checked human
PowerShell session. Use the same reviewed service and CLI images, synthetic
corpus, Answer arguments, offline MiniLM files, model asset closure, and raw
all-unavailable calibration in both attempts. Keep each run in its own
fresh private artifacts directory. The only Compact profile difference is
the reviewed CPU or strict CUDA execution binding. Read the native fixture
[contract](../../../crates/airk-service/src/service_runtime/evaluation_fixture_tests/CONTRACT.md)
for the selector behavior.

Run:

    python -B -S scripts/arbiter/evaluation/run_compact_answer.py --plan <absolute-private-plan.json> --plan-sha256 <sha256> --output <fresh-artifacts-dir> --check

The plan schema is airk.private.paired-compact-answer/1. Supply backend (cpu
or cuda), its exact manifest, and absolute path/SHA-256 pins for service,
cli, answer, scopes, corpus, inputs, and minilm. The corpus pin describes
the files in its sibling sources directory. The minilm pin is model.json
and minilm_assets names the exact payload digests. The CUDA plan also
contains the reviewed graph receipt and SHA-256. Remove --check for a real
dispatch. Run one backend at a time. The Windows Job independently verifies
32 GiB process and aggregate limits. Confirm both fixtures close and the Jobs
settle, then compare request and corpus digests, search pipelines, and the
complete ordered lists of observed native input digests and actual token
counts. Read actual CLI round-trip time from answer_round_trip_ns and
guarded inference time from the work receipts. Keep the complete CLI
envelopes for interpretation. This comparison measures the all-unavailable
calibration path and retains the real Answer abstentions.

## Private protected-QKV Answer roster

Run `stage_compact_qkv_answer.ps1 -ExpectedSid <verified-human-SID> -OutputName compact-qkv-answer-roster-r<N>` in the checked Windows shell. It authenticates the frozen three-model role list, the private Compact descriptor, and the original final-graph load receipt. The generated `inputs.json` uses the original static-1024 Focus and Large profiles plus seven byte-verified Compact assets. The staging command reads model descriptors and payloads only.

Use `prepare_compact_qkv_answer_plan.ps1 -ExpectedSid <SID> -ServiceImage <checked-sibling-image> -ServiceSha256 <sha256> -OutputName compact-qkv-answer-plan-r<N>.json` to bind the original synthetic workload to a newly sealed service. Validate the result using `run_compact_answer.py --check` and the plan SHA-256. For the private CUDA Answer selector, pin this roster through `AIRK_ARBITER_EVALUATION_INPUTS` and `AIRK_ARBITER_EVALUATION_INPUTS_SHA256`. Supply the original QKV load receipt through `AIRK_ARBITER_EVALUATION_CUDA_GRAPH` and its SHA-256 through `AIRK_ARBITER_EVALUATION_CUDA_GRAPH_SHA256`, along with the leased fixture's exact adapter UUID and normal resource consent. Place the service test image beside the exact reviewed `airk-cuda-runtime.json` payload. Inherit `ORT_CUDA_GEMM_OPTIONS=2` and `ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO=1` in that process before ORT initializes. The source-verified recipe requires 16 GiB cold, 8 GiB warm and 2 GiB headroom, checks the live final graph, and retains the service GPU grant and operation deadline through dispatch. Disabling GPU use drains the fixed private Compact variant. A new attempt requires a current lease and eligible variant.

This roster authenticates the original engineering profile with zero active calibration rules. A scored comparison and an installed release each require their own qualified inputs and receipts.
For concurrent Windows and native WSL builds, the checked human shell can launch watch-campaign-volumes.ps1 from this directory. Pass the expected interactive SID, a fresh host-volume-watch-rN.jsonl in the campaign directory on D:, the absolute cutoff, measured initial free bytes on D:, the outstanding Windows and Ubuntu-backing byte reservations, and the required free-byte floor. Every 30 seconds the monitor samples Windows D:, the registered Ubuntu VHDX and its physical C: volume. It flushes a row on D: and exits with an error if growth exceeds a reservation or remaining headroom falls below the floor. The campaign owner must hold new work, identify the writer and settle it when alerted. The Linux build separately monitors its native filesystem. Host VHDX size and guest free space measure different things.

After the original monitor stops on a C:-only reservation alert, hold C:-backed writers. For continued observation on D:, pass `-HeldBackingLogSha256` with the SHA-256 of the closed original log and the same limits to the monitor. The new log retains the original baselines and records `backing_writes_held` on every sample. Further Ubuntu VHD growth, a D: allowance overrun or a physical free-space floor alert stops this observation.

`verify_bounded_product_origins.py <retained-windows-compact-graph-observation>` counts CPU-incident value provenance in the exact Windows Compact QKV graph. It accepts the original native graph receipt SHA printed in its result. The [scientific CAL raw owner](selected_goal/cal_raw/README.md) documents the separate original-input, independent publisher and measured-score workflow.
windows_cpu_ort.py runs the Windows FP16 campaign's fresh CPU-only ORT build with two BelowNormal physical cores in a six-GiB Job. It verifies existing local source archives, expands a fresh D: source tree required by the stock verifier, and captures actual compiler scratch directories. Use the current checked CAL service process ID with --cal-pid. The original stock CPU environment and aggregate are subject to a separate artifact owner audit before Cargo uses them.

After the first stock CPU configure stopped before CMake, transfer-windows-cpu-ort-seed.ps1 moved only the validated downloads, immutable source and source-files.json to E/p. Original build receipts and logs remain under E/o. The original CPU verifier requires its source inventory, selected mirrors, notices, stock library inventory, compiler tools and aggregate on subsequent validation. Candidate cleanup after integration is limited to unreferenced scratch and intermediate CMake object files, with the full verifier rerun before claiming disk recovery.

The --attempt 3 run checks E/q after the tool-path transfer. For that recovery the stock Ninja rules requested the Microsoft librarian through its Windows short path. The --attempt 3 --control option executes that literal alias and verifies its actual process, canonical file and fresh hash. The previous configure/build logs, Ninja rules, and CMake intermediates remain in E/p. Reproducible intermediate files may be reclaimed only after the retained receipt and remaining input checks pass.

Run audit-windows-cpu-ort.ps1 after the third producer exits. It checks the settled native Job, recursively audits the producer and artifact trees for same-user ownership and directory access, reopens the CPU aggregate through the stock read-only verifier, and compares its complete result with the original environment receipt. Pass -AfterReclaim for a separate audit after authorized disposal of verified intermediate files.

The Windows campaign shares one 1.5 GiB offline Cargo home on D: across its native producers. Run prepare-windows-offline-cargo.ps1 once from the checked checkout. windows_cargo_seed.py enumerates the current Cargo.lock archives, sparse index entries and original rusqlite Git objects from the human cache. Preparation verifies every archive checksum, executes cargo tree with the original C cache database held against writes, and records the database hashes and timestamps before and after the real resolution. Before an authorized single-target Cargo run, dot-source use-windows-offline-cargo.ps1 and invoke Use-AirkOfflineCargo -Scratch with an existing guarded private campaign directory. The function selects the checked cache and installed 1.98.0 toolchain for the current process. Producers still apply their original target, profile, resource bounds and model inputs.

When product staging needs room, reclaim-windows-cpu-ort-intermediates.ps1 authenticates the closed stock build via windows_ort_intermediates.py, records and hashes every removed original Ninja .obj/.pch, and runs the full independent stock audit again. The aggregate, component libraries, build receipts and source remain available to the ordinary verifier.
