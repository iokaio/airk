---
id: explorer.evidence.inspector
title: Inspect a citation
route: evidence/inspector
area: explorer
aliases: verify citation, content hash, is this file current
related_airk_help: freshness, limits
---
# Inspect a citation

Evidence › Inspector shows the selected source file's indexed record and compares the saved bytes with the indexed content hash. Choose a file in the header to load it automatically when no citation is selected. **Inspect selected file** loads or refreshes that file, including after inspecting a citation.

To inspect a specific citation, go to Search and choose **Inspect** on a result, or use **Inspect** on a fact or checked citation. Selecting a source file does not replace an explicitly selected citation; use **Inspect selected file** to switch back. Files absent from the published index report an error.

## Results

- **matches index**: the saved file is byte-identical to what was indexed.
- **changed since index**: the file changed; cited lines may have moved.
- **not saved**: the editor holds unsaved edits.
- **missing**: no file exists at the indexed location.

## Limits

A matching hash shows the file is unchanged since indexing. It does not verify what the code does.
