# Native Arbiter lifecycle contract

## Actual outer delivery policy cut

The original ReadAttemptContext retains one aggregate expected-policy guard
through pipe, HTTP/SSE and native Admin chunk custody. Private combined members
defer registration to the whole reply owner; a changed binding roster cannot
replace a retained guard. Required Core delivery_bindings reconcile all actual
NN receipts, including abstention and unknown reservations, against full member
bodies. Missing or inconsistent facts refuse.

Every actual outer write/flush poll takes Store's nonblocking writer and existing
shared configuration-file lock, checks exact policy/profile/enrollment and the
original deadline/cancellation, then enters credential and native source gates.
Configure's order remains lifecycle -> Store writer -> configuration file ->
native gate. No lifecycle/status/application/Store call occurs inside a native
gate. The final raw-I/O liveness check uses only clock, cancellation and service
atomics. Pending releases every guard; resumed polls repeat them. A revoked
partial frame closes without an unguarded fallback or replay of completed work.

Actual native issue/selection-nonce custody is captured before work, separate
from public ScopeBinding hashes. At most one fixed epoch and one fixed selection
guard are retained. The final selected guard holds its original issue and live
nonce across the raw poll. An earlier fixed epoch is an additional check;
monotonic generation changes also invalidate the final fixed issue. Native nonce
revocation needs no generation bump and is checked explicitly. HTTP avoids a
duplicate native-gate acquisition when this source guard owns the actual cut.

Store uses try_lock/try_lock_shared, never a new waiting allowance. Empty validated
no-NN replies retain deadline-only custody and create no optional state. Guards
capture scalar time/cancellation and native owners, never their context/container.

## Joint ContextAction scoring

The private `ArbiterContextAction` I1 callback carries the REQUIRED original
ReadAttemptBudget, actual installed worker/read authority, original publication,
frozen whole-card roster, global ordinal and optional native partition allowance.
The service revalidates actual scope/configuration both before admission and
before delivery. Old NextRead enrollment/profile qualification cannot enable the
new task. The callback is refused by metadata relays and generic public control.

The selected profile authenticates the exact bounded Calibration asset bytes.
Core parses the new joint calibration, reciprocally binding all trained model,
external-weight and tokenizer assets, compiler, ABI and current task policy.
Retrieval scores the complete frozen roster once: only the unique global top
whole card may pass its own supported-class rule and raw-score margin. A tied,
unsupported or low-margin top abstains, without selecting a lower supported card.

A service-owned mutex ledger keys the ORIGINAL attempt globally across
partitions. Global ordinals 1..3 are one-use; aggregate NN tokens are at most2048
and each partition's captured allowance is enforced separately. Requested token
maxima are reserved before queueing. A compiled receipt records observed tokens
but unknown native completion retains the full reservation. Only actual native
completion may reconcile unused tokens; no ordinal is ever refunded. The shared
30s assessment cap cannot renew under a longer original read parent: its expired
tombstone remains until that original parent expires. Live ledger capacity is
bounded and pressure refuses new optional work.

Advice/2 retains separate reserved and optionally observed input-token counts;
Work contains only measured lower bounds. Missing compilation/inference remains
explicitly unknown. Native FFI reservations/completion continue to belong to the
existing inference owner; dropping a callback never fabricates completed work.
New policy/asset and real model/transport/resource qualification remain required.

Service owns explicit management and readiness; Core owns configuration/profile/download types, Retrieval owns the immutable compiled catalog and execution, Store owns descriptor-relative asset transactions, and the existing Supervisor/Inference owner mints resource permits and owns actual native completion. No read, status call or default-disabled startup starts a pool, creates an optional directory, or downloads bytes. No new process/factory, provider or runtime Python/curl dependency is introduced.

Provision accepts only a manifest admitted by ExecutionProfile::from_release: exact compiled qualified catalog, runtime, task, compiler, calibration, notices and complete Core transport closure. Every asset part is <=1GiB, fetched from exact official GitHub release authority with at most four approved HTTPS GitHub/CDN redirects. No credentials or source/evidence enter requests. TLS uses the pinned Mozilla roots; lengths, body/idle/total deadlines, each part SHA and complete logical SHA are enforced. HTTP errors or cancellation preserve the private original transaction and never publish a partial logical file.

Management operations run in bounded owned background work so cancellation/status remain available. Configuration uses the actual installation/catalog-bound Store CAS and current immutable index enrollment checks. No enablement silently enrolls another index. A changed configuration cancels obsolete work; stale completion cannot warm or overwrite a later choice. Switching to verified assets reserves current actual OS/service resources, starts only the existing native owner and waits for native warm/activation. Its resident old session cannot serve the new configuration; failed warming rolls back only when the exact candidate configuration is still current, using a new monotonic CAS revision. Later user choices win.

Cancellation revokes queued/warming work but does not equate dropped delivery with native completion. Removal disables a selected enabled profile, requests native release, waits on the native completion event and then removes only declared verified files under Store ownership. A deadline leaves Draining/Failed and recovery evidence, never a successful removal. Disabled configuration releases resident/warming profiles. Status compares native configured revision and active identity with the current persisted selection; resident fallback alone is not Ready.

Explicit recovery reconciles a matching unfinished receipt/configuration revision. A removal stays a removal; canceled/removed work is terminal and requires a new explicit Provision. Interrupted generation data remains private and preserved. Restart may warm only exact saved enabled/enrolled choice with a current qualified compiled profile and complete Ready receipt. Missing/incomplete/canceled assets never start a download on restart. Application and score admission retain actual WorkerIdentity/ScopedReadRef checks before dispatch/delivery; these helpers remain independent of management authority.

Cheap tests exercise Store with a real ServiceLease, streaming part/full-byte refusal, operation identity/cancellation and switch-CAS rollback. Actual qualified assets, resource refusals, native cancellation/drain, restart, release-byte acquisition and profile quality require the owning integration/platform campaigns. Empty/unqualified compiled catalogs honestly refuse provision/warm; developer exports and operator probes do not qualify a product profile.

Native Admin dispatch checks are pure and rechecked after owning waits immediately before configuration publication or background operation admission. ConfigurationStore.apply_checked invokes the supplied generic check after its actual writer lock and again before persisted configuration publication. Once an original background job is accepted, its ServiceLease, captured configuration revision and explicit cancellation own the operation; a later transport deadline is not a fictitious rollback of accepted work.

Under local-inference + test-support only, EvaluationOwner is a typed isolated-fixture constructor using the actual ServiceLease and native admission gate. It mints Kernel's opaque EvaluationCatalog from that same lease, accepts only unqualified candidate descriptions and seeds model bytes through retained read descriptors into the ordinary verified Store transaction. No I1 request, role flag or downloaded metadata attaches evaluation authority. EvaluationResult always serializes evaluation:true/release_qualified:false around the unchanged native result. The same application/scorer/management/resource/lifetime code runs; candidate evaluation never populates or bypasses the product release allowlist. Optional restore failures remain contained to Arbiter and preserve ordinary service startup.

Attaching that private owner to the fixture's ordinary control dispatch requires the exact same ServiceLease Arc and native admission gate instance supplied by the running fixture. Its native status/profile/application reasons explicitly identify private evaluation and an unqualified candidate, including ordinary transport results without EvaluationResult wrapping.

Scoped application discloses a selected profile only when Arbiter is enabled and that exact immutable index is enrolled. Disabled and unenrolled indexes return no profile; management status remains the owner-private global view.


The private ignored `service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture` runner is compiled only with local-inference + test-support tests. It requires explicit SHA-pinned private input rows, raw profile SHA pins, all four distinct model choices and qualification=None, readonly nofollow source descriptors, a fresh artifact work directory under a protected parent and an explicit lifetime. It uses actual first-start Genesis and then the ordinary service startup, sole ServiceLease, worker/catalog admission and control listener; its factory seeds verified Store assets while leaving configuration disabled. There is no product role, downloaded manifest admission, fake I4, fake Ready or automatic enablement.

Its private `airk.private-arbiter-fixture/1` ready record includes actual Core ServiceHealth (instance/PID/image/version/user/catalog clock), control root, pinned input digest and exact raw profile snapshot hashes. It is written only after an actual matching I1 Health response. Source-index creation, enrollment, selection, warm and canonical reads use ordinary native callers and retain full real outputs. The private test executable identity is recorded separately from matched client-pair pins; no receipt may claim the pair's service executable ran instead. Shutdown is observed through the ordinary owner request and actual service completion; a closed receipt additionally requires actual service lease reacquisition. A shutdown deadline writes Unknown and awaits actual completion without pretending a kill/drop is successful drain. Original assets/work/receipts remain private and preserved.

Canonical answer application additionally requires ContextNextRead in both the actual immutable-index enrollment and selected native profile. A relevance-only resident model can remain Ready in owner-private management status while answer application reports UnsupportedTask; this is not an applied answer adviser.


The second private ignored selector `emit_private_arbiter_configuration` observes the running fixture Health and Arbiter Status, verifies a SHA-pinned frozen source-root list against the canonical activated Store source census, and emits actual immutable index bindings plus a Rust-serialized Core ManagementRequest::Configure instance. It changes no index/configuration itself. Duplicate, multi-root, missing or foreign bindings refuse; the current actual configuration revision supplies the CAS. Existing native callers perform setup/apply and retain their full results. Configure output eligibility additionally records current ContextNextRead task-policy binding; relevance-only mechanics cannot be claimed as a goal arm. These private observation files are not new product DTOs or evaluation admission capabilities.

Resident native scoring reuses the exact charged reservation owned by that verified active session; it never reserves the full qualified load/score peak a second time. Ordinary work and concurrent warm still observe that same charge. Switching first attempts the independently charged new warm. If both peaks cannot fit, it pins the actual old resident's verified disk closure, requests its release, observes session completion and then awaits the last actual reservation-owner drop before one new reservation attempt. Failed/canceled warm likewise drains its actual session and last charge before an exact-CAS rollback rewarms the verified prior cached choice. A newer configuration prevents rollback. No policy limit or qualification floor is raised.

While an accepted lifecycle operation drains, Configure retains at most one replacement intent for the latest enabled, enrolled, already cached choice. Later Configure replaces it; global off, explicit Cancel and Owner teardown clear it. Settlement advances the existing lifecycle serial and admits that one cached-only continuation, with exact configuration/lease/catalog/index admission rechecked before warm. There is no automatic acquisition or retry loop: missing assets, pressure, stale admission or failed drain remain explicit failure/requested states. Older completion cannot steal a newer operation's intent. The original canceled native work must finish its actual drain before settlement starts the next warm.

The private goal template is serialized from Core ContextRequest plus the owning MCP ResponseArgs (Full and include_status=true), then deserialized through actual AnswerArgs. It preserves the independently frozen default budget while retaining the complete native answer packet; no Python request or response-policy DTO is introduced.

The same owning catalog/index enrollment admission is checked at lifecycle entry, after asynchronous reservation/readiness waits, immediately before native warm and before Ready delivery, including exact-resident reuse and rollback. Persisted configuration is rechecked on both sides. An incomplete native drain cannot admit a pending cached continuation or rollback warm; it remains an explicit refusal until observed recovery. The cheap event-barrier regression injects revocation without changing configuration and proves no warm/Ready stage follows it; actual catalog deletion remains an integration boundary.

Configure serializes its persisted CAS and cancellation/pending-intent invalidation under the lifecycle mutex, then the ConfigurationStore writer lock, with no await held. This prevents an earlier accepted handler that is scheduled later from canceling a newer committed selection. Native handle configuration remains monotonic; cached continuation rechecks the exact current configuration before admission. Startup and shutdown failures in the private fixture retain and observe the actual spawned service task, with ordinary shutdown attempted and no task-detach, abort or fabricated successful closure.
