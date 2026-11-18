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
Each Windows x64 GPU Profile and requirement row binds exactly one compute class
from SM86, SM89 and SM120. The SM86 product compatibility digest retains its
original bytes. SM89 and SM120 have separate compatibility digests and require
their own evidence-backed Profile and requirement row. The held public
provenance and package projection both require the exact configured architecture
option `86` or `86;89;120` and reject a manifest row outside that build's set.
This binds the declared class to authenticated configure metadata. The separate
new-build review inspects actual provider kernel targets before qualification.
The historical /6 proof remains scoped to its original producer identities.
Unknown schema, target, SM, build, provider, math, placement, resource and
source rows refuse. Signed held System32 driver identity remains an actual
observation. CPU encoding, execution and forced CPU continue through their
existing paths.

A non-Deserialize DeviceEligibility is created only by actual native owner
completion of bounded per-host admission under the existing management
warm/probe original<=120000ms deadline: authenticate exact artifacts, actual
qualified compute class and signed driver/modules. Check real free capacity against qualified
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

## Core-owned provider bridge initialization

Before explicitly mapping onnxruntime_providers_cuda.dll, the held native loader
uses the same pinned OrtApi's CreateCUDAProviderOptions, UpdateCUDAProviderOptions
with zero entries, and ReleaseCUDAProviderOptions. Create alone does not load the
provider bridge. In pinned ORT1.30, zero-entry Update invokes the owning
ProviderLibrary path: initialize the shared DLL and its genuine ProviderHost,
then load CUDA and initialize its kernel registry. Empty parsing performs no
device-id validation, factory creation, cudaMalloc, session creation or inference.
This is a provider initialization operation, not a GPU-fit or execution claim.

Full payload/PE/System32 authentication, baseline capture and restricted DLL search
precede this call under the existing Attempt custody and original deadline.
Reconcile every resulting mapping immediately after the API operation, then take
the loader's existing explicit CUDA module reference and reconcile again. Every
created options object is released on update error or panic; creation failure
cannot proceed. Any initialization/reconciliation failure poisons and retains the
existing held file/module custody until process exit. No fabricated ProviderHost,
private C++ symbol, second core, ambient search or session-builder bootstrap is used.
The DLL-load diagnostic retains the attempted absolute module path and OS error.
This changes native loader behavior/build identity, not runtime-manifest bytes,
provider DLLs, model formats or numerical/resource bounds. Historical failed
1114 evidence remains under its original image identity.


## Scientific NVIDIA dispatcher companion closure

The reviewed Windows scientific driver row binds System32/nvcuda.dll SHA256
f654977a2d34156c861a1434bbbbd1e9173b8d28b7e1c3f415bebca404df42be,
4466920 bytes, to the exact System32-relative companion
DriverStore/FileRepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvcuda64.dll,
SHA256 4cf09882e346099361d9bc1e125d34f076d36bdcf5838da88132e74d1af3d5c6,
29713128 bytes. This source-owned closure is separate from RuntimeManifest/1
identity and changes no persisted shape. It never grants product admission;
product discovery of driver companions remains unimplemented and fail-closed.
A different scientific dispatcher/package/companion requires a new reviewed row.

Installed oem67.inf equals package nv_dispwi.inf; its copy sections map nvcuda.dll
from nvcuda_loader64.dll (the exact dispatcher bytes) and include nvcuda64.dll.
Both files have observed trusted Windows Hardware Compatibility Publisher
signatures and version32.0.15.9706. Explicit membership in the particular
NV_DISP.CAT was not verified and is not claimed. Native held-file WinVerifyTrust
must independently succeed; an observation from PowerShell cannot replace it.

Before baseline capture or any driver/provider load, authenticate both exact
hashes/sizes, held-file signatures, and exact companion path. Traverse from the
authenticated System32 anchor through each named directory using retained,
non-reparse OpenedDirectory handles and check each effective trusted owner/DACL.
The companion remains in Attempt/LoadedRuntime held-file custody. Recheck the
original deadline after signature work and before loading; no new clock or limit.

The pinned dispatcher PE imports are exactly advapi32.dll/kernel32.dll, with no
delay imports. The companion imports exactly advapi32.dll/gdi32.dll/kernel32.dll/
shlwapi.dll/user32.dll/version.dll, with no delay imports. Authenticate the missing
explicit System32 imports using existing held-file/security policy, deduplicating
against manifest-owned files. No other NVIDIA static companion was found. Dynamic
or future unlisted dependencies still refuse rather than receiving a directory,
basename-prefix or trusted-signer wildcard. nvcuda64.dll is also a GPU-sensitive
baseline name: an existing wrong-path/identity mapping cannot bypass admission.
Unchanged reconciliation requires exact held basename, final path and object ID;
provider bootstrap ordering and failure retention remain unchanged.


## Staged complete mapping refusal diagnostics

Mapping admission remains exact: held approved name/path/object ID, or unchanged
baseline name/path/object ID/address. No crypto/system image receives new approval.
Every failed reconciliation reports all unapproved images at that stage rather
than stopping at the first. The private error-chain JSON schema
`airk.private.cuda-module-refusal/1` contains stage, unapproved_count,
reported_count, omitted_count, complete and mismatches. Each mismatch includes
actual name/final path/address/full native object ID, related baseline and
approved expectations, and classification new_image, address_reload or
path_or_identity_drift. Expectations are related by name, path, object ID or
nonzero address; they remain observations rather than authorization. Rows sort by
name/path/address/identity for deterministic evidence. Original predicate results
alone decide admission; diagnostic generation cannot turn failure into success.

Diagnostic encoding is bounded to1MiB independently of the unchanged module
census bound. If full rows do not fit, the deterministic prefix is retained with
complete=false and explicit omitted_count; this is incomplete diagnostic evidence
and remains refusal. Paths and IDs in included rows are never truncated. Initial
baseline GPU/core refusal uses the same report. Stages identify baseline capture,
driver load, public provider bridge, each explicit manifest load, post-cuInit,
runtime recheck, adapter observation, synchronization and final placement.

cuInit is now immediately followed by original-deadline checks and reconciliation
before LoadedRuntime publication. A newly mapped unapproved image at this boundary
fails inside the existing Attempt: owned DLL references and held files remain
retained and the process is poisoned. No runtime/model admission or retry follows.
The runtime-only diagnostic caller may retain the complete error chain without
calling adapters, primary-context retention, a provider factory, session or model;
a diagnostic refusal is never a component/qualification pass.


## Exact seven-image scientific cuInit closure

The preceding single-companion policy is extended only for the reviewed
scientific dispatcher f654977a.../4466920 and exact nv_dispwi package. The first
runtime-only diagnostic observed six genuinely new images at after-cuInit;
its complete6/6 report had no omissions and no model/context dispatch. All six
current held object IDs matched that native report, and native-equivalent
held-file WinVerifyTrust verify/close returned zero for each. This does not prove
future context/session/model lazy dependency closure.

Before any native load, the scientific owner authenticates all seven exact
System32-relative rows below, including the original nvcuda64 companion. Every
row requires exact path/hash/size, protected retained parent chain and independent
held-file signature verification under the original deadline. Missing, duplicate,
partial or changed rows refuse. All seven handles transfer into existing Attempt
custody together; no row is authorized by a basename, System32 membership, valid
signature alone or recursive import discovery.

| System32-relative path | Bytes | SHA256 |
|---|---:|---|
| DriverStore/FileRepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvcuda64.dll | 29713128 | 4cf09882e346099361d9bc1e125d34f076d36bdcf5838da88132e74d1af3d5c6 |
| cryptnet.dll | 265920 | 440af621e97fd5fcb93f06393b8f3fd5af936de550e763380cc1f4c71cd6c795 |
| drvstore.dll | 1591312 | f2bd8b7cfa74c7f001262ebc1fd909e48e9c377b682fa38952eb104b3130e725 |
| nvapi64.dll | 5533456 | 08ead220e3c0d1deda35473e7fc24614cf7fcdffec4941213514440312da1750 |
| DriverStore/FileRepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvdxgdmal64.dll | 626408 | cd27e222beaa5602190c614f1378f0cefacdc2dd571df3696371ee7c2418a675 |
| ole32.dll | 1716408 | 83f1f855f2985785a45976a6a5e2aacdc8900f61e373978668f5fa3456f09a66 |
| wldp.dll | 482720 | 2fd7baf055f1cd968ad1c783f7414d8aaa24ef93dc25fb1eeabc60afa3e5d4ea |

The unchanged installed INF maps nvapi64 to System32 (its package copy is byte
identical) and nvdxgdmal64 to the same DriverStore package. Both NVIDIA files are
WHCP-signed; the four Windows files are Microsoft Windows-signed. Direct PE
static/delay import inspection found no additional NVIDIA dependency. Those
rosters are observations, not permission to recursively admit Windows imports.
The existing six explicitly reviewed static nvcuda64 System32 imports retain
their preceding policy; no new generic System32 allowance is introduced.

In scientific mode only, baseline mappings with any of these seven row basenames
must match the held approved identity and path. A preexisting shadow OS image
cannot bypass the exact closure. Product mode never consumes this row; ordinary
OS baseline policy is unchanged there. nvapi64 and nvdxgdmal64 are additionally
recognized as GPU-sensitive names for baseline refusal in either mode, without
granting product driver discovery or authorization. All mapping reconciliation,
post-cuInit refusal/poisoning and diagnostic completeness semantics remain intact.
The next validation is runtime-only admission before any model attempt.


## Unsupported-platform compilation and pure metadata controls

The Windows x64 runtime constructor remains the only production CUDA descriptor
constructor. On other production targets RuntimeDescriptor is uninhabited; its
shared build/product/fit/requirement read signatures remain available and use an
exhaustive empty match, without fabricating a digest, fit or product decision.
Windows and unit-test descriptor variants and validation bodies remain unchanged.
The non-Windows native factory still refuses before reading runtime paths or
creating a runtime, even with an actual retained native service lease and compute
scope. A dedicated test retains that real lease and observes the typed refusal
against absent input paths; it creates no provider, session or model.

Compile platform-only provenance and private descriptor constructors using the
existing Windows-or-unit-test boundary. This preserves all27 pure provenance,
projection and checkpoint controls across hosts. Windows-only Device::poison is
compiled with its three Windows call sites. Shared eligibility/read algorithms,
policy digest bytes, scientific metadata preparation and unsupported-host public
refusal APIs remain intact. In particular RuntimeManifest::validate and its
payload/name helpers remain available with test-support on every host; product
manifest construction/validation and generic descriptor parsing require Windows
or unit tests. No dead-code lint suppression or new serialized representation is
introduced. Normal gpu-cuda library builds without test-support and all-feature
library/test builds both require scoped Clippy; the latter also runs the retained
pure metadata/refusal regressions.

RuntimeDescriptor::approved is Windows x64 only, matching its sole native loader
caller. Cross-host unit controls parse descriptors directly and exercise compiled
approval through RuntimeManifest::approved; they do not consume the Windows
descriptor approval wrapper. Its function body and Windows behavior are unchanged.

# Disposable CUDA session placement diagnostic

Private test-support only. Normal Scorer open/profiling retains CUDA-only provider
registration, disable_cpu_ep_fallback=1 and complete strict final placement gate.
The diagnostic requires a current native evaluation lease, unqualified Compact
FP32 profile and same admitted device/bounds; it cannot accept release authority.
It retains exactly the existing graph/external-byte/large-initializer/ABI setup.
Only its private OpenMode permits disable_cpu_ep_fallback=0 to inspect actual
partitioning. No CPU exception or hybrid policy is thereby admitted.

A successful diagnostic observes a complete final SessionState map, explicitly
synchronizes, drops the actual SessionOwner and synchronizes/checks poison and
pending claims again. It returns data before Scorer construction. No Session,
Scorer, run callback or executable capability is returned. Failed creation,
partial placement or uncertain settlement retains error/log/observation evidence
and existing unknown-work custody; Drop alone never proves release.

An opaque cloned log observer survives consumed ExecutionProfile/Binding failure.
It carries data only. Existing 65536-byte total and 4096-byte formatted-entry caps
remain. Selected CPU preference/missing/unsupported-kernel/assignment diagnostics
retain location and message; seen/selected/retained/omitted/bytes/truncated fields
make selection and loss explicit. Unselected optimizer noise counts as seen,
not omitted. Logs never substitute for the final same-core placement collector.

Native ignored selector cuda_load_only_placement_diagnostic returns private
schema airk.private.cuda-placement-diagnostic/1. It retains actual PID, binary,
request/profile/source/runtime pins, original120-second expiry, process16GiB,
Job24GiB, native host reservation, control/host release and poison observations.
It creates no operation journal and dispatches no forward. Session load is explicit.
Diagnostic complete requires session creation, full placement, untruncated selected
logs, observed pre/post teardown synchronization, pending claims zero, no poison,
actual host/control release and original deadline. Failure never becomes a pass.

The owning source transition explicitly advances current runtime.rs identity
without rewriting legacy CPU metadata, task/compiler identity or qualification.
Inverse textual replay of the reviewed runtime delta must reproduce prior9e.
G5's old9e->DDE supplement must be rebased onto this new source rather than copied.
This diagnostic grants no execution or precision admission, qualification or
publication.

A caught SessionOwner Drop panic means unknown native release even if a later
synchronization succeeds. Before that observation the diagnostic invokes the
existing device poison/epoch owner and retains a Binding clone; the native
Custody then observes actual device poison and retains the owner, lease, charge,
control and files. Teardown.poisoned records that device state. The separate
process_cuda_poisoned field remains the actual global runtime-loader observation;
per-device poison does not fabricate global loader poison. Neither can pass.

## Final graph metadata observation

A separate test-support graph-only diagnostic collects
`airk.private.ort-final-graph-metadata/1` through additive same-core export
`airk_ort_collect_final_graph_metadata_v1`. Existing placement-v1 and the strict
execution policy remain unchanged. Graph-only completion requires a complete
metadata inventory, exact full-node multiset agreement with placement-v1, original
deadline, actual session teardown and native custody release. Selected logs retain
their existing64KiB bound and explicit truncation fields; graph-only observation
does not require complete logs. The old placement diagnostic still refuses log
truncation. Neither mode grants a Scorer, forward or placement-policy exception.

Metadata JSON is compact and at most8MiB within the existing16MiB native result;
new native graph-only reports use bounded compact serialization. Limits are65536
nodes,262144 values,524288 ordered edges,4096 UTF8 bytes/string,rank16,64 attributes
per node and64 elements/typed attribute array. CPU-device immutable INT32/INT64/
BOOL constants alone permit up to64 elements each and1MiB total source payload,
always under the encoded budget. All unsupported, large or non-CPU constants
remain explicitly unread. Unknown types or outer bindings are never inferred
from names. No tensor transfers, full GraphProto or weight materialization occur.

Final Node indices and SessionState value IDs bind ordered explicit/implicit
inputs, outputs, graph boundaries, attributes, dimensions and constant provenance.
Consumers derive from the complete observed edge inventory. Both producer-to-edge
and edge-to-producer relations are checked. Callback cancellation, partial counts,
unknown identities, malformed ordering, duplicate records and nonfinite or invalid
UTF8 attributes refuse complete evidence. Original loaded-file security/reconcile
fences and poisoned-custody retention apply unchanged. A bounded C ABI record has
independent size/alignment/offset checks in both owners; no C++ object crosses it.

The optional assembly incremental_proof is a separate authenticated producer
mode, mutually exclusive with checkpointed_proof. It validates fresh static-core
ancestry through the existing provenance owner; old checkpointed receipts retain
their original identities and admission path. New core/public metadata must be
emitted under fresh artifact/source identities, never by retagging old receipts.
The incremental producer policy and independent fixtures belong to the
provenance owner and remain required for admission.

Observation capture failure or panic must not bypass session settlement. The
capture-and-settle owner retains that error, then always performs pre-sync,
SessionOwner Drop, actual unknown-release poison/retention if needed, and post-
sync. Capture locks are released before teardown. Recording teardown may fail,
but only after actual custody has settled or been retained; missing observations
never become release evidence.

## Incremental final-graph derivation

The new native derivation validator consumes separately pinned fresh raw producer and canonical proof/3. It authenticates the exact parent checkpoint using the existing unchanged validator, then proves the one translation-unit/object replacement, 39 retained session objects, 95 retained static libraries, exact provider pair, fresh session/archive identity, reviewed recipe/tool/dependency closure and naturally settled original bounded commands. No status boolean alone proves derivation. Fresh raw producer SHA is the new static_core_build; the ancestor is never reused as the child identity.

Assembly adds an omitted-by-default incremental_proof pin, mutually exclusive with checkpointed_proof. The native assembly and package projection share the same validator and retain custody callbacks through the original deadline. Historical request/public bytes remain unchanged when the new optional fields are absent. Public incremental ancestry contains hashes and bounded descriptive scalars only; checkpoint_history continues to describe the parent, never a cold rebuild or child qualification.

The independently authenticated maintained producer owns compilation and COFF archive member validation. Rust reopens all receipt/source dependencies consumed by its metadata proof, checks the exact integer/hash projection and before/after identity, and cannot admit an unknown raw schema, missing source, divergent provider, incomplete tree, renewed clock, changed retained member, missing final-graph export or a fabricated qualification claim.

New manifest/Profile/CAL/native-input/probe request and compiled manifest/static-build pins must be emitted from the fresh derivation. Old3179 metadata remains historical. No model execution follows from descriptive metadata success.

The maintained validator is fail-closed until its exact reviewed recipe authority is frozen; this is source implementation, not producer or runtime qualification.

## Fresh current-toolchain Windows assembly

Assembly recognizes the private cuda-fresh-static-build-proof/1 in the
existing checkpointed-proof slot by its exact schema. It requires a separately
reviewed current configure recipe and compile owner, directly held original
configure and compile evidence, the observed CMake/Ninja graph, all eight
source patch inputs and the same-build provider pair. The original raw compile
receipt supplies the static build identity. The produced assembly manifest
starts with an empty fit roster.

The acceptance path pins the reviewed Windows recipe
d10eb3ced57d17901e3749f05864e1f230b0d8988558a3ddaa98d72f61c1a863
and compile owner
9baab4661425582367f636c46d2a02871d43f8567d1b6c0e68445197e99813da
from the completed current-toolchain native build. The existing manifest,
package provenance and runtime activation gates apply.

## Private fixed-graph Compact FP32 scoring

The Windows x64 test-support entry opens the ordinary same-lease evaluation
session with profiling and accepts its exact reviewed mixed placement only
after the live complete graph matches the reviewed diagnostic's ordered rows
and counters. The reviewed diagnostic pin, profile manifest and active
adapter/driver identity are required. The private scorer owns the admitted
session and a separate policy identity. Strict execution, diagnostic, and
release paths keep their existing placement checks.

The gate accepts one resolved root graph with only CUDA and CPU providers.
Every present CPU incident value must be a scalar or fixed vector of at most
five INT64 or BOOL elements, up to 40 bytes per value and 34,131 bytes across
distinct values. CPU implicit inputs are refused because this ORT profiler
does not describe them. Each forward uses the native validation and device
lifetime path at 32, 128, 1024 or 8192 tokens under the original deadline.
The raw ORT profile is separate evidence for the caller to validate. Value
payload bounds do not measure allocation peaks or physical transfers.

### Compact FP32 resource measurement

The private resource open function reuses the same graph gate and the reviewed
22 GiB arena, 16 GiB warm-growth reservation and 2 GiB headroom. It leaves ORT
profiling disabled and emits a resource-specific policy identity. Resource rows
use T8192/N8 with all tokens and candidates live. Cold load uses the binding's
original expiry. The caller supplies the original expiry of each warm operation
and a separate fixed expiry for the whole experiment. A warm operation may
outlive the completed load's deadline and remains limited to 120 seconds and
the original experiment expiry. The normal native scorer owns synchronization,
claim release and teardown for both experiments.
