# SPDX-License-Identifier: Apache-2.0
import json
from pathlib import Path
base = Path('D:/code/Github/airk/artifacts/v190')
work = base / 'p10-worktree'
local = json.loads((base / 'p10-witness-correction-validation-r1.json').read_text())
neo = json.loads((base / 'p10-native-20261106-r4-witness-verification.json').read_text())
assert local['status'] == neo['status'] == 'passed' and local['source'] == neo['source']
assert len(local['steps']) == 7 and neo['passing_tests'] == 17
files = ['README.md', 'packages/vscode/README.md', 'docs/README.md', 'docs/releases/release-1.8.8.md',
    'docs/architecture.md', 'docs/source-policy.md', 'docs/index-splitting.md']
old_anchor = '#p10--frozen-qualification-passed-integration-review-pending-5-november-2026'
new_anchor = '#p10--local-phase-checkpoint-5-november-2026'
for name in files:
    path = work / name
    text = path.read_text(encoding='utf-8')
    text = text.replace(old_anchor, new_anchor)
    text = text.replace('**1.8.8 P10 candidate**', '**1.8.8 P10 working build**')
    text = text.replace('**Primary integration and review remain pending; P10 is not yet accepted or published.**',
        '**P10 has passed its local phase checks; the working build remains unpublished.**')
    text = text.replace('Primary\nintegration and review remain pending; P10 is not yet accepted or published.',
        'P10 has passed its local phase checks; the working build remains unpublished.')
    text = text.replace('Primary\nintegration and review remain pending; P10 is not yet phase-accepted or published.',
        'P10 has passed its local phase checks; the working build remains unpublished.')
    text = text.replace('Primary integration and review remain pending; P10 is not yet accepted or\npublished.',
        'P10 has passed its local phase checks; the working build remains unpublished.')
    text = text.replace('remain separate from accepted P9 history. Primary integration and review are\npending; P10 is not yet accepted or published.',
        'remain separate from accepted P9 history. P10 has passed its local phase checks;\nthe working build remains unpublished.')
    text = text.replace('**Primary integration and review remain pending; P10 is not yet accepted or\npublished.**',
        '**P10 has passed its local phase checks; the working build remains unpublished.**')
    text = text.replace('P10 is an implementation candidate in the unpublished **1.8.8 working build**.',
        'P10 is implemented in the unpublished **1.8.8 working build**.')
    text = text.replace('This guide describes that candidate behavior.', 'This guide describes that working-build behavior.')
    text = text.replace("P10's isolated **1.8.8 candidate**", "P10's **1.8.8 working build**")
    text = text.replace('The isolated [P10 splitting candidate]', 'The [P10 splitting workflow]')
    text = text.replace("P10's isolated [reviewed split candidate]", "P10's [reviewed split workflow]")
    text = text.replace('describes the P10 candidate in the', 'describes P10 in the')
    text = text.replace('implemented in the unaccepted P10 candidate', 'implemented in P10')
    text = text.replace('pending P10–P14 acceptance', 'pending P11–P14 acceptance')
    text = text.replace('P10–P14 acceptance remains pending', 'P11–P14 acceptance remains pending')
    path.write_text(text, encoding='utf-8', newline='\n')
ledger = work / 'docs/research/implementation-v190.md'
text = ledger.read_text(encoding='utf-8')
assert '### P10 — local phase checkpoint, 5 November 2026' not in text
text += '''

### P10 — local phase checkpoint, 5 November 2026

P10 passed its local phase checks for continued development in unpublished
**1.8.8**. The broad frozen R6 qualification above remains bound to `c713b469…`;
it is not relabeled as the later source or a published artifact.

Final review found that the remainder witness checked cancellation/deadline only
around an internal posting scan. The narrow correction adds a no-data checkpoint
before each shard, decoded candidate and final artifact validation. Existing
authorization and policy filtering still precede data-bearing selection; errors
refuse the lookup instead of claiming an empty remainder. Each indivisible map
read/decode remains bounded by existing value and 4,096-path shard limits.

Successor native source is
**`fadf30fbd45c92ccca0c384eb10e4802f12d1e7af47d07741c251d140c4e45f5` / 945 inputs**.
`p10-witness-correction-validation-r1.json` records seven passed Windows stages:
formatting; all 15 Store posting tests, including two deterministic cancellation
regressions; all 13 Kernel policy integration cases; the actual native-owner
cutover/funding-ack case; the actual witness beyond 256 selected-prefix files;
minimal Store/Kernel Clippy; and workspace/all-target/all-feature Clippy with
warnings denied. These focused regressions extend the earlier broad campaign;
the full workspace/latency/runtime suites were not repeated for this callback.

Mac Neo `p10-native-20261106-r4` separately passed formatting and the same 15 Store
tests plus both native witness/cutover cases: **four steps / 17 tests** on this
successor source. Collected cleanup proves `cargo clean` exit 0, target absent,
source/reports preserved and timings retained. Earlier 66/20-pass Neo batches
retain their own source identities. Independent source/test review and primary
integration review found no remaining concrete blocker in the reviewed scope.

The frozen release pair remains the earlier `c713b469…` pair; this checkpoint
does not relabel it as containing the callback correction. The final local test
installer must be rebuilt from the completed plan. Resource and platform limits
above still apply. P11–P14, deleted-source recovery, final local test installer
and contributor PR #60 assessment remain unfinished. No push, publication or
normal installation is part of this checkpoint.
'''
ledger.write_text(text, encoding='utf-8', newline='\n')
print('Updated eight phase documents; post-documentation checks and local DCO commit remain required')
