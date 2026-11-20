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
