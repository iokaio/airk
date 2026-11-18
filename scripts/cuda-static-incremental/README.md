# Incremental final-graph static-core producer

`../cuda-static-incremental.py` owns a fresh, single-translation-unit derivation of the retained CUDA static core. Its verifier supplies build-proof/3 and aggregate-proof/1 to the environment/5 handoff. This is build provenance only, never GPU model, FP16, task or product qualification.

The maintained entrypoint takes `--execute-reviewed-producer --recipe <private recipe.json> --recipe-sha256 <reviewed SHA256> --repo-root <checkout>`. Execution requires a separately reviewed executable recipe; a proposal with `execution_authorized: false` is refused. Do not execute a copy from an artifacts proposal directory after the maintained source has been adopted.

Read [CONTRACT.md](CONTRACT.md) before changing the producer. Tests are `python -B scripts/tests/test_cuda_static_incremental.py`; they use independent synthetic inputs and do not invoke compilers or load CUDA. Private recipes, frozen inventories and actual receipts remain under the campaign evidence directories. They are inputs and evidence, not executable authority to publish a runtime.

## Reproduce the input freeze

Run python -B scripts/cuda-static-incremental/freeze_inputs.py --seed <reviewed-seed.json> --seed-sha256 <exact-sha> in the verified human shell. The seed supplies configurable original/fresh roots and pinned tool/input identities. The generator authenticates its guard and tools, observes the same-user identity, queries only Ninja metadata and the pinned vcvars environment, and prints a non-executable proposal JSON. It does not compile or write files. Retain its stdout only through the checked output-parent and postwrite-owner guards. The retained historical generator remains evidence; this maintained copy owns reproduction. The former private validate-recipe checks are folded into the maintained validate_recipe function and focused tests.

## Historical source mapping

The retained private `freeze-inputs.py` SHA256 `c16e504cf4e8096b198ed2bf3af4f74ece45f7bab4fc6793dc44e2a0dc43a58d` is superseded operationally by maintained `freeze_inputs.py` (configurable, externally pinned seed; same metadata query and input-freeze logic).

The retained private `validate-recipe.py` SHA256 `8a37bbf3a4de3175343f567da506e201e3be2e14bdd37546d0ce73d36b7d566e` is superseded by `validate_recipe` in the maintained owner and independent focused tests. Historical files and receipts retain their old identities; they are not imported or executed by the maintained producer.

## Observe the unchanged base header closure

Run python -B -S scripts/cuda-static-incremental/probe_headers.py --recipe PATH
--recipe-sha256 SHA --output FRESH_DIRECTORY in the checked human shell.
The approved recipe supplies the unchanged base translation unit, actual
compiler pin, environment and compiler arguments. The probe changes the compile
operation to preprocessing and routes temporary files into the fresh directory.
It retains the bounded compiler log and a private observation with independently
hashed toolchain headers. This measures current local toolchain bytes and
produces no archive, runtime, or qualification result.
