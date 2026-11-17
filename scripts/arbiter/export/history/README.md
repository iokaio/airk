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
| `actual-composed-service-r2/compact-attention-proposal-r1` | Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model. | 2 |
| `actual-composed-service-r2/compact-attention-proposal-r2` | Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model. | 7 |
| `actual-composed-service-r2/compact-attention-proposal-r3` | Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model. | 9 |
| `actual-composed-service-r2/compact-attention-proposal-r4` | Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model. | 11 |
| `actual-composed-service-r2/compact-attention-proposal-r5` | Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model. | 12 |
| `actual-composed-service-r2/compact-gqa-same-artifact-proposal-r1` | Bind compact GQA numerical/reference work to the same actual artifact and prepare an owned six-reference roster; compare provenance and natural process settlement. | 2 |
| `actual-composed-service-r2/compact-gqa-six-reference-preparation-r1` | Bind compact GQA numerical/reference work to the same actual artifact and prepare an owned six-reference roster; compare provenance and natural process settlement. | 4 |
| `actual-composed-service-r2/large-gqa-proposal-r1` | Transform decoder ONNX attention subgraphs into grouped-query attention (GQA), checking query/key/value layout, rotary positions, external tensor ranges, removal sets and graph-shape/numerical semantics. Tiny synthetic graphs and owned runners support those checks; they do not qualify a real model. | 38 |
| `encoder_precision_diagnostic.py` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `intern4-export-r1` | Export or convert the retained decoder model candidate and preserve the original storage, graph and numerical-reference recipe. Publisher assets, trained weights, runtime versions and output seals remain external prerequisites. | 1 |
| `kev4-export-r1` | Export or convert the retained decoder model candidate and preserve the original storage, graph and numerical-reference recipe. Publisher assets, trained weights, runtime versions and output seals remain external prerequisites. | 1 |
| `minilm-native-inputs-mixed-r1` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `minilm-native-inputs-mixed-r4` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `minilm-native-inputs-mixed-r5` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `minilm-native-inputs-mixed-r6` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `minilm-original-mixed-baseline-r1` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `minilm-original-native-inputs-r1` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 1 |
| `native-minilm-original-baseline-r1` | Preserve versioned MiniLM encoder training, precision diagnostics, original/trained baseline export or independent numerical input verification. The runner filename and original family identify which checkpoint/quantization experiment it belongs to; these versions are not interchangeable. | 4 |
| `selected-native-goal-driver-r1` | Retain the exact tool-source snapshot used by a native goal experiment: training execution ownership, encoder/decoder export, numerical references and calibration/development helpers. The snapshot remains distinct from the current tools/retrieval-train owners. | 2 |
| `strands2-export-r2` | Export or convert the retained decoder model candidate and preserve the original storage, graph and numerical-reference recipe. Publisher assets, trained weights, runtime versions and output seals remain external prerequisites. | 1 |
