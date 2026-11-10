# AIRK simplification plan

**Status:** proposal, revision 3, 9 November 2026. No code was changed to
produce it. It authorizes no repair, build, commit, merge or publication. Work
starts only after the decisions in [section 9](#9-decisions-for-the-user).

**Execution note, 9 November 2026:** at the user's request, Track R items
R1–R4 and Milestone S1 were implemented on `feat/simplify` with the
recommended options of D1, D2, D8 and D11. The D10 repair was not resumed.
Results, gate records and corrections to this revision's Appendix A and §8
baselines are in the [S1 review record](simplify/s1-review.md). Milestone S2
waits for that review. The text below is revision 3, unchanged.

**Revision history:**

| Revision | Change |
|---|---|
| 1 | Initial analysis |
| 2 | Added change-kind measurements, determinism, hand-copied counts, ratchets, conformance, wire stability, release branch and success metrics, from a cross-review with a parallel simplification plan for another codebase |
| 3 | Incorporated an independent Codex review (disposition in [Appendix E](#appendix-e-disposition-of-the-revision-3-review)) |

**What revision 3 changes:**

- **Release stabilization and restructuring are separate tracks.**
  - Only evidence-backed dead-code removal and the seams that known blockers
    need may precede 1.9.0.
  - The stopped installed-editor failure remains an acceptance blocker.
- **Deletion needs a compatibility proof,** not a line-count target.
- **The component map is a hypothesis.**
  - A crate is extracted only when it meets explicit criteria.
  - Exhaustive enum dispatch stays where the compiler's completeness check is
    useful.
  - A dependency cycle in the revision-2 map is fixed.
- **Code generation starts as a pilot** on one read and one control operation.
- **Conformance uses a per-operation applicability matrix,** and states when
  exact bytes are compared and when parsed equality is enough.
- **Test expectations stay independent of the code under test.** Moves
  compare test identities and outcomes, not counts.
- **Guardrails gate boundaries, not vocabulary.** Structural metrics are
  reported separately from outcome metrics, and the history windows are
  labeled consistently.
- **The next step is one bounded milestone, with a review gate** before any
  crate split.

**Scope:** the whole repository on branch `feat/finetuning` at the checkpoint
commit `9337822` ("Checkpoint unfinished 1.8.16 installer repair and document
stop"). The package version is 1.8.16, with public 1.9.0 as a future target.

**Method:**

- Static analysis of the live working tree:
  - line counts;
  - `use crate::` and `airk_*::` reference counts;
  - the `#[path]`/`include!` inventory and visibility;
  - direct time reads, sleeps and statics;
  - long lines;
  - function lengths, found by matching each function's closing brace at its
    own indentation, which rustfmt guarantees (test files excluded);
  - for each kind of change, the set of files it must touch.
- Git history of all 249 commits up to `9c71c2b`.
- Review of the normative, planning and stop records, including
  [the 1.8.16 record](releases/release-1.8.16.md) and the
  [installer stop handoff](research/installer-stop-2026-11-09.md).
- Five parallel read-only investigations: CLI; Store; Core/Kernel/MCP/Ingest;
  extension, scripts and docs; git history. The primary agent re-measured the
  highest-impact claims against the live tree; they are marked *verified*.

The AIRK MCP server was unavailable (`direct_observation_unavailable`), so this
analysis used local tools only.

Counts are approximate where they rely on grep. They describe structure, not
runtime behavior. **Nothing here claims a defect in shipped behavior.**

---

## 1. Summary

AIRK's invariants (fences, epochs, receipts, leases, exact revision identity,
parity) are deliberate and valuable. The problem is that they are not located
anywhere a single change can see them.

**Headline measure:** 57% of the last 200 commits touch three or more
top-level components, and 30.5% touch five or more. As long as an ordinary
change has to cross that many owners, an agent working inside one context
window will keep missing a place.

The main structural causes, detailed in §2:

1. **Contracts have no owner** (M1). They are hand-mirrored across Rust,
   TypeScript and Python.
2. **One kind of change means editing many files** (M9). Adding a read tool
   touches about 12 code sites, and a `JobKind` is referenced in 54
   production files.
3. **The module graph is hidden** (M2). There are 314 `#[path]` mounts and 17
   `include!` uses of Rust source, and 2,061 lines of kernel source are never
   compiled.
4. **Every component is coupled to every other** (M3, M4). Inside `airk-cli`,
   every major cluster forms a cycle with every other except IPC, and Store's
   index engine imports control-plane types.
5. **Superseded code remains** (M5): an estimated 45–50k lines including
   tests. This is an estimate, not a deletion target; each removal needs the
   compatibility proof in §5.
6. **Tests depend on timing and need the full stack** (M6, M10): about 760
   direct time reads, 208 sleeps in Rust tests and 51 process-wide mutable
   statics.
7. **Agents get too much context and too little contract** (M7, M11).

**Current release position.** 1.8.16 is a stopped, unfinished checkpoint.
All three R39 installed-editor attempts failed, and the latest
(`clean-break-r39-installed-editor-r3`) stopped with an unresolved
`workspace_registration_changed`. That failure remains a 1.9.0 acceptance
blocker. **This plan does not authorize resuming the stopped repair.**

**Recommendation:** run two separate tracks.

- **Track R, release stabilization (narrow).**
  - Resolve the known blockers, but only when the user re-authorizes that
    repair.
  - Remove dead code only where there is evidence for it.
  - Add only the seams those blockers need.
  - Build the compatibility inventory.
- **Track S, restructuring.** Start with one bounded milestone, then review
  before anything larger. **Milestone S1:**
  1. establish the compatibility inventory;
  2. add the smallest useful regression seams;
  3. prove one contract-generation example;
  4. untangle one component in place.

  Review those results before committing to any crate split. The component
  map in §4 is a hypothesis that S1 tests; it is not a commitment.

---

## 2. Evidence

### 2.1 Size

| Component | Rust lines (incl. tests) | Non-test-file lines | `.rs` files | Test fns | `pub` items | `pub mod` |
|---|---|---|---|---|---|---|
| airk-cli | 164,551 | ≈97k | 294 | 1,115 | 499 | 35 |
| airk-store | 153,638 | ≈110k | 261 | 1,018 | 1,561 | 73 |
| airk-kernel | 65,497 | ≈40k | 150 | 524 | 503 | 37 |
| airk-ingest | 38,030 | ≈36k | 116 | 326 | 214 | 23 |
| airk-core | 31,560 | ≈29k | 96 | 182 | 1,264 | 78 |
| airk-mcp | 17,911 | ≈12.5k | 31 | 169 | 132 | 11 |
| airk-retrieval | 7,687 | ≈7k | 21 | 47 | 126 | 11 |
| airk-process | 2,257 | ≈1.6k | 7 | 19 | 41 | 3 |
| **Rust total** | **≈481,000** | | **976** | **≈3,400** | | **271** |

"Non-test-file lines" excludes `*_tests.rs`, `tests.rs` and `tests/`, but
includes inline `#[cfg(test)]` modules.

| Other area | Size | Notes |
|---|---|---|
| packages/vscode | ≈27.8k prod + 25.7k tests | ≈1,203 tests; average line length ≈90 characters; 530 lines over 300 characters; no formatter |
| scripts/ | 36.3k + 21.0k tests | 139 + 94 files, 923 tests; 62 scripts load siblings through `importlib` |
| artifacts/ (git-ignored) | ≈50k lines of harness | 1,195 scripts *verified*; untracked qualification layer |
| docs (all `.md`) | ≈67k lines | 269 files; 164 entries at top level of `docs/` |

### 2.2 History: how change actually flows

Windows are labeled per row. Section 8 uses the same windows.

| Metric | Window | Value |
|---|---|---|
| Commits | all | 249, from 2026-10-13 to 2026-11-08 |
| Files per commit (mean / median) | last 100 | 49.6 / 18 |
| Commits touching more than 100 files | all | 22 |
| Largest commit | all | `7037cae`: 599 files, 97k lines |
| Share of all line churn in the 10 largest commits | all | about 52% |
| **Commits spanning 3+ / 5+ components** | **last 200** | **57% / 30.5%** |
| Fix, repair or regression commits | all | 56 of 249 (22%) |

- **Co-change.** Core changes alongside Kernel or CLI in 40 of its 44
  commits. The extension changes alongside CLI in 60 of CLI's 67 commits.
  Docs change in nearly every code commit.
- **The parity chain.** The cross-crate file pairs that change together most
  are the `lib.rs` files of Core, Ingest, Kernel, MCP and CLI, plus the
  extension README and webview. That is the parity contract, with no owner.
- **Most regression-prone seam.** Native-to-extension packaging:
  `native.test.ts` has a fix ratio of 9/17 and `test_package_extension.py`
  6/11.
- **Hidden in-crate coupling.**
  - Store's `current/journal.rs`, `database.rs`, `read.rs`, `mod.rs` and
    `tests.rs` change as one unit.
  - CLI's `runtime_install*` and `runtime_activation` change together in 10
    of 13 commits.

**Outcome history.**

- `docs/one-airk-completion-plan.md` §0 records:
  - more than 40 "patch symptom, reseal, rerun" cycles;
  - 22 Windows debug runtimes;
  - Neo batches r6–r13;
  - about 20 staged-editor attempts;
  - five recurring fault classes.
- The 1.8.16 record adds:
  - r37: an installed-editor failure (`catalog_outcome_unconfirmed_reconcile_first`);
  - R38: a response-budget failure;
  - R39: three installed-editor failures before the stop.

### 2.3 Failure mechanisms

#### M1. Contracts without an owner

- **Control contracts.** `airk_core::service::ControlMethod` has 78 variants
  (*verified*). CLI matches on it 148 times in `service_management.rs` and 79
  times in `console_client.rs`.
- **Hand-written extension mirrors:**
  - 75 `validate*` functions;
  - 39 limit constants mirrored from native;
  - about 1,760 distinct snake_case wire literals.

  Some of these are field-shape checks. Others implement **semantic rules**.
  For example, `readRetry.ts` checks cross-field authority, selection
  identity and envelope agreement before it allows the single 50 ms retry.
- **Digest framing.** `AIRK-DIGEST-1` is implemented in Rust, 3 TypeScript
  source files, 4 Python scripts and 1 `.cjs` file (*verified*).
- **Hand-written schemas.** No crate uses a TypeScript generator
  (*verified*). Public MCP input schemas in `airk-mcp/src/combined.rs` are
  hand-written `json!` literals.
- **Parity.** The canonical read handler is the rmcp `ToolRouter`. Workbench
  dispatches through tool-name strings; CLI uses 8 `impl RemoteTools`
  adapters; and parity is asserted in at least six separate test locations.

#### M2. A hidden module graph

- **Hidden mounts:** 154 `#[path]` attributes in Store and 160 in CLI
  (*verified*).
- **Textual inclusion:** 17 `include!` uses of `.rs` source, not counting
  `OUT_DIR` code (*verified*). The embed-pool sources are compiled into the
  library, the `airk-embed` binary and a test fixture, with `cfg` changed
  for each target.
- **Orphan files:** 2,061 lines of kernel source are not compiled
  (`conditional.rs`, `lineage.rs`, `reconciliation.rs`, `suggestions.rs`,
  *verified*).
- **Dead code rustc cannot see:** 271 `pub mod` declarations. The
  investigations found 43 unreferenced `Kernel` `pub fn`s and 11 kernel
  `pub mod`s with no external user. `allow(dead_code)` is rare (about 16).

#### M3. God objects and hub files

| Hub | Evidence |
|---|---|
| `ServiceRegistry` | 62 `impl` blocks across 39 Store files. CLI calls `ServiceRegistry::open*` 134 times from 54 production files (*verified*). |
| `FileStore` | `impl` blocks in 24 files; holds control-plane fields |
| `Kernel` | `impl` blocks in 40 files; about 297 `pub fn`s |
| `airk-cli/src/main.rs` | 3,338 lines; about 45 `Command` variants that mix process roles with user commands; `run` is 585 lines |
| `RuntimeCommand` | 61 variants; `runtime_install_commands::run` is 887 lines |
| `service_runtime::State` | 19 fields |
| `kernel/src/workbench.rs` | 4,519 lines |
| `store/src/current/journal.rs` | 3,195 lines |
| `vscode/src/serviceExtension.ts` | 2,930 lines with a fan-out of 38 imports. Its binding-refresh path is the plausible race behind the R39 `workspace_registration_changed` failure, according to the installer stop record. |

#### M4. Dependencies in the wrong direction

- **CLI.**
  - Only the IPC cluster is a leaf.
  - `runtime_install` has a fan-in of 35, mostly for trivial helpers such as
    `safe_file` (40 uses).
  - There are cycles such as `service_runtime` → `service_management` →
    `project_supervisor` → `service_runtime::checkpoint`.
- **Store.**
  - The index engine imports control-plane types: `SetupDirectory`,
    `PrivateQueryAuthority` and `FundingTerminalPermit`.
  - `quota` ↔ `current` ↔ `service` form a cycle.
  - Hard-link and reparse-point checks are re-implemented in 33 files.
- **Core** mixes evidence, read, control-plane and catalog contracts. 24 of
  its modules are used by neither Ingest, Retrieval, Kernel nor MCP.
- **Kernel** holds about 2.9k lines of CLI-only service control plane
  (*verified*).
- **CLI bypasses the kernel** into Store internals 397 times; the kernel is
  referenced 186 times.

#### M5. Superseded code (estimates)

| Area | Approx. lines | Evidence class |
|---|---|---|
| Kernel orphans | 2,061 | **Not compiled** (*verified*) |
| Store generation-era storage and its tests | about 11k prod + 6.9k tests | **No production callers** (*verified* for `prepare_catalog_v2` and similar) |
| Store migration and adoption, registry steps, non-final layouts | 8.6k prod + 5.2k tests | **Superseded by policy.** Needs the compatibility proof (§5). |
| CLI legacy runtime paths | 8.0k prod + 3.5k tests | Superseded by policy; needs proof |
| Kernel and Core legacy modules | about 2k | Superseded by policy; needs proof |
| Store retirement | about 2.3k | Still on the kernel read path (`current_observation`) |
| Extension legacy paths | about 1k | Partly live clean-break capture |
| Scripts | 24 with no caller and no test | No caller |

The clean break discards predecessor authority. It still requires:

- predecessor discovery;
- source-selection and preference capture;
- proof that predecessor processes have shut down;
- resumable cleanup of owned artifacts.

Code that serves those duties is **live**, even when its name says "legacy".

#### M6. Verification only works end to end

- **Few seams:** CLI has 8 production traits and Store has 2.
- **Concurrency invariants are scattered:** about 20 lock files, lease types
  defined in 9 or more places, SQL compare-and-set fences in 22 files, and no
  lock-ordering document.
- **Tests need the whole stack:** 28 of 49 CLI integration test files spawn
  processes, and Store makes 435 `tempdir()` calls.
- **CI selection is too broad:** any non-Markdown extension change selects the
  three-OS native matrix.
- **Untracked harness layer:** the harness logic is duplicated across scripts,
  and the input seal exists only in docs and in 1,195 untracked harness
  scripts.

#### M7. Context overload

- CLAUDE.md (17.4 KB, about 4.3k tokens) is always loaded.
- The mandatory read list is about 45k tokens, and about 105k for extension
  work in the One AIRK stream.
- There is no per-component contract.
- `docs/architecture.md` is mostly a dated phase log.
- 92 docs contain test counts and 76 cite versions.

#### M8. Phase-scale change units

Commits of 159–599 files mean review and regression attribution happen at
the scale of a whole phase.

#### M9. One kind of change means editing many files

| Change kind | Places that must change today | Evidence |
|---|---|---|
| Add or rename a public read tool | About 12 code sites, plus count assertions and docs ([Appendix C](#appendix-c-places-each-kind-of-change-must-edit)) | **Six files hold tables of 8 or more read-tool names**: MCP `workbench.rs` and `combined.rs`, Kernel `workbench.rs`, `workbench/natural.rs` and `workbench/combined.rs`, and Retrieval `intent.rs` |
| Add a `ControlMethod` | Core enums, a 148-arm and a 79-arm match, client helpers, extension client and validators, console UI | Six sampled variants each appear in 7–25 files |
| Add a `JobKind` | Store jobs, background, recovery, inspection and audit; CLI `project_worker`; Kernel `service_resources`; UI | 54 production files |
| Add or adjust a language | Ingest `Cargo.toml`, `syntax.rs` (7 `match language` sites), `adapters/registry.rs`, modules, Core `search_filters`, Kernel `workbench.rs`, MCP `lib.rs` | `kotlin` appears in 10 files across 4 crates |

Some of these sites are duplicated catalogs, such as tool-name tables, counts
and help text. Others are **exhaustive matches** that make the compiler
report a missing handler. Only the duplicated catalogs are a defect of
structure; §3 treats the two differently.

**Long dispatchers with feature branches:**

| Function | Lines |
|---|---|
| `project_supervisor::start` | 1,257 |
| `project_worker::run` | 1,187 |
| `service_collection_verify::verify_turn_inner` | 901 |
| `runtime_install_commands::run` | 887 |
| `workbench::evidence_plan_with_reader_inner` | 836 |
| `current/indexing::index_current_once` | 763 |

37 production functions exceed 300 lines and 192 exceed 150 lines.

#### M10. Time, sleeps and process-wide state

| Source | Production | Tests |
|---|---|---|
| `Instant::now()` / `SystemTime::now()` | 701 / 59 | 531 / 6 |
| `thread::sleep` / `time::sleep` | 109 | **208** |
| Process-wide mutable statics | **51** | 7 |
| `thread_local!` | 8 | 3 |
| `#[ignore]` tests | | about 65, listed only in prose (`docs/testing/ignore-triage.md`) |
| Extension `Date.now()` | 89 | |
| Extension `setTimeout`/sleep | | 86 |

No `Clock` abstraction exists. A forward catalog-clock step caused three
separate failures, according to the completion plan.

#### M11. Hand-maintained counts and formatter blind spots

- **Tool count.** 29 is hard-coded in three test assertions and at least
  seven docs, while `docs/architecture.md:398` still says "28-tool catalog".
- **Version.** `1.8.16` appears in five extension test files and one Python
  test. `release-version.py` synchronizes only manifests and lockfiles.
- **Schema numbers** appear in 12 test assertions. Some are legitimate
  historical-fixture checks, which must stay; see §5 and Appendix D.
- **Long lines.** 819 Rust lines exceed 200 characters, and 755 of them
  contain string literals (SQL DDL and hand-written JSON Schema). There is no
  `rustfmt.toml`.

### 2.4 What is already good (keep it)

- Core is IO-free. `airk-process` and `airk-retrieval` are clean.
- The crate-level dependency direction holds. The problems are inside crates.
- `local_ipc.rs` already uses the right thin platform-switch pattern.
- 73 of 85 extension host modules do not import `vscode`.
- `cli-reference.md` is generated and drift-checked; `release-version.py` is
  the single version command; Strategy output has golden files.
- Ignored tests are inventoried, and AGENTS.md and CLAUDE.md are kept
  identical.
- The CLI HTTP and catalog clusters have strong in-process unit coverage.
- The run rules (cost tiers, freeze, two strikes, no weakening) are correct,
  and the 1.8.16 stop shows they are followed.

---

## 3. Design principles

1. **Delete only with evidence.** Accepted evidence is "not compiled", "no
   caller after the compiler checks" or a completed compatibility proof (§5).
   Line counts are estimates, never exit criteria.
2. **One owner per contract.** Mirrors are generated where generation is
   proven (the S1 pilot). Semantic rules that generation cannot express stay
   hand-written, and are tested against **shared Rust-emitted decision
   cases**. Digest vectors are shared by every implementation.
3. **Approve ownership and allowed edges before extracting crates.**
   - Module ownership and the allowed dependency edges are agreed first.
   - A crate is extracted only when it meets the criteria in §4.1.
4. **Break cycles inside a crate before splitting it.** Untangle in place,
   then re-measure the cycles.
5. **Prefer exhaustive enum dispatch over hidden registries.**
   - Keep `match` on per-plane enums where it makes the compiler report a
     missing handler.
   - Use registries to remove **duplicated catalogs**: tool-name tables,
     schemas, help, intent vocabulary and counts.
   - A registry-completeness check ties every enum variant to:
     - exactly one handler;
     - one declared authorization rule;
     - one conformance applicability entry.
6. **No `#[path]` mounts, `include!` of source or orphan files,** except
   the thin platform-switch pattern.
7. **Keep pure logic separate from side effects.** Decisions go in pure
   functions with an injected `Clock`; tests use a manual clock and wait for
   events rather than sleeping. Platform FFI stays thin.
8. **Gate boundaries; report everything else.**
   - CI gates forbidden dependency edges, module honesty, registration
     completeness and declared state ownership, with reviewed exceptions.
   - Sizes, visibility counts, time reads, sleeps, statics and vocabulary
     signals are **reported metrics**. They inform review but are not pass or
     fail gates, so they do not invite cosmetic compliance.
9. **Each component has** a `CONTRACT.md` of at most 150 lines (purpose, owned
   state, interface, invariants, lock and fence rules, conformance
   applicability, test tier, entry points), and one T0 command that runs in
   under about 60 seconds.
10. **Structural steps preserve behavior.**
    - Capture golden outputs first.
    - Compare exact bytes for digested and persisted formats. For other
      outputs, compare parsed results, normalizing only the documented
      transport identities and timing.
    - Compare selected test identities and outcomes, not counts.
    - Edit no test assertions during a structural step.
11. **Expectations stay independent of the code under test.**
    - Expected operation sets are reviewed, checked-in lists. Changing one is
      a reviewed diff; it is never derived from the catalog being tested.
    - Historical schema fixtures and compatibility assertions remain.
    - Only incidental copies go: versions in unrelated tests and counts in
      prose.

These principles keep AIRK's scope rules: no agent reasoning, no external
database or cloud, DiskANN as the vector engine, collection and root
isolation, and source read-only behavior.

---

## 4. Candidate target architecture (hypothesis)

This section is a **hypothesis about ownership and dependency direction.**
Milestone S1 tests it on one component. The following are proposed for
approval now:

- the **layers** and **allowed edges**;
- **module ownership** inside the existing crates.

Crate extraction is a later, separate decision for each candidate.

### 4.1 Candidate components and extraction criteria

**A candidate is extracted only when all of these hold:**

1. **No cycle** passes through it, measured on the in-place module graph.
2. It has a **stable interface** documented in its `CONTRACT.md`, which the
   preceding milestone used without change.
3. It has **independent consumers or build needs**: more than one consumer
   crate, a feature, platform or binary boundary, or a measured compile-time
   benefit.
4. It shows a **demonstrated testing benefit**: its T0 command runs in
   isolation in under about 60 seconds, without the process stack.

Until then, a candidate stays a module with an owner and enforced edges.

| Layer | Candidate | Contents | Est. prod lines | Allowed dependencies |
|---|---|---|---|---|
| 0 | contracts | Core evidence contracts, identity and `AIRK-DIGEST-1` | ≈9k | — |
| 0 | os | `airk-process` plus IPC, native process, one `secure_fs`, durability and capacity, lock registry, `Clock` | ≈9k | — |
| 1 | read-contracts | Core read contracts **plus pure read-operation descriptors**: name, argument schema, intent vocabulary and budget class | ≈10k | contracts |
| 1 | control-proto | Core control and catalog contracts, split by plane; worker protocols; client RPC | ≈11k | contracts |
| 2 | languages | Grammars plus one `LanguageSpec` per language | ≈12k | contracts |
| 2 | ingest | Discovery, capture, documents, chunking | ≈20k | contracts, languages, os |
| 2 | retrieval | Unchanged. **Workbench intent vocabulary is injected by the composition root**; Retrieval does not depend on any read layer. | ≈7.7k | contracts |
| 2 | quota | Quota, funding, usage ledger | ≈6.7k | contracts, os |
| 2 | index-store | `current/*`, CAS objects, `FileStore`, `WriterLock`; traits for control-owned permits and authority | ≈20k | contracts, read-contracts, quota, os |
| 2 | control-store | Registry, catalog, jobs, recovery, split, integrity, clean-break reset; table-group handles | ≈30k | contracts, control-proto, index-store, quota, os |
| 3 | kernel | Indexing, publication, retrieval, reads, budgets; sub-facades | ≈40k | contracts, read-contracts, ingest, index-store, retrieval, quota |
| 3 | read-api | **Executable** read handlers over the descriptors, `ReadService`, admission, response display, Workbench dispatch, parity suite | ≈8k | read-contracts, kernel |
| 4 | mcp | rmcp adapter | ≈5k | read-api |
| 4 | https | Credentials, TLS, trust, gateway | ≈5k | os, control-proto, read-api |
| 4 | inference-host | Inference engine, embedding pool, bundled assets | ≈7.5k | os, control-proto, retrieval |
| 4 | supervisor | Project supervisor and workers | ≈13k | kernel, control-store, inference-host, os |
| 4 | catalog-control | Catalog, split and reference-refresh control | ≈10k | control-store, control-proto |
| 4 | service | Daemon composition, per-plane handlers, broker policy | ≈9k | supervisor, catalog-control, https, inference-host |
| 4 | installer | Runtime lifecycle, layout, reset, OS service managers, standalone setup | ≈20k | os, control-proto, control-store |
| 4 | frontend | Client-side composition | ≈12k | read-api, control-proto, os |
| 5 | cli (bin) | Argument parsing, role dispatcher and composition root (it also injects the intent vocabulary) | ≈5k | all |

**Revision-2 cycle fixed.** Revision 2 had Retrieval read its vocabulary from
a registry in `read-api`, which depends on Kernel, which depends on Retrieval.
Now:

- pure descriptors live in `read-contracts`;
- executable handlers live in `read-api`;
- the composition root passes the vocabulary to Retrieval's intent scorer.

The Workbench scorer is a fixed trained asset whose label order is part of
its identity. The injection must therefore verify that the injected label set
matches the asset's label manifest, and fail closed if it does not.

**Candidate dependency graph:**

~~~mermaid
flowchart BT
  C[contracts]
  OS[os]
  RC[read-contracts: descriptors] --> C
  CP[control-proto] --> C
  L[languages] --> C
  I[ingest] --> L & OS
  R[retrieval] --> C
  Q[quota] --> OS
  IS[index-store] --> RC & Q
  CS[control-store] --> CP & IS
  K[kernel] --> I & IS & R
  RA[read-api: handlers] --> K
  M[mcp] --> RA
  H[https] --> RA & CP
  IH[inference-host] --> R & CP
  SV[supervisor] --> K & CS & IH
  CC[catalog-control] --> CS
  SVC[service] --> SV & CC & H
  INS[installer] --> CS & OS
  FE[frontend] --> RA & CP
  CLI[cli: composition root] --> SVC & INS & FE & M
  CLI -. injects intent vocabulary .-> R
~~~

### 4.2 How each mechanism is addressed

| Mechanism | Response |
|---|---|
| M1 | One owner per contract; generation where the pilot proves it; shared decision cases and digest vectors for semantic rules |
| M2 | Remove mounts, `include!` and orphans; default `pub(crate)` |
| M3 | Table-group handles, per-plane enums, sub-facades; extract only by the §4.1 criteria |
| M4 | Allowed-edge gate; traits defined lower; untangle in place first |
| M5 | Evidence-classed deletion with the compatibility proof |
| M6 | Per-component T0, the applicability-matrix conformance suite, the harness seal, affected-test selection |
| M7 | `CONTRACT.md`, boundaries-only architecture document, generated status |
| M8 | Step-size limits; scripted mechanical moves |
| M9 | Registries for duplicated catalogs; exhaustive matches kept and checked for completeness |
| M10 | `Clock`, manual clock, event waits, machine-checked quarantine |
| M11 | Reviewed expected sets; literals moved to assets; long-line report |

### 4.3 Extension (candidate layering)

```
protocol (generated where the pilot proves it; hand-written semantic rules tested against shared Rust cases)
  └─ native-client   (transports; one facade per plane: CatalogClient, ReadClient, AdminClient, JobsClient)
       └─ intents    (journal and receipts; pure, injected clock)
            ├─ runtime-install · agent-config
            └─ features: explorer · indexing · console · workbench · assistance
webviews — consume protocol types only
```

- Each feature gets only its plane facades, never the whole `serviceClient`.
- Split `serviceExtension.ts` into a composition root plus `CleanBreakCapture`,
  `HttpTrust`, `AgentConnections`, `McpProviders` and `TreeView`, and break
  both import cycles.
- A binding-identity seam (see S1.2) comes first, because the R39 failure
  points at that file's binding-refresh path.

### 4.4 Scripts, harnesses and evidence

- **`scripts/airk_harness/`** provides `proc`, `digest`, `receipt` and
  `seal`. T2/T3 entry points refuse to run unsealed, and refuse to report a
  pass if a sealed input changed.
- **One schema-versioned JSON record per gate run** holds the command, the
  selected test identities and their outcomes, input identities, elapsed time
  and log location.
- Reorganize the scripts by purpose, archive the uncalled ones, and promote
  or delete the `artifacts/` harnesses (decision D4).

### 4.5 Documentation

- `CONTRACT.md` per component.
- `architecture.md` describes boundaries only.
- History moves to `docs/history/`.
- `docs/status.md` is generated from gate records.
- Shared README text is generated from one source.
- A reported context-size measure with a target of about 25k tokens.

### 4.6 Conformance with an applicability matrix

Each registered operation (read tool, control method, job kind) declares which
properties apply, plus its own operation-specific cases. "Not applicable"
needs a one-line reason. The suite checks that every registration has a
declaration.

| Property | Typically applies to | What is checked |
|---|---|---|
| P1 Idempotent request identity | Accepted mutations, durable intents | The same request ID returns the same receipt, with no second effect |
| P2 No replay of uncertain outcomes | Mutations, submitted reads | An execution whose outcome is unknown is never retried automatically |
| P3 Scope rejection | All | A crossed project, grant, index or root is refused with a typed error |
| P4 Late-result fencing | Worker-delivered results, jobs, catalog outcomes | A result after an epoch, generation, worker or revision change is discarded, and the newer state is kept |
| P5 Typed retry classification | All refusals | Only an explicitly busy refusal made before acceptance is retryable, within its original budget |
| P6 Budget and truncation disclosure | Reads | Output stays within the budget and discloses truncation and continuations |
| P7 Revision identity disclosure | Reads | **A repeated read may observe newer state**, but must identify the publication or revision it observed |
| P8 Cross-transport equality | Reads | CLI, MCP, HTTPS and Workbench results compared as **parsed results**, normalizing only documented transport identities and timing |
| P9 Exact bytes | Digested and persisted formats (receipts, journals, credential ledgers, digest inputs) | Byte-for-byte equality with captured fixtures |

### 4.7 Wire-format and hash stability

Generators can silently change:

- field order;
- absent versus `null` fields;
- enum tagging;
- integers above 2^53;
- Unicode escaping.

Before any generation work:

1. Capture `fixtures/wire/`: canonical request and response JSON, digest
   vectors, persisted journal and receipt samples, and **malformed-input
   cases** with their expected rejections.
2. Every implementation must reproduce P9 formats byte for byte and reject
   the same malformed inputs.
3. Persisted formats under `airk-state/1` change only with a new state
   contract or rebuild boundary.
4. Any deliberate break is a single explicit decision (D8).

---

## 5. Execution plan

### 5.1 Track R: release stabilization (narrow)

Track R covers only what 1.9.0 needs. It runs only under explicit user
authorization, and follows the existing run rules, the installer stop record
and the two-strike rule.

**R0. Retain the blockers.**

- `clean-break-r39-installed-editor-r3` (`workspace_registration_changed`)
  and the remaining gates listed in the stop record are 1.9.0 acceptance
  blockers.
- This plan does not authorize resuming that repair. Resumption is a separate
  user decision (D10), starting from the investigation and cheap regressions
  that the stop record calls for.

**R1. Compatibility inventory** (shared with S1.1; Appendix D). Required
before any change that touches predecessor handling or schemas.

**R2. Evidence-backed dead-code removal only.**

- Kernel orphan files: not compiled.
- Store generation-era code: confirm no production callers by deleting it
  and getting a clean compile and Clippy, then compare selected test
  identities and outcomes.
- Scripts with no caller.
- **Not in Track R:** migration and adoption code, schema collapse,
  retirement code, scan API changes, embed-pool build changes, formatting,
  CI selection and the docs reorganization. These move to Track S.

**R3. Seams for known blockers only.**

- Add the smallest seam that a blocker's root-cause note names. For example,
  a deterministic test of binding-identity equivalence across
  `refreshOnce`/`hasManualVscodeConnection` with an injected refresh.
- Do not add any seam without a named blocker.

**R4. Measurement.** Add `scripts/dev/simplify_metrics.py` as a local,
report-only script. Changing CI selection is out of scope for Track R.

**Release branch.**

- Once the blockers are resolved and qualified, cut `release/1.9` from
  `main` (with authorization, D7).
- Only qualification fixes land there, each merged forward to `main`.
- Track S work beyond milestone S1 lands on `main` after the cut.

### 5.2 Track S: restructuring

#### Milestone S1: prove the approach on one bounded slice (next milestone)

**S1.1 Compatibility inventory** (Appendix D). For every supported or
recognized predecessor layout, record:

- how it is discovered;
- what is captured (source selections and preferences);
- how predecessor shutdown is proven;
- the resumable cleanup and retirement path;
- the code that implements each duty today;
- the tests or receipts that prove it.

Separately record the exact on-disk schema that `airk-state/1` installations
created by 1.8.16 now have, and the code that reads it. Fresh Genesis writes
`user_version 12` and then applies the authority, policy and split layers, so
that layered layout **is** the supported current layout.

**S1.2 Smallest useful regression seams.** At most three, chosen by recorded
failure classes:

- the extension binding-identity seam (shared with R3);
- a `Clock` with a manual clock in the S1.4 component only;
- one golden-output capture for the S1.3 operations.

**S1.3 One contract-generation pilot.** Pick one representative read
operation (for example `file_info`) and one control operation (for example a
catalog receipt read). Prove:

- serialization identical to `fixtures/wire/`, as P9 exact bytes where
  digested or persisted;
- rejection of the same malformed inputs;
- integer handling above 2^53, and bounds;
- how semantic validation is shared, using `readRetry`-style rules as
  shared Rust-emitted decision cases with the hand-written TypeScript
  retained.

Compare at least two tools (for example `schemars` plus a TypeScript
generator, and `ts-rs`). Choose a pipeline only after the pilot (D5).

**S1.4 Untangle one component in place.**

- *Recommended:* the CLI filesystem and layout helpers that give
  `runtime_install` its fan-in of 35 (`safe_file`, `validate_control_root`,
  `main_name`, `installed_control` and related helpers).
- Give them an owned module inside `airk-cli`, invert the `user_paths` →
  `runtime_install` call, and re-measure the module graph.
- Write the module's `CONTRACT.md`. Do not extract a crate.

**S1 exit and review gate:**

- The compatibility inventory has been reviewed.
- The seams have caught or reproduced at least one recorded failure class.
- The pilot results and the chosen tool, or the decision not to generate, are
  recorded.
- The S1.4 cycle and fan-in measurements are taken before and after.
- Gate records exist for every check that ran.
- **The user reviews the results before Milestone S2 is planned in detail.**

#### Later milestones (conditional on the S1 review)

These are candidates in a likely order. Each needs its own approval, and each
uses the principles in §3.

| Candidate milestone | Content | Precondition |
|---|---|---|
| S2 Deletion with proof | Migration and adoption, legacy runtime paths, retirement. Delete only rows of the compatibility inventory whose duties have a proven surviving owner. | S1.1 reviewed; D2 |
| S3 Schema consolidation | Write the current layered layout directly at Genesis **only if it produces a schema identical to existing `airk-state/1` installations** (compared from a `sqlite_master` dump), with retained 1.8.16 fixtures proving those installations still open. Otherwise keep the layering. Any different final layout needs a new state contract (D8). | S2; retained historical schema fixtures |
| S4 Module honesty | Remove `#[path]` mounts with committed scripts. Replace the embed-pool `include!` scheme. | Golden outputs; boundary checks below |
| S5 Boundary gates | Allowed edges, module honesty, registration completeness and declared state ownership, with a reviewed-exception file. Size and vocabulary checks stay report-only. Affected-test selection; machine-checked quarantine. | S4 for module honesty |
| S6 Contract ownership | Split Core by layer; adopt generation as far as the pilot proved it. | S1.3 results; D5, D8 |
| S7+ Untangle and extract | Repeat the S1.4 approach per cluster: CLI os and layout, HTTPS, inference host, Store filesystem and lock layer, quota and index-store, read-api, supervisor and catalog control, installer, extension layering. Extract crates only by the §4.1 criteria. | Per-candidate approval |

**Boundary checks for mechanical moves** (this replaces revision 2's blanket
"No T2 required"):

- Pure file moves with no feature, asset or package change: compile, Clippy,
  and selected test identities and outcomes identical.
- Embed-pool or feature changes: build every affected feature combination
  (lightweight no-feature main, `local-inference`, and `embedding-worker`
  while it is retained). Run the runtime builder's verification of the sealed
  main, embedded model execution and import inventory. Run the affected
  process tests.
- Changes that move assets into the binary (`include_str!` SQL or JSON):
  package seal verification, plus the native and VSIX packaging checks for
  the affected artifact.
- Anything that changes a packaged artifact's identity: the T2 check at that
  artifact's boundary, on frozen inputs.

---

## 6. Operating model for AI coding agents

1. **One component per task.** A task card names:
   - the component and its `CONTRACT.md`;
   - the files it may edit and the files it must not edit;
   - its T0 command and its conformance applicability;
   - the golden outputs to reproduce;
   - "Done when".
2. **Contract and kernel changes are their own small steps, landed first.**
   The integrator changes the contract, descriptor or hook first, then
   consumers follow in per-component tasks.
3. **Step size.**
   - A behavior step is at most about 25 files and 1,500 changed lines, not
     counting generated output.
   - A larger mechanical move must come from a committed script.
   - A structural step edits no test assertions.
4. **Read only what is needed:** `architecture.md`, the component's
   `CONTRACT.md`, and its source and tests.
5. **Run only the affected tests.** T0 is the component; T1 is the
   affected-test selection; T2/T3 run on sealed inputs and write gate records.
6. **Determinism.** New code takes a `Clock`. A flaky test is quarantined with
   an owner and expiry, never retried or given a longer deadline.

---

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Restructuring destabilizes the unfinished 1.8.16 baseline | Track R limits pre-release change to evidence-backed dead-code removal and blocker seams. All other restructuring is Track S, gated by the S1 review. |
| Deletion removes a live predecessor duty (discovery, capture, shutdown proof, cleanup) | Only inventory rows with a proven surviving owner are deleted. The tests for the surviving path must pass before deletion. |
| Schema consolidation breaks existing 1.8.16 state | S3 requires schema identity with existing `airk-state/1` installations and retained historical fixtures. Otherwise the layering stays. |
| Generation drops semantic validation | The pilot proves malformed-input rejection and semantic validation first. Semantic rules stay hand-written, tested against shared decision cases. |
| Registries hide missing handlers or authorization | Exhaustive enums are kept. The completeness check ties each variant to a handler, an authorization rule and a conformance applicability entry. |
| Guardrails invite cosmetic compliance | Only boundary properties gate. Size, visibility and vocabulary are reported, never gated. |
| Tautological tests | Expected sets are reviewed, checked-in lists, never derived from the code under test. |
| Crate proliferation without benefit | Crates are extracted only by the §4.1 criteria. Otherwise a module with an owner and enforced edges is enough. |
| Platform regressions | Decisions move out of FFI first. Neo scoped Clippy comes first after Unix-touching steps. |
| Release-branch fixes lost | Each fix is merged forward to `main`, with gate records citing both commits. |

---

## 8. Metrics

`scripts/dev/simplify_metrics.py` (Track R4) regenerates both tables. Windows
are fixed per row. Targets are set at the S1 review, except where shown.

### 8.1 Structural metrics (reported; inform review)

| Metric | Window or scope | Baseline (9 Nov 2026) |
|---|---|---|
| Commits spanning 3+ / 5+ components | last 200 commits | 57% / 30.5% |
| Files per commit (mean / median) | last 100 commits | 49.6 / 18 |
| Forbidden dependency edges (crate and top-level module), once edges are approved | tree | to measure after approval |
| Top-level module cycles in airk-cli | tree | every major cluster pair except IPC |
| `#[path]` (non-platform) / `include!` of `.rs` source / orphan files | tree | 314 / 17 / 4 |
| Places to edit per change kind (Appendix C) | tree | read tool: about 12 code sites; `JobKind`: 54 files; `ControlMethod`: 148 + 79 match arms; language: 10 files |
| Duplicated catalogs (tool-name tables, hand-copied counts) | tree | 6 tables; ≥10 hand-copied counts |
| `ServiceRegistry::open*` call sites in CLI | tree | 134 in 54 files |
| Hand-written wire mirrors (`validate*`, limits, digest copies) | tree | 75 / 39 / 8 |
| Crates over 20k non-test-file lines | tree | 5 |
| Files over 2,000 lines; functions over 300 / 150 lines | tree | 19; 37 / 192 |
| `pub mod` declarations | tree | 271 |
| Direct time reads in production; sleeps in tests (Rust / extension); mutable statics | tree | ≈760; 208 / 86; 51 |
| Rust lines over 200 characters | tree | 819 |
| Required reading for an agent task | docs | ≈45k tokens (≈105k for extension work) |
| Untracked harness scripts | `artifacts/` | 1,195 |

### 8.2 Outcome metrics (what the plan is for)

| Metric | Source | Baseline |
|---|---|---|
| Regression recurrence: the same failure class failing again after a fix | gate records and ledger | five recurring classes (completion plan §0) |
| T2/T3 runs per accepted change; two-strike stops | gate records | 22 Windows debug runtimes and more than 40 rerun cycles (historical); the R39 stop |
| Validation cost: wall time of T2/T3 per accepted change | gate records | to measure from S1 onward |
| Task duration: task card opened to accepted | task ledger | to measure from S1 onward |
| Fix commits as a share of commits | git, last 100 commits | to measure (22% across all commits) |
| Failures first found at T2/T3 that a T0/T1 check could have found | gate records and review | to measure from S1 onward |
| Median T0 time per component | gate records | crate-wide only today |

Success means the outcome metrics improve. Improving structural metrics alone
is not success.

---

## 9. Decisions for the user

**D1. Tracks and order.**

- *Recommended:* run Track R narrowly as in §5.1, and Milestone S1 in
  parallel; S1 only adds an inventory, seams, one pilot and one in-place
  untangling. Everything after S1 waits for its review.
- *Alternative:* defer S1 until after 1.9.0.

**D2. Deletion standard.** Approve deletion only by the evidence classes in
§3.1. Approve the compatibility inventory as the prerequisite for removing
any predecessor or migration code. Line counts are estimates only.

**D3. Governance changes.** Enacted early, on 9 November 2026, at the
user's request. CLAUDE.md and AGENTS.md (kept byte-identical) now include:

- a "Simplification and structural changes" section;
- the shorter, contract-first reading guidance;
- crate extraction only by the §4.1 criteria with user approval;
- this plan's §9 decisions among the frozen decisions.

Revisit the wording at the S1 review, based on which rules helped. The
boundary gates themselves (S5) still need their own approval.

**D4. Untracked `artifacts/` harnesses.** Approve promoting or deleting them.
They are untracked, so deletion cannot be undone from the repository.

**D5. Generation tool.** Decide after the S1.3 pilot, which compares at least
two tools.

**D6. Documentation archive** and a generated `docs/status.md`.

Step D6a (archive of top-level dated records into `docs/history/`) is
implemented by `scripts/dev/d6_archive_docs.py` and was reviewed on 9 November 2026. Step D6b
(`docs/reviews/`, `docs/issue-analysis/`, `docs/explorer/`) and the generated
`docs/status.md` are not started.

Step D6c (9 November 2026): `docs/architecture.md` now describes boundaries
only. Its former phase log is kept unchanged, apart from link updates, in
`docs/history/architecture-log-2026-11-09.md`.

**D7. Commit and branch authorization.** The checkpoint commit `9337822`
already exists. Separately authorize:

- commits for Track R and S1 work;
- merges to `main`;
- creation of `release/1.9`.

**D8. Wire and hash stability.**

- *Recommended:* no wire, digest or persisted-format change for 1.9.0.
- A later deliberate break needs a version bump and, for persisted state, a
  new state contract or rebuild boundary.

**Narrow exception approved by the user, 9 November 2026:** the 1.8.17
security update to `rmcp` 3.9.0 may add `ttlMs: 0` and
`cacheScope: "private"` to MCP catalog replies for protocol 2026-07-28 and
later. Keep these fields in complete transport results and test modern and
legacy negotiation explicitly. This authorizes no change to tool-call data,
digest inputs or persisted formats. See the
[security review](simplify/security-readiness.md) and
[release-readiness ledger](simplify/release-readiness.md).

**D9. Report thresholds.** These are reported, not gated:

- 20k lines per crate;
- 1,200/2,000 lines per file;
- 150/400 lines per function;
- 120 characters per line;
- 25 files and 1,500 lines per step;
- 60-second T0 commands;
- a 25k-token context.

**D10. Installed-editor repair.** Whether and when to resume repair of
`clean-break-r39-installed-editor-r3` and the remaining stop-record gates.
This plan does not authorize it.

**D11. S1.4 component.** *Recommended:* the CLI filesystem and layout helpers
behind `runtime_install`'s fan-in of 35. The alternative is the Store
filesystem and lock layer.

---

## Appendix A: Deletion candidates by evidence class

| Candidate | Est. lines | Evidence class | Track |
|---|---|---|---|
| Kernel `conditional.rs`, `lineage.rs`, `reconciliation.rs`, `suggestions.rs` | 2,061 | Not compiled | R2 |
| Store `sqlite.rs`, `sqlite/publication.rs`, `history.rs`, `history_policy.rs`, `maintenance.rs` (keep `sync_parent`, `check_writer`), `checkpoints.rs`, `retention.rs`, `normalized.rs`, `catalog_experiment.rs`, `vector_registry.rs`, `vector_policy.rs`, `verified_cache.rs`, generation part of `blobs.rs`; their `tests/` | ≈18k | No production callers (confirm with a clean compile) | R2 |
| Scripts with no caller or test; `release-1.4.4-repair` plan | ≈3k | No caller | R2 |
| `pub` items reported unused after `pub(crate)` tightening | to measure | Compiler-reported | S2 |
| Store `service_catalog_upgrade*`, `_compatibility`, `_v1_settlement`, `_settlement`, `_import`, `_exclusion`, `_adopted_*`, `legacy_adoption*`, `maintenance_legacy`, `legacy_saved` | ≈13.8k | Superseded by policy; **compatibility proof required** | S2 |
| Registry steps 1–12 and non-final `CatalogLayout` variants | part of the above | **Schema-identity proof required** (S3) | S3 |
| CLI `runtime_legacy_*`, `runtime_http_legacy`, `catalog_legacy_adoption`, `legacy_startup*`, `legacy_processes`, `embed_pool_legacy_*`, `runtime_data_migration` | ≈11.5k | Superseded by policy; compatibility proof required. Some implement live predecessor discovery or shutdown proof. | S2 |
| Kernel `legacy_adoption`, `service_catalog_upgrade`, `historical`; Core legacy and upgrade-recovery modules; `ArtifactRef.legacy_origin` | ≈2k | Same | S2 |
| Store retirement (`current/legacy_retirement`, `quota/retired*`) | ≈2.3k | Replace `current_observation` first | S2 |
| Ingest `adapters/legacy.rs` | 685 | Investigate callers | S2 |
| Extension legacy paths (keep clean-break capture) | ≈1k | Compatibility proof required | S2 |

## Appendix B: Hub and cycle quick reference

**airk-cli**

- Fan-in hubs: `local_ipc` 40, `runtime_install` 35, `runtime_pair` 21,
  `user_paths` 19, `service_manager` 18, `native_process` 16.
- Cycles: `runtime_install` ↔ {`service_runtime`, `service_manager`,
  `http_endpoint`, `user_paths`, `process_roles`}; `user_paths` ↔
  `universal_launcher`; `runtime_pair` ↔ `process_roles`; `service_runtime` →
  `service_management` → `project_supervisor` → `service_runtime::checkpoint`;
  `catalog_coordinator` ↔ `project_supervisor`; `service_runtime` ↔
  {`combined_frontend`, `project_client`, `usage_client`}.

**airk-store**

- Fan-in hubs: `quota` 35, `FileStore` 32, `service` root 30,
  `managed_collection` 26, `maintenance` 24, `WriterLock` 21,
  `catalog::setup` 19, `capacity` 17, `durability` 15.
- Cycles: `current` ↔ `service`; `quota` ↔ `catalog::split`; `quota` ↔
  `current`; `FileStore` ↔ `service`; the generation-era tangle.

**airk-core**

- Hub: `service.rs` imports 30 sibling modules.
- Cycles: `combined_reads` ↔ `verification`, and `workbench` ↔
  `verification`.

**packages/vscode**

- Fan-in: `indexingCatalog` 23, `explorerContracts` 23 (+19 webview),
  `serviceClient` 18 (+28 tests).
- Fan-out: `serviceExtension.ts` 38, `explorerController` 28.

## Appendix C: Places each kind of change must edit

Re-measure at every milestone exit. The goal is that duplicated catalogs
disappear, while compiler-checked exhaustive matches remain.

**Add or rename a public read tool:**

1. `airk-mcp/src/lib.rs`: the argument struct and `#[tool]` handler.
2. `airk-mcp/src/unified.rs`: the catalog entry and `unified_explorer.json`.
3. `airk-mcp/src/combined.rs`: hand-written `json!` schema properties.
4. `airk-mcp/src/workbench.rs`: the dispatch match.
5. `airk-mcp/src/help/topics.rs`.
6. `airk-kernel/src/workbench.rs`, `workbench/natural.rs`,
   `workbench/combined.rs`: tool-name tables.
7. `airk-retrieval/src/intent.rs`: the intent vocabulary.
8. `airk-cli/src/main.rs::query()` and `tools.rs`.
9. Core read contracts.
10. Extension `explorerContracts.ts` and Workbench webview.
11. Catalog-count assertions (3).
12. The generated CLI reference, plus hand-written counts in about 8 docs.

**Add a `ControlMethod`:** Core `ControlMethod`/`ControlResponse`;
`service_management.rs` (148 arms); `console_client.rs` (79 arms); client
request helpers; extension `serviceClient.ts` and validators; console UI;
docs.

**Add a `JobKind`:** Core; Store `service_jobs`, `service_background`,
`service_recovery`, `service_inspection` and `service_automatic_audit`; CLI
`project_worker`; Kernel `service_resources`; extension inspection. 54
production files in all.

**Add or adjust a language:** Ingest `Cargo.toml`, `syntax.rs` matches,
`adapters/registry.rs`, language and specialist modules, `project_build`; Core
`search_filters`; Kernel `workbench.rs`; MCP `lib.rs`; docs.

## Appendix D: Compatibility inventory template (S1.1 / R1)

One row per recognized predecessor layout, plus one row for each current
`airk-state/1` layout.

| Layout or version range | Discovery (code; proof) | Captured selections and preferences (code; tests) | Shutdown and exit proof (code; receipts) | Resumable cleanup and retirement (code; receipts) | Refusal cases | Code currently serving it | Surviving owner after S2 | Status |
|---|---|---|---|---|---|---|---|---|
| e.g. 1.5.x multi-process with registry schema 8 | | | | | | | | |
| e.g. 1.8.11 pair (launcher + `airk-embed`) | | | | | | | | |
| **1.8.16 `airk-state/1`, Genesis `user_version 12` + authority, policy and split layers** | n/a (current) | n/a | n/a | n/a | n/a | `service_catalog_genesis.rs`, `service_catalog.rs` layers | must remain readable | to record |

**Rules:**

- Code may be deleted only when every row it serves names a surviving owner
  whose tests pass.
- The `airk-state/1` row must have retained schema fixtures before S3.

## Appendix E: Disposition of the revision-3 review

| Review point | Disposition | Where |
|---|---|---|
| 1. Separate release stabilization from restructuring; keep the installed-editor failure as a blocker; no authorization to resume repair | **Accepted.** Track R is limited to evidence-backed dead-code removal, blocker seams, the inventory and report-only measurement. Schema collapse, scan API, embed-pool, formatting, CI selection and docs moved to Track S. | §1, §5.1, D1, D10 |
| 2. Compatibility proof for deletion; `airk-state/1` must stay supported; line counts are estimates | **Accepted.** Evidence classes, the inventory template and the schema-identity precondition for S3 were added. The line-count exit criterion was removed. | §3.1, §5.2 (S2, S3), Appendices A and D |
| 3. Cycle: retrieval → read-api → kernel → retrieval | **Accepted; it was a defect in revision 2.** Descriptors now live in `read-contracts`, handlers in `read-api`, and the vocabulary is injected by the composition root and checked against the scorer asset's label manifest. | §4.1 |
| 4. The crate map is a hypothesis; extract only with a stable interface, independent need and testing benefit; keep exhaustive dispatch | **Accepted.** Extraction criteria were added, the map is labeled a hypothesis, and registries are limited to duplicated catalogs with a completeness check. | §3.3–3.5, §4.1 |
| 5. Pilot generation first; semantic validators differ | **Accepted.** S1.3 pilots one read and one control operation and compares tools. Semantic rules stay hand-written against shared decision cases; digest vectors are shared. | §3.2, §4.7, §5.2 S1.3 |
| 6. Applicability matrix; exact bytes versus parsed equality | **Accepted.** Properties P1–P9 are declared per registration, with P8 parsed equality for transports and P9 exact bytes for digested and persisted formats. | §4.6 |
| 7. Independent expectations; compare identities and outcomes; remove the blanket "No T2 required" | **Accepted.** Expected sets are reviewed lists, historical fixtures are retained, and boundary checks are required for feature, asset and package changes. | §3.10–3.11, §5.2 |
| 8. Gate boundaries, not vocabulary; separate structural from outcome metrics; fix the 200/100 window mismatch | **Accepted, with one adjustment.** Vocabulary, size and visibility checks are kept as **reported** signals rather than dropped. They are cheap and point reviewers at real hotspots such as `project_worker::run`, but they never gate. Windows are now labeled per row. | §3.8, §5.2 S5, §8 |
| Next milestone: inventory, smallest seams, one generation example, one in-place untangling, then review | **Accepted** as Milestone S1 with an explicit review gate. | §5.2 |
