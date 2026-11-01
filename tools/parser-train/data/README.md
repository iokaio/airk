# Exact parser-training datasets

These deterministic gzip files contain the exact expanded JSONL bytes referenced
by the public model manifests and qualification reports, including historical
failed/unselected campaigns and held-out splits. They are data for review and
reproduction, not evidence of universal query understanding. Reused test fixtures
are regression measurements, not fresh blind tests. Do not train on held-out rows.

`catalog.json` records original paths, raw/compressed hashes and sizes, row counts,
roles, splits, provenance and model references. Identical raw datasets are stored
once. `current_model.selected_training` identifies the exact fitted examples;
`current_model.training_pool` identifies the larger selection pool when present.
Historical totals can include overlapping pools/subsets and are not counts of
unique utterances or the current fitted model's training size.
Original AIRK templates are Apache-2.0; public vocabulary and query inputs
retain their pinned source licenses and attribution in `licenses/`. License
texts remain plain `.txt`; the two pinned upstream README provenance
artifacts are exact `.txt.gz` files, retaining their original Markdown whitespace.
Their catalog/attribution entries distinguish stored gzip hashes/sizes from the
original raw hashes/sizes. Preview them with `gzip -dc` or Python's `gzip.open`.
Training dependency pins remain in `../requirements.txt`; no dependency implementation,
source checkout, private corpus, vocabulary dump or native index is distributed.

Preview: `gzip -dc FILE.jsonl.gz` or Python's `gzip.open(FILE, 'rt')`.
Verify from the repository root:
`python tools/parser-train/export_datasets.py verify`
Restore the exact original ignored target paths for fitting/evaluation:
`python tools/parser-train/export_datasets.py restore`
Restore refuses to overwrite different existing bytes. Model building still needs
the pinned Python training dependencies. Exact fitted-data replay and custom
extra-training through `../fit-saved.py` use the restored examples and held-out
sets without source checkouts or harvested vocabulary. Only full corpus
regeneration needs the verified public vocabulary inputs described in
`../TRAINING.md`. The expanded examples can be inspected without training tools.
See the contributor workflow in `../../../docs/parser-training.md`.

After an authorized campaign freezes, refresh the reviewed branch data with:
`python tools/parser-train/export_datasets.py publish`
Publication preparation never stages, commits, pushes or uploads files. Raw
training directories, local retained copies, clones and native indexes stay
ignored. The publisher refuses unknown row provenance instead of redacting or
silently changing the exact trained examples. gzip mtime/name fields are fixed;
compressed bytes are reproducible with the same Python/zlib implementation.
Validation checks exact hashes, row schemas and declared source/model identities;
it does not certify labels or reconstruct every historical template. Review the
public generation scripts, reports and source inventory when assessing quality.
The local `.gitattributes` preserves frozen JSON, compressed data and original
license text bytes across Git staging/checkouts; do not normalize those files.
Its `cr-at-eol` whitespace setting accepts original CRLF endings while retaining
the normal end-of-line, end-of-file and space-before-tab whitespace checks.
