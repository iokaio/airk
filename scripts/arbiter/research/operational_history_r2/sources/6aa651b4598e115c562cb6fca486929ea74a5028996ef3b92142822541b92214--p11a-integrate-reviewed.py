# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, subprocess, sys
from pathlib import Path

root = Path('D:/code/Github/airk')
base = root / 'artifacts/v190'
work = base / 'p10-worktree'
accepted = '512a9745a99e1f5bc32335d2d8de8aad6e64ad23'

def git(where, *args):
    return subprocess.check_output(['git', '-c', 'safe.directory='+where.as_posix(),
        '-c', 'core.autocrlf=true', '-c', 'core.safecrlf=false', '-C', str(where), *args], stderr=subprocess.PIPE)

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None

def names(data):
    return set(data.decode().strip('\0').split('\0')) - {''}

assert git(root, 'rev-parse', 'HEAD').decode().strip() == accepted
assert not git(root, 'status', '--porcelain')
qualification = json.loads((base/'p11a-final-qualification-r3.json').read_text())
assert qualification['status'] == 'passed' and len(qualification['steps']) == 12
assert all(row['exit_code'] == 0 and row['runtime_after_error'] is None for row in qualification['steps'])
assert (work/'AGENTS.md').read_bytes() == (work/'CLAUDE.md').read_bytes()
changed = names(git(work, 'diff', '--name-only', '-z', accepted))
new = names(git(work, 'ls-files', '--others', '--exclude-standard', '-z'))
native = ['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','crates',
    'scripts/build-service-runtime.py','scripts/provision-onnx-runtime.py',
    'scripts/provision-local-model.py','scripts/int8-policy.py','scripts/macho-identity.py',
    'scripts/license-notices.py','LICENSE','NOTICE','THIRD_PARTY_NOTICES.md','third_party']
native_names = names(git(work, 'ls-files','--cached','--others','--exclude-standard','-z','--',*native))
byte_only = {p for p in native_names if sha(work/p) != sha(root/p)} - changed - new
paths = sorted(changed | new | byte_only)
assert paths and all(not p.startswith(('artifacts/','temp/','.git/','.airk/')) for p in paths)
rows = []
for name in paths:
    source, destination = work/name, root/name
    assert source.is_file() and not source.is_symlink(), name
    if name in new:
        assert not destination.exists(), name
    rows.append({'path':name,'new':name in new,'byte_only':name in byte_only,
                 'before':sha(destination),'tested':sha(source)})
spec = importlib.util.spec_from_file_location('builder',work/'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec); spec.loader.exec_module(builder)
assert builder.source_identity() == qualification['source']
manifest = {'base':accepted,'source':qualification['source'],
    'compiled_runtime_source':qualification['compiled_runtime_source'],
    'files':rows,'applied':False,'phase_accepted':False,
    'excluded':'Ignored artifacts, private state, temp and non-native EOL-only differences; no deletions'}
output = base/('p11a-integration-manifest-r1.json' if '--apply' in sys.argv else 'p11a-integration-preview-r1.json')
assert not output.exists()
if '--apply' in sys.argv:
    for row in rows:
        source, destination = work/row['path'],root/row['path']
        assert sha(source) == row['tested'] and sha(destination) == row['before']
        destination.parent.mkdir(parents=True,exist_ok=True)
        destination.write_bytes(source.read_bytes())
    assert all(sha(root/r['path']) == r['tested'] for r in rows)
    spec = importlib.util.spec_from_file_location('integrated_builder',root/'scripts/build-service-runtime.py')
    integrated = importlib.util.module_from_spec(spec);spec.loader.exec_module(integrated)
    assert integrated.source_identity() == qualification['source']
    manifest['applied'] = True
output.write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'files':len(rows),'new':len(new),'byte_only':len(byte_only),'applied':manifest['applied']}))
