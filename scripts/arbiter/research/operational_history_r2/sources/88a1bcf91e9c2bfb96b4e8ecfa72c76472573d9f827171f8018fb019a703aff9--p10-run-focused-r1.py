# SPDX-License-Identifier: Apache-2.0
import json
import os
from pathlib import Path
import subprocess
import time

base = Path('D:/code/Github/airk')
work = base / 'artifacts/v190/p10-worktree'
cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'
steps = [
    ('store-split', 'airk-store', ['--lib', 'split']),
    ('store-funding', 'airk-store', ['--lib', 'quota::funding::tests']),
    ('store-funded-paths', 'airk-store', ['--lib', 'funded_']),
    ('core-split', 'airk-core', ['--lib', 'catalog_split']),
    ('core-partition', 'airk-core', ['--lib', 'source_policy::split']),
    ('ingest-partition', 'airk-ingest', ['--lib', 'positive_partition_']),
    ('ingest-directory-controls', 'airk-ingest', ['--lib', 'directory_inputs_require_complete_positive_subtrees_not_exact_assets']),
    ('kernel-positive-controls', 'airk-kernel', ['--test', 'source_policy', 'positive_partition_']),
    ('kernel-progressive', 'airk-kernel', ['--lib', 'progressive_']),
    ('cli-split-controls', 'airk-cli', ['--lib', 'catalog_split_control::tests']),
    ('cli-split-worker', 'airk-cli', ['--lib', 'project_worker_split::tests']),
]
receipt = base / 'artifacts/v190/p10-focused-native-r1.json'
assert not receipt.exists()
results = []
for name, package, selection in steps:
    log = base / f'artifacts/v190/p10-focused-{name}-r1.log'
    assert not log.exists()
    command = [cargo, 'test', '--locked', '--offline', '--no-default-features', '--timings', '-p', package, *selection]
    started = time.monotonic()
    with log.open('x', encoding='utf-8') as output:
        result = subprocess.run(command, cwd=work, env=os.environ.copy(), stdout=output, stderr=subprocess.STDOUT)
    body = log.read_text(encoding='utf-8', errors='replace')
    row = dict(name=name, command=command, exit_code=result.returncode, seconds=time.monotonic()-started, log=str(log),
               cargo_incremental=os.environ.get('CARGO_INCREMENTAL'), target=os.environ.get('CARGO_TARGET_DIR'),
               summaries=[line for line in body.splitlines() if line.startswith('test result:')])
    results.append(row)
    receipt.write_text(json.dumps(results, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(row), flush=True)
    if 'could not compile' in body:
        print('Compile failure: stop the dependent queue and retain all diagnostics.', flush=True)
        break
raise SystemExit(0 if len(results)==len(steps) and all(row['exit_code']==0 for row in results) else 1)
