# Working on AIRK

AIRK is an independent Artifact Indexing and Retrieval Kernel. This file holds the standing rules for agents. Campaign-specific state lives in "Current work" at the end and in the documents it names. Keep AGENTS.md and CLAUDE.md byte-identical.

Before changing behavior, read the affected source, its tests and the owning component's `CONTRACT.md`. Consult README.md, CONTRIBUTING.md and docs/architecture.md when relevant. Do not preload manuals or dated history.

## Repository map

| Path | What it owns | Contract |
|---|---|---|
| `crates/airk-core/` | Shared native types and read/service contracts | `src/read_limits/CONTRACT.md`, `src/service_records/CONTRACT.md`, `src/install_record/CONTRACT.md` |
| `crates/airk-process/`, `crates/airk-host/` | Native processes, platform adapters, install files/layout and checkpoint fences | Host `src/{native_command,install_files,install_layout,checkpoint_fence}/CONTRACT.md` |
| `crates/airk-ingest/`, `crates/airk-store/`, `crates/airk-retrieval/` | Extraction, persisted indexes and retrieval/inference primitives | Store `src/store_fs/CONTRACT.md`; otherwise owning source/tests |
| `crates/airk-kernel/` | Publication, query execution, budgets and Workbench plans | Owning source/tests and `docs/architecture.md` |
| `crates/airk-mcp/` | Canonical evidence handlers, MCP schemas and SDK transport | `docs/read-interface-parity.md`, native schemas/tests |
| `crates/airk-ipc/` | Same-user transport, frames and I1 control protocol | `CONTRACT.md` |
| `crates/airkctl/` | CLI, session and agent-config writing | `src/contract/CONTRACT.md` (I2) |
| `crates/airk-endpoint/`, `crates/airk-https/` | Endpoint contracts, HTTPS gateway and Admin handlers | Endpoint `src/admin_contract/CONTRACT.md`; HTTPS `src/admin_plane/CONTRACT.md` (I3) |
| `crates/airk-catalog-control/`, `crates/airk-frontend/` | Catalog mutations and service-side client/read adapters | Owning source/tests; I1/I2/I3 where crossed |
| `crates/airk-inference/`, `crates/airk-supervisor/` | Inference lifetime and in-process workers | Owning source/tests and `docs/process-model.md` |
| `crates/airk-installer/`, `crates/airk-service/`, `crates/airk-cli/` | Installation, resident service/setup and thin `airk` executable | Service `src/setup_role/CONTRACT.md`; Core install-record contract |
| `packages/vscode/` | VS Code host, Explorer and Workbench | I2 plus `docs/split/contracts.md` |
| `packages/admin/` | Browser/Admin presentation and shared UI workflows | I3 plus `docs/split/contracts.md` |
| `scripts/`, `.github/workflows/` | Checks, packaging, installers and release workflows | `CONTRIBUTING.md`, `docs/ci-and-builds.md`, script tests |
| `temp/`, `.airk/` and private fixture/state subpaths under `artifacts/` (listed in Current work) | Local scratch and private/index state. Never staged, packaged or published. | none |

Contract paths in the table are relative to their named crate. Dependency direction: clients/transports call owning service/kernel APIs; kernel uses ingest/store/retrieval, which use Core contracts. Core and process foundations have no AIRK dependencies. Exact edges live in Cargo manifests and `scripts/dev/boundary-exceptions.json`; architectural rationale is in docs/architecture.md.

## Commands

Run from the repository root with pinned toolchains and provisioned inputs. Select the affected crate/features/tests; these commands are a menu, not a mandatory full run for every edit.

| Purpose | Command |
|---|---|
| Format / verify | `cargo fmt --all`; `cargo fmt --all -- --check` |
| Scoped Rust check (Clippy) | `python scripts/dev-check.py -p <crate>` plus needed features/targets |
| Scoped tests | `cargo nextest run --profile ci -p <crate> --locked --timings -E '<test expression>'` (add required features/target directory) |
| Integration preflight | `python scripts/check-service-clients.py --runtime-dir <matched-debug-pair-dir> --files 16 --model --report <receipt.json>` |
| Doctests | `cargo test --doc --workspace --locked --timings` |
| Extension type check / build | `npm run compile --prefix packages/vscode`; tests: `npm test --prefix packages/vscode` |
| Admin build / verify / tests | `npm run build --prefix packages/admin`; `npm run check --prefix packages/admin`; `npm test --prefix packages/admin` |
| Read-surface parity | Use the [per-surface selections](docs/read-interface-parity.md#current-per-surface-selections) for CLI/session, MCP, Admin and Workbench |
| Final workspace gate | `cargo clippy --workspace --all-targets --all-features --locked --timings -- -D warnings`, then `cargo test --workspace --all-features --locked --timings` |
| Package tested native pair | `python scripts/package-native.py --target <native-target> --runtime-dir <tested-pair-dir> --output artifacts/<fresh-output>` |
| Package compiled universal VSIX | `python scripts/package-extension.py --channel stable --output artifacts/<fresh-output>` |
| Repository / whitespace checks | `pwsh -NoProfile -File scripts/check-repo.ps1`; `git diff --check` |

Nextest is pinned to `0.9.148`; the `ci` profile disables retries and excludes long campaigns by default. Final qualification restores excluded campaigns with `--ignore-default-filter` when using Nextest and runs doctests separately.

Build the matched debug pair with `python scripts/build-service-runtime.py --destination <matched-debug-pair-dir> --profile debug` after provisioning model/static ORT inputs and sealing Admin assets. See [required qualification](CONTRIBUTING.md#required-qualification) and [split build order](docs/ci-and-builds.md). The builder stages matching `airk`/`airkctl` images and metadata; do not mix binaries from separate builds. Packaging consumes tested artifacts; full local build wrappers are in CONTRIBUTING.md.

Do not run `cargo check` before an equivalent Clippy run; it compiles the same code twice.

## Definition of done

Select checks by changed behavior and the claim being made, using the owning contract, actual callers and active plan. Confirm selectors/features against live source. Combine applicable rows; broaden only for changed dependencies, unexplained failures or uncovered acceptance criteria. This is a selection aid, not an approval gate.

| Change | Checks | What it proves |
|---|---|---|
| Prose or agent instructions | Relevant content/links, repository check and whitespace; repository check enforces AGENTS/CLAUDE hash equality | Documentation consistency, not runtime behavior |
| Rust in one crate | Format, scoped dev-check and focused regressions, including refusal/recovery; report selected count | Selected crate behavior in the tested configuration |
| Platform-only Rust | Scoped Clippy first on that platform, then affected regressions | Changed platform path only |
| Component interface/state/invariants/lock order/test commands | Applicable checks plus owning CONTRACT.md update | Contract and implementation remain aligned |
| Shared native contract | Reviewed contract and fixtures landed before consumers; affected consumer checks | Agreed shape/semantics and exercised consumers |
| Structural move | Exact bytes for digested/persisted formats, parsed equality elsewhere; unchanged test identities/assertions; rerunnable script for large moves; affected embedded/package checks | Preserved behavior at measured boundaries |
| CLI/session, Admin, Workbench or MCP reads | [Per-surface selections](docs/read-interface-parity.md#current-per-surface-selections), full-result/input comparisons through canonical handlers and affected real transports; document inapplicable operations | Equivalent exercised inputs/results; mocks alone do not qualify transport parity |
| Frontend callback/view | Typecheck, affected behavior tests, regenerated asset verification; embedded/package consumer checks when qualifying an artifact | UI logic; artifact checks additionally cover shipped bytes |
| Harness/script | Focused script tests and affected consumers; no unrelated native rebuild | Harness behavior, not fresh native qualification |
| Installer, persistence or process ownership | Cheap failure/recovery regressions, then affected real process/platform/artifact checks on reviewed inputs | Ownership/recovery at the exercised boundary |
| Release or platform outcome | Final workspace gate, doctests, runtime/package checks and active plan's qualification campaigns | Only the qualified platforms/artifacts; scoped passes do not replace final gates |

## Authority

Do without asking:

- Complete the authorized outcome end to end, across approved component and milestone boundaries. Progress updates and agent handoffs are not approval gates.
- Make routine implementation, diagnostic, regression, documentation and validation choices.
- Continue in-scope corrections after failures. A failed-run count does not require reapproval.
- Commit and push reviewed milestones within the branch/remote scope recorded in Current work.

Ask first:

- A genuinely unresolved user-owned decision, or a material scope expansion.
- A new crate, unless the active plan already approves it and its extraction criteria are met.
- A change to wire shapes, digest inputs or persisted formats without a recorded contract decision.
- Anything requiring human consent.

Never without explicit authorization for that action: publish, deploy, merge, open a PR, or message other people.

While one dependency is blocked, continue independent work. Keep unrelated improvements out of the patch. Preserve unfinished and unrelated work in the tree.

Report facts, correct problems and continue. Do not apologize, invent approvals or claim results you did not observe.

## Product invariants

- Preserve collection/root isolation, indexed revision identity, original document locations, budgets, failure reporting and source read-only behavior.
- Never stage, package or publish `temp/`, `.airk/` and private fixture/state subpaths under `artifacts/` (listed in Current work). Package outputs under `artifacts/` are not automatically private state, but only verified distributable payloads may be published; never include fixtures, credentials, indexed source copies or live state.
- Do not add unapproved cloud providers, external databases, sidecars or agent orchestration.
- Munarium, VCP and `temp/` are inspiration. Do not copy their implementation.
- Preserve unrelated user work, ignore patterns, source SPDX headers and dependency license notices.
- Never alter identities or weaken a check to obtain a pass.

### Read-surface parity

airkctl CLI/session, Admin, Workbench and MCP reads go through one canonical implementation and must return equivalent results.

- Preserve all supported inputs and result fields across commands, structured views, copies and exports. This includes provenance, readiness, coverage, continuations and error/retry details.
- Disclose truncation and selected subsets.
- Regressions compare full equivalent results and input catalogs, normalizing only documented transport identities and timing.
- Exercise native handlers and the real transports wherever they reshape results. Renderer mocks alone do not establish parity.

## Contracts and structure

- Shared native contracts have one owner, in Rust. Extend the Rust owner and use the approved generation pilot and Rust-emitted semantic cases. Do not add handwritten TypeScript or Python mirrors; they drift.
- Existing `airk-state/1` formats need retained-fixture schema-identity proof, or an approved new state contract, before they change.
- Keep behavior changes and mechanical restructuring in separate, distinguishable steps so each can be reviewed for what it is.
- Add modules inside the owning crate. Untangle inside the crate before extracting. Keep new items private or `pub(crate)` unless an external caller needs them.
- Do not add Rust `#[path]` mounts (thin platform switches excepted), `include!` of Rust source, or unreachable source files.
- Keep exhaustive per-plane dispatch. Registries may consolidate duplicated catalogs, with one handler, one authorization rule and one conformance applicability per operation. Use owning hooks, not feature-specific branches, in generic indexing, worker or MCP paths.
- Delete code only with evidence: it is uncompiled, it has no caller after a clean compile and Clippy, or a reviewed compatibility row names a tested surviving owner. A legacy-looking name is not evidence. Predecessor discovery, source selection, shutdown proof and resumable cleanup may still be live duties.
- Test expectations must be independent of the code under test. Do not generate expected operation sets from implementation catalogs; such a test cannot fail when the catalog is wrong.
- Line counts and cosmetic splitting are not completion criteria.

## Validation

Cheap first:

- Start with the cheapest useful diagnostic. Scoped tests, type checks and compiler diagnostics may guide investigation without a full investigation or ledger entry first.
- Batch related regressions and compatible test configurations.
- A filtered run must report its selected count. Zero selected is a failure.
- Recheck only affected consumers during iteration. Reuse unchanged artifacts and passed evidence under their original identities.

Behavior defects:

- Trace the affected flow and its sibling callers. Keep a focused reproduction. Fix the whole known failure class, not the one instance.
- Review the ownership, persistence, concurrency, recovery and platform boundaries the change touches.
- Add a cheap seam when an expensive failure cannot otherwise be diagnosed locally.

Expensive runs (builds, installed journeys, qualification) confirm reviewed code. They are not the debugging loop.

- Review the combined change before an expensive run or corrective push. With delegated work, the primary agent reviews independently of the implementers. Working alone, do an explicit self-review and do not call it independent. Separately required security or contract reviews still apply.
- Freeze consumed inputs for expensive runs; retain validation evidence as specified in Task ledger and reporting.
- On failure, stop the batch and collect its diagnostics before correcting.
- After a repeated failure, produce a revised diagnosis and cheap regression evidence before the next expensive attempt.
- Never retry unchanged failed inputs until green, weaken assertions, widen limits or retries, or manufacture cleanup evidence.

Tests:

- Keep decision logic testable across hosts. Inject clocks and observers for deadlines and state transitions.
- No fixed sleeps. Wait on observed events under a deadline.
- Do not hide flakes with retries or relaxed limits. A quarantine needs an owner, a reason and an expiry.
- Keep assertion-only tests outside production modules where practical, to avoid release recompilation.

## Build and host

Primary development host: Windows with PowerShell (`pwsh`); commands use PowerShell quoting and repository-relative paths except Mac Neo commands, whose shell is specified in Current work.

- One Cargo owner at a time, covering builds and platform batches. Verify compiler descendants have exited before handing off.
- Reuse the persistent checkout target or the existing isolated target (see Current work). Do not routinely `cargo clean` or replace warm targets on Windows; rebuilding them is slow.
- Development incremental defaults: on for Windows, off elsewhere. Do not change release or CI policy to match local iteration.
- Keep Cargo `--timings` for necessary builds and Clippy.
- Reuse the pinned Nextest profiles and integration preflights from CONTRIBUTING.md.
- Mac checks: see Current work for the host. Batch them over the existing SSH setup. Disk there is limited, so copy evidence outside the target and `cargo clean` after every batch, including failed ones.

## Parallel agents

Use them when independent investigation, implementation or review helps. Give each clear file ownership and acceptance criteria. The primary agent reviews the combined change and may reassign work within the approved scope. Parallel agents edit and investigate; only the single Cargo owner builds.

## Navigation

- Once per session, discover the AIRK MCP tools and call status to confirm project binding. If they are absent or failing, say so briefly and use local tools. Do not retry repeatedly or change configuration to make discovery succeed. Inspect `.codex/config.toml` and `.mcp.json` only when diagnosing access.
- Prefer AIRK tools for scoped indexed navigation. The index may be stale: keep the revision identity and verify the live file before editing.
- Use `rg` for live verification and anything the index does not cover. Editing, builds, tests and Git use direct tools.

## Task ledger and reporting

Record substantive work in the existing ledger (see Current work): outcome, decisions, blockers, validation scope/results and expensive-run receipt paths. Receipts retain exact commands, source/artifact identities and timings; Cargo records also include scope, target directory, incremental setting and elapsed time. Keep entries proportional; routine edits need no separate plan or checkpoint.

At handoff distinguish findings, reproductions, hypotheses, passes and skipped/unavailable checks. State what the evidence proves and what remains unknown. Local success does not establish cross-platform qualification or unimplemented capabilities. Packaging produces a candidate; release completion requires the authorized release workflow and verified published outputs.

## Documentation, versions and commits

- Update documentation affected by implemented behavior. Keep dated historical records identifiable as such.
- Before a commit/push/PR cycle, check README, the documentation index, release notes and Marketplace/VSIX text against the current version and change. Edit only what is wrong.
- Keep native and extension versions and lockfiles synchronized. Honor an approved or completed bump once per PR. Before a feature or fix PR, remind the user about the patch/build increment if it has not been handled.
- Every commit uses the authorized human contributor's configured identity and `git commit -s`. Verify the DCO `Signed-off-by` trailer. Do not add agent signatures or invented approvals.

## Glossary

- **Receipt**: A retained result/evidence record tied to request, source or artifact identity; native operation receipts and local validation-run receipts have different owners.
- **Qualification**: Evidence from the required runtime, transport, package and platform checks on identified inputs; compilation alone is insufficient.
- **Plane**: A protocol/authority boundary, such as evidence reads, local control or browser Admin management.
- **Fence order**: The owning contract's order of locks and revision/authority checks that prevents stale or unauthorized publication or delivery.
- **Arbiter**: The approved optional local evidence-gathering/answer capability inside the existing service, bounded by the active plan.
- **P10**: The Arbiter package in the advanced-split plan. Historical build receipts also used this name for a Cargo target; call that directory the **isolated target** in new records.
- **Mac Neo**: The existing macOS arm64 SSH qualification/signing host; location and ignored connection configuration are in Current work.
- **I1**: Same-user local IPC control request/frame contract owned by airk-ipc.
- **I2**: airkctl caller contract: commands, exit/error/version documents and session protocol.
- **I3**: Browser Admin HTTP envelope, authentication/admission and operation contract.
- **Generation pilot**: The approved Rust schema-to-TypeScript generation path for I2/I3; semantic rules remain tested against Rust-emitted cases.
- **Compatibility row**: A reviewed compatibility-inventory entry identifying retained behavior, its surviving owner and preservation evidence.
- **Installed journey**: An end-to-end test against an installed product, including its actual host/process and cleanup behavior.
- **Matched debug pair**: airk and airkctl built/staged together by the runtime builder with matching source, version, contract and artifact metadata.
- **Isolated target**: The persistent Cargo build/cache directory assigned to this campaign; its path is in Current work.

## Current work

<!-- The only section expected to change between campaigns. Replace it when the campaign changes; do not accumulate history here. -->

Active campaign: the advanced split.

Delivery window: 14 November 2026 03:05 UTC through 15 November 2026 03:05 UTC,
as directed by the user. Complete the whole plan on `feat/advanced-split`, with
no PRs. This is an execution deadline, not permission to omit acceptance gates
or report unfinished work as complete.

Campaign execution rules:

- Assign complete feature outcomes to parallel owners, including their tests,
  documentation and recovery paths. Keep file ownership explicit; avoid repeated
  investigation-only handoffs when implementation is authorized.
- Record and review new contract/format decisions before dependent behavior.
  Once that decision and its fixtures are frozen, consumer implementation does
  not wait for a separate Git commit. Preserve contract-first ordering within
  the eventual milestone commits; combine review and validation into coherent
  feature batches instead of making every small step a commit gate.
- Use cheap scoped diagnostics during implementation. Batch compatible checks
  at integration points, freezing their actual consumed inputs. Reuse passed
  evidence under its exact identity. Reserve full workspace, platform, installed
  and published-artifact checks for reviewed integrated candidates.
- Keep the critical path moving while another owner works. Review the whole
  P0–P11 status at least every four hours, shift effort to unfinished acceptance
  criteria. The revised cutoffs are 14 November 10:05 UTC for integrated source
  and model inputs, 16:05 UTC for qualification, and 15 November 02:05 UTC for
  publication and downloaded-byte verification, leaving one hour for recovery.
  Move to qualification as soon as a coherent candidate is ready. Do not fill
  the window with optional cleanup or treat the deadline as a waiver of checks.

| Need | Source |
|---|---|
| Plan | docs/split/codex-plan.md |
| Recorded decisions | docs/split/split-plan.md |
| Progress ledger | docs/simplify/one-process-ledger.md |
| Standing simplification decisions | docs/simplify-plan.md |

- Precedence: later user instructions, then recorded approvals, then the plan text. Planning-only restrictions and gates already satisfied in those documents no longer apply.
- Current-version delivery: previous-version compatibility is not required. If indexed data cannot be upgraded, index replacement/regeneration is authorized. Preserve source files and unrelated user work; current-version contracts, authority, budgets, quality and release gates still apply. Historical receipts remain historical evidence, not compatibility obligations.
- Resume from the working tree, the ledger and the relevant receipts.
- The advanced split's approved local Arbiter work follows its own boundaries in the plan.
- DiskANN is the selected final disk-backed vector engine. The lexical/SQLite bootstrap storage is not that implementation; do not treat it as one.
- The installed-editor repair is superseded. Ordinary split work does not resume it.
- Authorized milestone commits/pushes: `feat/advanced-split` to `origin` (`https://github.com/iokaio/airk.git`); no force-push and no pushes to other branches.
- Isolated build target: `artifacts/v190/p10-target`.
- Mac host: Mac Neo; run command batches through POSIX `sh` over SSH. Checkout `~/code/airk`; connection details in ignored `artifacts/v190/neo-connection.local.json`.
- Private artifact subpaths include `artifacts/one-process/native-fixtures*/`, `artifacts/editor-qualification/`, and any fixture/state/cache/data roots created under a campaign's artifacts directory. Treat credentials, indexed source copies and live state there as private regardless of directory name. Build targets and verified installer/VSIX/native-package output directories are separate; being under artifacts does not authorize staging or publication.
