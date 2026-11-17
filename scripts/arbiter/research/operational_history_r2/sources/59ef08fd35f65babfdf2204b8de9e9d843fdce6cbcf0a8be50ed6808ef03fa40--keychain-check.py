# SPDX-License-Identifier: Apache-2.0
"""Read the existing project signing context; only its scoped unlock is allowed."""
import json,os,shlex,subprocess
from pathlib import Path
run=Path(__file__).resolve().parent
assert (run/'cleanup.json').is_file() and json.loads((run/'cleanup.json').read_text())['exit_code']==0
keychain=Path.home()/'Library/Keychains/airk-signing.keychain-db'
def public(command):
 p=subprocess.run(command,text=True,capture_output=True,timeout=30)
 return {'command':command,'exit_code':p.returncode,'output':(p.stdout+p.stderr)[-8192:]}
listed=subprocess.run(['/usr/bin/security','list-keychains','-d','user'],capture_output=True,text=True,timeout=30)
report={'schema':'airk.neo-signing-session/1','uid':os.getuid(),'keychain_exists':keychain.is_file(),'keychain_in_original_search_list':str(keychain) in listed.stdout,'search_list_query_exit':listed.returncode,
 'session':public(['/bin/launchctl','managername']),
 'settings':public(['/usr/bin/security','show-keychain-info',str(keychain)]),
 'identity':public(['/usr/bin/security','find-identity','-v','-p','codesigning',str(keychain)]),
 'changes':['No certificate trust, identity, private-key ACL, partition list or keychain search-list change.']}
(run/'evidence/keychain-diagnostic.json').write_text(json.dumps(report,indent=2)+'\n')
lookup=subprocess.run(['/usr/bin/security','find-generic-password','-s','airk-signing-keychain-password','-w'],capture_output=True,text=True,timeout=30)
unlock={'schema':'airk.neo-signing-unlock/1','credential_item_service':'airk-signing-keychain-password','credential_lookup_exit':lookup.returncode,'credential_available':lookup.returncode==0 and bool(lookup.stdout),'credential_values_logged':False,'keychain':str(keychain)}
if unlock['credential_available']:
 password=lookup.stdout.rstrip('\r\n')
 if not password or len(password)>4096 or any(c in password for c in '\r\n\0'):
  unlock['unlock_attempted']=False;unlock['reason']='Stored credential has an unsupported shape; manual unlock required.'
 else:
  # security's established interactive stdin API keeps the secret out of argv,
  # environment, exceptions and logs. Credential-tool diagnostics stay private.
  result=subprocess.run(['/usr/bin/security','-i'],input=shlex.join(['unlock-keychain','-p',password,str(keychain)])+'\n',capture_output=True,text=True,timeout=30)
  unlock.update(unlock_attempted=True,unlock_exit_code=result.returncode,diagnostics_suppressed=True)
  password=None;result=None
else:
 unlock['unlock_attempted']=False
 unlock['reason']='User interaction required to read the existing credential.' if 'interaction' in lookup.stderr.lower() else 'Existing purpose-specific credential could not be read.'
lookup=None
after=subprocess.run(['/usr/bin/security','list-keychains','-d','user'],capture_output=True,text=True,timeout=30)
unlock['search_list_unchanged']=after.returncode==listed.returncode and after.stdout==listed.stdout
(run/'evidence/keychain-unlock.json').write_text(json.dumps(unlock,indent=2)+'\n')
print(json.dumps({'keychain_exists':report['keychain_exists'],'keychain_in_search_list':report['keychain_in_original_search_list'],'session':report['session']['output'].strip(),'identity_exit':report['identity']['exit_code'],'unlock':unlock},indent=2))
