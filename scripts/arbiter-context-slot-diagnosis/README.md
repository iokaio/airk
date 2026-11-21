# Arbiter Context slot diagnosis

These scripts ask one question about a retained Context calibration panel:
does the model's choice among the action cards follow the card's content, or
the slot the card sits in? They describe the panel by slot, check the compiled
inputs against the pinned tokenizer, rebuild inputs with the cards, the reads
or the option symbols permuted, score them on the ONNX Runtime CPU provider
and count how the choice moved.

This is a development diagnosis. It fits no rule and qualifies nothing. The
inputs and every output are private campaign files. Keep them under the
private campaign root and do not commit them. The outputs hold ordinals,
slots, kinds, token ids and scores. They hold no row id and no question,
evidence or card text, but the token ids of a variant encode the private
prompt, so `variants.json` is as private as the panel.

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
slot shows. A variant is described by `sources`, one entry per slot.

| Family | What moves | `sources[j]` |
| --- | --- | --- |
| `cards` | The option texts. Symbols and option ids stay. | The original slot whose text slot j shows |
| `cards_ids` | The option texts together with their ids | The same |
| `reads` | The verified reads in the state. Options stay, so each read card names another read. | The original read that read slot j names |
| `letters` | The symbols. Every line keeps its place, id and text. | The line that symbol j is attached to |

`cards` builds every rotation and the reversal. `cards_ids` builds rotations
by 1 and 3 and the reversal. `reads` builds every rotation and rebuilds the
path table in first-use order, as the native builder would for that read
order. `letters` builds rotations by 1 and 3. The native compiler never writes
a `letters` input: that family is a probe that separates the symbol from the
line position.

## Scripts

Run every script with `python -I -B`.

### `slot_table.py`

Tabulates the panel as retained, with no model and no tokenizer.

- `--panel`: the compiled rows, each with `ordinal`, `id`, `roster` and
  `compiled` including the prompt.
- `--raw`: the raw score panel for the same rows.
- `--preflight`: the reference preflight that holds the certificates.
- `--compiler`: the compiler profile of the model, for the symbol token ids
  and the token limit.
- `--output`: a new file.

The output gives, per slot, the card kinds, the forms of the verified reads,
raw and centred scores, rank counts and certificates. It also gives the share
of the centred score variation that one fixed per-slot profile explains.
Under `profile_removed` it shows what each row prefers once the mean centred
scores of the other rows are taken away slot by slot: the winning slots, the
best read slots and the rank of each certified read among the read cards. It
ends with the count of rows that pass each static check: the option block
equals the cards of the roster, the option texts are distinct, the candidates
are the symbol tokens in order, the decision position is the last token, and
the token count is within the compiler limit and the remaining allowance.

### `build_variants.py`

Checks every row against the pinned tokenizer and builds permuted inputs for
chosen rows. It needs the `tokenizers` package.

- `--panel`, `--compiler`: as above.
- `--tokenizer`: the `tokenizer.json` whose digest the compiler profile pins.
  Another file is refused.
- `--ordinal`, repeated, and `--every N`: the rows to build variants for. All
  rows by default.
- `--family`, repeated: the families to build. All of them by default.
- `--output`: a fresh, empty directory. It receives `variants.json`.

The tokenizer checks run on every row: the compiled tokens equal the encoding
of the whole prompt, so nothing is truncated; they decode back to the prompt;
each symbol is one token and appending it leaves the prefix unchanged; the
candidates are the symbol tokens; and the input ends with the assistant turn.
The identity variant of a built row must reproduce the compiled tokens
exactly, or the run stops. The output also counts the `cards` variants whose
tokens are the original tokens in another order.

### `score_variants.py`

Scores the variants with a portable graph on the CPU provider. It needs
`onnxruntime` and `numpy`.

- `--model` and `--weights`: the graph and the weight files it reads.
- `--variants`: `variants.json`.
- `--reference`: the retained raw score panel. The identity variant of every
  row must reproduce its scores within `--tolerance`, 0.001 by default, or
  the run stops.
- `--intra-threads`: 8 by default.
- `--output`: a fresh, empty directory. It receives `scores.jsonl`, one line
  per forward as the run proceeds, and `scores.json` at the end.

### `report.py`

Counts how the choice moved. It needs no model.

- `--scores`, repeated: completed `scores.json` files. Runs over the same
  rows are joined by ordinal, and a repeated variant must agree.
- `--table`: the output of `slot_table.py`, for the option kinds and the
  certified slots.
- `--output`: a new file.

For every variant and family it counts, for the top card and for the best
read card, whether the choice went with the body (`content`), stayed on the
slot (`slot`) or did neither, and which slots and bodies won. A body that a
permutation leaves in place gives `unmoved_kept` or `unmoved_lost`.

The identity and all rotations of a family show every body at every slot
once. For that design the report splits the centred scores into a part that
follows the slot and a part that follows the body, each in common to all rows
and by row, and gives the mean centred score of every slot and body pair. It
also ranks the bodies of each row by their scores averaged over the slots,
and gives the rank of each certified read under that average. Centring
removes the mean of the scores of one forward, because a shift common to all
options does not change the choice.

## Use

```console
python -I -B scripts/arbiter-context-slot-diagnosis/slot_table.py --panel <compact.json> --raw <raw-panel.json> --preflight <reference-preflight.json> --compiler <compiler.json> --output <slot-table.json>
python -I -B scripts/arbiter-context-slot-diagnosis/build_variants.py --panel <compact.json> --tokenizer <tokenizer.json> --compiler <compiler.json> --ordinal 10 --every 5 --output <fresh-dir>
python -I -B scripts/arbiter-context-slot-diagnosis/score_variants.py --model <model.onnx> --weights <weights.bin> --variants <variants.json> --reference <raw-panel.json> --output <fresh-dir>
python -I -B scripts/arbiter-context-slot-diagnosis/report.py --scores <scores.json> --table <slot-table.json> --output <report.json>
```

A forward of about 1,400 tokens took about 5 seconds on 8 threads. The full
set is 17 forwards per row.

## Limits

- The scores come from a portable graph on the CPU provider, not from the
  calibrated CUDA identity. The identity check bounds the difference on the
  unpermuted inputs only.
- A permuted input is not an input the product would compile for the same
  request, except that a `cards` or `reads` variant is what a roster or a
  read queue in another order would give.
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
