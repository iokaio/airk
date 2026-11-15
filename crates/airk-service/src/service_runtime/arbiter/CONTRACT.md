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

Explicit recovery reconciles a matching removal receipt/configuration revision or activates fully verified cached assets. Incomplete acquisition requires a new explicit named Provision or owner-only Import with a local bundle; Recover never converts local acquisition into network authority. A removal stays a removal; canceled/removed work is terminal. Interrupted generation data remains private and preserved. Restart may warm only exact saved enabled/enrolled choice with a current qualified compiled profile and complete Ready receipt. Missing/incomplete/canceled assets never start a download on restart. Application and score admission retain actual WorkerIdentity/ScopedReadRef checks before dispatch/delivery; these helpers remain independent of management authority.

Cheap tests exercise Store with a real ServiceLease, streaming part/full-byte refusal, operation identity/cancellation and switch-CAS rollback. Actual qualified assets, resource refusals, native cancellation/drain, restart, release-byte acquisition and profile quality require the owning integration/platform campaigns. Empty/unqualified compiled catalogs honestly refuse provision/warm; developer exports and operator probes do not qualify a product profile.

Native Admin dispatch checks are pure and rechecked after owning waits immediately before configuration publication or background operation admission. ConfigurationStore.apply_checked invokes the supplied generic check after its actual writer lock and again before persisted configuration publication. Once an original background job is accepted, its ServiceLease, captured configuration revision and explicit cancellation own the operation; a later transport deadline is not a fictitious rollback of accepted work.

Under local-inference + test-support only, EvaluationOwner is a typed isolated-fixture constructor using the actual ServiceLease and native admission gate. It mints Kernel's opaque EvaluationCatalog from that same lease, accepts only unqualified candidate descriptions and seeds model bytes through retained read descriptors into the ordinary verified Store transaction. No I1 request, role flag or downloaded metadata attaches evaluation authority. EvaluationResult always serializes evaluation:true/release_qualified:false around the unchanged native result. The same application/scorer/management/resource/lifetime code runs; candidate evaluation never populates or bypasses the product release allowlist. Optional restore failures remain contained to Arbiter and preserve ordinary service startup.

Attaching that private owner to the fixture's ordinary control dispatch requires the exact same ServiceLease Arc and native admission gate instance supplied by the running fixture. Its native status/profile/application reasons explicitly identify private evaluation and an unqualified candidate, including ordinary transport results without EvaluationResult wrapping.

Scoped application discloses a selected profile only when Arbiter is enabled and that exact immutable index is enrolled. Disabled and unenrolled indexes return no profile; management status remains the owner-private global view.


The private ignored `service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture` runner is compiled only with local-inference + test-support tests. It requires explicit SHA-pinned private input rows, raw profile SHA pins, exactly the three selected stage/model choices and qualification=None, readonly nofollow source descriptors, a fresh artifact work directory under a protected parent and an explicit lifetime. It uses actual first-start Genesis and then the ordinary service startup, sole ServiceLease, worker/catalog admission and control listener; its factory seeds verified Store assets while leaving configuration disabled. There is no product role, downloaded manifest admission, fake I4, fake Ready or automatic enablement.

Its private `airk.private-arbiter-fixture/1` ready record includes actual Core ServiceHealth (instance/PID/image/version/user/catalog clock), control root, pinned input digest and exact raw profile snapshot hashes. It is written only after an actual matching I1 Health response. Source-index creation, enrollment, selection, warm and canonical reads use ordinary native callers and retain full real outputs. The private test executable identity is recorded separately from matched client-pair pins; no receipt may claim the pair's service executable ran instead. Shutdown is observed through the ordinary owner request and actual service completion; a closed receipt additionally requires actual service lease reacquisition. A shutdown deadline writes Unknown and awaits actual completion without pretending a kill/drop is successful drain. Original assets/work/receipts remain private and preserved.

Canonical answer application additionally requires ContextAction in both the actual immutable-index enrollment and selected native profile. A relevance-only resident model can remain Ready in owner-private management status while answer application reports UnsupportedTask; this is not an applied answer adviser.


The second private ignored selector `emit_private_arbiter_configuration` observes the running fixture Health and Arbiter Status, verifies a SHA-pinned frozen source-root list against the canonical activated Store source census, and emits actual immutable index bindings plus a Rust-serialized Core ManagementRequest::Configure instance. It changes no index/configuration itself. Duplicate, multi-root, missing or foreign bindings refuse; the current actual configuration revision supplies the CAS. Existing native callers perform setup/apply and retain their full results. Configure output eligibility additionally records current ContextAction task-policy binding; relevance-only mechanics cannot be claimed as a goal arm. These private observation files are not new product DTOs or evaluation admission capabilities.

Resident native scoring reuses the exact charged reservation owned by that verified active session; it never reserves the full qualified load/score peak a second time. Ordinary work and concurrent warm still observe that same charge. Switching first attempts the independently charged new warm. If both peaks cannot fit, it pins the actual old resident's verified disk closure, requests its release, observes session completion and then awaits the last actual reservation-owner drop before one new reservation attempt. Failed/canceled warm likewise drains its actual session and last charge before an exact-CAS rollback rewarms the verified prior cached choice. A newer configuration prevents rollback. No policy limit or qualification floor is raised.

While an accepted lifecycle operation drains, Configure retains at most one replacement intent for the latest enabled, enrolled, already cached choice. Later Configure replaces it; global off, explicit Cancel and Owner teardown clear it. Settlement advances the existing lifecycle serial and admits that one cached-only continuation, with exact configuration/lease/catalog/index admission rechecked before warm. There is no automatic acquisition or retry loop: missing assets, pressure, stale admission or failed drain remain explicit failure/requested states. Older completion cannot steal a newer operation's intent. The original canceled native work must finish its actual drain before settlement starts the next warm.

The private goal template is serialized from Core ContextRequest plus the owning MCP ResponseArgs (Full and include_status=true), then deserialized through actual AnswerArgs. It preserves the independently frozen default budget while retaining the complete native answer packet; no Python request or response-policy DTO is introduced.

The same owning catalog/index enrollment admission is checked at lifecycle entry, after asynchronous reservation/readiness waits, immediately before native warm and before Ready delivery, including exact-resident reuse and rollback. Persisted configuration is rechecked on both sides. An incomplete native drain cannot admit a pending cached continuation or rollback warm; it remains an explicit refusal until observed recovery. The cheap event-barrier regression injects revocation without changing configuration and proves no warm/Ready stage follows it; actual catalog deletion remains an integration boundary.

Configure serializes its persisted CAS and cancellation/pending-intent invalidation under the lifecycle mutex, then the ConfigurationStore writer lock, with no await held. This prevents an earlier accepted handler that is scheduled later from canceling a newer committed selection. Native handle configuration remains monotonic; cached continuation rechecks the exact current configuration before admission. Startup and shutdown failures in the private fixture retain and observe the actual spawned service task, with ordinary shutdown attempted and no task-detach, abort or fabricated successful closure.


## Independent stages and Focus native execution

Configuration/3 and Status/2 are the actual Core/Rust owners. Focus and Context
have separate enabled, selected-profile and explicit immutable-index enrollment
sets, independent lifecycle serial/cancellation/pending state, and one shared
monotonic revision and Store custody. Focus is Qwen3-Reranker-0.6B. Context offers
Qwen3 Compact (1.7B) and Large (4B), with one Context resident. The existing
bounded inference lane may retain one resident per stage; every resident keeps
its exact native source pins and memory charge through actual teardown. Neither
readiness nor consent on one stage grants authority to the other.

Management may complete under an unrelated other-stage revision only while its
own StageConfiguration, installation, catalog, operation serial, cancellation
and original admission still match. A warm uses a refreshed complete snapshot;
all native inference, advice and outer delivery require the exact full revision.
A warm invalidated solely by an unrelated stage revision must actually drain
before at most one cached-only continuation with new complete configuration.
It never redownloads, renews cancellation, or retries unchanged failed inputs.
Rollback restores only its own stage into the current monotonic configuration.

Private Focus application/rank I1 callbacks retain the original read-attempt
identity, absolute expiry, parent budget and native source authority. Admission
precedes tokenizer dispatch. A service-owned shared Focus/Context ledger binds
stage/index/local read ordinal, one-use Context ordinal, common original token
ceiling (at most 2048), partition token/call ceilings and immutable original
expiry. Focus uses exact qualified profile pair-token bounds and scores only the
already admitted original prefix. Each pair contains the original question and
one explicit source window; full source bodies and original source IDs remain
unchanged. The compiler authenticates the fixed publisher instruction.

Actual compilation and native completion are separate observations. Unknown
aggregate completion retains the entire reservation and reports no invented
zero-token settlement. Known completion alone reconciles unused tokens. A failed,
nonfinite, stale or partial prefix never reorders results. Complete valid scoring
orders the whole scored prefix by two-logit relevance, preserves ties stably and
appends the complete untouched tail. Relevance makes no sufficiency claim.
Actual receipts bind configuration/profile/compiler/calibration/task policy,
original full candidate identities, declared windows, scoring counts, known
work and reservations. Semantic input identity is Core native Focus input/2,
normalizing only the two relative transport-time carriers.

Outer delivery validates `(stage,index)` bindings, selected profile, current
stage task/enrollment and exact shared revision under the Store cut. The bounded
roster is Core MAX_DELIVERY_BINDINGS; Context historical omission bytes remain
owned by Core, while Focus bindings carry explicit stage. No metadata scan,
caller profile claim or public receipt replaces actual native admission.


Private I1 ArbiterImport accepts Core NativeOfflineImportRequest only from the
owner pipe. The supplied original absolute native directory is not canonicalized
by CLI, sent through Admin/HTTP, placed in public management schemas, or admitted
by generic metadata forwarding. The actual compiled release execution profile
derives its stage; the expected common revision and original stage lifecycle
admit one owned background operation. Store verifies local nofollow readonly
source descriptors and every compiled asset before destination publication.
Status/cancel remain available while verification/copy runs. Accepted unknown or
interrupted effects retain the original custody/recovery outcome, never an
invented rollback or a replay permission. Success retains verified assets;
warming occurs only for the separately saved enabled/enrolled exact choice.


Configuration load failure exposes Core's typed optional initialization status;
the included disabled configuration is explicitly a non-writable placeholder.
The actual retained legacy digest is included only when positively observed;
ambiguity or observation failure never becomes a claimed absent legacy state.
Actual resident identities, queues, running work and held charges remain real,
including under corruption. Native observation errors refuse rather than invent
zero work. Normal initialized status omits this marker, including genuine model
failures that leave a valid editable configuration. Initialization still rechecks
all native Store custody and expected-legacy checks and never migrates consent.
Stage/status diagnostics are capped at 1024 UTF-8 bytes on scalar boundaries.

Management snapshots refresh the complete current configuration after native
handle waits when the operation's own stage remains unchanged. Removal and
disabled rollback propagation preserve newer other-stage choices; cleanup keeps
its original asset-operation revision. Warm and NN delivery retain strict full
snapshot gates and their one specifically admitted cached continuation.


## Private selected-path CAL observation

Only local-inference plus test-support compiles ContextObservationSink and
EvaluationOwner.observe_context. The fixture attaches that native callback
before the existing exact ServiceLease Arc and NativeCatalogAdmission instance
check in into_owner_for. No public operation, downloaded profile, environment
setting in an ordinary service, or serialized metadata attaches this capability.
The ignored actual-fixture runner may explicitly select this private mode. Its
catalog remains opaque, isolated and release_qualified=false.

Capture uses the actual already-Ready/warmed Context profile and the actual
selected MiniLM/Focus path. It is not a pre-load shortcut or fake readiness.
Before capture the actor validates the installed worker/read authority, original
native attempt/clock, publication, exact current configuration, actual selected
profile, task policy and frozen roster policy. It checks the same fixed native
issue immediately before and after the private observer. Compiler census and
record I/O execute outside the admission mutex. The canonical caller then
rechecks original source authority, profile/configuration and deadline; a failed
postcheck refuses. A retained private snapshot alone is neither delivered
advice nor completed admission/qualification evidence.

The hook runs before the deployed Context score queue, Context NN reservation
and Context forward. Separate pure compiler census and private recording retain
their own measured work/receipts and consume elapsed original time; they cannot
be called deployed Context timing or hidden inside a zero-cost model claim.
After successful canonical postchecks, the explicit unavailable advice preserves
the requested original token ceiling and reports observed_input_tokens=Some(0)
with default deployed Context Work: Context scoring was known not dispatched.
It supplies no action, score, confidence or fitted/calibrated decision. Existing
actual Focus work and its shared original-attempt ledger charges remain intact;
no Focus reservation is refunded, renewed or converted into fresh Context work.

Captured Core requests, profiles and source references are serialized by their
Rust owners; no handwritten request/response mirror is introduced. Private
recordings retain full post-Focus provenance and actual selected profile/
pipeline identities. Duplicate/cap guards belong to this bounded private run,
not to a new product quota. Verified counterfactual read requests use separate
canonical transport attempts and disclose their preparation overhead; they are
not actuated learned trajectories or extra allowances in the captured request.
Artifacts remain private and unqualified until the required actual native,
independent-label, fitting, held-out, resource and product qualification gates.


## Private actual-dispatch and charge observation

Only native test-support compiles the optional execution observer. The public
ScoreRequest shape, ordinary score path, native authority, resource admission,
queue limits and original deadline remain unchanged. The service fixture attaches
one typed observer to its EvaluationOwner before the existing SAME ServiceLease
Arc and NativeCatalogAdmission instance check. No serialized request, role flag,
public operation or ordinary-service environment setting attaches it.

Bounded metadata preparation occurs before the existing final
`dispatch_before_original_deadline` guard. Only the passed guard's actual score
closure emits dispatch_admitted immediately before observed_score_call/scorer.score.
This is a native score-call boundary observation, not a claim to execute inside
FFI. Expired/canceled/pre-dispatch refusals emit no dispatch witness. The actual
returned success/error, raw f32 bit patterns (including nonfinite failures), and
owning native ScoreReceipt are observed BEFORE source/delivery refusal can discard
scores. Missing return never becomes known/zero work or a successful NN claim.

The witness is the LAST Job field: job_settled follows actual original compute,
delivery and per-job resident-permit drops without changing field-drop order.
It does not claim resident/session completion. Dispatch also supplies an opaque
OptionalRelease proof from that exact actual resident reservation; observation
retains only Weak allocation identity and release-state custody, never a strong
charge. The fixture deduplicates native release waits by actual charge identity.
Only that original opaque future's actual completion may emit profile_released
and charge_released for its dispatched bindings. Actual ResidentSession scorer,
source-pin and reservation drop order remains authoritative. A forged/crossed
binding is refused; JSON observations never grant source or release authority.

A bounded nonblocking try_send channel (at most 4096 events), try-lock metadata
registry (64 original charge allocations) and monotonic native event/dispatch
identities never perform observer I/O or await under native/source admission
locks. Full, contended or disconnected observations increment an explicit lost
counter without changing actual execution or inventing work. The private writer
is outside those locks; it bounds the run to 1024 dispatches, 8192 records and
64MiB, persists actual events to composed-observer.jsonl, and writes its owning
RecorderSummary to composed-observer.closed.json. Loss, writer failure or missing
actual return/release makes acceptance incomplete, never a retry permission.
Wall-clock deadline conversion is observational only; original monotonic native
deadlines remain the sole execution authority.

The JSON event and summary types are owned by Inference execution_observer,
using actual Core ScopeBinding/Task/digests and the native ScoreReceipt rather
than consumer mirrors. The ignored fixture alone accepts explicit true/false
AIRK_NATIVE_DISPATCH_OBSERVER; default is absent. Complete recorder evidence is
not model, task, source-trajectory or release qualification. The driver mutates
only after the actual dispatch marker, requires matching successful finite
native return after its trigger plus actual settlement/release evidence, and
reports a lost race as incomplete without retry. Ordinary source/delivery gates
and existing actual service shutdown/lease-reacquisition evidence remain required.

Cheap controls inject only score-return/clock seams and retain real native queue
and resource charges. They cover canceled/expired guard refusal, exact
success/failure/nonfinite work, last-job versus last-resident release, full/closed
observation loss, no invented completion, crossed release identity, and exact
fixture lease/gate attachment refusal. They run no NN and claim no qualification.


## Same-stage Configure intent and rollback

A successful own-stage Configure CAS records its accepted common revision in
that stage's native lifecycle state before cancellation and before releasing the
lifecycle mutex. Identical StageConfiguration fields still express fresh intent.
The marker is in-process custody, not a persisted field or wire authority.
Preparing a cached continuation may occur afterward; absence of a pending slot
does not authorize an older failed warmup to overwrite an accepted Configure.

Before restoration, rollback holds the same stage lifecycle mutex through the
Store read/CAS and refuses a superseded native serial or a later accepted
same-stage Configure revision. The existing lifecycle -> Store order remains.
An unrelated stage's Configure changes only that stage's intent marker; rollback
may still restore its original stage while preserving the current other-stage
snapshot. Actual native drains, pending capacity, cancellation token, original
operation serial/deadline and charge settlement rules remain owned by their
existing paths. No new retry, model dispatch or activation follows from this
marker alone.

The deterministic native regression stops between accepted identical Configure
CAS/cancel and pending preparation. The canceled old warmup cannot publish a
rollback; an ordinary failed warmup following only an unrelated-stage revision
still restores its own stage. Revoked pre-effect Configure leaves configuration,
intent marker and cancellation unchanged. No NN, fixed sleep or worker required.

Restoration rechecks that accepted same-stage intent under the lifecycle mutex
again before installing a fresh rollback cancellation token/profile. A newer
Configure accepted after rollback CAS cannot have its cancellation replaced by
an obsolete warmup. The deterministic regression also exercises this second
boundary; original whole-configuration/authority/deadline warm fences remain.

Synchronous failure after native lifecycle admission but before spawning removal
uses the same complete-and-resume owner as asynchronous acquisition/removal.
When a newer Configure wins the original removal configuration CAS, the failed
old removal must settle and transfer the one admitted fresh cached intent; it
cannot leave busy/pending state with no future owner. The event-gated regression
uses the real Store CAS and native pending selector without reading model bytes
or dispatching NN, and refuses a stale completion consuming that new intent.

### Private evaluation warm completion wakeup

The opt-in native test-support evaluation fixture may attach a bounded warm
completion observer before service composition, using the same real lease and
catalog gate. This is not a product event, persisted state, readiness authority,
or a qualification claim. Absent the explicit fixture environment opt-in,
existing Ready output and model behavior remain unchanged.

The observer captures expected request configuration without relabeling it as
actual Store state. At the existing owning native observation fences, it copies
actual active profile/revision without acquiring an additional native queue lock.
After the original warm returns (including retained-resident and failed/drained
paths), completion uses only in-memory lifecycle snapshots via try_lock. It
performs no Store load, filesystem I/O, awaited send, blocking observer lock,
model call or admission change. Successful uncanceled completion matching native
activation, lifecycle serial and captured same-stage Configure intent is a Ready
wakeup; superseded intent/serial/cancellation is Superseded, other completion is
Failed. The full actual current Configuration is established only by the fresh
canonical Status read. There is no invented actual_configuration snapshot.

The native activation_path distinguishes retained_resident from
native_warm_admitted and not_admitted. Neither a Ready wakeup nor a warm admission
alone proves a cold load or memory fit: those require original idle/absent-native
state and actual load/resource evidence. A separate bounded writer flushes events
outside ownership gates. Try-lock/queue loss and any captured Guard dropped
without completion make final journal closure incomplete. Service settlement and
all captured completions precede closure; no observer creates settlement.

A future private caller installs a watcher on this journal's actual directory
before scanning it, binds its native observer identity to original fixture Ready,
and binds exact Configure configuration/stage/manifest and a pre-Configure journal
sequence floor. Stale/foreign events never prove the new attempt. Failed or
Superseded matching completion stops the attempt. A matching Ready only wakes a
fresh canonical Status request, whose exact current selected/active profile,
configuration revision and Ready state remain authoritative. The original warm
deadline, whole phase deadline and 128-observation ceiling remain unchanged.
No sleeps, scoring probes, failed-operation retries or registry-file readiness
assumptions are introduced. Prior failed attempts remain failed.

The warm journal seam is compiled only for native unit-test fixtures with
local-inference and test-support. Feature-enabled non-test libraries expose no
observer attachment or lifecycle hook; no integration or product caller owns it.

### Private same-owner evaluation restart

The private ignored evaluation host may explicitly reopen its own previously closed CAL control root into a fresh metadata work directory. The pinned original Ready, genuine closed record and raw Status2 must name the same instance/control/catalog, unchanged three raw Profiles/input digest, no initialization placeholder and zero queued/running stage work. Native shutdown and lease-reacquisition proof is required; actual ServiceLease exclusion is acquired again and ordinary service startup owns the new incarnation. Existing activated catalog/index/cache/configuration stay in place: no catalog Genesis, seed transaction, database copy, source mutation or namespace remapping occurs. The factory accepts the exact saved Configuration only after native Owner loads the same installation/catalog and validates complete equality. Ordinary cached restore verifies current asset custody and enrollment and performs real hardware admission/warm; Ready does not assert that warm completed. Fresh Ready, observer IDs, journals, source bindings and canonical actual Status must be collected before reads. The public product/evaluation constructors retain their default refusal of configured isolated fixtures. Restart is test-only, opt-in through a SHA-pinned private descriptor, creates no public wire or persisted product format, and never transfers qualification or the old process's timing to the new process.

The restart-only optional guard on the existing private evaluation OwnerFactory runs under the ACTUAL retained startup ServiceLease before endpoint/log/source-supervisor/project-supervisor/management startup. It checks all native pending/running work, paused queued jobs and unsettled configuration continuations, plus exact saved Configuration, before recovery can admit work. Constructor checks repeat under the same lease/gate; preflight observations never replace that startup fence. Default product and ordinary isolated evaluation startup attach no guard.

On Windows the journal writer sync_data-flushes each complete record while its
file remains open before callers rely on directory last-write notifications.
Ordinary File::flush alone does not establish an observable write notification.
This storage flush runs only in the separate writer, never the warm callback.

## Partition identity domains

The installed native selection reference is authority; Context's validated
partition.selection is an inert whole-snapshot accounting identity and can
differ from that reference's scope_identity (notably combined Open/W4).
Focus scoring compares these identities only for Core's ordinary singleton
Focus purpose, which also requires an installed selection. Genuine Context
partitions keep their original frontend digest. Core still validates the
original attempt, index, parent/resources and candidate/source bindings before
this predicate, and the native issue/source/configuration/delivery fences stay
mandatory. No model work, budget or authority is created by this distinction.
