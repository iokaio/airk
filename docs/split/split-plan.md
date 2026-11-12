# Split AIRK: Service, airkctl and VS Code Extension

**Status:** approved plan, revision 4, updated 11 November 2026, on `feat/split`
(created from `feat/simplify` at `59dd3c721ca4c8301664827abaa9ea2ac6fd8dc8`).
Implementation, local builds, local signing, per-installer version bumps,
commits on `feat/split`, targeted hosted CI runs, tool installation, new
dependencies within policy, Docker/Neo checks, Neo signing, deletions confirmed
by independent review, Phase 8 qualification and the 1.9.0 bump are authorized
as U42–U69 record. Pull requests, merges and publication are not. The stopped
installed-editor repair stays superseded (U23). The inherited source version
is 1.8.23.

| Revision | Change |
|---|---|
| 1 | Two products: a standalone service installer and a UI-only extension that discovers the service. |
| 2 | Adds `airkctl`, a service-only `airk`, browser admin pages on the HTTPS endpoint, an extension that reaches the service only through `airkctl`, a fixed-layout installer that removes pre-split installations, and code and installer maps. Records the user's decisions of 10 November 2026. |
| 3 | Clarifies common-contract ownership, UI boundaries, recovery and deployment; maps CI selection, artifact reuse, signing and publication; adds force-local Windows installer/VSIX scripts, phase dependencies and acceptance evidence. Records the additional user decisions U18 onward. |
| 4 | Records the answers to O1–O12 and the remaining planning questions as U23–U83 (11 November): custom program and data/index locations, macOS setup fallback, phase gates, I2/I3 type generation, commit, test-machine, signing, CI, tool, dependency, agent and failure-handling authorizations. |

**Reading order.** §2 lists what the user has decided and what still needs an
answer. §4 defines the boundary between extension code and service code. §5–§9
specify each piece. §10 and §9.5 are the code and installer maps; §10.5 maps
the CI and release changes. §11 is the order of work. Unanswered O decisions
remain gates; the detailed design below does not approve them implicitly.

## 1. Outcome

AIRK ships as two separately installed products built from four deliverables.

| Product | Contains | Installed by |
|---|---|---|
| **AIRK Service** | `airk` (the resident service, no CLI), `airkctl` (the whole CLI), the admin pages (embedded in `airk`), default models and notices | A signed per-platform installer from **https://airk.ioka.io**, with versioned assets in [iokaio/airk GitHub Releases](https://github.com/iokaio/airk/releases). Works without VS Code. |
| **AIRK VS Code Extension** | Explorer and Workbench for search, evidence, review and strategies; status; agent connection for the workspace | The Marketplace or a VSIX. Contains no native code and installs none. |

~~~mermaid
flowchart LR
    B[Local browser] -->|HTTPS /admin, signed-in session| H
    A[Coding agents and local programs] -->|HTTPS /mcp and /api/v1, bearer| H
    E[VS Code extension] -->|child process only| C[airkctl]
    U[Terminal user] --> C
    C -->|owner-checked named pipe or Unix socket| M
    I[Service installer] -->|signed setup step| S
    subgraph S[airk: one resident service per user]
        H[Loopback HTTPS listener]
        M[Control and query endpoints]
    end
~~~

The split stays in this repository. It adds two new Rust crates, approved
as U27, and two new front-end packages (§10); revision 1's statement that no
new crate is needed no longer fits the lightweight `airkctl` boundary.

Dependency direction and the single resident service in
[architecture](../architecture.md) stay in force. Contract, deletion and review
rules in [the simplification plan](../simplify-plan.md) apply to every step
here. Simplification work that touches a split-owned component is folded into
the split's phases (U35); this plan starts no unrelated simplification
milestone.

## 2. Decisions

### 2.1 Recorded user decisions (10–11 November 2026)

These came from the user's request and answers. They are not agent choices.

| # | Decision |
|---|---|
| U1 | Two separately installed products. Installing or updating the extension never installs or replaces the service. |
| U2 | Windows and macOS installers are signed. Linux signing is optional. |
| U3 | `airkctl` is a separate, signed, lightweight CLI installed by the service installer. It carries the whole user CLI and talks to the service over the local pipe. |
| U4 | `airk` exposes no CLI. Started by hand, it reports that the service is already running, or starts it and explains how to stop it with `airkctl`. |
| U5 | Admin pages are served over HTTPS on the same host and port as the MCP endpoint, for a local browser user. |
| U6 | Management is admin-only. Index library, storage, service lifecycle, grants and recovery are managed in the admin pages and through `airkctl`, not in the extension. The extension keeps search, evidence, review, strategies and Workbench, shows status and opens the admin pages. |
| U7 | The first release of the admin pages also has search and evidence browsing. It has its own index selection and can use any index or index set the service holds. |
| U8 | The extension has no direct communication with the service. Everything, including Workbench reads, goes through `airkctl`. |
| U9 | The extension runs one long-lived `airkctl session` per window, plus one-shots for simple calls. |
| U10 | Explorer Assistance stays. Drafts reach the window through its `airkctl` session; the service never connects to the extension. |
| U11 | `airkctl` owns writing agent MCP configuration. The extension calls it. The admin pages manage connections and show the command to apply. The service never writes into project folders, and the installer never edits agent configuration. |
| U12 | The extension leaves any pre-split AIRK installation alone. It finds the split service and updates AIRK's MCP entries. If no service is installed it still removes the old AIRK MCP entries, and Explorer says that this happened. |
| U13 | The service installer stops old AIRK processes and services and deletes old indexes and old installed files. It first tells the user that this will happen, that indexes must be rebuilt, and that no source file is affected. |
| U14 | Removal is receipts-only. The installer removes only items that carry an AIRK ownership receipt, installs anyway, and reports what it left. |
| U15 | Fixed layout (folder choice amended by U39). The platform installer places files in one fixed per-user folder and replaces them in place. A small non-interactive setup step does old-install removal, login startup, certificate trust and start. Staged slots, rollback copies and organization-managed payloads are not carried into the split. |
| U16 | The executable that performs the setup step must be signed by CI. macOS rejects it otherwise. |
| U17 | One service per user account. A preview build replaces the stable one. |
| U18 | Keep Preact and esbuild for the shared UI and admin pages, using the existing frontend stack. |
| U19 | CI builds admin assets from locked source before compiling `airk`, embeds them, and records their digest in build and release receipts. Generated admin bundles are not checked into Git. |
| U20 | Keep `airkctl service uninstall` as a thin launcher for the signed setup/uninstall role. Setup owns all removal logic. This is a fifth narrow exception to pipe-only operation. |
| U21 | Both products must be buildable and installable locally before CI. Provide scripts to force a local service-installer build and a local VSIX build, at least on Windows. Add these scripts as required split-plan deliverables; do not implement current-product scripts in this planning change. |
| U22 | Local Windows builds use this machine's access to the same signing certificate/identity used by CI (11 November). Sign the native payload, setup executable, uninstaller and installer under the same verification policy. Use the ordinary VSIX with normal signature enforcement; no unsigned-test bypass is needed for this path. |
| U23 | (O8, 11 November) The stopped installed-editor repair (D10) and the untested HTTP-cleanup correction are superseded by the split. Neither is resumed; their evidence is kept as history. |
| U24 | (O2, 11 November) The contract changes are approved under the simplification plan's D8: new control methods (§5.2), the `airkctl` output and session contract (I2), the admin HTTP contract (I3) and a two-executable install record (I4). Evidence tool results and digest inputs do not change. Each contract still lands as its own reviewed step with independent fixtures before any consumer changes. |
| U25 | (O3, 11 November) The HTTPS boundary change is approved for the admin plane in §7, including management. The dedicated security review in §7.2 must still specify the launch-code exchange before admin code is written. |
| U26 | (O4, 11 November) The split keeps `airk-state/1` for registry, catalog, collection and credential formats. New installs use state, data and cache folder names no pre-split build used. Any change to a layout pin that `airk-state/1` owns needs a schema-identity proof against retained fixtures or a new user-approved state contract. |
| U27 | (O1, 11 November) Two new crates are approved: `airk-ipc` (library) and `airkctl` (binary). IPC is first untangled in place inside `airk-cli` and extracted only when the §10.1 criteria hold. `airkctl` depends only on `airk-core`, `airk-process` and `airk-ipc`. |
| U28 | (O6, 11 November) When the service has no index matching an AIRK MCP entry, the entry is removed and recorded, and Explorer offers to re-add it once an index exists. Rewriting an entry to a live index keeps today's single confirmation because it creates a grant. |
| U29 | (O12, 11 November) The extension ships as one universal VSIX, after Marketplace targeting and promotion of the existing platform-specific versions are audited. A universal VSIX never replaces native platform evidence. |
| U30 | (O5, 11 November) Every pre-split installation is removed with its indexes (U13). Split-to-split same-build repair and upgrades with a matching state contract keep indexes. This narrows the "through 1.9.0, same build also resets" rule to pre-split installations. |
| U31 | (O7, 11 November) The first split release is version 1.9.0. The products are named **AIRK Service** and **AIRK VS Code Extension** (§1). |
| U32 | (O9, 11 November) Uninstalling the split service asks whether to delete indexes. An unattended uninstall keeps them; an explicit purge removes them. |
| U33 | (O10, 11 November) The setup step is a private, non-interactive role of the signed `airk` image. No third executable is added. |
| U34 | (O11, 11 November) The `airk-cli` crate is renamed to `airk-service` as a scripted mechanical step, last, before final inputs are frozen. |
| U35 | (11 November) The split is the structural work after Milestone S1. Remaining simplification-plan work that touches a split-owned component is done inside the split's phases, under the simplification plan's rules and review gates. Other simplification milestones wait. |
| U36 | (11 November) The §9.5 install paths are approved as defaults; U39 lets the user choose others. Phase 0 still verifies that none collides with a pre-split root. |
| U37 | (11 November) The §10.5.1 local build entry points and their `-ForceBuild` interface are approved: `scripts/build-local-installers.ps1`, `scripts/build-local-service-installer.ps1` and `scripts/build-local-vsix.ps1`. |
| U38 | (11 November) The §7.2 dedicated security review is performed by an independent agent against the written specification. The user reviews its findings and approves before admin code is written (amended by U55). |
| U39 | (11 November) The service installer lets the user choose a different program folder and a different location for index data and service state, and for cache and logs. The defaults are the §9.5 paths. Only folders owned by the current user and not writable by others are accepted, with no elevation; machine-wide folders are refused. A small owner-only pointer always stays at a fixed per-user path and names the chosen locations. `airkctl`, the extension and later installers find the installation only through that pointer; there is still no editor setting for the location. This amends U15 and U36: the layout inside the chosen program folder is fixed, and the folder itself is chosen. |
| U40 | (11 November) If the macOS `Install AIRK.command` wrapper fails Gatekeeper from the notarized DMG, setup runs from inside the notarized `AIRK.app` on first launch instead. No installer package is introduced. |
| U41 | (11 November) CLAUDE.md and AGENTS.md wording that names `airkctl` as the CLI, the `airkctl` window session and the admin pages as parity surfaces is pre-approved, in the phase that makes it true. Both files stay byte-identical. Any other change to them still needs the user's approval. |
| U42 | (11 November) Completed, reviewed steps are committed on `feat/split` with `git commit -s` under the user's configured identity, without asking each time. No agent trailers. Pull requests, merges and publication still need the user's authorization. |
| U43 | (11 November) Installed-product tests (T2) may run on this Windows machine, including installs that remove its current AIRK installation and indexes. The user handles OS consent dialogs, such as certificate trust, when asked (see U56). |
| U44 | (11 November) Azure Trusted Signing access on this machine is set up for non-interactive local signing; the user keeps it working or reports a change. Pushing `feat/split` and running specific hosted CI or qualification workflows is pre-authorized for platform questions that cannot be answered locally (Linux, unsigned macOS checks). Hosted signing environments allow only `main`, so split-branch macOS signing runs on the Neo (U60) and Intel Mac evidence waits for `main` (U66). Publication is not. |
| U45 | (11 November) Implementation of this plan is authorized, starting with Phase 0, without a stop at the end of Phase 0 or at later phase boundaries. Work still stops for a named blocker, the U52 failure rule, a U55 high or critical security finding, a material scope change, or anything that needs authorization not granted here (pull requests, merges, publication). |
| U46 | (11 November) Interim split installers keep the standing per-build patch increments on 1.8.x (1.8.24, 1.8.25, …). Version 1.9.0 is set only for the release candidate (U31). |
| U47 | (11 November) Simplification decision D5 is reopened for I2 and I3 only. Phase 1 runs a small generation pilot: Rust `schemars` JSON Schema to TypeScript types, with checked-in output and a drift check. Semantic rules such as read-retry eligibility stay hand-written and are tested against Rust-emitted cases. Other contracts keep D5's "fixtures first, no type generation". U24 is the approved D8 exception for the split's wire additions in 1.9.0. |
| U48 | (11 November) Missing local prerequisites may be installed: Inno Setup at the CI-pinned version and `cargo-nextest` at the CI-pinned version with `--locked`. Tools are called by full path; PATH is not edited. |
| U49 | (11 November) This repository's `.mcp.json` and `.codex/config.toml` may be updated in the phase that makes the new `airkctl` entries valid, documented in that commit. |
| U50 | (11 November) The §9.1–§9.2 location rules are approved: a chosen folder must be empty or owned by this split installation; the pointer is written last; later installs keep the locations in use; index data moves only through the service's storage relocation. |
| U51 | (11 November) For the split, CLAUDE.md's investigation checkpoints are recorded in the task ledger and do not stop the work. Work stops only for a decision or a blocker outside existing authorization. |
| U52 | (11 November) For the split, the "two strikes" rule becomes three: after three failed expensive (T2/T3) runs for one component, stop running, write the root-cause note, complete the correction with the missing cheap regression, then run again. This does not require waiting for the user. |
| U53 | (11 November) Parallel subagents and Workflow orchestration may be used for independent investigation, implementation and review, within the default workflow size. Builds, tests and platform batches still go through one owner. |
| U54 | (11 November) New third-party crates and npm packages may be added without asking each time when they are well maintained, license-compatible, pinned and locked, and pass `dependency-audit.py` and `license-notices.py`. Each addition is reported in the phase summary. |
| U55 | (11 November) Amends U38: when the independent security review leaves no high or critical finding, medium and low findings are fixed in the specification, recorded, and admin work continues without waiting. A remaining high or critical finding, or a fix that would change a U decision, stops for the user. |
| U56 | (11 November) Automated installed-product runs decline certificate trust and check the admin pages with a test client that pins the installation issuer. One final run with real CurrentUser Root trust waits for the user to approve the OS dialog. |
| U57 | (11 November) No pull request is opened by the agent. The user opens the PR from `feat/split`; merging and publication stay with the user. |
| U58 | (11 November) Docker on this machine (Linux) and the Mac Neo over the existing SSH setup (macOS arm64) may be used for scoped checks without asking. The Neo target is cleaned after each batch, keeping needed evidence first. |
| U59 | (11 November) The extension README shown on the Marketplace (`packages/vscode/README.md`) must explain that the extension needs the separately installed AIRK Service, link to the download page (**https://airk.ioka.io**) and the versioned [GitHub Releases](https://github.com/iokaio/airk/releases), and give short per-platform install steps (Windows, macOS, Linux) plus how to confirm the service is running (`airkctl service status`). |
| U60 | (11 November) The hosted `macos-signing` and `native-signing` environments allow only `main`, so split-branch macOS signing runs on the Mac Neo: push `feat/split`, have the Neo pull it over the existing SSH setup, and sign there with the same Developer ID identity CI uses. As with U22, local signatures prove signed local builds, not hosted release provenance. |
| U61 | (11 November) Deletion rows in the compatibility inventory (simplification D2) are reviewed by an independent agent against the evidence classes. A row it confirms authorizes deletion; the user may audit the inventory at any time. |
| U62 | (11 November) At Phase 8 the full qualification workflow and the local T3 ladder run once per host on frozen inputs without asking, with reruns under U52. |
| U63 | (11 November) At the start of Phase 8 the agent sets version 1.9.0 in every manifest, lockfile and versioned document, and writes the 1.9.0 release notes and Marketplace text, so qualification runs on the release version and the branch is ready for the user's PR. |
| U64 | (11 November) The Neo checkout is `~/code/airk`. Its connection details stay in the git-ignored `artifacts/v190/neo-connection.local.json`, never in tracked files. |
| U65 | (11 November) The Neo holds AIRK's own signing keychain `~/Library/Keychains/airk-signing.keychain-db` with the `notarytool` profile `airk-notary` ([native signing](../native-signing.md)), so split-branch macOS arm64 builds are signed, notarized, stapled and Gatekeeper-checked there. |
| U66 | (11 November) No Intel Mac evidence is produced before the user's PR. macOS x64 signing and qualification run in hosted CI on `main` after the user merges; until then no x64 macOS claim is made. |
| U67 | (11 November) When work is blocked on the user (OS trust dialog, a U55 high or critical finding, a new decision), the agent sends a push notification, records the blocker in the task ledger and continues independent work. |
| U68 | (11 November) The Neo's existing checkout `~/code/airk` (clean, on `feat/finetuning`) is switched to `feat/split` for split work. No uncommitted work may be discarded; a dirty checkout stops the batch. |
| U69 | (11 November) Over SSH the Neo's login keychain is locked. The user creates an owner-only password file once at `~/.airk-signing/keychain-pass` (mode 600) on the Neo. Each signing batch unlocks the login keychain with it, reads the `airk-signing-keychain-password` item there into memory to unlock `airk-signing.keychain-db` (as [native signing](../native-signing.md) documents), signs and notarizes with `--keychain-profile airk-notary` and that keychain, and locks both keychains again, including after a failure. The password never enters the repository, logs or receipts. |
| U70 | (11 November) This machine has no code-signing certificate, and the repository signs locally only with a certificate thumbprint. Local Windows signing therefore installs the pinned Microsoft Trusted Signing client outside the repository and any AIRK root, and adds a local adapter to `scripts/sign-native.py`: `signtool /dlib` with the same account and certificate profile as CI and the Azure CLI credential, verified with the existing publisher policy. The hosted signing path and its guards are unchanged; local receipts stay distinct from hosted provenance. |
| U71 | (11 November) Simplification D4 is resolved for two untracked harnesses: the receipt runner (`artifacts/one-process/runner.py`) and golden capture (`artifacts/one-process/golden/capture.py`) are copied into committed scripts under `scripts/dev/`, and reviewed, normalized CLI goldens are committed under `fixtures/cli-golden/` with no absolute paths or user names. The `artifacts/` originals stay untouched. |
| U72 | (11 November) Amends §9.3 "no old binary is executed" for one case: an old macOS SMAppService login agent is unregistered by running only `runtime mac-login --action unregister` from the old installed binary, and only when its code signature and digest match its install receipt. Otherwise the agent is left and reported. No other old binary runs. |
| U73 | (11 November) An organization-managed pre-split installation (`managed-runtime.json`) is removed like any other pre-split installation when its AIRK ownership receipt proves it (U13, U14); otherwise it is left and reported. |
| U74 | (11 November) These indirect proofs count as U14 ownership receipts: (1) the old Inno `standalone` folder, `unins000.*`, uninstall registry key and shortcut, proven by the fixed path, the exact AppId key whose `InstallLocation` matches, and the pinned `runtime\airk.exe` digest; (2) the old extension's CLI copies and Windows PATH entry, proven by the extension-uninstall target receipt naming their storage folder; (3) AIRK-named leftovers (publish lock, extension-uninstall receipts) inside a receipt-owned universal-launcher folder; (4) 1.3 per-collection derived data under an old `--data-dir`, proven by the saved 1.3 service definition, removing only AIRK-owned files. Settings folders without a receipt are left and reported. The split Inno installer uses a new AppId. |
| U75 | (11 November) Recovery that must work while the service is down is done by `airkctl` locally: recording an error, resetting login startup, recovering from an expired HTTPS issuer, reading login status, and adding or removing the AIRK certificate trust (whose OS consent dialog the CLI can show). This extends §6.3; these operations still never open indexes, registries or stores, and each is listed in the I2 contract with its owner-private inputs. |
| U76 | (11 November) Workbench reference refresh no longer changes index configuration from the editor; Workbench shows a link that opens the refresh in the admin pages (U6). |
| U77 | (11 November) Selecting an existing index as a VS Code window's search scope issues a window-scoped read grant through that window's `airkctl session`, like the admin session's own grant. It never creates or changes indexes, index sets or agent grants. |
| U78 | (11 November) The macOS program folder is `~/Library/Application Support/AIRK/program/`, holding `AIRK.app` and its install record, so no installed file shares a folder with pre-split files. Retirement never removes the shared `AIRK` parent. |
| U79 | (11 November) The ten commands that use `--output <FILE>` today (Explorer draft and suggest commands) take `--output-file <FILE>` in `airkctl`, so `--output json` means the same thing on every command. This is the one approved exception to keeping argument names (§6.2); goldens compare with that rename applied. |
| U80 | (11 November) Amends U75: abandoning a stuck lifecycle record stays a service operation (at startup or over the pipe), because lifecycle records and the explicit-Stop flag live in the registry. `airkctl` gets no offline path for it. |
| U81 | (11 November) Update rustix to the patched 1.1.6 (RUSTSEC-2026-0486) with a reviewed exact-version exemption, and fix pre-existing unused code: remove it when it is truly unused, otherwise scope it so it is not reported. |
| U82 | (11 November) The Inno Setup compiler identity check in `package-standalone-windows.py` may change in Phase 7: the genuine 6.7.1 `ISCC.exe` reports ProductVersion 0.0.0.0, so identity moves to a pinned signer and digest. |
| U83 | (11 November) The user accepts the H1 and H2 fixes from the admin-plane security review (`contracts.md` §4.11): same-user peer admission with owner-only launch delivery, and no admin URL opened or forwarded from a remote extension host. |
| U84 | (11 November) `airkctl`'s native needs move into existing crates by committed scripts, one implementation each: the Unix owner-private file check joins `airk-ipc` beside its Windows twin; the per-user folder lookup (Windows Known Folder, Unix passwd home) and the detached-spawn primitive move to `airk-process`; `airk-cli` calls them. No new crate. |
| U85 | (11 November) `airkctl service stop`/`restart` are split-native now (step 3b.1e), not a port of the pre-split `runtime lifecycle`: the service records the request in its existing lifecycle ledger with the saved explicit Stop, answers with its instance and process id, drains and exits; `airkctl` waits for that process to exit, and Restart then runs `service start`, whose new instance completes the record. OS-manager ownership and forced stop join in Phase 7 with login startup; `runtime lifecycle` stays until it is retired. |
| U86 | (11 November) `airkctl service logs` has one log parser and text renderer for both paths: the pure parsing and rendering move from `airk-cli` `service_logs` into `airk-core` by committed script; the running service serves M13 with it, and `airkctl` reads the same owner-private files with it while the service is down (contracts §3.10.4), so output matches `airk service logs` in both cases. |
| U87 | (11 November) `airkctl service status` reports, in one document, the verified installation (as `airkctl version`), whether the service answers `Health` (its observation, or not running) and the pair; the saved Stop and a pending lifecycle record come from a small service read while it runs; login status joins in Phase 7. It replaces `runtime inspect`/`runtime diagnose`. |
| U88 | (11 November) Refines U86: recent log pages come from the running service through the existing `Logs {limit}` method; `--all` and `--raw` are always owner-checked local reads by `airkctl` with the shared parser, as `airk service logs` does today (the full export is never sent over service IPC); a stopped service's recent page is read locally too. No paged M13 method. |
| U89 | (11 November) Until Phase 7 the split service keeps its log rings beside the control root (where `log_parent` places them without a pre-split layout) and `airkctl service logs` reads there; Phase 7 setup makes the service write to the install record's `locations.logs` and `airkctl` follows the record. |
| U90 | (12 November) The offline HTTPS code `airkctl` needs (U75) moves into existing crates, as U84 did: the pure agent-credential token and credential-store parsing and verification into `airk-core`, the OS certificate-trust code into `airk-process`; `airk-cli` keeps thin callers. On-disk formats are unchanged (U26). |
| U91 | (12 November) Q8: `airkctl agents launch` starts Claude Code, Codex or VS Code with the credential in the child's environment (today's qualified set). The other nine `setup --client` values stay refused until each is qualified. |
| U92 | (12 November) Agent config entries name `<installed airkctl> agents credential --connection-id <id> --format header-json`. Only that form is recognized as AIRK-owned; entries with the pre-split `airk runtime http-credential …` helper are not touched by `agents list/repair/remove` (pre-split cleanup belongs to the Phase 7 retirement, §9.3). |

### 2.2 Formerly open decisions

All twelve were answered on 11 November 2026. Each row points to the §2.1
decision that records the answer.

| # | Question | Proposal | Needed before |
|---|---|---|---|
| O1 | Approve two new crates, as CLAUDE.md requires for extraction? | **Decided: U27.** | — |
| O2 | Approve the contract changes under the simplification plan's D8? | **Decided: U24.** | — |
| O3 | Approve the HTTPS boundary change? Today HTTP exposes no management and refuses every browser `Origin`. | **Decided: U25.** | — |
| O4 | Does the split keep `airk-state/1`? | **Decided: U26.** | — |
| O5 | What happens to indexes on later installs? | **Decided: U30.** | — |
| O6 | What does "update MCP entries" do when the service has no matching index, which is always true right after the first install? | **Decided: U28.** | — |
| O7 | Version and name of the first split release. | **Decided: U31.** | — |
| O8 | Is the stopped installed-editor repair (D10) and the untested HTTP-cleanup correction superseded? | **Decided: U23.** | — |
| O9 | Does uninstalling the split service delete indexes? | **Decided: U32.** | — |
| O10 | Which signed executable runs the setup step (U16)? | **Decided: U33.** | — |
| O11 | Rename the `airk-cli` crate to `airk-service`? | **Decided: U34.** | — |
| O12 | One universal VSIX or the existing platform catalog? | **Decided: U29.** | — |

O13 is resolved by U22: the user confirmed access to the CI signing identity
on this Windows machine. Neither proposed unsigned-test mode nor a separate
test certificate was selected.

U18–U22 were answered during revision 3. O1–O12 were answered on
11 November 2026 as U23–U34; the table keeps their numbers so earlier
references still resolve. Keep synchronized source versions and the current authenticated release train
until the user explicitly approves another policy. Separate installation and
I2 compatibility do not imply independent versioning or publication.

### 2.3 Consequences that follow from the decisions

- **Start must exist outside the admin pages.** A stopped service cannot serve
  them. The extension and `airkctl service start` keep Start; running `airk`
  also starts it. Stop and restart live in the admin pages and `airkctl`.
- **Removing entries without a service leaves agents without AIRK.** If a
  pre-split service is still running, it keeps running with no clients until
  the service installer removes it. Explorer must say both things (U12).
- **The extension keeps removal-only configuration code.** When `airkctl` is
  absent, nothing else can remove the old entries. All creation and rewriting
  goes through `airkctl` (U11).
- **Features that end with the split, by user decision:** staged runtime
  slots, rollback and pinning, organization-managed payloads, side-by-side
  channels, extension-driven install and repair, shell integration written by
  the extension, and management screens inside the extension.
- **Receipts-only removal can leave things behind,** including an old login
  task that restarts an old service at sign-in. The installer reports this
  prominently (§9.3).
- **The extension loses its bundled digest as a trust anchor.** It verifies
  `airkctl` by signature on Windows and macOS. On Linux it can only check the
  owner-private install record, which is weaker and is documented as such.
- **Linux browsers will not trust the local certificate automatically.**
  Per-user Linux trust enrollment is unsupported today.
- **`airkctl` has five narrow uses that are not pipe requests** (§6.3):
  starting `airk`, reading the install record, the agent credential helper,
  reading logs when the service is down, and delegating uninstall to setup.

## 3. Baseline (source review, 10 November 2026)

Three read-only investigations mapped the native code, the extension, and the
installers and release scripts. The primary agent spot-checked the claims
below. These are source observations, not test results.

**Native**

- The workspace builds one executable, `airk`
  (`crates/airk-cli/Cargo.toml`). `airk-launcher` and the role aliases are
  copies of it and are refused on current single-executable installs.
- The CLI is a clap tree of roughly 150 leaf commands in `main.rs`,
  `tools.rs` and `runtime_install.rs`. About 63 of them are `runtime`
  installation commands.
- Local IPC already exists: an owner-only named pipe on Windows and a
  peer-checked Unix socket elsewhere, with four-byte length-prefixed JSON
  frames (`local_ipc.rs`, `service_ipc.rs`). The control protocol has about 84
  methods (`airk-core/src/service.rs`).
- Read commands are already thin clients over the pipe. The CLI still does
  local work: it opens the service registry, plans registrations with the
  kernel, writes request journals, reads log and credential files, serves the
  tools catalog and help, and runs all installation code in-process.
- A second service start is refused with `service_already_running`
  (`service_runtime.rs`). `airk` with no arguments prints a clap usage error.
- Clients require the service version to equal their own
  (`project_client.rs`).
- The HTTPS listener binds `127.0.0.1`, preferring port 57421. It serves
  `/mcp`, `GET /api/v1/tools` and `POST /api/v1/tools/{name}` and nothing
  else. It refuses any request with an `Origin` header, requires a bearer on
  every request, and serves no HTML (`http_gateway.rs`,
  `airk-mcp/src/remote/http.rs`). It starts only when a catalog gate exists
  (`service_runtime/http.rs`).

**Extension**

- The VSIX is per-platform and carries the native runtime under `native/`.
  `serviceBootstrap.ts` verifies it and drives stage, activate, login startup,
  replacement and legacy cleanup. The extension is today's main installer.
- It reaches native code three ways: a per-window `airk service session`
  child (`nativeSession.ts`), a control relay or `service console` child
  (`serviceClient.ts`), and a persistent HTTPS MCP session for every Explorer
  and Workbench evidence read (`explorerEvidence.ts`, `mcpHttpSession.ts`).
- Explorer Assistance has the native side connect to a pipe hosted by the
  window (`explorerAssistanceHost.ts`).
- Agent configuration is written by about 2,100 lines of TypeScript. Entries
  are HTTPS-only and name an executable for the credential helper.
- The extension requires its version to equal the runtime's.
- The web front end is Preact, bundled by esbuild. `acquireVsCodeApi` appears
  in one file per webview. The real coupling is about 8,500 lines of host
  handlers that use `vscode.*` and the service client.
- `crates/airk-mcp/src/unified_explorer.json` and `explorer_templates.json`
  are generated from the extension's TypeScript, and the extension build
  reads `crates/airk-mcp/src/help/topics.rs`.

**Installers and release**

- All three installers are thin wrappers that run the packaged
  `airk setup --runtime-source … --approved-sha256 …`. Native code does the
  work.
- Predecessor removal covers one install root and refuses without ownership
  receipts. It does not sweep the preview channel, custom installs, 1.3
  schedulers, VS Code global-storage helper caches, the extension's CLI copy
  and PATH edits, or the Inno `standalone` folder.
- The one-executable rule is hard-coded in Rust (`runtime_pair.rs`,
  `install_files`), Python (`build-service-runtime.py`, `package-native.py`,
  `sign-native.py`, `package-standalone-linux.py`) and TypeScript.
- There is no signature check at run time. Trust is the SHA-256 approval of
  `airk-install.json`.
- The download site does not present service and extension as two steps, and
  `verify-published-release.py` does not check installers.

## 4. Target architecture and boundary

### 4.1 Components and ownership

| Component | Owns | Never |
|---|---|---|
| `airk` | Indexing, retrieval, storage, grants, jobs, watching, inference, the HTTPS listener, the admin API and embedded admin assets, the control and query endpoints, the request journal, the setup role | Parses user commands. Writes into project folders. Connects to an editor. |
| `airkctl` | The whole user CLI, text and JSON rendering, the per-window session, agent configuration files, service start and thin uninstall delegation | Opens indexes, registries or stores. Links the kernel, store, retrieval, ingest or MCP crates. Implements installation or removal. |
| Admin pages | Management screens and browser search; view state only | Hold credentials, durable requests or authority in the browser. Load anything from another origin. |
| Extension | Explorer and Workbench presentation, window search scope, workspace files, editor integration, status and guidance | Ships or runs `airk`. Opens a pipe, socket or HTTPS connection to the service. Reads service credentials or state. Installs, repairs or removes the service. |
| Service installer | File placement, the signed setup step, old-install removal, PATH, login startup, uninstall | Installs a VSIX. Edits agent configuration. Touches source files. |

### 4.2 The four interfaces

Everything that crosses a component boundary uses one of these. Each is a
contract with one owner, reviewed and landed before its consumers.

| # | Interface | Between | Owner | Status |
|---|---|---|---|---|
| I1 | Local IPC: owner-checked pipe or socket, control and query protocols | `airkctl` ↔ `airk` | `airk-core` types, `airk-ipc` transport | Exists; gains methods for work the CLI does locally today |
| I2 | `airkctl` command contract: arguments, JSON output, exit codes, and the `session` stream | Extension and scripts ↔ `airkctl` | `airkctl` crate, with Rust-emitted fixtures | New |
| I3 | HTTPS: `/mcp` and `/api/v1` for agents and local programs; `/admin` for the browser | Agents, browser ↔ `airk` | `airk-mcp` (MCP), `airk-cli` (gateway, admin) | `/mcp` and `/api/v1` unchanged; `/admin` new |
| I4 | Install record: where the service is, what it contains, how to verify it | Installer → `airkctl`, extension | Installer contract | New |

I2 is the extension's only runtime channel to the service. I4 is its explicit
read-only discovery and verification input, used before executing `airkctl`.
The I2 version handshake
replaces exact version equality between extension and service:
`airkctl version --output json` reports the product version, the I2 contract
version and a capability list. The extension decides compatibility from the
contract version and capabilities, never from version ordering alone.

`airk` and `airkctl` come from one installer and keep exact version equality
on I1.

**Contract ownership and generation.** Common contracts are data and rules,
not a shared application layer:

| Contract | Authoritative owner | Consumer rule |
|---|---|---|
| Transport-neutral control/read requests, results, identities, budgets and errors shared across Rust owners | `airk-core`, including the existing [service-record contract](../../crates/airk-core/src/service_records/CONTRACT.md) | Extract only the required pure types from current MCP/client coupling, as a separate reviewed step. No kernel, registry, filesystem or handler implementation enters Core. |
| I1 transport and typed clients | `airk-ipc` (U27) | Peer verification, framing and bounded I/O only. No application policy or dependency on `airk-cli`, `airkctl`, MCP or the indexing stack. |
| I2 arguments, output envelopes, exit status, session messages and capability catalog | An owned module in `airkctl` | The extension consumes Rust-emitted types/fixtures. Generic frame encoding may use IPC; editor-session dispatch and authorization stay in `airkctl`. |
| I3 admin envelopes and operation authorization | The service admin module; shared result types retain their existing native owner | The browser adapter consumes generated contracts. Keep MCP catalogs in `airk-mcp`; do not copy handlers or expose every control method automatically. |
| I4 install inventory and verification facts | Setup/install contract, with pure shared record definitions in Core where needed | Installer, CLI and extension use the same schema and fixtures. Filesystem and signature verification stay in adapters. The exact schema, including the U39 install pointer, is a Phase 1 contract under U24 and U26. |
| View state, display events and editor capability hooks | `packages/ui` for shared presentation; editor-only types remain in `packages/vscode` | TypeScript may own UI-only state. It must not become another handwritten definition of native results, retry rules or authorization. |

Before a contract lands, its inventory row records owner, consumers, allowed
surfaces, authorization, version and unknown-field rules, byte/deadline limits,
cancellation, retry and recovery behavior, and its golden fixtures. Name the
generation command and drift check, or explicitly record why generation is
outside the proven pilot. Only I2 and I3 types are generated, through the
U47 pilot; every other contract follows D5 (fixtures first). Generated output is never its own expected value:
operation sets and compatibility expectations are reviewed fixtures. Compare
exact bytes for persisted/digested data and parsed equality elsewhere,
normalizing only documented transport identities and timing.

I2 must distinguish a missing service, an incompatible CLI/service pair, an
unsupported capability and a refused request. Test a compatible extension
with differing product versions, unsupported contract versions, missing
required capabilities and mixed `airk`/`airkctl` versions. Unsupported
combinations fail with guidance before any mutation. This does not relax I1's
exact-pair requirement or approve an unspecified version range.

### 4.3 Process model after the split

Exactly one `airk` stays resident per user: the service. This replaces the
list in [process ownership](../process-model.md).

| Process | Lifetime |
|---|---|
| `airk` service | Resident. Started by login startup, by `airkctl`, or by hand. |
| PDF parser child of `airk` | At most one, per document. Unchanged. |
| `airk` setup role | Installer and uninstaller one-shot. |
| `airkctl` one-shot | Exits when the command finishes. Includes agent credential helpers. |
| `airkctl session` | One per VS Code window. Exits with the window's extension host. Replaces `airk service session`. |
| `airkctl tools shell` | Opened and closed by the user. |

No other `airk` process may appear. `scripts/check-self-spawn.py` and
`scripts/check-process-topology.py` gain reviewed entries for `airkctl` and
for `airkctl` starting `airk`.

### 4.4 Extension code and service code

**Rule.** Extension code and service code share no runtime channel except I2.
Their only shared source is presentation-only `packages/ui` and generated
contracts from the owners above. I4 is the discovery exception, not a second
service transport. Neither product imports the other's implementation.

| | Service side | Extension side |
|---|---|---|
| Source | `crates/`, `packages/admin/`, `installers/` | `packages/vscode/` |
| Shared | `packages/ui/` (presentation only, no transport) and generated I2 types | same |
| Ships in | Service installer | VSIX |
| Starts processes | `airk` (PDF child, setup role); `airkctl` starts `airk` | `airkctl` only |
| Network | Loopback listener in `airk` | None to the service |
| Files it may read outside its own storage | Source roots (read-only), its own state | The install record (I4) and workspace files |

**Enforcement, each a cheap check:**

- **VSIX contents.** `scripts/package-extension.py` and its verifier reject
  `native/`, executables, models and installers.
- **Process starts.** `packages/vscode/src/test/explorerBoundaries.test.ts`
  already lists every allowed process start. Its expected set becomes
  `airkctl` plus editor-owned tools. It is a reviewed, checked-in value.
- **No transport in the extension.** A boundary test fails on imports of
  `net`, `tls`, `http`, `https` or `http2` in extension host code outside
  tests, and on any reference to pipe names, the loopback endpoint, CA files
  or credential files.
- **One spawning module.** Only `packages/vscode/src/airkctl/` may start a
  process. Features receive a typed facade. Its reviewed roster includes the
  platform verification tools required before CLI execution and retained
  editor-owned tools; they do not create another AIRK transport. OS browser
  opening and terminal integration use the editor's adapters.
- **Crate edges.** `scripts/dev/boundary_check.py` and
  `boundary-exceptions.json` gain the reviewed edges in §10.1. `airkctl` may
  depend only on `airk-core`, `airk-process` and `airk-ipc`.
- **No native reads of extension source.** Native builds stop reading
  `packages/vscode`. The generated catalogs change owner (§10.4).
- **Transitive boundaries.** Check resolved Cargo features/dependencies and
  frontend bundle inputs, not just direct imports. `airkctl` must build without
  models, ORT or the service's native dependencies. `packages/ui` imports no
  VS Code, Node filesystem/process, browser-service or IPC adapter. Test-only
  seams cannot become production bypasses.

### 4.5 Dispatch and authority

The service composition root wires transports to typed handlers. I1 and the
admin API share management handlers; all evidence surfaces share canonical
read handlers. Keep exhaustive per-plane enums and a reviewed operation table
for catalog completeness, authorization and conformance applicability.

Preserve the metadata/evidence separation in [architecture](../architecture.md):
the metadata actor never opens project indexes, and evidence never queues
behind it. Browser search does not become a management request internally.
Workbench keeps one admitted lifetime through composed reads. Collection,
grant, revision and budget checks remain native, including explicit subsets,
continuations and complete error/retry details.

Adapters translate framing and presentation only. They cannot authorize work,
extend a deadline on reconnect, convert missing evidence into success or replay
an uncertain mutation. Disconnect cancels delivery; accepted native work keeps
its admission until it actually finishes. Preserve the existing read-retry
eligibility rules against shared Rust-emitted cases.

## 5. `airk`: service only

### 5.1 Invocation

| How `airk` is started | Result |
|---|---|
| No arguments, service running | Prints that the AIRK service is already running, with its version, and that `airkctl service status` shows details and `airkctl service stop` stops it. Exit 0. Starts nothing. |
| No arguments, service not running | Starts the service detached, waits for health under a deadline, then prints that the service has started, that it keeps running after the terminal closes, and that `airkctl service stop` stops it. Exit 0. |
| No arguments, start fails | Prints the typed failure and the log location. Non-zero exit. |
| Any user-style arguments | Prints that `airk` takes no commands and that `airkctl` is the CLI, plus the current running state. Non-zero exit. Starts nothing. |
| A private role selector | Runs that role: the service itself (from login startup or the detached start), the PDF parser child, or the setup role. |

A manual start is an explicit Start: it clears a saved explicit Stop, as
`runtime start` does today. Private role selectors are undocumented and are
not a user interface. Phase 1 specifies each admission path separately:
receipt-bound login-manager startup, manual detached startup, the PDF child's
parent handshake, and setup's owner-private operation file. Login startup
cannot depend on an installer parent that has already exited. Reject a bare
role argument without the authority required for that role.

### 5.2 What moves into the service

`airkctl` cannot open state, so the service takes over the CLI's local work.
Each item is a control-plane addition with one handler (O2).

| Today in the CLI process | After the split |
|---|---|
| Resolve a project or selection from a root or binding (`service_frontend::saved`, kernel registration planning) | Control method; the service resolves |
| Prepare storage and reserve registrations (`service_registration::prepare_storage`) | Control method |
| Write the durable request journal before index, configure and usage mutations (`client_requests::prepare`) | The service owns the journal; the client supplies an idempotency key and can read receipts |
| Model catalog | Control method |
| Tools catalog and help, Explorer draft and template commands | Served by the service from `airk-mcp` |
| HTTPS trust, issuer rotation, login startup policy | Control methods run by the service; the setup role covers install and uninstall time |
| Extension-held request journal and intents (`privateIntentStore.ts`, `serviceIntents.ts`) | The same service journal, shared by `airkctl`, the session and the admin pages |
| Explorer Assistance delivery to a window-hosted pipe | A bounded per-window queue in the service, drained by that window's `airkctl session` |

**Journal handoff is a contract change, not a file move.** Today
`client_requests.rs` saves the exact mutation/key before submission. Before
moving that ownership, I1/I2 must specify how a caller retains or recovers the
same request identity if the first acknowledgement is lost. Cover disconnect
before acceptance, after durable acceptance and after completion; client death;
service restart; duplicate keys; and the same key with different arguments.
Receipt lookup recovers the accepted operation. Reconnect must not invent a
new key or repeat an uncertain mutation. The service records acceptance before
effects; the UI distinguishes submitted, accepted, completed and unknown.
Phase 1 records the recovery mechanism and fixtures under O2 before handlers
or extension journals change.

### 5.3 What leaves `airk`

Removed only by the deletion rules: not compiled, no caller after a clean
compile and Clippy, or a reviewed compatibility-inventory row naming the
surviving owner.

- The user command tree and its rendering move to `airkctl`.
- `service session`, `service console` and `service rpc` are replaced by
  `airkctl session` and one-shots.
- Stdio MCP (`mcp`, `bridge`, `unified_mcp.rs`), the launchers
  (`universal_launcher.rs`, `runtime launch`) and role aliases are already
  refused on current installs.
- The staged installation engine (stage, activate, selection, rollback,
  managed payloads) is not used by a fixed layout (U15). The parts that read
  and remove pre-split installations survive as the retirement module (§9.3).

## 6. `airkctl`

### 6.1 Shape

- A new binary crate, `crates/airkctl`, depending on `airk-core`,
  `airk-process` and the new `airk-ipc`.
- "Lightweight" is checked by the crate-edge gate, not by a size limit.
  Binary size and cold-start time are measured and reported.
- Windows uses the existing owner-only named pipe. macOS and Linux use the
  existing peer-checked Unix socket. "Named pipes" in the request means this
  transport on each platform.
- Signed in CI with `airk`, and listed in every payload inventory.

### 6.2 Command surface

`airkctl` exposes the whole CLI. Phase 0 produces the reviewed mapping of
every current leaf command to one disposition. Surviving commands keep their
current names, arguments and output so documentation, scripts and golden
outputs carry over.

| Family | Disposition |
|---|---|
| Evidence reads (`search`, `files`, `read`, `info`, `outline`, `symbol`, `imports`, `facts`, `refs`, `context`, `changes`, `code-facts`, `relationships`, `status`, `parse`) and `tools list/help/call/shell` | Kept as they are |
| Index, job and project commands (`index`, `inspect`, `pause`, `resume`, `watch`, `configure`, `settings`, `setup-model`, `projects`, `model-catalog`) and `usage` | Kept; local work moves into the service |
| `explorer` draft and template commands | Kept; served by the service |
| `service health/logs/shutdown/register/requests …` | Kept under `airkctl service …`, with `start`, `stop`, `restart`, `status` |
| `runtime http-connection`, `http-credential`, `http-revoke`, `lookup-http-connection`, `setup --client` | Become `airkctl agents …` (§6.4) |
| `runtime http-trust`, `http-issuer-rotation`, `login …`, `inventory`, `purge`, `lifecycle` | Become `airkctl service …` subcommands backed by control methods |
| `runtime stage/activate/ensure/launch/managed/retire-*/replace-installation`, `setup`, old `uninstall`, `service legacy …` | Retired with the staged engine, or owned by the setup role. Each gets a compatibility-inventory row. `airkctl service uninstall` is only the approved setup launcher (U20). |
| `service session`, `service console`, `service rpc` | Replaced by `airkctl session` and one-shots |
| New: `airkctl admin open`, `airkctl admin url`, `airkctl version` | Sign-in link for the admin pages; the I2 handshake |

Every command accepts `--output json`. JSON is the complete native result,
never a reduced view.

### 6.3 Uses that are not pipe requests

`airkctl` talks to the service only over the pipe, with five documented
exceptions plus the offline recovery operations of U75. Each is narrow and has
a reason.

| Exception | Reason |
|---|---|
| Read the install pointer and install record | To find and verify `airk` and the chosen locations (U39) |
| Start `airk` detached | `airkctl service start`, and automatic start for commands that need the service. A saved explicit Stop is respected; only an explicit start clears it. |
| Agent credential helper reads the owner-private token file | Today's rule: the helper never starts the service and never enrolls |
| Read owner-private log files when the service is down | Diagnosing a service that will not start |
| Delegate `service uninstall` to the verified setup role | U20: create/pass the owner-private operation request, then hand off and exit so setup can remove the CLI. Setup owns consent, shutdown, receipts and removal; the CLI does not implement them. Setup is a private role of `airk` (U33); setup asks whether to delete indexes (U32). |

### 6.4 Agent configuration

One native implementation replaces the TypeScript writers.

- `airkctl agents list` classifies AIRK-owned entries in project and user
  files with today's classes (`stdio_retired`, `stdio_missing_command`,
  `https_unknown_connection`, `https_endpoint_mismatch`,
  `https_helper_missing`, `https_trust_missing`, `ok`).
- `airkctl agents connect` enrolls a reviewed selection and writes the entry.
  `repair` rewrites stale entries. `remove` deletes AIRK-owned entries.
- Credential helpers in written entries name the installed `airkctl`.
- Only recognized AIRK-owned entries are touched. Other servers, comments,
  disabled states and policies are preserved. User-level entries count only
  when AIRK provably wrote them. The installer's retirement receipt (§9.3)
  supplies that proof for pre-split entries.
- `airkctl` writes the files as the invoking user. The service never does.
- The extension keeps a removal-only recognizer for the no-service case. It
  is tested against shared cases emitted by the Rust implementation, so the
  two cannot drift.

### 6.5 Session mode

`airkctl session` is the per-window helper. It keeps today's session framing
and limits (length-prefixed JSON on stdin and stdout, at most eight requests
in flight, 16 MiB responses, cancellation).

- It carries every request the extension makes: evidence reads, Workbench
  plans, status, scope selection and agent commands.
- It keeps its pipe connections open, so a read does not pay process start.
- It adds a bounded event subscription for status and job changes, and
  delivery of Explorer Assistance drafts addressed to its window.
- It never returns bearer tokens. The VS Code MCP provider and the
  `airk.httpCredential` input resolve the token with a separate one-shot.
- On owner loss it cancels read-only work and exits. It no longer runs
  installer work, so the 600-second exception in the process model ends.

The I2 fixture set also specifies duplicate request IDs, interleaved replies
and events, truncated frames, oversized inputs/outputs, EOF, cancellation and
service restart. Keep event buffering bounded independently of reply budgets;
overflow is reported with a resynchronization path, never hidden as complete
history. Scope event cursors and Assistance delivery to a service incarnation
and an owned window identity. Reconnect cannot deliver another window's drafts
or reuse a stale grant. Accepted mutations continue under service ownership
and are recovered by receipt, not replayed by the session. Deadline tests use
injected clocks and observed events. Uninstall delegation is one-shot only.

### 6.6 Parity

`airkctl` is the CLI in the parity rule. Equivalent reads through `airkctl`,
MCP, Workbench (through the session) and the admin pages use the canonical
handlers and return the same complete results and inputs. During Phase 3 the
old `airk` CLI still exists, so `airkctl` output is compared against goldens
captured from it: exact bytes for digested and persisted formats, parsed
equality elsewhere.

## 7. Admin endpoint and pages

### 7.1 Endpoint

- Same listener, certificate and origin as MCP:
  `https://127.0.0.1:<port>/admin/`. Static assets are embedded in `airk`.
  The API is under `/admin/api/v1/`.
- The listener must start with the service even when no index exists. Phase 0
  establishes whether a fresh empty catalog already provides the gate that
  starts it. `/mcp` and `/api/v1` keep their existing admission.
- `airkctl admin open` is the supported entry point. It prints or opens the
  current URL. The port can change after a restart, so bookmarks are not
  reliable.

### 7.2 Authorization (O3)

A browser is a new kind of client. Any web page can send requests to a
loopback port, so the admin plane must be strict.

| Rule | Detail |
|---|---|
| Sign-in | `airkctl admin open` asks the service, over the owner-checked pipe, for a single-use code with a short lifetime. The browser exchanges it for a session cookie and is redirected. The code is consumed on first use and never logged. The extension's **Open AIRK Admin** runs the same command. |
| Session | `__Host-` cookie, `Secure`, `HttpOnly`, `SameSite=Strict`, `Path=/`, no `Domain`. Idle and absolute expiry. Bound to the service instance, held in memory and bounded in number. A restart ends every session. |
| Planes stay separate | Bearer credentials are refused on `/admin`. Cookies are refused on `/mcp` and `/api/v1`, which keep refusing every `Origin`. |
| Origin and host | `Host` must match exactly, as today. Compare `Origin` to the exact service origin. `Sec-Fetch-Site` contains classifications, not URLs: the approved rule must distinguish authenticated same-origin API traffic from initial top-level navigation launched by `airkctl`. Reject cross-origin API traffic and define missing-header behavior explicitly. No CORS headers are sent. |
| Mutations | `POST` only, with a session-bound anti-forgery token. Destructive actions use the existing native draft, preview and apply-with-digest flow plus a typed confirmation. |
| Content | Strict Content Security Policy with no inline script and no external origin, `frame-ancestors 'none'`, `nosniff`, `no-referrer`, and `no-store` on API responses. The pages work offline. |
| Never over HTTP | Anything that returns a bearer or private key, raw connection grants, upgrade checkpoints, uninstall. |

**One handler per operation.** The admin API dispatches to the same control
handlers the pipe uses. An operation table gives each operation its handler,
the surfaces allowed to call it, its authorization rule and its conformance
applicability. No second implementation exists for the browser.

The dedicated security review (U25, U38: an independent agent reviews the
written specification and the user approves its findings) must specify the
launch-code exchange before coding:
URL placement and redaction, browser history/referrer exposure, atomic
single-use consumption, expiry and replay, and first navigation with no
session. Test unauthenticated navigation separately from authenticated API
access; allowing the entry page must not allow management calls. Include
session invalidation, logout, forged/missing headers, cross-plane credentials,
path normalization, asset traversal and untrusted document rendering. Render
indexed evidence as data, never executable HTML. Neither the table nor passing
renderer tests substitute for that review or a real-browser check.

**Certificate trust.** The browser must trust the installation-local issuer.
The setup step offers the existing reviewed per-user trust action on Windows
(CurrentUser Root) and macOS (login keychain), with consent. The service
never falls back to plain HTTP. Linux has no automatic per-user trust;
browsers there warn until the user provisions trust. Each browser is
qualified separately.

**Documents that change with O3:** the statements in
[architecture](../architecture.md) and the
[local HTTPS API](../local-https-api.md) that HTTP exposes no lifecycle or
index-management commands and that browser origins are refused.

### 7.3 Pages in the first release

| Area | Content | Comes from |
|---|---|---|
| Overview | Service state, version, indexes, activity, leftovers from old installs | Explorer Management → Overview |
| Index library and index sets | Reviewed index setup, sources, split, policies, retire | Explorer Indexing |
| Activity | Jobs, plans, pause, resume, cancel, retry | Management → Activity |
| Storage | Locations, limits, relocation, cleanup | Management → Storage |
| Recovery | Repair plans and their receipts | Management → Recovery |
| Service | Restart, stop, resources, login startup, HTTPS issuer trust and rotation | Management → Service |
| Agents | Connections, grants, revocation, health of known entries, and the exact `airkctl agents …` command to apply | Management → Agents |
| Usage | Reports and policy | Management → Usage |
| Logs | Bounded logs and the redacted diagnostic report | Management → Logs |
| Search and evidence | Query, code, documents and the evidence inspector | Explorer Search, Code, Documents, Evidence |
| Help | Help topics | Explorer Help |

**Search in the browser (U7).** The admin session picks its own selection
from every index and index set in the catalog. That selection belongs to the
browser session. It does not read or change a VS Code window's scope or any
agent connection's grant. Reads go through the canonical handlers with an
explicit selection, a grant issued for the admin session, revision identity
and budgets, and return complete results.

**Not in the browser:** opening a file in the editor, workspace trust, plan
files saved into a workspace, git-based review, the Suggestions Inbox. These
are editor features.

**After Stop** the page has no server. It shows how to start the service
again: run `airk`, `airkctl service start`, or use Start in the extension.

### 7.4 Front-end code

The management screens move out of the extension. Search and evidence views
are needed in both places, so they move to a shared package (§10.4). The
8,500 lines of extension-host management logic do not move as TypeScript:
their durable-request, confirmation, paging and validation behavior is
re-homed in the service behind the admin API. This is the largest single
piece of new work in the plan.

Preact and esbuild remain the stack for both shells (U18). Share display
components and view state; keep browser HTTPS and editor postMessage/I2
adapters separate. Neither shell starts a frontend server in production.

## 8. VS Code extension

### 8.1 What it does

| Keeps | Changes | Loses |
|---|---|---|
| Explorer search, code, documents, evidence, review, strategies and planner | All reads go through the `airkctl` session | Bundled runtime and every install, repair, update and uninstall path |
| Workbench, saved sessions, exports | Window search scope is chosen among existing indexes through the session | Index library, storage, service, recovery, usage and log screens (to the admin pages) |
| Suggestions Inbox and Explorer Assistance | Drafts arrive over the session | The window-hosted Assistance pipe and Node MCP server |
| Agent connection for the workspace | Calls `airkctl agents …`; keeps removal-only code | TypeScript writers, HTTPS enrollment, trust and rotation code |
| VS Code MCP provider | Supplies the definition; resolves the token with an `airkctl` one-shot. VS Code's own MCP client connects to `/mcp`; the extension does not. | Direct reads of CA and token files |
| Status and guidance | **Open AIRK Admin**, **Start**, **Download AIRK Service**, **Check again** | Stop, restart, upgrade, rollback |

**Marketplace README (U59).** `packages/vscode/README.md` opens with a
"Requires AIRK Service" section: what the service is, the
**https://airk.ioka.io** download link and GitHub Releases link, short
Windows, macOS and Linux install steps, how to check `airkctl service status`,
and that installing or updating the extension never installs the service. The
**Download AIRK Service** action and the "not installed" state (§8.3) use the
same link. A cheap check pins the link and section heading against a reviewed,
checked-in expectation, and VSIX verification confirms the packaged README
carries them.

Choosing a window's search scope stays in the extension because it selects
among existing indexes for that window. Creating or changing indexes, index
sets and grants is admin-only (U6).

### 8.2 Finding and trusting `airkctl`

The extension must not execute something because it is on `PATH`, in the
workspace, or claims a version.

1. Read the install pointer from its fixed per-user path, then the install
   record (I4) it names in the chosen program folder (U39). There is no
   workspace or user setting for the location. Test hosts inject a fixture
   through the existing test seam.
2. Require that the pointer, the record and their folders are owned by the
   current user, are not writable by others, and contain no links or
   redirects.
3. Verify `airkctl` before the first execution. Windows: a valid Authenticode
   signature that meets the publisher policy in `scripts/sign-native.py`.
   macOS: `codesign --verify --strict` against the team requirement. Linux:
   the SHA-256 in the owner-private record, documented as weaker.
4. Cache the result by file identity and re-verify when it changes.
5. Run `airkctl version --output json` and apply the I2 compatibility check.

The expected publisher and team identity come from one checked-in signing
identity file that the signing scripts, installers and extension build all
read. They are not hand-copied. An unsigned development `airkctl` is accepted
only when the extension runs in development or test mode.

### 8.3 States

| Observed state | Explorer and Workbench behavior |
|---|---|
| Verified, compatible, running | Attach through the session. Show native readiness. |
| Service not installed | "AIRK Service is not installed", with **Download AIRK Service**, **Installation instructions** and **Check again**. The site opens only on user action. Old AIRK MCP entries are removed and listed (§8.4). |
| Only a pre-split installation exists | Same as not installed. Explorer adds that an older AIRK is still present, that this extension does not use or change it, and that the service installer will remove it. |
| Installed, service stopped | Show the stopped state and offer **Start**. An explicit Stop is shown as such; Start is the explicit action that clears it. |
| Incompatible contract | Show both versions and the reason. Point to the newer installer or extension. Never upgrade or downgrade silently. |
| Unverifiable or damaged `airkctl` | Refuse to execute. Explain the verification failure and point to the installer. |
| Reachable but refused | Keep the exact native error and retry details. Distinguish startup, credentials and absence. |
| Workspace untrusted or native execution disabled | Keep help and local UI. Do not run `airkctl` until the existing gate allows it. |

The multiple-installations state from revision 1 no longer exists (U17).

Recheck on request and on bounded recovery. If `airkctl` is replaced or
removed while a window is open, end the session, re-verify and reattach only
to the verified installation. Reopening the editor only reconnects; it never
creates an index or replays work.

The extension runs on the workspace's execution host. A remote workspace
needs the service installed on that host. A local service never stands in for
a remote workspace, and admin pages on a remote host are bound to that host's
loopback address. Unsupported remote arrangements are documented, not
claimed.

### 8.4 MCP entries (U12)

- **Service found.** The extension asks `airkctl agents list` for stale
  entries. Entries that match an existing index are rewritten by
  `airkctl agents repair` after one confirmation, because that creates a
  grant. Entries with no matching index are removed and recorded so Explorer
  can offer them again once an index exists (O6).
- **Service not found.** The extension removes recognized pre-split AIRK
  entries with its removal-only code and records them. Explorer lists what
  was removed, from which files, and why.
- Explorer never claims that a running agent reconnected.

### 8.5 Installation surfaces to retire

Every entry gets an explicit disposition in the Phase 0 inventory. A retained
command identifier may guide the user but must not install.

- Settings: `airk.cli.autoInstall`, `airk.cli.shellIntegration`,
  `airk.service.channel`, `airk.service.installDir`, the four
  `airk.service.managed*` settings, `airk.storage.path`.
- Commands: `installCli`, `setUp`, `removeTerminalCommand`, `reviewData`, the
  HTTPS trust and rotation commands, `serviceStop`, `serviceRestart`, and the
  indexing and storage commands that become admin links.
- `vscode:uninstall`: removes only editor-owned storage. It no longer runs a
  native uninstall.
- `extensionUpdateLoader.ts` hot-swaps host modules and seals
  `native/manifest.json`; its inventory drops the native entry.

## 9. Service installer

### 9.1 What the installer does

This is the user-facing summary. The transaction order in §9.6 governs file
replacement: run the verified setup/retirement preflight and stop/drain owned
executing images before overwriting them. The installer must provide a verified
setup entry point that remains available across placement and finalization.

1. **Tell the user (U13).** Before any change, state that every earlier AIRK
   installation will be stopped and removed, that existing indexes are
   deleted and must be rebuilt, that no source file is read, changed or
   deleted, and that coding agents must be reconnected. List the locations
   found. An unattended install needs an explicit flag that acknowledges
   this.
2. **Prepare replacement** with the signed setup step (U16): stop/drain and
   remove receipt-proven predecessors, recording leftovers and recovery state.
3. **Choose locations (U39).** Offer the default program folder and the
   default data/index, cache and log locations, and let the user choose
   others. Unattended installs take the same choices as explicit arguments.
   Validate every choice before any change: owned by the current user, not
   writable by others, no links or redirects, not inside a pre-split root, a
   source root or another chosen AIRK location, and either empty or carrying
   this split installation's ownership receipt. Refuse rather than adopt a
   folder holding pre-split or foreign content.
4. **Place files and finalize** in the fixed layout inside the chosen
   program folder using the platform installer's mechanism. Write the
   owner-only install pointer at its fixed per-user path last, after the
   install record. Setup verifies the complete inventory, registers
   login startup, offers certificate trust and starts the service.
5. **Report.** Show what was removed, what was left and why, the chosen
   locations, and how to open the admin pages.

The installer works with VS Code absent or closed. It installs no VSIX and
edits no agent configuration.

### 9.2 Later installs

| Found on the machine | Result (O5) |
|---|---|
| No AIRK | Fresh install, empty catalog |
| Any pre-split installation | Removed with its indexes, then fresh install |
| The same split build | Repair in place; indexes kept |
| An older split build, compatible state contract | Stop through `airkctl`, replace files, start; indexes kept |
| A split build with an incompatible state contract, or a newer build | Say so before proceeding; continue only as an explicit reset |

Later installs read the install pointer and default to the locations already
in use, so repair and upgrade do not silently move anything. Choosing a
different program folder on a later install places the new copy there,
removes the old receipt-owned program folder and updates the pointer.
Choosing a different data/index location on a later install, or after
installation, uses the service's existing storage relocation through the
admin Storage page or `airkctl`; the installer never copies or moves index
data itself. The exact pointer path, schema and interruption cases belong to
the I4 and setup contracts (Phase 1).

### 9.3 Removing old installations (U13, U14)

**Roster.** The setup step looks for every pre-split artifact, not only one
install root.

| Artifact | How it is found |
|---|---|
| Stable and preview install roots, with their state, data, cache, log and settings folders | Fixed per-user paths in `user_paths.rs` |
| Custom install roots | The universal-launcher pointer and the executable paths named by AIRK login registrations |
| Login registrations | Windows `airksvc-*` tasks and their saved XML, Linux systemd user units, macOS `io.iokaio.airk.service.*` agents, and 1.3 `airk-<hash>` schedulers |
| Universal launcher folder | Fixed per-user path |
| Inno `standalone` folder, its PATH entry and its uninstall registration | Fixed path and the `user-path-owner.txt` receipt |
| VS Code global-storage helper caches and CLI copies, for Code, Insiders and VSCodium | Paths in `runtime_legacy_origin.rs` |
| PATH and shell edits written by the old extension | Their ownership receipts |
| Custom index data folders | The old service registry |
| Per-user certificate trust added by AIRK | The trust ownership receipt |

**Rules.**

- **Receipts only.** An item is removed only when an AIRK ownership receipt
  proves it. This applies inside AIRK's own folders too.
- **Processes** are stopped by exact image path under a discovered root, on a
  verified handle. A name or a PID alone never authorizes termination.
- **No old binary is executed.** Old uninstallers are not run. The single
  exception is the verified macOS login-agent unregister (U72).
- **Links and redirects are never followed.** Sources are never touched.
- **Installation continues.** Anything unproven or locked is left, recorded
  with its reason, and shown at the end. The same list appears in
  `airkctl service status` and on the admin Overview page.
- **A leftover that can restart an old service,** such as an unproven login
  task, is reported first and in plain words.
- **The new service cannot adopt leftovers.** Its state, data and cache
  folders use names no pre-split build used (O4).
- **Retirement receipt.** The setup step records the removed executable
  paths, endpoint, issuer fingerprint and connection identifiers. `airkctl
  agents` uses this to prove that a stale entry was AIRK's.
- **Interrupted removal resumes** from its recorded operation and never
  resets the new installation.

Predecessor discovery, shutdown proof and resumable cleanup remain live
duties. The code that performs them is kept and moved, not deleted (§10.2).

### 9.4 Signing and delivery

| Platform | Delivery | Signed in CI |
|---|---|---|
| Windows x64 | Inno Setup EXE, per-user, no elevation | `airk.exe`, `airkctl.exe`, the uninstaller and the installer, with the existing Authenticode path |
| macOS arm64 and x64 | One DMG per architecture | `airk` and `airkctl` as nested code with hardened runtime, the `AIRK.app` bundle, and the DMG; notarized and stapled |
| Linux x64 | tar.gz with `install.sh` | Unsigned and labeled so; complete inventory digests, release checksums and GitHub attestations |

- Published setup runs from a CI-signed executable (U16); local Windows builds
  sign it with the same authorized identity (U22). The wrapper checks the
  signature or pinned digest before the first native execution.
- Whether the macOS `Install AIRK.command` wrapper passes Gatekeeper from the
  notarized DMG is a qualification question. If it fails, setup runs from
  inside the notarized `AIRK.app` on first launch instead (U40). No installer
  package is introduced.
- GitHub attestations and checksums are provenance and integrity evidence,
  not OS code signing. Marketplace signing is separate.
- Release assets keep the `airk-{version}-{target}-installer.*` names and
  manifests. `verify-published-release.py` gains installer checks.
- **airk.ioka.io** presents **Install AIRK Service** and **Install VS Code
  Extension** as two steps. It selects real published assets by platform,
  architecture, version and channel, with signature, checksum and
  compatibility information. Missing assets stay unavailable. A signed
  installer is advertised only after its published bytes pass verification.

### 9.5 Installer map

Paths are approved (U36). Phase 0 verifies that none collides with a
pre-split root.

| Item | Windows | macOS | Linux |
|---|---|---|---|
| Program folder (default; U39 allows another) | `%LOCALAPPDATA%\Programs\AIRK\` | `~/Library/Application Support/AIRK/program/AIRK.app` (U78), outside every pre-split root | `~/.local/lib/airk-service/` |
| `airkctl` | `bin\airkctl.exe` | `AIRK.app/Contents/MacOS/airkctl` | `bin/airkctl` |
| `airk` | `service\airk.exe` | `AIRK.app/Contents/MacOS/airk` | `service/airk` |
| Notices and licenses | `notices\` | Inside the bundle | `notices/` |
| Install record (I4) | Program folder | Beside the bundle | Program folder |
| Install pointer (U39) | Fixed per-user path, owner-only | Same | Same |
| User choice (U39) | Inno directory page plus an AIRK data-location page; command-line arguments for unattended installs | Setup asks during installation; arguments to `Install AIRK.command` or the first-launch setup | `install.sh` options and prompts |
| On `PATH` | `bin\` added to the user PATH under an ownership receipt. `airk.exe` is not on `PATH`. | A receipt-owned `airkctl` link in `~/.local/bin` when that folder exists; otherwise the path is printed | Same as macOS |
| State, data, cache, logs | Default: new folder names under the existing per-user AIRK roots. The user may choose other user-owned locations (U39). | Same | Same |
| Login startup | One Task Scheduler task | The bundle's SMAppService agent | One systemd user unit |
| Certificate trust | CurrentUser Root, with consent | Login keychain, with consent | Not automatic |
| Shortcut | Start menu: AIRK Admin | None | None |
| Uninstall | Inno uninstaller runs the setup role, then removes files and the PATH entry | `airkctl service uninstall` starts the setup role | Same as macOS |

No shell startup file is edited on any platform.

### 9.6 Replacement and incomplete installs

Fixed layout still needs an explicit transaction and recovery contract; it
does not provide rollback slots. Before Phase 7 implementation, record these
states and interruption cases in setup's contract and I4 fixtures:

1. Discover receipt-proven predecessors and disclose the reset/retention
   policy; acquire the installation operation lock. Serialize concurrent
   installers and Start against replacement.
2. Stop the owned service and drain affected CLI sessions before overwriting
   executing files. On Windows, a stopped service alone does not release an
   open `airkctl.exe`. Retain exact-image ownership and process-exit evidence;
   an image-name match never authorizes termination.
3. Verify and place the complete matched pair and its embedded asset inventory.
   Publish readiness through I4 only after the full inventory, signatures or
   integrity proof, and I1 version equality pass. A partially replaced pair is
   incomplete, not a compatible service.
4. Apply login/trust actions with their receipts, start, and verify health.
   Record file placement, trust, startup registration and service readiness
   separately so one successful action cannot hide another's failure.
5. On interruption, resume from the operation receipt. Report locked or
   unproven leftovers and readiness failures truthfully. The U14 instruction
   to continue past unremovable predecessors does not authorize running a
   half-installed successor or silently adopting old state.

Keep the signed setup executable available until it has completed its own
operation, including uninstall; define the platform wrapper's final file
removal. Test same-build repair, compatible upgrade, rejected mismatch,
concurrent Start, running windows, locked files and interruption at each
durable boundary. The extension re-verifies after replacement and reconnects
without replaying mutations or creating indexes. This adds no staged payload,
automatic downgrade or rollback policy.

## 10. Code reorganization map

Every row is a candidate for the Phase 0 inventory to confirm. Moves are
mechanical steps, separate from behavior changes, generated by a committed
script, and reproduce the captured goldens.

### 10.1 Workspace

| Crate or package | After the split | AIRK dependencies |
|---|---|---|
| `airk-core` | Unchanged owner. Gains only reviewed contract additions. | None |
| `airk-process` | Gains the OS primitives `airkctl` needs (U84): per-user folder lookup and detached spawn | None |
| `airk-ipc` (new) | Local transport (pipe and socket, owner checks, generic framing) and typed I1 control/query clients. No I2 editor-session policy. | core |
| `airkctl` (new, binary) | Command tree, rendering, session mode, agent configuration, service start | core, process, ipc |
| `airk-ingest`, `airk-store`, `airk-retrieval`, `airk-kernel` | Unchanged | Unchanged |
| `airk-mcp` | Unchanged owner. Owns the tool and template catalogs it serves. | core, kernel |
| `airk-cli` | The service: role dispatcher, service runtime, HTTPS and admin planes, setup and retirement | Adds ipc |
| `packages/ui` (new) | Shared search and evidence views, styles and view contracts. No transport. | None |
| `packages/admin` (new) | Browser shell, admin screens, HTTPS adapter. Its build output is embedded in `airk`. | ui |
| `packages/vscode` | Extension host, webview shell, `airkctl` adapter | ui |

**Extraction criteria for O1** (simplification plan §4.1):

1. *No cycle.* IPC is the one cluster in `airk-cli` that is already outside
   the module cycles. The client code around it is not: it references the
   store's service lease, the kernel's broker and MCP types. It is untangled
   in place first (Phase 2).
2. *Stable interface.* `airk-ipc` gets a `CONTRACT.md` that Phase 2 uses
   unchanged before the crate is extracted.
3. *Independent consumers.* Two binaries consume it. This is a binary
   boundary.
4. *Testing benefit.* Its T0 command runs without the process stack. Phase 2
   measures this.

### 10.2 `crates/airk-cli/src`

| Cluster today | Disposition |
|---|---|
| `main.rs` command enums, `tools.rs`, `cli_io.rs`, `help_guide.txt` | Command tree and rendering move to `airkctl`. `main.rs` keeps the role dispatcher and the manual-start messages. |
| `local_ipc*.rs`, `service_ipc.rs` | Move to `airk-ipc` |
| `project_client*`, `console_client.rs`, `usage_client.rs`, `client_owner.rs` | Pure I1 client halves move to `airk-ipc` after untangling. Inventory current MCP DTO and kernel helper dependencies first; pure cross-owner types move to Core in the contract step. |
| `session_protocol*` | I2 editor-session types and policy move to `airkctl`; only the generic framing codec belongs in `airk-ipc` |
| `service_frontend.rs`, `combined_frontend*`, `client_requests.rs`, `service_registration.rs` | Registry- and kernel-dependent parts stay in the service behind new control methods; the thin client parts move after the request-recovery contract is proven (§5.2) |
| `session.rs`, `session_commands*` | Replaced by `airkctl session`; allowlist logic moves with it |
| `service_runtime*`, `service_management*`, `service_manager*`, `service_configuration.rs`, `service_logs*`, `service_resource_policy.rs`, `catalog_*`, `project_supervisor*`, `project_worker*`, `source_*`, `worker_*`, `embedding_*`, `inference_*`, `service_inference*`, `checkpoint_fence/`, `native_command/` | Stay |
| `http_endpoint*`, `http_gateway*`, `http_backend.rs`, `native_admin_connections.rs` | Stay. A new admin module beside them holds sessions, the anti-forgery check, static assets and admin dispatch. |
| `explorer_assistance.rs` | Replaced by the service-side Assistance queue |
| `unified_mcp.rs`, `universal_launcher.rs`, `runtime_pair*`, `process_roles.rs` aliases, `legacy_startup*` | Retire with evidence; already refused on current installs |
| `runtime_install*`, `standalone_setup*`, `install_files/`, `install_layout/`, `runtime_legacy_origin*`, `legacy_processes.rs`, `runtime_macos_login.rs` | Split three ways. **Setup:** a small fixed-layout role for login startup, trust, start and uninstall. **Retirement:** the readers and removers for pre-split installations (`replacement.rs`, `upgrade_stop`, legacy-origin and old-layout readers), kept and widened to the §9.3 roster. **Staged engine:** stage, activate, selection, rollback and managed payloads, removed with evidence once no caller remains. |

Each new owned module gets a `CONTRACT.md`: `airk-ipc`, `airkctl`, the admin
plane, setup and retirement. The shared UI and both adapters also document
their allowed inputs, capabilities, dependency boundaries and scoped test
command. Update contracts in the same step as their interfaces or invariants.

### 10.3 `packages/vscode/src`

| Group today | Disposition |
|---|---|
| Install and update: `serviceBootstrap.ts`, `native.ts`, `cliInstall.ts`, `cliRemove.ts`, `runtimeRebuild.ts`, native parts of `extensionUninstall.ts` and `uninstallHook.ts` | Removed with evidence. `nativeGate.ts` stays and gates `airkctl`. |
| Transports: `nativeSession.ts`, `serviceClient.ts`, `serviceHttp.ts`, `mcpHttpSession.ts`, `mcpStdioSession.ts`, `privateDirectory.ts` | Replaced by `src/airkctl/`: discovery, verification, one-shot runner, session client, compatibility. `readRetry.ts` and `nativeOutcome.ts` stay as semantic rules tested against shared Rust cases. |
| Agent configuration: `agentConfig*.ts`, `agentMigration.ts`, `agentSetup.ts`, `agentToml.ts`, `unifiedAgentConfig.ts`, `globalAgentInventory.ts`, `agentConnections.ts`, `agentReconnect.ts`, `httpIssuerRotation.ts` | Writers, enrollment and rotation removed in favor of `airkctl agents`. A removal-only recognizer and the reconnect record stay. |
| Explorer evidence host: search, facts, inspect, review, strategies, suggestions, handoffs, plan files, workspace I/O, scope selection | Stay; reads use the session facade |
| Explorer management host: `explorerManagement.ts`, `indexingCatalog*.ts`, `indexingSplit.ts`, `serviceConsole*.ts`, `serviceDashboard.ts`, `usagePanel.ts`, `usageReport.ts`, `diagnostic*.ts`, `serviceIntents.ts`, `privateIntentStore.ts`, `intentAcceptedEvidence.ts`, `vectorModel.ts` | Leave the extension. Behavior is re-homed in the service and `packages/admin`. |
| Workbench host | Stays; reads use the session facade |
| `explorerAssistance/` Node MCP server, `explorerAssistanceHost.ts`, `explorerAssistanceLaunch.ts` | Replaced by session delivery |
| `explorerWebview/`, `workbenchWebview/` | Search and evidence views move to `packages/ui`. Management views move to `packages/admin`. The extension keeps a thin webview shell. |
| `serviceExtension.ts` (3,487 lines) | Split into a composition root plus discovery and status, agent connections, the MCP provider and views. Untangled in place before files move. |

Target layering, adapted from the simplification plan §4.3:

```
generated I2 types
  └─ airkctl/        the only module that starts a process
       ├─ agents/    airkctl agents calls, removal-only recognizer, MCP provider
       ├─ status/    discovery states, guidance, Open Admin, Start
       └─ features:  explorer · workbench · assistance
webview shell — mounts packages/ui views through a postMessage adapter
```

### 10.4 Front-end packages and generated catalogs

- **One presentation package, two adapters.** Inventory
  `explorerContracts.ts` before extracting it: it currently mixes display,
  management, upgrade and editor behavior. Move only shared view state/events
  to `packages/ui`. Native wire contracts come from their Rust owner (§4.2).
  Editor-only actions are explicit capabilities, absent in the browser. The
  extension adapter uses postMessage and the I2 host facade; the browser
  adapter uses I3 and its bounded same-origin event stream. Neither adapter
  is imported by the presentation package.
- **One frontend stack.** Use locked Preact/esbuild inputs for both shells
  (U18). Document package build order and dependency locks; build the service
  admin assets without installing or compiling `packages/vscode`. The VSIX
  build consumes shared views/generated contracts without building the service.
- **Theming.** The 74 `--vscode-*` uses become `--airk-*` tokens. The webview
  shell maps them to VS Code's variables. The browser shell supplies light
  and dark values.
- **Catalog ownership reverses.** Today the extension's TypeScript is the
  source of truth for `unified_explorer.json` and `explorer_templates.json`,
  and the template file embeds the extension version. After the split the
  service owns both, and they carry their own content version. Front ends
  read them from the service or consume a generated copy with a drift check.
  The extension build stops reading `topics.rs`.
- **Embedding (U19/U21).** Local builds and CI run the same locked admin build
  before compiling `airk`. Its sealed HTML/JS/CSS asset inventory is embedded
  and included in native build/release receipts. Editable source, manifests,
  locks and build scripts are tracked; generated bundles are build outputs,
  not checked into Git. They ship inside the service and work offline. A
  missing or stale bundle fails the native build; there is no network build
  step hidden inside Cargo and no runtime dependency on Node.

### 10.5 Local builds, CI, release and rosters

This is a required implementation map, not a claim that these changes have
landed. Script names below refer to `scripts/` unless qualified otherwise.
Keep one implementation of each build/package/verification operation, called
by both local scripts and workflow adapters. CI adds authenticated hosted
provenance; both paths use their configured access to the signing identity.
CI must not be the only way to build either product.

#### 10.5.1 Force-local-build scripts (U21)

Provide documented Windows entry points, approved as
`scripts/build-local-installers.ps1`, `scripts/build-local-service-installer.ps1`
and `scripts/build-local-vsix.ps1` (U37). These are new deliverables, not
commands available in the current tree. The combined entry point calls the other two;
they delegate to the canonical Python/native/frontend owners rather than
duplicate packaging logic. The CLI contract must support:

```powershell
# Proposed commands, implemented and verified during the split:
pwsh -NoProfile -File scripts/build-local-installers.ps1 -ForceBuild
pwsh -NoProfile -File scripts/build-local-service-installer.ps1 -ForceBuild
pwsh -NoProfile -File scripts/build-local-vsix.ps1 -ForceBuild
```

- **Force local production.** `-ForceBuild` builds from this checkout and
  bypasses completed runtime/installer/VSIX artifact reuse and CI downloads.
  Reuse verified dependency caches and the checkout's persistent Cargo target;
  do not implement it as `cargo clean`. Record incremental compilation honestly:
  forcing the build pipeline need not recompile unchanged dependencies.
- **Service sequence.** Check prerequisites; generate/check contracts; build
  shared UI and admin assets; seal assets; build `airk` with shipping inference
  and lightweight `airkctl`; validate and sign the matched payload; package and
  sign the Windows Inno installer in the §10.5.4 order. Default models, notices
  and setup inputs match CI's roster.
- **VSIX sequence.** Locked frontend install, contract drift checks, host/view
  compilation, UI-only packaging and content verification. It needs no native
  build or service installation. Produce the universal VSIX (U29).
- **Prerequisites and provisioning.** Check the pinned Rust, Node/npm, Python,
  PowerShell, Windows build tools, Inno Setup and configured signing access
  before an expensive step. Use the existing model/static-ORT provisioners.
  Downloads are explicit;
  offline mode accepts only verified cached inputs. No global PATH/tool changes.
- **Outputs.** Print absolute paths to the installable EXE and VSIX, version,
  target, source identity and signing status. Save step logs, input/output
  digests, commands, durations and failure diagnostics in an ignored local
  artifact directory. A failure stops subsequent packaging and returns a
  non-zero exit; an old artifact must not appear to be the new successful output.
- **Local installation before CI.** The produced artifacts must support the
  Windows local acceptance journey in §12. Build scripts do not silently
  install, stop the user's service, reset indexes, publish, or change trust.
  Installation is an explicit subsequent action with the normal disclosures.
- **Windows signing (U22; O13 resolved).** Use this machine's existing access
  to the same signing identity as CI. Share signing/verification owners and
  the checked-in publisher policy; do not require a new test certificate or
  disable signature checks. Verify Authenticode trust, publisher/identity EKU
  and timestamp, and record the actual signer and post-sign hashes. The current
  hosted profile permits leaf-certificate rotation: do not hard-code a permanent
  thumbprint or assume the key is exportable. Resolve local access through the
  existing authorized signing mechanism and document its explicit inputs.
- **Signing failure and provenance.** Missing signing access, timestamp or
  failed verification stops the signed installer build; never silently emit
  an unsigned replacement. Signing/timestamp services may need network access
  even when build dependencies are cached. Keep the hosted-only guards in
  `ci-windows-sign.py`; local scripts call shared logic through a local adapter,
  never spoof `GITHUB_*`. Signed local outputs retain local-source/build receipts
  and cannot satisfy authenticated hosted-release provenance on signatures alone.
- **Ordinary VSIX.** The local UI-only VSIX retains production verification
  and accepts the locally signed compatible service normally. Windows native
  Authenticode signing and Marketplace VSIX signing are separate; the local
  VSIX build does not require Marketplace publication or a developer bypass.

U21/U22 require installable local outputs that work together under normal
verification. The final script help and Windows instructions must contain
tested commands and signing prerequisites; implement and test the local path
before depending on hosted CI for product validation.

#### 10.5.2 Workflow and script change map

| Current owner | Required change and preserved guarantee |
|---|---|
| `.github/workflows/ci.yml`: `plan`, `quick`, `contracts`, `extension`, `native-build`, `native`, `container`, `required`; `ci-plan.py` | Classify new inputs, schedule frontend embedding before the service build, and update exact required results. Preserve the stable required check and success/skipped semantics. Any new job/output must update workflow dependencies, gate validation, producer authentication and independent expectations together. |
| `ci.yml` native build/check steps; `.config/nextest.toml` | Add scoped IPC/CLI checks and lightweight CLI dependency proof. Keep the shipping service's `local-inference` checks, model/import checks and Linux workspace/doctest coverage. Move test identities and exclusive process groups deliberately; do not drop a workload because its crate/command moved. |
| `build-service-runtime.py`, `package-native.py`, `ci-runtime.py` | Build and inventory both executables, embed the sealed admin output, and authenticate every consumed input through seal/restore/resume. Update runtime tests and install-record readers together. |
| `ci.yml` `native`; `check-service-clients.py`, `ci-local.ps1`, native Python/Node harnesses | Inventory every `--binary`, `AIRK_TEST_BINARY` and runtime-root consumer as service, CLI or matched pair. User commands move to `airkctl`; service/process-exit observations stay on `airk`. Preserve negative tests, cleanup receipts and actual transport coverage. |
| `package-extension.py`, `promote-vsix.py`, `verify-extension-release.py` | Produce a UI-only VSIX, reject forbidden native content, and remove the native subtree substitution. Preserve tested JavaScript and package identity through promotion; no rebuild during signing. |
| `ci-candidates.py`, `ci-plan.py`, `github-release.py` | Replace the current per-target native-plus-VSIX artifact assumptions with the universal-VSIX catalog (U29). Authenticate the complete producer run/attempt, commit/tree, target and digest set. A universal VSIX never replaces native platform evidence. |
| `sign-native.py`, `ci-windows-sign.py`, `ci-macos-sign.py`, `package-standalone-*.py`, `standalone-installer-contract.py`, `installers/` | Sign and verify both binaries; setup is a role of the signed `airk` (U33). Update pre/post-sign inventories, install records, installer extraction tests and failure cases; preserve provenance across signing transformations. |
| `.github/workflows/github-release.yml` | Keep preflight → authenticated candidate selection → signed copies → installer packaging → complete assembly → attestation → authorized publication. Include admin identity, both binaries and the tested UI-only VSIX. Update path triggers for every new release-tool input. |
| `.github/workflows/extension-release.yml`, `.github/workflows/macos-signing.yml` | Stop depending on native bytes inside VSIXs. Preserve authenticated signed-release assembly and Marketplace byte verification; macOS signing no longer rebuilds a VSIX to swap signed native content. |
| `.github/workflows/release-tag.yml` | Update its `ci-candidates.py tag` checks of the pushed tag and release provenance to the approved product catalog. Publication preflight must enforce completeness before publishing; this post-tag check remains an independent verification. |
| `.github/workflows/qualification.yml`, `ci-local.ps1` | Use split-product commands and input rosters. Keep qualification opt-in and retain full/native/installed gates and failure evidence. A script rewrite is not permission to resume the stopped installed-editor repair (O8). |
| `Dockerfile`, `.dockerignore`, `container-inputs.json`, `build-service-container.py` | Include both binaries and embedded admin build inputs. Replace the removed `runtime container-host`/self-test command paths with approved service-role and CLI paths. Keep Node/build tools in build stages, preserve source seals/private exclusions, and do not change listener exposure or authentication incidentally. |
| `verify-published-release.py`, `build-downloads.py`, `.github/workflows/downloads.yml`, `site/index.html` | Verify downloaded installers, executable/asset inventories and provenance in addition to VSIXs. Advertise only verified available targets. Include shared release-catalog dependencies in site workflow triggers. |
| `check-self-spawn.py`, `check-process-topology.py`, `scripts/dev/boundary_check.py`, `scripts/dev/boundary-exceptions.json`, `packages/vscode/src/test/explorerBoundaries.test.ts` | Review process roles, allowed crate edges, extension verification/spawn sites and forbidden transport imports. Keep expectations checked in and independent of discovered implementation. |
| `license-notices.py`, `dependency-audit.py`, `.github/workflows/dependency-audit.yml` | Include new frontend manifests/locks and generated/bundled dependencies in audit and notice rosters. Package notices for each actual artifact, without losing embedded Preact or native dependency licenses. |

#### 10.5.3 Change selection and build order

Preserve the host policy: PRs gate Linux x64, Windows x64 and macOS arm64;
main also gates macOS Intel. Authenticated identical-tree PR promotion may
reuse the first three; uncertain producer identity or changed full-build inputs
falls back to all four. Manual qualification covers all four. No duplicated
broad matrix is added just because the products split.

| Changed input | Required affected work |
|---|---|
| I1/I2/I3/I4 owner, shared native DTOs, generation or fixtures | Contract/generation checks plus all consuming native/frontend adapters and transport parity tests |
| `packages/ui/` | Both frontend builds/tests, VSIX content checks, and native service rebuild/validation because admin embeds these bytes |
| `packages/admin/` | Admin checks, native rebuild and served-asset validation; affected container/installer checks |
| `packages/vscode/` only | Extension checks and VSIX build; actual CLI/session integration where affected. Do not rebuild unchanged service inputs solely for presentation edits. Reuse must retain original native identity. |
| Frontend lock/toolchain/build scripts or contract generator inputs | Every dependent bundle and embedded-service build; invalidate affected reuse/cache decisions |
| Native/runtime/package/install/signing tooling | Relevant native and artifact gates, scripting contracts, and full-build fallback where provenance cannot prove safe reuse |
| Documentation only | Existing quick/repository checks; no native campaign |

These are dependency requirements, not permission to relax today's fail-closed
selection. Add reviewed path cases, including deletion/rename and unknown paths,
to `scripts/tests/test_ci_plan.py` before narrowing a gate. Packaging changes
need actual artifact checks as well as mocks. Preserve DCO, environment approvals,
credential-free PR execution, permissions and the required-check name.

Build order is contracts/catalogs → shared UI → admin bundle/seal → native
pair → native verification → service installer. The VSIX branches after shared
UI/contracts and builds independently of native compilation. Local scripts
follow the same order. Cache keys include the owning locks/toolchain and
target/features; caches are accelerators, not evidence of a passed build.

#### 10.5.4 Artifact identities and release assembly

`build-service-runtime.py::source_identity()` currently excludes frontend
source. Extend its consumed-input roster to admin/UI source, generated contract
inputs, manifests/locks, toolchain and build scripts, and the asset inventory.
Bind the resulting bundle digest into native receipts. Changing only frontend
production input must reject stale native reuse. Extend `ci-runtime.py`
seal/restore/resume and `package-native.py` validation together; no pre-split
one-executable receipt can qualify a split payload.

Every final artifact records source commit/tree (and local dirty-source digest
when applicable), version/channel, target or universal applicability, interface
versions and its build inputs. Service artifacts also record both executable
hashes and the admin inventory. VSIX receipts record their own UI/host inventory
and supported I2 contracts, without requiring a native build. Release assembly
binds these product receipts together. Hosted receipts additionally bind the
authenticated producer run/attempt. Keep native production, frontend production,
harness and fixture identities attributable separately. Never relabel a local
build as a hosted producer or a reused artifact as freshly compiled.

Windows signing keeps the order: native pair → uninstaller → embed signed
uninstaller → installer signing. macOS signs nested executables and bundle,
then notarizes/staples the delivered artifact and verifies execution. Recompute
affected hashes after signing with an explicit original-to-signed mapping;
never regenerate admin assets or VSIX JavaScript at this stage. Linux remains
explicitly unsigned with inventory digests, checksums and attestations (U2).

Keep same-run successful-build resume and the `fresh_build` opt-out, exact-tree
promotion proofs and complete-release verification. Update artifact catalogs,
workflow required results, signing and published verification as one reviewed
contract step, then their consumers. Missing/mixed attempts, one missing
executable, mismatched admin inventory or a native-containing VSIX fail closed.

Separate installation does not remove the existing shared version/lockfile
policy or the Marketplace workflow's authenticated `signed_run_id` prerequisite.
Publication still requires separate user authorization. Local build scripts
require neither publication permissions nor a successful hosted run.

#### 10.5.5 Cheap regressions before packaging

Extend the existing script suites as their owners change; the Phase 0 inventory
pins exact selected tests, rather than inventing a second test catalog:

| Concern | Existing tests under `scripts/tests/` |
|---|---|
| Selection, producer and transfer | `test_ci_plan.py`, `test_ci_candidates.py`, `test_ci_runtime.py` |
| Runtime, VSIX and promotion | `test_service_runtime_build.py`, `test_package_native.py`, `test_package_extension.py`, `test_release_promotion.py` |
| Signing and installers | `test_native_signing.py`, `test_ci_windows_signing.py`, `test_ci_macos_signing.py`, `test_standalone_windows.py`, `test_standalone_macos.py`, `test_package_standalone_linux.py` |
| Release, site and container | `test_github_release.py`, `test_extension_release.py`, `test_release_version.py`, `test_downloads_site.py`, `test_container_inputs.py`, `test_container_runtime_build.py` |

Add contract cases for the new local wrappers: force-build bypasses artifact
reuse, missing prerequisites fail early, failed steps stop packaging, build-only
never installs/publishes, and both output paths/receipts describe current inputs.
Mocks test command construction and failure propagation; actual local EXE/VSIX
installation establishes U21 acceptance. Other negative cases cover stale admin
inputs, absent binaries, forbidden VSIX files, incomplete producer matrices and
selection of every consumed input. Keep expected inventories independent of the
packager being tested.

### 10.6 Documentation

Updated with the change that makes each statement true, not before:
[architecture](../architecture.md), [process ownership](../process-model.md),
[local HTTPS API](../local-https-api.md),
[installation](../service-installation.md),
[replacement policy](../upgrade-rebuild-fallback.md),
[agent setup](../agent-integration.md), [headless use](../headless-cli.md),
the CLI guide and generated reference, [extension](../vscode-extension.md),
[read parity](../read-interface-parity.md), [versioning](../versioning.md),
the README, the documentation index, release notes and the Marketplace
description. The extension README carries the service download link and
install steps (U59) in the same change that removes the extension's install
path (Phase 6); until then it keeps describing the current product. Dated records stay identifiable as history.

CLAUDE.md and AGENTS.md name the CLI, the window session and the parity
surfaces. Their wording changes only with the user's approval and stays
byte-identical.

## 11. Implementation sequence

Work proceeds one phase at a time. Contract changes land before their
consumers. Mechanical moves are separate from behavior changes. A behavior
step stays within about 25 files and 1,500 changed lines.

U45 authorizes the phases in order without a stop at phase boundaries; the
rows below give dependencies and acceptance. Track S and the governing
simplification review gates remain in force (U35); this plan does not reopen
D10 (U23).

~~~mermaid
flowchart LR
    P0[0 Inventory and review] --> P1[1 Contracts]
    P1 --> P2[2 airk-ipc]
    P2 --> P3[3 airkctl]
    P3 --> P4[4 airk service-only]
    P4 --> P5[5 Admin plane and pages]
    P4 --> P6[6 Extension]
    P4 --> P7[7 Installers and release]
    P5 -. frozen shared UI and adapter contracts .-> P6
    P5 -. sealed embedded assets .-> P7
    P5 --> P8[8 Qualification]
    P6 --> P8
    P7 --> P8
~~~

| Phase | Work | Done when |
|---|---|---|
| **0. Inventory and review** | Disposition of every `airk` leaf command, extension command and setting. Predecessor fixtures and current-command goldens. Draft I1–I4, pure DTO dependency inventory, per-role authority, request recovery, empty-catalog listener behavior, §9.5 paths, U22's local signing adapter/prerequisites, and the complete §10.5 workflow/harness/input roster. The inventory, goldens and drafts are recorded and reviewed by the primary agent. O1–O12 are answered (U23–U34). Report the results to the user; per U45 there is no stop unless a new decision is needed. |
| **1. Contracts** | Control-plane additions (§5.2), I2 output/session, admin operation/security contracts, I4 and artifact receipt/catalog changes. Shared DTOs and UI adapter boundaries. Wire/recovery/malformed-input fixtures and generation/drift commands. | Each contract is reviewed and landed with independent expectations. No consumer has changed. Security specification and decision dependencies are explicit. |
| **2. `airk-ipc`** | Give IPC and its clients an owned module and facade inside `airk-cli`. Move callers in small groups and re-measure the module graph. Extract the crate when the §10.1 criteria hold. | Goldens reproduce. Selected test identities and outcomes are unchanged. |
| **3. `airkctl`** | Port the command tree. Add the service-side handlers. Move agent configuration to Rust. Add session mode. The old `airk` CLI still exists. | `airkctl` reproduces the goldens for every surviving command. Parity regressions pass on actual transports. |
| **4. `airk` service-only** | Remove the user CLI after equivalent `airkctl` coverage. Add manual start and private roles; migrate process/harness consumers and rosters. Delete replaced code with evidence. | §5.1 is tested; surviving commands and test identities have owners. Build/package gates understand both binaries. Retain original goldens and their source identity. Any interim nonshippable state is recorded, not published. |
| **5. Admin plane and pages** | Always-on listener, sign-in, admin dispatch, shared UI and admin shell, local/CI asset generation and seals, first-release pages. Freeze shared UI/adapters before Phase 6 integration. | Security regressions and dedicated review pass; admin reads match the other surfaces. Missing/stale assets fail build/package checks and the embedded pages work offline. |
| **6. Extension** | Verified discovery, I2 facade, evidence features, MCP entry handling and removal of install/management surfaces. Marketplace README with the service download link and install steps (U59). UI-only package and local VSIX build script. Follows U28/U29 and requires the frozen shared UI interface. | §4.4 boundaries pass. No install path remains. Local VSIX builds without a service build and exercises missing/incompatible/compatible service states. |
| **7. Installers and release** | Fixed-layout transaction, signed setup, removal/consent, user-chosen program and data/index locations (U39), local service and combined build scripts, signing rosters, CI identities/selection, release/site updates. Follows U30–U33 and U39–U40 and requires sealed admin assets. | Installer contracts pass against predecessor fixtures. Force-local signed Windows EXE and ordinary VSIX installation is demonstrated before hosted qualification; publication verifiers reject incomplete inventories or missing hosted provenance even when local signatures are valid. |
| **8. Qualification and handoff** | Ordered §12 checks on frozen inputs; final artifact and publication verification only when authorized. | Receipts collected, blockers reported, and claims limited to observed hosts. The approved `airk-cli` → `airk-service` rename (U34) is a separate mechanical step before freezing consumed inputs. |

Phases 5, 6 and 7 may perform independent work after Phase 4, with explicit
handoffs: Phase 5's shared UI/adapter contract precedes Phase 6 integration;
its sealed admin assets precede Phase 7's final native packaging. Common
contracts and artifact manifests are not edited concurrently by consumers.
Local packaging scripts are ready before the first installer validation, not
added after CI qualification. Each delegated task names its component, its
`CONTRACT.md`, the files it may edit, its T0 command, the goldens to reproduce and its "done
when". Builds, tests and platform batches go through one owner. The primary
agent reviews the combined change.

Before a feature or fix PR, remind the user to increment the patch version
and synchronize manifests and lockfiles, honoring a bump already made for
that PR. Do not relabel existing artifacts or reuse qualification under a new
version or source identity.

## 12. Validation and acceptance

Find defects with the cheapest check that can expose them. Record exact
selections, non-zero selected-test counts, input identities and receipts in
the [task ledger](../simplify/one-process-ledger.md). Mocks establish
decision behavior, not installation or signed artifacts.

| Gate | Required observations |
|---|---|
| T0: boundaries | Crate edges; VSIX contents; extension process starts and transport imports; self-spawn and process-topology rosters. |
| T0: `airk` and `airkctl` | The §5.1 invocation table. Every surviving command against its golden. Session framing, limits, cancellation and owner loss. Version and contract handshake. |
| T0: admin plane | Pure-function tests of host, origin and fetch-site checks, cookie attributes, anti-forgery, plane separation, single-use sign-in codes, session expiry and the never-over-HTTP list. Every operation has one handler, an authorization rule and a conformance entry. |
| T0: extension | Every state in §8.3. The packaged README contains the reviewed "Requires AIRK Service" section and download link (U59). No execution before verification. MCP entry removal with and without a service. No install-side call from activation, commands, update or uninstall. The removal recognizer against shared Rust cases. |
| T0: installer contracts | Against predecessor fixtures: proven items removed; unproven and locked items kept and reported; links not followed; sources untouched; installation continues; interrupted removal resumes. Two-executable inventories. Custom program, data/index, cache and log locations (U39): accepted user-owned folders, refused shared, linked, nested, pre-split and non-empty foreign folders, pointer written last, and later installs keeping the chosen locations. Bad or missing Windows and macOS signatures and failed Linux integrity checks are rejected. |
| T0: CI and local build contracts | §10.5.5 script cases: consumed-input selection, stale admin reuse, incomplete binary/asset inventories, UI-only VSIX, producer/attempt validation, force-local behavior and fail-fast output receipts. Verify signing failures stop packaging, ordinary VSIX signature checks stay enabled, and valid local signatures cannot substitute for hosted provenance. |
| T0/T1: parity | Complete read and input parity across `airkctl`, MCP, Workbench through the session and the admin pages, including provenance, coverage, continuations, errors and retries, on canonical handlers and actual transports. |
| T1: component and repository | Formatting, scoped Clippy with timings, affected extension and Python tests, and the repository, boundary and whitespace checks from [CONTRIBUTING](../../CONTRIBUTING.md). |
| T2: installed products | Install the service without VS Code. Verify manual `airk` start, `airkctl`, MCP, the admin pages in a real browser, and login startup. Install the VSIX with and without a service. Exercise detection after a manual install, service removal and replacement while a window is open, several windows, and reconnect without index creation. |
| T2: Windows local build acceptance (before hosted product qualification) | From one recorded checkout, run the combined §10.5.1 force-build command, or each individual command once, to produce a signed EXE and ordinary VSIX. Verify U22's signatures/timestamps, install explicitly on the local test machine, exercise the service alone and with the installed extension under normal verification, and confirm browser assets work offline. Reuse the same VSIX to test installation with no service. Record signer, separate native/frontend/harness identities, paths and receipts. This proves local signed build/install support, not hosted release provenance. |
| T2: predecessor removal | From a bundled-VSIX install, an old standalone install, a preview install and a custom root: complete proven cleanup, truthful leftovers, an empty successor, untouched sources. Locked files and interrupted recovery. Failed fixtures are preserved. |
| T3: final qualification | Workspace Clippy, tests and doctests, and the artifact and release gates from CONTRIBUTING. Installation evidence on Windows, Linux and both Mac architectures, with Windows and macOS signature verification and Linux integrity and provenance verification. Platform claims are limited to what was observed. |

Before any T2 or T3 run, review the combined diff and evidence, freeze the
consumed inputs, and record the exact commands, expected results and stop
conditions. Stop at the first failure and analyze the complete diagnostics
before correcting source. Two expensive failures for one component invoke the
root-cause stop rule.

**The split is complete when all of the following are demonstrated:**

- The service installer works on its own, with Windows and macOS signatures
  and Linux checksum and provenance verification, and the site selects its
  verified release asset for each host.
- Old AIRK installations are removed as §9.3 describes, the user was told
  beforehand, and no source file changed.
- `airk` takes no user command and behaves as §5.1 states. `airkctl` carries
  every surviving command with complete results.
- The admin pages manage the service and search any index, behind the §7.2
  rules, and `/mcp` and `/api/v1` behave as before.
- The extension's Marketplace README tells users to install AIRK Service,
  links to its download page and gives per-platform install steps (U59).
- The VSIX contains no native payload, has no install path, and reaches the
  service only through `airkctl`.
- The documented force-local-build scripts produce a working Windows service
  installer and VSIX without requiring any CI run, using the same build/package owners.
  Windows outputs use U22's signing identity and normal extension verification;
  local signing evidence does not replace hosted release provenance.
- CI selects every consumed input, rejects stale embedded assets and partial
  pairs, and preserves complete native, signing and published-product evidence.
- Read parity, native authority, source isolation, and revision and budget
  contracts are intact.
- Documentation and Marketplace text describe both products and their actual
  qualification status. Publication is claimed only after authorized
  workflows and published-artifact verification succeed.

## 13. References

| Area | Starting points |
|---|---|
| Native CLI and roles | `crates/airk-cli/src/main.rs`, `tools.rs`, `process_roles.rs`, `session_commands.rs`, `session_protocol.rs` |
| Local IPC and clients | `crates/airk-cli/src/local_ipc.rs`, `local_ipc_windows.rs`, `local_ipc_unix.rs`, `service_ipc.rs`, `project_client.rs`, `client_requests.rs`, `service_frontend.rs`; `crates/airk-core/src/service.rs` |
| HTTPS | `crates/airk-cli/src/http_gateway.rs`, `http_endpoint.rs`, `service_runtime/http.rs`; `crates/airk-mcp/src/remote/http.rs`; [local HTTPS API](../local-https-api.md) |
| Installation | [Installation guide](../service-installation.md), [layout contract](../../crates/airk-cli/src/install_layout/CONTRACT.md), [file contract](../../crates/airk-cli/src/install_files/CONTRACT.md), `crates/airk-cli/src/runtime_install/replacement.rs`, `standalone_setup.rs`, `user_paths.rs`, `service_manager.rs` |
| Packaging and signing | `installers/`, `scripts/package-standalone-*.py`, `standalone-installer-contract.py`, `build-service-runtime.py`, `sign-native.py`; [signing contract](../native-signing.md), [release verification](../release-verification.md) |
| Extension | `packages/vscode/src/serviceExtension.ts`, `serviceBootstrap.ts`, `nativeSession.ts`, `serviceClient.ts`, `explorerEvidence.ts`, `explorerContracts.ts`, `agentConfigHealth.ts`, `agentMigration.ts`; `packages/vscode/scripts/build-explorer.cjs` |
| Release and site | `.github/workflows/github-release.yml`, `extension-release.yml`, `downloads.yml`; `scripts/github-release.py`, `build-downloads.py`, `verify-published-release.py`; `site/` |
| CI selection, identity and local building | `.github/workflows/ci.yml`, `.github/workflows/qualification.yml`, `.github/workflows/release-tag.yml`; `scripts/ci-plan.py`, `scripts/ci-runtime.py`, `scripts/ci-candidates.py`, `scripts/ci-local.ps1`; [CI and builds](../ci-and-builds.md), [contributor checks](../../CONTRIBUTING.md); proposed local entry points in §10.5.1 |
| Governing documents | [Architecture](../architecture.md), [simplification plan](../simplify-plan.md), [process ownership](../process-model.md), [replacement policy](../upgrade-rebuild-fallback.md), [agent setup](../agent-integration.md), [read parity](../read-interface-parity.md), [installer stop record](../research/installer-stop-2026-11-09.md) |

These are source-review starting points, not claims that tests, installers or
signatures have passed for the split. This planning change runs documentation
checks only.
