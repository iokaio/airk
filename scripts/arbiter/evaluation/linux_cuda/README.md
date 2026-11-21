# Linux CUDA qualification preparation

Run `inventory.py` as the verified human in the registered WSL2 distribution,
using the system Python with isolated startup, disabled bytecode and no site modules. Pass the registered `--uid` and
`--user` after the script. Capture stdout to a fresh checked private receipt.
For the campaign host the command is
`/usr/bin/python3 -I -B -S inventory.py --uid 1000 --user tyler`.

The inventory records platform identity, the loader cache, selected library
search roots, enumeration errors and observed x86-64 ELF metadata. It lists
Ollama-private libraries separately. Runtime qualification requires its own
reviewed provider, exact source-build provenance, native loading, device and
model evidence. No library is activated during inventory.

Run `redistributions.py` with the same interpreter flags to read the exact
NVIDIA release metadata pins and emit the Linux x86-64 package roster for
CUDA 12.8 Update 1 and the CUDA 12 build of cuDNN 9.10.2.21. To stage the
original archives, pass `--stage` with a checked private directory after
checking every listed destination and its `.partial` sibling. A failed
transfer leaves its partial file for inspection. Completed files are
rehashable on later invocations. Preserve the publisher licenses and check
actual build and runtime dependencies separately. This command does not
install packages, change the WSL host driver, or load a provider.

Inside the verified Ubuntu instance, unpack the authenticated archives with
`/usr/bin/python3 -I -B -S unpack.py --uid 1000 --user tyler --acquisition <absolute-acquisition.json> --sha256 <receipt-sha256> --output <fresh-guest-input-root>`.
Use a fresh human-owned directory under the user's isolated build parent.
The extractor checks package hashes, bounds, owner, access policy, links
and overlapping paths. It keeps release notices per package. Capture stdout
to a checked private receipt outside the guest build root.

Run `/usr/bin/python3 -B -S compiler_probe.py --uid 1000 --inputs <absolute-guest-input-root> --output <fresh-sibling-output>`
from this directory. It compiles `compiler_probe.cu` for sm_86 with a single
NVCC worker under a 120-second process-owner deadline, retaining intermediates
and tool/object digests. It never invokes the compiled kernel. The probe expects
the provisioned user-owned GCC wrapper under the build parent's `toolchain/bin`.
Run `/usr/bin/python3 -B -S -m unittest -v test_preparation` here for the
six focused Ubuntu controls.

## Offline ORT build and validation

Use the registered non-root Ubuntu human and the campaign's original private
receipts. Invoke the tools below from this folder with `/usr/bin/python3 -B -S`.
The guest build parent is `/home/tyler/airk-builds/arbiter-final-linux-r1`.
Capture each JSON result to a fresh checked private receipt.

- `configure_ort.py --output <fresh-build-root>` verifies the source archives and
  offline mirrors, projects the protected loader bootstrap, gives the provider
  an ELF SONAME, and records the CMake/Ninja graph and compiler identities.
- `build_ort.py --configure <receipt> --sha256 <reviewed-receipt-sha256> --target shared --jobs 2`
  compiles the shared ELF. Select `provider` for the CUDA module or `core` for
  static archives. Use `--attempt <fresh-number>` to preserve earlier logs.
  `graph_proof.py` verifies included Ninja files, expanded commands and actual
  tools before and after compilation. Worker counts are two or four, at low
  priority, with one NVCC thread and a 12 GiB address-space limit per process.
- `staged_elf.py --inputs <unpacked-root> --shared <compiled-shared-ELF>`
  hashes the staged vendor images and their host dependencies with readelf.
- `compile_bootstrap.py --output <fresh-directory>` compiles the protected
  initial mapping in `ort_bootstrap.inc` with the genuine ORT Env compilation
  command. `bootstrap_projection.py` owns the source and linker transformations.
- `notices.py --output <fresh-directory>` collects the original source and
  NVIDIA notice bytes. The private inventory identifies their package, original
  location, staged name, byte count and hash.

Run `/usr/bin/python3 -B -S -m unittest -v test_ort_build` for the focused
producer controls. These use the original frozen r3 translation unit and do
not initialize a GPU or load the compiled provider.

## Source and compiler input authentication

Use `verified_inputs.py --acquisition <reviewed-download-receipt> --inputs <guest-input-root>`
to verify every installed CUDA file or symlink against the members of the twelve
original NVIDIA distributions. Use `source_seal.py --configure <configure-receipt> --sha256 <digest>`
to compare the ORT and cuDNN frontend sources with their original archives and the
six reviewed Linux source changes. Retain both JSON results as private receipts.

The campaign's `authenticated_build.py` consumes those exact receipts and accepts
the same configure, digest, target, worker and attempt arguments as `build_ort.py`.
It reopens both source trees and the CUDA inputs before and after the native build.
Run `/usr/bin/python3 -B -S -m unittest -v test_authentication` as the Ubuntu
build owner for the input tampering and roster controls.

## Interrupted compiler recovery and volume checks

`checkpoint_ort.py` seals the original two-worker provider build and its settled
object tree. After the recorded WSL restart, run `recover_ort.py --phase inspect`
and preserve stdout as a private receipt. It authenticates all original files,
both original Ninja journal prefixes, and the 323 later objects by their actual
completed commands. Pass that receipt and digest to
`recover_ort.py --phase build --custody <receipt> --custody-sha256 <sha256> --jobs 2`.
The original 10,800-second wall deadline applies across boots. No model executes.
`drvfs_probe.py` measures actual owner, mode, link and ELF behavior on a candidate
mount. Use the original guest-native build for inputs requiring private POSIX modes.

During this particular resumed build, run `watch_provider.ps1 -ReceiptRoot <campaign>`
from the checked Windows checkout. Every 30 seconds it records Windows free memory,
commit, C: free bytes, backing VHD length, and guest filesystem free bytes. It
requests an exact owned Ninja checkpoint if the 24 GiB volume-growth bound or
projected 100 GiB free floor is crossed. `pause_provider.py` can briefly suspend
only the verified provider build session through retained pidfds during a coordinated
Windows timing window. The control records the stopped census and resumes at its
deadline. Preserve both its output and the cross-host observations. For the
static targets and final archive, start the same host watcher with `-Stage core`
and `-Stage archive` respectively. Both retain the original volume baseline.

## Native source and package preparation

`maintained_gate.py --gate inspect` records the maintained Rust checkout inputs.
Pass this receipt via `--freeze <path> --freeze-sha256 <digest>` to the
`--gate clippy` and `--gate nextest` commands when the reviewed single-worker
reservation is available. These run on GNU against the existing CPU static archive.
Their source and process receipts describe compilation and CPU controls.

For the joint `f3978a12` baseline, add `--signed-r7` to all three commands.
`signed_gate.py` verifies the complete Git archive and its staged files on D,
then reopens the genuine GNU static ORT aggregate and scientific ELF descriptor.
The signed Clippy and Nextest gates use the same offline target and enforce the
original volume caps. Nextest includes the CUDA and CPU source transition tests.
For the resumed signed R7 Nextest only, pass `--revised-host-volume-log` with
its active `host-volume-watch-r4.jsonl` path. This admits 512 MiB of additional
Cargo writes under the original 24 GiB guest and VHD caps. It checks the approved
120 GB physical C: allowance separately and reserves the configured 16 GiB swap
backing. Both free-space floors, the original baselines and the 17:10 UTC cutoff
still apply. The native volume receipt retains the host watcher observations.
That cutoff has passed, so this option now refuses. New runs use the
[successor volume policy](#successor-volume-policy-and-guest-start).

The signed R7 Nextest gate preserves the original CI configuration bytes in a
per-attempt copy and adds only an absolute `store.dir` inside its checked GNU
attempt directory. The pinned Nextest CLI receives the copy through
`--config-file`. The receipt links the original and effective configuration
hashes and the JUnit output at `nextest-store/ci/junit.xml`. Source custody
still requires the exact signed file roster. Attempt 3 failed that check after
all 173 selected tests passed because its default store put the JUnit output
inside the staged source. Its original gate failure and preserved report
remain separate from the successor.

Run `cutlass_source.py` to verify the original protected Compact graph and weights,
the six ORT attention source files, the CUTLASS arithmetic specialization, and the
actual SM86 object in the Linux provider link. Its JSON describes native build
and input identities. Runtime operator selection requires a Linux graph and
kernel observation.

Use `finish_ort.py --phase provider` with the shared/provider command receipts
and attempt number after the authenticated provider has linked. Use the `core`
phase after the canonical static component targets finish. `stage_payload.py`
joins the native Rust-emitted payload roster with observed ELF, original package
notices and retained source and build evidence. The scientific assembly and native
product projection each require their own Rust observation and original process
record. Use `observe_scientific.py` to obtain the scientific descriptor, then
`link_scientific.py` with the core SHA, assembly receipt and passing GNU source
gate to link the audit image against the native static aggregate. Its image proof
ties the four exported ABIs to that archive and the scientific descriptor.
`stage_product.py` consumes the image proof and the Rust provenance example from
its original successful Cargo producer receipt. Run `product_projection.py` on
the staged product descriptor and audit image to retain both Rust observations.
The empty-requirements product descriptor enables packaging before model fits.
`../../../cuda-linux-package.py` reopens the native product and the
original static GNU build for `build-service-runtime.py`, `package-native.py`
and `package-standalone-linux.py`. The three paired path/digest arguments identify
the static handoff, native product projection and actual native execution evidence.
The product layout puts the verified manifest, native ELF images and projected
notice files next to `service/airk`. See [Linux FP16 qualification](FP16_QUALIFICATION.md)
for the separate runtime, score and resource evidence.

For the campaign's native debug pair, pass the original checkout receipt and
digest to `shipping_source.py --base PATH --base-sha256 SHA`. It projects current
source and all Rust embedded file inputs to the checked guest checkout, then
compares the shipping source identity before and after. Use
`shipping_notice_cache.py --source PATH --source-sha256 SHA` if Cargo's offline
legal inventory also needs platform-conditional archives. The copier reads the
existing human-owned archive cache and verifies each new archive against Cargo.lock.

Run `shipping_build.py --source PATH --source-sha256 SHA --attempt N` with a fresh
attempt. It runs the standard service builder with the verified Admin inventory
and the native Linux static, projection and original-execution inputs. The
output receipt records every staged file and the settled child. Use
`shipping_archive.py native --build PATH --build-sha256 SHA --attempt N` to
verify the pair, write a native ZIP, extract it independently and exercise the
service and model smoke checks. With the resulting `--native PATH
--native-sha256 SHA`, the `standalone` stage reuses that verified extraction
to generate and independently re-read the complete installer tar. Large members
retain their canonical placement, modes and checksum entries in the archive.

The watchdog samples the guest filesystem, the physical C: free space and the
original WSL VHD length every ten seconds. Both volumes retain the campaign's
24 GiB total growth limit and 100 GiB projected free floor. Native packaging
reserves one compressed and one extracted copy. Standalone streaming reserves
one archive plus 64 MiB for its generated manifests and wrapper. The original
three CUDA inputs, compiled pair and packaging-tool source remain independently
identified. A model fit or activation requires its own native evidence.

`shipping_service.py --build PATH --build-sha256 SHA --attempt N` stages the original
pair through the managed installer in the verified owner’s executable XDG tmpfs.
It exercises catalog setup, Context Compact enrollment, the global GPU setting,
Auto/FP16 and explicit CPU selection through the matched public CLI. The control
expects the descriptive build to refuse activation and retain its settings. It
records command responses, managed lifecycle settlement, guest and backing
capacity, tmpfs use and swap. Reserve 4 GiB for tmpfs and 8 GiB for the native
processes. A failed request retains the instance for diagnosis for up to 180
seconds. Lifecycle settlement releases the service. The native standalone
`install.sh` journey and model execution each require separate evidence.
The journey now runs `gpu_opt_in_workflow.py`: service-wide GPU use starts
off and turns on only through the service-wide setting. A pair built before
commit `0b5fe97c`, including the retained `linux-cuda-descriptive-service-r3`
pair this control still names, starts with GPU use on and fails that check.

### Experimental GNU R7 overlays

These four sources belong to the signed `f3978a12` CPU-only Retrieval check.
The campaign currently holds Ubuntu startup because the configured 16 GiB
swap file could be allocated on C: before a guest process can observe it.

When guest startup is authorized, run `python3 -B -S target_overlay.py` as
the verified native UID 1000 for the small private XDG copy-up control.
The historical receipt `linux-same-uid-overlay-r1.json` (SHA-256
`4306e8e36836a9c1f6fed52eba13d836163c6b6960f6f6fe12ae40600262b062`)
covers its earlier source. It observed unchanged lower bytes and inode, four
UID fields equal to 1000, cleared capabilities and zero VHD length growth.

`overlay_target.py` checks the real target/cache mount topology in a fresh
private namespace. `overlay_confinement.py` makes every inherited mount
read-only, restores the private tmpfs and checks the writable mounts before
dropping namespace capabilities. `python3 -B -S overlay_target.py` requires
the original guest and host volume budgets, spare RAM and executable user
tmpfs. Its real-target control still needs a successful native run.

`overlay_nextest.py` additionally requires disabled guest swap and an
original-user test namespace with only the private XDG tmpfs writable. It
checks these prerequisites before compiling. The runner uses the original
signed R7 source/static inputs and warm target, a 1.5 GiB tmpfs upper, one
Cargo worker and an 8 GiB address-space cap. It exports the checked test ELF
into user tmpfs so runtime tests can inspect the true system file owners.
The normal Ubuntu mount layout does not meet the original-namespace write
constraint. Windows host commit, pagefile and physical backing admission
remain separate prerequisites. The disabled-swap check belongs only to this
experimental zero-write diagnostic. A separately authorized Ubuntu run with
bounded D:-backed swap can use the ordinary native gate under its own approved
volume and memory budgets. Verify the actual backing location and allocation
before admission. R7 Nextest still needs native execution.

The retained host inventory identifies kernel
`5.15.167.4-microsoft-standard-WSL2`. Microsoft's matching configuration
enables Landlock; the tagged implementation supplies ABI 1. A read-only ABI
query in the actual guest remains pending. This ABI lacks file-truncation
control, and Landlock leaves some inode metadata changes unrestricted. An
original-UID per-process policy would preserve real system file owners, but
needs separate enforcement for those writes and genuine native validation.
See the [WSL kernel configuration](https://github.com/microsoft/WSL2-Linux-Kernel/blob/linux-msft-wsl-5.15.167.4/arch/x86/configs/config-wsl#L3412-L3416),
the [tagged ABI implementation](https://github.com/microsoft/WSL2-Linux-Kernel/blob/linux-msft-wsl-5.15.167.4/security/landlock/syscalls.c#L133)
and the [kernel's filesystem access documentation](https://docs.kernel.org/userspace-api/landlock.html#filesystem-flags).

## Successor volume policy and guest start

The rules these tools enforce are in the [admission contract](CONTRACT.md).
This section says what each tool needs, what it produces and how to run it.
Run Windows commands from the verified human `pwsh` at the repository root.
`CAMPAIGN` is `artifacts\v190\final-plan\exec-20261116T052942Z`. In the guest
the same folder is `/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z`.

**1. Start the host watcher** with the next unused log name, as described in
`../watch-campaign-volumes.ps1`, and leave it running for the whole batch.

**2. Read the log's binding.**
`python -B -S scripts\arbiter\evaluation\linux_cuda\gnu_volume_policy.py --volume-log <CAMPAIGN>\host-volume-watch-r<N>.jsonl`
prints `first_row_sha256` after checking that the first row carries the
recorded baselines and allowances. It writes nothing.

**3. Admit the guest start.**
`python -B -S scripts\arbiter\evaluation\linux_cuda\wsl_boot_admission.py --expected-sid <human SID> --receipt <CAMPAIGN>\linux-<name>-boot-r<N>.json --volume-log <log> --volume-log-first-row-sha256 <digest> --not-after <ISO-8601 end with offset>`

- Inputs: the three binding values, a fresh receipt path directly in the
  campaign root, `--reserve-bytes` for the storage the first job will reserve
  (default 0), `--other-host-bytes` (default 0) and `--min-distinct-cores`
  (default 8). The wrapper requires 12 GiB of free physical and commit memory
  for the guest's CPU gates. State in `--other-host-bytes` what other running
  jobs have reserved, plus whatever the planned guest job needs beyond 12 GiB.
- It admits a stopped instance, and also one where only `Ubuntu` is already
  running, so it can be repeated before each gate. It does not check the GPU.
- Output: the receipt, the same JSON on stdout, and exit code 0 only when
  `admitted` is true. A refusal is listed under `refusals`.
- `--plan` prints the six commands it would start and starts none.
- It is the only place that starts `wsl.exe` for the boot. Import
  `guest_command` from it to build any later guest command, so that the
  distribution and user are always stated.

**4. Run a GNU gate under the policy** by adding the binding to the retained
command, with the log's guest path:
`C:\Windows\System32\wsl.exe --distribution Ubuntu --user tyler --cd /mnt/d/code/Github/airk --exec /usr/bin/python3 -B -S scripts/arbiter/evaluation/linux_cuda/maintained_gate.py --gate inspect --volume-log <guest log path> --volume-log-first-row-sha256 <digest> --not-after <end>`

The same three options apply to `--gate clippy`, `--gate product-clippy` and
`--gate nextest`, with or without `--signed-r7` or `--signed-joint`. Clippy
reserves 128 MiB and the others 512 MiB. Under the policy a run that grows the
guest or the VHD beyond its reservation is stopped, so measure one
representative compile of new source before scheduling a batch.

**5. Run a shipping or packaging entry under the policy** through the
launcher, which leaves the entry's own arguments unchanged:
`/usr/bin/python3 -B -S volume_policy_entry.py --volume-log <guest log path> --volume-log-first-row-sha256 <digest> --not-after <end> -- shipping_archive.py native --build <receipt> --build-sha256 <sha256> --attempt <N>`

It accepts `shipping_build.py`, `shipping_archive.py` and `shipping_service.py`.
Each sample in the entry's `volumes.json` then carries the watcher row, the
D: reading and any refused limits.

Checks that run on Windows or in the guest without Cargo, a model or WSL
commands, from this folder:
`python -B -S -m unittest test_gnu_volume_policy test_volume_policy_entry test_maintained_gate_policy test_shipping_guard_policy test_wsl_boot_admission test_gpu_opt_in_workflow`

The gate and guard checks replace the owned process, the guest filesystem and
the watcher with recorded values. They show how the policy is selected and
applied. A native GNU run is still required for each gate.
