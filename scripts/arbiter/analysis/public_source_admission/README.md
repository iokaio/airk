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
- [IBM retained source grouping](audit_mtrag_retained_groups.py) runs offline
  under checked Windows Python `-I -B -S`. It joins the two frozen private
  receipts by original task, conversation, question-token digest, positive
  passage, source document and canonical URL. Its private output keeps
  connected cases together for later review. Human evidence sufficiency,
  source notices and native visibility remain open. The optional
  [original source reader](mtrag_cloud_original_sources.py) resolves the
  existing exact release pins. Acquiring files requires applicable source
  access and a checked private destination.
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

Run `audit_repository_publisher_commits.py --directory <retained-screen-directory>`
with the provisioned PyArrow interpreter to reproduce the publisher locator
check. It authenticates the eight original shards and joins only repository,
case, base and commit locator fields to the selected 50 cases and nine reserves.
All 59 publisher tree URLs identify their independently resolved base. Each
publisher commit list uses same-repository commit URLs. None includes the base,
and none of the 35 available merge OIDs appears in its case's list. The exact
per-case results are retained in `publisher-commit-links-r2.private.json` (SHA-256
`91ca6a7a507853b5bb634f952d836374052696d7a679c49a896936119a2594a7`).
File-level state and independent accepted source judgments remain unverified.

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
The [paired feasibility analysis](paired-confirmation-feasibility.md) reconciles
the exposed pilot, dependence assumptions, request weights, missing deliveries
and long-context coverage using source aggregates and hypothetical outcomes.

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

For the revised Focus profile, `diagnose_techqa_focus_pair.py --verify-1024`
also compiles the exact observed pair with the publisher reference and verifies
the complete untruncated IDs, mask, positions and left padding at 1,024.
DEV_Q002 informed this width choice and is an engineering regression case.

`contextbench_full_tree_inventory.py --gh <absolute-reviewed-gh>` verifies
the entire recursive Git tree at each of the six retained revision roots.
Run with checked Python `-I -B -S` and retain stdout in private storage.
The collector recomputes every tree object ID from upstream entry names,
modes and child IDs. Blob lengths are publisher metadata until their bytes
are materialized and authenticated. This stage only inventories revisions.

Run `python -IB scripts/arbiter/analysis/public_source_admission/verify_techqa_delivered_evidence.py` to check the original DEV_Q002 pilot Answer against its publisher span. It verifies the private source and Answer receipts, locates each literal snippet within its cited lines, and checks coverage across the returned byte intervals. Ambiguous snippets leave a missing span unresolved.

The [Rayon source audit](audit_contextbench_rayon_source.py) takes
`--archive <fresh-private-file> --audit <fresh-private-json>` in the same
checked private directory. With a verified interpreter, run under `-I -B -S`.
It retrieves one fixed public archive with no redirects or credentials,
reconstructs the preselected Git tree, checks every member against that
tree and records the original rights-review signals for each regular blob.
Its receipt precedes full-tree rights clearance and native staging.

The two smallest fd base revisions have separate whole-source engineering
fixtures. Run `contextbench_fd_sources.py` to authenticate all tar members
against the pinned recursive Git trees and inspect the applicable notices.
`stage_contextbench_fd_sources.py` verifies the copied lscolors material
against each Cargo.lock-pinned package, retains its license notices outside
the indexed roots and stages every verified source blob. Each script requires
its checked private output parents. `verify_fd_native_inventory.py` uses the
native cohort fixture to list the entire publication and verifies per-file
hashes and sizes, recording every omitted source path. These lists describe
source coverage before ranking.

`contextbench_fd_issue_inputs.py` uses the provisioned PyArrow interpreter
with `-I -B` and the checked output directories
`contextbench-fd-issue-inputs-r1/{sharkdp__fd-1394,sharkdp__fd-1162}`. It
extracts each original issue question by its exact publisher ID, binds its
source revision and file census, and preserves the original native request
budgets under the selected static 1,024 Focus profile. Audit the three
generated files per case and verify with `--check-only`. Both cases were
previously exposed. Scoring and native action menus need separate receipts.

Use the checked development runner with `-CaseId FD_1394` or `FD_1162`,
`-FocusWidth 1024` and a fresh attempt to capture each native offered-action
menu. The runner verifies all original source bytes, both license-bound tree
identities through the manifest, the frozen native inventory, the issue
request and original budgets before and after the actual Answer request.

Pass `--focus-width 1024` to verify the exposed widened pilot against the same source span. This consumes its own native Answer, admission observation, publication and settled Job receipt.

## AWS full publisher menu

The 41 retained requests share the complete `documents/` tree at publisher
revision `2d06ee9011f3192b2ff38c09f04e01f1ea9e0191`. The 27 previously
verified positive documents remain associated with their original selectors.
Keep all private data and receipts under the campaign artifact root.

1. Run `inventory_aws_natural_menu.py` using `python -I -B -S` and retain
   its JSON as `aws-natural-menu-inventory-r1.private.json`. It checks the
   complete publisher tree and binds all 41 active requests to the same menu.
2. Give `stage_aws_natural_menu_pack.py --output` a fresh checked private
   directory. It streams the revision archive, verifies each member against
   its original Git blob, and emits an inert source ZIP and per-file proof.
   The QA CSV stays outside the pack.
3. Run `map_aws_source_notices.py --output` with a fresh checked private
   receipt path. Every member retains its directory-level license and notice
   evidence, including nested terms and separate sample-code notices.
4. Run `expand-aws-natural-menu.ps1 -ExpectedSid <verified-human-SID>` to
   expand the complete original corpus into `aws-docs-full-r1`. The
   collection root corresponds exactly to the publisher's `documents/`.
   Run `verify_aws_expanded_sources.py <campaign>/aws-expanded-source-verification-r1.private.json`
   to rehash the expanded source, check the exact byte count, and verify that
   all 24 ambiguous publisher selectors are absent from the 41 active cases.
5. In the admitted CPU window, run `run_aws_publication.py --phase setup
   --output <campaign>/aws-publication-setup-r1 --not-after <UTC-cutoff>`
   using `python -I -B -S`. Verify the full tree, register a single
   collection, configure its vectors off, run the original native index job,
   and require its completed revision to equal its ready discovery publication.
   The offline setup has a four-hour ceiling. The campaign cutoff also bounds
   the owned Job. Each individual CLI command retains its 120-second maximum.
6. After verifying the output tree and setup checkpoint, run the same entry
   point with `--phase census --output <campaign>/aws-publication-census-r1
   --not-after <UTC-cutoff> --checkpoint <setup>/checkpoint.private.json
   --checkpoint-sha256 <verified-digest>`. This phase has a 20-minute ceiling.
   It authenticates the original source tree again, reopens the settled
   original control root through the native restart interface, verifies the
   same publication, and follows every file-list continuation. Each visible
   positive also receives an original-hash file-info check.
   After an interrupted census, preserve that attempt and use a fresh output
   and the same settled setup checkpoint. A failed or unsettled setup supplies
   no resume certificate. The underlying small-cohort diagnostic retains
   its original 240-second limit.

Run `test_aws_natural_menu.py` and `test_aws_publication_setup.py` for
pagination, source scope, independent phase clocks, and settlement controls.
The source census records actual source-visible paths. Semantic retrieval,
ranker offers and paired quality require separate observations.

`aws_normal_service.py` uses the signed installed Windows 1.8.206 pair.
It starts the normal service with a fresh campaign control root, verifies
its PID in the bounded Windows Job, configures vectors off, and indexes
the original sources. It stops and restarts the service, checks the same
published revision and durable scope, then requires every paginated file
list and each visible positive's file-info hash to use the complete new
service scope. The one-document pilot and the 282-file scaling guide
have their own receipts. Full mode uses the original 35,515-file tree,
a 6 GiB process and aggregate limit, four below-normal logical CPUs, an
8 GiB native store quota and a 12 GiB campaign disk reservation.
Invoke with `--mode pilot|scaling|full --output <campaign>/aws-normal-<mode>-<attempt>`
under the checked Windows procedure after resource admission. The original
small native-fixture and separate setup/census recipes above retain their
own image identities and evidence.

`techqa_normal_visibility.py` reads the pinned source-membership manifest
and published document bodies from `techqa-full-menu-corpus-r1`. It needs
the checked signed installed Windows 1.8.206 service and CLI. Use isolated
Python `-I -B -S` with `--mode pilot|full --check-only` for a read-only
source closure. The pilot uses exposed DEV_Q002. Full mode selects all 75
other menu IDs. Each named registration gets an independent exact
50-document directory and store under the one fresh D: service control.
Vectors are disabled. Each case receipt records the complete native file
inventory, publisher order, any absent paths and the original byte count
and SHA-256 for every visible member. No question or model is needed.

After checking resource admission, replace `--check-only` with
`--output <absolute-campaign>/techqa-normal-source-<mode>-rN`. Use the
same-human guard on the fresh parent and audit all descendants afterwards.
The job uses at most 2 GiB and four below-normal logical CPUs. Each index
boundary measures cluster-rounded case and control storage against the
2 GiB reservation. A failed or partial run retains its per-case evidence.
Verify the final owned Job receipt before counting the full population.

Run prepare_techqa_publisher_judgments.py with isolated Python and bytecode disabled after the 75-menu normal-source census closes. It joins each original human extractive answer range to the independent native line-range read and the identical full-menu source bytes without reopening question/answer text. The private roster retains the original ordered menu, published scalar and byte coordinates, native publication/scope, matched span digests, license notice, singleton exclusion, shared-document components and literal product overlap. Positive coverage of this one documented set can be measured prospectively. Other answers are unannotated. Cross-role family reservation, measured token widths, locked paired inputs and paired outcome measurement remain separate. No additional blind reviewer packet is generated for these documented positive sets.

Run `python -I -B -S scripts/arbiter/analysis/public_source_admission/score_techqa_answer_pair.py --exposed-control` for the DEV_Q002 one-arm comparator. The paired mode takes independently pinned `--freeze` and `--pairs` JSON files and their `--freeze-sha256` and `--pairs-sha256`. The freeze must bind the 75 original case IDs in order, publisher judgment and rubric SHA-256, exact request budget, service and CLI pins, search pipeline, and each arm's native profile-file, manifest, runtime, graph and weights. The fixture issuer is `private_service_fixture_through_cli`. Pair rows contain `case`, `fp16` and `fp32`; each arm supplies original plan, command, Answer, owned Job, ready/closed fixture, warm closure/log, dispatch observer closure/log and default search file pins. Pin the measurement file whenever the original producer emitted one. Each pin has an absolute path and SHA-256. The scorer authenticates native ordered file censuses, original framed questions, score returns, publication and delivered source bytes. It compares the two arms on the same initial request, budget, ordered sources, publication and scope. Each arm retains its actual dispatches, subsequent actions and delivered evidence. A checked terminal refusal or deadline without delivery counts as a miss, including when the sealed native observer recorded zero dispatches. If a complete supporting span was delivered before that terminal, it counts as a success. An incomplete supporting span counts as a miss. Missing process settlement, output or unresolvable source bytes keeps the pair unresolved. The installed Windows replay adapter accepts a separate `installed_windows_public_cli` freeze. Its `managed_install` uses the Windows timing collector’s original D installation specification. Retain the original `runtime_inspect` pin emitted with the command list. It requires the actual installation record digest, CLI and service pins, control directory, Focus manifest, the original case-to-index map and request SHA-256 map, plus a qualified Compact profile and nonempty calibration with independent file pins for each role. The owned child invocation is `<absolute-python> -I -B -S <absolute-collect_installed_techqa_answer.py> --freeze <path> --freeze-sha256 <sha> --case <id> --role <fp16|fp32> --directory <absolute-fresh-owned-commands>`. Freeze the interpreter and collector SHA-256, installation record and original request paths in advance. The collector reuses the existing installed timing collector’s `CurrentCLI` and source census. Supply its original freeze pin on each replay arm. The owned parent must retain its completed child argv, job settlement and the collector’s printed command list. Retain the original owned Job and all command and stdout files under its `commands` directory. Supply each arm’s ordered `commands` pin list, `request` pin and `owned` pin. The sequence is service-before, stage-before, the case’s status/list/50 file-info/pipeline calls, answer, stage-after and service-after. Use the same installed account, images, request, original native index, publication and scope throughout. The public Answer exposes delivered assessments, work and errors. Private dispatch counts are unavailable on that route. Installed scoring requires an actual qualified installed FP16 arm, an original qualified FP32 CPU arm and all 75 original pairs. The preregistered confidence procedure remains separate.

## Retained AWS source publication

The retained Windows store has one registered natural root covering 35,515
pinned publisher files. Its 41 eligible requests refer to 27 distinct
positive documents. Keep all 24 ambiguous requests excluded. Use the
same-human Windows guard and fresh private campaign outputs for these
commands. Run Python in isolated mode with bytecode disabled (`-I -B -S`).

`reinspect_aws_normal_store.py` retains the original installed service
reinspection, durable storage grant and configuration failure. Inspect its
original receipts before starting a candidate. The 8 GiB quota refusal
remains part of the evaluation.

`observe_aws_kernel_store.py --image-proof <receipt> --proof-sha <sha256>
--output <dir>` uses the native kernel example to enumerate the retained
revision. `classify_aws_kernel_omissions.py --output <json>` compares those
pages with source digests and the original ingest rules. It preserves each
absent source in the denominator. The first kernel census has an unresolved
aggregate custody discrepancy. Its classification records that limitation.

`observe_aws_activation_reads.py --mode control|activation --output <dir>`
captures file identities and hashes through `aws_kernel_entry_evidence.py`.
Activation requires the original settled no-op control digest via
`--control-sha`. It records separate native operation timings and changed
entries. Kernel results cover only the specified native calls.

`build_aws_worker_probe.py --output <dir>` requires the checked D-only
Cargo environment, the shared Windows target and a fresh prepared output
with `tmp` and `fixture` subdirectories. It checks the canceled-job
regression and stages the actual Cargo-built service, CLI and private test
starter with their build receipts. `--resume-required-pair` checks the sealed
R1 Clippy and Nextest receipts against the current native input group and
builds only the missing images under the separate 3 GiB reservation.
`aws_windows_allocation.py` reports NTFS file data allocations for the
shared target, D-only Cargo home and evidence directory, deduplicated by
volume and file ID. The builder also monitors full-volume free space.
Run `probe_aws_canceled_worker.py
--mode pilot --output <dir> --pair-proof <receipt> --pair-sha <sha256>` for
the one-file candidate service control. `test_aws_retained_source_inputs.py`
exercises the retained path against the real source-closure helper and
checks rejection after changing one file. Retained mode additionally needs
`--pilot-proof <owned.json> --pilot-sha <sha256>`. It checks the stopped
store identity and uses the original publication. Its outputs distinguish
startup, public pagination, source-range verification, storage changes
and job settlement. `--after-selection-busy` uses the sealed R2 stop,
original scope and subsequent store identity. `--two-lifetimes` on a pilot
uses three original documents and records native paginated lists and file
hashes before and after a normal stop/restart. `--after-renewed-grant`
uses this pilot proof and the sealed R3 stop. The public catalog selection
must match across lifetimes. A live same-request replay must retain its
full grant fingerprint. Each public read follows
only the native typed same-request directive, up to 41 attempts within
its original 120 seconds. Every attempted command retains separate
input and output evidence.

`--after-native-status` reopens the sealed R4 stop. The maintained
`observe_aws_public_gates.py` records the actual full-status result,
27 positive-file observations, 41 source-range outcomes and pagination
independently. Pagination requires a single grant, revision, bounded
unique cursors and a terminal native page. It reports any difference
from the earlier 35,400-file native inventory and preserves the complete
35,515-file source denominator. `test_aws_public_gates.py` replays
original native controls for revision drift and independent gate
failures. These source-only observations contain no Answer comparison.

The kernel census accepts `--from-retained-r5` with the original native
image proof to authenticate the stopped R5 store, deny SQLite writes and
require a byte-identical file identity/content witness before and after.

`observe_aws_guide_prefixes.py --mode pilot --output <dir>` reads six
original documents at their nested paths under three original guide
directories. It exercises one-result pages and two naturally settled
service lifetimes. Full mode takes `--kernel-proof`, `--kernel-sha`,
`--pilot-proof`, `--pilot-sha` and the R5 `--activation-sha`. It requires
the unchanged stopped publication and the complete authenticated kernel
listing. `aws_guide_partitions.py` groups whole first-level guides into
bounded service lifetimes and uses literal directory prefixes. Every
guide keeps a separate query, cursor chain and terminal result.
`test_aws_guide_partitions.py` checks disjoint names, complete assignment
and refusal of a guide larger than a bundle. The private union receipt
compares the live results with every kernel-visible path on the same
revision. Previous single-cursor and status results remain separate.
