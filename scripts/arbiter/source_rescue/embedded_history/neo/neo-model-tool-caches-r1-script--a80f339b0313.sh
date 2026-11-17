# SPDX-License-Identifier: Apache-2.0
set -eu
effective_uid=$(id -u)
human_uid=$(stat -f %u /dev/console)
printf 'effective_name=%s effective_uid=%s logged_in_name=%s logged_in_uid=%s\n' "$(id -un)" "$effective_uid" "$(stat -f %Su /dev/console)" "$human_uid"
[ "$effective_uid" = "$human_uid" ] && [ "$effective_uid" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'PY'
import glob,hashlib,json,os,subprocess
from pathlib import Path
home=Path.home(); ort=home/'airk-builds/one-airk-static-ort-1814-1794087059'; rows=[]
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for chunk in iter(lambda:f.read(1024*1024),b''):h.update(chunk)
 return h.hexdigest()
for p in [ort/'tools/cmake-3.31.6-macos-universal/CMake.app/Contents/bin/cmake',ort/'ninja-build/ninja']:
 row={'path':str(p),'exists':p.is_file()}
 if p.is_file():
  done=subprocess.run([str(p),'--version'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=15)
  row.update(uid=p.stat().st_uid,sha256=sha(p),exit_code=done.returncode,version=done.stdout.splitlines()[0])
 rows.append(row)
models=[]
for name in ('bundled-model-v2','bundled-model-v3'):
 root=home/'airk-builds/advanced-split-local-delivery-prerequisites-r1/inputs'/name
 if not root.is_dir():continue
 for folder,dirs,files in os.walk(root):
  p=Path(folder);depth=len(p.relative_to(root).parts)
  if depth>=2:dirs[:]=[]
  for name in files:
   if name not in ('model.json','model.onnx','tokenizer.json','LICENSE'):continue
   q=p/name;s=q.stat(); row={'path':str(q),'uid':s.st_uid,'bytes':s.st_size,'sha256':sha(q)}
   if name=='model.json':
    d=json.loads(q.read_text());row['manifest']=d
   models.append(row)
print(json.dumps({'existing_static_tools':rows,'model_inputs':models},indent=2))
PY
