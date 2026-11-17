# Metadata-reference projection contract

Only the four pinned families in families.json are projected. Verify all receipt
hashes/lengths before consuming their rows, then verify each exact source body and
any recorded source byte length, filename and URL. Require full declared counts and
unique source paths. Restrict source/evidence reads to repository model-input roots;
reject path escapes and sources above 16 MiB. Never execute/copy HTML or infer rights.

Output uses approved upstream_reference_document dispositions with content_sha256
identical to source_sha256 and pinned evidence. An unavailable acquisition receipt
must remain an explicit nonempty provenance_gap; companion metadata is not a receipt.
No Git revision fields are synthesized for documents. Public files contain compact
code/definitions only; full rows remain private and undergo the inventory owner's
normal per-row verification before any scope-completeness claim.
