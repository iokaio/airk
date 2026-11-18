# CUDA static runtime build tooling

This folder documents AIRK's Windows CUDA ONNX Runtime build and verification
workflow. [CONTRACT.md](CONTRACT.md) defines the accepted producer identities,
archive closure, toolchain and custody requirements. The maintained entry point
is [provision-cuda-static.py](../provision-cuda-static.py); its companion
[cuda-static-runtime.py](../cuda-static-runtime.py) verifies static build proofs.
The entry point's `--help` describes supported verification and staging modes.

The [final graph metadata collector](final-graph-metadata/README.md) is maintained
C++ source in patch form. It observes final optimized graph assignments and
connections for diagnostics. Its source review does not establish compilation,
model execution, GPU policy admission or qualification.

Reproduction requires the exact pinned ONNX Runtime/toolchain/dependency inputs
and the corresponding build recipe. Machine-local recipes and original receipts
are evidence, not replacement source code. Existing historical receipts must not
be edited to describe a rebuilt binary. Every new build keeps its own source,
toolchain, object/archive and command identities. Generated libraries, provider
DLLs and model artifacts do not belong in this source folder.

The [maintained verification helpers](verification/README.md) now own historical
checkpoint validation without executing ignored helpers. The [versioned build
sources](build/README.md) preserve the complete retained producer recipes,
patches and continuations. Their fresh-run manifest validator is read-only;
new root/tool/guard bindings and an end-to-end fresh build launcher remain
required before claiming standalone reproduction from a clean checkout.


The read-only [audit timing probe](build/audit_budget_probe.py) compares
scheduler settings on a deterministic sample of a reviewed Windows build
tree. Supply the configure recipe and its SHA, the verified human SID, the
compile and verifier source SHAs, and `--rolling-schedule 24 32` to measure
the current replay schedule. The [build contract](build/CONTRACT.md) defines
the checks and time limits used for full replay.
