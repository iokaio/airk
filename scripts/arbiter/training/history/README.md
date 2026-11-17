# Historical model reproduction source

These files preserve authored model-training/conversion and qualification-support
recipes. They are not supported new-run launchers. Original bodies are byte-identical;
only a missing Apache-2.0 SPDX header was prefixed. `source-map.json` records all
original paths/hashes, canonical destinations, lengths and reversible SPDX prefixes.
Identical originals share an owner; original sibling layout is recorded by each
source path even when its canonical owner is in a different version folder.

Do not execute or import these histories in place. Some execute at import time,
use historical paths or depend on private sibling metadata. Future reproduction
requires reconstructing the complete recipe layout into an approved ignored tree,
provisioning exact licensed inputs/runtime closure, then reviewing path/source
changes and producing fresh owned-process seals. No automatic materializer or
execution path is provided. Old receipts are never retagged for new source.

No corpora, training examples, model weights, ONNX binaries, credentials, runtime
environments, vendor source or receipts are included. Seeds, graph/training math,
fixed source/input hashes and process limits remain verbatim in the preserved code.
Exact runtime/tool versions must come from the original reviewed environment seals;
a generic requirements file would not reproduce their platform/native closure.

Read-only verification (source hashes, inverse SPDX identity and Python parsing;
no model imports, processes or writes):

```powershell
python -B -S scripts/arbiter/training/history/verify_history.py
```

Add `--originals` only where retained private artifact copies are provisioned; it
compares their bytes but does not execute them. The two history maps jointly account
for 189 authored paths/135 distinct original bodies. The Requests vendor/corpus
exclusions are recorded in training/history/excluded-inputs.json (18 paths, no payload).

## Family roles

| Original family | What the code does | Paths |
|---|---|---|
| `a1-visible-prefix-training-execution-r1` | Probe and seal the MiniLM training runtime, verify its retained closure, and preserve the early process-owner implementation/tests from before suspended child creation. The pre-suspend owner is historical evidence, not the corrected supported launcher. | 5 |
| `context-action-native-calibration-fit-r40` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `context-action-qwen-shared-experiment-r1` | Compare and prepare context/action model reference behavior for the retained shared experiment, preserving its input, source and owned-runtime bindings. | 4 |
| `minilm-development-r2` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `minilm-trained-r2` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `native-hybrid-calibration-capture-r1` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `native-hybrid-calibration-scores-cpu24-r1-arbiter-large-intern-4b` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `native-hybrid-calibration-scores-cpu24-r1-arbiter-large-kev-4b` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `native-hybrid-calibration-scores-cpu24-r1-arbiter-small-intern-08b` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `native-hybrid-calibration-scores-cpu24-r1-arbiter-small-strands-2b` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `native-minilm-production-decision-r1` | Prepare MiniLM model-rights/legal payload consumers and native source proposals, checking trained input identity. This prepares a decision artifact; it does not promote a model. | 3 |
| `native-parent-bag-grader-streaming-r1` | Grade weak-parent-bag candidate native DEV captures with bounded streaming and exact source/input provenance; preserve the grading and amendment regression evidence. | 4 |
| `native-parent-bag-native-r95-preparation-r1` | Prepare parent-bag native experiment packets, bind compiler/image/runtime evidence, retain owned process lifetime and phase deadlines, and validate source/receipt closure before numerical or DEV execution. | 11 |
| `native-parent-bag-native-run-preparation-r1` | Prepare parent-bag native experiment packets, bind compiler/image/runtime evidence, retain owned process lifetime and phase deadlines, and validate source/receipt closure before numerical or DEV execution. | 11 |
| `native-parent-bag-native-run-preparation-r2` | Prepare parent-bag native experiment packets, bind compiler/image/runtime evidence, retain owned process lifetime and phase deadlines, and validate source/receipt closure before numerical or DEV execution. | 13 |
| `native-parent-bag-proposal-r1` | Prepare TRAIN-only parent source/unit fixtures and native-stage proposals for weak parent bags; verify source identity and owned staging without inventing fragment-positive labels. | 12 |
| `qwen3-challenger-assets-r1` | Acquire explicitly pinned model assets from the original publisher manifest. Network access and model downloads are not performed by this migration; exact manifests, licenses and private output roots remain prerequisites. | 1 |
| `qwen3-challengers-r1` | Run the retained native build-stage command with compiler/process evidence for challenger model comparison. Historical build commands and paths remain evidence, not an authorized new native build. | 2 |
| `qwen3-reranker06-assets-r1` | Acquire explicitly pinned model assets from the original publisher manifest. Network access and model downloads are not performed by this migration; exact manifests, licenses and private output roots remain prerequisites. | 1 |
| `review-calibration-utility-r1.py` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `run-native-calibration-score-r1.ps1` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `run-native-calibration-step-r1.ps1` | Prepare, fit or summarize calibration against retained native model scores and fixed label/source inputs; preserve bounded native calibration launch/step helpers. No labels or score datasets are included and no terminal class becomes available from source preservation. | 1 |
| `selected-model-rights-r1` | Preserve the model conversion/storage-normalization source identity captured as rights evidence; executable code only, with no publisher assets or rights-document payload. | 2 |
| `selected-native-goal-driver-r1` | Retain the exact tool-source snapshot used by a native goal experiment: training execution ownership, encoder/decoder export, numerical references and calibration/development helpers. The snapshot remains distinct from the current tools/retrieval-train owners. | 8 |
