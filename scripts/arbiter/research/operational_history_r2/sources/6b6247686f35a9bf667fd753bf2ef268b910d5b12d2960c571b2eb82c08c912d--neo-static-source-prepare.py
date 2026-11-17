# SPDX-License-Identifier: Apache-2.0
import hashlib,io,json,subprocess,tarfile
from pathlib import Path
connection=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*connection['ssh_options'],'-i',connection['identity_path'],'-p',str(connection['port']),connection['user']+'@'+connection['host']]
names=['scripts/provision-static-onnx-runtime.py','scripts/provision-static-onnx-tools.py','scripts/provision-static-cmake-source.py','scripts/static-cmake-source-lock.json','scripts/static-onnx-aggregate.py','scripts/static-onnx-link-closure.py','scripts/darwin-waitid.py','scripts/tests/test_static_onnx_tools.py','scripts/tests/test_static_onnx_provisioning.py','scripts/tests/test_static_cmake_source.py','scripts/tests/test_static_ort_aggregate.py']
archive=io.BytesIO();seals={}
with tarfile.open(fileobj=archive,mode='w:gz') as output:
 for name in names:
  data=Path(name).read_bytes();seals[name]=hashlib.sha256(data).hexdigest();entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=0o644;output.addfile(entry,io.BytesIO(data))
transfer = subprocess.run(ssh + ['sh', '-c', "'cd \"$HOME/code/airk\" && tar -xzf -'"],
                        input=archive.getvalue(),capture_output=True,timeout=45)
if transfer.returncode:print(transfer.stderr.decode());raise SystemExit(transfer.returncode)
source=Path('artifacts/advanced-split/tools/cmake-3.31.6-source/cmake-3.31.6.tar.gz');assert source.stat().st_size==11710589 and hashlib.sha256(source.read_bytes()).hexdigest()=='653427f0f5014750aafff22727fb2aa60c6c732ca91808cfb78ce22ddd9e55f0'
check="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
import importlib.util
from pathlib import Path
s=importlib.util.spec_from_file_location('t',Path('scripts/provision-static-onnx-tools.py'));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);c=m.core_module();base=c.owned_destination(Path('artifacts/advanced-split/ort130-arm64'),'aarch64-apple-darwin');root=m.setup_owner(base,'aarch64-apple-darwin',c);p=c.child(root,'downloads/cmake-3.31.6.tar.gz')
if p.exists():c.verify_file(p,c.load_script('provision-static-cmake-source.py').LOCK);print('SOURCE_PRESENT')
else:print('SOURCE_MISSING')
PY
"""
r=subprocess.run(ssh+['sh','-s'],input=check.encode(),capture_output=True,timeout=45)
if r.returncode:print(r.stderr.decode());raise SystemExit(r.returncode)
if r.stdout.decode().strip()=='SOURCE_MISSING':
 package=io.BytesIO()
 with tarfile.open(fileobj=package,mode='w:gz') as output:
  entry=tarfile.TarInfo('artifacts/advanced-split/ort130-arm64/tools/downloads/cmake-3.31.6.tar.gz');entry.size=source.stat().st_size;entry.mode=0o644;output.addfile(entry,io.BytesIO(source.read_bytes()))
 sent = subprocess.run(ssh + ['sh', '-c', "'cd \"$HOME/code/airk\" && test ! -e artifacts/advanced-split/ort130-arm64/tools/downloads/cmake-3.31.6.tar.gz && tar -xzf -'"],
                        input=package.getvalue(),capture_output=True,timeout=45)
 if sent.returncode:print(sent.stderr.decode());raise SystemExit(sent.returncode)
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 -m unittest scripts.tests.test_static_cmake_source scripts.tests.test_static_onnx_tools scripts.tests.test_static_onnx_provisioning scripts.tests.test_static_ort_aggregate
printf 'Approved source inputs and actual Mac fixture checks ready; no C++ started'

"""
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=90)
Path('artifacts/advanced-split/receipts/delivery-04-neo-static-source-prepare.log').write_bytes(r.stdout+r.stderr);Path('artifacts/advanced-split/receipts/delivery-04-neo-static-source-prepare.json').write_text(json.dumps({'source_sha256':seals,'exit_code':r.returncode,'stdout':r.stdout.decode(),'stderr':r.stderr.decode(),'source_archive_sha256':'653427f0f5014750aafff22727fb2aa60c6c732ca91808cfb78ce22ddd9e55f0','scope':'approved source-tool fallback then original ORT C++ producer, no Cargo/model transfer/trust changes'},indent=2));print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
