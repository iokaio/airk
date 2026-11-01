# Fixed intent classifier training record

Record date: 2026-10-30. Source version: 1.7.0 development branch. No publication or human review approval is asserted.

The initial embedded smoke model used 12,480 examples expanded from original fictional templates. Its exact corpus is retained at the hash-named path in `manifest-smoke.json`; generated examples are not committed. The full protocol generated 40,000 training examples, 4,000 validation examples, 4,000 calibration examples and 7,800 repository-held-out examples. All expanded sets are saved under ignored `target/parser-train/data/` with seed, split, provenance and content hash. Earlier corpora, models, extraction manifests and reports remain saved; subsequent runs do not replace hash-named corpora.

Validation compares 10k, 20k and 40k training sizes and selects the highest accuracy, with smaller size breaking ties. Calibration alone selects acceptance thresholds; repository and independent fixture evaluations occur after those choices freeze. `evaluation.json` contains 150 separately authored utterances; labels were authored by the implementation agent and remain open to human review. `safety.json` contains unsupported/ambiguous requests. Neither is expanded into training data. Numerical gates and failures are recorded in `qualification.json`. These fixtures were measured in the initial run, so later measurements are regressions on retained fixtures, not a claim of fresh unseen evaluation.

## Measured learning curves

The first campaign plateaued at 76.2%, 76.35% and 76.4% validation accuracy for 10k, 20k and 40k rows. Repository intent accuracy was 88.6–90.6%; the 150-utterance fixture scored 78.7%, and the raw classifier accepted 13 of 20 unsupported requests. All four original qualification gates failed; see `qualification-initial.json`. No model was promoted.

Validation confusion identified missing action families for file contents, path enumeration and include declarations. Additional original phrasing from those gaps and the tool contracts, without increasing the 40k pool or examining individual final-fixture errors, raised validation to **95.2%, 95.55% and 95.225%**. Validation therefore selected the 20k model. Its repository intent accuracies were 96.15%, 97.46% and 97.15%, with every repository role above 90%. The separate 150-utterance result was **137/150 (91.33%)**, below the 92% floor. These results do not establish successful unsupported-input rejection or correct automatic plans. Additional slot duplicates above 40k are not supported by this learning curve; diverse phrasing, reviewed labels and rejection behavior need work first.

The initial calibration search forced at least 90% coverage and exposed unacceptable accepted-intent precision. The current protocol instead requires at least 99.5% calibration accepted-intent precision, then maximizes coverage; it records an excessive ask rate as a failed gate. The versioned report retains the actual thresholds, precision, abstention and raw unsupported acceptance. No threshold policy enables runtime execution on its own.

Before the broader campaign, protocol 4 selected threshold −180 and margin 412. Calibration accepted-intent precision was **99.60%**, with **24.95% abstention**, exceeding the 10% target. The separate 150-utterance fixture still had two wrong accepted classifications, and the raw scorer accepted **1/20 unsupported requests**. These are classifier measurements, not executions by AIRK. The candidate remains unqualified and saved locally; the embedded smoke asset is unchanged. Both 40k corpus revisions, all separate splits and the smoke corpus remain saved: eight hash-named JSONL files contain 108,280 expanded rows in total, including historical revisions rather than 108,280 distinct current-training examples.

Content-addressed copies of generated examples, vocabularies, candidate models, reports and source-hash sample manifests also live under ignored `.parser-train/`, outside Cargo's build directory. `retention.py` verifies existing copies and refuses mismatches; it excludes repository clones, source sample bodies and native index state. These saved examples survive `cargo clean`. Both local locations remain excluded from Git and release packaging.

## Broader phrasing and public query data

Two subsequent campaigns use 40,000 supported requests plus 8,000 unsupported requests: 6,000 original software tasks outside finite retrieval and 2,000 public conversational requests. Each has separate 5,000-row validation and calibration splits (4k supported/1k unsupported), plus 1,000 public unsupported test rows. Phrase-family variants stay in the same split. Exact normalized train/validation/calibration/test duplicates are rejected. CodeSearchNet topics are grouped by normalized query; CLINC keeps official source splits, with cross-split duplicates and ambiguous assistant self-description categories excluded.

The original phrasing covers questions, fragments, requests, file navigation, metadata, imports, requirements, changes and readiness. A second campaign adds request-verb/object variants from read-tool contracts and code-navigation terminology, plus multi-operation, external-web, reasoning, mutation and underspecified negatives. It does not train on the retained 150-utterance or 20-request safety fixtures. The evaluator verifies zero normalized fitted-text overlap with either fixture and with all three retained repository evaluation sets. These reused fixtures remain regression measurements rather than fresh blind tests; their labels still require human review.

Public sources are pinned in `online.lock.json`:

| Source | Revision | Use and attribution |
|---|---|---|
| [CodeSearchNet query resource](https://github.com/github/CodeSearchNet/blob/106e827405c968597da938f6b373d30183918869/resources/queries.csv) | `106e827405c968597da938f6b373d30183918869` | 99 published search queries, grouped by topic and wrapped in original explicit retrieval requests. Husain, Wu, Gazit, Allamanis and Brockschmidt (2019), GitHub; MIT repository documentation/resource notice. The upstream code corpus, which has separate per-source licenses, is not downloaded. |
| [CLINC150](https://github.com/clinc/oos-eval/tree/828f8093932c8fe6ca7936c3d2e52903b1c523de) | `828f8093932c8fe6ca7936c3d2e52903b1c523de` | Selected non-repository assistant tasks relabeled as unsupported by AIRK. Larson et al. (2019), *An Evaluation Dataset for Intent Classification and Out-of-Scope Prediction*; [CC BY 3.0 Unported](https://creativecommons.org/licenses/by/3.0/). These are AIRK transformations, not upstream AIRK intent labels. |

The [CoSQA authors](https://github.com/Jun-jie-Huang/CoCLR) distinguish MIT implementation code from a Computational Use of Data Agreement for their dataset. It was researched but not incorporated. [GitHub code navigation](https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github) and [VS Code navigation documentation](https://code.visualstudio.com/docs/editing/editingevolved) informed terminology; their prose was not imported as training utterances. Raw public inputs, original READMEs and complete licenses are stored in ignored local directories and durable copies. Attribution and transformation notices also appear in the product's third-party notices.

| Measurement | Prior protocol 4 | Broader /1 | Broader /2 review model |
|---|---:|---:|---:|
| Retained 150 intent accuracy | 137/150 (91.33%) | 146/150 (97.33%) | 146/150 (97.33%) |
| Wrong accepted classifications on those 150 | 2 | 0 | 0 |
| Calibration intent accuracy | 87.975% | 93.275% | 98.8% |
| Calibration accepted precision | 99.60% | 99.56% | 99.85% |
| Supported calibration abstention | 24.95% | 27.075% | 14.7% |
| Raw curated unsupported acceptance | 1/20 | 1/20 | 1/20 |
| Public unsupported test acceptance | Unmeasured | 3/1,000 | 4/1,000 |

The /2 model was selected using development validation, before final measurements: 40k supported/48k total fit rows, logistic regularization C=2, 99.65% supported validation accuracy and 0.6% unsupported acceptance at fixed development thresholds. Calibration alone selected deployment score threshold −344 and margin 690. Repository intent accuracies were 95.00%, 96.15% and 96.62%, with all repository roles above 90%. Reports remain in `qualification-broader.json` and `qualification-broader-v2.json`; hash-named reports, corpora and every candidate binary remain retained locally. Aggregate comparisons reflect the combined data changes, not an ablation proving an individual dataset's contribution.

At the October 30 checkpoint, the measured /2 binary was installed **for explicitly confirmed local-review proposals only**, with model revision `intent-broader/2`, asset SHA-256 `b18106b34110313fb9d1cd24c9a22d1ad300ffab7b4def80a381cbc0ac0a7c5d` and unchanged 852,116-byte feature-v1 format. Its exact input/report identities remain in `qualification-broader-v2.json`; the earlier smoke identity is preserved in `manifest-smoke.json`. Learned requests never automatically execute. The kernel requires a reviewed explicit plan submission, refuses missing selections, and tests all 20 unsupported regression requests for zero automatic evidence execution. At that checkpoint the raw model rejection and ask-rate gates still failed. Native regression tests also enforce the 150-utterance floor and exact Python/Rust integer inference goldens.

## Conversational follow-up — November 1, 2026, source candidate 1.8.2

The initial /3 campaign added separately split conversational typed/plain/quoted symbol and topic requests, including type-before and type-after wording, plus paired unsupported reasoning, mutation and compound tasks. Its actual 48k training set contains 744 supported `Find the` requests and 476 supported `Tell me about` requests, with 47 and 417 paired unsupported requests respectively. The 62-row conversational fixture is a known agent-authored regression, not a blind test; its labels remain open to human review. Exact fixture text is excluded from fitting without using fixture labels for model or threshold selection. Models are selected only on the separate development validation set, and calibration preserves the existing 99.5% accepted-precision policy.

The /3 experiment improved its known supported conversational intent accuracy from 20/48 to 45/48, but the retained 150-request fixture regressed from 146/150 to 143/150 with one wrong accepted classification. Curated unsupported acceptance rose from 1/20 to 2/20. The review installer refused the candidate. Its frozen model, hash-named corpus/report and `qualification-broader-v3.json` remain saved rather than being relabeled as successful.

Review identified two structural issues: feature-v1 lowercases the input without preserving identifier shape, and generic untyped `Tell me about X` requests cannot reliably be labeled as symbol-only based on a name. Protocol /4 preserves the rejected /3 families and fixture, adds feature specification 2 with five identifier-shape flags, and corrects generic untyped labels to text retrieval; explicit types, declaration requests and quoted identifiers use definition lookup. Its deterministic runtime companion can combine symbol and text retrieval for ambiguous untyped names. The old feature specification and shared fixtures remain unchanged; new Python/Rust parity fixtures cover the added flags. The generator also prevents contract verb/object combinations from recreating a held-out phrase verbatim.

The /4 campaign compared the same six 10k/20k/40k × C=0.5/2 development candidates before freezing a 40k supported/48k total C=0.5 model. Its training set contains **853 supported `Find the` requests** (638 definition, 215 search) and **631 supported `Tell me about` requests** (489 definition, 142 search), with 47 and 417 unsupported counterparts. Separate validation and calibration contain both prefixes under their own whole phrase families; exact counts are recorded in `corpus_coverage` in `qualification-broader-v4.json`.

| Measurement | Prior /2 review model | /4 review model |
|---|---:|---:|
| Retained 150 intent accuracy | 146/150 (97.33%) | 142/150 (94.67%) |
| Wrong accepted classifications on those 150 | 0 | 0 |
| Known policy-corrected conversational supported intents | 23/48 (47.92%) | 47/48 (97.92%) |
| Known conversational supported abstention | 100% | 52.08% |
| Calibration accepted precision | 99.85% | 99.84% |
| Supported calibration abstention | 14.7% | 21.8% |
| Raw curated unsupported acceptance | 1/20 | 0/20 |
| Raw public unsupported acceptance | 4/1,000 | 5/1,000 |
| Raw known conversational unsupported acceptance | 0/14 | 1/14 |

The comparison uses the same policy-corrected conversational fixture for both models; its generic untyped labels differ from the earlier /3 experiment and the two accuracy totals must not be mixed. The /4 repository accuracies and all role floors pass; the exact values and failed ask-rate gate remain in its qualification record. This is a measurable conversational coverage improvement with accuracy/abstention tradeoffs, not full model qualification. No additional fitting or threshold changes followed the final /4 regression measurement.

The /4 review asset is `intent-broader/4`, feature specification **2**, SHA-256 `10780b4d1c04f408e2a9ed996f0f853ad4eb55aa9c166e471ba78bee0845dab0`, with the unchanged 852,116-byte container and thirteen public intent labels. Calibration selected score threshold 40 and margin 240 using its frozen quantized model and the same precision policy, without fixture-specific threshold changes. Exact /2 asset, manifest and inference records are retained locally by digest; original reports remain committed. The installer passes its unchanged 92% retained-accuracy floor and zero-wrong-accepted requirement, preserves provenance and installs **confirmation-only proposals**. `qualified` remains false because calibration abstention exceeds 10%. Runtime deterministic grammar handles the supported simple requests directly; a raw learned label remains insufficient to execute evidence steps.

## Role expansion /5 — November 1, 2026, source candidate 1.8.2

The owner requested materially broader developer, writer, architect and QA queries, followed by exact public training data and a fork workflow. Four agents authored **476 original cases**: developer 127, writer 112, architect 120 and QA/tester 117. The public [role inventory](role-queries.json) retains literal targets, expected tools, family labels, notes and agent-authored provenance with human label review pending. Nine exact template duplicates across roles are retained and unioned; no contradictory exact labels were found. These are authored retrieval/abstention cases, not copied upstream utterances or generated answers.

The [joint split authority](role-phrase-splits-v5.json) covers 2,124 baseline/conversational/role variants in 793 authored families and 753 union groups. Canonical equivalent templates, kind-placeholder expansions and polite/role wrappers share one group. Hash-based 70/15/15 assignment and recorded coverage moves precede fitting; every role retains at least six whole groups in validation and calibration and every available role/intent remains represented in training. The role inventory itself has 323 training cases, 71 validation cases and 82 calibration cases. Known retained regression strings are excluded from fitting. New role-template families are distinct from the previous /4 campaign; their metrics cannot be compared with /4's original easier validation split.

The full pool contains 80,000 supported, 12,000 original unsupported and 4,000 CLINC unsupported rows. Separate validation and calibration sets each contain 8,000 supported/2,000 unsupported rows; the public unsupported test contains 1,000 rows. Models used 20k/40k/80k supported rows with proportional negatives at C=0.5/2. Development validation alone selected **40,000 supported +6,000 original unsupported +2,000 CLINC rows =48,000 actually fitted rows**, C=2. The exact selected row order is materialized separately; the 96,000-row pool is not reported as the fitted size.

| Current /5 input | Rows | Raw SHA-256 |
|---|---:|---|
| [Exact fitted selection](data/broader-fitted-v5-696b34a3fb904217.jsonl.gz) | 48,000 | `696b34a3fb904217bdd658eef3021758c058167b6efe026f5b253077a886150d` |
| [Full candidate pool](data/broader-train-ed896726fbc99279.jsonl.gz) | 96,000 | `ed896726fbc992791c0a80c2ba78ee4cd936aa55beda1af857acda3dffaf5e72` |
| [Validation](data/broader-validation-67d4880c1b359075.jsonl.gz) | 10,000 | `67d4880c1b359075afbe807f90f6a8e1b96e04de9665dfcc227f56350cfe28f9` |
| [Calibration](data/broader-calibration-975a1b224f8843fa.jsonl.gz) | 10,000 | `975a1b224f8843faa68f6a34341d52b6dd1a4bf5dd6daaf175d54a336f1ffd87` |
| [Public unsupported test](data/broader-public-test-378a1e3191ff42b5.jsonl.gz) | 1,000 | `378a1e3191ff42b55eb41c86173163f5435b959e34154216ef52d2c9362a79c0` |

The catalog preserves exact original `target/parser-train/data/` paths, gzip/raw hashes, sizes, rows, provenance and references for these and the historical campaigns. Restore reconstructs identical ignored raw files; gzip files are not added training examples. Validation, calibration and final tests remain outside the fitted file.

The actual fitted selection contains **27,704 role-inventory supported rows (69.26% of supported fit rows)** and 4,226 role-inventory unsupported rows. Its 31,930 role rows include every eligible authored training case (323 cases in 211 role families): developer 7,950, architect 8,016, writer 8,066 and tester 7,898. Prefix counts from the fitted file are 1,913 supported `Find the` requests plus 650 unsupported counterparts, and 386 supported `Tell me about` requests plus 131 unsupported counterparts. Role/slot substitutions expand those distinct authored phrases; the row totals do not imply 48,000 independent paraphrase families.

| Measurement | /4 on the indicated same data | /5 |
|---|---:|---:|
| New role-family validation intent accuracy | 83.50% | 97.95% |
| New role-family calibration intent accuracy | 79.79% | 92.23% |
| Retained 150 intent accuracy | 142/150 | 144/150 (96%) |
| Wrong accepted on retained 150 | 0 | 0 |
| Curated unsupported accepted | 0/20 | 0/20 |
| Public unsupported accepted | 5/1,000 | 0/1,000 |
| Known conversational supported intent accuracy | 47/48 | 40/48 |
| Known conversational supported abstention | 52.08% | 72.92% |
| Overall supported calibration abstention, respective campaigns | 21.8% | 52.125% |

Same-data role comparisons are computed from each exact role-family validation/calibration subset. The final row compares each campaign's own calibration set and is not a controlled same-data improvement comparison. Full per-role accepted/abstained/wrong-accepted counts and precision are in [qualification-broader-v5.json](qualification-broader-v5.json). For example, the new role calibration subset's writer accepted precision is 108/114 (94.74%) even though overall calibration accepted precision is 99.74%; role-specific uncertainty remains visible. The raw learned classifier is **unqualified** because the ask-rate gate fails. Deterministic runtime grammar and actual literal-target/plan tests are separate from model label accuracy. No general quality, performance, language-understanding or platform claim follows from these numbers.

All six fits completed before a post-fit reporting error: role coverage tried to access a query ID on CLINC rows, which intentionally lack one. The selected quantized asset and all exact datasets had already been saved immutably. Recovery preserved the chosen model, thresholds and six captured primary validation selection metrics, recomputed only the selected model's validation/calibration observations, and materialized the exact fitted slice. Finer unselected per-role diagnostics were lost and are explicitly unavailable. The reporting function now counts public negatives separately, with a regression test. No final fixture informed a new fit, threshold or selection after this recovery.

The installed **intent-broader/5** review asset uses feature specification 2, SHA-256 `3c6fee9de4093ca568c6c17725bcf50a5b9f75081c84672c7d783414dee07e4b`, threshold 448 and margin 476. The unchanged review gate accepted 144/150 with zero wrong accepted requests and passing repository/role/public gates. Automatic learned execution remains disabled; `qualified` remains false. Prior /2, rejected /3 and /4 identities/results are retained.

Exact selected-data replay using `fit-saved.py` reproduced the installed model **byte for byte** from the 48,000-row fitted file with frozen C=2, liblinear/max_iter=300, seed 84183, feature specification 2, quantization and calibration values. The environment was Python 3.13.7, NumPy 2.3.2, SciPy 1.16.1 and scikit-learn 1.7.1; the latter three match `requirements.txt`. The [public replay receipt](replay-v5.json) binds the exact fit/model/report identities. It verifies those bytes and this reproduction, not unseen request correctness.

After freezing, reporting ergonomics were corrected to derive role row counts and source-role counts for contributor inventories rather than assuming 476 forever. Current corpus/model bytes and per-row generation provenance remain unchanged. The qualification record retains freeze-time source identities; the public data catalog records the final code snapshot separately. The [fork guide](../../docs/parser-training.md) provides verify/restore, exact replay, extra-original-JSONL experiments and full regeneration commands. Custom experiments never automatically replace the product or qualify learned execution.

## Native vocabulary acquisition

The following public repositories are pinned, with root license contents checked and hashed in `repositories.lock.json`. Training and repository evaluation use disjoint repositories. Scopes are sparse checkouts listed in that lock. Native AIRK extracts a deterministic bounded sample of at most 64 files and 2 MiB per repository, with per-file limits and exact source-hash manifests retained under ignored `target/parser-train/corpora/`. This is vocabulary diversity sampling, not whole-repository coverage or language qualification. The initial full Backstage-docs index exceeded its ten-minute bound; that failed attempt is retained locally and is not claimed as successful coverage.

| Repository | Split | Revision | License files at that revision |
|---|---|---|---|
| backstage/backstage | train | `f60f04cc4e31534b18fbfed2e7e64b4e90580de9` | [LICENSE (Apache-2.0)](https://github.com/backstage/backstage/blob/f60f04cc4e31534b18fbfed2e7e64b4e90580de9/LICENSE) |
| nats-io/nats-server | train | `4e28758e1abbf2968c9438f311b37c13d3c403fa` | [LICENSE (Apache-2.0)](https://github.com/nats-io/nats-server/blob/4e28758e1abbf2968c9438f311b37c13d3c403fa/LICENSE) |
| nats-io/nats-architecture-and-design | train | `dc0b1c7200f0d716fdee9de1428759594538ed7d` | [LICENSE (Apache-2.0)](https://github.com/nats-io/nats-architecture-and-design/blob/dc0b1c7200f0d716fdee9de1428759594538ed7d/LICENSE) |
| dotnet/runtime | train | `9857961ed10086d04cf981039d12799520577055` | [LICENSE.TXT (MIT)](https://github.com/dotnet/runtime/blob/9857961ed10086d04cf981039d12799520577055/LICENSE.TXT) |
| actions/runner | evaluation | `110142c2c501c6ae9373d553efa826bedbaab5f7` | [LICENSE (MIT)](https://github.com/actions/runner/blob/110142c2c501c6ae9373d553efa826bedbaab5f7/LICENSE) |
| simpleiot/simpleiot | evaluation | `a4c5538bc1b3db1aad49a80911e0e7dd5061b254` | [LICENSE (Apache-2.0)](https://github.com/simpleiot/simpleiot/blob/a4c5538bc1b3db1aad49a80911e0e7dd5061b254/LICENSE) |
| tokio-rs/tokio | train | `6470b5e4b8e943ad014383a5bf6a93ba415678a3` | [LICENSE (MIT)](https://github.com/tokio-rs/tokio/blob/6470b5e4b8e943ad014383a5bf6a93ba415678a3/LICENSE) |
| rust-lang/rust-analyzer | train | `c59cdaa4c26ffe036a3403e4948f1681995d4125` | [LICENSE-APACHE (Apache-2.0)](https://github.com/rust-lang/rust-analyzer/blob/c59cdaa4c26ffe036a3403e4948f1681995d4125/LICENSE-APACHE), [LICENSE-MIT (MIT)](https://github.com/rust-lang/rust-analyzer/blob/c59cdaa4c26ffe036a3403e4948f1681995d4125/LICENSE-MIT) |
| fastapi/fastapi | evaluation | `79cab5bed28b24d92c185d236feb33f5783dfeb3` | [LICENSE (MIT)](https://github.com/fastapi/fastapi/blob/79cab5bed28b24d92c185d236feb33f5783dfeb3/LICENSE) |
| etcd-io/etcd | train | `1573d59416efa220421c451f5266b24a5d8374ef` | [LICENSE (Apache-2.0)](https://github.com/etcd-io/etcd/blob/1573d59416efa220421c451f5266b24a5d8374ef/LICENSE) |
| kubernetes/enhancements | train | `a904b50283fe82d0d4f46988d0fd9d1a35a86c25` | [LICENSE (Apache-2.0)](https://github.com/kubernetes/enhancements/blob/a904b50283fe82d0d4f46988d0fd9d1a35a86c25/LICENSE) |
| open-telemetry/opentelemetry-specification | train | `36ccfcfb33aca1e9c70a2cfd26e031ed9180949a` | [LICENSE (Apache-2.0)](https://github.com/open-telemetry/opentelemetry-specification/blob/36ccfcfb33aca1e9c70a2cfd26e031ed9180949a/LICENSE) |
| protocolbuffers/protobuf | train | `478ccb8c1874495997adefe6aca81f15546867c4` | [LICENSE (BSD-3-Clause)](https://github.com/protocolbuffers/protobuf/blob/478ccb8c1874495997adefe6aca81f15546867c4/LICENSE) |

License labels describe the inspected root files, not an assertion that every file or optional dependency in a repository has the same license. Original files remain in local ignored checkouts/samples; no upstream implementation is incorporated into AIRK runtime code. Root license identities/hashes and source pins remain reviewable. The original template/trainer code retains AIRK Apache-2.0 headers; development Python packages retain their own licenses and are not embedded in the runtime.

## Runtime gate

The current runtime asset remains suggestion-only until the required accuracy, role, independent-utterance and unsupported-input gates pass. Do not relabel training accuracy as held-out evidence, use test results to choose thresholds, or infer full plan correctness from intent classification alone. Runtime slot/scope/publication validation and source read-only behavior have separate tests. Failed metrics must stay visible; corpus growth is justified by validation learning curves, not a fixed row-count claim.
