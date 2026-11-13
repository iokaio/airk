# Codex review of `feat/split`

Reviewed 13 November 2026, after the 1.8.34 installer and VSIX were built and committed. **Recommendation: address the P1 findings before treating this branch as ready for distribution or broader qualification.** The split has useful contract boundaries, but several failures sit between individually tested components: installer publication and recovery, native selection and UI selection, response delivery and accepted mutations, and packaging after crate extraction.

This is a review and repair recommendation, not authorization to implement the changes below. No production code, tests, fixtures, configuration, or generated assets were changed. This report and its entry in the existing task ledger are the only repository edits.

## Scope and evidence

- Branch point: `59dd3c721ca4c8301664827abaa9ea2ac6fd8dc8` (`feat/simplify`). The local branch reflog confirms `feat/split` was created there.
- Reviewed head: `6fc3d3566217ae0af8a55f5c056cb3dc426b63d4`, including all 207 branch commits and the final net changes across 977 paths. This scope excludes older work inherited from `feat/simplify`; it is not merely a review of the last 1.8.34 patch.
- Built production source: `71d5ca12234e8732c695bfaa373727469dc2568f`. The final head adds the build record. The working tree was clean when review began.
- Five-minute monitoring ran until approximately 06:22 MST, when the completed build receipts, new commit, and clean tree were observed. The continuing existence of a Claude process was not treated as evidence that a build was unfinished.
- Installer: `artifacts/local-service-installer/20261113-061110/airk-service-1.8.34-x86_64-pc-windows-msvc-installer.exe`, SHA-256 `81a5df0f3d728ea1f2330f87b40187e440a96b8293c051e64835657ef22be93a`.
- VSIX: `artifacts/local-vsix/20261113-062052/vsix/publish/airk-1.8.34.vsix`, SHA-256 `03b2eccc4a08b4b497518ef381457771ab7df259d98fba83b18e6db9a5d8d7f3`.
- Both file hashes were recomputed and matched the receipts. The installer receipt reports local signing and a clean source commit; this review did not independently repeat Authenticode verification or establish hosted release provenance.

Eight read-only component reviews covered admin frontend, extension, installers/host, HTTPS/admin backend, CLI/IPC/process, catalog/read authority, service/supervisor/inference, and build/release paths. The primary checked the reported call paths, removed duplicate findings, ran three small diagnostics, and reviewed the combined report. Mechanical moves were compared with their previous owners; large unchanged bodies were used as dependency context rather than claimed as newly qualified line by line. Documentation, fixtures and test selection were included. This is broad source review, not exhaustive runtime proof of every changed line.

AIRK MCP was unavailable: deferred discovery exposed no AIRK tools, `.mcp.json` had no servers, and `.codex/config.toml` only configured agent concurrency. Git and live file reads supplied the evidence. No private index or `temp/` contents were used.

**Evidence labels:** “source-confirmed” means the trigger and outcome follow from the inspected implementation, without a fresh native reproduction. “Reproduced” below means a narrowly scoped in-memory diagnostic, not an installed journey. Hypotheses and previously accepted omissions are listed separately. P1 means a substantial correctness, recovery, isolation, or build blocker; P2 means a concrete defect or validation gap that should follow in a bounded repair.

## P1 findings

### R01. Reconnect can remove healthy agent connections

[serviceExtension.ts:177](../../packages/vscode/src/serviceExtension.ts#L177) checks whether any orphaned HTTPS or retired stdio entry remains after repair, then calls `removeAgents` without a filter. The wrapper invokes unrestricted `agents remove --no-user`; [manage.rs:336](../../crates/airkctl/src/agents/manage.rs#L336) removes every recognized AIRK entry in the selected project files when no connection is supplied.

**Trigger and impact:** a workspace contains a healthy or just-repaired entry alongside one orphaned entry. Choosing Reconnect → Repair can delete all those AIRK entries, although the confirmation describes removing only entries without a connection. This concerns project entries; the current caller excludes user-level entries.

**Recommendation:** carry the reviewed file/server-key identity into native removal and remove only the failing entries. A connection ID alone cannot identify retired stdio entries. Report individual repair/removal errors. **Regression:** exercise the composed command with healthy, repairable, orphaned and retired entries across clients; require unaffected entries to remain byte-identical. Source-confirmed.

### R02. Failed automatic index switching leaves the previous index active

[windowScope.ts:295](../../packages/vscode/src/airkctl/features/windowScope.ts#L295) changes the route binding to the newly selected project without suspending the old automatic grant. Its failure path at line 355 changes only the displayed route. [serviceExtension.ts:84](../../packages/vscode/src/serviceExtension.ts#L84) chooses W4 reads whenever `windowScope.active` is true, while [explorerController.ts:556](../../packages/vscode/src/explorerController.ts#L556) blocks `not_readable` but not `checking` or `failed`.

**Trigger and impact:** establish automatic scope A, select B, then delay or fail B's catalog lookup. Reads presented in B's context can still use A. **Reproduced:** after the injected B failure, the route was `failed`, `active` was still true, and the selected index remained A.

**Recommendation:** bind read admission to the requested backing binding and suspend the previous automatic route during a switch. Keep explicitly selected multi-index scopes distinct. **Regression:** defer and fail B's lookup at each resolution stage; no B-context read may use A before or after failure.

### R03. A stale native read can restore a scope the user replaced or cleared

[session.rs:1240](../../crates/airkctl/src/cli/session.rs#L1240) reselects the scope captured by an older read after catalog revision drift. Selection/release does not advance the service/window epoch used to reject stale effects; successful grant effects later overwrite the held scope. The native grant's own compare-and-swap protects changes during that grant call, not a user choice completed before this later stale reselect begins.

**Trigger and impact:** pause a read of A before its automatic reselect, select B or release the scope, then resume the old read. It can grant A again, undoing the user's choice.

**Recommendation:** serialize selection, release and automatic reselection, or carry a selection generation through the native authority mutation. A check after granting is too late. **Regression:** use barriers around stale-read reselection and assert B/cleared authority survives. The existing serial reselection test does not cover this interleaving. Source-confirmed; no native concurrency run.

### R04. Admin reports unknown outcomes and busy refusals as completed success

[admin_plane/mod.rs:988](../../crates/airk-https/src/admin_plane/mod.rs#L988) recognizes failures by a small set of status names and suffixes. `CatalogOutcomeUnknown`, `SourceChangeResolutionOutcomeUnknown`, and `SourceChangeBusy` fall through to HTTP 200 / `ok: true`. [mutations.rs:226](../../crates/airk-https/src/admin_plane/mutations.rs#L226) also classifies these as `Completed`. Native handlers and the CLI distinguish these outcomes from success.

**Impact:** `ApplyCatalog`, setup and Make readable advance on the successful envelope, potentially announcing acceptance or readability when the outcome is uncertain or work was refused. These are production variants, including setup lookup/commit failure outcomes, not hypothetical enum additions.

**Recommendation:** classify typed native outcomes explicitly and preserve unknown, refused, accepted and completed states, together with the complete native evidence. Dependent UI actions must wait for reconciliation. **Regression:** real-shaped native variants through the admin handler and frontend, including receipt recovery; a generic `Failure` fixture is insufficient. Source-confirmed.

### R05. Accepted stop/restart can become permanently pending when reply delivery fails

[service_runtime.rs:900](../../crates/airk-service/src/service_runtime.rs#L900) durably accepts the lifecycle request and records that shutdown is needed. At [line 988](../../crates/airk-service/src/service_runtime.rs#L988), response writing uses `await?` before `shutdown.notify_one()`. A disconnected client or failed write therefore returns without scheduling the accepted drain.

**Impact:** the service can remain running with durable `Draining` and saved Stop. A new lifecycle key is refused, and same-instance abandonment is refused. The CLI reports an unknown outcome rather than completing this missing transition.

**Recommendation:** make accepted shutdown scheduling independent of successful reply delivery, preserving idempotency and the original receipt. **Regression:** inject a writer failure immediately after durable acceptance; observe drain notification and lifecycle settlement. Source-confirmed; not reproduced against the running service.

### R06. An interruption between record and pointer publication blocks installer recovery

[setup_role/mod.rs:766](../../crates/airk-service/src/setup_role/mod.rs#L766) replaces `install-record.json`; the pointer is updated later at line 806. Every subsequent plan/prepare/resume first calls [files.rs:221](../../crates/airk-service/src/setup_role/files.rs#L221), which validates the new record against the old pointer's digest before executing the remaining receipt phases.

**Trigger and impact:** interrupt a same-folder repair/upgrade after record replacement but before pointer publication. The documented retry path refuses `installation_unverified` instead of resuming the interrupted transaction.

**Recommendation:** design receipt-bound recovery for the prior/new publication states, while keeping ordinary pointer verification strict. Any I4 format change needs its own reviewed contract step. **Regression:** filesystem interruption fixtures immediately before and after both publication writes. The pure remaining-action test does not prove this filesystem recovery path. Source-confirmed.

### R07. Installed `airk` cannot find its paired CLI on Windows/Linux

[manual_start.rs:29](../../crates/airk-installer/src/manual_start.rs#L29) always looks for `airkctl` beside the running `airk`. The [Windows inventory](../../installers/windows/airk-service.iss#L50) and [Linux installer](../../installers/linux/install.sh.in#L218) put them in `service/` and `bin/` respectively. `main.rs` passes the actual current executable to this resolver.

**Impact:** running installed `airk` with no arguments reports a missing CLI/reinstall instruction instead of starting or observing the service. The macOS sibling layout avoids this specific defect.

**Recommendation:** resolve the verified I4 executable pair, with any development layout handled explicitly. **Regression:** literal platform-layout fixtures followed by the installed no-argument command at the package boundary. Existing message-rendering tests do not exercise layout lookup. Source-confirmed; no installed start performed.

### R08. “Keep data” uninstall prevents the promised later reinstall

The [Windows uninstall prompt](../../installers/windows/airk-service.iss#L690) promises retained data for a later installation. Shared removal deletes the pointer and record at [setup_role/mod.rs:915](../../crates/airk-service/src/setup_role/mod.rs#L915). On reinstall, [location validation](../../crates/airk-service/src/setup_role/mod.rs#L435) recognizes nonempty owned locations only through that existing installation record. Without it, data/cache/logs must be empty and state may contain only `setup/`.

**Impact:** retained index data and registry state make default/original locations fail `location_refused`. This affects the shared native flow, not just the prompt.

**Recommendation:** retain authenticated ownership evidence sufficient for the supported reinstall path, without treating arbitrary nonempty folders as owned. **Regression:** uninstall-keep → reinstall with real retained directory contents, plus foreign-directory refusal cases. Source-confirmed.

### R09. Setup does not reject a newer or incompatible existing installation before replacement

[Split plan §9.2](split-plan.md#92-later-installs) requires explicit reset for a newer split build or incompatible state contract. Setup reads the existing record but does not compare those fields with the candidate before draining/placement. [Placement verification](../../crates/airk-service/src/setup_role/mod.rs#L653) compares the candidate inventory with its own binaries; writing the new record stamps the current state contract.

**Impact:** an older signed installer can replace a newer installation and retain its state as an ordinary install. Pointer/record consistency and candidate binary version checks do not establish state compatibility.

**Recommendation:** add a pure preflight compatibility decision before destructive effects. Require the already specified explicit reset where applicable. **Regression:** same-build repair, compatible upgrade, downgrade, and state-contract mismatch. Source-confirmed; no downgrade attempted.

### R10. Windows CLI uninstall bypasses the owner of program removal

[uninstall.rs:142](../../crates/airkctl/src/uninstall.rs#L142) launches the installed native setup role on every platform. On Windows, [setup_role/mod.rs:930](../../crates/airk-service/src/setup_role/mod.rs#L930) deliberately leaves program-file removal to Inno. The CLI path never invokes that uninstaller.

**Impact:** `airkctl service uninstall --yes` can remove the installation pointer/record while leaving executables, user PATH integration and Windows uninstall registration behind.

**Recommendation:** delegate to the verified Windows uninstaller or a complete owned platform removal handoff, carrying the index-deletion choice and honest failure/receipt state. **Regression:** platform launcher selection and the complete removal ownership chain; operation-JSON validation alone cannot establish uninstall completion. Source-confirmed.

### R11. Selection renewal can leave catalog admission fenced after revocation

[catalog_admission.rs:2047](../../crates/airk-catalog-control/src/catalog_admission.rs#L2047) removes the entire installed selection entry when the current lease is acknowledged revoked, but settles only that lease's pending obligation. A renewal can retain `pending_previous`. A catalog fence records current and previous leases, then the coordinator processes current first. The [supervisor's previous-lease path](../../crates/airk-supervisor/src/project_supervisor/catalog.rs#L1574) sees the entry already pruned and returns without settling that remaining obligation.

**Trigger:** a renewal's worker acknowledgment is delayed/uncertain past the private exchange timeout, followed by a catalog mutation before settlement. This is not a claim that all ordinary serialized renewals race.

**Impact:** `require_commit_ready` continues refusing; source/policy/retirement work and admission can remain blocked until worker reap or restart.

**Recommendation:** retain and settle every lease obligation before pruning, or use explicit worker proof that one acknowledgment settles the whole family. Keep the fence strict. **Regression:** inject expiry, reserve renewal without acknowledgment, close an Index fence and drive both revocations through the actual pending/ack sequence. Source-confirmed; runtime recurrence unmeasured.

### R12. The container build and smoke path were not migrated to the split pair

[build-service-container.py:184](../../scripts/build-service-container.py#L184) builds/stages only `airk`, then calls metadata production that now requires `airkctl` and the admin inventory. The Docker COPY/input roster omits `split_receipts.py` and its `standalone-installer-contract.py` dependency. The same driver still emits retired `airk runtime container-host` and `airk runtime self-test` commands; [check-container.py](../../scripts/check-container.py#L514) retains old CLI commands and schema 1/2 assumptions.

**Impact:** current container CI cannot produce and qualify the split image. Fixing only the first missing import will expose the next failure.

**Recommendation:** correct the entire container boundary in one reviewed change: Python import closure, two executables, admin seal, schema-3 metadata, supported role entrypoint, and `airkctl` smoke commands. Preserve the fixed approval pin and narrow copy roster. **Regression:** fixture-driven execution of the current-version container driver and recipe import checks before any Docker/native build. Source-confirmed; Docker was not run.

### R13. Local installer success receipts do not establish frozen consumed inputs

[build-local-service-installer.py:203](../../scripts/build-local-service-installer.py#L203) records commit/dirty state once, calls `cargo_pair` directly, signs the images, and reads the live admin digests again during staging. It does not compare pre/post native, admin or static-input identities. It also bypasses the canonical builder's CLI import and embedded-model checks.

**Trigger and impact:** a consumed source/admin/static input changes during the build/signing interval. The wrapper can report `built`, initially clean source, and an admin inventory that does not describe the compiled bundle.

**Recommendation:** share canonical staged-runtime validation and sealing while retaining one Cargo invocation. Freeze native/frontend identities separately and fail on consumed-input drift before announcing a successful artifact. **Regression:** inject changes between compilation and staging in the wrapper tests. **This is a build-contract defect, not evidence that the actual 1.8.34 files are mismatched.** Their hashes matched their receipts. Source-confirmed.

## P2 findings

### R14. Replacement sessions lose live event subscriptions

[session.ts:278](../../packages/vscode/src/airkctl/session.ts#L278) retires the old child on unbind/image replacement, but `retire()` does not emit `session_lost`. [WindowAssistance](../../packages/vscode/src/airkctl/features/windowAssistance.ts#L53) clears its subscribed latch only on that event. The old child's exit is ignored after detach.

**Reproduced:** a real `AirkctlSession` with injected children, connected to `WindowAssistance`, subscribed once on child 1 and zero times on child 2 after unbind/rebind. No `session_lost` event was delivered. Commands reconnected, but status/jobs/Assistance delivery did not.

**Recommendation/test:** notify consumers of every session-generation change; reset subscription and sequence state. Retain this composed fake-child regression and prove new-child events arrive. Existing replacement tests that launch no child and tests manually injecting `session_lost` miss the interaction.

### R15. Single-index window/admin selections use legacy response handling

[project_client.rs:886](../../crates/airk-frontend/src/project_client.rs#L886) reports only `catalog_session` through `scoped_selection()`. The new selection backend instead sets `selection_session` at line 1371. [Unified](../../crates/airk-mcp/src/unified.rs#L760) uses the former discriminator for scoped help, status, diagnostics, errors and enrichment.

**Impact:** one-index W4/A2 help loses selected-scope handling; other reads enter legacy enrichment and timeout handling instead of preserving canonical scoped results. Two-index coverage does not establish this path.

**Recommendation/test:** represent both authority sources in the single-index scope descriptor; compare complete one-index CLI/W4/A2/native results for help, status, diagnostics and parse, including timeouts/refusals. Do not invent a persisted grant to satisfy the discriminator. Source-confirmed.

### R16. Explorer retry decisions do not understand selection-authority receipts

[explorerEvidence.ts:200](../../packages/vscode/src/explorerEvidence.ts#L200) passes `spec.catalogSession`, absent for W4, into [readRetry.ts:138](../../packages/vscode/src/readRetry.ts#L138). The validator requires catalog selection and native-control fields, while the new native producer supplies `window_scope_selection` and `scope_identity`. Safe pre-submission connection failures therefore surface instead of receiving their authorized retry.

Core also accepts selection-sourced single-required-member decisions that the TypeScript mirror rejects. Those additions have Rust-only tests rather than shared decision cases. This combined-read gap is distinct from the ordinary direct one-index W4 path.

**Recommendation/test:** extend the hand-written semantic validator using reviewed shared native cases for both authority sources. Bind retries to exact active authority, the original deadline and the existing single-retry limit; uncertain execution must remain non-replayable. Source-confirmed.

### R17. Some admin unknown outcomes discard their only recovery identity

[mutations.rs:278](../../crates/airk-https/src/admin_plane/mutations.rs#L278) returns `Unknown` after two lost replies for non-journaled keyed operations. [mod.rs:503](../../crates/airk-https/src/admin_plane/mod.rs#L503) stores the review identity only for an answered operation. Policy resume and split recovery consequently return no handle for the receipt read, and Saved requests cannot recover them because they were not journaled. The policy page disables Read resume receipt when the review token is absent.

**Recommendation/test:** retain an opaque recovery handle before dispatch and expose recovery without minting another operation identity. Follow the shared-contract process for an envelope change. Test native acceptance followed by two lost replies and a successful receipt lookup through the actual admin handler. Source-confirmed.

### R18. Reviewed admin responses expose request identities reserved for the server

Redaction runs in generate-mode [issue()](../../crates/airk-https/src/admin_plane/mod.rs#L529), but reviewed apply and receipt responses bypass it through `native_request`/`native`. Real catalog and setup receipts carry caller/request-key fields. This violates the [I3 identity-custody contract](contracts.md#L182), although no privilege escalation was demonstrated.

**Recommendation/test:** consistently remove the server-held identity from all relevant outcome paths while retaining other native evidence. Use actual preview/apply/receipt response shapes; the current identity test substitutes `RequestEpoch` results that cannot expose this omission. Source-confirmed.

### R19. Admin selection races can display one object's details with another object's actions

- [Activity openJob/jobChange](../../packages/admin/src/activity.tsx#L374) assign `jobRecord` after awaiting without a selection guard. Select A then B; late A details can appear with B's identifier and Retry/Cancel target.
- [CatalogList.load](../../packages/admin/src/indexLibrary.tsx#L269) assigns rows, cursor and revision after every response, bypassing `useCall`'s display guard. Reload/filter/continuation races can replace a newer list.
- [IndexDetail](../../packages/admin/src/indexLibrary.tsx#L1870) labels an old successful detail cache with the current prop ID before the new request completes; the subtree is reused across indexes.

**Recommendation/test:** bind each accepted record/page to its identity, namespace/revision and request generation. Derive actions from that accepted record; key or invalidate detail state on selection changes. Test reverse-order reads, pending action plus selection change, and reload/continuation overlap. Source-confirmed; no browser race reproduced.

### R20. Response-body failures escape the admin adapter

[api.ts:55](../../packages/admin/src/api.ts#L55) catches `fetch()` rejection but awaits `response.text()` outside the catch. The launch-code exchange has the same pattern. [useCall](../../packages/admin/src/render.tsx#L83) never reaches its done state when that promise rejects; bootstrap can fail before showing an error screen.

**Reproduced:** a response with received headers and a rejecting body caused `call()` to reject instead of returning a transport outcome.

**Recommendation/test:** classify body-read failures, preserve observed status, clear authorization on an observed 401 even with a broken body, and provide unknown-outcome recovery for mutations without replay. Test read, mutation and bootstrap cases; include a final bootstrap error path.

### R21. “In VS Code” admin opening conflicts with anti-framing headers

[serviceExtension.ts:77](../../packages/vscode/src/serviceExtension.ts#L77) uses `simpleBrowser.show`. [Admin headers](../../crates/airk-endpoint/src/admin_contract/mod.rs#L929) set `frame-ancestors 'none'` and `X-Frame-Options: DENY`. Microsoft's Simple Browser puts the target in an iframe, as shown by its [official implementation](https://github.com/microsoft/vscode/blob/main/extensions/simple-browser/src/simpleBrowserView.ts#L154-L157).

**Impact:** trusted TLS does not make this embedded option displayable; successful command dispatch is not proof that the page loaded. **Recommendation/test:** retain anti-framing protection and use the external browser, or separately review a supported embedded design. Explain the actual limitation. Validate real headers against the chosen browser container; mocks of `showInVsCode` cannot catch this. Source-confirmed integration mismatch, not a browser reproduction.

### R22. The folder picker never receives its intended initial focus

[folderPicker.tsx:122](../../packages/admin/src/folderPicker.tsx#L122) declares and focuses `pathInput`, but the rendered input lacks that ref. Focus remains on Browse outside the modal, where its internal Escape/Tab handler does not control navigation.

**Recommendation/test:** attach the ref, make the background inaccessible while the modal is active, and restore focus only to a connected opener. A DOM test must cover initial focus, Tab/Shift-Tab and Escape. Source-confirmed; visual and screen-reader behavior was not exercised.

### R23. Generated certificate commands do not quote paths as literal shell data

[agents.tsx:271](../../packages/admin/src/agents.tsx#L271) inserts the CA path into double-quoted PowerShell/POSIX commands. Dollar signs and backticks can expand or evaluate; POSIX quote characters can terminate the value. The nearby executable-helper path restrictions do not protect this separate CA path.

**Recommendation/test:** use correct shell-specific literal quoting and test round trips for spaces, apostrophes, dollar signs and backticks. Do not execute test substitutions. This is a command-generation defect, not a demonstrated attacker-controlled exploit. Source-confirmed.

### R24. Windows installer finalization errors are swallowed

[airk-service.iss:639](../../installers/windows/airk-service.iss#L639) catches final setup, verification and PATH failures in `ssPostInstall`, shows a message, then returns normally. It does not propagate the native failure to the installer result.

**Recommendation/test:** preserve failed artifacts/receipts and return a failing installer outcome. Test a forced finalization failure through the real Inno wrapper; another source-string assertion cannot prove its exit behavior. Source-confirmed control flow; no installer failure was injected.

### R25. Native source identity omits the compiled-in signing policy

[signature.rs:36](../../crates/airk-process/src/signature.rs#L36) embeds `scripts/signing-identity.json`, but [source_identity()](../../scripts/build-service-runtime.py#L353) does not enumerate it. Different compiled signer policies can therefore have the same source digest, and an in-build policy change escapes the stability check.

**Recommendation/test:** include this input and audit external production `include_str!`/`include_bytes!` dependencies against the seal. Use an independently specified fixture proving that changing the signing policy changes source identity. Source-confirmed.

### R26. Extraction removed test coverage without changing the reported gates

Three distinct validation problems need correction:

1. [Routine Windows/macOS CI](../../.github/workflows/ci.yml#L255) still selects `airk-cli --lib`; private library tests moved into service, installer, host, supervisor and other crates. Linux workspace tests do not execute Windows/macOS-specific cases. Repoint the focused selection and compare test identities, not counts.
2. Moved component contracts still prescribe old owning-crate selectors. Examples: [setup_role](../../crates/airk-service/src/setup_role/CONTRACT.md#L245), [admin_plane](../../crates/airk-https/src/admin_plane/CONTRACT.md#L194), [admin_contract](../../crates/airk-endpoint/src/admin_contract/CONTRACT.md#L83), and the host contracts. A re-export does not bring a dependency's private tests into `airk-cli --lib`. Update owner/visibility claims and require nonzero relevant selections; retain the correctly located admin integration binary.
3. [The in-process Ctl adapter](../../crates/airk-cli/tests/common/ctl.rs#L62) records environment setters but uses only selected AIRK variables. [error_outcomes](../../crates/airk-cli/tests/error_outcomes.rs#L29) still labels cases with different backtrace environments without applying them. Preserve actual subprocess tests for environment/exit behavior.

These are source-confirmed selection/harness gaps, not fresh failed test runs. Restore the intended cheap coverage instead of compensating with repeated full qualification.

### R27. Online CLI commands disagree about automatic startup

[tools catalog reads](../../crates/airkctl/src/cli.rs#L1070), [library reads](../../crates/airkctl/src/cli/library.rs#L177), and [storage/folder commands](../../crates/airkctl/src/cli/storage.rs#L18) verify/discover the installation and send IPC directly. Project commands and library setup instead use `running()` and can start an exited service. This conflicts with the common automatic-start rule for commands requiring the service.

**Recommendation/test:** share online-command admission, preserving nonstarting version/status/offline diagnostics and explicit Stop. Inject running, exited and explicitly stopped states across representative command families. Source-confirmed; the user's service was not stopped to reproduce it.

## Make AIRK Admin more like Explorer

The request should be implemented as clearer workflows and presentation, while keeping the browser's authority separate from editor scopes. The current Agents, Usage and Activity pages already provide useful readable summaries and collapsed full results. Extend that approach to the remaining pages. This assessment is based on source, not a visual browser session.

| Priority | Page/problem | Recommended behavior | Existing Explorer reference |
|---|---|---|---|
| First | Search requires namespace, revision, index/project IDs, tool name and JSON. | Named index/index-set choices with readiness reasons; keep native identities internally. Show an applied-scope banner. Use a normal query form with search mode, path filter and bounded limits; advanced native arguments remain optional. | [Indexing scope choices](../../packages/vscode/src/explorerWebview/indexing.tsx#L73), [scope banner](../../packages/vscode/src/explorerWebview/scopeBanner.tsx), [search](../../packages/vscode/src/explorerWebview/search.tsx#L67). |
| First | Search results primarily expose raw response structure. | Result cards with original location, cited lines, excerpt, publication/revision, coverage and truncation. Keep complete JSON/Markdown export. Browser source actions need an explicit supported equivalent to editor-only opening. | Explorer search and complete-response components. |
| First | Overview asks users to interpret raw health and binding fields. | Service/version status, named index, last observed time, published readiness, current job/progress and one useful next action. Distinguish indexing, paused, stale, unavailable and not readable. | [Explorer header/status hierarchy](../../packages/vscode/src/explorerWebview/app.tsx#L99), [project status](../../packages/vscode/src/explorerWebview/projectStatus.tsx). |
| First | Index library is a long page mixing browsing and every advanced operation. | Searchable named list plus selected-index details. Use Overview, Sources, Activity, Usage and Advanced sections. Keep create, rename, retire and settings attached to the selected index; disclose exact targets before mutations. | [Indexing](../../packages/vscode/src/explorerWebview/indexing.tsx), [management views](../../packages/vscode/src/explorerWebview/manageViews.tsx). |
| First | Setup does not guide the user into monitoring and searching the created index. | A focused flow: folder → name/options → readable native review → create. After confirmed acceptance, open that index/job, show progress, and offer Search when ready. Never equate accepted with completed. | Explorer status/actions and selection/readiness messaging. |
| Next | Ordinary tasks require collection, digest, source-scope or grant JSON knowledge. | Put storage defaults and technical identifiers in Advanced. Use folder controls and named set/member/agent choices. Present the authoritative review as readable affected folders, scope and consequences, with complete details available. | Explorer named choices, fact cards and contextual notices. |
| Next | Eleven peer navigation entries compete equally. | Top level: Overview, Indexes, Search, Agents, Management. Group Activity, Usage, Storage, Service, Logs and Recovery under Management. Add deep links and contextual help. Preserve service-wide versus selected-index context. | Explorer's grouped navigation and contextual help. |
| Next | Errors and copies do not consistently explain outcome or completion. | Say what failed, the affected index, what remains known, and the exact recovery action. Copy buttons report success/failure. Preserve full native refusal, continuation and retry evidence below summaries. | [CompleteNativeResponse/CopyDetails](../../packages/vscode/src/explorerWebview/components.tsx#L8). |
| Next | Keyboard, narrow layout and status announcements are not behaviorally covered. | Fix modal focus first; add visible focus, semantic labels, tab keyboard behavior, error/status announcements, responsive tables/cards and non-color-only status. Check readable light/dark styles rather than assuming editor CSS works in a browser. | Reuse Explorer's presentation conventions, with browser-specific accessibility tests. |

**Concrete acceptance journey:** open Admin in its supported browser → create an index from a folder without typing internal IDs → see accepted/running/ready states → choose a named search scope → inspect cited results and complete evidence → pause/resume or recover a failed operation using a clearly identified target. A keyboard-only user should complete the same journey. Selection changes must never leave stale evidence or actions attached to a different index.

Reuse presentation patterns and generated native contracts; do not create another hand-maintained response model or move service authority into the UI. Keep offline assets, text-only evidence rendering, session-bound reviews, typed destructive confirmations and complete canonical results. Any shared frontend extraction should follow the repository's structural plan rather than being bundled into correctness fixes.

The Service page's **Restart** also needs truthful behavior: it currently initiates stop-for-restart and ends the browser session, while the browser cannot perform the required local startup step. Explain that continuation before confirmation, or deliver a separately reviewed complete restart path. Do not leave a page merely saying the service is restarting.

## Other recommendations and unresolved questions

- **Current documentation contradicts the implemented split.** The top of [README](../../README.md#L3) and [documentation index](../README.md#L3) still presents Phase 4 as current and says the extension uses the retired session command. The [architecture dependency table](../architecture.md#dependency-boundaries) omits the extracted owners and says HTTP exposes no lifecycle/index-management commands despite the separate admin plane. Some [contributor](../../CONTRIBUTING.md#L55) and [build guidance](../ci-and-builds.md#L258) says local signing/builds are unimplemented or unrun. Rewrite current entry points around the actual two-product behavior and qualification status; move superseded statements under explicit dated history. Align the Marketplace description and current release record without changing historical receipts.
- **Admin lacks a frontend behavior suite.** Its package scripts typecheck/build/check assets; operation-name and HTML-sink checks do not exercise async state, focus, response classification or user journeys. Add the deterministic cases above before another native rebuild. Native actual-transport parity remains necessary alongside UI tests.
- **Installer exclusion during placement needs one controlled confirmation.** The native install lock ends when `--prepare` returns; wrappers then place files before a separate completion call. No durable cross-invocation exclusion was found, and CLI Start does not consult the replacement notice. Test Start/second setup at that boundary before declaring the transaction isolated. This remains an unexecuted concurrency hypothesis.
- **Workbench's default route needs a targeted new-index test.** It prefers the registry project specification even when a window-scope entry exists. Ordinary Workbench transport can therefore use the legacy binding route that new split indexes lack a grant for. Verify create → default Workbench read with real canonical handlers before claiming the Explorer fix covers Workbench.
- **Session queues and agent-file writes deserve focused follow-up.** The session has unbounded input/reply channels despite bounded execution slots. Agent config replacement checks old contents before temporary-file work, then overwrites; that is not atomic compare-and-swap against another writer. Add blocked-reader/backpressure and deterministic concurrent-edit observations before selecting a repair.
- **Monitoring subsets need disclosure.** `LiveStatus` tracks only the first eight bindings. Prioritize the selected project and make omitted monitoring explicit; a ninth selectable index should not silently lack progress.
- **Parity normalization can mask meaningful differences.** The migrated harness renames every 32/64-hex token, including potentially stable content/model/revision hashes. This is documented U95 behavior, so tightening it is a reviewed improvement, not an assertion that an approved fixture failed. Prefer field-specific dynamic normalization with negative tests for changed stable provenance.
- **Unicode installation paths are currently rejected by wrappers.** ASCII-only operation encoding rejects default paths under some non-ASCII account names. Replace it with correct UTF-8 JSON handling and explicit Unicode fixtures while retaining canonical/native path validation.
- **Known hosted Windows compiler risk remains open.** Hosted packaging still passes `/NI` to pinned Inno 6.7.1; local packaging removed it after a real rejection. The ledger records this unresolved discrepancy. Confirm the hosted compiler's argument handling cheaply before rebuilding; this review did not reproduce a hosted failure.
- **Accepted omissions remain completion gaps.** macOS login/first-launch support, executable-preserving interrupted Unix uninstall, deferred admin events/rate limiting/operation coverage and complete transport parity, moved-program cleanup, and the remaining predecessor roster are explicitly incomplete in the plan/contracts. Do not relabel them as passing because a Windows artifact was built. Local versus hosted notice inventories and removed cold-client regression coverage also need explicit ownership.

## Validation performed and limits

1. Read-only branch/reflog/diff inventory and live source/contract/test review, with the revision identities above.
2. Recomputed SHA-256 for both newly built artifacts; both matched their receipts.
3. One batched in-memory diagnostic, PowerShell here-string piped to `node -`, using Node `v24.21.0` and the checkout's TypeScript `5.9.3`. Current source was transpiled only in memory; no compiler output or source files were written. **Three selected cases, all three defect observations reproduced:** replacement subscription loss (R14), failed automatic scope switch retaining A (R02), and response-body rejection escaping the adapter (R20). The session case used injected framed child streams; the scope and HTTP cases used injected observations. No native process or service mutation occurred. These are defect confirmations, not three passing product regressions. The exact diagnostic invocation/output remains in the review session's tool transcript.
4. `python scripts/release-version.py check --expected 1.8.34` passed: native, extension and lockfiles agree. `scripts/check-repo.ps1` passed, including document links, AGENTS/CLAUDE equality, the process-start roster and generated I2/I3 checks. `git diff --check` passed. The final report's local references/line targets and whitespace were also checked directly, including the new untracked file. Git needed a process-local safe-directory override; global configuration was unchanged. Sandbox warnings about the user's global ignore file and `.pytest_cache` access did not prevent these checks.

Claude's existing build/test ledger is inherited evidence under its original identities. Its successful typechecks, native component tests, mocked extension suite and artifact packaging were not rerun or relabeled as this review's qualification. This review did **not** run Rust formatting/Clippy/workspace tests, Docker, installed-editor journeys, browser journeys, actual-client campaigns, macOS/Linux batches, signing workflows, or hosted CI. No cross-platform, visual, release-publication or complete parity claim follows from the diagnostics above.

## Recommended repair order

1. **Protect targets and authority:** R01–R04, R11, R19. Capture deterministic stale-selection, stale-response and delayed-ack cases together; preserve exact scope/revision checks and fail closed.
2. **Complete lifecycle and installation recovery:** R05–R10 and R24. Review the whole state flow before coding; reproduce publication interruption, retained-data reinstall, version admission and platform uninstall ownership at cheap seams. Include the installer-exclusion question in that review.
3. **Repair packaging evidence and gate selection:** R12–R13 and R25–R26. Exercise current-version packaging with fixture binaries first; restore moved platform test identities and consumed-input coverage. Do not discover the next missing input through successive expensive builds.
4. **Finish interface parity/recovery:** R14–R18, R20–R21 and R27, with shared native cases and complete actual-transport comparisons where results are reshaped. Land any needed shared contract separately before consumers.
5. **Implement the admin usability journey in bounded steps:** begin with named scope/search and index setup/detail, then consistent management/status presentation and accessibility. Keep behavior changes separate from structural extraction and retain complete responses.

For each authorized repair, freeze and review the complete correction before one ordered T0/T1 batch. Only then use the necessary artifact/installed gate. Preserve failed evidence, avoid broad reruns for frontend-only corrections, and stop when the specific acceptance criteria are met. The 1.8.34 version bump is already present; do not repeat it for this review.
