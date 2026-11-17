# Rejected patches and the Mac audit-token probe

Two exact patch templates preserve rejected hunks and their original input identity
from the P11b/P11a composition review (four JSON pointers across two containers).
They are historical conflict evidence, not patches approved for current application.

The Python template preserves a Mac audit-token experiment embedded as a base64
literal in a retained failure traceback. It queries Mach task/audit-token APIs and
signals only a child it creates, under original hard-coded UID/host conditions.
It has not been executed during rescue. Its historical failure is not a pass.
The traceback itself and any surrounding receipt output are not copied.

`source-provenance.json` records direct JSON-pointer extraction for patches.
`encoded-source-provenance.json` uses `airk.encoded-source-migration/1`: it pins
container bytes, `/stderr` field bytes, exactly one `b64decode('BASE64')` literal,
strict base64 decoding, strict UTF-8 source, reversible SPDX prefix and maintained
bytes. The named `single-python-b64decode-literal/1` decoder never evaluates the
command. Original private containers are prerequisites for reconstruction.

Run `python -B -S scripts/arbiter/source_rescue/embedded_history/supplemental/rescue.py`
from the repository root for a read-only plan. `apply` accepts the reviewed plan on
stdin, validates every input before writing, and refuses existing provenance or
source outputs. Its caller must preflight every parent and audit every output
through the same-human execution guard. These templates require a separate current
review before any execution; host assumptions and original code remain unchanged.
