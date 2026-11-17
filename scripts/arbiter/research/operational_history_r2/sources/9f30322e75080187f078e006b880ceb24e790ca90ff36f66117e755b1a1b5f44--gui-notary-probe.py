# SPDX-License-Identifier: Apache-2.0
import json,os,plistlib,subprocess,time,uuid
from pathlib import Path
run=Path(__file__).resolve().parent;out=run/'evidence';uid=os.getuid();assert uid==501 and Path('/dev/console').stat().st_uid==uid
domain=f'gui/{uid}';subprocess.run(['/bin/launchctl','print',domain],check=True,stdout=subprocess.DEVNULL)
label='io.ioka.airk.build.notary-auth.'+uuid.uuid4().hex;plist=run/'gui-notary-owned.plist';child=run/'gui-notary-child.py';result=out/'gui-notary-child-result.json';assert not any(p.exists() for p in [plist,child,result])
code='''import json,os,re,signal,subprocess,time
from pathlib import Path
r=Path(RUN);command=['/usr/bin/xcrun','notarytool','history','--keychain-profile','airk-notary','--keychain',str(Path.home()/'Library/Keychains/airk-signing.keychain-db'),'--output-format','json'];start=time.monotonic()
p=subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True,start_new_session=True)
timed=False
try:_,raw=p.communicate(timeout=45)
except subprocess.TimeoutExpired:
 timed=True;os.killpg(p.pid,signal.SIGTERM)
 try:_,raw=p.communicate(timeout=3)
 except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);_,raw=p.communicate(timeout=3)
clean=re.sub(r'[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}','<redacted-email>',raw)
clean=re.sub(r'(?i)(password|token|secret|authorization|api[-_ ]?key)(\\s*[:=]\\s*)([^\\s,;]+)',r'\\1\\2<redacted>',clean)
t=raw.lower();kind='timeout' if timed else 'authenticated' if p.returncode==0 else 'keychain_locked' if 'locked' in t else 'session_access' if 'interaction' in t else 'missing_profile' if ('profile' in t or 'credentials' in t) and any(x in t for x in ['not found','missing','no ','could not find']) else 'authentication_rejected' if any(x in t for x in ['401','unauthorized','invalid credentials']) else 'other_public_diagnostic'
try:os.killpg(p.pid,0);absent=False
except ProcessLookupError:absent=True
report={'exit_code':p.returncode,'classification':kind,'stderr_redacted':clean[-4096:],'timed_out':timed,'process_group_absent':absent,'elapsed_seconds':round(time.monotonic()-start,3),'session':subprocess.check_output(['/bin/launchctl','managername'],text=True).strip(),'history_stdout_discarded':True}
(r/'evidence/gui-notary-child-result.json').write_text(json.dumps(report,indent=2)+'\\n')
'''.replace('RUN',repr(str(run)))
child.write_text(code);child.chmod(0o600);python=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools/python3.13'
plist.write_bytes(plistlib.dumps({'Label':label,'ProgramArguments':[str(python),str(child)],'RunAtLoad':True,'WorkingDirectory':str(run),'StandardOutPath':str(out/'gui-notary.stdout.log'),'StandardErrorPath':str(out/'gui-notary.stderr.log')}));plist.chmod(0o600)
report={'schema':'airk.neo-gui-readonly-notary-auth/1','uid':uid,'label':label,'signing':False,'notarization_submission':False,'keychain_changes':False,'password_prompt_requested':False}
try:
 subprocess.run(['/bin/launchctl','bootstrap',domain,str(plist)],check=True,timeout=15)
 deadline=time.monotonic()+60
 while not result.exists() and time.monotonic()<deadline:time.sleep(.2)
 report['observation']=json.loads(result.read_text()) if result.exists() else {'supervisor_timeout':True}
finally:
 p=subprocess.run(['/bin/launchctl','bootout',domain+'/'+label],capture_output=True,text=True,timeout=15);absent=subprocess.run(['/bin/launchctl','print',domain+'/'+label],capture_output=True,text=True,timeout=15)
 assert plist.parent==run and not plist.is_symlink();plist.unlink(missing_ok=True)
 report['cleanup']={'bootout_exit':p.returncode,'job_absent':absent.returncode!=0,'plist_removed':not plist.exists()};report['target_absent']=not (run/'target').exists()
 (out/'gui-notary-probe-result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
