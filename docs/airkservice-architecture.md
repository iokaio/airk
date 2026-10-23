# Proposed AIRK local service architecture and implementation plan

Status: opt-in native service control, durable jobs, supervised project workers, direct retrieval/MCP brokerage and executable split implemented; publication redesign, resource qualification and extension cutover remain in progress. Written October 22, 2026.

Target release: **1.4.0, a clean break from the 1.3.x process, storage and client lifecycle.** The service architecture, clean break and release number are settled. The maintainer permits breaking minor releases during this pre-adoption stage. Measurements guide implementation and qualify the result; they are not prerequisites for choosing this architecture. This document does not change the checked-in implementation version or authorize publication. The [current architecture](architecture.md) remains the description of implemented behavior until this plan is implemented and qualified.

Review the [chosen direction and remaining decisions](#11-chosen-direction-and-remaining-decisions), [publication contract](#6-publication-and-concurrent-reads), [installation lifecycle](#9-installation-and-lifecycle-without-administrator-access), [service console](#11-vs-code-service-console), and [implementation sequence](#15-implementation-sequence-and-exit-criteria). Platform details stay in this plan until implementation establishes contracts suitable for separate operating guides.

Implemented foundation: [core service contracts](../crates/airk-core/src/service.rs), [kernel broker/admission reference policy](../crates/airk-kernel/src/service_broker.rs), and [CLI IPC framing](../crates/airk-cli/src/service_ipc.rs). The policy bounds workers, credentials, connections, aggregate query concurrency and reserved response bytes; binds single-use grants to caller/worker/project identities; and retains running-work charges through expiry/disconnect/service loss. The codec bounds serialized JSON and rejects oversized/truncated frames. The [native protocol](service-protocol.md) connects the policy to supervised project processes and direct retrieval endpoints. Each worker enforces its allowance locally; no per-query service RPC or evidence forwarding is introduced. Existing collection operation remains active until the service cutover.

The [native control loop](../crates/airk-cli/src/service_runtime.rs) runs through `airk serve --runtime-dir <private-directory>`, with `airk service health` and `airk service shutdown` selecting that same directory. These operations do not open a project database. [Transport adapters](../crates/airk-cli/src/local_ipc.rs) use explicit user-only pipe/directory ACLs and peer token checks on Windows, and protected directories plus peer UIDs for Unix sockets. The service holds a process-lifetime instance lock, bounds control connections and frame deadlines, rejects incompatible protocols and isolates malformed/disappearing clients. [Registry persistence](../crates/airk-store/src/service_registry.rs) separately provides bounded project registration, revision-bound pages, pause state and monotonic fencing under project leases. Its 32 MiB database ceiling and SQLite rollback allowance bound service metadata independently of collection quotas; it never opens source/index files. An ownership receipt permits recovery of interrupted empty initialization and rejects unknown ledgers. The [durable job ledger](service-jobs.md) now provides atomic idempotent acceptance, bounded receipts, pending-work joins, later-change isolation, pause, fenced completion and conservative restart recovery. The native management endpoint now accepts registration, revision-bound project pages, request epochs, enqueue/inspect, pause and queued cancellation through a 32-slot metadata actor. Canonical roots and conflicting storage bindings are checked before acceptance. Health remains independent of registry locks or failures. The [project supervisor](../crates/airk-cli/src/project_supervisor.rs) starts bounded [lock-bearing workers](../crates/airk-cli/src/project_worker.rs) for accepted jobs or retrieval brokerage. Worker-owned query admission and [direct clients](../crates/airk-cli/src/project_client.rs) bypass the service payload path. The [shared embedding runtime](service-embedding.md) now reuses verified compatible inference across project workers through a separate direct endpoint. Replacement rebuild execution, publication redesign and resource-policy qualification remain pending.

Shared-pool validation on October 22: the Windows offline two-project native smoke passed supervised MiniLM indexing, exactly one compatible model load, hybrid MCP citations, helper-failure lexical availability, independent service health and unchanged original bytes. Three native engine tests cover compatibility, foreign model paths, full pipeline identity and interleaved distinct vectors. These are development-pair receipts; Linux/Mac pooled inference and installed-platform acceptance remain outstanding.

Supervision/brokerage validation on October 22: Windows and non-root Debian 12 x64 passed native job execution, direct retrieval with original citations, single-use grant replay rejection, actual stdio MCP initialize/list/call, management-tool rejection, service-loss admission shutdown and OS lock release. The final Linux run passed five native process regressions, seven adapter tests and eight service-store tests. Additional Windows and Linux regressions cover a foreign registry with independent health, predecessor locks that block global reallocation, and a late child that cannot open collection state before service activation. These opt-in workers still use the current publication and execution adapters; they do not establish service-wide pooled inference, fair-share resource scheduling, replacement rebuilds or installed-platform acceptance.

Management validation on October 22: the extended native process test passed on Windows and non-root Debian 12 x64, including registry contention without health blocking, duplicate receipts across restart, storage conflicts and unchanged source bytes. Linux also passed seven adapter and eight service-store tests. All-feature workspace Clippy passed. `airk service rpc --runtime-dir <directory> --request-file <json>` exposes the bounded management protocol for development clients.

Durable-queue validation on October 22: eight Windows service-store tests passed, including queue-capacity receipts, request-epoch expiry, stale completion and recovery that preserves current running work. All-feature workspace Clippy passed.

Control/registry validation on October 22: Windows passed seven lightweight adapter tests, the real racing-launcher/restart/control-failure regression, and four service-store tests. The [non-root Linux fixture](../scripts/service-runtime.Dockerfile) passed the corresponding seven adapter tests, process regression and four store tests on Debian 12 x64. The fixture runs with `docker build --file scripts/service-runtime.Dockerfile --tag airk:service-local-test .`; it uses the existing public-only context and does not package or publish a VSIX. Linux debug identity testing exposed an executable just over 512 MiB; the streamed executable ceiling is now 2 GiB and debug readiness allows up to 60 seconds. These are finite development limits, not measured release startup objectives. Mac execution and installed-platform lifecycle qualification remain outstanding.

The two-executable split is implemented as an opt-in native path: [runtime builder](../scripts/build-service-runtime.py), [pair verification](../crates/airk-cli/src/runtime_pair.rs), [embedding child adapter](../crates/airk-cli/src/embedding_client.rs) and [embedding executable](../crates/airk-cli/src/embed_main.rs). It supports retained-model inference, token planning, transformer tokenization and explicit offline asset extraction with independent kernel quota admission. The builder checks that the main dependency graph excludes inference and transformer-tokenizer libraries, then stages both executables and a version/platform/digest receipt. [Runtime smoke testing](../scripts/check-service-runtime.py) uses original temporary fixtures and blocked proxy settings. The service-wide loaded-session pool is implemented with finite initial limits and direct worker/helper traffic. Default extension cutover, global model-file installation, resource-policy qualification and VSIX packaging remain pending. Development builds remain at 1.3.2 until synchronized release preparation; 1.4.0 remains the approved target.

Executable-split validation on October 22: the native Windows runtime-pair smoke passed offline provisioning, external-model indexing, hybrid retrieval and unchanged source bytes. Lightweight IPC/pair-verification tests passed five tests; native retrieval tests passed 39 with three existing model-qualification skips; transformer tokenizer tests passed four; default provisioning regressions passed two. All-feature workspace and lightweight-only Clippy passed with warnings denied; Python contracts passed 128 tests with four skips. These are local development receipts, not final service or installed-platform acceptance.

Initial Windows foundation validation on October 22, 2026: locked workspace build, all-target Clippy with warnings denied, formatting, repository/link and whitespace checks passed on Windows. The workspace test run, including doctests, passed; that run began before the final content-free status and per-caller admission follow-ups, which passed the final seven broker tests, with four IPC codec tests also passing. Python contracts ran 128 tests with four skips, and license notices were generated without adding dependencies. Existing ignored native qualification tests remain ignored; no Mac/Linux execution, installed-service or two-executable packaging qualification is established by these results.

## 1. Decision and purpose

Replace independently launched indexing helpers with one user-owned local AIRK service per execution environment and release channel. The service owns the project registry, durable work requests, resource admission and worker lifecycle. Separate project workers own independent project indexes. VS Code windows, CLI commands and coding-agent MCP connections become clients of that service. This is an architectural choice for explicit ownership, shared resource policy and a consistent client lifecycle, independent of whether incremental changes could improve 1.3.x performance.

The service brokers authenticated, collection-scoped connections to project workers. Retrieval requests and evidence responses flow directly between clients and workers. Management, lifecycle, resource grants and status remain service responsibilities. Query traffic must not require a service forwarding hop.

Deploy two native executables per target: `airk` for clients, the service and project workers, and `airk-embed` for embedding workers and the bundled MiniLM payload. Package and upgrade them together. Section 4.4 defines the dependency and installation boundary.

The service must install and run from a platform VSIX under an ordinary user account. Administrator access, sudo, a Windows Service Control Manager registration, a system-wide launch daemon, a system-level systemd unit, Docker, a compiler, a package manager and runtime model downloads must not be prerequisites.

The architecture must accommodate many editor windows, projects and agents without multiplying indexing work or loading one model per client. A client connection is not an indexing worker. A registered project need not have a resident process when idle.

AIRK remains a local indexing and bounded retrieval kernel. It does not schedule agents, generate answers, route external model requests or require a cloud service or external database. The service coordinates AIRK's own work only.

### 1.1 Chosen direction and remaining decisions

The service architecture, fresh storage namespace, clean rebuild and explicit agent reconfiguration are settled for this plan. Implementation must resolve:

1. Exact lifecycle and protocol contracts for on-demand startup, optional user login integration and shared discovery across editor profiles.
2. Keep native, extension and lockfile versions synchronized at the approved **1.4.0** cutover. Breaking minor releases are permitted during the project's pre-adoption stage; no 2.0.0 numbering decision is pending.
3. The authenticated read/publication contract, bounded reader leases, replacement-build storage cost and revision invalidation; WAL is a candidate requiring proof.
4. Continuation of requested jobs after the last editor closes, idle shutdown, resource priorities, the supported platform matrix and measurable release objectives.

Proceed through phased implementation and validation of these contracts. A service process alone does not establish that indexing failures are fixed.

### 1.2 Alternatives considered and architectural rationale

| Alternative | Value | Tradeoff |
|---|---|---|
| Retain 1.3.x workers; improve publication and batching | Smaller lifecycle change; preserves agent configuration and current service registrations | Can improve performance but retains distributed client/worker coordination and the current lifecycle |
| Retain workers; add a shared embedding pool plus publication fixes | Reuses model sessions without replacing every client or registry | Adds IPC, ownership, crash recovery and pool admission while retaining the existing client/worker lifecycle |
| Chosen service, independent project workers and redesigned publication | One control surface, durable supervision and service-wide resource policy | Accepts a larger compatibility and integrity change, with explicit transition, storage safety and platform qualification work |

Choose the service architecture to establish a coherent long-term ownership and lifecycle model. A fresh namespace and rebuild let that model define its publication and recovery contracts without carrying legacy runtime compatibility. Incremental fixes remain useful technical inputs, but are not competing release paths in this plan. Phase 0 records the current baseline and validates storage mechanisms to guide implementation; it does not require comparative benchmarks to justify the architectural choice.

### 1.3 Terms

| Term | Meaning in this proposal |
|---|---|
| Incarnation | Identity of one rebuilt collection store; a rebuild creates a new identity and invalidates prior tokens |
| Fencing generation | Monotonic ownership epoch checked at write admission/publication so a superseded worker cannot continue writing |
| Channel | Explicit runtime/update track, such as stable or isolated preview, with its own endpoint and data namespace |
| Lease | Bounded ownership of a resource or admitted revision; expiry/disconnect/crash cleanup must release it without interrupting an atomic commit unsafely |
| Trusted host-specific locator | Owner-verified runtime selection/discovery record on the execution host, outside workspace control |
| Durable idempotency key | Persisted identifier bound to the caller, project and mutation payload; retries join the same operation, while conflicting reuse fails |
| Connection grant | Short-lived authorization to establish a retrieval-only connection to one authenticated worker for one collection; grants no resource capacity by itself |
| Resource grant | Service-issued, worker-bound allowance for finite execution capacity; workers enforce it across all their client connections |

## 2. Problems to address

The 1.3.x design has independent collection stores but coordinates processes through shared files and short scheduler/quota transactions. Four execution slots provide coarse admission. Separate helper processes can duplicate model sessions. The collection writer gate excludes competing writers during preparation; the exclusive request gate excludes readers during Applying and destructive maintenance. Prepared keeps the base logically readable. Preparation also performs and rolls back SQLite writes: rollback-journal file locking, including possible page-cache spill, is a candidate source of reader stalls, not a measured cause here. Phase 0 must distinguish writer-gate waits, request-gate waits and SQLite file-lock waits. See the [current publication and cache contract](current-index-runtime.md).

Progressive file-count targets double up to 4,096 files, while publication cadence already adapts to the preceding publication cost with count/byte/time bounds; see [current indexing](architecture.md#current-indexing-and-retrieval). Cost-aware count selection remains worth measuring. An earlier review cited a gap at 992 committed files, but no versioned diagnostic receipt for that observation is linked here, so it is not a baseline or evidence of a 1.3.2 regression. The [1.3.2 release notes](release-1.3.2.md) record the reporter's October 22 confirmation that the Windows VSIX resolved the reported two-project indexing issue. Any remaining stall needs a new reproduction with exact artifact identity, workload, phase timings and wait owners before attributing a cause.

An optional per-user scheduler service already supports `service plan/install/status/start/stop/uninstall` through Task Scheduler, systemd user units and LaunchAgents. It shares the existing queue and locks; the proposed service adds connection brokerage, supervision and a shared resource/model pool rather than inventing user-level lifecycle support. See [native scheduler installation](native-scheduler.md#explicit-per-user-installation).

Moving the same blocking publication path into a daemon would retain the central problem. The redesign must address publication, observability and worker scheduling together. The service must never depend on successfully reading a busy project database to answer a health request.

## 3. Non-negotiable requirements

- Isolate projects by canonical root, collection identity, applied configuration, storage ownership and revision identity.
- Run multiple projects concurrently when resources permit, without a fixed four-project ceiling.
- Deduplicate work from multiple windows and agents for the same project.
- Keep committed evidence searchable while preparing ordinary updates and replacement indexes.
- Keep service health, job status, cancellation and restart controls responsive during indexing.
- Install, upgrade, diagnose and uninstall under the current user's permissions, including from an offline VSIX.
- Support macOS Apple Silicon, macOS Intel, Windows x64 and Linux x64 as independently qualified native targets.
- Provide a service-wide VS Code view, including projects not open in that window.
- Support diagnosis, restart and repair of one project, a selected set, or all registered projects.
- Preserve source files, explicit failures, bounded retrieval, exact citations and local-only inference.
- Make resource limits, queue waits, incomplete work and unsupported environments visible.

"No administrator required" means no additional privileges beyond an allowed user installation and execution. It cannot guarantee execution when organizational policy prohibits AIRK, extension installation, child processes or native code. AIRK must report that boundary and support approved deployment; it must not bypass policy.

## 4. Process and ownership model

~~~mermaid
flowchart TD
    V[VS Code windows] --> C[User service control and event API]
    L[CLI client] --> C
    M[Agent stdio MCP bridges] -. broker connection .-> C
    C --> S[Durable scheduler and resource manager]
    S --> A[Project A worker]
    S --> B[Project B worker]
    S --> N[Additional project workers]
    M <-->|Direct bounded queries for selected collection| A
    M <-->|Direct bounded queries for selected collection| B
    M <-->|Direct bounded queries for selected collection| N
    L <-->|Direct retrieval| A
    V <-->|Direct retrieval| A
    A --> D1[Project A index and retained objects]
    B --> D2[Project B index and retained objects]
    N --> D3[Independent project stores]
    A --> E[Bounded embedding worker pool]
    B --> E
    N --> E
~~~

Solid client-to-worker links carry retrieval requests/responses; the dotted bridge-to-service link performs connection setup. Each grant selects one collection and worker; the multiple links illustrate separate collection-bound clients. CLI and editor retrieval follow the same direct path. Service-to-worker links carry ownership, resource grants and supervision, not forwarded evidence.

### 4.1 Service identity

The singleton scope is the OS user, host/runtime namespace and channel. Stable and explicitly isolated preview channels use different endpoints and data roots. Different VS Code profiles and editor distributions on the same host should discover the same compatible stable service rather than each creating a service inside its extension storage directory.

A remote SSH host, WSL distribution, development container or container namespace is a separate execution environment. There is no claim of a single allocator spanning a Windows host, WSL, containers and remote machines. Multiple storage volumes within one environment are managed by the same service and resource manager.

An owner-only instance lock elects one service. Starting from many clients simultaneously must produce one winner; other launchers connect or return a bounded startup failure. A PID alone is not identity: discovery includes instance ID, process start identity, executable identity, protocol range and endpoint ownership. Never remove a lock simply because a status call timed out.

For unmanaged installations, the verified selected runtime owns the stable channel. Different editor payload versions negotiate with that runtime; activation never replaces it merely to match an editor. Compatible clients connect within the tested range. Incompatible clients report the mismatch and offer a deliberate verified upgrade with drain/restart; they cannot downgrade, create a second stable instance or open its store directly. Concurrent upgrade requests serialize under the install lock and recheck selection. Unsupported older clients must update or use an explicitly isolated preview channel. Managed installations retain their approved pin.

### 4.2 Responsibilities

| Component | Responsibility | Must not do |
|---|---|---|
| Local service | Registry, client authentication, connection brokerage, durable requests, subscriptions, admission, worker supervision, maintenance coordination | Proxy retrieval payloads/responses, parse files or perform inference on the control loop |
| Project worker | Authenticated direct query endpoint, local admission within service grants, discovery, extraction coordination, revision publication and recovery | Modify another project, exceed its service-issued grants or create an independent resource policy |
| Embedding worker pool | Reuse verified model sessions, compute source-bound vectors under admitted budgets | Publish project revisions or choose a project's identity |
| VS Code extension | Install/connect, render state, register trusted projects, submit user actions, collect diagnostics | Run its own indexing scheduler or infer success from a spinner |
| MCP bridge | Preserve stdio MCP framing, obtain a scoped connection and exchange bounded read requests directly with its project worker | Install models, repair indexes or gain service management authority |
| CLI | Scriptable service/client operations and explicit maintenance requests | Silently fall back to opening an index as a competing writer |

Use one active project owner per collection, with concurrent bounded readers and parallel extraction inside admitted capacity. Spawn or retain workers according to activity; unload idle workers and model sessions under pressure. A worker crash should fail its in-flight requests and isolate its project while other workers continue.

The service is a single coordination dependency. Its crash interrupts control connections and invalidates admission even though query sockets connect directly to workers. Recovery must be automatic and bounded, but not described as zero downtime. On detected service ownership loss, workers stop admitting new queries and writes, cancel/drain admitted reads within their deadlines, and finish or preserve recoverable atomic publication before terminating or surrendering ownership. They cannot continue indefinitely on cached resource grants. A replacement service must establish prior ownership release before granting a new owner. Use process-lifetime handles and fencing generations, not heartbeats alone, to prevent surviving writers from overlapping new owners.

Ownership handoff must retain an OS-backed exclusive collection lock through publication and recovery. A replacement service cannot grant a new writer until the prior owner's lock is released and pending publication is classified. Check the durable fencing generation at write admission and commit under that ownership. A counter stored only in the replacement service cannot fence a surviving process with an open database. If exit or lock release cannot be established within the deadline, mark that project unavailable and keep other projects running; a timeout does not authorize concurrent ownership.

### 4.3 Existing crate boundaries

Keep lifecycle and IPC adapters in `airk-cli`, MCP adaptation in `airk-mcp`, scheduling/publication services in `airk-kernel`, and persistence, ownership locks and database adapters in `airk-store`. Extraction remains in `airk-ingest`; DiskANN algorithms and local inference remain in `airk-retrieval`; platform-neutral contracts belong in `airk-core`. Add modules in the owning crate before considering another crate. The extension calls native contracts rather than bypassing them.

The `serve` entry point and OS process/IPC supervision adapters belong in `airk-cli`; worker ownership policy, fencing and pool admission belong in `airk-kernel`. Pool processes load sessions through `airk-retrieval` and cannot publish. Reuse and adapt the existing lifecycle code in [kernel service adapters](../crates/airk-kernel/src/service.rs), moving OS adapter code toward the CLI boundary as needed. A new crate requires an independent responsibility and a reviewed dependency direction; the service name alone is not one.

The current dependency graph does not yet provide the executable split: `airk-kernel` depends on `airk-retrieval`, which includes inference and tokenizer dependencies. Implement optional inference modules/dependencies and an IPC embedding adapter within the owning crates so the main runtime retains DiskANN and retrieval without linking the inference implementation. Keep embedding contracts in core and pool admission in kernel. Review the final dependency direction against [architecture boundaries](architecture.md#dependency-boundaries) during implementation. Do not add a crate merely to obtain a second executable.

### 4.4 Two-executable deployment

Use the same separation on Windows x64, macOS arm64, macOS x64 and Linux x64; Windows payload filenames carry the `.exe` suffix.

| Executable | Process roles and payload | Boundary |
|---|---|---|
| `airk` | Stable launcher/CLI, stdio MCP bridge, service and project-worker modes; discovery, extraction, lexical retrieval and DiskANN | No embedded MiniLM weights or transformer inference/tokenizer implementation; submits embedding work over authenticated IPC; local byte-BPE usage counting remains available |
| `airk-embed` | Admitted embedding-worker modes, local inference dependencies and embedded pinned MiniLM assets | Loads/reuses model sessions and returns source/model-bound vectors; cannot publish project revisions or provide service management |

Multiple processes may run each executable. This is two executable payloads, not two total processes or one model session per project. Keep any required libraries, model manifests and legal notices in the verified package inventory. A stable launcher installation may use a managed copy of `airk`; it does not introduce a third executable implementation.

The primary benefit is keeping inference dependencies and initialization out of clients, the broker and project workers. Embedded read-only asset bytes do not imply a full private RAM copy in every process. Loaded model sessions, execution plans, buffers and inference threads are separate costs controlled by the shared pool. Measure private/proportionally shared memory and cold/warm startup on each target; executable size or summed resident-set counts alone do not establish the saving.

Place both binaries in one immutable version/digest runtime directory. The payload manifest binds their versions, target architecture, digests, model identity, dependencies and internal protocol compatibility. Verify both before selecting that runtime; launch `airk-embed` only by its verified absolute path from the selected payload. Reject missing, modified or incompatible helpers with a structured semantic-unavailable outcome. Preserve valid lexical retrieval with explicit semantic coverage/failure reporting; do not silently download a replacement or initialize inference inside `airk`.

Move the existing embedded-asset build/provisioning path to `airk-embed`. Explicit trusted setup may request bounded asset extraction/provisioning from that helper; kernel/store validation, ownership and quota-reserved installation remain authoritative. Ordinary MCP reads never install models. Preserve custom models and offline first use. Embedding requests carry project/source/model identities and admitted budgets as already required for pool isolation.

Build the lightweight and embedding artifacts with controlled feature sets, using separate build invocations where necessary to prevent Cargo feature unification from enabling inference in `airk`. Inspect the resolved dependency graphs and final artifacts to establish that only `airk-embed` contains the weights and inference implementation. Sign both executable payloads where required, including both Mac architectures, and qualify their actual VSIX extraction, helper launch, upgrade and uninstall paths. Stage and switch the pair together; never overwrite a running executable or mix helpers from different runtime directories.

## 5. Shared and independent state

| State/resource | Scope | Reason |
|---|---|---|
| Project database, facts, lexical records, graph and retained objects | One collection | Independent failure, publication and repair |
| Project work owner and commit serialization | One collection | Prevent duplicate or conflicting writers |
| Project registry and durable job metadata | One service | Join duplicate requests, recover work, enumerate all projects |
| CPU, memory and disk-I/O admission | One service environment | Allocate the same finite hardware fairly |
| Model files and compatible loaded sessions | Shared pool, partitioned by exact model/runtime identity | Reuse loaded sessions and bound memory/inference concurrency; the default model is already bundled since 1.3.1 |
| Usage ledger and estimation/report services | One collection, owned by its project worker | Preserve collection/root binding, existing opt-in policy and one observation per outer request |
| Quota accounting | Per storage root/volume, with per-project allowances | Prevent concurrent reservations from overspending available disk |
| Event routing and service health | One service | Observe work without opening the affected index |
| Client capabilities and request budgets | One connection/request | Separate retrieval from maintenance and bound abusive workloads |

Shared scheduler and quota mutations must be short. No extraction, inference, corpus traversal or user callback may run while their transaction/lock is held. Large project work must not accumulate inside the registry database. Disk reservations remain tied to actual storage volumes even though CPU/memory admission is service-wide.

Same-user workers are fault isolation, not a security sandbox against malicious code running as that user. In-memory embedding requests and results must carry project, source hash, model identity and cancellation identity so pooling cannot mix results between collections.

[Local usage reporting](usage-reporting.md) remains store-backed and collection-scoped through kernel services. The project worker records one observation per outer retrieval request using bridge-supplied framing metadata validated under the reporting contract; connection brokerage and internal worker/pool calls add no duplicate observations. Collection report retrieval uses the direct worker connection. Service-wide status may aggregate bounded content-free counters but does not create a global content ledger or forward source/query payloads; recording failures and partial estimates retain their existing disclosures.

## 6. Publication and concurrent reads

### 6.1 Proposed normal update path

1. Capture immutable source inputs and perform extraction outside the database writer transaction.
2. Prepare bounded fact, lexical and vector changes against a named base revision. Include policy/model identity and source hashes.
3. Reserve memory and disk growth, including journal/WAL and temporary objects.
4. Admit a single project writer. Recheck the base and reconcile or retry stale preparation.
5. Apply a bounded transaction and publish its coherent head. Never mix lexical, graph and source bindings from different revisions.
6. Emit a committed revision event and release completed work grants.
7. Reclaim unreferenced objects only after no live request/recovery record needs them.

Evaluate SQLite WAL and separate bounded read connections in an isolated Phase 0 storage spike, alongside service-skeleton work and before integrating the new publication path. WAL is not admissible in the current store: [database validation](../crates/airk-store/src/current/database.rs) explicitly rejects WAL/SHM sidecars, and [component verification](../crates/airk-store/src/blobs.rs) rejects unlisted overlays. Current physical lexical verification and guarded cache witnesses rely on rollback-journal main-file bytes; [quota admission](storage-quotas.md) reserves database growth and rollback capacity. Enabling WAL is an integrity and accounting redesign, not a PRAGMA change.

The spike must specify authentication of the SQLite snapshot actually read, including committed WAL content rather than only main-file bytes; cache invalidation/change witnesses across writer commits and checkpoints; head/digest coherence across lexical, graph and source rows; owner-verified sidecars and recovery; and reservations for WAL growth, checkpoint overlap, SHM and failed cleanup. Bound reader age, checkpoint backlog and retained objects together. Demonstrate that tampered/stale sidecars cannot overlay authenticated evidence and that crashes cannot release capacity still needed for recovery. If these properties cannot be proven within budgets, retain rollback journaling or evaluate another publication design before selecting a format.

Readers should observe a committed snapshot while a writer advances the database; writes still serialize per database. Long readers can prevent checkpoint completion. These are explicit engineering requirements, not a guarantee that WAL removes every busy outcome. See [SQLite WAL](https://sqlite.org/wal.html) and [snapshot isolation](https://sqlite.org/isolation.html).

Retain DiskANN as the vector engine and first prototype coherent graph/lexical updates in the project database. A separate vector directory is not inherently a concurrency solution; splitting storage requires a proven atomic manifest and crash protocol. Evaluate it only if measurements justify that added complexity.

### 6.2 Required contract redesign

The current implementation prepares and seals changes, rolls back, then repeats application from captured inputs under its durable journal contract. Removing that repetition or moving preparation outside exclusion changes the integrity and recovery design. Version 1.4.0 must specify a new authenticated publication contract and fault-test it; this is not a lock-removal patch.

The target is to let readers finish a bounded materialization against their admitted committed revision while updates proceed. Hold any required snapshot only through bounded request execution, not through a long-lived MCP session or cursor lifetime. Continue to expire tokens/cursors explicitly when their contract no longer permits use. This does not introduce historical search or indefinitely retained revisions.

Retained-object reclamation must protect active reads, captured work, pending journals and replacement builds. A database snapshot alone does not protect external object files. Define request leases and ownership accounting together, including crash cleanup and finite lease lifetimes.

Admit the revision snapshot and its external-object lease atomically with respect to publication and reclamation. Lease expiry requests cancellation; reclaim only after the reader releases its handles or its process exit is established. A timer alone cannot prove that an object is unused. Bound stalled readers through cancellation and, if necessary, termination of their owning worker, reporting the affected requests. Persist recovery ownership separately so a service restart cannot discard objects still required by a journal.

A request admitted before a commit may finish against its leased revision. A later request must select the new head, and a supplied token or continuation for the superseded head expires explicitly. No cursor pins an old snapshot between calls. Each response, including conditional validation, binds all evidence to its admitted revision and original locations.

### 6.3 Full rebuild and repair publication

Build a replacement index in a quota-reserved sibling directory while queries use the last verified current index. Validate replacement coherence and source/configuration identity before switching an owned manifest to the new incarnation. On Windows, do not depend on renaming or deleting a database directory with open handles: create a new uniquely named incarnation, switch the manifest, drain old readers, then retire it.

Account for both copies. When there is insufficient space, offer an explicit offline rebuild with its downtime and affected collections shown. Never delete the last readable index as an implicit fallback. A corrupt index is not served merely to claim availability; unaffected verified collections remain usable.

The manifest switch, reader lease handoff, interrupted switch recovery and cleanup must each have crash-injection tests. Rebuilding changes incarnation and invalidates old tokens explicitly.

## 7. Scheduling and resource policy

Replace the four execution-slot files with a service-wide admission model. Track extraction threads, inference threads, resident model memory, captured/prepared bytes, pending result bytes, query concurrency, disk reservations and background write pressure. Include inference-library internal threads in CPU accounting so multiple admitted sessions do not each claim the whole machine.

Expose user-level Balanced, Responsive and Throughput policies plus explicit CPU/thread, memory and background-I/O overrides. Their numeric defaults require measurement before release. Detect affinity/container limits where supported; treat missing resource observations conservatively and expose their absence. Observed memory use and OS pressure should reduce new admission; portable byte accounting is not an OS-enforced memory limit.

Use fair scheduling across projects and request classes. Interactive retrieval receives reserved admission capacity, while aging prevents permanent background starvation. A client cannot monopolize the service by opening more connections or raising its own priority. User-initiated indexing may receive a temporary boost, but other projects retain a minimum share. Paused or inaccessible projects must release unused grants.

The service allocates finite query concurrency, memory and inference allowances to workers over the control channel. Each worker admits direct requests locally within its current grants, enforcing per-client and per-collection limits across connections. Idle allowances return to the service; grants expire or are revoked through an acknowledged drain that preserves accounting for work still running. Never reassign capacity solely because its timer expired while its prior consumer is unaccounted for. Requests queue within finite limits or receive a structured busy/retry outcome. Resource replenishment carries bounded counters and grant identities, not query text, vectors or evidence, and does not require a service RPC for every query. Query embedding calls use the shared pool directly under an admitted allowance.

Allow expensive work in parallel across projects and within a project, but keep one project publication writer. Apply backpressure to bounded capture, extraction, embedding and publication queues. Parallel extraction must stop producing when the publication queue or memory budget fills.

Choose publication batches using actual prepared bytes, retained memory, recent publication duration and cancellation responsiveness, with hard count/byte ceilings. Grow only when measured cost permits; shrink after slow commits. Batch targets are not deadlines: a storage operation can still overrun, so phase timing and stuck-worker recovery remain necessary.

Coalesce repeated file events, join identical requests and reconcile at a durable high-water mark. New changes during a scan form later work; do not continually restart the scan. Watcher overflow requests a full reconciliation. Periodic reconciliation remains necessary for missed events and when no editor is open. Use one native watcher owner per registered project rather than one watcher per client; inactive-project watching is configurable and bounded by host limits.

## 8. Protocol, clients and security

### 8.1 Transport and discovery

Use an owner-restricted Windows named pipe or Unix-domain socket, with bounded framed messages. Do not expose a listening TCP port by default. Discovery is an owner-protected record under a user runtime directory, not a file in the workspace. It describes the authenticated instance, protocol and runtime endpoint; it contains no query/source content.

The discoverable service endpoint supports control and connection brokerage. Each worker has a separate owner-restricted retrieval endpoint, returned only through authenticated brokerage. Apply the same peer/endpoint ownership checks to both hops. A workspace path, stale endpoint name or PID is never sufficient to select or authenticate a worker.

Authenticate peer user identity using supported OS mechanisms, enforce endpoint permissions, and use an owner-only bootstrap credential where needed. Check endpoint ownership and symlink/reparse handling before connecting. Credentials must not appear in command arguments, workspace files or diagnostic exports. Authentication does not isolate AIRK from fully compromised same-user processes.

On Windows set an explicit pipe ACL for the intended user, reject remote pipe clients, and validate peer/owner identities; do not rely on the default pipe descriptor. Microsoft documents that default pipe permissions are broader than this design requires. See [named-pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights).

### 8.2 Contracts

Define protocol version negotiation, feature capabilities, maximum frame size, deadlines, cancellation, request IDs, structured errors and per-client outstanding-request limits. Service version, protocol version and index format version are separate identities. A compatible older 1.4-family client may connect only within the explicitly tested negotiated range; no 1.3 helper compatibility is implied.

Proposed service method groups, not existing commands:

- `service`: identity, health, capabilities, resource status, diagnostics, drain, restart intent.
- `projects`: paged list, trusted registration, status, settings, unregister without deleting sources.
- `jobs`: enqueue, join, inspect, cancel, pause/resume, and bounded event subscription.
- `connections`: obtain a retrieval-only worker connection grant for an already registered, authorized collection; no query payload or evidence response passes through this method.
- `maintenance`: plan, validate, execute and inspect verify/recover/rebuild operations.

Project-worker endpoints expose `queries` with the existing collection/revision-bound retrieval semantics, including collection-scoped usage reports. They expose no service management methods. Direct worker and service-control protocols negotiate capabilities separately within the tested runtime family.

Control/status execution uses bounded independent handlers and a small state snapshot maintained by events. It never waits behind query work or a project's exclusive writer. Authoritative on-disk verification is a separate job; status must identify cached observations and their age.

Slow subscribers receive a gap marker and resynchronize from a snapshot rather than consuming unlimited memory. Sequence events by service instance and monotonic sequence number. Persist job transitions and failures; coalesce high-frequency counters rather than journaling every file. Reconnection is retryable, but a client must not automatically replay an ambiguous mutating request without its durable idempotency key.

Persist the idempotency binding and initial job transition in one transaction before acknowledging acceptance. The same key and payload return the same job/receipt across reconnects and service restarts; conflicting payloads fail. Define bounded receipt retention and an explicit expired-key outcome, so a late retry cannot silently repeat maintenance. Semantic request joining is separate: compatible requests from different clients may share a job while keeping their own receipts. Canceling a client's wait does not cancel that job; explicit job cancellation requires control authority and is visible to every joined client.

### 8.3 MCP and privilege separation

Provide a stable user-installed `airk` launcher and a collection-bound stdio MCP bridge. The bridge can request on-demand service startup and a connection grant for its configured registered collection, then exchanges retrieval traffic directly with the worker. Brokerage can start an idle worker under service admission, but cannot register a new project, install a model or initiate indexing implicitly. The bridge receives retrieval capabilities only and exposes no restart, deletion, repair, project enumeration or registration tools to agents. Client disconnect cancels that client's requests, not a shared indexing job.

Management requires a separately authenticated control connection and explicit trusted client context. Workspace configuration cannot choose the service executable, endpoint, credentials or another project's identity. Paths supplied by clients are not proof of ownership; resolve registered canonical roots, aliases and allowed operations inside the service.

### 8.4 Brokered connection and reconnect contract

1. The launcher verifies the selected service and negotiates the broker protocol. The service authenticates the caller and resolves its requested root/storage/collection binding against trusted registrations.
2. The service locates or starts the single owned worker, establishes its resource allowance and returns a short-lived connection grant plus its authenticated endpoint and supported query protocol. Bind the grant to the service instance, worker instance/fencing generation, collection/root binding, caller connection identity, retrieval capabilities and expiry. Transfer credentials only over protected IPC; exclude them from logs, command arguments and diagnostic exports.
3. The bridge verifies the endpoint and worker identity; the worker independently verifies the peer, grant and current service ownership. Redeem a grant once during connection establishment, rejecting replay, expiry, wrong collection and superseded identities. Bind the established connection to that authorization; subsequent requests cannot select another collection.
4. The bridge sends requests directly to the worker. The worker validates request/response budgets, revision selectors and cancellation, executes under its service-issued resource allowance, and sends bounded evidence directly back. Each connection has finite outstanding requests and output buffering; a slow client cannot consume the worker's entire response budget or block service status.
5. Connection-grant expiry bounds setup, not an admitted request's revision lease. Established connections have a separately bounded authorization lifetime and close/drain on revocation, worker replacement or service ownership loss. Worker unload waits for active requests to drain or cancel; an idle bridge may reconnect on its next request without keeping a model session resident.
6. A broken connection fails pending requests explicitly. The bridge returns to the broker with capped backoff and obtains a fresh worker-bound grant; it never guesses a replacement endpoint or falls back to direct store access or service query forwarding. Read-only calls may retry within their original deadline, retaining any supplied revision/token. An interrupted read returns no partial success; discard incomplete output, and expire stale selectors rather than silently substituting a different revision.

Connection grants authorize access; they are distinct from worker resource allowances and per-request revision leases. Direct IPC provides collection fault isolation and avoids central payload forwarding, while the existing same-user security limitation still applies. A service restart requires fresh brokerage; this design does not promise uninterrupted queries through loss of service ownership.

## 9. Installation and lifecycle without administrator access

All platform adapters share these policy boundaries: remain within current-user authority, verify executable and endpoint ownership, preserve OS protections, and report denied installation/execution/persistence with an approved deployment path. Do not elevate, alter security policy or enable persistence merely to make a check pass. The platform sections add specific constraints to this common rule.

### 9.1 Common installation transaction

The platform VSIX contains the verified `airk` and `airk-embed` runtime pair, compatible launcher/MCP bridge modes, embedded model assets, legal notices and a complete payload manifest as specified in section 4.4. No runtime download is required. Additive network-based delivery may be considered later; it cannot be the only installation route.

On first activation eligible under Workspace Trust:

1. Locate the user-level AIRK installation using a trusted host-specific locator. User settings may select an organization-approved install location; workspace settings may not.
2. Detect the actual execution host OS/architecture and check the platform payload and minimum runtime requirements.
3. Acquire a user install lock; verify manifest hashes and release authenticity; stage in a new version/digest directory. A checksum detects corruption but is not publisher authentication by itself.
4. Verify ownership, safe extraction paths, executable permissions, dependencies and a bounded native self-test. Refuse redirected/unsafe install targets and never modify OS security policy.
5. Discover or elect the service and negotiate capabilities. Atomically select the verified runtime only after startup health succeeds.
6. Connect the extension, register the trusted project, and subscribe to service/project events.
7. Report the exact runtime, architecture, lifecycle mode, storage roots and installation result.

Repeat installation and activation must be idempotent under multiple windows. Failed staging leaves the prior verified runtime selected. No unsigned script fetched from the network is executed. Do not add a PATH entry, login registration or shell-profile modification as a prerequisite to using the extension.

The existing **Install or Repair CLI** capability remains available as an optional terminal setup path, including its user PATH/shell-profile changes; service operation uses the verified absolute launcher path. Current automatic CLI setup is documented in the [extension guide](vscode-extension.md#cli-installation-and-path); this proposal makes that setup optional for the service lifecycle.

Release authenticity in step 3 requires a trusted distribution or organization-approved payload identity in addition to manifest hashes. [Current Linux downloads are unsigned](../README.md#try-the-native-binary); this proposal does not treat their checksums as publisher signatures. Before release, define an offline-verifiable signed manifest and trust-root/rotation policy, or require a documented organization-approved payload digest delivered through a trusted channel. Report the verification basis and reject an install when its configured authenticity policy cannot be met. Signing/notarization requirements below are proposed release gates, not descriptions of every current package.

The service must live outside the versioned VS Code extension directory so extension replacement/uninstallation cannot delete a running executable. Keep runtime payloads immutable. A small stable launcher resolves the selected runtime; coordinate launcher replacement with Windows file-lock constraints. The CLI can use an absolute launcher path when PATH changes are prohibited.

### 9.2 Lifecycle modes

| Mode | Installation privilege | Startup and lifetime |
|---|---|---|
| On-demand user service, required baseline | Ordinary user | Extension/CLI/MCP launcher elects the service; persists across client reloads where permitted; exits after an idle grace period when no clients or admitted jobs remain |
| Per-user login integration, optional | Ordinary user where policy permits | Platform user startup manager starts/restarts AIRK within that user's session |
| Organization-managed deployment | Managed by organization | Extension connects to an approved runtime and reports policy-controlled lifecycle actions |

On-demand mode must not require persistence registration. Closing a window does not stop another client's work. Define whether requested background jobs continue without clients, expose that preference and drain safely at session shutdown. Restart backoff and a crash-loop circuit breaker apply in every mode.

Reuse the existing platform-manager adapters and ownership receipts where their contracts fit; replace their scheduler-worker command with the new service entry point only through an explicit transition. Existing installations keep their old-root registrations until the user reviews a plan to stop/uninstall those exact owned registrations and optionally install the new one. Do not silently overwrite definitions, enable login startup or adopt old running work. Test refusal of modified/foreign registrations and report any old worker still running. See [existing lifecycle behavior](native-scheduler.md#explicit-per-user-installation) and section 13.

### 9.3 Windows x64

Use a signed native x64 runtime on supported Windows versions. Proposed user install location: `%LOCALAPPDATA%\AIRK\runtime\<version>-<digest>`. Keep configuration and state under user-owned AIRK directories, with large indexes on a separately configurable local volume. Do not place active SQLite state on a roaming profile or network share by default.

The baseline "service" is a hidden background process running as the interactive user, not an SCM service. Launch without a console window or shell, release extension-parent lifetime dependencies, and make the elected service own project/embedding worker process groups. Use Windows Job Objects where supported to contain worker lifetimes and account for inherited jobs, nested-job constraints and host policies in installed tests. Do not promise survival of logoff or process policies that terminate descendants.

Use an explicit per-user named pipe identity and ACL. Include session context where required; across multiple logon sessions, connect only when owner and session-access policy permit. If policy requires separate session instances, show that reduced coordination scope rather than pretending admission is machine-wide.

An optional Start at sign-in integration can use a current-user startup registration if allowed. It must be separately enabled and removable, and is not required for indexing. Do not depend on scheduled tasks, service-account credentials, the Log on as a service right, HKLM writes or UAC elevation.

A traditional Windows service is an optional enterprise deployment adapter, not the extension's default installer. Creating an SCM service requires the corresponding SCM access rights, normally held by administrators. See [Microsoft's service access model](https://learn.microsoft.com/en-us/windows/win32/services/service-security-and-access-rights). A system-account service handling all users is outside this proposal; a managed adapter must preserve per-user authorization and storage isolation.

Upgrade by staging a new directory, draining writes, switching the trusted runtime selection and restarting. Never overwrite an executing `.exe` or loaded DLL. Defer old-runtime deletion until processes exit. Diagnose Defender/EDR blocks, missing dependencies, denied pipe access and read-only installation locations without requesting exclusion rules automatically.

### 9.4 macOS Apple Silicon

Build and qualify `aarch64-apple-darwin` / `darwin-arm64` natively, including model runtime dependencies. Prefer a native arm64 VS Code and native AIRK payload. An x64 extension host running under Rosetta must not silently select or download another architecture: detect and explain the mismatch, and offer the matching approved package/native-editor path. Universal packaging is a future option only if both slices and dependencies are verified.

Proposed runtime location: `~/Library/Application Support/AIRK/runtime/<version>-<digest>`. Put logs in `~/Library/Logs/AIRK`; keep the socket in a short, verified owner-only local runtime directory to avoid Unix socket path limits. Store large project indexes in a separately configurable local directory.

Ship Developer ID-signed runtime components and notarized distribution payloads, with stapled notarization evidence where supported by the artifact format. Validate the actual offline VSIX extraction/install path under Gatekeeper; notarizing a different wrapper is insufficient qualification. Follow [Apple's notarization guidance](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution). Never remove quarantine attributes or disable Gatekeeper to make installation pass.

### 9.5 macOS Intel

Build and qualify `x86_64-apple-darwin` / `darwin-x64` separately on supported Intel macOS versions. Use native Intel inference dependencies and measured thread/memory defaults. Do not assume Apple Silicon performance, CPU instruction availability or notarization results qualify the Intel artifact.

Use the same user directories, IPC protocol, control UI and lifecycle contracts as Apple Silicon. The documented minimum macOS version and dependency deployment target must be derived from the actual packaged binaries and release matrix. No Rosetta requirement applies to a native Intel installation.

### 9.6 macOS lifecycle and protected files

Both Mac architectures support the common on-demand mode. Optional login integration uses a user LaunchAgent under `~/Library/LaunchAgents`, owned by the current user, with absolute executable paths and bounded restart behavior. Register it in the current user's launchd domain, and expose manager state and errors in the extension. Never write `/Library/LaunchDaemons` or require root. Apple describes user agents as running for the logged-in user; they are not a promise of system-wide or logged-out operation. See [Apple's launchd guide](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).

macOS privacy permissions may differ between a VS Code child process and a launchd-launched process. Test access from the actual lifecycle mode, including protected directories and external volumes. Show which project/path category is denied; do not claim no-admin installation bypasses TCC, Full Disk Access policy or managed-device restrictions. Prefer accessible source locations and approved permission workflows. If login integration is prohibited but ordinary background execution is allowed, retain on-demand mode and report the limitation.

### 9.7 Linux x64

Build and qualify `x86_64-unknown-linux-gnu` / `linux-x64` against an explicit minimum glibc/runtime baseline. No runtime apt/dnf installation is required. Detect incompatible loaders/libraries before registration. musl/Alpine and ARM64 Linux require separate artifacts and qualification; do not imply the glibc x64 package covers them.

Use the XDG directories for configuration, data, cache and durable state. The proposed executable directory is `~/.local/lib/airk/runtime/<version>-<digest>`, with an optional user launcher under `~/.local/bin`. Use `$XDG_RUNTIME_DIR/airk` for an owner-only socket when valid. Without it, create a short private local runtime directory using safe OS primitives and verify ownership/mode; never use a predictable world-writable socket location. The [XDG specification](https://specifications.freedesktop.org/basedir/latest/) defines the distinct data/state/runtime roles.

Where a user systemd manager is available, optional integration installs a unit under the user's configuration directory and manages it with `systemctl --user`. Set working paths, bounded restart behavior and logs explicitly; use supported user resource controls only after capability detection. The [systemd user-unit search paths](https://www.freedesktop.org/software/systemd/man/latest/systemd.unit.html) include user-owned configuration locations.

Do not require systemd, a desktop session, root, polkit elevation or linger. On headless systems, containers or distributions without a usable user manager, use on-demand mode or an explicitly managed foreground `serve` process. Starting at boot without login is not guaranteed by this baseline. Report user-bus failures and whether an external supervisor owns restart responsibility.

Detect `noexec` install mounts, denied child execution, filesystem limitations, inotify exhaustion and cgroup memory/CPU restrictions. Offer an approved executable location or managed runtime. Do not remount filesystems, change kernel watch limits or disable SELinux/AppArmor. Native watcher overflow must degrade to bounded reconciliation rather than silently losing coverage.

### 9.8 Locked-down and offline environments

| Constraint | Required behavior |
|---|---|
| No admin/sudo, user native execution allowed | Full on-demand install and normal indexing from platform VSIX |
| No outbound network | Install runtime, model and legal payload entirely from VSIX; verify offline startup |
| Startup registration blocked | Use on-demand mode; show that automatic login startup is unavailable |
| User executable directory blocked | Connect to an organization-provisioned approved runtime or allow a policy-approved user executable location |
| Native execution or child processes blocked entirely | Report exact failure and deployment requirements; retain diagnostics; do not claim indexing works |
| Application allowlisting requires publisher approval | Supply signed artifacts, hashes and deployment instructions; wait for organization approval |
| Filesystem cannot support required local database/locking semantics | Reject active index placement and offer a supported local volume |
| VS Code extension installation itself prohibited | Organization must deploy/approve the extension; AIRK cannot self-install around that policy |

Use a managed-runtime setting with explicit machine/user scope, verified identity and protocol constraints. In managed mode, disable installation or restart actions forbidden by policy and state who owns the runtime. Read-only runtime directories are supported when writable state/runtime endpoints are supplied separately.

## 10. Remote VS Code and non-desktop hosts

Place the native extension backend on the workspace host. Remote SSH installs the matching runtime on that SSH host; WSL installs Linux AIRK inside that distribution; Dev Containers/Codespaces install in the workspace container. Do not index remote paths using the local desktop service. VS Code distinguishes local and remote extension hosts; see [extension host placement](https://code.visualstudio.com/api/advanced-topics/extension-host) and [remote extension guidance](https://code.visualstudio.com/api/advanced-topics/remote-extensions).

Show host/authority prominently on every service-wide action, especially Repair all and Restart service. Enumerate only projects registered in that service environment. Volumes and persistence follow the remote environment; explain when container deletion will remove indexes. Never mount a live SQLite index into independent hosts or containers as a substitute for IPC.

Preserve the [one-container deployment](../README.md#one-container-per-user-and-machine): one container per user/machine hosts independent collections selected by workspace MCP root/data-directory/collection configuration. The service and its workers run inside that container. Each `docker exec` stdio bridge discovers the broker through the same container-local runtime locator, obtains a grant for its configured collection, and connects directly to that worker's container-local endpoint. A container entrypoint may supervise foreground `serve`, or bridges may elect the on-demand instance; neither requires systemd. Qualify consistent container-user identity, permissions on both endpoints and persistent data mounts. Do not route a container bridge to the desktop host's service or create one service per workspace.

Web-only VS Code without a native workspace extension host cannot install or run this service. Show unsupported native functionality clearly. Declare Workspace Trust and virtual-workspace capabilities explicitly. An untrusted workspace may display existing service health but cannot register roots, start indexing, install workspace-selected executables or invoke repair. See [Workspace Trust](https://code.visualstudio.com/api/extension-guides/workspace-trust).

## 11. VS Code service console

### 11.1 Service overview

Add a Service view alongside the existing project dashboard. Every window connected to the same service sees the same job identities and state, with its current project highlighted. The view must include:

- Host/authority, OS/architecture, lifecycle mode, runtime/version/digest, protocol, instance uptime and connection freshness.
- Service health separately from project health; worker count, model sessions, admitted resources, observed memory, background queue depth and disk pressure.
- All registered projects, including closed workspaces, with canonical display root, storage root, current revision, coverage age, semantic coverage and last successful publication.
- Per-project phase, captured/extracted/prepared/committed counts, pending changes, last real work advance, elapsed phase time, and precise wait reason.
- Recent bounded failure history, crash-loop state, supported actions and policy-disabled actions.

Health is not progress. A responsive service with a busy worker is different from a disconnected service; a heartbeat is different from newly extracted files. Do not reset a no-progress timer merely because a worker sends liveness messages. Display unknown totals without invented percentages and never infer final completion from partial committed counts.

### 11.2 Controls

Provide command-palette and accessible view controls for Connect/reconnect, Start service, Restart service, Stop service, Open service logs, Copy diagnostics, Resource settings, Pause/resume project, Retry project, Verify index, Recover interrupted publication and Rebuild index. Include multi-select and all-project variants where applicable.

Restart project worker is distinct from Restart service. Prefer project restart for a single worker failure. A service restart shows the affected host, clients/jobs and planned drain, then uses a restart intent coordinated by the launcher or platform manager. Other windows observe the restart and reconnect with capped backoff instead of starting competing instances. Offer a bounded force-stop escalation for an unresponsive instance only after identity verification; never kill every process named `airk`.

Keep the console usable when the service is down. Local bootstrap diagnostics show installation verification, launcher errors, endpoint ownership, platform-manager state, last bounded logs and executable identity without requiring a service RPC. The webview must not receive raw credentials or execute command strings. Validate all messages in the extension host.

### 11.3 Troubleshooting report

Export a versioned bounded JSON report and readable summary containing service/client identity, platform capabilities, host scope, worker IDs/start identities, project/job states, observation ages, resource grants, lock/wait owners, phase timings, structured errors and recent transitions. Include whether each observation is live, cached, failed or unavailable.

Exclude source contents, embedding payloads, query text, environment dumps, tokens and full process command lines by default. Paths may be sensitive: show a preview and support redaction before copying/exporting. Logs rotate by size/count and remain local. Diagnostic export is explicit; there is no automatic upload.

## 12. Repair one project, selected projects or all projects

All-project operations mean all registered projects in the selected service environment and storage roots, not every repository on the machine. They do not require opening each project in VS Code, but do require valid registered identity and currently accessible source permissions. Never recursively discover arbitrary projects as part of Repair all.

### 12.1 Separate diagnosis from mutation

| Action | Purpose | Availability effect |
|---|---|---|
| Verify | Check database, bindings and retained inputs at a named revision | Bounded read/maintenance work; report incomplete verification |
| Recover | Complete or safely reject interrupted captured publication | Affected project may remain unavailable until recovery resolves |
| Reconcile | Inventory saved sources and queue changed/missing evidence | Existing verified evidence remains available with coverage disclosures |
| Rebuild vectors | Regenerate model-bound vectors from valid retained/source inputs | Lexical evidence stays available; vector coverage is explicit |
| Rebuild index | Create and validate a new project incarnation | Serve verified prior incarnation where possible; switch after validation |

"Repair" opens a plan that chooses among these operations based on verified state. It is not an unconditional wipe button. A journal with missing or corrupt captured inputs is not repaired by silently substituting changed live files. Offer an explicitly authorized fresh rebuild when captured recovery cannot be proven.

### 12.2 Planning and execution

1. Snapshot selected project identities, current incarnations, settings and observed failures into a maintenance plan.
2. Show the exact action per project, temporary disk estimate/reservation, availability impact, unavailable roots and skipped projects. For destructive/offline fallback, require explicit confirmation of the concrete plan.
3. On execution, revalidate ownership and revisions. Expired/stale plans must be refreshed rather than applied to a different incarnation.
4. Acquire project maintenance ownership, quiesce conflicting writes and preserve durable pending changes. Unrelated projects continue.
5. Execute bounded checkpoints with cancellation and structured progress. Cancellation cannot interrupt an atomic commit; it takes effect at a safe boundary.
6. Validate, switch if applicable, reconcile changes that occurred during the build, and publish a per-project receipt.

Bulk repair is a resumable job with bounded concurrency, not one transaction across all databases. Continue unaffected projects when one fails; expose succeeded, failed, canceled and skipped counts without declaring partial success complete. Survive extension reload, client disconnect and service restart using the durable plan and per-project checkpoints. Provide a finite retry policy, pause/cancel and disk-pressure backoff.

Never delete source trees, saved user settings or unknown directories. Old index retirement is owner-verified and quota-accounted. Corrupt derived state may be quarantined only within an explicit bounded allowance; diagnostics must not accumulate unbounded copies. A report should explain when failed retained inputs make a source-based rebuild necessary.

## 13. Clean break and service transition

The target release starts the service-backed family, with no optional 1.3 compatibility mode. No legacy direct-writer fallback, old scheduler participation, old index reader or in-place index conversion is required. Old helpers must not open service-owned state. Use the approved 1.4 storage namespace and synchronized 1.4.0 runtime/extension versions at cutover.

Use a new versioned storage namespace and explicit format marker. Do not imply that the existing `collections-1.2` layout or current database schema is accepted by 1.4. A new registry may accept user-confirmed roots and supported settings from a reviewed export, but every index is rebuilt from sources. Do not carry old journals, running jobs, locks, revision tokens or verification claims into the new store.

The first-run transition must:

1. Explain the clean break, new service lifecycle, rebuild cost and disk requirement.
2. Identify known old registrations/configurations without treating them as trusted executable instructions.
3. Ask the user to select trusted project roots and review settings for registration in the fresh service.
4. Configure agents to use the new stable launcher/MCP bridge, preserving unrelated configuration. Show proposed changes and report clients that still reference old versioned helpers.
5. Start bounded fresh indexing and display partial readiness accurately across projects.
6. Offer explicit removal of owner-verified old derived state after the user no longer needs 1.3. Keep sources and user configuration protected.

Separate namespaces permit an intentional fallback to the old installation without sharing live stores; that is not runtime interoperability. Old and new processes can still compete for physical resources, so detect/report known old AIRK workers and recommend stopping them before qualification. Do not kill them silently.

Within 1.4, upgrades stage immutable payloads, negotiate client capabilities, drain writers and restart deliberately. Unsupported format changes require an explicit rebuild/new incarnation; executable rollback must never open a newer incompatible store. Pin an approved runtime in managed environments rather than allowing competing windows to continually upgrade/downgrade the service.

Removing the extension does not delete project indexes. Provide separate Remove login integration, Uninstall runtime and Delete selected derived state operations. An uninstall request must account for other connected clients and service ownership; use explicit CLI cleanup when extension uninstall hooks cannot run. No source deletion is part of uninstall.

## 14. Reliability, metrics and performance qualification

Instrument control responsiveness, query queue/run time, source capture/extraction, prepared bytes, publication duration, model load/inference, watcher lag, disk admission, checkpoint time and recovery time. Report counts and timings without source/query content. Service health and real work progress have separate monotonic timestamps.

Measure cold connection brokerage separately from warm direct-query latency, including worker startup, grant redemption, resource waits and reconnects. Worker metrics feed bounded asynchronous service snapshots; the service never needs to inspect query payloads to report their timing or resource use.

Set separate control-plane deadlines, worker work-progress deadlines and shutdown grace periods. A missed worker deadline triggers diagnosis and bounded cancellation/restart, not repeated invisible retries. Preserve the committed head and captured recovery requirements. A query timeout releases its snapshot/lease; a dead client cannot pin old objects indefinitely.

Initial qualification objectives, to measure and finalize before release:

- Service health/status p95 below 250 ms on a responsive local host during sustained indexing; filesystem stalls must be reported separately.
- No collection-wide reader exclusion for the duration of source extraction or embedding.
- Interactive query p95 during background work within 2x the measured idle baseline for the declared workload, with explicit bounded admission rejection rather than unbounded queues.
- Idle connected clients add no indexing worker or model session solely because they connected.
- A worker crash or repair in one project does not prevent status or queries in another.
- After connection setup, retrieval payloads and responses bypass the service; increasing response size does not grow service payload buffers. Saturated or slow direct clients leave service health/status within its declared objective.
- No unbounded queue, WAL growth, diagnostic retention or replacement-index disk usage.

These are proposed acceptance objectives, not measured guarantees. Publish exact hardware, artifact identities, corpus characteristics, configured budgets, sample sizes, errors and tail latency. High-core machines must be tested for useful increased throughput and responsiveness; merely creating more threads is not evidence of scale.

## 15. Implementation sequence and exit criteria

Each phase produces reviewable contracts and evidence. Do not publish 1.4.0 until the complete minimum platform/installation matrix passes. All commands and UI names in this proposal remain provisional until implemented.

| Phase | Work | Required exit evidence |
|---|---|---|
| 0. Implementation contracts, baseline and storage spike | Instrument 1.3.2 writer/request/SQLite waits, preparation/replay costs and batching; prototype WAL authentication, reservations, bounded reader leases and replacement handoff in isolated state; define service protocol and lifecycle contracts | Exact artifact/workload baseline, including a new reproduction if a stall remains; bounded concurrent-reader/checkpoint and crash/tamper evidence to select storage mechanisms; written implementation contracts for the approved 1.4.0 release. Service architecture and clean break remain the chosen direction |
| 1. Service skeleton | Native serve/client adapters, user singleton, authenticated broker/control IPC, direct worker endpoints, bounded handlers, discovery and reconnect | Many concurrent launchers elect one instance; scoped grants establish direct connections; status remains responsive with a blocked synthetic worker; unsafe endpoints rejected |
| 2. Durable work and project ownership | New registry, idempotent enqueue/join, fencing, worker supervision, pause/cancel and recovery | Multi-window duplicate requests join; worker/service crashes never create two writers; unrelated projects remain usable |
| 3. Storage/publication implementation | Implement the Phase 0-selected format and authenticated commit/read contract, bounded reader leases, replacement builds and cleanup | Fault-injected commit/switch/recovery tests, exact revision evidence, bounded reader/checkpoint behavior; no long preparation gate |
| 4. Resource scheduler and embedding pool | Two-executable dependency split, fair service-to-worker grants, local query admission, bounded queues, exact model-session reuse, budgets and pressure handling | Lightweight artifact excludes weights/inference dependencies; direct connections cannot exceed aggregate allowances; grant drain/reallocation preserves running-work accounting; scale tests across hardware classes, no internal-thread oversubscription or mixed-project embedding results |
| 5. Platform installation and lifecycle | Offline VSIX with verified runtime pair, immutable install, user launcher, on-demand mode, optional platform startup adapters, managed runtime | Both executables pass target signing/authenticity policy and paired-version verification; clean non-admin offline installation/upgrade/uninstall on every supported target; denied-persistence and policy-failure tests |
| 6. Service console and maintenance | Service/project overview, subscriptions, down-service diagnostics, restart and resumable scoped/bulk repair | Installed-editor tests from two or more windows, closed-project repair, stale-plan rejection, partial-failure/cancellation receipts |
| 7. MCP/CLI cutover and clean-break setup | Stable launcher, brokered read-only bridges, direct retrieval, explicit legacy configuration review, fresh indexing | Multiple actual agent clients share one project job; evidence bypasses service; worker replacement requires fresh brokerage; old helpers cannot open new state; no legacy writer or proxy fallback |
| 8. Qualification and release | Signed/notarized target artifacts, full workload matrix, release documentation and controlled promotion | Exact artifact receipts, passing platform gates, documented limits and approved 1.4.0 release |

Phase numbers describe integration dependencies, not a requirement to serialize all development. Phase 1 can proceed alongside the Phase 0 storage spike; phases 2-4 need iterative prototypes before committing to final schema and worker counts. Storage integration waits for the authenticated publication/read contract. Installation and UI can be developed against a stable mocked protocol, but mocks cannot satisfy native lifecycle, publication or installed-platform exit criteria. Neither a missing stall reproduction nor an incremental performance improvement reopens the service or clean-break decision.

### 15.1 Required implementation contracts

Keep these contracts as versioned repository documents owned by the affected component. Each must name its invariants, concrete limits, state transitions, failure outcomes and validating tests before its implementation is treated as complete.

| Contract | Owner | Required decisions and evidence |
|---|---|---|
| Service protocol, brokerage and worker query endpoints | CLI/MCP adapters, kernel contracts | Separate control/query negotiation, authenticated worker identity, scoped single-use grants, direct framing/deadlines/backpressure, role restrictions, mixed-editor versions, revocation and reconnect |
| Worker ownership and durable jobs | Kernel with store persistence/locks | Ownership epochs and lock handoff, atomic acceptance/idempotency receipts, join/cancel semantics, restart recovery and surviving-worker tests |
| Publication, reads and reclamation | Store with kernel coordination | Authenticated snapshot/head, WAL or selected journal mode, witnesses, revision expiry, reader leases, crash/tamper tests and ownership-proven cleanup |
| Resource and quota admission | Kernel policy, store reservations, retrieval execution | Finite queue/thread/memory/disk limits, query capacity, model identity/session reuse, checkpoint pressure and failed-work grant recovery |
| Installation and transition | CLI platform adapters and extension | Offline authenticity basis, two-executable inventory/dependency separation, paired immutable runtime selection, helper protocol checks, old-registration handling, explicit agent configuration changes and installed-platform receipts |

Numeric defaults must be recorded before workload qualification, together with overload behavior and the measurement used to select them. A receipt identifies the tested source and packaged artifact; successful mocked or generated-definition tests cannot stand in for installed-runtime evidence.

### 15.2 Test matrix

| Area | Required scenarios |
|---|---|
| Native targets | Windows x64; macOS arm64; macOS x64; supported glibc Linux x64; minimum supported OS/runtime versions |
| Host environments | Desktop, SSH/headless Linux, WSL, development container; no user systemd manager; unsupported web-only host disclosure |
| Hardware | Low-memory laptop, typical workstation, high-core/high-memory workstation; fast and slower local disks; constrained VM/container |
| Client scale | 1, 4 and 10 active projects; 1, 10 and 50 concurrent agent/CLI clients; many windows on the same project; idle-client overhead |
| Corpus/workload | Small and large repositories, heterogeneous file sizes, generated-file churn, source edits during rebuild, semantic opt-out, multiple model identities |
| Failure isolation | Blocked extraction, worker crash, embedding crash, service crash with a surviving writer, corrupt project DB, unavailable source volume, one failed project in bulk repair |
| Publication | Concurrent reads/writes, exact lexical/vector/source revision coherence, stale preparation, crash at each journal/manifest boundary, long reader, checkpoint pressure |
| Resources | Memory pressure, full disk, insufficient replacement headroom, fair-share starvation tests, model-session reuse, library thread accounting |
| Installation | Standard user, no network, approved read-only runtime, startup registration denied, path spaces/non-ASCII, racing windows with different extension versions, upgrade while busy, old/modified scheduler registration, signature/manifest failure |
| Platform policy | Windows allowlisting/EDR denial; Mac quarantine/notarization/TCC; Linux noexec/SELinux/AppArmor denial; explicit actionable failure without bypass |
| Control/security | Wrong user, unsafe endpoint/redirected directory, oversized frames, slow subscribers, unauthorized MCP management, untrusted workspace, malicious project paths, ambiguous retry after crash, conflicting/expired idempotency key |
| Brokered retrieval | Wrong-worker/collection, replayed/expired grant, redirected endpoint, worker replacement, service loss during setup/query, slow direct reader, many connections exceeding one worker allowance, stale revision on retry, zero service payload forwarding |
| Executable split | Main artifact excludes weights/inference dependencies; separate-build feature sets; offline helper/model provisioning; missing/tampered/wrong-architecture or protocol-mismatched helper; lexical availability on embedding failure; paired upgrade/rollback; per-role memory/startup measurements on every native target |
| Maintenance | Single/selected/all registered projects; closed project; missing roots; stale plan; restart/resume; cancel during build; safe switch and cleanup |

Run repository-required formatting, clippy, workspace tests, extension tests, packaging/license/repository checks and whitespace checks for implementation changes. Platform-installed tests and measured load tests are additional requirements, not replacements for those checks. Keep unexecuted platform checks and unsupported architectures explicit.

Implementation progress is recorded at the start of this document. The plan itself does not authorize a version bump, commit, PR, publication or release. Each implemented phase needs its own validation evidence; a service process alone does not establish that indexing failures are fixed.
