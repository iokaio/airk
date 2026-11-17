# SPDX-License-Identifier: Apache-2.0
set -eu
effective_uid=$(id -u)
human_uid=$(stat -f %u /dev/console)
printf 'effective_name=%s effective_uid=%s logged_in_name=%s logged_in_uid=%s\n' "$(id -un)" "$effective_uid" "$(stat -f %Su /dev/console)" "$human_uid"
[ "$effective_uid" = "$human_uid" ] && [ "$effective_uid" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'PY'
import glob,hashlib,json,os,subprocess
from pathlib import Path
home=Path.home(); rows=[]; seen=set(); count=0
roots=[home/'airk-builds',home/'code/airk/target',home/'airk-evidence']
markers={'build-receipt.json','static-aggregate-receipt.json','static-consumer-link-receipt.json','receipt.json','owner.json','onnxruntime-1.30.0-source.tar.gz','onnxruntime-1.22.1-source.tar.gz','libonnxruntime.a'}
for base in roots:
 if not base.is_dir():continue
 for folder,dirs,files in os.walk(base,followlinks=False):
  p=Path(folder); depth=len(p.relative_to(base).parts); count+=1
  dirs[:]=[n for n in dirs if depth<5 and n not in ('.git','node_modules','incremental','deps','.fingerprint','site-packages','__pycache__','b','build','debug','release','release-local','third_party','crates','docs','scripts','tools','lib','include','source','dependency-source','pre-policy-source','extension-correction-source','native-correction-source','source-r2','onnxruntime-source','mirror') and not n.startswith('source')]
  for name in files:
   q=p/name
   if name not in markers or not any(t in str(q).lower() for t in ('ort','onnx')):continue
   if str(q) in seen:continue
   seen.add(str(q));s=q.stat();row={'path':str(q),'uid':s.st_uid,'bytes':s.st_size}
   if q.suffix=='.json':
    try:
     d=json.loads(q.read_text());row['sha256']=hashlib.sha256(q.read_bytes()).hexdigest()
     row['keys']=list(d) if isinstance(d,dict) else []
     if isinstance(d,dict):row['identity']={k:d[k] for k in ('schema','ort_version','version','target','result','upstream_commit','input_lock_sha256','source_inventory_sha256','build_result','source_commit','original') if k in d and not isinstance(d[k],(list,dict))}
    except (OSError,ValueError) as e:row['error']=type(e).__name__
   rows.append(row)
folders=[]
for raw in glob.glob(str(home/'airk-builds/*static*'))+[str(home/'code/airk/target')]:
 p=Path(raw)
 if p.is_dir():folders.append({'path':str(p),'uid':p.stat().st_uid,'entries':sorted(q.name for q in p.iterdir())[:45]})
node=home/'airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin/node'
npm=node.parent.parent/'lib/node_modules/npm/bin/npm-cli.js'
probe=subprocess.run([str(node),str(npm),'--version'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=15)
print(json.dumps({'static_receipt_candidates':rows,'root_entries':folders,'walk_directories':count,'npm_cli':{'path':str(npm),'uid':npm.stat().st_uid,'exit_code':probe.returncode,'version':probe.stdout.strip()}},indent=2))
PY
