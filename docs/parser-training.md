# Experiment with AIRK parser training in a fork

AIRK's parser maps supported requests to bounded, read-only evidence operations. A fixed local classifier supplies proposed plans when deterministic parsing does not recognize a request. Training an intent model does not teach AIRK to generate answers or enable learned automatic execution: learned proposals still require review and an explicit plan submission.

This guide is for contributors using their own fork and development environment. Run commands from the repository root. The [training record](../tools/parser-train/TRAINING.md) explains measured campaigns and attribution; the [trainer README](../tools/parser-train/README.md) describes the model format and qualification gates.

## Inspect and restore the included data

The canonical [training-data directory](../tools/parser-train/data/README.md) contains deterministic `.jsonl.gz` files with exact expanded examples, a [catalog](../tools/parser-train/data/catalog.json), and original source [licenses and attribution](../tools/parser-train/data/licenses/attribution.json). The catalog records raw/compressed SHA-256 values, original paths, row counts, roles, splits, provenance and model/report references. The 1.8.2 catalog contains 23 canonical corpora: 495,280 summed rows, 306,305,977 raw bytes and 29,002,254 gzip bytes (27.66 MiB compressed). Historical failed and unselected campaigns remain identifiable. Identical raw datasets are stored once; adding historical row counts does not measure unique current-training utterances.

```console
python tools/parser-train/export_datasets.py verify
python tools/parser-train/export_datasets.py restore
```

Both commands work without network access or training dependencies. Restore recreates exact ignored `target/parser-train/data/` inputs and refuses to overwrite different existing bytes. Preview a chosen catalog entry with `gzip -dc FILE.jsonl.gz`, or `python -c "import gzip; print(gzip.open('FILE.jsonl.gz', 'rt', encoding='utf-8').readline())"`.

The current [model manifest](../tools/parser-train/manifest.json) names the embedded asset, evaluated report and exact data identities. Distinguish a generated training **pool** from the **fitted subset** selected by validation: the protocol-5 pool has 96,000 rows, while the installed review candidate fits exactly 48,000 (40,000 supported, 6,000 original unsupported and 2,000 public CLINC requests). The [replay receipt](../tools/parser-train/replay-v5.json) records a byte-identical rebuild of its asset from those selected rows. Read `fitted_training`, `selected_total_training_rows` and `fit_parameters` in the evaluated report instead of treating the entire pool as fitted data. Validation, calibration, repository evaluation and unsupported-test splits are separate data, not additional training rows.

## Prepare a development environment

Use Python 3.11+, the pinned Rust/native build prerequisites in [CONTRIBUTING](../CONTRIBUTING.md), and a Python environment for the development-only packages:

```console
python -m pip install -r tools/parser-train/requirements.txt
```

## Replay the selected fit or add your own JSONL offline

After verify/restore and installing the pinned Python dependencies, replay the exact selected rows without vocabulary extraction or source checkouts:

```console
python tools/parser-train/fit-saved.py --report tools/parser-train/qualification-broader-v5.json
```

The replay checks the recorded data/row identities and requires the resulting asset digest to match. It writes an ignored receipt/model, preserving the product asset. Different Python/native numerical environments can fail the exact-digest check rather than silently being called a reproduction.

For a local experiment, create `target/parser-train/my-examples.jsonl`, one original labeled request per line, for example:

```json
{"text":"Locate the state type SessionGate.","intent":"find_definition","role":"developer"}
```

`text` must contain 1–4,096 UTF-8 bytes, `intent` must be one of the 13 labels in `train.py` or `unsupported`, and `role` is recommended for your own evaluation. Then run:

```console
python tools/parser-train/fit-saved.py --report tools/parser-train/qualification-broader-v5.json --extra-training target/parser-train/my-examples.jsonl
```

This appends your rows to the exact selected training bytes, preserves their digest/count, refits with the recorded hyperparameters, recalibrates against saved calibration and reports validation/calibration observations. It rejects exact normalized overlap with those held-out sets. It makes an explicitly **unqualified local experiment**, without editing the embedded asset, manifest or catalog. Its receipt is not an evaluated `broader-phrasing` report and cannot be passed to `install_review.py`. Keep new phrase families separately for evaluation; do not infer final-test accuracy from these observations.

## Prepare full corpus regeneration

The compressed corpora are sufficient for inspection and exact input recovery. The current regeneration pipeline also needs verified vocabulary samples from pinned public repositories and pinned public query inputs; it does not reconstruct those inputs from the gzip files. On a clean fork, provision native build inputs as described in CONTRIBUTING, then run:

```console
cargo build -p airk-cli --features parser-training --locked
python tools/parser-train/harvest.py --acquire
python tools/parser-train/harvest.py --extract target/debug/airk.exe --sample
python tools/parser-train/online.py
```

On macOS/Linux use `target/debug/airk` instead of `target/debug/airk.exe`. Acquisition and `online.py` download the pinned development inputs and verify their identities; fitting is offline after those inputs exist. `repositories.lock.json` and `online.lock.json` preserve revisions and attribution. Native `dump-vocab` exists only in the `parser-training` development build. Source checkouts, sampled source bodies, vocabulary exports, index state and local retention remain ignored and are excluded from runtime/VSIX payloads.

## Add original examples

The public [role inventory](../tools/parser-train/role-queries.json) contains developer, writer, architect and QA/tester requests. Edit its `queries` or add examples with the same schema. Phrase banks in `phrasing.py` and `conversational_v4.py` are also original training sources. For example:

```json
{
  "text": "Find the struct {s}.",
  "intent": "find_definition",
  "role": "developer",
  "family": "developer/my.symbol.type-first",
  "target": "{s}",
  "expected_tools": ["symbols", "outline", "read_file"],
  "label_provenance": "Original fictional example; label review pending."
}
```

`intent` selects a supported label from `train.py`, or `unsupported`. `role` is `developer`, `writer`, `architect` or `tester`. `expected_tools` and `target` describe the intended native operation and literal extraction, separately from intent accuracy. Use one `family` for related paraphrases. Counts in `source_counts` and the report's `role_source` are regenerated from the actual inventory. Review/update authorship and provenance when adding your own examples; do not retain agent-authored provenance for differently authored contributions. Slots include `{s}` for a symbol/topic, `{p}` for a path, `{scope}` for a path prefix, `{k}` for a kind and `{token}`/`{baseline}` for an exact change token. Unsupported/missing-selection examples need corresponding abstention expectations. Keep provenance accurate: existing agent-authored labels remain open to human review.

Use original or appropriately licensed examples and review any data before adding it to public source. The exporter rejects unknown provenance; it does not silently redact examples or certify their labels. Custom examples stay development inputs and are not indexed or shipped in the installed runtime merely because they are used for training.

## Regenerate splits, fit and evaluate

`broaden.py` calls `role_campaign.prepare()` to regenerate the [joint split authority](../tools/parser-train/role-phrase-splits-v5.json). It unions whole baseline, conversational and role families, including equivalent templates across sources, before assigning train/validation/calibration. `split` and `split_group` are generated fields; do not hand-place a paraphrase of a training request into held-out data. Contradictory labels and normalized cross-split duplicates are errors.

```console
python tools/parser-train/broaden.py
python tools/parser-train/broaden.py --evaluate PATH_TO_FROZEN_REPORT
```

The first command prints its actual hash-named `frozen_report` path. Pass that path to the second command; it evaluates the frozen asset without refitting or adjusting thresholds and prints the evaluated report path. Development validation selects the candidate, calibration selects thresholds, and final fixtures measure the frozen choice. Preserve the hash-named data/model/report records and prior versioned reports; do not replace a historical result with a later pass.

Inspect intent accuracy, accepted precision, abstention, unsupported acceptance, per-role results, literal slots and whole-plan behavior separately. Include genuinely new held-out phrases in your fork. Public role examples and repeatedly measured repository/150-request/conversational fixtures are known regressions, not fresh unseen tests. Publishing data does not itself put held-out rows into training; fitting or tuning against their labels does.

## Review an asset and test the native parser

After reviewing the evaluated report and its failed gates, the explicit local-review installer is:

```console
python tools/parser-train/install_review.py PATH_TO_EVALUATED_REPORT --suggestions-only
cargo test -p airk-retrieval intent --locked
cargo test -p airk-kernel workbench --locked
cargo build -p airk-cli --locked
```

The installer can refuse the candidate. On success it preserves the previous asset/records and updates the embedded asset, manifest and Python/Rust inference goldens. It installs a suggestion-only model; it does not qualify or turn on learned automatic execution. Correct labels alone do not prove correct slots, scope, budgets, citations or safe compound-request handling. Changes to those runtime behaviors need native parser/plan tests as well as model measurements.

For a changed installable build, assign a new model/campaign identity and synchronized application patch using [versioning](versioning.md); preserve previous reports, manifests and data identities. Rebuild the native runtime pair and extension, regenerate the version-bearing MCP template catalog, and run the applicable CONTRIBUTING checks. With its model/runtime and Node dependencies provisioned, the corresponding local commands are:

```console
python scripts/build-service-runtime.py --destination target/service-runtime --profile debug
node packages/vscode/node_modules/typescript/bin/tsc -p packages/vscode
node packages/vscode/scripts/build-explorer.cjs --write-mcp-catalog
npm test --prefix packages/vscode
python scripts/release-version.py check
```

Reprovision an installed runtime through the normal candidate installer so its compiled asset actually changes; editing source or a JSON manifest does not update a running service. This parser correction was prepared as candidate 1.8.2 on November 1, 2026 and is included in the consolidated 1.8.7 release. Honor the version bump already completed for a PR; do not repeat it for the same change.

After a reviewed campaign, `python tools/parser-train/export_datasets.py publish` refreshes the canonical compressed inputs and catalog, followed by `verify`. That command prepares source files; it never stages, commits, pushes or uploads them. Track only reviewed data, required reports, source attribution and the narrow allowed data paths. Raw local directories, clones, private corpora and native index state remain excluded.
