# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,stat,subprocess,tempfile,time,zipfile
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence';current=source/'artifacts/neo/signed-gui1/runtime';binary=current/'AIRK.app/Contents/MacOS/airk'
archive=run/'airk-1.8.7-aarch64-apple-darwin.zip'
def sha(path):
 with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
assert sha(archive)=='f65666bccef46f09e6b66f9f69dd5b60890c5b8cfd55505c50f2c86f0eee82cb'
fixture=Path(tempfile.mkdtemp(prefix='airk-historical-recovery-'));assert fixture.parent.resolve()==Path(tempfile.gettempdir()).resolve()
install=fixture/'install';state=fixture/'state';control=state/'control';old=fixture/'historical';old.mkdir();project=fixture/'project';project.mkdir();sentinel=project/'sentinel';sentinel.write_text('Original owned recovery fixture sentinel.\n')
env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1');rows=[];report={'schema':'airk.neo-historical-staged-recovery/1','scope':'Recovery from authentic staged1.8.7 signed payload, no old service activation; not installed-editor or notarized-release acceptance.','old_archive_sha256':sha(archive),'old_service_activated':False,'fixture':str(fixture)}
def call(name,args,timeout=90,raw=False):
 start=time.monotonic();p=subprocess.run([str(binary),*map(str,args)],cwd=source,env=env,capture_output=True,text=True,timeout=timeout)
 row={'name':name,'command':[str(binary),*map(str,args)],'exit_code':p.returncode,'elapsed_seconds':round(time.monotonic()-start,3),'stdout':p.stdout,'stderr':p.stderr};rows.append(row);(evidence/'historical-recovery-steps.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps({k:row[k] for k in ['name','exit_code','elapsed_seconds']}),flush=True)
 if p.returncode:raise RuntimeError(name+' failed: '+p.stderr[-4096:])
 return p.stdout.strip() if raw else json.loads(p.stdout)
try:
 with zipfile.ZipFile(archive) as z:
  members=z.infolist();assert len(members)<2200 and sum(m.file_size for m in members)<4*1024**3
  for m in members:
   p=old/m.filename;assert not m.is_dir() and not Path(m.filename).is_absolute() and p.resolve().is_relative_to(old.resolve()) and not stat.S_ISLNK(m.external_attr>>16)
   p.parent.mkdir(parents=True,exist_ok=True)
   with z.open(m) as src,p.open('xb') as dest:shutil.copyfileobj(src,dest)
   p.chmod(0o755 if (m.external_attr>>16)&0o111 else 0o644)
 old_main=old/'AIRK.app/Contents/MacOS/airk';assert sha(old_main)=='d28828af6eb8275c55748bc5202e9a7e642e8568ff915eaa7284e343bb56a066'
 old_digest=sha(old/'airk-install.json');new_digest=sha(current/'airk-install.json');report.update(old_install_manifest_sha256=old_digest,current_install_manifest_sha256=new_digest)
 def stage(name,payload,digest):return call(name,['runtime','stage','--source',payload,'--install-dir',install,'--state-dir',state,'--approved-sha256',digest,'--verification-basis','organization-digest'],180)
 stage('stage-authentic-1.8.7',old,old_digest)
 launcher=install/'airk-launcher';assert sha(launcher)==sha(old_main)
 old_receipt=json.loads((install/'launcher.json').read_text());assert old_receipt['sha256']==sha(old_main);report['old_receipt_produced_by_stage']=old_receipt
 rejected=subprocess.run(['/usr/bin/codesign','--verify','--strict',str(launcher)],capture_output=True,text=True,timeout=30)
 report['old_launcher_negative']={'exit_code':rejected.returncode,'stdout':rejected.stdout,'stderr':rejected.stderr,'executed':False};assert rejected.returncode!=0 and 'Info.plist' in rejected.stderr
 assert not (install/'selected.json').exists()
 stage('stage-current-signed',current,new_digest)
 call('activate-current-only',['runtime','activate','--install-dir',install,'--runtime-sha256',new_digest],120)
 expected=sha(current/'AIRK.app/Contents/MacOS/airk-launcher-image')
 for count in range(2):
  call('prepare-client-'+str(count+1),['runtime','prepare-client','--install-dir',install])
  assert sha(launcher)==expected
  verified=subprocess.run(['/usr/bin/codesign','--verify','--strict',str(launcher)],capture_output=True,text=True,timeout=30);assert verified.returncode==0,verified.stderr
  version=subprocess.run([str(launcher),'--version'],capture_output=True,text=True,timeout=30);assert version.returncode==0 and version.stdout.strip()=='airk 1.8.12'
 call('current-health',['runtime','ensure','--install-dir',install])
 python=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools/python3.13'
 with (evidence/'historical-recovery-mcp.log').open('x') as output:
  mcp=subprocess.run([str(python),'scripts/check-client-lifetime.py','--launcher',str(launcher),'--root',str(project),'--control-dir',str(control)],cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT,timeout=120)
 assert mcp.returncode==0;report['mcp_lifetime_exit_code']=mcp.returncode;report['launcher_after_reconnect_sha256']=sha(launcher);assert sha(launcher)==expected
 report['status']='passed'
except BaseException as error:report['status']='failed';report['error']={'type':type(error).__name__,'error':str(error)}
finally:
 cleanup=[]
 if control.exists():
  p=subprocess.run([str(binary),'service','shutdown','--runtime-dir',str(control)],capture_output=True,text=True,timeout=30);cleanup.append({'operation':'shutdown-exact-owned-control','exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr})
 if install.exists():
  p=subprocess.run([str(binary),'runtime','uninstall','--install-dir',str(install)],capture_output=True,text=True,timeout=90);cleanup.append({'operation':'uninstall-exact-owned-install','exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr})
 report['cleanup']=cleanup;report['sentinel_preserved']=sentinel.read_text()=='Original owned recovery fixture sentinel.\n';report['target_absent']=not (run/'target').exists()
 if all(row['exit_code']==0 for row in cleanup):
  assert fixture.parent.resolve()==Path(tempfile.gettempdir()).resolve() and fixture.name.startswith('airk-historical-recovery-') and not fixture.is_symlink();shutil.rmtree(fixture)
 report['fixture_removed']=not fixture.exists();report['steps']=rows
 (evidence/'historical-recovery-result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report),flush=True)
