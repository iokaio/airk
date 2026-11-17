# Versioned training source contract

The source map preserves original and maintained identities. Only SPDX metadata,
repository/input/output path discovery and explicit maintained dependency paths
change in this migration. Training math, data selection, source-pin refusal,
finite checks, ownership, deadlines and resource limits remain unchanged.

All executable dependencies belong to tracked source; private data and outputs
remain external. MiniLM uses AIRK_RESEARCH_MODELS for the private models tree;
Focus plan preparation requires an explicit execution root. No generated data may
be written into this source package. Old receipts are never relabeled as evidence
for new bytes. New source epochs/releases must explicitly account for changed
files before execution. Existing strict pins may block a fresh run until that
review happens; the migration must not weaken them.

Cheap tests may exercise pure decisions without models. Full tokenizer, Torch,
owned-process and actual training qualification remains separately bounded and
requires provisioned inputs and the owning execution guard.

Execution roots must be explicitly supplied and resolve beneath this repository's
artifacts tree. No tracked-source or external output-root exception is inferred.
MiniLM synthetic workers authenticate captured maintained loss bytes against an
explicit new --loss-source-sha256 argument before Torch imports, record that source
identity, and new prerequisite seals bind/pass it. Original packet pins remain
unchanged and still must pass; an old path's hash cannot authorize new source.
