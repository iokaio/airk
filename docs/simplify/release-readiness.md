# Local VSIX release readiness

## Correction: 1.8.19 and 1.8.20 broke docs/one-airk.md (10 November 2026)

**Observed on the maintainer's machine after installing 1.8.20.**

- Task Manager showed two `airk.exe` processes: the service (`runtime managed-serve`) and the per-window `service console`.
- The replacement had deleted `AIRK\launcher\airk-launcher.exe`. The workspace `.mcp.json` and `.codex/config.toml` still started it, so the coding agent's AIRK MCP failed with `CONNECTION_CLOSED`.
- Explorer gave no warning and offered no fix.

**What was wrong.**

- The 1.8.19 entry below records "agent configurations outside VS Code are not edited". That narrowing was **not a user decision**. It contradicts one-airk.md §1, §9 and §10.3 steps 1, 2 and 8, which require moving owned agent entries first and removing launchers only afterwards.
- The 1.8.20 agent check only appears in Management → Agents, reads project files only, and flags only stdio entries whose command is missing.
- The remedy it suggests (Connect a coding agent…) refuses exactly those entries.

**Conflicting 1.8.19/1.8.20 items, and their resolution.**

| Item | Resolution |
| --- | --- |
| Agent entries not migrated before the launcher was deleted | The replacement captures them first. Explorer reconnects them after one confirmation. |
| Old trust records left behind: the user CA, the extension trust bundles and the environment entries | Retired through their receipts before the folders move. |
| Startup registration removed before the ownership check | Order reversed. |
| Static `agents` string in the replacement output | Typed `client_references` and typed per-entry statuses. |
| Standalone `airk setup` still upgrades in place | Uses the same replacement boundary. |
| Service idle-exits in HTTPS mode; login startup only on fresh installs; Explorer restarts the service once after an idle exit | The service never idle-exits in HTTPS mode. Login startup is registered on every install. The restart workaround is removed. |
| The extension starts one `airk.exe` per operation | One session per window. |
| Workers run as separate `airk.exe` processes | Workers run inside the service. |

The decisions are recorded at the top of [one-airk.md](../one-airk.md). Implementation and evidence are tracked in [the one-process ledger](one-process-ledger.md). The 1.8.19 and 1.8.20 entries below remain dated historical records.

## 1.8.20: the service keeps running after installation (10 November 2026)

**Observed on the maintainer's machine after installing 1.8.19.**
- The replacement succeeded, then the fresh service logged `service_started`
  at 10:06:03, `idle_exit` at 10:09:11 and `service_stopped`.
- Explorer polls then reported `service_unavailable` every few seconds. Each
  poll ran `runtime inspect`, `diagnose` and `snapshot`; these were the
  short-lived `airk.exe` processes the user saw.
- `runtime inspect` reported `launcher: null`, and the snapshot reported
  `startup_manager.installed: false`.
- The workspace `.mcp.json` and `.codex/config.toml` still named
  `AIRK\launcher\airk-launcher.exe`, which the replacement had moved aside, so
  the coding agent's AIRK MCP failed with `CONNECTION_CLOSED`.

**Root cause.** The fresh install relied on the on-demand service and its
180-second idle exit, but nothing could start it again:
- health polls deliberately do not count as client activity;
- Explorer only observed status;
- HTTPS agents cannot start a process;
- no login startup was registered.

**User decisions.**
- A new installation must start the service and register it to start again
  after a reboot.
- Bump to 1.8.20.
- Implement the proposed Explorer fixes: restart after an idle exit, clearer
  advice, and a broken agent-configuration check.

**Implementation.**
- **Fresh install registers login startup.** After a fresh install, the
  extension (`RuntimeHost.prepare`) and standalone `airk setup` run
  `runtime login install`; both use the extracted `install_login_startup`, the
  same sequence the CLI command ran.
  - A refusal is reported and keeps on-demand operation.
  - The registered `runtime managed-serve` has no idle exit.
- **Replacement removes the old entry.** It removes a receipt-owned
  registration before moving folders; a failure stops the replacement before
  anything moves.
- **No console window at sign-in.** Before this change, a temporary
  current-user `InteractiveToken` task running a console program opened a
  visible Windows Terminal window. A probe program that called `FreeConsole`
  as the console's only process lost its window and kept running.
  `managed-serve` now does the same on Windows and clears its standard handles.
- **Explorer.**
  - It restarts an idle-exited on-demand service once per exited instance (a
    pure `idleExitedInstance` decision).
  - The unavailable-service advice was corrected.
  - Management → Agents checks recognized project agent files for AIRK stdio
    entries whose absolute command is missing. This check was delegated; the
    primary agent reviewed it.

Receipts are in `artifacts/readiness-1820/`.

| Check | Result |
| --- | --- |
| Extension compile (`ext-compile-r1`) | Pass, 13.7 s |
| Mocked extension suite (`ext-mocked-r1`) | 1,332 selected, 1,311 pass, 21 skipped, 0 fail; all 13 new or changed tests pass |
| Scoped Clippy `-p airk-cli -p airk-mcp --features local-inference` with fmt check (`clippy-cli-mcp-r1`) | Pass, 90.9 s, incremental on, timings retained |
| Native selection: replacement, managed_service, service_manager, user_paths (`native-tests-r1`) | Not run: link failed (`LNK1181`, ORT library) because the runner was invoked without `--static`; harness invocation error |
| Same selection with the recorded static environment (`native-tests-r2`) | 29 selected, 29 pass |
| Optimized runtime (`runtime-build-r1`, `build-service-runtime.py --profile release`, static ORT) | Pass, 818.3 s, inputs unchanged, timings retained; `airk.exe` SHA-256 `2195bfd28a6026c087f05b9aca48420cb4f870bafd03e802d9bab5562aa59678`, install inventory `f4d121d92043afd54265e873afeace076a488f006e9a3b1fe7e33f5f4db28314` |
| Login probe on the optimized runtime (`login-probe-r1`, owned fixture, `--state-dir`) | Pass, 228.8 s (detail below) |
| Native package (`native-package-r1`) | Pass, `airk-1.8.20-x86_64-pc-windows-msvc.zip` |
| Extension package, first attempt (`extension-package-r1`) | Failed: the packager's runtime-module allowlist did not list the new `agentConfigHealth.js`. Allowlist corrected in `scripts/package-extension.py`; only packaging was rerun |
| Extension package, second attempt (`extension-package-r2`) | Refused: the failed attempt's output folder already existed. The failed output is retained |
| Extension package, third attempt (`extension-package-r3`, `final-extension-b`) | Pass, 937 files; VSIX SHA-256 `32fc98c870fb27ae10a7ab9498cecc1ff42cb1720b111271d5e42d32660128fb` |
| Installed Explorer (`installed-explorer-r1`) | Pass, 12.0 s |
| Installed native editor, five scenarios (`installed-native-r1`) | Pass: same, different, multi, usage, untrusted; 470.4 s |

Login probe detail (`login-probe-r1`):
- `login install` registered a task, and the task started exactly one
  `managed-serve` process.
- No visible window belonged to that process.
- `ensure` reused the manager-owned instance.
- After 205 s of uptime the same instance was still serving, with no
  `idle_exit` event.
- `replace-installation --apply` removed the task, its definition file and both
  fixture folders, with no residue.

**Harness gap found.** The installed-editor journey's fresh installs now
register real logon tasks: four `airksvc-…` entries, from a baseline of 0. The
harness does not unregister them. They were removed afterwards through each
fixture's own owned `runtime login uninstall` (see
`installed-native-r1-login-cleanup.txt`), and the fixture folders were
retained. The installed-editor harnesses need to unregister login startup in
their own cleanup before they are run routinely.

Not run: macOS and Linux (the macOS shared login adapter remains unqualified),
full workspace tests, and hosted CI. Nothing was committed, pushed, published
or installed into the normal editor profile.

## 1.8.19: replace instead of upgrade before 1.9.0 (10 November 2026)

User decisions: every installer before 1.9.0 removes a different AIRK
installation and installs fresh; no index is created until the user adds one;
the build stays on 1.8.x (1.8.19). Recommended answers accepted: everything
before 1.9.0 is replaced, custom index data outside AIRK folders is left in
place. One recommendation was narrowed during implementation: agent
configurations outside VS Code are not edited (the existing editors for them are
the proof-heavy clean-break plans being retired); the installer tells the user
to reconnect them from Management → Agents.

Implementation: native `runtime replace-installation` (plan, or `--apply` to stop
processes running from the AIRK folders with the existing exact-image stop
primitive, move each folder aside atomically, then delete). Each folder must be
provably AIRK's own before anything moves. The extension calls the plan right
after discovery and, when required, applies it and continues as a fresh
install. The in-place clean-break code is unreached before 1.9.0; deleting it is
a separate step after a clean compile shows it has no callers.

Read-only plan on the maintainer's real installation (debug build): required,
installed 1.8.11, four locations: `data\index-v1`, `launcher`, `service-1.4`,
`state\service-1.4`.

| Check | Result |
| --- | --- |
| Native replacement unit tests (`r20-replacement-tests-r2`) | 10 selected, 10 pass, including a damaged layout naming an unrelated folder (refused, nothing moved) |
| Mocked extension suite (`extension-mocked-r20c.log`) | 1,320 selected, 1,299 pass, 21 skipped, 0 fail |
| Live replacement of a running 1.8.11 installation, debug build (`r20-debug-replacement-plain`) | Pass: processes stopped, folders removed in 0.6 s, client terminated, fresh install healthy, no index, sources unchanged |
| Same with five payloads and an unfinished 1.8.15 update (`r20-debug-replacement-machine`) | Pass, removal 1.8 s |

Two earlier attempts in this step are recorded: a guard that accepted any path
containing "airk" passed this checkout's own path and was replaced by a
provenance check (caught by its test); one debug build was misdirected to
`C:/Program Files/Git/debug-runtime-d` by a shell variable error, stopped
before writing anything there.

## First real-machine install and direction change (10 November 2026)

The first install of the committed 1.8.18 VSIX on the maintainer's own Windows
installation (selected 1.8.11; five retained payloads 1.5.3, 1.8.11, 1.8.14 ×2,
1.8.15; an unfinished 1.8.15 update with its checkpoint journal) refused at
staging: `runtime activation is pending`. The fixtures had never staged with an
unfinished update already present or with five payloads.

At the user's direction the retained-payload count no longer limits staging,
repair, uninstall or clean-break retirement, and the reset retires an
unfinished predecessor journal as evidence. A reproduction with five authentic
payloads, a genuine journal and the observed order passed (debug build,
`r19-debug-full-machine-repro-r3`; optimized build `r19-full-machine-repro`).

The second real install (VSIX SHA-256
`e05e0dd1866dd645761e852126683f0656c81105d01f326333e98504460e6a43`) refused in
the clean-break preflight before any reset step:
`clean_break_predecessor_process_observation_unsupported` for the 1.5.3
payload's never-materialized `airk-service.exe` role alias, because 176 of 410
processes could not be observed. The installation was left unchanged with
1.8.11 running. During that install a background qualification run was still
launching private VS Code windows on the same desktop; it was stopped
(`hot-update` had failed in that run and was not investigated).

The user then decided that installed predecessors do not justify an upgrade
path: everything before 1.9.0 is to be stopped and removed, and no index is
created until the user asks for one. That replacement supersedes the in-place
clean-break rebuild and is the next change; this 1.8.18 record is historical.

## Resumed and completed: 1.8.18 Windows candidate (9–10 November 2026)

At about 23:15 Mountain Time on 9 November the user instructed that the work be
resumed and finished overnight, hard decisions made where required, the build
number raised, and a manually installable Windows VSIX delivered. That
instruction resumes the stopped work below, including the service-runtime
component held at its two-strike stop. The earlier sections of this ledger are
retained unchanged as the dated record of the 1.8.17 working number; their
receipts keep their original source identities and do not qualify 1.8.18.

**Candidate:** `artifacts/readiness-1818/final-extension-b/publish/airk-1.8.18-win32-x64.vsix`,
SHA-256 `57f13fd09ea4a7440dfbf95cad83b315cb2d382bc2db3bafa7afcb758fc7cc62`.
Optimized runtime `artifacts/readiness-1818/final-runtime-b`, `airk.exe` SHA-256
`0bf8bef1258f75a4185417e0a4e970e31d74b5683595ed103ffa5cc6815157d3`, install
inventory `84d8e95a2dbfede46dc42734dc0d3f9a1bac65fa2e16a2b81d9ea2f00882a8aa`.
This is the second optimized build of the night. The first
(`final-runtime`, `final-extension-r4`, VSIX SHA-256
`15679608fc111c3f83f23cb393987946012ec027e421a038982e0ad488898095`) passed the
same installed journeys and is retained; it lacks only correction 3 below.
Built from the uncommitted working tree on `feat/simplify` at `08e94c96`; it is
a local, unsigned candidate. Nothing was committed, pushed, published or
installed into the normal editor profile.

### Decisions

- **Version 1.8.18.** No 1.8.17 package was ever built; the user asked for the
  number to be raised again. The state baseline stays 1.8.16 / `airk-state/1`.
- **Startup cooldown:** the calculated-scheduling correction chosen in
  [the stop note](replay-process-stop.md) was completed. The saved-operation
  check runs after a wait, not before the first attempt, and adds no
  explicit-stop refusal that the earlier route did not have. A clean-break
  rebuild starts its candidate once and keeps immediate admission.
- **Two-strike rule:** the installed-editor journey failed more than twice while
  qualifying. Each failure was collected and explained from retained evidence
  before the next run, and each had a different cause (listed below). Where no
  observation existed, an observation was added and a single-scenario
  diagnostic run made instead of a guess. One diagnosis was wrong (run r6) and
  is recorded as such.

### Defects found and corrected

Product (native):

1. Planned candidate restart refused by the on-demand startup cooldown
   (`runtime_install.rs`, `activation.rs`, `activation/rebuild.rs`). Reproduced
   at T0 by the earlier diagnostic, then confirmed by the process case reaching
   its next stage.
2. Index setup review admitted a source inside AIRK-private locations; only the
   later reservation refused it (`catalog_index_setup.rs`). The read-only
   `preflight_source_isolation` existed with no production caller.

3. After the MCP SDK security upgrade, an unterminated final request at end
   of input on the bounded stdio component no longer received a reply
   (`airk-mcp/src/remote.rs`). The bounded reader now terminates that
   already bounded frame once. Found by the first full workspace run.

Product (extension):

4. Saved search scope not restored after reload when an in-flight read was
   closed by a replaced control connection, and restoration depended on the
   Explorer view (`serviceExtension.ts`).
5. Per-index Usage administration launched through the retired launcher route
   and was refused with `stdio_mcp_retired` (`serviceExtension.ts`).
   Reproduced directly with the packaged CLI.
6. Routine installation-lock contention raised a user-visible HTTPS refresh
   warning.

Test and harness (no product behavior):

7. The replay process test fingerprinted the service lease file that its own
   lease locks; the log-binding test expected the draft refusal now provided
   by item 2.
8. `installedControlTrace` used the JavaScript `realpathSync`, which fails for
   extended-length (`\\?\`) paths on this host; it now uses the native variant.
9. Editor harness: publication waits treated the typed `publication_busy` and
   `source_projection_expired` replies as failures; plan-step results were read
   as bare bodies although every surface returns the complete tool envelope;
   the TypeScript parity helper lacked the documented, validated
   verification-deadline normalization of the native fixture; one step
   expected a provider definition while the harness's own current
   `.vscode/mcp.json` owned the connection; the idle observer lost its
   index-work proof when an automatic verify job followed; the usage scenario
   reused an MCP session fenced by later catalog commits.
10. The reference-refresh journey test's unoptimized future overflowed the
    default 2 MiB test-thread stack (it passes unchanged on a larger stack).
    It now runs its unchanged body on an owned thread with an explicit
    stack; no process-wide stack setting was added.
11. `check-runtime-upgrade.py` could not seed an unfinished ordinary update
   before a clean break; it now can (`--clean-break --interrupted-runtime`).

### Results for 1.8.18

Receipts are `artifacts/readiness-1817/r18-*.{json,log}` (runner unchanged) and
`artifacts/readiness-1818/`. All listed runs report consumed inputs unchanged.

| Check | Result |
| --- | --- |
| Scoped CLI/MCP shipping Clippy (`r18-clippy-cli-mcp`) | Pass |
| Installer/setup library tests plus the two corrected process cases (`r18-process-and-setup-r1`) | 330 selected, 330 pass. Earlier `process-preflight-remainder-r2` at 1.8.17: 4 selected, 2 pass, 2 fail (items 2 and 7) |
| Optimized runtime build (`r18-runtime-build-r1`) | Pass, 847.8 s, incremental unset, P10 target |
| Native package and VSIX (`r18-native-package-r1`, `r18-extension-package-r4`) | Pass; r1–r3 VSIX outputs are superseded and retained |
| Clean install, upgrade/rollback, resumable uninstall (`r18-clean-install-r1`) | Pass, 157 s |
| Service clients with model (`r18-service-clients-r1`) | Pass, 55 s |
| Clean break from authentic 1.8.11 (`r18-clean-break-11-r1`) | Pass, 100 s |
| Clean break from 1.8.11 with a seeded unfinished 1.8.15 update (`r18-clean-break-interrupted-r2`) | Pass |
| Mocked extension suite, pinned Node 24.15.0 (`extension-mocked-r5.log`) | 1,319 selected, 1,298 pass, 21 skipped, 0 fail |
| Installed Explorer on the r4 VSIX (`installed-explorer-r4.log`) | Pass |
| In-editor extension update 1.8.16 → 1.8.18 (`explorer-hot-update-r4.log`) | Pass |
| Installed native editor, five scenarios, seven roles (`r18-installed-native-r11`) | `status: passed`, `qualifiesFullScenarioSelection: true`, 386 s |

The rows above are the first optimized build. The final build repeated them:

| Check on the final build | Result |
| --- | --- |
| Scoped CLI/MCP shipping Clippy (`r18b-clippy`) | Pass |
| Framing component, all MCP library tests and the reference-refresh journey (`r18-framing-and-stack-r1`) | 176 selected, 176 pass |
| Optimized runtime build (`r18b-runtime-build`) | Pass, 770 s, incremental unset, P10 target |
| Native package and VSIX (`r18b-native-package`, `r18b-extension-package`) | Pass |
| Clean install, upgrade/rollback, resumable uninstall (`r18b-clean-install`) | Pass, 156 s |
| Service clients with model (`r18b-service-clients`) | Pass, 62 s |
| Clean break from authentic 1.8.11 (`r18b-clean-break-11`) | Pass, 99 s |
| Clean break from 1.8.11 with a seeded unfinished 1.8.15 update (`r18b-clean-break-interrupted`) | Pass, 110 s |
| Installed Explorer (`r18b-installed-explorer`) | Pass |
| Installed native editor, five scenarios, seven roles (`r18b-installed-native-r1`) | Pass on the first attempt, 397 s |
| In-editor extension update 1.8.16 → 1.8.18 (`explorer-hot-update-final.log`) | Pass |
| Mocked extension suite (`extension-mocked-final.log`) | 1,319 selected, 1,298 pass, 21 skipped, 0 fail |
| Script Node tests (`script-node-tests-final.log`) | 134 selected, 134 pass |
| Python contracts (`python-contracts-final.log`) | 924 selected, 11 skipped, 0 fail |
| Repository checks, whitespace check, version check, AGENTS/CLAUDE identity | Pass |
| Workspace Clippy all targets/features, lightweight CLI check (`r18-workspace-clippy`, `r18-lightweight-main`; before correction 3) | Pass |
| Doctests (`r18-workspace-docs`) | Exit 0 with zero doctests in every crate; this establishes no doctest execution |
| Full workspace tests, first run before corrections 3 and 10 (`r18-workspace-tests`) | 3,290 selected, 3,287 pass, 3 fail: the framing test (correction 3), the reference-refresh journey (item 10) and `service_watcher_owns_nested_changes_pause_and_restart_without_editor_events`, which passed in isolation (`r18-three-failures-r1`) |
| Full workspace tests on the final source, all features, routine exclusions restored (`r18b-workspace-tests`) | 3,290 selected, 3,290 pass (4 slow), 54 skipped by `#[ignore]`/platform; 2,768 s, two test processes, inputs unchanged |

Installed native editor attempts r1–r10 failed and are retained with their
fixtures under `artifacts/editor-qualification/`: r1 item 4; r2, r3 and r4
item 9; r5 and r6 the `.vscode/mcp.json` expectation (r6 after a wrong
diagnosis); r7 the idle observer; r8 item 5; r9 the fenced session; r10
`source_projection_expired`.

### Not run or not established

- `check-runtime-upgrade.py` cannot drive a single-image predecessor (1.8.14
  and later have no launcher client), so its runs with 1.8.15 and 1.8.16
  predecessors failed in the harness before any upgrade (`r18-clean-break-15-r1`,
  `r18-compatible-upgrade-r1`). The 1.8.16+ ordinary update is covered by the
  clean-install journey's exact-payload upgrade/rollback and the reviewed
  process case, not by a retained-state upgrade from an authentic 1.8.16
  installation.
- The seeded unfinished update has no checkpoint journal; an installation
  whose pending intent also has an `upgrade-journal.json` is not reproduced.
- macOS and Linux: the changed native files were not compiled or tested on
  either. The earlier Neo batch qualified different source.
- Container qualification, `ci-local.ps1`, hosted CI, signing and publication.
- The kernel quota test remains quarantined until 23 November 2026.
- Every catalog commit advances the native admission ticket and fences existing
  MCP HTTP sessions (clients receive 404 and start a new session). This is
  existing behavior, now asserted by the usage scenario; its effect on
  third-party clients was not evaluated.

## Earlier record: 1.8.17 working number, stopped 9 November 2026

**Stopped at the user's explicit request, 9 November 2026 at approximately
23:10 Mountain Time (10 November 06:10 UTC).** All agents were stopped and the
running library batch interrupted. Its log records 727 passing completions out
of 2,117 selected before interruption; remaining/in-flight outcomes are unknown.
This is not a completed suite or qualification pass. No new optimized runtime
or VSIX was produced, and no installed-editor acceptance was run.

The startup-admission correction in `runtime_install.rs`, `activation.rs` and
their tests was interrupted during implementation. It is **unfinished,
unreviewed and unvalidated**. Earlier successful checks retain their earlier
source identities and do not qualify these current edits. All changes remain
uncommitted. Preserve failed fixtures, receipts and the partial diff; no work
resumes without a new user instruction.

Opened 9 November 2026 (America/Denver), at `08e94c96` on `feat/simplify`.
The user requested execution of the attached closeout plan through a solid new
Windows VSIX for manual testing. This authorizes the focused repairs and isolated
installed qualification previously awaiting a separate go-ahead. Merge,
publication and release-branch actions remain separate. Earlier dated stop and
handoff records retain their original scope and failures.

The user subsequently requested overnight completion by **06:00 Mountain Time,
10 November 2026 (13:00 UTC)**. The handoff target is an identified Windows VSIX
tested through its actual installed path. Broader release claims remain bound
to collected evidence; the deadline does not convert pending checks into passes.

## Blockers and ownership

| Work | Owner | Evidence / selection | State |
| --- | --- | --- | --- |
| Service-client current-runtime admission | harness agent / primary | `service-client-readiness.md`; 47 focused tests and full Python contracts pass | Native client qualification pending |
| Interrupted-adoption restoration; stale runtime fixtures; uninstall mapping | restoration agent / primary | `restoration-readiness.md`; restoration and corrected library fixtures pass | Actual process/CLI selections pending; parent serializes Cargo |
| rmcp advisories and compatible dependency upgrade | security agent / primary | `security-readiness.md`; fresh audit, legal bundle and all MCP library tests pass | Installed transport/artifact qualification pending |
| Inference identities, S5 and script claims; kernel flake | evidence agent / primary | `qualification-evidence.md`; 19 shipping inference tests pass, identity mappings recorded | Claims narrowed; kernel quarantine explicit until 23 November |
| Candidate docs, version, fixture environment and qualification | primary | Synchronized 1.8.17; scoped Neo batch passed; exact commands/receipts below | Windows process correction and final package/installed gates pending |

## Investigation and invariants

Source findings are distinct from reproduced defects and qualification results.
The attached review and dated handoff identify a hard-coded schema-2 version
pin, pre-state-contract runtime fixtures, an omitted command mapping, missing
restart-path evidence and three rmcp advisories. These are investigation inputs,
not fresh reproductions. Each owner records complete related source review and
the diagnostic batch before correcting its component.

Preserve source read-only behavior, collection/root isolation, original
locations, revision identity, budgets, complete CLI/MCP/Workbench results and
typed failures. No wire/digest/persisted-format change, schema collapse, further
module split or unproven deletion is in scope. Catalog layers and existing
readers stay intact. Failed fixtures and incomplete cleanup receipts stay intact.

## Initial identities and environment

- Git HEAD: `08e94c96`; initial worktree clean. Remote not contacted.
- Initial native and extension version: `1.8.16`. The user authorized
  synchronization to `1.8.17`; manifests, locks and generated catalog now agree.
  `release-version.py check --expected 1.8.17` passed. The persisted baseline
  remains `1.8.16`.
- AIRK MCP tools absent; `.codex/config.toml` has `enabled = false`. Local live
  files are the evidence source; configuration unchanged.
- Sandbox user differs from checkout owner. Git reads use a per-command
  `safe.directory` exception; no global identity/configuration changes.
- Retained ladder runtime is debug `1.8.16`, source SHA-256
  `a95fae98888e01a3efa68e0e9aa87066d45a690e9eb7afdefb11a79f0684dd12`,
  executable SHA-256
  `319fd1599f5325a4b7ab57e85c0af31aba73bc57f2ac354ddc91bc9ab1718cae`.
  It cannot qualify new dependency/version inputs.
- Cargo exists under the owner's `.cargo/bin` but is absent from sandbox PATH.
  Toolchain 1.98.0 and existing `target/` caches are present. Reuse the persistent
  target; do not clean or replace it without a concrete cache diagnosis.

## Qualification ladder and receipts

No fresh build or test has run when this ledger is opened. Before each T2/T3
stage, record reviewed frozen native, extension, harness and fixture identities,
the exact command, expected result and stop condition. Stop at the first failure;
two failures of a component require a root-cause note before a third attempt.

1. Batched T0 regressions; formatting; scoped Clippy with `--timings`; Python
   contracts, mocked extension and repository checks. Confirm selected counts.
2. Review combined diff; freeze inputs. macOS starts with scoped Store/CLI
   Clippy, if existing SSH access is usable; retain evidence before Neo cleanup.
3. Shipping-feature workspace tests (including normally excluded campaigns) and
   doctests; matched development runtime process/transport preflight.
4. Native-enabled extension, CLI/MCP/HTTPS parity, service clients, clean install,
   clean-break rebuild, compatible upgrade, installed-editor acceptance and
   exact-owned cleanup, reusing unchanged artifacts only with verified seals.
5. Optimized payload/package verification and applicable container/platform
   gates. Deliver identified local VSIX and explicit tested/unavailable scopes.

Append collected results and receipt paths here as they arrive. A package-only
pass does not establish installed acceptance or cross-platform qualification.

## Collected evidence

Pinned Node 24.15.0 compilation of all three TypeScript targets and the esbuild
bundles passed in 14.85 s. Inputs and every compiled output byte match the prior
compilation; this establishes the final pinned tool prerequisite without
relabeling the earlier mocked suite's Node identity.
`artifacts/readiness-1817/extension-pinned-compile.{json,log}` retains the commands,
source/output hashes and result.

| Class | Command / selection | Result and receipt |
| --- | --- | --- |
| Reproduction then T0 correction | Service-client Python component batch | 47 selected; diagnostic reproduced the version pin and malformed metadata class; reviewed correction 47/47 pass. `artifacts/readiness-service-clients/{diagnostic,validation}.txt` |
| Diagnostic with retained binaries | `service_runtime-cadd3f6474c6bb3c.exe installed_ --nocapture --test-threads=1` | 2 selected, both fail `clean_break_required` as reported. 120.05 s. Binary/main hashes retained; compiled-source identity not established, so this is diagnostic only. `artifacts/readiness-1817/fixture-diagnostic.json`, `service_runtime-diagnostic.log` |
| Diagnostic limitation | Retained `cli_commands-db4189c7f37800cb.exe every_public_command_has_a_test_and_prints_help --exact --nocapture` | 1 selected, fails at a baked pre-S7 `abort-layout` path before reaching uninstall. Does not reproduce the current source's missing uninstall mapping. 6.50 s. Same diagnostic receipt, `cli_commands-diagnostic.log` |
| T1 extension | Compile all three pinned TypeScript targets, regenerate catalog; `node --test packages/vscode/out/test/*.test.js` | 1,315 selected, 1,294 pass, 21 skipped, zero failures, 28.75 s. Extension consumed inputs unchanged. `artifacts/readiness-1817/extension-mocked.{json,log}` |
| Prerequisite verification | `static-runtime-environment.py --environment-file artifacts/local/one-airk-1.8.14/static-environment-supplement.json` | Existing static ORT cache reverified; no rebuild. Five-value result: `artifacts/readiness-1817/static-environment-preflight.json` |
| T1 security | `dependency-audit.py --require-fresh --output artifacts/readiness-1817/dependency-audit-report.json` | Passed all requested fresh checks, existing dated advisory exceptions remain; inputs unchanged. `dependency-audit.{json,log}`, `dependency-audit-report.json` |
| T0 repository | `pwsh -NoProfile -File scripts/check-repo.ps1` | Passed, inputs unchanged. `artifacts/readiness-1817/repository-checks.{json,log}` |
| T1 compilation | `dev-check.py -p airk-cli -p airk-mcp --features local-inference --target-dir artifacts/v190/p10-target` | Failed on two test-only diagnostics assuming `ControlResponse: Debug`; 96.22 s, incremental on, inputs unchanged. Complete log/timing: `scoped-clippy.{json,log}`, `scoped-clippy-cargo-timing.html` |
| T1 Python | `python -m unittest discover -s scripts/tests -v` | 923 selected, 11 skipped; five failing subcases and two error subcases, all PowerShell HTTPS example fixtures before requests reached their server. Other variants passed. 74.00 s; inputs unchanged. `artifacts/readiness-1817/python-contracts.{json,log}`. Investigation pending; no pass claimed. |
| T1 corrected compilation | Same scoped Clippy, `--skip-fmt` reusing completed formatting | Pass, 5.41 s, incremental on, P10 target, inputs unchanged. `artifacts/readiness-1817/scoped-clippy-r2.{json,log}`, retained `scoped-clippy-r2-cargo-timing.html` |
| Environment diagnosis | One valid HTTPS request with fixture-only exception reporting | Sandbox reports SChannel `No credentials are available in the security package`, zero requests; identical host comparison passes with one request. Product source/TLS checks unchanged. `https-pwsh-diagnostic.json`, `https-pwsh-host-diagnostic.json` |
| T1 qualified Python | Complete Python suite in normal host context | 923 selected, 11 documented skips, zero failures/errors; 76.37 s, inputs unchanged. `artifacts/readiness-1817/python-contracts-host.{json,log}` |
| T1 focused Rust | Shipping-feature CLI/MCP/Store library selection via pinned Nextest CI profile | 204 selected, 199 pass, five fail; 153.45 s including 123 s compilation, 28.15 s execution. Inputs unchanged; `focused-native-libs-r2.{json,log}`, retained timing. All eight restoration, all 19 inference identities, both Store boundary tests and all existing MCP tests pass. |
| T1 legal | `license-notices.py --output artifacts/readiness-1817/notices --release` | Passed; bundle for 431 third-party packages, including packaged 3.9 SDK licenses. Existing exact publisher-declaration dispositions remain. `artifacts/readiness-1817/license-notices.{json,log}` and `notices/` |
| T1 final focused compilation | Scoped CLI/MCP shipping Clippy after complete fixture correction | Pass, 12.06 s, P10 target, incremental on; inputs unchanged. `fixture-clippy.{json,log}`, retained timing |
| T1 corrected regressions | All MCP library tests, completed receipt, initialized/uninitialized callback, registry-retry and standalone-launch modules | 182 selected, 182 pass, 54.62 s including compilation, 27.10 s execution. `fixture-regressions.{json,log}`, retained timing. Earlier unchanged restoration/inference/Store evidence retained with its original identities. |

The kernel quota test is temporarily quarantined until 23 November 2026, owner
Tyler Jensen. Every assertion is retained. This is an explicit unresolved
coverage exclusion, not a demonstrated production fix; see
[qualification evidence](qualification-evidence.md).

The user approved the narrow D8 exception for `ttlMs: 0` and
`cacheScope: "private"` in MCP catalog replies for protocol 2026-07-28 and
later. Complete transport replies retain these fields. Tool-call data,
persisted formats and digest inputs retain their existing contracts.

The complete first compilation/Python results were collected before editing.
The two compiler diagnostics share one cause: a serialized response enum lacks
`Debug`. Both diagnostics now serialize the complete response to JSON; a review
of all new diagnostics found no other such use. Rustfmt completed after this
coherent test-only correction. Production behavior is unchanged. The PowerShell
fixture failures have a separate investigation; they do not justify native
production changes or weakened assertions.

The test runner now uses a newly created `native-fixtures/` directory with
inheritable FullControl for its actual creator, SYSTEM and Administrators only.
No existing user directory ACL was changed. This implements the supported
isolated fixture precondition rather than relaxing native boundary admission.
The Mac Neo read-only SSH probe succeeded using the existing connection;
remote build commands await review and serialized Cargo ownership.

The first focused-library command never started Cargo: Windows process lookup
could not find `cargo` even though the child environment supplied PATH. Zero
tests were selected; this is a launch failure, not a passing check. The next
command uses Cargo's absolute installed path. No compilation or source repair
was needed for this launcher issue. Normal-host native tests use the separately
created owner-only `host-fixtures/` base; the sandbox-owned base remains intact.

The five focused-Rust failures were collected together before correction:

- The new SDK schema-cache regression omitted matching protocol metadata, so
  request validation returned `-32602` before schema lookup. The corrected
  fixture reaches the intended path, retains the original error expectations
  and adds a catalog-lookup observer; no production change.
- The registry-retry and standalone-token fixtures call `prepare_root` on an
  already-created temporary directory. They need fresh AIRK-created children;
  changing only the temporary parent does not establish their precondition.
- The completed-activation and external-managed callback fixtures predate the
  initialized state contract. These are stale fixture expectations, not proven
  native defects. Preserve the acknowledgment/callback invariants with honest
  prerequisites, add explicit prebaseline refusal cases and qualify completed
  public replay with a real matched runtime. Do not manufacture state authority
  or weaken refusals to make inert test executables pass admission.

At the end of that correction investigation, no T2/T3 run had started. Native
production inputs were unchanged by the test-fixture corrections. The complete
correction was reviewed and frozen before its passing compilation and batch.

## First platform batch freeze

Combined source and validation review completed before Neo launch. The first
T2 platform batch uses native seal
`3d9e0798580e58105a26a4dc055fa7a18858bf331db46a20f8aceef3a7f83b4d`
(1,132 files), entry commit `08e94c96` plus the recorded live changes. This is
not a clean-commit qualification claim. `artifacts/readiness-1817/native-freeze.json`
and `neo-readiness-1817-r1-snapshot.json` retain the exact roster.

Reviewed command plan: `artifacts/readiness-1817/neo-readiness-1817-r1-plan.json`.
First scoped Store/CLI/MCP no-default Clippy, then 37 explicitly named Unix,
restoration, MCP and wire tests and canonical static-cache verification.
Incremental off, two build jobs, isolated target in
`/Users/tylerjensen/airk-builds/readiness-1817-r1/target`. Expected all steps pass
with exact nonzero selections. Stop at the first failure or changed input.
Retain timing/log evidence outside target, then clean that exact target even
after a failed batch; unknown containment must remain visible.

Public export preflight proved the canonical native roster and all tracked
fixture inputs were included. No `temp/`, private state, index state or ignored
artifact content was transferred. Separate locked dependency fetch passed
without creating a native target (`neo-transfer-and-prefetch.json`); this is
availability evidence, not qualification. Mac shipping inference, optimized
packages and installed GUI are outside this scoped batch and remain separate.

Top-level artifact inventory: `artifact-top-level-inventory.json` records 624
entries and reviewed reference harnesses. Existing artifacts were preserved;
none was deleted or promoted into distributable packages.

Neo partial collection: Store no-default Clippy passed in 31.80 s with unchanged
source. The batch then advanced to CLI Clippy. Receipt:
`artifacts/readiness-1817/neo-status-current.json`; complete batch and cleanup
results are still pending. No Mac shipping/installed acceptance is inferred.

**Neo complete:** all three scoped Clippy commands and all **37/37** exact
selected tests passed, followed by canonical static-cache verification.
Store/CLI/MCP Clippy: 31.80 / 117.28 / 23.94 s. Store/CLI/MCP/Core test steps:
39.10 / 113.11 / 52.99 / 14.88 s including compilation. Inputs unchanged;
incremental off; two build jobs. Eight timing reports, all logs and complete
test identities/outcomes are retained under `artifacts/readiness-1817/neo-receipts/`.
`neo-collected.json` verifies their timing hashes, ordinary owned-group exit,
successful `cargo clean` and removal of only the exact batch target. Source and
fixtures remain. This closes scoped moved-Unix/security/restoration evidence;
Mac shipping inference, optimized payload and installed GUI remain unqualified.

Existing enabled-native extension evidence was also collected before planning
its new run. R39's 1,292-test batch failed the mocked owner-refresh case after
two transient failures (`clean-break-r39-extension-native-r1.log`). The current
source test forces that watcher/console overlap and matches its native owner;
the current mocked suite passes it. The new enabled-native run remains pending;
the older failure is not replaced by a mock-only qualification claim.

## Next Windows process preflight (reviewed before execution)

Reuse the same frozen native seal and P10 target, with verified static ORT,
`local-inference`, incremental on and normal-host private fixtures. Exact command
is retained by the `process-preflight` runner receipt: pinned Nextest CI profile,
`airk-cli`, integration targets `cli_commands`, `service_runtime`,
`native_owner_recovery`, `combined_reads`; select the two CLI mapping/uninstall
tests, the two installed-loss tests plus initialized completed replay, the three
actual HTTPS tests, all three owner-recovery cases and all 12 combined-read cases.
Expected 23 runnable cases. Run one test process at a time and fail fast, keeping
each test's internal concurrency and deadlines. Stop on the first failure,
changed consumed input, or zero/mismatched selection. This is the actual matched
development-image preflight before optimized payload construction; no earlier
debug artifact is relabeled as a new build.

Selection audit during collection: the exact declared command selects **52**
tests, rather than the initial 23-case estimate. The estimate counted top-level
process/parity cases and omitted nested shared HTTP/retry test modules included
by the explicitly selected binaries. Source inspection confirms these stay
inside the four declared targets and filters; the command and scope are
unchanged. Preserve the original plan receipt and report every actual identity
and outcome. Do not filter cases away to match the estimate or count skipped
helper entries as passing native coverage.

### Process preflight collected: first service-runtime T2 failure

`artifacts/readiness-1817/process-preflight.json` records 1,148.77 s total
(45 s compilation and 1,100.72 s execution), unchanged consumed inputs and
the complete command/timing evidence. Of 52 selected cases, 49 ran: **48 passed,
one failed, three did not run** because the command stopped at the first failure.
The passing cases include all 41 combined-read cases, three owner-recovery
cases, two CLI mapping/uninstall cases and two actual HTTPS cases. Retain those
identities and results; do not rerun the 48-case campaign to discover this fault.

The new initialized completed-replay case failed before its replay assertions:
its immediate `RequestEpoch` response was not `request_epoch`. The original
failure message omitted that unexpected response. The preserved normal-host
fixture is identified in `replay-failed-fixture.json`; read-only retained
diagnostics (`replay-retained-diagnosis.json`) establish a
`runtime_upgrade_in_progress` control failure after activation selected and the
checkpoint committed. The service subsequently exited on its ordinary idle
deadline; diagnosis did not restart or stop it and did not change its ACLs.

Source-review hypothesis: Health intentionally bypasses the metadata actor.
Release clears the checkpoint fence and acknowledges without synchronizing a
metadata actor already waiting inside its sampled frozen receive branch. That
branch can still reject one request before observing release on its next loop.
The cached error alone does not identify actor versus front-door emission.
Missing cheap coverage is the deterministic release-during-frozen-receive
ordering; missing fixture precondition is same-instance metadata readiness.
No broad RequestEpoch retry or production behavior correction is justified by
this evidence. Review a minimal deterministic seam, retain full response
diagnostics and observe read-only Projects readiness before the single epoch
request. This is the **first T2 failure** for this component. The next process
batch may include only the corrected replay and three unrun cases after their
T0/T1 checks and source review; a second failure invokes the two-strike stop.

### Reviewed replay correction freeze

The implementing agent returned ownership before formatting. The primary
reviewed both affected files and the complete combined diff, including the
separate review in `artifacts/readiness-1817/final-independent-review.md`.
`replay-combined-review.json` records canonical native source
`4ca433e2bc4ccf7a01782fe4ab5b8ad029f0c172c2ed133b7309cdf1378c5048`
(1,132 files). Earlier Neo and process results retain their original source IDs.

The private frozen-receive extraction preserves production behavior and exposes
the already-sampled branch through a no-op production observer. The regression
uses real owned fence creation/removal and real channels to prove that branch
can issue its exact unexecuted refusal after fence removal. It does not claim
to reproduce the front-door coordinator or identify the retained error's emitter.
The fixture observes SELECT-only Projects readiness under its original five
second deadline, bound to the candidate's actual instance, and confirms unchanged
instance/runtime/executable identity before its canonical epoch request. No new
mutation retry or public retry classification is added. Full unexpected replies
and phase are retained; original Stop/exclusion/state-preservation checks remain.

The reviewed T1/T0 batch is scoped shipping CLI Clippy followed by **13** cases:
eight metadata-actor tests, three checkpoint tests and two readiness/diagnostic
observer tests. Exact commands and timings are recorded by `replay-scoped-clippy`
and `replay-observer-regressions` receipts. Stop on failure, changed inputs or
unexpected selection. Only after these pass may the four-case remaining process
preflight start. No agent may edit consumed inputs during these runs.

Collected: scoped shipping CLI formatting/Clippy **passed in 21.54 s**
(13.67 s Cargo); all **13/13** T0 cases passed in **49.26 s** including
27.31 s compilation and 20.65 s execution. P10 target, incremental on, all
consumed inputs unchanged; both receipts retain their timing HTML. The
deterministic actor ordering and readiness/diagnostic rules are now exercised.
The four exact cases in `final-windows-plan.md` are approved for the next T2
batch against the same canonical source freeze; fail fast and stop on input
change, non-four selection or failure. Retain the original failed fixture.

**Second process attempt collected and stopped:**
`process-preflight-remainder-r1.{json,log}` selected four cases but stopped at
the replay case: **zero passed, one failed, three unrun**, 22.49 s total
(0.58 s compilation, 20.78 s execution). Inputs and wrapper unchanged;
timing HTML retained. The new refusal is
`on-demand restart backoff active; retry later`, rather than the earlier epoch
observation error. Its exact lifecycle phase and retained startup evidence need
investigation; it is not yet established as the same cause or a product defect.

This reaches the component's **two-strike stop**. No third service-runtime T2/T3
attempt, including a broad workspace run containing these cases, may begin
before the complete root-cause note has been brought to the user. Preserve both
failed fixtures and collect their evidence without repair, restart or ACL change.
Continue unaffected source review and T1 checks. The delegated restoration owner
is investigating the exact error origin, full fixture/startup sequence and
existing cheap startup-budget coverage; no new source correction is authorized
until that investigation is reviewed.

Unchanged-source gates collected during that stop: full workspace/all-target/
all-feature Clippy **passed in 43.69 s** (42.80 s Cargo), and the distinct
lightweight CLI/no-default/all-target compile gate **passed in 70.57 s**
(69 s Cargo). P10 target, incremental on, all consumed inputs unchanged.
`final-workspace-clippy` and `final-lightweight-main` retain exact commands,
source groups and timing HTML. These are T1 results, not permission to relabel
the failed process campaign or start a third expensive service attempt.

Read-only retained evidence and independent source review narrow the second
failure to **initial ordinary activation before completed replay**: it starts
the candidate, commits selection, deliberately drains it for catalog admission,
then probes it again inside the existing two-second startup cooldown. Its
pending intent and journal remain preserved. The wrong assumption was that
those deliberate lifecycle steps would take long enough to avoid the cooldown;
the missing cheap regression covers the whole admission/drain/start sequence.
No startup receipt, counter, backoff, queue or assertion will be cleared or
widened to manufacture a pass. Review the existing pre-launch quiescent catalog
proof and all migration/rollback/resume siblings before correcting production.

### Independent library partition during the service stop

To preserve progress while the CLI owner completes the cheap correction, the
required all-feature workspace suite is partitioned by package. First run all
seven libraries (`--workspace --exclude airk-cli --all-features`) with pinned
Nextest CI profile, `--ignore-default-filter`, two test processes and fail-fast.
This restores the four routine excluded campaigns. CLI tests remain stopped.
The final CLI partition and complete workspace identity reconciliation remain
required; no partial partition is called a full workspace pass.

Review confirmed no library Rust source refers to CLI source, installer modules
or `CARGO_BIN_EXE_airk`; dependency direction is CLI to libraries. The consumed
native-library group includes all manifests (including CLI's), lock, toolchain,
library sources, legal/model inputs; only unconsumed CLI source/build-script
files are excluded. Harness, shared extension fixtures and public fixture groups
remain frozen. The CLI owner may edit only its assigned source and root docs;
one Cargo owner remains primary. This is independent source work, not concurrent
build/test/platform batches.

`workspace-list.py --exclude-cli` records exact selected identities and requires
all four campaign identities exactly once before execution. Expected all
selected tests pass; stop on first failure, zero selection or consumed-input
change. The complete command, selected count and timings are retained in
`final-workspace-libraries-list` and `final-workspace-libraries-tests` receipts.
The known ignored quota case retains its explicit quarantine disposition.

Listing correction: compilation/listing passed, but its first catalog assertion
failed because only **three** of the four prose/configured campaign names exist.
The saved catalog contains **2,117** selected library tests and 22 filtered or
ignored cases. The first audit's assertion that the fourth belonged to CLI was
incorrect; retain that audit receipt without treating its pending-CLI field as
coverage evidence. Live source contains no such test. Independent history
review locates its intentional accepted deletion in R2 commit `69677a29`: the
unused catalog-experiment prototype and its six-test integration target were
removed after clean compile/Clippy, as accepted in S1 review §2. No surviving or
renamed equivalent identity exists. The dated experiment record preserves the
decision to retain full production catalog builds.

Therefore do not restore the discarded prototype to satisfy a stale count.
Correct the dangling Nextest filter and current three-campaign prose, retain the
retired fourth's evidence and add a cheap check for dangling routine exclusions.
`missing-sparse-campaign-review.md` holds the independent findings. Re-audit the
saved complete selection after that correction without rebuilding unchanged
library binaries. Run and report all current cases plus the three current long
campaigns; the retired campaign is a disposition, not an executed pass.

### Planned-start admission reproduced cheaply

The primary reviewed and formatted the behavior-preserving extraction of the
actual health-to-startup-receipt boundary, retaining the immediate production
policy. `planned-startup-diagnostic.{json,log}` collected all **six** cases:
**four passed, two reproduced the exact cooldown refusal**, in 27.04 s including
compilation (0.20 s execution). Real owned receipt files and a manual Tokio clock
were used; no service process or elapsed sleep was needed. Normal immediate
startup, healthy no-op, malformed/backwards-clock/circuit-open refusals and
existing three-attempt persistence remained passing. No inputs changed.

This confirms the analyzed class at T0. The complete correction is now assigned
to the CLI owner: only a pending activation's exact bound startup may schedule
admission at the existing cooldown boundary; unchanged ordinary startup remains
immediate. The same 60-second startup deadline must cover admission and health.
Only a typed pre-append refusal can wait; no process failure/unknown effect may
be repeated. Preserve counters, receipt bytes/history, backwards-clock refusal,
managed dispatch and all catalog/migration/checkpoint proof ordering. The
two-strike root-cause note was linked and explained to the user before any
third process attempt. That attempt still awaits complete correction review
and passing T0/T1 evidence.

Collected retirement correction: all **three** skip-inventory contract tests
passed (0.49 s execution, 1.20 s receipt elapsed), including the independent
three-name/live-owner assertion. The current selection audit reused the saved
listing without recompilation, passed in 2.74 s and sealed every test binary.
`final-workspace-libraries-selection-current.json` records all **2,117** selected
identities, 22 filtered/ignored cases, the three restored campaign identities
and the accepted retired fourth. Earlier incorrect audit records are retained.

The approved next command is pinned Nextest CI profile with `--workspace
--exclude airk-cli --all-features --ignore-default-filter --test-threads 2
--fail-fast --locked --timings`; expected **2,117** selected cases pass, with
the same library/fixture/model identities and unchanged compiled test binaries.
Stop on first failure, changed inputs or unexpected selection. This partition
does not exercise or resume the stopped CLI component.
