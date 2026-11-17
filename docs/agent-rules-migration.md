# Agent instruction preservation map — 17 November 2026

This is an editorial audit, not additional standing policy. Baseline: Git commit
a3dbc74331499855cd44befd5d234bafbbb65363, AGENTS.md and CLAUDE.md SHA256
`364c075016564f297beb2514aa52f42583b1daad4dfa7e8cfc862f382df587f4`.

The user authorized condensation while preserving rules and exceptions. Both
entry files remain identical. Commands and the change-to-validation matrix move
verbatim to CONTRIBUTING. The component map and glossary move to the current
architecture guide, with location wording adjusted. Campaign permissions,
constraints, private roots and exceptions remain mandatory through the linked
current assignment. The old deadline sequence, opening-commit authorization and
restart event are retained as history in the ledger.

The only added policy is the explicitly requested source-location rule:
authored implementation and reproduction code, including experiments, starts in
maintained nonignored source with READMEs. Private data/weights stay excluded;
generated source copies require maintained owners and provenance. No publication,
host, credential, format, deadline or qualification permission is broadened.

Each row identifies an original block by baseline line and opening text.
Multi-clause blocks map to the named section; validation clauses also span
[Validation and completion](../AGENTS.md#validation-and-completion).
The baseline Git file retains full original text for comparison.

| Original line | Original section | Original block opening | New location |
|---|---|---|---|
| 5 | Mandatory execution identity (Codex and Claude Code) | All work must run as the current logged-in human user on the execution host. Never use an agent-spec… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 10 | Mandatory execution identity (Codex and Claude Code) | - Before the first shell or filesystem operation in a session, select the execution mode that runs a… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 16 | Mandatory execution identity (Codex and Claude Code) | - On this Windows host, use the verified logged-in-user execution path Codex: require_escalated whe… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 21 | Mandatory execution identity (Codex and Claude Code) | - Apply the rule to every file and folder creation or mutation, including direct file-edit tools, sh… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 27 | Mandatory execution identity (Codex and Claude Code) | - On Windows, also verify the token's default file-owner SID WindowsIdentity.Owner equals the ve… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 35 | Mandatory execution identity (Codex and Claude Code) | - Child processes, background jobs, delegated agents, build/test runners, installers and services st… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 39 | Mandatory execution identity (Codex and Claude Code) | - Before a remote batch on Mac Neo, verify the SSH execution identity against that host's logged-in … | [Preserved requirement](../AGENTS.md#execution-identity) |
| 42 | Mandatory execution identity (Codex and Claude Code) | - Keep output paths in the authorized workspace or the verified user's own approved temporary/cache … | [Preserved requirement](../AGENTS.md#execution-identity) |
| 46 | Mandatory execution identity (Codex and Claude Code) | - If the identity is wrong, unknown or cannot be preserved by a tool or child process, stop that ope… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 52 | Mandatory execution identity (Codex and Claude Code) | - Existing foreign-owned files are not permission to adopt or overwrite them. Preserve them and repo… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 56 | Mandatory execution identity (Codex and Claude Code) | These requirements apply equally to both agents and every campaign. Keep this section and the rest o… | [Preserved requirement](../AGENTS.md#execution-identity) |
| 59 | Mandatory execution identity (Codex and Claude Code) | AIRK is an independent Artifact Indexing and Retrieval Kernel. This file holds the standing rules fo… | [Preserved requirement](../AGENTS.md) |
| 61 | Mandatory execution identity (Codex and Claude Code) | Before changing behavior, read the affected source, its tests and the owning component's CONTRACT.m… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 65 | Repository map |  /  Path  /  What it owns  /  Contract  /  | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 66 | Repository map |  / --- / --- / --- /  | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 67 | Repository map |  /  crates/airk-core/  /  Shared native types and read/service contracts  /  src/read_limits/CONTRACT.m… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 68 | Repository map |  /  crates/airk-process/, crates/airk-host/  /  Native processes, platform adapters, install files/l… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 69 | Repository map |  /  crates/airk-ingest/, crates/airk-store/, crates/airk-retrieval/  /  Extraction, persisted inde… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 70 | Repository map |  /  crates/airk-kernel/  /  Publication, query execution, budgets and Workbench plans  /  Owning source/… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 71 | Repository map |  /  crates/airk-mcp/  /  Canonical evidence handlers, MCP schemas and SDK transport  /  docs/read-inter… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 72 | Repository map |  /  crates/airk-ipc/  /  Same-user transport, frames and I1 control protocol  /  CONTRACT.md  /  | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 73 | Repository map |  /  crates/airkctl/  /  CLI, session and agent-config writing  /  src/contract/CONTRACT.md I2  /  | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 74 | Repository map |  /  crates/airk-endpoint/, crates/airk-https/  /  Endpoint contracts, HTTPS gateway and Admin handle… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 75 | Repository map |  /  crates/airk-catalog-control/, crates/airk-frontend/  /  Catalog mutations and service-side clien… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 76 | Repository map |  /  crates/airk-inference/, crates/airk-supervisor/  /  Inference lifetime and in-process workers  /  … | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 77 | Repository map |  /  crates/airk-installer/, crates/airk-service/, crates/airk-cli/  /  Installation, resident serv… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 78 | Repository map |  /  packages/vscode/  /  VS Code host, Explorer and Workbench  /  I2 plus docs/split/contracts.md  /  | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 79 | Repository map |  /  packages/admin/  /  Browser/Admin presentation and shared UI workflows  /  I3 plus docs/split/contr… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 80 | Repository map |  /  scripts/, .github/workflows/  /  Checks, packaging, installers and release workflows  /  CONTRIBU… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 81 | Repository map |  /  temp/, .airk/ and private fixture/state subpaths under artifacts/ listed in Current work  / … | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 83 | Repository map | Contract paths in the table are relative to their named crate. Dependency direction: clients/transpo… | [Preserved requirement](architecture/airk-architecture.md#component-contract-map) |
| 87 | Commands | Run from the repository root with pinned toolchains and provisioned inputs. Select the affected crat… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 89 | Commands |  /  Purpose  /  Command  /  | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 90 | Commands |  / --- / --- /  | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 91 | Commands |  /  Format / verify  /  cargo fmt --all; cargo fmt --all -- --check  /  | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 92 | Commands |  /  Scoped Rust check Clippy  /  python scripts/dev-check.py -p <crate> plus needed features/targets… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 93 | Commands |  /  Scoped tests  /  cargo nextest run --profile ci -p <crate> --locked --timings -E '<test expression>… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 94 | Commands |  /  Integration preflight  /  python scripts/check-service-clients.py --runtime-dir <matched-debug-pair… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 95 | Commands |  /  Doctests  /  cargo test --doc --workspace --locked --timings  /  | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 96 | Commands |  /  Extension type check / build  /  npm run compile --prefix packages/vscode; tests: npm test --pref… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 97 | Commands |  /  Admin build / verify / tests  /  npm run build --prefix packages/admin; npm run check --prefix pa… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 98 | Commands |  /  Read-surface parity  /  Use the per-surface selectionsdocs/read-interface-parity.md#current-per-s… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 99 | Commands |  /  Final workspace gate  /  cargo clippy --workspace --all-targets --all-features --locked --timings -… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 100 | Commands |  /  Package tested native pair  /  python scripts/package-native.py --target <native-target> --runtime-… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 101 | Commands |  /  Package compiled universal VSIX  /  python scripts/package-extension.py --channel stable --output a… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 102 | Commands |  /  Repository / whitespace checks  /  pwsh -NoProfile -File scripts/check-repo.ps1; git diff --check… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 104 | Commands | Nextest is pinned to 0.9.148; the ci profile disables retries and excludes long campaigns by def… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 106 | Commands | Build the matched debug pair with python scripts/build-service-runtime.py --destination <matched-de… | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 108 | Commands | Do not run cargo check before an equivalent Clippy run; it compiles the same code twice. | [Preserved requirement](../CONTRIBUTING.md#agent-command-reference) |
| 112 | Definition of done | Select checks by changed behavior and the claim being made, using the owning contract, actual caller… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 114 | Definition of done |  /  Change  /  Checks  /  What it proves  /  | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 115 | Definition of done |  / --- / --- / --- /  | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 116 | Definition of done |  /  Prose or agent instructions  /  Relevant content/links, repository check and whitespace; repository … | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 117 | Definition of done |  /  Rust in one crate  /  Format, scoped dev-check and focused regressions, including refusal/recovery; … | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 118 | Definition of done |  /  Platform-only Rust  /  Scoped Clippy first on that platform, then affected regressions  /  Changed pla… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 119 | Definition of done |  /  Component interface/state/invariants/lock order/test commands  /  Applicable checks plus owning CONT… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 120 | Definition of done |  /  Shared native contract  /  Reviewed contract and fixtures landed before consumers; affected consumer… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 121 | Definition of done |  /  Structural move  /  Exact bytes for digested/persisted formats, parsed equality elsewhere; unchanged… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 122 | Definition of done |  /  CLI/session, Admin, Workbench or MCP reads  /  Per-surface selectionsdocs/read-interface-parity.m… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 123 | Definition of done |  /  Frontend callback/view  /  Typecheck, affected behavior tests, regenerated asset verification; embed… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 124 | Definition of done |  /  Harness/script  /  Focused script tests and affected consumers; no unrelated native rebuild  /  Harnes… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 125 | Definition of done |  /  Installer, persistence or process ownership  /  Cheap failure/recovery regressions, then affected re… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 126 | Definition of done |  /  Release or platform outcome  /  Final workspace gate, doctests, runtime/package checks and active pl… | [Preserved requirement](../CONTRIBUTING.md#agent-validation-matrix) |
| 130 | Authority | Do without asking: | [Preserved requirement](../AGENTS.md#authority) |
| 132 | Authority | - Complete the authorized outcome end to end, across approved component and milestone boundaries. Pr… | [Preserved requirement](../AGENTS.md#authority) |
| 133 | Authority | - Make routine implementation, diagnostic, regression, documentation and validation choices. | [Preserved requirement](../AGENTS.md#authority) |
| 134 | Authority | - Continue in-scope corrections after failures. A failed-run count does not require reapproval. | [Preserved requirement](../AGENTS.md#authority) |
| 135 | Authority | - Commit and push reviewed milestones within the branch/remote scope recorded in Current work. | [Preserved requirement](../AGENTS.md#authority) |
| 137 | Authority | Ask first: | [Preserved requirement](../AGENTS.md#authority) |
| 139 | Authority | - A genuinely unresolved user-owned decision, or a material scope expansion. | [Preserved requirement](../AGENTS.md#authority) |
| 140 | Authority | - A new crate, unless the active plan already approves it and its extraction criteria are met. | [Preserved requirement](../AGENTS.md#authority) |
| 141 | Authority | - A change to wire shapes, digest inputs or persisted formats without a recorded contract decision. | [Preserved requirement](../AGENTS.md#authority) |
| 142 | Authority | - Anything requiring human consent. | [Preserved requirement](../AGENTS.md#authority) |
| 144 | Authority | Never without explicit authorization for that action: publish, deploy, merge, open a PR, or message … | [Preserved requirement](../AGENTS.md#authority) |
| 146 | Authority | While one dependency is blocked, continue independent work. Keep unrelated improvements out of the p… | [Preserved requirement](../AGENTS.md#authority) |
| 148 | Authority | Report facts, correct problems and continue. Do not apologize, invent approvals or claim results you… | [Preserved requirement](../AGENTS.md#authority) |
| 152 | Product invariants | - Preserve collection/root isolation, indexed revision identity, original document locations, budget… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 153 | Product invariants | - Never stage, package or publish temp/, .airk/ and private fixture/state subpaths under artifa… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 154 | Product invariants | - Do not add unapproved cloud providers, external databases, sidecars or agent orchestration. | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 155 | Product invariants | - Munarium, VCP and temp/ are inspiration. Do not copy their implementation. | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 156 | Product invariants | - Preserve unrelated user work, ignore patterns, source SPDX headers and dependency license notices. | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 157 | Product invariants | - Never alter identities or weaken a check to obtain a pass. | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 161 | Read-surface parity | airkctl CLI/session, Admin, Workbench and MCP reads go through one canonical implementation and must… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 163 | Read-surface parity | - Preserve all supported inputs and result fields across commands, structured views, copies and expo… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 164 | Read-surface parity | - Disclose truncation and selected subsets. | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 165 | Read-surface parity | - Regressions compare full equivalent results and input catalogs, normalizing only documented transp… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 166 | Read-surface parity | - Exercise native handlers and the real transports wherever they reshape results. Renderer mocks alo… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 170 | Contracts and structure | - Shared native contracts have one owner, in Rust. Extend the Rust owner and use the approved genera… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 171 | Contracts and structure | - Existing airk-state/1 formats need retained-fixture schema-identity proof, or an approved new st… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 172 | Contracts and structure | - Keep behavior changes and mechanical restructuring in separate, distinguishable steps so each can … | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 173 | Contracts and structure | - Add modules inside the owning crate. Untangle inside the crate before extracting. Keep new items p… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 174 | Contracts and structure | - Do not add Rust #path mounts thin platform switches excepted, include! of Rust source, or … | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 175 | Contracts and structure | - Keep exhaustive per-plane dispatch. Registries may consolidate duplicated catalogs, with one handl… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 176 | Contracts and structure | - Delete code only with evidence: it is uncompiled, it has no caller after a clean compile and Clipp… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 177 | Contracts and structure | - Test expectations must be independent of the code under test. Do not generate expected operation s… | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 178 | Contracts and structure | - Line counts and cosmetic splitting are not completion criteria. | [Preserved requirement](../AGENTS.md#source-and-architecture) |
| 182 | Validation | Cheap first: | [Preserved requirement](../AGENTS.md#working-procedure) |
| 184 | Validation | - Start with the cheapest useful diagnostic. Scoped tests, type checks and compiler diagnostics may … | [Preserved requirement](../AGENTS.md#working-procedure) |
| 185 | Validation | - Batch related regressions and compatible test configurations. | [Preserved requirement](../AGENTS.md#working-procedure) |
| 186 | Validation | - A filtered run must report its selected count. Zero selected is a failure. | [Preserved requirement](../AGENTS.md#working-procedure) |
| 187 | Validation | - Recheck only affected consumers during iteration. Reuse unchanged artifacts and passed evidence un… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 189 | Validation | Behavior defects: | [Preserved requirement](../AGENTS.md#working-procedure) |
| 191 | Validation | - Trace the affected flow and its sibling callers. Keep a focused reproduction. Fix the whole known … | [Preserved requirement](../AGENTS.md#working-procedure) |
| 192 | Validation | - Review the ownership, persistence, concurrency, recovery and platform boundaries the change touche… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 193 | Validation | - Add a cheap seam when an expensive failure cannot otherwise be diagnosed locally. | [Preserved requirement](../AGENTS.md#working-procedure) |
| 195 | Validation | Expensive runs builds, installed journeys, qualification confirm reviewed code. They are not the d… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 197 | Validation | - Review the combined change before an expensive run or corrective push. With delegated work, the pr… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 198 | Validation | - Freeze consumed inputs for expensive runs; retain validation evidence as specified in Task ledger … | [Preserved requirement](../AGENTS.md#working-procedure) |
| 199 | Validation | - On failure, stop the batch and collect its diagnostics before correcting. | [Preserved requirement](../AGENTS.md#working-procedure) |
| 200 | Validation | - After a repeated failure, produce a revised diagnosis and cheap regression evidence before the nex… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 201 | Validation | - Never retry unchanged failed inputs until green, weaken assertions, widen limits or retries, or ma… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 203 | Validation | Tests: | [Preserved requirement](../AGENTS.md#working-procedure) |
| 205 | Validation | - Keep decision logic testable across hosts. Inject clocks and observers for deadlines and state tra… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 206 | Validation | - No fixed sleeps. Wait on observed events under a deadline. | [Preserved requirement](../AGENTS.md#working-procedure) |
| 207 | Validation | - Do not hide flakes with retries or relaxed limits. A quarantine needs an owner, a reason and an ex… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 208 | Validation | - Keep assertion-only tests outside production modules where practical, to avoid release recompilati… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 212 | Build and host | Primary development host: Windows with PowerShell pwsh; commands use PowerShell quoting and repo… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 214 | Build and host | - One Cargo owner per host and target directory at a time. Windows and Mac Neo may build concurrentl… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 215 | Build and host | - Reuse the persistent checkout target or the existing isolated target see Current work. Do not ro… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 216 | Build and host | - Development incremental defaults: on for Windows, off elsewhere. Do not change release or CI polic… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 217 | Build and host | - Keep Cargo --timings for necessary builds and Clippy. | [Preserved requirement](../AGENTS.md#working-procedure) |
| 218 | Build and host | - Reuse the pinned Nextest profiles and integration preflights from CONTRIBUTING.md. | [Preserved requirement](../AGENTS.md#working-procedure) |
| 219 | Build and host | - Mac checks: see Current work for the host. Batch them over the existing SSH setup. Disk there is l… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 223 | Parallel agents | Use them when independent investigation, implementation or review helps. Give each clear file owners… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 227 | Navigation | - Once per session, discover the AIRK MCP tools and call status to confirm project binding. If they … | [Preserved requirement](../AGENTS.md#working-procedure) |
| 228 | Navigation | - Prefer AIRK tools for scoped indexed navigation. The index may be stale: keep the revision identit… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 229 | Navigation | - Use rg for live verification and anything the index does not cover. Editing, builds, tests and G… | [Preserved requirement](../AGENTS.md#working-procedure) |
| 233 | Task ledger and reporting | Record substantive work in the existing ledger see Current work: outcome, decisions, blockers, val… | [Preserved requirement](../AGENTS.md#validation-and-completion) |
| 235 | Task ledger and reporting | At handoff distinguish findings, reproductions, hypotheses, passes and skipped/unavailable checks. S… | [Preserved requirement](../AGENTS.md#validation-and-completion) |
| 239 | Documentation, versions and commits | - Update documentation affected by implemented behavior. Keep dated historical records identifiable … | [Preserved requirement](../AGENTS.md#validation-and-completion) |
| 240 | Documentation, versions and commits | - Before a commit/push/PR cycle, check README, the documentation index, release notes and Marketplac… | [Preserved requirement](../AGENTS.md#validation-and-completion) |
| 241 | Documentation, versions and commits | - Keep native and extension versions and lockfiles synchronized. Honor an approved or completed bump… | [Preserved requirement](../AGENTS.md#validation-and-completion) |
| 242 | Documentation, versions and commits | - Every commit uses the authorized human contributor's configured identity and git commit -s. Veri… | [Preserved requirement](../AGENTS.md#validation-and-completion) |
| 246 | Glossary | - **Receipt**: A retained result/evidence record tied to request, source or artifact identity; nativ… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 247 | Glossary | - **Qualification**: Evidence from the required runtime, transport, package and platform checks on i… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 248 | Glossary | - **Plane**: A protocol/authority boundary, such as evidence reads, local control or browser Admin m… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 249 | Glossary | - **Fence order**: The owning contract's order of locks and revision/authority checks that prevents … | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 250 | Glossary | - **Arbiter**: The approved optional local evidence-gathering/answer capability inside the existing … | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 251 | Glossary | - **P10**: The Arbiter package in the advanced-split plan. Historical build receipts also used this … | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 252 | Glossary | - **Mac Neo**: The existing macOS arm64 SSH qualification/signing host; location and ignored connect… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 253 | Glossary | - **I1**: Same-user local IPC control request/frame contract owned by airk-ipc. | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 254 | Glossary | - **I2**: airkctl caller contract: commands, exit/error/version documents and session protocol. | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 255 | Glossary | - **I3**: Browser Admin HTTP envelope, authentication/admission and operation contract. | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 256 | Glossary | - **Generation pilot**: The approved Rust schema-to-TypeScript generation path for I2/I3; semantic r… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 257 | Glossary | - **Compatibility row**: A reviewed compatibility-inventory entry identifying retained behavior, its… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 258 | Glossary | - **Installed journey**: An end-to-end test against an installed product, including its actual host/… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 259 | Glossary | - **Matched debug pair**: airk and airkctl built/staged together by the runtime builder with matchin… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 260 | Glossary | - **Isolated target**: The persistent Cargo build/cache directory assigned to this campaign; its pat… | [Preserved requirement](architecture/airk-architecture.md#agent-workflow-glossary) |
| 264 | Current work | Active campaign: Arbiter final implementation execution start 2026-11-16T05:29:42Z. | [Preserved requirement](agent-current-work.md) |
| 266 | Current work |  /  Need  /  Source  /  | [Preserved requirement](agent-current-work.md) |
| 267 | Current work |  / --- / --- /  | [Preserved requirement](agent-current-work.md) |
| 268 | Current work |  /  Ordered implementation plan and D01–D08 user decisions  /  docs/arbiter/final-plan.md  /  | [Preserved requirement](agent-current-work.md) |
| 269 | Current work |  /  Known issues and historical candidate evidence  /  docs/arbiter/knownissues.md  /  | [Preserved requirement](agent-current-work.md) |
| 270 | Current work |  /  Weight-experiment rationale  /  docs/arbiter/weights-v2-plan.md  /  | [Preserved requirement](agent-current-work.md) |
| 271 | Current work |  /  Progress ledger  /  docs/simplify/one-process-ledger.md  /  | [Preserved requirement](agent-current-work.md) |
| 272 | Current work |  /  Existing split decisions and contracts  /  docs/split/split-plan.md, docs/split/contracts.md  /  | [Preserved requirement](agent-current-work.md) |
| 273 | Current work |  /  Standing simplification decisions  /  docs/simplify-plan.md  /  | [Preserved requirement](agent-current-work.md) |
| 275 | Current work | - Branch: feat/arbiter-final, created from feat/advanced-split. The user authorized this branch and … | [Preserved requirement](agent-current-work.md) |
| 279 | Current work | - Current task: implement all steps of docs/arbiter/final-plan.md, authorized by the user on 15 Nove… | [Preserved requirement](agent-current-work.md) |
| 283 | Current work | - The final plan supersedes the old advanced-split execution deadline and campaign ordering. Preserv… | [Preserved requirement](agent-current-work.md) |
| 285 | Current work | - For this authorized implementation, the user superseded the original 24-hour target/48-hour cap, t… | [Preserved requirement](agent-current-work.md) |
| 290 | Current work | - D08 now includes FP32 GPU execution first, then FP16 with efficient attention, both available for1… | [Preserved requirement](agent-current-work.md) |
| 293 | Current work | - Preserve the selected bundled MiniLM plus independent optional Focus Qwen3-Reranker-0.6B and Con… | [Preserved requirement](agent-current-work.md) |
| 298 | Current work | - The approved qualification target is a new default of 8192 shared input tokens and 120 seconds, in… | [Preserved requirement](agent-current-work.md) |
| 303 | Current work | - Automatic agent-config edits require a reviewed safe policy for the entire file and recovery path;… | [Preserved requirement](agent-current-work.md) |
| 306 | Current work | - Current-version index regeneration is authorized when upgrade cannot preserve the current contract… | [Preserved requirement](agent-current-work.md) |
| 310 | Current work | - Resume from live source, the final plan, ledger and exact receipts. Keep contract-first ordering, … | [Preserved requirement](agent-current-work.md) |
| 314 | Current work | - User exception approved during implementation: reviewed in-scope tracked-file content edits may pr… | [Preserved requirement](agent-current-work.md) |
| 318 | Current work | - Restart checkpoint requested on 17 November 2026. Read the final restart checkpoint in docs/simpli… | [Preserved requirement](agent-current-work.md) |
| 326 | Current work | - The user explicitly authorizes repairing foreign ownership in this repository to TYAI\tdotn, SID S… | [Preserved requirement](agent-current-work.md) |
| 333 | Current work | - Retained training data, checkpoints, calibration/reference inputs and receipts under artifacts/ ar… | [Preserved requirement](agent-current-work.md) |
| 338 | Current work | - Build target for this execution: target/ human-owned warm checkout target. The designated artifa… | [Preserved requirement](agent-current-work.md) |
| 341 | Current work | - Mac Neo: existing SSH setup, POSIX sh batches, checkout ~/code/airk; connection details in ignored… | [Preserved requirement](agent-current-work.md) |
| 345 | Current work | - Private state includes temp/, .airk/, artifacts/one-process/native-fixtures*/, artifacts/editor-qu… | [Preserved requirement](agent-current-work.md) |

Independent preservation review restored the general independence requirement for test expectations and the explicit fixtures exclusion. Commands and validation matrix were compared verbatim; identity, scoped exceptions, campaign permissions, deadline and historical facts were reviewed against the baseline.
