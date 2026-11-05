# New issues from the 4 November 2026 external audit, hands-on test and latest GitHub reports

**Date:** 4 November 2026. **Inputs:** the ResearchCollab Technologies
*AIRK audit report v4* (static review of 1.8.7 source at `b624b31` plus a
hands-on macOS run), its companion *AIRK 1.8.7 hands-on test evidence* record,
and GitHub issues [#83](https://github.com/iokaio/airk/issues/83) and
[#82](https://github.com/iokaio/airk/issues/82) (the two most recent at the
time of writing). **Status:** analysis and a recommended plan, not implemented
behavior. **Boundary:** nothing in the *new issues* lists or the plan below is
part of the 1.9.0 release currently being implemented on `feat/finetuning`;
items marked *covered* are tracked by [plan-v190.md](plan-v190.md) and the
[implementation ledger](implementation-v190.md) and are listed here only so
the two sources agree.

Both external documents describe **1.8.7** installed from the GitHub release
asset `airk-1.8.7-darwin-arm64.vsix` on one Mac (macOS 26.6.1, Apple Silicon,
VS Code 1.143.1, clean standard user). The audit's static findings are from
source at `b624b31` (one docs-only commit after the `v1.8.7` tag). The working
branch is at **1.8.8** with P0a–P6e accepted and P6f in progress, so every
finding below must be rechecked against the live path before implementation;
several were already addressed or changed by accepted phases.

The audit also notes that the hands-on findings "do not change the scores
yet" and that a rescoring review is reasonable. Its rubric band (Isolation
only or Do not use, depending on the reader's gate G6 interpretation) is the
auditor's assessment of 1.8.7 and is not restated here as an AIRK claim.

## How the sorting was done

- **Covered by the current plan** means the behavior is an explicit outcome or
  acceptance obligation of a phase or package in plan-v190.md (P0–P14, REQ-*
  rows, or the P6a–P6f remediation table), and the ledger shows that work is
  accepted, in progress or scheduled. Where the ledger shows the phase as
  accepted, the finding needs only a recheck on the current branch.
- **New issue** means the plan does not name the behavior, names it only as a
  qualification obligation without a behavior change, or covers a neighbouring
  behavior that would not resolve the report. Several new issues overlap with
  GitHub issues #68–#81 filed on 4 November by other testers; those are listed
  as cross-references only and were not analysed here.
- Audit IDs use the report's own labels: hands-on F1–F6 and G1–G4; static
  `extension-*`, `rust-*`, `supply-*`, `quality-Q*`, `health-P*`; rubric
  criteria such as CON-01 or MCP-07; recommendations R01–R15.

## Part A. Audit report and evidence record: covered by plan-v190

| Audit ID | Finding (short) | Where the plan covers it | Ledger state (4 Nov) |
| --- | --- | --- | --- |
| F1, extension-H1 (binary provenance), SUP-01/SUP-03, R01/R02 | Installer copies the app-sealed `airk` out of `AIRK.app` to `runtime/airk-launcher`; the copy fails strict `codesign` (Info.plist not bound) and macOS kills it with SIGKILL. Same bytes run from inside the bundle. Reproduced; matches #66. | P6a — installed launcher and release producer (#66, #67): separately signed launcher image, strict copied-image checks, install/repair/upgrade/uninstall paths that execute copied binaries. | P6a local source gates complete; actual Developer ID signed copied-image probe passed on neo. **Notarization and installed-editor acceptance on a clean macOS account remain open release gates.** The auditor's Mac (26.6.1) is a second data point for the P6a/P14 installed matrix. |
| F2 | *Diagnose & Repair* fails the same way after asking two setup questions. | P6a (repair path uses the same copied image); P6f setup/repair entry points. | Same as F1. Recheck that the repair command uses the corrected image once the P6f installed-editor driver runs. |
| F5, health-P4, rubric CON-02/G8 | Raw code `runtime_client_no_longer_authorized` logged in Restricted Mode and during the failed install; on-screen text was plain language. Restricted Mode started no process and wrote nothing (pass). | P6f (#65): distinguish Restricted Mode from disposed/revoked client authority, offer *Manage Workspace Trust*, suppress unavailable native commands. | Ledger records "exact untrusted-versus-disposed reporting" corrections and passing trust suites. The logged wording during a failed install should be rechecked; see new issue N3 for the install-failure message. |
| supply-M1, supply-X2, SUP-02 | Marketplace darwin-arm64 package is a different, ad hoc signed build from the GitHub release asset; release page and release record disagree on Marketplace publication. | P6a: both channels consume one approved immutable signed native artifact set with provenance; P14 both-channel verification. | P6a producer work accepted locally; Marketplace selects `signed_run_id`. Publication is a separate authorized action. |
| rust-I6, supply-C3, DOC-02, R09 | Tool-count and wording drift: docs say sixteen/fifteen/27 tools, README says "read-only MCP tools" while 4 of 28 Explorer tools are `readOnlyHint: false`; `docs/vscode-extension.md` still presents 1.6.4/1.7.6 as current. | Plan line "tool-count/help drift and unhelpful errors warrant small catalog/documentation corrections" (P6f/G1); CLAUDE.md requires README/index/release notes to agree at every commit. | P6f catalog diagnostics distinguish UI-only tools and public aliases. The `manage.tsx:463` "27 tools" text is still present on the branch; see #82 item (c) below. |
| G1 | Keyword (lexical) search put a test script first for three of six questions and found the expected file in the top 8 for only 3 of 6. | P13a — S1 lexical ranking, conditional on frozen nDCG@10/Recall@10 gates. | Not started; conditional. The auditor's six-question set is a useful additional held-out sample for P13a if the fixture project can be shared. |
| G4 (first half), rust-M1 freshness, #64 | New files are not found until an index is queued unless watching is on; edits wait behind the embedding backlog. | P6d — source admission fairness (#64). | P6d accepted. The macOS watcher half of G4 is new; see N6. |
| extension-M3, CON-03 (auto-registration) | Every trusted folder is registered and indexed with no further prompt; home and drive roots excluded. | X2/X3 in P6f: explicit setup, reviewed window associations and grants; "Opening a folder must not silently create an index or start watching." | P6f in progress; installed-editor scenarios pending. |
| F3 (second half), OPS-02 (diagnostic retention) | "Could not retain installation diagnostic: Diagnostic control directory is unavailable." | P6f: "Explain missing selected data-leaf recovery; do not recursively create arbitrary paths." | Partially: the explanation contract is planned; retaining a failure diagnostic when the control directory does not yet exist is not. See N3. |
| rust-L2, supply-M3, quality-Q11 (rusqlite git pin) | `rusqlite`/`libsqlite3-sys` come from a pinned git revision, outside the registry yank/advisory flow. | P6a dependency policy: cargo-deny and cargo-vet with review required for Git revision changes. | Accepted in P6a (366 third-party versions vetted; 319 baseline exemptions explicitly unreviewed). Replacing the pin when a registry release lands is a follow-up; see N14. |
| quality-Q8, supply-I2 (no cargo-deny, self-attested advisories) | No cargo-deny; advisory dispositions self-reported. | P6a dependency policy (above). | Accepted; receipts under `artifacts/dependency-policy/`. |
| TEST-03, REQ-F-2 | No fuzzing; malformed fixtures only. | Selective Shark Tank REQ-F-2/REQ-S-4: opt-in fuzz targets for exposed parsers during P6e/P7/P11. | Scheduled by phase. |
| rust-I5, quality-Q4 (unsafe inventory) | 316 unsafe keywords in 40 production files; one `transmute_copy`. | Plan: review affected process/platform/unsafe boundaries when changing them; "a source-wide unsafe count alone establishes no safety result" (REQ-C). | Review obligation per phase, no separate action. |
| PROC-06, TEST-04, EV414 throughput | Embedding at about 16 vectors/s on one core; Apple Silicon model quality unqualified. | Plan: "Observed vector throughput and an Apple Silicon run do not establish model/performance qualification"; P14 platform matrix. | Recorded limit. |
| supply-L10, extension-M1 (PATH blocks), health-P4 | Setup edits shell startup files; consent dialog discloses it; Customize offers two opt-outs. Observed working as documented (hands-on steps O, R, T). | Existing consent design; P6f setup forms. | Behavior to preserve. |
| EV405–EV407 (passes) | Restricted Mode inert; dialog dismissal changes nothing; `AIRK.app` Developer ID signed, notarized, hardened runtime. | Behaviors the plan requires preserving (P6a, P6f). | Preserve; add the auditor's Restricted Mode and dismissal checks to the installed-editor driver if not already present. |
| supply-M4 (release binaries from a PR CI run) | Release targets can be built in a `pull_request` run and promoted when the tree matches. | P6a immutable signed producer with source/run/attempt/hash provenance. | Accepted; the governance half (zero required reviews) is new, see N22. |
| REQ-S-5, S-8–S-9 residue qualification | Install → first index → cited result → upgrade → uninstall/residue cases. | Folded into P6a/P6f and P14's installed matrix. | Qualification only; the residue **behavior** (F4) is new, see N2. |

## Part B. Audit report and evidence record: new issues

Severity is the auditor's where given; "Rec." is this analysis's suggested
priority for post-1.9.0 work. Line references are to the current branch.

| ID | Source | Issue | Severity / Rec. | Evidence and notes |
| --- | --- | --- | --- | --- |
| N1 | F6, EV409, EV411 | After the consent dialog is dismissed once (window lost focus), *AIRK: Enable Native Execution* and *AIRK: Diagnose & Repair* start a runtime install without showing the dialog again. The dialog is gated on a missing installation, not on recorded consent. | Observation / **needs maintainer decision** | `serviceExtension.ts` first-use review (around :575–:719). Explicit user commands may legitimately skip the review, but the install still writes PATH blocks, registers folders and copies about 650 MB without a second disclosure. Related: #83 item 11 and 15. |
| N2 | F4, EV412, EV416; GitHub #68, #70 | A failed install leaves about 906 MB in `~/Library/Application Support/AIRK` (launcher, runtime, state) and `/tmp/ak-<id>` (mode 700, one 64-byte `identity` file). Uninstalling the extension keeps all of it; the readme says to run *Review and Remove AIRK Data* first, a command that no longer exists after uninstall; runtime payloads are not mentioned as preserved. | Low (auditor) / **High for first impressions** | Plan covers residue *qualification* (REQ-S-5) and P6a resumable uninstall, not cleanup of a failed candidate install, nor data removal after the extension is gone. Options: the uninstall worker removes failed/unactivated runtime payloads; a standalone `airk runtime uninstall --data` path documented for post-uninstall cleanup; temp identity directories removed on failure. |
| N3 | F3, EV408, EV409, OPS-02 | The install failure surfaces as "airk failed: exit code 1" and "approved service exited before health: signal: 9 (SIGKILL)"; nothing names the signature or Gatekeeper cause. The diagnostic cannot be retained because the control directory does not exist yet. | Low to Medium / **Medium** | P6b preserves actual failure reasons and warns not to label every SIGKILL a signature failure. New work: run strict `codesign --verify` (or the platform equivalent) on the copied image before health and report that result as the cause when it fails; retain the diagnostic in the extension's own storage when the native control directory is absent. |
| N4 | extension-M2, CON-01, CON-03, EV406 | Three native self-checks (`runtime self-test`, `runtime location`, `runtime inspect`/`snapshot`) run on every trusted activation before the consent dialog; the dialog does not mention them, nor MCP provider registration, the Explorer assistance endpoint or Workbench persistence defaults. | Medium (auditor) / **Low–Medium** | Disclosed only in the `airk.runtime.nativeExecution` setting text. Add one line to the dialog, or move inspect/snapshot after consent where the install does not exist. |
| N5 | G3, EV416, EV417; GitHub #69 | `airk_status` reports `health: "unknown"` and `freshness: "unavailable"` while indexing and search keep working (observed three times, after `setup-model`). | Low / **Medium** (truthful status is a plan principle) | `crates/airk-mcp/src/lib.rs:910` returns this shape when published index metadata could not be read (`index_status_unavailable`). Cause not found by the auditor; likely a metadata read failing while a vectors publication is in flight on the hand-run `serve` route. Needs a reproduction with diagnostics output. |
| N6 | G4 (second half), EV416 | With watching on, the reply shows `backend: "periodic_fallback"`, `native_roots: 0` on macOS; native FSEvents watching was not used. Files were still found within 10 s. | Informational / **Low–Medium** | Plan P6d covers admission, not the watcher backend. Investigate why the native backend registered zero roots on macOS 26.6.1 for a root under the user home; record the fallback reason in the watch reply. |
| N7 | G2; GitHub #71 | Hybrid/semantic search for a nonsense query ("kubernetes cluster autoscaler") returned 8 unrelated hits; lexical returned 0. No visible relevance cutoff; scores not printed in the CLI output used. | Low / **Medium** | Not in P13a (ranking) or P2b (empty-result reasons). Options: expose scores in CLI/MCP/Workbench results (parity audit required), add a disclosed similarity floor or "low confidence" annotation, and say "no lexical match" when the lexical half is empty. |
| N8 | rust-M1, rust-M2, rust-X4, SEC-02, R04, R14; GitHub #72 | Secret exclusion is by file name only; a `.airkignore` negation can re-include the soft credential defaults (`.npmrc`, `id_rsa`, `kubeconfig`); `.mcp.json` and `.claude/settings.local.json` are indexed. | Medium (auditor) / **Medium, policy decision** | Documented ("not a secret scanner"). Candidates: promote agent/editor config files that commonly hold tokens to the default list; make negation of credential defaults require an explicit `allow-credentials` policy; keep content scanning out of scope. |
| N9 | extension-H1, SEC-07, EV005/EV006 | The full VS Code extension-host environment is passed to every native child (`native.ts:164`, `serviceClient.ts:42`). Tokens in that environment are visible to the native process. | Medium (auditor) / **Low–Medium** | Rust reads few variables (rust-I2). Pass an allowlisted environment (PATH, HOME, XDG/TMP, locale, AIRK_*) and document it. |
| N10 | MCP-01 note, gate G6, R09, EV043 | Each Explorer UI tool call (`draft_*`, `suggest_view`, help, templates, handoff) spawns a one-shot adapter process (`unified_mcp.rs:752–763`); the tool descriptions do not say so; only `docs/vscode-extension.md:138` does. | Rubric-pending / **Low** | One sentence in the 14 Explorer tool descriptions and the server instructions. Parity: CLI help catalog and MCP `tools/list` must agree. |
| N11 | extension-M6, rust-L1 | The Explorer assistance launch descriptor names a runtime executable that is not pinned or hashed (only ownership/mode checked); the server script hash is self-recorded. | Low / **Low** | Same-user only. Record and verify the editor runtime digest, or restrict to known editor binaries. |
| N12 | MCP-07 (QC adjusted to 1) | `serviceDashboard.ts:22` CSP allows `style-src 'unsafe-inline'`; all other webviews use nonce-only. | Low / **Low** | Move inline styles to a nonce'd style block. |
| N13 | supply-L2, supply-L1, SUP-04 | `verify-published-release.py` does not check macOS `codesign`/`spctl`/stapler; no platform-signed provenance attestation (SLSA Build L2) on release assets. | Low / **Medium for trust** | Add macOS signature checks to the verifier; evaluate GitHub artifact attestations in the release workflow (P6a producer boundary, after 1.9.0). |
| N14 | rust-L2 follow-up | Replace the `rusqlite` git pin with the registry release carrying the SQLite 3.53.3 fix when published. | Low / **Low** | Tracked by cargo-vet policy; needs a dated check. |
| N15 | supply-M5 | macOS signing workflow builds, tests and signs in one job; build scripts of 62 `cc`-dependent crates run on the runner that later holds the key. | Medium (auditor) / **Low** | P6a's immutable producer reuses CI-built bytes for the main release path; the repair path still compiles in the signing job. Split or document. |
| N16 | supply-L4, supply-L5, supply-L7, supply-L8, supply-L9 | Docker base images pinned by tag not digest; `taiki-e/install-action` downloads not repo-verified; tag provenance workflow does not run for workflow-created tags; stale `third_party/licenses/` directories; Developer ID certificate expires 1 Feb 2027. | Low / **Low (hygiene batch)** | One hygiene commit plus a release-runbook note for certificate rotation. |
| N17 | supply-C1, supply-C5, supply-C6, DOC-03, DOC-05 | README does not literally say "no telemetry"; PATH/registry edits are not prominent in the README; two home domains (`airk.ioka.io` in the manifest, `ioka.io/airk` in links); limits are scattered across README, SECURITY.md and vscode-extension.md. | Note / **Low** | Documentation only; fold into the next README/Marketplace audit. |
| N18 | supply-C4, extension-M5 | README wording "preserves unrelated client settings" versus observed rewriting of AIRK's own entries without a per-edit prompt; the auditor read a "never overwrites" impression. | Note / **Low** | Reword; optionally show a one-line notice listing files updated during setup (also #83 item 15). |
| N19 | quality-Q2, Q3, Q5, Q6, Q12 | 24 files over 1,500 lines; functions of 500–1,000 lines; about 220 `unwrap` and 97 `expect` in production (23 in `build.rs`); no `[lints]` table, `clippy.toml` or eslint; dense TypeScript (lines up to 1,197 chars); CONTRIBUTING is 62 lines of 12.5 KB. | Scores 3–6/10 / **Low, continuous** | Plan excludes broad rewrites and coverage ratchets. Reasonable post-1.9.0 steps: workspace `[lints]` with `unwrap_used` warn in `airk-ingest`/`airk-mcp`, eslint with `no-explicit-any`, and a short human quickstart in CONTRIBUTING. |
| N20 | quality-Q10, DOC-02 | 264 docs files, 32 `service-*.md`, 25 `v2-*` files; dated plans mixed with current guidance; `docs/vscode-extension.md:21/:112/:145` cite 1.6.4, 1.5.7 and 1.7.6 as current. | 4/10 / **Low–Medium** | REQ-D covers entry paths; a docs archive/status-banner pass is new. The stale version lines are a current-doc correctness fix. |
| N21 | VEN-03, OPS-03 | SECURITY.md has no confirmed private reporting channel or response commitment; CODE_OF_CONDUCT names no contact. | 1/4 / **Low, maintainer action** | Enable GitHub private vulnerability reporting and state a response target. |
| N22 | supply-M4, supply-X1, VEN-01, health-P2 | Branch ruleset requires 0 approving reviews; single maintainer; 188 of 210 commits unsigned. | Medium (auditor) / **Governance, not code** | Options: signed commits, environment approvals for signing/publish, documented compensating review. Not an implementation item. |
| N23 | CODE-03 (QC adjusted), supply-M2 | 57 tree-sitter grammar crates compile C that runs in the indexing process; only the PDF worker is isolated. | Low / **Deferred** | Isolating syntax extraction is a large change; record as a known boundary unless a fuzz finding (REQ-F-2) motivates it. |

Hands-on passes worth adding to the installed-editor driver as regression
checks: Restricted Mode starts no process and writes no data folder (EV405);
dismissing the consent dialog changes nothing on disk (EV406); no shell file
or LaunchAgent change at any point (EV412, EV415); the fake `.env` secret never
returned (EV417); only Unix sockets in a mode-700 directory (EV416).

## Part C. GitHub #82 — imports labeled as definitions, wrong qualified name

Reported 4 November by a Windows tester on 1.8.7 (Marketplace win32-x64,
Claude Code 2.1.316, producer `airk-tree-sitter/javascript`).

| Item | Reported behavior | Sorting | Analysis on the current branch |
| --- | --- | --- | --- |
| (a) | `const { formatPrice } = require("./utils/formatPrice")` and `const http = require("http")` are labeled **definition**. | **New** | `crates/airk-ingest/src/syntax.rs` JavaScript/TypeScript/TSX profiles list `variable_declarator` among definition kinds and only `import_statement` among imports. A declarator whose value is a `require(...)` call is therefore a definition; PHP already recognizes `require_expression`. Fix: classify a `variable_declarator` (and `lexical_declaration` member) whose initializer is a `require` call expression as an import with the module path as the target; keep ordinary declarators as definitions. Add JS/TS fixtures for destructured and plain `require`, and `import()` dynamic imports. Parity: `symbols`, `imports`, `code_facts` and Explorer › Code must all change together. |
| (b) | A call result in `server.js:8` shows `qualified: total::calculateOrderTotal`, where `total` is the variable receiving the result. | **New** | `syntax.rs:585–612` builds the scope chain from every ancestor whose kind is in the profile's definition list; `variable_declarator` is in that list, so an enclosing `const total = …` becomes a scope. Fix: build scopes only from container kinds (function, method, class, module, impl, namespace), or exclude `variable_declarator` unless its value is a function/class/arrow expression. Note that `qualified_name` feeds `code-qualified-name/1` fingerprints and `symbol_id`; changing the chain changes identities, so this needs a profile/producer version note and re-extraction disclosure rather than a silent change. |
| (c) | Management › Agents says one connection provides 27 tools; Claude Code `/mcp` lists 28. | **Covered** (tool-count drift, P6f/G1) | `packages/vscode/src/explorerWebview/manage.tsx:463` still reads "27 tools" on the branch. Fix is a one-line text correction, ideally derived from the catalog length rather than a literal. Verify it lands with the P6f documentation audit; otherwise carry it into the plan below. |
| (d) | *Open source* on a result citing lines 1–6 selects only the first word in the editor. | **New** | `packages/vscode/src/explorerWorkspaceIO.ts:45` opens with `selection: new Range(line, 0, line, 0)` (a zero-width cursor at the start line); the end line of the citation is not threaded through `openSource`. Fix: pass `endLine` (and column where known) and select the cited range, keeping the cursor-only behavior for extracted-text line bases. Workbench *Open source* should receive the same change (parity). |

## Part D. GitHub #83 — UX suggestions from a first-time Windows tester

All fifteen items are explicitly suggestions, not bugs. Sorting uses the same
rule as above; "partial" means a plan phase delivers part of the suggestion.

| # | Suggestion (short) | Sorting | Notes |
| --- | --- | --- | --- |
| 1 | One status card on every screen with one state-dependent action (Start / Reindex / Fix); header badge, Overview banners and Service tab currently disagree. | **New (partial overlap)** | P6f delivers the Indexing screen and P2b next-read guidance; P6e fixes the false "older index" label. A single shared status model across Explorer/Management/status bar is not planned. |
| 2 | Connect does three things: install config, add the agent instructions to CLAUDE.md, run a one-line test question. | **Partial** | P8 copies live-capability-checked harness instructions. Writing into CLAUDE.md and a smoke query are new; writing into a user file needs explicit consent and the N18 file list. |
| 3 | Put the strongest parts up front (Evidence Checker, Studio); Journeys cards and Help › About text are below the fold. | **New** | Information architecture. |
| 4 | Basic and advanced modes; hide hashes, raw codes (`service_unavailable`), "Not observed" rows and disabled buttons by default. | **New (partial)** | P6f/#65 removes raw codes from trust errors; a general basic/advanced switch is new. Must not hide the honesty disclosures the plan requires; it can collapse them. |
| 5 | Human formatting: local time, "2 min ago", MB not bytes, paths without `\\?\`. | **New** | Presentation only; keep exact values available in copies/exports (parity rule). |
| 6 | Fewer places to look: 8 Explorer sections, 8 Management tabs, about 24 screens, about 50 palette commands; *Set Up* sorts near the bottom. | **New** | Command naming/grouping and palette ordering. |
| 7 | Say each caveat once; the "does not prove saved source files still match" text repeats on most cards; the post-Open-source wording is clearest. | **New (partial)** | The caveat is P4's Report-mode disclosure; consolidating it into one place per view is a presentation change that must keep the per-result receipt in exports. |
| 8 | A reason next to every disabled button. | **New (partial)** | P6f suppresses unavailable native commands; disabled-reason tooltips are new. |
| 9 | Usage should lead with a verdict; "Estimated tokens avoided −2,045 / −30.7%" shown first on a small project; #78 (repeated excerpts) may explain it. | **New** | Also check the estimate for small corpora; #78 is a separate open issue. |
| 10 | Storage should show the full footprint (runtime 925 MiB, launcher 171 MiB) next to the index; the setup popup could state the install size. | **New** | Relates to N2 and P6a's measured launcher size (171 MB) and removed role copies (514 MB). |
| 11 | Setup popup: one headline and the folder path, details behind a toggle, an *Open .airkignore* button, three buttons instead of five. | **New (partial)** | P6f setup forms are in progress; this is a design refinement of the consent dialog, to be taken with N1 and N4. |
| 12 | Strategy cards: "you give, you get" and one button; steps and template ids under *How it works*; explain runnable vs preview. | **New** | Workbench/Studio presentation. |
| 13 | Outline as a compact tree, one line per item. | **New** | Explorer › Code presentation. |
| 14 | Workbench empty state should lead with what it does best, with examples from the open project. | **New** | Workbench presentation. |
| 15 | List the files AIRK writes into the project during setup (`.mcp.json`, `.airk/strategies`, `.airk/plans`). | **New** | Same disclosure gap as N1/N18; also #73 (absolute paths in `.mcp.json`). |

The tester's positive observations (indexing in seconds, `.env` skipped,
spaces in paths handled, path:line citations with hashes, 4 of 4 agent
questions right, live edits reaching the index in about 2 minutes) are
behaviors to preserve and add to the Windows installed-editor driver.

## Part E. Recommended implementation plan (after 1.9.0)

These workstreams start only after P14 and the 1.9.0 publication gates close.
They follow the plan's existing rules: one phase at a time, reviewed human
DCO-signed commits, CLI/Workbench/MCP parity for every read-contract change,
no telemetry, no new background work, and a patch/build version bump reminder
before each PR. Order is by user impact and dependency, not by effort.

| Order | Workstream | Items | Outcome and commit boundary | Acceptance evidence |
| --- | --- | --- | --- | --- |
| 1 | **Install failure cleanup and cause reporting** | N2, N3, #83-10 | Failed or unactivated runtime payloads and temp identity directories are removed on failure; uninstall removes payloads it owns and documents the data it keeps; a post-uninstall removal path exists; copied-image signature verification runs before health and its result is the reported cause; failure diagnostics are retained in extension storage when the native control directory is absent; Storage shows runtime, launcher and index sizes. | Isolated installed fixtures for failed install, repair after failure and uninstall after failure on Windows and macOS with residue inventories; the macOS case uses the actual signed image (P6a gate), not an in-bundle `--version`. Docs: `vscode-extension.md` uninstall section and release notes. |
| 2 | **Consent and disclosure refinements** | N1, N4, N18, #83-11, #83-15 | Record consent as the gate (not only missing installation); explicit commands that install after a dismissal show a short confirmation naming the effects; the dialog lists pre-consent self-checks, MCP registration and files written into the project; three-button layout with details toggle and *Open .airkignore*. | Trust/consent component tests for dismiss → Enable Native Execution → confirmation; installed-editor driver screenshots; no new native work before consent. Maintainer decides whether explicit commands may skip the dialog. |
| 3 | **Truthful status and watcher backend** | N5, N6, #69 | Reproduce `health: unknown / freshness: unavailable` during a vectors publication; return the actual readiness with a typed reason instead of `index_status_unavailable` when metadata is readable; report why native watching fell back to periodic on macOS and expose the reason in the watch reply. | Native transport tests across CLI/MCP/Workbench for status during publication races; macOS FSEvents registration test on neo with a home-directory root; no per-read health probes. |
| 4 | **Code facts correctness (JavaScript/TypeScript)** | #82 (a), (b), (d), (c) if not landed | `require` declarators classified as imports; scope chain limited to container kinds; cited end line threaded through *Open source* in Explorer and Workbench; "27 tools" derived from the catalog. | Ingest fixtures for plain/destructured `require`, dynamic `import()`, nested arrow functions and class methods; producer/profile version note because qualified-name fingerprints change; `symbols`/`references`/`code_facts` parity tests; editor test asserting the selected range. Re-extraction disclosure in Indexing. |
| 5 | **Retrieval honesty for empty and weak matches** | N7, G1 sample, #71 | Scores or a confidence annotation in search results on every transport; a disclosed semantic floor or "no lexical match" note when the lexical half is empty; add the auditor's six-question set to held-out labels for P13a. | Parity tests comparing full results and copies/exports; frozen relevance gates unchanged; no ranking change in this workstream. |
| 6 | **Secrets policy defaults** | N8, #72 | Maintainer decision on agent/editor config files (`.mcp.json`, `.claude/settings.local.json`, `.vscode/mcp.json`) joining the default exclusions; `.airkignore` negation of credential defaults requires an explicit policy; documentation states the file-name-only model plainly in README and SECURITY.md. | Ingest scope tests for negation with and without the policy; Explorer search and MCP `search` confirm exclusion; migration note for existing indexes (re-extraction, not a silent change). |
| 7 | **Supply-chain and provenance hygiene** | N9, N13, N14, N15, N16, N17 | Allowlisted child environment; macOS signature checks in the verifier; evaluate artifact attestations; replace the rusqlite pin when available; Docker digest pins; stale license directories; certificate-rotation runbook note; README wording. | Verifier run against the next published release assets; attestation verification documented in `release-verification.md`; no change to runtime behavior except the environment allowlist, which gets native-client tests on all three platforms. |
| 8 | **Management UX consolidation** | #83-1, 3–9, 12–14, N10, N11, N12 | One shared status model feeding the status bar, Explorer header and Management; basic/advanced display with disclosures collapsed not removed; human formatting with exact values retained in copies; disabled-reason tooltips; palette grouping and ordering; Usage verdict first; Strategy, Outline and Workbench empty-state redesigns; tool-description sentence for spawned adapters; CSP nonce for the dashboard; descriptor runtime digest. | Design review before code; webview component tests; parity audit for every view whose JSON/Markdown copy or export changes; installed-editor driver screenshots on Windows and macOS. |
| 9 | **Code health and documentation structure** | N19, N20 | Workspace `[lints]` table with `unwrap_used`/`expect_used` warn in `airk-ingest` and `airk-mcp`; eslint with `no-explicit-any`; human CONTRIBUTING quickstart; docs archive with status banners; correct the stale version statements in `vscode-extension.md`. | Clippy and eslint clean on the touched crates/packages; link and repository checks; no behavior change. |
| — | **Governance (maintainer, not code)** | N21, N22, N23 | Private vulnerability reporting and a response target; signed commits or environment approvals; a recorded decision on grammar-parsing isolation. | Not a release gate for code work. |

Dependencies: workstream 1 depends on the P6a signed image and the P6f
installed-editor driver; 2 depends on the P6f setup forms being accepted; 3
and 5 touch read contracts and need the parity audit from CLAUDE.md before
they ship; 4 changes extraction identities and needs an Indexing re-extraction
disclosure; 8 should follow 1–3 so the status card reflects the corrected
observations rather than restyling the current ones.

## Evidence limits of this analysis

- The audit's static findings describe `b624b31`; line numbers quoted in the
  audit may differ on the current branch. Only the lines cited in Parts C and
  the new-issue table were rechecked live (`syntax.rs`, `explorerWorkspaceIO.ts`,
  `manage.tsx`, `airk-mcp/src/lib.rs`, `native.ts`, `serviceClient.ts`,
  `serviceDashboard.ts`, `serviceConsole.ts`).
- Hands-on results come from one Mac and one Windows machine on 1.8.7; nothing
  here qualifies the 1.8.8 working candidate on either platform.
- GitHub issues #68–#81, filed the same day, overlap several new issues and
  were listed as cross-references only; they need their own triage.
- No scores, gates or bands from the audit's rubric are adopted as AIRK
  statements; the auditor records them as pending a rescoring review.
