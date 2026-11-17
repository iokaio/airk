# Historical qualification and diagnostic source

Two authored runbooks retain substantial executable command chains: the Windows
qualification plan includes a pinned-editor verification program and staged gates;
the native Help runbook includes environment import, scoped emitter execution and
consumer checks. Their original paths, selections and policy assumptions remain
historical and require a separate current review before any execution.

A third template contains only the 639-byte Windows `CreateFileW` diagnostic
fragment from an investigation. The surrounding report and observed outputs are
not copied. It expects a caller-supplied `paths` collection and is not standalone.

The provenance map records whole-container hashes/lengths and exact maintained
hashes/lengths. Runbooks are byte-identical. The diagnostic declares its exact byte
range and extracted hash followed by an SPDX prefix; no source bytes are normalized.
No source, commands, patches, installers, builds or models were executed here.

Run `python -B -S scripts/arbiter/source_rescue/runbook_history/test_identity.py`
for read-only inverse/roster/refusal controls. With original containers available,
`python -B -S scripts/arbiter/source_rescue/runbook_history/extract.py` prints the
reconstruction plan. Its `apply` mode accepts only a reviewed plan, refuses existing
outputs, and requires caller-side same-human output-parent preflight and output
ownership audit. The metadata-only source proof is not qualification of any old
command, environment adjustment or reported result.
