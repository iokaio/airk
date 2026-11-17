# Model metadata reference inputs

This read-only projector distinguishes retained publisher/rights web pages from
AIRK implementation during source-preservation inventory. It verifies four pinned
families: 1,585 QASPER arXiv metadata pages against the exact bulk-rights receipt,
38 additional and 55 primary versioned pages against exact acquisition records,
and one HotpotQA publisher homepage against its retained content hash and companion
publisher/dataset metadata. It neither interprets license eligibility nor adopts
paper text or training labels.

`families.json` contains only four family definitions and evidence hashes. Downloaded
HTML, private metadata receipts and the full 1,679-row output remain external/private.
`verify_dispositions.py` reads and hashes those inputs; it never executes HTML,
imports captured code, fetches a URL, writes files or starts a process. Python 3.13
standard library is sufficient. Run from the checkout root under the verified human
identity after provisioning the exact evidence at the recorded relative paths:

```powershell
python -B -S scripts/arbiter/analysis/model_metadata_inputs/verify_dispositions.py
python -B -S scripts/arbiter/analysis/model_metadata_inputs/test_dispositions.py
```

`--emit` prints the standard `airk.source-disposition/1` document to stdout. Retain
that output only in a guarded private audit directory; do not commit it or the HTML.
Supply its exact SHA to the source inventory's explicit disposition-map input so
normal per-row source/evidence verification still applies. Missing/drifted evidence
refuses instead of guessing from a folder name. The compact definitions do not prove
whole-artifacts completeness or an absence of authored CUDA in unrelated ORT trees.

The HotpotQA page has one explicit provenance gap: its original acquisition receipt
is unavailable. Its hash and companion records support reference-input classification,
not acquisition-time/transport custody or a publisher Git revision. This gap is
preserved in every projected row and summary. It is not an unpreserved authored-code
finding, and no upstream revision is invented.
