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
For Answer /3 loop records this is amended by "Record identity without
per-read status" below (change-log entry 54): the identity binds the result
without its per-read status, custody keeps the result as received, and the
reply carries the first search's status once as `read_status`.
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
jobs. It names the one-thread embedding sessions of the shared model, 1..=32
(`MAX_INFERENCE_THREADS`; it was 1..=4 until the indexing performance plan's
step P2.3, 24 November 2026), with the default and the shared model charge
described in `docs/service-resources.md`. The stored shape is unchanged; every
earlier stored value stays valid. Optional Arbiter execution uses
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


## Service-wide GPU use is opt-in (decided 20 November 2026, America/Denver)

Owner: Core `service_resources::ResourceConfiguration`. Host
`service_resource_policy` owns the stored file. The service resource policy
owns live GPU eligibility. The user decided both points: GPU use is opt-in on
every platform through the one service-wide setting, and a configuration that
already stores GPU use as enabled is reset to off once and the user is told.
This amends the earlier approval under which a configuration without a stored
value allowed GPU use. Device policy and GPU precision keep their stored
values. The Auto order, CPU tiers, typed reasons, per-result disclosure and
provider values are decided separately.

**Default.** `allow_gpu` is off in a new configuration, in a stored
configuration that lacks the field, and in a replacement request that omits
it. No platform or build turns it on. While it is off no Arbiter GPU route
is eligible and Arbiter Auto selects CPU.

**Encoding.** The stored file stays `resources-1.4.json` and grows the way it
has before: one added field that is optional on read. The field is
`gpu_opt_in`, written directly after `allow_gpu`. It is omitted when absent,
so a configuration with GPU use off and no notice keeps its earlier bytes.

| `allow_gpu` | `gpu_opt_in` | Meaning |
| --- | --- | --- |
| false or absent | absent | Off. No notice is pending. |
| true | `user_choice` | The user turned GPU use on. Kept across restarts. |
| false | `default_reset` | Off after the one-time reset. The notice is pending. |
| true | absent | Stored under the earlier enabled default. Never in effect. |

`allow_gpu: true` with `default_reset`, and `allow_gpu: false` with
`user_choice`, are invalid.

**Distinguishing rule.** `allow_gpu: true` is honored only when the same
configuration carries `gpu_opt_in: user_choice`. The service writes that
record only for an accepted request that states `allow_gpu: true`. A value
stored under the earlier default cannot carry it, because no earlier build
wrote the field.

**One-time reset.** `service_resource_policy::load` reads an enabled value
without the record as `allow_gpu: false` with `gpu_opt_in: default_reset`.
This is an interpretation of the stored bytes. It writes nothing, and it
gives the same result on every start until an accepted request stores the
current encoding. After that the stored value either carries the record or
is off, so the reset cannot apply again. A stored off value and a missing
field are off with no notice, because neither stored GPU use as enabled.
`load_stored` returns the bytes as written. Only a carrier that copies the
configuration unchanged uses it, and its `allow_gpu` is not the value in
effect.

**Report.** The service reports the reset as `gpu_opt_in: "default_reset"`
inside `configuration` of every `Resources` response: the `service.resources`
read and the replies to `service.allow_gpu` and `service.configure_resources`,
on the pipe, airkctl and Admin surfaces. That read returns the supervisor's
cached snapshot. It initializes no GPU and loads no model. The report clears
when an accepted request states the GPU choice: `SetGpuAllowed` with either
value, or a replacement with `allow_gpu: true`. A replacement that leaves GPU
use off keeps it. A refused, failed or unknown request keeps it, and so does
a restart.

**Requests.** `SetGpuAllowed` with `true` stores `allow_gpu: true` and
`user_choice`. With `false` it stores `allow_gpu: false` and no record. A
request equal to the state in effect changes nothing, as before. `false`
during a pending notice is therefore a change: it is the user's answer, it
clears the notice, and it causes no GPU transition. `ConfigureResources`
replaces every setting and takes `allow_gpu` as stated. The service derives
the record itself: `user_choice` when GPU use is on, the pending
`default_reset` when it stays off, otherwise none. `gpu_opt_in` in a request
is not an instruction. The reply shows the stored result, so a client may
send back a configuration it read.

**Refusals.**

- A stored file with an invalid pair or an unknown `gpu_opt_in` value is a
  configuration error. The existing handling applies: the default
  configuration is in effect with GPU use off, GPU admission is closed, and
  new work is blocked until a valid replacement is accepted.
- An unknown `gpu_opt_in` value in a request is refused at decoding.
- `SetGpuAllowed` with any other field or a non-boolean value is refused at
  decoding. During a configuration error it is refused with
  `resource_configuration_requires_repair`. Both are unchanged.

**Compatibility.** `ConfigureResources.configuration` and
`ResourceSnapshot.configuration` gain the optional field. `SetGpuAllowed`,
the I3 schema and the generated client types are unchanged. The installer
rebuild journal embeds this type and re-encodes it byte for byte. A journal
without the record is unchanged, and the installer copies the stored
configuration through `load_stored` so that the successor service applies
this rule. A build that predates the field refuses a file that carries it as
a configuration error, as it does for the earlier added fields.

Core tests use literal stored bytes for each row of the table, the reset, the
user's answer in both directions, the replacement rule and the invalid pairs.
Host tests read and write real files. A service test restarts an owned
service over a file stored under the earlier default.

`GpuOptIn` derives its JSON Schema and joins the I3 schema roots, so Admin
and the extension read `gpu_opt_in` through generated types. The wire value
is unchanged. This replaces the earlier statement that the I3 schema and the
generated client types are unchanged.

The later [indexing performance plan Q10](../../../../docs/indexing/performance-plan.md#q10-the-setting-for-embedding-on-the-graphics-card-follows-from-q2)
adds independent `allow_embedding_gpu` to this configuration. Missing values
permit embedding GPU admission. A replacement that omits the field preserves
an earlier explicit choice. `SetGpuAllowed` affects Arbiter only.
`Resources` reports the effective embedding setting when configuration is valid.
`ConfigureResources` applies the setting to the native embedding owner after
persistence succeeds. An invalid resource configuration closes admission.
The setting permits a qualified provider. Actual inference reports whether the
provider ran, including CPU fallback. The Rust-emitted I3 resource schema
covers the existing configuration request used by Admin and the extension.


## CPU ladder, Auto route order and typed fallback reasons (decided 21 November 2026, America/Denver)

Owner: Core `arbiter::device`. The service device lifecycle applies it. This
records approvals A1 and A2 of the FP16 delivery plan and amends the
same-model, same-width fallback in "D08 current configuration4/status3 and
device variants". Configuration4, DeviceCatalog/1, existing profile bytes,
every receipt shape and the Search and Answer schemas are unchanged.

**Tier.** A CPU tier is a catalog profile. Its token budget is `max_tokens`
and its CPU class is `cpu_fit`. The target class of this release is the
existing 8-core, 8-thread tier. Full is the profile at 8,192 tokens and Fast
the profile at 2,048 tokens for the same model, graph and weight file. No
tier is guaranteed. The service-wide `inference_threads` setting bounds
MiniLM embedding work. It does not bound a model forward, whose threads are
the profile's `cpu_fit`.

**Order.** For one stage Auto takes the first candidate that is admitted and
fits. The requested model comes before the next smaller model, which is
Compact for Large. Within a model, a qualified GPU route for the observed
adapter comes first, then CPU tiers by larger token budget, then by larger
CPU class. Device policy `cpu`, or service-wide GPU use that is not turned
on, removes every GPU candidate. NVIDIA on Windows and Linux uses CUDA. Any
other adapter on Windows or Linux, and an Apple GPU on macOS, uses WebGPU
once that route is qualified there. Until then such an adapter is reported
`no_qualified_gpu_route` and the CPU ladder applies.

**Planner.** Core `attempt_order` owns this order. It is a pure function of
the requested model, the device policy, the GPU precision preference, the
service-wide GPU use, the remaining time and the candidates. GPU use is one
of three states: not turned on, turned on, or turned on with an adapter
enumeration that found no adapter. A candidate carries its token budget, its
CPU class and, for a GPU variant, the adapter it was observed on and that
adapter's vendor. A GPU variant that no adapter was bound to carries its
rejection, or is excluded by precision, and is never offered. The planner
returns the candidates to try in order, with one row for each candidate it
refused. The service tries them in that order and adds one row for each that
fails at dispatch. Among GPU routes the existing preference stays: FP32
before FP16, then backend priority. The planner grants no execution, source
or qualification authority.

**Reasons.** `SelectionReason` gains `gpu_not_opted_in`,
`no_qualified_gpu_route` and `smaller_model_selected`. Every candidate that
was considered and not selected has one row.

| Cause | Reason |
| --- | --- |
| Device policy `cpu` | `forced_cpu` |
| GPU use not turned on: never chosen, turned off or reset | `gpu_not_opted_in` |
| No adapter observed | `no_gpu` |
| An adapter with no qualified route or variant for this model | `no_qualified_gpu_route` |
| Provider library absent or not built | `runtime_unavailable` |
| Provider failed to start | `provider_initialization_failed` |
| Device or host memory does not fit | `device_memory_insufficient`, `device_memory_unknown`, `host_memory_insufficient`, `host_memory_unknown` |
| Too few usable physical cores | `cpu_fit_insufficient` |
| GPU precision preference excludes the variant | `gpu_precision_excluded` |
| A smaller model was taken | `smaller_model_selected`, with the rows of the larger model |
| No CPU tier fits | `no_qualified_cpu` |

`forced_cpu` is no longer recorded for the service-wide setting. A selection
holds at most 20 reason rows. `no_gpu` is recorded only when an adapter
enumeration ran and found none. When GPU use is on and the model has no GPU
candidate, the row is `no_qualified_gpu_route`. A CPU candidate that was
refused has its own row. `no_qualified_cpu` also closes an order that ended
with nothing selected when the model had no CPU candidate at all.

**No fit.** When the order ends without a selection, the stage records a
selection with `selected: null` and its rows and reports `unsupported_system`.
One case differs: when a candidate was passed over only because its verified
assets are not installed, the phase is `missing_assets`, because a missing
download is not an unfit system. The other stage and retrieval continue. An expired deadline, changed
authority or unknown settlement records `operation_failure`, reports `failed`
and tries nothing further. No work is replayed.

**Smaller model.** A selected variant may belong to the next smaller model of
the same stage only under Auto and only with `smaller_model_selected`. This
part takes effect with its own validator change. Until then nothing emits
`smaller_model_selected`, a selected variant belongs to the requested model,
and a candidate of another model is refused. The value joins the enum now so
that the vocabulary changes once.

**Disclosure.** Core writes the stage reason from typed facts. It names the
route, the model, the threads or adapter route, the token budget and the
cause of any fallback. It makes no statement about weight precision. It is
the stage `reason` and so the `reason` of each result's `IndexApplication`.
A Context advice reserves the smaller of the remaining request tokens and
the profile's `max_tokens`. A compiled input above that ceiling is a known
refusal with no native work and no held reservation.

**Compatibility.** The three values are additions to a closed enum in
status/3, device-status/1 and scoped-device-status/1. Schema names are
unchanged and `arbiter_device_contract` becomes 2. No stored file holds a
`SelectionReason`. The Rust owner regenerates `arbiter.schema.json`, the
generated TypeScript and the device semantic cases, all with
qualification false.

Core tests give the planner literal candidates, opt-in state, adapter
vendor, core count and remaining time, and compare literal reason lists.


## Answer loop wire contract version 3 (decided 22 November 2026, America/Denver)

Owner: Core `arbiter::answer_loop` (reply, decision log, card advice, compact
Focus receipt, loop calibration), with the private native inputs in Core
`native_loop`. This is the L1 contract decision of the final plan, section 12
(12.1 stages, 12.3 contracts, 12.4 time, 12.4.1 queueing), recorded under the
section 11 row "Answer becomes a local multi-round loop (section 12)": the
user's decision to change Answer's wire shape, budgets and digest inputs, with
the exact shapes settled here. The budget ranges, card tasks, decision record
and log, record uses, conflict kinds, unrun reasons, degradation kinds,
compact Focus receipt, loop calibration and calibration set, and the review
corrections below, are the primary agent's L3, L4 and L5 design decisions of
the same day. Version 3 lands beside version 2. Request and response /1 and
/2, the /2 `PartitionAllowance`, `NativeContextActionRequest` and
`NativeNextReadRequest` stay unchanged and compiling until L3 and L4 switch the
kernel, MCP, frontend and CLI and delete them; there is no backward
compatibility after that.

### Optional caller inputs (user decision, 26 November 2026)

A version 3 request may carry four optional fields. Each is omitted from the
serialized request when absent or empty, so a request without them keeps its
bytes and its digest. The schema name stays `airk.arbiter.context.request/3`.

- `needs`: up to 8 statements (`MAX_NEEDS`), each 1 to 400 bytes of UTF-8
  without control characters, no two alike. When present they are the
  sub-needs of the request, in the caller's order, and the kernel derives none
  of its own. A request with `needs` is one part.
- `anchors`: up to 16 exact strings (`MAX_ANCHORS`), each 1 to 200 bytes, no
  two alike. Each gets an exact lookup in the first round. An anchor is data
  and is never interpreted as a pattern.
- `deliverable`: one of `code_change`, `test`, `document`, `analysis`.
- `known`: up to 32 repository-relative paths (`MAX_KNOWN`), each whole or
  with an inclusive one-based line range. Evidence wholly inside a known
  range is not returned again and is counted as withheld.

The four fields are inputs to the request digest when present, so two requests
that differ in any of them are different jobs.

The single part of a request with caller needs holds up to `MAX_NEEDS` (8)
sub-needs. Everything about kernel-derived sub-needs keeps `MAX_SUB_NEEDS`
(4): the budget's `max_sub_needs`, Plan's chosen cards, `MAX_CARD_SEGMENTS`
(16) and `MAX_DECISIONS` (101). One part of eight caller needs takes 50
decisions, inside the same bound. `MAX_SUB_NEEDS` keeps its name and value
because the card policy sources that use it (`answer_loop/cards.rs` and the
retrieval crate's `loop_cards.rs`) are hashed into each model profile's task
policy identity: editing them, even to rename a constant, makes every
installed profile stale.

A part's first round may hold one exact lookup per anchor beside its part
search and its sub-needs' reads. A reply does not carry its request, so the
reply check allows `MAX_ANCHORS` (16) such reads in round one for any reply.
The request's call budget still bounds the reads actually made.

A reply sub-need carries `origin`: `caller` when the request stated it, and
absent when the kernel derived it, so replies without caller needs keep their
bytes. A part's sub-needs are all derived or all the caller's. A part may hold
more than the budget's `max_sub_needs` only when its sub-needs are the
caller's and it is the reply's only part.

Tests: `a_request_without_caller_inputs_keeps_its_bytes` and
`caller_inputs_round_trip_and_keep_their_bounds` in `answer_loop::tests`, and
the reviewed schema capture `packages/contracts-gen/arbiter.schema.json`.

Not done yet, each with its owner: the kernel uses `needs` and `anchors`
(kernel `arbiter/CONTRACT.md`); `deliverable` and `known` are accepted and
validated and do not yet change gathering. The public Answer tool's schema is
generated from this request type, so it now advertises the four fields
(`fixtures/split/arbiter/public_answer_cases.json`, 70 lines added). The
description asking callers to fill them and the `task` argument followed in
change-log entry 74 ("Public Answer call" below).

### Public Answer call (user decision, 25 November 2026)

The user chose option A of the completion plan ("Approved public Answer call
for the Stage A baseline"). Owner: `answer_loop::AnswerCallV3`, a schema root
beside `ContextRequestV3`. It is the advertised input of the public Answer
tool (MCP `AnswerArgs` decodes it); the request type, its bytes and its
digest are unchanged.

- **`task` is the only required argument.** Its text becomes the request's
  `question` verbatim, so it has the question's bounds. The service supplies
  `schema` (`airk.arbiter.context.request/3`); a task call may still name it,
  and `validate` refuses any other value.
- **Budget.** Omitted with `task`, the request takes the complete default
  budget (`BudgetV3::default()`, the table under "Budget"). An explicit budget
  passes through with the caller's values and is validated as sent, never
  replaced field by field.
- **The existing full request** (`schema`, `question`, `budget`) is accepted
  unchanged throughout the 1.9 line: it converts to the same
  `ContextRequestV3`, with the same bytes and request digest, as before. It
  still requires its `schema` and `budget` (`answer_request_incomplete`, with
  the decoder's former `missing field` text).
- **Refusals** (`AnswerCallError`, the code first in the message): `task` with
  `question` is `answer_task_and_question`; neither is `answer_task_required`.
- **Caller inputs** `needs`, `anchors`, `deliverable`, `known` and
  `continuation` are advertised beside `task` and pass through unchanged.
- **One conversion.** `AnswerCallV3::into_request` is the only mapping. MCP
  `AnswerArgs` deserializes through it (`#[serde(try_from)]`), so the native
  handler, the public and combined entries, the CLI/session path, the
  supervisor's Admin partition check, the frontend combined Answer and the
  service job's `request_digest` all see the canonical request. A `task`
  call and its equivalent full request are one job.

Tests: `a_task_alone_becomes_the_canonical_request_with_the_default_budget`,
`a_task_keeps_an_explicit_valid_budget_field_by_field`,
`the_existing_full_request_and_its_task_call_are_one_canonical_request`,
`a_call_with_both_or_neither_of_task_and_question_is_refused`,
`caller_inputs_pass_through_a_task_call_unchanged` and
`the_public_call_schema_requires_task_alone` in `answer_loop::tests`; the
public tool path in MCP
`public_answer_sdk_preserves_advertised_schema_full_utf8_question_and_budget`.

### Reply frame: 128 KiB for Answer (user decision, 22 November 2026)

The user raised the Answer reply limit to 128 KiB (plan section 12.9, which
lets execution proceed on recorded decisions). `MAX_ANSWER_RESPONSE_BYTES`
(131,072) is Answer /3 only. A reply, measured on the whole canonical
envelope as transported (`ContextResponseV3::from_canonical`) and on its typed
form, takes at most `min(max_bytes, 131,072)` (`reply_limit`); Union packs
against that. `MAX_RESPONSE_BYTES` (65,536) stays for every other tool and for
Answer /2. The full-loop test (`a_full_default_loop_with_realistic_records_is_shortened_to_the_frame`)
builds the default loop (4 parts, 4 sub-needs, 3 rounds, all 85 decisions)
with 48 search records at the median sizes of 115 real search records from
the one-turn evaluation captures (four hits of about 1,000 bytes with content
hash and location, about 2,300 bytes of metadata, no scope and no Focus
receipt): 6,833 bytes a record. Unshortened, the reply is 393,844 bytes, three
times the frame. Union keeps all 85 decisions and 9 of the 48 records, discloses
the other 39 as omitted evidence and marks the parts shortened: 125,017
bytes, with 6,055 bytes to spare. A full loop therefore always shortens its
records; the cited ids of every omitted record stay in the reply.

Transport ceilings an Answer /3 reply passes, as of this decision. Only the
airk-core Answer constant is raised here; L2 changes the others for Answer.
Caps below 128 KiB that would refuse or cut a full reply are marked **cuts**.

| Ceiling | Where | Bytes | Effect on a 128 KiB reply |
| --- | --- | --- | --- |
| Core reply limit for all tools | `airk-core/src/lib.rs:93` `MAX_RESPONSE_BYTES` | 65,536 | Stays for other tools; Answer /3 uses `MAX_ANSWER_RESPONSE_BYTES` |
| MCP rendered-result check | `airk-mcp/src/lib.rs:1619-1625` | 65,536 | **cuts**: replaces a larger scoped reply with a reply-budget error |
| MCP scope renderer room | `airk-mcp/src/response.rs:36`, `:190` | 65,536 | **cuts** |
| Remote MCP stdio line cap | `airk-mcp/src/remote.rs:1309` (`service::MAX_FRAME_BYTES`) | 65,536 | **cuts**: a longer line overflows |
| Frontend read body budget | `airk-frontend/src/combined_frontend.rs:64`, `:78` | 65,536 | **cuts** |
| Frontend Answer outer bytes | `airk-frontend/src/combined_frontend/answer.rs:182`, `:191`, `:213` | 65,536 | **cuts** |
| Frontend Answer verification | `airk-frontend/src/combined_frontend/answer/verification.rs:25` | 65,536 | **cuts** verification room |
| Frontend console frame | `airk-frontend/src/console_client.rs:113` | 65,540 | **cuts** if Answer replies use it |
| Frontend public envelope | `airk-frontend/src/combined_frontend.rs:51` | 1,048,576 | Fits |
| Kernel Focus reply budget | `airk-kernel/src/arbiter/focus.rs:800` | 65,536 | **cuts** search-Focus room inside Answer |
| Kernel context reply reserve | `airk-kernel/src/arbiter.rs:229` (`context_reply_reserve`) | 4,096 reserved | Reserve only; recheck against 131,072 |
| Service IPC default frame | `airk-core/src/service.rs:34`, `:1305` (`MAX_FRAME_BYTES`) | 65,536 | **cuts** for any Answer path on a default method. The worker query reply of an Answer now uses `MAX_ANSWER_REPLY_FRAME_BYTES` (147,456; `query_reply_frame`) at the worker's write and the frontend's read (change-log entry 53); before, an Answer reply above 64 KiB failed its frame, the worker dropped the stream and the caller saw `scoped_observation_unavailable` |
| Service tools-call and Arbiter frames | `airk-core/src/service.rs:1278`, `:1295`, `:1300` | 4,194,304 | Fits |
| Host session response frame | `airk-host/src/session_protocol.rs:32` | 16,777,216 | Fits |
| airkctl rpc request limit | `airkctl/src/cli.rs:2912` | 65,536 | Request side only |
| VS Code stdio session response | `packages/vscode/src/airkctl/session.ts:30` | 16,777,216 | Fits |

**L2 outcome (22 November 2026).** Raised for Answer only, through
`MAX_ANSWER_RESPONSE_BYTES`: the MCP rendered-result check and the kernel's
scoped reply check it calls (`airk-mcp` `response::reply_limit`, airk-kernel
`annotate_scoped_read_within`), the frontend `ReplyBudget::Context` body
budget, the combined Answer outer bytes and its verification room. Found not
to carry an Answer reply, so unchanged: the remote MCP stdio line cap bounds
the server's *input* (requests), not replies; the console client refuses
`ToolsCall` (`metadata_relay_refusal`), so no Answer reply uses its frame; the
MCP scope renderer room (`response.rs` `focus_reply_headroom`) is Search's
Focus room and `with_scope_observation` wraps only help, scope discovery and
errors. The kernel Focus reply budget (`arbiter/focus.rs`) and
`context_reply_reserve` belong to L3. Time ceilings for Answer are recorded in
the [read-attempt contract](../read_attempt/CONTRACT.md).

### Request

`airk.arbiter.context.request/3` is `schema`, `question` (the /2 bound: not
blank, at most 16,384 UTF-8 bytes), `budget` (`BudgetV3`) and an optional
`continuation`, a ticket or continuation id used to attach to an existing job
(behaviour in L2b; only the shape lands now). The request refuses a
`max_bytes` smaller than the question plus `MIN_REPLY_SKELETON_BYTES` (4,096,
the smallest reply around a question; a test measures it). The public tool
also accepts `task` in place of `schema`, `question` and `budget` ("Public
Answer call"); both convert to this request.

### Answer job options and delivery record (L2c, decided 23 November 2026)

Recorded contract decision (final plan section 12.4.2; the job rules are
`airk-service` `answer_jobs/CONTRACT.md`, "Service-owned jobs"). Both shapes
are owned here (`answer_loop::AnswerJobOptions`,
`answer_loop::AnswerDeliveryRecord`) and are schema roots. They sit **beside**
`ContextRequestV3` and `ContextResponseV3`, not inside them, so the request
digest, the reply's identity and its validation are unchanged.

**Options**, flattened into the `airk_answer` arguments beside the request
(every transport carries the arguments unchanged); all optional:
- `fresh` (boolean, default false): start an independent run; never reuse a
  job by digest. The evaluation harness always sends it.
- `invocation_key` (1 to 128 bytes of `A-Z a-z 0-9 . _ : -`): exact retry
  recovery. The same key and request recover the same job; the same key with
  another request is the typed refusal `answer_invocation_conflict`.
  Workbench uses its turn identifier.
- `wait_ms` (1..=`deadline_ms`): how long this call waits for the job. It
  never shortens the job, whose work budget is `deadline_ms`.
- `interactive` (boolean, default false): the job may run to completion with
  no caller waiting (Workbench, where a person reopens the panel).

A combined Answer member ignores them: it is never a job of its own.

Worked example (request):

```json
{"schema":"airk.arbiter.context.request/3","question":"Where is the reply frame limit enforced?",
 "budget":{"deadline_ms":300000,"max_bytes":262144,"max_parts":4,"max_sub_needs":4,"rounds":3,
   "calls":132,"fanout":8,"candidates":132,"input_tokens":196608,"round_input_tokens":32768,
   "context_calls":48,"focus_calls":24,"focus_pairs":192},
 "fresh":true,"wait_ms":60000,"invocation_key":"turn-7f3a"}
```

**Delivery record**, `delivery` beside the reply in the Answer envelope, for
every reply a service-owned job delivers (absent on the direct path):
`job` (the job's handle; it grants nothing), `queue_ms` (submission to
dispatch, 0 for a reused finished result's own call), `run_ms` (the job's
dispatch to its finish, or to now while it runs), `wait_ms` (this call's own
wait) and `reused` (this call did not make the work it received). The reply's
`elapsed_ms` is this call's own wait, so the reply validates against this
call's own budget; the job's run time is `run_ms`.

```json
"delivery":{"job":"9c1e...64 hex","queue_ms":0,"run_ms":48210,"wait_ms":1520,"reused":true}
```

### One call returns evidence; handle-only continuation (L2d, proposed 23 November 2026)

**Status: design for the primary agent's decision; not built.** Plan section
12.4.2 ("Never a bare ticket", "A continuation returns only what is new",
"Continue with the handle alone"), step L2d. It answers the Workbench
client's questions (branch `loop/workbench-v3`).

**Handle.** The job's handle is `delivery.job` (equal to `ticket.id` on a
ticket). Workbench sends its turn id as `invocation_key` beside the request.
The same key with changed inputs is `answer_invocation_conflict` (L2c).

**Request shapes** (the `airk_answer` arguments; `AnswerArgs` becomes an
untagged choice, so its owners in the frontend, CLI and Admin plane change
with it):
- **Ask:** as today, the `ContextRequestV3` fields and the options.
- **Continue:** `{"continuation": "<job>"}`, optionally with `wait_ms`. The
  question and budget are the job's own; the call is authorized again like
  any call, and an unknown, expired or foreign handle gets the one
  indistinguishable refusal.
- **Cancel:** `{"continuation": "<job>", "cancel": true}`, with the same
  authorization. Typed outcomes: `cancelled`, `already_finished` (the
  result stays available by Continue while kept), `unknown` (also for a
  foreign or expired handle). "Stop waiting" is client-side only and needs
  nothing from the service.

**Reply.**
- **Never a bare ticket.** A call whose job will not finish within its wait
  runs the deterministic (no-model) loop at once, under the caller's own
  kernel and within the wait, and returns that cited evidence. The job keeps
  the model part. The reply discloses `model` (to none, reason `queue_wait`)
  as today.
- **The delivery record gains:**
  - `state`: `queued` (with the ticket: position, estimated start and
    finish, `retry_after_ms`), `running` (with `stage`, `part` and `round`
    from the job's last progress), `finished`, `failed` (the typed failure)
    or `expired` (evicted, past retention or the service restarted; typed);
  - `more`: while the job still has something to deliver, what a follow-up
    would add (`context_ranking`, `focus_ranking`, `rounds`) and the exact
    follow-up call (`{"continuation": "<job>"}`);
  - `delivered`: the record ids this handle has delivered before, kept per
    job in the table.
- **A continuation returns only what is new.** A record already delivered
  under the handle is not repeated: a sub-need may cite it, and the reply
  carries it as a reference row `{record, pointer}`, the naming the reply
  already uses for `same_as` and `overlaps` (`canonical_result.spans`,
  change-log entry 45), with `pointer` naming the earlier delivery
  (`delivery:<job>/<n>`). A client merges one turn by record id. The
  remaining gaps are the parts' gaps, as today.
- **Timing** is the L2c record: `queue_ms`, `run_ms`, `wait_ms`, `reused`.

**Status by handle and the airkctl session.** A Continue with a short
`wait_ms` is the status call: its `state`, `stage`, `part` and `round` come
from the table, so a client that receives no progress notifications (the
airkctl session carries only the heartbeat) can show stages by polling. The
airkctl `admin_call_request` limit of 125,000 ms (Admin Search inside
Explorer) is not changed by this.

Worked examples:

```json
{"continuation":"9c1e...","wait_ms":2000}
{"continuation":"9c1e...","cancel":true}
"delivery":{"job":"9c1e...","queue_ms":1200,"run_ms":30500,"wait_ms":2000,"reused":true,
  "state":"running","stage":"coverage","part":2,"round":2,
  "more":{"adds":["context_ranking"],"follow_up":{"continuation":"9c1e..."}},
  "delivered":["5b0f...","a3d2..."]}
```

Open for decision: whether the immediate deterministic reply is also run
when the owner has no model (it is then the job's own result), and whether
`delivered` is kept per handle (above) or per invocation key.

### Budget

Every field is required and validated, never silently replaced.

| Field | Range | Default | Derivation |
| --- | --- | --- | --- |
| `deadline_ms` | 1..=600,000 | 300,000 | Section 12.4 |
| `max_bytes` | 1,024..=524,288 | 262,144 | Retained while gathering; the reply takes `min(max_bytes, 131,072)` |
| `max_parts` | 1..=4 | 4 | Section 12.1 |
| `max_sub_needs` | 1..=4 | 4 | Per part |
| `rounds` | 1..=3 | 3 | Per part |
| `calls` | 1..=160 | 132 | Round one 4 x (1 + 4 x 6) = 100, later rounds 4 x 2 x 4 = 32; 28 spare |
| `fanout` | 1..=12 | 8 | Peak concurrent reads in one round of one part; default as /2 |
| `candidates` | 1..=160 | 132 | One candidate record per read before dedup, so equal to `calls` |
| `input_tokens` | 1..=380,000 | 380,000 | Attempt pool, user decision of 23 November 2026 (change log entry 50): four parts get 95,000 each (less a quarter of the Split call), above the about 84,900 a part spends over three rounds with evidence-carrying card calls. No multiple is required: the round allowance caps a part-round, it does not divide the pool. Was 196,608 (entry 44) with a maximum of 262,144 |
| `round_input_tokens` | 1..=32,768 | 32,768 | Two batched coverage calls and one follow-up call at 8,192, plus 16 Focus pairs at 512 |
| `context_calls` | 1..=64 | 48 | Card calls (slots): 1 Split + 4 Plan + 12 x 2 coverage + 8 x 2 follow-up = 45 with two-sub-need batching |
| `focus_calls` | 1..=48 | 24 | 4 x 3 x 4 unbatched; default two calls a part-round |
| `focus_pairs` | 1..=384 | 192 | 12 part-rounds x 32, the batch ceiling; default 16 a part-round; 384 x 512 = 196,608 tokens fit the pool |

`round_input_tokens` may not exceed `input_tokens` and `focus_calls` may not
exceed `focus_pairs`. Structurally, round one of a part runs its one part
search plus at most six reads for each sub-need (search, symbols, references,
imports or outline, the governing CONTRACT.md or README, tests); each later
round runs at most one read per sub-need. One Context card call is at most
8,192 tokens. The log holds at most 1 + 4 x (1 + 3 x 4 x 2) = 101 decisions.

### Reply

`airk.arbiter.context/3` with packet `airk.arbiter.packet/2`.

- **Kept from /2:** `question`, `records` (deduplicated canonical reads with
  exact arguments and intact canonical results), `paragraphs` (grounded),
  `work`, `candidate_count`, `omitted_count` and `applications`, plus `scope`
  (the one index partition, as /2's `assessment.scope`).
- **Where the other /2 fields went:** `gaps` became per-part `gaps` with the
  same `Gap` kinds (definition, constraint, example, test, counterevidence,
  ambiguous, unsupported, sufficient) beside the sub-need kinds; `stop`
  became per-part `stop`, which keeps the /2 terminals clarification_required
  and no_justified_read; `assessment_history` became `decisions` (compact)
  plus the custody log; `assessment` became the parts' coverage. The /2 rules
  carry over: sufficient only with covered, covered means exactly
  `[sufficient]`, clarification needs ambiguous, no_justified_read needs a
  gap. Each decision keeps a compact basis (presented and omitted source unit
  counts and a basis digest) with the full EvidenceRef list in the log, its
  typed `AssessmentUnavailable` reason for abstained and unavailable sources,
  `work_unknown`, and validated applicability: only the latest decision per
  (part, sub-need, stage) may be current.
- **parts:** numbered from 1 in question order, each with its UTF-8 `span`
  (ordered, not overlapping, on character boundaries), its deadline share
  (shares sum to at most the deadline), sub-needs, gaps, per-round provenance
  (`calls`, `fanout`, observed `input_tokens`, `new_evidence`,
  `follow_ups_run`, `sub_needs_covered`), its `stop` (covered,
  clarification_required, no_justified_read, no_progress, rounds, budget,
  deadline, stale, refused, unsupported) and `shortened`.
- **sub-needs:** numbered from 1 in their part, own query `text`, `kind`
  (definition, callers, tests, contract, documentation, configuration,
  other), `coverage` (covered and partial cite evidence, missing cites none),
  cited record ids, `evidence_omitted` and `unrun_reads`, each with a reason
  (deadline, budget, round_limit, not_chosen) consistent with the part's
  stop. A cited record may be retained or listed in the reply's
  `omitted_evidence` (ids the frame omitted); `evidence_omitted` counts the
  latter. A covered sub-need has no unrun reads.
- **record_uses:** every (record, part, sub-need, round, cause) finding of a
  retained record; causes are part_search, need_search, structural_read
  (round one), follow_up (later rounds) and shared_read.
- **union:** `repeats_merged` and `overlaps_trimmed` count the span rows
  whose text the reply carries once in another row ("One text, one copy in
  the reply"). `dedup_count` equals the findings merged into a record another
  part also found when nothing was omitted, and is at least that otherwise;
  `conflicts` are divergent_definition (two or more records) or
  divergent_verdict (one or more), each naming two or more parts that found
  its evidence. Conflicts are noted, never resolved.
- **partial** (required, nullable) names the stage and reason (queued,
  deadline, budget, bytes, stale, refused), an optional part and an optional
  continuation. A part stopped on budget, deadline, stale or refused, or any
  omission, requires it. Its reason must be observed: a part's stop, an
  omission for bytes, or Compose or Union running out of deadline or budget
  with no part stopped by it.
- **ticket** (required, nullable): id, queue position, estimated start and
  finish and `retry_after_ms`. A ticket reply carries no parts, records,
  decisions or work and is marked partial at admission with reason queued.
- **degradations:** context_absent (no card call was dispatched: every
  logged decision is deterministic or not_dispatched),
  context_abstained {count, equal to the logged abstentions}, focus_absent
  (present exactly when no Focus ran: no batched call, and no search Focus,
  which the loop defers), model_share {part, stage},
  model_outcome_unknown (the log has unknown work), configuration_changed
  (the only way an application may name another configuration revision),
  batch_split, search_focus_deferred, and the 12.4.1 degrade-to-fit kinds
  parts, sub_needs and rounds (the applied value then bounds the reply),
  model (Large to Compact, or any model to none; the degraded model then
  decides nothing among the included decisions) and device (a GPU to the
  CPU). One per kind; model_share once per part and stage.
- **decisions** (compact `DecisionRecord`): ordinal (the position in the whole
  log, 1..=101), stage (split, plan, coverage, follow_up), part, need, round,
  `roster_digest`, `card_slot` (present only for model-scored decisions, 1..=`context_calls`,
  shared by the segments of one batched call), chosen cards (symbol and class;
  several only for Plan, at most four), source (model {mode rule or
  top_score, rule}, abstained {fallback, reason}, unavailable {fallback,
  reason}, not_dispatched {fallback, reason}, deterministic {rule}), top and
  runner-up raw scores and their
  margin (exactly top minus runner-up), model, device, precision (GPU only),
  `batched_with` (symmetric, same slot and receipt), applicability, receipt
  id, basis and `work_unknown`. Only Compact or Large score cards; Large never
  runs on the CPU. A deterministic decision carries no model facts; a
  not_dispatched one carries only the model that was present.
- **decision_log** (`DecisionLogSummary`): the custody log's digest, total,
  included and omitted counts, card calls (distinct slots), counts per stage
  and source, Focus calls and pairs, model tokens per part and round
  (`attribution`), measured timings and unknown-work count. The reply's
  decisions are a subset with increasing ordinals. Union omits decisions
  oldest deterministic first (the no-model ones and the ones not
  dispatched): a decision of a dispatched call may be omitted only when none
  of those remains included. ContextAbstained,
  `model_work`, slot counts, Focus work and round tokens reconcile against
  these counts, not against the included list.
- **model_work:** card calls, Focus calls and Focus pairs, equal to the log's,
  and `focus_profile` (required, nullable), the one profile of the logged
  identified Focus receipts (see "Combined Answer /3 allowance").
- **Focus pairs are pairs dispatched** (change-log entry 53): the summary's
  `focus_pairs` is the sum of each Focus receipt's `candidate_count`, the
  pairs the call reserved and the budget's `focus_pairs` bounds, as the
  kernel and service ledgers count them. A receipt dispatches 1 to 32 pairs
  and scores at most those (its `pairs`, in custody); a call that scored
  fewer is not applied and its spans stay unscored, disclosed by the kernel.
  So every call has a pair (`focus_calls <= focus_pairs`) and at most 32
  (`focus_pairs <= 32 x focus_calls`). Each bound's refusal names itself and
  its figures, as do the card-call, round-token, pool-token and original
  work bounds.
- **records:** a v3 record (`LoopEvidence`) carries no scope: it is always
  the reply's `scope`, so 48 records do not repeat 587 bytes each. A loop
  search defers in-search Focus (L3 design), so a search record carries no
  native Focus receipt: `observed_focus_work` over the search records must
  report no receipt, no work, no unknown work and no token overrun. Focus in
  the loop is the batched Focus of the decision log only.
- **Accounting:** `work.calls` is the sum of round reads, `work.fanout` their
  peak and `work.rounds` the most rounds of any part. `work.input_tokens`
  retains every round's tokens and the Split and Plan tokens (round 0); each
  part-round charge is retained by its round. `elapsed_ms` is the wall-clock
  time from admission to the reply, set by the kernel; it may exceed
  `deadline_ms` by at most `DEADLINE_TOLERANCE_MS` (1,000 ms, for Compose,
  Union and writing the reply after the last deadline check). Summed model
  timings are disclosure only: they must be retained in `work`, but they
  overlap across batched and concurrent calls and are not compared with the
  deadline. `candidate_count` is at most `work.calls`.

### One text, one copy in the reply

Recorded decision of 22 November 2026, on the user's rule "The Answer
plumbing should ignore duplicate or repeated facts. It just wastes space."
(change log entry 45). It changes what a record's projection carries and
adds two counts; no location and no record is ever dropped by it.

- **Where the text is:** each retained record's `canonical_result` is the
  kernel's span projection (`airk.arbiter.loop-evidence/1`): one row per
  hit, with its `pointer`, `path`, `start_line`, `end_line` and
  `content_hash`. A row carries the hit's `text`, or names the row that
  does.
- **Exact repeats:** of rows with the very same text, one carries it. The
  others have no `text` and a `same_as` object `{record, pointer}`: the
  record id and hit pointer of the carrying row. This holds across
  sub-needs, rounds and parts, because the rule runs over every row of the
  reply.
- **Which row carries it:** the highest-ranked one. A row with a Focus
  relevance ranks above one without, a higher relevance above a lower one,
  then the earliest read, then the earliest hit of that read.
- **The same text at another place:** the row at the other place is a
  repeat like any other and keeps its own location. The carrying row lists
  every other place in `also_at`, as `path:start-end` strings.
- **Overlapping spans of one file:** two rows are in one region when their
  path and content hash are equal and their line ranges meet. Among such
  rows the longer text keeps what they share (ties by the rank above).
  - A text found whole in a longer one has no `text`, `same_as` the longer
    row and `within: true`.
  - A text that shares its head or tail with one is carried without those
    bytes, and `overlaps` lists `{record, pointer, bytes, side}` for each
    part trimmed, with `side` `head` or `tail`. The row's whole text is its
    `text` with those parts of the named rows put back.
  - An overlap shorter than 32 bytes is left alone.
  - A row already trimmed is never `within` another, and neither is a row
    whose `also_at` lists places: such a row keeps its own text (after the
    review of `5f971a70`, M1 and L2).
  - Parts are put back in `overlaps` order: head parts before the text,
    tail parts after it. A `same_as` row without `within` has the named
    row's whole text, rebuilt this way.
- **Only bytes that are delivered:** overlaps are trimmed, and `within` is
  used, only while the reply sends every carrying row whole (Union's
  trimming levels 0 and 1). From the level where Union cuts the excerpts
  it sends, only exact repeats are merged: a repeat's text is its carrier's
  as sent. A row may name another only for bytes that row really delivers
  (after the review of `5f971a70`, H1).
- **Location and `truncated`:** a row's `path`, `start_line`, `end_line`
  and `content_hash` always describe the hit's whole original span, also for
  a trimmed row and a `same_as` row. Lines must never be derived from
  `text`, which may be a part of the span. `truncated` is true whenever the
  text delivered for the row, in its own `text` or through the row it
  names, is less than the hit's text.
- **Exact only:** every comparison is of bytes. Texts that differ in one
  byte, in indentation or in line endings are different texts and are both
  carried. Near-identical texts are not merged.
- **Counts:** `union.repeats_merged` is the number of rows without `text`,
  and `union.overlaps_trimmed` the number of rows with `overlaps`. Both are
  absent when zero and at most 4,096 (`MAX_MERGED_SPANS`). They count the
  rows of the reply as sent; Core does not recompute them, because a
  projection is opaque to it.
- **Budget:** the reply frame is measured on the reply as sent, so a merged
  repeat costs its reference and not its text, and Union reaches its
  trimming levels and record omissions later or not at all. Retention while
  gathering (`max_bytes`) is unchanged: it counts what was read, and
  custody keeps every canonical result intact.
- **Every path and transport:** the rule is part of Union, which builds the
  one reply for the no-model path and the model path and for every
  transport.
- **Generated views:** the arbiter schema and the generated TypeScript views
  carry the two counts. The projection rows are JSON inside
  `canonical_result` and are not typed there.

### Record identity without per-read status

Recorded decision of 23 November 2026, a contract amendment made by the
primary agent under the standing authorization of plan section 12.9
(changes to Answer's reply and its digests when section 12 needs them), on
the user's rule that Answer carries no repeated facts (change-log entry 54).
It amends the Answer /3 reading of the record rule above ("intact canonical
result; its identity binds both", "never removes fields", the first
search's "full execution/readiness metadata").

- **Why:** two runs of one Answer at one publication must give the same
  records, read sizes, basis and decisions. Every canonical read result
  carried its own `request_id` and the freshness and readiness observations
  its transport attaches after the read. Those differ on every run, and one
  of them (`discovery_readiness` and `semantic_readiness`, or
  `discovery_observation` when the observation failed) changed a record's
  size by 491 bytes when a concurrent observer met contention. Record ids,
  `full_digest`, the basis digests, the decision log digest, paragraph
  evidence ids and the verification selection digest all followed.
- **Volatile fields:** `VOLATILE_READ_FIELDS` in `answer_loop.rs` lists
  them in one place: `request_id`, `freshness`, `discovery_readiness`,
  `semantic_readiness`, `discovery_observation` and `indexing`, at the top
  level of a result (and of a failure receipt's `structuredContent`). A
  field found to differ between two runs of one read must be classified
  there; the MCP test `answer_reads_vary_only_in_classified_status_fields`
  fails until it is.
- **What the identity covers:** a record's id binds the dispatch key and
  the result without the volatile fields (the stable form). `full_bytes` is
  the serialized length of the stable form and `full_digest` its digest;
  retention accounting while gathering (`max_bytes`, omission) counts the
  stable form too, so two runs keep or omit the same records. `work.bytes`
  is therefore smaller than before this change by each retained read's
  status bytes; measurements on either side of it are not comparable.
- **What custody holds:** the complete result as received, status
  included. Nothing is removed from what custody keeps; only the identity
  and the reply leave the status out.
- **One status per reply:** `read_status` carries `READ_STATUS_FIELDS`
  (the volatile fields except `request_id`) of the mandatory first search
  once, as received; no record repeats them and no id or digest binds
  them. It is optional: absent when the first search carried none. The MCP
  render keeps it in full mode or with `include_status=true` and drops it
  from a terse reply otherwise. The first search's execution metadata
  (everything that is not per-read status) stays in its record's identity.
- **Validation:** Core accepts a reply with or without `read_status`; a
  present one is non-empty and carries only `READ_STATUS_FIELDS`. The
  arbiter schema and generated TypeScript views carry the optional field.
  The saved-source verification selection digest leaves `read_status` out,
  as it leaves the other status fields out.
- **Standalone reads and Parse:** unchanged. A tool called directly keeps
  its status fields, and Workbench steps keep the full MCP result.
- **Readiness observation:** an MCP read's readiness annotation that meets
  typed contention is repeated within two seconds; if contention lasts, the
  read fails with its typed `repeat_same_request` hint instead of carrying
  `discovery_observation` in place of the readiness observation.

### Decision log (service custody, retained by L2b)

`DecisionLog` (`airk.arbiter.decision-log/1`) holds the question, scope, one
`DecisionLogEntry` per decision (the compact record, task policy, full cards
with symbol, class, text digest and Split partition, all raw scores, chosen
indexes, and the presented and omitted EvidenceRefs of the basis), every
`CardAdvice` and every `FocusBatchReceipt`. Its validation: contiguous
ordinals; at most one Split decision; only the latest decision per (part,
sub-need, stage) is current; the compact record agrees with the entry (chosen
cards, top and runner-up, presented and omitted basis counts and the basis
digest over both, and `roster_digest`, recomputed from the stage and every
card record, so a moved or swapped Split partition fails); a Split card is a
whole partition of the question into one (KeepWhole) or two to four
(SplitAt) contiguous, non-overlapping, covering ranges; decisions of one slot
name one receipt; each receipt's task, policy and slot match; every
decision's part and round is charged in its receipt; decisions sharing a
receipt name each other; every receipt segment maps to a decision naming
that receipt and agrees with its outcome, scores, rule and margin; Focus
receipts are distinct and within the Focus budget. A native card's
`text_digest` covers its text and partition, and a native roster digest is
the same `roster_digest` over the cards' log records.

`DecisionLog::summary` produces the reply's `decision_log`; its counts are
per stage, source and model, so a Large-to-Compact degradation is checked
against every logged decision. An empty log (no decisions, receipts or Focus
calls) has the fixed digest
`ab089e739cf3e4ed3e1b500ace2b8f30b3ecf32c0dcbfe1a5c72de5e4a80ae63`
(`empty_log_digest`, framed domain `airk.arbiter.decision-log.empty/1` with
no fields), whatever its question and scope; a reply with an empty summary
must carry it, and no other summary may.

`ContextResponseV3::validate_with_log` checks a reply against its log, and
the kernel must call it before delivering an Answer /3 reply (L3). It
validates the reply and then requires: the log's question and scope equal the
reply's; the log is valid; the reply's summary digest is the log's digest and
the summary equals `log.summary`; every logged decision's part, sub-need and
round exist in the reply; every included decision equals its logged
decision; the omitted decisions are exactly the oldest deterministic ones,
then the oldest model-scored ones; and the chosen Split partition has one
range per reply part, each containing its part's span, unless a Parts
degradation explains the difference.

### Card advice and native inputs

`Task` gains `airk.context_split.v1`, `airk.context_plan.v1`,
`airk.context_coverage.v1` and `airk.context_follow_up.v1`, one policy each in
`Profile.task_policies`. `Stage::permits`, `SelectedModel::supported_tasks`
and enrollment are unchanged; admitting the new tasks is L4's. `Decision`
gains `loop_card` and `loop_cards`; Advice /1 and advice /2 refuse them.

**Which tasks a profile may declare (`Stage::declares`, change log entry
46).** A profile of a stage may declare, and a status row of that stage may
list, the stage's enrollment task and, for Context, the four card tasks.

- `Profile::validate_stage` (and so `Profile::validate`),
  `StageStatus::validate` and the device status rows of
  `device/state.rs` use `declares`.
- `Stage::permits`, enrollment (`StageConfiguration::validate`) and
  eligibility still name only the enrollment task. The card tasks run under
  the Context enrollment (change log entry 31), so no persisted
  configuration names them.
- Core does not say which tasks a Context profile declares together. The
  retrieval admission owner does: the one-turn task, or exactly the four
  card tasks, never both.
- A Focus profile that declares a card task, and a Context profile that
  declares the Focus task or a task of no current stage, are refused as
  before ("Arbiter profile has unsupported stage task").
- Until this rule, Core refused every profile that declared a card task,
  so a profile from `admit_loop_tasks` could not be validated, enter a
  catalog or open a scorer on any device.

`CardAdvice` (`airk.arbiter.advice/3`) is one batched card call: the advice
/2 bindings, its `card_slot` (1..=64), reserved and observed tokens (at most
8,192), a per-part and per-round attribution equal to the observed tokens,
and one segment per decision ordinal (1..=101) with raw scores and outcome.
`NativeCardRequest` (`airk.arbiter.advice.request/3`) carries a BudgetV3
parent, one task and policy, up to 16 segments that share one `card_slot`
and name their decision ordinals, a reserved attribution that charges every
segment's part and round and nothing else, and an optional
`PartitionAllowanceV3`. `NativeFocusRequestV3` carries up to 16 distinct
questions and up to 32 scored candidates that name their question by index;
the candidate identity (`airk.arbiter.focus.candidate/2`) hashes the
candidate's own question text.

**Evidence, verdict and calls not dispatched (recorded wire decisions, 22
November 2026; change log entries 43 and 44).** A Coverage or Follow-up
`CardSegment` carries `evidence`: the spans gathered for its sub-need in this
round, as `CardEvidence` rows.

- **Why:** the Coverage question asks which verdict fits a sub-need "given
  only its shown evidence", and no evidence was shown (entry 43). A Follow-up
  roster asked for the read that best closes a gap without saying what kind
  of sub-need it was, how far it was covered or what had been read (entry
  44).
- **A row:** `path`, `start_line`, `end_line`, an optional `focus` relevance
  in 0..=1, `excerpt`, `skipped_bytes`, `cut_bytes`, `truncated` and an
  optional `same_as`.
  - `excerpt` is the shown part of the span's text, on character boundaries.
    `skipped_bytes` and `cut_bytes` are the bytes of that text before and
    after it, which are not shown; together at most the retention ceiling.
  - `truncated` repeats `Hit.truncated`: the read itself returned only part
    of the text there.
  - `same_as` is the position of an earlier row of the same call that shows
    the same text, counted over all the call's rows from 0, segment after
    segment in request order. Such a row carries no excerpt and no hidden
    bytes. The row it names must come earlier in the call and must itself
    show text. (Until step LH it was the row's place, which two rows can
    share: review L-b, change-log entry 62.)
- **Which segments:** Coverage and Follow-up only, at most eight rows each.
  Split and Plan segments are refused with evidence, a verdict or an unshown
  count. A segment without spans carries none; the fields are then absent on
  the wire, so a request without them serializes and hashes as before.
- **Order:** Focus rank order. Scored spans come first by falling relevance,
  then the spans Focus did not score.
- **`unshown`:** the gathered spans a call shows nothing of, at most eight.
- **`verdict` (Follow-up only, where it is required):** `coverage` (`partial`
  or `missing`; a covered sub-need has no follow-up) and `by_model` (a
  model's Coverage choice set it; otherwise the deterministic order did).
- **Allowance:** the excerpts of one call total at most
  `evidence_allowance(remaining_input_tokens)` bytes: five bytes for every
  four reserved tokens above `CARD_FRAME_TOKENS` (2,048). The 8,192-token
  reservation allows 7,680 bytes, 960 for each of eight spans. A repeat row
  takes none of it. `NativeCardRequest::validate` refuses a request above its
  allowance.
- **Least excerpt:** a cut excerpt holds at least `MIN_EXCERPT_BYTES` (256);
  a shorter one runs to the end of its span's text. 256 bytes is about six
  lines of code or one short paragraph, the least a model can judge by. The
  kernel does not make a call that cannot give every span that much.
- **No new budget shape:** the bound comes from the reservation the call
  already holds. The loop ledger and the per-part, per-round allowance are
  unchanged.
- **The measurements behind the ratio and the frame** (entry 44;
  `scripts/arbiter/evaluation/card_excerpt_tokens`), with the gate profiles'
  tokenizer on the gate's two repositories, in 960-byte windows quoted as
  the view quotes them:
  - munarium: median 0.374 tokens a byte, p99 0.669, p99.9 0.753, maximum
    0.863;
  - bookbuilder-v2: median 0.308, p99 0.585, p99.9 0.746, maximum 2.000 (43
    of 94,879 windows, number tables and runs of escaped characters, need
    more than 0.895);
  - the view around the excerpts of a two-sub-need call with eight cards and
    eight located spans is modelled at 1,269 tokens (Coverage) and 1,321
    tokens (Follow-up);
  - so a full call fits its reservation while its excerpts average at most
    0.895 tokens a byte. A call of only munarium's densest window uses 7,948
    of 8,192 tokens. A call above its reservation is refused by the lane's
    token ceiling before any model work, like any oversized input, and the
    decision takes the deterministic order.
- **Binding:** the request's `input_digest` covers the evidence, the unshown
  count and the verdict, so a receipt binds exactly what was shown.
- **What the log's basis means:** an entry's `basis` and `basis_omitted`
  (and the reply's compact `basis` counts) describe what the decision that
  stands rests on.
  - When a model's choice stands: `basis` is the byte range of each span the
    model was shown, and `basis_omitted` the ranges of those spans it was
    not shown (before and after an excerpt) and every span not shown at all.
  - When the deterministic order decided (no model, a call not dispatched,
    refused or lost, or an abstention): `basis` is the whole spans that
    order read, and nothing is omitted. The excerpts a refused, lost or
    abstained call was built with are not the basis of the decision.
- **A call not dispatched (`not_dispatched`):** `DecisionSource` and
  `SourceKind` gain this kind. It records a roster for which a Context model
  was present but no card call was sent, with the deterministic `fallback`
  rule and a `reason`:
  - `token_budget`: no card slot, token share or round allowance was left,
    or the reservation left the spans less than the least excerpt;
  - `deadline`: the part's model time for this barrier was below the
    estimated call time, or the loop had reached its Compose and Union
    reserve;
  - `unavailable`: the model was not usable (blocked after a lost or
    declined call, a changed configuration, or a stage that is not ready).

  Such a decision names the model that was present and carries no slot,
  device, scores or receipt. `deterministic` now means only the no-model
  path: no Context model takes part, or the roster holds one card. A reader
  can count, per stage and model in `decision_log.counts`, the decisions a
  model took (`model`), the ones it declined (`abstained`), the ones whose
  call was sent and refused or lost (`unavailable`), the ones whose call was
  not sent (`not_dispatched`) and the no-model ones (`deterministic`).
- **Generated views:** `CardSegment`, `CardEvidence` and `CardVerdict` are
  private native inputs outside the generated schema. The decision source
  is public: the arbiter schema, the generated TypeScript views and the
  Rust-emitted semantic cases carry `not_dispatched` and the reworded basis
  descriptions.
`PartitionAllowanceV3` (`airk.context-partition-allowance/3`) is a recorded
wire-shape decision: attempt, selection, index, a BudgetV3 parent, resources
within that parent, and strictly increasing `card_ordinals` (card slots,
1..=`context_calls`) and `focus_ordinals` (1..=`focus_calls`); model tokens
are allocated exactly when either list is non-empty. The /2 allowance is
unchanged.

### Combined Answer /3 allowance (recorded contract decision, 22 November 2026)

Recorded by the primary agent under plan section 12.9 (authorization to
change Answer's wire shape and budgets, replacing v2 at once), amended on the
L3 implementer's proposal. **Why:** the combined (multi-index) Answer hands
each member a partition allowance, and the /2 `PartitionAllowance` has a /2
`Budget` parent and one *global* assessment ordinal per assessment. In /3
every member runs its own Answer, whose card slots and Focus call ordinals
are numbered from 1 per reply, so global ordinals have no meaning; a second
/3 allowance type beside L1's `PartitionAllowanceV3` would duplicate it.

- **Reuse `PartitionAllowanceV3`** (`airk.context-partition-allowance/3`,
  `native_loop.rs`) for each member. Its `card_ordinals` and
  `focus_ordinals` must be exactly the prefixes `1..=k` (counts in effect,
  with no cross-member meaning); its resources become `LoopResources`:
  calls, input tokens, candidates, bytes and **`focus_pairs`** (new). Focus
  calls require pairs (`focus_ordinals` empty exactly when `focus_pairs` is
  0, and calls at most pairs); model tokens are allocated exactly when a
  list is non-empty. A batched Focus request under a partition may score at
  most `focus_pairs`.
- **Clamping.** `PartitionAllowanceV3::clamp` bounds a request budget by the
  allowance (fields a BudgetV3 requires to be positive stay at least 1; the
  allowance carried by every card and Focus request refuses what it does not
  grant). The kernel's `answer_with` applies it to whatever budget the member
  request names, and card and Focus requests carry the allowance.
- **Initial allocation** (`combined_context::initial_allocations_v3`): reads,
  candidates and bytes are shared over the admitted members; card slots,
  Focus calls and pairs over the members that can carry model work (at most
  `max(context_calls, focus_calls)` and the token pool), which also share the
  token pool. A member with no Focus calls gets no pairs. Nothing is minted:
  the sum fits the parent BudgetV3.
- **Hand-back by count** (`LoopUse`): a member's use is `work.calls`,
  `work.input_tokens`, `max(candidate_count, work.candidates)`,
  `max(work.bytes, reply bytes)` and the decision log's `card_calls`,
  `focus_calls` and `focus_pairs`. Unused counts (allowance minus use) grow
  the next member's allowance (`grow_allowance`) or make one for a member the
  initial allocation could not admit (`returned_allowance`). A member reply
  with unknown model work (`decision_log.work_unknown > 0`) or no valid reply
  keeps its whole allowance charged, and later members get no model work
  (`without_models`).
- **Summary `airk.combined-context/2`** (`CombinedContextSummaryV2`): the
  BudgetV3 parent, one `PartitionReportV3` per member (allowance and outcome
  `unqueried`, `known {used, read_depth, peak_fanout}` or `unknown`), known
  and held `LoopUse` totals, termination and delivery bindings. A /3 member
  reply is validated under its clamped budget, so it cannot exceed its
  allowance; there is no known-violation outcome and the summary refuses the
  `accounting_violation` terminal. The combined wrapper's `context` field is
  `CombinedContext`, `/2` for Answer /3 or `/1` for Answer /2 until /2 is
  deleted.
- **Delivery bindings** (`reply_delivery_bindings`): Focus when the reply's
  `model_work.focus_profile` names the Focus profile, Context when any card
  call was made, with the profile of the reply's own application; both bind
  the reply's scope. `context_delivery_bindings` reads them from a singleton
  /3 reply and re-validates every member reply of a `/2` summary under its
  clamped budget.
- **Frame and time:** the combined Answer takes the 131,072-byte Answer frame
  and the 600 s Answer ceiling, for Answer only. In the wrapper, a member
  that ran declares the summary's budget deadline as its
  `allocation.deadline_ms` (300,000 ms by default, above the 120,000 ms
  request clock that every other combined read shares among its members).
- **Reply addition: `model_work.focus_profile`** (required, nullable). The
  batched Focus receipts stay in custody, so a delivery owner holding only
  the reply could not bind the Focus stage. The reply now names the one
  Focus profile its identified receipts ran on; `validate` refuses it without
  Focus calls and `validate_with_log` requires it to equal the logged
  receipts' single profile. Change log entry 28.

Landed in `594e9fbf` (mislabelled; see change log entry 28) and completed by
the next `Complete L3-10 (a)` commit.

Closed by the independent review of L3-10 (a), in the L3-10 (b) commit
(change log entry 40):

- **A configuration change after card work never delivers (finding 4).**
  Choice: the typed stale refusal, not a second binding. Every card call
  records a non-deterministic decision (model, abstained or unavailable), so
  `Ledger::has_native_work` holds after any card or Focus call, and a
  configuration change before delivery ends the Answer with
  `AnswerPostExecutionStale` (`arbiter_context_action_stale`, not retryable,
  not replay-safe). A delivered /3 reply with card calls therefore always has
  its application on the reply's own scope, and `reply_delivery_bindings`
  refuses one that claims otherwise. The finding's scenario did not
  reproduce; the existing kernel test
  `configuration_change_after_native_work_is_refused_without_replay` covers
  it, and `delivered_card_work_binds_context_to_its_own_application` was
  added.
- **Model shares over the members that can carry model work (finding 6).**
  `initial_allocations_v3` now shares card slots and Focus calls over the
  model members, and Focus pairs over the members that get a Focus call, as
  the bullet above says (before, they were shared over every admitted member
  and the rest stranded). Test
  `initial_v3_allocations_share_every_counter_with_ordinal_prefixes`.
- **No model work after unknown model work (finding 7).** Members run in
  roster order; `CombinedContextSummaryV2::validate` refuses a member after
  an `unknown` outcome whose allowance keeps card or Focus ordinals, tokens
  or pairs. Test `combined_context_v2_summary_reconciles_known_and_held_counts`.
- **A member needs the question plus a minimal reply (finding 3).**
  `returned_allowance` takes the caller's minimum (`question.len() +
  MIN_REPLY_SKELETON_BYTES`); the frontend admits a member only when its
  share of the frame holds it, and reports the rest unqueried.

No airk-state/1 persisted format changes: the allowance travels in the
read-attempt context and the host query, which are not persisted.

### Compact Focus receipt

`FocusBatchReceipt` (`airk.arbiter.focus.batch/1`) binds a scope digest,
optional identity, a reason of at most 256 bytes, input, order and questions
digests, the ranked permutation of candidate indexes, at most 32 (candidate,
question) pairs of two finite logits, tokens and their attribution. It is
bounded by 4,096 + 6 per candidate + 96 per pair; a full 32-pair batch over
100 candidates is within 12,288 bytes of custody, against the full receipt's
12,288 + 136 per candidate + 512 per score.

### Loop calibration

`LoopCalibration` (`airk.arbiter.loop-card.calibration/1`) binds model, card
task, compiler, task policy, score ABI and the complete trained asset map,
with at most 32 rules `{class, minimum_raw_margin}`, one per class of the
task's stage; with zero rules every decision abstains. `LoopCalibrationSet`
(`airk.arbiter.loop-card.calibration-set/1`) is one document per Context
profile holding exactly one LoopCalibration per card task the profile admits,
in card-task order. The profile keeps its one Calibration asset: the set's raw
bytes are that asset, and their SHA-256 is both the asset digest and the
profile's `calibration`, so the Profile shape is unchanged. The release ships
every entry with zero rules; rules are fitted from L7.

### Strictness

Unit variants of internally tagged enums (for example `context_absent`) keep
v2 parity: their strictness is serde's for such variants, as for the /2
tagged enums. No new strictness is claimed for them.

### Digest identities

No digest input changes. The Context semantic closure (`context_action.rs`,
`context_action/calibration.rs`, `context_action/calibration/terminal.rs` and
the Focus digest) and the Focus closure (`focus.rs`, `native_focus.rs`,
`device/receipt_runtime.rs`) keep their bytes; version 3 lives in new files
(`arbiter/answer_loop*`, `native_loop*`). The current Focus and Context
task-policy digests and the `cpu_source_transition` pins do not move in L1.
The new files are outside both closures until the loop's task policies are
hashed: L4 must bind the card and batched-Focus semantics into the new task
policies' identities when it activates them, and moves the pins then through
their own recorded decision. `mod.rs` and `schema.rs` changed and are not
digest inputs.

L4 (22 November 2026) adds `answer_loop/semantics.rs`:
`loop_semantic_policy_digest` (`airk.arbiter.loop-core-semantics/1`) frames
the LF sources of `answer_loop/cards.rs`, `calibration.rs`, `focus_batch.rs`,
`log.rs`, `semantics.rs` and `native_loop.rs` with the Focus Core semantics.
The retrieval loop task policies hash it, so editing any of these files now
moves every loop task identity and needs a recorded decision (values in
`crates/airk-retrieval/src/arbiter/compiler/CONTRACT.md`). It is a new
closure; the Context and Focus closures and their digests are unchanged.

The evidence decisions above edit digest inputs, so the loop semantics
digest and every loop task identity moved with each. Entry 43 edited
`native_loop.rs`. Entry 44 edited `native_loop.rs`, `answer_loop/cards.rs`
(the `not_dispatched` source) and a doc comment of `answer_loop/log.rs`;
`calibration.rs`, `focus_batch.rs` and `semantics.rs` did not change. The old
and new values are recorded in the retrieval compiler contract ("Fourth
move" and "Fifth move").

L4 also adds `native_loop::NativeCardModel` (the selected Context model,
device and GPU precision a card call would use; observation only, in
`native_loop/card_model.rs` so it stays outside the loop semantics digest,
review L4) and the
private control methods `ArbiterCardModel`, `ArbiterScoreCards` and
`ArbiterFocusBatch` with their nullable responses `ArbiterCardModel`,
`ArbiterCardAdvice` and `ArbiterFocusBatch`. They are refused by the
metadata relay and use the private Arbiter request and response frames.
`PROTOCOL_VERSION` is unchanged: the variants are additive and only the
same-version native supervisor sends them.

### Generated views

`arbiter_contract` becomes 3. The Rust owner regenerates
`packages/contracts-gen/arbiter.schema.json` (`AIRK_WRITE_CONTRACT_SCHEMA=1`);
`scripts/dev/contract_types.py arbiter --write` then renders `arbiter.ts` and
`packages/vscode/src/generated/arbiter.d.ts`. New roots: ContextRequestV3,
ContextResponseV3, DecisionLog, FocusBatchReceipt, LoopCalibration and
LoopCalibrationSet; AnswerCallV3 joined them with change-log entry 74. Rust-emitted semantic cases live in
`fixtures/split/arbiter/answer_loop_cases.json`
(`answer_loop_semantic_case_emitter`, written only with
`AIRK_WRITE_ANSWER_LOOP_CASES=1`): the default budget, each field's minimum
and maximum with their neighbours, the valid two-part reply and ticket, and
every refusal as JSON-pointer replacements of the valid reply with its
expected error. All are qualification false. `download_origin_cases.json`
embeds a schema snapshot written only on request; it is not compared and was
not rewritten.

## Context ladder (plan 12.5), retired in step LH

Step LH removed the ladder with the Large model (change-log entry 62): the
Context stage has one model, so there is no step from one model to another.
`smaller_model_selected` stays in the reason vocabulary, reserved and never
emitted. The text below records what the ladder was.

`device::context_ladder` read the finished Large and Compact selections and
names the rung: Large on the GPU, then Compact on the GPU, then Compact on the
CPU. Large never runs on the CPU: a Large CPU selection is refused with a
`no_qualified_gpu_route` row. A step down from Large adds the
`smaller_model_selected` row, the existing reserved reason, so the wire list is
unchanged. Every reason is kept in order within the status limit, and the
step-down rows always survive truncation: the Large-on-CPU refusal row and the
`smaller_model_selected` row are kept first, then the earlier and later reasons
fill the room left. With no rung, the outcome is `None` and Answer runs the
no-model loop. The function is advisory like `select`.

- **Holds:** the rung rule and its typed reasons, tested in
  `device::ladder::tests` (7 tests).
- **Does not hold yet:** the service's product device selection does not call
  it, because no product CUDA Context route exists to step down from.
  Follow-up owner: the Compact FP16 product route (L5, design item 1.6).
- **Closed (L5C review L2):** the first version could drop the Large-on-CPU
  refusal row when many Large reasons filled the limit. The rows are now kept
  as one group (test `the_large_cpu_refusal_survives_the_reason_limit`).

## Runtime capacity (plan 12.5.1)

`device::capacity::Capacity::step` is the per-stage, per-device rule for
concurrent runs on one loaded session, with the clock and observations
supplied by the caller. Auto allows at most 2 runs; a user cap lowers it at
once, also during a pending trial. A run is added only for sustained need and
only if free memory after its measured working memory stays at or above the
reserve floor, the larger of 1.5 GB and 10% of the device (other stages'
pending working memory on the device counts). An added run stays only if
throughput rises at least 1.3 times with every run within budget. Below the
floor or under external load the stage sheds to one run at once, and adds runs
back one at a time after a 120 s quiet period.

- **Holds:** the rule, tested in `device::capacity::tests` (6 tests), including
  shedding under injected memory pressure.
- **Does not hold yet:** nothing calls it. Concurrent runs on one session are
  not built, because `Scorer` serializes every run behind its session mutex in
  retrieval `runtime.rs`, and per-run CUDA streams need that change first. The
  gain was not measured, since a second session would be a second copy of the
  model. Follow-up owner: L5b, after L4's `runtime.rs` change integrates.
- **Closed (L5C review L3):** a lowered user cap was ignored while a trial was
  pending. It now applies first (test `a_lowered_user_cap_ends_a_pending_trial`).

## One Context model: Arbiter (step LH, decided 23 November 2026)

The user dropped the Large Context model after the LG gate and named the
remaining one Arbiter (change-log entries 56 and 62).

- **Selectable models.** `SelectedModel` is `focus` or `arbiter` (wire
  values). `arbiter` selects `ModelChoice::ContextQwenCompact`
  (`arbiter-context-qwen3-17b`, Qwen3-1.7B). The stage keeps its key
  `context`; tasks `airk.context_*` and the degradation `context_absent`
  keep their names. `ModelChoice::ContextQwenLarge` stays decodable, because
  retained private profiles carry it, but `SelectedModel::from_model` refuses
  it: it has no device slot.
- **Device catalog.** `airk.arbiter.device-catalog/2` has two slots, `focus`
  and `arbiter`; the binding digest is framed with the /2 name.
- **Configuration.** `airk.arbiter.configuration/5` is configuration/4 with
  the new model values and an optional `context_retired` (`context_large`).
  `DeviceConfiguration::migrate_v4` reads exact canonical /4 bytes of this
  installation bound to the compiled transition's old binding and returns /5
  under its new binding with the revision bumped: Focus as it was; a
  `context_compact` selection becomes `arbiter`; a `context_large` selection
  becomes a disabled Context stage with no model, its enrollments, device
  policy and precision kept and `context_retired` set. Arbiter is never
  selected for it. A /3 document that selected the Large CPU profile
  migrates the same way. `context_retired` requires a disabled Context stage
  with no model; the next Context configuration clears it.
- **Device status.** The Context stage lists one model, `arbiter`. Status
  text names the models "Focus" and "Arbiter".
- **Answer /3.** `Degradation::Model` keeps its shape; the only valid step
  is to no model (`applied` absent). A decision's model is `arbiter`.
- **Card rows.** `CardEvidence.same_as` names the earlier row by its
  position in the call (above).
- **Holds:** `device::migration::tests` (6 tests) on hand-written /4 bytes;
  the service's retained-fixture tests on the bytes the build before LH
  wrote; the store's migration test on a real configuration store.
