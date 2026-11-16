# Arbiter research attribution and data notices

AIRK credits the researchers, annotators and source authors below. These notices
cover research and evaluation provenance, including historical experiments and
candidate inputs under review. They do not assert that a candidate dataset was
admitted for training, that the selected publisher weights were trained by AIRK,
or that a model passed quality, platform or release qualification. Attribution
does not imply endorsement by any author, institution or publisher.

No raw research dataset, source corpus, questions, answers, annotations, training
pairs, private fixtures or indexed user files are included in this notice bundle.
Original model licenses and exact model provenance remain separate. AIRK's own
Apache-2.0 license does not replace any data or source-work license.

## CoSQA and CoCLR — candidate research source

Junjie Huang, Duyu Tang, Linjun Shou, Ming Gong, Ke Xu, Daxin Jiang, Ming Zhou,
and Nan Duan. 2021. *CoSQA: 20,000+ Web Queries for Code Search and Question
Answering*. ACL-IJCNLP, pages 5690–5700. [Paper and citation](https://aclanthology.org/2021.acl-long.442/).
The publisher credits Beihang University, Microsoft Research Asia's NLC group
and Microsoft STC Asia's NLP group.

The [pinned publisher declaration](https://github.com/Jun-jie-Huang/CoCLR/blob/14ebcacf9e9bc3e7109102632bc63047876f27d2/README.md)
assigns **MIT to its code** and **Computational Use of Data Agreement (C-UDA)
to its dataset**. The [unaltered CoCLR MIT text](CoCLR-MIT-LICENSE.txt) retains
Copyright (c) 2021 Jun-jie-Huang. The [unaltered C-UDA 1.0 terms](C-UDA-1.0.md)
permit computational use; redistribution of Data retains received credit and
binds recipients to C-UDA. Sections 2.2/5.5 place no restrictions on qualifying
Results, including trained models containing no more than a de minimis portion
of the Data. Section 4.1 does not warrant underlying rights. A repository MIT
license is not a blanket MIT grant over the query/code dataset. Exact source
revision, underlying-code rights, splits and native alignment remain admission
requirements. This acknowledgement makes no new-training claim for CoSQA.

## CodeSearchNet — candidate research and evaluation source

Hamel Husain, Ho-Hsiang Wu, Tiferet Gazit, Miltiadis Allamanis, and Marc
Brockschmidt. 2019. *CodeSearchNet Challenge: Evaluating the State of Semantic
Code Search*. [Paper](https://arxiv.org/abs/1909.09436).

The [pinned repository](https://github.com/github/CodeSearchNet/tree/106e827405c968597da938f6b373d30183918869)
releases tooling/documentation under MIT; [its unaltered license](CodeSearchNet-MIT-LICENSE.txt)
retains Copyright (c) 2019 GitHub. Corpus source-code licenses are recorded in
per-language `_licenses.pkl` files and require source-level review. The
publisher's 99-query human relevance annotations are for evaluation only;
mechanical function/docstring training pairs are a different population. Do not
convert those evaluation judgments into training labels. No CodeSearchNet
training admission or new trained AIRK weights are asserted here.

## ContextBench — candidate source-audit input

Han Li, Letian Zhu, Bohan Zhang, Rili Feng, Jiaming Wang, Yue Pan, Earl T. Barr,
Federica Sarro, Zhaoyang Chu, and He Ye. 2026. *ContextBench: A Benchmark for
Context Retrieval in Coding Agents*. [Paper](https://arxiv.org/abs/2602.05892v1).
Publisher research groups include Nanjing University and University College London.

The [pinned benchmark repository](https://github.com/EuniAI/ContextBench/tree/1436c28a8eb95496da4ea69ad458b9f8a8eb7d61)
has an [unaltered Apache-2.0 license](ContextBench-Apache-2.0-LICENSE.txt).
Underlying issue/repository source keeps its own revision, license and notices;
the wrapper is not a blanket Apache grant over those sources. AIRK inspected
publisher TRAIN records for source/rights alignment only. Candidate source
matches are not native admission, model training or neural scoring. No benchmark
records, issue text or repository code are bundled here.

## TechQA — original IBM dataset/corpus candidate audit

Vittorio Castelli, Rishav Chakravarti, Saswati Dana, Anthony Ferritto, Radu Florian, Martin Franz, Dinesh Garg, Dinesh Khandelwal, Scott McCarley, Michael McCawley, Mohamed Nasr, Lin Pan, Cezar Pendus, John Pitrelli, Saurabh Pujar, Salim Roukos, Andrzej Sakrajda, Avi Sil, Rosario Uceda-Sosa, Todd Ward, and Rong Zhang. 2020. *The TechQA Dataset*. ACL, pages 1269–1278.
[Paper and canonical citation](https://aclanthology.org/2020.acl-main.117/),
DOI 10.18653/v1/2020.acl-main.117. The original dataset creators are IBM Research
AI, IBM Watson, and IBM Finance and Operations; source questions came from IBM
Developer/DeveloperWorks forums with accepted answers in IBM Technotes.

AIRK audited original publisher TRAIN source/split/rights lineage only. This is
not native admission, model training or neural scoring. NVIDIA's transformed
records and the PrimeQA archive mirror retain their own provenance; an
Apache-2.0 wrapper declaration does not replace the original data license.

The original archive README licenses both the question/answer dataset and the
separate Technote corpus under **CDLA-Permissive-1.0**. The
[complete original license PDF](TechQA-CDLA-Permissive-1.0.pdf) is retained
verbatim; [official terms](https://cdla.dev/permissive-1-0/) define the grant.
Section 2 permits Use; §3.4 imposes no obligations/restrictions on qualifying
Results, with the de-minimis limit in §1.10. Publication of Data remains subject
to §3.1, including agreement access, change notices and retained provider credit.
This legal notice redistributes no TechQA questions, answers or Technote corpus.
[Legal provenance](provenance.json) pins the original publisher archive,
README declaration and PDF, separately from transformed wrapper metadata.

## QASPER and underlying research papers — retained research provenance

Pradeep Dasigi, Kyle Lo, Iz Beltagy, Arman Cohan, Noah A. Smith, and Matt Gardner.
2021. *A Dataset of Information-Seeking Questions and Answers Anchored in Research
Papers*. NAACL-HLT, pages 4599–4610. [Paper](https://aclanthology.org/2021.naacl-main.365/),
[dataset](https://allenai.org/data/qasper), [pinned metadata](https://huggingface.co/datasets/allenai/qasper/blob/fdc9d8214fbab5dd782958601db4d678e6934a54/README.md).
QASPER annotations are CC BY 4.0; each underlying paper's rights remain separate.

The historical AIRK MiniLM v2 experiment used approved QASPER TRAIN examples.
[Its unchanged full notice](../models/minilm-v2-LICENSE.txt) preserves all 36
training-paper author/title/version credits, their CC BY 4.0 terms and extraction
changes. [Training provenance](../models/minilm-v2-TRAINING-NOTICES.md),
[exact provenance records](../models/minilm-v2-provenance.json) and the
[official CC BY 4.0 text](../models/CC-BY-4.0.txt) accompany this research notice.
Those historical trained candidates are distinct from the current selected
original publisher graphs. No raw paper or annotation records are bundled.

## HotpotQA and Wikipedia — evaluation only

Zhilin Yang, Peng Qi, Saizheng Zhang, Yoshua Bengio, William W. Cohen, Ruslan
Salakhutdinov, and Christopher D. Manning. 2018. *HotpotQA: A Dataset for Diverse,
Explainable Multi-hop Question Answering*. EMNLP, pages 2369–2380.
[Paper](https://aclanthology.org/D18-1259/), [publisher and data terms](https://hotpotqa.github.io/).
The dataset and processed Wikipedia are CC BY-SA 4.0. The baseline repository's
Apache-2.0 code license does not replace those data terms. AIRK's retained
HotpotQA/Wikipedia material is evaluation-only; it did not feed the historical
MiniLM v2 training or distractor mining. No raw dataset or Wikipedia text is
bundled. Underlying article rights, attribution and adaptations must be checked and
retained with private source records; redistribution requires separate compliance.

## Requests source and documentation — historical TRAIN-only pairs

Requests contributors; upstream retained notice: **Requests — Copyright 2019
Kenneth Reitz**. Requests 2.25.1, revision
[`c2b307dbefe21177af03f9feb37181a89a799fcc`](https://github.com/psf/requests/tree/c2b307dbefe21177af03f9feb37181a89a799fcc),
Apache-2.0. Historical AIRK MiniLM v2 research paired 218 original docstrings with
their containing functions mechanically; those pairs are not independent human
relevance or sufficiency judgments. The complete original Requests LICENSE and
NOTICE are preserved inside the [unchanged historical full notice](../models/minilm-v2-LICENSE.txt).
No Requests implementation source is bundled in this research notice.
