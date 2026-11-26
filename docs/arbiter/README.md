# Arbiter research and attribution

Arbiter is AIRK's optional local evidence/answer capability. See the
[operator guide](user-guide.md), [implementation plan](final-plan.md) and
[execution ledger](../simplify/one-process-ledger.md) for behavior and observed
qualification. The selected publisher models, historical AIRK-trained candidates
and proposed research datasets have separate identities. Attribution is not a
claim of training admission, model quality, independent held-out evidence or
release qualification.

## Windows CUDA completion research

The development index census, source checks and follow-up Search replay
are available separately for [Bookbuilder](windows-index-research-bookbuilder.md)
and [Munarium](windows-index-research-munarium.md). The
[completion plan](completion-plan.md) records qualification and review gates.
For the Windows review, [Claude's retrieval analysis](retrieval-strategy-anthropic.md)
traces the 20 development tasks with ordinary source tools, maps each step to
today's AIRK reads and proposes how to close the gaps. It is design input,
not evaluation evidence.

## Device and precision controls

The Service page separates Focus/Context model choice from execution variants.
Each stage has **Auto / CPU** device policy and **Auto / FP32 / FP16** GPU
precision preferences, independent enablement and explicit index consent.
Selecting a model changes a draft; downloads remain separate explicit actions.

Auto may fall back to an admitted CPU variant. Native status shows requested
settings, selected execution and fallback/refusal reasons. GPU precision
preferences leave the CPU profile unchanged: requested FP16 with a CPU selection
does not mean FP16 execution, and unreported CPU precision stays unreported.

The controls and native admission/fallback machinery are implemented. The current
scientific GPU path is **Windows x64 CUDA SM86 only**. **No Arbiter GPU or FP16
model variant is qualified yet.** Implemented controls and scoped tests do not
establish GPU/model quality, memory/latency or supported-platform qualification.

Use `airkctl arbiter status` for complete native status/3 and configuration/4.
Current CLI arguments are:

| Action | Arguments after `airkctl arbiter` |
| --- | --- |
| Apply stage choices | `configure --file <request.json>` |
| Download or remove an exact variant | `provision` or `remove`, with `--expected-revision <N> --profile <SHA>` |
| Cancel or recover a stage operation | `cancel` or `recover`, with `--expected-revision <N> --stage <focus-or-context>` |
| Import a verified local bundle | `import --expected-revision <N> --profile <SHA> --directory <absolute-folder>` |
| Explicit disabled initialization | `initialize`, optionally `--expected-legacy-configuration <SHA>` |
| Explicit retained-state migration | `migrate --expected-revision <N> --expected-configuration <SHA>` |

Configure files contain the complete native `configure_device` management
request: `stage`, `expected_revision` and a nested per-stage `configuration`.
`device_policy` and `gpu_precision` are fields inside that configuration. Use the
[Rust-generated schema](../../packages/contracts-gen/arbiter.schema.json) and
actual native revisions, index IDs and variant manifests. Explicit `status-legacy`
and `--legacy` retain CPU configuration/3 and status/2 semantics; active device
state that they cannot represent is refused. The [native CLI contract](../../crates/airkctl/src/contract/CONTRACT.md#d08-additive-native-device-management-transport)
owns these flags and compatibility rules.

## FP16 development tooling

The maintained [graph exporter](../../scripts/export-arbiter-fp16.py) implements
half-precision learned weights with protected FP32 operations. Its
[contract](../../scripts/arbiter-fp16-export/CONTRACT.md) records exact source
lineages, conversion rules, attention semantics and numerical gates. Synthetic
controls validate conversion mechanics. The [conversion custody contract](../../scripts/arbiter-fp16-execution/CONTRACT.md)
binds the selected model's Windows FP32 scoring and timing evidence and the
separate native G5 prerequisites. The resulting candidate requires independent
whole-graph, numerical, backend, resource and product checks.

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
