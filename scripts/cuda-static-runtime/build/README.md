# CUDA parent build source and explicit rebuild inputs

## Fresh Windows CUDA static build

The fresh build uses the pinned Visual Studio 2022 MSVC compiler
19.44.35230 and Windows SDK 10.0.26100.0. CMake 3.31.6 completed the native
compiler checks, CUDA provider build, and static archive assembly with
this toolchain on Windows.
The CUDA configuration selects CUDA Toolkit 12.8.1 (nvcc 12.8.93),
cuDNN 9.10.2.21, Ninja 1.12.1, Python 3.13.7 and GNU patch 2.7.6.
The CUDA build targets SM86 and uses C++20. The configure driver
spells the exact MSVC path with forward slashes so CMake preserves it
when passing the host compiler argument to nvcc. The MSVC CUDA provider patch
keeps the legacy alternative-token header available to ORT and CUTLASS and
keeps warnings as errors enabled. Its private compile definition applies only
to the header's C++20 deprecation for this provider's C++ and CUDA targets.

The configure recipe binds the exact tools, compiler environment, source
and dependency bytes, fresh output root and original time limit. Keep
.EXE in the pinned PATHEXT so nvcc can resolve its Toolkit helpers
after Visual Studio initializes the compiler environment.
[Checked Windows execution](../../../CONTRIBUTING.md#checked-windows-execution)
[Build contract](CONTRACT.md)

Before starting the build clock, check the effective write permissions of
the exact recipe, repository source owners, build tools, patches, and their
parent folders. The native Windows manifest assembler checks write access as
well as ownership. A checkout can have the right owner and inherit Modify
grants from the drive. Use a protected checkout or have its owner preview the
[checkout permissions helper](../../dev/protect-airk-checkout.ps1). Recheck
the original hashes and permissions after any repair.

Run these stages in order with the pinned Python executable and reviewed
input hashes.

```text
python -B -S scripts/cuda-static-runtime/build/fresh_configure.py --recipe PATH --recipe-sha256 SHA
python -B -S scripts/cuda-static-runtime/build/fresh_compile.py --configured PATH --configured-sha256 SHA --approved-script-sha256 SHA
python -B -S scripts/cuda-static-runtime/build/fresh_aggregate.py --recipe PATH --recipe-sha256 SHA
```

The Windows parent build has up to 10,800 seconds from configure launch
through compilation and archive assembly. The campaign cutoff can shorten
this window. Stage receipts include natural settlement of the owned command
and its descendants.
The reviewed recipe can set `parallel_workers` to 2 or 8. Omitting it
selects two. Eight Ninja slots require measured host headroom for several
simultaneous NVCC pipelines. Both target stages record the same selected
worker count, and the existing NVCC and Flash thread settings still apply.

Compile consumes the settled configure receipt. Aggregate consumes a
reviewed archive recipe and the compile receipt. CUDA compiler detection, the
session target and archive assembly each require their corresponding receipts.

`fresh_verify.py` reopens an aggregate by its reviewed file pin and
checks the recorded compile lineage, archive membership, exports and
process settlement. Its caller supplies a read-only deadline. The check
returns observed facts in memory and preserves the recorded execution window.
`fresh_projection.py` derives the private build proof before archive launch
and checks the recorded command-plan binding when projecting aggregate evidence.

Verify the successful aggregate and print the private static handoff with
`python -B -S scripts/provision-cuda-static.py --destination BUILD_ROOT --fresh-aggregate-receipt RECEIPT --receipt-sha256 SHA --verification-seconds SECONDS`.
Pass `--write-environment BUILD_ROOT/environment.json` to retain it as
a new, checked file under the verified build root.

## Recover a timed-out configure output audit

Use `fresh_finalize.py --recipe PATH --recipe-sha256 SHA` after the
original configure command and its process tree have settled with exit 0
and the following file audit hit its per-call limit. The reviewed recovery
recipe pins the original configure recipe, successful stage receipt, failed
outer receipt and log, and maintained recovery owners. It retains the
failed evidence and checks the complete existing build tree and reconstructed
source inputs under the original deadline. The separate recovery receipt binds
the resulting configure receipt.

The configure, compile and recovery audit scheduler starts with batches of 48 paths and
up to eight workers. A timed-out batch can be split into smaller batches under
the same original deadline. Each new call uses the existing ten-second guard
limit and rechecks every path. Other guard refusals propagate immediately.

For a read-only timing comparison on 128 retained generated paths, run
`python -B -S scripts/cuda-static-runtime/build/audit_budget_probe.py --recipe PATH --recipe-sha256 SHA --human-sid SID`.
The probe tries 10 and 16 seconds in separate child processes and prints
the sample identity, elapsed time and complete-audit result for each.

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

## Prospective Windows build on D

Run `fresh_windows_d_job.py prepare` from the reviewed, DCO-signed checkout.
It verifies the original CU15 archive, native and compiler inputs and writes
a fresh bound recipe beneath the campaign receipt directory. Use the returned
review SHA with `launch --sha256 SHA --other-host-gib N --other-disk-gib N
--exclude-mask MASK` after reconciling outstanding reservations and the
physical siblings used by calibration. The single owned Job checks measured
volume growth, original request clocks and actual native compiler placement.
The only allowed destination is `artifacts/v190/final-plan/cu18`. Source
archives and installed compilers are read in place. The private receipt
records a prospective build and the ensuing environment verifier consumes
the same version-six fields. Product projection and native execution have
their own subsequent acceptance checks.
