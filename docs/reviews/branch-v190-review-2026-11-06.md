# 1.8.8 branch review and manual-test candidate

## Scope

This review starts at the `feat/finetuning` snapshot `8546b54` and its
main-branch merge base `0dbd2f0`. The captured branch diff contains **737 changed
files**. Eight parallel review areas cover CLI catalog authority, native runtime,
Store, Kernel/retrieval, Core/ingestion/MCP/process contracts, the extension,
tooling and documentation. Follow-up reviews close the larger funding, storage,
upgrade, dispatch, client-validation and rendering gaps. The primary agent
reviews the combined corrections and owns all build/test execution.

Review covers changed production paths and their callers, contracts and owning
tests. Static review does not establish unexecuted platform, crash, signing or
installed-editor behavior. Dated phase evidence retains its original identities;
failed historical runs are not relabeled as passing.

## Findings and corrections

| Finding | Correction |
| --- | --- |
| Unknown permanent-exclusion policy could become an unqualified directory walk. | Unknown bindings admit no traversal or events. Losing authority withdraws old native handles, and exact-epoch retries use the existing bounded coordinator. Healthy bindings replan independently. |
| Retirement evidence disappeared after cleanup or later setup at the same source. | Retired catalog indexes retain their exact inactive registration identity without continued cleanup polling. Older purged state still needs its complete fenced receipt and matching retained cleanup identity. |
| Refused or expired scope installation could leave a worker's retained lease unrecoverable. | Unsettleable installation replies enter the existing exact-worker quarantine/drain/reap path. Unknown ownership is never reset to absent. |
| Reading an Applied receipt could orphan provider cleanup, or an unrelated gate failure could hide the full receipt. | Receipt reconciliation retains exact deferred cleanup for the metadata owner. The actor preserves only complete validated Applied receipts while ordinary live responses remain fenced. |
| Funded captures could publish an open allocation before its lease existed. | A tracked closed allocation owns the debit first; the same locked, synced lease is carried through opening and handle construction. Interrupted construction and adoption retain recoverable custody without a new schema or orphan sweep. |
| A partial terminal-intent write could poison split finalization permanently. | Terminal intent uses the existing protected, exact-prefix scratch writer and atomic no-replace publication. Foreign or malformed final state remains preserved and refused. |
| Dangling native authority markers could select legacy embedding mode. | Only genuine metadata absence permits legacy mode. Symlinks, nonregular markers, corruption and observation failures remain refusals. |
| Recommendations still called implemented exclusions/splitting unavailable. | Canonical advice and copied instructions describe separate native reviews; the existing capability-qualified split-review action is reachable. |
| New Explorer writes used read-timeout/retry guidance after a lost acknowledgement. | Typed request policy distinguishes persistent writes, interactive reads and ordinary reads; uncertain writes retain no-replay guidance. |
| Current guides mixed legacy registration and older development checkpoints with current setup. | Headless setup documents native draft/apply/receipt methods; historical examples and qualification records are explicitly identified. |
| Three export/help punctuation bytes were invalid UTF-8. | Only those bytes were replaced; strict UTF-8 scanning then passed for all 601 changed production/documentation text files. |

These review corrections introduce no new dependency, database, storage schema,
cloud service or OCR change. The maintainer-selected working version remains **1.8.8**.

## Validation and manual acceptance

Current reviewed native source is `2fb291ea...` / **973 inputs**. The
[working-build record](../releases/release-1.8.8.md#6-november-2026---branch-review-validation-checkpoint)
and [ledger](../research/implementation-v190.md#6-november-2026---branch-review-validation-checkpoint)
retain its exact identity and the earlier failed runs.

The first complete Windows native run selected **2,681 tests**:
**2,674 passed / 7 failed**, with **51 ignored**. The first corrected owning
selection passed **6 of 7**. After correcting the quiet-watcher fixture's
interpretation of completed wait/recovery history, its original monitoring and
idle-disable scenario and timeouts remain intact. The final corrected selection
passed **all 7 tests**, in **25.616 s** of Nextest time (**35.518 s** owner elapsed).
Those selected passes do not turn the failed full invocation into a passing one.
Final formatting and strict workspace/all-target/all-feature Clippy passed in
**8.122 s**, using the warm Windows target and development incremental setting.

The selected Mac Neo batch passed **37 native tests** and **15 split Node tests**
on its original `b409ff3d...` / **973-input** source. Timings/results were retained
before successful `cargo clean`; the owned target was absent afterward. The final
source differs only in two test fixtures, with unchanged production code and
extension inputs. The Mac results retain their original identity; they are not a new
final-source full Mac, optimized/signing or installed-editor qualification.

| Final prepackage gate | Current recorded status |
| --- | --- |
| Unfiltered Windows native R2, including the four routine excluded campaigns | Passed: 2,681 tests; 51 ignored; all four restored campaigns passed |
| Serial workspace doctests | Command passed; eight crates, zero doctest examples |
| Lightweight-main check and workspace build | Both passed |
| Fresh optimized matched runtime pair and complete seal | Passed: 675 files, current native identity, 821.198 s; release incremental disabled |
| Actual native client/model and isolated installation checks | Passed: two projects with 16 files each, model inference, launcher repair, upgrade/rollback and resumable uninstall |
| Complete compiled extension and Python suites | Node: 830 passed, 4 skipped. Python: 471 passed, 9 skipped |
| Final repository/version, notices, dependency policy and whitespace checks | Passed; dependency policy retains three existing reviewed exceptions; notices cover 366 packages |

Use the checksum supplied with the manual-test Windows VSIX to identify the exact
candidate. Manual acceptance on another machine and release promotion/publication
remain separate. This checkpoint does not claim a package result before execution.
