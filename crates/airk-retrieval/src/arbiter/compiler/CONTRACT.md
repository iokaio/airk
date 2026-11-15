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
