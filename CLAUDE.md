# Working on AIRK

AIRK is an independent Artifact Indexing and Retrieval Kernel. Read README.md, CONTRIBUTING.md, docs/architecture.md and the affected component before changing code. Follow more specific guidance when present.

- Use AIRK MCP first by default for repository discovery and indexed code/document evidence unless the user instructs otherwise. Prefer focused search, symbols, outlines and bounded reads; retain revision identity and check freshness/coverage when relevant. Verify live source before edits. If AIRK is unavailable, incompatible, stale for the required evidence or lacks the needed capability, briefly state why and use direct tools for that work. Editing, builds, tests and Git operations remain direct-tool work; do not repeatedly retry a failing MCP call.
- Keep scope local: file discovery, native extraction, indexes and bounded retrieval. Do not add agent reasoning, orchestration, external databases or cloud providers.
- DiskANN is the selected final disk-backed vector engine; the runnable scaffold's lexical map/SQLite retained snapshot is a bootstrap adapter, not that final implementation.
- One container per user/machine hosts independent collections selected by workspace MCP configuration. Native operation targets Windows, macOS and Linux.
- Preserve collection/root isolation, indexed revision identity, original document locations, budgets, failure reporting and source read-only behavior.
- Munarium, VCP and temp/ are inspiration only. Do not copy their implementation. Never stage, package, reproduce or publish temp/ or private/index state contents.
- Preserve unrelated changes. Do not remove user work, weaken ignore patterns or alter identities to make checks pass. Do not publish, deploy or message others without authorization for that action.
- Keep dependency direction in docs/architecture.md. Add adapters inside the owning crate before adding crates with no independent purpose. Keep original source SPDX headers and dependency license notices.
- Do not claim complete language understanding, resolved references, semantic search or cross-platform qualification without implementation and relevant evidence.
- Run formatting, clippy, workspace tests, repository checks and whitespace checks from CONTRIBUTING.md. Record unavailable checks honestly.
- Before creating a PR for new features or bug fixes, remind the user in Codex or Claude Code to manually increment the patch/build number (for example, 1.1.2 to 1.1.3). Keep native and extension versions and their lockfiles synchronized. Honor an already requested or completed version bump; do not increment it again for the same PR. Release-time version-bump PRs are a fallback, not a replacement for this reminder.
- Keep AGENTS.md and CLAUDE.md byte-identical. Human DCO sign-offs belong to the authorized contributor; do not add agent signatures or invented approvals.
