# SPDX-License-Identifier: Apache-2.0
import json,pathlib,hashlib,subprocess,time
base=pathlib.Path(__file__).resolve().parent
receipt=json.loads((base/'preserved-adoption-r16-readonly-census.json').read_text(encoding='utf-8-sig'))
row=receipt['fixtures'][0];exe=pathlib.Path(row['images'][0]);assert hashlib.sha256(exe.read_bytes()).hexdigest()==row['image_sha256'][0]
fixture=pathlib.Path(row['fixture']);out=base/'preserved-adoption-r16-native-health.json';assert not out.exists()
start=time.monotonic();p=subprocess.run([str(exe),'service','rpc','--runtime-dir',str(fixture/'state'/'control'),'--stdin'],input=json.dumps('health').encode(),capture_output=True,timeout=90,creationflags=subprocess.CREATE_NO_WINDOW)
assert len(p.stdout)+len(p.stderr)<=2*1024*1024
record={'schema':1,'fixture':str(fixture),'executable':str(exe),'sha256':row['image_sha256'][0],'read_only':True,'exit_code':p.returncode,'elapsed_seconds':time.monotonic()-start,'stdout':p.stdout.decode('utf8',errors='replace'),'stderr':p.stderr.decode('utf8',errors='replace')}
out.write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record))
