# Card excerpt token cost

Measures how many tokens a Context tokenizer needs for evidence excerpts as
the Answer loop's Coverage card view shows them. It is the evidence for the
excerpt allowance rule in `crates/airk-core/src/native_loop.rs`
(`evidence_allowance`): one Coverage card call may show
`reserved tokens - 2,048` bytes of excerpts. The rule makes two assumptions:
an excerpt never needs more than one token a byte, and the rest of the model
view fits 2,048 tokens.

- **Purpose:** check both assumptions against a real tokenizer and real
  repository text. The kernel has no tokenizer, so it bounds excerpts in
  bytes; the native lane still refuses a call whose compiled input passes its
  token reservation.
- **Inputs:** a profile's `tokenizer.json` (`--tokenizer`), and the
  repository files listed in `measure.py` (`--repo`, the repository root).
  Each file is read in 768-byte chunks (`--bytes`), at most 60 a file
  (`--chunks`). Each chunk is quoted as the model view quotes it: a JSON
  string inside the state document, quoted again by the Qwen single-choice
  recipe.
- **Outputs:** on stdout:
  - the tokenizer's SHA-256;
  - the mean and worst tokens per byte for each kind of file;
  - the ratio of eight synthetic texts;
  - the worst repository chunk;
  - the tokens of one whole Coverage call without excerpts (the frame), and
    with 6,144 bytes of excerpts of each kind of file.

  Exit 0 only when no repository chunk needs more than one token a byte, the
  frame fits 2,048 tokens and every whole call fits 8,192. Nothing is
  written.
- **Use:** `python -I scripts/arbiter/evaluation/card_excerpt_tokens/measure.py --tokenizer <tokenizer.json> --repo .`
  It needs the Python `tokenizers` package.

The whole-call figures come from a model of the view written in the script
(two sub-needs, four located spans and four cards each). It follows the
texts of `compiler/qwen.rs`, `loop_coverage.rs` and `loop_cards.rs`, and it
is an estimate: it does not run the Rust compiler.

## Measured on 22 November 2026

Tokenizer `aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4`
(the Compact and Large Qwen3 tokenizer of the FP16 gate profiles), on the
tree of the commit that added this folder:

| Text | Mean tokens per byte | Worst chunk | Whole call, of 8,192 tokens |
| --- | --- | --- | --- |
| Rust | 0.273 | 0.367 | 2,736 |
| Markdown | 0.240 | 0.370 | 2,729 |
| JSON | 0.356 | 0.630 | 3,662 |
| TypeScript | 0.238 | 0.322 | 2,525 |
| Python | 0.361 | 0.676 | 2,046 |
| TOML, lock file included | 0.591 | 0.665 | 4,549 |

- The worst repository chunk needs 0.676 tokens a byte.
- The frame of a whole call, without excerpts, is 1,062 tokens of the 2,048
  the rule keeps for it.
- A whole call at the full allowance uses between a quarter and a little
  over half of its 8,192-token reservation.

The synthetic texts show where the first assumption ends. Runs of digits, hex
digits or backslashes need 1.0 token a byte. Runs of nothing but quotes or
line breaks need 2.0, and control characters 6.0, because quoting expands
them. A span made only of such text can therefore pass the reservation. The
lane then refuses that call before any model work, the reservation is
released, and the decision takes the deterministic order with the refusal
recorded.

These figures describe token counts only. They say nothing about model
quality. The figures move a little when the measured files change.
