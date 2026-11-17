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


## Additive software-v2 resource/numeric evidence (/2)

D06/D07's Core ResourcePolicy is frozen before scoring:8192 shared/Context,
512 static Focus, N8 Context,120s/120000000us, twenty warm observations and
nearest-rank index18. The new qualification/software_v2 owner consumes the
Core ResourceRunInput with exact source-manifest, profile and file-map pins.
It retains current target, native binary hash, same-user identity, actual ISA,
CpuFit and Scorer runtime/provider/thread/physical-core identity. Historical /1
selectors, formats, limits and fixtures remain unchanged and do not qualify /2.

Separate ignored resource and numeric entrypoints keep missing current numeric
references from suppressing independent resource measurements. Actual native
ServiceLease/NativeInferenceOwner custody spans load, all score returns and
cleanup. Resource input is native-compiled exact8192/eight-all-live Context or
the authenticated near-cap static512 Focus pair. Load and one warmup are reported
separately; a known warmup overrun stops before measured admission. Twenty warm
samples use Core ResourceSeries. Every known duration uses u64 microseconds,
with actual process-lifetime RSS; every sample is retained before its validation
can refuse. A first overrun ends the series without further forwards or partial
quantiles. A native error retains attempted count and no fabricated zero sample;
known score-return error duration/RSS remains a separate observation.

The numeric template emitter performs no NN work: it serializes actual native
model inputs for independent reference production. Its missing reference fields
cannot pass. Current Context reference rows must be17/3,128/8,8192/8; old2048
rows never satisfy the last row. Focus retains twelve static512 controls. The
numeric runner checks current model/compiler/policy and separately pinned
producer/reference-asset identities, finite output/cardinality, full raw-logit
and normalized-mass diagnostics and the exact fixture argmax. Context exact-FP32
limits are0.001 versus both references; Focus retains0.25/0.05 publisher and
0.001/0.00001 same-artifact limits. Native-produced scores are never expected
references. Missing producer/reference evidence is unavailable, not acceptance.

These primitive observations establish neither human task quality, calibrated
terminal support, actual composed service allocation/end-to-end latency, load
fit on another platform, product RAM floors nor release/publication eligibility.


## Native original-reference asset metadata decision

The separate ignored software-v2 original-reference emitter performs zero NN
work. It consumes the current Core ResourceRunInput and a SHA-pinned private
`airk.private.original-numeric-source-input/1` containing only the selected
model and opaque Core ResourceFilePin values. Python supplies no Asset DTO,
role, byte count or logical name. Rust derives a fixed per-model filename set
and authenticated publisher SHA roster, streams actual regular non-reparse
files, and refuses missing, duplicate, drifted or cross-model inputs.

The emitted `airk.arbiter.original-numeric-source-assets/1` serializes genuine
Core Assets for the complete original safetensors checkpoint and tokenizer:
the first checkpoint file is Model, additional shards ExternalWeights, and
tokenizer.json Tokenizer. Required publisher config.json and (for Context)
model.safetensors.index.json remain separately measured original_source_files;
they are model configuration, not native Compiler, Calibration or Fixture
assets. Original_reference_files maps native-generated safe logical names to
actual path, bytes and SHA. All original_source_files must still be frozen and
sealed by the independent Torch owner. Publisher rights/full-source closure
remains independent evidence and is not replaced by this neural roster.

The roster binds model, current compiler/resource policy and the original
pinned request. Its emission grants no reference scores, execution custody,
qualification, public metadata authority or NN admission. An empty roster in
the existing numeric template remains unavailable until independently produced
references and actual original source/custody are supplied. Historical emitters,
fixtures and Core ResourceRunInput wire bytes remain unchanged.


The fixed neural/tokenizer pins derive from retained authenticated publisher
manifests: Focus research-asset-pins-full.json SHA412bba9ad1c10197ef0d2b67db20690b415194fe3473e3031a35e056509b9710,
revision e61197ed45024b0ed8a2d74b80b4d909f1255473; Context
research-asset-pins.json SHA4cac4fc751d996e987bb4701fb90ce8cbd600af06152151b4ea99bed8d6c664c,
Compact revision70d244cc86ccca08cf5af4e1e306ecf908b1ad5e and Large
revision1cfa9a7208912126459214e8b04321603b3df60c. Configuration/index SHA256
pins were independently recomputed after the complete original publisher
SHA/Git-blob closure check. Output records these provenance identities without
claiming that the minimal neural roster replaces that complete source audit.


## Original-operation external observation for software-v2 qualification

The test-only software_v2 operation module emits create-new, append-only
OUTPUT.with_extension("operations.jsonl") observations for resource load,
warmup0 and samples 1 through 20, or numeric load and each original full fixture row.
It has no production Scorer hook, new provider, caller signal or public API.
An event binds the pinned native run-input/profile/image, actual process ID,
strict sequence, kind/ordinal and exact compiled input digest where present.
The image is hashed once before operations, never once per forward.

The clock is the existing airk_process::monotonic owner: Windows QPC scaled to
nanoseconds; Unix/Darwin use the existing monotonic/uptime contract. Each start
samples one original absolute policy120s expiry and is flushed/synced BEFORE
FFI. Publishing that start consumes the original allowance. Expiry while
publishing refuses dispatch and emits not_dispatched; no signal can renew it.
Immediately after a native load/score returns, success/error and original
start/expiry/return clocks are durably retained. A failed observer cannot admit
further native work. A native error remains an actual returned error.

A matching external Windows watcher owns the same original absolute QPC
expiry, with the actual suspended-assigned direct-child process/job handles,
PID/image/SID and exact input identity. It observes complete journal lines by
native directory-change notification and process-handle waits. Notifications
or later records never refresh an active expiry. Missing end at expiry, wrong
identity/sequence/clock or changed journal prefix fails the execution; any
termination applies only to that exact owned Job. Actual exit handles and Job
accounting still prove settlement. A forced/unknown outcome cannot pass and
never fabricates native cancellation, reservation release or zero NN work.
A separate fixed whole-process bound covers preflight/compilation and the batch;
it is neither a new per-operation limit nor p95 evidence.

The new measurement identity is
native-original-operation-start-to-post-end-retention/1.
Actual full-operation elapsed is rounded conservatively upward to u64
microseconds; raw native-call elapsed remains separate in returned events.
The resource series therefore includes both durable observer costs, stops on
its first known deadline/RSS overrun and has no partial p95/maximum. Load and
warmup limits precede measured admission; twenty valid warm samples and sorted
index18 remain required. Numeric rows retain exact inputs/raw outputs and the
original tolerances, with each actual row retained before its deadline refusal.
A post-end-sync native clock check retains both observer flush costs. The external
watcher does not clear an active expiry merely on Returned bytes: it keeps that
original expiry until the next authenticated Started or the actual process-handle
exit. Thus a hung end sync cannot escape the original external cutoff. The same
conservative fence can refuse during inter-call Core/body retention, compilation
or final source rechecks after an actual end sync; this stricter observed-execution
refusal is separate from the measured p95 sample interval. Prospective Started
markers never prove an FFI dispatch or fabricate an NN count.

Old image/measurement identities and /1 receipts remain immutable and are
never retagged. Existing software-v2 NN0 metadata/reference input provenance is
retained under its original source manifest; a later patched execution image
needs its own actual build/source custody and explicit compatibility review.
No whole-source group, process image or source manifest is silently rewritten.
Unix source clocks remain portable; this Windows watcher does not itself
qualify a Neo external-observer implementation or any model/platform outcome.


When a native operation returns an error and any later clock, sequence or end
retention observation also fails, retain the original typed native error as the
cause and add the complete observation failure context. Failure remains closed;
no duration is invented when a clock fails and no subsequent call is admitted.
A post-call observer failure after native success records that distinction.

## Proposed Compact attention diagnostic profiling

Test-support may create one profiling-enabled scorer with the same native
profile/source admission, CPU/provider/thread/arena options and input guard.
Ordinary open calls the identical configured owner with an identity builder;
profiling and optimized-graph output are test-support APIs only. The diagnostic
keeps the original native operation journal/watchdog and actual ServiceLease/
NativeInferenceOwner through scorer drop and source rechecks. It records actual
ORT allocation/provider/type/shape data, not declared output names as proof.
No cache output becomes public and no zero-value dependency is added to scores.
One full8192 Compact forward is diagnostic-only; all-row qualification selectors,
weights/compiler/inputs, numerical/resource limits and source custody remain
unchanged. Old same-artifact seals cannot qualify a different graph. Synthetic
GQA operator controls are separate from model forwards and prove only actual
registered CPU kernel/allocation shape, never model or task qualification.


Private Compact attention proposal R3: the profiled full-model constructor uses
pinned ORT externalized optimized serialization (fresh weights basename, 1024
byte threshold). It must never use default Model::Save on injected multi-GiB
weights. Save time and memory remain inside the original 120 second load and
16 GiB native budget. Fresh graph <=64 MiB and external weights <=8 GiB are
checked before forward; the actual execution owner must observe these bounds
while synchronous serialization is in flight and retain all emitted bytes.
No profiled execution is authorized by source preparation. Ordinary Scorer::open
keeps its original builder and performs no optimized serialization. Tiny graphs
contain no learned/external weights and retain their separately bounded save.


Software-v2 private file reads reserve the actual observed file length exactly
before the existing bounded Take/read_to_end, matching production Store
PinnedAssets::read. They do not preallocate the caller's maximum or accept
geometric Vec growth as required model memory. File/reparse/size checks,
maximum-plus-one overflow/extra-byte detection, exact Asset length and SHA checks
remain. Allocation failure refuses normally. Historical /1 readers and model
math/weights/inputs/CPU/deadline/memory budgets remain unchanged. This removes
qualification-reader allocation slack only; it does not establish native model
fit. Original failed runs remain failures and new source/image measurements are
required before a new diagnostic attempt.


The private full-model profiling diagnostic now uses measurement identity
compact-fp32-gqa-profiled/2. Scorer::open_profiled applies only with_profiling to
the exact unchanged ordinary open_configured builder. It never sets an optimized
model path or save configuration, so pinned ORT uses ordinary !saving_model
initializer removal. The sole full-model caller records optimized_save.enabled
false, emits no optimized output record and refuses unexpected optimized files.
Existing tiny synthetic controls retain their separate optimized-save helper
unchanged. This removes a diagnostic-only retention mode; profiling still has
its own overhead and actual production load fit remains unmeasured until a new
owned run. Prior save-heavy refusals remain failures, not production OOM proof.

Original120s load/score and RSS/committed/CPU/input/source/numeric limits remain.
Actual28 CPU GQA kernels and all3 exact allocated output shapes/bytes are still
required. Candidate structural evidence and separately actual tiny optimized
attributes remain evidence; no full-model optimized graph/attribute snapshot is
claimed in /2. No task/model/release qualification is granted. Removing the old
save-heavy helper is conditioned on its single caller migration and clean
scoped compilation; immutable old image/run evidence remains preserved.


Private selected-model GQA diagnostic extension (proposal; no qualification)

The existing test-support-only same-weight metadata regeneration /2 accepts
exactly one optional graph override: retained compact_graph or additive
large_graph. /1 accepts neither; both together are refused. Rust selects the
original model independently of caller graph metadata. Compact remains exact
28-layer/Q16/KV8/head128/context2048, CPU4/16GiB. Large is exact
36-layer/Q32/KV8/head128/context4096, CPU16/40GiB; its backbone hidden2560 is
not its attention output4096. Large keeps its original 48GiB outer Job cap and
120s native load/score deadlines. No ordinary Scorer constructor or public
catalog/download authority changes. No full-model optimized snapshot is saved.

The new source-pinned Large graph rewrite keeps all original initializers,
external byte ranges, projection/QK-normalization/rotary/readout nodes and the
six-input/scores-only ABI. Only the reviewed attention subgraphs and their
exclusive literal Unsqueeze-axis constants may disappear. The exact original
Large graph is f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94:
398 original initializers (326 external learned Float weights, 72 inline),
36 original post-QK float32 scale bits f304b53d, no past/cache public input or
output. Post-RoPE Q and K transpose before flatten; Q is4096, K/V1024. The
original finite causal mask remains an input to the native GQA kernel. Generic
shape inference cannot substitute for native allocation/semantics evidence.

Native metadata emission requires the exact original8192/N8 baseline profile,
unchanged compiler/tokenizer/weights/notice/fixture physical paths and bytes,
unchanged CpuFit/memory/disk/ABI/task policy, authenticated graph/proof/settled
creation pins and a genuine matching native build receipt/source closure. A
new model manifest and rebound unqualified calibration never transfer old
qualification or same-artifact expected-array custody. All terminal classes
remain unsupported; no semantic/training admission is introduced.

Separate additive Large synthetic17/8192 controls preserve the Compact controls
and require actual CPU execution, private KV outputs, numerical group mapping
and native allocated output shapes [1,T,4096], [1,8,T,128] twice. The Large full
single-forward diagnostic uses exact original all-live8192/N8 tensors and
independent original-publisher FP32 arrays under existing raw<=.001,
mass<=.05 and unchanged argmax checks. Actual36 GQA events/provider/durations/
allocated output bytes and original process/Job settlement are required; the
structural proof alone cannot authorize inference or claim resource/p95 gates.
Original failed Large120s evidence stays failed. Full20-warm qualification is
separate from this one diagnostic forward.

## Private Large explicit-mask dense attention proposal

This additive test-support proposal keeps the ordinary Scorer/open_configured
builder and scoring body byte-identical. No provider, runtime, crate, public DTO,
wire shape, persisted format, qualification source or production fork is added.
Global ORT_GQA_DISABLE_FLASH_ATTENTION stays literal0; Compact's causal1 path,
prior Large flash controls, profile identities and failures remain historical.

The candidate graph uses the existing CPU GQA causal0 branch, preserving the
full original additive causal AND padding bias. Its exact native regeneration
proof/profile/source/asset closure is a separate Retrieval-owned gate. The new
full diagnostic requires modelLarge AND profile_id
private-large-fp32-explicit-mask-dense-diagnostic-v1. An old flash descriptor
cannot be called Dense, and the Dense descriptor cannot use the old flash
measurement identity. Native numeric/source/file admission remains unchanged.

New tiny Dense controls are only17/128 under the existing8GiB/120s toy owner,
CPU16/Q32/KV8/head128/context4096. Each size runs three separate sessions and
retains a separate actual profile/optimized tiny graph for each: (1) explicit
causal prefix mean; (2) zero-bias full mean independent of query position,
which MUST refuse a causal1 kernel; (3) explicit causal bias with key1 masked
and key0 always valid, checking the exact mean of allowed keys. Values contain
nonzero head/dimension and time sentinels. Only the padding-hole control adds
a literal+0.25 value spike at key1: correct masking excludes it without changing
the required mean, while ignoring that padding yields error above0.0001 for
every affected query at17/128. Other control vectors and all model math remain
unchanged; zero tensors cannot satisfy them.
Only tiny graphs may be saved optimized. Every session requires exactly one
CPU GQA event, positive native duration and all3 exact output allocations.
These tests have no learned weights, selected-model or full8192 authority.
The older FlashTiny8192 result, including failure, never certifies Dense8192.

The separate ignored full Dense8192/N8 diagnostic retains original all-live
native tensors and independent original-publisher FP32 arrays. It keeps actual
CPU16, inter1/sequential/no-spin,40GiB RSS/native reservation, original120s
load/score Journal deadlines, raw<=.001/mass<=.05/exactargmax, input/source
postchecks and36 unique CPU GQA events. Exact outputs are [1,T,4096] plus
[1,8,T,128] twice, totaling T*6144*4bytes per node. Full-model optimized saving
stays forbidden. Distinct receipt kind large_dense_attention_diagnostic and
measurement large-fp32-explicit-mask-dense-profiled/1 never relabel an old run.

Tiny tests and structural shape support do not establish full-model dense
correctness, memory or latency. Full original Dense8192 must actually execute
and pass its own unchanged gates; unsupported shapes or OOM/timeouts stay
refusals. Structural all1..8192 preservation belongs to the independently
validated native graph/source chain, not inferred interpolation of tiny results.
No task/data/training/CAL/release qualification is granted by this proposal.

## D08 typed device execution owner

The existing native lane retains distinct legacy configuration3 and device
configuration4; configuration4 is never decoded or projected as configuration3.
Actual admitted ExecutionProfiles and authenticated runtime/device witnesses
bind its selected variants. Advisory Core selection is never compute authority.
The exact catalog/configuration, adapter epoch and original deadline are checked
before dispatch and before delivery. All source/nonce/publication guards remain.
GPU score calls use the original operation deadline and synchronize actual CUDA
work before delivery or release. Unknown device settlement quarantines the
entire resident, source pins and host charge under the original native owner;
it cannot become a known zero-work failure, CPU fallback, or completed barrier.
No-GPU and unavailable-runtime builds preserve the admitted CPU route. Device
preference never changes the model; GPU precision preferences do not prohibit
an admitted FP32 CPU fallback. Known failed CPU variants are not fallback fit.


Actual typed GPU runtime/provider score failures alone may issue an opaque
native fault notification. The original read receives its measured failure; no
query is replayed. The key binds actual adapter UUID/driver API/runtime build,
profile/execution identity, device epoch and current catalog/configuration.
Source revocation, caller cancellation, exhausted time and compiler/input errors
do not constitute runtime faults. The notifier carries the exact original host
charge release observer; a future cached lifecycle continuation must observe
session/source teardown and that final release before a different admitted
variant can warm. Failed native synchronization leaves the provider barrier
incomplete and prevents notifications/recovery. No serializer creates a fault.


Runtime initialization itself holds a real host reservation on the existing
native lane. A cold product probe uses an approved compiled model host peak;
its one charge transfers into model warm, rather than being counted twice. An
already authenticated GPU resident may retain its existing full host peak for
the serialized shared-runtime probe; no second full peak is required. Every
probe holds that charge and actual ServiceLease until it really returns, even
if delivery times out. GPU model allocation/work retains its independent actual
device reservations. No uncharged DLL/driver initialization qualifies a model.

The distinct test-support evaluation method accepts a genuine same-lease
EvaluationProfile, explicit Core GpuProbeReservation, native phase-capacity plan,
actual adapter UUID, real host charge and the original deadline. It refuses
foreign leases, overflows, exceeded host/device bounds or release-qualification
claims. Its bound candidate remains explicitly probe_only and cannot enter
product selection/status Ready. Runtime/model qualification is still absent
until actual numeric/resource/placement evidence and the owning promoter pass.
The no-GPU build creates no device witness; it preserves the admitted CPU path.


Native device fault circuit identity excludes ephemeral witness generations.
Epoch remains mandatory for dispatch/reuse freshness, while actual UUID, driver,
static runtime build, execution profile and current document/source identity
retain the fault across wrapper recreation. A known settled fault can notify only
the bounded future-management owner; no current score retry or budget reset is
introduced. Evaluation probes require the actual matching ServiceLease, explicit
host/device bounds and the original deadline; generated metadata is no permit.

Private GPU component diagnostics are owned by
qualification/software_v2/cuda_qualification/CONTRACT.md. They reuse the actual
lease/compute owner and native resource charge, one original absolute deadline
and maintained watchdog journal. They emit Rust-owned private requests and
metadata before measured CUDA work; no scalar request or metadata grants product
admission, class support or release qualification. Unknown GPU settlement
retains the full real native custody. Historical CPU qualification and timing
records retain their original identities.


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

Product /2 observes adapter metadata without retaining every primary context.
It selects only one compatible dedicated SM86 adapter: greatest physical total,
native ordinal tie. Only that context is retained and its actual free capacity,
cold requirement/headroom and full eligibility are checked. Failure describes
that selected device, not every GPU. Other adapters are not initialized. Native
same-model precision/CPU choices keep the original budget; no selector expands.

Known-settled typed eligibility/load failure is also memoized synchronously by
the serialized native lane before reply/dequeue. The bounded16 memo reuses the
actual stable DeviceFaultKey; renewed management nonce, joint generation and
wrapper epoch cannot clear it. Every actual retained target and every queued cold
load is checked before native work. Product probe filters memoed variants. This
fences populations queued before the asynchronous Service recorder runs. Only
actual returned genuine faults under original live authority/cutoff and proven
settlement are memoized; source/fixture/auth/deadline/cancel/unknown are not.
Unknown custody/barrier is unchanged. Saturation fails closed without evicting
a row. Only relevant typed configuration/catalog/source scope change prunes it.
No new actor/lane/proof/clock or current-query replay is introduced.


The metadata-only component request emitter optionally carries the native
RuntimeAssemblyInput checkpointed_proof Core pin, omitted for historical input
identity. AIRK_CUDA_CHECKPOINTED_PROOF and _SHA256 must be supplied together.
The actual bounded native assembly owner delegates only this explicit branch
to the shared strict checkpoint-composite verifier and retains all held
source/proof/section/payload custody through original-deadline recheck. New
checkpoint target-section receipts never independently count as full producer
success; no timing/model/numeric/placement admission transfers.

Optional custody_copies rows follow the Retrieval native owner. The request
emitter reads only an exact AIRK_CUDA_METADATA_CUSTODY +_SHA256 native input;
empty is omitted. Copied old metadata remains ancestry, never old protection
or a failed-build success. Runtime payload/code authentication never aliases.
The package projection selector emits only descriptive NN0 bytes; the builder
must validate exact genuine native image/source/input/output and actual parent
process-tree/Job settlement through the maintained execution owner.

A separate optional historical_tool_copies list is emitted from exact native
AIRK_CUDA_HISTORICAL_TOOL_CUSTODY +_SHA256 data. Only named frozen R12 CMake/
Ninja evidence may stream a protected inert copy; no executable activation or
generic DLL/runtime authentication redirects. The opaque projection never
claims original tool-path protection or old failure promotion.

The owning package selector additionally obtains the descriptor filename from
Host::gpu_runtime::installed_paths and refuses any different supplied basename.
This shares the actual installed loader owner without a new dependency or
production filename literal. The lower descriptive/scientific accessor remains
generic; no metadata roster can accidentally stage an unusable descriptor name.


The private ignored emit_cuda_metadata_custody_inputs selector accepts a pinned
UTF8 locator file of six TAB-separated fields per nonempty row: class
(metadata|historical), role (-|named historical tool), original absolute path,
existing protected-copy absolute path, expected SHA256, exact byte count.
This is data location input, not a second native DTO. Existing Retrieval types
and owning encode/validation routines emit both custody envelopes. The owner
reopens each concrete original/copy with bounded no-reparse metadata readers,
checks expected SHA/length before and after serialization under one <=120s
original deadline, and writes only fresh output files. No copy, runtime/DLL/
provider activation or capability is created; the real held Source repeats
protected custody checks at consumption. Incorrect role, duplicate/cross-class
mapping, mismatch and expired deadline refuse. The process receipt records only
this actual metadata scope, not future parent Job settlement or observed NN0.
