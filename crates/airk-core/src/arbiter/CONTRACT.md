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
