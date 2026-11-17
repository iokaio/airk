# SPDX-License-Identifier: Apache-2.0
"""Read-only signing/tool/cache and predecessor-presence detail; never accesses private keys."""
from pathlib import Path
import json,subprocess,hashlib,time
ROOT=Path(__file__).resolve().parents[3];p=ROOT/"artifacts/v190/neo-connection.local.json";d=json.loads(p.read_bytes())
remote=r'''import os,pwd,subprocess,json
from pathlib import Path
home=Path(pwd.getpwuid(os.geteuid()).pw_dir);repo=home/'code/airk'
def run(argv):
 try:
  x=subprocess.run(argv,capture_output=True,text=True,timeout=30);return x.returncode,x.stdout,x.stderr
 except Exception as e:return None,'',type(e).__name__
code,out,error=run(['/usr/bin/security','find-identity','-v','-p','codesigning']);identities=[line.strip() for line in out.splitlines() if '5PX7D99PXA' in line or 'valid identities found' in line]
paths=[home/'Library/Keychains/airk-signing.keychain-db',home/'Library/Application Support/AIRK',home/'Library/Caches/AIRK',home/'Library/Logs/AIRK',home/'.local/bin/airkctl',home/'.airk',Path('/opt/homebrew/bin/node'),Path('/usr/local/bin/node'),repo/'artifacts/advanced-split',repo/'target',repo/'artifacts/v190/p10-target']
found={str(path):{'exists':path.exists(),'symlink':path.is_symlink()} for path in paths}
roots=[home/'.local/share',home/'Library/Caches',repo/'artifacts',repo/'artifacts/advanced-split']
candidates=[]
for parent in roots:
 if parent.is_dir():
  for child in parent.iterdir():
   if any(word in child.name.casefold() for word in ['node','python','ort','static','sign','keychain','mac-current']):candidates.append(str(child))
report={'schema':'airk.private-neo-delivery-readonly/1','read_only':True,'signing_identity_probe_exit':code,'matching_public_signing_identities':identities,'signing_probe_error':error[:200] if code else '', 'paths':found,'candidate_existing_tools_caches':candidates[:100],'no_private_key_access':True}
print(json.dumps(report))
'''
start=time.monotonic();x=subprocess.run(['ssh','-p',str(d['port']),'-i',str(d['identity_path']),*d['ssh_options'],d['user']+'@'+d['host'],'/usr/bin/python3','-'],input=remote,capture_output=True,text=True,timeout=60)
r={'ssh_exit':x.returncode,'elapsed_seconds':time.monotonic()-start,'credentials_printed':False}
if x.returncode==0:r['observation']=json.loads(x.stdout)
else:r['error']='SSH failed; no mutation attempted'
out=Path(__file__).with_name('neo-delivery-readonly.json');assert not out.exists();out.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
