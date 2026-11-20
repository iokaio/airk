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
