# MiniLM export recipes

These are maintained source copies of ten versioned local export families, including the superseded six-role parent-bag preparation. Original ignored copies remain untouched. `source-map.json` records all 60 source/template/contract/helper origins, exact original and maintained SHA-256 values, reversible edits and 11 groups of identical original bytes. Local helper copies remain beside runners to preserve import and historical package boundaries.

Only code and contracts are retained here. Checkpoints, ONNX weights, tokenizers, corpora, labels, generated model JSON, receipts, private provenance/release documents, environments and third-party snapshots remain external inputs. Do not commit them or write export outputs into this directory. The small literal strings in numerical regression probes are synthetic tests, not a training corpus.

## Prerequisites and status

Use the verified human execution identity and audited output parents described in the repository instructions. These historical workers do not implement the current filesystem guard themselves; an owned launcher must verify the identity, enumerate and audit every output/cache path, and preserve the original process limits. Use only fresh ignored output directories and separately reviewed source/input seals.

The retained export receipts name Torch 2.10.0+cu128, ONNX 1.19.0, ONNX Runtime 1.30.0 and Transformers 5.10.1. NumPy, tokenizers and safetensors are also required; their exact versions and platform closure come from the separately retained runtime seals, not an invented generic requirements file. CPU-only export stages keep their explicit thread/device limits. A CUDA-capable Torch installation does not authorize GPU execution.

The migration has no numerical or model-quality outcome. Re-running numerical gates, native staging, DEV/FINAL experiments, packaging or promotion requires their actual inputs and applicable reviewed execution workflow. Historical receipt hashes remain historical: these maintained source copies have new identities where path/header fixes were necessary. Do not substitute their hashes into old receipts.

## Commands

Run the read-only, standard-library migration checks from the repository root:

```powershell
python -B -S scripts/arbiter/export/minilm/test_migration.py
```

Each recipe README lists its entry points and private prerequisites. The export runner interface is:

```powershell
python -B scripts/arbiter/export/minilm/<recipe>/runner.py --checkpoint <private-checkpoint-directory> --corpus <reviewed-corpus.json> --documentation <reviewed-documentation.json> --output <fresh-ignored-export-directory>
```

This is an actual model export command, not a dry run. It must be dispatched through the reviewed owned-process and same-user output workflow. Supply exact pinned local inputs; source hash guards deliberately refuse unrelated checkpoints or datasets.

## Reproduction limits

The mixed-r3/r4/r5 historical export receipts sealed their runner but omitted the imported helper. Their supplied `signed_dynamic.py`/`safe_int8.py` copies match retained source snapshots and are explicitly marked as not sealed by those historical runs. This migration cannot repair that past provenance gap.

Parent-bag and visible-prefix orchestration also needs private adaptation/provenance/release documents, execution seals, captures and native compiler receipts. Those are deliberately absent. The seven preparation adapters below require an explicit private run root; they retain exact model/data/native pins and refuse missing or tracked output roots before mutation. Generated worker root discovery is independent of staging depth. The old preparation tests that require those documents are retained as implementation evidence; they are not part of the data-free migration test invocation.

Graders now resolve the maintained [blind metadata audit](support/blind-metadata-r5/README.md) and [visible-prefix grader](visible-prefix-r1/grade_candidates.py), with exact updated source pins. Their original pins and every path change remain reversible in the source map. Reusable grading owners remain under `tools/retrieval-train/`. Fresh execution needs new source/input seals; historical receipts are unchanged.

## Recipe index

- [exports-r1](exports-r1/README.md)
- [exports-r2](exports-r2/README.md)
- [mixed-r1](mixed-r1/README.md)
- [mixed-r2](mixed-r2/README.md)
- [mixed-r3](mixed-r3/README.md)
- [mixed-r4](mixed-r4/README.md)
- [mixed-r5](mixed-r5/README.md)
- [mixed-r6](mixed-r6/README.md)
- [native-parent-bag-r1](native-parent-bag-r1/README.md)
- [native-parent-bag-r1/superseded-six-role-preparation-r1](native-parent-bag-r1/superseded-six-role-preparation-r1/README.md)
- [visible-prefix-r1](visible-prefix-r1/README.md)
- [support](support/README.md)

## Private preparation interface

The three visible-prefix generators and current/superseded parent-bag `prepare_seal.py` and `prepare_native_packet.py` now separate maintained `SOURCE_DIR` from private `HERE`. They require `AIRK_RECIPE_RUN_ROOT`: an absolute, existing directory below this repository's `artifacts/`, ignored by Git and containing no tracked files. The shared [path admission helper](support/recipe_paths.py) is read-only; it does not replace the human-identity/ACL guard. The launcher must create and audit the run root and every output parent before execution. Generated filenames must be absent; original receipts and source copies remain untouched.

From the repository root, after that guard preflight:

```powershell
$env:AIRK_RECIPE_RUN_ROOT = '<absolute-ignored-private-run-directory>'
python -B scripts/arbiter/export/minilm/visible-prefix-r1/prepare.py
python -B scripts/arbiter/export/minilm/visible-prefix-r1/prepare_native.py
python -B scripts/arbiter/export/minilm/visible-prefix-r1/prepare_final_native.py
```

`prepare.py` generates runner/helper copies and `adaptation.json` in the private root from canonical tracked training tools. The other two generators create `candidate.rs` and `fresh_final.rs` there. Their historical native source anchors must still match; a changed anchor is a genuine blocker requiring a reviewed adapter update, not removal of a pin. Generated copies need review and new seals before execution; generating source is not compilation or qualification.

For parent-bag preparation, place the separately authenticated private `provenance.json` in the selected run root. Retain its actual checkpoint/training closure and the historical parent runtime seal at their pinned paths. A fresh release must pin the maintained recipe's runner and that private provenance. Then:

```powershell
python -B scripts/arbiter/export/minilm/native-parent-bag-r1/prepare_seal.py --release <reviewed-release.json> --release-sha256 <exact-release-sha256>
python -B scripts/arbiter/export/minilm/native-parent-bag-r1/prepare_native_packet.py --artifact-lock <actual-lock.json> --compile-receipt <actual-compile.json> --output "$env:AIRK_RECIPE_RUN_ROOT/native-packet.json"
```

The CPU seal is written under `cpu-export-execution/` in that root and includes maintained source/helper identity. Native preparation requires the actual matching `execution.json` there, plus the existing exact export/native/reference inputs, and binds a fresh `native/` output under the same root. It performs no CPU or native execution. Existing historical launchers are still historical; a separately reviewed owner must consume the fresh seal and paths. The superseded six-role variant uses the same interface through its own directory and needs its own run root/release; never mix its evidence with the current four-role experiment. No expected artifact, checkpoint, compiler or numerical pin was automatically rewritten.
