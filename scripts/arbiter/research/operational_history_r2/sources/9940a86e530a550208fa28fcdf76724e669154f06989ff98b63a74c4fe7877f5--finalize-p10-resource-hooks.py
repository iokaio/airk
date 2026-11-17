# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
import hashlib
import json

base = Path(__file__).resolve().parent
live = base / 'p10-worktree/crates/airk-cli/tests/combined_reads/split.rs'
handoff_path = base / 'p10-resource-hooks-handoff.json'
handoff = json.loads(handoff_path.read_text(encoding='utf-8'))
assert hashlib.sha256(live.read_bytes()).hexdigest() == handoff['base_sha256'], 'Live source changed; rebase first'
original = live.read_text(encoding='utf-8')
proposed = (base / 'p10-split-with-metrics.proposed.rs').read_text(encoding='utf-8')
helper = (base / 'p10-split-metrics.proposed.rs').read_text(encoding='utf-8')
prefix = 'crates/airk-cli/tests/combined_reads/'
patch = ''.join(difflib.unified_diff(original.splitlines(True), proposed.splitlines(True), fromfile='a/'+prefix+'split.rs', tofile='b/'+prefix+'split.rs'))
patch += ''.join(difflib.unified_diff([], helper.splitlines(True), fromfile='/dev/null', tofile='b/'+prefix+'split_metrics.rs'))
(base / 'p10-resource-hooks.proposed.patch').write_text(patch, encoding='utf-8', newline='\n')
handoff['checks'] = 'Rebased clones parsed/formatted with rustfmt only. Five mocked Python adapter tests passed after final context/cancel/budget changes. No Cargo typecheck or native launch; no tracked compiler inputs changed.'
handoff['patch_sha256'] = hashlib.sha256(patch.encode()).hexdigest()
handoff_path.write_text(json.dumps(handoff, indent=2)+'\n', encoding='utf-8')
print(json.dumps({'patch_sha256':handoff['patch_sha256'], 'patch_bytes':len(patch.encode()), 'base_sha256':handoff['base_sha256']}))
