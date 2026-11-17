# SPDX-License-Identifier: Apache-2.0
"""Independent read-only final F06 payload/source check; no native execution."""
import hashlib
import importlib.util
import json
import re
from pathlib import Path

repo = Path.cwd().resolve()
artifacts = repo / 'artifacts/optimizations'
location = json.loads((artifacts / 'f06-authority-runtime-location.json').read_text())
runtime = Path(location['runtime']).resolve(strict=True)
task = Path(location['task']).resolve(strict=True)
assert runtime.parent == task and runtime.name == 'runtime'
assert not runtime.is_relative_to(repo) and not repo.is_relative_to(runtime)
assert task.name == 'airk-optimizations-f06-authority-fnw0nvde'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


execution = json.loads((artifacts / 'f06-authority-runtime-build-execution.json').read_text())
gate = json.loads((artifacts / 'f06-n-cli-authority-corrected-execution.json').read_text())
extension = json.loads((artifacts / 'f06-extension-e-authority-corrected-execution.json').read_text())
clippy = json.loads((artifacts / 'f06-clippy-authority-corrected-execution.json').read_text())
for receipt in (execution, gate, extension, clippy):
    assert receipt['exit_code'] == 0 and receipt['source_unchanged']
assert execution['builder_sha256'] == digest(repo / 'scripts/build-service-runtime.py')
assert extension['AIRK_TEST_BINARY'] == 'unset'
inventory = json.loads((runtime / 'airk-install.json').read_text())
pair = json.loads((runtime / 'airk-runtime.json').read_text())
build = json.loads((runtime / 'airk-build.json').read_text())
assert inventory['schema'] == pair['schema'] == build['schema'] == 1
assert inventory['version'] == pair['version'] == '1.8.0'
assert inventory['os'] == pair['os'] == 'windows'
assert inventory['arch'] == pair['arch'] == 'x86_64'
assert build['runtime'] == pair and build['profile'] == 'release'
expected = inventory['files']
assert 0 < len(expected) <= 2048
actual, total = set(), 0
for path in runtime.rglob('*'):
    assert not path.is_symlink() and not path.lstat().st_file_attributes & 0x400
    assert path.resolve(strict=True).is_relative_to(runtime)
    if path.is_file() and path != runtime / 'airk-install.json':
        name = path.relative_to(runtime).as_posix()
        assert name in expected and digest(path) == expected[name], name
        actual.add(name)
        total += path.stat().st_size
assert actual == set(expected)
assert pair['airk_sha256'] == expected['airk.exe']
assert pair['embed_sha256'] == expected['airk-embed.exe']
spec = importlib.util.spec_from_file_location('f06_primary_builder', repo / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
source = builder.source_identity()
assert source == build['source'] == {'sha256': '5c4333f298c502844abb36a8eedf996ed6aa70fbbc8791876c899845024f5466', 'files': 708}
for receipt in (execution, gate, extension, clippy):
    assert receipt['source_before'] == receipt['source_after'] == source
scripts = {
    'check-service-clients.py': 'a4588c7202f3ba3aee27057bf0aa9e15dfeb627815ee61e3c8df249491d6984b',
    'check-service-pool.py': '6a35e24cb66497b722c6c252d23d7c6203dd6402be2eba25bdc935cd5102ae5f',
    'check-source-scope-original-identity.py': '02d6c9573972cb1b21aa67813c29d1effde48bd7237c6374b8b36e7a542e82ae',
    'f06-healthy-status-isolated.py': 'cbb0b8806f89fbefcee8b841f6834afa74083f10c1adf214ae025c0704c5a1cd',
}
for name, expected_digest in scripts.items():
    assert digest(task / 'scripts' / name) == expected_digest, name
assert digest(artifacts / 'f06-healthy-status-isolated.py') == scripts['f06-healthy-status-isolated.py']
log = (artifacts / 'f06-n-cli-authority-corrected.log').read_text(encoding='utf-8')
summaries = re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log, re.M)
counts = [sum(int(row[i]) for row in summaries) for i in range(3)]
assert counts == [425, 0, 7] and len(summaries) == 30, (counts, len(summaries))
record = {'primary_verification': 'passed', 'runtime': str(runtime),
          'install_sha256': digest(runtime / 'airk-install.json'),
          'main_sha256': pair['airk_sha256'], 'helper_sha256': pair['embed_sha256'],
          'native_source': source, 'inventory_files': len(actual), 'inventory_bytes': total,
          'profile': build['profile'], 'os': pair['os'], 'arch': pair['arch'],
          'n_cli_counts': {'passed': counts[0], 'failed': counts[1], 'ignored': counts[2], 'summaries': len(summaries)},
          'qualification_helpers': scripts, 'functional_native_execution': False}
output = artifacts / 'f06-primary-authority-provenance.json'
assert not output.exists(), 'Keep the original pre-execution verification record'
output.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record, indent=2))
