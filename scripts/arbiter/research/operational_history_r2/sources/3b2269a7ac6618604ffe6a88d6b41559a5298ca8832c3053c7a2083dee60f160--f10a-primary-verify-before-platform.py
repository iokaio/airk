# SPDX-License-Identifier: Apache-2.0
"""Independent F10a provenance check; no native code execution."""
from pathlib import Path
import hashlib
import importlib.util
import json
import re
import sys

phase = sys.argv[1]
assert phase in ['pre', 'final'] and __debug__
repo = Path.cwd().resolve(strict=True)
artifacts = repo / 'artifacts/optimizations'
location = json.loads((artifacts / 'f10a-runtime-location.json').read_text())
task = Path(location['task']).resolve(strict=True)
runtime = Path(location['runtime']).resolve(strict=True)
assert task == Path('C:/Users/tdotn/AppData/Local/Temp/airk-optimizations-f10a-gt3c10_2').resolve(strict=True)
assert runtime.parent == task and runtime.name == 'runtime'
assert not runtime.is_relative_to(repo) and not repo.is_relative_to(runtime)

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def plain(path, parent):
    assert not path.is_symlink() and not path.lstat().st_file_attributes & 0x400, str(path)
    assert path.resolve(strict=True).is_relative_to(parent), str(path)

plain(task, task)
builder_path = repo / 'scripts/build-service-runtime.py'
assert digest(builder_path) == '322656c8f413f54350d5610041cd7ce494a419762721ecde2a8cb0114b9f3c59'
spec = importlib.util.spec_from_file_location('f10a_primary_builder', builder_path)
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
source = builder.source_identity()
assert source == {'sha256': '10089d98ad9a061112f06557b75c567391df5760813f15441b4917448447b89d', 'files': 715}
build_gate = json.loads((artifacts / 'f10a-runtime-build-execution.json').read_text())
gates = {kind: json.loads((artifacts / f'f10a-corrected-{kind}-execution.json').read_text())
         for kind in ['fmt', 'clippy', 'full-n', 'full-python']}
for receipt in [build_gate, *gates.values()]:
    assert receipt['exit_code'] == 0 and receipt['source_unchanged']
    assert receipt['source_before'] == receipt['source_after'] == source
assert build_gate['builder_sha256'] == digest(builder_path)
inventory = json.loads((runtime / 'airk-install.json').read_text())
pair = json.loads((runtime / 'airk-runtime.json').read_text())
build = json.loads((runtime / 'airk-build.json').read_text())
assert inventory['schema'] == pair['schema'] == build['schema'] == 1
assert inventory['version'] == pair['version'] == '1.8.0'
assert inventory['os'] == pair['os'] == 'windows'
assert inventory['arch'] == pair['arch'] == 'x86_64'
assert build['runtime'] == pair and build['profile'] == 'release' and build['source'] == source
expected = inventory['files']
assert 0 < len(expected) <= 2048
actual, total = set(), 0
for path in [runtime, *runtime.rglob('*')]:
    plain(path, runtime)
    if path.is_file() and path != runtime / 'airk-install.json':
        name = path.relative_to(runtime).as_posix()
        assert name in expected and digest(path) == expected[name], name
        actual.add(name)
        total += path.stat().st_size
assert actual == set(expected)
assert pair['airk_sha256'] == expected['airk.exe']
assert pair['embed_sha256'] == expected['airk-embed.exe']
assert pair['airk_sha256'] != pair['embed_sha256']
helpers = {
    'check-service-clients.py': 'a4588c7202f3ba3aee27057bf0aa9e15dfeb627815ee61e3c8df249491d6984b',
    'check-service-pool.py': 'e4e9034b0d0b5c42ccceaee7e175e2d665f704ccf853a142ac64ab99633a08f5',
    'check-service-install.py': '3cda629c2830858300962540d288a817e17818e68c95cc154d83f7dec344b7ce',
    'check-client-lifetime.py': 'b960c491331b29dc44f5c439457226d6c3dbd8bb1070bdf9eb9ebd9206c966ed',
    'check-managed-runtime.py': '90c99fc123844f000513c0b08b6b54e2e05272b83269717f856ce53dc17d7b3a',
    'check-source-scope-original-identity.py': '02d6c9573972cb1b21aa67813c29d1effde48bd7237c6374b8b36e7a542e82ae',
}
assert {item['name']: item['sha256'] for item in location['scripts']} == helpers
plain(task / 'scripts', task)
for name, identity in helpers.items():
    path = task / 'scripts' / name
    plain(path, task / 'scripts')
    assert digest(path) == identity, name
    if name != 'check-source-scope-original-identity.py':
        assert digest(repo / 'scripts' / name) == identity, name
log = (artifacts / 'f10a-corrected-full-n.log').read_text(encoding='utf-8')
assert 'test result: FAILED' not in log
summaries = re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log, re.M)
counts = [sum(int(row[i]) for row in summaries) for i in range(3)]
assert counts[0] > 0 and counts[1] == 0 and len(summaries) >= 60
python_log = (artifacts / 'f10a-corrected-full-python.log').read_text(encoding='utf-8')
assert re.search(r'^OK(?: \(skipped=\d+\))?$', python_log, re.M)
native = {}
if phase == 'final':
    pre = json.loads((artifacts / 'f10a-primary-pre-files.json').read_text())
    assert pre['install_sha256'] == digest(runtime / 'airk-install.json')
    for kind in ['pool', 'scope', 'install', 'managed', 'clients']:
        receipt = json.loads((artifacts / f'f10a-native-{kind}-execution.json').read_text())
        assert receipt['exit_code'] == 0 and receipt['source_unchanged'], kind
        assert receipt['source_before'] == receipt['source_after'] == source, kind
        assert receipt['approved_install_sha256'] == pre['install_sha256'], kind
        native[kind] = {'exit_code': 0, 'elapsed_seconds': receipt['elapsed_seconds'],
                        'log_sha256': digest(artifacts / f'f10a-native-{kind}.log')}
record = {'primary_verification': 'passed', 'phase': phase, 'runtime': str(runtime),
          'install_sha256': digest(runtime / 'airk-install.json'),
          'main_sha256': pair['airk_sha256'], 'helper_sha256': pair['embed_sha256'],
          'native_source': source, 'inventory_files': len(actual), 'inventory_bytes': total,
          'profile': build['profile'], 'os': pair['os'], 'arch': pair['arch'],
          'reported_n_counts': dict(zip(['passed', 'failed', 'ignored'], counts)),
          'reported_n_summaries': len(summaries), 'qualification_helpers': helpers,
          'full_python_gate': gates['full-python'],
          'full_python_log_sha256': digest(artifacts / 'f10a-corrected-full-python.log'),
          'native_execution_during_this_check': False, 'completed_native_gates': native}
with (artifacts / f'f10a-primary-{phase}-files.json').open('x', encoding='utf-8') as output:
    json.dump(record, output, indent=2)
    output.write('\n')
print(json.dumps({key: value for key, value in record.items() if key not in ['qualification_helpers', 'full_python_gate']}, indent=2))
