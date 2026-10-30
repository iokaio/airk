# AIRK 1.7.0 candidate — Workbench and bounded evidence plans

Status: unreleased feature branch, 2026-10-30. The contributor requested version **1.7.0** for this feature. Native workspace crates, local Cargo lock entries, the extension manifest and npm lock root entries are synchronized. This record is not publication, installed-artifact acceptance or cross-platform qualification.

AIRK Workbench adds a secondary-sidebar evidence transcript for direct local queries without an agent. It includes project and path scope, slash commands, reusable evidence recipes, native schema-generated forms, editable plans, Structured/JSON/Markdown output, cited source/context actions, selection and copy/export, and bounded local sessions. Compatible inputs can be saved through the existing guarded Strategies workflow; unsupported conversions are refused.

Agents and the CLI share the kernel's `parse` prepare/execute entrypoint. The unified MCP catalog has **28 public tools**; the native catalog has **18 compatibility aliases** including `airk_parse`. Explicit plans compose read-only evidence operations with typed bindings, a fixed scope and publication, call/fan-out/time/byte limits, per-step outcomes and explicit completeness. They cannot invoke management, source writes, shell commands, recursive plans, external services or agent reasoning. A missing editor does not prevent native execution.

Deterministic supported requests can execute directly. Learned requests produce inert proposed plans: choose **Review proposed plan**, check the operation and arguments, then **Run plan**. Raw unsupported rejection and calibration abstention still miss automatic-execution targets. No automatic learned-intent execution or complete language/call understanding is claimed. The [implementation record](../workbench/implementation.md) lists contracts, differences from the [target design](../workbench/design.md), and remaining qualification.

The extension's automatic-upgrade allowlist covers forward movement within service-compatible 1.4–1.7 lines, including a 1.6.4 runtime upgrading to 1.7.0. Downgrades, malformed versions and unknown later minor lines remain blocked. This policy change does not substitute for installed upgrade testing. Native pairs and VSIX candidates use fresh 1.7.0 identities. No index storage-format or vector-model change is introduced by Workbench.

Historical release records now live in [docs/releases](README.md); incoming links and current tooling follow that archive. The pinned 1.4.4 repair retains its original source-commit paths.

## Classifier evidence

The broader offline campaign contains **40k supported + 6k original unsupported + 2k attributed public assistant requests** for training, plus separate 5k validation and 5k calibration sets. Validation selected the 40k supported-example candidate. Calibration accuracy is 98.8%, accepted precision 99.85% and abstention 14.7% (target at most 10%). The retained utterance fixture scores **146/150 (97.33%)**, with no wrong accepted classifications there. Raw rejection accepts one of 20 curated unsupported requests and four of 1,000 public unsupported requests. Three repository regression sets score 95.00%, 96.15% and 96.62%.

These results support a review-only model; automatic learned execution remains disabled. The fixtures are agent-labeled and await independent label review. Model SHA-256: `b18106b34110313fb9d1cd24c9a22d1ad300ffab7b4def80a381cbc0ac0a7c5d`. All expanded examples, raw public inputs, provenance and earlier revisions are saved under ignored local paths, with content-addressed copies outside Cargo's cleanup directory. See the [training record](../../tools/parser-train/TRAINING.md).

## Validation

Local Windows validation completed:

- Full all-feature workspace tests passed: **1,466 passed, 17 ignored**. Subsequent parser review fixes passed all three focused kernel Workbench tests, including short-file read bounds, explicit confirmation and unsupported-input safety. A real CLI/public-MCP parity test covers structured requests, project selection, citations and unchanged sources.
- Formatting and workspace Clippy with all targets/features and warnings denied passed after the final native change.
- Extension compilation/bundle checks and tests passed: **415 passed, 15 skipped**. Script contracts passed: 207 tests, five skipped, including packaging allowlists and release archive paths. The five classifier-training contracts also passed.
- Fresh normal-feature debug and final release runtime pairs passed native-client/model checks with two isolated projects, 18 native aliases, 28 public tools, three concurrent client identities and reconnects, retained-source recovery, worker exit and unchanged source bytes. The serial Workbench check sends a learned request, verifies an inert proposal, explicitly confirms it once and retrieves its cited short-file contents. Concurrent ordinary reads retain the existing typed contention-retry contract; whole plans are never automatically replayed.
- The final release pair also passed managed-frontend smoke tests covering reviewed registration, pre-index diagnostics and stable-launcher evidence with sources present/unavailable. These are disposable local fixtures, not proprietary agent or installed-editor UI qualification.
- Browser fixtures exercise the real compiled Workbench bundle in narrow dark/light and wider layouts, inert source text, source handles, format switching, slash input and explicit plan confirmation. They use fictional host messages and do not establish installed-editor accessibility.

The fixed intent asset is 852,116 bytes; Workbench JavaScript/CSS are 47,917/5,382 bytes. The final release main/helper executables are 176,263,680/55,316,992 bytes. The verified Windows x64 VSIX is 69,439,170 bytes (66.22 MiB). Same-profile before/after native-size acceptance has not been measured.

Installed-editor visual/accessibility checks, installed upgrade acceptance, four-core repeated pre-PR qualification, other platforms, hosted CI, independent label review and publication were not performed. Runtime notice generation retains three existing license declarations needing release review (`adobe-cmap-parser`, `pdf-extract`, `type1-encoding-parser`). Existing platform, installed-editor, license and release provenance gates remain in place. No commit, push, PR or publication was requested or performed.

The local unsigned review installer is saved at `artifacts/workbench-review-1.7.0/extension/publish/airk-1.7.0-win32-x64.vsix`, with installation steps in the review folder's `README.md`, checksums in `extension/publish/SHA256SUMS` and validation receipts in `review-evidence.json`. VSIX SHA-256: `2f16f24b75a3850b8b779dbd3ea2d34e06b5e9811174de0e62751c4a3146f29d`. The package was verified without installing it into the user's VS Code.
