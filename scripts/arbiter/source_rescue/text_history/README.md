# Authored source retained in text artifacts

This family preserves 27 reviewed source-text inputs as 23 distinct files, plus
one Python preparation probe decoded from a traceback. These were source programs
or fragments stored with `.txt` suffixes, and one source-transfer runbook with
three substantial Python/shell code fences. They are not private input corpora.

The retained roles are:

- MiniLM parent-bag native admission, control and development proposals.
- Context native execution, FP32 storage and verification-capacity proposals.
- Focus singleton custody and owned benchmark configuration regression fragments.
- Mac public-source transfer templates, including deliberately omitted sealed payloads.
- A complete historical PR56 source-review diff and its original newline bytes.
- The source-only Mac transfer runbook, retained whole to preserve its code fences.
- A Mac preparation probe recovered from a single base64 command literal; surrounding
  failure output is not copied, and the failure is not reclassified as a pass.

`source-provenance.json` pins every original and maintained byte sequence. Only
explicit SPDX prefixes are added where absent; stripping that prefix recovers the
original bytes, including unusual historical CR/CRLF sequences. Exact duplicates
share a destination. `.in` files are uncompiled templates; omitted payloads and
original host/source/input pins make these historical sources unsuitable for direct
execution. No code or patch has been executed or applied during rescue.

`encoded-source-provenance.json` uses the explicit `utf8-text` variant of
`airk.encoded-source-migration/1`. An empty pointer selects the complete strict UTF-8
container, whose field hash is pinned before the single base64 literal is decoded.
The decoder uses strict base64/UTF-8 validation and never evaluates a command.

From the repository root, `python -B -S scripts/arbiter/source_rescue/text_history/rescue.py`
and `python -B -S scripts/arbiter/source_rescue/text_history/decode_probe.py` print
read-only reconstruction plans. Original private containers and Git are prerequisites.
Their `apply` mode accepts a reviewed plan on stdin, refuses existing outputs, and
requires external same-human parent preflight and created-output auditing. Use
`python -B -S scripts/arbiter/source_rescue/text_history/test_identity.py` for
read-only original/maintained inverse and decoder refusal controls; no recovered
program is imported. Current execution needs a separately reviewed maintained
entry point and fresh inputs/seals, not these historical outcomes.
