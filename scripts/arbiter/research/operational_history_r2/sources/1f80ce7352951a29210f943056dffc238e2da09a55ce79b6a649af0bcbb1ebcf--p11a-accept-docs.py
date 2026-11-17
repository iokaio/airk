# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190')
work=base/'p10-worktree'
paths=['README.md','packages/vscode/README.md','docs/README.md','docs/releases/release-1.8.8.md']
pending='P11a awaits primary integration/review and its phase commit; it is not yet accepted.'
accepted='P11a has passed its local phase checks and is accepted for unpublished local development.'
for name in paths:
    path=work/name;text=path.read_text(encoding='utf-8');assert pending in text
    text=text.replace(pending,accepted).replace('per-file profile family reporting candidate','per-file profile family reporting')
    path.write_text(text,encoding='utf-8',newline='\n')
for name in ['docs/reply-detail.md','docs/workbench/implementation.md']:
    path=work/name;text=path.read_text(encoding='utf-8')
    old='Primary P11a integration/review and the phase commit remain pending; P11a is not yet accepted.'
    assert old in text;text=text.replace(old,accepted)
    text=text.replace('## P11a candidate:','## P11a:').replace('The unaccepted [family-reporting candidate]','The [family report]')
    path.write_text(text,encoding='utf-8',newline='\n')
path=work/'docs/architecture.md';text=path.read_text(encoding='utf-8')
text=text.replace('P11a\'s unpublished **1.8.8 candidate**', 'P11a\'s unpublished **1.8.8 working build**')
text=text.replace('P11a awaits primary integration/review and its phase commit.',accepted)
path.write_text(text,encoding='utf-8',newline='\n')
path=work/'docs/profile-family-reporting.md';text=path.read_text(encoding='utf-8')
text=text.replace('# Per-file profile family reporting — P11a candidate','# Per-file profile family reporting — P11a')
start=text.index('This is the **unaccepted P11a candidate**')
end=text.index('No new extraction profile is introduced.')
text=text[:start]+('The unpublished **1.8.8 working build** includes this locally accepted P11a feature.\n'
    'Focused native tests, Mac ARM tests, Workbench tests and actual native/CLI/console/MCP\n'
    'parity checks passed, followed by final qualification and primary review. See the\n'
    '[dated evidence and limits](research/implementation-v190.md#p11a--local-phase-checkpoint-6-november-2026).\n')+text[end:]
path.write_text(text,encoding='utf-8',newline='\n')
path=work/'docs/cli.md';text=path.read_text(encoding='utf-8')
text=text.replace('## Per-file profile metadata — P11a candidate','## Per-file profile metadata — P11a')
text=text.replace('The unpublished **1.8.8 P11a candidate**','The unpublished **1.8.8 P11a working build**')
text=text.replace('P11a is **not yet accepted**.','P11a is accepted for unpublished local development.')
path.write_text(text,encoding='utf-8',newline='\n')
path=work/'docs/research/implementation-v190.md';text=path.read_text(encoding='utf-8')
old='Its final qualification gates passed; primary integration/review and the phase\ncommit remain pending. P11a is not yet accepted; the remaining P11b–P14/deleted-source work remain pending.'
assert old in text
text=text.replace(old,'Its final qualification gates and primary review passed; P11a is accepted for\nunpublished local development. The remaining P11b–P14/deleted-source work remain pending.')
heading='### P11a — reporting qualification passed; primary integration pending, 6 November 2026'
assert heading in text;text=text.replace(heading,'### P11a — local phase checkpoint, 6 November 2026')
tail=text.index('### P11a — local phase checkpoint, 6 November 2026')
prefix,entry=text[:tail],text[tail:]
entry=entry.replace('P11a is **not yet accepted**.','P11a is accepted for unpublished local development.')
entry=entry.replace('P11a qualification has passed and awaits primary integration/review and a phase\ncommit. P11a is accepted for unpublished local development.','P11a qualification and primary review passed. P11a is accepted for unpublished local development.')
text=prefix+entry
path.write_text(text,encoding='utf-8',newline='\n')
assert (work/'AGENTS.md').read_bytes()==(work/'CLAUDE.md').read_bytes()
print('Updated current P11a acceptance statements; historical phase records preserved.')
