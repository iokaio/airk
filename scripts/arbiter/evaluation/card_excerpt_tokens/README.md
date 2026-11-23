# Card excerpt token cost

Measures how many tokens a Context tokenizer needs for evidence excerpts as
the Answer loop's Coverage and Follow-up card views show them. It is the
evidence for the excerpt allowance rule in
`crates/airk-core/src/native_loop.rs` (`evidence_allowance`): one card call
may show five bytes of excerpts for every four tokens it reserves above a
2,048-token frame, which is 7,680 bytes at the 8,192-token reservation.

- **Purpose:** set the bytes-per-token ratio and the frame from measured
  text. The kernel has no tokenizer, so it bounds excerpts in bytes. The
  native lane still refuses a call whose compiled input passes its token
  reservation, before any model work.
- **Inputs:**
  - a profile's `tokenizer.json` (`--tokenizer`);
  - one or more sources. `--source NAME=DIRECTORY` reads a directory tree as
    files. `--git-source NAME=REPOSITORY@COMMIT` reads a commit through git
    objects only (`ls-tree` and `cat-file`), so the checkout is not touched;
  - `--window` (default 960 bytes, one span's share of a full call) and
    `--tail N` (also print the paths of the N densest windows a source).

  Every text file is cut into windows of that size. Each window is counted
  as the view quotes it: a JSON string inside the state document, quoted
  again by the Qwen single-choice recipe. Binary files, files that are not
  UTF-8 and empty files are skipped and counted.
- **Outputs:** on stdout only:
  - the tokenizer's SHA-256 and the rule under test;
  - the tokens per byte of eight synthetic texts;
  - the frame: the tokens of a whole two-sub-need Coverage call and a whole
    Follow-up call without excerpt text;
  - for each source: files, bytes and windows; the mean, median, p95, p99,
    p99.9 and maximum tokens per byte; the file kinds with the densest text;
    and the densest run of eight consecutive windows of one file;
  - for one, five-fourths and three-halves bytes a token: the density a full
    call can hold, the windows and single-file calls above it, and the
    tokens of a call made only of the densest ordinary window.

  The script reads its sources and writes nothing. It prints numbers and,
  with `--tail`, file paths. It never prints source text.
- **Exit code:** 0 when the frame fits 2,048 tokens and, under the rule in
  the script, at most one window in a thousand of each source needs more
  tokens a byte than a full call can hold (`MAX_OVER_SHARE`). A call made
  only of such text is refused before model work and decided by the
  deterministic order.
- **Use:**
  `python -I scripts/arbiter/evaluation/card_excerpt_tokens/measure.py --tokenizer <tokenizer.json> --git-source munarium=<checkout>@<commit> --source bookbuilder-v2=<export> --tail 45`
  It needs the Python `tokenizers` package.

The frame figures come from a model of the view written in the script (two
sub-needs, four located spans and four cards each). Its prompt, question and
rules texts are copied by hand from `compiler/qwen.rs`, `loop_coverage.rs`,
`loop_follow_up.rs` and `loop_cards.rs`, and nothing checks that they still
match. It is an estimate: it does not run the Rust compiler. The kernel
arbiter contract lists this as an open point.

## Measured on 22 November 2026

Tokenizer `aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4`
(the Compact and Large Qwen3 tokenizer of the FP16 gate profiles), on the two
repositories of the Answer gate, in 960-byte windows:

| Source | Text files | Bytes | Windows | Mean | Median | p95 | p99 | p99.9 | Maximum |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| munarium at commit `9948345d` | 1,020 | 30,225,327 | 31,884 | 0.377 | 0.374 | 0.604 | 0.669 | 0.753 | 0.863 |
| bookbuilder-v2 export `238b22a722ce` | 8,091 | 88,307,599 | 94,879 | 0.332 | 0.308 | 0.522 | 0.585 | 0.746 | 2.000 |

The figures are tokens per byte.

- **The tail, by kind of file:**
  - munarium: the 45 densest windows are JSON (27 windows in 22 files, 0.748
    to 0.863) and Python (18 windows in 9 files, 0.744 to 0.807, generated
    code). By kind, the highest p99 values are Python 0.740, JSON 0.688, SVG
    0.684, JSON Lines 0.673 and lock files 0.647.
  - bookbuilder-v2: the 45 densest windows are JSON (33 windows in 4 files,
    0.866 to 2.000, most of them runs of escaped characters) and CSV (12
    windows in 2 files of number tables, 0.875 to 1.192). By kind, the
    highest p99 values are JavaScript 0.786, plain text 0.756, JSON 0.605
    and Vue 0.582.
- **Eight consecutive windows of one file** (a call that shows one file):
  munarium p99 0.645 and maximum 0.739; bookbuilder-v2 p99 0.560 and
  maximum 2.000.
- **The frame** of a whole call without excerpt text is 1,269 tokens
  (Coverage) and 1,321 tokens (Follow-up) of the 2,048 the rule keeps.

What each candidate ratio means for a full call at the 8,192-token
reservation, with the larger frame:

| Bytes a token | Excerpt bytes | Fits up to | munarium windows above | bookbuilder windows above | Call of munarium's densest window |
| --- | --- | --- | --- | --- | --- |
| 1 | 6,144 | 1.118 tokens a byte | 0 | 34 (0.036%) | 6,622 tokens, 19.2% to spare |
| 5/4 (the rule) | 7,680 | 0.895 | 0 | 43 (0.045%) | 7,948 tokens, 3.0% to spare |
| 3/2 | 9,216 | 0.746 | 41 (0.129%) | 95 (0.100%) | 9,273 tokens, does not fit |

- **Why five bytes for four tokens:** it is the largest of the three at
  which a call made only of munarium's densest window still fits. At three
  bytes for two tokens that call does not fit, and the share of windows a
  full call cannot hold passes one in a thousand in both sources.
- **What one byte a token would buy:** nine fewer refused windows in
  bookbuilder-v2 (34 against 43). The tail there is number tables and runs
  of escaped characters at 1.2 to 2.0 tokens a byte, which no ratio near
  ordinary text can hold.
- **Windows a full call cannot hold, at the rule:** no munarium window, and
  43 of 94,879 bookbuilder-v2 windows (0.045%). Of 58,118 runs of eight
  consecutive bookbuilder-v2 windows of one file, 27 pass the limit
  (0.046%).
- **What this does not measure:** the share of real calls that are
  refused. A call is refused when the average of its excerpts passes the
  limit, which needs at least one such window in it. If every such window
  sat in a different call, the share of refused calls would be up to eight
  times the share of windows; a dense window beside ordinary text still
  fits, which lowers it. The windows are also fixed-offset slices of every
  file, not the spans retrieval returns. The rate in use is read from the
  gate's receipts (card advice with zero observed tokens).
- **What a refusal costs:** the lane refuses the call before any model work
  and releases its reservation. The decision takes the deterministic order
  on the whole spans, the receipt records zero observed tokens, the card
  slot stays used and the model goes on to its next call.

The synthetic texts show where a byte bound ends. Runs of digits, hex digits
or backslashes need 1.0 token a byte. Runs of nothing but quotes or line
breaks need 2.0, and control characters 6.0, because quoting expands them.

These figures describe token counts only. They say nothing about model
quality. They move a little when the measured files or the view's texts
change.
