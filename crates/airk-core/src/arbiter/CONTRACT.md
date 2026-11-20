# `airk_core::arbiter` contract

Owner: Core. Decision recorded 14 November 2026 under the user's authorization
to complete the advanced split; primary reviewed the static ONNX Runtime
1.30.0 choice. This is a new contract, not an `airk-state/1` migration.
Model and platform qualification remain separate from implementing this contract.

## Joint ContextAction foundation decision

Recorded current-version decision: the user explicitly does not require previous
version compatibility; native replacement/regeneration may be used when an upgrade
is unavailable. The current matched ContextAction request, local response and
advice are `/2`. Historical receipts remain facts under their original identities;
they are not reinterpreted or relabeled. This foundation does not promise older
packet decoding/migration and does not authorize deleting current user work.

`context_action` owns the inert `ContextRequestV2`, `ContextResponseV2`,
`ContextActionRoster`, exact source-unit references, whole-card hypotheses,
assessment history and validators. New `Task::ContextAction` is
`airk.context_action.v1`; `Decision::ContextAction` names one frozen native card.
It does not inherit `ContextNextRead` qualification, policy/calibration, prompt or
resource evidence. Old Advice cannot admit the new decision: joint advice uses
`airk.arbiter.advice/2` with complete native profile/compiler/calibration/policy,
scope/input/evidence/work bindings. Contract presence grants no execution authority.
The new native task policy binds LF-canonical retrieval task/roster-construction
source plus Core joint validation/calibration semantics via an owning digest
function. No cross-crate source mount or old NextRead policy rewrite is used.

One assessment is ONE joint scoring invocation over at most EIGHT TOTAL cards,
including reads and terminals. A card is a native-generated Read(choice),
Sufficient, Clarify or NoJustifiedRead, with a typed gap/reason and at most eight
source-unit role/relevance hypotheses. No model-supplied operation, argument,
path, pointer, evidence identity, arbitrary explanation or independent field
probability is admitted. At most one card per terminal action is permitted (three
terminals total), and one per verified read choice; changing an ID cannot duplicate
an alternative. Native frozen-roster membership and full selected-card
equality are checked before effects; its IDs/digest domain must bind question,
input, source basis, scope/publication and the new policy. The compiler/policy
owner, not a renderer or inference score, creates this binding and masks classes
without qualified annotation/calibration support.

EvidenceRef resolves within one exact scoped record's COMPLETE canonical result
to an actual Core Hit, with a nonempty exact UTF-8 text window and typed citation.
The supported pointer is exactly `/hits/<canonical nonnegative integer>` in a
Search, Context or ReadEvidence record. Nested/future metadata, leading-zero
aliases and other read operations cannot supply source units even when their
values happen to deserialize as a Hit; future support needs an owning contract.
Metadata/scalars cannot substitute for source units. This is structural validation,
not new source authority: canonical read/publication/citation owners establish the
record's actual provenance, current scope and source-read permission. Omission
counts distinguish available/presented/partial source units and available/scored/
omitted verified choices. The count universe is the native recipe's bounded
source-unit population, not all facts in the index. Full canonical results,
including unknown fields, remain unchanged in copies and exports.

Sufficient requires nonempty retained useful/complementary source support and a
matching reason; it is a fallible question-relative assessment, not exhaustive
coverage, empty gaps, a useful-read score or budget exhaustion. Clarify requires
qualified ambiguity/conflict semantics; an unanswerable label is not that proof.
NoJustifiedRead is forbidden while ANY remaining verified choice is omitted from
the scored roster, and speaks only about that current recipe. Read requires the
exact frozen verified choice and matching missing-need reason. Joint-only generic
Read permits precisely Gap::Unsupported + AdditionalEvidenceMayHelp: semantic
need remains UNCLASSIFIED, not NoUsefulRead/NoJustifiedRead/Sufficient. It does
not change the separate ContextGap task or invent Definition/Constraint labels.
Where typed need/role proof is absent the finite constructor uses this pair with
Unknown/Uncertain source hypotheses; semantic alternatives require their own
annotation/qualification. No additional option, scoring pass or budget is added.
Role/relevance are
whole-card hypotheses, not source facts; roles may have legitimate alternatives
or overlap, so training labels cannot fabricate exclusive human truth.

The original public Budget ranges are unchanged, including canonical rounds1..4,
16,384 UTF-8 question bytes and all existing maxima. Joint optional decisions have
the explicit policy cap min(3, requested rounds), not a fourth model invocation.
All partitions share ORIGINAL total input-token/call/round/fanout/candidate/byte
limits and the earliest admitted absolute deadline (at most30s). One joint
decision per assessment does not cap the entire answer at one assessment; further
assessments require actual remaining original budgets. No fresh duration after
queueing, forced follow-up, extra time reserve or compulsory second large pass.

The native service's private reservation ledger is keyed by the ACTUAL earliest
admitted attempt and ordinal, not a public request/digest/renderer ID. Bind each
ordinal1..min(3,requested rounds) once to its partition, frozen input/policy/profile
and original remaining allowance. Concurrent partitions cannot reset totals.
Unknown postdispatch work retains its charge/reservation and blocks more optional
decisions; cancellation/timeout never permits replay. Ledger admission, worker
identity and actual deadlines are future private native hook responsibilities,
not execution capabilities embedded in this pure public contract.

Nested assessment Work describes SCORING only: input_tokens and measured phase
timings are additive across distinct ordinals and must remain in aggregate Work.
Advice/2 REQUIRES reserved_input_tokens and observed_input_tokens (nullable).
It also REQUIRES task_policy, the exact new constructor/roster/task-policy digest;
policy retains its separate existing meaning as the authenticated Profile policy
asset SHA. Service checks task_policy against the selected profile's task_policies
AND frozen roster before queueing; Kernel checks returned task_policy against the
actual immutable roster. A single history cannot silently change task policy.
Work.input_tokens equals observed.unwrap_or(0), a known lower bound; None means
UNKNOWN, never measured zero. Accepted requires observedSome>0, known compile and
inference timing and actual use<=reservation<=original remaining tokens. Unknown
work retains its actual native reservation and prevents later learned assessment
or reuse. A compiled receipt alone does not release held capacity while actual FFI
completion is unknown. Known completion may reconcile exact tokens; the ordinal
remains used forever in the original attempt. Failure keeps truthful observations.
An observed token violation may be retained exactly ONLY in an explicit Budget
response with unassessed coverage and a final Unavailable(TokenBudget/Unavailable)
actual advice. It cannot permit another assessment, a successful learned terminal
or reservation reuse. This reports a violation; it neither widens execution
budgets nor caps/hides the actual observation to make it fit.
Known timings cannot disappear into None or overflow. A successful learned terminal
cannot retain over-deadline timing. Failed/expired/unknown work may actually overrun;
retain that measurement without a success/coverage claim, deadline extension or
permission to replay. Nested calls/rounds/fanout/candidates/bytes are zero: scoring
does not perform canonical reads. Kernel accounts canonical calls/retained bytes
additively and round depth/fanout/candidates as peaks once in response Work,
avoiding duplication of those counters in every scorer observation. Unknown
postdispatch cost remains explicit and conservatively reserved by the ledger.

Local history is bounded, append-only and ordered by sparse global ordinals;
each Kernel stores ONLY its own partition's subset, not a fabricated remote roster.
Selected events retain full card/advice; abstention/unavailable retain measured
work and explicit uncertainty. Only the latest local event can remain Current;
coverage references that selected latest ordinal in the same partition. Later
read/abstention/unavailability cannot leave an earlier sufficiency claim current.
All non-Covered terminals reject a Sufficient gap; learned terminal gaps equal the
selected whole card's gap. A read makes pre-read assessment historical;
new evidence is not automatically sufficient. Unit omission invalidates every
dependent paragraph/role/coverage projection, marks the event BasisOmitted and
coverage unassessed. Publication change applies existing delivery refusals. The
Kernel keeps exact source-unit/assessment dependencies for its native extractive
paragraph templates; a different unit at the same path/pointer cannot repair them.
Measured history remains historical, never reconstituted live authority. Packing
must preserve mandatory initial canonical evidence and cannot retain Covered after
removing its source support.

### Local and combined Answer ownership

Internal history may retain actual application cfg2 beside original record/advice
cfg1 ONLY with identical installation/catalog/index/authorization/publication,
unassessed coverage, no Current event or successful learned terminal. The cause
is HistoricalAfterConfigurationChange, never a fabricated read or rewritten
revision. This is NOT public-delivery permission: disablement/withdrawal still
revokes delivery immediately. Recheck owning application after native wait,
before actuation and at final delivery; stale Read/Covered/role projections never
actuate or deliver. An answer carrying any actual NN receipt across a revoked
outer cut returns a content-free nonretryable/nonreplayable stale refusal,
retaining known native work privately. Ordinary primitive/pre-NN-disabled reads
remain available. Missing/unavailable work is never relabeled measured zero.

The closed private I1 follow-up uses Core `native_context_action::NativeContextActionRequest`:
the REQUIRED actual ReadAttemptBudget AND actual public parent Budget, installed worker/ScopedReadRef, scope,
question, full records, complete captured verified ReadChoices, frozen roster,
global ordinal, optional captured private
PartitionAllowance and remaining tokens/ms. No missing-attempt fallback exists.
The complete parent budget is pinned even for a singleton: remaining NN tokens,
assessment ordinal and remaining_ms cannot exceed its original input_tokens,
min(3,rounds) or deadline_ms. A present partition's complete parent Budget must
equal the request's captured parent Budget; no512-token/one-round request may
reset to default2048/three later. Native service ledger pins that exact budget
across every partition/worker of the actual original attempt.
The service compares worker/authority to its actual installed lease, checks the
original clock/expiry and configuration/publication, and reserves actual attempt
plus ordinal exactly once BEFORE queueing. The optional partition is native-only;
its selection was validated by the existing whole-selection owner, not minted by
this local decoder. Unknown work keeps the one-use reservation; no retry or later
ordinal may reclaim it. A decoder structural pass cannot create a grant or claim
that the question/roster was scored. The closed response is ContextActionAdvice/2;
public generic control/CLI routes never expose this hook. Consumers follow this
recorded shape after review and native semantic cases.

Actual singleton path: MCP `airk_answer` -> `execute_verified_response` ->
`Kernel::answer_with_reader_v2` -> one local ContextResponse. The new local /2
response retains complete records, arguments, paragraphs, gaps, work, omissions
and applications, plus local assessment/history/termination. It cannot assert
whole-scope coverage for other authorized partitions.

Current selected mixed reads use frontend `combined_frontend::Backend::dispatch`
-> `execute_read` -> `read_member` -> Kernel `combined_reads::compose`. CURRENT
Answer is excluded from that multi-member admission list and the MCP combined
typed validator; thus mixed Answer currently refuses. Full mixed Answer is
REQUIRED by the plan and remains an explicit consumer obligation after this
foundation, not an invented already working property or a local-Kernel fallback.

The existing combined frontend owns the actual eligible partition roster and
original ReadAttemptContext. Its native service ledger must coordinate aggregate
budgets/ordinals; independently invoking full per-member budgets is forbidden.
Private disjoint allowances may contain zero optional decisions even though the
public canonical rounds lower bound remains1. No partition observes another's
content in tokenization/features/cache/scoring. Exactly-once member execution or
a reviewed private resume seam must prevent rerunning initial reads to reclaim
unused resources. The consumer owner must review that allocation shape first.

The existing native combined compositor owns any whole-answer summary INSIDE its
complete combined wrapper: selection, publication_vector, rank_policy, members
with full native results, ranking, shared_values, partial/truncated, failure and
read_retry, plus frontend continuation/verification/delivery fields. No parallel
outer response or flattened substitute is introduced here. Local Sufficient,
Clarify and NoJustifiedRead cannot cancel/relabel pending authorized partitions.
All currently Covered partitions may yield whole Covered; mixed settled outcomes
remain Partial with exact states; unvisited/pending/budget/deadline/failure and
omitted choices stay explicit. Unknown member effects/work never become zero work
or permission to replay the query.

### Foundation and qualification boundary

The approved follow-up calibration asset is
`airk.arbiter.context-action.calibration/1`, owned by `context_action::calibration`.
It binds exact ModelChoice, compiler, NEW task policy, complete ScoreAbi, and the
complete explicit asset-name -> role/SHA map for Model, ExternalWeights and
Tokenizer. The selected authenticated profile binds the calibration's raw SHA;
the asset cannot bind its own hash or profile manifest circularly. Missing/changed
weights/tokenizer/name/role/ABI/policy refuse before scoring. Up to32 closed rules
admit a WHOLE action/gap/reason/ALL role/relevance combination with a finite,
nonnegative raw top-runner margin. Evaluate every frozen offered card once; only
the unique GLOBAL top may be accepted. Ties, unsupported top or insufficient margin
abstain; never pick a lower supported card, remove scored unsupported cards and
renormalize, assert probabilities or inherit old task thresholds. Any native
planner filtering precedes roster freeze and discloses omitted legal choices;
NoJustifiedRead cannot dismiss them. No thresholds/classes are invented here.

Land reviewed Core types, schema and independent Rust-emitted semantic cases
before service/compiler/Kernel/frontend consumers. This substep does not activate
ContextAction, declare any optional profile qualified, or qualify mixed transport.
Current human support/answer annotations do not establish broad human gap,
ambiguity, role, relevance or sufficiency labels. Mechanical support episodes are
identified as projections; preserve alternative annotation families rather than
unioning them. Whole-card correctness includes every reason/role/relevance claim.
Unsupported classes stay masked and full A6 remains incomplete until independently
qualified. New native-consumed per-model policy/calibration and composed aggregate
resource/deadline/cancellation proofs are required; old one-pass or NextRead
receipts retain only their original measured scope and identity.

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
Profiles describe exactly the two smaller and two larger choices in plan section 9.1.
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
retained native result. The initial original-question canonical search is mandatory: it preserves the
actual retrieval population, including its full execution/readiness metadata.
If the first receipt cannot fit the original gathering retention allocation
(one half of `max_bytes`, unchanged), observation explicitly refuses before a
success packet can be finished. The initial receipt is never counted as an
omitted candidate that allows a metadata-only success. Exact-boundary receipts
remain whole; later receipts retain the existing counted-omission behavior.
Packing omits later search receipts newest first, then later other receipts
newest first, preserving the initial search and exact omission accounting.
If the initial receipt and remaining packet metadata alone exceed the escaped
native envelope budget, the operation explicitly refuses with a reply-budget
error; it never returns an apparently complete packet without that receipt.

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

Profile's optional `downloads` list is omitted when empty, preserving legacy qualification-profile bytes. It contains Rust-owned `AssetDownload { asset_name, parts: Vec<AssetPart { url, backup_urls, bytes, sha256 }> }`. A product compiled release requires a complete unique download row for every declared logical asset, including compiler, calibration, fixture and notices. Developer qualification may omit transport but cannot become a provisionable release profile that way.

Each ordered part is positive and at most 1 GiB, at most sixteen parts per logical asset, and has its own length/SHA-256. Primary and optional backup URLs use Core `download_origin`: exact `https://github.com/iokaio/airk/releases/download/<tag>/<filename>` or the reviewed immutable HF forms below, with bounded safe ASCII components and no credentials/query/fragment/percent escapes/traversal. Part lengths sum to logical Asset.bytes; a singleton also has the logical hash. The native download transaction hashes every part and the entire concatenation before publication. Only the exact profile compiled into the native authenticated catalog supplies transport authority; local/downloaded metadata and candidate exports never do.

## Private evaluation admission decision (2026-11-14)

A `test-support`-only opaque EvaluationCatalog may be constructed by a private native fixture retaining its actual exclusive ServiceLease. It grants unqualified candidate execution through the same scorer, native lifetime, verified asset closure and canonical answer implementation. Derived execution capabilities retain that lease and carry an explicit evaluation identity; no wire request, configuration field, downloaded descriptor or qualification flag creates them. Product catalog admission and publication still require final qualification and complete compiled downloads. Evaluation receipts explicitly say evaluation=true and unqualified candidate; they confer no release qualification.

## Task and platform qualification binding (2026-11-14)

Profile adds omitted-when-empty task_policies (Task to native policy-source SHA-256) and qualified_targets (QualifiedTarget { target, execution_identity, receipt }). Existing prototype/developer profile bytes remain decodable without these fields and never acquire product readiness from omission. A ContextNextRead execution requires its exact native prompt-constructor source SHA; overall Profile.policy continues to bind calibration, resource and model policy. Short prompt aliases never replace native choice/record IDs in authority or Advice.

Each product target row names one supported exact Rust target triple, exact CpuFit runtime execution identity and that target's actual qualification receipt SHA. Product activation requires the running target's row; unknown targets refuse. Windows measurements never qualify macOS/Linux, CPU8 never transfers to CPU1 or six-core hardware, and evaluation capabilities remain explicitly unqualified. Native source validation rejects duplicate target rows and mismatched runtime/tier identities.

## Original native attempt and unobserved configuration (2026-11-14)

The internal NativeNextReadRequest has optional omitted read_attempt using the existing ReadAttemptBudget. The actual production native controller fills its original same-host monotonic absolute expiry before transport; service scoring requires it and intersects all remaining bounds before admission. Optional scoring uses one transport dispatch, never registry-busy replay. Metadata-only developer transcripts may omit it and gain no execution authority.

ScopeBinding.configuration_revision=0 explicitly means unobserved optional configuration, only on IndexApplication availability Failed, profile absent, and no Advice. The service first and last validates actual source scope and retains actual installation/catalog/index/authorization/publication identities. This sentinel is never persisted configuration, Ready, enrollment or score authority. A malformed optional configuration yields explicit NotApplied/repair guidance while canonical source evidence can continue only under its own live authority/publication checks. Typed source-scope failures always refuse.

## Canonical neighboring context expansion (2026-11-14)

NativeRead appends Context, retaining all existing serialized variants. Legal expansion is the canonical context handler at a verified hit's indexed start line, before=0, after=32, max_bytes=4096 (or the smaller original allocation). When the exact original question bytes occur in the returned hit text, expansion instead anchors at the first occurrence's returned UTF-8 newline offset, only if that line remains within the verified citation span; otherwise the indexed start line remains the fallback. This preserves a late literal match in a full-chunk hit without an external source read, generated query or wider budget. The owning context operation clamps at indexed EOF and preserves source-location fences. No guessed EOF, hidden file-info read or weakened strict read_file range validation is introduced. The planned target range is an upper bound; EvidenceRecord retains actual canonical arguments/result and returned span, including truncation/coverage. Distinct ranked hits remain distinct choices; exact duplicate targets deduplicate. Publication/source/deadline checks remain unchanged.

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
keeps the native TensorProto bound to an in-memory tensor instead of embedding
its raw_data. With the current Rust tensor-buffer adapter, ORT 1.30 makes a full
native CPU tensor copy; this is not zero-copy. The verified file buffer, supplied
typed tensor buffer and native copy can coexist, and their actual process peak
is included in resource measurement.
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
assets are streamed into fresh, exclusively-created single-linked files in a
private flat closure. Source length/hash and independent destination identity
are checked, then the actual strict OpenedDirectory consumer handle verifies
every destination byte. Existing output is never overwritten. Original source
aliases/profile/weight/fixture bytes remain unchanged; no reader alias guard is
weakened. Historical hardlinked R1 fixture outputs remain failed artifact history. Product catalog/qualification/download gates remain separate.


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


The CPU16 Intern-4B full twenty-sample gate also failed (p95 30.576960s against
30s); all samples, including the slow first two after the single frozen warmup,
remain counted. Before any CPU24 measurement, a distinct closed physical24/intra24
large-model tier is approved, inter1/sequential/spinning0. It preserves the same
T2048/B1/N8, one-warmup/twenty-sample/rank19, 30s, 24-GiB and quality floors.
Only new profile IDs/policy/manifest bytes bind CPU24; CPU8/16 identities and
failures remain unchanged. No retry, extra warmups, discarded samples, automatic
fallback or qualification transfer applies.

Current Answer/2 advertises and consumes the same explicit saved-source
verification controls as other evidence operations, including report/require/off
and their bounded source/byte/deadline settings. The Core ReferenceInventory
visits only typed EvidenceRecord canonical_result values under the exact
ContextResponseV2 schema. Each record keeps its exact ScopeBinding publication;
duplicate record IDs, mixed partition scopes, malformed/mismatched publication
envelopes and foreign combined-member index bindings mark the inventory
incomplete. Existing combined-read member traversal reuses this owner recursively.
Request arguments, assessment/paragraph prose and arbitrary JSON fields are not
locators. No source-free verification claim substitutes for retained records.
## Independent Focus / Context foundation (15 November 2026)

Primary recorded the new current-version decision before dependent behavior,
SHA-256 `6bd58ec8d78a6bb9adb189efdbcdbaa397d1b709e65ff86b03174c6a8509c698`.
Approved product names are **Arbiter Focus**, **Arbiter Context — Compact**
(Qwen3-1.7B) and **Arbiter Context — Large** (Qwen3-4B). Two selectable Context
choices and one shared Focus ranking component supersede four-choice completion.
Selection is not qualification. MiniLM remains a separate bundled encoder.

Core configuration is `airk.arbiter.configuration/3`, with installation/catalog,
one monotonic revision, and independent `focus`/`context` StageConfiguration
blocks. Each has enabled, selected_profile and explicit immutable-index Enrollment
rows. Stage::Focus permits only new `airk.focus_rank.v1`; Stage::Context currently
permits ContextAction. Enabling/selecting one never enrolls or loads the other.
Both disabled is the side-effect-free default. Context may run without Focus;
when both are explicitly eligible the selected pipeline uses Focus first. Index
rename preserves identity; recreation/import never inherits either stage consent.
Unknown/noncanonical fields, empty/duplicate/wrong-stage enrollment refuse.

The new private state root is `arbiter-v3`, with newly versioned receipts binding
configuration schema AND stage in addition to existing identities/custody. It is
not an airk-state/1 alteration. Prior arbiter-v1/v2 bytes remain retained and
cannot supply v3 consent or custody. Absence creates nothing. Present v3 is
current; if only legacy optional state exists, optional operation reports repair
until explicit initialization publishes both-disabled v3 under the actual native
lease, preserving legacy bytes. Store must update its owning contract before
these new custody consumers. No automatic migration/download occurs on a read.

Catalog/status are `/2`. Current ModelChoice IDs are
`arbiter-focus-qwen3-reranker-06b`, `arbiter-context-qwen3-17b`, and
`arbiter-context-qwen3-4b`. Historical four ModelChoice variants remain only for
retained decoding/cleanup and are never current selectable stage profiles.
Profile.stage is omitted when None: retained prototype profile serialization and
asset/qualification identities remain unchanged, with an independent prior-owner
serialization fixture proof. Current profiles require a matching explicit stage,
matching tasks and exact per-task policy bindings. StageStatus exposes independent
selected/active/availability/reason/queue observations, native declared CpuFit and
qualified-target facts; missing fit is not a hardware promise.

Context uses new QwenSingleChoiceV1, its own exact tokenizer/control/symbol assets,
and DecisionSymbol raw logits over the native closed-card roster. Distinct actual
option IDs are required; no generation/chain-of-thought or old pointer/token
parity claim is admitted. Existing ContextAction semantic/calibration/roster/work
rules remain. New Focus QwenRerankYesNoV1 instead has the distinct FocusYesNo ABI:
three int64 [batch,tokens] inputs input_ids/attention_mask/position_ids, one float32
[batch,labels] logits output with exactly no/yes labels. Its compiler binds exact
no/yes tokenizer IDs/order. Every actual pair uses batch one and is charged.
Publisher two-logit normalization is a relevance ordering score, never a calibrated
sufficiency, correctness, abstention or source-authority probability. It cannot
validate as ContextAction advice; old task/CAL results do not transfer.

NativeFocusRequest is private inert data carrying the actual original read attempt,
parent Budget, WorkerIdentity, ScopedReadRef, ScopeBinding, question, complete typed
candidate Hits and exact UTF-8 source windows, complete order digest, prefix count,
canonical-read ordinal, optional partition allowance and remaining time/tokens.
Core validates same-host original expiry and the structural scope/worker/authority,
publication, bounds, exact full-hit/window/order hashes and partition binding.
These checks cannot create a native lease, grant, source proof or one-use charge;
service must validate those actual owners before tokenizer and native dispatch.
No caller-defined request/digest supplies a new reservation or fresh deadline.

FocusApplicationReceipt, optionally carried by SearchResponse.focus, binds the
actual scope/input/profile/compiler/calibration/task policy, entire original and
ranked candidate order, finite score observations, prepared source windows with
exact original UTF-8 offsets/full source length, presented/scored/omitted counts,
reserved versus observed tokens and actual scoring Work. Preparation is not proof
of successful FFI. NativeFocusResponse validates every source window against the
complete original Hit and every order/identity against the exact native request.
A successful application is the complete stable score permutation of the scored
original prefix followed by the unscored original tail, retaining every complete
hit/citation/future field. Exact ties retain original order. Native search's
100-returned-candidate cap and original optional Budget caps remain; qualified
profile/context/resource limits may narrow them, never enlarge them. No universal
five-pair/128-token or hardware-fit claim is made by this contract.

Failed/not-applied Focus keeps the entire original order; finite partial score
observations and actual partial costs may remain disclosed, never partial ranking.
Observed token overruns remain truthful only as Failed/not applied, without a
second dispatch or a released-charge claim. Applied results require known complete
work within the original allocation and deadline. Unknown native work retains
actual admission, source/asset pins and reservations until observed completion.
Scope/configuration/publication are checked before dispatch and delivery; common
revision changes suppress stale composite delivery. Off/excluded ordinary results
retain exact bytes where representation permits, full parsed equality otherwise;
None focus is omitted from the existing SearchResponse serialization.

Focus applies only to bounded relevance Search candidates. It grants no new reads,
query rewrite, refill, drop, source access or rank of exhaustive/list/exact/read
operations. Mixed requests partition before tokenizer/features/cache/scoring and
retain excluded ordering/metadata/continuations. Context receives the same complete
canonical results and counts all optional stages under the original common limits.
One selected Context residency and one shared Focus residency are independently
charged once with actual opaque permits; aggregate admission includes MiniLM,
combined peaks, load copies and fresh headroom. Turning off one stage does not
silently unload the other; actual FFI drain precedes release/removal.

Rust owns schema and semantic emission. Core test emitters produce independent
configuration/status/management/Focus request-and-result cases through the approved
generation pilot before consumer behavior. No handwritten client schema mirror
or unmeasured qualified model fixture is introduced. Core tests and schema presence
prove structural contracts only; model/reference/numeric/task/calibration/resource,
actual transport, installed/platform/package and release gates remain separate.
Ordinary single-index Search has a distinct Core-owned optional allocation from
its actual Host-captured ReadAttemptBudget: calls/rounds/fanout1, candidates the
minimum of actual returned/requested/64/qualified profile cap and the conservatively
reserved total-token/pair-token ratio, at most2048 total input tokens, at most65536
receipt-metadata bytes intersected with the existing frame allowance, and the
same earliest original expiry capped30s and floored once to whole milliseconds.
Zero candidates, submillisecond expiry or insufficient metadata allowance grants
no optional invocation. This adds no Search argument or attempt and does not
change Search max_bytes, which remains TEXT-only. Nested Answer must supply its
actual original parent and observed remaining allocation, never these defaults.
Unpartitioned mixed ordinary reads remain complete NotApplied until the actual
Host supplies a shared original-work partition. FocusApplication exposes validated
profile-derived pair/candidate/total caps only on Ready; unavailable grants none.

Private I1 adds ArbiterFocusApplication (actual worker/authority/publication) and
ArbiterFocusRank (NativeFocusRequest), using existing128KiB tool request and4MiB
tool response frame classes without raising constants. Both are refused by the
metadata relay. Their typed responses require an explicit nullable application
or response; absent fields cannot be treated as disabled/no work. A delivered
None represents native not-dispatched absence, never lost/unknown dispatched work.
Actual transport peer/worker/source/deadline/one-use checks remain adapter duties.

### Complete staged outer delivery and compact pre-dispatch observations

SearchResponse.focus is now an optional tagged FocusObservation: Receipt carries
the full actual native FocusApplicationReceipt; NotApplied is a compact known
pre-dispatch notice with bounded reason/availability/candidate count/optional order
digest and required work_unknown=false. Off/excluded or zero metadata headroom
retains None and the original complete result. A compact notice cannot replace
unknown dispatched work, claim Ready, inflate an existing frame or drop source
hits. The native Focus response still returns the full typed receipt. A Ready
FocusApplication includes actual profile/runtime/compiler/calibration/task-policy
identity as well as qualified caps, permitting truthful failed/unknown custody
if the callback closes after dispatch; placeholders cannot supply these facts.

ContextDeliveryBinding now has stage defaulting to Context and omitted when
Context, preserving retained legacy bytes. Focus is explicit. Rosters are keyed
by (stage,index), bounded by two times existing MAX_SELECTION inside unchanged
frame constants. Core extracts only canonical Search.focus, typed Search records
inside actual Context packets and the packet's own Context native receipt; no
arbitrary nested metadata or source text is scanned. Duplicate/conflicting stage
bindings refuse. The actual service must recheck that stage's selected profile,
consent/task/configuration and source/publication through the final outer write.

Distinct Focus score observations add known tokens and phase timings to Context
aggregate work with checked overflow, while candidates are a peak and source
calls/bytes remain owned once by canonical reads. Unknown Focus work makes old
Context assessments historical and cannot support a learned terminal or another
optional dispatch. A known Focus token-reservation violation remains Failed with
actual observed work, explicit Budget/unassessed terminal and no current/later
learned Context assessment. Actual one-use native reservations enforce time/order;
the wire does not invent an interleaving proof from commutative totals. Current
Core task semantic policy binds Focus validation/accounting and native input rules;
old calibration identities never transfer to these new semantics.

NativeFocusRequest semantic input digest uses a new framed
`airk.arbiter.focus.input/2` domain under the separately recorded primary decision.
Only remaining_ms and read_attempt.remaining_ns are zeroed in a private owning
Rust clone before hashing, as native transport can monotonically shorten them.
Original attempt ID/clock/absolute and retry expiry, parent allocation, token
allowance, ordinal/partition, authority/scope and full source/window/order data
remain bound. Runtime validation still checks the actual shortened carriers;
zeroing for identity grants neither time nor authority. Prior /1 fixtures and
receipts retain their original source identity; new Rust-emitted /2 cases prove
the exact normalization and all immutable-field refusals.

### Owner-only verified offline import (15 November 2026)

The separately recorded offline-import decision adds NativeOfflineImportRequest
outside the public ManagementRequest and Arbiter schema. Required fields are the
original positive expected configuration revision, exact compiled qualified
profile SHA-256, and explicit native absolute LOCAL bundle directory (UTF-8,
4096-byte/control-character bound). Structural validation rejects URI/relative,
UNC/network namespaces, device roots, traversal and Windows alternate-stream
syntax while retaining supported local extended Windows drive paths. This pure
Core check neither opens a filesystem nor proves local volume, alias exclusion,
source custody, source immutability or model qualification.

Private owner-only I1 ArbiterImport carries the typed request; its typed response
requires a non-null Status observation. Both request/response retain the existing
128KiB/4MiB tool frame classes. Metadata relay/public console forwarding refuses
it. No HTTP/I3/browser source-path operation, public management import variant or
public schema path is introduced. Unknown fields and absent/null required response
observations refuse rather than imply success or no work.

The native service must check actual local-owner admission, exact expected current
revision and the compiled qualified stage profile before reading declared assets.
The caller supplies neither profile metadata nor URLs/commands/provider code.
Read every source through the existing no-follow/native-identity-aware directory
and file helpers; reject unsupported aliases/streams, mapped/network sources,
foreign/changing bytes and incomplete external tensor closure. Check all compiled
lengths/digests and use actual ServiceLease-bound Store transactional staging,
custody and original partial/recovery proofs. Preserve source/index bytes and
all prior failed receipts; a failed or unknown operation never fabricates Ready.
Import does not change stage selection, enablement or immutable-index enrollment.
Only separately configured and eligible stages may activate verified assets.
Rust-owned native fixtures prove this private interface; actual source/process,
transaction/refusal/recovery and installer/offline journeys remain consumer gates.

### Ordinary singleton Focus allowance purpose

The separately recorded Focus-only allowance decision refines existing private
PartitionAllowance validation without a new field or change to old valid Context
bytes. is_ordinary_focus is true only for schema1, empty Context assessment
ordinals, parent calls/rounds/fanout1, resources.calls1, positive optional tokens
at most2048, candidates at most64 and metadata bytes at most65536, all within the
validated original parent allocation. Old Context ordinal/token rules remain
unchanged for every other shape, including known-zero optional allocations.
NativeContextAction explicitly rejects a Focus-only allowance. Actual Host proves
the original singleton selection/attempt/index; this predicate supplies purpose,
not source permission, fresh capacity or caller-controlled authority.

Host/MCP admission permits these private Focus-only allowances only for canonical
Search and genuine Context allowances only for canonical Answer, before storage
or scorer work. Nested native Focus inside Answer may retain its genuine Context
allocation. Focus-only direct Search must keep its own outer native delivery guard;
it is not a combined Context child whose parent will perform that final cut.
Original attempt/selection/index bindings and common service one-use/token/frame
limits still apply. Rust emits purpose, malformed/cross-purpose, oversubscription
and retained-byte cases before consumers; ordinary unpartitioned mixed reads
remain explicit NotApplied until an actual Host partition is available.


### Cumulative nested Focus allocation

The recorded nested-stage allocation decision gives Focus at most half (rounded
down) of the actual original local parent optional token allocation when the
initial actual Context application is Ready. Capture that readiness before reads
once; neither subsequent rounds nor later availability changes renew the ceiling.
Core focus_token_allowance intersects the ceiling less cumulative Focus cost with
the shared remaining tokens and qualified profile token bound, then rounds down
to complete qualified pairs. Known actual Focus costs and unknown held reservations
count cumulatively. A zero result means no dispatch. Unknown work still blocks
later optional work under the existing refusal rule; this helper grants no new
authority and replaces no native common total ledger.

When initial Context is unavailable, or for independent ordinary Search, Focus
retains the full original local allocation subject to those same intersections.
Context continues using actual shared remaining capacity, independently of whether
the Focus ceiling is exhausted. At local2048/pair512, Focus can consume at most1024
when Context starts Ready. The remaining capacity does not guarantee any Context
roster fits: original compiler bounds still refuse or abstain without cropping.
No wire field, total budget increase or per-round reset is introduced. Rust emits
independent local/cumulative/shared/profile/whole-pair cases before consumers.


### Explicit initialization observation

Status2 initialization is optional and omitted on normal initialized status,
preserving normal serialized bytes. When present, its typed actual_legacy_digest
is the positively observed retained digest (omitted if unavailable) and its native
reason is nonempty and at most1024 UTF8 bytes. Presence marks the disabled, empty,
configuration as a non-writable placeholder. It is neither a current CAS draft
nor permission to initialize or start work. Configuration load failure can follow
native work: preserve actual resident profiles, queued/running counts, draining
observations and charges. The marker must not fabricate zero work or completion;
stages cannot report Ready against a placeholder.

Native configuration load failure reports this marker, including missing, legacy,
ambiguous and corrupt state; normal stage failure with a valid configuration does
not require it. Clients retain the complete observation/outcome, display explicit
Initialize controls and native digest/reason, and create no editable stage draft
from a placeholder. No reason-string inference or automatic initialization occurs.
The existing Initialize expected_legacy_configuration checks remain authoritative,
including ambiguous retained-state refusal. No consent is migrated automatically.
Rust emits these observations and proves unchanged normal bytes before consumers.


### Explicit CPU4 execution tier

The recorded CPU4 decision adds the exact minimum_physical_cores4/intra_threads4
pair to the closed CpuFit tiers1/8/16/24. All tiers still require inter_threads1,
sequential=true and spinning=false; mixed thread/core pairs and arbitrary tiers
refuse. There is no new field, fallback or change to existing runtime identities.
New CPU4 profiles carry their own runtime/profile/qualification identities; older
CPU1 numerical observations and other tiers never transfer qualification. This
selects one baseline Focus qualification configuration suitable for testing on
the six-core Mac host, not a hardware fit or performance claim. Native CPU4 actual
resource/latency/correctness and combined-stage gates remain required under the
original deadlines, budgets and service resource admission. Rust emits the valid
CPU4 value and independent mixed-tier/execution-mode refusals before consumers.


### Selected Context Qwen six-input ABI enforcement

Both selected Context Qwen profiles require DecisionSymbol readout with exactly
these ordered inputs: input_ids, attention_mask and position_ids each Int64 with
axes[batch_1,tokens]; decision_positions Int64[batch_1]; candidate_positions
Int64[batch_1,options]; option_mask Bool[batch_1,options]. The sole scores output
is Float32[batch_1,options]. Core Profile.validate_stage calls the shared
ScoreAbi.validate_context_qwen validator before asset/session consumers. Missing,
extra, reordered, renamed, mistyped or differently shaped tensors refuse. Native
Rust cases prove the complete selected ABI and independent refusals; synthetic
current Context profiles use it explicitly. Historical four-model fixtures remain
unchanged and gain no selected-model eligibility. No wire field, tokenizer/model
math, budget, persisted format or historical qualification identity changes.


### Immutable model acquisition origins and complete backup ordinals

The recorded optional-source decision adds AssetPart.backup_urls, serde default
and omitted when empty. At most two distinct backups share the SAME part bytes
and SHA; no alternate payload identity exists. Empty lists retain original part
and Profile canonical bytes, demonstrated by literal old part bytes and the
retained old nested Rust Profile serializer/hashed fixture. No airk-state/1
fixture is rewritten. Nonempty release descriptors require a new authenticated
manifest/catalog through final promotion, never rewritten consent or enablement.

Core download_origin is the sole initial-source predicate. Official
iokaio/airk GitHub release URLs use safe tag and file components. Hugging Face
URLs use HTTPS on huggingface.co and one of the exact repositories
Qwen/Qwen3-Reranker-0.6B, Qwen/Qwen3-0.6B-Base, Qwen/Qwen3-1.7B, Qwen/Qwen3-4B,
onnx-community/Qwen3-Reranker-0.6B-ONNX, or iokaio/airk-arbiter-models. Each
resolve path binds a lowercase 40-character hexadecimal commit and one safe
root-level filename. Core rejects credentials, explicit ports, branches,
queries, fragments, escapes and traversal. Availability, rights and payload
qualification require separate evidence. Native transport owns bounded redirect
admission.

AssetDownload.source_count validates a complete same-count backup ordinal on
EVERY ordered part, at most initial+two alternatives. AssetPart.source_url(0)
returns primary and later ordinals its declared backup, without changing bytes
or SHA. Part locations are distinct across the asset; existing positive/1GiB
part bounds, sixteen-part cap, full length sum and singleton whole SHA remain.
Profile.validate_release_downloads validates this entire closure.

Only explicit Provision acquires network bytes. Recover stays local reconciliation
and warm; startup/query/status/import grant no download. Whole-asset fallback
requires narrow Store-owned sealed-partial restart, no failed-part append or
mutation of completed/foreign/unsealed files. Every original per-part deadline
(900s, or the part's bytes at 256KiB/s when longer, from 1.8.203),
30s response/idle limit and shared four redirects per part remain across source
attempts; no retry or new time follows from a backup. Cancellation, supersession,
revocation or local custody failure stops immediately. Native publication lists
only actual GitHub-hosted files; all HF descriptors stay in authenticated catalog
closure. Rust structural GH-backup cases assert no working URL or publication.

## Relaxed ceilings in the 1.8.201 local candidate

At the user's direction the structural ceilings below were multiplied by four so
a local candidate can be tested. This is **not** qualification. Every changed
value, its reason and the evidence needed to restore it is in
`docs/arbiter/relaxations.md`.

- Token ceilings: compiler/Context profile `max_tokens`, `Budget.input_tokens`,
  `FocusProfileLimits.{pair,max}_input_tokens`, Focus local parent allocation,
  `reserved_input_tokens`, native Focus/Context `remaining_input_tokens` and
  combined-context `resources.input_tokens` accept 1..=8192 (was 2048).
- Deadline ceilings: `Budget.deadline_ms`, native `remaining_ms`, combined
  `deadline_ms` and `read_limits::MAX_DEADLINE_MS` accept up to 120000 ms (was
  30000). This candidate originally retained the 2048-token/30000-ms default.
  Final-plan D06 supersedes that default with 8192 shared input tokens and
  120000 ms. Historical controls now specify 2048/30000 explicitly; their
  retained fixtures and evidence identities are unchanged. The additive native
  budget cases and [runtime budget contract](../../../../docs/arbiter/runtime-budget-contract.md)
  separate shipping defaults, ceilings, actual allocations and qualification.
- The exported Focus graph has a static `[1, max_tokens]` input width (see
  `selected_graph_shape`). A Focus profile wider than the exported 512 cannot
  load without a new graph export. Context graphs have dynamic token axes.
- Qualification floors (T2048, 30 s p95, resource and numeric rows) were not
  changed and still describe the earlier evidence.


## D05 terminal-class calibration extension (15 November 2026)

The final-plan D05 decision authorizes an additive calibration `/2`, not
terminal deployment. Historical calibration `/1` bytes and interpretation remain
unchanged. `/2` requires exactly one explicit supported/unsupported disposition
for Read, Sufficient, Clarify and NoJustifiedRead. An unsupported class has no
acceptance rule. Each supported terminal class binds its own exact human-backed
qualification SHA; enum presence, generic Read certificates and model scores
cannot supply that evidence. Read support retains its separately qualified scope.

One frozen complete roster is scored once. The unique global winner and its raw
margin are computed before class admission. Unsupported terminal winners, ties,
invalid scores and insufficient margins abstain; no runner-up substitution.
`/2` class metadata is authenticated policy, not proof that labels or qualification
exist. It changes the native task-policy identity through the existing source
digest; old trained/profile evidence cannot silently transfer.

Rust owns typed terminal CAL/confirmation labels, raw captures, criteria and
qualification. CAL chooses a threshold; confirmation replays it unchanged. Human
whole-card labels bind exact source-visible roster/input identity and all card
fields. Each terminal class needs its own human-backed positives and zero observed
false admissions. Frozen risk/coverage criteria precede scores. For Sufficient,
one-sided 95% false-acceptance upper risk must be strictly below .01. The native
zero-error calculation uses a separately reviewed effective independent sample
count, never an unreviewed raw-task count. Exact repository clusters and accepted
sample closure bind a separately pinned independence analysis. No minimum of 299
repositories is introduced. Missing suitable labels or independence analysis
leaves the class unsupported.

Final promotion independently reads/replays the original human annotations,
native scores and independence analysis from approved exact SHA-pinned files.
Calibration metadata alone cannot grant terminal release authority. Sufficient
means complete task-required retained support; Clarify means actual ambiguity;
NoJustifiedRead means no justified choice in the complete verified menu, never
global unanswerability, timeout or an omitted choice. This extension adds no
wire action, persisted state format, new scoring pass or runtime learning.

`TerminalCriteria` also freezes exact CAL/confirmation population identities and
the semantic whole-card rule template before scoring. Only the numerical raw
margin is selected on CAL. Native population identities include the exact
visible-input and human annotation sources; compiler refusals remain in the
denominator. A model cannot qualify by omitting low-scoring/failed questions.
The test-support terminal fitter accepts pinned original sources, emits native
experimental profiles and leaves failed/missing classes explicitly unsupported.
All its output remains unqualified and needs the separate final-promoter checks.

Terminal promotion reconstructs the full original question and verified ReadChoice
bodies through the native roster/prompt owner and re-tokenizes with the exact
authenticated compiler/tokenizer assets. Scored rows must match that actual
compiled-input digest and token count. An opaque roster hash cannot certify
substituted question/menu bytes. The owner also recomputes the deterministic
CAL-only cutoff during promotion; a confirmation-selected cutoff refuses even
when it happens to retain zero observed errors and sufficient coverage.

### D05 semantic digest-input decision

The Context semantic closure explicitly includes LF-canonical bytes of
`context_action.rs`, `context_action/calibration.rs` and
`context_action/calibration/terminal.rs`, plus the existing Focus semantic
digest. `terminal.rs` owns the runtime unique global winner and raw-margin
primitive as well as terminal acceptance/fitting; its production bytes are
therefore digest inputs. Assertion-only test files are outside that closure.
Retrieval's existing Context task-policy owner also hashes its complete
`context_action.rs`, so the native prompt reconstruction hook is covered.
No newly extracted source controlling admitted card semantics is omitted.

This is an explicitly new current-version policy identity. Native mutation
cases demonstrate that changing any of those three source inputs changes the
semantic identity, while CRLF normalization is preserved and lone CR refuses.
All old Context profiles/calibration/captures retain their exact original
identities as historical evidence; they cannot be relabeled or admitted under
the new policy. Regenerate metadata with the Rust owner from unchanged verified
weights, stage new descriptors privately, and collect new native evidence.
Existing immutable published metadata/source URLs remain unchanged.

### Explicit native schema emission

`arbiter::schema::tests::schema_matches_reviewed_capture` remains a comparison
by default. With the exact explicit environment value
`AIRK_WRITE_CONTRACT_SCHEMA=1`, the same Rust owner writes its derived schema
to the fixed tracked `packages/contracts-gen/arbiter.schema.json` destination
before comparing it. This is the reviewed generation path, not a handwritten
JSON/TypeScript mirror. The write truncates the existing file in place; the
verified same-user runner must preserve its owner and access policy and retain
the source/output identities. Other environment values do not authorize a write.
The terminal budget/source/acceptance emitters remain separately explicit and
never confer model or publication qualification.

## D08 current configuration4/status3 and device variants

Core device contracts preserve Profile CPUbytes by omitted device_execution=None. GPU variants have distinct manifest/execution identities, Float32/Float16 only with TF32off and exact graph/math/runtime/capacity bindings. DevicePolicy Auto/Cpu and GpuPrecisionPreference Auto/Fp32/Fp16 are independent. Auto tries admitted fitting FP32 GPU, then admitted fitting FP16 GPU of the SAME selected model, then the existing admitted CPU path; explicit GPUprecision restricts GPU variants without relabeling CPU fallback. FP32 actual native execution precedes FP16 efficient-attention work. MiniLM CPUINT8/vector identity remains unchanged. Pure advisory selection never grants compute/source/qualification authority. Scope-wide authority/deadline/unknown-settlement failures stop the whole operation.

DeviceCatalog/1 is exactly three slots Focus/Compact/Large containing distinct exact native Profile manifest/execution references. Materialization resolves each unique descriptor once and returns the SAME checked reference. DeviceCatalogBinding is separate from Configuration.catalog: the latter is the native SOURCE registry catalog namespace, preserved by migration. The new binding contains a framed digest of exact validated DeviceCatalog bytes plus its authenticated compiled source identity. Configuration4 decoding/CAS requires that exact expected device-catalog/source binding; no ambient or client manifest supplies it. Existing catalog2 and CPU Profile schemas/digests remain identifiable.

Configuration4 fields are schema, installation, catalog, revision, device_catalog, focus/context DeviceStageConfiguration {enabled,selected_model,device_policy,gpu_precision,indexes}. New disabled settings default both policies Auto. Native upgrade resolves exact selected CPU profile into the same model slot, preserves enablement, consent/enrollment and source namespace, increments revision once and chooses Auto/Auto. Configuration3 has no explicit CPU device preference. Existing explicit configuration4 Cpu/precision survives load/save. Unsupported legacy schemas, mismatched original/source/devicecatalog identities, unknown fields and noncanonical bytes refuse. Configuration3/raw receipts remain retained evidence, never active parallel authority after4 publication.

Status3 contains its exact Configuration4, optional disabled initialization facts, and independent DeviceStageStatus. Each stage contains requested model/device/precision, active profile, availability/reason/queue counts, actual selection record and exactly the appropriate model slots with variant status. ModelAdmissionStatus distinguishes qualified model/task from disclosed version-scoped local candidate and unavailable. GPU Ready additionally requires exact device numerical/resource/placement evidence bound to the actual execution identity; those descriptive hashes are not a capability. Existing1.8 native CPU localcandidate allowance is retained under its owner/version and honest modelstatus, not promoted to final1.9 model/trio/task/CAL acceptance. Failed Large CPU device evidence remains an Auto refusal. Status3 never projects GPU into Status2 CPU identity; legacy management must refuse when it cannot represent active4.

ManagementRequest adds StatusDevice, InitializeDevice, MigrateDevice, ConfigureDevice, ProvisionDevice, CancelDevice, RemoveDevice, RecoverDevice; old variants/payloads remain unchanged CPU semantics. Current private ArbiterImportDevice carries the SAME owner-only NativeOfflineImportRequest as legacy, with identical private role/metadata-relay refusal and frame budgets, returning ArbiterDevice(Box<DeviceStatus3>). Existing legacy replies retain Status2. Private local paths are absent from public management/schema. Shared Rust schema producer emits current types alongside legacy roots; generated clients must consume complete native fields and Rust-emitted semantic cases, not handwritten DTOs.

DeviceMemory required_peak/required_joint_peak fields are declarations, not invented measurements. GpuProbeReservation/1 is separate unqualified host/device/120s request bounds and cannot mint a DeviceExecution or resource pass. Authentic native owner/capacity/synchronization and final-device evidence are still required before production Auto dispatch. G1/config/store code alone does not implement the GPU runtime or relax any existing final promotion/scientific gate.

## Device catalog/source upgrades within configuration4

Ordinary configuration4 decode stays strictly bound to the expected compiled DeviceCatalogBinding; it never accepts arbitrary source/catalog changes. A native-only upgrade may rebind4->4 using authenticated expected OLD and NEW compiled catalog snapshots/descriptor resolvers, exact original raw configurationSHA/revision, unchanged installation and SOURCE catalognamespace, and the live owning authorization predicate. Both catalogs must independently validate their exact manifest/execution references and bound source; unknown ancestors/rows or changed bindings refuse. The transition clones all stage settings (model, enabled state, enrollment/consent, device policy and GPU precision), changes only device_catalog and revision+1, and uses the same existing writer/mutex/OSlock and prior-HANDLE CAS. Prior exact4 bytes are create-new retained and fsynced before atomic replace; no profile/device/task promotion is inferred. No public request accepts old/new catalog objects or an authorization predicate. Native StatusDevice startup must use the owning compiled transition inventory automatically, otherwise return explicit unsupported-upgrade rather than reinterpret stale configuration or require routine manual selection.

ConfigurationBindingObservation is diagnostic metadata only for locating an authenticated old compatibility snapshot. It is not active configuration, source permission or device/model qualification. Native caller must authenticate the old/new snapshots independently before CatalogRebindRequest/rebind_checked; ordinary load stays strict and an unknown snapshot refuses. Once4 publishes, stale expected revision/binding cannot overwrite later settings. Already-committed rebind is observed by loading the new expected binding, not replaying the old CAS.

## GPU receipt execution tags and full device status parity

Profile::advice_runtime preserves exact historical CPU RUNTIME bytes for device_execution=None. GPU receipts use the full DeviceExecution digest identity, never a bare provider label. The shared compiled receipt_runtime helper accepts only exact onnxruntime-cpu/1.30.0 or exact CUDA/DirectML/CoreML1.30.0 /device-execution/<64 lowercase hex>; unknown versions/backends, bare GPU names, CPU tier strings, uppercase/short/long digests, suffixes and controls refuse. Advice/1, ContextActionAdvice/2 and all three Focus binding/ranking validators share this helper. Existing JSON shapes/CPU serialized values stay unchanged; this vocabulary extension grants NO execution/profile/source authority. Native builders obtain the value from the actual admitted Profile; never relabel GPU as CPU. Dependent caches/receipts must keep the resulting distinct execution identity.

This changes actual native policy source closure. Focus core semantics now use domain /2 with exact current Focus, native_focus and normally compiled receipt_runtime helper bytes. Context core semantics binds current context source and that new Focus identity. No edited bytes are excluded. Current Focus/Context task-policy digests therefore change; native CPU/GPU metadata regeneration is required before activation. Old profiles, CAL/task/resource receipts remain historical, never retagged. A CPU compatibility path requires reviewed closed exact before/after source, graph, tokenizer, output ABI and runtime mapping, not blanket runtime-prefix acceptance. Preserve working CPU eligibility until compatible current metadata/bridge is actually ready.

The service dependency correction shares scope and allowance records with the
native Focus and Context request owners. The framing and validators retain their
existing fields and limits. The current `native_focus.rs` import changes enter
the existing exact source digest. Regenerate dependent native CPU and GPU
metadata for the new policy identities before activation. Preserve the earlier
profile, calibration and qualification receipts under their original pins.

DeviceVariantStatus preserves every prior ProfileStatus datum: cpu_fit, qualified_targets, tasks, download_bytes and memory_reservation_bytes, in addition to exact manifest/execution/backend/precision/lifecycle/admission/device-evidence fields. from_description is descriptive copying only: exact immutable native Profile metadata, checked sum of logical asset bytes, no inferred permission or qualification. Per-model summary retains availability/reason/tasks even with no variants. Native owner sets model Requested only for admitted enable-intent without an actual Ready variant, Ready only for an actual Ready variant, Unqualified if none admitted; transient runtime/assets/fit failures remain in variants/stage. This is a UI affordance observation, never an execution permit. Frontends never invent task, fit, target, byte or admission facts.

Rust-native semantic cases cover independent stages, all three model slots, requested vs actual FP32/FP16/CPU fallback, typed refusals, local candidate vs qualification, every lifecycle availability, eight current management operations, twelve disabled configuration combinations and exact legacy raw-SHA/revision migration input. All fixture cases explicitly qualification=false; synthetic evidence hashes do not qualify hardware/models. Actual absence is ordinary disabled configurable state without InitializationStatus; retained v1/v2 may have a positively proven non-writable initialization observation. Ambiguous/corrupt/unresolved current state is typed failure, never fake disabled current revision1.

Empty-variant model summaries retain Rust-owned supported model-family task roles (FocusRank or ContextAction), even when availability remains Unqualified. SelectedModel::supported_tasks is descriptive only, never model/device admission. Frontend enrollment controls consume native metadata; they never infer roles from labels. Nonempty variants retain the union of actual immutable descriptions.


## D08 all-unavailable Context probe calibration

Calibration `/2` may contain zero acceptance rules only when its complete, unique
four-class disposition list marks Read, Sufficient, Clarify and NoJustifiedRead
Unsupported with bounded nonempty reasons. Missing, duplicate or inconsistent
dispositions refuse; every Supported class still requires its matching rule and
all existing terminal proof constraints. The full model/compiler/task/ABI and
trained-asset bindings remain mandatory. With zero rules, every action and margin
abstains: this describes unqualified native numerical/resource probe metadata,
not advice, model admission, task qualification or terminal permission.

Historical `/1` still requires nonempty rules and forbids `/2` dispositions. Its
retained bytes and interpretation are unchanged. This source change intentionally
changes the current Context task-policy digest; native current-policy metadata
must be regenerated. Earlier profiles, CAL and measurements retain their original
identities and are never retagged as current evidence.


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
