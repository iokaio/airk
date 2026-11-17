# SPDX-License-Identifier: Apache-2.0
import base64,json,shlex,subprocess,time
from pathlib import Path
cfg=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());assert cfg['host']=='192.168.68.61'
out=Path('artifacts/local/one-airk-1814')/('neo-schema1-predecessor-'+str(int(time.time())));out.mkdir()
code='''
import hashlib,json,os,platform,stat,subprocess,time
from pathlib import Path
root=Path('/Users/tylerjensen/airk-builds/explorer-notary-1812-ee5ad77-20261107/source3c6e81c/artifacts/neo/signed-notarized-3c/runtime')
assert root.resolve(strict=True)==root and os.getuid()==501 and platform.machine()=='arm64'
tick=time.monotonic();digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert digest(root/'airk-install.json')=='47f0b790645a707077c00015e5e38cc6256b57e0510f6da97c6c2db25002e038'
inventory=json.loads((root/'airk-install.json').read_text());metadata=json.loads((root/'airk-runtime.json').read_text());build=json.loads((root/'airk-build.json').read_text())
assert inventory['version']==metadata['version']=='1.8.12' and inventory['schema']==metadata['schema']==1 and inventory['os']=='macos' and inventory['arch']=='aarch64'
expected=set(inventory['files'])|{'airk-install.json'};actual=set()
for path in root.rglob('*'):
 info=path.lstat();assert not stat.S_ISLNK(info.st_mode)
 if path.is_file():
  assert stat.S_ISREG(info.st_mode) and info.st_nlink==1
  relative=path.relative_to(root).as_posix();actual.add(relative)
  if relative!='airk-install.json':assert digest(path)==inventory['files'][relative],relative
assert actual==expected
assert build['source']=={'sha256':'51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa','files':978}
report={'schema':1,'purpose':'read-only genuine historical schema1 prerequisite; no oldserviceactivation or qualification of newsource','root':str(root),'version':'1.8.12','install_manifest_sha256':digest(root/'airk-install.json'),'file_count':len(actual),'all_inventory_bytes_verified':True,'original_compiled_source':build['source'],'original_profile':build['profile'],'commands':[]}
requirement='=anchor apple generic and certificate leaf[subject.OU] = "5PX7D99PXA" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf = H"4d8496dcab73794378194d040d9177fb28a0f982"'
for relative in ['AIRK.app','AIRK.app/Contents/MacOS/airk','AIRK.app/Contents/MacOS/airk-embed','AIRK.app/Contents/MacOS/airk-launcher-image','AIRK.app/Contents/MacOS/libonnxruntime.dylib']:
 p=subprocess.run(['/usr/bin/codesign','--verify','--strict','--verbose=2','-R',requirement,str(root/relative)],capture_output=True,text=True,timeout=30)
 report['commands'].append({'kind':'strict_pinned_developer_id','relative':relative,'exit_code':p.returncode,'stderr':p.stderr[-2048:]});assert p.returncode==0
for relative in ['AIRK.app/Contents/MacOS/airk','AIRK.app/Contents/MacOS/airk-launcher-image']:
 p=subprocess.run([str(root/relative),'--version'],capture_output=True,text=True,timeout=30)
 report['commands'].append({'kind':'version_only_no_service','relative':relative,'exit_code':p.returncode,'stdout':p.stdout.strip(),'stderr':p.stderr[-2048:]});assert p.returncode==0 and p.stdout.strip()=='airk 1.8.12'
p=subprocess.run(['/usr/bin/xcrun','stapler','validate',str(root/'AIRK.app')],capture_output=True,text=True,timeout=30)
report['commands'].append({'kind':'staple_validate','exit_code':p.returncode,'stdout':p.stdout[-2048:],'stderr':p.stderr[-2048:]});assert p.returncode==0
report['elapsed_seconds']=round(time.monotonic()-tick,3);report['result']='passed';print(json.dumps(report,indent=2))
'''
python='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
command=shlex.quote(python)+' -c '+shlex.quote('import base64;exec(base64.b64decode('+repr(base64.b64encode(code.encode()).decode())+'))')
p=subprocess.run(['ssh','-i',cfg['identity_path'],*cfg['ssh_options'],cfg['user']+'@'+cfg['host'],command],capture_output=True,timeout=200)
(out/'proof.json').write_bytes(p.stdout);(out/'stderr.txt').write_bytes(p.stderr)
print('EVIDENCE '+str(out));print(p.stdout.decode());print(p.stderr.decode());raise SystemExit(p.returncode)
