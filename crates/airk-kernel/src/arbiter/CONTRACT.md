# Native optional evidence stages

Owner: `airk-kernel`. Shared wire types, source binding and validation belong
to `airk-core::arbiter` and `native_focus`; the service owns model eligibility,
native admission, reservations and delivery fences.

Arbiter Focus orders a complete canonical semantic Search result. It does not
grant source access, produce evidence, decide sufficiency or remove results.
Lexical Search and other read operations are excluded. Focus disabled or
excluded retains the ordinary result without added metadata. Context may be
disabled while Focus runs. Search navigation computes its native definition
hints before Focus and preserves those hints.

The original Host-captured read attempt, actual absolute deadline, worker and
authority are required. Ordinary Search uses Core's bounded allocator once;
the optional deadline can only shorten the original deadline. Nested Answer
uses its actual parent allocation and observed remaining work. Request clones
share private accounting; a new captured attempt starts new accounting. No
retry, new attempt, replacement grant or independently reset token allowance
is created. Unpartitioned combined ordinary Search retains its complete
original result with a bounded NotApplied observation.

Both native Answer entry points bind that parent once before their first
canonical read; adapters clone the owning Kernel rather than assigning a
replacement budget. Selection-backed ordinary singleton Search requires the
actual Host-captured Focus-only `PartitionAllowance`, checked with Core's
purpose predicate and bound to the original attempt, selection and index.
Runtime/profile/frame caps only reduce its resources; its captured parent is
retained exactly. The per-index Kernel never infers singleton authority from
its local scope. Focus-only allowances authorize Search, while Context
allowances authorize Answer. Direct ordinary Focus retains its own outer
delivery guard; only genuine combined Context children defer to their parent.

When the initial actual Context application is Ready, Core limits nested
Focus to a cumulative half of the actual local optional token allocation.
Each prefix intersects that remaining stage ceiling, shared remaining work and
qualified whole-pair limits. Known Focus work or unknown held reservations
consume the same ceiling across reads; no round resets it. Context can still
use shared remaining capacity after Focus reaches its ceiling. If Context is
not Ready, Focus has the full shared allowance. Independent ordinary Search
is unchanged. Remaining capacity does not guarantee a Context input will fit.

Eligibility and scope are checked before candidate preparation. The request
binds the complete returned candidate order, full Hits, explicit UTF-8 source
windows and a bounded scoring prefix. A successful complete prefix is applied
atomically with stable ties; the unscored tail and all other fields survive.
Failure or timeout retains original order. Source-authority refusal propagates.
Unknown native work retains its reservation and blocks further optional work;
measured work above reservation also blocks later learned Context decisions.
Post-execution source, policy or identity changes refuse stale delivery.

The transport owner supplies actual reply headroom. Complete receipt framing
is reserved before dispatch, including already retained mandatory Focus facts
and Context history for nested Answer. There is no response-ceiling increase.
Combined children also reserve the actual eligible Focus outer binding before
scoring and in final packing; the parent cannot reclaim that metadata capacity.
An undispatched compact notice is included only when it fits; literal zero
headroom preserves the ordinary result. A native receipt with actual or
unknown work cannot be silently omitted by Gathering or packet packing.
Packing trims presentation and uncharged secondary records first, discloses
omissions and invalidates affected assessments. If mandatory custody cannot
fit the original allocation, the operation fails with no automatic replay.

Canonical typed Search records are the only nested source of Focus costs and
delivery bindings. Focus and Context retain separate stage bindings and share
the original parent work allocation. Costs and phase timings are additive;
candidate counts retain the peak rather than double-counting source reads.

Cheap checks: `arbiter::focus::tests::`,
`arbiter::joint::focus_pack_tests::` and MCP `focus_handler_tests::`.
Injected scorers establish application, scope, custody and framing behavior;
they do not establish optional-model quality or installed transport behavior.

Context-only history/token accounting mutators compile with `local-inference`,
matching their learned Context actuator callers. The shared availability accessor
also compiles for tests so feature-independent Focus accounting regressions keep
their assertions. A no-default-features production build omits these unused
Context helpers; canonical Focus dispatch, authority and budget behavior do not
change, and no local inference capability is added to the lightweight caller.

Canonical Answer reserves its resolved Report/Require saved-source verification
metadata before optional scoring and final packet packing. The reserve is
derived from the native typed receipt, actual limits and its existing metadata
growth allowance; it does not change the public request budget or native frame.
Explicit Off retains the legacy 4096-byte closed-render reserve. Cumulative
mandatory Focus custody and final post-application/deadline packing use the same
additional reserve. Metadata-only refusal remains valid if mandatory custody
cannot fit; no source verification or uncertain native work is replayed.

A genuine combined Context partition's selection digest is the frontend's
whole current member snapshot identity. Open and W4/A2 snapshot domains may
differ from the original installed selection identity; the digest grants no
source authority and must not be compared as if it were that credential.
Kernel still checks Core allowance validity, exact original attempt/index and
actual installed authority fingerprint. Only the ordinary singleton Focus
purpose additionally requires its captured selection digest to equal the
actual installed SelectionScopeRef identity (and an installed selection).
The service Focus scorer applies that same purpose distinction after native
request/worker/source/parent/partition validation, so nested Focus retains the
original whole-selection accounting identity without creating an allowance.


## D08 native host capacity phases

A resident retains its full admitted host peak charge through actual session
teardown. Warm compute may carry a separately authenticated extra-host bound;
it never requires the full resident allocation to be free a second time. The
broker checks that bound against actual fresh OS availability and its unchanged
policy epoch, and the compute lane repeats the same bound before dispatch.
Legacy callers retain their existing conservative full-peak warm bound. Unknown
headroom, foreign charges, policy pressure and stale epochs refuse. Pending
GPU reservations cover future, unallocated work only; real resident device
usage is already reflected in actual driver free memory and is not subtracted
again. A GPU capacity reservation never authorizes a model or runtime.


Release-observer equivalence is exact Arc identity of the authentic reservation
release state, not manifest equality. Service may deduplicate repeated borrowing
of one charge without forgetting distinct unresolved charges. NativeConfiguration
selection equality retains the complete typed document and selected descriptions;
it never narrows configuration4 to a CPU model-only identity.


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

**A publication that lands during the device read (23 November 2026).** The
Kernel's fence after the native read keeps two outcomes apart. A changed
scoped-read authority is `ScopeReadRefused` (`scoped_read_unavailable`, not
retryable). A newer publication under the same authority, such as an
Index's semantic step landing just after its text step, is not a refusal:
the observation is taken again against the new publication, at most three
observations in all within the original deadline. A publication still
moving after the third is `CurrentGateContention::Applying`, rendered as the
retryable `publication_busy` with `repeat_same_request`. Each observation is
a read-only native snapshot, so repeating it repeats no work. Tests:
`airk-mcp` `scoped_device_diagnostics_tests` (a publication landing once,
and on every read).

Combined diagnostics keeps the complete observation under each corresponding
member.result; it never fuses authorities or global enrollment state. Complete
copy/export retains precision, native fallback reasons and all variant metadata.
The additional observation is bounded256KiB inside unchanged original reply/frame
ceilings. No retries or replacement clock are introduced.

## Fences as consistency checks (23 November 2026, change-log entry 67)

Phase A of the [local trust plan](../../../../docs/simplify/local-trust-plan.md).
A changed authority is still a refusal; a publication that moved under the
same authority is not.

- **Post-work delivery fence** (`Kernel::require_delivery_fence`, used by the
  v1 gathering in `arbiter.rs`, the Answer /3 loop's `finish`, both joint
  fences and the Focus fence). A changed scoped-read fingerprint is
  `ScopeReadRefused` (`scoped_read_unavailable`). A newer publication is
  Core's `PublicationAdvanced`: code `publication_advanced`,
  `retryable: false`, `query_replay_safe: false`, no `action`. The model
  work already spent is never replayed and never `repeat_same_request`.
  Before this entry both cases were an untyped `read_failed`.
- **Focus stale render.** `FocusPostExecutionStale` (the Focus policy or
  identity changed after native work) renders as
  `arbiter_context_action_stale`, `retryable: false`,
  `query_replay_safe: false`, the Answer and joint stale family. It rendered
  `read_failed`.
- **Read-only scoped reads** (kernel `scoped_reads.rs`, no other contract
  file owns them):
  - `scoped_index_metadata` reads again when a publication lands during
    the read, at most three observations; a publication still moving is
    `CurrentGateContention::Applying` (`publication_busy`,
    `repeat_same_request`).
  - `scoped_source_projection` of an expected publication that a newer one
    replaced is `publication_busy`; the combined frontend decides whether
    the member's evidence is read again.
  - A source configuration ahead of the publication is
    `ScopeReconcilePending` (code `scope_reconcile_pending`, retryable,
    `retry_after_ms: 250`, `repeat_same_request`) only when the same request
    would pass once that configuration is published: the authority resolves
    against it and, for index metadata (settings, roots, rebuild roster),
    still covers every configured source in full (`covers_sources`). A scope
    that resolves against neither, or that the configuration would leave
    partial (a configured root outside the authority, a saved recovery
    source it does not cover), is still `ScopeReadRefused`; reconciling
    would never lift it (entry 67 addendum). A published root outside the
    authority still makes it a refused partial scope.
  - Genesis (sequence 0, no collection state): the authority is checked
    against the source configuration, so a multi-root scope whose roots are
    configured is `BootstrapPublicationPending` like a single-root one; an
    unconfigured root is still refused.
- **Not changed here:** the batched Focus fence (`FocusBatchStale`) still
  covers a publication move with the Answer's typed stale refusal, and a
  member source projection read for a combined continuation still expires
  its cursor (`combined_cursor_expired`).

Tests: kernel `arbiter::focus::tests` (`focus_publication_moved_after_native_work_is_publication_advanced`,
`joint_delivery_after_a_publication_landed_is_publication_advanced`,
`v1_delivery_after_a_publication_landed_is_publication_advanced`),
kernel `scoped_reads::tests` (`configuration_ahead_of_the_publication_is_pending_reconcile_not_refusal`,
`scoped_metadata_with_configuration_ahead_is_pending_reconcile`,
`scoped_metadata_reads_again_when_a_publication_lands`,
`source_projection_of_a_replaced_publication_is_publication_busy`,
`genesis_multi_root_scope_with_configured_roots_is_pending`), `airk-mcp`
`arbiter_handler_tests::publication_landing_after_model_work_is_typed_publication_advanced_without_replay`
and `focus_handler_tests::focus_policy_change_after_native_work_renders_typed_stale_refusal`.

## Private evaluation change set 1 (`software.rs`)

Every layer is off unless the private evaluation fixture binds layers for its
process (`software::bind_evaluation_layers`); product gathering, packing and
Focus are unchanged and `Gathering::new` keeps the product shape. With layers
bound, one Gathering state machine still issues every read through the same
canonical reader under the original revision fence and call, round, candidate,
byte and deadline budgets:

- `followups`: whole chunks of the kept hits, `symbols`/`references` lookups
  for identifiers of the question and kept hits, their `read_file` excerpts
  (with the recipe's `known_hash`), callers and tests, file outlines, and
  contract or README documents that govern a hit's folder. A governing
  document is queued only after a canonical `file_info` read at the same
  revision shows it indexed. A failed optional follow-up (round 2 or later)
  keeps its canonical failure receipt and the remaining queue; a failed
  initial search still refuses.
- `spread`: follow-ups merge one read per kind per cycle; the queue may hold up
  to `fanout.min(candidates)` reads.
- `pool`: the first search returns 16 focused snippets.
- `focus`: Focus scores at most six pairs per Answer, each over a bounded
  window; kept hits follow Focus relevance. Focus still removes nothing from a
  record.
- `present`: every hit is presented once (contained or mostly overlapping
  ranges are dropped), grouped by purpose with exact locations, with a closing
  disclosure; packing omits the least useful kinds first.
- `device`: a Context call is admitted only when its measured cost plus a read
  reserve fits the deadline.

Records, receipts and paragraph grounding keep their existing validation.

### Layer tokens and binding (22 November 2026)

`Layers::parse` parses the value the private fixture reads from
`AIRK_ARBITER_EVALUATION_ANSWER_LAYERS`. It accepts `all`, `none`, the layer
names above, `-name` to remove one, and three change-set-2 tokens:

- `context_b`: Context option B. Joint assessment builds its roster with
  `airk_retrieval::arbiter::context_wide` (wider roster, its own task policy,
  the best Read taken even when a terminal card scores highest). Only a
  profile that declares the option-B policy is scored under it. Product rosters
  are otherwise unchanged.
- `focus_keep=<1..=16>`: hits kept for whole chunks and follow-ups
  (default 4).
- `focus_threshold=<0..=1, at most three decimals>`: the Focus relevance a
  scored hit needs to be kept (default 0.10).

Replay tuning tokens (change set 1 tuning). Each default, and each second
value below, is change set 1's behaviour, so `all` serializes unchanged:

- `pool=<1..=16>`: first-search pool size (default 16); needs `pool`.
- `pool_full=<0..=16>`: top pool hits fetched with full chunks. At or above
  the pool, the first search returns full chunks; 0 keeps snippets; between,
  the snippet search is followed next by one full-chunk `search` of the same
  question for the top hits, its byte bound reserved from the per-read split.
  Default: full chunks only when the pool is at most 8.
- `read_floor=<512..=8192>`: lower clamp of the per-read byte split (default 1536).
- `kept=<1..=16>`: an alias of `focus_keep`.
- `subjects=strict|loose`: strict looks up only snake_case or multi-word
  CamelCase names that a kept hit's text names; loose is the first-three rule.
- `governing=project|any`: project probes governing documents only inside a
  hit's project folder (the folder after the first `crates`, `packages`,
  `apps`, `libs` or `src`, otherwise the top-level folder).

The first search's `pool` and `pool_full` come from the process binding
(`evaluation_layers()`), the same layers `Gathering::with_layers` receives in a
service.

`all` turns on exactly the six change-set-1 layers, never `context_b`. `all`
and `none` reset the values, so values must come after them. An empty,
repeated or unknown token is refused. `bind_evaluation_layers` exists only in
test-support builds. It binds once per process, and a second binding must
repeat the same layers. Without a binding, and always in a product build,
`evaluation_layers()` is `Layers::default()`: every layer off and every value
unset, so the product's gathering, roster and Focus shape are unchanged.

## Answer /3 admission plan (`answer/admission.rs`, L2b)

The service's Answer job table decides how much of a request fits its
deadline (final plan section 12.4.1). The kernel runs the loop under that
decision and says so in the reply.

- **`Kernel::answer_admitted(request, env, plan)`** runs the loop of
  `answer_with` under an `AdmissionPlan`. `answer_with` and `AnswerEnv` are
  unchanged, and a call without a plan behaves as before.
- **The plan bounds, it never widens.** It limits the request's parts,
  sub-needs and rounds, and a plan without the Context model runs the
  no-model loop. A limit the budget already keeps (a combined member's
  allowance, say) is not the plan's reduction.
- **The reply discloses the plan.** It carries the plan's reductions
  (`parts`, `sub_needs`, `rounds`, `model`) beside the loop's own
  degradations, and it is validated against the budget as asked, as the Core
  reply contract expects.
- **Refused before the loop starts:** a reduction the plan does not disclose,
  a change of Context rung (the loop cannot change rung per Answer; it can
  only drop to no model), and a disclosure that is the loop's own.
- **`Kernel::answer_scope`** returns the binding the loop would check before
  it runs, from the same owner observation and with the same checks. The
  service keys its job table on it, never on client input. It refuses a
  combined member, which is not a job of its own.
- **`Kernel::answer_context_model`** names the Context model the card calls
  would run on, for the admission estimate only.

Evidence: `arbiter::answer::admission::tests` (the plan), and the wired path
in `airk-mcp` `answer_jobs::tests` and `airk-service`
`answer_jobs::wiring::tests`.

## Answer job kernel (`answer/admission.rs`, L2c, decided 23 November 2026)

Recorded contract decision (final plan section 12.4.2; the service half is
`airk-service` `answer_jobs/CONTRACT.md`, "Service-owned jobs"). A
service-owned Answer job needs read authority and a native attempt of its own,
so that its maker's session teardown does not end it (review H1).

- **`Kernel::answer_job_kernel()`** derives the job's kernel from a live
  caller kernel:
  - the same frozen scoped-read authority, fingerprint, observation, selected
    read snapshot and native owner, and a **revocation flag of its own**.
    `revoke_scoped_reads` on the maker's kernel no longer reaches it; on the
    job kernel it ends the job's reads. The owner revokes it with the
    selection or open read the maker redeemed.
  - **no native attempt.** The job mints its own at dispatch and binds it
    with the existing `with_optional_read_attempt_for("airk_answer", ..)`,
    which refuses a second attempt. The attempt carries the job's own request
    id and expiry; the maker's attempt is not replaced, it is simply not used.
  - fresh Focus and loop accounting.
- **Refused:** a caller kernel whose scope is already revoked, and a
  combined member (a partition allowance: it runs under its parent and is
  never a job). A kernel without an Arbiter owner is allowed and keeps the
  no-model loop.
- It widens nothing: the job kernel reads exactly what the maker could, in
  the same root, collection and index, on the same publication.

Evidence: `arbiter::answer::admission::tests` (the derivation) and
`airk-supervisor` `project_worker::scopes::answer_job_tests` (a job kernel
outlives its maker's session and ends with its selection).

## Answer /3 loop (`answer/`), what holds and what does not

`Kernel::answer_with` runs the section 12 loop: Split, per-part Plan,
Retrieve, Read and Coverage, Follow-up and Compose, then Union. Every read is
a canonical dispatch through the one shared read owner (`answer/gather.rs`);
models only choose among cards the kernel proposes. The design and its
recorded deviations are in the private L3 notes; the ones that bear on
callers are listed here.

What holds:

- **Initial part retrieval.** The first broad canonical Search for each
  Answer part requests up to 32 hits and 24,576 hit-text bytes. Each sub-need
  keeps its own narrower read recipe. Existing request limits still govern
  retention, verification and the reply frame.
- **Concurrent reads, deterministic reply.** A wave's reads run on at most
  four scoped threads (`answer/wave.rs`). Claims follow dispatch order, every
  claimed read runs, and the reads that ran are a prefix of the wave; the
  rest are released. Accounting and observation run after the wave in
  dispatch order, so completion order never changes the reply, the decision
  log or custody. A read that panics halts the wave and its panic (the
  lowest-index one) is raised again. Governing `file_info` probes, batched
  Focus and card calls run on the driver thread at the round barrier.
- **A wave worker reads as the caller.** The runner captures the driver
  thread's worker bindings (`service_execution::Inherited`: the in-process
  `ExecutionScope`, the embedding `ConnectorScope` and the accepted grant's
  allowance) before it spawns, and binds them around each threaded read,
  restoring the worker's previous bindings when the read returns or unwinds.
  A read therefore finds the same embedding connector, drains, cancels and
  expires at its checkpoints exactly as an inline read on the driver thread
  does, and nothing one read binds reaches the next read on that worker or
  the caller. An unbound driver (a child-process owner) binds nothing, so its
  workers keep the process slots. Everything else a read is fenced by
  (deadline, read attempt and its cancellation, revision, publication fence,
  scoped-read authority) is carried by the reader and its arguments, not by
  the thread. Tests `every_wave_worker_reads_under_the_callers_scopes` and
  `a_binding_left_on_a_worker_reaches_neither_its_next_read_nor_the_caller`.
- **Per-part model shares.** Shares are cut after Split. A card call batches
  rosters of one part only and is charged to that part's slot share; its
  reservation is capped by the part's token share. Focus is sized by each
  part's token and pair shares and half the part-round allowance, and spans
  past it stay in coverage unscored. A finished part hands back what it did
  not use. Every roster a present model could not score is disclosed as
  `ModelShare`.
- **Batched Focus accounting** (change-log entry 53). A batch is whole
  sub-needs of the barrier, at most `min(round_input_tokens / pair tokens,
  32)` pairs and 16 questions. Before sending one the kernel checks the
  call count, the pairs (`focus_pairs + batch <= budget`), the stop time and
  the token pool; when one fails it sends nothing more and discloses every
  part whose spans stay unscored. Pairs count as dispatched, one way in the
  kernel ledger, the service loop ledger, the receipts (`candidate_count`),
  the decision log summary and `model_work`; a receipt's `candidate_count`
  must equal the batch sent. A call that scored fewer pairs than it
  dispatched is not applied; each part it served discloses `ModelShare {
  read }` and the loop continues. An error from the native owner is unknown
  work: the call stays counted with no scored pair, Focus stops for the
  Answer and `ModelOutcomeUnknown` is disclosed.
- **Card estimate per call.** Model-time admission estimates one batched
  call (one forward pass), not one per segment; Focus is bounded by the
  model deadline, its shares and the original deadline instead of a time
  estimate.
- **Model calls end before the Compose and Union reserve**
  (`deadline - compose_union_ms`).
- **Stale refusal.** `AnswerPostExecutionStale` is returned when the
  configuration changed after native work, and also when a Focus batch went
  stale mid-loop; it holds the custody so far, the optional reply and the
  cause privately, and its receipt is nonreplayable.
- **Union packs the escaped reply** (change-log entry 53): the reply's JSON
  text embedded as a JSON string (`verification::quoted_len`) fits the reply
  frame less the handler's reserve (4,096, or twice the verification
  receipt floor plus 4,096 when verification is on), the representation the
  handler's reply ceiling and the receipt allocation are measured on.
- **Clause paragraphs cite the packed reply.** They list delivered source
  reads before discovery results within a clause. A span must survive at its
  original citation pointer to appear in the location list. Clipped excerpts,
  withheld spans and omitted records are counted in the paragraph, including
  the compact form when it can retain a citation. The typed coverage, stop,
  record and omission fields retain their existing meanings.
- **Presentation trimming is not omission.** Union levels 1 to 4 shorten
  records (uncited hits, excerpt cuts, compact paragraphs) and disclose it
  per projection (`hits_total`, `rows_total`, `truncated`); a part is
  `shortened` only when records are omitted, as Core validates against
  `omitted_count`.
- **The reply carries each text once** (`answer/repeats.rs`, change log
  entry 45; Core contract, "One text, one copy in the reply").
  - Union plans what every span row of the reply carries
    (`repeats::plan`) over the retained records' hits: all of them at the
    first trimming level, the cited ones from then on. The plan is made
    again for every packing attempt, so a row never names one that was
    trimmed away.
  - An exact repeat names the row that carries its text (`same_as`), and
    that row lists other places (`also_at`). Among spans of one file region
    the longer text keeps an overlap: a span inside it names it
    (`within`), and a span that shares its head or tail is trimmed
    (`overlaps`). Comparisons are of bytes; nothing near-identical is
    merged.
  - Overlaps and `within` are planned only at the levels where every
    carrying row is sent whole (`EXCERPT_BYTES` unbounded, levels 0 and 1).
    From level 2 the plan merges exact repeats only, so no row names bytes
    its carrier's 768- or 384-byte excerpt does not deliver, and a repeat's
    `truncated` includes its carrier's cut (`union::carrier_cut`). A row
    already trimmed, or one whose `also_at` lists places, is never made
    `within`; it stays a trimmed or whole text (review of `5f971a70`, H1,
    M1, L2).
  - Every row keeps its pointer and location, so no provenance is lost, and
    `hits_total` still counts every hit. A row's `path`, `start_line`,
    `end_line` and `content_hash` are always the hit's whole original span,
    also for a trimmed or `same_as` row; lines must never be derived from
    `text`.
  - The freed bytes stay in the reply frame: Union measures the reply as it
    will be sent.
- **Card rosters show bounded evidence excerpts** (change log entries 43
  and 44).
  - A Coverage or Follow-up card call shows each sub-need the spans gathered
    for it this round, in Focus rank order, as `CardEvidence` rows of its
    `CardSegment`.
  - The call's excerpts total at most its excerpt allowance (Core
    `native_loop::evidence_allowance` of the tokens the call reserves: 7,680
    bytes at 8,192 tokens). The existing reservation bounds it.
  - **Which part of a span is shown** (`coverage::window`): all of it when
    it fits its share. Otherwise a window of the share's size that holds the
    start of the text Focus scored (the span's Focus window,
    `software::focus_window`, carried back from the Focus ranking). The
    window starts there, or earlier where fewer bytes than the share follow,
    so the share is used fully. A span Focus did not score is shown from its
    start. The relevance beside an excerpt therefore describes text that is
    shown. `skipped_bytes` and `cut_bytes` say what lies before and after
    it, and `truncated` repeats `Hit.truncated`. Until the delta review of
    `936c9b94` an excerpt always started at the scored text, which hid the
    head of a span that would have fit whole.
  - **A text is shown once in a call.** A later span of the call with the
    same text (the same span gathered for a second sub-need, or a copy at
    another place) is a row that names the row showing it (`same_as`) and
    takes none of the allowance. The comparison is of whole span texts
    within one call. Overlapping spans, and repeats across calls, rounds and
    parts, are not merged here.
  - **The cut** (`coverage::excerpt_cuts`): every shown text gets the same
    number of bytes, or all of itself where it is shorter; that number is
    the largest that keeps the sum within the allowance. Each excerpt then
    ends on a character boundary. The result depends only on the texts and
    the allowance, not on their order.
  - **No call without room.** A cut excerpt shows at least
    `MIN_EXCERPT_BYTES` (256). When the allowance cannot give every shown
    span that much, or all the span has, the call is not made
    (`coverage::present` returns `None`): the deterministic order decides,
    the decision is logged `not_dispatched` with reason `token_budget`, and
    the part's `ModelShare` is disclosed. A call for sub-needs with no spans
    is made without evidence. Character boundaries can leave a window a few
    bytes short of 256 although its share was enough; it then grows by whole
    characters while the call's allowance has spare bytes.
  - Excerpts are parts of hits of records already read and retained under
    the Answer's revision fence. Building them reads nothing.
  - **Subjects:** a Coverage and a Follow-up roster name the sub-need in the
    words of its Plan card (`plan::wording`, for example
    `need callers: restart_guard`). A definition and a callers sub-need of
    one identifier have the same query, so the bare query did not say what
    evidence to look for.
  - **Structural coverage:** a definition, callers or contract sub-need
    receives a Coverage roster only after native reads establish its structural
    coverage. Until then its deterministic assessment feeds the usual bounded
    Follow-up choice. A lexical or semantic hit alone cannot close these needs.
  - **Follow-up verdict:** a Follow-up roster states its sub-need's coverage
    (`partial` or `missing`) and whether a model's Coverage choice or the
    deterministic order set it (`NeedState::coverage_by_model`).
- **A card call's reservation** is the least of 8,192 tokens, the pool left,
  the charged parts' token shares left, and what each charged part and round
  has left of its round allowance. The last cap is new in entry 44: a round
  near its allowance gets a smaller call with shorter excerpts, where a
  whole reservation would have been refused.
- **The log says what each decision rests on** (Core contract, "What the
  log's basis means").
  - Only a decision a model took rests on excerpts: its `basis` is the byte
    range of each excerpt, and `basis_omitted` is the text before and after
    each excerpt and every span not shown. The reply's compact basis counts
    both, so a cut is visible in the reply.
  - A decision the deterministic order took names the whole spans and omits
    nothing. That covers the no-model path, a call not dispatched, a call
    the lane refused before model work, a lost call and an abstention. The
    basis is chosen after the call's outcome is known (`model::decide`).
- **A reader can tell how each decision was taken.** A roster a present
  model did not score is logged `not_dispatched` with the model and a reason
  (`token_budget`, `deadline` or `unavailable`); `deterministic` is the
  no-model path or a one-card roster. `context_absent` is disclosed when no
  card call was dispatched at all.
- **Model-time shedding.** `shares::CostModel` keeps one estimate per Answer
  and stage: 12,000 ms before the stage's first call
  (`software::FIRST_CONTEXT_ESTIMATE_MS`), then three quarters of the old
  value plus one quarter of each call's measured wall time. It has no
  per-profile constant; one Answer runs on one Context profile, so after a
  stage's first call it follows that profile and device. A card call is not
  sent when the estimate exceeds the part's model time for the barrier
  (`Shares::round_model_ms`: the time left before the Compose and Union
  reserve, divided by the active parts and the rounds left). The decision is
  then logged `not_dispatched` with reason `deadline`.
- **A document hit gets no line window** (change log entry 55). The
  whole-chunk `context` read that Retrieve (Chunks, Literal and
  Documentation steps), Follow-up and the /2 change-set planner give a hit
  is shaped by `software::whole_chunk_after` from the hit's own citation
  location, never from its path. A `Lines` hit, or a `Mapped` region of a
  UTF-8 composite host, keeps its window (at least
  `WHOLE_CHUNK_LINES` past the first cited line, at most 100). A hit at a
  native document location (page, block, slide, table or worksheet row) is
  read one location at a time by the context tool, which refuses a range
  that crosses into the next location; so its read is confined to its own
  cited lines and planned only when the hit's text was cut (a focused
  snippet). An uncut document hit already carries those lines, and no read
  is planned. Documentation picks the first documentation hit a read can add
  to. Outline on a document returns its headings; `imports` on a document
  returns no rows (a wasted call, not a failure); `symbols`, `references`
  and excerpt reads stay inside the cited rows they come from. Tests
  `document_hits_get_no_line_window_and_code_hits_keep_theirs` and
  `a_document_top_span_is_read_only_within_its_own_location`.
- **A refused document range is typed.** The context and read-file tools
  refuse a range across document locations with
  `airk_core::read_arguments::crossed_document_locations()`: code
  `invalid_arguments`, field `range`, not `retryable`, rendered by the MCP
  handler as that typed receipt instead of `read_failed`. The Answer loop
  does not read message text.
- **Record identity without per-read status** (change log entry 54, Core
  contract section of the same name). `gather::retain` keeps two forms of
  each read: `body`, the result without Core's `VOLATILE_READ_FIELDS`
  (`answer::volatile::stable`), and `received`, the result as received. The
  record id, `bytes` (the reply's `full_bytes`), `full_digest`, retention
  accounting and every rule that reads a record use `body`; custody's
  `canonical_result` is `received`. Union sets the reply's one `read_status`
  from the first retained search's `received` status. Two runs of one
  request at one publication therefore give byte-equal records, decisions,
  basis and log digest (`runs_differing_only_in_read_status_give_byte_equal_replies`).
What does not hold yet, with the follow-up owner:

- **Canonical reads are not cancelled at the reserve** (L4/L2b). A read
  starts only before the 5 s dispatch reserve, but `CanonicalReadFn` takes
  no deadline, so a read that outlasts the reserve still makes the reply
  fail validation. The service read attempt must bound reads by the Answer
  deadline. Change log entry 20.
- **Custody is not retained** (L2b). Until the Answer job table keeps it,
  the MCP handler delivers `outcome.response` and drops `outcome.custody`;
  the reply keeps the log digest. Change log entry 21.
- **The partial marker names one reason** (Core contract). With both an
  early stop and byte omissions, `partial` names the stop; omissions remain
  disclosed by `omitted_count`, `omitted_evidence`, `evidence_omitted` and
  `shortened`. Reporting both needs a Core wire decision.
- **Combined members gather within their frame share** (limit, L3). Combined
  Answer runs /3 since L3-10: the kernel side in (a)
  (`with_answer_partition_for`, the clamp in `answer_with`, card and Focus
  requests carrying the allowance, `combined_reads::compose_answer` and
  `answer_payload_admission`), the MCP handler, the read-attempt and host
  carriers and the frontend in (b). Core's initial allocation shares the
  retention bytes; the frontend then clamps each member's bytes to its share
  of the frame's admitted payload (`frame_shares`), and in /3 the same field
  bounds the member's retention and reply, so a member gathers within its
  frame share. A member whose share cannot hold the question and a minimal
  reply is not dispatched and is reported unqueried.
- **Answer clients still send /2** (owner: the client owner the primary
  assigns). The VS Code Workbench (`packages/vscode/src/workbenchView.ts`),
  the Admin Search screen (`packages/admin/src/SearchScreen.tsx`, built into
  `crates/airk-cli/admin-assets`) and `scripts/check-service-clients.py`
  build `airk.arbiter.context.request/2` requests and render /2 replies.
  Since L3-10 (b) the MCP handler takes /3 only, so their Answer is refused
  until they move to /3. Change log entry 40.
- **An idle model reload is not named** (L3). An Answer that meets a reload
  discloses only `context_absent` or `focus_absent`; naming it needs a typed
  unavailable reason and a matching degradation (change log entries 24, 27
  and 28).
- **The /2 kernel path is not deleted yet** (L3-10 (c), deferred by the
  primary to after the LG gate). The joint path, the v1 `Gathering`,
  `pack_native_response`, `native_response_bytes`, `software::Layers` and
  `focus::bind_focus_shape` remain, with their tests; the MCP handler no
  longer reaches them. Change log entry 40.
- **An unscored span is shown from its start** (limit). Without a Focus
  score an excerpt is the first bytes of its span's text. A span whose
  relevant lines come after the cut shows the model its beginning; the cut
  is disclosed (`after` in the view, `basis_omitted` in the log), not
  corrected. A snippet is marked (`snippet`), not completed.
- **Closed in step LH (change-log entry 62): review lows L-a, L-b, L-c and
  the stale `CardEvidence.excerpt` comment.** L-a: the Coverage and
  Follow-up rules now say a repeat row keeps its own Focus score, judged for
  its own sub-need, which may concern lines outside the part shown (the Core
  rank order keeps the relevance, so the rule sentence is the fix). L-b: a
  repeat names the row that shows its text by its position in the call
  (Core `CardEvidence.same_as`), shown to the model as
  `s<segment> row <place>`, so two rows at one place are never confused.
  L-c: the rules say a segment is judged against its own cards, subject and
  evidence, and that the text a repeat row names in another segment counts
  as its own; the Follow-up rules carry Coverage's sentence on an empty
  evidence list. The `excerpt` comment says an excerpt holds the start of
  the scored text and may start before it.
- **A window short of the least excerpt grows only from spare bytes**
  (delta review L-d, fixed in part). It is not paid for by shortening
  another span's window, so a call can still be refused when another split
  of its allowance would have fit.
- **The allowance ignores a profile's compiler ceiling** (review L1 of
  `54b1e7a2`; owner: whoever admits the card tasks on a CPU Context
  profile). A call reserves up to 8,192 tokens whatever the profile, and the
  compiler refuses an input above the profile's `max_tokens`. The CUDA
  Context profiles of the gate are 8,192. On a 2,048-token profile every
  call with evidence would be refused before model work and decided by the
  deterministic order. Fix: size the allowance from the smaller of the
  reservation and the profile's ceiling.
- **`basis.omitted` counts two things** (review L3; owner: the primary
  agent, with the next Core wire decision). A range cut from a shown span
  and a span not shown at all are each one unit, so the reply's compact
  count cannot tell them apart. The log's `basis_omitted` ranges can.
- **The measured frame is a model of the view, not the view** (review L5;
  owner: whoever next changes the view's texts). The measurement script
  copies the prompt, question and rules texts by hand, and nothing checks
  them against `loop_cards.rs`. A Rust test that prints the compiled token
  count of a real view under the real tokenizer would replace it.
- **The 2,048-token frame is assumed, not checked** (review L6; owner: the
  primary agent). Core accepts a 16,384-byte subject, eight 1,024-byte cards
  and 4,096-byte paths, so a call can pass the frame. The lane then refuses
  it before model work and the deterministic order decides. Nothing bounds
  the frame's parts to make that impossible.
- **A card call can still show overlapping text** (owner: the primary
  agent; change log entry 45). A call shows a text once when two of its
  spans have the very same text. Two different spans of one file whose
  lines overlap (a chunk and its neighbour) are still shown as two
  excerpts, and the shared lines take allowance twice. The reply trims
  such overlaps; the call does not yet. A text shown in one call is shown
  again in a later call or round that gathers it: each call is scored on
  its own, so this is not a repeat the rule removes.
- **Near-identical texts are not merged** (decision, change log entry 45).
  Only byte-equal text is. Any looser rule could merge two different
  facts, such as two versions of a function.
- **A span that holds a trimmed shorter span is not merged with it**
  (limit of `repeats::plan`). The longer text is planned first, so this
  needs a span trimmed at one end that then lies whole inside a third
  span. Both texts are then carried; since the review of `5f971a70` (M1)
  this is the rule, so a trimmed row never claims to be part of another.
- **From trimming level 2 overlapping spans are sent twice** (limit, the
  H1 fix). Each is cut to its own excerpt; shared lines can then appear in
  two rows. Trimming against a cut carrier would need the plan to follow
  each row's excerpt.

### Review findings and resolutions (Answer /3 loop)

Each line: what was wrong, what changed, the commit, the evidence. Test
counts are for `nextest -p airk-kernel -E test(/arbiter::/)` at that commit.

Review of L3-10 (a) (`594e9fbf`, `f60fce96`), closed in the L3-10 (b)
commit (change log entry 40):

- 1 (high), `compose_answer` capped the wrapper at
  `SharedBudgets.max_bytes` (at most 65,536), below the Answer frame that
  admission sized. An Answer summary now takes `reply_bytes()` alone. Test
  `combined_answer_v3_composes_a_wrapper_above_the_ordinary_frame`.
- 2 (medium), member bytes were shares of retention, not of the frame. The
  frontend clamps them to the frame payload shares (`frame_shares`); the
  "Known limit" text above is corrected. Test
  `member_bytes_are_frame_shares_and_admit_only_a_question_sized_reply`.
- 3 (medium), a member with fewer than question + 4,096 bytes failed
  `answer_with`. It is no longer dispatched; it is reported unqueried
  (frontend and `returned_allowance`). Same test, and the Core hand-back test.
- 4 (medium), the configuration-changed delivery binding: did not
  reproduce; the stale refusal already covers it (Core contract, "Combined
  Answer /3 allowance"). Test
  `delivered_card_work_binds_context_to_its_own_application`.
- 5 (low), admission sized members without an initial allowance as `None`.
  Every member is now sized with the widest allowance. Test
  `combined_answer_v3_admission_takes_the_answer_frame`.
- 6 and 7 (low): Core, see the Core contract.

First review (L3-7, L3-8, `ed2e0926`), closed in `7b65167a` (165 passed):

- H1, Focus sizing deleted spans from coverage. Now only the spans that fit
  are sent and the rest stay unscored. Test
  `focus_short_of_tokens_leaves_spans_unscored_not_dropped`.
- H2, Focus and card tokens could pass the token pool together. The pool is
  checked before each batch, cards included. Test
  `focus_and_card_tokens_share_one_pool`.
- H3, an unretainable failure receipt counted as an omission with no
  shortened part. It is now a failure without a receipt, no omission and no
  stop. Test `an_unretainable_failure_receipt_is_not_an_omission`.
- H4, conflict notes kept parts whose evidence Union removed. Parts are
  recomputed from retained evidence (`union::surviving`). Test
  `a_conflict_keeps_only_parts_whose_evidence_survives`.
- M1, per-part model shares were not enforced. Slot, token and pair shares
  are cut after Split, checked and handed back as used. Test
  `card_slots_follow_each_parts_share`.
- M2, deterministic fallback was undisclosed. ContextAbsent follows Core's
  rule; every fallback a present model could not score is a ModelShare.
  Assertions in `abstained_or_none_falls_back_with_recorded_rule`.
- M3, a re-tagged shared read reacted twice and duplicates filled the
  round-one cap. It reacts once; the cap counts new dispatches only. No
  driver test (the re-tag timing cannot be forced); see M4's test.
- M4, stopped parts kept planning reads and could be handed shared reads.
  They plan nothing and are removed from queued reads. Test
  `a_stopped_waiter_is_never_handed_a_shared_read`.
- M5, model calls ran into the Compose reserve. They end at the model
  deadline. Test `model_calls_end_before_the_compose_reserve`. Reads stay
  open: "What does not hold", change log entry 20.
- M6, Plan advice above the sub-need limit was logged then truncated. It is
  refused as unusable advice. Test
  `plan_advice_beyond_the_sub_need_limit_is_refused`.
- M7, top-score advice was not checked against its scores. Chosen cards
  must be the highest-scoring ones. Tests
  `top_score_advice_must_choose_the_highest_card` and the argmax assertion
  in `batched_segments_argmax_per_segment`.
  Refined in L4 (change-log entry 39): Plan's reference card ("no further
  sub-need", the last card) is a threshold, never choosable. Advice that
  chooses it is refused, and the top-score check ignores it: no choosable
  card left out may score above a chosen one. Before, top-score Plan advice
  was refused whenever the reference scored highest, although retrieval's
  Plan selection then takes the best proposal. Test
  `top_score_plan_is_accepted_when_the_reference_card_scores_highest`.
- M8, no driver test ran with Focus. Synthetic Focus owners now drive the
  barrier path (H1, H2, M9 tests).
- M9, a stale Focus batch surfaced as another error and lost custody. It is
  `AnswerPostExecutionStale` with custody. Test
  `a_focus_batch_gone_stale_is_the_answer_stale_refusal`.
- L1, the shares doc said Split was charged first; it now is (M1).
- L2, disputed: model time at barrier r of n divides by n - r + 1 barriers,
  this one included; a comment says so.
- L3, L6, L7: recorded above ("Card estimate per call", "Presentation
  trimming is not omission", "The partial marker names one reason").
- L4, token attribution could charge a part 0 tokens. Every charged part
  and round gets at least 1.
- L5, a repeated error literal restarted the scan at its closing quote. It
  is skipped whole. Test `a_repeated_error_literal_is_skipped_whole`.

Second review (L3-9 `038472de` and `7b65167a`), no high or medium findings;
the lows are closed in the commit after `7b65167a`:

- N1, a card call grouping two parts' rosters was charged to the first part
  only, so a part out of slots took the other part's roster down. Calls now
  batch rosters of one part only. Assertion in
  `card_slots_follow_each_parts_share`.
- N2, a rejected literal advanced only past its opening quote, so text
  between two literals could become an error term. A literal that closes on
  its line is skipped whole; a quote right after a letter or digit
  (an apostrophe) opens nothing. Test `a_rejected_literal_is_skipped_whole`.
- N3, a panicking wave read neither halted the wave nor kept its payload.
  It halts the wave, and the lowest-index panic is raised again. Test
  `a_panicking_read_halts_the_wave_and_the_lowest_panic_is_raised`.
- N4, the wave doc overstated the stop: a worker that passed its check may
  claim one more read after another saw the stop. The doc now says so.
- Test note: `decisions_are_independent_of_read_completion_order` used a
  30 ms timed grace. It now waits on events only: the wave runner announces
  each wave's size (`wave::ON_WAVE`, test builds only) and reads are released
  latest first once the wave's in-flight reads are all inside the reader.


### Gather Context development routing

A planned sub-need explicitly addressed as a repository-relative path and
one line or an inclusive line range uses the existing canonical Read File
operation for its first read. It requires a safe path and positive ordered
coordinates and stays within the existing excerpt byte and line bounds.
The broad initial Search for that part still runs. Plain paths and other
unaddressed needs retain their existing first-read behavior. The retained
read is scoped and fenced through the same gather dispatch as any other
read, including canonical refusal for unavailable or expired evidence.


In the development reader comparison, one internal literal or structural
probe can nominate a scope-bound range. The scan reads current retained
original UTF-8 bytes from the same index member. Irrelevant content,
incomplete scans, and fact or relationship candidates carry no proof of
absence. A nominated range is read via canonical Read File with the content
hash and publication. The proof competes for the existing round-one
sub-need, part, and total call bounds. The probe and its loaded and retained
bytes are charged to the same total budgets. The private kernel outcome
tracks the source record that prompted it. Excerpt evidence alone joins the
public records. The published request and reply schemas are unchanged.


### Caller needs and anchors (26 November 2026)

Core owns the request fields (Core CONTRACT, "Optional caller inputs"). The
kernel uses two of them.

- **`needs`.** A request with caller needs is one part over the whole
  question, with no Split decision. Its sub-needs are the caller's
  statements, in order, each searched in its own words, with no Plan
  decision. Each takes the overview recipe: a search, whole chunks of the
  kept hits and their outlines. The reply marks each such sub-need
  `origin: caller`.
- **`anchors`.** Each anchor gets one exact lexical search in round one, as
  a part-level read beside the part search. The anchor is the whole query
  and is never read as a pattern. Unlike the part search, a lookup that
  fails or finds nothing does not refuse the request. On the wire its
  record use is a part search.
- **`deliverable`** selects standing sub-needs (rule 2 of "Gathering rules"
  below). **Not used yet:** `known` is validated by Core and does not change
  gathering. Per-need jobs still share one part's budget shares and one
  reply paragraph each; the returned vocabulary of the plan is not built.

Tests: `arbiter::answer::tests::caller` (needs become the ordered sub-needs
of one part without Split or Plan decisions, a request without needs keeps
its reply shape, every anchor gets one lexical lookup, and a failed lookup
does not refuse the request). They run the real loop on canned reads with
no model.

A later round can follow an exact quoted action or route from a cited
native source read. A known second occurrence with an independent source
context permits a hash-bound Read File. With no known target, lexical
Search proposes source locations to verify. A round that only discovered
navigation targets leaves the need's prior coverage in place. An exact
spelling alone establishes a possible join. The canonical read and normal
coverage checks still determine what the range supports. The current
public contract funds one follow-up read per need per later round.


### Gathering rules (user decision, 26 November 2026)

The user approved building the fetching rules a prototype proved
(`scripts/arbiter-answer-eval/arbiter_loop_prototype.py`) into the kernel loop on 26 November
2026 (change-log entry 75). Owner: `answer/rules.rs` (switches and standing
texts), `answer/extend.rs` (rules 3 and 4), `answer/deliver.rs` (rules 5
and 6) and `answer/report.rs` (rule 7), driven from `answer.rs`.

**When they apply.** The rules apply to a one-part request whose caller
inputs select them: caller `needs`, or a `deliverable` that brings standing
sub-needs (`code_change`, `test`). Every other request, including every
request without caller inputs, gathers exactly as before: the same reads in
the same order (test
`a_request_without_caller_inputs_keeps_its_reads_whatever_the_rules`). Its
reply gains only the additive `gathering` report, with no rule named.

**Switches.** `GatheringRules` has one switch per rule: `lanes`,
`standing`, `guidance`, `cited`, `structure`, `cross`, `delivery`. All are on
in a product build. A test-support build may bind another set for its
process once (`bind_gathering_rules`, the private evaluation pattern of
`software::bind_evaluation_layers`); `GatheringRules::parse` takes `all`,
`none` or a comma list, with `-name` turning one off (`all,-structure`).
The private evaluation fixture (service `evaluation_fixture_tests`,
`restart/change_set.rs`) binds it from
`AIRK_ARBITER_EVALUATION_ANSWER_GATHERING` when that is set and not empty.
Tests pass the rules directly (`answer_with_rules`, test builds only).

1. **Search lanes** (`lanes`). Each sub-need's round-one search also runs
   with `evidence_kind: code` (six hits), and a standing sub-need about the
   test command or the build file also with `evidence_kind: configuration`
   (four hits). A lane returns bounded snippets (`full_chunks: false`,
   `max_bytes` 4,096), not whole chunks: whole text comes from the
   sub-need's recipe (whole chunks of its kept hits) and from rule 4's
   definition reads (change-log entry 76). The lanes are beside the
   sub-need's recipe and outside its six-read cap; Core bounds them with
   `SEARCH_LANES_PER_SUB_NEED` (2).
2. **Standing sub-needs** (`standing`). The deliverable adds, after the
   caller's or the derived sub-needs and with the next ids: project rules,
   the test command, the build file and, for `code_change` only, the error
   path (texts in `rules::Standing::text`). `code_change` takes four, `test`
   three, any other deliverable or none three only beside caller needs. A
   multi-part request takes none. Each is an overview sub-need searched in
   its own words, with `origin: standing` on the wire (Core
   `SubNeedOrigin::Standing`, at most `MAX_STANDING_NEEDS`).
3. **Project guidance and cited files** (`guidance`, `cited`). When round
   one's searches and recipes have run, one `list_files` lookup per folder
   (the root, the top-level folders and the folders of each sub-need's three
   best passages, at most six) finds `CLAUDE.md`, `AGENTS.md`,
   `CONTRIBUTING.md` and `README.md` directly in it; each found file (at
   most eight) is read from line 1 (`context`, 101 lines). Then the
   repository paths named in those files and in the best passages (a folder,
   a file name with an extension, safe and relative), most cited first, at
   most six not already read, are read the same way. A root listing is one
   page of 200 paths; a guidance file past it is not found.
4. **Structure following** (`structure`). For each sub-need's two best code
   passages (at most six files), from the index's code facts with minimal
   rows: the file's definitions (one lookup) and the innermost definitions
   under 200 lines the passage overlaps; the calls inside each (a line-range
   lookup); a called name defined in the same file, else up to three looked
   up by name; up to two enclosing names' call sites by name; and up to two
   inherited type names per file, whose users outside the file's folder come
   from one `references` lookup each. At most 24 lookups. Definition ranges
   of one file closer than 20 lines are one hash-bound `read_file` (the
   index's file hash, at most 240 lines and 8,192 bytes); call sites (12
   lines each side) and users (15) are `context` reads.
5. **Every passage to every sub-need** (`cross`). At each barrier a
   sub-need's spans gain up to three passages of this round's reads for the
   part's other sub-needs, the ones sharing most of its rarer words (weight
   `1/sqrt(1 + passages holding the word)`). Focus, when enabled, scores
   them with its own spans within its existing shares.
6. **Delivery by need** (`delivery`). After the last round each sub-need
   ranks its own reads' hits (dispatch order), the part-level reads' and,
   with `cross`, the six other passages sharing most of its rarer words. A
   passage Focus scored keeps its best relevance; any other scores its
   share of the sub-need's rarer words, plus `0.25 / (1 + place)` among its
   own (0.25 for a passage rules 3 or 4 fetched for it). It keeps passages scoring at least half its best (at most eight,
   and at least its best). The budget is half the reply frame of passage
   text: a quarter split evenly, the rest by kept counts; passages go out in
   rounds across sub-needs, first within each share, then within the whole
   budget. A passage already sent, or four fifths held by sent passages of
   its file, is cited instead. A sub-need that kept any passage gets its
   best even past the budget, cites the delivered records (plus its
   structural ones) and is at least partial. The reply then carries the
   text of delivered spans only, and each sub-need's paragraph lists them.
7. **Counts and timings.** Every reply but a ticket carries `gathering`
   (Core `GatheringReport`): the rules on, and per stage (retrieve,
   guidance, cited, structure, coverage, follow_up, delivery) the reads by
   operation, the private lookups charged as calls, passages fetched,
   offered (per showing to a ranking) and delivered, model judgments (Focus
   pairs and model-scored card decisions) and elapsed milliseconds. Reads
   plus lookups equal `work.calls`. The delivery stage ends before the
   kernel takes the reply's `elapsed_ms`, so Union's packing is not in it.
   A service job reply re-attached by a later call carries that call's
   wait as `elapsed_ms`, while the stages keep the run's own times.

**Budgets.** The gathering reads and lookups of a part (rules 3 and 4)
stop at `GATHERING_READS_PER_PART` (48), within the part's call share and
the Answer's calls and candidates, after one follow-up read per sub-need for
each later round, and while a quarter of the retention bytes stays free.
When rule 3 or 4 is on, round one's recipe reads (every round-one read
that is not a search: whole chunks, outlines, excerpts, governing reads)
leave half the retention bytes free for them and the later rounds
(`ROUND_ONE_KEEPS`, `Gathering::recipe_floor`): a recipe read whose
`max_bytes` bound, with the bounds of the reads in flight and what is
retained, would leave less is not dispatched and is forgotten as if never
offered. Its nominations' byte allowance is taken from above the same
floor. Searches (the part search, anchors, sub-need searches and lanes)
are never held back. Measured fault it closes (first kernel baseline, 27
November 2026): at the default 262,144 bytes round one retained 257,684
bytes in 40 calls and the request stopped on `budget` inside `retrieve`;
guidance, cited and structure never ran (change-log entry 76).
Lookups are charged as nominations are (a call, a candidate, their bytes)
and kept in custody beside them; they are never evidence. Every passage
these rules add is an ordinary canonical read on the Answer's revision.

Tests: `arbiter::answer::rules::tests`, `answer::deliver::tests`,
`answer::extend::tests` (pure rules, expectations by hand) and
`answer::tests::gathering` (the real loop on canned reads, no model and with
synthetic Focus); `the_default_retention_leaves_round_one_room_for_guidance_cited_and_structure`
fills every evidence read to its bound at the default budget.
