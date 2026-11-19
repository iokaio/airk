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
