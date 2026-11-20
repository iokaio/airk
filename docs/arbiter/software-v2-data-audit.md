# Software-v2 data and evaluation audit

Observed 15 November 2026 (America/Denver), during execution of the
[final plan](final-plan.md), Steps 7–8. This is an input audit, not a model
quality or qualification result. No new model inference, gradient step,
CAL/FINAL scoring or publication was performed by this audit.

The additive [software-v2 protocol](../../tools/retrieval-train/software-v2-freeze.json)
preregisters the approved 8192-token/120-second shared policy, measured p95
limit, unchanged quality floors, split/label acceptance, terminal refusal rules
and M1/F1/F2/C1 drafts. It explicitly remains non-executable until actual source,
rights, native-visible examples, checkpoint identities, sample/power rationale
and measured compute caps are bound. The original A0, encoder, baseline-selection
and resource protocols remain byte-identical under their historical identities.

## Available evidence

These are metadata counts from retained local inputs. They do not grant a new
use, establish current native alignment or transfer earlier model results.
The private input inventory retains complete paths and SHA-256 identities at
`artifacts/advanced-split/models/weights-v2/software-v2-inventory-r1/inventory.json`.
It contains no questions, answers, training examples or source text.

| Input | Observed scope | Acceptance limitation |
| --- | --- | --- |
| Retained human corpus | QASPER: 107 TRAIN, 14 DEV, 29 CAL, 44 TEST; Hotpot: 512 TEST; 65 QASPER source-rights decisions | Research-paper evidence and evaluation-only Hotpot do not supply the planned broad software corpus. Preserve all human alternatives and unresolved alignment. |
| Requests source/docstring pairs | 218 TRAIN-only tasks; one repository; exact Apache-2.0 source revision and retained LICENSE/NOTICE | Containing-function/docstring association is mechanical supervision, not independent human relevance, whole-card or sufficiency judgment. No independent software DEV/CAL/FINAL population. |
| Historical encoder TRAIN audit | 315 positive rows, 287 questions: 69 QASPER questions from 36 papers and 218 Requests tasks | 58 cropped positive rows, 147 cropped distractors and 82 distractors whose mined lexical terms disappeared. The old corpus does not satisfy the new software target. Paper families are not repository families. |
| Selected Context CAL reference certifier | Original-reference complement for actual canonical Read choices; no-gain observations remain UNKNOWN | Its producer explicitly sets human semantic gold count to zero and excludes terminal, gap and source-role gold. Generic Read evidence does not qualify Sufficient, Clarify or NoJustifiedRead. |
| Retained fresh Hotpot metadata audit | Before rights/alignment: 746 DEV, 731 sealed CAL, 2396 sealed FINAL; 3532 already exposed | Counts alone establish no admitted cohort. Hotpot remains evaluation-only; reserved text is not a training source. |
| Retained Context structural census | Request/card/source counts and byte sizes with earlier compiler refusals | JSON byte size is not native prompt-token count. Earlier remaining budgets and profile widths do not qualify 8192-token execution. |

The inspected sources establish no accepted 2000-query-group corpus spanning
20 unrelated software repository families, no stratified human review for that
corpus, and no class-specific terminal human adjudication. This is a scoped
finding about the retained corpus, audit and selected-driver metadata, not a
claim that no suitable public benchmark exists.

## Missing acceptance packet

Before any M1/F2/C1 gradient probe, the data owner must provide:

1. A licensed software-source inventory with exact repository/fork revisions,
   underlying file rights and retained notices; mutually assigned 40% code/tests,
   35% docs/contracts/configuration, 15% cross-file diagnostics and 10% general
   replay, with the planned language coverage and a concentration audit.
2. TRAIN/DEV/CAL/policy-confirmation/FINAL repository, issue/symbol-family and
   chronology closures, deduplication evidence and an untouched FINAL reservation.
   Prior failed, exposed or partly cancelled FINAL remains consumed.
3. Exact source spans, original question provenance and alternative coherent
   support sets, mapped to Rust-emitted visible examples. Refusals stay in the
   original denominator; invisible positives and unjudged negatives are refused.
4. Suitable existing human judgments for stratified TRAIN review, critical
   final evidence, teacher conflicts and each proposed terminal class. Record
   annotation authority and uncertainty. Agent review and native projection
   cannot fill a missing human label.
5. A before-score repository-cluster sample/power rationale and frozen rubric,
   followed by executable cards binding data/checkpoint/policy hashes, seed,
   optimizer, controls, maximum steps and measured compute/settlement caps.

Terminals additionally need independent whole-card class evidence and the exact
native support/calibration proofs. Sufficient requires zero observed false
accepted decisions and a one-sided 95% false-acceptance upper bound below 1%,
with a defensible repository-cluster analysis. The roughly 299 independent
zero-error accepted-case guide does not impose 299 repositories or establish
independence by itself. Unsupported winning classes must abstain without
substituting a lower-scoring Read.

## Work that can proceed independently

Current native input/padding/token/menu census, acquisition/lifecycle checks,
resource probes on already authorized inputs, generic-Read qualification and
ordinary product/platform checks can proceed with their own admitted inputs.
They do not require pretending that the software training data has passed.

The native tokenizer census seam is
`arbiter::compiler::qwen::tests::context_prompt_census_old_versus_new_against_profile_cap_and_remaining_budget`
in `airk-retrieval`. Its ignored test consumes `AIRK_QWEN_TOKENIZER` and
`AIRK_CONTEXT_CENSUS_REQUESTS`, optionally writing a fresh
`AIRK_CONTEXT_CENSUS_OUTPUT`. It measures old/new prompt counts against 2048,
8192 and the original request's actual remaining shared budget. Use complete
retained native request bytes and pin their hashes; do not rebuild requests,
prompts or tokens in Python. A new census is required on current source before
claiming fit. The sole Cargo owner selects and records the actual command.

The first execution of that historical census stopped at row 14 because its
shrinkage assertion failed: the native verbose view counted 602 tokens and the
compact view 649; both fit 2048. The original failed receipt and assertion are
preserved. The separate ignored diagnostic
`context_census::complete_context_prompt_census_records_growth_and_refusals`
retains every original native request and records growth, shrinkage and refusal
without assuming that compact always means fewer tokens. It uses the native
Qwen compiler-profile emitter and public compiler, changing only a private
diagnostic metadata width to 8192. Original request budgets remain unchanged;
no graph executes and no profile/resource fit is established. Counts exist only
for successful compiles; structural, prompt or compiler refusals keep null
counts and full raw detail, explicitly reducing comparable measurement coverage
rather than guessing over-cap tokens or cropping input. The run requires exact
`AIRK_CONTEXT_CENSUS_REQUESTS_SHA256` and `AIRK_QWEN_TOKENIZER_SHA256` pins in
addition to the same input/tokenizer/output paths. The complete report retains
input identities, diagnostic/compiler policies, every original row and refusal,
and population/comparison counts. This diagnostic source does not alter the
hash-bound production compiler recipe or weaken the historical test.

F1 is gradient-free. Existing QASPER DEV human evidence can support a separately
identified general-document diagnostic after native admission and lifecycle
checks. It cannot select a software instruction or qualify software gains.
Requests TRAIN must not be relabeled DEV, and the 24 agent-authored AIRK pilot
questions remain unadjudicated diagnostics with their existing use restrictions.
F1 changes native prompt/policy identity and needs its owning contract/fixtures;
source review or a copied string is not an executed prompt experiment.

Pending accepted adaptations, retain exact original MiniLM U8/S8 and selected
Qwen publisher weights. This audit reports no new scientific success or failed
experiment. Original optional models still require their own current-artifact
CAL, resources, held-out and composed evidence. The all-three qualified catalog
and native publication capture remain mandatory for final-series delivery.

## Validation

The metadata-only [audit tool](../../tools/retrieval-train/software_v2_audit.py)
verifies historical protocol hashes and refuses executable research cards with
pending data/native/probe bindings. Its optional research source metadata checks
repository/issue/clone and source-content separation, explicit FINAL reservation,
rights/notice use, exposure and existing-human-review attribution before any
training consumer reads text. This metadata format is not a native contract;
passing it never authorizes training or proves annotation adequacy. The
[independent synthetic controls](../../tools/retrieval-train/test_software_v2_audit.py)
exercise these refusals: 11 tests passed. Exact commands, source hashes and
timings are retained in the private inventory directory as `validation.json`.

The additive JSON parses; its original-protocol SHA-256 bindings match live
files. The audit's relative links resolve and new files have no trailing
whitespace. The private inventory's input hashes match the observed bytes.
These checks establish documentation and input identity only. No model quality,
training, runtime, transport, platform or release pass follows from them.


## Public existing-human-label source audit (16 November 2026)

Observed16November2026. This expands the earlier retained-local-input census. Public human judgments exist; source/rights/native admission and unsupported claims must be distinguished. No gradients, model scoring, new human annotation or FINAL question exposure were performed. All raw data stays private and outside distributable payloads.

### Actual source audit

| Source | Observed human/source scope | Admission boundary |
| --- | --- | --- |
| [CoSQA](https://aclanthology.org/2021.acl-long.442/) / [CoCLR](https://github.com/Jun-jie-Huang/CoCLR) | Authentic published TRAIN20,000 queries:9,707positive/10,293negative, code map6,267functions. Model-data owner joined1,955human TRAIN pairs (979positive/976negative),465functions and316original pinned files across38canonical repository families with retained permissive rights/notice evidence. Publisher annotations use at least three humans per pair. | These are actual source/rights candidates, not native-admitted examples. Python-only query/function labels do not establish exact complete-evidence or native card/terminal correctness. Keep CodeSearchNet overlap, repository/fork/issue/chronology splits and refusal denominators. |
| [CodeSearchNet](https://github.com/github/CodeSearchNet#annotations) | Actual public CSV:4,006human annotations,99queries,573query-language pairs,2,874codeURLs and2,258literal repository URLs across six languages. | Upstream designates human annotations evaluation-only, distinct from mechanical docstring TRAIN. Preserve that upstream role unless a separately recorded pre-score research assignment explicitly changes use. Literal URL counts do not prove independent fork families. |
| [ContextBench](https://github.com/EuniAI/ContextBench) / [paper2602.05892v1](https://arxiv.org/html/2602.05892v1) | Published human expert context evidence:1,136issues/66repositories/eight languages. Only publisher `data/train.parquet` acquired:450rows/58canonical identifiers,3,625span headers,285multifile tasks. Fixed first-case20-family audit retained62basecommit source files and all20root license closures. Seven whole cases/7families align all their human code spans;90of131selected spans align individually. | Native-admitted0. The published older TRAIN format uses context headers and explicit line counters, differing from the current full-release JSON schema. Keep originals and exact formatting transformations. Generated/outside-repository paths, changed code and stale ranges remain refused. Docs/config path flags63 and test flags84 overlap; they are not human task-stratum assignments. |
| [TechQA](https://aclanthology.org/2020.acl-main.117/) via [PrimeQA original archive](https://huggingface.co/datasets/PrimeQA/TechQA) and [NVIDIA mirror](https://huggingface.co/datasets/nvidia/TechQA-RAG-Eval) | Blind original-ID filtering isolated600TRAIN records (450answerable/150unanswerable) from the mirror's910records;310DEV questions/answers/text were never output or used. TRAIN388document names overlap40of148DEV filename-only references; shared files were excluded before extracting348TRAIN-only documents.396answerable cases match their retained corpus contexts;394have exactly one whole literal answer in matching source. | These394 are source-stage document candidates, native-admitted0. The mirror is an automated transformation of IBM human QA, not new human review. Original archive README confirms human answer spans and CDLA data rights; original600QA byte-for-byte comparison was not performed because the archive places a2.7GB corpus member before training files. Preserve exact mirror revision/IDs and this provenance limit; no unanswerable-to-terminal translation. |
| [QASPER](https://huggingface.co/datasets/allenai/qasper) | Human research-paper questions plus separately annotated answers/evidence. Existing retained licensed TRAIN can supply a general-document replay candidate population. | Existing rights, native visibility and exposed/reserved source closures remain authoritative. Paper families do not count as software repositories; paper answerability does not establish native terminal classes. |
| [CoNaLa](https://conala-corpus.github.io/) and [StaQC manual labels](https://github.com/LittleYUYU/StackOverflow-Question-Code-Dataset#22-manual-annotations) | Human curated code/intent or standalone-snippet labels exist separately from automatically mined bulk. CoNaLa has2,379manual TRAIN examples and500test examples. | Stack Overflow post/date attribution and underlying rights need joining; no software repository/commit coverage is implied. Do not relabel mined pairs as human-reviewed. No pickle execution or bulk corpus acquisition occurred in this audit. |
| [SWE-QA, Peng et al](https://github.com/peng-weihan/SWE-QA-Bench), [Agent Retrieval Bench](https://github.com/eyuansu62/agent-retrieval-bench), [CORE-Bench v3](https://arxiv.org/html/2606.11864v3) | SWE-QA current release has720manually validated repository QA pairs/15Python projects with commit roster. ARB provides427workflow-derived cases/25repositories; CORE reports180Kqueries/106Kbroader-context labels. | Do not infer independent human span adjudication from workflow/patch/model-derived labels. Exact release, annotation authority, source rights and SWE/CodeSearchNet overlap require audit. SWE-QA is distinct from the similarly named2026multiple-choice release. Only metadata was inspected here. |

### Rights decisions

CoSQA's data is governed by C-UDA; its tooling is MIT. The [actual Microsoft agreement1.0](https://github.com/microsoft/Computational-Use-of-Data-Agreement/blob/a28ca06f6f8ecac0b5856ca6179ac49e55f00104/C-UDA-1.0.md) permits computational use and does not restrict qualifying Results, explicitly including trained models containing no more than de minimis source data (§§1.1,2.2,5.5). Modified raw records remain Data; redistributed Data carries attribution and C-UDA obligations (§3). Underlying rights are not warranted (§4.1), hence the actual original CodeSearchNet source/license joins. There is no blanket computational-training prohibition or automatic conflict with AIRK's Apache2 code license. The ACL paper's CC-BY notice applies to the paper, not as replacement rights for data/source files.

The actual TechQA archive README, not merely a wrapper card, states that both QA and Technote corpus are [CDLA-Permissive-1.0](https://cdla.dev/permissive-1-0/). The exact original license PDF is retained. Sections2.1/2.2 grant use rights; §3.4 leaves qualifying Results unrestricted, with the de-minimis bound in §1.10. Publication of Data/Enhanced Data requires license and credit/change notices (§3.1). NVIDIA and PrimeQA Apache declarations do not erase the original CDLA terms. This audit redistributes neither source corpus nor training examples.

ContextBench's Apache2 wrapper does not replace underlying repository rights. Each audited basecommit retains its actual root license/notice. The seven complete source-ready cases use BSD-family, MIT or Apache2 sources. Other selected cases include Flipt's GPL3 license and remain distinct; no blanket all-Apache or all-admitted claim is made.

### Strict derived Context span proposal — not accepted automatically

The original annotation and declared range remain immutable. For a stale range only, the ENTIRE raw gold body must occur byte-for-byte exactly once in the SAME already pinned basecommit file, at line boundaries. Record original declared lines, raw body/source hashes and derived byte/line coordinates separately. No code trimming, fuzzy match, guessed range, patch, revision substitution, outside-repository file or unsupported formatter transformation is allowed. This is derived span projection, not additional human review or a changed label.

The strict probe checked41previously refused spans and found4eligible projections (threeFlipt,onecode-server). It creates no additional fully complete case: the other required spans in those cases still refuse. Original seven complete cases remain seven; native-admitted count remains0. Root must review/accept this mapping before any consumer uses it.

### Pilot feasibility and remaining gates

The original2000-group plan requires800code/tests,700docs/contracts/config,300cross-file and200general replay groups, mutually assigned, with at least20unrelated software repository families. The source audit now establishes substantial human source candidates and a concrete394-case document pool; it does not establish700software-doc groups,300fully aligned cross-file groups, or a complete admitted2000-group corpus.

A justified smaller pre-score1000-group pilot would preserve the same mix:400code/tests,350software-docs,150cross-file and100general replay. The1,955source-joined CoSQA pairs across38families and394TRAIN-only TechQA document candidates make its code/docs quantities plausible before native visibility refusals. It still requires enough valid cross-file cases, rights/split closure, independent DEV/CAL/confirmation/FINAL reservations, stratified TRAIN review appropriate to each exact visible claim, sample/power rationale, native-emitted examples and executable compute cards. No size change or gate waiver is made by this proposal.

No audited source supplies AIRK human whole-card Sufficient, Clarify or NoJustifiedRead labels on exact retained evidence/menu. Those classes remain unsupported. Missing dependent labels block only their training acceptance/promotion; resource, numerical, lifecycle, transport and platform work continue independently.


### Public TRAIN source audit and stronger TechQA fence (16 November 2026)

This follow-up records source availability, not native admission or training
acceptance. The expanded ContextBench audit filtered the current release by the
450 original TRAIN IDs before question/context materialization. All 450 match
prior IDs, exact question bytes and base commits; the other 686 rows remain
outside semantic output, mining, scoring and training. Of 1,599 pinned source
files, 1,592 were authenticated. Of 4,207 human spans, 3,560 match declared ranges
and 302 have a separately retained unique exact-entire-content projection in the
same pinned file. Original annotations are preserved. There are 280
source-complete cases, including 150 multifile cases; native-admitted count is
zero. Of those 150, 126 have retained permissive source terms and 24 have actual
GPL/AGPL terms. Local research disposition and any later model distribution need
separate review; neither a missing license classifier nor a wrapper license
settles the underlying source rights.

The original TechQA publisher archive is now retained privately with streaming
and reopened SHA256 equality: 2,959,973,525 bytes, SHA256
`6b094ef9a69718f727ce8d7e15c4d961e51032cefaa952e0d6af9d176d7ba118`.
All 600 mirrored TRAIN records compare exactly with original IDs, answers,
answerability and relevant document references. Their questions are exactly the
original QUESTION_TITLE, two line feeds and QUESTION_TEXT. All 396 original
answer offsets checked in the initial source-disjoint population select the
human answer exactly. Original QA and corpus terms are CDLA-Permissive 1.0;
the mirror's Apache declaration does not replace them. The mirror omits the
original DOC_IDS candidate pools in all 600 cases, so source pairs cannot be
called complete original menus or terminal labels.

A stronger original DEV DOC_IDS-only metadata fence covers 310 records and
12,355 held-out source IDs. It quarantines 134 of 388 TRAIN gold source IDs,
94 more than the earlier relevant-document-only fence, excluding 160 of 450
answerable TRAIN questions. This leaves 290 positive question/gold-source pairs
across 254 original documents. Only 15 positive original whole candidate menus
are disjoint. The prior 394-candidate snapshot and proposed 350-doc contribution
to a 1,000-group pilot are superseded; neither was native-admitted. A fresh
private fixture physically contains only the 254 allowed documents and 290
original question/human-offset pairs, with exact character-to-UTF8 span proof.
Source-ID disjointness does not establish content-clone, product-family,
chronology, native visibility or held-out model acceptance.

An intermediate mirror crosscheck broadly decoded already retained mixed
NVIDIA dictionaries while constructing reference metadata. No DEV question was
emitted, scored or trained on, but the no-materialization claim for that attempt
was invalid. Its receipt and scope caveat remain preserved. The corrected pass
materializes only original DEV DOC_IDS arrays and performs no broad mirror
decode. Original 600-record comparison remains separate from that correction.

An 800-group composition of 320 code/tests, 280 software-docs, 120 cross-file and
80 general replay is only a source-count planning possibility. No pilot size is
approved by this audit. Actual native visibility/refusal census, repository and
clone splits, rights, label scope, negative masks, cluster variance/power,
DEV/CAL/confirmation/FINAL reservations and compute limits govern any pre-score
decision. CoSQA labels cover only their judged pairs; other pairings stay
unknown. No unjudged negatives, whole-card or terminal labels are invented.

Private evidence: `public-human-label-admission-r1/` contains ContextBench full
450-case receipt SHA256
`363c5a8bebd4fe179c498d1c557e7be49b71b4388ae7c84bff630db7af8c6499`,
TechQA original comparison
`f348fad09baf0fbbce6438dc00afa25289d7dcd605c54a7c551047ab645032a9`,
corrected full DEV source fence
`a0c1eeaf4302a7390d38494dfcde62950daeb239e2306cafdca53d3938473a1c`
and source fixture
`3e9506f1e6c3b5223d1d77f9289c941402b5a41ab385cbc0cefdc9a02f770642`.
Raw examples and indexed source copies remain private and excluded from
packages. Independent runtime, model resource and product work continues.

### Initial Context FP16 confirmation-source inventory

On 19 November the user confirmed that the two original downloads are
the available judged sources and accepted auditing those first. The
current retained ContextBench archive includes all 1,136 rows, and the
original TechQA tar includes the publisher validation members alongside
TRAIN and DEV. The census projected identifying and structural fields
without exposing questions or expected answers for tuning.

ContextBench's blind 686-row complement contains 613 rows on 49 literal
TRAIN repository clusters and 59 unresolved repository label or URL
conflicts. Of those 59, 56 touch a plausible TRAIN identity. Fourteen
cases across eight literal repository clusters survive a provisional
metadata fence covering observed alias, same-commit and instance
signals. There are six singleton clusters, one cluster with two cases and
one with six. This census does not settle fork, textual clone, chronological
or prior third-source exposure, native-visible gold spans, rights or sample
power. The local archive has expert evidence spans and no candidate-menu
column. Any paired evaluation must freeze complete corpus-derived menus.

The pinned TechQA publisher validation includes 20 questions, each
matched to its exact ordered reference menu. All candidate documents
resolve in its retained corpus. Eleven rows are answerable with a
complete menu, gold document and numeric offsets. Nine are unanswerable.
All 994 distinct candidate documents appear in original DEV menus. Eight
also overlap original TRAIN gold documents. The publisher describes
this validation subset as a submission smoke test. Keep it for evaluation
plumbing and audit the original larger DEV partition for confirmation
eligibility. Retain the historical DEV mirror exposure caveat.

Exact aggregate receipts remain private and no case IDs, rows,
question text, evidence texts or candidate documents were exported.
The ContextBench inventory is
1b4b0dd4c402bf41a30002ea02ff0fb18a4ea771dc7df81f928338eb5370c4fe.
The TechQA validation inventory is
4b9dc72722ee4a33c1b882871d8963001abd2e073a2370454fdbba86cfd641aa.
Neither inventory admits a judged cohort or reports FP16 task quality.

### Original TechQA DEV confirmation candidates

The structural audit verified 310 publisher DEV questions. Of the 160
answerable rows, 158 have a nonempty duplicate-free ordered candidate
menu, an in-menu publisher gold document and ordered offsets. The valid
ordered-menu count across all rows is 308. Seventy-seven of the 158
answerable candidates have no menu document ID in the complete set of
388 original TRAIN gold IDs. All 12,355 distinct DEV menu document
IDs match the earlier frozen metadata fence. Of those, 134 overlap
original TRAIN gold. Use the 77 as the first provisional
technical-support candidates. Document ID separation still requires
clone, product-family, chronology and actual model exposure review.

The shared training/DEV document member is 291,957,707 bytes. The
structural run retained its header. The next bounded metadata scan
will determine menu document availability. Before paired scoring,
verify source versions, native-visible gold spans, independent held-out
reservation and paired sample size. The original DEV member pin is
df62a05619b07b386159bcbaaf3e0e50be93eaf983aea8a5f8ed1513a3c2ec6f.
The exact aggregate under techqa-original-dev-metadata-r1.json is
8d595e5c963bd838991282b66332b930fe169184de5cf9b6150c9e221ec98a75.
No question, answer or document text was decoded.

The subsequent exact-member corpus scan verified 28,482 archive
document-map entries. All 12,355 distinct DEV menu keys resolved
and matched their entry IDs, with zero missing or mismatched. This
covers all 308 structurally valid ordered menus, including all 158
answerable rows with publisher offsets and all 77 whose menus avoid
original TRAIN gold IDs. The scan kept the original archive and
300-second deadline and read the document body only as opaque bytes.

The exact shared corpus member SHA is
69261b5f4436d70379fd7172a8d97062c93f5d433c386d196937d198c66bf75a.
The aggregate techqa-original-dev-corpus-metadata-r1.json is
443510ac51f41f2f4b12c0a2b4efde505144443d11a79550811bd7521f42f35f.
This proves archived menu membership. The 77 cases require
source-family exposure and native-span checks and a prospective
paired sample-size assessment before admission.

### TechQA menu dependence and confirmation readiness

The exact publisher DEV and shared corpus replay retained the 77
provisional answerable cases. Each has a complete 50-document menu
that avoids original TRAIN gold document IDs. The cases have 76
distinct menus, 71 distinct exact gold documents and 3,527 unique menu
document IDs. In 15 cases the gold ID occurs in another case's menu.
Sixty-seven of 2,926 case pairs share a menu document. Connecting exact
shared document IDs transitively gives 33 components, with one of
37 cases, eight of two and 24 singletons. These components measure menu
dependence. Product families, forks and text-level clones require
their own checks.

Run the maintained audit with `--original-dev-dependencies`. The 37.819621 s
aggregate retained under techqa-original-dev-menu-dependencies-r1.json
is 83a98930e567e829d1e14cc2421564307b4cc37f3c53832b931597df7eab2c02.
The source is 1f511e1d6c0f0a1d254a77b802aedbce7a5ae8a3d134c9f7b91ed605f7c3874d.
The exact archive, DEV member, and shared corpus pins remain embedded.
The aggregate emits no case IDs, answers or document text. Eight focused
controls passed. The ContextBench metadata replay still gives 14
provisional cases across eight repository clusters.

For scale, assume both variants always succeed and the 91 provisional
cases are independent. In that simplified perfect-baseline case the
one-sided exact 95% lower bound on FP16 success is 96.76%. Even perfect
outcomes would need at least 149 independent cases to reach 98% in that
example. The actual acceptance analysis uses paired outcomes, their
discordances and corpus-dependence clusters. The example is a sensitivity
check rather than a prescribed sample size. Confirm eligible populations,
freeze the complete menus and native-visible gold spans, and size the
paired long-context strata before running confirmation.

The current private Q/K/V profile binds calibration
b5ef749ce5e3349513f380c5fd03d7e1839a1f9830a4a7172ec1db59e37c1be3. It has
zero active rules and all four action classes unsupported. The historical
FP32 Answer timing receipt adfc16d7c410ab5f87a507c29545966c76ad08be00686274227fdbf615c51ae6
covered one warm request per CPU and CUDA arm. Both ended at the budget
with three abstentions. It gives no paired precision or threshold result.
The candidate needs exact-artifact calibration and a prospectively frozen,
native-visible judged corpus to measure consequential decisions through
the complete Answer path.

### Supplemental 90 and 50 candidate roster

The approved screening targets are filled with 90 technical-support
candidates and 50 repository candidates. Keep these as separate
supplemental source strata alongside the original TechQA and
ContextBench inventories. This stage admitted zero cases.

| Source | Cases | Verified evidence and remaining work |
| --- | ---: | --- |
| [IBM MTRAG-UN Cloud](https://github.com/IBM/mt-rag-benchmark/blob/e0970de1b1207c2a9506adf238e6e1a22037e38a/mtragun-human/README.md) and [MTRAG Cloud](https://github.com/IBM/mt-rag-benchmark/blob/e0970de1b1207c2a9506adf238e6e1a22037e38a/mtrag-human/README.md) | 25 | Four newer and 21 original answerable, standalone, human-authored queries from different conversations with publisher positive passage judgments. Exact aliases were screened by task, encoded question and passage roster. The original release predates the pinned Qwen3 release. Check earlier exposure, document rights, full support sets and native-visible spans. |
| [AWS documentation QA](https://github.com/siagholami/aws-documentation/tree/2d06ee9011f3192b2ff38c09f04e01f1ea9e0191) | 65 | Original published QA mapped to exact documentation across 35 files in 12 product families. Fifty-three answer strings match source bytes literally. Twelve formatting-based matches require exact span review. The older public source needs exposure review. Publication answer and document labels still need complete acceptable evidence sets and native alignment. |
| [SWE-bench-Live MultiLang](https://huggingface.co/datasets/SWE-bench-Live/MultiLang/tree/eb09c2e5db6e4c5a95e01c903c8fd1d5da33c4f3) | 50 | The eight pinned shards supplied 1,077 metadata rows. Excluding 30 known repository, alias or commit overlaps and 280 older publisher dates left 767 rows over 297 literal repositories. The selection has 50 distinct repository names and base commits across all eight languages, with publisher dates April–July 2026. Current GitHub metadata resolved 49 distinct canonical IDs with no reported forks. One alias redirect remains unresolved. Verify actual issue-publication dates, forks and clones, file rights at the base commits, and blinded pre-fix source-span judgments. |

The private aggregate under context-fp16-supplemental-recruitment-r1.json
is 12c08960c1a879be0884e6a9fc036d1e43a4985b7b15934ec8a9254333169bcb.
It links the exact private source rosters, reports statuses and contains
no case text. The Cloud output
techqa-style-ibm-mtrag-cloud-screen-r1.json is
779c51e74f6e73c6ff6c59749c1978b47f1ee2894f0044007e2d184e785c75f0.
The AWS output aws-techqa-style-fallback-r1/aws-qa-candidates.json is
272de43ac4c79e941a053ae5e7f246d297e81f1c236f0aa00f9fe3194ba22132.
Both MultiLang receipts are under repository-confirmation-multilang-r1.
The roster is 28abd5497d6446adda3915b6f0f97a3374d40db28310762ae2de9c8891ae150e.
The upstream audit is d0b49948f596daca6d6cbad1d7fe66dff0a170cf8c6be456cf10372e42b3354c.

Review source rights and overlap against the actual model, development,
calibration and historical evaluation lineage first. Prepare private
native corpus roots, freeze complete menus and map exact support spans
before paired scoring. The native Answer fixture can capture the full
offered choices and verification reads. Its observer is bounded to
100 captures per fixture. The Rust complete Context prompt census then
measures the 8,192-token stratum against the original remaining budgets
and retains all refusals. Human reviewers adjudicate complete evidence
sets without precision or output identities. The 98% decision still
needs a powered, clustered paired comparison on admitted inputs.
[Native visibility and collector requirements](../../scripts/arbiter/analysis/public_source_admission/README.md#context-fp16-confirmation-source-censuses)

### Source validation results for the confirmation candidate pools

The original TechQA DEV join verified 76 of 77 publisher answers at their
exact Unicode scalar offsets in the original source. All 3,527 ordered-menu
document IDs resolved across 28,482 archive entries. One in-bounds span
differs from the published answer and is withheld. Preserve the original
offsets. The private techqa-original-dev-source-spans-r1.json receipt is
f8b022efff9fd0ffaa8a7c7f307f4aa4e6ab2a7aa4d0623beb6851d1232701e9.

The IBM audit joined all 60 passage pointers for the 25 Cloud questions
to 49 literal documents in the exact official passage archive, which has
72,442 rows. Exact identifiers, canonical URLs and hosts did not overlap
388 original TechQA TRAIN gold documents, 12,355 original DEV menu
documents, or 254 retained TRAIN source documents. Every selected query
was authored before the April 2025 Qwen3 publication. The original
release was already public. The four newer-release questions have
earlier authored dates and unresolved private exposure. Review source
notices, family aliases, clones and exposure before canonical admission.
The private techqa-style-ibm-mtrag-cloud-source-metadata-r1.json
receipt is 2bd2a972bf385d0732af3fa24a716b9f580e0ca8ec4887a2fcb47d8846493ee5.

The AWS projection reauthenticated all 35 source files and returned
41 unique original byte and line ranges. Twenty-nine are literal and
12 use the verified Markdown range projection. Twenty-four answers
occur more than once and have no selected span. One unique range crosses
fenced code and carries a separate rights flag. The root and 12 family
notice sets were pinned. Exact comparisons found no match in the inspected
1,514 retained file descriptors, 2,525 TRAIN queries or 394 TechQA TRAIN
candidate question digests. Full rights, semantic clones, calibration
and model exposure remain open. The private AWS source projection receipt
is 9a248fa25c34b0b0b5c4b9caf83a44b2127331971ac547f1885e0c7bc990bf9f.

The MultiLang pinned-revision screen resolved all 50 canonical repository
identities, including the earlier redirect. It resolved 69 of 72 historical
ContextBench claims and found zero known canonical or fork overlaps in that
scope. All 50 exact base commits were tied to their pull requests and
49 were ancestors of the observed default heads. GitHub issue chronology
showed 49 post-model, ordered issue-to-merge pairs. The pinned root-license
API matched 48 base commit blobs. Thirty-nine cases jointly meet the
post-model chronology, exact pull-base association, permissive root
classifier and observed overlap screen. Only 16 frozen source commits
were no later than their original issue. Later snapshots need an explicit
pre-fix evidence and chronology review. Per-file rights, the three
unresolved historical aliases and clone/later exposure remain open.
The pinned-upstream-screen.private.json receipt is
dcef091a79fc092e5422b39c62c47c197810a5263c19d1d4a66c08362f3987c4.

The exact CUDA Compact Q/K/V calibration still has zero rules and
all four action classes unsupported. The retained matching-reference
development raw errors remain 0.006991, 0.068153 and 0.046494 at
128, 1,024 and 8,192 tokens respectively, above the 0.001 parity
requirement. Source verification supplies inputs for adjudication.
Before the judged comparison, freeze native-visible evidence sets,
complete the native menus for the two precision arms, and verify
8,192-token coverage and the prospective paired analysis.

### Replacement screening and repeated AWS answers

Of the 39 original MultiLang cases that met the scoped screen, 12 had
a base commit date no later than the issue. All 12 had a GitHub User
issue author and an explicit issue closure link. The other 27 had later
base commits. Of those, 25 had a User actor and two lacked that actor
classification. A User actor alone does not establish human authorship.

The 24 repeated AWS answers account for 485 literal occurrences across
13 documents in eight documentation families. Five of those documents
also contain a uniquely located answer for another case. Keep those
shared documents together when assessing dependence across the roster.

The repeated-answer audit reauthenticated the 13 source blobs and the
publisher CSV. Every case has at least two word-bounded hits. Seven
answer locations across three cases contain all the remaining question
terms on the answer-bearing lines after removing stopwords and answer
terms. Only one case has a unique lead by that rule. No candidate
paragraph contains its question verbatim. One occurrence intersects
fenced code. The CSV supplies no offset that chooses among these
locations. Independent reviewers can use the private byte and line
ranges and paragraph digests to judge which passages supply sufficient
evidence. All 24 cases remain withheld. The private receipt
aws-repeated-answer-locations-r1.json is
4cf5c6484e736ca176333515c7a063e312222e832fae9d2b9b6949dd255feeba.

Two successive 16-case MultiLang batches followed the original
language-interleaved ranking after excluding the original roster's
repositories, repository leaf names and base commits. The first batch
yielded nine scoped passes and the second yielded 11. Across both,
seven of the 20 passing cases had bases dated no later than the issue.
All 20 had a GitHub User issue author and an explicit issue closure link.
Twelve cases were withheld. The overlapping reasons were a missing
permissive root-license match (10), issue/merge chronology (4), and
pull/base association (1).

Take the first 11 passing rows in receipt order for the 50-case review
roster alongside the original 39. That yields 15 base commits no later
than the issue and 35 later snapshots needing pre-fix review. Retain
the other nine passing rows as reserves. Three historical repository
aliases remain unresolved. Per-file rights, exposure, native visibility
and independent evidence judgments remain open for both batches.

The private receipts under repository-confirmation-multilang-r1 are
replacement-screen-16-r1.private.json
(96c939368ad6ac79cfc91a1f309f9887aa6ef22e1be82033f4a91a5774d3f8aa)
and replacement-screen-16-r2.private.json
(0b6fdcde3a79efddd4b64b8dc2a1989de9564f01635c163ff077b11d6d415e42).

### Active source cohort after the AWS exclusions

The user excluded all 24 ambiguous AWS selectors from the current comparison.
Keep their receipts for provenance. The 41 remaining selectors map to 27
unmodified Markdown documents totaling 162,383 bytes in nine guide families.
The private corpus preserves the five documents shared with excluded cases.
Its sibling manifest binds the 29 literal and 12 Markdown-projected spans,
including one with fenced code, and retains the source notices separately.
The source subset does not establish a complete retrieval menu.

The original TechQA DEV freeze retains all 77 ordered publisher menus and
3,527 document hashes. Among the 76 exact publisher matches, the 3,800 menu
slots resolve to 3,526 distinct documents. Each verified case retains its
publisher scalar offsets, UTF-8 byte range and answer digest. One mismatch
is explicitly withheld. These are original source coordinates. Neither
native token budgets nor the delivered evidence sets have been measured.

For the 35 later MultiLang bases, 23 are direct parents of the linked
merge. The other 12 reach the merge over two to 43 verified parent edges.
All 35 have a different root tree from the merge, and their base dates
fall after the issue and no later than the merge. A tree change somewhere
in a repository cannot establish the state of an individual evidence file.

| Private evidence | SHA-256 |
| --- | --- |
| aws-unique-source-corpus-r1/manifest.private.json | ef2844ba2406de3b57ac1f5a6b13e582320bacae27779b704d38d4786375e882 |
| techqa-original-dev-ordered-menu-freeze-r1.json | 50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7 |
| repository-confirmation-multilang-r1/premerge-topology-r1.private.json | 69dd8e32d8d2a6a034f1db870617eb52942abfbab19daf8095dc951f9144ad65 |
| repository-confirmation-multilang-r1/premerge-extended-history-r1.private.json | ccf73de8c7afe837baf9a486afe9e5687a16c028bc2a1b9ecd17ed0a05a47b4c |

### Product-family exposure and repository evidence labels

None of the 76 active original TechQA menus shares a canonical URL or
upstream document ID with the 388 original TRAIN gold documents. No menu
document hash matched the 1,514 retained file descriptors. All 76 menus
had the four publisher metadata fields used in the comparison. Seventy-five
menus share at least one literal product ID with TRAIN, including 56 gold
documents. Product-name overlap also occurs in 75 menus, including 64 gold
documents. Account for family exposure before reservation. The private
techqa-menu-train-exposure-r1.json digest is
2de707182f15be6e486a0e0fea004a52575e232a1db4caa4f3ad7d9ef992657c.

The eight pinned MultiLang shards contain 1,077 rows with a common 20-field
schema. It includes solution and test patches and no separate independently
annotated source file or span. Keep the 50 selected MultiLang cases in the
review pool until independently judged. ContextBench has publisher expert
annotations. The 14 eligible cases across eight repository clusters had
125 spans on 65 distinct repo/base/path tuples. The declared-line join
verified 64 blobs and matched 101 spans. Five spans differed, 18 exceeded
file bounds and one blob was unavailable. A separate exact whole-body,
line-boundary check located two more spans. Six cases matched all their
published spans. Eight remain partial.

The selection fence was sealed before projecting gold into Python objects.
The underlying parquet row group spans the full release. A publisher search
result incidentally displayed part of an Astropy case outside TRAIN. Its
repository was already excluded from the 14-case selection. It contributed
no evidence. No case acquired native or task-quality admission through
these archive and blob checks.

| Private file under repository-confirmation-multilang-r1 | SHA-256 |
| --- | --- |
| source-label-schema-r1.private.json | 7ae992b7385245c4871ecd7c7cfd38f3526e0f84754f0669ce80c3f81b41c512 |
| contextbench-14-selection-r1.private.json | 51adc46cedf7f9d8803a91e790dffae5c2cfbabcc2c9018bae9221ae67dc722b |
| contextbench-14-declared-lines-r1.private.json | a80f26b9026bf0ee3fdf3dc4f9c10523cb6bfa61b734dcd3d61f9b154636c66e |
| contextbench-14-derived-lines-r1.private.json | 84da2e363f508e2e1d138099b222f9a34a65c5cf17553b9c0f7786a52b5542b8 |

### Active source dependence before paired measurement

The active pool contains 142 technical-support and 64 repository candidates.
The 76 TechQA cases form 33 groups under shared menu documents, with 36
cases in the largest. IBM Cloud's 25 requests form 24 groups under shared
positive documents. All use the same host. AWS spans nine guide families
or 27 answer documents. ContextBench spans eight repositories, with five
represented among the six cases whose evidence spans all match. MultiLang
adds 50 screened repositories without independent acceptable evidence sets.

Treating those within-pool groups as separate gives 124 groups when AWS uses
guide families, or 142 when AWS uses documents. Cross-pool and model exposure,
missing labels, and actual paired development variance still affect sizing.
For scale only, if all 124 were independent and both arms always succeeded
against a known perfect baseline, the one-sided exact 95% lower success
bound would be 97.61%. This illustration does not estimate the required
paired contrast or prove that a gate is impossible. Neither overall nor
8,192-token judged paired outcomes have been measured. The private
active-confirmation-source-groups-r1.private.json digest is
2468b79029dd4fab82d92f5960503824841b1c9fa83f874f426151f10e909c92.

### Full TechQA menu corpus and ContextBench file provenance

The private TechQA corpus materialization verified every one of the 3,526
distinct document byte identities needed for the 76 active menus. The
76 ordered lists contain 3,800 positions, with 266 documents used in more
than one menu. The first exact case by publisher ID has a 50-document
private native root. The service listed exactly those 50 paths from one
publication and returned matching source sizes and hashes for every member.
The 50 files had 63 indexed chunks. This inventory precedes the separate
native offered-action and Context prompt captures.

The six fully aligned ContextBench cases reference five upstream repositories
at six bases. All 14 repo/base/path tuples matched their Git blob identities.
Observed canonical and fork lineages had no overlap among 60 resolved TRAIN
claims. Two TRAIN aliases remain unresolved. The private stage contains
13 source files and 16 license or notice files. Five cases have their entire
annotated source set staged. One case has three staged files and a withheld
generated dependency lockfile pending provenance review. The source roots
preserve the original revision-relative paths and exclude the notices.

| Private evidence | SHA-256 |
| --- | --- |
| techqa-full-menu-corpus-r1/manifest.private.json | 34109e91b1aa8acd13af56fc706ef3f73006502e3fdffed23843d13b263d1821 |
| techqa-menu-native-source-reads-r1/native-reads.private.json | 120e9b270f8fb5a0accc7ed31f33ba99f52e3016736409d7720592b84e1633ff |
| repository-confirmation-multilang-r1/contextbench-six-source-rights-r1.private.json | 34d01f50c1e6f4892c4ffcd54243a32af09abc122406a86a9ff5c2e03795a98a |
| contextbench-six-private-corpora-r1/manifest.private.json | 46dede63b8ceb84464471a8d66846989184bf3a2b8525a5bef76278ed1a5a9b8 |

### Additional technical-support recruitment screen

The 24 AWS requests with ambiguous gold occurrences stay excluded. The
active technical-support pool has 142 candidates against the planning
target of 154, before applying native menu and exposure checks. The
additional source screen yielded zero verified independent replacements.
The IBM release documents the available corpus and Telco restrictions.
The NVIDIA and TechSupportEval releases reuse TechQA questions. Counting
these copies would duplicate existing coverage.
[IBM release](https://github.com/IBM/mt-rag-benchmark/blob/e0970de1b1207c2a9506adf238e6e1a22037e38a/mtragun-human/README.md) · [NVIDIA dataset description](https://huggingface.co/datasets/nvidia/TechQA-RAG-Eval/raw/main/README.md) · [TechSupportEval release](https://github.com/NetManAIOps/TechSupportEval/blob/6500194228b667159e4b9a61fcbfe9e1d38df560/README.md)

### Native reads at six ContextBench revisions

Six isolated native services returned exact paths, source hashes and cited
bytes for all 24 staged spans. Five cases have full publisher span coverage.
The Tailwind case returned four of its twenty spans. Its other sixteen
are in the withheld lockfile. Each service reached a ready publication and
exited with zero remaining owned processes. This establishes source
visibility. Offered actions and judged paired outcomes still require
separate measurements.

| Base prefix | Native read receipt SHA-256 | Owned seconds |
| --- | --- | ---: |
| 7b36544e | 02fe079fa1d40d3cc990dcd65c0297d7a118fa8cc6d981dba1dea84d2ef8447f | 92.445 |
| cbd11d8a | f33eb0025848c43f5948643ec682cf94203e03246426476de62fe040e83c9cdd | 88.196 |
| 2de810e9 | 01d2c7e5f12391654ff5c6dcfd8b77e15baa36faed196e58f1120be5b763ca2d | 53.008 |
| 93cdb262 | 046c3272278d00c815db741ce80d7c1ee6228f4700046a6580b894fd73025ccc | 54.733 |
| daea6623 | fcf18d49bd64cf553cd300e9ecfb6c02e6b2284b97ebe64874fcc605478cd340 | 50.833 |
| f7997256 | d9997ae2dbc03e4feea3d423f9cb38395c42afc1bd795ae90a61fd30829e10ce | 62.236 |

### First TechQA offered-action pilot

DEV_Q002 returned its native Answer in 14.802 seconds with six records, eight
candidates, eight operations and no assessment. The first optional Focus
task failed with unknown observed work after reserving 2,048 tokens. Four
subsequent searches declined Focus while the request still counted that
work as unsettled. The native admission directory and dispatch journal both
had zero records. Both compiler prompt lengths and paired precision
outcomes remain unobserved. The positive-capture wrapper reported failure
after the native service and its Job settled. Reconciliation of the original
bytes preserved this zero-admission case. The private census SHA-256 is
`196c0f35e6eb5444b4e18837197f2156583a05fcd42c20865260c365ae28d8b8`.
