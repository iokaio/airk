# `airk_core::arbiter` contract

Owner: Core. Decision recorded 14 November 2026 under the user's authorization
to complete the advanced split; primary reviewed the static ONNX Runtime
1.30.0 choice. This is a new contract, not an `airk-state/1` migration.
Model and platform qualification remain separate from implementing this contract.

The private I1 `NativeNextReadRequest` adapter is owned by `service`; this
module owns its inert ScopeBinding, evidence records and read choices. The
adapter's worker identity and scoped authority do not create a dependency from
the pure Arbiter contract back to service dispatch types. This ownership move
preserves all fields, serde attributes and wire bytes.

## State and trust

`airk.arbiter.configuration/2` is a separate canonical JSON record beneath the
service-owned private control directory, `arbiter-v2/configuration.json`.
Its owner is Store; only authorized service management writes it. Bind the
record to the installed service identity, catalog namespace and monotonic
configuration revision. Enrollment names immutable IndexIds and supported task
versions. Absence means disabled; global enablement never supplies enrollment.
Renames retain enrollment; deletion/recreation/import cannot transfer it.
No existing index bytes, representation identity, state digest or install record
changes. Installation binding is the existing full SHA-256 HTTP installation
identity, without truncation or a second identity domain. The initial `/1`
prototype and `arbiter-v1` fixtures remain historical and are refused rather
than silently reinterpreted as `/2`. Persist no prompt, evidence, tokens,
features or model decisions.

State writes compare the expected configuration revision under the owning
management lock and atomically publish a complete canonical record. Decode
strictly, with bounded bytes/counts and no unknown fields. Restart never resumes
canceled or superseded provisioning. Verified assets may warm for the exact
saved choice; absent assets never cause startup or read-triggered downloads.

`airk.arbiter.catalog/1` is authenticated by an exact catalog digest compiled
into the distributed native image. A local or downloaded manifest cannot add a
trusted digest. The native release allowlist pins each complete manifest,
asset length/hash/role, approved source, license/notice, compiler, calibration,
policy, task and qualification identity. Initially unqualified profiles remain
visible but unavailable; acquisition alone does not add them to the allowlist.
Profiles describe exactly the two smaller and two larger choices in plan ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â§9.1.
Downloads contain data only, never code, DLLs, scripts or custom operators.

## Advice and bounded gathering

`airk.arbiter.advice/1` binds task/profile/runtime/compiler/calibration/policy,
installation/catalog/index, authorization digest, configuration revision and
exact publication. It retains the bounded input digest, original evidence
identities and locations, measured work and typed outcome. Outcomes are
accepted, abstained, unsupported, invalid_input and unavailable. Accepted false
is an answer, not a failure. Scores retain their native units; softmax alone is
never a calibrated probability.

Existing task versions remain separate: request_supported, retrieval_route,
recipe_choice, evidence_relevance and evidence_role, all under `airk.*.v1`.
Add `airk.context_next_read.v1`: choose one of the current native-verified read
choices or abstain. Add `airk.context_gap.v1`: classify the remaining information
gap or task-relative sufficiency with an explicit reason. Relevance does not
qualify either new task. A profile lacking independent task fixtures and
calibration returns unsupported; it cannot silently reuse a different head.

`airk.arbiter.context.request/1` accepts an information question and one fixed
budget: calls, rounds, fanout, input tokens, candidate count, output bytes and
deadline. It cannot enroll indexes, choose an arbitrary executable operation,
increase authority or change the selected model. Instructions are data.
The composed public read is `answer` (`airk_answer` internally). Existing
`context` retains its neighboring-lines contract. Management uses typed
status/configure/provision/cancel/remove/recover requests over the authorized
service control plane. A public read never exposes service-wide enrollment;
the answer reports application only for its authorized index partitions.

The advertised Answer input is the complete strict ContextRequest: `schema`,
`question` and `budget` are required. Its schema value is
`airk.arbiter.context.request/1`; omission is not a question-only/default-budget
API. The question retains up to 16,384 UTF-8 bytes, and every supplied budget
field is validated without silently replacing it. MCP's Rust schema and actual
public Unified-to-native SDK tests retain this same shape. Rust-emitted plain
examples and the full public input schema live in
`fixtures/split/arbiter/public_answer_cases.json`; stale generated views do not
define an alternative request contract.
Kernel owns finite recipes over canonical indexed search, definitions,
references, document evidence and verified test/example targets. It captures
the original scope/publication before the first read, rechecks each dispatch
and delivery, deduplicates choices and stops on no progress, refusal, staleness,
missing support or exhausted original budget. No recursive MCP or agent loop.

`airk.arbiter.packet/1` retains intact original excerpts/citations and full
canonical read receipts, explicit selected/omitted counts, coverage, gaps,
continuations and legal full-evidence routes. `airk.arbiter.context/1` adds
organized cited facts and qualifications, with model judgments visibly separate
from source facts. Every assertion identifies its supporting evidence. Exact
reads keep their complete existing results. Mixed indexes remain separate;
excluded partitions never enter optional tokenization, scoring or caches.
Each retained record contains the exact dispatched canonical arguments and
intact canonical result; its identity binds both. The existing 64 KiB native
reply limit remains in force. Internal canonical reads use at most 4 KiB each;
an oversized composed packet omits whole records and their dependent paragraphs,
with exact omission counts and `budget` stop. It never removes fields from a
retained native result. Additional indexed reads take precedence over an
initial search receipt when whole records must be selected to fit that frame.

## Installation identity decision (2026-11-14)

Configuration `/2` and all Arbiter scope bindings use the exact existing
`HTTP InstallationBinding.installation` SHA-256 identity and exact catalog
namespace. This is neither the I4 installation Incarnation nor a truncated or
newly derived identity. Native adapters must validate the protected control
root and matching current catalog/index authority before supplying this binding.
The unpublished `/1` prototype used an Incarnation; its retained fixture is
historical and must refuse ordinary `/2` decode. No automatic rewrite or
identity reinterpretation is permitted. Existing airk-state/I4 bytes do not change.

I1 uses `Arbiter { request: ManagementRequest }` for owner-only management.
The separate native read application/scoring calls carry the actual redeemed
`WorkerIdentity` and `ScopedReadRef`, including its installation lease generation.
They never accept a public fingerprint as substitute authority. Application
returns only that index's `IndexApplication`. A score request binds the original
`ScopeBinding`, question, complete canonical records, closed verified read
choices and remaining original milliseconds. The service validates its own
installation/catalog and installed scope before admission, dispatch and delivery.
It returns typed `Advice` or an explicit unavailable/unsupported outcome; no
management configuration or other enrollment is present in the public answer.

## Runtime and lifetime

Retrieval owns compiler/tokenizer/full scoring and exact per-profile ONNX ABI.
Static ONNX Runtime is pinned to 1.30.0; the existing Rust wrapper is retained
only after C API compatibility evidence. Intern uses its trained decision-symbol
LM head at compiler-selected marker positions. Strands/Kev require their trained
adapter and pointer head. Generic decoder logits cannot substitute for those
heads. Per-profile manifests and independent reference fixtures pin tensor
names/types/shapes, masks, positions, option order and raw score tolerances.

Inference owns one optional native execution at a time and a finite fair queue,
under the retained actual service lease. Dispatch reserves measured resource
capacity separately from ordinary MiniLM work. Configuration disablement revokes
queued work and delivery immediately; native calls retain pins and charges until
observed completion. Drained is acknowledged only when actual work completes.
Stuck execution opens the circuit; unloading or inventing completion is forbidden.
Model switch/rollback and explicit owned-asset removal use the same drain fence.

The additional CPU8 tier was approved and frozen before its measurements:
at least eight natively observed usable physical cores, intra-op eight,
inter-op one, sequential execution and spinning disabled. The CPU1 tier remains
separate and unqualified when its own measurements fail. No silent thread,
device or profile fallback is allowed. Omitted `cpu_fit` in old developer
qualification data means CPU1 only; product-qualified profiles require explicit
fit metadata. Runtime receipts expose the exact execution identity and physical
core observation. CPU8 qualification does not transfer to six-core Mac Neo.

The existing provider factory admits one opaque native owner, shared by its
ordinary and optional compute lanes. Its monotonic admission count remains one
per provider, not one per thread. The provider completion barrier joins both
lanes and counts both cached sessions, queued/running work and deliveries.
Optional source-authority callbacks run outside the optional queue lock.
The service resource actor serializes optional memory admission with ordinary
work grants. Native RAII reservations remain charged through actual execution
and session teardown; unknown fresh OS headroom refuses optional compute.

No new crate, executable, cloud service, sidecar, implicit enrollment, source
mutation or embedding-regeneration action belongs to this contract.

## Explicit compiled asset transport decision (2026-11-14)

Profile's optional `downloads` list is omitted when empty, preserving legacy qualification-profile bytes. It contains Rust-owned `AssetDownload { asset_name, parts: Vec<AssetPart { url, bytes, sha256 }> }`. A product compiled release requires a complete unique download row for every declared logical asset, including compiler, calibration, fixture and notices. Developer qualification may omit transport but cannot become a provisionable release profile that way.

Each ordered part is positive and at most 1 GiB, at most sixteen parts per logical asset, and has its own length/SHA-256. URLs are exact `https://github.com/iokaio/airk/releases/download/<tag>/<filename>` with bounded safe ASCII tag/filename components, no credentials/query/fragment/percent escapes/traversal. Part lengths sum to logical Asset.bytes; a singleton also has the logical hash. The native download transaction hashes every part and the entire concatenation before publication. Only the exact profile compiled into the native authenticated catalog supplies transport authority; local/downloaded metadata and candidate exports never do.

## Private evaluation admission decision (2026-11-14)

A `test-support`-only opaque EvaluationCatalog may be constructed by a private native fixture retaining its actual exclusive ServiceLease. It grants unqualified candidate execution through the same scorer, native lifetime, verified asset closure and canonical answer implementation. Derived execution capabilities retain that lease and carry an explicit evaluation identity; no wire request, configuration field, downloaded descriptor or qualification flag creates them. Product catalog admission and publication still require final qualification and complete compiled downloads. Evaluation receipts explicitly say evaluation=true and unqualified candidate; they confer no release qualification.

## Task and platform qualification binding (2026-11-14)

Profile adds omitted-when-empty task_policies (Task to native policy-source SHA-256) and qualified_targets (QualifiedTarget { target, execution_identity, receipt }). Existing prototype/developer profile bytes remain decodable without these fields and never acquire product readiness from omission. A ContextNextRead execution requires its exact native prompt-constructor source SHA; overall Profile.policy continues to bind calibration, resource and model policy. Short prompt aliases never replace native choice/record IDs in authority or Advice.

Each product target row names one supported exact Rust target triple, exact CpuFit runtime execution identity and that target's actual qualification receipt SHA. Product activation requires the running target's row; unknown targets refuse. Windows measurements never qualify macOS/Linux, CPU8 never transfers to CPU1 or six-core hardware, and evaluation capabilities remain explicitly unqualified. Native source validation rejects duplicate target rows and mismatched runtime/tier identities.

## Original native attempt and unobserved configuration (2026-11-14)

The internal NativeNextReadRequest has optional omitted read_attempt using the existing ReadAttemptBudget. The actual production native controller fills its original same-host monotonic absolute expiry before transport; service scoring requires it and intersects all remaining bounds before admission. Optional scoring uses one transport dispatch, never registry-busy replay. Metadata-only developer transcripts may omit it and gain no execution authority.

ScopeBinding.configuration_revision=0 explicitly means unobserved optional configuration, only on IndexApplication availability Failed, profile absent, and no Advice. The service first and last validates actual source scope and retains actual installation/catalog/index/authorization/publication identities. This sentinel is never persisted configuration, Ready, enrollment or score authority. A malformed optional configuration yields explicit NotApplied/repair guidance while canonical source evidence can continue only under its own live authority/publication checks. Typed source-scope failures always refuse.

## Canonical neighboring context expansion (2026-11-14)

NativeRead appends Context, retaining all existing serialized variants. Legal expansion is the canonical context handler at a verified hit's indexed start line, before=0, after=32, max_bytes=4096 (or the smaller original allocation). The owning context operation clamps at indexed EOF and preserves source-location fences. No guessed EOF, hidden file-info read or weakened strict read_file range validation is introduced. The planned target range is an upper bound; EvidenceRecord retains actual canonical arguments/result and returned span, including truncation/coverage. Distinct ranked hits remain distinct choices; exact duplicate targets deduplicate. Publication/source/deadline checks remain unchanged.

Task policy source identity uses framed domain airk.arbiter.native-task-source-lf/1 and LF-canonical Rust constructor bytes: replace CRLF byte pairs with LF only; reject lone CR. No other code, string or whitespace normalization is permitted. This prequalification decision replaces raw checkout-byte hashing; no qualified profile existed to reidentify. Actual compiled input digests remain exact byte/token identities.

## Controlled encoder comparison fixture (2026-11-14)

A separate test-support EvaluationScope may be entered only by the actual NativeInferenceOwner inside its ordinary ServiceComputeScope, retaining the actual ServiceLease and exact sealed model.json SHA-256. It bypasses only product catalog matching for those exact bytes; all normal asset, tokenizer, ABI, batching, attention-mask, pooling and normalization checks remain. The native model retains that lease through session teardown. There is no public flag or serialized evaluation authority.

Fresh original-weight fixture ModelObjectSets use the actual current ORT1.30 adapter plus their exact original weight manifests, giving distinct pipeline/vector identities from both old shipped-v1 objects and trained-v2 objects, in separate databases. Old saved-v1 sets cannot be relabeled or opened as current. Receipts distinguish original shipped graph weights on ORT1.30, original weights with the same new mixed-precision recipe, and trained-v2; failing old numerical seams remain disclosed baseline observations, never product admission.

Work.input_tokens and stage timings count observed returned compiled-work receipts. If one-shot optional transport closes after dispatch, actual native work/tokens may be unknown: the packet's Failed/NotApplied reason says so, timings remain absent where unobserved, and all further optional calls in that original attempt are disabled. Zero observed tokens never licenses replay or renews the original input budget; source evidence retains its independent original budgets. Native campaign receipts record actual fully observed vectors/tokens/timings separately.

## Literal next-read catalog decision (2026-11-14, before A6 final scores)

The same deterministic and model conditions receive one stable catalog of at most eight legal reads. Each verified ranked hit contributes its EOF-safe context choice, then source-literal search choices in returned byte order, interleaved before the next hit. Literal spans are maximal adjacent capitalized alphabetic word runs grouped into two through four words, separated only by original space/tab; at most eight spans per hit and 96 UTF-8 bytes per span. No stop list, title manifest, human labels, generated query or normalization applies. Exact case-insensitive dedup chooses the first native hit/span provenance; the actual argument retains original source bytes. The original question query is never replayed. A canonical literal search has limit=4, max_bytes=4096, full_chunks=true, saved collection retrieval mode and unchanged scope/publication/budgets. All source calls remain within the original 8-call/3-round/8-choice/T2048 limits (or smaller user budgets).

ReadChoice's existing opaque target string can carry versioned airk.literal-search/1 provenance: originating record ID, original indexed path/line, returned-hit UTF-8 start/end and exact literal query. Core owns its strict codec and source-span rule; public DTO fields remain unchanged. Actual full search arguments/results stay intact. A separate interpretation paragraph connects the source-span record and search-result record, so exact byte provenance survives without modifying raw canonical results. A duplicate/no-hit read skips to remaining eligible choices; it does not imply that unrelated choices cannot progress. Exhaustion still stops and never implies sufficiency. Task-policy identity combines the LF-canonical native prompt source and this literal catalog policy; new calibration transcripts precede final A6 scores. The independent A1 vector/10/full-chunks/32768 query freeze is unchanged.


The verified-memory loader preserves original ONNX bytes. For external tensors
at or above 2 GiB it uses ORT 1.30 AddExternalInitializers with typed OrtValues
created from the exact authenticated tensor slice and declared shape. The pinned
ORT implementation injects these values before file-in-memory initializers and
keeps large data as external memory, avoiding TensorProto's embedded-data cap.
The remaining authenticated blob is supplied through the existing memory API;
its pass skips tensors already replaced by in-memory external data. No source
path is reopened, model bytes are rewritten or precision changed. Admission
checks exact element width, dimensions, range and unique tensor name; all external references outside direct main-graph initializers and unsupported
large tensor types refuse. Current qualified-shape candidates use FLOAT
and UINT8 only. Native parity and resource gates remain unchanged.


Profile.compiler and Profile.calibration are raw SHA-256 digests of the exact
single Compiler and Calibration assets respectively. They are not parsed-JSON,
recipe or semantic-canonical digests. Profile validation requires exactly one
Model, Tokenizer, Compiler and Calibration role and verifies those two descriptor
bindings; notices and independent fixture files may have multiple entries.
Actual raw qualification inputs already obey this rule. No digest domain, asset
bytes, public shape or previously retained profile bytes are rewritten.


Private empirical evaluation materialization uses a new identity domain,
`airk.arbiter.evaluation-profile/1`, over compact Rust-serialized Profile bytes
with manifest zeroed, qualification None and qualified_targets empty. It creates
new profile IDs and ContextNextRead/current task policy/calibration-asset and
policy-evidence hashes without overwriting prototype or product identities.
The Rust-only fixture emitter reads a pinned reviewed hybrid empirical summary,
emits `airk.arbiter.empirical-next-read/1` containing only aggregate counts and
evidence hashes, and omits raw human question IDs, source text and per-episode
labels. Its rule is unique finite argmax with exact ties abstaining; it makes no
probability, sufficiency, authorization or final qualification claim. Original
assets are linked into a fresh private flat closure and every byte is verified
against its original length/hash; original profile/weight/fixture bytes remain
unchanged. Product catalog/qualification/download gates remain separate.


A resident session owns one qualified maximum-peak resource charge. Serialized
scores reuse that exact opaque resident permit; they do not reserve the full
model a second time. The resource actor checks actual owner/id/bytes, current
policy and fresh OS headroom, producing a nonserializable reuse witness. Resource
policy/checkpoint changes invalidate queued witnesses; native dispatch rechecks
the witness, fresh headroom and the original deadline/cancellation. Different
concurrently warming models still require separate charges. Ordinary work sees
the protected peak throughout native session/FFI completion.

The last actual reservation owner publishes release only after removing its
charged row. An opaque observer retains notification state, never the charge;
its bounded, cancellation-safe std Future has no polling sleep or Kernel executor
dependency. Lifecycle operations wait under their existing deadline for actual
session drain and this last-charge event before warming a replacement when old
and new peaks cannot coexist. Retained verified assets support an explicit old
configuration/session rollback; SessionNone alone is not a released-charge proof.


The existing service inference_threads setting governs ordinary MiniLM/embedding
jobs and retains its existing bounds/default. Optional Arbiter execution uses
the explicit chosen profile's fixed CPU fit (including the separately approved
8-physical-core/intra-8 tier), after global and immutable-index/task consent.
Status reasons disclose the optional profile's declared threads/physical fit;
resource controls label their existing setting as embedding inference threads.
Neither setting promises a global total-thread cap or silently changes the
other. No runtime/default limit, wire shape or fallback is introduced.


After the CPU8 Intern-4B resource failure (20 measured samples, p95 34.777394s
against the unchanged 30s bound), a distinct CPU16 large-model execution tier is
approved before any CPU16 measurement: minimum_physical_cores=16, intra_threads=16,
inter_threads=1, sequential=true, spinning=false. It preserves T2048/B1/N8,
30-second p95, 24-GiB large-model RSS and all quality/numeric floors. New profile
IDs/policy/manifest identities bind CPU16; CPU8 profiles and their failed evidence
remain unchanged and unqualified. There is no fallback or qualification transfer.
A separate native developer probe uses one warmup and three observations only;
it cannot produce a 20-sample resource qualification claim.
