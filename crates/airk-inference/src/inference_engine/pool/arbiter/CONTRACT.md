# Optional native score completion ownership

Owner: `airk-inference::inference_engine::pool::arbiter`. Recorded under the
advanced-split authorization. This is a private Rust error/measurement contract;
it adds no wire, persisted format, task, model, deadline or execution authority.

The one owned lane retains each job's actual resident charge, source pins and
compute/delivery lifetime through its real synchronous score return. A canceled
consumer cannot make that return happen early. The compiled receipt is emitted
immediately after successful compilation, before post-compile token/deadline/
authority admission checks; those checks still precede every native score call.

`ScoreReceipt` contains only the exact compiled input digest/token count and
known queue, compile and score elapsed microseconds. `KnownScoreFailure` carries
that receipt, a `source_delivery_refused` flag, and the original private error
cause. It is created only after the score call has actually returned, whether
successfully or with an error. The successful `Scored` shape is unchanged. The
existing score timing start is preserved, including its surrounding admission
cost; these measurements do not newly assert that an FFI dispatch happened when
a scorer rejected input before entering its native implementation.

A post-score source/configuration/queue delivery refusal retains the same known
receipt on both successful and failed score returns; scores are withheld. Such
a receipt supplies accounting, never renewed source permission or authority to
act on the rejected result. Service callers may downcast `KnownScoreFailure` to
retain its measurements before fallible ledger/policy reconciliation. Actual
source authority and the outer native delivery gate remain fail closed.

A pre-dispatch deadline/cancellation or an undelivered/closed completion channel
cannot fabricate a completed receipt. Missing completion means uncertainty, not
zero work. Retaining known work does not refund an uncertain reservation, permit
replay, invoke another score, or release an actual resident/native charge early.
The lane still drops delivery/job ownership in its existing order after the real
call and final delivery check. Static error display does not include evidence;
the original internal cause remains available to the owning native consumer.

Cheap controls inject only the clock and score-return seam used by production.
They cover successful and failed known completion, post-score revocation of
both outcomes, retained causes, and canceled/expired-before-dispatch absence of
completion. They are not model, FFI, platform, or resource qualification.


## Independent resident stages (2026-11-15)

The existing single bounded compute lane may retain one Focus scorer and one
selected Context scorer. No new lane, thread, process or provider is created.
Each resident retains its own actual resource reservation and authenticated
asset-source pins; composed admission is charged to the existing shared native
resource actor, including ordinary MiniLM headroom. A serialized profile hash
never substitutes for the actual opaque resident charge.

The native queue has at most two pending warms, at most one per stage, and one
actual warm/score dispatch at a time. Existing global and per-index source-job
bounds remain unchanged. Stage consent/profile checks precede compilation and
repeat before FFI and delivery against the common configuration revision. A
change in either stage suppresses stale composed results. Revocation discards
queued source bytes; in-flight synchronous native work keeps its pins, charge
and completion accounting until actual return. Releasing one profile only
revokes that profile's queued work and resident session, preserving the other
stage. Native observations expose each stage separately.

The session map drops each actual scorer before its source pins and resident
charge. Completion counts report the actual two resident references, and cannot
reach the native barrier until both sessions and actual work have settled.
Stage-specific cancellation, failed warm, disable and pressure never imply
completion or automatic retry. Optional queueing and model residency cannot
block ordinary lexical, health, metadata or protected MiniLM progress.

## Selected Focus qualification

The explicit ignored Focus qualification child consumes the Rust-owned selected
Profile and its exact prepared asset overlay. It reauthenticates every asset and
the independent publisher512 fixture, acquires a real private native control root
and ServiceLease, and enters the existing NativeInferenceOwner. Neither a JSON
profile nor a fixture manufactures compute, indexed-source or release authority.
The selected experimental baseline is CpuFit physical4/intra4/inter1, sequential
and without spinning. Prior CPU1 numerical results do not qualify this tier.

Native numerical parity compares ten actual compiler-produced model-view pairs
and two explicitly marked raw publisher diagnostic pairs. The raw diagnostic
pairs grant no escaped-caller parity. All twelve three-input static512 executions
must return finite raw no/yes pairs with exact argmax agreement, within the frozen
publisher weight8 limits (raw 0.25, relevance 0.05) and independently retained
Python weight8 limits (raw 0.001, relevance 0.00001). Partial numerical failures
retain the known inputs and observations in a failed receipt.

The resource child records actual native load, one warmup and twenty warm
measurements at full static512 shape, the original near-cap source case, process
lifetime peak RSS and actual execution identity. Those observations alone do not
qualify combined Focus/Context admission or latency, cancellation/recovery, source
authority, a general end-user RAM floor, another target or a release. The actual
composed service path must separately retain its original shared token allowance
and earliest deadline, including the protected Context allocation. No observation
renews a request deadline or authorizes an extra dispatch.


## Selected Context exact-FP32 numeric/resource qualification

The selected FP32 fallback consumes the byte-identical packed graph and original independent official hidden/trained-row controls. Native all-live/H scores must agree with the direct original reference within0.001, and the independently retained same-artifact original FP32 known-live scores within0.001. The obsolete approximate0.25 gate does not apply to FP32. Exact corresponding argmax identities remain required.

Predeclared experimental candidate reservations are ContextCompactCPU4/16GiB and ContextLargeCPU8/40GiB. Separate native job hard limits remain32GiB/1200s and48GiB/1800s. These are candidate capacities, not measured product hardware floors. Genuine composed Focus/Context admission and complete original operation budget remain required.

One separately reported warmup precedes the twenty measured2048-token/eight-live-option forwards. Each observed measured-call duration and actual lifetime peak RSS is added to the partial receipt before validation. Stop on the first observed call exceeding the unchanged30-second deadline or RSS exceeding its declared reservation. Do not execute remaining forwards after a known failure. Failed receipts record attempted/completed counts, exact partial samples, false measurement_complete and false candidate_numeric_resource_passed. Full20-sample p95 and maximum exist only after all twenty valid calls complete. Pure injected-observer controls cover overrun on the second call, first-call RSS refusal, exact-boundary acceptance and truthful completion. No production clock or deadline changes are introduced.

This primitive retains actual ServiceLease/NativeInferenceOwner and ordinary Scorer admission. It does not infer semantic quality, calibration, release eligibility or composed service fit. Raw-only Rust-created profiles remain unqualified, without targets/downloads; product distribution remains separately reviewed.

The actual exclusive native ServiceLease and NativeInferenceOwner remain alive across load, all forwards, observations and cleanup. Input descriptors bind Rust-emitted unqualified profiles and authenticated numeric fixtures; they do not grant compute or source authority.

Only private receipts retain live paths or source/request data. Product fixtures require separate licensed distribution review.


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


## ContextLarge hardware requirement correction

The actual ContextLarge FP32 CPU8 run passed all three all-live numeric controls, but the first measured2048-token/eight-option call took30,585,085us, exceeding the unchanged30,000,000us product bound. It stopped after1/20 measurements, with a naturally settled nonzero test exit and no source drift or forced termination. CPU8 is not a qualified fit. The failed artifact, profile, command and receipts remain immutable.

One new Root-declared ContextLarge hardware candidate requires at least16 physical cores, intra16/inter1, sequential/no-spinning, retaining40GiB reservation and48GiB/1800s job hard limits. This changes the hardware eligibility requirement; it does not relabel the CPU8 result. CompactCPU4/16GiB remains under its original passed identity. No model, precision, token/card shape, numeric1e-3 threshold,20-sample requirement, first-observed-failure stop or original30s deadline changes. Genuine composed service admission and independent semantic/platform gates remain required. No NN is authorized by this preparation.
