# Arbiter

Arbiter is AIRK’s optional local model capability for gathering cited evidence
from indexed sources. MiniLM retrieves candidates, Focus can rerank them, and
Arbiter, the Context model, can assess what is missing and choose further reads
within the caller’s scope and budget. The MCP `answer` tool returns an evidence
packet for the calling agent to use. Gather Context is the name used in the
current design for that workflow. See the [operator guide](user-guide.md) for
setup and model controls.

## Development history

This history follows the split branches, the Answer-loop integration branches,
and `feat/arbiter-final` through `26fbdc93` (26 November 2026). Commit IDs
mark milestones within each batch.

### 1. From semantic retrieval to local evidence assessment · 28 October–15 November

AIRK 1.6.0 shipped bundled INT8 semantic retrieval (`5e0b8b3b`). The first
Arbiter concept, checkpointed on `feat/split` and revised in `a45e09ce`,
asked what the local service could do after retrieval. It proposed separate
jobs for reranking candidates and deciding whether the evidence justified
another read. The service would keep index access, deadlines and model
activation under native control.

On `feat/advanced-split`, `b6e5efc0` defined native and private Admin
contracts, and `8fa1a2d0` made Focus and Context independent stages. The
1.8.x candidates tried explicit, on-demand model downloads and index
enrollment. MiniLM supplied candidates, Qwen3-Reranker-0.6B served as Focus,
and Context offered Compact (Qwen3-1.7B) and Large (Qwen3-4B). The branch
handed off at `76d84c53`.
[Arbiter concept](concept.md) · [Split plan](../split/split-plan.md) ·
[Split contracts](../split/contracts.md) · [Candidate issues](knownissues.md)

### 2. Model preparation, Windows CUDA and the FP16 decision · 15–21 November

The final plan opened `feat/arbiter-final` with `b4cfb7d6` and organized
lifecycle fixes, scoped model experiments and exact-artifact qualification.
The weights work audited the rights and labels of candidate datasets and
tested adaptations against the existing models. The team retained the
publisher MiniLM weights after the replacement failed to improve the fixed
development tasks. Device policy and runtime ownership landed in `c02643ad`,
followed by conversion tooling and protected operations for FP16.

Windows Large FP32 CUDA passed its prerequisite on 18 November
(`e57f34a7`). Compact FP16 measurements then traced numerical differences
through projection and attention. On 20 November, the user chose FP16 weight
delivery for CPU and GPU, with FP32 arithmetic for the selected CPU fallback
recipe, and made GPU use opt-in (`0b5fe97c`, `eaca06d5`). The 100-question
calibration capture and the TechQA experiments remained useful engineering
records. The product decision moved to real software tasks because those
experiments could not show whether Answer helped an agent finish its work.
[Final plan and decisions](final-plan.md#1-recorded-authority-and-decisions) ·
[Weights experiment plan](weights-v2-plan.md) ·
[FP16 exporter contract](../../scripts/arbiter-fp16-export/CONTRACT.md) ·
[Evaluation redirection](final-plan.md#evaluation-direction-21-november-2026-americadenver)

### 3. One-turn Answer exposed the need for follow-up · 21–22 November

The revised plan (`90c97ddd`) compared Answer, Search and Parse on coding,
writing, testing and architecture tasks from AIRK and Bookbuilder. A correct
finish required the agent to receive the essential evidence and include it
correctly in its brief. On the ten AIRK baseline tasks, Search finished two
correctly and Answer finished zero. Even 100 results from the full task
instruction reached the required region for only 5 of 53 essential notes.
For 35 notes, the file itself was absent. Bookbuilder's baseline also ended
with zero correct finishes across all paths.

The first change sets tried wider evidence and different Search and Focus
settings. The turn logs showed that Search agents recovered facts through
further searches and file reads. Answer mainly read around its first hits,
rarely pursued definitions, callers or tests, and could lose evidence while
packing the reply. On 22 November the user directed Answer to do the
investigative follow-up itself. The one-turn work was archived
(`179bb1da`), and the loop plan took over (`4f103d0e`).
[Evaluation plan](final-plan-fp16.md) ·
[One-turn baseline](answer-evaluation/01-baseline.md) ·
[One-turn change sets](answer-evaluation/02-change-set-1.md) ·
[Second change set](answer-evaluation/03-change-set-2.md)

### 4. The local loop and the Large model decision · 22–24 November

Answer /3 split compound tasks into parts, planned what each part needed,
retrieved and read evidence, checked coverage, followed leads, and composed
and united the results. Work across the loop branches addressed deadlines,
model batching, evidence identity, client parity and service-owned jobs.
Munarium replaced AIRK in the evaluation, with ten new scenarios alongside
Bookbuilder's ten. The integration included `loop/lh` and the
`loop/l2c` merge (`38166936`). The parallel
`codex/ui-arbiter-integration` branch checked the unified Workbench against
native Answer at `d2b15aae`.

On the same Windows CUDA build and indexes, the three model arms each
finished zero of 20 tasks correctly (`c347dd7a`). Compact with Focus
delivered 21 of 113 essential notes in its first replies, Large with
Focus delivered 20, and Focus alone delivered 17. Large lost its only
decisive comparison with Compact and was slower at the median.
Compact also missed its acceptance bar against Focus alone. The user
retired Large, renamed Compact to Arbiter, and kept separate switches
for Focus and Arbiter (`360178c5`), allowing all four configurations.
[Loop plan](final-plan-fp16.md) · [Loop change log](answer-loop-changes.md) ·
[Windows model gate](answer-evaluation/04-gate-lg.md) ·
[UI unification plan](../design/ui-review-unification-plan.md)

### 5. Search and index experiments located where evidence was lost · 24–26 November

The first judged Windows loop baseline (`ed039529`) ran 100 sessions on
Bookbuilder and Munarium. Every path finished zero tasks correctly, and
every Answer model configuration took more tool turns on average than
Search in both repositories. Some Arbiter replies stopped as covered
while missing structural evidence. The first change set required a
definition, caller or governing-contract witness for those needs and
widened initial retrieval. Its controlled replay recovered eight net
essential spans in first replies. The judged sessions still produced
zero correct finishes.
[Loop baseline](answer-evaluation/05-loop-baseline.md) ·
[Loop change set 1](answer-evaluation/06-loop-change-set-1.md)

The plain-index read side was integrated in `0e5ad589`. A separate Search
series tried ranked keywords, focused semantic queries, more diverse
files, better source windows and an adaptive result count
(`02e20e98` through `0b429557`). First-reply coverage rose from 14 to 20
of 113 notes overall. Munarium rose from 9 to 17 of 52, while Bookbuilder
fell from 5 to 3 of 61. No first reply covered all the essentials for its
task. Later follow-up replays also lost two Munarium notes, so the Windows
research retained the original Search control. Wider Focus scoring could
only reorder the candidates it received.

The per-repository audits verified archived Read File results against
source bytes and tested task wording against fact-specific diagnostic
queries. They found examples where the required source was present but
the query, ranking or packed response lost it. A separate native
experiment repaired missing lexical postings and replayed the original
Window Search after verified publication (`d2553e18`). Windows CUDA
also exercised the model switches and matched client transports
(`68ff4c6a`, `49c5fa2f`).
[Indexing performance plan](../indexing/performance-plan.md) ·
[Search experiments and outcomes](search-improvement-plan.md#what-we-achieved-and-where-it-falls-short) ·
[Bookbuilder audit](windows-index-research-bookbuilder.md) ·
[Munarium audit](windows-index-research-munarium.md) ·
[Autonomous repair plan](completion-plan.md#autonomous-index-repair)

### 6. Complete evidence and the Gather Context review · 26 November

To measure how agents used evidence once delivery was assured, the Windows
experiment put every required source span in Sol's first reply. That amounted to 113 spans
across 20 tasks. The agreed bar was 12 correct finishes overall and at
least five per repository. Sol finished two Munarium tasks correctly
and zero Bookbuilder tasks, triggering review before the fresh baseline
and three change sets (`3b97a1eb`).

Separate OpenAI and Anthropic walkthroughs traced the same 20 tasks with
ordinary source tools. The design drew on how the agents started from a
visible anchor, followed identifiers from retrieved source, read the
governing branch or contract, and looked for a test or other evidence
that resolved the remaining uncertainty. The Windows Gather
Context diagnostic added scoped reads, source-derived leads and
provenance-preserving assembly. Wider ordinary-tool allowances still
produced zero correct finishes, and regrouping the same archived spans
produced two where the original presentation produced zero. The
deterministic candidate delivered the same authenticated ranges at higher
native cost. Matched transport checks passed, and the review kept the
judged baseline paused (`89a98609`, `68ccc194`, `3c646489`).
[Complete-evidence experiment](completion-plan.md#what-the-gate-needs-and-how-to-steer-toward-it) ·
[Combined walkthroughs and strategy](retrieval-strategy-combined.md) ·
[Windows review disposition](../simplify/one-process-ledger.md#26-november-2026--gather-context-windows-review-disposition)

### 7. Caller-stated needs and one Arbiter stage · 26 November

The next design combined Focus and Context in one FP16 Arbiter stage and
started judging Gather Context replies by whether they gave the caller
enough evidence to proceed (`e3c7583d`). An external Qwen3-Reranker-4B
probe tested the same frozen candidates with different queries. Whole-task
queries put 40 of 113 required passages in the first five, fixed clause
splits put 46 there, and needs written by a calling agent from the task
alone put 56 there. Evaluator-written needs reached 92 and served as a
diagnostic upper bound. Early coverage scores also gave incomplete
passages high scores, so a relevance score alone could not reliably
decide when to stop (`6749be0c`, `cfda4be6`).

The user chose one call with optional `needs`, `anchors`, `deliverable`
and `known`, and deferred quantization while the FP16 design was being
proved. Commit `26fbdc93` added those fields to Answer /3 and wired stated
needs and exact anchor lookups into the kernel. Independent jobs and
result union by need remain part of the design. In a four-task run with
both models off, supplying needs still opened only 1 of 21 required
passages. The external FP16 prototype scored and followed evidence by
need using live fixture reads and opened up to 12 of 21 (9 of 10 on
Munarium, 3 of 11 on Bookbuilder). It took 100–385 seconds per task
across measured runs. Bookbuilder's remaining misses included project
guidance, test and routing files. The experiment points to candidate
selection, follow-up reads and fewer scored pairs as the next problems
to solve before integrating the model into the service.
[Single-stage strategy](retrieval-strategy-combined.md#windows-cuda-model-comparison-and-iteration) ·
[Answer contract change](answer-loop-changes.md#73-caller-stated-needs-and-anchors-on-the-version-3-request) ·
[Prototype measurements](../simplify/one-process-ledger.md#26-november-2026--single-stage-loop-prototype-outside-the-runtime-first-results)

The [native CLI contract](../../crates/airkctl/src/contract/CONTRACT.md#d08-additive-native-device-management-transport)
and [generated schema](../../packages/contracts-gen/arbiter.schema.json) track
the management interface.

## Research credits and permitted-use boundaries

| Source | Authors and citation | AIRK research scope and rights |
| --- | --- | --- |
| CoSQA / CoCLR | Junjie Huang, Duyu Tang, Linjun Shou, Ming Gong, Ke Xu, Daxin Jiang, Ming Zhou, and Nan Duan (2021), [*CoSQA: 20,000+ Web Queries for Code Search and Question Answering*](https://aclanthology.org/2021.acl-long.442/), ACL-IJCNLP, 5690–5700. Publisher: Beihang University, MSRA NLC and STCA NLP. | A reviewed existing-byte subset supports the preregistered exploratory binary-relevance pilot; no trained-model or promotion pass is asserted. The [publisher declaration](https://github.com/Jun-jie-Huang/CoCLR/blob/14ebcacf9e9bc3e7109102632bc63047876f27d2/README.md) assigns MIT to code and C-UDA to data. Preserve underlying-code provenance and notices. |
| CodeSearchNet | Hamel Husain, Ho-Hsiang Wu, Tiferet Gazit, Miltiadis Allamanis, and Marc Brockschmidt (2019), [*CodeSearchNet Challenge: Evaluating the State of Semantic Code Search*](https://arxiv.org/abs/1909.09436). | Audited code provenance supports the private exploratory relevance pilot; no derived-model quality or promotion is asserted. The 99-query human judgments are evaluation-only; function/docstring pairs are mechanical supervision. [MIT covers tooling/docs; corpus code has separate licenses](https://github.com/github/CodeSearchNet/blob/106e827405c968597da938f6b373d30183918869/README.md#licenses), retained in language-specific `_licenses.pkl` metadata. |
| ContextBench | Han Li, Letian Zhu, Bohan Zhang, Rili Feng, Jiaming Wang, Yue Pan, Earl T. Barr, Federica Sarro, Zhaoyang Chu, and He Ye (2026), [*ContextBench: A Benchmark for Context Retrieval in Coding Agents*](https://arxiv.org/abs/2602.05892v1). | Candidate TRAIN records used for source/rights audits only; no native admission, gradient update or neural scoring is asserted. [Apache-2.0 benchmark wrapper](https://github.com/EuniAI/ContextBench/tree/1436c28a8eb95496da4ea69ad458b9f8a8eb7d61) is separate from each underlying repository's own revision, license and notices. |
| TechQA | Vittorio Castelli et al. (2020), [*The TechQA Dataset*](https://aclanthology.org/2020.acl-main.117/), ACL, 1269–1278; full authors below. | Original IBM publisher TRAIN source/split/rights audit only; no native admission, training or neural scoring. The original QA dataset and Technote corpus use CDLA-Permissive-1.0; NVIDIA/PrimeQA wrapper metadata does not replace that grant. No raw corpus is bundled. |
| QASPER | Pradeep Dasigi, Kyle Lo, Iz Beltagy, Arman Cohan, Noah A. Smith, and Matt Gardner (2021), [*A Dataset of Information-Seeking Questions and Answers Anchored in Research Papers*](https://aclanthology.org/2021.naacl-main.365/), NAACL-HLT, 4599–4610. | Retained human research/evaluation evidence and historical MiniLM v2 TRAIN. [Dataset annotations are CC BY 4.0](https://huggingface.co/datasets/allenai/qasper/blob/fdc9d8214fbab5dd782958601db4d678e6934a54/README.md); underlying paper rights are reviewed separately. All 36 historical training-paper author/title/version credits and extraction changes remain in the full original notice. |
| HotpotQA / Wikipedia | Zhilin Yang, Peng Qi, Saizheng Zhang, Yoshua Bengio, William W. Cohen, Ruslan Salakhutdinov, and Christopher D. Manning (2018), [*HotpotQA: A Dataset for Diverse, Explainable Multi-hop Question Answering*](https://aclanthology.org/D18-1259/), EMNLP, 2369–2380. | Evaluation only; excluded from historical MiniLM v2 weight updates/mining. [Dataset and processed Wikipedia: CC BY-SA 4.0](https://hotpotqa.github.io/). The [baseline tooling's Apache-2.0](https://github.com/hotpotqa/hotpot/blob/master/LICENSE.txt) is a separate code license. Preserve original article attribution and data terms. |
| Requests 2.25.1 | Requests contributors; upstream NOTICE: Requests — Copyright 2019 Kenneth Reitz. [Source revision](https://github.com/psf/requests/tree/c2b307dbefe21177af03f9feb37181a89a799fcc). | Apache-2.0 source/docstrings supplied 218 historical TRAIN-only containing-function pairs; these are mechanical pairs, not human sufficiency labels. Complete original LICENSE/NOTICE are preserved in the historical full notice. |

TechQA's canonical paper authors are Vittorio Castelli, Rishav Chakravarti, Saswati Dana, Anthony Ferritto, Radu Florian, Martin Franz, Dinesh Garg, Dinesh Khandelwal, Scott McCarley, Michael McCawley, Mohamed Nasr, Lin Pan, Cezar Pendus, John Pitrelli, Saurabh Pujar, Salim Roukos, Andrzej Sakrajda, Avi Sil, Rosario Uceda-Sosa, Todd Ward, and Rong Zhang. Original publisher
credits are IBM Research AI, IBM Watson, and IBM Finance and Operations. The
[verbatim original full CDLA-Permissive-1.0 terms](../../third_party/research/TechQA-CDLA-Permissive-1.0.pdf)
cover the original data and corpus. [Official terms](https://cdla.dev/permissive-1-0/)
permit Use (§2), leave qualifying Results unrestricted (§3.4, definition §1.10),
and retain separate publication/credit conditions for Data (§3.1). This is a
candidate source audit, not data redistribution or an admitted training cohort.

C-UDA 1.0 permits computational use, including model training, and does not
restrict qualifying Results containing no more than a de minimis portion of the
Data (§§2.2/5.5). Redistributed Data retains received credit and binds recipients
to C-UDA (§3.1); the provider does not warrant underlying rights (§4.1). These
terms are distinct from the CoCLR tooling's MIT license. The
[full retained agreement](../../third_party/research/C-UDA-1.0.md) and
[research notice](../../third_party/research/ARBITER-RESEARCH-NOTICES.md) provide
source pins and context. Availability and a citation do not complete AIRK's
rights, split, original-source revision, native alignment or human-label checks.
See the [data audit](software-v2-data-audit.md).

## Preserved notices and installer delivery

The maintained [research attribution notice](../../third_party/research/ARBITER-RESEARCH-NOTICES.md)
and [publisher legal provenance](../../third_party/research/provenance.json)
retain CoCLR/CodeSearchNet MIT, ContextBench Apache-2.0, C-UDA and the original TechQA CDLA-Permissive-1.0 terms verbatim. The unchanged
[historical MiniLM v2 training record](../../third_party/models/minilm-v2-TRAINING-NOTICES.md),
[full license/36-paper/Requests notice](../../third_party/models/minilm-v2-LICENSE.txt),
[provenance](../../third_party/models/minilm-v2-provenance.json) and
[CC BY 4.0 legal text](../../third_party/models/CC-BY-4.0.txt) remain separately
identified research records; they are not the license of the current selected
original publisher graphs.

The existing license-notices producer stages these exact pinned files in native
runtime `notices/research/` and `notices/models/`. Windows installer staging
preserves them under `notices/dependencies/research/` and
`notices/dependencies/models/`; the complete installer payload inventory binds
their bytes. Native packaging refuses missing or modified current research
notices. Old candidate receipts retain their original notice inventory; changed
legal payloads require fresh packaging and qualification.

No raw dataset, questions/answers, source snippets, labels, private fixtures or
indexed user files are bundled as research notices. Source authors, annotators
and publishers do not endorse AIRK. Dataset terms do not relicense AIRK, and
AIRK's Apache-2.0 license does not relicense those datasets or their source works.

## Ongoing work

The [completion plan](completion-plan.md) records the Windows review and the
remaining FP16 qualification. The planned Windows baseline and change sets
remain paused at that review. The [combined retrieval strategy](retrieval-strategy-combined.md)
tracks the move to one Arbiter stage, including retrieval by caller-stated
need, evidence sufficiency, and the follow-up work needed to make the prototype
fast enough for the service.

Complete this history when that work is finished and AIRK 1.9 is released.
Record the final design, the qualified platforms and model variants, and the
published release.
