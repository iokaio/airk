# SPDX-License-Identifier: Apache-2.0
import json
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190');work=base/'p10-worktree'
assert json.loads((base/'p11b-final-qualification-r1.json').read_text())['status']=='passed'
assert json.loads((base/'p11b-neo-combined-verification-r1.json').read_text())['status']=='passed'
oldslug='p11b--scoped-local-qualification-passed-primary-integration-pending-6-november-2026'
newslug='p11b--local-phase-checkpoint-6-november-2026'
paths=['README.md','packages/vscode/README.md','docs/README.md','docs/releases/release-1.8.8.md',
    'docs/extraction-policy.md','docs/profile-family-reporting.md','docs/reply-detail.md',
    'docs/workbench/implementation.md','docs/architecture.md','docs/research/implementation-v190.md']
for name in paths:
    path=work/name;text=path.read_text(encoding='utf-8');text=text.replace(oldslug,newslug)
    if name=='docs/research/implementation-v190.md':
        old='qualification has passed and awaits primary integration/review and its phase\ncommit; P11b is **not yet accepted**.'
        assert old in text;text=text.replace(old,'qualification and primary review passed; P11b is accepted for unpublished\nlocal development.')
        heading='### P11b — scoped local qualification passed; primary integration pending, 6 November 2026'
        assert heading in text;text=text.replace(heading,'### P11b — local phase checkpoint, 6 November 2026')
        index=text.index('### P11b — local phase checkpoint, 6 November 2026')
        prefix,entry=text[:index],text[index:]
        entry=entry.replace('P11b is **not yet accepted**;', 'P11b is accepted for unpublished local development;')
        entry=entry.replace('P11b awaits primary integration/review and its phase commit; it is not yet\naccepted.', 'P11b qualification and primary review passed; it is accepted for unpublished local development.')
        entry=entry.replace('P11b remains unaccepted pending primary integration/review and its phase commit.', 'P11b is accepted for unpublished local development.')
        text=prefix+entry
    else:
        replacements={
            'P11b awaits primary integration/review and its phase\ncommit; it is **not yet accepted**.':'P11b is accepted for unpublished local development.',
            'The **unaccepted P11b candidate**':'The **locally accepted P11b working build**',
            'The candidate selects':'The implementation selects',
            'P11b awaits primary integration/review and its phase commit; it is not yet accepted.':'P11b is accepted for unpublished local development.',
            '## Separate P11b retention candidate':'## Separate P11b retained profiles',
            "P11b's unaccepted [versioned retention candidate]":"P11b's [versioned retained profiles]",
            'The candidate report carries':'The report carries',
            'Scoped P11b qualification passed; primary integration/review\nand the phase commit remain pending. P11b is not yet accepted.':'Scoped P11b qualification and primary review passed. P11b is accepted for unpublished local development.',
            '## P11b retained-profile candidate remains independent':'## P11b retained profiles remain independent',
            'The unaccepted [versioned retention candidate]':'The [versioned retained profiles]',
            'Scoped P11b native/release and focused Mac qualification passed; primary integration/review and its phase commit remain pending. P11b is not yet accepted.':'Scoped P11b native/release qualification, focused Mac checks and primary review passed. P11b is accepted for unpublished local development.',
            '## P11b candidate: reviewed retained-profile migration':'## P11b: reviewed retained-profile migration',
            'Primary P11b\nintegration/review and the phase commit remain pending; P11b is not yet accepted.':'P11b primary review passed; it is accepted for unpublished local development.',
            '## P11b candidate retention ownership':'## P11b retention ownership',
            'The unaccepted P11b candidate':'The locally accepted P11b implementation',
            'P11b awaits primary integration/review and its\nphase commit, not publication.':'P11b is accepted for unpublished local development; publication remains separate.',
        }
        for old,new in replacements.items():text=text.replace(old,new)
        if name=='docs/profile-family-reporting.md':
            text=text.replace('These proposed checks supplement actual native transport tests; renderer mocks\nalone do not establish CLI/MCP/Workbench parity. Full qualification and manual\nacceptance remain pending in the [implementation ledger](research/implementation-v190.md).',
                'Per-phase checks and actual native transport tests passed as recorded in the\n[implementation ledger](research/implementation-v190.md). Renderer mocks alone\ndo not establish parity. Broader P14 qualification and manual acceptance of the\nfinal test installer remain pending.')
    path.write_text(text,encoding='utf-8',newline='\n')
print('Current P11b acceptance and links updated; earlier dated records preserved.')
