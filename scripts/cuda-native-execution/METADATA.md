<!-- SPDX-License-Identifier: Apache-2.0 -->
# Owned Windows CUDA assembly and profile metadata

Use this entry point with the reviewed preserved Windows Inference test image
and a private plan for one stage. The existing package-projection dispatcher
has its own entry point and receipt. All four stages here emit metadata under
the original NN0 custody rule.

    python -I -B scripts/cuda-native-execution/metadata.py --execute-reviewed-nn0 --approved-script-sha256 <dispatcher-sha256> --plan <absolute-plan.json> --plan-sha256 <reviewed-plan-sha256>

Provide a separate reviewed `airk.private.cuda-profile-nn0-plan/1` for each
stage. The common fields are `schema`, `kind`, `selector`, `human_sid`,
`repository_root`, `limits`, `output_root`, `output_pins_expected`,
`environment`, `input_seals`, `wrapper`, `custody_owner`, `component_owner`,
`output_guard`, `output_guard_lock`, `checked_guard`, `powershell`,
`parent_python`, `ownership_helper_root`, `native_image`, `image_packet`,
`native_build_receipt`, `native_group`, and `required_native_sources`.
Paths are canonical absolute Windows spellings. Source roster keys are
repository-relative paths. Every file pin has `path`, `bytes` and `sha256`.
The original `input_seals` maps canonical absolute file paths to their exact
SHA-256 values. Seal the listed FilePins, all three maintained custody helpers,
the required native sources, and the original CUDA or CPU inputs used by the
stage. The dispatcher adds the plan's separately reviewed SHA to the receipt.
Each streamed hash retains the original file handle. The dispatcher rechecks
the canonical path and compares the original handle to a fresh open of that
path using the same Windows metadata API on both handles.

Point `wrapper` to this entry's Python source, `custody_owner` to
`scripts/cuda-native-execution.py`, and `component_owner` to
`scripts/cuda-component-execution.py`. Pin the maintained R8 output guard
(`scripts/cuda-static-runtime/build/r8/provision_cuda_runtime.py`) and its
sibling producer lock. Seal the CPU policy source and lock authenticated by
that R8 lock. Use the tracked `scripts/dev/assert-human-execution.ps1` as
`checked_guard` with a pinned Windows `pwsh.exe`. The three process helpers
come from `scripts/cuda-native-execution/helpers`. The image, build receipt and
source group must be the original exact preserved Inference image lineage.

The limits are `wall_seconds=120`, `host_committed_bytes=8589934592` and
`log_bytes=67108864`. The same-human parent and suspended child must have
the reviewed user and default owner SID. The owner keeps the original Job
and member handles through natural exit. Preflight, launch, native output
verification and final original input verification share the same deadline,
clipped to the campaign cutoff. Each stage has its own fresh guarded direct
child of the campaign output root. It retains its own stdout/stderr and
`execution.json`; the receipt's final storage is custody-checked after the
last measured clock sample.

## Stages

All selectors have the prefix
`inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::`.
The owner uses exactly `--exact <selector> --ignored --nocapture --test-threads=1`.
The following are the exact stage environment keys. When a pin is called
for, supply the absolute path key and its companion `_SHA256` key.

| Kind | Selector suffix | Stage environment | Expected native files |
| --- | --- | --- | --- |
| `assembly-request` | `emit_cuda_component_probe_request` | `AIRK_CUDA_REQUEST_KIND=assembly`, `AIRK_CUDA_RUNTIME_ROOT`, `AIRK_CUDA_PRODUCER_NAME`, `AIRK_CUDA_PRODUCER_SHA256`, `AIRK_CUDA_CHECKPOINTED_PROOF` pin, `AIRK_CUDA_MODULE_NAMES`, `AIRK_CUDA_LOAD_ORDER`, `AIRK_CUDA_SYSTEM_IMPORTS`, `AIRK_CUDA_DEADLINE_MS=120000`, `AIRK_CUDA_REQUEST_OUTPUT` | The request under this stage root |
| `assembly` | `emit_cuda_runtime_manifest_from_held_native_closure` | `AIRK_CUDA_QUALIFICATION_INPUT` pin to the actual Assembly request, `AIRK_CUDA_QUALIFICATION_OUTPUT` | `airk-cuda-runtime.json` and `airk-cuda-runtime.receipt.json`, in that order, directly beside the sealed staged runtime |
| `request` | `emit_cuda_component_probe_request` | `AIRK_CUDA_REQUEST_KIND=prepare`, `AIRK_SOFTWARE_V2_NATIVE_INPUT` pin, `AIRK_CUDA_RUNTIME_MANIFEST` pin, `AIRK_CUDA_HOST_LIMIT_BYTES`, `AIRK_CUDA_DEVICE_LIMIT_BYTES`, `AIRK_CUDA_DEADLINE_MS=120000`, `AIRK_CUDA_COLD_BYTES`, `AIRK_CUDA_WARM_BYTES`, `AIRK_CUDA_HEADROOM_BYTES`, `AIRK_CUDA_JOB_LIMIT_BYTES`, `AIRK_CUDA_REQUEST_OUTPUT` | The Prepare request under this stage root |
| `metadata` | `emit_cuda_fp32_probe_metadata` | `AIRK_CUDA_QUALIFICATION_INPUT` pin to the actual Prepare request, `AIRK_CUDA_METADATA_DIRECTORY` | `profile.json`, `native-input.json`, `probe-request.json`, `commands.json`, `metadata-receipt.json`, in that order, in a fresh direct child of the stage root |

The assembly list environment values are semicolon-delimited. Seal the
actual unqualified current-toolchain static build proof, byte-identical
staged `fresh-compile.json` and exact named runtime DLLs. Rust authenticates
the private proof and closure. The dispatcher checks the emitted
fits-empty descriptor, stage input, actual Job leader, and retained file
identities. It also checks the exact CPU source and profile asset seals when
preparing the profile metadata. Native code emits the sixth metadata file,
a calibration asset. The owner independently derives its leaf from the
emitted profile, reopens its bytes, and checks that it is the only additional
file and that the emitted native input names it.

Context metadata must list the exact 17-, 128- and 8192-token component
selectors in order. Focus metadata lists the single 512-token selector. Every
row binds the held native image and emitted probe request.

Consume only a successful, complete `airk.private.owned-cuda-profile-nn0/1`
receipt with `kind=metadata` at the component's `metadata()` boundary.
That receipt carries the original plan, complete original input seals and
exact six output pins. The dispatcher and its native selectors confer no
runtime approval, GPU probe, model execution, installation or activation.
