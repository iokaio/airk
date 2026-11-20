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
- [Original TechQA DEV span audit](audit_techqa_dev_source_spans.py) checks
  the 77 exact publisher cases from the existing metadata census. Pass a fresh
  checked private `--output` path with pinned Windows Python `-I -B -S`.
  The audit decodes only the selected answer values and gold document text,
  projects the publisher's Unicode scalar offsets without shifting them, and
  retains aggregate literal-match, mismatch and out-of-range counts.
- [IBM Cloud support screening](audit_mtrag_cloud_metadata.py) reads the
  frozen publisher qrels and classification metadata for MTRAG-UN and
  MTRAG. Run it with checked Windows Python `-I -B -S` and no arguments.
  The script fetches the five exact small release files from the pinned
  publisher revision and emits one private JSON roster to stdout.
  It retains publisher passage pointers and conversation groups for
  answerable human questions whose standalone status is established.
  It compares exact encoded question digests and passage rosters between
  releases. It skips answer and passage texts and authenticates only the
  relevance role of the qrels. Source rights, exposure, complete menus,
  sufficient evidence and native visibility need separate review.

- [IBM Cloud source admission audit](audit_mtrag_cloud_admission.py)
  runs under checked Windows Python `-I -B -S` and emits one private metadata
  receipt to stdout. It joins the frozen screening pointers to the
  exact official passage archive, authenticates publisher annotation
  dates and compares the retained TechQA TRAIN/DEV source metadata.
  It skips corpus text and keeps document rights, complete evidence
  judgments and native visibility pending.
- [AWS documentation QA screening](audit_aws_documentation_metadata.py)
  reads the pinned publisher CSV and its exact source documents and notices.
  Run with checked Python `-I -B -S` and `--count 65 --seconds 90`.
  The private stdout receipt retains CSV row selectors, source identities
  and answer-match types. Selection prefers literal matches and rotates
  through documentation families within each match type. Formatting-based
  matches require span review. Corpus-wide rights, exposure, complete
  evidence judgments, menus and native visibility still need verification.
- [AWS source projection](audit_aws_documentation_source_projection.py)
  takes the exact private `--screen` receipt under checked Python
  `-I -B -S`. It reauthenticates selected AWS source bytes and license
  notices and retains raw byte and line coordinates for unique origins.
  Repeated literals remain ambiguous. Formatting projections and fenced
  code are explicitly flagged. The receipt also records the scope of the
  frozen exposure comparison. This is input preparation for the native
  visibility and independent evidence-set checks.
- [Repository candidate collector](collect_repository_confirmation_candidates.py)
  checks the eight immutable SWE-bench-Live MultiLang shards and reads only
  six identifier and date columns. Use the checked pinned Python and the
  provisioned PyArrow. Supply the private `--shards` directory and exact
  ContextBench `--release`, `--selection` and `--blind-history` paths.
  The private stdout roster retains 50 cases from distinct literal
  repositories after screening known aliases, base commits and source dates.
- [Repository upstream metadata audit](audit_repository_confirmation_upstreams.py)
  takes the exact sealed `--roster` under the same Python interpreter.
  It makes up to 50 GitHub metadata requests, retains redirects and other
  refusals, and reports current canonical repository, fork and license
  classifiers. Immutable commit reachability, actual issue date and
  file-specific rights remain separate.
- [Pinned repository upstream screen](screen_repository_confirmation_upstreams.py)
  takes the exact private `--roster`, preliminary `--previous` receipt, and
  pinned `--gh` executable. Run under verified same-human custody with
  provisioned PyArrow and the user's existing GitHub CLI authentication.
  It reads only public selected identity, issue chronology, commit and
  license metadata. It compares canonical lineage against the blind
  ContextBench fence and verifies the root license blob at the exact base
  commit. Per-file rights, unresolved aliases, clone overlap and human
  evidence judgments remain separate checks.

Run the pinned interpreter locally under the verified human identity. Redirect
the one-line aggregate only to a new audited private campaign path. These
checks establish metadata scope. Native visibility, rights, independent
judgments, paired comparison and product qualification have their own checks.
