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

The measured /2 binary is installed **for explicitly confirmed local-review proposals only**, with model revision `intent-broader/2`, asset SHA-256 `b18106b34110313fb9d1cd24c9a22d1ad300ffab7b4def80a381cbc0ac0a7c5d` and unchanged 852,116-byte feature-v1 format. `manifest.json` records this scope and the exact input/report identities; the earlier smoke identity is preserved in `manifest-smoke.json`. Learned requests never automatically execute. The kernel requires a reviewed explicit plan submission, refuses missing selections, and tests all 20 unsupported regression requests for zero automatic evidence execution. The raw model rejection and ask-rate gates still fail, so this is not a claim of full qualification or generalization to arbitrary unsupported requests. Native regression tests also enforce the 150-utterance floor and exact Python/Rust integer inference goldens.

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
