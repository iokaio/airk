# AIRK 1.2.5 candidate — not released

Native, extension and lockfile versions are synchronized at 1.2.5 in source. This document records pending behavior for review; no release, VSIX or Marketplace listing has been published for 1.2.5.

## MCP evidence and tool catalog

The unsupported `airk_history` tool is retired, leaving fifteen read-only MCP tools. The published 1.2.4 helper still has sixteen. Historical evidence and checkpoints remain unsupported. Existing clients with an explicit `airk_history` allowlist entry must remove it when adopting the new helper.

Search now returns a focused excerpt of at most 768 UTF-8 bytes per hit by default; `full_chunks: true` requests the complete indexed chunk subject to the existing byte budget. Snippet citations keep the source hash and adjust indexed line coordinates. A cut sets truncation, and `airk_read_file` or `airk_context` expands selected evidence. Search remains approximate across mixed code and documents. A related style guide or book passage does not establish an application's configuration; scope a known project with `path_prefix` and inspect the implementation.

Set `AIRK_MCP_RESPONSE_MODE=terse` on an MCP worker to replace repeated full publication/readiness/indexing blocks in evidence replies with a compact coverage summary and exact publication token. The default full envelope remains available. Status, capabilities and change validation retain their detailed reporting. Terse output still includes citations, hashes, validators and truncation; it does not prove live-source freshness.

MCP runs four evidence and two control requests at once, with up to sixteen additional requests waiting for a permit for five seconds. Queued requests are cancelled without consuming worker capacity. A full queue or timeout returns a retryable error. The control pool remains separate.

VS Code MCP definition resolution and the helper check no longer wait for a full initial index. The helper check can verify its handshake and fifteen tools before the first publication, reporting indexing as pending; search becomes available as soon as a base batch is committed. The dashboard shows processed and committed file counts, elapsed time, and whether published evidence can already be served. A scan with no native progress for ten minutes is stopped with a recoverable error; the existing one-hour total cap remains. A scheduled semantic batch has a twenty-minute cap, retains earlier committed evidence, and can be retried through Diagnose & Repair or Resume after the cause is addressed. These bounds do not make a slow scan faster, and a single observation cannot prove a process is stuck.

`known_hash` now fails explicitly when a present indexed source has a different hash. `airk_references` selects candidate chunks from the current authenticated lexical index before verifying exact-case whole identifiers in retained source. It remains bounded and textual: strings, comments and declarations can match; a truncated result does not prove complete impact coverage. Agent Lean still omits call occurrences, so relationship results cannot establish a complete caller graph at that detail level.

## Qualification status

On 2026-10-21, local Windows checks passed: `cargo build --workspace --locked`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, the repository checks, Python repository tests, and the native-backed VS Code suite (132 passed, 2 skipped). The focused regressions cover the queue, hash mismatch, snippet coordinates, CamelCase references, 15-tool catalog, MCP availability before indexing, progress parsing and idle termination. macOS and Linux qualification and measured indexing or token savings remain open before release.
