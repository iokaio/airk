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
`answer_loop/log.rs` moved the loop semantics digest (change-log entry 37).
Before that they moved twice within L4 and the
earlier values are not current: once when `NativeCardModel` joined
`native_loop.rs`, and again in the review fixes (`03aa3bbf`), when the
choice-to-decision mapping moved into `loop_cards.rs`, Plan selection took
`max_sub_needs`, and `NativeCardModel` moved out to
`native_loop/card_model.rs`:

| Task | Policy |
| --- | --- |
| Core loop semantics | `488496bcf1c6d6879400eeb7b61fbb4534ac787861f6f9bf142c3a3480a817d4` |
| `airk.context_split.v1` | `ccea442cabb436794ae09f9fda204aa0359f08c1aba0d667099b65e9874a03ce` |
| `airk.context_plan.v1` | `5d219860d09afdd727fb2e3da50fbc04d990e8462da1de400076850894548215` |
| `airk.context_coverage.v1` | `c671150649dcd1deeb7e0b1d528182357f7eb1347cb728156256a027d15921fa` |
| `airk.context_follow_up.v1` | `89809e2d8377aaa29a14c86d87d2243336e079a0b1cbf9e378498f45fa62de15` |
| Batched Focus over the release pair policy | `50ba5c856d5f3ed81d86eacd62094de14599f56fcb39939126643758a7fae04a` |

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

**Model view.** One prompt per card call: state
`{task, segments: [{s, part, need, round, subject, cards}], rules}` of at most
65,536 bytes, the stage question, and candidates `c1..c8` in call order with
text `[s<k>] <card text>`. Machine identities never enter it.

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
