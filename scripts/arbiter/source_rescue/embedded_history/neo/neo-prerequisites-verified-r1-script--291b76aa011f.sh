# SPDX-License-Identifier: Apache-2.0
set -eu
effective_uid=$(id -u)
human_uid=$(stat -f %u /dev/console)
printf 'effective_name=%s effective_uid=%s logged_in_name=%s logged_in_uid=%s\n' "$(id -un)" "$effective_uid" "$(stat -f %Su /dev/console)" "$human_uid"
[ "$effective_uid" = "$human_uid" ] && [ "$effective_uid" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'PY'
import glob,hashlib,json,os,subprocess
from pathlib import Path
home=Path.home(); tools=home/'airk-builds/timing-1.6.4.PutU1P'; results=[]
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for chunk in iter(lambda:f.read(1024*1024),b''):h.update(chunk)
 return h.hexdigest()
commands=[(tools/'python/cpython-3.13.18-macos-aarch64-none/bin/python3.13',['-I','-B','-c','import hashlib,json,ssl,sys,tomllib,venv; print(json.dumps({"version":sys.version,"file_digest":hasattr(hashlib,"file_digest"),"openssl":ssl.OPENSSL_VERSION,"tomllib":True,"venv":True}))']), (tools/'tools/node-v24.15.0-darwin-arm64/bin/node',['--version'])]
for p,args in commands:
 row={'path':str(p),'exists':p.is_file()}
 if p.is_file():
  row.update(uid=p.stat().st_uid,sha256=sha(p),resolved=str(p.resolve()))
  done=subprocess.run([str(p),*args],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=15,env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1'))
  row.update(exit_code=done.returncode,output=done.stdout.strip())
 results.append(row)
for raw in glob.glob(str(home/'airk-builds/*/tools/*')):
 p=Path(raw)
 if p.name in ('cmake','ninja','python3.13','uv') or p.name.startswith(('cmake-3.31','ninja-','node-v24')):
  results.append({'path':str(p),'uid':p.stat().st_uid,'kind':'directory' if p.is_dir() else 'file'})
patterns=[str(home/'airk-builds/*/*static*'),str(home/'airk-builds/*/inputs/*'),str(home/'airk-builds/*/target/static-onnx-runtime/*'),str(home/'code/airk/target/static-onnx-runtime/*'),str(home/'airk-inputs/*'),str(home/'airk-builds/*/source/target/static-onnx-runtime/*')]
roots=[]
for pattern in patterns:
 for raw in sorted(glob.glob(pattern)):
  p=Path(raw)
  if p.is_dir() and any(t in str(p).lower() for t in ('static','onnx','model')):roots.append(p)
rows=[]
for p in sorted(set(roots)):
 row={'path':str(p),'uid':p.stat().st_uid,'markers':[]}
 for marker in ('owner.json','build-receipt.json','static-aggregate-receipt.json','build-attempt.json','evidence2/receipt.json','b/CMakeCache.txt'):
  q=p/marker
  if not q.is_file():continue
  m={'path':marker,'bytes':q.stat().st_size,'sha256':sha(q)}
  if q.suffix=='.json':
   try:
    d=json.loads(q.read_text())
    m['keys']=list(d) if isinstance(d,dict) else []
    if isinstance(d,dict):m['identity']={k:d[k] for k in ('schema','ort_version','version','target','result','upstream_commit','input_lock_sha256','source_inventory_sha256','build_result','source_commit') if k in d}
   except (ValueError,OSError) as e:m['error']=type(e).__name__
  row['markers'].append(m)
 rows.append(row)
print(json.dumps({'verified_tools':results,'static_or_model_roots':rows,'disk':subprocess.check_output(['df','-h',str(home)],text=True).strip()},indent=2))
PY
