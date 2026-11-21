# Arbiter Context slot diagnosis

These scripts ask one question about a retained Context calibration panel:
does the model's choice among the action cards follow the card's content, or
the slot the card sits in? They describe the panel by slot, check the compiled
inputs against the pinned tokenizer, rebuild inputs with the cards, the reads
or the option symbols permuted, score them on the ONNX Runtime CPU or WebGPU
provider and count how the choice moved. Two probe forms ask what would make
the scores follow the read: option lines that state their read in words, and
two-option inputs that ask about one read at a time.

This is a development diagnosis. It fits no rule and qualifies nothing, and a
probe form is not a proposed product change. The inputs and every output are
private campaign files. Keep them under the private campaign root and do not
commit them. The outputs hold ordinals, slots, kinds, token ids and scores.
They hold no row id and no question, evidence or card text, but the token ids
of a variant encode the private prompt, so `variants.json` is as private as
the panel.

## Layout the scripts rely on

The native prompt builder is `compact_prompt` in
[context_action.rs](../../crates/airk-retrieval/src/arbiter/context_action.rs),
and the template is `prompt` in
[qwen.rs](../../crates/airk-retrieval/src/arbiter/compiler/qwen.rs). A
compiled prompt has a system turn, then `State:`, `Question:` and `Options:`
in the user turn, then the assistant turn. Each option line is
`SYMBOL = "id": "text"`. The scores are read at the last token for the symbol
tokens, in symbol order. `layout.py` parses that shape and refuses a prompt
that does not render back to the same characters.

A slot is a symbol: the score at index j belongs to symbol j. A body is what a
slot shows. A variant is described by `sources`.

| Family | What changes | `sources` |
| --- | --- | --- |
| `cards` | The option texts move. Symbols and option ids stay. | Per slot, the original slot whose text it shows |
| `cards_ids` | The option texts move together with their ids. | The same |
| `reads` | The verified reads in the state move. Options stay, so each read card names another read. | Per read slot, the original read it names |
| `letters` | The symbols move. Every line keeps its place, id and text. | Per symbol, the line it is attached to |
| `stated` | Every read card also states its read in words. Nothing moves. | The identity |
| `stated_reads` | The reads move, and every read card states the read it now names. | As for `reads` |
| `pair`, `pair_swapped` | Two options only: one generic read card for a single read, and the sufficient card. | The one read that is kept |

`cards` builds every rotation and the reversal. `cards_ids` builds rotations
by 1 and 3 and the reversal. `reads` builds every rotation and rebuilds the
path table in first-use order, as the native builder would for that read
order. `letters` builds rotations by 1 and 3.

Three families are probes with a form that the native code never writes:

- `letters` separates the symbol from the line position.
- `stated`, with `stated_reads`, adds the read to its option line, after the
  read number and in brackets: the search string of a literal search, or the
  path and line range of a context read. The words come from the state of the
  same prompt. Everything else is unchanged. `--family stated` builds the
  original read order and every rotation.
- `pairs` builds, for every read, a `pair` input and a `pair_swapped` input.
  The state keeps only that read, as read number 1, and counts the others as
  omitted. The options are the first read card and the sufficient card under
  the first two symbols and ids; `pair_swapped` exchanges the two texts. The
  native roster builder always adds the clarify card, so it never writes this
  roster, but the template accepts two options.

A probe prompt is still checked against the rules the compiler applies: it
parses as the template, meets the input limits, offers at least two options
and is extended by each symbol as one token.

## Scripts

Run every script with `python -I -B`.

### `slot_table.py`

Tabulates the panel by slot, with no model and no tokenizer.

- `--panel`: the compiled rows, each with `ordinal`, `id`, `roster` and
  `compiled` including the prompt.
- `--raw`: scores for the same rows. This is the retained raw score panel, or
  a `scores.json` of `score_variants.py`, whose identity variants are used.
  The input digest ties each score vector to its row. Two models can share
  one compiled input, so for a score receipt the table also records the
  graph that scored it.
- `--preflight`: the reference preflight that holds the certificates.
- `--compiler`: the compiler profile of the model, for the symbol token ids
  and the token limit.
- `--output`: a new file.

The output gives, per slot, the card kinds, the forms of the verified reads,
raw and centred scores, rank counts and certificates, and the counts of the
winning slot and of the best read slot. It also gives the share of the
centred score variation that one fixed per-slot profile explains, over all
options and over the read cards alone. Under
`profile_removed` it shows what each row prefers once the mean centred scores
of the other rows are taken away slot by slot: the winning slots, the best
read slots and the rank of each certified read among the read cards. It ends
with the count of rows that pass each static check: the option block equals
the cards of the roster, the option texts are distinct, the candidates are
the symbol tokens in order, the decision position is the last token, and the
token count is within the compiler limit and the remaining allowance.

### `build_variants.py`

Checks every row against the pinned tokenizer and builds inputs for chosen
rows. It needs the `tokenizers` package.

- `--panel`, `--compiler`: as above.
- `--tokenizer`: the `tokenizer.json` whose digest the compiler profile pins.
  Another file is refused.
- `--ordinal`, repeated, and `--every N`: the rows to build variants for. All
  rows by default.
- `--family`, repeated: the families to build. All of them by default.
- `--identity-only`: build the unpermuted input alone, to score a whole panel.
- `--output`: a fresh, empty directory. It receives `variants.json`.

The tokenizer checks run on every row: the compiled tokens equal the encoding
of the whole prompt, so nothing is truncated; they decode back to the prompt;
each symbol is one token and appending it leaves the prefix unchanged; the
candidates are the symbol tokens; and the input ends with the assistant turn.
The identity variant of a built row must reproduce the compiled tokens
exactly, or the run stops. The output also counts the `cards` variants whose
tokens are the original tokens in another order, and gives the smallest and
largest token count of every family.

### `score_variants.py`

Scores the variants with a portable graph. It needs `onnxruntime` and
`numpy`, and for WebGPU the `onnxruntime-ep-webgpu` plugin package. It takes
its session set-up and padding from
[the WebGPU probe](../arbiter-webgpu-probe/README.md).

- `--model` and `--weights`: the graph and the weight files it reads.
- `--variants`: `variants.json`.
- `--reference`: the retained raw score panel, when one exists for the model.
  The identity variant of every row must reproduce its scores within
  `--tolerance`, 0.001 by default, or the run stops. Without a reference
  nothing checks the unpermuted scores: only the rebuilt tokens are known to
  be exact.
- `--provider`: `cpu` (default) or `webgpu`, with `--option key=value` for
  WebGPU provider options and `--session-config key=value` for session
  configuration entries.
- `--pad-to` and `--pad-token-id`: right-pad every input to one token count.
  A WebGPU run requires it, because the session keeps buffers for every input
  length it has seen. The graph must support right-padding, which the WebGPU
  probe reads from its structure. A length that does not hold the longest
  input is refused before the session opens.
- `--intra-threads`: 8 by default, for a CPU run.
- `--output`: a fresh, empty directory. It receives `scores.jsonl`, one line
  per forward as the run proceeds, and `scores.json` at the end. A WebGPU
  receipt also records whole-adapter memory before load, after load, at peak
  and after the run.

Run one GPU process at a time.

### `compare_scores.py`

Compares two completed `scores.json` files of one model over the variants
they have in common, for example a CPU run and a WebGPU run. It prints the
largest score difference and the count of forwards whose top option differs,
in total and by family.

### `report.py`

Counts how the choice moved. It needs no model.

- `--scores`, repeated: completed `scores.json` files of one model. Runs over
  the same rows are joined by ordinal, and a repeated variant must agree.
- `--table`: the output of `slot_table.py` for the same panel and model, for
  the option kinds, the read forms and the certified slots.
- `--output`: a new file.

For every variant and family it counts, for the top card and for the best
read card, whether the choice went with the body (`content`), stayed on the
slot (`slot`) or did neither, and which slots and bodies won. A body that a
variant leaves in place gives `unmoved_kept` or `unmoved_lost`. A
`stated_reads` forward is compared with the `stated` forward of its row, and
every other forward with the identity.

A base forward and all rotations of a family show every body at every slot
once. For that design the report splits the centred scores into a part that
follows the slot and a part that follows the body, each in common to all rows
and by row, and gives the mean centred score of every slot and body pair. It
also ranks the bodies of each row by their scores averaged over the slots.
There are three designs: `cards` over all options, and `reads` and `stated`
over the read slots. For the two read designs, `slot_averaged_reads` gives
the slot-averaged score of a read by read form, and for certified reads
against the other reads of their rows. Centring removes the mean of the
scores of one forward, because a shift common to all options does not change
the choice.

`certified` gives the rank of each certified read among the read cards: by
the unpermuted scores, by the `stated` scores and under the slot average of
each design. `certified_summary` counts the first places and gives the mean
rank. With five read cards, chance is one first place in five and a mean rank
of 3.

`pairs` describes the two-option forwards by the read-minus-sufficient score
difference of each read: with the read card first, with it second, and
averaged over the two orders, in which a preference for the first or the
second option cancels. For each it gives the share of the variation that lies
between rows and within rows. It also gives how much being listed first adds,
how often the order decides which option wins, the mean difference by read
number and by read form, the correlation of a row's mean difference with the
full roster's best read minus sufficient, and the rank of each certified read
among the reads of its row.

## Use

```console
python -I -B scripts/arbiter-context-slot-diagnosis/slot_table.py --panel <compact.json> --raw <raw-panel.json> --preflight <reference-preflight.json> --compiler <compiler.json> --output <slot-table.json>
python -I -B scripts/arbiter-context-slot-diagnosis/build_variants.py --panel <compact.json> --tokenizer <tokenizer.json> --compiler <compiler.json> --ordinal 10 --every 5 --output <fresh-dir>
python -I -B scripts/arbiter-context-slot-diagnosis/score_variants.py --model <model.onnx> --weights <weights.bin> --variants <variants.json> --reference <raw-panel.json> --output <fresh-dir>
python -I -B scripts/arbiter-context-slot-diagnosis/score_variants.py --model <model.onnx> --weights <weights.bin> --variants <variants.json> --provider webgpu --option storageBufferCacheMode=simple --session-config "optimization.disable_specified_optimizers=ConstantFolding;FuseFp16InitializerToFp32NodeTransformer" --pad-to 2048 --pad-token-id 151643 --output <fresh-dir>
python -I -B scripts/arbiter-context-slot-diagnosis/compare_scores.py <cpu-scores.json> <webgpu-scores.json>
python -I -B scripts/arbiter-context-slot-diagnosis/report.py --scores <scores.json> --table <slot-table.json> --output <report.json>
```

A forward of about 1,400 tokens took about 5 seconds on 8 CPU threads for the
Compact graph. On WebGPU, padded to 2,048 tokens, it took about 0.8 seconds.
All families together are 32 forwards per row.

## Limits

- The scores come from a portable graph on the CPU or WebGPU provider, not
  from the calibrated CUDA identity. The identity check bounds the difference
  on the unpermuted inputs only, and only for a model that has retained
  scores.
- A permuted input is not an input the product would compile for the same
  request, except that a `cards` or `reads` variant is what a roster or a
  read queue in another order would give. A probe input has a form the
  product never compiles.
- The slot and body split describes the scored rows. It is not a test of
  significance, and a small sample of rows carries a small number of
  certificates.

## Tests

`test_layout.py` and `test_scripts.py` cover the pure helpers. They need no
tokenizer, model, provider or private file, and their expected values are
written by hand.

```console
python -I -B -m unittest discover -s scripts/arbiter-context-slot-diagnosis -p "test_*.py"
```
