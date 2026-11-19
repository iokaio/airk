# Public source admission recipes

These scripts reconstruct source identity, publisher TRAIN membership, provenance and rights evidence before proposed native visibility or training. They read retained metadata and may acquire original commit-bound public files; they never confer model quality or product release approval.

The root contextbench-owning-gold.py and contextbench_source_join variants bind publisher TRAIN annotations to original repository commits, files and spans; they preserve independent human-gold/source distinctions.

- [contextbench-current-blind-train-r1](contextbench-current-blind-train-r1/README.md): Audit the 450-source blind TRAIN cohort.
- [cosqa-source-join-r1](cosqa-source-join-r1/README.md): Join exact CoSQA labeled literals to CodeSearchNet Python TRAIN, collect original repository grants, verify and stage public sources, and prepare native visibility.
- [f2-code-pilot-applicability-r1](f2-code-pilot-applicability-r1/README.md): Project CSN metadata and propose a source/label applicability cohort; contains a known failed draft.
- [split-clone-chronology-r1](split-clone-chronology-r1/README.md): Audit split and clone chronology against retained evidence.
- [techqa-original-publisher-archive-r2](techqa-original-publisher-archive-r2/README.md): Boundedly parse original TechQA publisher archives and audit development document IDs.

These are historical source recipes, preserved byte-for-byte except declared SPDX prefixes. Many execute filesystem/network work at import and bind old private paths; do not import or run them as relocated commands. A fresh adapter must supply guarded private input/output roots, reviewed source/runtime seals and the original admission checks. No network, model or historical native job ran during this capture.

The family-root source-provenance.json maps original and maintained SHA-256 values. Original evidence remains in artifacts. No corpus payloads, weights, credentials, labeled examples or source-evidence copies were included. Source preservation grants no rights, training, publication or product qualification.

## Context FP16 confirmation-source censuses

- [ContextBench blind metadata census](contextbench_blind_metadata_census.py)
  checks the retained full archive and original TRAIN ID recipe. It projects
  identifier and repository metadata and reports aggregate overlap and alias
  indicators without reading question or evidence columns.
- [TechQA validation metadata audit](audit_techqa_validation_metadata.py)
  verifies the original archive and streams only selected metadata from its
  validation members. It reports question/menu/corpus agreement and exposure
  against previous DEV and TRAIN gold source IDs without decoding answer text.
  The --original-dev mode audits the larger publisher DEV member and retains
  the shared corpus member header. The --original-dev-corpus mode also
  scans the exact pinned shared corpus and verifies the DEV menu document
  keys and their embedded IDs. The `--original-dev-dependencies` mode uses
  the same exact corpus check and emits an independent aggregate receipt for
  answerable DEV cases with ordered offsets and complete menus disjoint from
  TRAIN gold document IDs. It reports duplicate menu and gold-document counts,
  shared-document case pairs, and connected component sizes. A component joins
  cases transitively through shared exact document IDs. It measures menu
  dependencies and leaves product families, clones and statistical power open.
  All modes skip publisher question, answer and document text values.

Run the pinned interpreter locally under the verified human identity. Redirect
the one-line aggregate only to a new audited private campaign path. These
checks establish metadata scope. Native visibility, rights, independent
judgments, paired comparison and product qualification have their own checks.
