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
- [AWS repeated-answer locations](audit_aws_repeated_answer_locations.py)
  takes the sealed AWS source projection via `--projection` under checked
  Python `-I -B -S`. It authenticates the CSV and source documents and
  emits every overlapping literal occurrence, its raw byte and line range,
  its paragraph bounds and digest, and lexical hints for independent review.
  Send stdout to a fresh private receipt. Each row retains a null publisher
  range because the CSV identifies the document and answer text only.
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

- [Repository replacement screen](screen_repository_confirmation_replacements.py)
  replays the exact original roster from the retained shards and checks
  the next two batches of 16 distinct repository candidates. Use the
  provisioned PyArrow interpreter with `-IB`, `--directory` set to the
  absolute original roster directory, and `--gh` set to the pinned GitHub
  CLI executable. The second batch (`--tranche 2`) also requires the
  sealed first receipt in that directory. Capture stdout in a fresh private
  file for each batch. Keep every refusal and preserve receipt row order.

Run the pinned interpreter locally under the verified human identity. Redirect
the one-line aggregate only to a new audited private campaign path. These
checks establish metadata scope. Native visibility, rights, independent
judgments, paired comparison and product qualification have their own checks.

The original TechQA DEV span audit also accepts `--freeze-menus` with a fresh
private output. It stores each original question ID, its ordered 50-document
menu, the digests and byte extents of those documents, and the exact publisher
range in UTF-8 bytes when verified. Refused cases remain identifiable.
The metadata supports reconstruction from the pinned archive before native capture.

- [Stage unique AWS source ranges](stage-aws-unique-source-corpus.ps1) runs
  under checked PowerShell with `-Run`. It reauthenticates the 27 source
  documents for the 41 unique cases. Only original Markdown appears under
  the private corpus root. The sibling manifest retains spans, source notices
  and selectors. The timed Compact Answer harness expects flat inputs.
- [Verify repository merge ancestry](audit_repository_premerge_topology.py)
  takes the absolute private roster directory via `--directory` and the pinned
  CLI via `--gh`, under isolated Python with bytecode disabled. The first
  receipt covers all 35 later bases. Run again with `--extend-history` against
  its sealed receipt to check missing paths in the merge history. The evidence
  covers observed Git parent edges and tree identities.

[Verify the staged AWS native reads](verify_aws_native_source_reads.py) runs
under checked Python -I -B -S with --output set to a fresh absolute campaign
directory. It pins the separate CPU service image, CLI and corpus manifest,
indexes the staged source tree in a disposable catalog, and verifies each
indexed line read and its original answer bytes against one publication.

[TechQA menu exposure](audit_techqa_menu_exposure.py) compares the frozen
76 exact DEV menus against the 388 original TRAIN gold documents and 1,514
retained source descriptors. Run checked Python -I -B -S and redirect stdout
to a fresh private receipt. The result distinguishes each answer document
from its full menu and reports missing fields independently of equality.

The [repository annotation schema audit](audit_repository_label_schema.py)
accepts the original absolute roster directory via --directory. Run the
provisioned PyArrow interpreter with -I -B. It authenticates all eight
MultiLang shards and the ContextBench archive before examining their schemas.
The [ContextBench declared-line audit](audit_contextbench_confirmation_spans.py)
uses the same arguments to produce the private 14-case selection. With that
receipt sealed, add --verify-spans --gh <absolute pinned executable> to check
the selected annotations against original public blobs. The
[strict-derived span check](audit_contextbench_derived_spans.py) takes the
same directory, reopens only the partial cases, and reports entire annotation
bodies that occur at exactly one line boundary. Both location records survive.

[Active confirmation groups](audit_active_confirmation_groups.py) takes the
private campaign root via --directory. Run checked Python -I -B -S and
capture stdout in a fresh private receipt. It links overlapping menus and
sources within each publisher pool, retaining alternate document and family
groupings, incomplete annotations and screening-only rows. These groups
support prospective sizing. They do not establish independent outcomes.

[Stage exact TechQA gold sources](stage_techqa_exact_gold_corpus.py) takes
--output pointing to a fresh checked techqa-exact-gold-source-corpus-rN
directory in the private campaign root. Precreate and audit the empty corpus
subdirectory. It rereads the authenticated publisher archive and materializes
the 71 original documents used by the 76 verified spans. The
[TechQA native line-read check](verify_techqa_native_source_reads.py) uses the
sealed r1 manifest and the common native cohort reader with checked Python
-I -B -S --output <fresh absolute private directory>. It waits for a ready
native publication, verifies each citation and source digest, and preserves
typed refusals. The publisher's full 50-document menus are separately pinned.

[Materialize the complete TechQA menus](stage-techqa-full-menus.ps1) runs
the [archive collector](stage_techqa_full_menus.py) and verifies the checked
output inventory. Use -VerifyOnly for the existing sealed r1 tree. Shared
documents have one canonical byte copy. The first exact case by ID has a
dedicated 50-document root. The [native menu pilot](verify_techqa_native_menu.py)
takes --output <fresh private campaign directory> with checked Python -I -B -S.
It lists the complete native inventory and requests file information for every
publisher menu member, comparing original hashes in publisher order.

[Check ContextBench file provenance](audit_contextbench_confirmation_rights.py)
on the six cases with exact declared spans using the provisioned PyArrow
interpreter with -I -B and the absolute --directory and --gh inputs.
[Stage checked sources](stage-contextbench-confirmation-sources.ps1) takes
that private evidence via -Receipt and a fresh private -Destination. Source
files are separated by base revision, and governing notices are staged beside
the source roots. The manifest records every withheld path.

For the six ContextBench revision roots, run
`stage-contextbench-native-roots.ps1` once, then invoke
`python -I -B -S scripts/arbiter/analysis/public_source_admission/verify_contextbench_native_source_reads.py --output <absolute-private-revision-output>`
once for each base. Use a fresh `cb-<first-eight-base-hex>-native-source-reads-r1`
directory per invocation. Each service records its own index, publication,
cited paths and source hashes, along with withheld spans and shutdown.

The single-request TechQA pre-dispatch pilot uses
`prepare_techqa_predispatch_pilot.py` to freeze the original DEV_Q002
request and 50 sources. Run
`python -I -B -S scripts/arbiter/analysis/public_source_admission/run_techqa_predispatch_pilot.py --check-only`
to verify the sealed inputs. Under the campaign's checked Windows owner,
supply `--output <absolute-fresh-campaign-pilot-directory>` to run the native
observer and retain its offered actions, prompt census and dispatch journal.

The first DEV_Q002 Answer produced zero native assessment observations.
`reconcile_techqa_predispatch_pilot.py` verifies the original r2 files and
writes the observed zero census to a fresh checked private destination.
The run ended after its eight-call budget. No compiled prompt was available.

`diagnose_techqa_focus_pair.py` replays the original DEV_Q002 pair with the
retained publisher tokenizer and original source window. It reports component
lengths, the tokenizer version and the separately observed native disposition.
Use the reviewed Python environment with tokenizers installed.
