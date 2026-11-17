# SPDX-License-Identifier: Apache-2.0
import hashlib,io,json,subprocess,tarfile
from pathlib import Path
connection=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*connection['ssh_options'],'-i',connection['identity_path'],'-p',str(connection['port']),connection['user']+'@'+connection['host']]
names=['scripts/provision-static-cmake-source.py','scripts/tests/test_static_cmake_source.py']
archive=io.BytesIO();seals={}
with tarfile.open(fileobj=archive,mode='w:gz') as output:
 for name in names:
  data=Path(name).read_bytes();seals[name]=hashlib.sha256(data).hexdigest();entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=0o644;output.addfile(entry,io.BytesIO(data))
transfer = subprocess.run(ssh + ['sh', '-c', "'cd \"$HOME/code/airk\" && tar -xzf -'"],
                        input=archive.getvalue(),capture_output=True,timeout=45)
if transfer.returncode:print(transfer.stderr.decode());raise SystemExit(transfer.returncode)
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 -m unittest scripts.tests.test_static_cmake_source scripts.tests.test_static_onnx_tools scripts.tests.test_static_onnx_provisioning scripts.tests.test_static_ort_aggregate
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
import importlib.util
from pathlib import Path
s=importlib.util.spec_from_file_location('t',Path('scripts/provision-static-onnx-tools.py'));m=importlib.util.module_from_spec(s);s.loader.exec_module(m);c=m.core_module();base=c.owned_destination(Path('artifacts/advanced-split/ort130-arm64'),'aarch64-apple-darwin');root=m.setup_owner(base,'aarch64-apple-darwin',c)
print('COMPLETED_ORIGINAL_SOURCE_BUILD', c.load_script('provision-static-cmake-source.py').complete_retained_build(root,'aarch64-apple-darwin',c,m))
PY
if test -e artifacts/advanced-split/receipts/ort130-mac-source-r6.pid; then echo 'Existing exact owned source launch retained'; exit 28; fi
PYTHONDONTWRITEBYTECODE=1 nohup /usr/bin/python3 scripts/provision-static-onnx-runtime.py --target aarch64-apple-darwin --destination artifacts/advanced-split/ort130-arm64 --download --download-tools --build-cmake-source --build --parallel 2 --environment-file artifacts/advanced-split/ort130-arm64-env.json > artifacts/advanced-split/receipts/ort130-mac-source-r6.log 2>&1 < /dev/null &
printf '%s' "$!" > artifacts/advanced-split/receipts/ort130-mac-source-r6.pid
printf 'Started owned CMake-source/ORT producer pid=%s' "$!"
"""
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=90)
Path('artifacts/advanced-split/receipts/delivery-04-neo-static-source-resume.log').write_bytes(r.stdout+r.stderr);Path('artifacts/advanced-split/receipts/delivery-04-neo-static-source-resume.json').write_text(json.dumps({'source_sha256':seals,'exit_code':r.returncode,'stdout':r.stdout.decode(),'stderr':r.stderr.decode(),'source_archive_sha256':'653427f0f5014750aafff22727fb2aa60c6c732ca91808cfb78ce22ddd9e55f0','scope':'approved source-tool fallback then original ORT C++ producer, no Cargo/model transfer/trust changes'},indent=2));print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
