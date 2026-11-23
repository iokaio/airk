# Native Arbiter compilers

Core owns `CompilerRecipe`, `CompilerProfile` and stage/task identities. This
module owns prompt construction and tokenizer execution. Downloaded profiles
bind immutable data; they supply no executable template or model authority.

`QwenSingleChoiceV1` renders the fixed Qwen3 system/user chat and the publisher's
empty nonthinking assistant prefix. It scores the final prefix position against
the ordered A–H option vocabulary rows. Each symbol must encode as one token;
appending it must preserve every prefix token. It does not generate an answer,
use an Intern decision marker, or transfer an older model's qualification.

The Context input is batch one and unpadded, with at most 2,048 tokens and eight
complete native options. Order and original option identifiers are preserved.
Original state, question, identifier and option text are admitted under their
existing byte limits. Control-like `<|NAME|>` text and the nine bound chat,
thinking and tool controls are escaped before JSON string quoting. Reserved
decision markers, duplicate identifiers and token/option overflow refuse;
truncation, refill and reordering are forbidden.

The new Qwen compiled-input digest binds the original supplied tuple, exact
consumed compiler asset bytes, LF-canonical parent/child source policy and the
actual transformed prompt and all six tensors with their batch axes. Original
identity and model view remain distinct. Existing recipe digest inputs remain
unchanged. A digest is an identity, not permission to execute or read evidence.

Qualification tooling emits the selected compiler asset through the Rust
owner. The ignored native fixture test compares every prompt/tensor field with
an independently frozen publisher-template/tokenizer fixture and retains the
actual Rust-emitted bytes. Handwritten research metadata is not a native
fixture or a qualification receipt.

Context production graphs expose exactly the six declared inputs and the
single `scores` output. Diagnostic reference graphs may retain hidden states;
they are not the production ABI. The independently owned Focus compiler uses
its three-input relevance ABI and raw no/yes readout, with its own policy and
qualification. It grants no Context action or source authority.

Runtime admission checks shape, padding, positions, masks and closed readout
rows before tensor allocation. Executing a graph additionally requires the
service compute owner and an authenticated release or native evaluation
capability. Compiler parity, synthetic numerical parity and export do not prove
task quality, calibration, resource fit, platform qualification or publication.

`QwenRerankYesNoV1` has an independent early dispatch, accepting exactly empty
state, the original nonempty question and one original candidate identifier plus
its explicitly declared complete source window. It enforces the existing
question/identifier/window byte limits and binds the exact publisher control
catalog, no=2152 and yes=9693. The fixed publisher instruction, prefix and suffix
come from native code; question and document never supply instructions or template
controls. A single pass doubles original backslashes and quotes angle brackets as
literal ASCII `\u003c` and `\u003e` in the model view. This transform is injective;
original question, identifier and source-window bytes remain distinct and bound.
Actual source bodies and their line endings are never normalized or edited.

Focus separately tokenizes the exact publisher prefix, transformed pair body and
suffix without added special tokens, concatenates them, and refuses overflow
before inference. No cropping or refill occurs. It left-pads to the authenticated
profile's maximum tokens with 151643, with mask zero for padding and one for every
real token. Position IDs are the full static `0..tokens` sequence; the decision
position is its final live token and the metadata readout rows are always
`[2152, 9693]` with both enabled. One candidate pair is distinct from these two
output labels. The Rust qualification emitter currently emits static512 assets;
that does not establish a universal pair count, token allowance or resource tier.

The Focus input digest binds the original supplied tuple, exact consumed compiler
asset bytes, LF-canonical parent/child native policy plus the owning Core Focus
semantic policy, complete transformed prompt,
all three actual tensors with batch axes and decision/readout metadata. Service
additionally binds the full canonical Hit, exact original source-window offsets,
scope and native candidate identity. The task policy is the public owning
`compiler::focus::policy_digest`; neither a Context policy nor a caller-chosen
digest can substitute. Native parity fixtures must compare against the official
reference using the same transformed bytes, including control-like source/query
and literal-backslash collision cases. Raw publisher delimiter controls alone
prove no escaped-caller parity. Relevance scores grant no sufficiency or source
authority.


## Private F1/F2 exact-human-pair experiments

The test-support Focus experiment owner exposes exactly publisher F0 and one
software-evidence instruction F1. This is a private recipe enumeration, not a
new production CompilerRecipe, profile, wire/state format or execution grant.
Production compile_focus still selects the original publisher instruction.
Both experiments call the same injective quoting, independently tokenized
prefix/body/suffix, closed no2152/yes9693 catalog and no-crop static512 owner.
The full original tuple and recipe/instruction/native-source identity bind
every emitted model view. Changing the instruction requires a new census;
old publisher512 fit/quality results do not transfer. Editing this source
changes the existing full-source policy digest and requires fresh bindings.

The emitter reads the pinned original Rust visibility report and frozen role/
rights roster, checks all original question/function hashes and labels, and
retains every denominator row for both recipes as compiled or explicit refusal.
UNKNOWN rights and original overflow remain excluded from loss; fitting a
prompt never grants human-label, rights or training authority. A paired
comparison uses only the declared intersection with both refusal rosters
reported, never refills or recrops. Original DATA/exposure/FINAL restrictions
remain. Replay windows require the exact source/span/question/annotation and
accepted private-rights claims; compiler fit cannot adjudicate those claims.

Actual raw no/yes score observation calls an existing Scorer under the actual
service-compute owner and the caller original deadline. It retains both raw
finite logits and their bits, input/recipe identities and timings. It mints no
query permit, native runtime admission, learned cutoff, probability calibration,
terminal/evidence-role gold or qualification. An error after submission stops;
no replay, refund or new deadline is permitted.

Private training consumes only native-emitted prompt/tensor records, with full
vocabulary CE applied to the authentic last-prefix position and original human
yes/no target. Prompt tokens have no separate loss. Native inference width512,
left padding/positions and source bytes stay unchanged. LoRA adapts only q/k/v/o
projections under the frozen rank/rate/batch/epoch/seed/replay and original
owned-process/GPU/time caps. Reference checkpoint/head remain complete; merge
into a full checkpoint precedes a separate native export/numerical comparison.
Pre-score instruction selection, measured resource probe and replay acceptance
remain required. This code-only exploratory arm cannot establish broad Step8
mixture, fresh FINAL, ranking/retention quality or promotion.

General QASPER replay retains the original publisher instruction; only software code rows use the DEV-selected instruction. Both are native-emitted. Code comparisons use the identical selected instruction for original/adapted weights; replay is original human paragraph relevance, never complete evidence.

The private instruction-census emitter additionally reports native_focus_task_policy from the actual owning Rust policy_digest and focus_compiler_source_lf_sha256 from the actual compiled focus.rs bytes with CRLF→LF normalization and lone-CR refusal. These are emitted source identities, not guessed caller/profile values. A pending exact CPU compatibility row may consume them only after actual original1955publisher full-view/refusal parity,13focused compiler controls and the closed-row refusal controls. No arbitrary stale policy is accepted or old numeric/CAL proof transferred.

## Private Answer evaluation policy routing (22 November 2026)

This section covers the private evaluation build only. Product code paths,
product policies and every existing profile identity are unchanged.

**Focus SoftwareEvidenceV1 (change set 1).** `experiment::software_policy_digest`
binds the product publisher policy, the SoftwareEvidenceV1 instruction and
the LF-normalized `experiment.rs` source in the domain
`airk.private.focus-pair-policy/software-evidence-v1`. `compile_software` uses
the publisher recipe and admission of `compile_focus` with that instruction,
and it binds that policy into the input digest. In `runtime.rs`, under
`cfg(any(test, feature = "test-support"))` only, a profile whose `FocusRank`
task policy is exactly that digest validates against it and compiles through
`compile_software`. Every other profile keeps the publisher policy and
`Compiler::compile`.

**Context option B, `context_wide.rs` (change set 2).** This is a separate
task owner. `context_action.rs`, whose policy hashes that whole file, stays
byte-identical. Option B's policy (`airk.private.context-action-task-policy/option-b1`)
binds the product policy and the LF-normalized `context_wide.rs` source. Its
roster presents up to 8 source units (the product presents 4) and up to 6
Read cards (the product offers 5), with Sufficient and Clarify. It adds
NoJustifiedRead only into a free slot under the Core bound of 8 cards. Its
prompt repeats the product's request and rebuild checks and uses the
product's compact rendering. A kernel parity test holds that rendering equal
on product rosters. Selection takes the highest-scoring Read even when a
terminal card wins. The margin is measured over the next Read, a tie between
Reads abstains, and the calibration still decides acceptance.
`policy_for`, `prompt_for` and `selected_action_for` choose option B only in
test-support builds and only for a profile or roster that declares exactly
that policy. `runtime.rs` uses `policy_for` for Context policy validation, the
evaluation catalog check and joint calibration parsing.

**Digest-input change.** `runtime.rs` is a digest input. Its LF-normalized
source is pinned by the legacy CPU Focus source transition
(`cpu_source_transition`) and included in the CUDA FP16 math-policy digest
(`cuda/fp16.rs`). The routing above moves it from
`7b4650767610eafdfea4b6c273e042008e4eae4654dde516d438c6ee2f8449b3` to
`258d43714a14169d8ca0dd5993890bdcaa26204c13a8e583f83bb6e63bb698e0` (working
tree at 04:01 UTC on 22 November). This is a recorded digest-input change of
the private evaluation build, not a re-pin: the transition pin is not
updated, and its 8 `cpu_source_transition` tests are a known failing check
(owner: primary agent). The FP16 math-policy digest changes with it. The
evaluation's CUDA cells are FP32. Any product or FP16 CUDA use needs this
routing reverted or a recorded decision with a reviewed source row.

**Routing removed from runtime admission (L4, 22 November 2026).** The
one-turn evaluation is history, so `runtime.rs` no longer routes option-B or
SoftwareEvidenceV1 profiles: Context and Focus admission take the product
policies only, and `Scorer::compile` always uses `Compiler::compile`. The
recorded decision and its inverse-replay proof are in
`../runtime/cpu_source_transition/CONTRACT.md`, "Answer loop admission
supplement". `context_wide.rs` and `focus/experiment.rs` stay compiled for
their remaining kernel and service callers until Answer v2 is deleted.

## Answer loop card task policies (L4, 22 November 2026)

Owner: `airk_retrieval::arbiter::loop_cards` and its four stage modules
`loop_split`, `loop_plan`, `loop_coverage` and `loop_follow_up`. They are
compiled without `local-inference` so the no-model loop records the same
policies; only the model view (`loop_cards::prompt`) needs the compiler.

**Identity.** Each stage's policy is
`framed_digest(b"airk.arbiter.loop-<stage>-task-policy/1", [LF source of the
stage module, LF source of loop_cards.rs, the stage QUESTION, Core
loop_semantic_policy_digest])`. The Core digest
(`airk.arbiter.loop-core-semantics/1`) binds Core `answer_loop/cards.rs`,
`calibration.rs`, `focus_batch.rs`, `log.rs`, `semantics.rs`,
`native_loop.rs` and the Focus Core semantics, so the card and batched-Focus
semantics are part of every loop identity. Recorded values (control
`recorded_loop_policy_identities`). They moved a third time when L4 was
rebased onto L3-10 (a), whose Core edits to `native_loop.rs` and
`answer_loop/log.rs` moved the loop semantics digest (change-log entry 38).
Before that they moved twice within L4 and the
earlier values are not current: once when `NativeCardModel` joined
`native_loop.rs`, and again in the review fixes (`9fea69ae`), when the
choice-to-decision mapping moved into `loop_cards.rs`, Plan selection took
`max_sub_needs`, and `NativeCardModel` moved out to
`native_loop/card_model.rs`.

**Fourth move: Coverage evidence excerpts (recorded decision, 22 November
2026; change log entry 43).** These values were replaced by the fifth move
below.

- **Why they moved:** two digest inputs changed. Core `native_loop.rs`
  gained `CardEvidence`, the `evidence` field of `CardSegment`, their
  validation and the excerpt allowance, which moves the Core loop semantics
  digest and so every identity that hashes it. `loop_cards.rs`, hashed by
  the four card policies, now renders the evidence into the Coverage view.
- **What a Context model is shown changed for Coverage only.** Split, Plan
  and Follow-up views are byte-identical to before; their identities moved
  because they share the two inputs. Batched Focus ranking did not change;
  its identity moved because it hashes the Core loop semantics.
- **Proof that the old values no longer describe the source:** the control
  `recorded_loop_policy_identities`, run on the changed source with the
  values of `545c5025` still pinned, fails at the first identity (private
  receipt `answer-loop/L4/int-nextest-2c-ids-old-pins.log`). The pins were
  then moved to the values the source produces. No check was removed.
- **What did not move:** `runtime.rs`, the closed CPU Focus row, the release
  Focus pair policy and the one-turn Context policy. A profile that declares
  the card tasks must be emitted with the new task policies; one emitted
  with the old ones is refused at admission.

| Identity | Before (`545c5025`) | Now |
| --- | --- | --- |
| Core loop semantics | `488496bcf1c6d6879400eeb7b61fbb4534ac787861f6f9bf142c3a3480a817d4` | `307cb4a31434224446ee3712207c0aacbb6ab71d1fa556058e0eba789f35bb50` |
| `airk.context_split.v1` | `ccea442cabb436794ae09f9fda204aa0359f08c1aba0d667099b65e9874a03ce` | `fecca3cb9efebbe23abf2c9e029608fe377b07eed20acbe859c37e5c2c9f8377` |
| `airk.context_plan.v1` | `5d219860d09afdd727fb2e3da50fbc04d990e8462da1de400076850894548215` | `487f22fdf94b0eabeb396184e7abc3cafc48ae6a826eeade9a91a19de84f1db5` |
| `airk.context_coverage.v1` | `c671150649dcd1deeb7e0b1d528182357f7eb1347cb728156256a027d15921fa` | `846f1db0ae1af64921166cc29e872e1094b842606cf6824353e32204e8f4bf58` |
| `airk.context_follow_up.v1` | `89809e2d8377aaa29a14c86d87d2243336e079a0b1cbf9e378498f45fa62de15` | `104b24bbda6baccb77609f1bbe1490f8c9beacc526c839ba920665c291f391b1` |
| Batched Focus over the release pair policy | `50ba5c856d5f3ed81d86eacd62094de14599f56fcb39939126643758a7fae04a` | `19e8564765aaa68cad46fb08c287c22f78399657ab3e9e1ae345790b2dda0df4` |

**Fifth move: Follow-up evidence and the review of the fourth move
(recorded decision, 22 November 2026; change log entry 44).** These values
were replaced by the sixth move below.

- **Why they moved:** three digest inputs changed.
  - Core `native_loop.rs`: the evidence fields `skipped_bytes`, `truncated`
    and `same_as`, the segment fields `unshown` and `verdict`, evidence on
    Follow-up segments, the least excerpt and the allowance of five bytes
    for four tokens.
  - Core `answer_loop/cards.rs`: the decision source `not_dispatched`; and a
    doc comment of `answer_loop/log.rs` on what a basis means.
  - `loop_cards.rs`, hashed by the four card policies: the view's fixed key
    order, the Follow-up verdict and evidence, the new row fields, the
    rewritten decision marker and the corrected rules texts.
- **What a Context model is shown changed for all four stages.** Coverage
  and Follow-up views gained content. Split and Plan views hold the same
  values in a different key order (`task`, `rules`, `segments`). Batched
  Focus ranking did not change; its identity moved because it hashes the
  Core loop semantics.
- **Proof that the old values no longer describe the source:** the control
  `recorded_loop_policy_identities`, run on the changed source with the
  values of `54b1e7a2` still pinned, fails at the first identity (private
  receipts `answer-loop/L4/int2-nextest-ids-old-pins.log` and
  `int2-nextest-ids-live.log`). The pins were then moved to the values the
  source produces. No check was removed.
- **What did not move:** `compiler.rs` and `compiler/qwen.rs` (the Qwen
  compiler policy), `runtime.rs` (the FP16 math policies), the closed CPU
  Focus row, the release Focus pair policy and the one-turn Context policy.
  A profile that declares the card tasks must be emitted with the new task
  policies; one emitted with the old ones is refused at admission.

| Identity | Before (`54b1e7a2`) | Now |
| --- | --- | --- |
| Core loop semantics | `307cb4a31434224446ee3712207c0aacbb6ab71d1fa556058e0eba789f35bb50` | `2f89a3ae30da2ed9b8a01625babd0b276ea2c5f61e6fa199c969792742264065` |
| `airk.context_split.v1` | `fecca3cb9efebbe23abf2c9e029608fe377b07eed20acbe859c37e5c2c9f8377` | `f1ecc7d9755edc7d6d165e752a1ae461d500b5269f998c35fd960644524f12da` |
| `airk.context_plan.v1` | `487f22fdf94b0eabeb396184e7abc3cafc48ae6a826eeade9a91a19de84f1db5` | `064ec90a339ea369e1c9161d7aed2a3d2a2dbf1d180a844c0b6ed1b052959ba1` |
| `airk.context_coverage.v1` | `846f1db0ae1af64921166cc29e872e1094b842606cf6824353e32204e8f4bf58` | `c2eab6c9e147edaf437a407e5999f5674b3013f5d7b5d21f3c3ffa473643cfce` |
| `airk.context_follow_up.v1` | `104b24bbda6baccb77609f1bbe1490f8c9beacc526c839ba920665c291f391b1` | `b1edccc6d81d102beaa6c3fec2e4052e6147401278dc3f32c7ff8b79fe6614a1` |
| Batched Focus over the release pair policy | `19e8564765aaa68cad46fb08c287c22f78399657ab3e9e1ae345790b2dda0df4` | `f90f9c773879cc23d462e0c99d485ed219666dff6d57cc5cd2479e80c450798f` |

**Sixth move: Focus pairs counted as dispatched (recorded decision, 23
November 2026; change log entry 53).** The values below are current.

- **Why they moved:** one digest input changed. Core `answer_loop/log.rs`
  counts the decision log summary's `focus_pairs` and the log's own Focus
  budget check by each receipt's `candidate_count` (pairs dispatched)
  instead of its scored `pairs`, refuses a receipt outside 1 to 32
  dispatched pairs or with more scored than dispatched, and names the
  figures in its budget error. That moves the Core loop semantics digest
  and every identity that hashes it. There is no correct fix without it:
  the summary the reply is validated against is built in `log.rs`.
- **What a model is shown did not change:** no card view, question, rule or
  Focus ranking changed; the card and batched Focus identities moved only
  because they hash the Core loop semantics.
- **Proof that the old values no longer describe the source:** the control
  `recorded_loop_policy_identities`, run on the changed source with the
  values of `2994c65d` still pinned, fails at the Core digest (private
  receipt `answer-loop/L4/focusbudget-nextest-retrieval-pins-oldpins.log`).
  The pins were then moved to the values the source produces
  (`focusbudget-nextest-retrieval-pins-new2.log`, 14 passed). No check was
  removed.
- **What did not move:** `compiler.rs`, `compiler/qwen.rs`, `runtime.rs`,
  the closed CPU Focus row and the `cpu_source_transition` pins, the
  release Focus pair policy and the one-turn Context policy. Profiles that
  declare the card tasks, and loop Focus profiles, must be re-emitted with
  the new task policies.

| Identity | Before (`2994c65d`) | Now |
| --- | --- | --- |
| Core loop semantics | `2f89a3ae30da2ed9b8a01625babd0b276ea2c5f61e6fa199c969792742264065` | `99b87f98a14d427d5e1a16a3a0710dc838682298846cc464060947aa292209fd` |
| `airk.context_split.v1` | `f1ecc7d9755edc7d6d165e752a1ae461d500b5269f998c35fd960644524f12da` | `32fcf95cfba16db0fcfb969d39ca55a08fa0ae0622ae00a731e3e41fa3568755` |
| `airk.context_plan.v1` | `064ec90a339ea369e1c9161d7aed2a3d2a2dbf1d180a844c0b6ed1b052959ba1` | `e180b6be12945656bbb863dfe6ad59771182356c6ad2da68a1d77de7d3492593` |
| `airk.context_coverage.v1` | `c2eab6c9e147edaf437a407e5999f5674b3013f5d7b5d21f3c3ffa473643cfce` | `f71c7c7eecd867d97b1a07e3ec0faf12869f6761c3c34c02c10138568d73462e` |
| `airk.context_follow_up.v1` | `b1edccc6d81d102beaa6c3fec2e4052e6147401278dc3f32c7ff8b79fe6614a1` | `fe85b1e08cefbf6446a9156c3cd5e37aab112733a225ff3847e5ce8392be7ae7` |
| Batched Focus over the release pair policy | `f90f9c773879cc23d462e0c99d485ed219666dff6d57cc5cd2479e80c450798f` | `839af2a52020bfc7d8bbb2e2a13fbea517c86ba9d866b6830ab378853c493c90` |

**Rosters.** Kernel code proposes every card; this owner never adds, drops
or rewrites one. Before compile it checks each segment's shape beyond Core's
validation: symbols are the prefix A, B, C, ...; a call is one stage with at
most eight cards; Split is one unbatched segment, keep-whole first and then
only whole partitions; Plan is at least one `need ...` card followed by the
reference card `no further sub-need`; Coverage is covered first and then
only read-more or search cards; Follow-up has at least two reads. A roster
that breaks these refuses. This replaces the design's rebuild-from-proposals
proof, because the proposers live in the kernel (L3) and the decision log
already binds each roster by `roster_digest`.

**Model view.** One prompt per card call: a state document of at most
65,536 bytes, the stage question, and candidates `c1..c8` in call order with
text `[s<k>] <card text>`. Machine identities never enter it.

- **Key order is part of the view.** The state is built from typed rows, so
  its keys are in a fixed order whatever map type `serde_json` uses: `task`,
  `rules`, `segments`; and in a segment `s`, `part`, `need`, `round`,
  `subject`, `verdict`, `verdict_by`, `evidence`, `not_shown`, `cards`. The
  model reads the rules before the evidence and the evidence before the
  cards. Four tests pin the bytes of a Coverage, a Follow-up, a Plan and a
  Split view.
- **Split and Plan** segments hold `s`, `part`, `need`, `round`, `subject`
  and `cards`, and the base rules alone.

**Evidence and verdict in the view (change log entries 43 and 44).** Every
Coverage and Follow-up segment has an `evidence` list, in the request's
order (Focus rank order, checked by Core):

- a row is `{at, focus, text, before, after, snippet, same_as}`:
  - `at` is `path:start-end`, the form a Covered card cites;
  - `focus` is the span's relevance to three decimals, absent when Focus did
    not score it;
  - `text` is the excerpt;
  - `before` and `after` are the bytes of the span's text left out before
    and after the excerpt, each absent when zero;
  - `snippet` is present (true) when the read itself returned only part of
    the text there;
  - a repeat row has `same_as`, the `path:start-end` of the row that shows
    its text, and no `text`;
- `not_shown` counts gathered spans the call shows nothing of, and is absent
  when zero;
- a sub-need with nothing to show has `evidence: []`;
- a Follow-up segment has `verdict` (`partial` or `missing`) and
  `verdict_by` (`model` or `fixed rules`);
- **rules:** Coverage and Follow-up add sentences that explain the rows.
  They say what the list holds (this round's spans; not earlier rounds, and
  not reads that return only locations) and how it is ordered (scored spans
  first, highest first, then the gathered order). Follow-up also explains
  the verdict and what each kind of proposed read does;
- **the decision marker:** the compiler refuses an input that holds
  `<decision>`. The view writes it `<¦decision¦>`, as the compiler writes
  every other control token, in the state and in the card texts. Indexed
  text that quotes the marker therefore costs nothing. The request, its
  input digest and the decision log keep the original text and byte ranges;
  `compiler.rs` is unchanged;
- **indexed text is data:** the state is one JSON document, quoted again by
  the Qwen recipe. A test puts quotes, backslashes, chat-template tokens,
  the decision marker and text imitating rules, options and evidence rows
  through the view, the compiler's escape and the prompt, and checks that
  the prompt keeps its three turns and two options and that the text arrives
  as one string. What an indexed file can still do is fill its own excerpt
  with misleading words; the model only scores the kernel's cards, and
  calibration gates acceptance;
- the view shows what the request carries and never cuts or reorders it. The
  kernel bounds the excerpts by the call's token reservation and Core
  refuses a request above the allowance (Core arbiter contract, "Evidence,
  verdict and calls not dispatched").
Card texts, rosters, selection and the decision mapping are unchanged.

**Selection** (`loop_cards::select`), per segment from that segment's slice
of the call's scores:
- the margin is exactly the segment's top raw score minus its runner-up
  (kernel contract), and a one-card segment abstains;
- calibrated mode: a unique top card (a tie abstains) is accepted only when
  the stage's `LoopCalibration` has a rule for its class at that margin
  (Plan uses class `plan_need`); zero rules abstain, so the kernel's
  deterministic order decides;
- Plan: the proposals scored above the reference card, highest first, at
  least one and at most four; the reference card is never chosen;
- top-score mode (test-support only, `DecisionMode::TopScore`): the top card,
  ties to the lowest symbol, never abstaining, with no rule.

**A loop profile is a valid profile (change log entry 46).** Core accepts a
Context profile that declares the card tasks (`Stage::declares`), so a
profile from `admit_loop_tasks` passes `Profile::validate`,
`ExecutionProfile::for_qualification` and
`EvaluationCatalog::from_native_lease`, and reaches the scorer's own
checks. Release publication (`publication.rs`) still takes only the
one-turn task for a Context profile: publishing loop profiles in a release
catalog is the product rollout after L7 (change log entry 31).

**Admission.** `runtime/loop_tasks.rs` admits a Context profile that declares
exactly the four card tasks under these policies, or exactly the one-turn
task, never both. `admit_loop_tasks` declares the tasks on a Context profile
and binds its single Calibration asset to a zero-rule `LoopCalibrationSet`
(one entry per card task, in card-task order); the emitter recomputes the
manifest. `Scorer::loop_calibration()` returns the set authenticated at open;
the inference lane looks up one card task's entry in it.

**Decision mapping.** `loop_cards::outcomes` turns each segment's choice
into its advice outcome: one accepted card (Plan: several, in chosen order)
by text digest with its rule and margin, or the abstention and its reason.
It lives here, so the mapping is part of every card policy identity (review
L4).

**Batched Focus (`loop_focus.rs`).** A batch receipt records
`loop_focus::policy_digest(pair policy)` =
`framed_digest(b"airk.arbiter.loop-focus-batch-policy/1", [pair policy, LF
source of loop_focus.rs, Core loop semantics])`, so a change to batched
ranking or the batch receipt moves the identity recorded on the receipt it
changes, while the pair policy and the closed CPU Focus row stay put.
`loop_focus::rank` orders a complete batch's scored prefix by relevance
(ties to the lower index), then the unscored tail in canonical order.

**Plan limit.** `select` takes the request's `max_sub_needs` (clamped to
1..=4); Plan never chooses more (review M2).
