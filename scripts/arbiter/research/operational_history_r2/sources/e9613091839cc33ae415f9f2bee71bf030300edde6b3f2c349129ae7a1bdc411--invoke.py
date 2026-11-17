# SPDX-License-Identifier: Apache-2.0
"""Root-only reviewed frozen-source transfer/native batch; preparer never runs it."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import types

ROOT = Path.cwd().resolve()
HERE = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('mode', choices=('source', 'run', 'fetch'))
parser.add_argument('--expected-seal', required=True)
args = parser.parse_args()
prepared = json.loads((HERE / 'prepared-inputs.json').read_bytes())
assert args.expected_seal == prepared['source_seal']
for name, digest in prepared['files'].items():
    assert hashlib.sha256((HERE / name).read_bytes()).hexdigest() == digest, name
source = json.loads((HERE / 'source.json').read_bytes())
if args.mode in ('source', 'run'):
    for name, row in source['files'].items():
        path = HERE / 'frozen-public' / name
        assert path.is_file() and not path.is_symlink()
        assert hashlib.sha256(path.read_bytes()).hexdigest() == row['sha256'], name
if args.mode == 'fetch':
    connection = json.loads((ROOT / 'artifacts/v190/neo-connection.local.json').read_bytes())
    ssh = ['ssh', *connection['ssh_options'], '-i', connection['identity_path'], '-p', str(connection['port']),
           connection['user'] + '@' + connection['host'], 'sh', '-s']
    result = subprocess.run(ssh, input=(HERE / 'fetch.sh').read_bytes(), capture_output=True, timeout=60)
    assert result.returncode == 0, 'Read-only evidence fetch failed; inspect retained connection separately'
    record = json.loads(result.stdout)
    out = HERE / 'observed-final'
    out.mkdir(exist_ok=False)
    for name, encoded in record['files'].items():
        relative = Path(name)
        assert not relative.is_absolute() and '..' not in relative.parts and 'native-fixtures' not in relative.parts
        path = out / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(base64.b64decode(encoded))
    (out / 'compiler-exit-observation.json').write_text(json.dumps({'active_compilers': record['active_compilers']}, indent=2) + '\n')
    print(json.dumps({'retained_files': len(record['files']), 'active_compilers': record['active_compilers']}))
    sys.exit(0)
helper_name = 'scripts/provision-static-onnx-runtime.py'
helper_path = HERE / 'frozen-public' / helper_name
helper_raw = helper_path.read_bytes()
assert hashlib.sha256(helper_raw).hexdigest() == source['files'][helper_name]['sha256']
owner = types.ModuleType('frozen_root_mac_transport_owner')
owner.__file__ = str(helper_path)
exec(compile(helper_raw, str(helper_path), 'exec'), owner.__dict__)
script = (HERE / ('apply-source.sh' if args.mode == 'source' else 'run.sh')).read_bytes()
if args.mode == 'run':
    script = ('AIRK_CARGO_OWNER_BATON=mac-current-read-' + args.expected_seal[:16] + '\nexport AIRK_CARGO_OWNER_BATON\n').encode() + script
request = HERE / ('transport-' + args.mode + '-request.json')
stdin = HERE / ('transport-' + args.mode + '.stdin')
receipt = HERE / ('transport-' + args.mode + '.json')
log = HERE / ('transport-' + args.mode + '.log')
assert not any(p.exists() for p in (request, stdin, receipt, log))
stdin.write_bytes(script)
request.write_text(json.dumps({'root': str(ROOT), 'script': str(stdin), 'sha256': hashlib.sha256(script).hexdigest()}, indent=2) + '\n')
result = owner.run_owned([sys.executable, '-I', '-B', str(HERE / 'transport_worker.py'), str(request)], ROOT, log,
                         2700 if args.mode == 'run' else 120)
receipt.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
# A transport close does not prove remote Cargo stopped/cleaned. Root must
# inspect matching native receipts, cleanup evidence and compiler observation.
sys.exit(0 if owner.successful(result) else 1)
