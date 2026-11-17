# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,stat,subprocess,tempfile,time,uuid,zipfile
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';assert json.loads((run/'notary4-result.json').read_text())['status']=='passed'
publish=source/'artifacts/neo/native-notarized/publish';manifest=json.loads((publish/'manifest.json').read_text());archive=publish/manifest['archive']
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
archive_sha=sha(archive);assert archive_sha==manifest['archive_sha256'];rows=[];error=None
def command(name,args,timeout=60):
 start=time.monotonic()
 try:
  p=subprocess.run(list(map(str,args)),capture_output=True,text=True,timeout=timeout);row={'name':name,'command':list(map(str,args)),'exit_code':p.returncode,'stdout':p.stdout[-8192:],'stderr':p.stderr[-8192:],'elapsed_seconds':round(time.monotonic()-start,3)}
 except subprocess.TimeoutExpired:row={'name':name,'command':list(map(str,args)),'timed_out':True,'elapsed_seconds':round(time.monotonic()-start,3)}
 rows.append(row);return row
owner=Path(tempfile.mkdtemp(prefix='airk-notarized-transfer-')).resolve(strict=True)
report={'schema':'airk.neo-final-package-transfer/1','native_archive_sha256':archive_sha,'quarantine_scope':'Simulated downloaded quarantine on exact owned extracted app and standalone copy only; original artifacts untouched.','network_ticket_lookup_needed':'unknown; network was not disabled','original_native_producer_reused':True}
try:
 root=owner/'payload';root.mkdir()
 with zipfile.ZipFile(archive) as z:
  members=z.infolist();assert {m.filename for m in members}==set(manifest['payload']) and sum(m.file_size for m in members)<4*1024**3
  for m in members:
   p=root/m.filename;assert not m.is_dir() and p.resolve().is_relative_to(root) and not stat.S_ISLNK(m.external_attr>>16)
   p.parent.mkdir(parents=True,exist_ok=True)
   with z.open(m) as src,p.open('xb') as dst:shutil.copyfileobj(src,dst)
   p.chmod(0o755 if (m.external_attr>>16)&0o111 else 0o644);assert sha(p)==manifest['payload'][m.filename]
 report['complete_extracted_inventory_verified']=True
 app=root/'AIRK.app';image=app/'Contents/MacOS/airk-launcher-image';launcher=owner/'airk-launcher';shutil.copy2(image,launcher);launcher.chmod(0o700);assert sha(launcher)==sha(image)
 before={name:sha(root/name) for name in manifest['payload']};launcher_before=sha(launcher)
 value='0083;'+format(int(time.time()),'x')+';AIRKLocalQualification;'+str(uuid.uuid4())
 for name,p in [('app',app),('standalone',launcher)]:
  command('set-owned-'+name+'-quarantine',['/usr/bin/xattr','-w','com.apple.quarantine',value,p])
  command('read-owned-'+name+'-quarantine',['/usr/bin/xattr','-p','com.apple.quarantine',p])
 command('app-strict',['/usr/bin/codesign','--verify','--strict','--verbose=2',app])
 command('app-stapler',['/usr/bin/xcrun','stapler','validate',app])
 command('app-gatekeeper',['/usr/sbin/spctl','--assess','--type','execute','--verbose=4',app],90)
 command('standalone-strict',['/usr/bin/codesign','--verify','--strict','--verbose=2',launcher])
 command('standalone-gatekeeper',['/usr/sbin/spctl','--assess','--type','execute','--verbose=4',launcher],90)
 command('standalone-version',[launcher,'--version'],30)
 report['extracted_file_hashes_stable']=all(sha(root/n)==h for n,h in before.items());report['standalone_sha256']=sha(launcher);report['standalone_hash_stable']=sha(launcher)==launcher_before
 report['original_archive_hash_stable']=sha(archive)==archive_sha
except BaseException as exc:error={'type':type(exc).__name__,'error':str(exc)}
finally:
 assert owner.parent.resolve()==Path(tempfile.gettempdir()).resolve() and owner.name.startswith('airk-notarized-transfer-') and not owner.is_symlink();shutil.rmtree(owner)
 report.update({'checks':rows,'error':error,'owned_temp_removed':not owner.exists(),'target_absent':not (run/'target').exists() and not (source/'target').exists()})
 report['status']='passed' if not error and all(row.get('exit_code')==0 for row in rows) and report.get('extracted_file_hashes_stable') and report.get('original_archive_hash_stable') else 'failed'
 (run/'evidence/notarized-transfer-proof.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
