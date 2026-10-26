---
id: explorer.evidence.inspector
title: Inspect a citation
route: evidence/inspector
area: explorer
aliases: verify citation, content hash, is this file current
related_airk_help: freshness, limits
---
# Inspect a citation

Evidence › Inspector shows one cited file's indexed record and compares the saved bytes with the indexed content hash.

## Results

- **matches index**: the saved file is byte-identical to what was indexed.
- **changed since index**: the file changed; cited lines may have moved.
- **not saved**: the editor holds unsaved edits.
- **missing**: no file exists at the indexed location.

## Limits

A matching hash shows the file is unchanged since indexing. It does not verify what the code does.
