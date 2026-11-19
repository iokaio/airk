# Artifact source inventory

This read-only utility inventories implementation candidates under explicit
repository artifacts roots for reproducibility and migration review. It never
imports, copies, moves, deletes or rewrites candidate code or model/fixture data.
It creates no output files or caches.

Run from a verified human shell under the repository execution-identity rules:

```powershell
python -B scripts/arbiter/analysis/source_inventory/inventory.py --root artifacts/advanced-split/models --root artifacts/v190/final-plan/exec-20261116T052942Z
python -B scripts/arbiter/analysis/source_inventory/tests.py
```

JSON goes to stdout. To retain it, the caller must guard the output parent before
writing and audit the resulting owner afterward. Exit0 means no observed errors
within the declared roots and selection rules. Exit1 retains permission,
traversal, file-change, size-cap or Git errors; those never become an empty pass.
`--prior-scan-note` records reported prior limitations separately from this run's
observed errors. A successful scoped run cannot erase a broader scan's errors.

The utility obtains `git rev-list --objects HEAD`, confirms types with
`git cat-file --batch-check`, and computes raw Git blob identities using the
repository's actual SHA-1 or SHA-256 object format. It also computes source
SHA-256 and hashes filename-selected tracked worktree code. Raw identity never
uses newline or encoding normalization. Classification prioritizes exact committed
blobs, exact tracked bytes, verified migration maps, separately labeled normalized
text matches, then remaining implementation **candidates**.
Remaining artifact-only duplicates receive a same-byte count and a distinct-hash
summary. No class proves that a file is active, maintained, original or disposable.
No classification authorizes automatic purge or migration.

Common source/script/build-language extensions and notebook containers are
selected. Model weights, checkpoints, datasets and other non-code suffixes are
not read. VCS internals, environments, package dependencies, targets, vendored
sources and fixture directories are explicitly pruned and listed. Directories
named models or weights are not blindly skipped: source can live beside data.
Candidates above16MiB are reported errors, not silently omitted. Symlinks and
reparse points are listed and never traversed.

Coverage is deliberately scoped. JSON receipt-embedded scripts are not extracted;
notebooks are hashed as raw containers. Per-file stat checks detect observed
changes, but a concurrently edited tree is not an atomic snapshot. A committed
match may belong to an older revision, and a tracked match may be uncommitted.
The report preserves source paths, SHA-256, Git object identities, reasons,
exclusions and errors. Human review must identify surviving owners/callers,
READMEs, reproducible inputs and tests before migration. No model, native build,
training or qualification runs are performed.


## Disposition refinement

A separately labeled normalized-text match replaces CRLF with LF in strict
UTF-8 non-NUL text and compares its Git blob identity. It never claims raw-byte
identity, does not normalize lone CR, and is not a semantic equivalence proof.
Declared source-map.json/source-provenance.json files under scripts/arbiter are
pinned and validated for the two maintained migration schemas. Original bytes,
actual maintained target SHA-256 and declared sizes must match. Declared originals
are restricted to artifacts/ or tools/retrieval-train/, and targets to
scripts/, crates/, packages/ or tools/; path escape,
map tampering and stale target bytes refuse that row and remain reported errors.
Map commitment/tracking status is reported separately: a verified local migration
map is not automatically a committed or qualified workflow.

Named ContextBench source-join receipts and the retained CosQA/CodeSearchNet
native visibility input identify corpus payload roots. Those specific source
inputs are excluded with their evidence pins; authored collectors beside them
remain candidates. Earlier corpus staging copies are excluded from migration
gaps only when their actual hash/size matches the declared input payload.
Repository-shaped snapshots and native/client proposals are separated for review,
not automatically declared obsolete. Unmapped operational/research scripts and
other unresolved source remain in an explicit migration-gap review list.

--review-limit (default200, maximum2000) bounds that distinct-SHA review list;
omitted groups and same-hash paths are counted. The full candidate list remains
available. Remaining groups report top folder, extension, disposition, path count
and unique SHA-256 count. These labels guide owner review and do not authorize
copying corpus inputs, discarding snapshots, or declaring every remaining file
inactive. Old reports are preserved; a refined run is a new observation.

## Exhaustive rescue routing

`python -B scripts/arbiter/analysis/source_inventory/disposition.py <inventory.json>`
emits all candidate rows partitioned into explicit review lanes. It never uses
only the bounded review sample. E operational Python/PowerShell/shell code goes
to the historical-source lane; model orchestration/evaluation goes to its owner;
training/conversion, native/client/UI source and public-source/upstream research
have separate lanes. These names record current coordination, not source policy.
Routing is never proof of migration: pending rows stay pending until actual
maintained destination bytes and provenance or explicit corpus/vendor evidence
are verified. Even existing Git matches still require commit/path review for a
claim about maintained ownership. Do not equate zero unassigned routing rows
with zero unrescued implementations. Historical versions may be preserved as
non-executed source with a README and hash map; never claim a fresh runtime pass.

Patch/diff files are source candidates. A terminal .in, .template or .tmpl is
recognized when the preceding filename is a known source/build filename, such
as owner.rs.in or CMakeLists.txt.in. Fixed filename selection cannot establish
absence of source embedded in arbitrary JSON, Markdown, text or extensionless
files; a separate content/provenance audit must account for those before any
all-code completion claim. Model/data JSON is not blindly treated as source.

## Explicit evidence dispositions

The approved `airk.source-disposition/1` document has `files` rows containing
source, source_sha256, source_bytes, disposition, reason and nonempty evidence
pins (path/sha256, optional bytes). `upstream_research_input` additionally names
upstream repository, revision and path; unknown provenance stays unresolved.
`generated_source_snapshot` additionally pins an exact-byte maintained scripts/
owner (path/sha256/bytes; scripts, crates, packages or tools) and retained source/build evidence. The separate
`verify_disposition_row` checks source/evidence/current-owner bytes and safe
repository-relative ancestry. Its successful result is migration evidence only,
never model qualification. The routing command accepts explicit --disposition-map PATH=SHA256 arguments, verifies their
schema and pins, and retains errors with exit1. It never silently loads a map.

## Embedded source indicator supplement

Run `python -S -B scripts/arbiter/analysis/source_inventory/embedded.py --root artifacts` from the verified human shell. JSON goes to stdout; guard any retained output. Focused controls: `python -S -B scripts/arbiter/analysis/source_inventory/test_embedded.py`.

The supplement traverses directories without target/vendor/fixture/environment name pruning, and inspects UTF-8 JSON, text and Markdown up to 2 MiB per file. It reports hashes, JSON pointers and source-syntax indicators, never private text. Inaccessible, oversized, non-UTF8 and invalid JSON inputs remain explicit gaps; VCS administrative directories and reparse points are not traversed. These heuristic candidates require human ownership review. A clean scan does not prove absence of all embedded implementation, authorize copying private data, or establish full source coverage.

`generated_from_maintained_typescript` is a distinct approved subtype: pin the
actual compiler, options, method, retained TypeScript source version, current
maintained or Git commit/path owner, independent replay receipt and reproduced
JavaScript SHA. It is not source-byte identity. Git text normalization requires
exact declared `CRLF to LF only`; unknown ownership remains unresolved.

`python -S -B scripts/arbiter/analysis/source_inventory/reconcile.py <inventory.json>`
rechecks all source bytes and current migration/disposition maps, then emits every
row with evidence or unresolved status. Prior traversal errors/exclusions remain.
Zero unclassified accessible candidates does not prove coverage of arbitrary
embedded source or inaccessible roots, nor committed status or qualification.

`python -S -B scripts/arbiter/analysis/source_inventory/byte_audit.py` validates
mapped targets and explicit exact-file attributes by comparing raw Git blobs with
`git hash-object --filters`. It writes JSON only to stdout, never the index or
object store (no `-w`). Run through the verified human execution path.

Reconciliation rehashes currently tracked owner files and re-enumerates reachable
Git blobs from the recorded HEAD. An old worktree match whose owner changed or
vanished stays unresolved unless another verified migration/Git owner covers it.
Verifier source hashes are captured at start and checked again before completion.

`python -S -B scripts/arbiter/analysis/source_inventory/historical_links.py --inventory <path> --inventory-sha256 <sha256>`
adds recorded-HEAD reachable blob/path labels. These are historical identity
locators only, not proof of a current maintained owner or commit-at-path binding.

`pwsh -NoProfile -File scripts/arbiter/analysis/source_inventory/access_inventory.ps1 -ExpectedSid <humanSID> -Depth 2`
reports shallow access/owner/ACL observations to stdout under the same-human
identity rule. It does not alter access policy or repair ownership. Denied roots
remain explicit coverage blockers even when their owner is the verified human;
readable parent ACL metadata does not prove readable child contents.

The candidate suffix roster also includes CUDA (.cu/.cuh), frontend markup/style
and component files, and workflow/schema candidates (.yaml/.yml/.toml/.proto).
These are candidates, not an assertion that every configuration file is authored
implementation. `--extension-only .cu --extension-only .cuh` selects a narrow
supplemental suffix pass (including their source-template variants), disabling
other suffix/basename selections. It preserves the same error/hash/ancestry
checks and records the actual selection; it does not replace earlier coverage.

`generated_fixture` requires an actual maintained generator pin, explicit method,
matching reproduced SHA and retained independent proof. A fixture-looking name
alone never qualifies. The verifier authenticates those observed proof records;
it does not execute the generator or turn a fixture into model qualification.

Reconciliation accepts explicit `--disposition-map <receipt.json>=<sha256>` arguments for
additional independently retained resolution receipts, applying the same schema,
source and evidence checks. `python -S -B scripts/arbiter/analysis/source_inventory/fixture_resolution.py`
replays the specific reviewed synthetic-fixture literals without executing ignored
code; consult its CLI help for inputs. It creates no qualification claim.

Broad reconciliation hashes independent candidate files with eight worker threads
(default; --hash-workers 16 permits sixteen). Each source path is freshly hashed.
Shared immutable evidence documents are hashed once when first consumed and
again before completion; any changed evidence refuses the batch. This avoids
rehashing a common multi-megabyte proof for every vendor member.

`upstream_build_input` is distinct from upstream research. It requires pinned
upstream repository/revision/member evidence plus explicit representation:
raw requires its expected source SHA; CRLF-to-LF independently recomputes the
normalized Git blob and never claims raw-byte identity. Archived tool/source
payloads are not copied into maintained implementation folders.

`delta.py --baseline <scoped.json> --inventory <broad.json> [--inventory ...]`
routes every newly observed (path, source SHA) candidate from frozen receipts into
disjoint review lanes. It is a coordination report, not a claim that every routed
file is AIRK-authored; vendor, generated and reference-input evidence must be
resolved before rescuing code. It never repeats the directory traversal.

`upstream_reference_document` records URL, content digest and an explicit
provenance-gap state with actual available evidence. Missing acquisition receipts
remain disclosed; classifying reference HTML as input does not manufacture its
provenance or turn it into AIRK implementation.

External disposition maps require an explicit lowercase SHA256 alongside the path;
no passed flag or unpinned external map is accepted. Bulky vendor/reference row
maps remain private receipts. Commit the maintained projector/verifier, compact
family rules/pinned inputs and README, not vendor payloads or large input rosters.

Evidence archives are streamed with bounded memory under an explicit4GiB cap;
source/JSON parsing retains its16MiB cap. Exact historical Git generated templates
are checked against their reachable owner, actual literal and parameter policy,
then reproduced byte-for-byte without running the output.

Private external indexed fixtures require pinned producer/input/summary evidence,
concrete roots and copy-relative paths, plus current-original comparison. Chained
copies additionally require copied_tree:true and the complete pinned origin chain.
Exact native historical inventory matches are checked where supplied. If no exact
historical hash witness exists, the explicitly authorized identity_gap states that
only source role is established by the pinned copy chain; no historical-byte or
current-match claim follows. No private fixture payload is copied into Git.

Embedded JSON migrations authenticate container bytes, RFC6901 pointer, exact
UTF-8 decoded bytes and maintained prefix/body inverse. They do not execute a
script or treat every text field as authored source. Reachable Git TypeScript
sources are verified by blob/content/HEAD reachability; historical path labels
are not invented commit-at-path proofs.

Encoded source migrations additionally authenticate the original JSON string and
allow only one strict Python b64decode single-quoted literal, decoded with validated
base64 and strict UTF-8. Unsupported methods or additional decoder calls refuse.
Historical fixture literals are limited to the two reviewed TypeScript write-call
shapes, an explicit JavaScript escape subset, an authenticated reachable Git
producer and an exact whole-output comparison. A retained non-HEAD branch requires
its actual named ref and head pin; it is not relabeled as current implementation.

Observed tool inputs require identical installed source under the verified human
profile, the pinned fixture producer and logs binding the exact private tool home.
This establishes observed input role, not an authenticated upstream release.

Directory-name exclusions are traversal selection limits. A name such as fixtures,
node_modules or vendor does not prove that every omitted file is non-authored.
Explicit source/member proofs apply only to their observed paths and bytes. Broad
coverage reports retain these exclusions and inaccessible/parse/size gaps separately
from identified missing AIRK implementation; zero classified candidates alone is
not an all-artifacts completion claim.

For encoded source retained in a raw traceback, container_format: utf8-text requires
an empty json_pointer and strict UTF-8 for the entire pinned container. JSON is the
default and still requires a field pointer. Both formats use the same strict decoder
and exact maintained-body inverse; neither executes traceback commands. Maintained
owners may be under scripts, crates, packages or tools, with identical path/reparse
checks; private artifact destinations cannot serve as maintained owners.

Reconcile compatible frozen inventories in one pass with
`reconcile.py <primary.json> --additional-inventory <extra.json> --additional-inventory <repaired.json> --hash-workers 16`.
All inputs must share repository, recorded HEAD and Git object format. The report
retains each input hash, byte length and candidate count, plus all original errors,
exclusions and prior limitations. Same-path identity conflicts refuse; exact duplicate
paths merge their observed tracked-owner candidates. No directory traversal repeats.

`pwsh -NoProfile -File scripts/arbiter/analysis/source_inventory/access_reobserve.ps1 -ExpectedSid <humanSID> -Manifest <prior-access.json> -ManifestSha256 <sha256>`
rechecks only the exact previously denied paths from a pinned access report. It
prints current readability and ownership observations without changing access or
ownership; the prior denied observations remain historical evidence.

Shared TypeScript compiler/source proof pins use the same batch cache and final
rehash as other shared evidence, retaining the 16 MiB source cap. Every output
candidate remains freshly hashed; cached input hashes are not a final pass until
the shared evidence recheck succeeds.

`observed_source_recovery` authenticates the upstream archive and selected member,
a maintained exact edit record, and all four reviewed recovery/archive helper
sources. The pure bounded recovery owner reconstructs the observed source, whose
complete SHA256 and length must match. The report labels this recoverable observed
output, not a byte-identical maintained file, AIRK authorship, historical patch
application or runtime qualification. Record commitment is checked separately in
the authorized source-rescue commit; missing original artifacts still refuse.

`generated_javascript_fixture` is limited to the reviewed extension-update fixture
owners. It authenticates the current generator, maintained AST extractor,
TypeScript parser and installed Node binary, then parses each unique generator
under a 30-second bound. Exact AST kind/output/span and the fixed TAG values must
reproduce the entire observed fixture bytes. It never runs the generator or the
fixture. This is an exact fixture-output proof, separate from historical compiled
output role observations that lack exact source-version or regeneration evidence.

A reviewed runbook source range requires exactly two declared operations: an
in-bounds byte-range extraction with its own SHA256, followed by the fixed SPDX
prefix. The verifier rechecks the entire container and exact maintained inverse.
The migration index uses the extracted body's SHA256, retaining the container pin
separately; capturing a source fragment never claims preservation of the whole
surrounding report. Malformed ranges, reordered operations or extra prefix code
refuse.

Reconciliation uses the selected 8 or 16 workers for independent disposition
proof rows as well as fresh candidate checks. Results merge in input order on
the main thread. Maps containing recovery imports, JavaScript AST fixtures or
literal/template proofs remain serial. Each row still checks its current source
and ancestry and validates its declared pins. A per-key memo loads shared proof
inputs once without serializing unrelated keys; failed loads release the key
and are never cached. All shared file evidence is freshly rehashed after workers
settle. Immutable Git commit/path cache entries retain raw blob validation and
per-row declared-pin checks.

Use `--progress` to emit phase and map boundaries plus each 1,000 completed rows
to stderr. Reports retain phase timings. This does not renew the caller's total
deadline, suppress failures or turn input-role evidence into exact reproduction.
