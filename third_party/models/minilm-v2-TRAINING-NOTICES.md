# AIRK 1.9 trained MiniLM v2 candidate provenance

This record describes the trained v2 candidate assets. Their source and license
closure does not establish held-out quality, supported platforms or release
qualification. The original v1 model notice and identities remain historical
compatibility records.

Base model: sentence-transformers/all-MiniLM-L6-v2, revision
`10dbd2f06a8baf40ad285b037edda610c1b9a57c`, under the publisher's Apache-2.0
declaration. [Exact publisher source](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/tree/10dbd2f06a8baf40ad285b037edda610c1b9a57c).
AIRK changed encoder weights by contrastive training and exported explicit mixed
precision U8 and S8 variants. The tokenizer is unchanged. FP32 embeddings and six
FFN output projections are retained; the other 30 of 36 constant linear weights
are quantized. The selected checkpoint is epoch 1, chosen on development data.
Final held-out questions were not used for checkpoint selection or weight updates.

The 315 positive training pairs comprise 97 question/evidence pairs from 69
QASPER questions and 218 mechanical original Requests docstring/function pairs.
Unjudged distractors are not human judgments of irrelevance. Documentation pairs
are not human context-role, sufficiency or AIRK action annotations.

QASPER attribution: Pradeep Dasigi, Kyle Lo, Iz Beltagy, Arman Cohan, Noah A.
Smith and Matt Gardner (2021), *A Dataset of Information-Seeking Questions and
Answers Anchored in Research Papers*, QASPER v0.3, CC BY 4.0.
[Dataset](https://allenai.org/data/qasper),
[pinned dataset metadata](https://huggingface.co/datasets/allenai/qasper/blob/fdc9d8214fbab5dd782958601db4d678e6934a54/README.md),
[license](https://creativecommons.org/licenses/by/4.0/).
Only the approved publisher training portion fed weight updates and distractor
mining. Underlying paper rights were checked separately from the annotation
license. The 36 training-paper authors, exact titles, arXiv version URLs, CC BY
4.0 terms and extraction changes appear in the accompanying full notice and
machine-readable provenance. QASPER/S2ORC extraction normalized text; AIRK
excluded quote-bearing paragraphs and non-text figures/tables and retained
original human alternatives. The source authors and dataset creators do not
endorse AIRK.

Requests 2.25.1, revision `c2b307dbefe21177af03f9feb37181a89a799fcc`,
Apache-2.0. [Exact source](https://github.com/psf/requests/tree/c2b307dbefe21177af03f9feb37181a89a799fcc).
Retained upstream notice: Requests; Copyright 2019 Kenneth Reitz.
AIRK mechanically associated original docstrings with their containing original
functions for training only. No Requests implementation source is bundled with
the trained weights or this notice.

The full [minilm-v2-LICENSE.txt](minilm-v2-LICENSE.txt) is byte-identical to the
already sealed model LICENSE: SHA-256
`9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd`.
It retains the base Apache license, all 36 paper attributions, training change
notice, and complete Requests LICENSE and NOTICE. The separate
[CC-BY-4.0.txt](CC-BY-4.0.txt) retains the official Creative Commons legal text.
[minilm-v2-provenance.json](minilm-v2-provenance.json) records exact model,
checkpoint, corpus, training-row, source-work and license identities without raw
dataset examples or local/private paths. These companion files must accompany
distribution of the v2 trained weights; they do not change the sealed six-file
model archives or their asset hashes.

HotpotQA/Wikipedia fixtures were reserved for evaluation and did not feed these
weight updates or mining. Raw source copies, questions, answers, evidence,
training pairs, local indexes and live state remain private and are not part of
the distributable notice bundle. Public benchmark exposure in upstream model
pretraining remains unknown; no uncontaminated-pretraining claim is made.
