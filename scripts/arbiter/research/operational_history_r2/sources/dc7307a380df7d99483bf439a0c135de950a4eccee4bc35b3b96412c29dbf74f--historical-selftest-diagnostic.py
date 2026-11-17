# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil,stat,subprocess,tempfile,zipfile
from pathlib import Path
run=Path(__file__).resolve().parent;archive=run/'airk-1.8.7-aarch64-apple-darwin.zip'
with archive.open('rb') as stream:assert hashlib.file_digest(stream,'sha256').hexdigest()=='f65666bccef46f09e6b66f9f69dd5b60890c5b8cfd55505c50f2c86f0eee82cb'
with tempfile.TemporaryDirectory(prefix='airk-old-selftest-diagnostic-') as temporary:
 root=Path(temporary)
 with zipfile.ZipFile(archive) as z:
  for m in z.infolist():
   p=root/m.filename;assert not m.is_dir() and p.resolve().is_relative_to(root.resolve()) and not stat.S_ISLNK(m.external_attr>>16)
   p.parent.mkdir(parents=True,exist_ok=True)
   with z.open(m) as source,p.open('xb') as dest:shutil.copyfileobj(source,dest)
   p.chmod(0o755 if (m.external_attr>>16)&0o111 else 0o644)
 commands=[['/usr/bin/codesign','--verify','--strict','--verbose=2',str(root/name)] for name in ['AIRK.app','AIRK.app/Contents/MacOS/airk','AIRK.app/Contents/MacOS/airk-embed','AIRK.app/Contents/MacOS/libonnxruntime.dylib']]
 commands.append([str(root/'AIRK.app/Contents/MacOS/airk'),'runtime','self-test'])
 rows=[]
 for command in commands:
  try:
   p=subprocess.run(command,capture_output=True,text=True,timeout=45);rows.append({'command':command,'exit_code':p.returncode,'stdout':p.stdout[-16384:],'stderr':p.stderr[-16384:]})
  except subprocess.TimeoutExpired:rows.append({'command':command,'timed_out':True})
 report={'schema':'airk.neo-historical-selftest-diagnostic/1','read_only_payload':True,'old_service_activated':False,'checks':rows,'target_absent':not (run/'target').exists()}
report['temporary_removed']=not root.exists()
(run/'evidence/historical-selftest-diagnostic.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
