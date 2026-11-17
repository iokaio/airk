# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path

base = Path(__file__).resolve().parent
root = base.parents[2]
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()

original = json.loads((base/'completion-freeze-r26-final.json').read_text())
current = json.loads((base/'completion-fixture-compatibility-freeze-r1.json').read_text())
before, after = original['groups']['native'], current['groups']['native']
assert before.keys()==after.keys()
changed = {name for name in before if before[name]!=after[name]}
assert changed == {'crates/airk-cli/src/project_supervisor_guarded_qualification_tests.rs',
                   'crates/airk-cli/src/project_supervisor_modern_checkpoint_tests.rs'}
runtime = base/'windows-debug-r26/runtime'
manifest_identity = sha(runtime/'airk-install.json')
assert manifest_identity=='2069c2d6e3acc48bba07a808fae4b160c46def14b76b03cb12a768357cf2138a'
manifest = json.loads((runtime/'airk-install.json').read_text())
observed = {path.relative_to(runtime).as_posix() for path in runtime.rglob('*') if path.is_file()}
assert observed == set(manifest['files'])|{'airk-install.json'}
for name,digest in manifest['files'].items():
    assert sha(runtime/name)==digest,name
build = json.loads((runtime/'airk-build.json').read_text())
assert build['source']==original['compiled_native_source']
vsix = base/'windows-debug-vsix-r26/publish/airk-1.8.14-win32-x64.vsix'
vsix_identity = sha(vsix)
assert vsix_identity=='e91f564a2db03a3e97fa1832aeb9f6075bb55be5b189c5f96720fe2bca8a8272'
proof = {'schema':1,'runtime_manifest_sha256':manifest_identity,'runtime_seals_verified':len(manifest['files']),
         'original_compiled_source':build['source'],'current_test_source':current['compiled_native_source'],
         'production_native_inputs_unchanged':True,'original_external_test_deltas':sorted(changed),
         'vsix_sha256':vsix_identity,'harness_freeze':current['identities']['harness'],
         'build_reused':False,'description':'Read-only original artifact verification; no build or acceptance invocation'}
with (base/'fixture-compatibility-artifact-proof.json').open('x') as output:
    json.dump(proof,output,indent=2);output.write('\n')
print(json.dumps(proof))
