# SPDX-License-Identifier: Apache-2.0
"""Temporary same-UID GUI LaunchAgent runs read-only keychain probes only."""
import json,os,plistlib,re,subprocess,time,uuid
from pathlib import Path
run=Path(__file__).resolve().parent;evidence=run/'evidence'
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('explorer-1812-')
uid=os.getuid();console_uid=Path('/dev/console').stat().st_uid
domain=f'gui/{uid}';label='io.ioka.airk.build.keychain-probe.'+uuid.uuid4().hex
assert re.fullmatch(r'io\.ioka\.airk\.build\.keychain-probe\.[0-9a-f]{32}',label)
exists=subprocess.run(['/bin/launchctl','print',domain],capture_output=True,text=True,timeout=15)
receipt={'schema':'airk.neo-gui-keychain-probe/1','ssh_uid':uid,'console_uid':console_uid,'uid_matches':uid==console_uid,'gui_domain_exists':exists.returncode==0,'label':label,'operations':'Read-only settings/identity/search-list queries; no password lookup, unlock, signing, trust or ACL changes.'}
result=evidence/'gui-keychain-context.json';script=run/'gui-keychain-context-child.py';plist=run/'gui-keychain-context.plist'
assert not script.exists() and not plist.exists() and not result.exists()
if uid!=console_uid or exists.returncode:
 receipt['status']='unavailable';receipt['reason']='SSH UID does not match active console UID or no existing GUI domain.'
else:
 child='''import json,os,subprocess\nfrom pathlib import Path\nr=Path(RUN)\nk=Path.home()/'Library/Keychains/airk-signing.keychain-db'\ndef query(command):\n try:\n  p=subprocess.run(command,capture_output=True,text=True,timeout=15);return {'command':command,'exit_code':p.returncode,'output':(p.stdout+p.stderr)[-8192:]}\n except subprocess.TimeoutExpired:return {'command':command,'timed_out':True}\nlisted=subprocess.run(['/usr/bin/security','list-keychains','-d','user'],capture_output=True,text=True,timeout=15)\no={'uid':os.getuid(),'session':query(['/bin/launchctl','managername']),'settings':query(['/usr/bin/security','show-keychain-info',str(k)]),'identity':query(['/usr/bin/security','find-identity','-v','-p','codesigning',str(k)]),'airk_keychain_in_search_list':str(k) in listed.stdout,'readonly':True}\n(r/'evidence/gui-keychain-context.json').write_text(json.dumps(o,indent=2)+'\\n')\n'''.replace('RUN',repr(str(run)))
 script.write_text(child);script.chmod(0o600)
 payload={'Label':label,'ProgramArguments':[str(Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools/python3.13'),str(script)],'RunAtLoad':True,'ProcessType':'Background','StandardOutPath':str(evidence/'gui-keychain-context.stdout.log'),'StandardErrorPath':str(evidence/'gui-keychain-context.stderr.log'),'WorkingDirectory':str(run)}
 plist.write_bytes(plistlib.dumps(payload));plist.chmod(0o600)
 try:
  started=subprocess.run(['/bin/launchctl','bootstrap',domain,str(plist)],capture_output=True,text=True,timeout=15)
  receipt['bootstrap_exit']=started.returncode
  if started.returncode:receipt['status']='unavailable';receipt['bootstrap_error']=started.stderr[-4096:]
  else:
   deadline=time.monotonic()+50
   while time.monotonic()<deadline and not result.exists():time.sleep(.2)
   receipt['status']='observed' if result.exists() else 'timed_out'
   if result.exists():receipt['gui_observation']=json.loads(result.read_text())
 finally:
  stopped=subprocess.run(['/bin/launchctl','bootout',domain+'/'+label],capture_output=True,text=True,timeout=15)
  absent=subprocess.run(['/bin/launchctl','print',domain+'/'+label],capture_output=True,text=True,timeout=15)
  receipt['bootout_exit']=stopped.returncode;receipt['job_absent']=absent.returncode!=0
  assert plist.parent==run and not plist.is_symlink();plist.unlink(missing_ok=True);receipt['temporary_plist_removed']=not plist.exists()
(evidence/'gui-keychain-probe-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt,indent=2))
