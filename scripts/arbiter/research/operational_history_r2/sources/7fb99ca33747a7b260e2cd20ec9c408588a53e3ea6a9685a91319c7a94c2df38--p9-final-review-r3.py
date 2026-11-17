# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[2]
env = os.environ.copy()
count = int(env.get('GIT_CONFIG_COUNT', '0'))
env[f'GIT_CONFIG_KEY_{count}'] = 'safe.directory'
env[f'GIT_CONFIG_VALUE_{count}'] = root.as_posix()
env['GIT_CONFIG_COUNT'] = str(count + 1)
def git(*args):
    return subprocess.check_output(['git', *args], cwd=root, env=env, text=True).splitlines()
names = sorted(set(git('diff', '--name-only')) | set(git('ls-files', '--others', '--exclude-standard')))
assert all(not name.startswith(('temp/', 'artifacts/', '.airk/')) for name in names)
record = {
    'review': 'Primary agent post-implementation review; not independent review.',
    'native_identity': '224ee433248d09cbfb88f51a996464748b0adf9c552a4db011e930616a8b68bc',
    'boundaries_reviewed': [
        'Final audit delta: protected exact-path administrative capture, original/resolved path authorization, pin digest and epoch checks, model source adapters/helper handoff/CPU gate, calibration captured-byte installation and final digest verification. Primary review supplemented by separate read-only agent review.',
        'Final receipt delta: Store exact-key read-only proof, native configuration/job/result binding, missing/expired fail-closed behavior, supersession independence, full CLI and Workbench retention, separate successor status. Two independent agent reviews report no concrete defect.',
        'Ignore delta: process-CWD global rule base matches dependency; whole-snapshot nested failure latch; bounds and ordinary no-policy adapter preserved. Windows writable fixture handle closed without weakening source handle sharing.',
        'Canonical policy mutation and native selector preparation: source/policy/catalog revision fences, engine/configuration/job lineage, bounded impact metadata, no source mutation.',
        'Admission and recovery: actual worker drain, source watcher acknowledgement/absence proof, exact operation retry, pending source/vector eligibility, separate policy/source completion selection.',
        'Kernel/Store reads: shared epoch fence, retained publication policy identity, native path and graph endpoint predicates, imported analysis/cache retirement, conditional and scoped read identity.',
        'Retirement: pending capsule materialization before purge, stable owner capability outside immutable digest, exact current job retention, inactive FK ownership, no cleanup polling or fresh retired resume.',
        'CLI/MCP/Workbench: canonical full native status and job data, unknown impact counts, selected-scope omission, retry only on established reads, current versus exact accepted resume job distinction, full copies/exports.',
        'Platform paths: positively qualified naming boundaries, uncertain Unicode refusal, excluded ignore control refusal, actual original predecessor handlers, Windows/Neo focused regression receipts and declared qualification limits.',
        'Documentation: implemented stable capsule/owner layout and retired registration refusal reconciled with ADR 002; working version1.8.8 and no release/promotion claim.'
        ,'Later deltas reviewed: applied empty-policy observation versus pending restriction; complete actual status parity; inventory qualified discovery and epoch/cancellation callbacks; private cache digest binding; explicit unsupported-name/control errors; managed replacement/reopen and budget/source-preservation regressions.'
    ],
    'remaining_acceptance': ['Collect full Windows workspace and one frozen runtime campaign', 'Collect Neo R11 and verify cargo clean target removal', 'Synchronize accepted local phase status and final documentation/whitespace checks', 'Human DCO phase commit'],
    'files': {name: hashlib.sha256((root/name).read_bytes()).hexdigest() for name in names},
}
out = root/'artifacts/v190/p9-final-review-r3.json'
out.write_text(json.dumps(record, indent=2)+'\n', encoding='utf-8')
print(json.dumps({'files': len(names), 'receipt': str(out), 'review': record['review']}))
