# Native CUDA runtime custody and selected scorer

This additive owner is enabled only by gpu-cuda on Windows x64. CPU open and
MiniLM do not enumerate adapters or open optional DLLs. One CUDA-capable static
ORT1.30 core serves both CPU and CUDA. Loading another onnxruntime core refuses.

RuntimeManifest/1 is Rust-owned authenticated build input, not a Core permit.
It describes the successful producer receipt, exact static build, payload DLL
bytes/import+delay-import closure/order and an explicit System32 import set.
Only the SHA compiled as AIRK_APPROVED_CUDA_MANIFEST_SHA256 and static producer
SHA compiled as AIRK_STATIC_CUDA_BUILD_SHA256 admit it. Neither variable has a
default. Root may bind those only after reviewing the genuine successful C++
producer, PE census and same-source static/provider outputs. No available,
qualified or approved JSON boolean grants authority. An absent pin refuses GPU.
The native assembly factory emits unapproved observations only; it cannot add
compiled authority. No Python/TypeScript runtime DTO mirror is introduced.

Host's safe adapter retains the actual Arc<ServiceLease>. Its only unsafe
Retrieval bridge requires that exact lease and an already-entered ordinary
ServiceComputeScope. No client descriptor, advisory selection or scalar receipt
constructs a runtime/device/capacity witness. Witnesses have private fields and
no Deserialize implementation. Module/file custody may remain process-global
because ORT's static provider bridge retains provider function pointers; the
service lease itself is not made process-global. Device contexts and service
charges remain owner-held through real completion, synchronization and teardown.

Payload directories/files are opened no-follow with original ancestors pinned.
Actual held-handle DACL/owner checks precede execution; payloads are human-owned,
foreign read/execute grants are allowed, while foreign write/delete/DAC/owner
grants refuse. Actual owner/held-child integrity checks remain mandatory.
Every DLL is held with
read sharing alone, stream-hashed before loading and rechecked afterward. Native
PE64 imports/delay-imports must exactly match the admitted manifest and resolve
only to those authenticated payloads or explicit System32 imports. Loading uses
absolute paths and DLL_LOAD_DIR|SYSTEM32 only; no PATH/cwd/user DLL discovery or
diagnostic recursive probing. Any already-loaded GPU/core basename outside its
expected held object refuses. Loaded paths and identities are reconciled after
each load and before calls. The driver is the native System32/nvcuda.dll only,
non-writable by untrusted principals and trusted by held-file WinVerifyTrust.

Native driver calls observe API/driver version, every adapter UUID, CC, total
VRAM and current free VRAM. Context-scoped queries/synchronization use the exact
retained device primary context and restore the caller context. An observed
adapter/epoch is not model qualification. Missing driver, unknown state, changed
module/driver/adapter identity or failed synchronization refuses and poisons the
witness. No successful receipt is synthesized from optional library presence.

Cold-load claims reserve an explicit cold peak plus headroom against current
free VRAM and other not-yet-spent claims. After actual session loading and device
synchronization that claim ends: current free VRAM already includes resident
allocations. Warm work reserves only separately declared/measured incremental
workspace plus headroom. It never subtracts a full model peak again atop its
already-resident allocation. Approved phase requirements are exact-variant
native qualification inputs, while test-support probe bounds remain explicitly
unqualified. Arena limits are not total VRAM hard limits or peak measurements.
The external owning actor/qualification observer still enforces actual peaks.

FP32 CUDA sessions bind actual G1 DeviceExecution, runtime/compiled policy and
graph identity, use CUDA error_on_failure, TF32off and no CPU provider plus
session.disable_cpu_ep_fallback=1. BoundedHybrid and FP16 remain refused in this
first implementation. Complete actual final SessionState placement plus kernel/profile proof is
required; verbose logs or API24 basic/minimal maps cannot substitute. The
AIRK-only same-TU C API extension receives Session's public AsPointer, checks
the exact same OrtApiBase and walks final GraphViewer nodes plus child
SessionStates through the owning public accessors. It shares the same static
core and uses no guessed vtable offset, weighted save or second ORT core.

GPU score requires the caller's original Instant deadline through
score_with_deadline; legacy score is CPU-only. Every run result, including an
error, is followed by actual device synchronization before claim release or
CPU fallback. Deadline expiry never creates a new budget. A failed wait poisons
and retains the outstanding claim; the owning actor must retain the resident,
host charge and service lease until actual teardown settles or the process ends.
A late successful wait may prove settlement while still returning a timeout.
No new release capability or G4 qualification is claimed by this module.

The first compiled compatibility policy is Windows x64 SM86, driver API>=12080.
A later driver image/API or adapter UUID requires fresh exact variant evidence;
compiled qualified fits pin all three actual numerical/resource/placement
records distinctly, plus the adapter/driver, immutable execution and measured
host warm-extra requirement. Missing fit/evidence yields no qualified GPU.
Zero incremental bytes are permitted only as explicit measured/declared values;
absence is not zero. Headroom remains explicit and concurrent claims preserve
the largest outstanding headroom. Runtime DLLs and manifest are direct children
of the native image directory, as required by the hardened legacy provider
loader. Native System32 hardlinks may be legitimate; private payloads require
one link. Failed partial module loading retains held files and refuses reuse
until process restart. A failed primary-context release likewise retains the
real service lease and poisons the GPU runtime rather than inventing cleanup.

PE imports use a shared bounded metadata reader: at most1MiB headers and1MiB
per import directory, not multi-GiB DLL copies before host-model admission.
Native hashed bytes remain streamed under the original deadline. Before any
optional GPU mapping, the process DLL default search is restricted to the
native application directory and System32, so later dependency-internal basename
loads do not reintroduce cwd/PATH. This is process-lifetime hardening, not a
system/user environment edit, and requires affected provider regression checks.

The final-placement symbol is optional at Rust link time. Resolve it only from
GetModuleHandleEx(FROM_ADDRESS) for the actual statically linked OrtGetApiBase,
which must be the native main image, then use its exact typed private C header.
The GPU core's owning C API TU exports that function explicitly; old CPU cores
have no export and GPU admission returns RuntimeUnavailable before model work.
This preserves CPU --all-features linking without any CUDA build/dependencies.
The compiled producer/static-build pins and authenticated closure are still
mandatory: a version string or an available export alone grants no GPU runtime.

## R3 review corrections and first proof scope

Runtime code/data payloads use integrity protection, not confidentiality:
owner remains the human, effective foreign write/append/delete/delete-child/
WRITE_DAC/WRITE_OWNER/GENERIC_WRITE/ALL refuse, but foreign RX is supported.
INHERIT_ONLY ACEs do not apply to the current object; each actual child file's
own descriptor is checked. INHERITED without INHERIT_ONLY still applies. This
changes no service/store/data directory privacy rule and edits no ACL.

One active actual lease+manifest maps to one CudaRuntime and one adapter broker;
re-entry returns the same Arc. Another live lease refuses instead of creating
an independent book over the same free VRAM. Context restoration has an owning
RAII guard: push failure never pops, success owns exactly one pop even on panic.
Wrong/failed pop globally poisons before teardown and retains the exact primary
context/service lease; it never calls release under an unknown thread stack.

All newly mapped images, including arbitrary helper/renamed-core basenames,
must match explicit authenticated closure file path+identity. A snapshot of
pre-existing trusted process mappings is separate; its address/name/path/ID
must stay exact when reused. No prefix heuristic authorizes a new image. Any
unlisted driver/system transitive image refuses for explicit subsequent review,
not an automatic PATH/System32/DriverStore wildcard. GPU closure file custody
is held; unrelated pre-existing baseline images are observed, not permanently
locked by this module.

GPU transfers own the actual input Values and RunOptions/preallocated host
score Value in a ManuallyDrop guard through native return and real sync. On
error/panic the same guard synchronizes before dropping any transfer backing;
unknown work retains those exact buffers plus Session/device/lease. CPU score
keeps its original macro path.

prepare_fp32_probe_profile is a test-support-only metadata constructor. It
accepts pinned raw Rust-emitted RuntimeManifest bytes (bounded128KiB, validated,
fits empty) and current native8192/N8 CPU original c79/f8 Context metadata.
Manifest data is not a runtime/device capability; no CUDA operation or compute
scope is needed to prepare it. The actual native evaluation lease/device/build
must still bind inside the owning inference lane.

The constructor authenticates the CPU calibration asset/current task binding,
then constructs distinct /2 rules=[] plus complete four reasoned Unsupported
classes. It returns PreparedFp32ProbeProfile with the new exact calibration
bytes/name and updated profile asset/hash; callers retain/write them in fresh
output and update the actual source map before EvaluationCatalog admission.
Final native CAL/profile validation is mandatory and depends on Core's reviewed
/2 all-Unsupported semantics; historical /1 behavior is unchanged. The profile
keeps original weights/graph/tokenizer/compiler/ABI and CPU4/16 host fit, clears
qualification/targets/downloads, and frames a distinct GPU manifest including
actual runtime metadata pin and explicit probe bounds. Neither old CPU class
rules nor any CAL/release qualification transfers. No evaluation/lease/device
execution permit is created.

RuntimeManifest/1 exact UUID/driver-fit rows are the first A6000 scientific
proof scope only. Eventual user eligibility needs a separately reviewed native
compatibility-policy/per-user observation bridge; reference UUID is evidence,
not a universal product requirement. No other GPU/driver is qualified here.
Likewise the current producer sidecar is private fixture provenance. Do not ship
raw private build paths/log records; a Rust-owned public projection + approved
package contract is still required before distributable activation.

The first executable component scope additionally admits the unchanged selected
Focus5072 W8 graph with FP32 activations/static512/no-yes raw binary output.
Its fresh uncalibrated relevance metadata comes from the existing native
qualification owner, extracted once with unchanged serialization field order
and values. This is raw scoring only; no learned probability/action confidence
or CPU evidence transfers. Context still requires the native all-Unsupported
/2 artifact. Public CPU Focus legacy compatibility is a separate closed native
source witness, not GPU qualification or a Context policy exception.

Executable ignored selectors live in Inference software_v2/cuda_qualification,
which owns real ServiceLease/NativeInferenceOwner and optional host reservation.
Retrieval adds no dependency edge to Kernel/Store. process_poisoned is a read-only
actual native poison observation for failures before a device could be returned;
it never permits execution. The fixed-absolute operation journal and component
contract preserve genuine owned-process timing/custody requirements.

## G3: product compatibility and actual local eligibility (proposed)

Scientific RuntimeManifest/1 VariantFit and exact reference UUID/driver pins
retain their original scope/bytes. Product RuntimeManifest/2 has the explicit authenticated closure/requirements
fields from the G3 decision and a Core Notice Asset pin to the separately
Rust-owned public provenance projection; it does not ship the private raw
producer sidecar. It accepts no `fits`, adapter UUID or driver-image equality rule.
A new approved manifest/policy digest and GPU Profile identity are mandatory.
Unknown schema/target/SM/build/provider/math/placement/resource/source rows
refuse; initial actual supported class is Windows x64/SM86 only. Signed held
System32 driver identity remains mandatory actual observation, not a reference
card's universal allowlist. CPU encoding/execution and forced CPU remain intact.

A non-Deserialize DeviceEligibility is created only by actual native owner
completion of bounded per-host admission under the existing management
warm/probe original<=120000ms deadline: authenticate exact artifacts/actual
SM86+signed driver/modules; check real free capacity against qualified
conservative requirements and actual charges; native load; complete final
all-subgraph placement; one pass of independently referenced Context17/128/8192
or all twelve Focus512 cases; real synchronization and final deadline/authority/
capacity recheck. No per-user20/p95/peak/coexistence or semantic requalification
benchmark is introduced. Release/variant G4/G5 remains tied to tested hardware.

A different user UUID/driver-image hash is not itself a refusal or a passing
qualification. Neither raw JSON, saved receipt, reference A6000 evidence nor
metadata flags construct the live witness. Actual user peak/p95 stay unobserved
unless genuinely measured; these checks do not assert those quantities.
DeviceEvidence keeps its genuine release/variant scope; local diagnostics are
not rebranded as G4. No persisted permit/public config/status field is added.

The witness binds actual UUID/driver API+signed image/module/runtime/model/math/
fixture/catalog/configuration/joint-set identity. Recheck actual free capacity
and authority before dispatch, avoid charging backed residents twice, and retain
all custody until real completion. Unknown settlement invalidates acceptance
and never advances fallback. Private scientific /1 cannot mint product /2
acceptance; actual old failure/evidence remains immutable.

The accepted short scope is proposal57aacf58bc1a503a14f2574b0edb32162bca106d12bfd8c54e56c3b0cef6a1ce.
Product /2 requirements/data never mint DeviceEligibility. The real native
resident must establish it under one existing original management deadline;
query entry/delivery also fence the actual native configuration/source/joint
key. Reaccepting retained stages shares that same management cutoff and actual
sessions/charges; no duplicate loads, budget renewal or hidden Answer work.
Management liveness is checked only during establish, never retained as a
120s expiry on future Ready. Actual session teardown revokes the witness.

The private native acceptance fixture is airk.arbiter.device-acceptance-input/1,
strictly decoded and bound to the selected compiler/task/resource/trained-asset
closure and exact compiled Fixture Asset hash. Its complete twelve Focus or
three Context rows retain both independently produced same-artifact and
original-FP32 full score arrays. Native compiler readout validation precedes
any row dispatch. This is one fixed acceptance pass, never an input-derived
query or a twenty-sample benchmark. The original management authority is
sampled before/after rows and is not retained as a future Ready expiry.

Only the lane's current validation thread and exact approved fixture-row digest
may dispatch before acceptance. Other pending product scores refuse. A live
witness holds weak references to the actual session/device/state; teardown,
poison, changed native key, or revocation makes it stale even if a queue retains
its Arc. Numeric/complete observed placement mismatches are typed admission
refusals; malformed sources/fixtures and deadline/cancellation are not invented
provider faults. Actual CUDA allocation error2 and observed insufficient free
capacity carry the separate capacity class. No error-string matching grants a
fault classification. Incremental host bytes come from the authenticated
Binding; None refuses while explicit zero remains zero.

The /2 product runtime admission opens the path-free native public provenance
and every direct legal Notice under the existing held protected-file owner.
The test-support projection bridge implements the public projector's Source
callbacks with real held/no-follow hash and effective-owner/integrity checks,
then rechecks all held sources within the original deadline. It never changes
an ACL, ships raw producer records, or creates execution authority. The raw
checkpointed producer/composite schema is a separate pending strict bridge;
this implementation does not alias its status to the historical /1 producer.

## Explicit checkpointed scientific build bridge

The private RuntimeAssemblyInput adds an optional checkpointed_proof Core
ResourceFilePin, default omitted so historical request bytes retain their
shape. Without it the original producer status/provider branch remains exact.
With it only the separately owned strict native validator accepts canonical
proof airk.cuda-static-build-proof/2 and raw composite
 airk.private.cuda-checkpointed-build/1 with
PASS_CHECKPOINTED_PRODUCER_ONLY_UNQUALIFIED. The helper reopens the pinned
actual sections/source/ancestry/custody/natural closure, returns only verified
same-build provider rows and the actual raw-composite static identity. No
status alias or generic new-success acceptance exists. The scientific manifest
still has fits=[] and remains unapproved; no device/model proof transfers.

The assembly owner retains its no-write/delete handles for payloads and the
shared Source's exact raw/proof/section inputs through final recheck, under
the same original metadata deadline. A staged raw composite may have a new
pathname only when its bytes/size/SHA match the proof's original build_receipt.
Private raw timing/custody receipts never become distributable public metadata.
The ignored native request emitter accepts explicit
AIRK_CUDA_CHECKPOINTED_PROOF + _SHA256 pins; no data-language DTO constructs
the new field or invents canonical proof output hashes.

The test-support observe_runtime_package_projection accessor accepts actual
caller-pinned RuntimeManifest/2 bytes and the protected direct payload root.
It parses the Rust owner, opens/holds/hashes all exact declared DLLs, compares
actual PE import/delay-import metadata, validates the path-free public
provenance and all exact legal Notice assets, then rechecks every held input
under the same original metadata deadline. No ORT/CUDA initialization, driver
load, provider/session creation, model request, activation or eligibility occurs.
Scientific /1 descriptors refuse this distributable path. There is no fabricated
positive descriptor/proof while the genuine completed build is unavailable.

The opaque output schema airk.native.cuda-package-projection/1 contains the
actual descriptor {name,bytes,sha256}, static_core_build, and sorted files rows
{name,bytes,sha256} including that descriptor plus every DLL/public provenance/
legal asset. qualification=false and activation=false are descriptive scope,
never authority. A builder consumes the exact native projection and genuine
owning image/source/input/output plus actual parent process/Job settlement
proof; arbitrary matching JSON is insufficient. The ignored owning selector
emits a separate NN0 input/output/PID/user receipt but cannot claim its parent's
future process settlement. No Python RuntimeManifest DTO is introduced.

## Pinned metadata custody copies

Root-approved MetadataCustodyCopy is the Rust-owned test-support private row
{original:Core ResourceFilePin,copy:Core ResourceFilePin,bytes:u64}. It preserves
the exact original logical pin/path as ancestry. Source.read may consume a
fresh protected human-owned byte-identical copy of bounded inert metadata,
including script/source bytes treated only as pinned build metadata. Original
and copy SHA must be equal/nonzero, actual copied size exact/nonzero<=16MiB,
paths absolute/distinct/unique, and the actual held protected copy must rehash
and retain the same named object. Duplicate destinations/physical copy objects,
chains, unknown or mismatched requested pins and unused mapping rows refuse.
Windows case/dot-path aliases do not create additional mapping identities.

No Source.authenticate operation redirects, and no active executable/DLL/PE or
runtime payload path is aliased. Direct actual payload custody remains its real
held path/hash/PE proof. A protected metadata copy can be reopened directly or
through its one exact original logical pin, but never represents protection of
the old path, ownership adoption, a rewritten receipt or promotion of a failed
attempt. Source final recheck verifies every consumed copy/held source and
refuses any unused list entry under the original deadline. Existing ACLs and
receipts remain untouched.

The projection input and RuntimeAssemblyInput each add custody_copies, omitted
when empty to preserve historical private serialization. The standalone native
custody decoder owns airk.native.cuda-metadata-custody/1 {schema,copies}; the
ignored request emitter accepts AIRK_CUDA_METADATA_CUSTODY +_SHA256 together.
It consumes this opaque native input rather than adding data-language mirrors.
Copies must already exist under the approved protected user C anchor; this
native reader creates no copies or directories and never relaxes integrity.

A shared checkpoint validator may call Source.recheck midway through public
projection; that rehashes all files held so far under the original cutoff.
Only the outer complete assembly/projection emission checks that every declared
custody mapping has been consumed. This preserves strict final unused refusal
without falsely rejecting valid mappings for later legal/source inputs.

## Inert historical tool evidence

Root additionally approved a distinct historical_tool_copies list of native
HistoricalToolCustodyCopy {tool,original:Core ResourceFilePin,copy:Core
ResourceFilePin,bytes}. It is omitted when empty. Only the exact frozen R12
CMake (12,029,392 bytes, SHA3fe22eb02...) and Ninja (600,576 bytes,
SHAedccb4c55...) original logical paths and tool roles admit aliases. Their
original producer before/after seals and the maintained canonical proof remain
authority for historical identity; current original path protection is never
claimed. The actual protected copies are streamed/held/rehashed in metadata
scope only. Source.authenticate_historical_tool is the sole typed consumer;
its default is the original strict authenticate for direct protected tools.

Source.read, generic authenticate and active executable/DLL/PE/runtime/aggregate
operations refuse historical aliases, including after a typed use cached the
copy. Wrong/unknown role, SHA, byte count, original path, duplicate/cross-class
origin or destination, physical-copy alias, chain and unused mapping refuse.
No copy is loaded or executed, no original receipt is rewritten and no ACL is
changed. Exact known historical tool evidence is not executable authority.
The existing build.ninja graph remains ordinary metadata, not the Ninja EXE.

The standalone decoder owns airk.native.cuda-historical-tool-custody/1
{schema,copies}; the ignored emitter accepts
AIRK_CUDA_HISTORICAL_TOOL_CUSTODY +_SHA256 together. The generic historical
callback is invoked only after named actual producer tool fields and canonical
proof pins have been reconciled. Provider/runtime/aggregate paths retain direct
strict authentication. Both metadata and historical consumed-list closures are
checked only by the outer complete emission; intermediate shared rechecks
rehash files held so far. Actual aliases are current custody, not old-path
security or an upgrade of historical failed build outcomes.


Owning encode_metadata_custody and encode_historical_tool_custody helpers
validate the same native types and serialize the existing envelopes under the
same1MiB cap. Serialization grants no protected-custody or activation proof;
the held Source revalidates actual pinned copies at consumption.
