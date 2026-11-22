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

Combined diagnostics keeps the complete observation under each corresponding
member.result; it never fuses authorities or global enrollment state. Complete
copy/export retains precision, native fallback reasons and all variant metadata.
The additional observation is bounded256KiB inside unchanged original reply/frame
ceilings. No retries or replacement clock are introduced.

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

## Answer /3 loop (`answer/`), what holds and what does not

`Kernel::answer_with` runs the section 12 loop: Split, per-part Plan,
Retrieve, Read and Coverage, Follow-up and Compose, then Union. Every read is
a canonical dispatch through the one shared read owner (`answer/gather.rs`);
models only choose among cards the kernel proposes. The design and its
recorded deviations are in the private L3 notes; the ones that bear on
callers are listed here.

What holds:

- **Concurrent reads, deterministic reply.** A wave's reads run on at most
  four scoped threads (`answer/wave.rs`). Claims follow dispatch order, every
  claimed read runs, and the reads that ran are a prefix of the wave; the
  rest are released. Accounting and observation run after the wave in
  dispatch order, so completion order never changes the reply, the decision
  log or custody. A read that panics halts the wave and its panic (the
  lowest-index one) is raised again. Governing `file_info` probes, batched
  Focus and card calls run on the driver thread at the round barrier.
- **Per-part model shares.** Shares are cut after Split. A card call batches
  rosters of one part only and is charged to that part's slot share; its
  reservation is capped by the part's token share. Focus is sized by each
  part's token and pair shares and half the part-round allowance, and spans
  past it stay in coverage unscored. A finished part hands back what it did
  not use. Every roster a present model could not score is disclosed as
  `ModelShare`.
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
- **Presentation trimming is not omission.** Union levels 1 to 4 shorten
  records (uncited hits, excerpt cuts, compact paragraphs) and disclose it
  per projection (`hits_total`, `rows_total`, `truncated`); a part is
  `shortened` only when records are omitted, as Core validates against
  `omitted_count`.
- **Coverage rosters show bounded evidence excerpts** (change log entry 43).
  - A Coverage card call shows each sub-need the spans gathered for it this
    round, in Focus rank order, as `CardEvidence` rows of its `CardSegment`.
  - The call's excerpts total at most its excerpt allowance (Core
    `native_loop::evidence_allowance` of the tokens the call reserves: 6,144
    bytes at 8,192 tokens). The existing reservation bounds it; no budget,
    share or pool rule changed.
  - `coverage::excerpt_cuts` decides the cut. Every span of the call first
    gets an equal share of the allowance. What the shorter texts leave goes
    to the cut ones in call order (segment order, then rank). Each excerpt
    then ends on a character boundary. The result depends only on the texts,
    their order and the allowance.
  - Excerpts are prefixes of hits of records already read and retained under
    the Answer's revision fence. Building them reads nothing.
  - The roster's subject names the sub-need's kind in the words of its Plan
    card (`plan::wording`, for example `need callers: restart_guard`). A
    definition and a callers sub-need of one identifier have the same query,
    so the bare query did not say what evidence to look for.
- **The log says what a Coverage call showed.**
  - For a decision taken in a dispatched Coverage call (model, abstained or
    unavailable), the entry's `basis` is the byte range of each excerpt, and
    `basis_omitted` is every cut tail and every span not shown at all. The
    reply's compact basis counts both, so a cut is visible in the reply.
  - A decision taken without a card call is unchanged. No excerpt is built,
    and its basis names the whole spans the deterministic rule read.

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
- **Follow-up rosters show no evidence** (owner: the primary agent assigns;
  found while closing the Coverage gap, change log entry 43). A Follow-up
  roster's subject is the sub-need's bare query, without its kind, and its
  cards are the candidate reads. The model sees neither the coverage verdict
  nor any text that was read. Core refuses evidence on a segment that is not
  Coverage, so showing it there needs a recorded wire decision, and that
  moves the loop identities again. Naming the kind in the subject is a
  kernel-only change that moves no identity.
- **A span is cut from its start only** (limit). An excerpt is the first
  bytes of its span's text. A span whose relevant lines come after the cut
  shows the model its beginning; the cut is disclosed (`cut` in the view,
  `basis_omitted` in the log), not corrected.

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
