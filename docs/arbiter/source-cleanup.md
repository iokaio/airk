# Retired artifact source cleanup

The user authorized removing obsolete code duplicates after rescuing working code into the repository. This document plans that cleanup. It does not authorize deleting research inputs or changing historical evidence. No files have been deleted by this plan. The root agent must review the combined changes and commit the rescue and verification transition before the first cleanup batch; another user confirmation is not required for this already authorized scope.

## Eligible files and preserved material

A candidate is one ordinary, human-owned source file under this checkout's `artifacts/` whose entire original byte sequence can be reconstructed from an exact committed replacement. Admit either byte-identical copies or an explicitly recorded, reversible SPDX prefix. A changed implementation, normalized-only match, filename match, same-size file, directory classification or source-map reference alone is insufficient. Adapted code needs a separately committed exact historical owner or reviewed reversible representation first. Record the chosen replacement commit and raw Git blob identity, not just its current working-tree path.

Preserve training examples, datasets, weights, checkpoints, calibration/reference inputs, native indexes, private copied/indexed sources, vendor distributions and archives, reproduction inputs, receipts, logs, inventories and source maps. Preserve foreign-owned files and targets. Never delete a mixed JSON/log/notebook container merely because some embedded code was rescued: the surrounding receipt or input remains evidence. Duplicate upstream or private-fixture classifications are not cleanup eligibility. Do not sweep `artifacts/`, clean a Cargo target, remove folders recursively, or infer disposability from extension or age.

## Known consumers that must transition first

| Current consumer | Observed dependency | Required transition before removal |
|---|---|---|
| `scripts/cuda-native-execution.py` validation, including required helper pins | Historical `plan['ownership_helper_root']` files remain inputs even though execution loads tracked helpers | Authenticate original plan pins against a committed exact inverse plus retained pre-removal observation; maintain separate current execution-source pins. |
| `scripts/arbiter-fp16-execution.py` old-parent validation | Old `ownership_helper_root` files are reopened while checking parent evidence | Apply the same explicit historical-source verification contract; preserve old receipt identities. |
| `scripts/arbiter/research/operational_history_r2/test_preserve.py` | Inverse checks reopen each `row['source']` | Prove reconstructed committed bytes equal the original recorded SHA and byte length, and authenticate a pre-removal proof tied to that exact source path. |
| `scripts/arbiter/research/source_rescue_r1/test_rescue.py` | Reconstructed source is compared with the artifact original | Add the same retained-proof path without silently skipping absent originals. |
| `scripts/arbiter/analysis/javascript_provenance/test_capture.py` | Preserved source is compared with artifact bytes | Separate permanent committed inverse tests from historical custody verification. |
| `scripts/arbiter/export/minilm/native-parent-bag-r1/test_preparation.py` and its superseded preparation variant | Adaptation metadata's `parent_source` is reopened | Resolve the exact parent to a committed historical source with its original hash; do not redirect blindly to a behaviorally adapted owner. |
| Source-inventory migration verification and any active plan's source roster | Original paths are authenticated during reconciliation | Preserve the old inventory/map and append an explicit removal-proof join; absence without an exact removal row remains an error. |

`evaluation/research_history/test_source_identity.py` already reconstructs exact/prefix source bytes from maintained files and verifies recorded original hashes without reopening originals. This is a useful permanent-test pattern, but it does not itself prove artifact custody or authorize deletion. Embedded-source inverse tests continue to read their retained JSON containers; those containers stay in place.

This table records observed consumers, not an exhaustive dependency audit. For each proposed source path, search tracked scripts/tests, source maps, active plan inputs and maintained launchers for the exact path, basename, containing source root and dynamically assembled roots. Classify every hit as active execution, historical verification, documentation or retained evidence. A missing literal reference does not establish that a path is unused. Historical captured programs remain clearly nonoperational; changing them silently would destroy their preservation proof.

## Concrete first-pass candidates and deferrals

- `artifacts/advanced-split/models/qwen3-reranker06-experiment-r1/focus_reference.py` has a maintained Qwen source-map owner. Verify the committed owner minus its declared SPDX prefix reproduces original SHA `c6cd23cffbbf0882959100d86d82ecaf4553cac6e68ee872350691d8ece66d54`, then audit its consumers. It is a candidate, not yet an approved delete row.
- The original `reference_owned.py`, `reference_parent_no_site_r2.py` and `reference_worker.py` under `weights-v2/software-v2-independent-numeric-r1` have byte-identical maintained helpers, but the two live validation consumers above currently defer removal.
- Adapted `independent_numeric/prepare_reference.py` and `assemble_references.py` do not provide byte-identical/prefix-only inverses in their relocation map. Defer their originals unless a distinct committed historical copy or reversible representation proves all original bytes.
- Artifact code inside private fixture, vendor or reproduction-input trees remains excluded even if the source inventory can match a Git blob. Role and dependency proof are required in addition to byte identity.

## Per-file manifest and pre-removal proof

The implementation should produce a private, immutable manifest with a fresh run identifier and this explicit row shape. Place full source paths and observations in guarded private receipts; keep only the verifier and contract in Git.

```json
{
  "schema": "airk.source-cleanup-plan/1",
  "mode": "dry-run",
  "rescue_commit": "<full committed revision>",
  "human_sid": "S-1-5-21-3678370870-3858642417-826968105-1002",
  "files": [{
    "source": "artifacts/<exact file>",
    "source_sha256": "<observed original SHA256>",
    "source_bytes": 123,
    "replacement": {
      "path": "scripts/<exact committed file>",
      "commit": "<full revision>",
      "git_blob_oid": "<raw committed blob>",
      "sha256": "<raw committed SHA256>",
      "bytes": 123,
      "inverse": {"kind": "identity-or-explicit-prefix", "prefix_utf8": ""}
    },
    "source_map": {"path": "<committed map>", "sha256": "<SHA256>", "row": "<stable source identity>"},
    "observed_owner_sid": "S-1-5-21-3678370870-3858642417-826968105-1002",
    "resolved_path": "D:/code/Github/airk/artifacts/<exact file>",
    "parent_policy_observation": "<pinned guarded observation>",
    "consumer_review": "<pinned disposition of all known consumers>",
    "pre_removal_proof": "<pinned exact-original/inverse verification receipt>",
    "eligibility": "ready-or-deferred",
    "reason": "<specific evidence or blocker>"
  }]
}
```

Manifest rows are individually reviewed. Reject duplicate/case-colliding paths, path traversal, aliases/reparse points, alternate streams and roots outside the workspace. A row is ready only when the committed replacement, committed source map, byte inverse, source owner/access observations and consumer transition all pass. The immutable pre-removal receipt records both exact observed source bytes and independently reconstructed committed bytes by SHA/length, the source and replacement identities, commands, verifier identity, owner, timestamp and the reviewed manifest hash. It need not retain another executable copy of the original.

Permanent inverse tests must still fail if the committed source, declared prefix, original digest/length or receipt/map binding changes. Before cleanup, exercise both present-original and planned-removed cases using a pure observer seam; an unrecorded missing original, changed original, missing proof or mismatched replacement must fail. Never replace an equality assertion with `if exists`, skip a missing fixture, accept a new digest automatically or retag historical qualification as current success.

## Applying a reviewed batch

1. Complete and commit the rescued source plus the necessary validation contract/test changes. Verify each replacement's raw committed blob, checkout bytes and `.gitattributes` behavior. Freeze the explicit manifest and retained pre-removal proof after root review.
2. In the verified human shell, reobserve effective/default-owner/interactive-user SID and every source and parent access policy. Re-resolve each ordinary file within the authorized workspace, reject aliases and verify its current SHA/length immediately before its operation. Any drift or ownership/access discrepancy defers that file; do not repair policy as part of cleanup.
3. Remove only the single approved literal file path for each ready row, using the same PowerShell process and `Remove-Item -LiteralPath` without `-Recurse`. Do not construct a shell command from manifest strings. A deletion helper must not claim atomic confinement where the platform guards only provide preflight observations; active writer uncertainty is a refusal.
4. Record per-file success, refusal or failure with the original proof and manifest pins. Stop on an unexplained failure, retain earlier results, and diagnose before continuing. Revalidate committed replacements, preserved adjacent inputs and the relevant verification consumers. Retain all old receipts unchanged and write a new cleanup receipt; do not rewrite prior maps to pretend the original path was never used.
5. Run affected inverse tests and consumer checks. Reconcile subsequent inventories using explicit successful-removal records while retaining prior scan limitations. Report exact removed file count/bytes, deferred files and the preserved-material scope. Empty-directory cleanup is outside this first per-file batch.

The [source cleanup implementation](../../scripts/arbiter/analysis/source_cleanup/README.md) now provides a read-only planner/verifier, a guarded per-file PowerShell apply entrypoint, and pure refusal tests. The apply entrypoint has not been executed. Its committed-source preflight deliberately refuses uncommitted tooling and plans with deferred rows. Existing preservation consumers remain unchanged until the separate verification-transition milestone after the broad rescue commit. No deletion may run before combined review, committed rescue and the necessary consumer transitions. This document intentionally contains no deletion command that could run against an unreviewed wildcard or directory.
