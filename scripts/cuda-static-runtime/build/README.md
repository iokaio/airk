# CUDA parent build source and explicit rebuild inputs

## Fresh Windows CUDA static build

The fresh build uses the pinned Visual Studio 2022 MSVC compiler
19.44.35230 and Windows SDK 10.0.26100.0. CMake 3.31.6 completed native
C and C++ compiler identification and ABI detection with this combination.
The CUDA configuration selects CUDA Toolkit 12.8.1 (nvcc 12.8.93),
cuDNN 9.10.2.21, Ninja 1.12.1, Python 3.13.7 and GNU patch 2.7.6.
The CUDA build targets SM86 and uses C++20. The configure driver
spells the exact MSVC path with forward slashes so CMake preserves it
when passing the host compiler argument to nvcc.

The configure recipe binds the exact tools, compiler environment, source
and dependency bytes, fresh output root and original time limit. Keep
.EXE in the pinned PATHEXT so nvcc can resolve its Toolkit helpers
after Visual Studio initializes the compiler environment.
[Checked Windows execution](../../../CONTRIBUTING.md#checked-windows-execution)
[Build contract](CONTRACT.md)

Run these stages in order with the pinned Python executable and reviewed
input hashes.

```text
python -B -S scripts/cuda-static-runtime/build/fresh_configure.py --recipe PATH --recipe-sha256 SHA
python -B -S scripts/cuda-static-runtime/build/fresh_compile.py --configured PATH --configured-sha256 SHA --approved-script-sha256 SHA
python -B -S scripts/cuda-static-runtime/build/fresh_aggregate.py --recipe PATH --recipe-sha256 SHA
```

Compile consumes the settled configure receipt. Aggregate consumes a
reviewed archive recipe and the compile receipt. CUDA compiler detection, the
session target and archive assembly each require their corresponding receipts.

## Historical parent source capture

The versioned r8, r12 and checkpoint-v1 packages capture the actual producer,
continuation, complete-process-tree, checkpoint and composer implementation used
in the retained parent build path. source-map.json records all 21 source/input
mappings, original SHA256 values, maintained SHA256 values, SPDX transitions and
the original target commands. Fifteen formerly ignored files are captured here;
six already maintained acquisition/archive/verification owners are referenced.

These are historical source versions, not a claim that an old command can safely
be launched against a fresh tree. The original f9fac producer, 042c aggregate,
receipts and campaign deadline remain historical identities. New executions must
have fresh source identities, explicit inputs, output guards and approved
deadlines; they cannot be retagged as the old parent or treated as qualified.

The historical manifest validator is read-only:
    python -S -B scripts/cuda-static-runtime/build/rebuild.py --input-manifest PATH --manifest-sha256 SHA

It validates airk.cuda-parent-rebuild-input/1, exact ordered stages
(r8, r12, flash, llm, final, compose, aggregate), pinned maintained source,
explicit input contracts and input closure, a new future UTC cutoff and proposed
fresh output directories. It does not create directories, install tools, launch
commands, compile, sign or infer. It has no execution switch.

The manifest fields are schema, repository_root, campaign_cutoff_utc, stages,
inputs, output_directories and qualification=false. Each stage contains name,
source FilePin, input_contract FilePin and an explicit argv list. Every FilePin
has absolute path, positive integer bytes and lowercase SHA256. Do not use fake
pins or invent private fixtures. Real receipts, source inventories, downloaded
dependencies, caches and model/training data remain outside tracked source.

The historical manifest has no launch binding: legacy HERE-sidecar paths, fixed target
plan/runner authority, stage output names, process/output guards and historical
cutoff handling need a reviewed binding to the new manifest. This capture makes
the full implementation reviewable and available for reproduction; it is not an
end-to-end rebuild or qualification result. Do not execute historical mains as a
workaround. Current retained-parent verification uses ../verification instead.

Exact-file .gitattributes rules preserve authenticated source bytes, including
CRLF, across Git filtering and fresh checkouts. Do not normalize protected files
without deliberately updating their reviewed source/provenance identities.
