# SPDX-License-Identifier: Apache-2.0
"""Root-authorized launch: bind test-only evidence, then execute core queue once."""
import hashlib
import importlib.util
import os
import subprocess
import sys
from pathlib import Path

base = Path(__file__).resolve().parent
work = base / 'p10-worktree'
os.environ.update(PYTHONUTF8='1', GIT_CONFIG_GLOBAL=str(base / 'p10-owned-ignore-controls/global.gitconfig'),
                  AIRK_PRIVATE_CENSUS_BASE=str(base / 'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'))
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
spec = importlib.util.spec_from_file_location('p14_launch_builder', work / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
source = builder.source_identity()
assert source['files'] == 960
acceptance, decision = base / 'p12-local-acceptance-commit-r1.json', base / 'p13-decision-r1.json'
runtime, release, log = base / 'p12-runtime-final-r1', base / 'p12-release-runtime-r1.receipt.json', base / 'p12-release-runtime-r1.log'
seal, proof = base / 'p12-runtime-final-r1-seal.json', base / 'p14-test-only-class-seal-r2.json'
common = ['--p12-acceptance', str(acceptance), '--p12-acceptance-sha256', sha(acceptance),
          '--p13-decision', str(decision), '--p13-decision-sha256', sha(decision)]
subprocess.run([sys.executable, str(base / 'p14-generate-class-seal-r1.py'),
                '--prepared', str(base / 'p14-class-seal-generator-prepared-r1.json'),
                '--source-hash', source['sha256'], *common, '--runtime', str(runtime),
                '--release-receipt', str(release), '--release-receipt-sha256', sha(release),
                '--release-log', str(log), '--runtime-seal', str(seal), '--runtime-seal-sha256', sha(seal),
                '--output', str(proof)], check=True)
subprocess.run([sys.executable, str(base / 'p14-core-queue-r2.py'), '--source-hash', source['sha256'],
                '--source-files', str(source['files']), '--focus-pair', str(runtime), '--class-seal', str(proof),
                '--class-seal-sha256', sha(proof), *common, '--prepared', str(base / 'p14-core-prepared-r2.json'),
                '--label', 'p14-core-r2'], check=True)
