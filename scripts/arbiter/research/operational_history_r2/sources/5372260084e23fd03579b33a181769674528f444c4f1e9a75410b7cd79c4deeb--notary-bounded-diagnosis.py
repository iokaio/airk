# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,plistlib,re,shutil,subprocess,tempfile,time,uuid
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence';runtime=source/'artifacts/neo/signed-notarized-r2/runtime';report={'schema':'airk.neo-notary-transfer-diagnosis/1','original_quarantine_experiment_status':'failed; unchanged','policy_changes':False,'new_submission':False,'signing':False}
def call(command,timeout=60):
 try:
  p=subprocess.run(list(map(str,command)),capture_output=True,text=True,timeout=timeout);return {'command':list(map(str,command)),'exit_code':p.returncode,'stdout':p.stdout[-16000:],'stderr':p.stderr[-16000:]}
 except subprocess.TimeoutExpired:return {'command':list(map(str,command)),'timed_out':True}
with tempfile.TemporaryDirectory(prefix='airk-cli-notary-readonly-') as temporary:
 owner=Path(temporary).resolve(strict=True);launcher=owner/'airk-launcher';image=runtime/'AIRK.app/Contents/MacOS/airk-launcher-image';shutil.copyfile(image,launcher);launcher.chmod(0o700)
 report['standalone_checks']=[call(['/usr/bin/codesign','--verify','--strict','--verbose=4','--check-notarization',launcher],90),call(['/usr/bin/codesign','--verify','--strict','-R','=anchor apple generic and certificate leaf[subject.OU] = "5PX7D99PXA" and certificate leaf = H"4d8496dcab73794378194d040d9177fb28a0f982"',launcher]),call(['/usr/bin/codesign','--display','--verbose=4',launcher]),call([launcher,'--version'],30)]
 display=report['standalone_checks'][2]['stderr'];m=re.search(r'^CDHash=(\w+)$',display,re.M);report['standalone_cdhash']=m.group(1) if m else None
report['owned_copy_removed']=not owner.exists()
prior=json.loads((evidence/'notarized-transfer-proof.json').read_text());path=next(row['command'][-1] for row in prior['checks'] if row['name']=='standalone-gatekeeper');unique=Path(path).parent.name;assert re.fullmatch(r'airk-notarized-transfer-[a-z0-9_]+',unique)
report['exact_prior_quarantine_path_os_log']=call(['/usr/bin/log','show','--last','30m','--style','compact','--predicate','eventMessage CONTAINS "'+unique+'"'],45)
uid=os.getuid();assert uid==501 and Path('/dev/console').stat().st_uid==uid;domain=f'gui/{uid}';label='io.ioka.airk.build.notary-log.'+uuid.uuid4().hex;plist=run/'notary-log-owned.plist';child=run/'notary-log-child.py';result=evidence/'accepted-notary-public-log.json';assert not any(p.exists() for p in [plist,child,result])
submission='d008dddf-0a23-4cb8-8f70-edc7b9a6387b'
code='''import json,subprocess,re
from pathlib import Path
r=Path(RUN);p=subprocess.run(['/usr/bin/xcrun','notarytool','log',SUBMISSION,'--keychain-profile','airk-notary','--keychain',str(Path.home()/'Library/Keychains/airk-signing.keychain-db')],capture_output=True,text=True,timeout=45)
if p.returncode==0:
 data=json.loads(p.stdout);safe={k:data[k] for k in ['jobId','status','statusSummary','statusCode','sha256','issues','ticketContents'] if k in data};report={'exit_code':0,'public_notary_scan':safe,'credential_values_logged':False}
else:
 clean=re.sub(r'[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}','<redacted-email>',p.stderr);report={'exit_code':p.returncode,'stderr_redacted':clean[-4096:]}
(r/'evidence/accepted-notary-public-log.json').write_text(json.dumps(report,indent=2)+'\\n')
'''.replace('RUN',repr(str(run))).replace('SUBMISSION',repr(submission))
child.write_text(code);child.chmod(0o600);python=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools/python3.13';plist.write_bytes(plistlib.dumps({'Label':label,'ProgramArguments':[str(python),str(child)],'RunAtLoad':True,'WorkingDirectory':str(run),'StandardOutPath':str(evidence/'notary-log-gui.stdout.log'),'StandardErrorPath':str(evidence/'notary-log-gui.stderr.log')}));plist.chmod(0o600)
try:
 subprocess.run(['/bin/launchctl','bootstrap',domain,str(plist)],check=True,timeout=15);deadline=time.monotonic()+55
 while not result.exists() and time.monotonic()<deadline:time.sleep(.2)
 report['accepted_public_notary_log']=json.loads(result.read_text()) if result.exists() else {'timed_out':True}
finally:
 p=subprocess.run(['/bin/launchctl','bootout',domain+'/'+label],capture_output=True,text=True,timeout=15);absent=subprocess.run(['/bin/launchctl','print',domain+'/'+label],capture_output=True,text=True,timeout=15);assert plist.parent==run and not plist.is_symlink();plist.unlink(missing_ok=True);report['gui_cleanup']={'bootout_exit':p.returncode,'job_absent':absent.returncode!=0,'plist_removed':not plist.exists()}
scan=report.get('accepted_public_notary_log',{}).get('public_notary_scan',{});tickets=scan.get('ticketContents') or [];report['standalone_cdhash_in_accepted_ticket']=any(row.get('cdhash')==report.get('standalone_cdhash') for row in tickets if isinstance(row,dict));report['target_absent']=not (source/'target').exists() and not (run/'target').exists()
(evidence/'notary-bounded-diagnosis.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
