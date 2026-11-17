# SPDX-License-Identifier: Apache-2.0
"""Independent F09 final payload/source and completed native gate check."""
from pathlib import Path
import hashlib
import importlib.util
import json
import re

repo = Path.cwd().resolve()
artifacts = repo / 'artifacts/optimizations'
location = json.loads((artifacts / 'f09-runtime-location.json').read_text())
task = Path(location['task']).resolve(strict=True)
runtime = Path(location['runtime']).resolve(strict=True)
assert runtime.parent == task and runtime.name == 'runtime'
assert task.parent == Path('C:/Users/tdotn/AppData/Local/Temp').resolve(strict=True)
assert task.name.startswith('airk-optimizations-f09-')
assert not runtime.is_relative_to(repo) and not repo.is_relative_to(runtime)
assert not task.lstat().st_file_attributes & 0x400

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

execution = json.loads((artifacts / 'f09-runtime-build-execution.json').read_text())
gates = {kind: json.loads((artifacts / f'f09-corrected-{kind}-execution.json').read_text())
         for kind in ['fmt', 'clippy', 'full-n']}
gates['full-e'] = json.loads((artifacts / 'f09-final-corrected-full-e-execution.json').read_text())
for receipt in [execution, *gates.values()]:
    assert receipt['exit_code'] == 0 and receipt['source_unchanged']
assert execution['builder_sha256'] == digest(repo / 'scripts/build-service-runtime.py')
assert gates['full-e']['AIRK_TEST_BINARY'] == 'unset'
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
for path in [runtime, *runtime.rglob('*')]:
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
assert pair['airk_sha256'] != pair['embed_sha256']
spec = importlib.util.spec_from_file_location('f09_primary_builder', repo / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
source = builder.source_identity()
assert source == build['source'] == {
    'sha256': '568283bade24bee26a5f6fa24bc025e9e75a57e636f32192f6660c9b2e9af64f', 'files': 708}
for receipt in [execution, *gates.values()]:
    assert receipt['source_before'] == receipt['source_after'] == source
names = ['check-service-install.py', 'check-client-lifetime.py', 'check-managed-runtime.py',
         'check-service-console.py',
         'check-service-clients.py', 'check-service-pool.py']
helpers = {name: digest(repo / 'scripts' / name) for name in names}
helpers['check-service-console-client.cjs'] = 'c24d2630f7c3ba937d95f64d9a022cb65e724ce789b9ad078c0d31d3c1328cb5'
helpers['check-service-console-client-f09-corrected.cjs'] = '7071bd4c51d7fee4b7befdfc161eee4f69cc480b1d840273c0de17963cddcad5'
assert digest(repo / 'scripts/check-service-console-client.cjs') == helpers['check-service-console-client-f09-corrected.cjs']
helpers['f09-source-scope-port.py'] = '9c74cfe2fc291cd2c85146324b7324aea5a43c20244ae99cee09190dd03e4999'
assert digest(artifacts / 'f09-source-scope-port.py') == helpers['f09-source-scope-port.py']
helpers['f09-source-scope-native-binding.py'] = '3e36e5d8276eecff669f4b112ace696fe5767a3b418efd8b4f5ee2651cc3fd0f'
assert digest(artifacts / 'f09-source-scope-native-binding.py') == helpers['f09-source-scope-native-binding.py']
for name, identity in helpers.items():
    path = task / 'scripts' / name
    assert not path.is_symlink() and not path.lstat().st_file_attributes & 0x400
    assert path.resolve(strict=True).is_relative_to(task)
    assert digest(path) == identity, name
log = (artifacts / 'f09-corrected-full-n.log').read_text(encoding='utf-8')
assert 'test result: FAILED' not in log
summaries = re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log, re.M)
counts = [sum(int(row[i]) for row in summaries) for i in range(3)]
assert counts[0] > 0 and counts[1] == 0 and summaries
native = {}
for kind in ['host-corrected2', 'scope-corrected', 'smoke', 'assistance', 'install', 'managed', 'console', 'console-client-corrected', 'clients']:
    receipt = json.loads((artifacts / f'f09-native-{kind}-execution.json').read_text())
    assert receipt['exit_code'] == 0 and receipt['source_unchanged'], kind
    assert receipt['source_before'] == receipt['source_after'] == source, kind
    assert receipt['approved_install_sha256'] == digest(runtime / 'airk-install.json'), kind
    native[kind] = {'exit_code': 0, 'elapsed_seconds': receipt['elapsed_seconds'],
                    'log_sha256': digest(artifacts / f'f09-native-{kind}.log')}
preserved = {}
for name in ['f09-full-n', 'f09-corrected-full-e', 'f09-final-ts-focused-compile',
             'f09-native-host', 'f09-native-host-corrected', 'f09-native-scope', 'f09-native-console-client']:
    path = artifacts / f'{name}.log'
    preserved[path.name] = digest(path)
compiled = json.loads((artifacts / 'f09-console-client-out-copy.json').read_text())
assert compiled['files'] == len(compiled['files_inventory']) == 140
for directory in [repo / 'packages/vscode/out', task / 'packages/vscode/out']:
    compiled_set = set()
    for path in [directory, *directory.rglob('*')]:
        assert not path.is_symlink() and not path.lstat().st_file_attributes & 0x400
        assert path.resolve(strict=True).is_relative_to(directory.resolve(strict=True))
        if path.is_file():
            name = path.relative_to(directory).as_posix()
            value = compiled['files_inventory'][name]
            assert path.stat().st_size == value['bytes'] and digest(path) == value['sha256'], name
            compiled_set.add(name)
    assert compiled_set == set(compiled['files_inventory'])
assert sum(value['bytes'] for value in compiled['files_inventory'].values()) == compiled['bytes'] == 3083489
assert all(name.startswith('node:') for name in compiled['bare_requires'])
record = {'primary_verification': 'passed', 'runtime': str(runtime),
          'install_sha256': digest(runtime / 'airk-install.json'),
          'main_sha256': pair['airk_sha256'], 'helper_sha256': pair['embed_sha256'],
          'native_source': source, 'inventory_files': len(actual), 'inventory_bytes': total,
          'profile': build['profile'], 'os': pair['os'], 'arch': pair['arch'],
          'reported_n_counts': dict(zip(['passed', 'failed', 'ignored'], counts)),
          'reported_n_summaries': len(summaries), 'qualification_helpers': helpers,
          'native_execution_during_this_check': False, 'completed_native_gates': native, 'preserved_failure_log_sha256': preserved,
          'compiled_controller_copy_files': compiled['files'], 'compiled_controller_copy_bytes': compiled['bytes'],
          'compiled_controller_copy_manifest_sha256': digest(artifacts / 'f09-console-client-out-copy.json')}
with (artifacts / 'f09-primary-final-files.json').open('x', encoding='utf-8') as output:
    json.dump(record, output, indent=2)
    output.write('\n')
print(json.dumps(record, indent=2))
