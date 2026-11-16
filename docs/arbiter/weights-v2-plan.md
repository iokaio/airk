# AIRK weights v2: software evidence retrieval and Arbiter adaptation

Planning date: 15 November 2026, America/Denver. Status: research and proposed experiments; no training, model promotion, runtime change or publication is performed by this document.

The objective is to improve the evidence available to software-development agents and Workbench users: implementation, tests, contracts, configuration, explanations, exceptions and counterevidence. Success means better supported task completion with less follow-up work under the same total budget. Retrieval metrics support that outcome; they do not replace it.

Keep the selected combination: bundled MiniLM, independent optional Focus using Qwen3-Reranker-0.6B, and Context using either Qwen3-1.7B or Qwen3-4B. Improve each model for its own job. First establish reliable measurements and representative software data; then run small, successive weight experiments, export promising checkpoints early, and qualify the complete pipeline. Preserve the original weights whenever a replacement fails its gate.

This plan incorporates the user's request to reconsider MiniLM's role and subsequent preference to retain the combination. It reopens planning for future experiments after the installer-focused pause; it does not restart the stopped campaign or authorize an experiment through an old planning instruction. Only this file is changed. Findings and validation for this planning task are recorded here instead of modifying the execution ledger.

## 1. Review basis and what has actually been learned

### Review window

The six-hour code review was anchored when work began: **15 November 2026, 15:11:06–21:11:06 America/Denver**, equivalent to **15 November 22:11:06 through 16 November 04:11:06 UTC**. It covers 17 commits after `a650b156f58fa2e8afe5f37e7195e4bea66efbec` through `db26edae53c992a9eadf34faa8256cbae5a27e3e`. Concurrent version/generated-file work subsequently became `9771df28b1da2aaa8e3b7dddd2d141ba9d3da9af`, the 1.8.206 candidate; that delta was also inspected. No training or weight-export implementation changed in this six-hour window.

The review read [training.md](training.md), the relevant [implementation plan](../split/codex-plan.md), [execution-ledger](../simplify/one-process-ledger.md) entries and retained model receipts, and verified the live training/compiler/runtime source. [whatsleft.md](whatsleft.md) is an earlier stopped-work handoff: some of its proposed fixes have since landed. [knownissues.md](knownissues.md) and [relaxations.md](relaxations.md) describe the local candidate, not qualification. Live source and dated receipts determine which statement applies.

| Recent changes | Meaning for the experiments |
| --- | --- |
| `42555838`: installer seal correction, stale DiskANN graph refusal, size-dependent model-part download deadlines | Freeze working installation and index identities before measuring weights. Acquisition failures, stale vectors and graph incompatibility are separate failure classes. |
| `da5ff042`, `af5109a1`: failed/abandoned setup recovery; `9a4f3e88`: existing-installation presentation | Use a known matched runtime and retain recovery evidence; installation state must not confound comparisons. |
| `e1f78dd4`: Windows executable version resources; `24f208c8`: Admin default data-root reliability | Useful operational changes, with no evidence of neural improvement. Record binary hashes as well as displayed versions. |
| `db26edae`: U110 token-free loopback MCP, per-call scope, workspace defaults, `list_scopes`, scope annotations, client/harness changes | Rebase both baseline and treatment onto this transport. Use explicit index/set IDs for reproducibility and retain `read_scope` or receipt-bound `selection.scope_identity`. Test workspace resolution separately. |
| Candidate version/asset commits, training paper, installer records and UI issue records; then `9771df28` | Packaging/documentation progress does not qualify weights. Preserve all historical results under their original source/artifact identities. |

U110's lack of a bearer-token requirement does not remove scope resolution, local ingress checks, source identity, stage enrollment or budget enforcement. See the [current split contracts](../split/contracts.md), [open-read owner](../../crates/airk-service/src/service_runtime/open_reads.rs) and [graph regeneration check](../../crates/airk-kernel/src/current/enrichment.rs).

### MiniLM: real training, inadequate evidence for promotion

The [training implementation](../../tools/retrieval-train/train_encoder.py) genuinely changed the encoder weights. The original run used 256-token inputs, masked-mean pooling, L2 normalization, InfoNCE, in-batch and lexical distractors, batch 8, temperature 0.05, AdamW at `2e-5`, and three epochs. Its 315 positive-pair rows represented only 287 questions and 37 source groups: 69 QASPER questions from 36 papers plus 218 mechanically associated Requests documentation tasks. This was a small, highly concentrated corpus, not broad software-development supervision.

The original trained U8/S8 exports achieved mean fact recall **0.5231707317** and complete evidence on **72/205 questions**, failing the frozen 0.85/0.70 absolute floors despite gains over the two unsigned controls. Preserve that failure. It is not a current corrected-runtime baseline.

A later TRAIN audit found 58 cropped positive rows, 147 cropped distractors, and 82 hard distractors whose mined lexical terms disappeared from the consumed prefix. It observed zero known-positive negative-label violations; false negatives remain a risk to control, rather than a demonstrated cause of that particular run. Visible-prefix mining addressed one mismatch but did not establish improvement.

The latest weak parent-bag experiment aligned training to native semantic units and avoided declaring every fragment independently sufficient. However, it admitted only **99/315 rows: 96 QASPER and three Requests**, retaining 216 refusals. One preselected epoch contained 13 optimizer steps. Both adapted exports and both unsigned controls tied at **17/20 complete evidence and 11/12 mean recall** on DEV64, with **44 alignment refusals**. This is a tie on 20 evaluable questions, not an 85% pass on all 64 questions, and not proof that the models rank identically.

A major independent confound was DiskANN connectivity. With unchanged U8 weights, the corrected graph changed the same DEV result from **59/120 mean recall and 6/20 complete** to **11/12 and 17/20**. A 378-vector self-query probe then showed exact/self recall of 1.0 at k10 and k64 on that sample. These establish a repaired graph boundary, not general question-retrieval quality. The corrected models still produced differing ranked bodies, so coarse coverage metrics can conceal useful ranking differences.

The justified disposition was to retain original publisher graphs as `minilm-l6-v2-u8-v3` and `minilm-l6-v2-s8-v3`. The failed historical 205 run and the later partially consumed/cancelled FINAL remain historical; neither becomes an untouched test set.

Local receipt anchors, all private and not distributable:

- `artifacts/advanced-split/models/minilm-trained-r3/receipt.json`
- `artifacts/advanced-split/models/a1-train-only-recipe-audit-r1/audit-result.json`
- `artifacts/advanced-split/models/native-parent-bag-candidate-r1/`
- `artifacts/advanced-split/models/original-minilm-retention-decision-r1.json`

### Focus and Context: preparation, not completed AIRK neural adaptation

For the three selected Qwen models, the observed work exported publisher weights, bound tokenizers and readouts, checked numerical equivalence, measured some component resource costs, and prepared optional bundles. No completed AIRK fine-tuning of these selected models was found. Older four-model head experiments and failed calibration belong to superseded profiles; they must not be described as selected-Qwen training or transferred qualification.

Focus's W8 preparation passed retained numerical comparisons. Compact W8 exceeded the existing raw-logit error tolerance: **0.290778875 at T128 against a 0.25 limit**. Matching argmax on those cases did not waive the failure. Compact and Large therefore retained FP32 exports. Their roughly 1.13 GB, 6.90 GB and 16.10 GB optional payloads describe Focus, Compact and Large respectively; they are not RAM requirements.

Historical component measurements included Focus CPU4 p95 about 2.17 s, Compact CPU4 about 19.46 s, and Large CPU16 about 23.13 s. Large's earlier CPU8 attempt exceeded 30 s. These used different fits and retained input limits; summing them is not an observed composed latency, and none proves current 8192-token performance. Selected-Qwen task calibration, end-task gains and release qualification remain incomplete. The live release catalog contains three explicitly unqualified local-test profiles, not three qualified models.

### The exact interfaces the weights must learn

| Component | Live model view/output | Consequence |
| --- | --- | --- |
| MiniLM | 256 WordPieces; 384-dimensional masked-mean, normalized vectors; batch-one native inference | Tune query-to-native-unit retrieval. Preserve tokenizer, dimensions and pooling in the first experiments. Code identifiers fragmented or case-folded by the tokenizer still need lexical/exact retrieval. |
| Focus | One query/passage pair, fixed publisher instruction currently phrased for web search, escaped source controls, left-padded static 512 tokens, raw `no=2152`/`yes=9693` logits | Train the exact native input recipe. The compiler refuses overflow; it does not silently truncate. A domain instruction or wider graph is a separate recipe/export experiment. |
| Context | Fixed nonthinking Qwen chat; final-prefix-position scores for ordered single-token A–H choices; six-input graph and `scores[1,N]` | Train choice-symbol prediction with existing vocabulary rows. This is not a newly trained pointer head, free-form plan or generated answer. |
| Context evidence/menu | Current constructor presents at most four newest hit prefixes, each at most 192 UTF-8 bytes; up to five verified Read cards, plus terminal cards within eight total | A correct label must be inferable from this actual view. Full-file labels cannot silently become labels for tiny prefixes. Missing evidence/menu options establish a representation or proposal limit. |

Context chooses a **whole card**: action, gap, reason and every source-role/relevance hypothesis. Generic Read cards use Unknown/Uncertain evidence hypotheses. `Sufficient`, `Clarify` and `NoJustifiedRead` require separate semantic support and qualification; NoJustifiedRead cannot apply while verified choices are omitted. There are at most three learned assessments under the original shared allowance.

Owners: [Core Arbiter contract](../../crates/airk-core/src/arbiter/CONTRACT.md), [compiler contract](../../crates/airk-retrieval/src/arbiter/compiler/CONTRACT.md), [Focus compiler](../../crates/airk-retrieval/src/arbiter/compiler/focus.rs), [Qwen compiler](../../crates/airk-retrieval/src/arbiter/compiler/qwen.rs), [Context roster/prompt](../../crates/airk-retrieval/src/arbiter/context_action.rs), and [experimental profile contract](../../crates/airk-retrieval/src/arbiter/runtime/qualification/CONTRACT.md).

## 2. Why keep MiniLM alongside the little Qwen reranker?

**Keep the embedding role, and retain MiniLM as its current implementation. The selected Qwen reranker is not a replacement sentence encoder.**

MiniLM computes a vector for each passage when it is indexed. A query is encoded once, then DiskANN searches stored vectors. Focus jointly reads a query and one candidate passage, producing a relevance score specific to that pair. Its score cannot be precomputed once per document for all future queries. These are complementary operations, as the publisher interfaces distinguish: [MiniLM embeddings](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) and [Qwen pair reranking](https://huggingface.co/Qwen/Qwen3-Reranker-0.6B).

| Question | MiniLM | Focus |
| --- | --- | --- |
| What does it produce? | Reusable 384-dimensional passage/query vectors | Query-dependent passage relevance scores |
| When is most corpus work done? | At indexing; changed passages are re-encoded | At query time for every admitted pair |
| How does it search many passages? | Approximate nearest-neighbor lookup over stored vectors | Needs a candidate generator, or must score the entire collection |
| Primary optimization target | Find necessary evidence within a small candidate budget, including paraphrases | Put the most useful available evidence ahead of near misses |
| Availability in AIRK | Bundled, independent of Arbiter | Optional download, separate enablement/enrollment |

For a corpus of N passages and an admitted pool of k, a reranker-only exhaustive search requires N joint pair evaluations per query; the normal pipeline uses one query embedding, index lookup and k pair evaluations. This is a work-count argument, not a latency estimate. The present batch-one CPU Focus profile makes corpus-wide pair scoring especially unattractive, and calling the model "0.6B" does not make that work free.

A lexical-first system followed by Focus is technically possible. It can work well for exact symbols and error strings, but cannot recover a semantically relevant passage absent from its lexical candidate pool. For example, a question about avoiding duplicate execution may need a contract phrased in terms of idempotency and a test whose name shares neither phrase. Improving MiniLM should increase the chance that both reach Focus. The model does not need to answer the question itself.

There is a separate **Qwen3-Embedding-0.6B**, with a different learned retrieval objective and embedding interface. Its publisher offers dimensions up to 1024 and configurable smaller outputs. It could be evaluated as a future encoder replacement, but it is not the selected reranker's weights reused as an encoder, and using it alongside Focus still entails two separately trained models. Even a 384-dimensional setting would require new representation identity, native export, corpus re-embedding and resource qualification. No such replacement is proposed in this campaign. [Official Qwen embedding card](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B).

As a cheap architectural control, measure lexical-only, lexical+Focus, original-MiniLM hybrid, and hybrid+Focus at matched budgets. Report evidence contributed uniquely by the semantic branch, particularly on low-word-overlap questions. This measures the value of the retained combination; it is not a new model tournament. Removing bundled semantic retrieval would also change the existing offline, Arbiter-disabled product behavior and requires a separate product decision.

## 3. Research that determines the experiment order

The sources below were inspected online for this plan. Published results motivate hypotheses; none establishes an AIRK gain. Recent 2026 preprints merit replication before reliance. Pin exact paper/dataset/model revisions when an experiment begins.

| Primary source | Applicable method | Boundary of the evidence |
| --- | --- | --- |
| [CoRNStack, ICLR 2025](https://proceedings.iclr.cc/paper_files/paper/2025/file/285b0e63256699c04dbf2c8d4163fba4-Paper-Conference.pdf) and [authors' repository](https://github.com/gangiswag/cornstack) | Verify query/code positives; filter likely false negatives; introduce diverse, progressively harder negatives. Directly relevant to code retrieval, reranking and issue localization. | Different models/data; teacher filtering may reject difficult true positives. Audit disagreements. The repository does not establish complete reproduction tooling for every reported experiment. |
| [CodeRetriever, EMNLP 2022](https://aclanthology.org/2022.emnlp-main.187/) | Mix natural-language-to-code and useful code-to-code relationships. | Docstrings, comments and co-occurrence are weak supervision; a caller and a callee are not interchangeable meanings. |
| [CodeSage, ICLR 2024](https://arxiv.org/abs/2402.01935) | Code-aware denoising before retrieval tuning is a later hypothesis for persistent code-representation gaps. | Does not justify expensive continued pretraining before simpler supervised adaptation. |
| [Qwen3 Embedding report](https://arxiv.org/html/2506.05176v1) | Preserve Focus's task-specific yes/no response supervision and official input/output semantics. | Embedding-model results do not transfer to the reranker or to Context. Publisher context capacity is not AIRK's exported width. |
| [Yuanbao-Reranker, November 2026](https://arxiv.org/html/2611.10405v1) | Ground synthetic queries in actual documents, verify positives, and combine relevance supervision with capped teacher ranking preferences. Includes code retrieval evaluation. | Starts from Qwen instruction backbones; reported training used millions of groups and a large GPU fleet. Our continued-reranker pilot is an adaptation of the idea, not a replication. |
| [MemReranker, May 2026](https://arxiv.org/html/2605.06132v2) | Continued Qwen3-Reranker training with general replay, soft relevance targets and later group ranking losses. | Dialogue-memory evidence, not software evidence; useful methodological precedent only. |
| [System One Models Are Not Better Rerankers, October 2026](https://arxiv.org/html/2610.32318v1) | Keep relevance ranking distinct from generic decision tuning and from probability calibration. | English retrieval experiments do not settle code ranking or Context action selection. |
| [SWE-QA-Pro](https://arxiv.org/html/2603.16124v1), [official implementation](https://github.com/TIGER-AI-Lab/SWE-QA-Pro) | Repository QA taxonomy, grounded synthetic tasks, supervised trajectories before preference/RL work. | Its Qwen3-8B, larger contexts and many tool turns differ substantially from AIRK's finite cards and three assessments. Do not import its server or action space. |
| [Qwen3 technical report](https://arxiv.org/html/2505.09388v1) | Distillation and supervised preparation are credible first experiments for the smaller Qwen models. | General generation/distillation results do not establish closed-card utility or safe stopping. |
| [EfficientRAG](https://arxiv.org/abs/2408.04259), [Adaptive-RAG](https://arxiv.org/abs/2403.14403) | Train compact decisions from actual retrieval outcomes; distinguish iterative evidence acquisition from final answer generation. | Open-domain QA formulations require new software-specific labels and native action mapping. |
| [Self-RAG](https://arxiv.org/abs/2310.11511), [Sufficient Context](https://arxiv.org/abs/2411.06037) | Separate relevance, support and sufficiency; evaluate abstention as well as successful answers. | Do not import reflection generation or treat an automated sufficiency judge as human truth. |
| [DAgger](https://proceedings.mlr.press/v15/ross11a.html) | Collect supervision on states the current learner actually reaches, addressing sequential distribution shift. | Use offline native replay. No production learning or general-purpose agent loop is added. |

Evaluation should combine complementary primary benchmarks:

- [CoIR](https://aclanthology.org/2025.acl-long.1072/) supplies text/code/code-to-code retrieval slices; overlapping CodeSearchNet-derived tasks are not independent replications.
- [CORE-Bench v3](https://arxiv.org/html/2606.11864v3) addresses code understanding, issue localization and broader repository context. Its supervision is closer to the intended candidate-retrieval job than docstring matching alone.
- [Agent Retrieval Bench](https://arxiv.org/html/2607.24882v1) covers implementation-to-test, review-comment context, stack-trace localization and edit impact. It also distinguishes natural no-useful-local-evidence cases from artificial wrong-repository controls. Its file-level and uneven-repository results do not establish exact-span coverage; supplement them with AIRK span labels.
- [RepoQA](https://arxiv.org/abs/2406.06025) is a useful semantic function-localization diagnostic. [Peng et al. SWE-QA](https://arxiv.org/abs/2509.14635) supplies repository questions involving intent and cross-file reasoning. Pin its exact release: it is distinct from [Elkoussy/Perez SWE-QA](https://arxiv.org/abs/2604.24814), a multiple-choice repository benchmark with the same short name.
- [CodeRAG-Bench](https://arxiv.org/html/2406.14497v1) and [SWE-bench](https://arxiv.org/abs/2310.06770) motivate downstream checks with code and documentation. Patch success is supplementary evidence: changed files are not a complete relevance oracle, and the downstream agent remains responsible for code generation.

Do not merge all these datasets indiscriminately. Assign each family to training or evaluation before access, check overlaps, and use a manageable, licensed subset with a clear question-to-evidence purpose.

## 4. Stages, dependencies and checkpoints

| Stage | Outcome | Entry/exit decision |
| --- | --- | --- |
| 0 | Trustworthy native baseline and error attribution | Finish a bounded diagnostic batch; separate infrastructure/input failures from weight errors. |
| 1 | Software-focused corpus, labels and untouched evaluation | Freeze source rights, splits, native-visible labels, metrics and experiment budgets. |
| 2 | Better MiniLM candidate retrieval | Compare supervised adaptations and both native INT8 exports against exact original controls. |
| 3 | Better Focus ordering | Continue yes/no relevance training, then test preference distillation only if useful. |
| 4 | Better Context read decisions | Train Compact/Large on exact native cards and learner-visited states. |
| 5 | Independently supported Context terminals and calibration | Qualify each action class; keep unsupported classes unavailable. |
| 6 | Efficient, faithful native artifacts | Diagnose precision loss, measure native quality/resources, and freeze each export. |
| 7 | Composed evidence and downstream gains | Evaluate the frozen cascade and both Context sizes under equal budgets. |
| 8 | Controlled promotion and recovery | Re-index for encoder changes; complete artifact/platform/product gates before release claims. |

Stages 0–1 precede gradient work. MiniLM, Focus and Context owners may prepare independent experiments after the common data/input freeze. A promising Focus teacher can feed a second MiniLM experiment; any resulting upstream change requires refreshed downstream evaluation. Precision feasibility can be investigated early, but final calibration and qualification use the final native artifact. There is no calendar deadline that overrides a failed gate.

### Stage 0 — Re-establish the baseline and locate the bottleneck

**Question:** Which misses are caused by weights, and which are caused by candidate generation, indexing, evidence visibility, transport or budget?

1. Freeze the matched runtime pair, original MiniLM U8/S8 assets, all three selected Qwen profiles, tokenizer/compiler/task policies, source snapshots, platform and actual CPU fit. Use fresh isolated experiment indexes; preserve existing user indexes. Run the smallest service/client preflight that proves this runtime and U110 scope path work.
2. Reuse graph-connectivity regressions and run a small exact-search-versus-DiskANN comparison on the same vectors and queries. Separately measure semantic-only, lexical-only and production hybrid retrieval. Count extraction/alignment refusals, unavailable graphs, fallback modes and partial results in the original task denominator.
3. Re-score a development diagnostic set with original and retained adapted MiniLM checkpoints under identical corrected extraction, graph and return-window behavior. This is diagnosis of old hypotheses, not requalification or another use of the historical FINAL for selection.
4. Capture actual native model views: MiniLM's encoded units, Focus's complete admitted windows and overflow refusals, Context's source prefixes and whole cards. Measure token lengths, padding charges, missing gold spans, lost identifiers and the proportion of menus containing a useful legal next read.
5. Obtain two ceilings: a human/teacher choice from the **same visible input and menu**, and an explicitly separate full-evidence oracle. A large gap between them identifies representation/proposal problems that more training cannot solve. A low offered-choice ceiling calls for native recipe work before Context tuning.
6. Retain trace timings for retrieval, compilation, queueing, each forward, additional reads, packing and transport. Diagnose the retained Answer timeout before using long captures as a training loop. A raised deadline alone is not a correction.

First use the existing 2048-token/30-second default. Structural ceilings of 8192 tokens/120 seconds are a separate research configuration; Context's emitted profile still needs an explicit compatible limit, and Focus's graph remains 512. Two full-width Focus calls already account for 1024 tokens before Context. Measure that tradeoff instead of assuming a large candidate list and several Context passes fit.

**Deliverable:** frozen baseline receipt; per-task failure taxonomy; actual token/menu census; exact-search and native results; a short list of weight-addressable errors. Stop further neural work on a failing seam until its cheap reproduction is understood. Keep separate baseline populations for unchanged/default versus explicitly wider profiles.

### Stage 1 — Replace the narrow corpus with software evidence supervision

**Question:** Can the examples teach retrieval and action usefulness at the granularity AIRK actually consumes?

Start with a proposed **2,000–5,000 query-group training pilot** across at least 20 unrelated repository families, adjusted only before scoring if rights or annotation capacity prevent that size. Scale toward 25,000–50,000 groups only after a measured learning curve and a useful native pilot. These are planning sizes, not existing assets or a claim of statistical power.

Suggested initial training mix: 40% code/tests, 35% developer documents/contracts/configuration, 15% cross-file issue/diagnostic tasks, and 10% general-document replay. Treat these as mutually assigned task strata even when evidence spans several file types. Balance repository families and report the actual language/file-type mix. Include Rust, TypeScript/JavaScript, Python, Go, C/C++, shell/PowerShell, Markdown, structured configuration, and supported extracted technical-document formats. Do not let the single Requests family or Python-only benchmarks dominate.

Build tasks around concrete needs: find implementation and its contract; connect a failure to source and regression tests; explain a configuration default and override; locate platform differences; identify a migration constraint; find a negative test or exception contradicting an apparent rule; trace a caller/callee or interface/test dependency. Mix exact-symbol, stack-trace, prose-only and intentionally ambiguous questions.

Each example retains repository/fork family, pre-fix commit where applicable, license/provenance, original path and exact evidence spans, native unit/window mapping, question origin, alternative support sets, relevance grades, annotation authority and uncertainty. Preserve alternative valid evidence sets; never construct a fictitious complete answer by unioning incompatible human alternatives.

Use five disjoint populations: TRAIN, DEV, CAL, policy-confirmation and FINAL. Split by repository/fork, issue/symbol family and chronology **before** mining, synthesis or teacher scoring; deduplicate code clones, copied docs, paraphrase families and near-identical fixes. Reserve whole unseen projects and some language/task slices. The final source closure is for evaluation only, not unsupervised training, negative mining or teacher-guided recipe choice. Historical evaluation-only restrictions remain in force even when a test is consumed.

Public file-level labels must be adjudicated into relevant visible spans. A future patch can help an offline annotator identify an error, but its text, filenames revealed only by the solution, and later documentation cannot enter the pre-fix query or candidate corpus. Assess existing tests/docs that were not modified too.

Use document-grounded synthesis for TRAIN: select a genuine evidence span or multi-span relation, formulate the information it supports, then produce diverse developer questions. Verify each against original source; reject hallucinated APIs and unsupported premises. Store teacher identity and rationale as annotation metadata. Human review must cover every final critical-evidence/terminal label, teacher conflicts, and a stratified training sample. Agent review is not human gold. No private workspace, customer documents or live requests become training data without separate explicit consent; use already authorized local compute and teachers.

Negatives should include wrong overloads, neighboring modules with similar names, related prose missing the required condition, incorrect platform/version, and tests for a different failure mode. Mine from TRAIN lexical and dense candidates, adding verified medium/easy negatives. Unjudged evidence is unknown, not automatically negative. A passage that contradicts the presumed answer may be highly relevant. Mask all known alternative positives and clone-equivalent passages from in-batch negative sets.

Use exact native-visible units for positive and negative judgments. If the decisive span is absent, record a representation miss; do not train a fragment to assert unseen content. Multi-instance parent supervision can remain a separately labeled weak-data arm, with equalized bag-size controls and its original denominator. It must not silently replace direct span supervision or shrink software coverage to a handful of examples again.

Dataset rights require per-source verification. The [CodeSearchNet license guidance](https://github.com/github/CodeSearchNet#licenses) distinguishes the repository's MIT tooling from corpus-source licenses; [The Stack v2 card](https://huggingface.co/datasets/bigcode/the-stack-v2-dedup) has source-license/provenance and access conditions. A benchmark wrapper license does not settle rights to every underlying document. Retain the existing Hotpot evaluation-only restriction. Pin accepted inputs and notices; keep raw fixtures and teacher captures private.

**Exit:** an audited native-alignment report, meaningful coverage in every required software stratum, independently authored evidence labels, a sample-size/power rationale and an untouched final reservation. Retain a general-document regression panel. Reject a corpus whose nominal size hides concentrated repositories or extensive alignment exclusions.

### Stage 2 — Improve MiniLM's weights for candidate recall

**Primary hypothesis:** native-aligned, multi-positive software supervision with useful negatives will improve evidence reachability more reliably than extending the old narrow run.

Keep 256 WordPieces, 384 dimensions, masked-mean/L2, tokenizer and batch-one deployed ABI fixed. Start each comparable arm from the exact original publisher checkpoint. Small-model full-encoder tuning is the first choice; a frozen decision head would not change retrieval representations.

| Experiment | Change | Control and decision |
| --- | --- | --- |
| M0 | Original weights on corrected native pipeline | Establish exact dense and hybrid results; both unsigned historical controls remain identifiable. |
| M1 | Multi-positive contrastive tuning on native units, with general-document replay | Compare easy-negative and screened lexical+dense-negative arms on the same rows. Use all known useful positives without pushing complementary evidence away. |
| M2 | M1 plus audited Focus/teacher ranking distillation | Run only after the teacher demonstrates superiority on the relevant DEV pairs. Compare contrastive-only, margin-distillation-only and their small combined pilot. |
| M3 | Refresh TRAIN hard negatives once using the selected M1/M2 encoder; balance difficult software strata | Compare with equal additional training on the old negatives. This distinguishes new information from extra optimizer steps. |
| M4, conditional | Short code/document denoising followed by the winning retrieval recipe | Only for demonstrated residual code-representation gaps; compare with equal compute spent on more retrieval supervision. |

M1 should use a multi-positive-aware contrastive objective: average the positive log-loss across independently required evidence units or evidence-need groups, excluding all known positives from negative columns. Pool score mass only among genuinely interchangeable alternatives within a group. Do not pool every useful passage into one numerator: one easy implementation match could then satisfy the loss while the required test or contract remains poorly ranked. Preserve alternative valid support sets and use their adjudicated grouping, not invented exclusive role labels. Keep a separate complete-evidence-set metric; making each positive individually retrievable does not ensure the set fits top-k.

Proposed pilot settings are AdamW, one epoch initially, learning rates `5e-6`, `1e-5`, `2e-5` explored sequentially, and effective contrastive batches 32–128 if memory permits. Keep the initial temperature fixed at 0.05 to isolate data/objective changes; tune it later only if needed. Plain gradient accumulation does **not** enlarge the set of in-batch negatives: use explicit negative groups or a verified cached contrastive method. The [Sentence Transformers loss reference](https://www.sbert.net/docs/package_reference/sentence_transformer/losses.html) provides relevant multiple-negative and margin-distillation formulations; pin implementation versions and independently test masking.

For M2, distill score differences or a distribution over shared candidates, not vectors from a differently sized model. The Focus teacher must see the same candidate content being labeled, or labels must be checked for inferability from the MiniLM unit. Freeze teacher, prompt and candidate generation; retain human overrides and uncertain pairs. This transfers some of Focus's judgments into cheap retrieval without making MiniLM run Focus at query time.

A separate **effectiveness arm with unchanged weights** should examine native chunk boundaries, identifier fragmentation, title/path context and lexical fusion only where Stage 0 identifies a loss. Use a 2×2 comparison of original/adapted weights and original/revised input recipe. Changing semantic input construction changes pipeline identity and requires its own recorded decision. Never attribute a chunking or DiskANN correction to weight learning. Longer inputs or a new tokenizer are outside the first experiment.

Measure Recall@10, complete evidence-set coverage, nDCG/MRR, recall at the actual Focus admission depth, semantic-only rescue cases, general-document regressions, and per-language/repository strata. Exact dense retrieval identifies encoder quality; production DiskANN and hybrid results establish delivered effect. Export promising checkpoints to both U8 and S8 before scaling. A float-only gain that disappears in the shipped profiles is not a successful replacement.

**Exit:** improvement over both original controls on fresh DEV/confirmation, stable across three finalist training seeds, with intact provenance and acceptable native cost. Retain the original encoder if only rankings change without a useful coverage/efficiency gain or if a critical stratum regresses. Final promotion additionally requires the frozen gates in section 6.

### Stage 3 — Improve Focus's relevance weights

**Primary hypothesis:** continued domain-specific relevance training will distinguish useful implementation, tests, constraints and counterevidence from topically similar distractors within the bounded pool.

Use Qwen3-Reranker-0.6B as the base, preserving yes/no scoring and the current native source-escaping behavior. Train on candidate windows emitted by AIRK, including the real 512-token admission behavior. Reject or separately model overflows; never silently crop them in Python while Rust refuses the same input.

| Experiment | Change | Required comparison |
| --- | --- | --- |
| F0 | Frozen publisher weights, current native instruction | Baseline on a fixed candidate pool, including near misses and contradictory-but-relevant evidence. |
| F1 | Prompt-only software-evidence instruction | A separately versioned native recipe and fixtures, with unchanged weights. Keep the current prompt control; do not combine this gain with SFT attribution. |
| F2 | Continued relevance SFT on exact query/window pairs | Compare original versus adapted weights under the same chosen prompt; include general-domain replay. |
| F3 | Teacher preference/distillation added to F2 | Test one additional objective at a time: soft relevance targets, groupwise loss or capped preference hinge. Keep pointwise supervision. |
| F4 | One refreshed hard-negative round and robustness repair | Compare equal-compute continued F2; stop if harder mining increases mislabeled negatives or harms evidence diversity. |

F2 uses response-position supervision, masking the prompt. Start with full-vocabulary cross-entropy on the correct yes/no token, matching the publisher-style task, and measure the deployed yes/no logit difference. Do not assume teacher sigmoid scores are calibrated labels. Multiple passages can be relevant; an exception or counterexample earns relevance even when it opposes the question's premise.

Use a small LoRA pilot to conserve training resources, then compare full tuning only if the adapter shows a measured capacity limit. Suggested initial rank 16, learning rate `5e-5`, 1–2 epochs, effective batch 32 and a fixed replay fraction; vary rank or rate in the next experiment, not all dimensions at once. These are starting proposals, not measured optimal settings. Merge accepted adapters into a full reference checkpoint for the existing ONNX export path. [LoRA](https://arxiv.org/abs/2106.09685).

For F3, the Yuanbao-inspired arm uses verified grounded positives plus graded genuine negatives and capped teacher score-gap preferences; mask ties and conflicts. The MemReranker-inspired arm uses soft relevance targets and later group ranking. Neither needs millions of examples to test the hypothesis, and neither paper proves the adapted small AIRK run will work.

Evaluate nDCG@k and necessary-evidence/exception retention on a **fixed pool**, then evaluate the integrated pool from the selected encoder. Report original pool size, scored subset, overflow rate and delivered subset. A reranker cannot fix missing candidates; ordinal gains that discard a critical complementary passage are failures. Keep deterministic diversity/overlap controls separate from learned score changes.

Do not add a relevance cutoff merely because scores lie between zero and one. If Focus only reorders, calibration is a diagnostic. Any discard/abstention policy needs a separately frozen task definition and native-supported semantics. Wider pair windows are an export/profile experiment only if observed evidence loss justifies the extra token/time cost.

**Exit:** better useful-evidence ordering and retention at the same admitted pool/budget, with preserved general-document behavior and a numerically valid native export. Freeze a teacher candidate for M2 only after this evidence exists.

### Stage 4 — Train Context for useful, bounded evidence gathering

**Primary hypothesis:** direct supervision of the exact whole-card decision will be more effective than relying on generic language-model capability or training on long free-form agent traces.

Start with Compact for the inexpensive training loop; retain unchanged Compact and Large controls on identical states. Prepare Large's data and export path in parallel. Both selected sizes remain in scope and require their own results. Carry a winning Compact recipe to Large, then test Large-to-Compact distillation only if Large is a measurably stronger teacher.

| Experiment | Objective | What is held fixed |
| --- | --- | --- |
| C0 | Score frozen publisher Compact/Large on native states and cards | Upstream retrieval, prompt, eligible options, budgets and labels. |
| C1 | Whole-card supervised LoRA adaptation, initially useful Read decisions | Native A–H readout, actual evidence view and complete card semantics. |
| C2 | Add counterfactual read-utility labels and bounded bridge-read cases | Same native action catalog; only authorized TRAIN reads; no future observations in input. |
| C3 | Aggregate labels on states reached by the adapted learner | Compare against equal-size/equal-compute additional expert-state SFT. |
| C4, conditional | Restricted action preferences or teacher-distribution distillation | Same state/menu; independently measured utility; SFT/reference regularization. |

For C1, loss applies at the decision symbol over the allowed options. Use a target distribution over equivalently useful whole cards when several reads are valid; arbitrary single winners teach position and annotation accidents. Start with LoRA rank 16, learning rate `5e-5`, dropout 0.05, effective batch 32 and one epoch, then separately test rank 32 or `1e-5`/`1e-4` if diagnostics justify it. Limit the pilot to 1–3 epochs with DEV early stopping. Train attention projections first; add MLP projections only after a measured underfit result. Keep tied vocabulary/readout semantics intact and verify the merged model.

Balance correct option positions in TRAIN through separately emitted, provenance-bound native fixture variants; do not reorder a production roster after its identity has been frozen. Test sensitivity to option position, alias names, equivalent source paths and evidence order on isolated fixtures. Original runtime fixtures remain the primary evaluation. Avoid an always-read policy by representing complete support, duplicate-only choices, ambiguities and absent useful choices. Keep every roster option in native scoring: the current selector takes the global winner and its margin over the full roster, then applies class calibration. An unsupported terminal winner causes abstention, never fallback to a lower-scoring Read. During read-only C1, retain states requiring terminal decisions as refusal/evaluation probes until Stage 5 supplies their supervised labels; do not fabricate a Read target. There is no independent abstain token in this ABI.

For C2, reset to the same pre-action state and execute each eligible counterfactual read in the offline harness. Label the marginal gain in independently required evidence and its actual token/read/time cost. A candidate utility is:

`new required support + new exception/counterevidence - duplicate evidence - measured work cost`.

Freeze the weights of those terms on DEV before CAL/FINAL. This is a proposed training target, not a native authority rule. Critical support cannot be traded away merely to save calls. Include limited two-step TRAIN lookahead when a bridge read reveals the next permissible target; one-step gain alone can mislabel such a read as useless. All trajectories obey the existing maximum assessments, legal menus and original total budgets.

The student receives only the pre-action question, shown evidence and native cards. Future returns may label utility but must not leak into the input. If the best action is indistinguishable from the visible state, record ambiguity or use a soft/unknown label; do not teach hidden facts. Audit **every assertion in the selected card**, not just whether its read eventually retrieves a useful paragraph.

C3 collects the model's own mistakes and recovery states on TRAIN snapshots, obtains corrected labels, and retrains. This is offline dataset aggregation inspired by DAgger. It addresses the mismatch between neat expert trajectories and real partial/failed reads without introducing runtime learning.

C4 is conditional. Compare simple pairwise action ranking with restricted-symbol [DPO](https://arxiv.org/abs/2305.18290) only after SFT leaves a defined residual. Preferences must compare actions from the same state/menu and be grounded in evidence and costs. Do not reward confident explanations, hidden solution knowledge or extra reads. Online GRPO and broad autonomous search are deferred; current evidence does not justify their cost or product scope. Large-to-Compact distillation uses audited distributions over native cards, not generated chain-of-thought.

**Exit:** improved useful-read precision, critical-evidence gain per accepted read and bounded-trajectory coverage, including learner-visited states and Focus-off inputs. Report the rate of missing useful options and input insufficiency separately. If these dominate, return to a separately reviewed evidence-window/proposal experiment with frozen weights rather than scaling neural training.

### Stage 5 — Train and calibrate terminal judgments separately

**Question:** Can a specific exported Context model safely accept a particular whole-card action, and when should it abstain?

Create human-adjudicated software cases for each existing class. Sufficient requires all task-required support in the retained visible evidence, not an answer recoverable from the model's memory. Clarify means genuine question ambiguity or a supported need for clarification, not merely retrieval failure. NoJustifiedRead concerns the current complete verified menu; it is neither global unanswerability nor a synonym for exhausted time. Unknown read outcomes remain unknown.

Use paired cases that remove one decisive constraint, replace a current API with a stale version, introduce a conflicting test, conceal a platform qualification, or leave only duplicate reads. Include natural no-useful-local-evidence cases as well as easy wrong-repository controls. Agent Retrieval Bench's calibration gap motivates treating those populations separately. A model that can identify random wrong repositories has not proved it can recognize a subtle missing contract.

The current selected-Qwen native fit must admit at least one certified positive and zero accepted UNKNOWN. If no cutoff meets both conditions, no qualified profile results. Current promotion scope is only generic Read with Unsupported/AdditionalEvidenceMayHelp/Unknown/Uncertain semantics. Terminal research therefore needs a recorded extension of the owning qualification/promotion contract and independent fixtures before terminal acceptance can be deployed; the existence of terminal enum variants is insufficient. See the [final-distribution contract](../../crates/airk-service/src/service_runtime/arbiter/final_distribution/CONTRACT.md).

Train terminal-bearing cards in a separate C-terminal arm after useful-read SFT. Label roles/gaps as part of whole-card correctness; preserve overlapping legitimate role interpretations. Do not map a QASPER answerability label or a mechanically projected support addition directly to human sufficiency/clarification gold.

Fit native-supported action-class raw-margin acceptance on a disjoint CAL population using the **final export and exact policy**. Confirm once on the independent policy-confirmation split. If temperature scaling is investigated, keep it offline unless a recorded native calibration-contract decision adds the required consumption path; the existing margin schema cannot silently consume a new probability model. [Temperature-scaling research](https://proceedings.mlr.press/v70/guo17a.html) supports a diagnostic baseline, not a probability-of-completeness guarantee.

Report risk versus coverage, per-class accepted precision, false-sufficiency counts, unnecessary clarification, useful-read rejection, raw-margin distributions and all unsupported/invalid/unavailable outcomes. Brier/NLL or calibration error are meaningful only against a clearly defined, valid target distribution; conditional A–H softmax is not the probability that the repository has been exhaustively understood.

Proposed additional terminal research target: a one-sided 95% upper bound below 1% on false accepted Sufficient decisions, while retaining the existing zero-observed-false-sufficient gate. Roughly 299 independent accepted zero-error examples would be needed for the elementary binomial bound; correlated repository cases require cluster-aware analysis and potentially more data. This target must be frozen before scoring and does not replace existing gates. CAL100 alone cannot establish it. Insufficient evidence means a class remains unsupported, not a weaker post-hoc threshold.

**Exit:** independently supported acceptance rules for each admitted class, exact native metadata, useful coverage and transparent uncertainty. Generic useful-Read qualification may proceed while terminal classes remain unsupported; do not claim complete Context functionality from that partial result.

### Stage 6 — Make the improved weights practical in the native runtime

**Question:** Do the quality gains survive export and precision changes at an acceptable local cost?

Separate three comparisons: trained reference versus FP32 ONNX; that ONNX versus quantized ONNX; and each native artifact versus the frozen end-task baseline. Preserve exact tokenizer behavior, escaped prompt bytes, positions, masks, output vocabulary rows and source/task identities. Include near-tied candidates and difficult real software states, not only synthetic tensor shapes.

For MiniLM, qualify both U8 and S8 and their actual CPU instruction paths. Preserve the existing mixed-precision protections until an independently tested change improves the outcome. Numerical cosine agreement alone is insufficient: measure changed neighbor/rank order and evidence coverage. Quantization and approximate search have different error sources and require separate measurements.

For Focus/Context, first diagnose the failed Compact W8 export using matched intermediate tensors and score margins. Test sensitive-layer exclusions or per-channel/block scaling before a whole new expensive campaign. The [official ONNX Runtime guide](https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html) recommends matching float/quantized tensors to locate loss. [AWQ](https://arxiv.org/abs/2306.00978) supplies an activation-aware hypothesis, conditional on the pinned native provider supporting the resulting operators. It is not permission to replace AIRK's runtime or download an unrelated community export.

An [open ORT issue concerning Qwen3 and MatMulNBits accuracy levels](https://github.com/microsoft/onnxruntime/issues/29849) reports near-tie argmax changes with int8 activation handling. Inspect whether AIRK's failed graph even uses that operator/path before testing the setting. This is a diagnostic lead, not an established explanation for the observed AIRK failure.

Try W8/mixed precision before more aggressive W4. Keep a full-precision reference and reject any artifact outside the frozen numeric limits, even if its small fixture argmax agrees. If post-training quantization repeatedly loses genuine task gains, consider a separate quantization-aware training experiment only after a cheap reproduction. [QLoRA](https://arxiv.org/abs/2305.14314) is a training-memory technique; successful QLoRA training does not qualify a four-bit ONNX deployment.

Measure download bytes, staging/disk peak, load-time peak, steady and peak process memory, warm/cold p50/p95, throughput and ordinary MiniLM progress under concurrent optional work. Record physical cores, ISA/provider/runtime, actual threads, input widths, candidate counts and shared reservations. Keep acquisition time separate from inference. Measure combinations in one resident service; do not add isolated component RSS numbers and call the sum a measured requirement.

Recheck actual cancellation/drain, OOM/recovery, disabled/excluded indexes, scope changes and offline inference. Neither timeout nor transport disconnect proves that native work ended. The current longer download allowance solves a transfer constraint, not memory or inference efficiency.

**Exit:** a native artifact with retained quality, valid numerics, supported operators and measured resources. Refit/confirm calibration for that exact artifact. If compression fails, keep the valid larger artifact labeled with its measured limitations; do not silently switch precision or lower tolerances. A resource-unfit artifact remains unpromoted.

### Stage 7 — Demonstrate the composed benefit on software tasks

Freeze finalists, prompts, thresholds, corpus, agent/scaffold and budgets before FINAL. Use DEV to reduce the matrix; do not launch a full Cartesian product of every training checkpoint.

The minimum attributable comparison set is:

| Condition | Purpose |
| --- | --- |
| Original MiniLM hybrid, ordinary follow-up reads | Real baseline with normal follow-up capability. |
| Original MiniLM hybrid, deterministic evidence packet | Separates native gathering/organization from neural benefit. |
| Adapted MiniLM hybrid, no optional stages | Measures the encoder's own contribution. |
| Selected encoder + original versus adapted Focus, Context off | Isolates Focus ordering. |
| Selected encoder + Compact or Large, Focus off | Qualifies Context's independent mode. |
| Selected encoder + selected Focus + original versus adapted Compact | Isolates Compact adaptation in the composed path. |
| Selected encoder + selected Focus + original versus adapted Large | Independently measures Large; Compact's result does not transfer. |

Add focused weight/recipe cross-controls where a prompt or chunk representation changed. If MiniLM adaptation is rejected, keep the original encoder in downstream comparisons; do not make a rejected model a mandatory prerequisite. Recheck final Focus and Context on actual upstream distributions, including fallback/excluded-index populations.

Score required-evidence coverage, complete coherent support alternatives, citation/source identity, counterevidence retention, unsupported claims, task completion, external agent turns, all internal canonical reads, inference work and end-to-end resource cost. For repository tasks, add correctness of implementation/test/contract localization and a fixed-agent repair subset if appropriate. Grading uses independent evidence rubrics, not just model judges or whether a patch happens to pass tests.

Allow the ordinary baseline its legitimate searches and follow-ups. All conditions share the same authorized corpus and total read/token/byte/deadline budget. Count internal Arbiter work separately from visible MCP calls. Report all-task outcomes and failure costs as well as the paired common-success population; a system must not appear faster because it failed its difficult tasks.

Use repository-cluster paired uncertainty, report repository-macro and task/language metrics, and retain query-level deltas. Select the strongest ordinary/deterministic baseline using the existing baseline-only rule before considering Arbiter results. Keep seed stability and aggregate statistical evidence separate from the exact historical gate calculation. A small coarse tie is inconclusive, not proof of equivalence.

**Exit:** the exact default or separately qualified larger-budget profile passes the frozen end-task and native gates. Large must justify its cost with measured coverage/quality or other accepted benefit; a larger parameter count does not establish that benefit. If final results fail, retain the failure, choose no replacement, and prepare a new hypothesis and fresh final reservation for the next iteration.

### Stage 8 — Promote independently identified models with recovery

Promote only components that pass their applicable gates. MiniLM changes require a new embedding/pipeline identity even at 384 dimensions, cache invalidation and corpus re-embedding/regeneration. Never query original document vectors with adapted query embeddings. Preserve lexical availability and honest semantic readiness during regeneration. Test interrupted regeneration, stale-graph refusal, resumption and rollback using the owning store/kernel paths.

A rollback selects original weights **and matching original vectors**, or rebuilds those vectors. Retaining the original model file alone does not make a candidate index compatible. A Focus/Context-only switch leaves semantic vectors unchanged, but invalidates model/policy/calibration-dependent caches and any stale advice.

Bind each optional profile to merged checkpoint, graph/external data, tokenizer, compiler/task/calibration policies, legal notices, measured resource fit and qualification evidence. Keep training checkpoints/optimizer states/private examples out of installer and optional-download payloads. Optional models remain explicit downloads with independent index enrollment; MiniLM remains bundled and works when Arbiter is off.

Complete applicable scoped regressions and native transport checks before reviewed expensive runs. Final promotion/release claims still require current workspace, platform, real service/client, package, asset lifecycle and installed-product gates. Exercise complete results across CLI/session, MCP, Admin and Workbench where model results are exposed. Verify source identity, omissions, scope annotations, stop reasons, fallback and work accounting survive every presentation/export.

No local candidate, new weight file or planning approval is a public-release authorization. Public publication remains a separate action on concrete verified artifacts.

## 5. Experiment discipline, ownership and stopping rules

Use a small sequential program, not a large hyperparameter sweep. The first batch is Stage 0 plus the Stage 1 pilot; then M1, F1/F2 and C1. Teacher distillation, hard-negative refresh and learner-state aggregation follow measured residuals. M4, preference learning, wider inputs and W4 are conditional branches, not prerequisites that consume time automatically.

For each experiment, freeze before execution: hypothesis; one primary change; unchanged controls; task/split and rights hashes; exact native-visible example identities; starting checkpoint; training seed/config; selected primary metric; unchanged safety/quality floors; expected diagnostic; maximum steps and compute allowance; and stop rule. Profile a short bounded training run first, then record the projected resource envelope and cap. Training hardware and user inference requirements are separate facts; no new paid compute or cloud teacher is assumed.

Run one pilot seed during method exploration and three seeds for finalists. Select a checkpoint by the preregistered DEV rule, not the most favorable FINAL result. A learning curve should show whether more examples, better labels or optimization is the limiting factor. Stop after a failed numeric/admission check and retain diagnostics; a repeated failure requires a revised diagnosis and cheap regression before another expensive run.

Assign future owners by outcome, with explicit file ownership:

| Owner | Complete responsibility |
| --- | --- |
| Data/evaluation | Rights, source/split closure, independent labels, native alignment, leakage checks, grading and final reservation. |
| MiniLM | Training/export experiments, representation identity, U8/S8 checks, exact/ANN comparisons and regeneration/recovery evidence. |
| Focus | Prompt controls, relevance training, hard negatives, export, teacher validation and ranking/resource regressions. |
| Context | Whole-card labels, Compact/Large training, learner-state replay, terminal qualification and calibration. |
| Native integration/qualification | Single Cargo ownership, profile/ABI checks, exact runtime custody, shared budgets, actual transports, installed/package checks. |

Owners can work in parallel after the common freeze. Primary review must independently inspect the combined change before expensive native runs. Keep one Cargo owner; use the campaign's persistent target. This plan does not create another crate, state format or serving process.

A future receipt should include source commit plus dirty-input manifest, commands, elapsed time, hardware/runtime, seeds, training hyperparameters, dataset/teacher provenance, checkpoint/export hashes, selected/refused task counts, metrics/uncertainty, all failures and recovery status. Cargo receipts additionally retain scope, target directory and incremental setting. Suggested private output root: `artifacts/advanced-split/models/weights-v2/<experiment-id>/`; this is a proposed location, not a directory created by this planning task.

Reuse unchanged evidence only under its original identity. Rust owns native schemas, prompt/task projections, semantic fixtures and profile emission. Research Python can consume those emitted data; it must not become a handwritten mirror of the protocol. Extend existing [developer model tools](../../tools/retrieval-train/README.md) when implementation is authorized. Reverify their current selected-model applicability: historical four-model wording and commands are not a current-Qwen readiness guarantee.

## 6. Acceptance gates and decision rules

Preserve the relevant before-score rules in [A0 freeze](../../tools/retrieval-train/a0-freeze.json), [encoder baseline freeze](../../tools/retrieval-train/encoder-baseline-freeze.json), [goal baseline selection](../../tools/retrieval-train/goal-baseline-selection-freeze.json), and the active native contracts. Bind the software-v2 corpus and metrics in a **new additive experiment freeze** before results; do not silently edit or relabel historical protocols.

| Boundary | Existing floor to retain where applicable | Additional v2 reporting/decision |
| --- | --- | --- |
| MiniLM | Recall@10 at least 0.85; complete original support alternative at least 0.70; at least 0.03 absolute paired complete-recall gain against each unsigned control; mean recall regression at most 0.02 | Report both exports, exact and ANN/hybrid behavior, per-repository/code/document coverage and actual Focus-pool recall. Freeze equivalent software evidence-set definitions before scores. |
| Provenance | Source/citation identity precision 1.0 | No fabricated paths/spans, hidden subset or stale model/vector mixing. |
| Optional/end-task evidence | Completion at least 0.75; at most 0.02 absolute regression versus strongest baseline | Preserve critical-evidence/citation floors; separate Compact, Large, Focus-only and combined contributions. |
| Follow-up work | At least 10% mean external-turn reduction on paired common-success tasks against the selected strongest baseline | Also report all-task failures/turns, internal reads, model compute and resource cost. Empty common-success sets cannot pass. |
| Current Context fit | At least one certified positive and zero accepted UNKNOWN; only the currently supported generic-Read class | Whole-roster argmax/margin precedes acceptance. Terminal promotion requires the separately reviewed contract/fixture extension. |
| Sufficiency | Zero observed false-sufficient cases in the defined unanswerable population | Add software missing-constraint/counterevidence populations, coverage and risk bounds; unsupported is preferable to an unjustified terminal. |
| Optional approximate export | Maximum raw-logit drift 0.25, normalized-mass diagnostic drift 0.05, identical fixture argmax under the frozen admission rule | Preserve these limits; add real near-boundary decisions and native task results. Passing export admission is not task qualification. |
| Budgets/resources | Default 2048 tokens/30 s; existing qualification sample/floor and actual approved per-profile CPU/memory policy | Measure the complete cascade. 8192/120 s is a distinct unqualified configuration until its own reviewed evidence exists. |

The older A0 resource freeze describes CPU1 and smaller/larger memory floors; later selected-model work records different CPU fits and reservations. Stage 0 must inventory the **actual recorded amendments and their scope**, rather than applying an old label to a new model or treating a reservation as a passing measurement. Historical resource successes and failures remain visible. The 20-run nearest-rank p95 protocol describes those samples, not an arbitrary tail guarantee.

One documentation discrepancy needs later correction: `knownissues.md` describes a 0.02 completion **gain**, whereas the frozen baseline-selection and A0 protocols specify no more than 0.02 completion **regression**, alongside the separate 0.75 floor and 10% turns gate. This plan follows the frozen protocols and leaves the other file unchanged.

Proposed v2 statistical criteria, data sizes and terminal-risk targets are additions to be frozen before execution, not claims of existing approval or measured performance. Choose final sample sizes from repository-cluster variance and the desired detectable effect; the old minimum 100 questions/30 source documents is a floor, not adequate proof for every code/language/action slice. If power is inadequate, collect more independently held-out tasks before the one-shot final run or report the uncertainty without promotion.

The decision after every stage is one of: retain incumbent; continue a specified experiment with new evidence; repair an identified non-neural limitation; or advance a frozen candidate to the next gate. Better training loss, export success, more generous limits and a downloadable bundle are never substitutes for the relevant measured outcome.

## 7. First execution batch and planning validation

The first future batch should produce five concrete outputs: a corrected-runtime baseline with exact-search controls; a native token/window/menu census; a licensed software pilot with split/label audit; preregistered M1, F1/F2 and C1 experiment cards; and a measured small-run training/export resource estimate. This puts MiniLM effectiveness, Focus discrimination and Context utility on the same evidence foundation before spending on successive adaptations.

This planning review inspected source, tests/contracts, historical experiment records and primary online research. Historical test/model results above are attributed observations, not reruns in this task. No weights were acquired or trained, no native build or qualification ran, and no other repository file was edited by this task. Independent read-only reviews covered training research, local experiment attribution/native contracts, and Context calibration/precision. Their corrections to complementary-positive loss, full-roster abstention and current calibration/promotion scope are incorporated. The primary author checked those corrections against live source.

Validation: all 21 relative document links resolve; plan whitespace checks and git diff --check pass. The repository-wide scripts/check-repo.ps1 check stops at the pre-existing AGENTS.md/CLAUDE.md byte mismatch (their committed blobs also differ); neither file was changed. The remaining repository-wide checks therefore have no pass claim. Only docs/arbiter/weights-v2-plan.md was written by this task.
