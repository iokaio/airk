# Gather Context prospective transfer check

Five newly written requests covered eleven evidence needs on an independent Rust source fixture. The frozen bundled Arbiter returned complete cited source for **4 of 10 positive needs**, compared with **2 of 10** for deterministic selection. Widening the bundled passages with spare bytes preserved every original line and supplied no additional reference on these requests. The 80-judgment range challenger supplied **5 of 10**, with two gains, one loss, and misleading candidate links for the one unavailable need. The comparison measures retrieval quality and wire bytes. It contains no paid calling-agent sessions and does not establish agent cost or patch correctness.

## Matched results

| Policy | Needs with complete references / 10 | Correctly linked complete references / 10 | Total wire bytes | Total passage bytes | Local model judgments |
| --- | ---: | ---: | ---: | ---: | ---: |
| Deterministic, six files and 14k passage bytes | 2 | 0 | 94,249 | 69,445 | 0 |
| Bundled file scoring | 4 | 0 | 97,814 | 69,215 | 157 shared |
| Bundled file scoring with spare-only expansion, the finalist | 4 | 0 | 98,722 | 69,995 | Same 157 |
| Range scoring with spare-only expansion, secondary challenger | 5 | 4 | 100,148 | 69,966 | 378 additional |

Under each policy, the number with any partial overlap equaled the number with a complete reference. The primary comparison exposed task-shared evidence under the original caller request with no budget override. Its zero link count follows from the bundled policy explicitly saying that individual needs were not scored. For the withheld nightly build timing, it offered no per-need link and made no affirmative abstention finding. The challenger attached six candidate ranges to that need, including a published developer benchmark example. None established the latest nightly builder's timing. The tool emitted candidate evidence, not a numeric answer.

| Request | Deterministic | Bundled | Finalist | Range challenger |
| --- | ---: | ---: | ---: | ---: |
| F01, exact byte-record trimming implementation (1 positive need) | 0 | 0 | 0 | 1 |
| F02, header tutorial, runnable example and regression (3) | 1 | 1 | 1 | 1 |
| F03, unequal-width reader behavior and regression (2) | 0 | 1 | 1 | 0 |
| F04, writer-side follow-up on the same error (2) | 0 | 0 | 0 | 1 |
| F05, published benchmark and regression (2 positive, 1 unavailable) | 1 | 2 | 2 | 2 |

### Where the six finalist misses occurred

Reconstruction of all five candidate file/card inputs matched the original model input digests. The diagnosis used the frozen prompts, source and code, with no rescoring or selector changes.

| Stage | References missed | Evidence |
| --- | ---: | --- |
| File pool | 1 | F02's runnable example never entered its 31-file pool, even though the task names the example. |
| Range nomination | 2 | The tutorial file for F02 and the writer file for F04 were delivered, but none of their four nominated ranges reached the needed tutorial passage or writer regression test. |
| Passage packing | 3 | For the F01 trimming implementation, F03 reader regression, and F04 writer rule, the correct files were selected and an enclosing range was offered. The respective complete ranges were 3,266, 2,988, and 2,659 bytes. They did not fit the 14k passage allocation. Final headroom was 127, 141, and 415 bytes. |

The challenger recovered F01 and the writer rule in F04. It lost the reader rule in F03. Its correctly attributed passages covered F01, F02's regression, F04's writer rule and F05's regression. F05's published benchmark reached the caller elsewhere in the reply. The higher count came with 2.41 times as many actual judgments as the bundled scorer. These five cases leave the comparative winner uncertain.

## Design and verification

The private fixture contains 58 files and 454,415 bytes from the locally retained `csv` 1.4.0 crate, including source, examples, regression tests, documentation and upstream notices (MIT or Unlicense). Every included file matched the Cargo archive; its SHA-256 also matched the repository lockfile. The actor received only the frozen eligible tree and the five requests, without grader notes. The conditional evidence question concerned an actual local deployment measurement; the admitted corpus contains only upstream materials. F04 is a fixed prospective follow-up, not a generated second turn. The source is public and its pretraining exposure is unknown. The prompts and labels were frozen before candidate selection and scoring. The production-shaped actor exercised the normal proposal/renderer and a verified local single-Arbiter instance using offline fixture identities rather than an installed native publication.

The independent checker verified the actual serialized reply, source path, inclusive locations, original source digest, LF-normalized excerpt digest and text for each passage. It credited full unions of the predeclared reference spans. A mere citation pointer or other lines from a relevant file earned no source-text credit. An independent auditor reproduced the three primary counts without using the panel grader, then confirmed the secondary arm's five whole needs and six links on the absent need. Four whole needs were in correctly attributed enclosing ranges, but only one was already whole within the short excerpt the model had scored. The secondary independent audit digest is `d6df099151392136d76cf59470165b094945e5daace652079a7db73622aa5326`.

Private artifacts are under `replay/prospective-transfer-a-r1`. The design digest is `d8f98ab911bbcc4b8f10926973947e02e692a7f8bea5b2baa6ef6129dcfbd2a8`, the primary actor digest is `e85a52e22eaa7730ca9c84d8e06f05cb590b2b838f60afdef5283c28cad03de8`, and the independent primary audit digest is `b2ea96c1629192a0b96dc9a7c8be90f5c75b8b10b3f6e21b659472189d6a766b`. The secondary actor digest is `496085e8b65f8e6a022e42e29eab8d4c86e7f6405dd969b3483ff1c0c98e529f`. It used the unmodified predeclared range selector (`d9e442c0fdbde89683eb880e8d5f0974caaed10c620679c4f32fec9bedbded24`) and the same candidate source ranges, underlying scorer instance, passage ceiling, file ceiling and full MCP wire renderer.

### Reproduction

Run from `scripts/arbiter-answer-eval` in the identity-checked environment. Supply the frozen panel directory and manifest digest. The acting runner never opens the private grader file. The range challenger verifies each original bundled input digest before calling the scorer. Outputs require fresh paths.

```text
python -B prospective_transfer.py PANEL --freeze-sha256 SHA --check-only
python -B prospective_transfer.py PANEL --freeze-sha256 SHA --server QUALIFIED_STATE --policy-lock POLICY_LOCK --output ACTOR
python -B prospective_transfer_grade.py PANEL ACTOR GRADES --freeze-sha256 SHA
python -B prospective_transfer_range.py PANEL ACTOR QUALIFIED_STATE SECONDARY --freeze-sha256 SHA --baseline-sha256 ACTOR_SHA
```
