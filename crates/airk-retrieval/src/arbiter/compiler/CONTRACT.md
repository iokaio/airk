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
