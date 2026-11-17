# SPDX-License-Identifier: Apache-2.0
import hashlib,io,json,subprocess,tarfile
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
ssh=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"]]
names=['scripts/provision-static-onnx-runtime.py','scripts/provision-static-onnx-tools.py','scripts/tests/test_static_onnx_tools.py','scripts/tests/test_static_onnx_provisioning.py']
archive=io.BytesIO();seals={}
with tarfile.open(fileobj=archive,mode='w:gz') as output:
 for name in names:
  data=Path(name).read_bytes();seals[name]=hashlib.sha256(data).hexdigest();entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=0o644;output.addfile(entry,io.BytesIO(data))
transferred = subprocess.run(ssh + ['sh', '-c', "'cd \"$HOME/code/airk\" && tar -xzf -'"],
                        input=archive.getvalue(), capture_output=True, timeout=45)
if transferred.returncode:print(transferred.stderr.decode());raise SystemExit(transferred.returncode)
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 -m unittest scripts.tests.test_static_onnx_tools scripts.tests.test_static_onnx_provisioning
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
from pathlib import Path
import importlib.util,json
spec=importlib.util.spec_from_file_location('tools',Path('scripts/provision-static-onnx-tools.py'));tools=importlib.util.module_from_spec(spec);spec.loader.exec_module(tools);core=tools.core_module()
base=Path('artifacts/advanced-split/ort130-arm64').resolve();root=tools.setup_owner(base,'aarch64-apple-darwin',core)
row=tools.LOCK['cmake']['Darwin'];entries=tools.extract(root/'downloads'/row['name'],root/'cmake',row,core);tools.cmake_cli_closure(entries,row)
print('AUTHENTICATED_VENDOR_LINKS',sum(entry['kind']=='vendor_symlink' for entry in entries),flush=True)
log=core.probe_log(root,'cmake-signature-repair');result=core.run_owned(['/usr/bin/codesign','--verify','--deep','--strict',str(root/'cmake/CMake.app')],root,log,30);core.write_json(log.parent/'result.json',result)
print('SIGNATURE',json.dumps(result),repr(log.read_bytes()),flush=True)
if not core.successful(result):raise SystemExit(35)
record=core.probe_version(root/'cmake'/row['program'],tools.LOCK['cmake_version'],root,'cmake-repaired');print('CMAKE',json.dumps(record),flush=True)
PY
"""
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=120)
Path('artifacts/advanced-split/receipts/delivery-04-neo-vendor-repair.log').write_bytes(r.stdout+r.stderr)
Path('artifacts/advanced-split/receipts/delivery-04-neo-vendor-repair.json').write_text(json.dumps({'source_sha256':seals,'exit_code':r.returncode,'scope':'authenticated vendor-link additive completion, signature check, read-only tool probe; no Cargo/model transfer'},indent=2))
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
