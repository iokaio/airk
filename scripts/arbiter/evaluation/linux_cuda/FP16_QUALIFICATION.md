# WSL2 FP16 scientific qualification

Run the GPU selectors serially on the campaign's registered Ubuntu WSL2
instance after Windows FP16 passes and the A6000 is released. Verify the
interactive Windows SID and owner and the registered non-root guest UID before
the batch. The emitter and scorer use the sealed Linux static ORT core, Linux
CUDA provider, compiled Linux descriptor and original private input hashes.
The original independent development receipts retain their Windows source
locations. The native worker translates locations under the checkout's
artifacts directory to their read-only WSL mount and verifies the pinned bytes.

Create the Linux resource input with the model-free ignored selector
`fp16_compact::linux::emit_linux_compact_cpu_resource_input_from_pinned_original`.
Give it `AIRK_SOFTWARE_V2_NATIVE_INPUT{,_SHA256}` for the archived Compact
CPU input (SHA-256 `18722ef1b3a38777afff5a3ff4ddcb2a396f2730e4e5097a462092e584ce8c27`)
and a fresh `AIRK_CUDA_QUALIFICATION_OUTPUT`. It rewrites file locations
through the Rust-owned private resource input, preserving all input hashes.

Generate the Linux native preparation request with
`emit_cuda_component_probe_request` (prepare mode) and run
`emit_cuda_compact_fp32_qkv_projection_probe_metadata` with the Linux CPU
resource input and the pinned QKV stage. Keep the fresh metadata directory
in the private Linux build root beside its output receipt. The native receipt
supplies a pinned numeric probe and the maximum page-rounded private ELF
memfd backing. The hardware owner must authenticate the installed scorer,
seal the original metadata/validator hashes and record the guest driver and hardware identity. The supervisor sets the
reviewed GEMM2 and proof-only attention-debug1 environment before exec.

Use the host-side Python interpreter from the verified WSL user. For each
step allocate a fresh private output directory under
`/home/tyler/airk-builds/arbiter-final-linux-r1`. The scorer image and its
native descriptor belong together in the sealed package directory. The
supervisor selects exactly one ignored native test from that image.

```text
python3 -B fp16_supervisor.py MODE --image IMAGE --image-sha SHA256 \
  --metadata RECEIPT --metadata-sha SHA256 --validator-sha SHA256 \
  --gpu-uuid GPU-UUID --directory FRESH_DIRECTORY
```

Run `graph` first. Independently review and compile the emitted Linux
load-only graph receipt SHA as `AIRK_REVIEWED_LINUX_CUDA_QKV_GRAPH_SHA256`
before `t32` or `wider`. Supply the original independent projection
reference for the selected mode. The wider roster contains 128, 1024 and
8192 tokens. Review the real Linux T32 and wider score receipts, then compile
their pins as `AIRK_REVIEWED_LINUX_CUDA_T32_NATIVE_SHA256` and
`AIRK_REVIEWED_LINUX_CUDA_WIDER_NATIVE_SHA256` before `fixed` and
`mixed`. Supply `--timing-validator-sha` for the reviewed
`scripts/arbiter-fp16-execution/timing_control.py` in those modes. The native timing harness checks the complete eight-score words,
the original width order, the fixed twenty-sample population and all mixed
cycles. The parent checks all expected FlashAttention records through the
existing reviewed validator in `scripts/arbiter-fp16-execution/probe_compact.py`.
Use `AIRK_CUDA_FP16_REVIEWED_GRAPH{,_SHA256}`,
`AIRK_CUDA_FP16_PROJECTION_STAGE{,_SHA256}`,
`AIRK_CUDA_FP16_PROJECTION_REFERENCE{,_SHA256}` and both
`AIRK_CUDA_FP16_TIMING_{T32,WIDER}{,_SHA256}` where their selectors require
them. The supervisor supplies the probe request, metadata, output and profile
locations. Stage the original independently reviewed CPU proof and its pin as
required by the metadata emitter.

The scored process receives one address space with its soft and hard limit
set to the original 32 GiB host budget less the authenticated private ELF
backing. The kernel admits normal same-address-space thread creation and
denies process creation. A pidfd watchdog enforces the original 120-second
parent deadline. The native scorer checks actual UID, resource limits,
parent-death signal, seccomp, the 24 GiB device budget, completed device
synchronization, empty claims, real host charge release and native control
cleanup. The parent retains the exact PID, binary, target UUID, exit,
component output and operation journal hashes, fixed/mixed p50 and p95,
per-cycle resident host and adapter maxima, resident/virtual/private host snapshots and sampled
adapter-wide memory observations. Raw NVML device growth depends on other
adapter work; retain exclusivity and the baseline. Kernel and driver memory
outside the scorer address space and private memfds, process admission under
CUDA virtual-address reservation, product eligibility, recovery and final
release evidence require their own observations. Linux score and placement
evidence must come from the Linux candidate. The release maximum absolute raw
score error is 0.007 against the matching reference, with identical winning
utilities and both original mass bounds. Existing receipts retain their
original predicates. The 0.070 comparison is a development diagnostic.

## Large metadata and graph

Use `large_native.py metadata` with the original Linux Large CPU resource
input and a Rust-emitted prepare request. Supply `--image`, `--image-proof`,
`--image-proof-sha256`, `--request`, `--request-sha256` and a fresh
`--directory`. The metadata selector emits five pinned assets and runs zero
model forwards. For the load-only observation, use `large_native.py graph`
with the emitted probe request, `--metadata`, `--metadata-sha256`,
`--gpu-uuid` and `--validators` followed by the reviewed SHA-256 digests
of `cuda-component-execution.py`, `probe_large.py` and `large_placement.py`
in that order. The parent checks the final typed graph, 36 CUDA attention
nodes, CPU shape payload sizes, device identity and settlement under the
48 GiB host, 44 GiB device and 120-second limits. Review the resulting Linux
receipt before compiling a scoring profile. The Large release requires its
own matching score references, measured fit and timing results.

Run the CPU-only controls from this directory with
`python3 -B -m unittest -v test_fp16_process test_fp16_supervisor test_large_native`.
They exercise an actual bounded scorer-style exec, allowed threads, denied
process creation and excessive mappings, unmapped memfd backing, independent
watchdog termination, kernel memory observation and settlement refusal. The Large
parent regression uses the original Windows receipt to test the parser, including
truncated diagnostic text alongside complete typed graph rows. GNU model evidence
requires a new observation under its own image.

## Protected Compact CUTLASS

The protected Compact graph has 196 FP32 MatMuls and 28 efficient multi-head
attention operators. Run `python -I -B -S scripts/arbiter/evaluation/linux_cuda/cutlass_references.py`
from the checkout to authenticate the original independent CPU references at
T32, T128, T1024 and T8192. The command reads the pinned original execution,
producer and fixture for each width and prints the eight matching scores,
original FP32 scores, argmax and compiled input digest. It reads no CUDA device.

The GNU test-support selector `gnu_cutlass::compact_gnu_cutlass_graph` opens
the separately approved scientific ELF descriptor and the protected model
and weights under the original native owner. It requires a bounded parent,
a 120-second absolute monotonic deadline and the actual GNU compiled image.
Its load-only result records typed graph rows, placement and CPU shape refusal.
Add GNU bounds only after reviewing the complete GNU observation. The four
CPU references alone establish no GNU kernel, matching, fit or paired result.

After the Windows acceptance and original CPU/GPU capacity checks, use
`cutlass_graph.py --image PATH --image-proof PATH --image-proof-sha256 SHA
--directory FRESH_PRIVATE_DIR` for the load-only GNU observation. This parent
uses the native ext4 runtime staging directory, original static aggregate,
scientific ELF manifest, fixed physical A6000, 32 GiB host and 24 GiB device
ceilings, a single 120-second deadline and the private process guard. Retain
both `parent.json` and `native.json`, including any CPU shape refusal, before
requesting a scored run. `python -B -m unittest -v test_cutlass_observation`
exercises the parser with the archived Windows graph and a synthetic custody
envelope. Its checks make no GPU calls.

For Large, supply `--model large` to run `gnu_cutlass::large_gnu_cutlass_graph`
with a 48 GiB host ceiling and 44 GiB device limit. The selector authenticates
the direct FP32-activation graph, the 8,045,328,384-byte half-weight file and
the 36-attention/252-projection placement under its original GNU owner. The
parent rejects Compact receipts, altered graph bytes or any model score. The
portable parser suite covers both original Windows graph structures.

To check retained Large CPU inputs, run `python -E -B -S
scripts/arbiter/evaluation/linux_cuda/large_direct_references.py --row WIDTH
EXECUTION_SHA256` with one `--row` for each original, independently completed
width. It authenticates the original CPU Job and reference, publisher revision,
weight and graph identity and all eight scores. The T32/T128 input receipt
records just those two widths. A GNU runtime score is a later distinct result.

For the signed `fdf13652` scientific source, `joint_source.py` authenticates
the original R7 archive and the 35-file native delta. Its 10,151-file view
belongs at `artifacts/v190/final-plan/gnu-joint-source` for the duration of
the build. Preserve the original R7 tree. Once that view and storage are
available to the native build, run `maintained_gate.py --signed-joint` with
`--gate inspect`, then `clippy` and `nextest` against the pinned inspection
receipt. Both crates are included. Give the genuine Nextest receipt and its
SHA-256 to `link_scientific.py --source-gate` and `--source-gate-sha256`,
with the original scientific assembly, static-core producer and a fresh
`--attempt`. The linker rechecks the original image, descriptor and sibling
CUDA payloads and retains the new executable under its own SHA-256 name.
The two native graph selectors require independent GNU graph observations.
