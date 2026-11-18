# CUDA cu17 direct warm compile continuation

This compile-only continuation consumes the exact cu17 configure receipt
and failed session-stage receipt with its observed empty Job. The original
receipt, request and logs retain their bytes. The existing Ninja objects
and archive are reused through the original pinned Ninja targets. The
original session archive is separately identified before recovery.
Configure bytes, source, effective CMake/Ninja graph and live output
ownership are checked before and after the actual direct Job stages.

Run with the verified interactive Windows user and the original Python.

```text
python -B -S scripts/cuda-static-runtime/build/fresh_architecture_warm.py
  --configured CU17/fresh-configure.json --configured-sha256 SHA
  --failed-stage CU17/logs/core-session-build.receipt.json --failed-stage-sha256 SHA
  --approved-script-sha256 SHA --output CU17/warm-continuation-r1
```

Use absolute paths. The output directory is a fresh direct child of cu17.
The 10800-second original configure deadline applies. The warm Ninja
`onnxruntime_session` and `all re2` commands run through the existing
suspended Job ownership primitive and must naturally settle. The pinned
CUDA inspector inventories the actual provider SASS and PTX through that
same owner. The distinct `warm-compile.json` retains all new stage evidence
without manufacturing an original process-tree receipt. The standard
fresh aggregate consumer requires a separate reviewed continuation-proof
adapter. This does no model execution or runtime qualification.

Run focused controls with
`python -B -S scripts/cuda-static-runtime/build/test_fresh_architecture_warm.py`.
Optional `AIRK_TEST_WARM_CU17` verifies the original failure read-only.
Optional `AIRK_TEST_WARM_DIRECT_LOG` runs a harmless Python process under
direct Job ownership and retains its log in the guarded workspace.
