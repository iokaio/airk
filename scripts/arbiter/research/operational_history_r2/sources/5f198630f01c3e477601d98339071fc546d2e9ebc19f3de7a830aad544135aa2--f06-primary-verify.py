# SPDX-License-Identifier: Apache-2.0
"""Independent read-only F06 payload/source inventory check; no native execution."""
import hashlib
import importlib.util
import json
from pathlib import Path

repo = Path.cwd().resolve()
artifacts = repo / 'artifacts/optimizations'
location = json.loads((artifacts / 'f06-runtime-location.json').read_text())
runtime = Path(location['runtime']).resolve(strict=True)
task = Path(location['task']).resolve(strict=True)
assert runtime.parent == task and runtime.name == 'runtime'
assert not runtime.is_relative_to(repo) and not repo.is_relative_to(runtime)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


execution = json.loads((artifacts / 'f06-runtime-build-corrected-execution.json').read_text())
gate = json.loads((artifacts / 'f06-n-cli-options-corrected-execution.json').read_text())
assert execution['exit_code'] == gate['exit_code'] == 0
assert execution['source_unchanged'] and gate['source_unchanged']
assert execution['builder_sha256'] == digest(repo / 'scripts/build-service-runtime.py')
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
assert source == build['source'] == execution['source_before'] == execution['source_after'] == gate['source_after']
assert source == {'sha256': 'cf3aeac64af289bf763d3823179700c66741083a48f12f6d83030948bee82807', 'files': 707}
scripts = {
    'check-service-clients.py': 'a4588c7202f3ba3aee27057bf0aa9e15dfeb627815ee61e3c8df249491d6984b',
    'check-service-pool.py': '6a35e24cb66497b722c6c252d23d7c6203dd6402be2eba25bdc935cd5102ae5f',
    'check-source-scope-original-identity.py': '02d6c9573972cb1b21aa67813c29d1effde48bd7237c6374b8b36e7a542e82ae',
    'f06-healthy-status.py': '5d246955e997dc125266d41873fb7b751bcf877a651c4ed0374ba9d557b5222a',
}
for name, expected_digest in scripts.items():
    assert digest(task / 'scripts' / name) == expected_digest, name
assert digest(artifacts / 'f06-healthy-status.py') == scripts['f06-healthy-status.py']
record = {'primary_verification': 'passed', 'runtime': str(runtime),
          'install_sha256': digest(runtime / 'airk-install.json'),
          'main_sha256': pair['airk_sha256'], 'helper_sha256': pair['embed_sha256'],
          'native_source': source, 'inventory_files': len(actual), 'inventory_bytes': total,
          'profile': build['profile'], 'os': pair['os'], 'arch': pair['arch'],
          'qualification_helpers': scripts, 'functional_native_execution': False}
(artifacts / 'f06-primary-provenance.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record, indent=2))
