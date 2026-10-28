# AIRK 1.5.7 development record

28 October 2026. Work on `feat/manageindices`; not a publication record. Native and extension manifests and lockfiles remain synchronized at 1.5.7.

Management sections use the common horizontal tabs: Indexing, Activity, Storage, Recovery, Service, Agents, Usage and Logs. Only the selected section mounts. The selected Management project persists across tabs, including machine-wide Service and Agents; changing the Explorer project resets that scope. Legacy `manage/projects` opens Indexing and `manage/indexing` opens Recovery. Keyboard navigation and active-tab accessibility use the existing Explorer contracts.

## Recovery corrections

The earlier report that 1.5.6 was recovering the remote index was followed by repeated failures and repeated successful Recover jobs with no new source publication. That initial progress report did not establish recovery. Review found a reproducible eligibility bug: `current-bootstrap.json` is a permanent completed database receipt, but the service treated its presence as unfinished recovery. The service now checks `current-runtime-bootstrap.json` and the publication transaction phase instead. A regression covers completed bootstrap, reopen, no-op recovery, later source publication and a genuine malformed unfinished runtime recipe.

Successful Recover, including an approved Auto Repair that selected Recover, resets exhausted watcher attempts so outstanding source revisions can run. It does not mark those revisions indexed, enable manual watching, unpause a project or clear explicit cancellation. Later successful publication/recovery supersedes old recovery attention without rewriting failed historical receipts. Queued work after a finished attempt is described as queued, with the preceding job named.

A persisted source-failure budget stops new watcher hints from replenishing repeated terminal failures across jobs and restarts. Explicit accepted retry/index/recovery can reset applicable budgets; replaying an accepted request does not. The existing same-job continuations, guarded repair approvals and independent helper/worker budgets remain authoritative.

Shared quota-ledger lock admission now waits up to thirty seconds during service work, with pause/cancellation checkpoints and native-lock progress observations. Only acquiring the lock retries, never the ledger operation. Optional accounting retains its shorter deadline. A real contention test exceeds the former two-second limit and verifies single execution and cancellation without stealing the lock. This reproduces a plausible failure path, but the remote machine's original two-second `project_operation_failed` remains unattributed: its 1.5.6 logs omitted the underlying cause.

Known recovery-stage, SQLite and filesystem failures now retain safe native codes rather than collapsing every error to `project_operation_failed`. Quota contention retains both its typed ledger context and underlying native lock cause, preserving existing bounded usage and scheduler admission callers. Control-failure log records retain a safe code. Raw source-bearing exception text is not written to logs. Read-only MCP execution-context zero worker counts are explicitly distinguished from the service scheduler's background capacity.

## Setup and diagnostics

A matching verified runtime version/payload and saved successful installation receipt skip terminal installation, runtime cleanup and setup notifications on ordinary reopen. New folders can still register, and accepted native setup continues without the editor. Runtime completion is saved before project preparation. A failed first registration/preparation receives project-specific wording and a deduplicated warning, preserving quiet reopen; explicit service Stop is reported as stopped. Connection migration runs once after workspace preparation, and explicit repair no longer installs the terminal command twice. Interrupted runtime installation, terminal/cleanup issues and explicit Install or Repair CLI retain the installation path. This is covered by the reopening harness; it is not a claim of a new cross-process installation mutex or complete multi-editor qualification.

Logs offers Generate report, Open report, Copy report and Save report. All use the exact captured bytes. The metadata-only report includes visible project observations, job/configuration outcomes, recovery budgets, watcher state and up to 128 cached native events within a 256 KiB bound, with explicit omissions and default path/name redaction. Save retained service logs separately exports the full available four-file, 4 MiB native log ring through the local CLI. Recent service IPC stays bounded to 32 records; rotated-away records remain unavailable and malformed records are counted and omitted.

## Unified MCP and startup migration

One `airk` server now advertises 26 unprefixed tools. Native evidence, optional editor help and inert UI drafts retain separate owners and availability. Names distinguish immediate reads from reviewed drafts; status/help combine duplicate former entry points, and view suggestions share one typed tool. Multi-root connections require an explicit configured binding, and window admission checks prevent UI selection from redirecting a proposal. Native discovery/help do not elect the service. Previous names remain hidden callable aliases. The [complete catalog and rationale](unified-mcp.md) documents the contract.

Trusted startup consolidates recognized project-local Codex, Claude Code, Copilot, Gemini, Antigravity, Cursor, Roo and Junie configurations in one guarded save per file. Both old definitions are removed; unrelated servers, disabled state and supported policies remain authoritative. Custom conflicts and unsaved/unsafe files remain for review. A recognized manual VS Code entry suppresses the duplicate provider, including cached resolution. User-wide/client-private approval stores require the client's controls and renamed tools require client approval after restart.

## Large-project limits

This branch improves durable work, progress and failure recovery; it does not introduce sharded databases or raise extraction ceilings. The automatic eligible-source budget remains machine-aware and capped at 4 GiB, with 100,000 extracted files by default. Reviewed scan policies support up to 64 GiB source/128 GiB extracted data and one million files. Individual source/text/parser limits, logical quotas and temporary-growth admission remain independent. These are admission ceilings, not qualified workload sizes. See [ingestion limits](ingestion-limits.md) and [scale qualification](scale-qualification.md). Bookbuilder's original remote failure and whole-project capacity still require confirmation on that machine.

## Validation and candidate scope

Windows native validation covered the workspace and targeted correction/completion runs. The broad workspace command reached 1,395 passing tests and 17 ignored tests before exposing one quota-contention regression: the new typed wrapper had hidden the original WouldBlock cause from bounded accounting admission. The wrapper now retains both types. The unchanged store usage suite then passed all 14 cases, including that contention regression; all 306 store library cases passed. The remaining 15 publication integration cases and workspace doctests completed separately. This is combined validation evidence, not a claim of one clean full post-correction workspace invocation.

Final affected checks passed 40 MCP library tests, nine real CLI/MCP protocol tests and 38 kernel service tests. All-target/all-feature workspace Clippy with warnings denied, formatting, version synchronization, whitespace and repository checks passed. Packaging-script tests passed 177 cases with four skips. The implementation review explicitly covered the combined native, installer, schema, configuration migration, UI and diagnostic changes; it was performed by the implementer.

The installed-runtime extension suite passed 342 tests with two platform-specific skips on the initial unified pair. It covers a real single endpoint, explicit Stop with offline help/catalog, closed/tampered editor adapters, admission-time project selection, two configured native projects, semantic work and same-request source restoration. The final pair includes the later quota-cause/help corrections; its exact artifact validation is recorded below after packaging. The compiled Management browser fixture passed its 38 assertions. Reports, quiet completed-runtime reopen, first project preparation failure, configuration migration and queued-work wording have separate extension regression coverage.


The earlier tabs-only candidate remains at `artifacts/local/management-1.5.7-20261027/vscode/publish/airk-1.5.7-win32-x64.vsix`, SHA-256 `9c999d5f60b179fb06a22804e180eab17f93c94f1487e4f50c50d58fe67da22b`. It is superseded for recovery testing and does not contain the later corrections. Its helper/client smoke and source identity apply only to those earlier bytes.

This is uncommitted local development work. Review is performed by the implementer, not an independent reviewer. No commit, push, publication, Linux/macOS/container or installed-editor qualification is claimed. The unified MCP implementation and replacement candidate are described above; earlier artifacts retain the old endpoints and must not be used to test migration.

## Final Windows installer

The exact final release pair passed the installed-runtime extension suite: 342 passed, zero failed, two platform-specific skips. It also passed real offline MiniLM inference, persistent DiskANN and default/vector/hybrid retrieval, model opt-out/re-enable, and the two-project native client campaign with 16 files per project. The latter verifies 16 native compatibility tools and all 26 public MCP tools, source preservation and resumption of the same accepted job after its source returns. These are bounded fixtures, not Bookbuilder or maximum-capacity qualification.

Installer: `artifacts/local/management-1.5.7-unified-20261028/vscode/publish/airk-1.5.7-win32-x64.vsix`.

SHA-256: `4d8a6f633dc054f0ff2b4cba7eaf0e89ee576b44b40af328c536654a2c36ba79`.

Runtime install-manifest SHA-256: `aa86e0c27f2f96ac29a7c554987cc80b121aa02e934722e05e854816568d4030`. Native source receipt: `49c4fb38ffa446db42f4f9f70be535ae8e8f996863fc464623a7adc2ad33ab1f` over 675 files. Package verification checked the complete inventory, exact tested executable pair, current compiled extension modules and current native source identity. The local candidate remains unsigned; publication/signing and other platforms are separate.

Install this VSIX manually on the affected Windows machine, restart VS Code and external coding-agent sessions, and verify the single AIRK tool catalog. Existing projects should resume their saved work without repeating completed installation steps. Review Management > Activity and export Logs if the original failure recurs.
