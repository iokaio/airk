# Original R8 producer

provision_cuda_runtime.py retains the original full acquisition/configure/build
logic. Its locked loader, offline-helper, frontend-import and placement patches
are captured with exact source hashes in ../source-map.json. The shared loader
patch is one directory above this package, preserving its original relative
layout. test_provision_cuda_runtime.py is the historical regression source.

The original entry required --destination, --approved-script-sha256,
--approved-lock-sha256 and --execute-build, with at most10800 seconds. These
historical flags are documentation, not a ready fresh-run command. Tool/source
archives and CUDA/cuDNN inputs remain externally provisioned, pinned data. Start
with the read-only ../rebuild.py manifest validation. Fresh-run launch bindings
and complete current ownership guards remain required before execution.
