# SPDX-License-Identifier: Apache-2.0
"""Prepared root-only source/batch transport; preparer never executes SSH/Cargo."""
import argparse,hashlib,importlib.util,json,subprocess,sys
from pathlib import Path
ROOT=Path.cwd();PREPARED_DIRECTORY=ROOT/'artifacts/advanced-split/mac-cli-await-r11'
p=argparse.ArgumentParser();p.add_argument('mode',choices=('source','observe','run','fetch'));p.add_argument('--expected-seal',required=True);args=p.parse_args()
plan=json.loads((PREPARED_DIRECTORY/'plan.json').read_text());assert args.expected_seal==plan['current_source_set_sha256'],'Root-reviewed seal differs'
for name,sha in plan['prepared_scripts'].items():assert hashlib.sha256((PREPARED_DIRECTORY/name).read_bytes()).hexdigest()==sha,name
manifest=json.loads((PREPARED_DIRECTORY/'source.json').read_bytes())
if args.mode in ('source','run'):
 for name,row in manifest['files'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==row['sha256'],('Local public source changed',name)
connection=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*connection['ssh_options'],'-i',connection['identity_path'],'-p',str(connection['port']),connection['user']+'@'+connection['host'],'sh','-s']
if args.mode=='observe':
 r=subprocess.run(ssh,input=(PREPARED_DIRECTORY/'observe.sh').read_bytes(),capture_output=True,timeout=60)
 (PREPARED_DIRECTORY/'observe-transport.log').write_bytes(r.stdout+r.stderr);assert r.returncode==0,r.stderr.decode('utf-8','replace')
 observation=json.loads(r.stdout);(PREPARED_DIRECTORY/'observed-preflight.json').write_text(json.dumps(observation,indent=2)+'\n');print(json.dumps(observation,indent=2));raise SystemExit(0)
if args.mode=='fetch':
 r=subprocess.run(ssh,input=(PREPARED_DIRECTORY/'fetch.sh').read_bytes(),capture_output=True,timeout=60);assert r.returncode==0,r.stderr.decode('utf-8','replace');v=json.loads(r.stdout);dest=PREPARED_DIRECTORY/'observed';dest.mkdir(exist_ok=False)
 import base64
 for name,data in v['files'].items():
  relative=Path(name);assert not relative.is_absolute() and '..' not in relative.parts and 'native-fixtures' not in relative.parts
  target=dest/relative;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(base64.b64decode(data))
 (dest/'compiler-exit-observation.json').write_text(json.dumps({'active_compilers':v['active_compilers']},indent=2)+'\n');print(json.dumps({'retained_files':len(v['files']),'active_compilers':v['active_compilers']}));raise SystemExit(0)
# Reuse the existing actual-process owner and its retained stdout/stderr/timeout
# result. A transport failure does not claim remote Cargo has stopped or cleaned.
spec=importlib.util.spec_from_file_location('mac_cli_owned_transport',ROOT/'scripts/provision-static-onnx-runtime.py');owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
script=(PREPARED_DIRECTORY/('apply-source.sh' if args.mode=='source' else 'run.sh')).read_text()
if args.mode=='run':script='AIRK_CARGO_OWNER_BATON=mac-cli-await-'+args.expected_seal[:16]+'\nexport AIRK_CARGO_OWNER_BATON\n'+script
script_path=PREPARED_DIRECTORY/('transport-'+args.mode+'.stdin');script_path.write_text(script,encoding='utf-8',newline='\n')
request_path=PREPARED_DIRECTORY/('transport-'+args.mode+'-request.json')
request_path.write_text(json.dumps({'root':str(ROOT),'script':str(script_path),'sha256':hashlib.sha256(script_path.read_bytes()).hexdigest()},indent=2)+'\n')
result=owner.run_owned([sys.executable,'-I','-B',str(PREPARED_DIRECTORY/'transport_worker.py'),str(request_path)],ROOT,PREPARED_DIRECTORY/('transport-'+args.mode+'.log'),2700 if args.mode=='run' else 120)
(PREPARED_DIRECTORY/('transport-'+args.mode+'.json')).write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result));raise SystemExit(0 if owner.successful(result) else 1)
