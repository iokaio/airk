# SPDX-License-Identifier: Apache-2.0
set -eu
effective_uid=$(id -u)
human_uid=$(stat -f %u /dev/console)
printf 'effective_name=%s effective_uid=%s logged_in_name=%s logged_in_uid=%s\n' "$(id -un)" "$effective_uid" "$(stat -f %Su /dev/console)" "$human_uid"
[ "$effective_uid" = "$human_uid" ] && [ "$effective_uid" -ne 0 ] || exit 21
/usr/bin/python3 -I -B - <<'PY'
import glob,json,os,platform,subprocess,sys
from pathlib import Path
home=Path.home()
patterns=[str(home/'airk-builds/*/tools'),str(home/'airk-evidence/*'),str(home/'.local/bin/python*'),str(home/'.pyenv/versions/*/bin/python*'),str(home/'.nvm/versions/node/*/bin/node'),str(home/'.volta/tools/image/node/*/bin/node'),str(home/'.asdf/installs/python/*/bin/python*'),str(home/'.asdf/installs/nodejs/*/bin/node'),'/opt/homebrew/Cellar/python*/*/bin/python*','/opt/homebrew/Cellar/node*/*/bin/node','/Library/Frameworks/Python.framework/Versions/*/bin/python*']
rows=[]
for pattern in patterns:
 for raw in sorted(glob.glob(pattern)):
  p=Path(raw)
  try:
   s=p.stat(); row={'path':str(p),'uid':s.st_uid,'kind':'directory' if p.is_dir() else 'file'}
   if p.is_file() and os.access(p,os.X_OK) and (p.name in ('node','python3') or p.name.startswith('python3.')) and not p.name.endswith('-config'):
    done=subprocess.run([str(p),'--version'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=10,env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1'))
    row.update(version=done.stdout.strip(),exit_code=done.returncode)
   rows.append(row)
  except (OSError,subprocess.SubprocessError) as e: rows.append({'path':str(p),'error':type(e).__name__})
roots=[home/'airk-builds',home/'airk-evidence',home/'code/airk/target',home/'Library/Caches/Homebrew/downloads',home/'.cache',home/'Library/Caches/uv',home/'.local/share/uv/python']
interesting=[]; errors=[]; visited=0; truncated=False
for root in roots:
 if not root.exists(): continue
 todo=[(root,0)]
 while todo:
  folder,depth=todo.pop(); visited+=1
  if visited>10000: truncated=True; break
  try:
   entries=list(folder.iterdir())
  except OSError as e: errors.append({'path':str(folder),'error':type(e).__name__}); continue
  for p in entries:
   n=p.name.lower()
   if any(t in n for t in ('onnx','minilm','static-ort','python-3.','python3.','node-v24','node@24')):
    try: interesting.append({'path':str(p),'uid':p.stat().st_uid,'kind':'directory' if p.is_dir() else 'file','bytes':p.stat().st_size if p.is_file() else None})
    except OSError: pass
   if depth<4 and p.is_dir() and not p.is_symlink() and n not in ('.git','node_modules','incremental','deps','build','.fingerprint','debug','release','release-local','site-packages'):
    todo.append((p,depth+1))
  if truncated: break
print(json.dumps({'system_python':{'executable':sys.executable,'version':platform.python_version()},'tool_candidates':rows,'cache_candidates':interesting[:300],'walk_directories':visited,'walk_truncated':truncated,'errors':errors[:20]},indent=2))
PY
