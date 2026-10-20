# AIRK 1.2.2

Version 1.2.2 fixes unfinished semantic indexing in the VS Code extension. In 1.2.0 and 1.2.1, **Index now** could finish the saved-file pass and leave semantic work pending, often at the first 32-file batch. The extension marked that request complete but did not drive the remaining native batches. A separately running scheduler could mask the problem. Saved-file coverage could correctly report 100% while the vector index remained partial.

The extension now advances bounded semantic batches for its trusted, active workspace collections after indexing and on reopen. This continues with the dashboard hidden and in Manual mode for work already requested. It does not install a background service. Closing the workspace stops further batches; the next trusted session discovers retained pending work. A running batch may finish before pause, storage selection or shutdown takes effect.

The native `semantic run --scheduled` command advances at most one 32-artifact batch for the selected collection. It uses the existing storage-root worker lock, respects saved pause and blocked failures, and never executes source jobs or enrichment for another collection. The unflagged `semantic run` remains the explicit command to drain retained work. Model or storage failures stop automatic retries and remain visible; Resume or a new index request permits another attempt.

The dashboard and status bar distinguish semantic work from saved-file indexing. The completed/selected counts refresh after each batch, and the collection reports **Building semantic index** or **Semantic indexing pending**. The 100% coverage bar retains its existing saved-file definition.

No storage format or model identity changes are introduced. Upgrade the extension and its matching bundled helper together. Existing 1.2 pending work can continue without a full reindex. Native and extension versions and their lockfiles are synchronized at 1.2.2.

Regression coverage exercises service-free continuation after Index now and reopen, hidden-dashboard and Manual behavior, pause/resume, failure blocking, worker contention, collection isolation, storage selection and disposal. Release publication still requires CI for the exact main commit and the repository's explicit release workflows; this document is not a publication receipt.
