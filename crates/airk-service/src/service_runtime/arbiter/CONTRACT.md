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
completion may reconcile unused tokens, and only a refusal that the native lane
observed before any model work releases a Context reservation whole; no ordinal
is ever refunded. The shared
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
ceiling (at most 8192, relaxed from 2048 in 1.8.201), partition token/call ceilings and immutable original
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


## Canonical optional asset sources and bounded whole-asset fallback

Only explicit Provision may use network transport. Recover locally reconciles
and warms already verified assets; status, startup, read and import never download.
Core's immutable origin policy and complete source ordinal roster own every URL.
Service accepts native primary and at most two actual same-byte backup rosters;
it never creates a URL, mirror, profile or source authority from downloaded data.

Every native part retains its first established absolute 900-second deadline and
four-redirect budget across whole-asset attempts. Response and body-idle deadlines
remain thirty seconds. Successful earlier parts of an uncommitted failed asset
may be replayed into the fresh writer, retaining their original clocks/counters;
expired earlier parts refuse replay. Cancellation, supersession and custody or
policy failures stop without backup. Only source-class transport/status/body/hash
failure can move once to the next complete compiled ordinal, at most three
whole-asset attempts total. No fallback grants time, consent or enrollment.

An alternate starts only after Store's same-Transaction restart authenticates
and removes the exact positively sealed current failed asset/proof and returns a
new zero-byte writer. No failed-part append and no deletion of completed assets
or foreign/unsealed/replaced state. Other completed assets use verified cache.
Full part hashes and complete logical asset SHA/length still gate publication.
Failed sources retain bounded diagnostics by asset, ordinal and failure class;
signed redirect queries and arbitrary response content are never logged.

Initial sources use Core download_origin. GitHub redirects stay on exact existing
official release CDN routes. HF redirects stay on the original pinned repo/file
resolve-cache route or the actually observed HTTPS us.aws.cdn.hf.co
xet-bridge-us/<24hex>/<64hex> route. No wildcard CDN, source cross-over, arbitrary
relative path, credentials or port. Redirect transport is not an independent
backup and never changes native bytes/hash authority. Authenticated publisher
metadata/HEAD alone does not claim an actual complete model transfer.

No optional Arbiter weights enter installer/native/VSIX payload inventories.
Existing core MiniLM embedding remains separate. Manual offline import is
secondary; genuine canonical source availability and final model/platform gates
are required before shipping. Empty backup lists retain old native bytes.

Production composition binds the already-running service Log to both stage
lifecycle states before launch. Provision clones that handle under the lifecycle
lock, releases the lock, then uses the existing nonblocking record_code path
outside every Store proof guard. Existing ModelFailed records carry only public
asset SHA, bounded source ordinal and static failure class (at most128 bytes);
no URL, signed query, token, body or local path is recorded. Existing dropped-log
telemetry remains authoritative; retention is not infallible. Pure tests may
explicitly omit the sink and make no durable-diagnostics claim. No new log kind,
record schema, file format, writer thread or logging framework is introduced.


## Historical and current private Answer request templates

The original native_goal_context_template and actual_private_arbiter_fixture
selectors retain an explicit historical2048-token/30000ms request, independent
of Budget::default. Their emitted native request/response-option bytes remain
the historical control; a current default never relabels that population.

The separately named native_software_v2_context_template test and
actual_private_arbiter_fixture_software_v2 selector use the literal current
8192-token/120000ms budget, checked against Core ResourcePolicy before startup.
Both templates serialize the complete native ContextRequestV2 plus canonical
MCP ResponseArgs (Full/include_status) and decode through native AnswerArgs.
The current emitter has its own explicit output environment variable and a
fresh no-clobber artifact destination. Python never constructs or rewrites the
budget/response-option contract. The selected Rust fixture startup passes its
complete template into the unchanged owner/lease/custody path; only question
substitution and actual separately admitted tool selectors belong to consumers.
No profile, asset-parent, source authority, restart or cleanup boundary is relaxed.


## Private explicit reviewed selection for composed fixture emission

Ordinary U103 setup creates its own one-index set transactionally. Other reviewed
Projects may legitimately include that same immutable index. The ignored private
emitter cannot require global membership uniqueness when canonical setup already
returned an exact native-reviewed grant selection. It must neither delete valid
Projects nor choose by name, row order, or first matching membership.

An additive OPTIONAL reviewed_selections array in the existing private source
input carries unchanged canonical Core ScopedConnectRequest values returned by
reviewed_fixture. The old source_roots-only input retains its original strict
unselected ambiguity refusal and historical tests. When the field is present,
there must be exactly one explicit selection for every expected source/index,
with no duplicate/extra indexes. All namespace/catalog revisions stay exact;
caller-side revision rolling or authority fabrication is forbidden.

The native emitter resolves each actual selection through existing
catalog_coordinator::native_scoped_selection and
catalog_authority::build_catalog_read_authority. These owners authenticate the
host/context, current persisted grant/provenance/revocation/revision, backing,
index/source policy and live/frozen membership permissions. A missing, revoked,
stale or foreign selection refuses. The private owner then requires exactly one
selected Project and one exact current whole-source membership, covering all
original source IDs with empty subtree paths. The selected Project may not merge
other indexes or defaults. The frozen granted scope must also cover exactly
those whole roots, with current source/policy identities and descriptors. Other
legitimate Projects containing the same index are left intact and unselected.

Only existing native WindowScopeSelection is emitted; no new Core DTO, wire,
persistence, source corpus, request budget or read authority is introduced.
Native resolved selections/authorities are retained in a separate private audit
file; the existing bindings shape is unchanged. Actual catalog namespace/revision
are rechecked before output. Later W4 admission/delivery retain ordinary exact
catalog/selection gates. This is not a bearer issuance or qualification grant.

The driver may forward its actual canonical registration selection as opaque
JSON into the native-owned field after setup, retaining the exact file SHA and
source cohort. It must never derive an arbitrary Project ID or alter memberships
to make the old guard pass. No native run is authorized by this proposal.


## D08 current native device configuration and lifecycle

Device management retains typed configuration4/status3 and authentic compiled
catalog/source binding throughout migration, CAS, lifecycle, import, read
admission and final delivery. Historical operations retain their typed legacy
contract and refuse current device state. Migration preserves model, consent,
index enrollment and unrelated state; only the Store owner publishes retained
original bytes and CAS. Auto tries the candidates in the order that Core
`attempt_order` returns: a qualified GPU route for the observed adapter, then
CPU tiers by larger token budget, then by larger CPU class. The Core decision
"CPU ladder, Auto route order and typed fallback reasons" owns that order and
the reason of every candidate that is not selected; the service keeps no
ordering of its own. A preference restricts GPU only.
Runtime metadata, status and advisory candidates cannot mint ExecutionProfiles,
capacity claims or GPU evidence. Runtime and adapter observation happen on the
existing native compute lane, with the actual service lease and original
operation deadline. No provider/model downloads occur during evidence reads.
Unknown native work retains all pins/charges and cannot trigger an automatic
score replay. Pre-work unsupported/runtime/initialization/capacity failure may
try the next admitted variant under the same original budget and current
consent. Post-work fallback is future-only after actual synchronization and settlement.
The failed read remains failed and is never replayed or given a fresh budget.

The device layer starts with service-wide GPU use off. Only the resource
configuration that the service applies turns it on, so a missing or failed
application leaves every GPU route closed. With GPU use off, Auto records
`gpu_not_opted_in`; an explicit CPU policy records `forced_cpu` whatever the
setting is. With GPU use on and no admitted GPU variant of the model, Auto
records `no_qualified_gpu_route`. The only adapter enumeration in this build is
the CUDA probe, which observes NVIDIA adapters; `no_gpu` is recorded only when
that probe ran and reported nothing. An admitted GPU variant that the precision
preference excludes is recorded `gpu_precision_excluded` and is not probed.

When the order ends with nothing selected, the stage drains any resident it
still holds, installs a selection with `selected: null` and its rows, keeps no
native selection and reports `unsupported_system`. When a candidate was
passed over only because its verified assets are not installed, the phase is
`missing_assets` instead and the stage reason leads with that fact; an idle
stage in that phase reports `requested`, as it does after assets are removed.
The other stage and retrieval continue. An expired deadline, changed authority or unknown
settlement stops the operation: it reports `failed`, tries nothing further and
records `operation_failure` with the rows gathered so far. That record is made
only while this document is still the installed one and the stage is not
resident; a resident stage keeps the selection it is serving. An idle enabled
stage reports `unsupported_system` or `constrained` when that is its settled
phase, and `requested` otherwise.

The stage reason of a resident selection and of a no-fit or stopped selection
is the Core disclosure sentence, and each variant's reason is the Core variant
sentence with the typed cause recorded for it. A Context assessment reserves
the smaller of the remaining request tokens and the selected profile's
`max_tokens`. When the native lane refuses an input before any model work,
because the compiler refused it or because the compiled input is above that
reservation, the assessment is a known refusal: no inference ran, the token
reservation is released and the ordinal stays used.


### Native device lifecycle review boundaries

A successful retained resident activation and a cold activation use the same
Ready commit: exact current document/selected profile, live source and original
cancellation, authentic lane observation and existing host reservation, and the
retained device epoch must validate before Ready. Reinstalling a selection clears
Ready until that commit. Repeated borrowing of one actual reservation deduplicates
its release observer by the reservation's unique Arc identity. Distinct unresolved
charges are retained even when their manifests match; successful actual release
is the only pruning proof.

InitializeDevice itself holds both stage admission mutexes, in Focus then Context
order, through the Store checked mutation and every original liveness callback.
Busy, pending, draining, resident, queued, running, stopping or unreleased work
refuses; a previously rendered disabled status grants no mutation authority.
Lock order is lifecycle admission -> Store configuration -> native selection ->
source/credential/native delivery gates. Selection install never enters Store,
lifecycle or native gates while holding selection. The final Store checked
commit holds that same native selection mutex through the real delivery effect;
a sampled atomic generation alone is not a serialization fence.

CancelDevice invalidates the stage continuation generation, removes its bounded
pending device command, and cancels the current notifier token even when idle.
Final native operation admission checks that generation while holding the same
stage lifecycle mutex, so an already-popped command cannot start after Cancel.
Actual active/warming stage profiles are asked to release through their native
owner; unknown work and charges remain held. Other stages are not canceled.

Fault circuits use actual adapter UUID, signed driver identity/API, runtime build,
profile execution identity, configuration revision and catalog/source identity.
An ephemeral wrapper/device dispatch epoch is freshness evidence and cannot
clear a circuit merely because the wrapper was dropped or reprobed. Actual epoch
checks still fence resident reuse and delivery. No advisory fallback helper can
authorize replay of the failed current request.


### Exact native CPU Focus source-transition caller

Focus receipts obtain a CPU task identity only from the admitted native
ExecutionProfile::validated_task_policy owner after exact complete description
comparison. That owner validates current source or its closed, source-pinned
CPU transition witness. Receipt identity retains the actually declared task;
current native compiler input digests are unchanged and never retagged. The
owning transition audit stays separate from canonical evidence bodies. GPU
Focus remains current-policy-only and has no CPU transition path. Context has
no compatibility exception. Raw Profile metadata or a manifest-specific branch
cannot grant this admission. All source/deadline/native-session/delivery fences
remain as before.


## Product G3 live local eligibility

Product GPU Ready requires the provider's live non-Deserialize DeviceEligibility,
not an artifact description or declared/scientific fit. One existing explicit
management operation keeps its original absolute<=120s cutoff for actual load,
full placement, one complete independent canonical roster and synchronization.
Affected retained GPU sessions are reaccepted with their same scorer/source/lease
and original charges within that SAME cutoff; no per-session renewal or reload.
Failed/unfinished acceptance cannot retain stale Ready. No per-user20/p95/peak or
release qualification is asserted. Scientific RuntimeManifest/1 remains unchanged.

Explicit Cpu management never performs a GPU self-check. If its actual residency
change invalidates another GPU witness, that stage remains constrained until an
appropriate explicit Auto management operation succeeds. Evidence queries never
establish eligibility, retry the failed query or receive a fresh budget. A joint
change alone is not a device fault and does not schedule fault recovery.

The native lane derives ephemeral configuration/source/joint keys from exact
Core4 canonical bytes, the actual compiled catalog source, and real retained
stage/profile/device UUID+epoch/charge identities. One native management nonce is
shared across the affected population. Actor revocation only removes readiness;
only real provider validation creates proof. Session removal/drop, configuration,
joint residency or device changes invalidate proof. Entry and final delivery
retain original authority/deadline and current proof. Unknown settlement retains
actual session/source/device/host charge/lease and an incomplete barrier.



## G4 scoped device diagnostics (additive read contract)

Canonical airk_diagnostics may carry scoped_device_status, schema
airk.arbiter.scoped-device-status/1. Its exact ScopeBinding and per-stage
IndexApplication distinguish this index's eligibility from complete native
DeviceStageStatus residency/selection observations. No configuration document or
global enrollment list is serialized. Typed unavailable uses revision0 and no
invented stage/model state. Observations are descriptive, never execution or
qualification capabilities. Scalar status and Search/Answer schemas are unchanged.

The native read-only ArbiterDeviceRead method carries actual worker/scope, pinned
publication and original ReadAttemptBudget; response uses the same typed name.
Service repeats actual scope gates and original monotonic budget before/after;
Kernel repeats actual scope/publication fences. Only strict current4 reads and
metadata snapshots are permitted: no migration/CAS/rebind, model/device load or
probe, downloads or native continuation. Default absent controller/original
attempt omits the optional observation. Native absent/retained/corrupt4 yields
truthful unavailable, never fake disabled/current configuration.

Combined diagnostics keeps the complete observation under each corresponding
member.result; it never fuses authorities or global enrollment state. Complete
copy/export retains precision, native fallback reasons and all variant metadata.
The additional observation is bounded256KiB inside unchanged original reply/frame
ceilings. No retries or replacement clock are introduced.

Product /2 observes adapter metadata without retaining every primary context.
It selects one compatible dedicated CUDA adapter from authenticated requirements
for the permitted model and precision profiles. It ranks devices that fit a
permitted variant ahead of those with insufficient physical capacity. Within
each group it ranks greatest physical capacity first with a native ordinal
tie-break. Only that
context is retained and its actual free capacity,
cold requirement/headroom and full eligibility are checked. Failure describes
that selected device, not every GPU. Other adapters are not initialized. Native
same-model precision/CPU choices keep the original budget; no selector expands.


Diagnostics additionally requires positive physical current4 binding observations before/after each strict read (canonical raw SHA, revision and native catalog binding), repeated around the complete stage snapshot. Management load() synthesized disabled defaults are never accepted as physical current4. Both stages also require equal read-only Shared selection/residency stamps before/after: actual generation, cached document, selected manifests/reasons, ready and intent maps, eligibility and cached/native epoch/poison state. Changed selection/eligibility yields unavailable. This does not claim atomic unrelated download or work-progress counters. No CUDA probe is permitted.

Delivery captures local eligibility only for stages named by the existing exact
expected result bindings. A known-settled failed GPU refresh cannot block an
unrelated admitted CPU result. The same GPU result still requires its current
proof. Operation-wide unknown settlement, generation and native epoch/poison
fences remain unchanged; no replay, renewed cutoff or relaxed input is allowed.

Retained reaccept and cold warm use the same actual returned typed-fault recorder.
Only known-settled genuine faults under current original authority create a
stable circuit for the actual failed resident, including another retained stage.
All actual retained GPU targets are checked for open circuits before reaccept
or cold native work, after any replaced self has really drained. Explicit Cpu
is exempt from GPU self-check. No unchanged failed retained roster is replayed
by recovering an unrelated stage. Generic production probe refuses scientific
RuntimeManifest/1 before context/candidates; only explicit evaluation may use /1.

### Private Windows CUDA Answer comparison

The ignored CUDA Answer fixture opens the actual isolated evaluation
service with an exact Context Compact FP32 profile, adapter and graph result
pin. Only the private EvaluationFactory can attach this mode. Its 32 GiB
host charge and R2 22 GiB CUDA arena, 16 GiB warm declaration and 2 GiB
headroom fit under the existing 24 GiB device budget. Probe and load share
the original 120-second warm deadline clipped to the fixture lifetime.
The standard Answer request uses the ordinary service transport, compiler,
dispatch and settlement. The CPU and CUDA comparisons use the same
frozen request, corpus and underlying model assets. The observed backend
and compiled input must agree with the intended measurement. The paired
timing is private component and transport evidence.

The same private seam also admits one Context Large FP32 profile: the
corrected batch-view graph with its retained reviewed placement result. It
opens through the reviewed Large resource route under the same Answer limits
and original deadline checks. Its host charge is the profile's 40 GiB
reservation and its device envelope is the 44 GiB (40 GiB cold, 8 GiB warm,
4 GiB headroom) the corrected graph ran its four widths under; Compact keeps
its 32 GiB host and 24 GiB device bounds. Up to two CUDA Context profiles,
at most one Compact and one Large, are attached per isolated lifetime, at once
and once (`with_private_gpu`; `with_cuda_answer` is its one-entry form). The
lifecycle keeps them in a map keyed by manifest, all bound to the same lease,
gate and adapter. The ignored `cuda_answer::same_install` selector can
attach them to a same-install restart of the CPU evaluation fixture: the CPU
roster stays the restart identity, and the CUDA profiles join only the
evaluation catalog, with their weights held as read descriptors and never
seeded. A protected FP16 profile (bounded-hybrid placement) carries no graph
result: its review pin is the pass-1 graph diagnostic receipt's bounded
policy, which the profile's placement must name and the scorer checks again
against the live session. GPU revocation releases and drains every private
entry the native owner holds. Focus has no CUDA route in this seam.

### Context switch within one lifetime (plan 12.5.2 item 4)

Switching between device profiles goes through ordinary Configure. Before a
warm reserves its host charge, it releases a different active resident of the
same stage when that resident is a private CUDA entry: `release_profile`, the observed release, then
the final charge drain, and only then the new reservation and load. The two
models are therefore never resident together, and per-model peak memory is
the model's own. A store-backed prior keeps its verified files pinned for
rollback, exactly as the budget-driven drain does; a private CUDA prior keeps
its held descriptors instead. A failed new warm rolls back through the
existing restoration path, which reloads the prior. Other switches keep the
existing budget-driven drain; product device variants already drain in device
management before their own load. Every such release, and the idle unload,
goes through one order (`release::release_then`): request the release,
observe it, observe the final charge drain, and only then reserve. A
cancellation before the release keeps the resident; one during the drain lets
the drain finish and reserves nothing; a failed or timed-out drain never
reserves. Unit tests drive that order with recorded fake steps. The private
warm observer records the released prior and its release time.

No live service lifetime has run this switch on the GPU yet. The A6000
figures (reload 7.8 s for Compact and 16.9 s for Large, release under 0.2 s,
bit-identical Compact scores across the switch) come from the ignored
`l5c_switch` selector, which opens and drops the private Answer scorer
directly in one lease. They are scorer-level measurements, not measurements
of Configure, the service warm or `drain_prior`.

- **Holds:** the switch and revocation selectors
  (`lifecycle::tests::device_switch_releases_a_different_resident_before_reserving`,
  `gpu_revocation_releases_every_private_entry_the_owner_holds`) and the
  release order (`release::tests`, 4 tests: order, drain timeout, cancellation
  before and during the drain).
- **Does not hold yet:** no live service lifetime has switched two CUDA
  entries on the GPU, and the warm path around the release order (charges,
  phases, rollback) is not driven by a test, because the service unit tests
  have no native model owner. Owner: the LG preparation run (two-entry
  roster, a `gpu_fixture.py` two-entry mode and a fixture image).
- **Closed (L5C review M4):** the first version had no test of the order at
  all and described the scorer-level figures as the switch measurement. The
  order is now one function with a test seam, and the text says what was
  measured.

### Idle unload (plan 12.5)

Each stage's resident unloads after a period with no model call (default 30
minutes; `None` or zero never unloads). A per-stage watcher, started on the
first model call, applies a pure rule with the clock supplied: only a settled
Ready stage with no call in flight and no lifecycle operation unloads. The
unload claims the lifecycle like Configure (a Configure that arrived first
wins), then uses the same release and drain as a profile change. Each model
call is counted before it reads the stage's availability. The unload checks
that no call is in flight and sets Draining in the same critical section, so a
call either holds the unload off or reads a stage that is not Ready. Status
shows Draining while it unloads, then Requested with an "unloaded after the
idle timeout" reason. A drain that fails or times out stays Draining and still
leaves the reload marker. The first model call after an unload starts the
reload through the verified-restore path (private CUDA entries included). It
takes the marker only when no lifecycle operation owns the stage, and puts it
back if the restore is refused. A refused restore marks the stage Failed only
when no operation has claimed the lifecycle since. That call is answered at
once from the not-yet-Ready stage, so it returns no advice and the Answer runs
on reads or the no-model loop. The reply discloses this as the existing
`context_absent` or `focus_absent` degradation; a reload-specific reason in the
reply needs a wire addition. A reload never blocks or times out an Answer.
In the product the reload is the device-configured warm of the current
document, the same route a service start takes; in the private evaluation it is
the verified restore.

- **Holds:** the rule (`idle::tests`), the claimed unload with recorded fake
  release steps (in order, skipped while a call is in flight, a failed drain
  left reloadable), the marker rules, and the refused-restore rule
  (`lifecycle::tests::a_refused_restore_never_overwrites_a_newer_operation`).
- **Does not hold yet, with owners:**
  - The time is settable only from the private evaluation
    (`with_idle_unload`). A product setting and an explicit pre-warm request
    change persisted configuration and the wire; owner L1 (contract
    decision).
  - A reload-specific reason in the Answer reply needs a typed unavailable
    reason in the service's scoring reply and a matching degradation; owner
    L1/L3.
  - The admission estimator's use of the measured reload time; owner L2b.
  - No live service lifetime has run an idle unload on the GPU; owner: the
    LG preparation run.
- **Closed (L5C review M1, M2):**
  - M1: a call could read Ready before it was counted, so an unload could
    release under it, and a failed drain left the stage Draining with no
    marker. Calls are now counted before availability is read; the unload
    checks for calls and sets Draining in one critical section; a failed
    drain leaves the marker.
  - M2: the reload marker could be taken while the unload itself still held
    the lifecycle, and a refused restore lost it and overwrote the phase. The
    marker is now taken only when no operation owns the lifecycle, and
    restored when the reload is refused; a refused restore marks Failed only
    when nothing claimed the lifecycle since.

The ignored `emit_paired_compact_cpu_comparator` selector stages only private
CPU profile metadata. It takes the reviewed CPU profile, held CUDA profile and
its exact raw all-unavailable CAL/2 as three SHA-pinned inputs. It requires
matching current Context limits, model assets, compiler and task policy,
unchanged CPU fit, and the original CUDA calibration bytes. It emits a
separate CPU manifest and byte-identical CUDA calibration under artifacts.
The normal Answer actor validates that calibration before dispatch and
applies its empty rules after native scoring. Verify real matched native
dispatch and successful score return on both backends. The resulting timing
covers the actual pipe, native scoring and abstention on the shared frozen
input. Positive choice timing and calibration qualification require their own
evidence.

The private CUDA fixture authenticates the complete declared asset closure
through the retained read-only source descriptors under its original
deadline. Only the exact graph-bound manifests can use that source for a
fresh, same-lease Context activation. Its ordinary native scorer
also verifies the bytes it consumes. Status projects the private source
availability while the normal Configure, scope, graph and model load
checks continue to govern Ready. No legacy or device asset receipt is
created for that GPU manifest. Legacy seeding for the other profiles keeps
its existing Store contract. Private CUDA failures retain their original
result without a legacy cached continuation.

### Private catalog-only evaluation profiles (22 November 2026)

`catalog_only.rs` compiles only with local-inference and test-support, and
only the ignored private fixture can register (`register` is `cfg(test)`).
A registration holds every asset of one unqualified `private-` CPU profile
with no downloads and no device binding. Each original file's length and
SHA-256 are authenticated once and its read descriptor is kept. Nothing is
written to the asset store, no weights are copied, and the Scorer still
checks the SHA-256 of the bytes it consumes. A registration is bound to the
registering `ServiceLease` by a weak reference. Another lease, a dead lease
or an unregistered manifest sees nothing. A live registration cannot be
replaced.

The `assets.rs` hook (`StageOwner::catalog_only_held` and the asset-source
lookup) answers only for a stage with an evaluation catalog. A product build
or product stage always gets `false` and the ordinary store source.

Three `lifecycle.rs` availability hunks, authorized by the user on 22
November 2026, treat exactly a held registration as provisioned in place of a
store receipt: Status projects `Requested`, the cached-intent gate counts the
assets as ready, and the restore path validates enrollments and launches.
Configure, scope, enrollment, hardware admission, warm and the deadlines
still govern Ready. No receipt, persisted format or wire field is added.

The service resource policy owns global GPU eligibility. A change to allow_gpu applies to both stages while preserving their stored selections and revision. GPU use is opt-in under the Core decision "Service-wide GPU use is opt-in" in the `airk_core::arbiter` contract. At startup the service applies the configuration in effect before it restores any stage: a missing value is off, and an enabled value stored under the earlier default is off with its reset notice pending. Only a value the user set through the service-wide setting allows GPU use. Answering that notice with off changes no GPU state. A configuration error closes GPU admission until the resource policy is repaired. A disable transition fences pending GPU probes, activation, resident reuse and delivery, revokes device readiness and schedules same-model CPU selection for active automatic stages. A stage without active work stays idle. The ordinary warm, drain, fit, permission and deadline checks apply on each transition.

## Answer /3 card scoring, batched Focus and the loop ledger (L4, 22 November 2026)

Owners: `card_scoring.rs` (card model observation and one batched card
call), `focus_batch.rs` (one batched Focus call), `loop_ledger.rs` (model
accounting). They serve the kernel's `NativeController::card_model`,
`score_cards` and `focus_batch` through the private control methods
`ArbiterCardModel`, `ArbiterScoreCards` and `ArbiterFocusBatch`, which are
refused by the metadata relay and framed like the other private Arbiter
methods. Answer /2's `joint_ledger.rs` and `joint_scoring.rs` are unchanged
and are deleted with v2.

**Card model.** `ArbiterCardModel` returns the selected Context profile's
model, device and (GPU only) precision when the stage is ready, the index is
enrolled for Context and the profile declares all four card tasks; otherwise
`None`, and the kernel's deterministic order decides. Large is never
reported on the CPU.

**Enrollment.** Loop card tasks run under the Context stage's existing
per-index enrollment (`airk.context_action.v1`); the persisted configuration,
`Stage::permits` and enrollment validation are unchanged. The lane maps card
tasks to that enrollment (`pool::arbiter::enrollment_task`).

**One card call.** After the scope issue and structural validation, the
service refuses before any reservation or compile, with a known zero-work
`Unsupported` advice, when the selected profile does not declare the task
under the request's policy or the roster breaks its task policy's rules. It
then reserves the call's slot and its per-part, per-round attribution in the
loop ledger, compiles the task policy's view, and scores it on the resident
session under the lane key (original attempt, lowest part). Observed
completion re-attributes the measured tokens in proportion to the reserved
attribution, the remainder to the lowest part and round, so the advice's
attribution equals its observed tokens and charges every segment's part and
round. Selection is per segment from the authenticated `LoopCalibration` of
the resident profile: margins are exactly top minus runner-up; zero rules
abstain. A lost outcome keeps the whole reservation and every segment
unavailable; a known input refusal releases the tokens and keeps the slot; a
late scope or configuration change discards decisions and keeps the work.

**Batched Focus: per-candidate questions.** Each pair is already its own
lane job, so a per-candidate question costs no extra model work: a pair job
carries `questions[candidate.question]`. One batch takes one Focus ordinal and
one ledger reservation (pair tokens times scored pairs, attributed per part
and round), instead of one call per sub-need group, which would have spent
the `focus_calls` slots on fixed per-call overhead (IPC, application
observations and a reservation each). This is the plan 12.9 decision "by
measured cost": the structural cost is equal per pair either way, and
grouping saves the per-call overhead. A wall-clock measurement of that
overhead against per-pair time on the A6000 belongs to L5/LG; it was not run
here (no GPU work in L4). Ranking applies only when every scored pair
completed with finite logits before the deadline and authority still holds.

**Ledger.** Card slots are valid in `1..=context_calls`, Focus ordinals in
`1..=focus_calls`, one card call at most 8,192 tokens, one Focus call at
most `round_input_tokens`; the attempt pool is `input_tokens` and each part
and round (round 1 or later) at most `round_input_tokens`. Split and Plan
(round 0) draw on the pool only. A partition limits its card and Focus
ordinals, tokens and Focus pairs, and the attempt's Focus pairs stay within
the parent's `focus_pairs` (added in `38e141a0`, change-log entry 39). Slots are never reused; the parent budget and attempt
identity are fixed once charged; the window is the parent deadline, at most
600 s.

**Decision mode.** `DecisionMode::TopScore` (evaluation only) is set only by
`EvaluationOwner::with_decision_mode`, which requires the private native
evaluation catalog; product builds have no constructor for it.

### L4 review closures (22 November 2026)

- **Known non-dispatch (M3).** Everything before a card job enters the lane
  queue (the resident lookup, the reuse admission, and `score`, whose every
  error is a typed `AdmissionRefused`) is a known non-dispatch; a deadline can
  expire only while awaiting the reuse admission, before `score` runs. Such a
  card call is released through `loop_ledger::refused` and reported
  `Unavailable` with zero observed tokens; the slot stays used. A Focus batch
  that queued no pair is released the same way. Before, these kept the whole
  reservation as unknown work.
- **Plan limit (M2).** The service passes the parent budget's
  `max_sub_needs` into selection, so Plan never accepts more sub-needs than
  the reply can hold.
- **Batched Focus attribution (L1), recorded.** The native Focus request does
  not say which part a candidate serves. Observed tokens are re-attributed in
  proportion to the reservation (pairs reserved per part and round), so a
  batch cut short charges every reserved part-round in proportion, not only
  those whose pairs ran. Exact attribution needs a per-candidate part in
  `NativeFocusRequestV3`.
- **Lane keys (L3).** A card call names every part its segments serve; a
  Focus batch every part it charges.
- **Identity (L4).** The choice-to-decision mapping is
  `loop_cards::outcomes` (part of the card policy identities); a batch
  receipt's identity is `loop_focus::policy_digest(pair policy)` and its
  ranking is `loop_focus::rank`.
- **Tests (L5).** `settle_card_call` and `settle_focus_batch` hold the
  settlement after the reservation and are driven by a fake lane in
  `card_scoring::settlement_tests` and `focus_batch::tests`. The stage
  owner's admission against a live configuration and supervisor is not
  covered; it needs a native resident.
- **Scoped device status.** Its control now follows the Answer read
  ceiling: 605 s accepted, one nanosecond more refused, other tools refused
  above 125 s.

All closed in `9fea69ae`; counts in `docs/arbiter/answer-loop-changes.md`
entry 35.
