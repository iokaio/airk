# AIRK — Audit Remediation Requirements

**Derived from:** AIRK Shark Tank Audit and Test Readiness Review, 3 November 2026 (audited at v1.8.7).
**Audience:** coding agents (Cline, Claude Code, Codex) and human maintainers implementing remediation on `github.com/iokaio/airk`.
**Status of this document:** draft requirements extracted from the audit. Every requirement traces back to an audit section (`§`). Nothing here changes product behaviour unless a requirement says so explicitly and the owner has approved it.

---

## How to read this document

- **Requirement IDs** are stable (`REQ-<area>-<n>`). Reference them in branch names, PR titles, commit trailers and issue titles.
- **Priority:** P0 = blocks M1–M3 and the pitch; P1 = needed for M4; P2 = valuable, not on the critical path.
- **Milestone** maps to the audit's 90-day plan (M1 Week 2, M2 Week 3, M3 Week 6, M4 Week 9, M5 Week 12).
- **Acceptance** criteria are the only definition of done. An agent must not mark a requirement complete on the basis of "implemented"; it must produce the evidence the acceptance criteria name.
- **Owner-decision** items are flagged. Agents prepare the work; the owner (Tyler Jensen) makes the call.

### Area index

| Area | Code | Audit source | Milestone |
|---|---|---|---|
| Global constraints | G | §8 (last paragraph), §7 CI cost control, §5, §11 | all |
| Reproducible build and bootstrap | A | §7 T0, §4 truth 10, §9 M1 | M1 |
| Ignore triage | B | §7 T0 exit, §6 Native integration | M1 |
| Coverage | C | §7 T1, §6 Coverage, §9 M2 | M2 |
| Retrieval quality evaluation | R | §7 T2, §3.3, §4 truths 3 and 7, §9 M3 | M3 |
| Property-based testing and fuzzing | F | §7 T3, §6 Adversarial inputs | M3/M4 |
| Performance and scale | P | §7 T4, §4 truth 4, §6 Scale | M4 |
| Fault, soak, privacy and security | S | §7 T5, §3.4, §4 truths 5 and 6, §6 | M4 |
| Mutation testing | M | §7 T6, §6 Coverage and test quality | post-M4 |
| CI integration and cost control | CI | §2.3, §7 CI cost control | ongoing |
| Risk-path code review | X | §8 item 6 | M4 |
| Documentation, positioning and Marketplace | D | §1, §4 truth 1, §8 item 7 | M3 |
| Release process and bus factor | L | §3.5, §4 truths 2 and 8, §8 item 8 | ongoing |
| Activation measurement | T | §3.2, §4 truth 9 | owner decision |
| Owner-only actions (not agent work) | O | §3.1, §3.2, §9 M5, kill criteria | M5 |

---

## G — Global constraints (apply to every requirement)

### REQ-G-1 — Protected surfaces require explicit owner direction
- **Source:** §8 "What I should not do without your explicit direction"
- **Requirement:** Agents shall not change, without a written instruction from the owner naming the change: index formats; MCP contracts; collection identity; crate dependency direction; version numbers; signed-release workflows; promotion workflows (`scripts/ci-plan.py` main-branch promotion logic). Agents shall not commit to `main`; all work lands via branches and pull requests.
- **Acceptance:** Every PR touching `.github/workflows/`, `scripts/ci-plan.py`, `rust-toolchain.toml`, any `Cargo.toml` `[package].version`, or any crate's public contract types links to the owner instruction authorising it, or is limited to additive new jobs/files that do not alter existing jobs.

### REQ-G-2 — Verify "absent" claims before implementing
- **Source:** §11 "Documentation was sampled, not exhausted … check before acting on any 'absent' claim in sections 2.2 and 6"
- **Requirement:** Before implementing any requirement that assumes a capability is missing (coverage, proptest, fuzzing, benches, mutation testing, adversarial corpus, upgrade matrix, privacy scripts), the agent shall search the repository (`docs/`, `scripts/`, `crates/*/tests`, `packages/vscode`, `.github/`) for an existing implementation. If one exists, the agent extends it rather than duplicating it and records the finding in the PR description.
- **Acceptance:** Each PR for a "missing capability" requirement contains a "Prior art check" section listing the search terms used and the result (none found / found at `<path>` and extended).

### REQ-G-3 — Preserve the lean-CI policy
- **Source:** §7 "CI cost control"
- **Requirement:** Routine pull-request CI gains at most: the coverage-delta gate (REQ-C-3) and a bounded fast subset of property tests (REQ-F-4). Retrieval evaluation, benchmarks, scale campaigns, fault/soak/privacy campaigns and mutation testing run only on a nightly schedule or on release candidates.
- **Acceptance:** A measured comparison of PR-pipeline wall-clock and billable minutes before and after each CI change is recorded in `docs/ci-and-builds.md`; per-PR cost increase attributable to this programme stays within a budget the owner sets (default proposal: +15% minutes).

### REQ-G-4 — Conform to existing project conventions
- **Source:** §5 (SPDX headers, DCO, pinned action digests, parity rule, validation vocabulary)
- **Requirement:** All new files carry SPDX headers; all commits are DCO signed-off; any new GitHub Action is digest-pinned with checksum verification consistent with existing workflows; any new behaviour exposed to users respects CLI/Workbench/MCP parity; all new qualification documents use the project's existing vocabulary (`implemented`, `bounded`, `not run`, `unqualified`).
- **Acceptance:** `scripts/check-repo.ps1`, actionlint and the existing `quick` job pass on every PR; reviewers confirm vocabulary use.

### REQ-G-5 — Every measurement carries its provenance
- **Source:** §7 T4 ("reporting hardware identity with every number"), §7 T5 ("dated receipts")
- **Requirement:** Every number published under this programme (coverage, IR metrics, latency, RSS, soak results, privacy receipts) shall be recorded with: date (UTC), git commit hash, hardware identity (CPU model, core count, RAM, storage type, OS and version), corpus identity (manifest hash or commit), and the exact command used.
- **Acceptance:** A shared report header schema (`docs/testing/report-header.md` or a JSON schema under `scripts/`) exists and every published table/JSON in this programme uses it.

### REQ-G-6 — No egress, ever
- **Source:** §3.4 (no cloud egress is a verified strength), §9 kill-criteria fallback ("nothing leaves the machine")
- **Requirement:** No requirement in this document may be implemented in a way that causes the shipped product to send data off the machine. Evaluation harnesses may fetch public corpora at development time only; shipped binaries and the extension do not.
- **Acceptance:** Any PR touching network code in shipped crates or the extension is rejected unless it cites an owner instruction.

---

## A — Reproducible build and contributor bootstrap (M1, P0)

### REQ-A-1 — One-command bootstrap scripts
- **Source:** §7 T0, §8 item 4, §4 truth 10
- **Gap:** A bare machine with Python 3.12, Node 22, uv and git cannot run a single test. Contributors must discover `rust-toolchain.toml`, `scripts/provision-local-model.py --download` and `scripts/provision-onnx-runtime.py --download` on their own.
- **Requirement:** Provide `scripts/bootstrap.ps1` (Windows) and `scripts/bootstrap.sh` (Linux/macOS) that take a clean clone to a fully green test run with no further human input. The scripts shall:
  1. Check prerequisites (git, rustup, Python 3.12+, Node 22+) and print one actionable install instruction per missing prerequisite, then exit non-zero.
  2. Run `rustup show` to install the toolchain pinned in `rust-toolchain.toml` (1.98.0 at audit time) and any required components (clippy, rustfmt, llvm-tools-preview once REQ-C-1 lands).
  3. Run `python scripts/provision-local-model.py --destination target/local-vector-model --download` and `python scripts/provision-onnx-runtime.py --download`, honouring existing digest verification; skip when already provisioned.
  4. Install `cargo-nextest` if absent (or fall back to `cargo test` with a warning).
  5. Run, in order: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo nextest run --workspace --no-fail-fast`; `npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode`; `npm test --prefix packages/vscode`; `python -m unittest discover -s scripts/tests -v`.
  6. Print a final summary: pass/skip/fail counts for each of the three suites, total elapsed time, and the commit hash.
  7. Be idempotent: a second run on the same machine performs no re-downloads and finishes faster.
  8. Support `--check-only` (prerequisites only), `--no-tests` (provision and build only), and `--fast` (skip clippy) flags.
- **Acceptance:**
  - On a fresh VM or container for each of ubuntu-24.04, windows-2025 and macos-15, a person who has never seen the repository runs one command and obtains counts matching the recorded 1.8.7 results within tolerance (approximately 1,811 Rust passed / 36 skipped, 541 extension passed / 19 skipped, 296 Python with 6 skipped), or the differences are explained in the PR.
  - Wall-clock time from clone to green is recorded per OS in `CONTRIBUTING.md`.
  - The scripts themselves are covered by a CI job that runs `--check-only` on all four OS targets (full run is nightly, see REQ-CI-1).

### REQ-A-2 — CONTRIBUTING.md leads with bootstrap
- **Source:** §7 T0, §3.5
- **Requirement:** `CONTRIBUTING.md` shall begin with a "Clone to green" section of at most 10 lines: the bootstrap command per OS, expected duration, expected counts, and where to look when it fails. All prerequisite detail currently scattered in the document moves below it or into `docs/`.
- **Acceptance:** A reviewer with no prior knowledge reads only the first screen of `CONTRIBUTING.md` and can state the single command to run.

### REQ-A-3 — Reproduction record for 1.8.7 numbers
- **Source:** §7 T0 exit, §11 "No test execution"
- **Requirement:** Run the full T0 sequence on at least one machine and record the actual counts, OS, hardware identity, toolchain versions and duration in `docs/validation.md` (or the location the project uses for validation receipts) using the REQ-G-5 header.
- **Acceptance:** The entry exists, is dated, and either matches the release-1.8.7 numbers or lists every discrepancy with a cause.

---

## B — Ignore triage (M1, P0)

### REQ-B-1 — Classify all `#[ignore]` tests
- **Source:** §7 T0 exit, §6 Native integration ("37 ignores, several requiring a sealed payload and manual approval")
- **Requirement:** Produce `docs/testing/ignore-triage.md` listing every `#[ignore]` attribute in the workspace with: crate, file path, test name, current reason string (if any), and classification: **(a)** subprocess/helper entry point not meant to run standalone; **(b)** qualification-only (needs sealed payload, manual approval, or long runtime); **(c)** real debt — a test that should run but does not. For every (c) entry, state the blocker and the remediation.
- **Acceptance:**
  - Count in the document equals the count from `rg -c '#\[ignore' crates` on the same commit.
  - Every (c) item has a tracking issue opened on the repository, linked from the document.
  - The list of (c) items is published (linked from `docs/validation.md`).

### REQ-B-2 — Machine-enforced ignore reasons
- **Source:** §7 T0 exit (keeps the triage from rotting)
- **Requirement:** Every `#[ignore]` shall carry a reason string of the form `#[ignore = "<category>: <reason>"]` where `<category>` is one of `helper`, `qualification`, `debt`. Add a check to the existing `quick` CI stage (extend `scripts/check-repo.ps1` or add a small Python script under `scripts/`) that fails when an `#[ignore]` lacks a category or uses an unknown one.
- **Acceptance:** The check passes on the triaged tree and fails on a test fixture containing a bare `#[ignore]`.

---

## C — Coverage baseline and gate (M2, P0)

### REQ-C-1 — Coverage measurement job
- **Source:** §7 T1, §2.2 "Coverage measurement: absent", §8 item 2
- **Requirement:** Add a `cargo-llvm-cov` based coverage run (`cargo llvm-cov nextest --workspace --html --output-dir artifacts/coverage` plus an `lcov`/JSON export) as a CI job. It runs on `main` after merge and nightly; on PRs it runs only the delta gate (REQ-C-3). The HTML and JSON reports are uploaded as workflow artifacts.
- **Acceptance:** A workflow run on `main` produces per-crate and per-module line and region coverage in both HTML and machine-readable form; the job is documented in `docs/ci-and-builds.md`.

### REQ-C-2 — Published coverage baseline
- **Source:** §7 T1 exit, §9 M2 ("Coverage is a number in docs/")
- **Requirement:** Commit `docs/testing/coverage.md` containing: per-crate line and region coverage with REQ-G-5 provenance; a table of every module (file) over 500 lines with coverage under 40%, each with a one-line note on why it is low and whether it is scheduled for work; and the floors from REQ-C-4. Update it on every release.
- **Acceptance:** The document exists; the low-coverage table is generated by a script (`scripts/report-coverage.py`) so it can be regenerated rather than hand-maintained.

### REQ-C-3 — Coverage delta gate on PRs
- **Source:** §7 T1 exit ("a CI gate set at (baseline − 2) so coverage cannot silently fall")
- **Requirement:** On pull requests, fail when workspace line coverage drops more than 2 percentage points below the current `main` baseline (`cargo llvm-cov report --fail-under-lines <baseline-2>`). The baseline value lives in a committed file (`.coverage-baseline`) updated by the `main` job, not hard-coded in the workflow.
- **Acceptance:** A test PR that deletes tests for a covered module fails the gate; a neutral PR passes; the baseline file is updated automatically after merge to `main` when coverage rises.
- **Constraint:** This is one of the two permitted per-PR additions (REQ-G-3). Measure its cost.

### REQ-C-4 — Coverage floors as targets
- **Source:** §7 T1 ("Target floors: airk-core and airk-kernel 75%, airk-store 70%, airk-retrieval 80%")
- **Requirement:** Record the floors in `docs/testing/coverage.md` as targets. When a crate meets its floor, raise that crate's gate to the floor (per-crate `--fail-under-lines`) so it cannot regress below it. Do not enforce floors that are not yet met.
- **Acceptance:** Each crate's status (below target / at target and gated) is visible in the coverage document and the CI configuration matches it.

### REQ-C-5 — Extension and Python coverage (P2)
- **Source:** §2.1 (541 extension tests, 296 Python tests exist but have no coverage number either)
- **Requirement:** Report line coverage for `packages/vscode` (c8/istanbul via the existing test runner) and `scripts/tests` (coverage.py) in the same document, without gates initially.
- **Acceptance:** Both numbers appear in `docs/testing/coverage.md` with provenance.

---

## R — Retrieval quality evaluation (M3, P0 — highest value)

### REQ-R-1 — Pinned, licence-clean evaluation corpus
- **Source:** §7 T2, §3.3
- **Requirement:** Define five public repositories, one each predominantly Rust, TypeScript, Python and Go, plus one mixed-documentation corpus (prose, Markdown, PDF/DOCX/XLSX where available), each pinned to a commit. Record them in `eval/corpus/manifest.json` with: repository URL, commit hash, licence (must be OSI-approved and permit redistribution), approximate file count and size, and a content digest. Provide `scripts/fetch-eval-corpus.py` that materialises the corpus from the manifest with digest verification.
- **Owner decision:** whether corpus files are vendored into the repository ("checked-in" per the audit) or fetched at evaluation time. Default proposal: fetch with digest verification, vendor only the mixed-docs corpus if it is small; the owner chooses, mindful of the 47,793 KB repository size.
- **Acceptance:** `fetch-eval-corpus.py` produces a byte-identical tree on two different machines (digest match); licences are listed in the project's third-party notices if any corpus content is vendored.

### REQ-R-2 — 300 blind, graded queries
- **Source:** §7 T2 ("300 queries with graded relevance labels, written blind to AIRK's behaviour"), §8 item 1 ("hold them blind until the first run")
- **Requirement:** Author at least 300 queries across the five corpora in `eval/queries/*.jsonl` using a documented schema (`eval/queries/SCHEMA.md`): `id`, `corpus`, `category`, `query`, `language` (of the query text), `relevant` (list of `{path, start_line, end_line, grade}` with grades 0–3), `author`, `authored_at`, `notes`. Categories and minimum counts: symbol lookup (≥ 60), conceptual search (≥ 60), cross-file relationships (≥ 60), document facts (≥ 60), multilingual text (≥ 30), reserve (remaining). Queries shall be written and labelled before any AIRK result is observed, and the authoring commit shall precede the first evaluation run in git history.
- **Acceptance:** Schema validator (`scripts/validate-eval-queries.py`) passes; category minimums met; the git history shows the query set committed before the first `docs/retrieval-quality.md` result; an inter-annotator spot check on ≥ 30 queries by a second person (or a second, independently prompted agent with no access to the first labels) is recorded with agreement rate.

### REQ-R-3 — Evaluation harness
- **Source:** §7 T2, §8 item 1
- **Requirement:** Implement `scripts/evaluate-retrieval.py` that, for each corpus: builds a fresh AIRK collection (via the CLI, so parity holds); runs every query in **lexical**, **vector** and **hybrid** modes at the product's default settings and at any additional settings the owner names; runs the **ripgrep baseline** (top-N files by match count, N = 10, with a documented tie-break); optionally ingests a **manual-baseline results file** (`eval/baselines/<tool>.jsonl`) produced by running one commercial agent search by hand on the same queries; and computes Recall@5, Recall@10, MRR, nDCG@10 (graded), p50/p95 query latency, cold-vs-warm latency, and peak RSS of the AIRK process. Output: `artifacts/retrieval-eval/<date>-<commit>.json` and a Markdown table with the REQ-G-5 header.
- **Acceptance:**
  - Metric implementations are unit-tested against hand-computed fixtures (`scripts/tests/test_evaluate_retrieval.py`).
  - Two consecutive runs on the same machine and commit produce identical IR metrics (latency may vary within a stated tolerance).
  - The existing `scripts/check-retrieval-quality.py` is retained as a smoke check and its docstring is updated to say it does not qualify relevance (it already says so for synthetic vectors — make the relationship to the new harness explicit).

### REQ-R-4 — Committed retrieval quality report
- **Source:** §7 T2 exit, §9 M3
- **Requirement:** Commit `docs/retrieval-quality.md` containing: method (corpus, query set, modes, baselines, metric definitions, hardware, commit); results per mode per corpus; a headline comparison sentence against ripgrep in the form the audit asks for ("X% better Recall@10 than ripgrep on 300 labelled queries"); known limitations; and a link to the JSON artifact. Regenerate the tables from the JSON by script.
- **Acceptance:** The document exists with real numbers; every number has provenance; the headline sentence is reproducible by re-running the harness on the pinned commit.

### REQ-R-5 — Nightly retrieval regression gate
- **Source:** §7 T2 exit ("wired into nightly CI with thresholds")
- **Requirement:** A nightly workflow runs REQ-R-3 on a fixed runner class, compares each metric against thresholds in `eval/thresholds.json` (initially set from the first published run minus a tolerance the owner approves, proposal: 2 points absolute on Recall@10 and nDCG@10), and fails with a diff table when any threshold is breached. Results attach to the nightly run and to each release record.
- **Acceptance:** A deliberate regression (e.g. disabling hybrid fusion on a branch) causes the nightly job to fail; a normal run passes and uploads the JSON artifact.

### REQ-R-6 — Additional baselines (P1)
- **Source:** §3.3 ("against ripgrep, against embeddings-only, and against one commercial agent search")
- **Requirement:** Add an embeddings-only baseline (brute-force exact cosine over the same chunks, no ANN) to measure ANN recall loss, and document the procedure for the manual commercial-agent baseline (which tool, prompt wording, how results were mapped to paths/ranges) in `eval/baselines/README.md`.
- **Acceptance:** Both baselines appear as columns in `docs/retrieval-quality.md`; the exact-cosine baseline is automated; the manual baseline has at least one recorded run.

---

## F — Property-based testing and fuzzing (M3/M4, P1)

### REQ-F-1 — Property test suites for the five highest-blast-radius modules
- **Source:** §7 T3, §8 item 3
- **Requirement:** Add `proptest` as a dev-dependency (no runtime change) to the relevant crates and write property suites with strategies derived from the real contract types, not invented ones:
  1. **airk-ingest chunkers and budget accounting.** Invariants: sum of chunk bytes ≤ budget; chunk offsets strictly monotonic and non-overlapping; concatenation of chunks reproduces the input with no byte lost or duplicated; arbitrary UTF-8 (including invalid boundaries, RTL and zero-width characters) never panics.
  2. **airk-kernel path/alias normalisation and traversal.** Invariants: a normalised path never escapes the collection root for any input (including `..`, symlink-like components, mixed separators, UNC/drive forms on Windows, NUL); never panics on arbitrary UTF-8; normalisation is idempotent.
  3. **airk-store SQLite row mapping and manifest parsing.** Invariants: malformed input yields a typed error, never a panic; a failed parse or write leaves no partial write observable (verify via re-open and integrity check).
  4. **DiskANN file provider.** Invariants: page arithmetic cannot overflow; reads never extend outside the file length for any (page index, page size, file length) triple; out-of-range requests return bounded errors.
  5. **PDF/DOCX/XLSX/XML extraction entry points.** Invariants: hostile bytes return bounded errors within a documented time and memory budget; no panic; no unbounded allocation.
- **Acceptance:** Each suite exists under the owning crate's `tests/` or inline test module, is tagged for the fast/full split (REQ-F-4), runs green with a fixed seed, and the invariants are stated verbatim in a module doc comment.

### REQ-F-2 — Fuzz targets
- **Source:** §7 T3 (`cargo fuzz init && cargo fuzz add manifest_parse`)
- **Requirement:** Add a `fuzz/` crate with `cargo-fuzz` targets at minimum for: `manifest_parse`, `path_normalise`, `chunk_text`, `diskann_page_read`, and one target per document extractor (`extract_pdf`, `extract_docx`, `extract_xlsx`, `extract_xml`). Check in a seed corpus per target under `fuzz/corpus/<target>/` and a regression corpus under `fuzz/regressions/<target>/` into which every crash-reproducing input is added together with a conventional regression test.
- **Acceptance:** `cargo fuzz build` succeeds on Linux; each target runs for at least 10 minutes in a nightly job without a crash; a documented 24-hour run per target has been completed with zero panics and its receipt recorded in `docs/testing/fuzzing.md`.

### REQ-F-3 — Seed policy
- **Source:** §7 T3 exit ("fixed seed plus a nightly randomised seed")
- **Requirement:** Routine CI runs proptest with a fixed seed and a bounded case count (proposal: 256 cases per property); the nightly job runs with a randomised seed and a larger count (proposal: 4,096), persisting any failing seed and minimal input as a regression file committed by a follow-up PR.
- **Acceptance:** Both configurations are selectable by environment variable or feature; the nightly job logs the seed used.

### REQ-F-4 — Fast subset on PRs
- **Source:** §7 CI cost control ("a fast subset of T3")
- **Requirement:** Only the fixed-seed, bounded-count property tests run on pull requests; fuzzing and randomised-seed runs are nightly only. Measure and record the added PR minutes.
- **Acceptance:** PR pipeline time increase from property tests is recorded in `docs/ci-and-builds.md` and is within the REQ-G-3 budget.

---

## P — Performance and scale gates (M4, P1)

### REQ-P-1 — Criterion benchmark suite
- **Source:** §7 T4, §2.2 "Benchmark harness: absent"
- **Requirement:** Add `benches/` (criterion) covering: ingest throughput per supported format (bytes/s and files/s); cold and warm query latency per mode (lexical, vector, hybrid); hybrid fusion overhead relative to its inputs; ANN recall@10 versus exact cosine search on a fixed embedded set; and process RSS sampled during each benchmark. Benchmarks must be runnable with `cargo bench --workspace` and emit JSON alongside criterion's HTML.
- **Acceptance:** All benchmarks run on a developer machine in under 20 minutes at default settings; results include REQ-G-5 provenance; the suite is documented in `docs/testing/benchmarks.md`.

### REQ-P-2 — Scale campaigns at 10k / 100k / 1M entries
- **Source:** §7 T4, §4 truth 4, §9 M4 ("real 100k-file profile")
- **Requirement:** Provide `scripts/scale-campaign.py` that synthesises or assembles corpora of approximately 10,000, 100,000 and 1,000,000 discovered entries with a documented, reproducible shape (file-size distribution, language mix, directory depth), ingests them into a fresh collection, and records: cold and warm ingest time; cold and warm query latency p50/p95 per mode; peak and steady-state RSS; physical disk read/write bytes (platform-specific counters, documented); index size on disk; and any admitted-space or quota refusals. Each size is a separate campaign and a separate table.
- **Acceptance:** The 100k campaign has at least one completed run with real numbers recorded (M4 requirement); the 10k campaign runs nightly (REQ-P-3); the 1M campaign is run at least once per release candidate or explicitly recorded as `not run` with reason.
- **Note:** Supersede or extend the existing 64-file `benchmark-code.py` rather than leaving two overlapping tools; document the relationship.

### REQ-P-3 — Nightly performance regression gate
- **Source:** §7 T4 ("Nightly regression gate with an explicit budget (for example 15% on p95), reporting hardware identity with every number")
- **Requirement:** A nightly job runs the criterion suite and the 10k campaign on a fixed runner class, compares p95 latencies, ingest throughput and RSS against `eval/perf-baseline.json`, and fails when any metric regresses beyond its budget (default 15% on p95 latency, 15% on RSS, −15% on throughput). The baseline is updated only by an explicit PR with justification.
- **Acceptance:** A synthetic regression (e.g. an added sleep in the query path on a branch) fails the gate; the job reports hardware identity in its summary.

### REQ-P-4 — Scale qualification document and roadmap update
- **Source:** §7 T4 exit
- **Requirement:** Create or extend `docs/scale-qualification.md` with one table per campaign size, each row carrying REQ-G-5 provenance. When the 100k campaign has a recorded run, update `docs/roadmap.md` to move "large repositories" from open to bounded-with-evidence, stating the bound (the largest size with recorded numbers) explicitly.
- **Acceptance:** The roadmap wording references the qualification document and the bound; no claim exceeds a recorded campaign.

---

## S — Fault, soak, privacy and security campaigns (M4, P1)

### REQ-S-1 — Publication-fault checks in routine CI
- **Source:** §7 T5, §4 truth 5
- **Requirement:** Run `scripts/check-publication-faults.py` with a reduced attempt count (proposal: 1 attempt per fault class) on the `main` post-merge job and nightly; run the full campaign (existing attempt count or higher) on every release candidate. Any failure in the full campaign is a release blocker.
- **Acceptance:** The reduced run adds no more than a bounded number of minutes (record it); the release-candidate workflow lists the full campaign as a required check; the release record links the campaign receipt.
- **Constraint:** Adding a required check to the release workflow touches a protected surface (REQ-G-1): prepare the change and obtain owner direction before merging.

### REQ-S-2 — Full-disk and quota-exhaustion-at-write cases
- **Source:** §7 T5, §6 Crash and power loss
- **Requirement:** Add fault cases that fill the storage volume (or a size-limited loopback/temporary filesystem) and exhaust the configured quota at the moment of a publication write, then assert: the previously published revision remains readable and integrity-checked; no partial revision is admitted; the process reports a typed refusal; recovery after space is freed succeeds without manual repair.
- **Acceptance:** Cases exist in `check-publication-faults.py` or a sibling script, run on at least Linux and Windows, and their results are recorded in `docs/fault-and-privacy-qualification.md` (REQ-S-7).

### REQ-S-3 — 72-hour soak with 16 concurrent clients
- **Source:** §7 T5, §6 Concurrency ("No sustained multi-client stress profile with assertions")
- **Requirement:** Provide `scripts/soak.py` that drives 16 concurrent MCP/CLI clients against one service with a mixed workload (queries across modes, incremental re-ingest of mutating files, collection listing) for a configurable duration (default 72 hours; a 1-hour smoke profile for nightly), asserting throughout: p95 latency stays within a stated bound; no leaked child processes at end (owned-process containment); RSS does not grow monotonically beyond a stated bound; no admitted revision becomes unreadable. Report a timeline of latency, RSS and process counts.
- **Acceptance:** A completed 72-hour run has a receipt in `docs/fault-and-privacy-qualification.md`; the 1-hour profile runs nightly and fails on any assertion breach.

### REQ-S-4 — Adversarial corpus
- **Source:** §7 T5, §6 Adversarial inputs
- **Requirement:** Build `eval/adversarial/` (generated by `scripts/build-adversarial-corpus.py`, not vendored where files are large) containing at minimum: zip bombs (nested and flat); 4 GiB sparse files; deep symlink chains and symlinks escaping the collection root; RTL, zero-width and mixed-script text; single lines of tens of MB; files that mutate during ingest (driven by the script); files shaped like credentials and secrets (fake values). For each class, define the expected behaviour (bounded error, skip with receipt, or successful bounded ingest) consistent with existing documented limits, and assert it. For credentials-shaped files, document the current policy and do not change it without owner direction.
- **Acceptance:** Ingest of the full adversarial corpus completes with zero panics, zero hangs beyond the documented timeout, and zero escapes of the collection root; results recorded in REQ-S-7's document.

### REQ-S-5 — Execute the three "not run" privacy and lifecycle checks
- **Source:** §3.4, §4 truth 6, §7 T5, §8 item 5, §9 M4
- **Gap:** `docs/validation.md` records three checks as not run: (1) Unix custom-storage privacy under a second OS account; (2) the Windows Assistance descriptor under a second OS account; (3) installed-editor lifecycle (install, PATH check, uninstall, residue) with failure injection.
- **Requirement:** Provide scripts under `scripts/privacy/` that execute each check end-to-end on disposable accounts and capture evidence (command transcript, exit codes, permission bits/ACLs, file listings before and after), producing a receipt file per check. Where CI runners permit creating a second local account, run them in a nightly or release-candidate job; where they do not, document the manual procedure precisely enough that the owner can run it once and paste the receipt.
- **Acceptance:** Each of the three checks has a dated receipt — pass or fail — recorded in `docs/validation.md`, replacing the `not run` entries. Any failure is converted into a tracking issue and a release blocker.

### REQ-S-6 — Automated upgrade/rollback matrix
- **Source:** §7 T5, §6 Upgrade and rollback
- **Requirement:** Provide `scripts/upgrade-matrix.py` that, on Windows and one POSIX host, installs 1.7.6, creates and queries a collection, upgrades to 1.8.7 (and subsequently to each new release), verifies the collection remains readable and query results are consistent, then rolls back to 1.7.6 and verifies either a clean refusal with a typed error or continued operation — whichever the project's documented policy specifies. Extend the matrix with each release.
- **Acceptance:** The matrix runs on release candidates; results recorded in REQ-S-7's document; the documented upgrade path in `docs/` matches observed behaviour.

### REQ-S-7 — Fault and privacy qualification record
- **Source:** §7 T5 exit
- **Requirement:** Create `docs/fault-and-privacy-qualification.md` as the single record for REQ-S-1 through REQ-S-6: one section per campaign, each with REQ-G-5 provenance, the result (pass / fail / not run with reason), and links to receipts. State explicitly that any failure is a release blocker.
- **Acceptance:** The document exists, is linked from `docs/validation.md` and the release checklist, and every campaign section has at least one dated entry by M4.

### REQ-S-8 — Diagnose the macOS Intel cold-client failure
- **Source:** §3.4 ("the Intel cold-client failure, undiagnosed since the 1.8.7 preparation notes"), §6 Cross-platform proof
- **Requirement:** Reproduce the macos-15-intel cold-client failure recorded in the 1.8.7 preparation notes; capture logs and timing; identify root cause or, if not reproducible within a bounded effort (proposal: 2 days), record the attempts, the hypothesis list and the detection added. Add a regression test or CI assertion targeting the identified cause.
- **Acceptance:** `docs/releases/` or `docs/validation.md` contains either a root-cause entry with a linked fix/regression test, or a bounded "unqualified" entry with the investigation record.

### REQ-S-9 — Real macOS signing status (owner decision)
- **Source:** §6 Cross-platform proof ("real Mac signing deferred")
- **Requirement:** Document the current signing/notarization status per platform in one place and the criteria for lifting the deferral. Agents do not touch signing workflows (REQ-G-1).
- **Acceptance:** A dated status entry exists; the owner has recorded a decision or a date.

---

## M — Mutation testing (post-M4, P2)

### REQ-M-1 — Mutation run on airk-core and airk-retrieval
- **Source:** §7 T6, §6 Coverage and test quality
- **Requirement:** Run `cargo mutants --package airk-core --package airk-retrieval --timeout 300` on a nightly or release-candidate schedule. Publish the surviving-mutant list in `docs/testing/mutation.md`; triage each survivor as (a) assertion to strengthen, (b) equivalent mutant, or (c) accepted gap, and open issues for (a).
- **Acceptance:** First run completed and published; at least the top 20 survivors by blast radius triaged; mutation score recorded with provenance.

---

## CI — Integration and cost control (ongoing)

### REQ-CI-1 — Nightly workflow
- **Source:** §7 CI cost control
- **Requirement:** Add `.github/workflows/nightly.yml` (additive; does not modify `ci.yml` jobs) that runs: full bootstrap on all four OS targets (REQ-A-1); coverage on `main` (REQ-C-1); retrieval evaluation (REQ-R-5); randomised-seed property tests and 10-minute fuzz runs (REQ-F-2, F-3); criterion and 10k scale campaign (REQ-P-3); reduced publication-fault run and 1-hour soak (REQ-S-1, S-3); privacy checks where runners permit (REQ-S-5). Each job uploads its JSON artifact. All actions digest-pinned (REQ-G-4).
- **Acceptance:** The workflow runs green on a scheduled trigger; its total billable minutes are recorded in `docs/ci-and-builds.md`.

### REQ-CI-2 — Release-candidate workflow hooks
- **Source:** §7 CI cost control ("on release candidates, with results attached to the release record")
- **Requirement:** Define the release-candidate campaign set (full publication-fault campaign, 100k scale campaign, upgrade matrix, mutation run if adopted, full fuzz run) as a separately triggered workflow and specify how its artifacts attach to the release record in `docs/releases/release-<version>.md`. Prepare the change; do not modify promotion or signed-release workflows without owner direction (REQ-G-1).
- **Acceptance:** The workflow exists and runs on manual dispatch; the release checklist (REQ-L-1) references it; the owner has approved any wiring into existing release gating.

### REQ-CI-3 — CI cost ledger
- **Source:** §3.2 (CI minutes are the cost base), §7 CI cost control
- **Requirement:** Maintain a table in `docs/ci-and-builds.md` of per-PR, per-merge, nightly and release-candidate pipeline minutes, updated whenever a workflow changes, with the date and the change that caused it.
- **Acceptance:** Every CI-touching PR updates the ledger.

---

## X — Risk-path code review (M4, P1)

### REQ-X-1 — Review airk-store publication and quota paths
- **Source:** §8 item 6, §6 Unit and contract tests ("Untested branches in airk-store (88,768 lines)")
- **Requirement:** Perform a structured code review of the publication, admission-space and quota-handling paths in `airk-store` using the coverage data from REQ-C-2 to target untested branches. Deliver `docs/testing/review-airk-store-publication.md` listing: each finding with severity, the untested branch it concerns, and the proposed test or fix. Map undocumented coupling between `airk-store` and its dependants before proposing any refactor. No behaviour changes in the review PR itself.
- **Acceptance:** Findings document exists; every High-severity finding has an issue; tests proposed for untested branches are implemented in follow-up PRs and coverage for the reviewed modules rises measurably.

### REQ-X-2 — Expand installed-editor lifecycle e2e coverage
- **Source:** §8 item 6 (`bmad-qa-generate-e2e-tests`), §6 End-to-end
- **Requirement:** Extend the qualification-gated installed-extension lifecycle suite with generated end-to-end scenarios covering install, first index, first cited retrieval, PATH check, upgrade, uninstall and residue check. Keep the suite qualification/nightly-gated (REQ-G-3).
- **Acceptance:** New scenarios run green in the nightly workflow; scenario list documented.

---

## D — Documentation, positioning and Marketplace (M3, P0 after REQ-R-4)

### REQ-D-1 — README first 200 words
- **Source:** §1, §4 truth 1, §8 item 7, §9 M3
- **Requirement:** Rewrite the first 200 words of `README.md` around one ICP and one measured outcome. The measured outcome is the headline sentence from `docs/retrieval-quality.md` (REQ-R-4), linked to the method. The ICP is the owner's decision (REQ-O-1); until decided, draft two variants: (a) coding-agent users who want cited, local retrieval; (b) local-only / regulated / air-gapped engineering teams. The rewrite shall also answer, in one line each, the four buyer questions from §1: compared to what; how much better, measured how; who it is for; what it costs (or that it is free and why).
- **Acceptance:** Word count of the opening section ≤ 200 before the first heading after the title; every quantitative claim links to a provenance-bearing document; the owner has approved the ICP wording.
- **Dependency:** REQ-R-4.

### REQ-D-2 — Benchmark page
- **Source:** §8 item 7 ("add a benchmark page with the method")
- **Requirement:** `docs/retrieval-quality.md` (REQ-R-4) and `docs/scale-qualification.md` (REQ-P-4) are linked from the README as "Benchmarks", with a short index page `docs/benchmarks.md` that states, in order: what was measured, against what, on what corpus, on what hardware, and when.
- **Acceptance:** The index page exists and links resolve.

### REQ-D-3 — Marketplace description
- **Source:** §8 item 7 ("cut the Marketplace description to a single promise plus one number")
- **Requirement:** Reduce the `description` and the opening of the Marketplace README in `packages/vscode` to a single promise plus one number from REQ-R-4, with the rest of the listing deferring to the benchmark page.
- **Acceptance:** Draft prepared as a PR; the owner approves before publishing (publishing is a release action under REQ-G-1).
- **Dependency:** REQ-R-4.

### REQ-D-4 — "Compared to what" section
- **Source:** §1 question 1
- **Requirement:** Add a short, evidence-only comparison section (README or `docs/benchmarks.md`) covering ripgrep, the agent's own file reading / native agent search, and embeddings-only, using only measured numbers from REQ-R. Named commercial tools appear only where a recorded manual baseline run exists (REQ-R-6); otherwise state that no comparison has been measured.
- **Acceptance:** No unmeasured comparative claim appears anywhere in README, Marketplace text or `docs/`.

### REQ-D-5 — Documentation entry path
- **Source:** §4 truth 1 (216 documents, low signal), §3.5 (operational knowledge in prose)
- **Requirement:** Add a `docs/README.md` (or extend the existing index if one exists — REQ-G-2) that routes three reader types in at most three clicks each: a new user (install → first cited result → troubleshooting), a contributor (bootstrap → architecture → parity rule), and a maintainer (release runbook → validation → qualification records). Do not delete or merge existing documents under this requirement.
- **Acceptance:** Each of the three paths is walked by a reviewer and reaches its destination in ≤ 3 links.

### REQ-D-6 — Usage-evaluation guidance update
- **Source:** §1 question 2 (`docs/usage-evaluation.md` refuses to claim savings)
- **Requirement:** Once REQ-R-4 numbers exist, update `docs/usage-evaluation.md` to reference them as the project's measured baseline while retaining the self-measurement guidance.
- **Acceptance:** The document links to `docs/retrieval-quality.md` and its claims are consistent with it.

---

## L — Release process and bus factor (ongoing, P1)

### REQ-L-1 — Release runbook from documentation alone
- **Source:** §3.5 ("a second maintainer who ships one release end-to-end from the documentation alone"), §4 truth 8
- **Requirement:** Produce `docs/release-runbook.md`: an ordered checklist that a second maintainer can follow to cut a release using only the repository — version agreement, change classification, four-OS matrix expectations, signing and notarization prerequisites (as documentation; agents do not touch the workflows), release-candidate campaigns (REQ-CI-2), release notes template, promotion, Marketplace publish, and post-release verification. Include a `--dry-run` section describing which steps can be rehearsed without signing or publishing.
- **Acceptance:** A second person (or an agent with no prior context, under the owner's supervision) completes a dry-run release from the runbook; every point of friction is converted into a documentation fix in the same PR series; the time taken is recorded.

### REQ-L-2 — Release cadence policy (owner decision)
- **Source:** §4 truth 2, §8 item 8
- **Requirement:** Record a release cadence policy in `docs/release-runbook.md`: proposal is one release every two weeks with a named theme, with patch releases reserved for security or data-integrity fixes. Agents draft; the owner decides.
- **Acceptance:** Policy recorded with the owner's decision and effective date.

### REQ-L-3 — Roadmap and validation records track evidence
- **Source:** §5 Intellectual honesty, §7 exits
- **Requirement:** As each campaign in R, P and S produces a recorded run, update `docs/roadmap.md` and `docs/validation.md` in the same PR to reflect the new status using the project's vocabulary. Claims never run ahead of receipts.
- **Acceptance:** Reviewer confirms every status change links to a receipt.

### REQ-L-4 — External-contribution readiness (P2)
- **Source:** §4 truth 8 (0 forks, no external contributors)
- **Requirement:** Label at least 10 (c)-category ignore items (REQ-B-1), low-coverage modules (REQ-C-2) or proptest targets (REQ-F-1) as `good first issue` with the bootstrap command and acceptance criteria in each issue body.
- **Acceptance:** Issues exist and are discoverable from `CONTRIBUTING.md`.

---

## T — Activation measurement (owner decision required before any implementation)

### REQ-T-1 — Local-only, opt-in activation receipt
- **Source:** §3.2 ("a measured activation metric such as 'first cited retrieval within 10 minutes of install'"), §4 truth 9 ("no telemetry, no funnel")
- **Constraint:** REQ-G-6 forbids egress. This requirement is therefore scoped to a **local** record only.
- **Requirement (if approved):** Record locally, in the user's own AIRK storage, timestamps of install, first successful index publication and first cited retrieval, and expose them via CLI/Workbench/MCP (parity) as a user-viewable "activation" receipt. Provide an explicit, user-initiated export command that writes a JSON file the user may choose to share with a design partner programme. No automatic transmission of any kind. Document the data stored in the existing privacy documentation.
- **Acceptance:** Owner approval recorded; privacy documentation updated before the feature ships; the three timestamps are visible through all three interfaces; no network code is added.

---

## O — Owner-only actions (not agent work; listed for completeness and sequencing)

| ID | Action | Source | Why it is not agent work |
|---|---|---|---|
| REQ-O-1 | Choose the ICP (horizontal "local search for coding agents" vs local-only/regulated/air-gapped teams) | §3.1, §9 kill criteria | Positioning decision; agents draft both README variants (REQ-D-1) |
| REQ-O-2 | Decide on pricing / paid tier mapped to a cost driver (org policy, shared index servers, support) | §3.2 | Commercial decision; blocks any pricing text in REQ-D-1 |
| REQ-O-3 | Recruit 10 design partners, reach 5 weekly-active, write one case study | §9 M5 | Human outreach; agents can draft the case-study template |
| REQ-O-4 | Appoint and onboard a second maintainer; have them execute REQ-L-1 | §3.5 | Staffing |
| REQ-O-5 | Run the manual commercial-agent baseline (REQ-R-6) and the privacy checks that CI runners cannot host (REQ-S-5) | §7 T2, T5 | Requires accounts and tools agents may not hold |
| REQ-O-6 | Approve all protected-surface changes under REQ-G-1 | §8 | Authority |
| REQ-O-7 | Decide the kill criteria outcome at day 90 | §9 | Strategy |

---

## Traceability — audit finding to requirement

| Audit item | Requirement(s) |
|---|---|
| §1 Q1 Compared to what | REQ-R-3, R-6, D-4 |
| §1 Q2 How much better, measured how | REQ-R-2, R-3, R-4, D-6 |
| §1 Q3 Who is this for | REQ-O-1, D-1 |
| §1 Q4 What does it cost | REQ-O-2, D-1 |
| §2.2 Coverage absent | REQ-C-1, C-2, C-3, C-4, C-5 |
| §2.2 Property-based absent | REQ-F-1, F-3, F-4 |
| §2.2 Fuzzing absent | REQ-F-2 |
| §2.2 Benchmark harness absent | REQ-P-1 |
| §2.2 Mutation testing absent | REQ-M-1 |
| §2.2 Real-corpus relevance evaluation absent | REQ-R-1 … R-6 |
| §2.3 Qualification-gated checks not routine | REQ-S-1, CI-1, CI-2 |
| §3.2 No activation metric / telemetry | REQ-T-1 (owner decision) |
| §3.3 No baseline on 300 real queries | REQ-R-2, R-3, R-4 |
| §3.4 Three privacy checks not run | REQ-S-5 |
| §3.4 Intel cold-client failure undiagnosed | REQ-S-8 |
| §3.4 No power-loss or full-disk campaign | REQ-S-1, S-2 |
| §3.5 Bus factor one; single-owner release | REQ-A-1, A-2, L-1, L-4, O-4 |
| §4.1 216 docs / 14 installs | REQ-D-1, D-3, D-5 |
| §4.2 23 releases in 20 days | REQ-L-2 |
| §4.3 Quality claim unmeasured | REQ-R-* |
| §4.4 Scale claim unmeasured | REQ-P-* |
| §4.5 Durability unmeasured in routine CI | REQ-S-1, S-2 |
| §4.6 Privacy unverified by second account | REQ-S-5 |
| §4.7 No baseline | REQ-R-3, R-6, D-4 |
| §4.8 Bus factor 1 | REQ-L-1, L-4, O-4 |
| §4.9 No pricing, telemetry, funnel | REQ-O-2, T-1, O-3 |
| §4.10 Build not reproducible bare | REQ-A-1, A-2, A-3 |
| §6 Native integration — 37 ignores | REQ-B-1, B-2 |
| §6 End-to-end not run per PR | REQ-X-2, CI-1 |
| §6 Concurrency — no sustained stress profile | REQ-S-3 |
| §6 Adversarial inputs | REQ-F-1, F-2, S-4 |
| §6 Upgrade and rollback | REQ-S-6 |
| §6 Cross-platform — Intel flakiness, Mac signing | REQ-S-8, S-9 |
| §7 T0 | REQ-A-1, A-2, A-3, B-1, B-2 |
| §7 T1 | REQ-C-1 … C-4 |
| §7 T2 | REQ-R-1 … R-5 |
| §7 T3 | REQ-F-1 … F-4 |
| §7 T4 | REQ-P-1 … P-4 |
| §7 T5 | REQ-S-1 … S-7 |
| §7 T6 | REQ-M-1 |
| §7 CI cost control | REQ-G-3, CI-1, CI-2, CI-3 |
| §8 item 1 | REQ-R-1, R-2, R-3 |
| §8 item 2 | REQ-C-1, C-2, C-3 |
| §8 item 3 | REQ-F-1 |
| §8 item 4 | REQ-A-1 |
| §8 item 5 | REQ-S-5 |
| §8 item 6 | REQ-X-1, X-2 |
| §8 item 7 | REQ-D-1, D-2, D-3 |
| §8 item 8 | REQ-L-2 |
| §8 "should not do" | REQ-G-1 |
| §9 M1 | REQ-A-1, A-2, A-3, B-1, B-2 |
| §9 M2 | REQ-C-1, C-2, C-3 |
| §9 M3 | REQ-R-4, R-5, D-1 |
| §9 M4 | REQ-S-5, S-7, P-2, P-4 |
| §9 M5 | REQ-O-3, T-1 |
| §11 Limitations | REQ-G-2, A-3 |

---

## Suggested execution order and dependencies

```
Week 1–2  (M1):  G-* → A-1 → A-2, A-3 → B-1 → B-2 → CI-3 (ledger started)
Week 2–3  (M2):  C-1 → C-2 → C-3 → C-4   (C-5 optional)        | R-1, R-2 begin (queries written blind, committed before any run)
Week 3–6  (M3):  R-3 → R-4 → R-5 → R-6   | D-1, D-2, D-3, D-4, D-6 (after R-4) | F-1, F-3, F-4 in parallel | CI-1 assembled as jobs land
Week 6–9  (M4):  S-5 (start early — may need owner-run) | S-1, S-2 | P-1 → P-2 (100k) → P-3 → P-4 | S-3, S-4, S-6, S-8 | S-7 record | X-1 → X-2 | F-2 | CI-2 | L-1, L-3
Week 9–12 (M5):  M-1 (optional) | L-2, L-4 | D-5 | T-1 only if owner approves | O-* owner actions throughout
```

Hard dependencies:
- Everything depends on REQ-A-1 (nothing can be verified until the build reproduces).
- REQ-R-2 (queries) must be committed before REQ-R-3 is first run.
- REQ-D-1, D-3, D-4, D-6 depend on REQ-R-4 (no numbers, no claims).
- REQ-P-4 roadmap wording depends on a recorded REQ-P-2 run.
- REQ-S-1 release gating, REQ-CI-2 wiring, REQ-D-3 publishing and anything under REQ-G-1 wait on owner direction.

---

## Definition of done for the programme

The audit's 90-day ask is satisfied when all of the following are true and each is backed by a dated, provenance-bearing record in `docs/`:

1. A second person reaches a green test run from a clean machine using one command and the repository alone (REQ-A-1, A-2).
2. Workspace coverage is a published number with a CI gate that prevents silent regression (REQ-C-2, C-3).
3. `docs/retrieval-quality.md` reports Recall@5/10, MRR, nDCG@10, latency and RSS for lexical, vector and hybrid modes against ripgrep (and at least one other baseline) on 300 blind, graded queries across five pinned corpora, and the README's headline claim is drawn from it (REQ-R-4, D-1).
4. The three "not run" privacy checks have pass/fail receipts, and at least one 100k-entry scale table has hardware, date and RSS (REQ-S-5, P-2).
5. Fault, soak, adversarial and upgrade campaigns have a single qualification record with explicit failures treated as release blockers (REQ-S-7).
6. No comparative or scale claim anywhere in README, Marketplace or `docs/` exceeds a recorded measurement (REQ-D-4, L-3).