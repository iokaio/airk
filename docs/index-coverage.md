# Workspace indexing coverage plan

The extension should show how much of the active folder's eligible content is indexed and current, with a click to update it. This is a planned addition; today's status item shows scheduler state and opens AIRK controls. Snapshot file counts alone cannot establish coverage.

## Definition and eligibility

Use `current indexed eligible files / eligible files in the latest completed workspace inventory` as the main percentage. Also report indexed-but-stale, missing, extraction-failed and capacity-blocked counts. Previously indexed files deleted from the workspace belong to pending removals, not the denominator. Unsupported/binary formats, ignored/generated/secret files, excluded symlinks and configured per-file exclusions are outside eligibility. Files otherwise eligible but awaiting indexing or blocked by aggregate storage/corpus capacity stay in the denominator. Extraction failures and unreadable candidates need separate counters; if eligibility or the scan is incomplete, label coverage partial/unknown rather than asserting 100%.

Native ingestion must own the shared discovery/classification rules, including ignore files, selected data-directory exclusion, supported extraction, UTF-8 fallback and root boundaries. The extension's event filter is only a scheduler optimization and must not become a competing eligibility implementation. An extension-based file count or extension list is insufficient: arbitrary UTF-8 files can be indexed and supported documents can fail extraction. Cache extraction eligibility by source digest and extractor version to avoid repeatedly parsing Office/PDF files for the footer.

Coverage needs collection/root binding, index generation, inventory revision/time, eligibility-policy version, scan completeness and counts. Source hashes provide freshness; metadata/event hints can cheaply mark a file pending but cannot prove its content still matches. Unsaved editor buffers should appear as pending edits separately because the native index reflects files on disk. Stale snapshots and scans made before a rapid-edit burst must not appear current. Footer percentages are a measured inventory view at a stated time, not a guarantee that no later filesystem change occurred.

## Status-bar behavior

For a completed inventory, show for example `AIRK: 87%` with a tooltip such as `174 of 200 eligible files current; 10 stale; 16 awaiting indexing`. Clicking queues **Index Now** through the existing serialized scheduler. Preserve the controls menu as a separate command/palette entry. While a job is active, show indexing state and coalesce additional clicks. Paused automatic indexing does not prevent an explicit update; storage/trust errors remain visible.

Show `Scanning`, `Coverage unknown`, `Not indexed` or `No eligible files` when applicable. Never turn zero eligible files into a 100% claim. Avoid rounding a near-complete fraction up to 100%; reserve that value for all eligible files current with a complete inventory. In multi-root workspaces, prefer the active editor's folder and name that folder in the tooltip; otherwise select a folder before updating. A workspace aggregate must sum numerators/denominators, not average percentages.

Inventory work runs in the background, coalesces changes and uses the existing delay/idle choices. Do not traverse or hash an entire repository on every keystroke, MCP read or status-bar repaint. Reconcile after indexing, ignore-policy changes, folder/storage changes and watcher overflow; use a bounded background rescan for changes missed while the editor was closed. Keep percentage updates separate from MCP server-definition changes so a footer refresh does not restart clients.

## Delivery and checks

First extract authoritative ingest eligibility/inventory helpers and add kernel coverage contracts with bounded background scanning. Then persist/cache revision-aware coverage under the selected collection data directory and add a CLI coverage/status view. Finally consume that view in the extension, wire the click to indexing and expose a details view explaining exclusions and blocked work. Any MCP coverage summary stays fixed to its worker scope; adding a tool requires updating the advertised inventory and integration probes.

Acceptance includes ignored/unsupported files, UTF-8 fallback, PDF/DOCX/PPTX/SVG, failed extraction, per-file and aggregate limits, added/changed/deleted/renamed files, unsaved buffers, rapid edits during a scan/index, incomplete scans, zero eligibility, near-100% rounding, multi-root selection, trust, pause/manual modes and click coalescing. Index publication failures retain prior evidence while coverage reports outstanding work. Coverage is separate from [usage/estimated savings](usage-reporting.md): a full index does not establish retrieval relevance or token savings.
