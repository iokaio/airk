# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import base64,hashlib,json,shlex,subprocess,time
root=Path.cwd();base=root/'artifacts/advanced-split/local-delivery-preparation-r1';inventory=root/'artifacts/advanced-split/receipts/final-help-resume-r2/admin-assets.json';raw=inventory.read_bytes();pin=hashlib.sha256(raw).hexdigest();assert pin=='2390999ff5e181925e8037dd8ad7319e000ac0325c90a5b8dc34c350733b3150'
program=r'''set -eu
umask 077
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
from pathlib import Path
import base64,hashlib,importlib.util,json,os
assert os.geteuid()==501
source=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r1/source')
inputs=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1/inputs')
assert inputs.is_dir() and not inputs.is_symlink() and inputs.resolve()==inputs
assert source.is_dir() and not source.is_symlink() and source.resolve()==source
original_index=Path('/Users/tylerjensen/Code/airk/.git/index');before=hashlib.sha256(original_index.read_bytes()).hexdigest()
raw=base64.b64decode('__DATA__');assert hashlib.sha256(raw).hexdigest()=='__SHA__'
spec=importlib.util.spec_from_file_location('copied_admin_inventory_owner',source/'scripts/build-admin-assets.py');owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
block=owner.block_from_inventory(json.loads(raw))
handoff=inputs/'runtime-handoff-r1';assert not handoff.exists() and not handoff.is_symlink();handoff.mkdir(mode=0o700)
output=handoff/'admin-assets.json'
with output.open('xb') as stream:stream.write(raw)
assert hashlib.sha256(output.read_bytes()).hexdigest()=='__SHA__'
assert hashlib.sha256(original_index.read_bytes()).hexdigest()==before
print(json.dumps({'result':'checked_admin_inventory_matches_copied_source','inventory':str(output),'inventory_file_sha256':'__SHA__','admin_build_block':block,'original_git_index_unchanged':True,'no_build_install_or_service_change':True}))
PY
'''.replace('__DATA__',base64.b64encode(raw).decode()).replace('__SHA__',pin)
(base/'copy-neo-admin-inventory-r113.sh').write_text(program);connection=json.loads((root/'artifacts/v190/neo-connection.local.json').read_bytes());ssh=['ssh','-p',str(connection['port']),'-i',str(connection['identity_path']),*connection['ssh_options'],connection['user']+'@'+connection['host']]
started=time.monotonic();response=subprocess.run([*ssh,'sh','-s'],input=program.encode(),capture_output=True,timeout=60)
(base/'neo-admin-inventory-r113.stdout.log').write_bytes(response.stdout);(base/'neo-admin-inventory-r113.stderr.log').write_bytes(response.stderr)
result={'command':'existing normal-owner SSH -> POSIX sh -s','program_sha256':hashlib.sha256(program.encode()).hexdigest(),'exit_code':response.returncode,'elapsed_seconds':time.monotonic()-started,'source_inventory_sha256':pin,'credentials_printed':False}
if response.returncode==0:result['result']=json.loads(response.stdout)
(base/'neo-admin-inventory-r113.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result));raise SystemExit(response.returncode)
