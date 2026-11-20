# Native Linux CUDA runtime

The GNU/Linux x86-64 service admits a scientific or product descriptor using
the Linux manifest and static-core pins compiled into the executable. The
inputs are the manifest, its producer evidence and notices, Linux ELF libraries
beside the native image, and host libraries from the approved system directories.
The module retains the service lease, measured object handles, adapter identity
and primary context. It writes no package files.

The ELF reader checks the program headers and the dynamic string table on the
actual held image. Private executable bytes are copied to Linux memfds and
sealed against writes, growth and shrinkage before loading. Their memory cost
belongs in the host qualification measurement. Root-owned dependencies use
held descriptors to their canonical files. The ordered DT_NEEDED names,
SONAME, lengths and digests must match the manifest. Executable mappings and
resolved function addresses are compared against kernel file identities.

The linked ORT core owns provider initialization and the final SessionState
observations. The Linux provider bootstrap installs the core's ProviderHost
before opening the authenticated CUDA provider. Partial admission retains
mapped handles and poisons the process. Adapter observation and synchronization
use the actual retained primary context under the caller's existing deadline.

With the test-support feature, the native inference worker can assemble an
unapproved scientific descriptor from held ELF and provenance inputs. The
package projection command requires a product descriptor and emits the
measured distributable file list, including original legal notices. Both
commands use the existing exclusive service lease. Run the ignored inference
selectors `emit_cuda_runtime_manifest_from_held_native_closure` and
`emit_cuda_runtime_package_projection_from_held_native_closure` with a pinned
request and fresh output path.

Run the airk-retrieval GPU-feature Clippy and Nextest checks on the managed
Linux target. The cuda::elf::tests cases exercise the bounded image reader
without mapping a shared library. GPU scoring and release qualification use
the separate campaign gates. See the [CUDA execution contract](../CONTRACT.md).
