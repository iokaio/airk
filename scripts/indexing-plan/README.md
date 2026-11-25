# Indexing plan measurements

Scripts for the [indexing performance plan](../../docs/indexing/performance-plan.md).
They measure how long indexing takes and where the time goes, before and
after the plan's changes, so the two can be compared on the same inputs.

## `baseline.py`

Runs one indexing operation through the kernel's component harness and writes
a receipt.

- **Input:** the `vector_performance` test binary of `airk-kernel`, built with
  the `airk-core/stage-timing` feature; a private copy of a source tree; an
  empty or existing index state folder for that tree.
- **Output:** one JSON receipt for each operation, with wall time, the
  kernel's index report, stage totals and the size of the index on disk.
  A receipt holds counts and timings only. The report's path-bearing
  diagnostic lists are reduced to their lengths, and search results to a hit
  count.

Each operation is a new process of the harness, so a search sample includes
opening the index and loading the model. It is not the latency of a running
service.

### Build the harness

Build through the verified static runtime environment, as
[the model document](../../docs/onnx-models.md) describes:

```console
python scripts/static-runtime-environment.py --environment-file <verified-report> -- cargo test -p airk-kernel --test vector_performance --features airk-core/stage-timing --locked --offline --timings --no-run
```

Without the feature the harness still runs and the receipts' stage lists are
empty.

### Measure

```console
python scripts/indexing-plan/baseline.py run --harness <vector_performance.exe> --source <tree> --data <state> --temp <scratch> --label munarium-cold --operation index --output <receipt.json>
python scripts/indexing-plan/baseline.py run ... --label munarium-rescan --operation index --output <receipt.json>
python scripts/indexing-plan/baseline.py run ... --label munarium-edit --operation edit --edit-file README.md --marker <unique-token> --output <receipt.json>
python scripts/indexing-plan/baseline.py run ... --operation install-model --model-dir <profile-folder> --output <receipt.json>
python scripts/indexing-plan/baseline.py run ... --operation search --query "<text>" --mode vector --output <receipt.json>
python scripts/indexing-plan/baseline.py summary <receipt.json> <receipt.json>
```

Operations:

| Operation | What it measures |
|---|---|
| `index` | An index pass. On an empty state folder it is the cold pass; repeated, it is the rescan with nothing changed. With a model installed it also builds every vector before it returns. |
| `reindex` | A full rebuild. |
| `edit` | Appends one marker line to a file, indexes, searches for the marker, and restores the file. |
| `install-model` | Installs one embedding model profile into the state folder. |
| `search` | Repeated lexical or vector searches, each in a new process. |
| `status` | The kernel's status for the state folder. |

The `edit` operation writes to the source tree and restores it afterwards.
Give it a private copy, never a working repository.

`summary` prints one row for each receipt and its largest stages. A parent
stage includes its children: `publish` includes `publish.metadata`, which
includes `publish.metadata.load`.

## `run-baseline.ps1`

Runs the fixed sequence the plan compares before and after (steps P0.3 and
P7.2), one receipt for each operation:

| Step | Operations |
|---|---|
| `small-base` | cold index, rescan, one edited file |
| `large-base` | cold index, rescan |
| `small-vectors` | install the model, index with every vector built, lexical and vector search samples |

- **Input:** the harness, a run folder that already holds `sources\<name>`
  for each source plus the folders `data`, `receipts`, `logs` and `tmp`, the
  repository root, the verified human's SID, and for `small-vectors` one
  model profile folder.
- **Output:** receipts in `receipts\`, one command line for each operation in
  `logs\commands.log`, and an ownership audit of each index it creates.

```powershell
pwsh -NoProfile -File scripts/indexing-plan/run-baseline.ps1 -Harness <vector_performance.exe> -Run <run-folder> -Root <repository> -ExpectedSid <human-SID> -Small <small-source-name> -Large <large-source-name> -ModelDir <profile-folder>
```

It refuses to reuse a state folder or overwrite a receipt, so a repeated run
needs a new run folder. It runs as the verified human and checks each output
folder with the guard in `scripts/dev/assert-human-execution.ps1`.

## `seam_census.py`

Counts the call sites of the old index store's public functions, for step
P1.1 of the plan.

- **Input:** the repository's `crates` folder. It reads source text only.
- **Output:** a Markdown table on standard output and, with `--json`, the
  full census: each public function of the reader, the writer and the file
  store, where it is defined, and how many textual call sites each crate
  has, apart from tests.

```console
python -I -B scripts/indexing-plan/seam_census.py --crates crates --json <census.json>
```

A count is a sizing aid. A common name such as `head` also matches calls on
other types; the table marks those names. The compiler is the authority once
the old functions are removed.

### Where receipts go

Receipts, state folders, source copies and scratch files belong in the
plan's private folder under `artifacts/indexing-plan/`, which Git ignores.
Only this folder's scripts and the figures quoted in the plan are committed.

## Tests

```console
python -I -B -m unittest discover -s scripts/indexing-plan
```

The tests cover the script's own logic and run no harness.

## `export_minilm_fp16.py`

Prepares the pinned publisher MiniLM weight-only FP16 input. The exporter
stores each of the 101 FP16 initializers once and adds Cast nodes back to
FP32 before the original encoder operations. A Python ORT check compares
it with explicit FP32 widening at four token widths and reports drift
against the original FP32 graph. The fixed source batch size is one.
Native runtime admission, a batched CUDA view and retrieval quality require
their own checks.

- **Input:** publisher acquisition folder, pinned original FP32 export and
  receipt, and the original Apache-2.0 license. Use the provisioned Python
  environment with ONNX, NumPy and CPU ONNX Runtime.
- **Output:** one weight-only ONNX, matching tokenizer, license, manifest and
  proof receipt in a fresh, owner-checked private folder outside Git.

```console
python -I -B scripts/indexing-plan/export_minilm_fp16.py --source <publisher-inputs> --reference <original-fp32-export> --license third_party/models/minilm-v2-LICENSE.txt --output <empty-private-folder>
```

## `export_minilm_fp16_dual.py`

Builds a diagnostic single-ONNX MiniLM asset from the pinned original FP32
export and the weight-only CPU proof. The top-level graph widens embedded
FP16 initializers to FP32. The `airk.cuda_graph.onnx.base64` metadata entry
holds a small ONNX graph with FP16 matrix inputs. Its external
`minilm-fp16.bin` ranges point into the complete top-level ONNX byte buffer,
so both views consume the same half values with one stored weight copy.

The dense graph views use dynamic batches. The proof checks exact CPU
widening and compares in-memory external weight binding against an inline
half view through Python ORT, including 32 inputs at 256 tokens. It does not
run a CUDA provider. Keep this diagnostic separate from catalog admission.

```console
python -I -B scripts/indexing-plan/export_minilm_fp16_dual.py --source <publisher-inputs> --reference <original-fp32-export> --cpu-bundle <prior-cpu-proof-bundle> --output <empty-private-folder>
```

## Provisioning the reviewed FP16 build input

The dual-view export proof remains immutable. The local build input carries
its exact graph and tokenizer plus the Apache-2.0 `LICENSE` from the shipped
publisher MiniLM profile. The license bytes and tokenizer match that profile,
and `model.json` changes only the model digest from the shipped schema-2 ABI.
Provisioning checks the export receipt and all input hashes before publishing
the input files. No model download or native execution occurs.

Create and ownership-check a fresh private build-input root and its
`minilm-l6-v2-fp16-v1` subdirectory, then run:

```console
python -I -B scripts/indexing-plan/provision_minilm_fp16.py --export-bundle <dual-view-proof-bundle> --shipped-license <original-profile-LICENSE> --destination <private-build-input-root>
```

Set `AIRK_BUNDLED_MODEL_DIR` to the parent build-input root. Once the native
policy contains this exact profile, `scripts/provision-local-model.py` can
verify the prepared directory or stage it into another owned root. For an
INT8 historical run, use the exact former `fixtures/models/model-policy.json`
from commit `766ca68f` with `--historical-policy <capture>` and explicitly
provide `--profile <old-id>` and `--source-directory <authentic-old-profile>`.
