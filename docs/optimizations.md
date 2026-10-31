# Startup and tool performance optimization plan

Status: proposed work; no runtime optimization in this document has been implemented or qualified. Investigation date: October 30, 2026, America/Denver (October 31 UTC). Source baseline: `44a95e93068527387400972d210673b309a1d6b0`, version **1.7.6**. Indexed navigation was checked against live source and Git history. This plan covers installed startup, existing-index availability, MCP requests and the editor work that can amplify their costs.

## Findings and limits

The reproduced failure is a deadline mismatch: the unified `status` wrapper allows two seconds for the whole backend call, including authenticated installation readiness, registration resolution, brokerage and the index observation. The installed service and published index can be healthy while that preparation exceeds the deadline. `diagnostics` shares preparation but does not have this two-second wrapper. The native scalar status implementation is substantially cheaper once connected.

This is not evidence that every reported timeout has the same cause. Source review also found two-second registry waits, two-second optional usage-lock waits, and status waiting behind another evidence exchange. These can independently consume the same deadline. They need isolated measurements rather than attribution from the error code alone.

### Measured installed Windows observations

These are individual samples from the installed release, not percentiles or a controlled comparison of release binaries. A fresh session means a new MCP frontend, not an emptied OS filesystem cache. The scripts sent ordinary MCP initialize/tool requests through the configured launcher; they did not change settings, rebuild indexes or warm the first status with diagnostics.

| Sequence | Wall time | Result |
|---|---:|---|
| Session A: initialize through installed launcher | 2,243 ms | Initialized |
| A: first `status` | 2,014 ms | `status_timeout`, unknown readiness and null evidence |
| A: next `status` | 1,234 ms | Published index available |
| A: three immediate status reads | 4 / 3 / 3 ms | Successful cached observations |
| Session B: initialize | 2,234 ms | Initialized |
| B: first `diagnostics` | 2,149 ms | Published index available |
| B: following `status` | 143 ms | Available, uncached |
| B: status after 26 seconds, expiring cache and direct reuse | 143 ms | Available, uncached |

Session A took about 5.5 seconds from the initialization request to a confirmed status, including a failed first check. Session B demonstrates why the larger diagnostic read can succeed while the advertised quick check times out. These measurements do not isolate the time of every preparation stage or reproduce the other machine's storage, antivirus, CPU load or corpus.

Read-only installed-payload measurements found 674 manifest files totaling **248,244,233 bytes**, a **176,640,512-byte** main executable, a **55,317,504-byte** helper and three role copies of the main executable. Hashing only the manifest files with Python SHA-256 took **244.3 and 247.7 ms** with warm filesystem caches. This excludes native path checks, pair verification, role hashes, topology traversal and locking; it is not a native verification benchmark.

### What changed

| Change | Evidence and implication |
|---|---|
| 1.4.4 implementation, `3c64111` | `installed_control` already verified selection and then called `ensure_running`, which verified it again. Installation-lock and receipt costs predate the latest releases. |
| 1.5.12 implementation, `d3b80dd` | MCP pre-initialization payload verification and the universal launcher's identical-binary shortcut add other verification boundaries. |
| 1.6.4 consolidated changes, `da0d540` | Introduced the two-second unified status deadline and split scalar status from detailed diagnostics. Existing startup work became visible as `status_timeout`. |
| Follow-ups `3264011` and `4c41337` | Retained one session-owned installation-verification task across timeouts and made smoke fixtures handle timeout/admission/recovery. They did not remove repeated hashing. |
| 1.7.2, `c62c1e6` | Workbench adds another evidence client and can recreate it for each new transcript. Session persistence and full-state rendering add editor-host work. These can repeat or amplify startup costs. |
| 1.7.3 and 1.7.6 | Parser/dependency hardening, setup/privacy work and Windows descriptor ACL setup changed. The central status deadline, verification and vector-worker paths were not replaced. Profile these additions separately before blaming them for steady-state reads. |

The inspected local 1.7.2 VSIX contains a 176,263,680-byte main and a 55,316,992-byte helper. The installed 1.7.6 main is only 376,832 bytes larger, about **0.214%**; the helper differs by 512 bytes. This is a file-size comparison, not an independent provenance verification or an old-runtime benchmark. There is no evidence here of a dramatic binary-size increase between those versions. Release LTO/stripping settings, MiniLM weights and the selected DiskANN engine did not change in the reviewed range. The 852,116-byte intent scorer added in 1.7.2 is embedded in the binary and validated/used lazily by prediction, not status.

### Four verification passes on an ordinary unmanaged launch

The current source-bound route can perform these complete verification passes before the first native tool completes:

1. Universal shortcut or delegated stable launcher: `selected` before executing the engine.
2. MCP construction: `root_mcp_control` -> `installed_metadata_control` -> `selected`, before initialize is answered.
3. First tool: `RuntimeReadiness::ensure` -> `installed_control` -> `selected`.
4. That same readiness operation: `ensure_running` -> `selected` again, followed by authenticated service health.

Each `selected` calls `verify_runtime`: hash all manifest files, rehash main/helper for the executable-pair receipt, traverse the complete topology and hash each present role copy. For the measured installation, that is approximately:

```text
248,244,233 manifest bytes
+ 176,640,512 main + 55,317,504 helper, checked again for the pair
+ 3 x 176,640,512 distinct role-file bytes
= 1,010,123,785 logical hash bytes per full pass
x 4 passes = 4,040,495,140 bytes, about 3.76 GiB
```

This is source-derived logical SHA-256 input, not measured physical disk reads or an instrumented count of every installed launch. It excludes launcher self/equality hashes, extra main-image hashes and filesystem metadata work. Managed pins and repair/compatibility routes must be measured separately.

Sources: [universal routing](../crates/airk-cli/src/universal_launcher.rs), [stable launch](../crates/airk-cli/src/runtime_install_commands.rs), [frontend resolution](../crates/airk-cli/src/service_frontend.rs), [installer verification/startup](../crates/airk-cli/src/runtime_install.rs), [pair verification](../crates/airk-cli/src/runtime_pair.rs), [topology verification](../crates/airk-cli/src/runtime_managed.rs), [role files](../crates/airk-cli/src/process_roles.rs), [unified readiness](../crates/airk-cli/src/unified_mcp.rs).

## Outcomes and measurement contract

The first outcome is fast access to an **existing authenticated publication**. Opening Explorer must not make a saved index wait for fresh source reconciliation, semantic completion, every workspace project, agent-file migration, shell setup or full diagnostics. First installation, upgrades, incompatible formats, pending recovery and unregistered roots remain distinct paths with real prerequisites.

The second outcome is an interface that promptly shows what is available. Render the shell immediately; show saved observations with their age while confirming the connection; enable reads only after the selected scope/publication is authenticated. Separate connection progress, published text availability, source updates and semantic readiness. A service health response or a remembered count alone must never become a green ready state.

Add opt-in, content-free monotonic timing at the outer launch/extension boundary before optimizing. Current usage spans start after admission and snapshot selection, and recording finishes after elapsed time is calculated; they cannot explain end-to-end latency on their own. Capture these stages independently:

- launcher process creation/routing, full verification pass count, hash calls and bytes, path/topology checks;
- installation-lock wait and hold, lifecycle settlement, health verification and unnecessary receipt writes;
- extension activation, discovery, registration lookup, launch-spec publication and first UI render;
- registry opens/pages/busy waits, brokerage, direct authentication, worker startup and inner session creation;
- cache hit/miss, backend-mutex wait, native admission, snapshot selection, scalar status and serialization;
- usage open/resource sampling/baseline/record waits, completed native submissions after waiter cancellation;
- first lexical result, first usable semantic result, UI confirmation, background polls and Workbench saves.

Keep reports path-free and query/source-free. Report physical I/O as unknown unless measured by the OS; logical bytes hashed are not disk traffic. Record build identity, CPU/core limits, memory, storage class, filesystem-cache assumptions, corpus counts, active projects and whether the service was already running. Count native submissions separately from client retries. Do not add diagnostic probes to ordinary status to collect these measurements.

Proposed local acceptance targets below are engineering goals, not current guarantees. Validate against comparable before/after builds on the same fixtures; adjust documented targets only with recorded evidence.

| Scenario | Target |
|---|---|
| Healthy service, existing published project, fresh editor/MCP frontend | First authenticated text read within 2 seconds at p95 on the reference four-core host, measured from trusted extension activation or MCP client process spawn, including launch/handshake; report stage times separately |
| Warm uncached scalar status | p95 below 250 ms on the reference host; no normal-workload deadline failures |
| Valid fenced status cache hit | p95 below 25 ms; no whole-registry scan or payload hash |
| New session when the service is stopped | Separately bounded service-start budget and explicit progress; measure startup before selecting a percentile target |
| Editor activation | Shell and truthful startup state within 250 ms after view resolution; no waiting for native diagnostics to render |
| Transcript reset / idle UI | Same-project reset creates no new MCP frontend; hidden views create no periodic detailed native probes |

Use at least 30 fresh-session samples for comparative percentiles in explicit local qualification; do not represent the two investigation sessions as that campaign. Use deterministic hash/RPC/submission counters and injected waits for routine correctness gates rather than fragile broad wall-clock tests.

## Prioritized implementation work

### P0: make costs and first-response regressions observable

**O1. Add the stage measurements above and a reproducible installed-startup benchmark.** Extend the existing [managed client qualification](../scripts/check-service-clients.py), [installed lifecycle checks](../scripts/check-service-install.py) and focused native MCP fixtures. Keep one tiny original corpus and one representative bounded code/document corpus. Test source-bound and pinned frontends, a running/stopped service, zero/one/many projects, lexical-only and semantic-enabled indexes, and one versus simultaneous Codex/Claude/Explorer-style clients. Do not execute an older binary against the current live registry; compare isolated installations with compatible fixtures and verify their exact artifacts.

**O2. Separate first-call success from eventual recovery.** Preserve the existing timeout -> typed busy -> recovery correctness cases, but add first-agent-visible-response measurements without a diagnostics warm-up. [The current helper](../packages/vscode/src/test/nativeReadRetry.ts) permits repeated unknown observations for up to 60 seconds. That is useful recovery coverage but conceals the bad initial experience if used as the only performance gate. Record timeouts and retries as failures of the first-response target even when recovery succeeds.

### P1: remove duplicated startup work without weakening verification

**O3. Verify selection once within an authorized readiness operation.** Refactor installer-owned `installed_control`/`ensure_running` to consume one verified-selection object containing approval, manifest identity, verified component digests, topology and control binding. Preserve lock ordering, pending lifecycle/activation/repair/uninstall checks, managed policy and exact-instance/version/payload health. Recheck selection after a lock wait or any boundary that can invalidate the proof. This removes the immediate double verification; it does not authorize skipping separate execution trust boundaries.

**O4. Reuse computed digests within that verification.** Executable-pair validation can compare against the main/helper digests already authenticated from the manifest; launcher equality can use the verified launcher digests rather than opening both binaries again. Carry verified main identity into role selection/health in the same operation. Every distinct role file must still be independently authenticated; equal expected hashes are not proof that different files have equal bytes. Retain no-follow, owner/ACL, complete-topology, byte-budget and unknown-file checks.

**O5. Keep hashing and filesystem verification off the async protocol executor.** `RuntimeReadiness` currently spawns an async task that calls synchronous verification; the CLI runs two async worker threads. Move blocking work into bounded blocking execution with shared ownership and cancellation checkpoints. A `spawn_blocking` task cannot simply be aborted once running: session disposal must bound/drain its resource ownership and stop any later service-start action. This improves responsiveness; report byte reductions separately from scheduling gains.

**O6. Reduce installation-lock contention and unnecessary writes.** `ensure_running` holds the installation lock over expensive verification and health/start work, with up to 60 seconds of lock waiting. First remove duplicate work inside the lock. Then evaluate verification outside the critical section only with a safe immutable proof and a final locked selection/election recheck. Concurrent clients must still elect one service. In unmanaged `start`, a healthy service older than 60 seconds currently causes the default startup-attempt receipt to be rewritten and synced on every new readiness check. Reset only a nondefault failure/backoff record; preserve the circuit breaker and durable failure history.

**O7. Reuse a verified launch proof across process boundaries only after a separate trust design.** The remaining launcher, MCP-construction and first-request passes are real opportunities. A future handoff must be authenticated, bounded, bound to executable/manifest/install/control/user identity and protected against mutation/replacement, replay and inherited-descriptor misuse. Stable verified handles or an authenticated receipt protocol need platform-specific proof. A bare environment flag, path/mtime cache, persisted `verified=true`, or healthy broker is insufficient. Start with same-operation reuse; do not make cross-process proof reuse a prerequisite for the first correction.

**O8. Reduce Windows launcher overhead after measuring it.** Reuse equality digests now. Evaluate a guarded refresh of the retained universal launcher and its compatible direct route to remove an unnecessary forwarding hop. Preserve unknown-launcher delegation, repair routing, Job Objects, EOF/owner cleanup and active clients. Do not kill launchers or merge independent agent streams to reduce a process count. See [process ownership](process-model.md).

Acceptance for O3-O8: deterministic verification counts/bytes; tampering of main, helper, each role, pair receipt, manifest and launcher; same-size replacement; selection change during lock wait; managed external/user-on-demand policies; repair/activation/uninstall states; two simultaneous clients; session disposal; no repeated write of an already-default startup receipt. Preserve the [installation contract](service-installation.md).

### P1: restore a dependable status and request path

**O9. Define separate setup and observation budgets.** Retain one session-owned setup task. Recommended contract: give cold authenticated setup its own bounded budget, provisionally 10 seconds, and give an established status observation the existing two seconds. Validate the smaller latency targets above after removing redundant work; a larger budget is not the optimization. This deliberately changes the current two-second total-call promise and requires synchronized schemas/help/docs/client tests. If the product must keep two seconds total, return a typed, promptly delivered setup-pending observation with stage/retry delay and null evidence while the same bounded task progresses; do not call pending setup an unavailable index. Keep real setup errors observable and never elect the service merely for catalog/help discovery.

**O10. Coalesce pending status observations rather than abandon and restart them.** Cancellation currently marks a partially exchanged direct stream unhealthy, correctly, while admitted native work continues. Retain at most one owned status/readiness observation per exact scope and reuse its fully received completion for waiting callers. Share the authenticated logical observation and render per caller, or explicitly key sharing on response options; preserve client-local omission receipts, runtime/work state and native submission accounting. Fence scope to registration/registry incarnation and the worker/project identity required by the observation; reject delayed old-scope completions. Bound task lifetime and refresh its age/publication rules. Never reuse a half-read stream or replay a submitted evidence query. Closing the session must stop admission and drain/cancel owned work according to its actual execution phase. Preserve received status receipts only after a complete response.

**O11. Replace per-request registry enumeration and duplicate status opens.** `RootScope::resolve` scans 32-row pages of full registrations up to 1,024 projects on every source-bound call. Status then opens the registry again in `status_scope` before its cache check. Add a bounded exact canonical-root/optional-collection lookup returning registered/missing/removed/ambiguous plus registry incarnation/revision. Use a revision-fenced lookup cache or an indexed source key if measurements justify a registry migration. Share the verified observation with status-cache fencing. `open_saved` has a two-second SQLite busy timeout: use remaining-budget-aware read admission and typed unsubmitted contention replies instead of accumulating cancelled blocking lookups. Preserve removal, late registration, ambiguity, replacement and root isolation.

**O12. Keep cached status out of the long evidence-exchange mutex.** `Backend::call` holds its state mutex through brokerage and the complete worker exchange; a concurrent slow search can prevent even a valid cache hit. Separate immutable cached observation metadata from the serialized connection owner. Fence a cache hit to the verified project/registry scope and disclose its age. An uncached observation must still obey native admission and publication gates; do not solve this by raising concurrency or pinning a request snapshot indefinitely.

**O13. Bound optional usage accounting so it cannot consume readiness latency.** Usage-store opening and recording each take lifecycle file locks with a two-second contention budget, and recording/pruning/quota work runs before the tool reply returns. Profile open, resources, baseline and record separately. Give request-side best-effort accounting one small total admission/optional-work budget, provisionally 10 ms; count dropped/unknown observations honestly. This is not a hard cancellation deadline around an accepted SQLite commit or filesystem sync: started writes must finish or settle under owned work before releasing lifecycle protection. Measure tail latency explicitly. If necessary, evaluate a bounded worker-owned queue with explicit overflow, drain, policy-change and clear/delete semantics. Preserve event-ID deduplication, root/collection scope, opt-out, exact-once accounting of admitted work and truthful loss reporting. Never silently turn recording off to make status pass.

Sources: [unified deadline](../crates/airk-mcp/src/unified.rs), [direct client/cache](../crates/airk-cli/src/project_client.rs), [worker admission](../crates/airk-cli/src/project_worker.rs), [registry reads](../crates/airk-store/src/service_registry.rs), [usage collection](../crates/airk-kernel/src/usage.rs), [usage locks/ledger](../crates/airk-store/src/usage.rs).

Acceptance for O9-O13: cold first check without diagnostics, timeout -> busy -> recovery, multiple waiters/representations, held registry/usage locks, slow evidence plus cached/uncached status, no native replay, receipt and observation-age fencing, publication changes, cache hits racing project removal/replacement/invalidation, cancelled callers and bounded cleanup. Run usage-enabled/disabled controls in isolated fixtures; do not deliberately stall the user's live usage ledger.

### P1: existing indexes available before editor setup finishes

This workstream includes native availability and the UI's connection path; a faster spinner alone does not satisfy it. Keep startup branches explicit: healthy existing installation, stopped service with an existing publication, upgrade/recovery, first installation and first indexing.

**O14. Publish a usable selected-project connection as soon as authenticated discovery permits.** The completed-install path currently awaits `discover` -> `reconnect` -> `prepareWorkspaceProjects` -> `refresh` before publishing launch specs and any first-use Workbench auto-reveal. Discovery itself serially awaits bundled-file verification, self-test, location, runtime inspect and runtime diagnose. Workspace preparation enumerates all active/inactive registrations, reconciles every open folder, registers missing projects and migrates agent connections. These are code-confirmed barriers even when the selected project already has a saved index.

Split mandatory authenticated discovery from optional diagnostic/lifecycle inspection for a completed, compatible installation. Existing completed-install receipts and coalesced discovery already exist; measure their hit rate and the work they still await before adding another cache. Resolve the selected eligible root authoritatively, publish its validated binding/spec and allow evidence connection while bounded reconciliation of other roots and agent configuration continues. Preserve atomic publication of each coherent spec snapshot and fence background results by workspace generation, registry revision, install/control identity and runtime selection. Saved editor definitions are candidates, not authority over native removal or ambiguity. Selected-project reads must not wait for terminal integration, global cleanup, model preparation or every management RPC. Trust, native-execution opt-out, explicit stop, update/version compatibility and pending lifecycle/recovery remain gates.

Sources: `automaticSetup`/`prepareWorkspaceProjects`/`refreshOnce` in [extension startup](../packages/vscode/src/serviceExtension.ts), `discoverOnce`/`verifyBundledRuntime` in [runtime discovery](../packages/vscode/src/serviceBootstrap.ts), and [saved definitions/registry pagination](../packages/vscode/src/serviceWorkspace.ts). Workbench currently loads recent transcripts before starting its first status; run connection readiness independently of history loading.

**O15. Activate a safe retained reader before ancillary worker initialization.** `project_worker::run` sends `Ready` early but withholds `Activated` until storage/collection opening, rebuild/job receipt recovery, watcher-root recording, semantic reconciliation, recovery/retention checks, broker registration and local MCP setup finish. `reconcile_semantic_setup` performs full service inspection/status, which can read imported analysis. The supervisor waits for `Activated` before granting a connection. Thus a saved publication has a real native startup barrier beyond the UI.

Define the minimal reader-activation boundary: exact worker/storage/root ownership, current collection/head/journal safety, required durable receipt reconciliation, direct endpoint and query admission. Move watcher metadata refresh, semantic reconciliation and nonessential retention/inspection work after that boundary as separately admitted existing service work. Determine and test which recovery checks must remain before activation; do not move all recovery behind reads. Retained lexical/fact reads must not require ONNX loading, completed semantic work or a fresh source scan. Disclose freshness/semantic state independently. Prioritize the selected project's query headroom under existing resource policy; never eagerly open all collections. Preserve refusal during unsafe Applying/recovery states.

Sources: [worker activation](../crates/airk-cli/src/project_worker.rs), [supervisor grant ordering](../crates/airk-cli/src/project_supervisor.rs), [full service inspection](../crates/airk-kernel/src/service_inspection.rs). Prove the change with actual retained `list_files`/`read_file` and lexical reads before optional worker initialization completes.

**O16. Publish verified health before the full Management observation finishes.** `ServiceConsoleController.observe` sets `connected=true` immediately after valid health, then serially fetches resources, model/source status, logs, request epoch, projects, workers, events and an optional plan. `refresh` publishes only after the entire sequence. Explorer Search is enabled from this console connection state, so a slow optional probe can keep it disabled after health succeeds.

Publish verified health promptly, with independent pending/cached/failed section observations and ages. Let authenticated selected-project evidence requests proceed without awaiting the full console poll or a redundant preflight status; the request itself still observes native scope/publication/admission checks. Use a fresh scalar or direct evidence reply to confirm actual saved-index availability, never service health alone. Fence late partial results by exact instance/project and extension generation. A slow logs/model/events probe must not hold connection feedback or permitted text reads hostage.

Resolve/show the view before awaiting native work. Display a remembered selected project and last observation only as saved information with age, then replace it with fresh authenticated evidence. Show semantic preparation and source refresh separately, respecting saved search policy and explicit vector-only requests. Use short product messages such as “Connecting to AIRK”, “Saved index available”, “Updating saved files” and a specific retryable failure. Show elapsed progress and a usable retry/details action if startup exceeds the target; reserve error styling for actual failures. A timeout must not erase previously observed evidence or fabricate current readiness. Do not expose hashing/registry implementation details in the normal flow. Sources: [console publication](../packages/vscode/src/serviceConsoleController.ts), [connection mapping](../packages/vscode/src/explorerControl.ts), [Search admission UI](../packages/vscode/src/explorerWebview/search.tsx).

Use the same truthful state model in Explorer, Workbench and the status bar, while preserving each view's chosen scope. A displayed remembered state is never authorization for a read. A later fresh scope/version failure must disable affected actions and preserve the reason. An explicit first-use setup review must not appear on an ordinary healthy reopen.

**O17. Remove supervisor startup head-of-line blocking and demand-start the helper.** Worker spawn and activation each synchronously wait up to ten seconds on the supervisor control thread. A cold project can delay other projects' grants/control work. The same supervisor starts the embedding pool whenever a nonderived project worker exists, including a worker requested only for status/lexical reads; helper verification/readiness can wait up to ten seconds too. This proves eager helper-process startup, not eager loading of ONNX model sessions.

Use bounded per-project startup states and asynchronous completion inside the existing supervisor/ownership model, preserving one worker owner, grants, budgets, drain and failure reporting. Keep health and already-running project grants responsive while another project starts. Start the shared helper when semantic/model work actually needs it and publish helper readiness/failure separately. Do not lengthen authorization grants, add a second scheduler or silently change hybrid/vector policy. Sources: [worker supervision](../crates/airk-cli/src/project_supervisor.rs), [helper startup](../crates/airk-cli/src/embedding_pool_process.rs).

Measure idle reopen churn too: the worker retires after 15 seconds with no active connections/work, query connections have a 30-second idle read timeout, and frontend direct reuse expires at 25 seconds. Compare reopen after 15/30/60 seconds, including memory and process counts. Evaluate bounded residency for the selected workspace only within existing quotas/ownership when the measurements justify it; do not permanently pin workers, lengthen grants or poll solely to prevent retirement. Helper startup follows an admitted worker and can block later control/grant work; it need not delay that worker's first grant.

Acceptance for O14-O16/O17: installed-window reopen with a populated text index; editor window opened while source/semantic jobs run; service stopped/running; retired project worker; helper unavailable; offline operation; multiple roots with one slow/new/broken root; selected project switch during setup; hidden/open Explorer; delayed optional shell/agent/model/logs/events tasks; large Workbench history; trust/native-execution disabled; extension replacement requiring reload; genuine recovery failure. Measure activation-to-shell, binding-to-readability, first lexical result and fresh UI confirmation separately. Verify actual reads without diagnostics, source indexing or semantic preparation as warm-up. A running second project's reads must remain prompt while another worker/helper starts; unsafe Applying still refuses reads.

### P2: remove repeated tool-layer work

**O18. Reuse authenticated metadata within one request.** `status_summary` and readiness helpers reread/decode collection state. Pass one authenticated state through the helpers or memoize bounded immutable metadata inside the same `CurrentRead`. General rendering also opens another reader when adding missing readiness; carry readiness from the operation's original request head where possible. Preserve expiry and `unavailable_after_read` semantics. Never retain an idle `CurrentRead`, SQLite transaction or request gate between calls. Sources: [scalar/readiness](../crates/airk-kernel/src/current/runtime.rs), [semantic readiness](../crates/airk-kernel/src/current/enrichment.rs), [discovery readiness](../crates/airk-kernel/src/current/progress.rs), [current reads](../crates/airk-store/src/current/read.rs), [rendering](../crates/airk-mcp/src/lib.rs).

**O19. Reduce JSON copying and repeated catalog construction.** Rendering converts results to a logical value, clones that value for display, serializes text and serializes the enclosing reply for byte accounting. Profile large facts/changes/full-chunk/parse results; reuse immutable values or serialized buffers where exact accounting permits. Cache immutable catalog/schema/alias lookup tables rather than rebuilding/sorting/parsing them per dispatch. Preserve byte budgets, citations, source hashes, revision tokens, validators, omission memory and alias behavior. Test byte-for-byte contracts where ordering is contractual; do not reduce evidence to improve timing.

**O20. Measure direct dispatch and diagnostic fan-out.** Direct connection renewal constructs an internal duplex MCP transport and SDK handshake in `LocalTools::session_with_client`. Benchmark it separately before replacing it with a shared dispatcher using the same schemas, handlers and admission. Keep per-client attribution, runtime/work state and response receipts independent. In explicit diagnostics, collect independent runtime/work observation groups concurrently while retaining exact worker/service identity, individual timestamps and partial failures. Do not add those probes to status. Sources: [local sessions](../crates/airk-mcp/src/local.rs), [runtime probes](../crates/airk-cli/src/runtime_metadata.rs).

Acceptance for O18-O20: same-head metadata-read/proof counters, warm/fresh integrity equivalence, unchanged full/terse validators and byte bounds, publication races, client-local diagnostic isolation and exact usage-byte accounting. Keep lower-impact changes separate from the initial startup correction.

### P2: stop editor behavior from repeating expensive paths

**O21. Reuse Workbench transport when only the transcript changes.** `newSession` calls `select`, which disposes the evidence client and issues a cold status even for the same project. Retain the client for same-project New/Clear operations; keep transcript generations and late-result rejection separate from transport lifetime. Reconnect on real launch-spec/project/trust/native-execution changes or transport failure. Do not merge Workbench with an unrelated agent's stdio stream. Source: [Workbench view](../packages/vscode/src/workbenchView.ts).

**O22. Avoid rereading and parsing session envelopes twice per autosave.** `SessionStore.save` scans all sessions for pruning; `WorkbenchView.save` then calls `list`, scanning them again. Ten retained 4 MiB sessions can mean about 40 MiB parsed per scan; a directory may temporarily contain up to 100 accepted entries before the inventory bound, so the ten-session figure is not a worst-case adversarial bound. Return summaries from save, maintain revision-validated bounded summary metadata, coalesce saves and avoid full clones/serialization when only state labels change. Preserve atomic writes, exclusive writer/revision checks, invalid-file handling, result-retention preferences and bounded inventories. Source: [session storage](../packages/vscode/src/workbenchSessions.ts).

**O23. Send smaller Workbench updates.** Current state building regenerates evidence cards/continuations for all turns and posts the whole session even for “Saving”/“Auto-saved” changes. Memoize immutable turn projections, send small state deltas and page long transcripts with explicit omitted-history behavior. Test a near-limit ten-session/200-turn fixture for extension-host stalls, parse count, posted bytes, allocations and memory. Keep copy/export/citation behavior unchanged.

**O24. Make ordinary observations cheap and visibility-aware.** Explorer currently invokes full `airk_diagnostics` automatically at least every five seconds while retained/open; the console separately polls health/resources/embedding/source/logs/request-epoch/projects/workers/events every two seconds after completion. Active-editor changes can request a full console refresh even while management is hidden. Use scalar status/shared cached project inspection for ordinary Overview, details on demand or a justified failure transition, visibility/activity-aware cadences, debouncing and shared in-flight metadata reads. These older paths are amplifiers, not proven new 1.7.6 regressions. `request_epoch` can prune receipts: move acquisition toward actual mutation/receipt work; do not relabel it read-only or relax its retry rules. Sources: [Explorer](../packages/vscode/src/explorerController.ts), [console lifecycle](../packages/vscode/src/serviceConsole.ts), [console observation](../packages/vscode/src/serviceConsoleController.ts), [extension events](../packages/vscode/src/serviceExtension.ts).

**O25. Remove synchronous Windows descriptor setup from the extension host.** `restrictWindowsDirectory` runs `whoami.exe`/`icacls.exe` synchronously on first use of each directory. Use asynchronous fixed commands or the owning native ACL adapter; share account lookup and concurrent setup. Publish no descriptor/credential until restrictive ACL setup succeeds. This affects editor startup; it does not explain a standalone native scalar read by itself. Source: [private directories](../packages/vscode/src/privateDirectory.ts).

**O26. Profile new ignore/extraction work only where it runs.** Credential-default discovery adds matcher construction and a per-entry mutex; excluded credential changes can still request watcher reconciliation. Measure matching cost and unnecessary reconciles, then consider shared matcher construction or watcher/scanner policy alignment with correct user whitelist and ignore-file invalidation. Recent ZIP/XML safety changes and the eligibility-policy bump can affect extraction or a one-time coverage refresh; compare unchanged scans, changed-file extraction and policy migration separately. Preserve credential exclusions, archive/XML budgets, skip/failure reporting and source safety. Do not infer that these paths run on every status request. Sources: [scanner](../crates/airk-ingest/src/lib.rs), [scope matching](../crates/airk-ingest/src/source_scope.rs), [watcher](../crates/airk-kernel/src/watcher.rs), [1.7.6 behavior](releases/release-1.7.6.md).

Acceptance for O21-O26: no new MCP process on same-project transcript reset; distinct transcript identities and late-result fencing; bounded save/message work; no hidden-view detailed polling; accurate failed-job/publication transitions; log actions responsive during stuck control reads; descriptor setup failure; credential whitelist and ignore-file changes. Headless MCP and editor latency must be reported separately.

## Execution order and review gates

1. **Baseline/instrumentation:** O1-O2, including the Explorer startup dependency trace. Record results before changing behavior. A release-history comparison must use isolated, authenticated artifacts; the current samples establish a defect, not a quantified regression against all prior versions.
2. **First correction:** O3-O6, O9-O13 and the minimal selected-project/UI path in O14-O16. Prefer same-operation reuse and bounded accounting over a new cross-process trust protocol. Review the combined change against managed installation, source/pinned bindings, worker admission, receipts, removal and recovery before one coherent push.
3. **Startup and amplifier removal:** reader activation/supervision in O15/O17, O21, O24-O25, plus remaining measured gating dependencies from O14. Confirm actual existing-index reads improve, including multi-root reopen. Land native activation changes as a coherent separately reviewed unit if their recovery audit is larger than the first correction.
4. **Measured follow-ups:** O18-O20 and O22-O23. O7-O8/O26 proceed when measurements justify their additional trust, lifecycle or migration cost.
5. **Qualification/release:** compare cold/warm results on native Windows, Linux and macOS, including a constrained four-core workload and simultaneous clients. Run the required formatting, workspace Clippy/tests, repository, license/package and whitespace checks from [CONTRIBUTING](../CONTRIBUTING.md), plus `scripts/ci-local.ps1` for native changes on Windows. Use focused local regressions first; preserve routine CI selection and provenance gates. Full qualification is opt-in and must be reported separately. Before a feature/fix PR, remind the authorized human to increment the patch/build number once and synchronize native/extension lockfiles; do not bump again if already done for that PR.

Require a separate complete-diff and validation review before a corrective push. Report passed, failed, unrun and unknown results explicitly; successful local Windows samples do not qualify other platforms. Update current README/index, MCP/installation/architecture/extension guidance and the new release record when implementation changes land. Keep the historical release evidence identifiable. This planning change does not change the 1.7.6 version or claim new behavior.

## Non-negotiable boundaries

All work stays in the existing local CLI/MCP/kernel/store/editor owners and the dependency direction in [architecture](architecture.md). No agent orchestration, external database/cloud provider or replacement of DiskANN is proposed. Source roots stay read-only; collection/root isolation, current revision identity, original citations, validators, budgets, failure reporting and lifecycle ownership remain intact.

Do not hide the issue by increasing all timeouts, disabling usage/integrity checks, returning remembered counts as current, widening query admission, keeping request gates forever, eagerly loading models, batching INT8 inputs without an embedding-identity/quality study, deleting process copies, or weakening storage/descriptor permissions. The existing [semantic optimization record](semantic-index-optimization.md), [indexing optimization ledger](indexingenhancements.md) and [storage reliability review](storage-reliability.md) cover separate work; their historical proposals are not evidence that those optimizations are still missing.

## Evidence still needed

- Stage timings and lock/usage contention on the reported second machine; the investigation did not access its files or retained state.
- Exact published 1.6.4/1.7.2/1.7.3/1.7.6 before/after runtime benchmarks and a controlled editor reopen capture. Only current installed requests and local artifact sizes were measured here.
- CPU, allocation, page-fault and OS I/O profiles under cold filesystem cache, antivirus scanning and modest hardware.
- Measured elapsed contributions of the confirmed extension, worker-activation, helper-startup and console-publication gates; their ordering is verified, but their individual latency is not yet profiled.
- Native-platform evidence for any new proof reuse, dispatcher, cache, ACL or startup-state contract. None is qualified by this plan alone.
