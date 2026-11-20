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
