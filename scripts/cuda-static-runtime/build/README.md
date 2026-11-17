# CUDA parent build source and explicit rebuild inputs

The versioned r8, r12 and checkpoint-v1 packages capture the actual producer,
continuation, complete-process-tree, checkpoint and composer implementation used
in the retained parent build path. source-map.json records all 21 source/input
mappings, original SHA256 values, maintained SHA256 values, SPDX transitions and
the original target commands. Fifteen formerly ignored files are captured here;
six already maintained acquisition/archive/verification owners are referenced.

These are historical source versions, not a claim that an old command can safely
be launched against a fresh tree. The original f9fac producer, 042c aggregate,
receipts and campaign deadline remain historical identities. New executions must
have fresh source identities, explicit inputs, output guards and approved
deadlines; they cannot be retagged as the old parent or treated as qualified.

The supported entry point is read-only:
    python -S -B scripts/cuda-static-runtime/build/rebuild.py --input-manifest PATH --manifest-sha256 SHA

It validates airk.cuda-parent-rebuild-input/1, exact ordered stages
(r8, r12, flash, llm, final, compose, aggregate), pinned maintained source,
explicit input contracts and input closure, a new future UTC cutoff and proposed
fresh output directories. It does not create directories, install tools, launch
commands, compile, sign or infer. It has no execution switch.

The manifest fields are schema, repository_root, campaign_cutoff_utc, stages,
inputs, output_directories and qualification=false. Each stage contains name,
source FilePin, input_contract FilePin and an explicit argv list. Every FilePin
has absolute path, positive integer bytes and lowercase SHA256. Do not use fake
pins or invent private fixtures. Real receipts, source inventories, downloaded
dependencies, caches and model/training data remain outside tracked source.

Fresh-run launch is still unavailable: legacy HERE-sidecar paths, fixed target
plan/runner authority, stage output names, process/output guards and historical
cutoff handling need a reviewed binding to the new manifest. This capture makes
the full implementation reviewable and available for reproduction; it is not an
end-to-end rebuild or qualification result. Do not execute historical mains as a
workaround. Current retained-parent verification uses ../verification instead.

Exact-file .gitattributes rules preserve authenticated source bytes, including
CRLF, across Git filtering and fresh checkouts. Do not normalize protected files
without deliberately updating their reviewed source/provenance identities.
