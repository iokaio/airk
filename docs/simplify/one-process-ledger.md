# One resident process: task ledger

## Service/extension split planning (10 November 2026)

- Request: create `feat/split` from the current `feat/simplify` HEAD and write
  `docs/split/split-plan.md`; preserve the source branch's features.
- Acceptance: branch starts at `59dd3c721ca4c8301664827abaa9ea2ac6fd8dc8`;
  the plan assigns signed standalone service installation to downloads from
  airk.ioka.io/GitHub Releases and limits the extension to Explorer/Workbench,
  verified discovery and missing-service guidance. Cover feature preservation,
  predecessor handling, packaging, compatibility, signing and validation.
- Boundary: planning documentation and this ledger only; no implementation,
  installer repair, build, version bump, commit, push or publication.
- Review checkpoint: at most 15 minutes. AIRK MCP is absent from available
  tools; `.codex/config.toml` disables it. Use live local source inspection.
- Source-review findings: standalone installer source already exists; extension
  source still bundles and installs native payloads. Current version is 1.8.23;
  the stopped HTTP-cleanup correction has no rebuilt/qualified installer.
- Required blocker for eventual delivery: installed transition and cleanup
  proof. This planning task does not resume that stopped repair. A universal
  VSIX and independent product versioning remain proposals to assess.
- User clarification: Linux signing is optional. The plan retains Windows/macOS
  signing requirements and verified, explicitly unsigned Linux delivery.
- Intended validation: review plan against live installation/extension source
  and contracts; check local Markdown links; run `pwsh -NoProfile -File
  scripts/check-repo.ps1`, `git diff --check`, verify branch and source identity.
  No reproduction, compiler/test campaign or installed qualification is claimed.
- Results: `feat/split` and `feat/simplify` both resolve to the acceptance SHA.
  Plan completed and reviewed by the primary agent and a read-only service
  reviewer; no review blockers. Repository checks passed for 2,230 public files,
  including local Markdown links and the production self-spawn roster. The
  tracked diff passed whitespace checks; the new untracked plan needed a trailing
  blank line removed before its final whitespace check.
- Environment: sandbox process is `TYAI\codexsandboxoffline`; checkout, `.git`,
  new branch reference, plan and ledger ownership are `TYAI\tdotn`. Git commands
  use command/process-scoped `safe.directory`, without persistent configuration
  or ownership changes. Unrelated Git ignore/cache read-permission warnings
  remain environmental limits; repository checks completed successfully.
- Status: documentation-only task complete; final whitespace verification passed,
  including the Linux-signing clarification.
  No builds, Rust formatting/Clippy/workspace tests or installed qualification
  were run; no executable inputs changed.

### Split plan revision 2 (10 November 2026)

- Request: review, improve and expand `docs/split/split-plan.md`. Add a clear
  extension/service boundary, HTTPS admin pages on the MCP endpoint, a separate
  signed `airkctl` CLI over the local pipe with a service-only `airk`, code and
  installer maps, an extension that leaves old installs alone and handles MCP
  entries, and a service installer that removes old AIRK.
- Boundary: the plan and this ledger only. No implementation, build, test
  campaign, version bump, commit or publication.
- AIRK MCP: configured in `.mcp.json` but the server closed its connection
  (`CONNECTION_CLOSED`). Local tools were used.
- Source-review findings (not reproductions): three read-only investigations
  mapped native process/IPC/HTTPS, the extension, and installers/release. The
  primary agent spot-checked the gateway `Origin` refusal, the
  `service_already_running` refusal, client/service version equality, pipe
  naming, the single-executable rule, `REPLACEMENT_THROUGH`, the window
  session spawn, the HTTPS evidence client and the process-topology roster.
- **User decisions** (answers given in this session; plan §2.1 U3–U17):
  management is admin-only; `airkctl` owns agent configuration writes; one
  `airkctl session` per window; Explorer Assistance routed through that
  session; fixed-layout installer with a CI-signed setup step; receipts-only
  removal that continues and reports; admin search and evidence in the first
  release with its own index selection; one service per user.
- **Agent proposals awaiting the user** (plan §2.2 O1–O12): two new crates,
  the D8 contract changes, the HTTPS boundary change, state-contract scope,
  later-install index policy, MCP entries without a matching index, version,
  whether the stopped repair is superseded, uninstall index policy, the home
  of the setup step, the crate rename and the universal VSIX.
- Validation: local Markdown links and repository checks through
  `scripts/check-repo.ps1`; whitespace check of the tracked diff and of the
  untracked plan.
- Results: `pwsh -NoProfile -File scripts/check-repo.ps1` passed for 2,230
  public files, including the self-spawn roster. `git diff --check` passed. The
  untracked plan was checked separately: every local Markdown link and every
  source path it names resolves, with no trailing whitespace, tabs or carriage
  returns. No build, compiler, test or installed run was made, and no
  executable input changed.
- Status: documentation-only task complete. Implementation waits for the
  user's review of the plan and the open decisions.

### Split plan revision 3 review (10 November 2026)

- Request: improve the split plan, including the concrete CI changes and
  architectural separation needed for implementation, maintenance and deployment;
  account for the revision-2 pass notes. Ask before changing technology choices
  or adopting a recommendation contrary to the plan.
- Acceptance: a source-backed CI/build/release change map, explicit common
  contract ownership and dependency rules, phase prerequisites and validation
  evidence. Preserve U1–U17 and distinguish all unanswered proposals from approval.
- Boundary: `docs/split/split-plan.md` and this ledger only. No implementation,
  version bump, commit, build, installed repair or publication.
- Review checkpoint: at most 15 minutes; extend only for a named acceptance
  blocker. Parallel read-only reviews cover CI/release and architecture/contracts;
  the primary owns edits, combined review and all checks.
- AIRK MCP: no available AIRK tools; `.codex/config.toml` disables the server,
  while `.mcp.json` still names its launcher. Live local files are the evidence.
- Source-review questions: which workflow dependencies assume one executable or
  a bundled VSIX; where shared contract generation belongs; which phase edges
  and open decisions must precede consumers. These are review questions, not
  reproduced failures. No executable correction or qualification is in scope.
- Intended validation: inspect the combined documentation diff, verify local
  links and cited current paths, run `pwsh -NoProfile -File scripts/check-repo.ps1`
  and `git diff --check`, and confirm only these two files changed. Use the
  command-scoped Git safe-directory exception for this checkout; do not change
  persistent Git configuration or file ownership.
- Review checkpoint: the bounded source review is complete. Required planning
  corrections were missing frontend inputs in native build identity, omitted
  CI selection/producer/container owners, conflicting shared-contract ownership,
  and incomplete phase/deployment dependencies. These are source-review findings,
  not reproduced runtime failures. No broader hardening work was started.
- User decisions added as U18–U21: retain Preact/esbuild; generate and seal admin
  assets before native compilation; keep thin setup-owned uninstall delegation;
  require local service-installer and VSIX force-build scripts, at least for
  Windows. The user explicitly limited the script request to plan deliverables.
- Open choice O13 was asked directly: local unsigned test artifacts with explicit
  developer opt-in versus local test-certificate signing. No answer recorded yet;
  the plan does not weaken production verification. O1–O12 remain unanswered.
- Changes: expanded contract owners, dispatch/admission, journal/session recovery,
  shared UI/adapters, fixed-layout replacement, CI job/input/receipt/signing maps,
  local PowerShell script contracts and product acceptance. Native/service
  receipts and independently buildable VSIX receipts remain separate until
  release assembly. Stopped repairs remain stopped.
- Review: the primary inspected the complete diff and both read-only reviewers
  checked their affected sections. Corrected installer pre-placement ordering,
  independent VSIX receipts, post-tag verification wording and duplicate-build
  risk in local acceptance. No code or workflow files changed.
- Validation: `pwsh -NoProfile -File scripts/check-repo.ps1` passed for 2,230
  public files, including local Markdown links, AGENTS/CLAUDE identity and the
  production self-spawn roster. Cited CI/test paths were verified against the
  live tree; proposed package/script paths are labeled as future deliverables.
  `git diff --check` passed. Only the plan and this ledger changed. Existing
  Git ignore/cache read-permission warnings remain environmental limitations.
- Status: revision 3 documentation task complete, with O13 still awaiting an
  answer and O1–O12 unchanged. No builds, compiler invocations, Rust formatting,
  Clippy, workspace tests, installed qualification, commits or publication run;
  no executable inputs changed.

### Local Windows signing clarification (11 November 2026)

- User answer to O13: this machine has access to the same signing certificate
  as Windows CI; use it. Recorded as U22. The earlier pending O13 status above
  describes the previous review checkpoint, not the current decision.
- Acceptance/boundary: update only the split plan and this ledger to require
  signed local native/setup/uninstaller/installer outputs and an ordinary VSIX
  with normal verification. Preserve distinct local and hosted provenance.
  No certificate access, signing, implementation or build is requested here.
- Source review: AIRK MCP remains unavailable/disabled. The read-only CI reviewer
  checked existing signing ownership and policy. Reuse the authorized signing
  identity/profile and publisher checks; do not assume an exported key or fixed
  leaf thumbprint. Preserve the hosted signing adapter's guard.
- Validation: primary review complete; `scripts/check-repo.ps1` passed for 2,230
  public files and `git diff --check` passed. Only the plan and ledger changed.
  Existing ignore/cache read-permission warnings remain environmental limits.
  No compiler, certificate access, signing or qualification campaign ran.
- Status: documentation update complete; O13 resolved, O1–O12 unchanged.

### Split plan decisions U23–U58 (11 November 2026)

- Request: ask every question needed to plan and implement the split without
  interruption, then a second pass for remaining approvals; record answers in
  the plan as they arrive.
- Answers: O1–O12 recorded as U23–U34. Further decisions U35–U58 cover the
  Track S relationship, install paths, local script names, the security-review
  route, user-chosen program and data/index locations (U39, U50), the macOS
  setup fallback, CLAUDE.md wording, commits, the test machine, signing and
  hosted CI, phase gates (no stops, U45), interim versions, I2/I3 type
  generation (D5 reopened for those only, U47), tool installation,
  repository MCP configuration, checkpoints, the three-failure rule (U52),
  agents and workflows, dependencies, security-finding thresholds, trust in
  automated runs, no agent-opened PR, and Docker/Neo use.
- Source review for the second pass: `simplify-plan.md` §9 (D5 "fixtures first"
  conflicted with the plan's generated I2/I3 types), and local prerequisites:
  Inno Setup and `cargo-nextest` are missing; cargo is in `~/.cargo/bin` but not
  on this shell's PATH; signtool, az, Docker, Node and Python are present.
- Validation: `git diff --check` passed; `scripts/check-repo.ps1` passed for 2,230 public files. Documentation only;
  no build, compiler, signing or installation ran.
- Status: plan revision 4 complete and committed on `feat/split`. Phase 0 is
  next and authorized (U45).

### Split plan final decision pass U59–U69 (11 November 2026)

- U59 (extension README links to the service download) was requested directly.
  The final pass found that the hosted `macos-signing` and `native-signing`
  environments allow only `main` (`gh api …/deployment-branch-policies`), and
  `macos-signing.yml` also guards the ref. The user chose Neo signing (U60).
- Neo probe (11 November, read-only): connection from the ignored
  `artifacts/v190/neo-connection.local.json`; Darwin arm64; `~/code/airk` clean
  on `feat/finetuning`; one Developer ID Application identity; 272 GiB free.
  `notarytool --keychain-profile airk-ci` failed with `keychainLocked` over SSH,
  leading to U69. The SSH alias `neo` does not resolve on this Windows host.
- Other answers: independent-agent deletion review (U61), Phase 8
  qualification (U62), 1.9.0 bump at Phase 8 (U63), Intel Mac after merge
  (U66), push notifications for blockers (U67), switch the Neo checkout (U68).
- Pending user action: create `~/.airk-signing/keychain-pass` (mode 600) on
  the Neo before the first signing batch.
- Neo signing check (11 November, user-approved): the agent created the empty
  owner-only file and the user filled it. Login keychain unlock passed; a
  hardened-runtime, timestamped test signature with `Developer ID Application:
  Ioka LLC (5PX7D99PXA)` verified with `codesign --verify --strict`. No
  `airk-ci` profile exists on the Neo. The documented AIRK keychain
  `airk-signing.keychain-db` (profile `airk-notary`, password item
  `airk-signing-keychain-password` in the login keychain) unlocked, holds one
  Developer ID identity, and `notarytool history` listed 85 submissions. Both
  keychains were relocked. U65 and U69 now name that keychain and profile.
  No secret was printed or written.

### Split execution (started 11 November 2026)

Execution plan approved by the user: setup S0, Phase 0 inventory (P0.1–P0.9),
then Phases 1–8 as work packages decomposed here before each phase starts.
Decisions U70 (local Trusted Signing client and adapter) and U71 (promote the
receipt runner and golden capture; commit normalized goldens) came from the
planning pass.

Planning findings (read-only exploration, not test results):
- Local signing: no code-signing certificate in either Windows store; no
  `Azure.CodeSigning.Dlib.dll`; `az` login active and a code-signing token is
  issued. `sign-native.py` signs only by thumbprint; Azure signing exists only
  in `github-release.yml` (`azure/artifact-signing-action`).
- Tools: Inno Setup absent (CI pins 6.7.1 via the runner image);
  `cargo-nextest` absent (CI pins 0.9.148); cargo not on PATH.
- Goldens and the receipt runner live only in git-ignored `artifacts/one-process/`.
- `nativeOutcome.ts` has no shared Rust-emitted cases.
- `build-service-runtime.py::source_identity()` excludes frontend inputs; the
  runtime builder, packager, signer and Inno script assume one executable;
  `airk.iss` sets `DisableDirPage=yes`.
- The HTTPS listener starts only with a catalog gate (`service_runtime/http.rs`
  `Owner::start`).

**Resume point (update after every step).** Phases 0–2 complete (see 'Phase 2 exit'). Phase 3 (`airkctl`, old `airk` CLI frozen; steps in 'Phase 3 decomposition'): 3.0, 3.1 and 3a (3a.1, 3a.2a–d, handback) done (`tools shell` moved to 3c). 3b.1a and 3b.1b done (Neo for 3b.1b recorded in its row). Next: 3b.1c (I4 reader). M2 lessons: per-method response budgets (contracts §2.8) apply on both sides through `ControlMethod::response_frame_budget`; documents whose field order matters travel as text; integration tests that start a reviewed service need `--features local-inference`. Package plan: 3.0 additive role dispatcher and private role admission in `airk` (C6 §5.7); 3a `tools list/help/call/shell` then evidence reads (C2 variant groups M2/M3/N1 land just before their handlers; `airkctl` gains `airk-ipc` and `airk-process` edges); 3b service start/stop/restart/status/logs/health/requests with detached start, install-pointer reading and the `airkctl → airk` self-spawn roster entry; 3c resolve/prepare/journal handlers (C3a recovery fixtures); C11a then 3d Explorer; 3e agents; 3f session (C4 frames, C3b window plane); 3g admin open/url, version, credential helper. Each package: service handler wraps the existing canonical function; parity by differential capture (old `airk` vs `airkctl` against the same seeded service, P0.3b); goldens; Windows Clippy both configs; Neo for Unix-touched code; full Python suite at handback; checks run with exit status checked directly. Follow-ups: Linux `linux_raw` compile (CI or Docker); macOS local installer receipt (C7). Receipts: `artifacts/split/receipts/`. Build env: `--target-dir artifacts/v190/p10-target --static-environment artifacts/local/one-airk-1.8.14/static-environment-supplement.json`.

#### Phase 3 decomposition (11 November 2026)

Rules for every step: the old `airk` CLI stays frozen and working; a service
handler wraps the existing canonical function (no reimplementation) and lands
in the commit just before its `airkctl` caller, together with its C2 variant
group (`not_yet_served` arm replaced, `fixtures/split/i1/control_methods.json`
row); parity is shown by differential capture (old `airk` and `airkctl` against
the same seeded service, P0.3b) plus the committed goldens; a behavior step
stays within about 25 files and 1,500 lines. Windows Clippy both configurations,
Neo for Unix-touched code, full Python suite at handback.

| Step | Content | Depends on | Done when |
|---|---|---|---|
| 3.0a | Private role selector `__airk_role <name>` in `airk` beside today's argv roles; bare selector without authority → `role_admission_refused` (C6 §5.7); existing hidden commands keep working | C6 | Role table fixture (`fixtures/split/i4/role_admission.json`) pinned; refusal tests |
| 3.0b | `service-detached` role: owner-private start request (`<control root>/start/<nonce>.json` until I4 exists; ≤ 30 s, consumed atomically; `explicit` clears a saved Stop, `automatic` honors it) wrapping today's detached start | 3.0a | Start/refusal/expiry/replay tests with an injected clock |
| 3.1 | `airkctl` gains `airk-ipc` and `airk-process` edges; differential-capture harness (`cli_golden.py --service`) seeding a fixture service | G6 | Harness tests; one smoke command compared |
| 3a.1 | M2 `ToolsCatalog` handler + `airkctl tools list/help` | 3.1 | Goldens and differential capture equal |
| 3a.2a | M3 contract step: `ToolsCall { tool, arguments, selection: Project{binding} \| Catalog{request: ScopedConnectRequest}, response_mode, usage_client }` → `ToolResult { result }`; response budget 4 MiB; client deadline covering the 125 s call context | 3a.1 | Fixture group in `control_methods.json`; `not_yet_served` arm; contract amendment recorded |
| 3a.2b | M3 handler: the old CLI's call path moved behind one library function (`public_project_backend`, response-mode insertion, `call_observed`) used by both `airk` and the service; `Catalog` resolves through `service_frontend::resolve_scoped` exactly as `--catalog-session` does | 3a.2a | Old CLI goldens and `tests/tools.rs` unchanged; handler tests |
| 3a.2c | `airkctl tools call` and the 15 evidence reads (`status`, `parse`, `search`, `files`, `read`, `info`, `outline`, `symbol`, `imports`, `facts`, `code-facts`, `relationships`, `refs`, `context`, `changes`) with `--service-runtime-dir`, `--service-project`, `--catalog-session`; the old `query()` argument mapping is ported (sole owner after Phase 4) | 3a.2b | Cross-binary parity test on the reviewed indexed fixture: same stdout, stderr class and exit for every read |
| 3a.2d | `--combined-session` selection (`combined_frontend::resolve`, `project_ids`/`index_ids`/`member_availability` defaults) over M2/M3; `tools shell` moves to 3c (needs M1 for its header) | 3a.2c | Parity cases for combined reads |
| 3a.2e | N1 `NativeReadScoped` (usage reads) moves to 3c with `usage` | — | — |
| 3b.1a | `airkctl service health` and `service shutdown` over I1 (`Health`; `Shutdown` fenced to the observed instance), output as `airk service health|shutdown --runtime-dir` printed it | 3a | Parity against the old commands on a live fixture service |
| 3b.1b | U84 moves by committed scripts: Unix owner-private file check → `airk-ipc`; per-user folder lookup and detached spawn → `airk-process`; `airk-cli` calls them | 3b.1a | Identity maps, goldens, boundary gate; Neo for the Unix halves |
| 3b.1c | I4 reader in `airkctl`: pointer path (`airk_process::user_folders`), owner/link/size checks (`airk_ipc::endpoint::require_owner_private`), `InstallPointer`/`InstallRecord` decode, executables inside `program` and their digests, `version` reporting the installation; signatures are 3b.1c-sig | 3b.1b | `fixtures/split/i4` cases on both platforms |
| 3b.1c-sig | Executable signature checks of contracts §5.4.3 (Windows Authenticode against the C10 publisher policy; macOS `codesign --verify --strict` against the team id; Linux digest only, documented weaker). Scheduled as its own step because no signed install pair exists before Phase 7; exercised on locally signed builds (U22/U60). Not dropped: the reader reports `installation_unverified` until it lands for a record that names a signature | 3b.1c | Signed and tampered fixtures on Windows and the Neo |
| 3b.1d | `service start`: owner-private start request + detached spawn of the record's verified `airk` (`__airk_role service-detached`) + Health readiness; `check-self-spawn.py` roster entry `airkctl → airk` | 3b.1c | Start/refusal tests; roster |
| 3b.1e | M19 `Lifecycle` contract, then the service half of `lifecycle::run` moves into the service (ledger, drain, fenced shutdown); `service stop` and `restart` (observe exit, then start) | 3b.1d | Lifecycle record fixtures; stop/restart on a live service |
| 3b.1f | `service status` and `service logs` (M13 while running; record log location while down, §3.10.4) | 3b.1e | Parity with `airk service logs` |
| 3b.2 | `service requests …`, M13 logs export, M14 record-error, U75 offline operations | 3b.1, C3a | Recovery fixtures pass on real transports |
| 3c.1–3c.4 | M1 resolve, M4 plan/prepare, M5–M8 journal + recovery, then `index/job/project/configure/settings/usage` | 3b | `request_recovery.json` cases on real transports; goldens |
| 3d | C11a catalog ownership, then M11/M12 Explorer commands (`--output-file`, U79) | 3c | Explorer goldens (recorded content-version change) |
| 3e.1–3e.2 | Agents: classifier + shared Rust cases, then writers/`connect`/`repair`/`remove`/`launch`/`verify` (V1); U49 repo MCP config | 3b | Shared cases consumed by the extension recognizer test |
| 3f.1–3f.3 | Service events + Assistance queue + window plane (W1–W8), then `airkctl session` framing/limits, then delivery and scope select | 3a, C3b, C4 | Session sequences and §6.5 fixture set on real transports |
| 3g | `admin open/url` (A1, H1/H2 rules on the `airkctl` side), `version` with the install record, credential helper | 3f, C5, C6 | I2 compatibility matrix fixture consumed |

| Step | State | Evidence |
| --- | --- | --- |
| S0.1 Record U70/U71 | done | split-plan §2.1 |
| S0.2 Inno Setup 6.7.1, cargo-nextest 0.9.148 | done | Inno 6.7.1 from the official GitHub release (`innosetup-6.7.1.exe`, Authenticode Valid, Pyrsys B.V., sha256 `4D11E805…A7B0`), installed with `/CURRENTUSER` to `%LOCALAPPDATA%\Programs\Inno Setup 6\` (log says 6.7.1). **Finding for Phase 7:** the genuine 6.7.1 `ISCC.exe`, `ISCmplr.dll` and `Compil32.exe` carry ProductVersion `0.0.0.0`, so `package-standalone-windows.py::compiler_identity()` will refuse this compiler; identity must come from another reviewed source (signature plus pinned digest). `cargo install cargo-nextest --version 0.9.148 --locked`: `cargo-nextest 0.9.148 (cd1d6d546 2026-10-07)`. |
| S0.3 Trusted Signing client | done | Microsoft renamed it Artifact Signing. `Microsoft.ArtifactSigning.Client` **1.0.146** (published 7 Oct 2026; 1.0.151 of 3 Nov is too recent) from nuget.org, nupkg sha256 `7761E5D8…6F70`, extracted to `~\.airk-dev-tools\artifact-signing\1.0.146\`. `bin\x64\Azure.CodeSigning.Dlib.dll`: Authenticode Valid (Microsoft), sha256 `123FA0C5…8A19`. **Probe (early S-sign check):** `signtool sign /fd SHA256 /tr http://timestamp.acs.microsoft.com /td SHA256 /dlib … /dmdf metadata.json` on a scratch copy of a third-party EXE, with the CI account/profile and only the Azure CLI credential enabled: signed, `Get-AuthenticodeSignature` Valid, signer `CN=Ioka LLC, …`, Microsoft timestamp; `scripts/verify-authenticode.ps1 -Publisher … -IdentityEku …` passed (exit 0). The signer role is available to this login; no repository file changed. |
| S0.4 Promote receipt runner and golden capture | done | `scripts/dev/receipt_runner.py` and `scripts/dev/cli_golden.py`, generalized from the ignored harnesses (machine paths, pinned Node location and target directory became arguments; receipts default to ignored `artifacts/split/receipts/`; selected-test count recorded and an empty selection exits 4). Goldens are stored normalized (`<FIXTURE>`, `<BINARY>`, `<HOME>`, `<USER>`). `fixtures/cli-golden/cases.json` seeds the original 30 cases. T0: `python -m unittest discover -s scripts/tests -p test_cli_golden.py` 10/10 and `-p test_receipt_runner.py` 6/6 pass. |
| P0.1 Native command disposition | drafted | `docs/split/inventory/commands.md`: 137 visible leaves + 20 other entry points; 24 new control methods, each wrapping a cited existing function. Source review only. |
| P0.2 Extension disposition | drafted | `docs/split/inventory/extension.md`: 59 commands, 18 settings, 13 spawn sites, 122 files, 76 `explorerContracts.ts` exports classified. |
| P0.3 CLI goldens (offline, Windows) | done | Binary: receipt `artifacts/split/receipts/p0-3-golden-build.json` (`cargo build -p airk-cli --bin airk --no-default-features --features local-inference --locked`, P10 target, static ORT supplement, 29 s, inputs unchanged). `fixtures/cli-golden/cases.json`: the 30 seeded cases plus `--help` for every path in `docs/cli-reference.md`, `tools help` for all 29 tools, `tools call airk_help`, `explorer list-templates` (204 cases). Normalization rules: path-derived digests (`expected_predecessor`, `binding`) and the `airk.exe` suffix. `runtime inventory --install-dir <fixture>` was excluded: it still measures the real per-user AIRK roots (non-hermetic). `fixtures/cli-golden/windows/airk.json`: 408 runs, exits {0,1,2}; recapture byte-equal; no user name or `airk.exe` remains. **P0.3b pending before Phase 3:** live-service captures against seeded fixtures and a Linux capture through Docker. |
| P0.4 Predecessor roster | drafted | `docs/split/inventory/predecessors.md`: 11 artifact classes, Appendix-D rows; today's `replacement.rs` refuses the whole replacement on one unproven item, so U14 needs a behavior step (four tests change expectations). |
| P0.5/P0.7 Dependencies, listener, paths | drafted | `docs/split/inventory/dependencies.md`. **Reproduced pre-existing failure:** `python scripts/dev/boundary_check.py` fails at HEAD (`module_cycles`: the 14-module client/runtime cycle merged with the catalog cycle into 20 modules; closing edge `project_supervisor.rs:380` → `project_worker::serve`, added in `fe607425`). Correction: Phase 2 group G5 injects the two closing calls as hooks; the reviewed baseline is not loosened. **Source review after a trial (11 November):** one hook is not enough. The baseline holds the catalog cycle {catalog_admission, catalog_coordinator, project_supervisor} and the 14-module client/runtime cycle separately; they merged because paths now run both ways. Catalog → client/runtime: `project_supervisor` → `project_worker` → `runtime_metadata`/`runtime_install`. Client/runtime → catalog: `service_runtime` → `http_gateway` → `catalog_admission`. A further edge `project_worker` → `reference_refresh_control::execute` (`project_worker.rs:1114`) → `catalog_coordinator` → `project_supervisor` pulls `project_worker` and `reference_refresh_control` into the catalog cycle. A complete G5 needs (a) the runtime/work/rebuild observers injected through the supervisor's constructor from its service-side creator (the supervisor must not name `runtime_metadata` either), and (b) a hook for reference-refresh job execution. The trial (observers wired from `project_supervisor`) still failed the gate and was reverted uncommitted; no source change remains. P0.3b refinement: live-service parity is captured differentially in Phase 3 (old `airk` and new `airkctl` against the same seeded service in one run) instead of as static goldens with per-run identities. |
| P0.8/P0.9 Build roster, signing adapter | drafted | `docs/split/inventory/build.md`; Inno `compiler_identity()` to pin digest + signer (`scripts/inno-setup-lock.json`); `check_lightweight_dependencies()` is never called; local receipts use `…-local/1` schemas. |
| C10 Signing identity file | done | `scripts/signing-identity.json` (`airk-signing-identity/1`) with `sign-native.py::load_identity()` (refuses unknown/missing fields, lowercase digests, team id format); `AZURE_SIGNING` derived with unchanged keys and order. T0: `test_native_signing.py` 12/12 (new: reviewed literal, loader refusals, hosted workflow inputs equal the file across all 3 action blocks, macOS constants equal the file); `test_ci_windows_signing.py`, `test_github_release.py` 26/26, `test_standalone_windows.py` 13/13, `test_ci_macos_signing.py` pass. |
| C1 Core move (usage_page) | done | Committed move script `scripts/dev/split_c1_usage_page.py` (git mv, `airk_core::`→`crate::` in the moved file, kernel `pub use airk_core::usage_page`). Identities (`test_identities.py`, core+kernel lib): before core 173 ok, kernel 227 ok + 2 ignored; after: exactly the 2 `usage_page::tests::*` moved kernel→core, 0 outcome changes (receipts `c1-identities-before/after`). `cargo fmt --check` ok; `dev-check.py -p airk-core -p airk-kernel` pass, 55 s, P10 target, incremental on, timings `c1-clippy-cargo-timing.html`. Rebuilt `airk` (receipt `c1-golden-build`): goldens 408/408 equal. Resolution shapes were not moved: `RootObservation` holds store/fence/`Instant` and none is serialized, so the M1 response is a new C2 payload. The credential-record split moves to package 3g. `fixtures/wire/usage_page.json` (independent vectors) remains a separate C1b step. |
| C2 landing strategy | decided | Per plan C2 alternative and `contracts.md` §2.2–2.3: each I1 variant group lands as its own contract commit (variants, `not_yet_served` arm, console refusal arm, `fixtures/split/i1/*` rows) immediately before its Phase 3 handler package, instead of one 26-variant commit. Reason: several payload types (M1 resolution result, selection unions, receipts) must be designed as new Core wire types, and one commit would exceed the step limit. |
| C4a `airkctl` crate skeleton | done | `crates/airkctl` (lib `contract` + bin): exit classes 0–8, error line, version document, session protocol 2 frames (scope/Assistance/handoff frames wait for C3); bin serves `--version`/`version` locally, everything else exit 5 `not_yet_served`. Fixtures (independent): `fixtures/wire/airkctl_session_frames.json`, `fixtures/split/i2/{exit_codes,error_lines,version}.json`. Edge gate `airkctl → airk-core`; `release-version.py` now also syncs `airkctl` (`LOCAL_CRATE`, test 14/14; check 1.8.23 ok); `supply-chain/source-policy.json` first-party roster gains `airkctl`. T0 `cargo test -p airkctl --locked` 9/9 (receipt `c4a-tests`); `dev-check.py -p airkctl` pass 20 s; smoke: `--version`, `version --output json`, exit 5 JSON/text, `--output xml` exit 2. `license-notices.py --release` ok (431). **Pre-existing, unrelated:** `dependency-audit.py` fails on live advisory RUSTSEC-2026-0486 (rustix 1.1.5 unsound, published 2026-10-27, no disposition) in cargo-audit dispositions and cargo-deny; follow-up, not in this step. Compatibility-matrix and offline-operation fixtures land with their first consumers. |
| C4b U47 I2 generation pilot | done | `schemars` 1.2.2 (already locked via `rmcp`, same features; no new identity) derives on the I2 types; `contract/schema.rs` writes identity patterns and opaque I1 values by hand. `cargo test -p airkctl --lib contract_schema` pins `packages/vscode/src/generated/i2.schema.json` (explicit `AIRK_WRITE_CONTRACT_SCHEMA=1` rewrite only). `scripts/dev/contract_types.py i2 --check|--write` renders `i2.ts` (strict subset; unknown constructs fail); `check-repo.ps1` runs the check. Extension `tsc --noEmit` (strict) passes with the generated file included. T0: `cargo test -p airkctl --locked` 10/10 (receipt `c4b-tests`); `test_contract_types.py` 4/4; `dev-check.py -p airkctl` pass 21 s; `check-repo.ps1` pass. Deviation from contracts.md §3.11: the drift check runs in `check-repo.ps1` (quick CI) instead of `npm run compile`, so the extension build needs no Python. Pilot findings recorded in `docs/simplify/contract-generation-pilot.md`. |
| C1b + C8 shared cases | done | Delegated source work (agent ran no builds), reviewed and validated by the primary. Generators `fixtures/wire/{usage_page,read_retry_envelopes,native_outcomes}.py` (`--check` 0): 33 usage-page cases, 71 envelope cases (16 with emitters `airk_mcp::remote::scoped_setup_failure` `remote.rs:101` and `response::typed_error_result` `response.rs:56`), 15 native-outcome cases. One compile error in the delegated test (`to_vec(parsed)` → `to_vec(&parsed)`) fixed by the primary. T0: `cargo test -p airk-core --lib` 183/183 (receipt `c8-core`); `cargo test -p airk-cli --test native_read_retry` 2 selected, 2 pass (`c8-cli`); extension `npm run compile` + `wireFixtures.test.js` 133/133 (`c8-ext`). Clippy `-p airk-core -p airk-cli --features local-inference` pass 42 s (`c8-clippy-li`). No Rust native-outcome classifier yet (its inputs belong to the I2 exit mapper). Direct-query emitters are private and unchecked (shape-only cases). **Correction:** C4a/C4b had added `include_str!`/runtime fixtures without container inputs (`test_container_inputs.py` failed since `06692d01`); all fixtures are now in `Dockerfile`, `.dockerignore` and `container-inputs.json` (4/4, `test_container_runtime_build.py` 13/13). Cheap check added to the routine: run `test_container_inputs.py` whenever a crate gains a fixture. **Pre-existing, unrelated:** `dev-check.py -p airk-cli` without features fails on unused `Child` import (`project_supervisor.rs:22`, used only under a feature/test); follow-up. |
| C6 I4 pure records | done | `airk_core::install_record` (+ `CONTRACT.md`): pointer, record, setup operation, phases; canonical bytes; pure checks with injected clock; refusal codes. Independent generator `fixtures/split/i4/i4_cases.py` (9 pointer, 12 record, 7 operation cases, 14 phases). T0 within `cargo test -p airk-core --lib` 183/183 (5 install_record tests). Role admission enforcement and the filesystem/signature reader adapters land with their consumers (3.0, 3b, Phase 6). |
| U38/U55 security review | done | Independent review `docs/split/security-review-i3.md` of `contracts.md` §4: 0 critical, 2 high (H1 cross-user launch-code capture from process arguments plus no peer identity on loopback; H2 remote-workspace port forwarding), 5 medium, 9 low, 6 info. Verdict: no high or critical remains if the recommended changes are applied; no fix changes a U decision. Under U55 the medium-and-above texts are adopted as normative §4 amendments (`contracts.md` §4.11); low and informational items join the C5 checklist. C5 may proceed. |
| rustix 1.1.6 (U81) | done | RUSTSEC-2026-0486. `cargo update --precise` also moved 7 unrelated crates' `windows-sys` and was discarded; the lockfile changes only the rustix entry (rustix 1.1.6 accepts `windows-sys >=0.52, <0.62`; other minimums already met). Archive checksum matched; safe extraction; targeted review of the 30-file diff; `build.rs` unchanged. Records `supply-chain/reviews/rustix-1.1.6-2026-11-11.{md,json}`, exact exemption, source policy. `cargo metadata --locked` ok; `dependency-audit.py` passes all checks. Neo (macOS, libc backend): `dev-check.py -p airk-store` pass 29 s, evidence `~/airk-evidence/split/rustix-1.1.6-airk-store.log`, target cleaned. Linux `linux_raw` backend not compiled locally (CI or Docker later). Commit `596d6308`. |
| Unused `Child` import (U81) | done | Not removable: used by a Unix fixture and by local-inference tests through `super::*`. Import gated `cfg(all(test, any(unix, feature = "local-inference")))`. `dev-check.py -p airk-cli` pass without features (48 s) and with `local-inference` (17 s). Commit `ac948385`. |
| C3a/C3b I1 fixtures | done | Delegated source work, reviewed by the primary: `fixtures/split/i1/{request_recovery,window_plane,admin_launch_code}.{py,json}` (21/18/11 cases, generators `--check` 0) and `scripts/tests/test_split_i1_fixtures.py` 17/17. `contracts.md` §2.4/§2.5/§2.7 decisions marked [P]: registry wire spellings `KeyConflict`/`KeyExpired` kept (no D8 change), M5 check order and new codes, new keys only after a definitive M5 refusal, recovery by nonce after client death, W4 selection without a catalog grant id (in-memory per U77), window/lease/queue/handoff codes, launch code format (base64url 43, SHA-256 stored, 60 s). Not yet in container inputs (no Rust consumer). Session frames `scope_select`, `scope_release`, `assistance_ack`, `handoff_publish` join the `airkctl` contract next. |
| C7 receipt schemas | done | Delegated source work, reviewed by the primary: `scripts/split_receipts.py` (pure validators, typed refusal codes) and `fixtures/split/receipts/*.json` (10 schemas, 22 valid and 176 refused cases); `test_split_receipts.py` 8/8, also showing that pre-split decoders refuse every split schema and split validators refuse every pre-split one. `test_container_inputs.py` 4/4, `test_ci_plan.py` 15/15, `test_container_runtime_build.py` 13/13. `contracts.md` §9 owner row now names `split_receipts.py`. Schema decisions not in the contract (names `airk-extension-candidate/2` and `airk-release-catalog/1`, field shapes, the local-provenance key ban) are listed in the agent report and the module docstrings; consumers adopt them in Phases 5 and 7. No macOS local installer receipt yet (follow-up). |
| Session window frames | done | `airkctl` session protocol 2 gains `scope_select`/`scope_release`/`assistance_ack`/`handoff_publish` and replies `scope`/`scope_released`/`acknowledged`/`handoff`/`refused`, matching C3. Fixture `airkctl_session_frames.json` +4 requests, +4 invalid, +6 responses (file re-rendered with uniform indentation). The no-secret-key test now checks only valid frames and whole key words (`scope_release` is not `lease`). Schema and `i2.ts` regenerated; `cargo test -p airkctl --locked` 10/10 (`c3-frames-tests`); Clippy pass; `tsc --noEmit` pass; `contract_types.py i2 --check` ok. `contracts.md` §3.7 rows added. |
| C5 I3 admin contract | done | Delegated source work (agent ran no builds), reviewed and validated by the primary. Leaf module `crates/airk-cli/src/admin_contract/` (+ `CONTRACT.md`; serde/serde_json/schemars only; not in any module cycle): route allowlist, pure `admit()` with injected same-user peer proof and clock (§4.7 + H1/M2), launch-code exchange, cookie rules, response headers (CSP with Trusted Types), envelopes and codes, operation-table schema and validator, never-over-HTTP list, redaction lists. Generator `fixtures/split/i3/admission_cases.py` → 5 fixtures (90 admission and 55 path cases, 21 exchange entries, 26 envelope entries, 90 operation-table entries). Schema `packages/contracts-gen/i3.schema.json` (explicit write), `i3.ts` rendered and strict-`tsc` clean; `check-repo.ps1` checks i3. T0: `cargo test -p airk-cli --lib admin_contract` 14/14 (`c5-tests`); Clippy `airk-cli` lightweight 34 s and `local-inference` 35 s pass; boundary check reports nothing new. Primary correction: `service.restart` admitted on the admin surface (plan §7.3, P0 decision; Q4 closed). Size: about 2,300 hand-written lines (generator 725) — over the 1,500 step guide for a contract step; recorded, not split. Open numbers (rate limits, admin semaphore), the Rust operation-table coverage test, `AdminEndSessions` and the `AdminScopeGrant` I1 additions wait for Phase 5. **Correction:** `test_supply_chain.py` expected 8 first-party packages; C4a made it 9 (`airkctl`), now updated. Full `python -m unittest discover -s scripts/tests`: 987 tests OK (11 skipped). Routine change: the full Python suite now runs at every handback. |
| C9 UI view-state contract | done | `fixtures/split/ui/view_messages.json` classifies all 101 `WEBVIEW_MESSAGES` types (25 shared UI, 34 editor-only with their capability, 42 admin) and `capabilities.json` lists the `ViewHost` capability flags (both shells: clipboard, external links, scope select; ten editor-only). `packages/vscode/src/test/viewMessageSplit.test.ts` pins completeness, counts, capabilities and that `manage.*`, `usage.*` and non-scope `indexing.*` never stay in the editor. T0 4/4 (`c9-ext`); full mocked extension suite 1,517 selected, 1,496 pass, 0 fail (`c9-ext-full`, 30 s). The `packages/ui` package and the boundary import check are created in Phase 5 when shared views move. |
| Phase 1 exit | done | Contracts landed: C10, C1, C1b, C8, C4a, C4b, C6, C7, C3a/b (+ session window frames), C5 (after the U38/U55 review, U83), C9. C2 variants land per Phase 3 package; C11a lands immediately before 3d. No user stop (U45). Next: Phase 2 (G1–G6 including the two-cut G5). |
| G1 `ipc` module | done | Committed script `scripts/dev/split_g1_ipc_module.py`: `local_ipc*.rs` → `ipc/endpoint.rs` + `ipc/endpoint/{windows,unix}.rs` + `ipc/endpoint/windows/{job,path_tests}.rs`, `service_ipc.rs` → `ipc/frame.rs`; platform files are plain `cfg` submodules aliased to `platform` (no `#[path]` left; the two module-honesty exceptions deleted); `lib.rs` re-exports the old paths. `ipc/CONTRACT.md` added. Identities (`-p airk-cli --lib`, local-inference): 990 before/after; exactly the 12 mapped tests renamed, 0 outcome changes. `cargo test … ipc::` 12/12; Clippy lightweight and local-inference pass; goldens 408/408 equal; boundary: honesty and edges pass, cycles unchanged (pre-existing). A history-doc link to the old files was updated (`6659b42b`). **Neo (macOS):** first run failed on a pre-existing Unix-only dead-code lint, `native_process::attach_owned` (used in production only on Windows, in tests everywhere); gated `cfg(any(windows, test))` per U81 (`6f8d558c`); Windows Clippy both configs pass; Neo `dev-check.py -p airk-cli` pass 83 s, evidence `~/airk-evidence/split/g1-airk-cli-clippy-r2.log`, target cleaned. **Process correction:** check-repo was piped through `tail`, hiding its exit status, and a commit went ahead after it failed; checks are now run with their exit status checked directly. |
| G1b (part 1) lease → root | done | `ipc::endpoint::{prepare_control_endpoint, retire_endpoint_namespace}` take the lease root; callers `service_runtime.rs:530` and `clean_break_cleanup.rs:601` pass `lease.root()` (ownership proof stays at the call site); `ipc` no longer names `airk-store` (`0624cacd`). Windows Clippy lightweight and local-inference pass. Neo: Clippy `-p airk-cli` pass 82 s; `cargo test -p airk-cli --lib -- ipc:: clean_break` 58/58 on macOS, evidence `~/airk-evidence/split/g1b-neo*.log`, target cleaned. Remaining G1b: move Windows named-job security (embedding-pool only) and the `q-`/`e-` predecessor child-role policy out of `ipc`. |
| G1b (part 2) deferred | decided | The remaining `ipc` pieces (Windows named-job security `ipc/endpoint/windows/job.rs`, used only by the paired embedding pool; `q-`/`e-` predecessor child-endpoint policy) serve code already marked for deletion with the process roles (step H10). Moving them first would be churn. G6 prerequisite: delete them with their owners or move them before `airk-ipc` is extracted. |
| G2 `ipc::control` | done (Windows) | Committed script `scripts/dev/split_g2_ipc_control.py` moves `registry_busy_retry` and `request_once` verbatim to `ipc/control.rs` (`pub(crate)`); the baseline-gate policy and the `request*` wrappers stay in `service_runtime`, which imports them. Clippy lightweight and local-inference pass; `cargo test … -- service_runtime:: ipc::` 26/26; identities vs post-G1: 0 added, 0 removed, 0 changed; goldens 408/408; boundary: `ipc` in no cycle. |
| G3/G4 scope | decided | Under Option A (P0 decisions) `airkctl` reaches the service only through I1, so it needs only the pure control transport that G2 moved. The client-side baseline gate (`require_baseline_control`) is a pre-split install concept and stays with the pre-split clients in `airk-cli` until Phase 4 retires them; `NativeReadFailure`, the read-budget encoder and a fresh-incarnation helper move only when an `airkctl` handler needs them (Phase 3). Not needed for the boundary gate or for extraction. |
| G5 worker hooks (boundary gate green) | done | Committed script `scripts/dev/split_g5_worker_hooks.py`: `project_worker::Hooks` (runtime bundle, runtime work, rebuild observation, reference-refresh executor as plain function pointers) built by `runtime_metadata::project_worker_hooks()` and passed from `service_runtime` through `Supervisor::start` → `catalog::execute`/`start_worker` (`WorkerOwnerContext.hooks`) → `ChildWorker::spawn` → `start_in_process` → `run_in_process` → `project_worker::serve`; the child-process role uses a crate-root `run_project_worker()`. Same functions called, so behavior unchanged. First compile batch found 4 more `WorkerOwnerContext` sites in `project_supervisor/catalog.rs`, one test site, and `too_many_arguments` on `Supervisor::start` (8/7); the script was completed and reapplied from a clean tree; the lint uses the crate's existing targeted, commented allow (precedent in `catalog.rs`, `main.rs`, `project_client.rs`). **`boundary_check.py` passes all gates** (module cycles failed since `fe607425`); `--write-baseline` adds only `"airkctl": []`. Clippy lightweight and local-inference pass; lib tests `project_supervisor:: project_worker:: runtime_metadata reference_refresh service_runtime::` 88 selected, 80 pass, 8 `#[ignore]`; identities vs post-G2: 0/0/0; goldens 408/408; integration `tests/tools.rs::tool_calls_preserve_evidence_scope_errors_and_argument_budgets` (live service, `airk_diagnostics` through the hooks) pass. Neo (macOS): Clippy `-p airk-cli` pass 82 s; the same lib selection 74 pass, 1 ignored; evidence `~/airk-evidence/split/g5-neo*.log`; target cleaned (`boundary_check.py` needs Python 3.11 and runs on Windows only). |
| G6 extract `airk-ipc` | done | Committed script `scripts/dev/split_g6_extract_airk_ipc.py`: `git mv` of `ipc/{endpoint.rs, endpoint/, frame.rs, control.rs, CONTRACT.md}` into `crates/airk-ipc`; one in-crate path fixed (`crate::service_ipc` → `crate::frame`); the 11 items `airk-cli` calls widened `pub(crate)` → `pub`; `local-inference` gates removed (feature absent in the new crate); `airk-cli` re-exports `pub use airk_ipc as ipc` so no caller changes; workspace and `airk-cli` dependency; edge `airk-ipc → airk-core`. **Criterion §10.1.4:** `cargo test -p airk-ipc` builds and runs alone, 12 selected, pass, 31 s. Clippy `-p airk-cli -p airk-ipc` lightweight and local-inference pass; identities: the 12 tests left `airk-cli` and reappear unchanged in `airk-ipc`, nothing else changed; `service_runtime::` 14/14; goldens 408/408; boundary all gates pass. Supply chain: first-party roster + test count 10; `dependency-audit.py` all checks pass; `license-notices.py --release` 431; `release-version.py check` 1.8.23 (the `airk-` prefix covers the crate). Full Python suite 987 OK. Contract and `docs/architecture.md` updated; legacy job security and predecessor cleanup documented for deletion at H10. Neo (macOS): `cargo test -p airk-ipc` alone 7 pass (Unix subset) in 18 s; Clippy `-p airk-cli -p airk-ipc` pass 83 s; evidence `~/airk-evidence/split/g6-neo*.log`; target cleaned. |
| Phase 2 exit | done | `airk-ipc` extracted (U27 edges: `airk-ipc → airk-core`); the boundary gate passes all checks for the first time since `fe607425`; G3/G4 not needed under Option A; two legacy `airk-ipc` pieces wait for H10. No user stop (U45). Next: Phase 3. |
| 3.0 role admission (`service-detached`) | done | Core: `install_record::StartRequest` (`airk-start-request/1`, ≤ 30 s, canonical, injected clock) + 9 independent cases in `i4_cases.py`. `airk-cli::role_admission`: `__airk_role <name>` parsed before clap; bare/unknown → `role_admission_refused`; `service-login`, `pdf-child`, `setup` → `role_not_available` until their steps; `service-detached --runtime-dir <abs> --start-request <nonce>` renames `<control>/start/<nonce>.json` away (single use), checks owner-private (Windows ACL via `require_owner_private`; Unix uid, one link, mode `0600`), decodes, deletes even on refusal, clears the saved Stop in `explicit` mode through `ServiceRegistry::with_existing_lifecycle`, then runs the unchanged `serve`. Reviewed table `fixtures/split/i4/role_admission.json` (container input registered). T0: `install_record` + `role_admission` 12 selected, pass; Clippy `-p airk-cli -p airk-core` both configs pass; boundary passes; goldens 408/408. **End-to-end smoke** (built `airk`, private fixture root): bare and pending selectors refused; a valid explicit request started a real service (`service health` ok), was consumed, its replay refused, `service shutdown` 0, role exit 0. The smoke first failed because the script wrote CRLF on Windows; the role correctly refused the non-canonical bytes and consumed them. Neo (macOS): Clippy pass 84 s; `role_admission` 7/7 (incl. the Unix `0644` refusal) and `install_record` 6/6; evidence `~/airk-evidence/split/r30-neo*.log`; target cleaned. |
| 3.1 differential harness | done | `scripts/dev/cli_golden.py differential --old --new --cases [--out]` (P0.3b): one fresh owner-only fixture per run, a live fixture service started with the old binary's `serve` and observed through `service health` under a deadline (unless `"service": false`), each case run once per binary, per-binary path normalization, reviewed `replace` renames applied to the old side, exact exit/stdout/stderr comparison, optional JSON report. Unit tests `test_cli_golden.py` 14/14 (4 new). Live self-differential with the built `airk` against a real fixture service (`tools list --json`, `projects` RPC on the empty catalog): 2 cases equal; service stopped cleanly. The `airkctl` edges to `airk-ipc`/`airk-process` are added with the first I1 call (3a.1), not before. |
| 3a.1 M2 `ToolsCatalog` + `airkctl tools list/help` | done | **Contract step** (`b22c3c71`): `ControlMethod::ToolsCatalog { detail: List{schemas} \| Help{tool} }` → `ControlResponse::ToolsCatalog { document: String }`. Source review before compiling found two contract facts: the complete catalog is 85 KB compact (138 KB pretty), above the 64 KiB default frame, so `ControlMethod::response_frame_budget()` applies the contracts §2.8 budget (M2 1 MiB) to both the service writer and the `airk-ipc` reader; and `serde_json` here is built without `preserve_order` (the only `preserve_order` is tree-sitter's build dependency), so a `Value` round trip would sort the help document's struct field order. The document is therefore the exact pretty JSON text the old CLI printed. Reviewed fixture (now `fixtures/split/i1/control_methods.json`, group `tools_catalog`) (bytes, budgets, six refused methods, two refused responses; container input); the retiring console relay refuses M2 (`console_method_not_reviewed`, the contracts §2.3 code). T0 `m2-contract-t0`: 26 selected, pass. **Handler step:** `airk_cli::tool_catalog` wraps `airk_mcp::unified::tools()`; `selects()` (exact name or after `airk_`) is now shared by the old `tools help` and M2; unknown → `unknown_tool`. `airkctl` gains the `airk-ipc` edge (no new third-party identity; tokio already present) and serves `tools list|help [--json] --runtime-dir <dir>` (explicit flag until the I4 pointer, step 3b); text is rendered from the document exactly as before with `airk tools` guidance renamed; service missing → exit 3, unknown tool → 1 with the native failure kept, other subcommands → 5, argument errors → 2. Evidence: Clippy `-p airk-core -p airk-ipc -p airk-cli` (both configs) and `-p airkctl -p airk-cli` pass; **differential** `fixtures/cli-golden/differential/tools.json` (list text/JSON, help text/JSON for all 29 tools, prefixed names; reviewed rename `airk tools ` → `airkctl tools `) against a live fixture service: 62/62 equal, no nonzero exit, list JSON 137,922 bytes (`m2-differential.json`); unknown-tool and service-down refusals checked live; goldens 408/408 unchanged; boundary gate passes with the new edge; dependency audit passes; Python 992 OK (11 skipped). T0 `m2-handler-t0` first selected `binary(tools)` under `--no-default-features` and 2 of its integration tests failed in fixture setup (`runtime self-test`: "single executable runtime requires embedded inference"): a test-configuration error, not this change; rerun with `local-inference` (`m2-handler-tools-li-t0`): 15/15 pass; the other 30 selected in the first run passed. Primary review found one defect: an unserved `tools` subcommand with options (`tools call x --args …`) answered a usage error (2) instead of `not_yet_served` (5); fixed by checking the subcommand first, then `airkctl` tests 14/14 and the differential 62/62 rerun (`m2-airkctl-final-t0`). Capability `tools/1` is added to `airkctl version` with `tools call` (3a.2). Neo not run: no Unix-only source changed; batched with 3a.2. |
| 3a.2a M3 contract | done | Investigation (one read-only agent, primary-verified at the cited lines): with `--service-runtime-dir` the old CLI takes `--service-project <binding>` and never resolves root/data (`main.rs:1699`), so M1 is not needed for 3a; evidence reads in `tests/` use `--catalog-session` on the reviewed fixture; HTTPS already makes loopback control requests from inside the service (`LazyBackend` → `project_client`) under checkpoint admission, so M3 can run the CLI's own path; the CLI inserts `response_mode` from `AIRK_MCP_RESPONSE_MODE` and errors print `operation_failed` with the full chain ≤ 4,096 characters (`main.rs:1256`). Contract: `ToolsCall { tool, arguments, selection: Project{binding} \| Catalog{request}, response_mode, usage_client }` → `ToolResult { result }` or `ToolCallFailed { message }`; `ControlMethod::request_frame_budget()` (M3 128 KiB: 64 KiB arguments as the CLI bounded them plus selection and re-encoding room); the service reads up to `MAX_CONTROL_REQUEST_BYTES` with `read_message_sized` and drops a frame over its method's budget unanswered, as before; the client writes with the request budget and waits 135 s for M3 (125 s call context). **Amendment:** M3 response 4 MiB, not 16 MiB (frame reader cap 4 MiB, public envelope 1 MiB, HTTPS limit ≤ 4 MiB); `read_budget` dropped from M3 because the service-side `project_client` keeps the CLI's per-exchange budget. Fixture group `tools_call` (3 methods, 9 refused methods, 2 responses, 3 refused responses) and request budgets pinned for every method case. `not_yet_served` arm; console relay refuses. Evidence: Clippy `-p airk-core -p airk-ipc -p airk-cli` default and `-p airkctl -p airk-cli` with `local-inference` pass; T0 `m3-contract-t0` (`airk-ipc`, `airkctl`, fixtures, console, tool catalog, `tests/tools.rs`, `tests/service_runtime.rs`): 91 selected, pass. |
| 3a.2b M3 handler | done | `airk_cli::tool_call` holds the CLI's call path, moved from the binary and now shared: `project_backend` (was `tools::public_project_backend`), `call` (response-mode insertion + `remote::call_observed`, was `RemoteSession::call`), `require_envelope`, and the in-process help rule `is_local_help`/`local_help`. `service_runtime::tools_call` answers M3: help without a catalog session is answered in process before any resolution (as `main.rs:1536`); otherwise `resolve_control` (`Project`) or `resolve_scoped` (`Catalog`) exactly as `--service-runtime-dir` did, `project_client::tools`/`scoped_tools` with the requested usage client, the shared call, then explicit `close`/`shutdown` (a one-shot CLI released its connection by exiting); any error → `ToolCallFailed` with the CLI's message. **Findings during the step:** (1) the end-to-end test caught a real parity gap: with `--service-project` the CLI answers `help` in process, not through the backend; fixed by sharing the rule, and the contract gained `ToolSelection::Unselected` (help only; another tool → `project binding required`) because `airk tools call help` needs no project (fixture case added). (2) The boundary gate refused a top-level `tool_call` that resolves projects: it joined the client/runtime module cycle. The baseline was not loosened: the pure path stays in `tool_call` (depends only on `airk-mcp`) and the handler lives under `service_runtime`, which already uses those modules. Evidence: Clippy `-p airk-cli` both configs, `-p airk-core` pass; boundary passes; old CLI goldens 408/408 (`m3-goldens-airk.json`); T0 `m3-handler-t0c` (`tests/tools.rs` incl. the new `control_tool_calls_answer_what_the_cli_printed`: six tools under `Catalog` and `Project` plus `Unselected` help equal to the CLI after the CLI/MCP parity tests' observation-field normalization; unit tests for response mode, envelope, message bound, unresolvable and unselected selections; the binary's parity tests; fixtures): 26 selected, pass. Not run in this step: `tests/mcp.rs` (exercises the moved `RemoteSession::call` through the CLI; behavior-identical move) — included in the 3a handback batch. |
| 3a.2c `airkctl tools call` + evidence reads | done | `airkctl` now parses with Clap (`clap`, `anyhow`, `tempfile` dev: already in the graph; dependency audit passes) and serves `tools list|help|call` and the fifteen evidence reads with the global `--service-runtime-dir`, `--service-project` and `--catalog-session` (old names; `tools list|help` moved from 3a.1's `--runtime-dir`, differential case file updated). The fifteen Clap variants, their option structs, `SearchMode` and `query()` are copied verbatim from `airk` by the committed `scripts/dev/split_3a2c_port_evidence.py` into `crates/airkctl/src/evidence.rs` (`#[rustfmt::skip]`, byte-exact; reviewed EDITS only rename the enum, drop two unreachable catch-alls and make the types public); `scripts/tests/test_split_port_evidence.py` fails on drift without a Rust toolchain. Command execution lives in `airkctl::cli::run` with injected streams; `main` is a thin wrapper. Output follows the CLI: the complete envelope as pretty JSON, a tool error then exit 1 (`MCP tool failed; see the result above`, or `AIRK help request failed` for in-process help), `ToolCallFailed` → exit 1 with the CLI's message; unserved subcommands → 5, argument errors → 2 (I2 classes), no `--service-runtime-dir` → 3. `airkctl version` lists `tools/1`. **Finding:** the first parity run used a stale `debug/airkctl.exe`: Cargo builds a package's plain executable only for its own integration tests, and `airkctl` has none, so a sibling-binary test could silently run old code (in CI too). Corrected by testing the command line in process through a test-only `airk-cli` → `airkctl` dev-dependency (the boundary gate counts normal edges only); the process-level path stays covered by the differential. Two further failures were my test inputs (`symbol`/`context` take positionals, `read` requires its line range); every case was then validated against the parser before the next run. Evidence: Clippy `-p airkctl -p airk-cli` both configs pass; T0 `ctl-reads-t0e`: 39 selected, pass, including `airkctl_reads_print_what_airk_printed` (20 command lines — all fifteen reads, a tool error, `tools call` with `--args`/`--args-file`, help — each equal under `--catalog-session` and `--service-project`: exit, normalized stdout, final message) and the in-process I2 class tests; differential `ctl-differential.json` 62/62 with fresh binaries; boundary passes; Python 995 OK (11 skipped); repository checks pass. Old CLI goldens not rerun: `airk` sources unchanged in this step. |
| 3a.2d contract (combined) | done | Source review: with `--combined-session` the old CLI lists and helps from the session's catalog (`Unified::combined` → `airk_mcp::combined::public_catalog`, not the unified one) after `combined_frontend::resolve` validates and snapshots the session, never answers help in process, and adds `--project-ids`/`--index-ids`/`--member-availability` to every call's arguments with a `conflicting {key} selections` refusal (`tools::run_backend`). Contract: M2 gains `combined?: CombinedConnectRequest` (absent keeps every existing byte) and M3 `ToolSelection::Combined { request, project_ids?, index_ids?, member_availability? }`; optional fields refuse an explicit `null` (`combined_present`, as the request itself does). Until the handler both answer `not_yet_served`. Fixture: 1 M2 and 2 M3 methods, 5 refused methods. `tools shell` moves to 3c: its header prints the project's collection, workspace and storage, which only M1 resolution provides. Evidence: Clippy `-p airk-core -p airk-cli -p airkctl` both configs pass; T0 `combined-contract-t0`: 44 selected, pass. |
| 3a.2d combined handlers | done | `service_runtime::tools_call` serves both: M2 `combined` resolves the session (`combined_frontend::resolve`), lists from `Unified::combined`'s catalog through the shared `tool_catalog::response_from`, then releases it; M3 `Combined` resolves, adds the defaults through `tool_call::with_defaults` (moved from the binary's `run_backend`, now shared; `conflicting {key} selections`), runs the shared call and releases the session. Help is answered in process only for `Unselected`/`Project`; a combined resolution failure answers `ToolCallFailed` for M2 as for M3. `airkctl` gains `--combined-session`, `--project-ids`, `--index-ids`, `--member-availability` with the old Clap relationships, and the old order of `parse --project` refusals (catalog, combined, then conflicting binding). Evidence: Clippy `-p airk-cli -p airkctl` both configs pass; T0 `combined-t0g`: 46 selected, pass, including `airkctl_combined_reads_print_what_airk_printed` on the owned combined fixture — 12 command lines (list text/JSON, help, `tools call` incl. help through the backend, search with and without `--project-ids`/`--index-ids`/`--member-availability require`, files, status, an ambiguous `info`, a conflicting `project_ids` argument) equal in exit, stdout and final message after the file's own authenticated logical reconstruction (`logical_comparison_copy`) and observation-field normalization; the three refusals (`files` across the combined session and the ambiguous `info` are tool errors; the conflict) are pinned. While building it: the list text needed the differential's reviewed `airk tools ` → `airkctl tools ` rename, and a positional header timestamp needed the logical reconstruction instead of field stripping (both test-side; no production change). Old CLI goldens 408/408 (`combined-goldens-airk.json`); differential 62/62; boundary passes; repository checks pass. **Known difference:** after a successful call the old process exited 1 if releasing the combined session then failed; the service returns the result (a cleanup failure never replaces a result). |
| 3a handback | done | Windows T1 `handback-3a-t1`: `tests/mcp.rs`, `tests/combined_reads.rs`, `tests/tools.rs`, `tests/service_runtime.rs` and `airkctl` in full, `local-inference`: 132 selected, 132 passed (2 skipped by the default filter), 1,076 s, inputs unchanged. Neo macOS (`5c11966c`): Clippy `-p airk-ipc -p airkctl -p airk-cli` pass 82 s; `cargo test -p airk-ipc -p airkctl` 8/8 and 19/19, `airk-cli` lib `tool_call|tools_call|tool_catalog` 12/12 (the first attempt used `cargo nextest`, which the Neo does not have; rerun with `cargo test`); evidence `~/airk-evidence/split/r3a-neo-*.log`; target cleaned. **3b.1 decision (U84, user):** `airkctl`'s native pieces move into existing crates (owner check → `airk-ipc`; folder lookup and detached spawn → `airk-process`); 3b.1 decomposed into 3b.1a–f. |
| 3b.1a `service health`/`shutdown` | done | Source review (one read-only agent, primary-verified): `airk service health --runtime-dir` sends `Health` through the console client (same version and engine digest, else no output and exit 1) and prints the compact response; `service shutdown` repeats that check, sends `Shutdown` fenced to the observed instance and prints the compact response, refusing a failure with `native request refused: {code}`. `airkctl service health|shutdown [--runtime-dir]` (default: the global `--service-runtime-dir`) does the same over I1 with the pair check it can make before the install record — version and protocol (the `airk-ipc` identity refusal now maps to exit 4, `service_rebind_required`); the executable-digest check joins with the I4 reader (3b.1c). A transport failure after `Shutdown` was sent is exit 6 `service_request_outcome_unknown`, never retried. Other `service` subcommands stay `not_yet_served`. Evidence: Clippy `-p airkctl -p airk-cli` both configs pass; T0 `svc-health-t0`: 20 selected, pass, including `airkctl_service_health_and_shutdown_print_what_airk_printed` on a live service (health equal to `airk`'s apart from `uptime_ms`, with `--runtime-dir` and with the global flag; `airk` shutdown, then `airkctl` health exits 3; restart, `airkctl` shutdown prints `{"status":"stopping"}` and the service exits cleanly). |
| 3b.1b U84 moves | done | Structural, no behavior change. (1) The Unix owner-private file check moved from `role_admission` into `airk-ipc` (`endpoint::require_owner_private`, now exported on both platforms); `role_admission` keeps one wrapper adding its refusal context, as its Windows path already did (on Unix an I/O error now also carries that context). (2) `scripts/dev/split_u84b_user_folders.py` moved `checked`, `known_folder` (behind a `KnownFolder` enum) and the three `home` variants into `airk_process::user_folders`; `user_paths.rs` forwards `checked` and `home`, so callers and tests are unchanged (`airk-process` gains `Win32_System_Com`, `Win32_UI_Shell`). (3) `scripts/dev/split_u84c_detached_spawn.py` moved `without_error_dialogs`, `DetachedChild`, `windows_argument` and `spawn_detached` into `airk_process::detached` (re-exported by `native_process`) and the inline Unix `setsid` block of the on-demand start into `detached::new_session` (`Win32_System_Diagnostics_Debug`); the self-spawn roster entry is re-keyed to `crates/airk-process/src/detached.rs` (same kind and reason). Both scripts refuse to run twice and were re-run from a clean state. Evidence: Clippy `-p airk-process -p airk-ipc -p airk-cli` both configs pass; T0 `u84-moves-t0` (native process incl. detached fixtures, user paths, role admission, `airk-ipc`, `airk-process`, live `service health/shutdown`): 48 selected, pass; old CLI goldens 408/408 (`u84-goldens-airk.json`); boundary gate, dependency audit and the self-spawn roster pass. Neo macOS (`500b6a42`): Clippy `-p airk-process -p airk-ipc -p airk-cli` pass 82 s; `cargo test -p airk-process -p airk-ipc` pass; `airk-cli` lib `role_admission|user_paths|native_process` 14/14 (incl. the Unix `0644` start-request refusal through the moved check and the passwd home); evidence `~/airk-evidence/split/u84-neo-*.log`; target cleaned. |
| P0 decisions | recorded | User: U72–U78. Primary (within existing authority): every management operation gets an `airkctl` command and `airkctl service rpc` stays (U6, parity); foreground/container entries become private roles; explicit directory flags stay as user-supplied CLI arguments (the extension never passes them); restart = `airkctl` observes exit then starts; Explorer handoffs flow through the window session; `setup --launch` becomes `airkctl agents launch`; `verifyMcp` becomes `airkctl agents verify`; the service creates Genesis on first start; the listener serves `/admin` even without a catalog gate; Option A (query client stays in the service; `airkctl` uses I1 methods); process-start primitives move into `airk-process`; retirement supersedes old journals and uses them only as ownership evidence; an unproven old service still running → "installed, not started"; the split extension clears its own `environmentVariableCollection` entries; new Inno AppId. |
| P0.6 Contract drafts | drafted | `docs/split/contracts.md` (898 lines): C1–C11 as 13 ordered commits; 26 I1 methods (M1–M23, N1 scoped NativeRead, V1 `agents verify`) each wrapping a cited function, with explicit `not_yet_served` arms; journal format byte-identical, writer moves to the service, recovery on existing registry key rules (`jobs.rs:636-729`); window plane W1–W8 and pipe-only admin launch code; I2 exit codes 0–8, 9-row compatibility matrix, session protocol 2 with bounded events; I3 flagged for the security review; I4 pointer (written last, fixed paths ignoring overrides), 10-phase transaction, role admission (PDF child gains a handshake). User: U79 (`--output-file`), U80 (abandon-lifecycle service-only). Primary: offline OS adapters (trust, login, issuer rotation) move into `airk-process`; admin Restart uses the OS login manager when registered, otherwise shows `airkctl service restart`; window and admin grants are in-memory, session-bound (no `airk-state/1` change); redirected `LocalAppData` discovery is settled in C6 review. |
| U61 deletion review | done | `docs/split/inventory/deletion-review.md`: 118 rows — 17 confirmed, 94 conditional on named later steps, 7 rejected (live duties). 12 inventory corrections listed in its §5. |
| Phase 0 exit | done | All P0 outputs reviewed by the primary at summary level; deletion rows independently reviewed (U61). Open technical items carried into Phase 1: C1–C11 per `contracts.md`; G5 two-cut cycle correction in Phase 2; P0.3b differential live capture and Linux capture in Phase 3. No user stop (U45). |
| S0.5 Neo readiness | done | `feat/split` pushed to origin (U44). Neo `~/code/airk` was clean on `feat/finetuning`; switched to `feat/split` at `850c2a0f` (U68); `~/.airk-signing/keychain-pass` present, mode 600. rustc 1.98.0; system `python3` is 3.9.6, which runs `scripts/dev-check.py` but not the 3.11+ packaging suites (use Windows or Docker for those). `python3 scripts/dev-check.py -p airk-core --incremental off`: pass, 17 s; log and Cargo timing report kept in `~/airk-evidence/split/`; `cargo clean` afterwards (278 GiB free). |

## Reported HTTP cleanup refusal (10 November 2026)

Acceptance: correct the reported `HTTP cleanup requires the exact prebaseline
predecessor or its retained reset snapshot` refusal for replacement of an
initialized modern installation. Preserve exact selected/predecessor identity,
disposal-only authority, unavailable-metadata reporting and read-only lookup.
The repair boundary is native HTTP disposal lookup and its existing regressions;
no installed-editor journey, live installation mutation or publication is planned.
Initial review checkpoint: 15 minutes.

Observed: the current editor AIRK log
`20261110T144528/window1/exthost/output_logging_20261110T221444/5-AIRK.log`
records the exact automatic-installation failure with elapsed 90 ms. Installed
runtime inspection is unavailable to this sandbox (access denied); its exact
version and authority have not been established. AIRK MCP tools are absent and
the workspace Codex declaration is disabled. Local source navigation is used.

Source-review finding: `captureReplacementHostArtifacts` queries disposal proof
for the selected installed runtime before native replacement. Native lookup
accepts a retained exact reset snapshot, otherwise requires both selected
predecessor equality and `state_contract::required`. The latter is false for an
initialized modern runtime, so this caller composition is rejected despite
having exact current predecessor identity. This is a source-supported cause,
not yet a reproduced defect. Missing snapshots must not be inferred as deleted.

Validation plan: batch modern active/expired/revoked ownership, crossed identity
and unavailable-ledger regressions in the existing native test owner; run one
scoped diagnostic batch before production changes. Review the complete minimal
correction, then format, run scoped CLI Clippy with Cargo timings and the affected
native regressions using persistent `artifacts/v190/p10-target`, incremental on.
Record selected counts and receipts. Full workspace, platform, runtime/VSIX and
installed acceptance remain separate unrun gates unless explicitly undertaken.

First diagnostic attempt (`artifacts/one-process/http-cleanup-diagnostic-r1.json`)
did not execute tests: all three link targets lacked the static ORT handoff
(`LNK1181`, placeholder ORT library). Inputs were unchanged; elapsed 31.71 s,
incremental on, persistent P10 target and Cargo timing retained. This is a
validation-environment failure, not a reproduction. The existing runner's
`--static` option validates and applies the retained static-library handoff;
the unchanged diagnostic selection will use that option before any correction.

**Stopped at the user's direction:** apply only the small fix, commit and push;
do not test the fix, build another installer or create a PR. The diagnostic r2
was interrupted: it selected six tests and recorded three expected old-guard
failures and two passing refusal cases before interruption, with no final batch
receipt. This confirms the analyzed native failure class only; it does not
qualify the correction or the user's installed runtime. The provisional test
additions were removed. No test-owned AIRK/Cargo processes remained in the
post-stop process observation; the user's installed service/window session were
left untouched.

The final correction accepts exact selected predecessor equality and separately
retains `state_contract::required(root)?` as saved-state identity validation.
Its Boolean result no longer rejects disposal-only proof for initialized modern
state. All snapshot, endpoint, private-ledger and current-namespace checks remain.
No schema, persisted format or response field changed. No post-fix test, build,
runtime/VSIX package or installed qualification was run. Commit scope is this
small correction and its service-installation documentation; unrelated existing
working-tree changes and the earlier staged release-record rename were preserved
by that first commit (`9d332e7c`).

The user then explicitly requested bundling **all** pending work into
`feat/simplify` and removing the linked Git worktrees. The main checkout stays.
The current 1.8.23 source, extension, scripts, fixtures and documentation,
including the previously authorized Arbiter future concept, are checkpointed
without more tests or builds. Four dirty historical worktrees are preserved as
signed-off source snapshots in branch ancestry while the latest integrated tree
remains authoritative. Clean diagnostic branches retain their original history;
obsolete diagnostic tracing is not restored as production code. Ignored receipts
and failed/private fixtures move to ignored local worktree-retention storage
before worktree removal; none is staged or published. No PR is authorized.

Push destination evidence: origin is `https://github.com/iokaio/airk.git`,
matching Cargo and extension repository metadata. Authenticated GitHub account
`tylerje` reports `ADMIN` permission on the public `iokaio/airk` repository.
This read-only check followed an automatic approval rejection of the initial push
for missing destination-trust evidence.

Historical source preservation (no historical source copied over the integrated
1.8.23 tree):

| Retiring worktree | Signed-off source snapshot | Source paths |
| --- | --- | --- |
| `artifacts/v190/p10-worktree` | `2240086ce2e9e749bbd88b57aacfce7d03a04c14` | 59 |
| `artifacts/v190/p6e-worktree` | `af79e434f25ca93e24ab1735a17c16b0dccb1688` | 13 |
| `artifacts/v190/p6f-worktree` | `22c32f3e501338544c1ec45dd169302cd5885079` | 117 |
| `artifacts/worktrees/one-process-workers` | `d2bf99ca3d99f1e7c23b5d842a8e20a0695611f8` | 28 |

The preservation merge records all four snapshots as parents while retaining
the current integrated source tree. Local raw-byte archives and digest manifests
also retain CRLF-only/stat-only working-copy differences under ignored
`artifacts/worktree-retention/2026-11-10/`. The three clean cold-status worktrees
have no uncommitted work; existing diagnostic branch refs remain intact.
Worktree removal and ignored-evidence relocation receipts are collected there
after source preservation. No build or test is part of this housekeeping.

Removal completed: all seven linked worktrees were removed; only
`D:/code/Github/airk` on `feat/simplify` remains registered. Receipt:
`artifacts/worktree-retention/2026-11-10/retirement.json`. The entire ignored
artifact trees from P10 and one-process-workers, plus their retained Nextest
receipts and P6f's Nextest receipts, were moved locally before removal.
Source archive/digest manifests remain alongside them. Existing diagnostic and
archive branch refs were preserved. No private state, index or artifact was
staged; no test, build, installed repair or PR followed the stop request.

## Manual-test installer correction (10 November 2026)

**Windows debug manual-test installer built; installed-editor acceptance pending.** The user
reported broken agent reconnection, a pre-install PID 1080, an unexpected index
and growing service memory, then authorized fixing the installer and removing
automatic index generation. Installations through **1.9.0 inclusive** must
discard predecessor indexes and wait for explicit index setup. Source folders
remain read-only. Ordinary window activation is not another installation.


### Bounded Windows installer handoff (10 November 2026)

Final VSIX: `artifacts/one-process/bounded-extension-1823-final/publish/airk-1.8.23-win32-x64.vsix`
(86,525,906 bytes). SHA-256:
`9913a08e1fc6323b7dff01b6a26e8857e87f0cddd59e6f8bbeaa234690329b89`.
Final package receipt: `artifacts/one-process/installer-bounded-final-package-r1.json`;
exit zero, unchanged consumed inputs, 21.33 seconds. Only the package README
status changed after the native UI run; no runtime or JavaScript rebuild occurred.

The user set a 30-minute delivery limit. The delivered candidate uses the debug
runtime, not an optimized release runtime. No production installation was changed.
The native ZIP and VSIX package checks passed with unchanged consumed inputs:
`artifacts/one-process/installer-bounded-build-r1.json`. Cargo used the persistent
`artifacts/v190/p10-target`, incremental compilation enabled, with timing receipts;
the final debug runtime compilation took 24.62 seconds. The build sequence also
passed the one-test ordinary-reopen regression and one-test generated CLI reference.

Validation before packaging: scoped native Clippy passed; the native batch selected
54 library tests (53 passed; the remaining ordinary-reopen defect was corrected and
its one-test rerun passed), and both actual CLI lifecycle tests passed. The full
mocked extension batch selected 1,425 tests: 1,404 passed, zero failed, 21 gated skips.
Receipts: `installer-bounded-native-validation-r1.json`,
`installer-bounded-native-tests-r1.json`, `installer-bounded-extension-tests-r1.json`.
The final packaged-runtime native UI selection passed all four tests, with zero
failures or skips, in 73.84 seconds. Unchanged source inputs and runtime seals
are recorded in `installer-bounded-native-ui-r1.json`. Only the README status
changed for final VSIX packaging; the tested runtime and extension JavaScript
were reused unchanged. Process inspection after the tests found only the user's
pre-existing PIDs 1080 and 10188; no test AIRK, Cargo or rustc processes remained.

Full current-source workspace tests, optimized builds, installed-editor acceptance,
macOS/Linux, signing and release publication were not completed for this handoff.
The pre-existing structural boundary failure and earlier failed receipts remain
recorded below; they have not been waived or relabeled as passing. This is an
unpublished Windows manual-test candidate. AGENTS.md and CLAUDE.md now contain the
same bounded investigation, scope and stopping rules and remain byte-identical.

Original observed evidence (collected before tests or production changes):

- Windows process metadata identifies PID 1080 as the 1.8.22 bundle's
  `airk service session`, started at 17:20:29 by VS Code PID 16828 (started
  17:20:18). PID 10188 is the installed `runtime managed-serve`, started
  17:20:58. The session predates the runtime installation and executes native
  installer commands; it is not evidence of an old service surviving cleanup.
- The current editor log,
  `%APPDATA%/Code/logs/20261110T144528/window1/exthost/output_logging_20261110T172019/13-AIRK.log`,
  records removal of 1.8.20, then no registered index (line 11), then
  `Reconnect created index` for this repository (line 23), followed by both
  `reconnect_scope_unconfirmed` failures (lines 24–25). The visible index came
  from Reconnect, not a retained testing index.
- Installed `out/serviceExtension.js` and live source both require
  `catalogSession` in `reconnectScope`. The real Search scope path produces
  `combinedSession`, including singleton selections. Creation precedes that
  refusal and is retained. Existing tests mock away this composition.
- Replacement's same-runtime exemption preserves state during an explicit
  reinstall; the version gate also permits ordinary upgrades at exactly 1.9.0.
  Removing the exemption without distinguishing installation from reconnection
  would incorrectly wipe state on every window activation.
- Replacement captures existing executable files only, missing stale entries
  whose launcher was already removed. It omits custom registered data roots.
  Process-stop errors can disappear if subsequent directory renames succeed.
  These are source-review findings, not reproductions of this installed run.
- A genuinely empty service does not eagerly load inference. Index creation
  and resumed native work can explain memory activity; no memory leak has been
  established by these observations.

Correction invariants and ownership:

- Native installer owner: explicit reinstall admission through 1.9.0;
  authenticated interrupted-install resumption; complete owned data inventory;
  verified predecessor shutdown before removal; historical launcher identities.
- Extension reconnect owner: no index setup from Reconnect; accept reviewed
  singleton/combined scopes without broadening authority; preserve full failure
  information and unrelated configuration bytes.
- Primary: installation/reconnection intent at extension call sites, source and
  documentation integration, independent review of delegated changes, and sole
  ownership of compilation/testing. No commit, push or live installation change
  is requested. A new package must use the next unused patch under the standing
  version instruction.

Validation plan: add sibling regressions together, run one scoped diagnostic
batch before production corrections, complete/review the combined correction,
then freeze inputs for scoped Clippy with timings and affected native/extension
checks. Record selected counts and receipts. Final repository/workspace/package
gates remain required; no earlier installed-editor failure is relabeled as a
pass and no third expensive attempt occurs without resolving its missing cheap
seam first. Preserve existing `airk-state/1` schemas and owned-source protections.

Diagnostic receipts collected before production correction:

- `artifacts/one-process/installer-empty-diagnostic-r1.json`: no tests selected;
  MSVC could not create a temporary linker file under the retained owner-private
  fixture directory with the restricted token. This is a host access failure.
- `installer-empty-diagnostic-r2.json`: **19 selected, 13 passed, 6 failed**;
  all six intended native defects reproduced. Command:
  `cargo test -p airk-cli --no-default-features --features local-inference --locked --timings --lib runtime_install::replacement::tests:: -- --test-threads=1`.
  Persistent target `artifacts/v190/p10-target`, incremental `1`, elapsed
  100.66 seconds, native inputs unchanged. Timing evidence:
  `installer-empty-diagnostic-r2-cargo-timing.html` beside the receipt.
- `installer-empty-ts-diagnostic-r2.json`: **15 selected, 6 passed, 9 failed**;
  unwanted Reconnect index creation, missing installation-operation forwarding,
  missing generation and replay behavior reproduced. Two combined-scope tests
  instead exposed incomplete synthetic fixtures; those failures do not reproduce
  the product defect. Initial restricted-token fixture failures are retained in
  `installer-empty-ts-diagnostic-r1.json`.
- `installer-empty-scope-diagnostic-r3.json`: **3 selected, 0 passed**. The
  no-index regression reproduced the side effect; the other two still stopped
  at synthetic host binding/context preconditions. A complete fixture state-flow
  review is required before another scope diagnostic, rather than production
  correction based on those precondition failures. No native rebuild is needed
  for a fixture-only change.

Latest user constraint: a same-build installation must complete a full uninstall
first. Reconnection is not reinstallation. Explicit retry retains its operation
identity until setup completes; concurrent windows share a native idempotent
installation operation. Neither Reconnect nor installer startup may create an
index. Explicit Index library setup remains the indexing admission path.

- `installer-empty-scope-diagnostic-r4.json`: **2 selected, 0 passed**;
  the direct reviewed-combined-adapter composition reproduced the scope refusal,
  and the no-index case reproduced index setup mutations. Production Reconnect
  correction followed this reproduction.
- `installer-empty-extension-compile-r1.log`: complete pinned Node 24.15.0
  extension, both webview/assistance typechecks and Explorer bundle compilation
  passed for the coherent extension correction.
- `installer-empty-extension-scoped-r1.json`: **224 selected, 221 passed,
  2 failed, 1 skipped**, 24.88 seconds, frozen extension inputs. Actual scope
  Apply composition, no-index behavior, loader generation, operation forwarding,
  fresh-install generation admission and cleanup resumption passed. The replay
  race fixture retained an old version after simulating completed replacement,
  triggering its unrelated legacy-rebuild mock. An earlier expectation also
  incorrectly treated a second explicit same-build install as a reconnect.
  Both need one reviewed fixture/policy-expectation correction before repeating
  the affected extension batch. No native artifact is consumed by these changes.
- `installer-empty-extension-test-compile-r2.log`: pinned host TypeScript
  compilation passed after the fixture/policy expectation correction; no native
  rebuild and no repeated webview compilation.
- `installer-empty-extension-scoped-r2.json`: **224 selected, 223 passed,
  1 skipped, 0 failed**, 25.02 seconds, extension inputs unchanged.
- `installer-empty-extension-full-r1.json`: **1,400 selected, 1,379 passed,
  21 skipped, 0 failed**, 27.79 seconds, extension inputs unchanged. This is the
  mocked extension suite, not installed/native transport qualification.

Independent native source review is in progress before compilation. It found
additional correction obligations: keep default roots within their owning
channel, defer destructive inventory on ordinary reconnect, preserve consumed
generation identity across standalone reinstalls, protect new successors from
pending cleanup, and allow exact retired-directory cleanup after partial
deletion removes an owner marker. All are source-review findings until their
new regressions run. The existing installer JSON writer already bounds the full
receipt to 512 KiB; the initially suspected unbounded journal write was ruled
out by reading that canonical writer.

Native implementation and primary source review are complete; inputs are now
frozen for the first correction validation. Regressions include the six original
reproductions, replay/generation continuity, no-clobber retirement, partial-delete
recovery, original-control reauthentication, missing registered storage, channel
isolation and actual CLI full replacement/reactivation with an empty successor.
Public destructive replacement requires an operation identity. The new journal
is installation metadata outside removed roots; existing `airk-state/1` formats
are unchanged. Unsupported unproven legacy layouts and older timestamp-named
residue are not silently treated as proven deletion targets.

Next ordered validation: formatting, scoped `airk-cli` Clippy with local-inference
and timings in `artifacts/v190/p10-target` (Windows incremental on), then one
batched affected native selection including actual CLI transport. Expected:
nonzero selected tests, no failures, empty successor catalog and no indexing jobs,
original source bytes preserved. Stop at compiler/check failure and collect the
complete diagnostics before another correction. No optimized build starts until
these checks pass and the combined review is complete.

- Independent final native review: all concrete findings dispositioned. The
  last missing-storage report was stale: live physical-plan construction already
  filters absent roots. A real native regression was added for that boundary.
- `installer-empty-clippy-r1.json`: **failed**, 81.74 seconds, inputs unchanged;
  complete diagnostics identify two `collapsible_if` style errors in the new
  launcher discovery paths, and no other errors. The primary collected both lib
  and lib-test diagnostics, corrected both sites together using let chains,
  reviewed unchanged evaluation order and formatted the owning file. No tests
  were selected because Clippy failed. Timings retained beside the receipt.
- `installer-empty-clippy-r2.json`: **passed**, 34.19 seconds, all-targets
  `airk-cli`, no-default-features + local-inference, Windows incremental `1`,
  persistent P10 target, frozen native/offline inputs. Timings retained.
- `installer-empty-native-scoped-r1.json`: **40 library tests selected,
  33 passed, 7 failed**, 64.04 seconds including compilation. Cargo stopped
  before the CLI integration binary; those transport tests were not run.
  All seven failures share Windows directory rename refusal while the installer
  itself holds write exclusions on child executable files. The full failure
  batch was collected before editing. No T2/T3 or installer build was started.
- Cheap Windows API matrix (no Cargo):
  `artifacts/private/installer-image-exclusion-matrix-r1/receipt.json` and
  `close-before-move-receipt.json`. No-handle directory moves pass; child file
  handles block parent moves regardless of sharing or replacement flags.
  Unlinking the exact owned image under its held exclusion makes the name
  unreachable and permits parent movement while the exclusion remains held.
  This agrees with Microsoft's
  [documented directory rename restriction](https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information).
  The correction must durably record qualified image retirement after frozen
  source/namespace proof, unlink under exclusion, then move/delete directories;
  resume and rollback disclosures must retain that irreversible phase.
- `installer-empty-legacy-loader-diagnostic-r1.json`: **3 selected, 0 passed**.
  Source review found that an already loaded 1.8.22 update loader cannot expose
  the new installation-generation Symbol to corrected hot-loaded code. The
  actual older-host path reproduced an undefined native operation. A bounded
  reader of the installed manifest must share the loader's validator and bind
  the exact product/version before deriving the same generation. Helper API and
  crossed-manifest expectations are new protective cases, not separate observed
  installation failures. No live installation/configuration was changed.
- `installer-empty-compat-extension-compile-r1.log`: full pinned extension and
  webview/assistance compilation passed with the shared bounded compatibility
  reader. `installer-empty-compat-extension-scoped-r1.json`: **229 selected,
  228 passed, 1 skipped, 0 failed**, 21.33 seconds. The real older-host path and
  captured module graph both pass. `installer-empty-compat-extension-full-r1.json`:
  **1,405 selected, 1,384 passed, 21 skipped, 0 failed**, 23.41 seconds. All
  extension inputs stayed frozen; native integration remains a separate gate.
- Additional native source review identified a trust-boundary gap in the newly
  introduced journal: a regular bounded JSON file in a shared custom parent is
  not private deletion authority. The correction uses an adjacent owner-private
  child validated through canonical `local_ipc` checks before each read/write;
  readonly planning never creates it and existing permissive children are refused
  without changing their permissions. This is a source-review finding awaiting
  its native negative regression, not an observed attack or live user-state change.

- `installer-empty-clippy-r3.json`: formatting and all-targets `airk-cli`
  Clippy with local-inference passed, 23.08 seconds, Windows incremental `1`,
  persistent P10 target; native and offline inputs unchanged, timings retained.
- `installer-empty-native-scoped-r2.json`: **44 library tests passed**,
  including private-journal refusal, held-image unlink ordering and interrupted
  recovery. Actual CLI transport selected **2 tests: 1 passed, 1 failed**.
  Total 49.39 seconds, frozen inputs. The failed transport fixture invented
  `runtime stop`, which is not a command; all three stop sites, including its
  result-discarding Drop, share that harness error. Production library evidence
  remains valid. The test-only correction must use canonical instance-fenced
  Lifecycle Stop and retain both owner/source fixtures before activation.
- The failed fixture `.tmpNfRH9B` left test-owned PID 16640 running after its
  destructor partially removed installation metadata. Exact process image,
  creation time, health/image digest and runtime identity were verified before
  recovery. Native `runtime inspect` failed because metadata was already absent,
  so no complete original Lifecycle Stop receipt is claimed. The exact service
  then exited after graceful `service shutdown`; the failed fixture and both
  `manual-cleanup-receipt.json` and `manual-graceful-cleanup.json` remain there.
  User processes 1080 and 10188 were preserved. This is a harness cleanup defect,
  not evidence of predecessor cleanup passing or failing in the new installer.

- `installer-empty-cli-validation-r3.json`: scoped formatting/Clippy passed;
  **2 CLI tests selected, 1 passed, 1 failed**, 65.85 seconds total. The full
  same-build removal, reactivation, new namespace and exact shutdown completed.
  Its final assertion then requested an invalid pending-project page of 64;
  the canonical registry API accepts 1–32. Failed owner/source fixtures remain
  retained after verified Stop (`.tmps0Z4HR`, `.tmpUwVtcu`). Primary audited the
  complete test and cleanup API calls, corrected the request to the supported
  maximum of 32 with the empty-result assertion unchanged, and reuses the
  completed Clippy evidence for this test-only literal correction. The native
  production artifact and 44 passing library cases are unchanged. The generated
  CLI-reference check was not reached by this stop-on-failure batch.

- `installer-empty-cli-validation-r4.json`: **2 actual CLI replacement tests
  passed**, 25.84 seconds of test execution. Full owned same-build removal,
  reactivation into a different empty namespace, no pending indexing projects,
  preserved source bytes and verified final Stop all passed. The following
  generated-reference test failed only on native Clap option ordering. Its
  canonical generator was run using that exact existing test executable; the
  retained `installer-cli-reference-generated.diff` changes option ordering
  only. A fresh readonly run of that same generated-reference test passed
  (**1 selected**) without recompiling or repeating installation tests.
- Standing candidate authorization applied: manifests and lockfiles changed
  **1.8.22 → 1.8.23** immediately before build preparation. Pinned Node 24.15.0
  host compilation, generated native catalog, complete extension compilation
  and `release-version.py check --expected 1.8.23` passed in 22.17 seconds;
  `installer-1823-prepare.log` retains exact commands. No earlier artifact was
  relabeled. Primary reviewed the complete native/extension correction and the
  five native-fixture failure-retention changes before final validation.

Final Windows qualification ladder (frozen inputs; primary is sole Cargo owner):

1. `installer-1823-final-t1-r1`: workspace all-targets/all-features Clippy via
   `scripts/dev-check.py`, separate lightweight CLI all-targets check, boundary
   and archive-document checks, full Python suite, repository/whitespace checks.
   Persistent target `artifacts/v190/p10-target`, Windows incremental `1`;
   exact native/harness/fixture/offline hashes and timings retained by runner.
2. Once T1 passes, matched development runtime:
   `python scripts/build-service-runtime.py --profile debug --destination artifacts/one-process/debug-runtime-1823`.
   Expect version/source seals, embedded model execution and static imports to
   pass. Native package plus extension `--stage-only` will provide a complete
   matched development stage for F09; actual adapter tests use isolated owners
   and retain failed fixtures. No normal-profile editor installation is run.
3. Actual native extension/client preflight, required workspace tests/doctests,
   then one optimized runtime:
   `python scripts/build-service-runtime.py --profile release --destination artifacts/one-process/final-runtime-1823`.
   Release incremental environment is unset. Reuse passed development evidence
   with its identity; the optimized payload still needs its own artifact/model
   and package verification. Exact commands and consumed identities are frozen
   in each runner receipt. Stop at the first failed gate and collect diagnostics
   before correction. Do not repeat the stopped installed-editor Help campaign.
4. Fresh native ZIP and Windows VSIX, verified at their artifact boundaries.
   No commit, push, live-profile install or publication is authorized. Report
   actual Windows evidence and preserve unqualified Linux/macOS/signing limits.

- `installer-1823-final-t1-r1.json`: workspace all-targets/all-features Clippy
  passed (83 seconds; `installer-1823-workspace-clippy-timing.html`), and the
  separate lightweight CLI all-targets check passed (76 seconds, one existing
  unused `Child` import warning in the non-inference configuration). The batch
  then stopped at the module-cycle guard. Module honesty and crate edges pass;
  the unreviewed SCC membership fails. Total 171.74 seconds; inputs unchanged.
  Independent review matched all 19 non-installer members byte-for-byte to the
  successful **1.8.22** `runtime-build-r2.json` source roster and reproduced the
  exact SCC after excluding every installer-repair-modified file from graph
  evidence. This is a retained pre-existing one-process boundary failure, not
  a passing check or an exemption. No baseline or production code was changed
  to suppress it. Repair behavior's scoped T0/T1 checks and both configurations
  pass; a local manual candidate is not full structural/release qualification.
- `installer-1823-repository-checks-r1.json`: remaining independent checks pass,
  118.07 seconds, frozen inputs: Python **932 selected, 921 passed, 11 skipped**;
  archive-document check; repository checks over **2,229 public files**;
  whitespace; and strict-offline dependency checks. The latter are limited
  cached Rust evidence: npm/fresh advisories were not checked. No release-audit
  qualification is claimed. The pre-existing cycle check remains failed.

- `installer-1823-debug-runtime-r1.json`: **passed**, 147.11 seconds, native
  inputs unchanged; Cargo development compilation 103 seconds, incremental `1`,
  P10 target and timing HTML retained. Source-sealed **1.8.23** runtime at
  `artifacts/one-process/debug-runtime-1823`; embedded model/static-import and
  release-license inventory checks passed (431 dependency notice packages).
- Development native preflight command, reviewed before execution:
  `runner.py --groups native,harness,extension,offline-assets --static --runtime artifacts/one-process/debug-runtime-1823 --tier T2 --fixture-base artifacts/one-process/native-fixtures-private installer-1823-native-preflight-r1 python artifacts/one-process/installer-native-preflight.py`.
  The wrapper creates fresh canonical debug native/extension stages, then runs
  the full extension suite serially with the sealed current native binary and
  explicit F09 stage root. Expect all 18 current native-gated cases to run
  (including F09's additional Windows ACL child), no failures, and only the two
  POSIX cases plus unavailable historical predecessor-helper case to skip.
  Stage bytes are independently hashed before/after; failures retain fixtures.

- `installer-1823-native-preflight-r1.json`: **failed**, 634.81 seconds total,
  suite 607.86 seconds: **1,406 tests, 1,400 passed, 3 failed, 3 skipped**.
  Runtime, complete staged extension and consumed source hashes stayed unchanged.
  All three failures share one qualification-plumbing cause: the Assistance
  probe omits the explicitly injected stdio transport, so it refuses before
  native execution. This affects the independent Assistance test and F09's
  Windows ACL child (with parent failure propagation). The native UI/help-only
  forwarder remains supported; evidence MCP's production stdio refusal stays
  required. Exact affected call sites and existing transport fixtures were read
  before proposing a correction. Failed fixtures and Stop receipts are retained.
  All other current native cases passed, including setup without inference,
  actual scope orchestration, cold HTTPS and sequential/concurrent clients.
  Only two POSIX cases and the unavailable historical helper skipped. These
  passing cases retain their original debug artifact and test identities.
- **User-directed review pause (10 November, local evening):** no further
  expensive validation/build starts while the anticipatory review is incomplete.
  The preceding failures exposed avoidable harness assumptions (`runtime stop`,
  pending-project limit 64, missing explicit transport) that source/API review
  should have caught. Primary requested four read-only reviews covering native
  installation/process state, extension intent/index admission, all native-gated
  harness/API assumptions, and CLI cleanup assertions. Each review must name
  concrete invariants, consumed API/value rosters, interruption/recovery paths,
  ranked findings and the smallest cheap reproduction; no source edits or tests
  occur during collection. Batch sibling regressions and one coherent correction
  after findings, then review/freeze before validation. Existing passed native
  artifacts and unaffected results must be reused. The manual installer remains
  the authorized objective; no 1.8.23 VSIX or optimized runtime exists yet.

Anticipatory source review (four component reviews plus primary; no new runs):

| Finding | Evidence class and correction boundary | Cheap confirmation |
|---|---|---|
| Native replacement omits managed-policy gate | Concrete source finding: external provider image can lie outside removed roots; reuse canonical managed refusal before admission | Existing managed External/UserOnDemand/detach fixtures; no policy/state/provider/journal changes |
| Default names override another installation's ownership | Concrete source finding: `owned_by_airk` accepts reserved names even when authenticated owner binds another install | Inject default-candidate roster under a temporary owner, retain real native ownership reads and prove foreign state/data untouched |
| First-use defer occurs after persistent admission | Concrete source finding with user effect: valid generation leaves pending host/native operation; next candidate then refuses despite deferred install | Valid-generation Not now/dismissal: readonly discovery allowed, no operation Memento or native admission |
| Host retirement sweeps unrelated HTTP records/variables | Concrete source finding: global key/token-name regex ignores installation ownership | Two installations with distinct enrollment/trust records; replace one and compare other's complete preimages |
| Reconnect ledger uses discovery positions after grouped execution | Concrete source finding: interleaved roots assign B/C each other's outcomes | Three interleaved entries with distinct outcomes; compare destination/container/server identities |
| Trust bundle unlink failures count as completed retirement | Concrete source finding: catch/log followed by planned count and completion | Inject unlink failure; retain per-item retry evidence and report only actual completion |
| Explicit/automatic host operation can overwrite a shared slot | Source-supported race hypothesis, not yet reproduced | Gate two contexts' shared Memento access and interrupt native winner; preserve accepted operation for retry |
| Upgrade qualification uses obsolete replacement arguments | Concrete script finding: apply lacks required explicit operation; null clean-break evidence also passes weak assertion | Mock exact argv and require complete canonical snapshot/table evidence |
| CLI shutdown qualification stopped its predecessor first | Coverage gap, not runtime failure: it never exercised installer-driven shutdown | Owned selected-main foreground Child, restart before replacement, exact child exit; complete live empty observations before Stop and stable full checkpoint afterward |
| Native test failure retention misses cleanup errors/Assistance storage | Source findings plus retained r1 evidence: outside-owner storage removed; finally errors bypass body catch | Tiny existing-fixture cleanup seam with injected close errors; preserve original errors, Stop proof and storage |
| Polling accepts late success | Source finding, not reproduced; existing timeout-zero one-observation expectation is intentional | Existing mocked-clock seam; preserve timeout-zero expectation and full refusal details |

Two findings remain explicit limits, without an unreviewed policy rewrite:
the new journal safely refuses after 128 distinct consumed IDs (automatic VSIX
normally adds one, explicit operation with a new generation can add two), and
native uninstall does not retire that history. No silent pruning or budget
increase is authorized by a passing test. Session image handoff intentionally
allows accepted work to drain while a new image serves new work; an existing
test explicitly requires that overlap. JavaScript disposal is not process-exit
proof. Do not rewrite that assertion or claim a changed first-upgrade guarantee
from an already-loaded 1.8.22 module. Installer-owned predecessor exit receives
its own exact-child regression. Any later drain-policy change is a separate
decision supported by evidence, not an inferred fix.

The installed 1.8.22 manifest was read: its VS Code `installedTimestamp` is an
actual numeric metadata field, validating the old-loader fallback assumption on
this host. This observation does not qualify later real installed-host timing.
Tests-first work is now scoped to the findings above. All owners must finish and
freeze together before one cheap diagnostic batch; no expensive run or optimized
build starts during this work. New source-review findings remain distinct from
the original reproduced defects and from qualification results.

The new diagnostic inputs are frozen after all four owners' source reviews and
primary review. Formatting and pinned host TypeScript compilation passed before
the batch. Exact command:
`runner.py --groups native,harness,extension,fixtures,offline-assets --static --tier T0 --fixture-base artifacts/one-process/native-fixtures-private installer-review-diagnostic-r1 python artifacts/one-process/installer-review-diagnostic.py`.
This collects scoped Clippy, all named replacement library/CLI assertions, 13
named Node cases and the upgrade script's Python module in one diagnostic batch.
Known new semantic failures are expected; the capacity and timeout-zero sibling
cases should pass. Native assertions do not run if Clippy fails; independent
Node/Python diagnostics still collect. No source changes occur until the entire
batch has been collected and assessed, and no expensive qualification is run.

- `installer-review-diagnostic-r1.json`: **60.09 seconds**, all inputs frozen.
  Scoped Clippy found one shared test-fixture export warning repeated across
  other integration binaries; native assertions were correctly not run.
  Node **13 selected, 1 passed, 12 failed**: eleven semantic regressions confirmed
  the anticipated defects; the trust-unlink test instead hit TypeScript's
  getter-only module wrapper before its injection, so that case is not yet a
  reproduction. Python **47 tests**, 15 expected subtest failures expose the
  obsolete argv and weak empty-evidence checks. Entire diagnostics were collected
  before edits. No long campaign or native package rebuild ran.
- Reviewed harness corrections: a crate-local type alias replaces the unused
  reexport without lint suppression; unlink injection now targets the actual
  writable Node module with an explicit attempted-unlink precondition. Python
  source corrections follow the reproduced argv/evidence cases. Native production
  ownership checks and all other extension production behavior remain unfixed
  until their required focused semantic observations are collected.
  `installer-review-native-diagnostic-r2` reuses passed Node evidence and selects
  only the remaining native assertions, trust-unlink observation and corrected
  Python module. Its exact commands and frozen identities are retained by runner.

- `installer-review-native-diagnostic-r2.json`: **131.16 seconds**, frozen
  inputs. Scoped Clippy passed. Library **50 selected, 45 passed, 5 failed**:
  all four managed-policy/detach/origin cases reproduced actual fixture metadata
  removal while leaving provider bytes; these are confirmed ownership defects.
  The foreign-default case instead tried writing through `open_saved` during
  fixture setup, so it does not yet reproduce that source finding. Its canonical
  writable registry fixture is corrected under the existing service lease;
  `installer-review-foreign-observation-r3` selects only that remaining unit case.
  The capacity-bound case passed, retaining IDs and rejecting excess admission.
- Actual CLI **2 selected, 1 passed, 1 failed**: the strengthened test reached
  fresh live resource observations after proving exact running predecessor Child
  exit. Catalog, inference and source checks passed. Its invented zero policy
  reservation failed: the retained sample has zero admitted jobs/workers/threads,
  I/O and drains, RSS **36,253,696 bytes**, and a fixed cold policy reservation
  **1,375,731,712 bytes**. `Resources::reserved` explicitly includes model,
  service and watcher headroom with no running work. This is a wrong harness
  expectation, not proof of index admission or a memory leak. Source-reviewed
  correction captures an independent complete cold sample before setup and
  compares the post-reset snapshot to that golden, excluding only documented
  host telemetry (RSS, available memory, pressure, telemetry completeness).
  Every zero-work assertion and complete archived response remains required.
- The focused trust-unlink case reproduced the real lost-retry/false-completion
  defect after its injection precondition was fixed. Corrected upgrade-script
  Python module: **47 passed**, 0.473 seconds. Those results are retained rather
  than rerun for unrelated source work.

Coherent correction ownership now active: extension install/reconnect owner
fixes first-use ordering, scoped host records, per-entry ledger identity,
truthful resumable trust cleanup and positively reconciled per-operation host
records. Qualification/transport owner fixes explicit Assistance injection,
complete failed-cleanup retention and late-success budgets without changing
timeout-zero behavior. Native ownership correction waits only for the remaining
foreign-owner observation; actual managed mutation is already reproduced.
Primary remains sole build/test owner. No optimized runtime or new VSIX exists;
old debug artifacts retain their historical source seals and cannot be relabeled
as current after the native ownership changes.

## Previous candidate (10 November 2026, evening)

The user chose to test 1.8.22 by hand. The VSIX is `artifacts/one-process/final-extension-1822/publish/airk-1.8.22-win32-x64.vsix` (SHA-256 `01e342b3…`).

The installed-editor journeys stopped after two strikes; see the root-cause note below. A third harness attempt waits for the user's report.

Nothing is committed. The section below is the earlier pickup note and is now history.

## Resume here (paused 10 November 2026, VS Code restart)

**State of the tree.** Everything is uncommitted in the main checkout `D:\code\Github\airk` on branch `feat/simplify`: about 133 changed or untracked paths, including the earlier uncommitted 1.8.19/1.8.20 work. Nothing has been committed or pushed. No installer has been built. The manifests are still at **1.8.20**; the docs already say 1.8.21. No agents, builds or locks are running; `artifacts/one-process/*.lock` is clear.

- **Approved plan:** `C:\Users\tdotn\.claude\plans\that-was-a-terrible-luminous-puzzle.md`.
- **User decisions:** top of `docs/one-airk.md`.
- **This ledger** is the authoritative record.

**Done and validated (Windows):**

- A, B, C, D, E, F, G, J. The last green runs were `integ-compile-r2` and `integ-ext-full-r2` (1,382 selected, 0 fail), `review-tests-r1` (96 selected, 96 pass) and `owner-tests-r1`.
- The H patch is merged; `integ-clippy-r1` (cli/mcp/kernel/retrieval/ingest) is clean.
- The docs (step I) are drafted for 1.8.21.

**Interrupted:**

1. **`integ-tests-r1`** (combined native batch) was stopped at about 1,537 of 1,846 tests. It has no receipt and must be rerun after the workers corrections. Command, run from `artifacts/one-process`:

   `python runner.py --static --tier T1 --fixture-base native-fixtures-private integ-tests-r2 cargo nextest run -p airk-cli -p airk-kernel -p airk-ingest -p airk-retrieval -p airk-mcp --no-default-features --features local-inference --locked --profile ci --no-fail-fast --lib --test service_runtime --test derived_native --test unregister_native --test pdf_worker --test native_worker_storage --test integrity_restart --test helper_authority_roles --test cli_commands --test tools --test mcp --test combined_reads --test native_read_retry --test helper_outage_reads --test usage_native --test configuration_native --test native_owner_recovery`

   Cargo is at `C:\Users\tdotn\.cargo\bin`. The target directory is `artifacts\v190\p10-target`.
2. **Workers review corrections were not started.** The agent was stopped while still reading; it made no edits. Its full brief is the 13-item list under "Workers patch review" below. Restart that work from that list:
   - quarantine replaces every in-process `exit(75/76)`;
   - drain coverage before `install` and in startup, activation and reads;
   - the derived-worker Drop guard;
   - quarantine for the stall watchdog;
   - PDF slot classification, busy deferred and interrupted treated as drain;
   - usage spans treated as overlapped when another owner is active;
   - poisoned-cache recovery and per-root cache release;
   - a non-blocking `ChildWorker::drop`;
   - Unix PDF parent-death exit;
   - bool-checkpoint audit;
   - watcher stale pulse handled by quarantine;
   - `check-self-spawn.py` false negatives;
   - a production in-process start test.

   Only these crate files are in scope: `project_worker.rs`, `derived_worker.rs`, `project_supervisor.rs`, `source_supervisor.rs`, `worker_host.rs`, `worker_scope.rs`, `airk-kernel/src/{service_execution,resources}.rs`, `airk-ingest/src/{pdf_slot,pdf_worker,extraction_pool,extraction_jobs}.rs`, the caches (`source_policy.rs`, `current/lexical/cache.rs`, `verified_cache.rs`), `split.rs`, and `scripts/check-self-spawn.py`.

**Then, in order:**

3. Fill in the docs placeholders that the docs agent marked "see the ledger": the stuck-owner status field names and reason, the drain period, the PDF wait and retry bounds, and whether circuit-open is reported separately. They are in `docs/process-model.md` and `docs/releases/release-1.8.21.md`. Also fix `docs/service-progress.md:22`, which still says exit code 76.
4. Run one combined validation:
   - scoped Clippy, then `integ-tests-r2` above;
   - rerun the 15 tests that failed only in the worktree, in the main checkout: 6 `project_worker_split`/`split_tests` lib tests, plus `native_owner_recovery` ×3, `permanent_trim`, `reference_refresh` and `split` ×4;
   - the extension compile and full suite;
   - `python scripts/check-self-spawn.py`, `scripts/check-repo.ps1` and the whitespace check.
5. Bump to **1.8.21** immediately before building (standing user authorization; see "Versioning" below), then `python scripts/release-version.py check --expected 1.8.21`.
6. Build:
   - the optimized runtime with `scripts/build-service-runtime.py --profile release` and the static ORT environment (about 14 min);
   - the native ZIP, then the VSIX (`scripts/package-extension.py`).
7. Write and run `scripts/check-process-topology.py` (plan Verification section), then the installed-editor journeys. Two failures on the same component means stop and write a root-cause note.
8. Hand the VSIX to the user for manual testing.
9. Commit only when the user asks. Include `docs/arbiter/concept.md`. Use the user's DCO sign-off and no agent trailers.

**Still unverified:**

- whether Codex trusts a CA installed in the OS certificate store;
- which Windows shell runs the header helpers;
- Linux and macOS: Clippy, the in-process suites, and provider process groups without a guardian.

**Deferred with the user's knowledge:**

- H7: direct in-memory calls instead of in-process pipes;
- H10: deleting the worker role subcommands after migrating their test callers.

**Open user question:** stable, preview and custom installs each run their own service. That is by design; the user has not answered whether to change it.

Plan approved by the user on 10 November 2026. Its user decisions:

- Only the service stays resident. Workers run inside it.
- The only allowed temporary processes are:
  - one session per VS Code window;
  - CLI and installer one-shots;
  - one PDF parser child;
  - a user-opened `tools shell`.
- Agents connect over HTTPS directly.
- Explorer warns about broken agent entries and offers a one-click **Reconnect**, which runs after one confirmation.
- Everything ships together.

This record reverses the 1.8.19 narrowing "agent configurations outside VS Code are not edited". That narrowing was made during implementation and never approved by the user.

States: `pending`, `in_progress`, `blocked`, `implemented`, `qualified`.

Each entry distinguishes:

- source-review findings;
- reproductions;
- passing hypotheses;
- qualification results.

Receipts are kept in `artifacts/one-process/`.

## Build ownership

- One agent runs Cargo per checkout at a time.
- Before running Cargo, an agent creates `artifacts/one-process/cargo-owner-<checkout>.lock` with an exclusive create, writes its name into it, and deletes the file when the run ends.
- The extension TypeScript compile and Node tests use `artifacts/one-process/ts-owner.lock` in the same way.

## Versioning

The user gave standing authorization on 10 November 2026: every new installer build gets the next unused patch number. The first installer from this work is **1.8.21**.

- Bump immediately before the VSIX/native package build, not during implementation.
- Keep native, extension, lockfiles, the template catalog and docs synchronized.
- Verify with `python scripts/release-version.py check --expected <version>`.

## Commit scope

User instruction, 10 November 2026: the commit for this work also includes `docs/arbiter/concept.md` (72,199 bytes, untracked). It is a future-plan concept, not current behavior. List it in the documentation index only as a future concept. Commits use the user's DCO sign-off with no agent trailers.

## Integration runs (primary)

| Receipt | Scope | Result |
| --- | --- | --- |
| `integ-compile-r1` | `npm run compile`: extension, webviews, Explorer bundles, uninstall worker | Pass, 15 s. This run includes the integrator's allowlist and packager-module edits and the login opt-out wiring. |
| `integ-ext-full-r1` | Full mocked extension suite | 1,375 selected, 1,354 pass, 0 fail, 21 skipped |

## Independent review (10 November 2026, read-only, no runs)

Review of the main-checkout diff, sorted by severity.

**Confirmed from source, not reproduced:**

1. HIGH: a cancel/answer race kills the window session, including running installer work.
2. The extension's 5-second close grace kills exclusive work that the native drain contract allows to finish.
3. `http-credential` was admitted in-session, so the session could return bearer tokens.
4. A partial replacement failure leaves unreported moved folders.
5. The credential helper points at a version-specific runtime path.

**Plausible risks, being fixed:**

- in-session output budget below the former one-shot buffer;
- no install lock around replacement;
- an unreadable install record is treated as a replacement;
- weak ownership proof for state/logs folders;
- ledger capture misses normalized URLs and helper-only references;
- unverified Codex helper key.

**Plausible risks, recorded without change:**

- The extension replaces a pre-1.9.0 installation without a prompt. This matches the user's 1.8.19 decision.
- Cancelled work continues after `cancelled`; cancellation is cooperative.
- Single instance is per control root. Stable, preview and custom installs are separate installations by design.
- On owner loss, the session drains started exclusive work up to that request's deadline. This is a documented exception to "exits with its window".
- Different-build one-shot fallbacks are limited to predecessors.

**Integrator contract change:** session responses go up to 16 MiB, matching the former one-shot buffer (`service_ipc::MAX_CODEC_BYTES`). The fixture now carries a `budgets` object that both Rust and TypeScript assert. The corrections were sent back to the native, extension and agents owners.

**Extension corrections: implemented.**

- Each cancel id is kept until it is answered. `cancelled`/`failed` answers to cancels are benign.
- Release drains started work up to its own deadline plus 5 s, and kills only after that.
- The TypeScript budgets equal the fixture's `budgets`.
- There is one window-session subscription.
- `ext-d-full-r2`: 1,379 selected, 1,358 pass, 0 fail, 21 skipped.

**Agents corrections: implemented.**

- Runtime transitions refresh owned credential helpers before retire-previous runs.
- New finding `https_helper_missing`, repaired through the refresh path rather than a full Reconnect.
- Normalized endpoint URLs and helper-only references are now captured.
- Nothing is written before the confirmation.
- The Codex `mcp_servers.<id>.http_headers_helper` key is confirmed verbatim in the Codex config reference.
- `t1-full-r3`: 1,382 selected, 1 fail, a test fixture whose runtime image path didn't exist. That fixture was fixed. `t0-r3` (agentReconnect + agentConfigHealth): 31 selected, 31 pass.
- The full suite is still to be rerun in the integration batch.

**Native corrections: implemented.**

- `http-credential` and `runtime launch` are refused in-session.
- A partial move is rolled back newest first. If the rollback fails, every folder left aside is listed as residue.
- A `.<leaf>.replace-1.lock` beside the installation refuses a concurrent replacement before any side effect.
- Unreadable records cause a typed refusal.
- Layout folders outside the default locations need an AIRK owner record.
- Diagnostics are scoped to the request's stderr.
- Display-formatted reconnect instruction.
- The 16 MiB response budget is judged on the encoded frame.
- `review-clippy-r1` passes. `review-tests-r1`: 96 selected, 96 pass.

**Integration rerun:** `integ-compile-r2` passes. `integ-ext-full-r2`: 1,382 selected, 1,361 pass, 0 fail, 21 skipped.

## Workers patch review (10 November 2026, read-only)

**Confirmed from source:**

1. A query past its 125 s deadline calls `process::exit(75)`, which now ends the whole service.
2. Drain only works inside `service_execution::run`. A slow startup or activation, or a blocking read, leads to exit 76, and recovery replays the work, so every project goes into a crash loop.
3. On early `?` returns, the cleanup owner exits without joining its job.
4. The stall watchdog restarts the whole service.
5. PDF slot outcomes are misclassified: busy is treated as a fatal job failure, and an interruption marks the document as disappeared.
6. Usage spans see whole-service counters.
7. A poisoned shared cache affects every project, and cache notification handles outlive their owner.
8. `ChildWorker::drop` can block the supervisor.
9. On Unix, the PDF child can be orphaned after a service crash.
10. `scripts/check-self-spawn.py` has false negatives.

**Plausible risks:**

- the watcher memory check is gone;
- exit 76 during stop/upgrade;
- callers that turn a checkpoint into a bool can continue past a drain;
- the production in-process start path is not covered by native tests.

**Correction round, resumed after the reboot (10 November 2026).** Item 12 was done by the primary; three agents own the rest, with disjoint files. No agent runs Cargo, so there is one combined batch after review.

- workers agent: items 1–4, 8, 10, 11 and 13;
- PDF agent: items 5 and 9;
- caches and usage agent: items 6 and 7.

**Item 12, `scripts/check-self-spawn.py`: implemented, Python T0 passed.**

Source-review false negatives:

- An image path passed to a start through a parameter, field or const was invisible. Examples: `embedding_pool_process::start_from_main`, `universal_launcher::run`, `start_with_observation_clock`.
- `CreateProcessW` (`native_process::spawn_detached`, `child_job_named::spawn_guarded_io`), `spawn_guarded*` and `Invocation::new` were not start primitives.
- A `;` inside a signature (`[u8; 4]`) ended the function early.
- `br"…"` raw strings were not recognized.
- A role literal in a const was skipped.

Correction:

- A reviewed `STARTS` roster now lists every production function that creates a process, with what it starts: service, one-shot, pdf, legacy, external or primitive. A new start fails until it is classified.
- The parser fixes above.
- The regression test is `test_former_false_negatives_are_found`.

Evidence: `python -m unittest scripts/tests/test_check_self_spawn.py` ran 4 tests and all passed. On the current tree: 6 self-spawn sites, 5 legacy, 33 reviewed starts.

**Items 6 and 7 (caches and usage agent): implemented, not compiled.**

- **Usage spans.** The epoch advances on every start and end. Disabled-usage requests are counted as unmeasured work. A new `airk_kernel::OwnerActivity` guard covers owner work, and a poisoned tracker lock is recovered and reported as overlapped. Reporting goes through the existing `ResourceMeasurement.overlapped` / `ResourceSummary.overlapped_intervals` fields; there is no wire change.
- **Caches.** Poison recovery and per-collection release for `verified_cache`, `lexical/cache` (change-notification handles) and kernel `source_policy` `RUNTIMES`, the latter for expired bindings only.
- **New public items:** `airk_kernel::{OwnerActivity, release_collection_caches}` and `airk_store::current::release_collection_caches`.
- **Integrator wiring is pending** until the workers agent finishes: `OwnerActivity` in `service_execution::run` and in the source watcher, and release at project/cleanup owner exit.
- **Recorded risk:** more spans report as overlapped and carry no counters.

**Items 5 and 9 (PDF agent): implemented, not compiled; the Unix code is uncompiled.**

- A busy slot defers the document: its previous artifact stands in under an unchanged binding, it gets one retry at the next drain, and is then recorded with the `extraction_deferred` skip code.
- An interrupted wait stops the scan with the typed `PdfSlotInterrupted`, settling nothing.
- Unix parent death: Linux `PR_SET_PDEATHSIG` plus a `getppid` re-check; macOS kqueue `NOTE_EXIT`. The decision rule is a pure `parent_watch`.
- Bounds: 60 s slot wait per attempt, at most two waits per document per scan, a 15 s / 10 s CPU / 512 MiB parse.

Integrator follow-ups:

- **B, done:** `inventory.rs` coverage returns the slot error instead of caching `ExtractionFailed` for that content hash. Risk: coverage fails while another owner holds the slot.
- **C, done:** a `#[serde(skip)] IndexReport::extraction_deferred` count, with no wire change. It is incremented in `ScanSink::defer`, and `verify_source_inventory` refuses when it is nonzero.
- **A, pending, in the workers agent's file:** `service_execution::run` maps `PdfSlotInterrupted` to the owner's typed `Draining`/`Canceled`/`WorkStalled`.
- **D, not done:** optional progress text.
- **Linux residual:** the window between fork and the first `getppid` remains; closing it needs `pre_exec` in `child_group.rs`.

**Items 1–4, 8, 10, 11 and 13 (workers agent): implemented.**

- `ExecutionScope` keeps scope-level `draining`/`stuck` flags that exist before `install`. `checkpoint()` honors drain with or without a grant.
- `StopStep::Quarantine` replaces `ExitService`; `exit_service_for_stuck_owner` is removed.
- Stopping owners are polled from a thread-local `STOPPING` list on the actor's turn. `ChildWorker::drop` never waits for an in-process owner.
- The query deadline and the stall watchdog quarantine the owner's own scope. Only a child-process fixture still exits 75/76.
- The lease is held in `WorkerScope` until the owner thread returns.
- The `CleanupJob` guard joins its job thread.
- The watcher gets `StoppingWatch`: 5 s, then 20 s.
- Item 10 audit: `split.rs:174`, `observe_native_integrity`, `record_watch_roots` and `observe_maintenance` were fixed; the rest were reviewed and are OK.
- New tests: the quarantine regression and its release, self-reported stuck, the production start refusal, the in-process start, the watcher quarantine, and drain before install.

**Integrator integration (primary), source-reviewed:**

- **`catalog.rs` `DrainWorker`.** It relied on the old blocking `Drop`. It now returns a typed `WorkerDrainPending` while the owner is in `STOPPING`, and `CatalogControlHandle::drain_worker` asks again for up to 35 s (stop grace + drain + 5 s). After that it fails as quarantined.
- **`service_execution::run`.** It holds `OwnerActivity`, and maps `PdfSlotInterrupted` to the owner's `Draining`/`Canceled`/`WorkStalled`; if no flag is set, the original error is kept.
- **Cache release.** `worker_scope::CollectionCaches`, a drop guard calling `airk_kernel::release_collection_caches`, is placed right after the registration in `project_worker::serve` and `derived_worker::serve`, so it drops after the kernels and the job.
- **Decision: the source watcher is not registered as `OwnerActivity`.** Marking its 250 ms pulses would make nearly every span overlapped. Residual: the watcher's own CPU can still land in a concurrent span.
- **Shutdown with a quarantined owner was checked.** `ChildWorker::drop` returns without joining, and the actor loop ends once every remaining owner is quarantined.
- **Docs filled in.** `process-model.md`, `release-1.8.21.md` and `service-progress.md:22` now have the real field names, codes and bounds. The drafted "typed stuck-owner status observation" was corrected: status uses the existing `automatic_start_blocked` and cannot tell a quarantine apart from an exhausted circuit; the reason is in the service log. A distinct status field would be a D8 contract change, left to the user.

**Validation (frozen inputs):**

- `cargo fmt --all`.
- `integ-clippy-r2` (cli/mcp/kernel/retrieval/ingest/store, `--all-targets`, local-inference): exit 101 with one diagnostic, E0308 at `project_worker.rs:190` (`Ok(())` matched against `Result<Sha256Digest>`). Fixed with `Ok(_)`.
- `integ-clippy-r3`: exit 0 in 29.3 s, inputs unchanged. The `cfg(unix)` code is uncompiled.
- **`integ-tests-r2`** (the full combined native batch, private fixture base): 1,863 selected, 1,862 pass, **1 fail**, 44 skipped, 1,269 s, inputs unchanged.
  - Every new test from the three agents passed.
  - `native_owner_recovery` ×3 passed. The fixture-ACL diagnosis is confirmed.
  - The `project_worker_split`/`split` and `combined_reads` tests (`permanent_trim`, `split`, `reference_refresh`) passed. The 15 worktree-only failures do not recur in the main checkout.
- **The one failure was the new test's wrong premise, not a code defect.** `production_owner_start_refuses_an_unverified_runtime_before_starting_any_owner` assumed `engine_identity()` refuses in a test image. An absent receipt is the valid single-executable identity (`runtime_pair.rs:113`), so the production path correctly started an owner in process. The test was rewritten as `production_owner_start_runs_in_the_service_holds_its_lease_and_is_reaped`: the real `ChildWorker::spawn` and real engine identity against a registered project, then an in-process start, the lease held, a non-blocking stop, a reap and the lease freed.
- `integ-clippy-r4` (airk-cli, all targets): exit 0.
- `integ-t0-r1` (`project_supervisor::tests::*`, store `verified_cache::*`, `current::lexical::cache::*`): 13 selected, 13 pass.
- `cargo fmt --all -- --check`: clean.
- Extension suite:
  - `integ-ext-full-r3`, run under the **system Node 24.21.0 by mistake**: 1,382 selected, 1,360 pass, 1 fail, the late-Memento variant of `serviceOnboarding.test.ts:28` (`console_operation_failed`). This is not a valid receipt because the pinned toolchain is 24.15.0. The failure is unexplained; the possible Node-version sensitivity is not investigated.
  - **`integ-ext-full-r4` under the pinned Node 24.15.0:** compile plus 1,382 selected, 1,361 pass, 0 fail, 21 skipped.
- `scripts/check-self-spawn.py` passes, and its unit tests pass (4/4).
- `git diff --check` is clean.
- `integ-check-repo-r1` (`scripts/check-repo.ps1`): passed, 2,225 public files.

**Version bump and build (10 November 2026):**

- Bumped to **1.8.21**: workspace `Cargo.toml`, the 8 local crates in `Cargo.lock`, `package.json`, both root entries in `package-lock.json`, and `explorer_templates.json` (regenerated with `build-explorer.cjs --write-mcp-catalog`). `release-version.py check --expected 1.8.21` reports "Native, extension and lockfiles agree: 1.8.21".
- `runtime-build-r1` (`build-service-runtime.py --profile release`, static ORT, `--release`): pass, 867 s, inputs unchanged. `airk.exe` SHA-256 `b76bdcc4291a738803585d0885052ee09738a5127478fdc8f26abdedb566cc54`.
- `native-package-r1`: pass, 16 s.
- `extension-package-r1` (`--channel stable`): pass, 34 s. `final-extension/publish/airk-1.8.21-win32-x64.vsix`, 939 files, 77.45 MB, platform VSIX verified.
- New `scripts/check-process-topology.py`, with modes snapshot, watch, cli-burst and refuse-serve, plus `scripts/tests/test_check_process_topology.py` (4/4 pass). A live snapshot of this machine showed one service, the installed 1.8.20 `runtime managed-serve`.
- `installed-explorer-r1` on the 1.8.21 VSIX: pass, 18 s, inputs unchanged.

- **`installed-native-r1` on the 1.8.21 VSIX: FAIL (strike 1 for the installed-editor component),** 309 s, inputs unchanged.
  - Scenarios `same` and `different` passed; `multi` failed; `usage` and `untrusted` did not run.
  - Failure: after the real window reload (23:16:59.4), window A's driver read in `definitions()` was refused 14 s later by `airk.exe` with exit 1 in 35 ms: "another AIRK installer/launcher is active; retry after it finishes: lock acquisition failed because the operation would block". The service ran throughout (23:16:28 to its cleanup stop at 23:17:18).
  - Evidence: `artifacts/editor-qualification/native-installed-SvhRg4/multi/` (`a-error.json`, `a-phase.json`, the post-reload `2-AIRK.log`).
  - **Source-review hypothesis, not reproduced:** read-only install observations (`inspect-legacy-caches`, `legacy_caches.rs:322`; `lookup-http-connection`, `http_commands.rs:230`; launcher inspect; `commands.rs:170`) take `existing_install_lock`, an exclusive `try_lock` with no wait. Two concurrent observations therefore refuse each other at once.
    - The reloaded extension runs `inspect-legacy-caches` during activation (`serviceExtension.ts:1673`). The driver runs the same command in `provePrivateCacheScope`.
    - 1.8.21 makes the overlap likelier: the window session runs requests concurrently, and the new agent-health check adds `lookup-http-connection` at activation.
    - 1.8.20 passed the same journey.
  - **Not established:** which process held the lock at 23:17:13.6.
  - **Missing cheap test:** a T0 native test of two concurrent read-only observations on one installation.
  - Correction direction, for review: read-only observations take a shared lock, or wait a bounded time like `connection_install_lock`. Writers stay exclusive.
- `topology-watch-native-r1`, a forbidden-role watch alongside the journeys for 600 s: **pass**.
  - It saw no worker-role, guardian, stdio MCP or alias process.
  - It did see: service ×5 across the scenarios (`runtime managed-serve` ×3, `serve` ×2), `service session` ×6, `runtime self-test` ×2, and the harness's own one-shots (`runtime inspect` ×11, `http-connection-info` ×9, `inspect-legacy-caches` ×3, `service rpc` ×4, and others).

**User decision (10 November 2026): fix first, then 1.8.22.**

Lock correction (`runtime_install.rs`):

- `existing_install_lock` stays exclusive, because `inspect-legacy-caches` can settle an interrupted withdrawal. It now waits for a held lock up to `EXISTING_INSTALL_LOCK_WAIT` (15 s) before refusing, through `existing_install_lock_within(root, budget, pause)` with an injected pause.
- `install_lock`, used by long installers, still refuses at once. The activation writer `ensure` already uses the bounded `connection_install_lock`.
- Callers: legacy-cache inspect/retire/reprepare, legacy-launcher inspect, `lookup-http-connection`, predecessor-cleanup lookup, and the `commands.rs:170` registry transaction. All run on CLI one-shot or session-request threads, never on the service runtime.

Regressions:

- `connection_lock_tests::concurrent_existing_install_observations_wait_for_each_other`: the second observation is observed waiting, by event and without a sleep, then acquires once the first releases.
- `connection_lock_tests::an_existing_install_observation_refuses_after_its_wait_budget`.

Validation and docs:

- `lockfix-clippy-r1` (airk-cli, all targets): pass.
- `lockfix-tests-r1` (`connection_lock_tests`, `legacy_caches`, `legacy_launchers`, `http_cleanup`, `http_commands`): 36 selected, 36 pass.
- Docs: the current-guidance references move to 1.8.22, and `release-1.8.21.md` is renamed to `release-1.8.22.md` with an "About 1.8.21" section. This ledger keeps 1.8.21 as dated history.

**1.8.22 build and installed runs:**

- `runtime-build-r2`: pass, 909 s, inputs unchanged. `airk.exe` SHA-256 `479e1a6e90a31f0f07601ed5bae8443cbc0fc3cada6f15a4a3d445cfcfabb932`.
- `native-package-r2`: pass.
- `extension-package-r2`: pass. `final-extension-1822/publish/airk-1.8.22-win32-x64.vsix`, 939 files, 77.46 MB, SHA-256 `01e342b3ec01c335e31e81f37ea370a1a7e8dd14b83896231e6a997615b69f12`.
- Pre-build checks: `integ-check-repo-r2` passed (2,227 files). `integ-ext-full-r5` (pinned Node 24.15.0): 1,382 selected, 1,361 pass, 0 fail.
- `installed-explorer-r2`: pass, 12 s.
- **`installed-native-r2`: FAIL, strike 2 for the installed-editor component.** 416 s, inputs unchanged. `same` and `different` passed; `multi` failed; `usage` and `untrusted` did not run.
  - The lock correction is confirmed at the earlier failure point. After the reload, the restored-definition read, endpoint refresh, standalone assistance and Explorer-ready steps all completed (`multi/a-reload-definition-observations.json`, `a-c8-*`, `a-explorer-ready.json`).
  - The new failure is in the external UI automation (`scripts/editor-ui.cjs:302`), on the "AIRK Help" Quick Pick, clicking "Browse all help": "Quick pick click has an unknown outcome; no fallback dispatch". The real mouse click was dispatched, and 100 ms later the picker was still open with that row focused. The harness by design never re-dispatches.
  - The picker is a plain `vscode.window.showQuickPick` with two fixed items (`explorerController.ts:730`), unchanged on this branch. The failure is not yet explained: either the product did not accept the pick, or the 100 ms post-click observation is too short under load. At the sample there were no notifications and one widget.
  - Evidence: `artifacts/editor-qualification/native-installed-1G96R7/multi/` (`a-ui-diagnostics.json` holds the picker samples).
  - The harness failure cleanup was incomplete: "Failure Stop response is incomplete or crossed", and `native-stop.json` is missing. Afterwards no fixture `airk.exe` was running; only the user's installed service, PID 3832.
- `topology-watch-native-r2`, 900 s alongside `installed-native-r2`: **pass**, with no forbidden role. New processes seen: service ×6, `service session` ×6, `runtime self-test` ×2, one-shots ×57.
- **Residue found and removed.** Both installed runs left six fixture login tasks (`airksvc-*`) pointing at `artifacts/editor-qualification/.../native-service` runtimes, including from passed scenarios. Cause: step E registers login startup on every fresh install, and the journey harness never unregisters it; 1.8.20 needed the same manual cleanup. All six were removed with each fixture runtime's `runtime login --install-dir <fixture> uninstall` (log: `installed-native-login-cleanup.txt`). Only the user's own `airksvc-251cb4b…` remains.

**Root-cause note (two strikes; stopped before a third attempt).**

- **Wrong assumption:** that one installed-journey run would confirm the corrected candidate. In fact the `multi` scenario failed at two unrelated points, each needing a 15-minute rebuild plus a journey run to observe.
- **Missing cheap tests:**
  1. A T0 native test of concurrent install observations. It now exists (`connection_lock_tests`).
  2. A harness-level unit test that the Help Quick Pick accepts within the observation window, or a harness rule that waits for the picker to close within a bounded deadline before declaring the outcome unknown.
  3. A harness cleanup step that unregisters login startup for every owned fixture installation.
- **Next step, awaiting the user:** none, until the user decides.
  - If the Quick Pick failure is harness timing, only the harness changes, and the journeys can rerun on the same 1.8.22 VSIX without a rebuild.
  - If it is product behavior, the cause has to be reproduced first.

Review status: the primary reviewed the integration paths and the riskiest agent code (stop polling, quarantine, query deadline, lease retention, shutdown, catalog drain). This was a targeted review, not a line-by-line independent review of every agent change.

**The `native_owner_recovery` ×3 failures are a fixture ACL problem, not a code defect.** They come from the partial `integ-tests-r1.log` (`native Windows boundary policy unavailable`).

- Cause: the runner's `native-fixtures/` directory inherits the repository ACL, which includes Authenticated Users with Modify rights. The native private-boundary check rightly refuses that.
- The documented precondition (release-readiness.md, "native-fixtures") is a directory with FullControl for only the creator, SYSTEM and Administrators. `readiness-1820/native-fixtures` has the same broad ACL.
- Correction: a new directory, `artifacts/one-process/native-fixtures-private/`, has its inheritance removed and those three grants. No existing ACL was changed.
- `integ-tests-r2` passes `--fixture-base artifacts/one-process/native-fixtures-private`.

Open for the user: the Explorer Assistance start (`explorer_assistance::command`) runs VS Code's Node runtime for an external agent. It is not an `airk.exe` process, but it is a per-agent process.

**Integrator design decision:** quarantine replaces the service exit. A stuck owner keeps its lease, its project's circuit opens and its status reports the problem; other projects keep running, and an explicit restart is the recovery. The full correction batch was returned to the workers owner, who now works in the main checkout.

## Steps

| Step | Owner | State | Evidence |
| --- | --- | --- | --- |
| A. Session protocol contract (`crates/airk-cli/src/session_protocol.rs`, `fixtures/wire/session_frames.json`) | primary | implemented | `session-contract-r2`: T0, 5 selected, 5 pass. The first run, r1, failed to compile: the types derived `Debug`/`Clone`/`PartialEq` over `ControlMethod`/`ControlResponse`, which don't implement them. |
| B. CLI output sink | native | implemented | **Golden capture** (`artifacts/one-process/golden/`): 60 offline runs.<br>- 56 are byte-identical.<br>- 4 differ: `runtime --help` and `service --help` (two runs each), only by the new `http-credential`/`session` rows.<br>- Before binary `f358b213…`, after `173c5da2…`. |
| C. `airk service session` | native | implemented | **Tests:** `bc-tests-r1`, then `efj-tests-r1`: 94 selected, 3 fail. `efj-tests-r2`: 22 selected, 22 pass. Scoped Clippy `efj-clippy-r3` passes.<br>**Real binary:** `cli_commands::service_session_matches_one_shot_output_and_exits_on_eof` checks hello identity, then four in-session runs that are byte-equal to the one-shot output, then that `serve` is refused, then exit on EOF.<br>**Owner loss fixed:** the session now drains instead of exiting immediately. It cancels read-only work and lets a started exclusive command finish, but never past that request's own deadline (pure `drain_until`/`may_exit`, injected timing and owner probe). `owner-clippy-r1` passes; `owner-tests-r1`: 19 selected, 19 pass.<br>**Recorded limits:**<br>- success-only exit status;<br>- non-UTF-8 stdout is converted lossily;<br>- Unix stream-redirect code not yet compiled. |
| D. Extension: one session per window | extension | implemented | Evidence: `nativeSession.ts`, the spawn guardrail in `explorerBoundaries.test.ts`, and wire-fixture encode/decode tests.<br>`ext-d-r1`: 580 selected, 576 pass, 0 fail, 4 skipped. `ext-d-full-r1`: 1,367 selected, 1,346 pass, 0 fail, 21 skipped.<br>Tested only against a fake child and the fixture; no real-binary run yet.<br>Remaining allowed spawns: the session; different-build and window-less one-shots; the console for a different-build runtime; the uninstall worker; the `tools shell` terminal. The stdio MCP spawn was returned to the agents agent for removal. |
| E. Continuously available service | native + extension | implemented | **Native:**<br>- `--idle-exit-seconds` is omitted when the saved catalog authority contract is 1 or higher.<br>- Typed `service_already_running` for a second start.<br>- `startup-opt-out.json` in the install root is written by `login uninstall` and cleared by `login install`. It is reported as `startup_manager.user_opted_out`. This is a new install-root file, not an `airk-state/1` format.<br><br>Earlier extension note: | **Extension:** login startup is now registered after upgrades too. The idle-exit restart is replaced by a recovery path used only when OS registration was refused.<br>**Native:** no idle exit in HTTPS mode and a persisted login opt-out record, both in progress. |
| F. Replacement installer obeys one-airk §10.3 | native + agents | implemented | **Extension:** captures the reconnect ledger (`airk.reconnect.ledger.1`) before `--apply`, then retires the replaced host artifacts. **Native:** ownership is proven before anything else, the CA is retired through its receipt, `client_references` is reported, a replacement lock is taken, a partial move rolls back, and `http-credential` exists; see "Native corrections" above. A replacement deletes `startup-opt-out.json` with the old folder, so only an upgrade honors the opt-out. |
| G. Explorer agent warning and Reconnect | agents | implemented | Second round:<br>- One confirmation now runs the reviewed chain: index setup draft/apply, then the grant through `applySearchScope`, then `enrollHttpSpec`, then the rewrite.<br>- **Disclosed side effect:** the grant makes the new index this window's search scope.<br>- The stdio MCP spawn is retired; the native-execution-off switch closes the window session.<br>- Evidence: `t1-full-r2`, 1,374 selected, 1,353 pass, 0 fail, 21 skipped.<br>- The integrator added `agentReconnect.js` to the packager modules and `nativeSession.ts` to the uninstall bundle allowlist.<br><br>First round: | **Implemented:** classification, notification, banner, status bar and Reconnect for reviewed indexes. Evidence: agents `t0-r2`, 271 selected, 271 pass; `t1-full-r1`, 1,367 selected, 0 fail, 21 skipped.<br>**Returned to the agent:** creating the index and grant after the user's single confirmation. The agent had stopped at `awaiting_index`.<br>**Open:** whether Codex trusts the OS certificate store outside VS Code; which Windows shell runs the header helpers. Both need checks against the real clients. |
| H. Workers in the service | workers | in_progress | **Merged into the main checkout** (patch applied cleanly; `scripts/check-self-spawn.py` roster key updated to `run_with`). `integ-clippy-r1` over cli/mcp/kernel/retrieval/ingest is clean. **One PDF child for the whole service** is enforced by `airk-ingest/src/pdf_slot.rs`: `h8-ingest` 15/15, `h8-integration` 69/69. The review corrections above are in progress (quarantine replaces exit 76).<br>Earlier worktree record: |**Implemented (Windows evidence):** H1 transport carriers, H2 per-owner scopes, H3 in-process source watcher, H4 in-process project and cleanup owners, H5 cooperative drain.<br>**H5 drain timing:** after 10 s, then a drain, then 20 s more; after that the service exits with code 76. The 20 s is the agent's choice.<br>**H6:** accounting is count-based plus service RSS. Usage spans now see whole-service counters.<br>**H9:** no guardian in-process. Unix cleanup on abrupt service death is not done.<br>**H11:** `scripts/check-self-spawn.py`.<br>**Evidence:** `final-clippy-2` clean. `final-cli-workers`: 220 run, 214 pass; the 6 failures also fail at unmodified HEAD in the worktree (fixture location). `h4-parity`: 140 selected, 131 pass; the 9 failures also fail at HEAD (`head-parity-failures`). Those 15 must be rerun in the main checkout.<br>**H8:** the PDF child is spawned by the service, but up to 4 can run concurrently. Returned to the agent: one PDF child for the whole service.<br>**Deferred:** H7, direct in-memory calls instead of in-process pipes (no extra process involved). H10, deleting the role subcommands after migrating their test callers. |
| I. Documentation | primary | pending | |
| J. CLI outside VS Code | native | implemented | `StartRoute`/`start_route()` at both election sites. With login startup registered, the CLI starts the service through the OS manager and never spawns `serve` itself. A second `serve`/`managed-serve`/`foreground`/`host` fails with a `service_already_running:` message; the JSON `code` is still `operation_failed`. The OS-manager start still needs the T2 topology check. |
