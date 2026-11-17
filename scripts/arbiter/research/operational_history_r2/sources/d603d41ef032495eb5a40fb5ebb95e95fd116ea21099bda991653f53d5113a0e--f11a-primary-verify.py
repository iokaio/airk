# SPDX-License-Identifier: Apache-2.0
"""Primary F11a source/payload/evidence verification; never executes native code."""
from pathlib import Path
import hashlib
import importlib.util
import json
import re
import sys

phase = sys.argv[1]
assert phase in ['pre', 'final'] and __debug__
repo = Path.cwd().resolve(strict=True)
a = repo / 'artifacts/optimizations'
location = json.loads((a / 'f11a-runtime-location.json').read_text())
task = Path(location['task']).resolve(strict=True)
runtime = Path(location['runtime']).resolve(strict=True)
assert task == Path('C:/Users/tdotn/AppData/Local/Temp/airk-optimizations-f11a-pdxvw2xa').resolve(strict=True)
assert runtime.parent == task and runtime.name == 'runtime'
assert not runtime.is_relative_to(repo) and not repo.is_relative_to(runtime)

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def plain(path, parent):
    assert not path.is_symlink() and not path.lstat().st_file_attributes & 0x400, str(path)
    assert path.resolve(strict=True).is_relative_to(parent), str(path)

def record(name, source):
    receipt = json.loads((a / name).read_text())
    assert receipt['exit_code'] == 0 and receipt['source_unchanged'], name
    assert receipt['source_before'] == receipt['source_after'] == source, name
    return receipt

plain(task, task)
builder_path = repo / 'scripts/build-service-runtime.py'
assert digest(builder_path) == '322656c8f413f54350d5610041cd7ce494a419762721ecde2a8cb0114b9f3c59'
spec = importlib.util.spec_from_file_location('f11a_primary_builder', builder_path)
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
source = builder.source_identity()
assert source == {'sha256': '927128a5eb90a71fb74ea53dac53a56b787ba3f86792d96ca3cfeb5d938344c5', 'files': 718}
gates = {name: record(name, source) for name in [
    'f11a-corrected-pre-n-fmt-execution.json',
    'f11a-corrected-pre-n-clippy-execution.json',
    'f11a-corrected-pre-n-standalone-execution.json',
    'f11a-full-n-execution.json',
    'f11a-full-python-execution.json',
    'f11a-runtime-build-execution.json',
]}
assert gates['f11a-runtime-build-execution.json']['builder_sha256'] == digest(builder_path)
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
    'check-source-scope-current-pool.py': 'b0f5d8c617204dcce751da09f2537d6b4bbe95e956ffca0b8cf19f74a8f8ec57',
}
assert {item['name']: item['sha256'] for item in location['scripts']} == helpers
plain(task / 'scripts', task)
for item in location['scripts']:
    path = task / 'scripts' / item['name']
    plain(path, task / 'scripts')
    assert digest(path) == digest(Path(item['source'])) == helpers[item['name']]
repair = location['approved_repair_driver']
assert Path(repair['path']).resolve(strict=True) == a / 'f11-fresh-pair-coverage/qualify-approved-repair.py'
assert digest(Path(repair['path'])) == repair['sha256'] == '2943e05a16fef4ff21ea53cac6daef772c599c7dbc0d6918172f42290cc8db58'
assert Path(repair['base_path']).resolve(strict=True) == a / 'f11-fresh-pair-coverage/BASE.json'
assert digest(Path(repair['base_path'])) == repair['base_sha256'] == 'f5d7f5ec2f0065ebc2a76db0ca6ee376268b91447c15e57f3479aaa7701c1663'
for name, identity in json.loads(Path(repair['base_path']).read_text())['files'].items():
    assert digest(repo / name) == identity
query_driver = a / 'f11a-query-coexistence.py'
assert digest(query_driver) == '56fd6c8b49d1b22f48af363654d0d175ef8b62fc013ef29153568f93a3815200'
query = location['query_coexistence_driver']
assert Path(query['path']).resolve(strict=True) == query_driver
assert query['sha256'] == digest(query_driver)
assert Path(query['base_path']).resolve(strict=True) == Path(repair['base_path']).resolve(strict=True)
assert query['base_sha256'] == repair['base_sha256']
log = (a / 'f11a-full-n.log').read_text(encoding='utf-8')
assert 'test result: FAILED' not in log
summaries = re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', log, re.M)
counts = [sum(int(row[i]) for row in summaries) for i in range(3)]
assert summaries and counts[0] > 1600 and counts[1] == 0
python_log = (a / 'f11a-full-python.log').read_text(encoding='utf-8')
assert re.search(r'^OK(?: \(skipped=\d+\))?$', python_log, re.M)
native = {}
if phase == 'final':
    pre = json.loads((a / 'f11a-primary-pre-files.json').read_text())
    assert pre['install_sha256'] == digest(runtime / 'airk-install.json')
    for kind in ['repair', 'query-coexistence', 'pool', 'scope', 'install', 'managed', 'clients']:
        receipt = record(f'f11a-native-{kind}-execution.json', source)
        assert receipt['approved_install_sha256'] == pre['install_sha256'], kind
        native[kind] = {'exit_code': 0, 'elapsed_seconds': receipt['elapsed_seconds'],
                        'log_sha256': digest(a / f'f11a-native-{kind}.log')}
result = {
    'primary_verification': 'passed', 'phase': phase, 'runtime': str(runtime),
    'source': source, 'install_sha256': digest(runtime / 'airk-install.json'),
    'airk_sha256': pair['airk_sha256'], 'embed_sha256': pair['embed_sha256'],
    'payload_files': len(expected), 'payload_bytes': total,
    'printed_test_summaries': len(summaries), 'printed_test_counts': counts,
    'counts_include_child_test_summaries': True,
    'full_n_log_sha256': digest(a / 'f11a-full-n.log'),
    'full_python_log_sha256': digest(a / 'f11a-full-python.log'),
    'gates': {name: item['elapsed_seconds'] for name, item in gates.items()},
    'helpers': helpers, 'approved_repair_driver': repair,
    'approved_query_driver_sha256': digest(query_driver), 'native': native,
}
with (a / f'f11a-primary-{phase}-files.json').open('x') as output:
    json.dump(result, output, indent=2)
print(json.dumps(result, indent=2))
