# Adaptive passage-budget comparison

The experiment replays fixed Arbiter scores on the 20 development tasks. It tests whether a caller gets more reference evidence by allowing a larger reply or by prioritizing unmet needs. No arm performs additional model inference.

## What changes between arms

| Arm | Selection | Passage-byte ceiling |
| --- | --- | ---: |
| Fixed 14k | Original order and six-file spread | 14,000 |
| Fixed 20k | Same order and spread | 20,000 |
| Need-first 20k | Give an unrepresented need first opportunity, retaining original order within the priority group | 20,000 |
| Adaptive | Same priority. Admit a first passage for an unrepresented need if at least one nonblank line is new. For other passages, require at least half of the nonblank lines to be new. | 20,000 |

Each arm uses the same 40 scored files, per-need ranking, first 16 files nominated for context assembly, candidate source ranges and six-file return limit. Only the enclosing range of a directly scored excerpt carries its need attribution. Additional source ranges can contain relevant evidence, and the grader counts it wherever the text arrived. A line is new if both its file/line location and its normalized text are new within this reply. Every original candidate range is either delivered intact or recorded with a byte-limit, file-limit or novelty reason. The shared six-file and byte caps can leave some needs without directly attributed evidence.

The source-text comparison uses the actual UTF-8 bytes of all delivered passages. A separate check serializes the full prototype MCP stdio response, including the JSON-RPC request ID and framing, the encoded tool content, task, need text/status and citation locations, the source revision, omission count, and the actual passage text. It tests whole-frame limits of 8,192, 14,000 and 20,000 bytes on each arm. It preserves code points, full lines, and verified source ranges; passages that do not fit are omitted whole. A mandatory reply that cannot fit yields a bounded budget error or an explicit local refusal when even the framed error is too large. The offline source revision identifies a Git snapshot. Live native publication identities and any additional response metadata must participate in the same final serialization.

## Reproduction

Run from `scripts/arbiter-answer-eval` using the checked human-owned environment and fresh private output paths.

```text
python -B adaptive_passage_budget.py CAMPAIGN PAIRED_RECEIPT REFERENCE ALLOCATIONS --caller-passage-limit 20000
python -B passage_reply_budget.py CAMPAIGN PAIRED_RECEIPT ALLOCATIONS REFERENCE WIRE --caps 8192 14000 20000
python -B passage_budget_report.py CAMPAIGN ALLOCATIONS WIRE REFERENCE REPORT
python -B -m unittest test_adaptive_passage_budget test_passage_reply_budget
```

The source snapshot, file identities, candidate ranks, caller inputs and reference file digests are checked first. The fixed 14k result must reproduce the recorded direct reply's passage sequence, ownership, hashes and byte count for each task. The independent grader resolves every delivered location and digest against the source and compares complete intervals with the frozen reference and its allowed alternatives. A whole reference is a strict delivery measure. A five-line overlap may contain a useful clue but does not establish that a compound question was answered. Report broad and narrow results separately.

## Results on the frozen 20-task panel

**Keep fixed 14k as the default for the next comparison.** Fixed 20k returned more whole references when only passage text counted. The gain required 29.7% more source bytes and disappeared when the complete MCP frame was limited to 20,000 bytes. Need-first ordering alone made no difference to whole-reference or need totals. The adaptive rule removed one useful OpenAPI reference despite having room for its passage.

There were 239 located positive notes, including 113 essential notes. Of 98 caller needs, 86 had a positive located reference. These counts measure delivery on known development cases. No caller model was involved in this budget replay.

| Passage allocator | UTF-8 passage bytes, total | Returned ranges / 940 | Whole positive / 239 | Additional five-line partials | Whole essential / 113 | Needs with a whole reference / 86 | Needs with five-line or whole reference |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Fixed 14k | 249,868 | 240 | 28 | 17 | 16 | 19 | 33 |
| Fixed 20k | 324,184 | 284 | 30 | 18 | 18 | 21 | 35 |
| Need-first 20k | 324,310 | 288 | 30 | 19 | 18 | 21 | 36 |
| Adaptive | 306,743 | 272 | 29 | 18 | 18 | 20 | 35 |

The four allocators delivered correctly attributed whole references for six needs each. A reference appearing elsewhere in the returned text still counts for the whole-reference delivery metric.

| Allocator | Six broad tasks: bytes / whole notes / needs with any whole note | Fourteen narrow tasks: bytes / whole notes / needs with any whole note |
| --- | --- | --- |
| Fixed 14k | 78,545 / 6 / 4 | 171,323 / 22 / 15 |
| Fixed 20k | 107,610 / 6 / 5 | 216,574 / 24 / 16 |
| Need-first 20k | 107,497 / 6 / 5 | 216,813 / 24 / 16 |
| Adaptive | 97,391 / 6 / 5 | 209,352 / 23 / 15 |

### Why evidence changed

Only four tasks changed their whole reference set across the four passage allocators. On ten other tasks, fixed 20k spent more bytes than fixed 14k and returned exactly the same whole references. The remaining six had the same byte count and whole references.

| Task | What changed versus fixed 14k |
| --- | --- |
| Bookbuilder T1 | All 20k arms gained the existing DOCX test fixture and cleanup pattern (N6), giving need 3 a whole reference. Adaptive delivered it in 11,592 passage bytes for this task. |
| Munarium C2 | The wider arms admitted an earlier 108-line README range with the documented TTL default (N3, including line 262), gaining need 1. Later complete ranges for the datastore pin lifetime (N9) and the actual database-backed test fixture (N10) no longer fit. Need 4 lost its whole reference. The greedy scan can choose different files and ranges as soon as one larger early range fits. |
| Munarium N-C2 | All 20k arms gained the Python async facade showing sync and async share the request spec (N9). The need already had another reference. |
| Munarium N-A1 | Fixed and need-first 20k gained the committed OpenAPI window description (N12), giving need 4 a complete reference. Adaptive discarded its 3,775-byte range while only 9,692 bytes were in use. All 120 nonblank source locations were new; only 14 normalized text lines were new. Repeated JSON punctuation and schema keys masked the route-specific evidence. |

Fixed 14k skipped 527 ranges for file width and 173 for bytes. Fixed 20k skipped 531 and 125. Need-first 20k skipped 529 and 123. Adaptive skipped 529 for width, 110 for bytes and 29 for low textual novelty. Adaptive saved 5.4% of bytes against need-first 20k, at the cost of the OpenAPI reference. Increasing the byte cap does little when the candidate or six-file width is the binding constraint.

### Complete serialized replies

The following entries are **needs with any whole reference / whole positive notes** after parsing and grading the text that survived the complete MCP frame cap. The same caller ID, need wording, source revision and framing were used for every arm. The test uses a Git revision citation and accurately counts this renderer's metadata; a real native publication receipt can consume a different number of bytes.

| Allocator | 8,192-byte full frame | 14,000-byte full frame | 20,000-byte full frame |
| --- | ---: | ---: | ---: |
| Fixed 14k | 15 / 18 | 17 / 23 | 19 / 28 |
| Fixed 20k | 14 / 18 | 16 / 23 | 18 / 26 |
| Need-first 20k | 14 / 18 | 16 / 23 | 18 / 26 |
| Adaptive | 14 / 17 | 16 / 23 | 19 / 28 |

At the 20,000-byte frame cap, the fixed 14k arm retained every selected passage. Fixed 20k dropped 43 ranges during serialization, need-first dropped 48, and adaptive dropped 33. Adaptive tied fixed 14k on aggregate whole notes and covered needs while losing Munarium C2's two notes (including its test fixture) and gaining the Bookbuilder DOCX fixture and Munarium Python async facade. At 8,192 and 14,000 bytes adaptive lost the need backed by the Munarium C2 test fixture. A larger passage ceiling alone is therefore insufficient to justify a product default.

Private receipts under the campaign's `replay/adaptive-passage-budget-r1` record every allocation, omitted range and reason, delivered source digest, whole-frame size and digest, and per-task comparison. The frozen direct receipt SHA-256 is `d3121a27a1f5981ddb185119829e04884b7512f3b9bbe3b5725aa8f935628180`. Reference mapping v2 is `d3b787bb0c01a1914008a64fad7bd0ed119ec38f51fb3b5e0b47249452d6a699`. All 20 fixed 14k controls matched the direct reply's recorded passages. The allocator receipt is `3713acd862997978b5126a2f7c6e91e1b6c4db97f8bdbb4a1c753580114a61e5`, whole-wire receipt `0cccc7567f8e1a28c44666b3e726c53f8230f1a1c94d45867e8b32f07d8ab6ed`, and the report `a2a9cd20652f9f842f75399d233adc557cbf3ef5dbf87e0e47821bf796c407d3`.

## Follow-up: preserve the complete fixed-14k response before adding evidence

The follow-up rebuilt the fixed-14k and fixed-20k wire controls at each cap. All 120 wire hashes matched (20 tasks × three caps × two controls). The corrected selector kept every fixed-14k passage in order, with the same source bytes and original need linkage. It then considered the same original scored nominations in stable order. An addition needed an unrepresented need, a new nonblank source location, space within the six-file limit, and enough room for the whole reserialized reply. Identical text at a different source location remained eligible. The rule never widens a range or discards already-delivered evidence.

| Full wire cap | Allocator | Wire bytes across 20 tasks | Passage bytes | Whole notes / 239 | Covered needs / 86 | Needs with five-line or whole evidence |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8,192 | Fixed 14k | 155,065 | 100,446 | 18 | 15 | 25 |
| 8,192 | Fixed 20k | 153,034 | 97,884 | 18 | 14 | 25 |
| 8,192 | Protected additions | 155,681 | 100,864 | 18 | 15 | 25 |
| 14,000 | Fixed 14k | 249,543 | 178,783 | 23 | 17 | 30 |
| 14,000 | Fixed 20k | 251,755 | 179,441 | 23 | 16 | 30 |
| 14,000 | Protected additions | 257,874 | 186,584 | 23 | 17 | 30 |
| 20,000 | Fixed 14k | 330,408 | 249,868 | 28 | 19 | 33 |
| 20,000 | Fixed 20k | 334,098 | 253,239 | 26 | 18 | 32 |
| 20,000 | Protected additions | 331,440 | 250,720 | 28 | 19 | 33 |

**No task gained or lost a whole reference or covered need against the fixed-14k control.** At 8,192 bytes only Munarium N-C4 gained an extra range, a 10-line .NET model excerpt. At 14,000 bytes that excerpt and a 73-line design-document passage on Munarium N-C3 fit. At 20,000 bytes the only addition was a 19-line server operations passage on Munarium N-A2. These extra nominations consumed 616, 8,331 and 1,032 wire bytes respectively without a measured gain. Directly attributed whole-reference coverage was unchanged at 5, 5 and 6 needs respectively. No broad task changed.

Compared with fixed 20k at the same wire cap, the protected arm preserved Munarium C2's test fixture (need 4) at all three caps. At 8,192 and 14,000 it omitted the Python async facade on Munarium N-C2 that fixed 20k supplied; another reference already covered that need. At 20,000 it also preserved the Munarium C2 datastore horizon note. These are differences already present in the original fixed-14k control. Protecting baseline output solved the displacement problem, but the additional nominations supplied no independently verified reason to change that baseline.

The selector was frozen before execution at SHA-256 `e86b4e5d0a67eeb729c94e8498a0b7c1f1af0002b978c33054196c8157b25cab`; the private receipt `protected.json` is `0d5d36f5be6c76d2c77ec071be9b1350c918cf8c9c8cafc9d99b5cdce8c450df`. Reproduce from the evaluation script directory with `python -B protected_passage_budget.py CAMPAIGN PAIRED_RECEIPT WIRE REFERENCE OUTPUT`. The two focused correction tests cover repeated text at a distinct location and a nominally fitting excerpt whose citation metadata would exceed the caller's frame cap.

## Postselection sensitivity for the bundled finalist

The selected finalist scores 40 files, admits up to 14,000 passage bytes, and uses spare bytes to extend the returned ranges by up to 16 lines on each side. This diagnostic authenticated its full framed response for each of the 20 development tasks. It kept all selected ranges and examined the original skipped candidates in their existing order. A complete range could be added if it supplied at least one previously unseen source line, fit the six-file limit and kept both the 20,000-byte passage ceiling and the caller's complete frame cap. It required zero additional model judgments.

| Caller frame cap | Needs with a whole reference / 86, before → after | Whole positive notes / 239 | Whole essential notes | Mean full reply bytes, before → after |
| --- | ---: | ---: | ---: | ---: |
| Default, 131,072 | 35 → 38 | 40 → 47 | 26 → 30 | 20,339 → 24,926 |
| 8,192 | 21 → 21 | 21 → 21 | 15 → 15 | 8,186 → 8,186 |
| 14,000 | 27 → 27 | 30 → 30 | 19 → 19 | 13,996 → 13,996 |
| 20,000 | 33 → 33 | 38 → 38 | 25 → 25 | 19,557 → 19,772 |

At the default frame cap, 52 additional ranges fit across 13 tasks. They added **72,013 passage bytes and 91,733 complete reply bytes across the panel**, or 3,601 and 4,587 bytes per task on average. The mean frame grew 22.6%. Three needs gained their first whole mapped reference:

- Munarium N-A1, need 1, asks where the reports API parses and validates the time-window parameter.
- Munarium N-W1, need 1, asks for the clients' tokens-and-reports guide that mentions seven reports.
- Munarium N-W1, need 2, asks for the evidence and matrix report handlers, including routes, access, query parameters and response shape.

Each gain means at least one mapped reference range was delivered in full. All three gains occurred on narrow tasks. All 80 paired responses retained their original passages and whole reference coverage. The 20,000-byte frame admitted six extra ranges on four tasks without gaining a whole mapped reference.

**This postselection observation has no independent validation.** It used only the development panel. The recorded finalist and transfer comparison retain the 14,000-byte passage budget. The reproduction command is `python -B bundled_postselection_headroom.py CAMPAIGN FRESH_OUTPUT` from the evaluation script directory. The private receipt `replay/bundled-postselection-headroom-r1/outcomes.json` has SHA-256 `5ea83f6bbeef9d31c03a00b0b0c1fa9b61c1d28e3f4622882d44914b67166acc`.
