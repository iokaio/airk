# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, os, subprocess, time
from pathlib import Path

root = Path(__file__).resolve().parents[3]
os.chdir(root)
base = Path(__file__).resolve().parent
proof = json.loads((base / 'windows-debug-c8-installed-r16-reuse.json').read_text())
for name, expected in {**proof['unchanged_production_outputs'], **proof['current_test_sources']}.items():
    path = root / ('packages/vscode/' + name if name.startswith('out/') else name)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected, name
runtime = base / 'windows-debug-r23/runtime'
spec = importlib.util.spec_from_file_location('builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
compiled = json.loads((runtime / 'airk-build.json').read_text())['source']
assert builder.source_identity() == compiled
private = json.loads((base / 'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private, TMP=private, AIRK_PRIVATE_CENSUS_BASE=private,
                  AIRK_EDITOR_SCENARIOS='same,different,usage,untrusted', AIRK_EDITOR_CONTROL_TRACE='0')
args = json.loads((base / 'windows-debug-c8-installed-native-editor-r16.json').read_text())['arguments']
assert hashlib.sha256(Path(args[-1]).read_bytes()).hexdigest() == proof['vsix_sha256']
start = time.monotonic()
with (base / 'reconcile-c8-installed-r1.log').open('x') as log:
    result = subprocess.run(args, stdout=log, stderr=subprocess.STDOUT)
receipt = dict(arguments=args, exit_code=result.returncode, elapsed_seconds=time.monotonic()-start,
               selected_scenarios=os.environ['AIRK_EDITOR_SCENARIOS'], compiled_source=compiled,
               reuse_proof='windows-debug-c8-installed-r16-reuse.json',
               qualification='Original sealed r23 debug / VSIX r4; pre-optimization missing actual journeys; not fresh build')
with (base / 'reconcile-c8-installed-r1.json').open('x') as out:
    json.dump(receipt, out, indent=2)
print(json.dumps(receipt), flush=True)
raise SystemExit(result.returncode)
