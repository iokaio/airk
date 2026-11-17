# SPDX-License-Identifier: Apache-2.0
import base64,hashlib,json,subprocess
from pathlib import Path
source=Path('scripts/dev/receipt_runner.py').read_bytes();payload=base64.b64encode(source).decode();sha=hashlib.sha256(source).hexdigest()
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote=r'''set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
import base64,hashlib,json,os,subprocess,sys,tempfile
from pathlib import Path
source=base64.b64decode('__SOURCE__');assert hashlib.sha256(source).hexdigest()=='__SHA__'
root=Path.cwd().resolve();namespace={'__file__':str(root/'scripts/dev/receipt_runner.py'),'__name__':'candidate_receipt_runner'};exec(compile(source,namespace['__file__'],'exec'),namespace)
base=root/'artifacts/advanced-split/receipts/runner-tmpdir-probe-r1';base.mkdir();(base/'runner-candidate.py').write_bytes(source)
foreign=base/'foreign';foreign.mkdir();private=base/'expected-owned-base';os.environ['TMPDIR']=str(foreign)
args=namespace['parse'](['--fixture-base',str(private),'--groups','','temp-choice',sys.executable,'-c','pass']);environment=namespace['environment'](args,base/'output')
child='import tempfile; from pathlib import Path; owner=tempfile.TemporaryDirectory(); print(Path(owner.name).resolve().parent); owner.cleanup()'
result=subprocess.run([sys.executable,'-I','-B','-c',child],env=environment,capture_output=True,text=True,check=True,timeout=15)
assert Path(result.stdout.strip())==private
for key in ['TEMP','TMP','TMPDIR']:assert environment[key]==str(private)
record={'platform':sys.platform,'candidate_runner_sha256':hashlib.sha256(source).hexdigest(),'actual_child_temp_parent':result.stdout.strip(),'expected_owned_base':str(private),'existing_product_runner_unchanged':True,'source_scope':'candidate same-byte Python environment seam; no Cargo/native rerun or source overwrite'}
(base/'result.json').write_text(json.dumps(record,indent=2));print(json.dumps(record))
PY
'''.replace('__SOURCE__',payload).replace('__SHA__',sha)
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=35);Path('artifacts/advanced-split/receipts/runner-tmpdir-mac-probe-r1.log').write_bytes(r.stdout+r.stderr);print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
