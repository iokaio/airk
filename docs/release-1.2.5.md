# AIRK 1.2.5 candidate — not released

Native, extension and lockfile versions are synchronized at 1.2.5 in source. This document records pending behavior for review; no release, VSIX or Marketplace listing has been published for 1.2.5.

## MCP evidence and tool catalog

The unsupported `airk_history` tool is retired, leaving fifteen read-only MCP tools. The published 1.2.4 helper still has sixteen. Historical evidence and checkpoints remain unsupported. Existing clients with an explicit `airk_history` allowlist entry must remove it when adopting the new helper.

Search now returns a focused excerpt of at most 768 UTF-8 bytes per hit by default; `full_chunks: true` requests the complete indexed chunk subject to the existing byte budget. Snippet citations keep the source hash and adjust indexed line coordinates. A cut sets truncation, and `airk_read_file` or `airk_context` expands selected evidence. Search remains approximate across mixed code and documents. A related style guide or book passage does not establish an application's configuration; scope a known project with `path_prefix` and inspect the implementation.

Set `AIRK_MCP_RESPONSE_MODE=terse` on an MCP worker to replace repeated full publication/readiness/indexing blocks in evidence replies with a compact coverage summary and exact publication token. The default full envelope remains available. Status, capabilities and change validation retain their detailed reporting. Terse output still includes citations, hashes, validators and truncation; it does not prove live-source freshness.

MCP runs four evidence and two control requests at once, with up to sixteen additional requests waiting for a permit for five seconds. Queued requests are cancelled without consuming worker capacity. A full queue or timeout returns a retryable error. The control pool remains separate.

`known_hash` now fails explicitly when a present indexed source has a different hash. `airk_references` selects candidate chunks from the current authenticated lexical index before verifying exact-case whole identifiers in retained source. It remains bounded and textual: strings, comments and declarations can match; a truncated result does not prove complete impact coverage. Agent Lean still omits call occurrences, so relationship results cannot establish a complete caller graph at that detail level.

## Qualification status

The focused MCP and kernel regressions cover the queue, hash mismatch, snippet coordinates, CamelCase references and 15-tool catalog. Full workspace and platform qualification are recorded separately before release; no cross-platform or measured token-savings claim follows from these changes alone.
