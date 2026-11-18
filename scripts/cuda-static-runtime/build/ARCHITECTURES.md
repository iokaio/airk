# Windows CUDA architecture compile candidate

The retained cu15 driver and verified source roster have the original
SM86-only meanings. Their scripts, private inputs and provenance receipts
stay available under their existing paths and hashes.

The additive producer accepts the exact ordered architecture list
`86;89;120`. The inherited configure, resource, process tree, output and
recovery rules apply to these candidate stages. The former campaign
calendar cutoff remains part of historical invocations. The candidate
uses the reviewed recipe's original wall allowance, up to 10,800 seconds
from configure start. Compilation and code inspection consume the same
original window.

Start from a reviewed fresh configure recipe with an unused protected
build root and the same pinned tool, patch, archive and environment
closure. Use this candidate schema and three additional fields.

| Field | Required value |
| --- | --- |
| `schema` | `airk.private.cuda-fresh-architecture-recipe/1` |
| `cuda_architectures` | `86;89;120` |
| `cuda_architecture_owner` | FilePin of the exact reviewed `fresh_architectures.py` |
| `cuda_code_inspector` | FilePin of the installed CUDA 12.8 `bin/cuobjdump.exe` |

The existing configure recipe's `owners` and `output_audit_owner` retain
the exact reviewed historical helper source pins. The new architecture
owner applies a single explicit CMake argument replacement in its private
process and records its own pin in the recipe and code receipt. The
generated CMake cache and direct CUDA compiler edges must agree with
the selected architecture list. Pinned ORT's MSVC source emits
`sm_86`, `sm_89` and `sm_120a` in its general provider and selects
`compute_120` PTX for its Windows LLM target. The independent code
census binds the actual compiled CUDA provider DLL and its real and
virtual code objects.

Run with the verified interactive Windows human and reviewed Python.

```text
python -B -S scripts/cuda-static-runtime/build/fresh_architectures.py configure
  --input REVIEWED_RECIPE --input-sha256 SHA --approved-script-sha256 SHA
python -B -S scripts/cuda-static-runtime/build/fresh_architectures.py compile
  --input COMPLETED_CONFIGURE_RECEIPT --input-sha256 SHA --approved-script-sha256 SHA
```

Use `finalize` with the existing checked configure-audit recovery
recipe if the originally successful configure command settled and its
output audit failed. The recovery input retains the original candidate
configure recipe. The existing recovery checks and original deadline
still apply.

Compilation retains the existing stage evidence and a separate
`fresh-cuda-code.json` receipt with exact linked provider, pinned
inspector, both settled cuobjdump commands and observed code counts.
This is a compile candidate. A separately reviewed multi-architecture
aggregate/runtime/preservation consumer and actual RTX 40 and RTX 50
hardware runs are required for qualification.

Run focused controls with
`python -B -S scripts/cuda-static-runtime/build/test_fresh_architectures.py`.
