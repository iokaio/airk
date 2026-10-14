# AIRK 0.1.0 release review and test checklist

Use this checklist for one exact commit and its four platform packages. An unchecked item is unverified, not implicitly passed. Record evidence and an authorized human reviewer; do not add an agent sign-off. A successful build alone does not qualify installation, language understanding, retrieval quality or performance. Release scope includes the roadmap features requested for this review; unresolved implementation/qualification gaps must remain release blockers or receive an explicit human scope decision.

## Candidate record

| Field | Record before approval |
|---|---|
| Commit / PR / CI run | Full SHA and links; all changes reviewed through the protected `main` branch |
| Version / channel | Native and extension `0.1.0`; explicitly choose preview or stable |
| Package identity | `iokaio.airk`, intended display name and engine minimum |
| Native/VSIX manifests | SHA-256 of each manifest, archive and embedded helper; clean checkout |
| Review date / reviewers | Human names, findings and evidence links |
| Installation environment | OS build, CPU architecture, editor/agent versions and remote/local host |
| Decision | Ready / blocked, with explicit exceptions and owner; never infer approval from this document |

## 1. Repository and change controls

- [ ] Read every code/configuration change since the last reviewed candidate, including generated lock changes and dependency licenses.
- [ ] Confirm the live [main ruleset](https://github.com/iokaio/airk/rules/25054101) matches [.github/main-ruleset.json](../.github/main-ruleset.json): required PR, no bypass, no force push/deletion, linear history, resolved conversations and current `Required checks` / `signed-off` checks from GitHub Actions.
- [ ] Verify a test branch can create a PR and merge only after those checks pass. Do not test protection by pushing an actual release change directly to `main`.
- [ ] Confirm contributor DCO trailers belong to the real authorized contributors. No invented signatures or approvals.
- [ ] Check `AGENTS.md` and `CLAUDE.md` byte equality, dependency boundaries, source SPDX headers and notices.
- [ ] Inspect native/VSIX inventories for private corpora, credentials, `temp/`, `.airk/`, build trees, test files and unexpected runtime modules. Test injected payload/traversal/link rejection.

## 2. Required automated validation

Run from the release checkout with the pinned toolchain and locked dependencies. Record command, exit status, platform and log/artifact link.

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
pwsh -NoProfile -File scripts/check-repo.ps1
python scripts/license-notices.py --output target/notices
python -m unittest discover -s scripts/tests -v
npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode
npm test --prefix packages/vscode
git diff --check
```

- [ ] Run extension tests again with `AIRK_TEST_BINARY` set to the candidate helper; a skipped native-adapter test does not count as a pass.
- [ ] Inspect all four matrix jobs, the Linux container test stage and the one-container persistence/isolation smoke.
- [ ] Run the code benchmark on original fixtures and retain measurements. Record cold/warm latency, indexing duration, RSS, disk growth and environment; distinguish a smoke from large-repository qualification.
- [ ] Run `python scripts/check-document-tables.py --binary <candidate-helper>` against the exact native candidate; verify DOCX/XLSX facts, coordinates, hashes, budgets and failure/reindex behavior.
- [ ] Run `python scripts/check-storage-lifecycle.py --binary <candidate-helper>` for aliases, pinned origins, quota rollback, relocation and source-preserving pruning/removal.
- [ ] Run `python scripts/check-usage-inputs.py --binary <candidate-helper>` for optional input validation, replay, calibration, price scenarios and disabled recording.
- [ ] Run installed-editor tests against the **packaged** helper and extension, including clean profile and upgrade.
- [ ] Validate YAML/action references with actionlint and inspect workflow permissions, triggers and artifact identity checks.

## 3. Host and installation matrix

Fill every cell with a result and evidence link; `pending` is the initial state. Runner labels are not minimum supported OS claims.

| Host | Native tests/package | Clean VSIX install/start | Upgrade/restart | Offline runtime | Editor/agent end-to-end |
|---|---|---|---|---|---|
| Windows x64 | pending | pending | pending | pending | pending |
| Linux x64 | pending | pending | pending | pending | pending |
| macOS Apple Silicon | pending | pending | pending | pending | pending |
| macOS Intel | pending | pending | pending | pending | pending |

- [ ] Test paths with spaces and Unicode, read-only sources, non-ASCII filenames, case differences and long paths.
- [ ] Check helper platform/version/digest rejection, stable per-user helper paths after extension replacement and new agent configuration after upgrade.
- [ ] Test WSL, SSH and dev-container hosts where advertised. Verify the helper and data directory reside on the workspace host. Browser-only use must fail clearly.
- [ ] Record minimum supported OS/editor versions based on real tests. Confirm the manifest's editor engine is available on every tested host.
- [ ] Review Windows signing, macOS signing/notarization and Linux executable permissions; label unsigned artifacts honestly.

## 4. Discovery, extraction and citations

- [ ] Verify ignore rules, `.airkignore`, Git exclusions, generated/secret paths, selected data-directory exclusion, symlinks/junctions and workspace escapes.
- [ ] Exercise arbitrary UTF-8 fallback, unsupported binary data, empty files, per-file/corpus/file-count limits and unreadable paths. Excluded binary data must not consume eligible indexing capacity.
- [ ] Test representative definitions, imports, signatures, written types, scopes and structural framework cues for every advertised language. Partial syntax remains labeled.
- [ ] Check Markdown/text headings, dates, requirement IDs, units, bounds, ambiguous dates and qualitative/design/decision candidates against source text.
- [ ] Verify PDF page, DOCX paragraph/table and PPTX presentation-order slide locations, including empty paragraphs and multiline CSV headers.
- [ ] Check DOCX explicit/candidate headers, empty cells, nested tables, horizontal/vertical merges, and row requirements with IDs/units/dates. Verify cell boundaries cannot join separate values into fabricated constraints.
- [ ] Check XLSX workbook/sheet order, original row/cell addresses, shared/inline strings, hidden/sparse rows, merged ranges and requirements spread across columns. Test malformed/out-of-range references and shared-string/header amplification limits.
- [ ] Verify XLSX stored formulas never execute or read external files; missing/stale caches stay labeled. Test both Excel date epochs, the fictitious 1900 leap day and raw custom/ambiguous number formats.
- [ ] Verify corrupt/oversized archives, missing parts, external relationships, XML declarations/entities and extraction limits fail with bounded diagnostics.
- [ ] Confirm no OCR claim for image-only documents without an implemented/qualified OCR adapter.
- [ ] Validate SCIP producer/root/source/build binding, digest/coordinate validation, changed build-input invalidation and native-versus-imported provenance.

## 5. Retrieval, storage and DiskANN

- [ ] Test collection/root separation through every CLI/MCP route, exact facts, relationships, path prefixes, UTF-8 truncation, result counts and response budgets.
- [ ] Change live files after indexing and prove returned text/hash/location remain from one indexed revision. Verify later indexing updates all components together.
- [ ] Exercise retained JSON migration, SQLite transactions/rollback, corrupt catalogs, disk-full writes and generation overflow without losing the last good revision.
- [ ] Reopen the persistent lexical index in a fresh process and verify parity with known lexical fixtures. No query should silently load unrelated collection data.
- [ ] Verify DiskANN uses actual disk graph/vector reads, bounded caches and reader leases. Record provider/version, graph parameters, distance metric and host results.
- [ ] Provision a deliberately licensed, digest-verified local embedding model; verify dimensions, normalization and model identity. Missing/mismatched assets must report not-ready without remote fallback.
- [ ] Measure ANN recall against an exact oracle, using more than trivial nearest-self queries; include cold reopen, changed/deleted documents and corrupted model/vector/graph data.
- [ ] Test generation publication interruption and concurrent old/new readers; remove only generations without active reader leases. Document any retained bootstrap caps or full-snapshot memory dependence.
- [ ] Verify compact inventories and immutable raw-source/extraction objects: reuse unchanged bytes, fetch individual artifacts, reject tampered inventory digests and retain blobs referenced by leased generations.
- [ ] Add, remove and rebind a source alias; before publication retrieval must retain the old alias binding and coverage must show pending configuration. Check overlapping roots and alias/path collisions are rejected.
- [ ] Exhaust root and collection quotas during source retention, lexical/vector builds, model copies, SQLite publication and accounting. Preserve the last published revision, release abandoned reservations, and keep read-only scheduler status available at capacity.
- [ ] Preview and perform relocation with all clients stopped; verify hashes, collection/source identities, source preservation, rejection at the old root and recovery after interrupted copy/activation. Record filesystem free-space requirements separately from logical-byte quotas.
- [ ] Qualify a representative large repository with measured RSS, disk I/O and latency before claiming scalable operation or relaxing safety caps.

## 6. Scheduling, coverage and maintenance

- [ ] Run 100 rapid edits, edits during indexing, repeated Index Now clicks, external agent edits, save/idle/debounced/manual modes and minimum-delay controls.
- [ ] Test concurrent editor windows/CLI workers sharing a data root, durable pending revisions, restart/crash recovery, pause/resume and failure without busy retry loops.
- [ ] Compare coverage with native eligibility for ignored/binary/oversized/unreadable/capacity-blocked files, extraction failures, additions/deletions/renames and unsupported formats.
- [ ] Check stale/unknown/partial inventory, changes during scanning, unsaved buffers and scans after editor closure. Zero eligible files and near-complete rounding must not show false 100%.
- [ ] Test active-folder selection and multi-root totals; clicking coverage queues indexing through the scheduler. Status refresh must not restart MCP connections.
- [ ] Verify index storage changes, soft/hard budget labels, collection inspection, source-preserving removal and refusal while registered writers/readers are active.
- [ ] Test retention/pruning, abandoned generations, safe storage migration if offered, and explicit behavior where migration is not implemented.

## 7. Usage reports and user value

- [ ] Verify one outer CLI/MCP observation per request, with setup/nested calls and replay IDs deduplicated; generation/root/window scope must match.
- [ ] Inspect persisted files for query strings, source paths/text, prompts, credentials, raw errors or arbitrary client labels. Default storage is content-free aggregates.
- [ ] Test disabling, retention/caps, multiple workers, flush failures/full disk, clearing/deletion and late writers. Accounting failure must not fail retrieval or recreate a removed collection.
- [ ] Check missing/partial windows, unknown drops, wide totals, signed negative reductions and export encoding for JSON/CSV/Markdown.
- [ ] Compare normalized same-revision full returned sources with serialized responses, including metadata/framing. Empty/error/index/report calls receive no fabricated savings credit.
- [ ] Keep byte-based token estimates, optional user-supplied monetary scenarios and actual provider measurements visibly distinct. No unsupported billing/task-success claim.
- [ ] Validate client/session and paired-task imports, scope/replay bounds, explicit provenance and missing cache splits. Verify local byte-BPE calibration stores counts and tokenizer identity without retaining sample text; use held-out samples before any accuracy claim.

## 8. Agent and extension experience

For each client, record exact version, configuration scope, startup/tool-list/status, one cited retrieval and an indexing refresh. A generated config alone is insufficient.

| Client | Config preserves other servers/policy | Discovery and approval | Evidence request and refresh |
|---|---|---|---|
| VS Code / Copilot Chat | pending | pending | pending |
| Claude Code | pending | pending | pending |
| Codex CLI / IDE / desktop | pending | pending | pending |
| Gemini CLI | pending | pending | pending |
| Other advertised clients (list/version each) | pending | pending | pending |

- [ ] Confirm workspace trust prevents native execution and MCP registration until granted; revoked/changed scopes cannot expose another folder's collection.
- [ ] Verify pause, coverage, usage, collection controls, storage selection and guided setup in light/dark themes and with keyboard navigation.
- [ ] Review the [extension content specification](extension-content.md), actual listing, alt text and original illustrations against implemented behavior. Add genuine sanitized screenshots from public fixtures.
- [ ] Verify client-specific routes against current official documentation. VS Code provider registration must not be described as configuring all agents.
- [ ] Explain derived-state privacy and that the selected agent can send returned evidence to its provider.

## 9. Manual release workflow rehearsal and publication

- [ ] Merge the reviewed workflow changes through a PR. Run **Actions → Extension release → Run workflow**, choose the exact checked-in version/channel and leave **publish** false. Inspect all four verified native/VSIX artifacts and receipts.
- [ ] Verify deliberate wrong version/platform/channel/digest/incomplete-set inputs fail before publication. Build-only and PR jobs must have no publishing identity.
- [ ] Configure the `iokaio` Marketplace publisher's trusted-publishing policy for `iokaio/airk` and `.github/workflows/extension-release.yml`, including the `vscode-marketplace` environment/appropriate claims. The workflow requests a short-lived OIDC credential; no silent PAT fallback.
- [ ] Verify the `vscode-marketplace` GitHub environment permits only `main`. A publisher account/trust policy is an external prerequisite; workflow code cannot prove it exists.
- [ ] Review pinned dependency notices and a complete SBOM/license record, including native parsers, storage/vector engine, embedding assets and distributed binaries.
- [ ] Human approves the exact release candidate, public description, version/channel and known limitations.
- [ ] Authorized maintainer dispatches with **publish** true. Verify every platform's Marketplace entry and install the published package from a clean profile.
- [ ] Retain logs/checksums and record partial-publish recovery. Marketplace publication of several platforms is not atomic; do not blindly overwrite/retry a version that may already exist.
- [ ] Create release notes and recovery instructions for the exact artifacts. Announce or distribute only with authorization for that action.

## Release blockers and evidence log

| Finding or unavailable check | Impact | Evidence | Owner / resolution |
|---|---|---|---|
| Complete host/client matrix and physical power-loss/oldest-OS testing | Cannot claim cross-platform release qualification from one workstation | Attach CI/editor/client results | Human review pending |
| Marketplace publisher trust and signing/notarization credentials | Publication/signature readiness is separate from build success | Record account setup without secrets | Publisher owner |
| Any remaining roadmap implementation or scale/recall gap | Full-roadmap completion is not established by the checklist | Link specific code/tests and limitations | Resolve or explicitly revise release scope |

Copy this table into the candidate's review record and replace placeholders with observed results. A checked box is a claim that must be supported by the linked evidence.
