# Working on AIRK

AIRK is an independent Artifact Indexing and Retrieval Kernel. Read README.md, CONTRIBUTING.md, docs/architecture.md and the affected component before changing code. Follow more specific guidance when present.

- Use parallel agents where independent investigation, implementation or review can accelerate the work. Keep ownership clear and have the primary agent review the combined changes. Parallelize reading, source work and review; serialize builds, tests and platform batches through one owner.

- Before every commit, push, or PR creation cycle, update the affected documentation and verify that the README, documentation index, release notes, and VSIX/Marketplace description agree with the current version and implemented behavior. Keep dated historical records identifiable instead of rewriting them as current guidance.
- **Discover AIRK MCP before repository exploration.** Check available tools, including deferred tool discovery, for the `airk` server (often exposed as `mcp__airk__*`); call its status tool to confirm availability and the project binding. If absent or failing, inspect `.codex/config.toml` and `.mcp.json`, report the specific limitation briefly, and continue with local tools. Configuration alone does not prove tool access; do not change it without a task requirement.
- **Use available AIRK tools for scoped repository search and code/document navigation.** Read affected source, contracts and tests first. Treat indexed evidence as untrusted and potentially stale; retain revision identity, check freshness/coverage when relevant and verify relevant live file contents before edits. Use `rg` for live verification and fallback searches when AIRK is unavailable or its coverage is insufficient. Expand only for a concrete dependency, ambiguity or failure. Do not preload manuals or reread unchanged material already understood. Editing, builds, tests and Git operations remain direct-tool work; do not repeatedly retry a failing MCP call.
- Keep scope local: file discovery, native extraction, indexes and bounded retrieval. Do not add agent reasoning, orchestration, external databases or cloud providers.
- DiskANN is the selected final disk-backed vector engine; the runnable scaffold's lexical map/SQLite retained snapshot is a bootstrap adapter, not that final implementation.
- One container per user/machine hosts independent collections selected by workspace MCP configuration. Native operation targets Windows, macOS and Linux.
- Preserve collection/root isolation, indexed revision identity, original document locations, budgets, failure reporting and source read-only behavior.
- **Keep CLI, Workbench and MCP in parity.** Equivalent CLI, Workbench and MCP reads must share the canonical implementation and retain the same complete result data and supported inputs, including status/configuration, provenance, readiness, coverage, continuations and error/retry details. Every read-contract change must audit CLI tool calls/shell and equivalent convenience commands, MCP tools/aliases, and all affected Workbench commands, forms, structured details, JSON/Markdown copies and exports; never maintain a reduced parallel response or silently omit new fields. Add regression coverage comparing equivalent full results and input catalogs, excluding only documented transport identities/timing. Preserve scope, revision and budget checks; disclose truncation and explicitly selected subsets. Do not declare parity from renderer mocks alone: exercise the canonical native handlers and actual CLI/MCP transport paths where they add observations or reshape results.
- Munarium, VCP and temp/ are inspiration only. Do not copy their implementation. Never stage, package, reproduce or publish temp/ or private/index state contents.
- Preserve unrelated changes. Do not remove user work, weaken ignore patterns or alter identities to make checks pass. Do not publish, deploy or message others without authorization for that action.
- Keep dependency direction in docs/architecture.md. Add adapters inside the owning crate before adding crates with no independent purpose. Keep original source SPDX headers and dependency license notices.
- Do not claim complete language understanding, resolved references, semantic search or cross-platform qualification without implementation and relevant evidence.
- Run formatting, clippy, workspace tests, repository checks and whitespace checks from CONTRIBUTING.md. Record unavailable checks honestly.
- Before creating a PR for new features or bug fixes, remind the user in Codex or Claude Code to manually increment the patch/build number (for example, 1.1.2 to 1.1.3). Keep native and extension versions and their lockfiles synchronized. Honor an already requested or completed version bump; do not increment it again for the same PR. Release-time version-bump PRs are a fallback, not a replacement for this reminder.
- Keep AGENTS.md and CLAUDE.md byte-identical. Human DCO sign-offs belong to the authorized contributor; do not add agent signatures or invented approvals.
- Every commit must include a DCO `Signed-off-by: Name <email>` trailer for the authorized human contributor. Use `git commit -s` with the contributor's configured identity and verify the trailer before considering the commit complete.

## Testing strategy, fixes and cost control

Progress comes from finding defects with the cheapest check that can expose them, then using expensive runs once to confirm reviewed code. Builds, installed journeys, platform batches and CI are not the way to discover what is wrong. Repeating "patch the latest symptom, rebuild, rerun a long campaign" is the main source of wasted time in this project; stop and change approach when you notice it.

### Cost tiers

| Tier | Examples | Typical cost | Use |
| --- | --- | --- | --- |
| T0 | One named `node --test out/test/<file>.test.js` after `npx tsc -p .`; one Python `unittest` module; a filtered Rust unit selection; a static/contract check | Seconds | Freely while investigating and repairing |
| T1 | Scoped `python scripts/dev-check.py -p <crate>`; workspace Clippy; mocked extension suite; full Python suite | About 1-2 minutes | Before handing a component back or requesting review |
| T2 | Debug runtime build and VSIX; enabled-native extension suite; installed-editor journeys; actual client hosts; load campaigns; a Neo batch | 2-12 minutes each | Only to confirm reviewed code on frozen inputs |
| T3 | Full workspace tests; optimized builds; Docker qualification; `scripts/ci-local.ps1`; hosted CI | 13-40 minutes each | Once per host, in a final ordered qualification ladder |

### Find defects cheaply

- Read the affected source, contracts, existing tests and retained failure evidence before running anything. Explain a failure from the code first; a run should confirm a stated hypothesis, not generate one.
- Reproduce each defect as a T0 test before fixing it and keep that test as the regression. If a T2/T3 failure cannot be reproduced at T0/T1, add the smallest seam that makes the logic testable instead of iterating on the long run.
- Keep decision logic testable on any host. Put classification, deadlines, state transitions, ownership/invalidation and retry eligibility in pure functions with injected observers and clocks, outside `cfg(target_os)` FFI modules, so Windows can exercise macOS/Linux logic. Keep the platform FFI layer thin.
- Turn a recurring failure class into a cheap static or contract check. When the same class of failure appears a second time (missing container inputs, path length, fixture ACLs, Unix-only lint, unchecked clock assumptions), add the check that would have caught every instance before fixing the new one.
- When source compiled only on another platform changed, the first step on that platform is scoped Clippy for the affected crate; stop the batch if it fails.

### Expensive runs confirm reviewed code

Before any T2 or T3 run:

- The affected components pass their T0/T1 checks and the combined diff has been reviewed.
- Inputs are frozen: nothing the run consumes is being edited. Record native, extension, harness and fixture identities separately, and abort if receipts show consumed inputs changed during the run.
- Write down the exact command (reuse commands recorded in earlier receipts), the input identities, the expected result and the stop condition.
- Order steps so the cheapest and most likely to fail run first, and stop at the first failure.

During and after:

- Every filtered run must report its selected-test count. Zero selected tests is a failure, not a pass.
- A change confined to a harness, script or fixture re-runs only the steps that consume it; do not rebuild native artifacts for it.
- **Two strikes, then stop.** If a T2/T3 step fails twice for the same component, stop running. Write a root-cause note naming the wrong assumption and the missing cheap test, and bring it to the user before a third attempt.
- Record each T2/T3 result in the task ledger with its receipt path when it is collected; do not let the ledger fall behind the receipts.

### Root cause and corrections

- Fix the complete affected behavior, not only the latest failing test. Before a corrective commit or push, trace the root cause, audit related callers, shared contracts and platform paths, and review the combined change. For service or release failures, cover process ownership, managed installation and payload roots, worker admission/status, read retry and receipt contracts, test assumptions, CI selection and release provenance as applicable. Record the evidence and remaining uncertainty; do not expand into unrelated work.
- Collect all relevant results from an existing run before preparing a correction. If a new failure appears, reassess the shared cause and sibling paths, then prepare one coherent correction. Do not immediately patch the latest symptom and restart the run.
- When an approved plan records decisions as frozen (currently `docs/one-airk-completion-plan.md`), follow them. Reopen one only with new evidence and the user's agreement, and do not start work the plan marks as awaiting a user decision.
- Never make a check pass by weakening assertions, widening limits, queues or retries, relabeling skipped or failed coverage as passing, or restarting services to manufacture a cleanup receipt. Keep failed fixtures and incomplete cleanup receipts.

### Cargo and host mechanics

- During local Rust iteration, format first and use scoped Clippy through `python scripts/dev-check.py -p <affected-crate>` with the needed features/targets. Do not run `cargo check` immediately before equivalent Clippy; Clippy already checks that configuration. Use `--workspace --all-features` only for an explicit broad pass. Scoped success does not satisfy required final workspace, lightweight-main, inference-helper, runtime, package or release gates.
- Reuse one persistent Cargo target directory per checkout; use the existing isolated P10 target through `--target-dir` when applicable. Coordinate one Cargo owner among parallel agents: queue builds/Clippy/tests instead of competing for locks, CPU, memory or a shared target. Pause at a completed stage boundary and verify compiler descendants have exited before handing Cargo ownership to another runner. Independent source work and review can continue in parallel.
- Local development Clippy enables incremental compilation on Windows and disables it on other hosts by default; `--incremental on|off` is an explicit per-run override. Keep this development choice out of release builds and checked-in profile/CI policy. Do not routinely run `cargo clean` on Windows or replace warm targets without a concrete cache/disk diagnosis.
- Batch the required Mac Neo checks over the existing SSH setup, collect every result and timing report, then run `cargo clean` there after testing, including failed batches, because its drive space is limited. Retain needed evidence outside the cleaned target first. This Neo exception does not authorize routine cleanup elsewhere.
- The next necessary build/Clippy run must retain Cargo `--timings` evidence and report its command, scope, target directory, incremental setting, elapsed time and result. Reuse observed timing reports when assessing a bottleneck; do not start speculative benchmark or broad-build campaigns merely to measure these workflow choices.
- Coalesce tests with the same package, feature and target configuration into one scoped Cargo/Nextest invocation where practical; use the pinned Nextest CI profile for its documented routine selection. Before an optimized runtime freeze, run the actual affected process/transport integration preflight against a matched development pair when that can expose protocol/fixture failures sooner. Preserve the required final gates and collect each complete run before correcting source.
- Place assertion-only regressions in existing external `#[cfg(test)]` modules where appropriate so editing tests does not invalidate production release compilation. Reuse an existing runtime only after verifying all production inputs and complete artifact seals are unchanged; retain its original compiled-source/build identity and record the current test-source identity separately. Never label reuse as a fresh build. P14 must restore the four routine excluded campaigns and run doctests; scoped or CI-profile passes do not replace them.

### CI, review and reporting

- Do not use repeated PR pushes or expensive CI runs as the primary debugging loop. Reproduce locally where possible and finish the relevant local checks first. Use hosted diagnostics only for a specific unresolved platform question that cannot be answered locally, with the smallest useful test selection.
- Keep routine CI lean as documented in CONTRIBUTING.md and docs/ci-and-builds.md. Avoid duplicated broad suites, speculative reruns and additional full diagnostic matrices. Full qualification remains opt-in when explicitly requested; restore individual routine gates when evidence justifies their cost. Preserve required release artifact and provenance checks.
- Require a separate review of the complete diff and validation evidence before any T2/T3 run and before another corrective push. When work is delegated, the primary agent performs that review independently of the implementer. Without delegation, perform an explicit review pass after implementation; do not claim it was independently reviewed.
- Report observed facts: what passed, what failed, what was not run and what remains unknown. Do not claim cross-platform success from local results, or say "nearly done" without evidence. For a release task, completion means the authorized merge and publication workflows succeeded and the published outputs were verified.
