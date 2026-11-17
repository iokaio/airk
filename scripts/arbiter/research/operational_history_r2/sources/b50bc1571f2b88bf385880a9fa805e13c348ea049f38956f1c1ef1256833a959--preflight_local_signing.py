# SPDX-License-Identifier: Apache-2.0
"""Read-only existing Azure/profile/tool preflight; no tokens/login/sign/build."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
AZPY=Path('C:/Program Files/Microsoft SDKs/Azure/CLI2/python.exe')
PROFILE='/subscriptions/7c791224-1e94-4b7b-9a3f-2adb6032b435/resourceGroups/wfp-signing/providers/Microsoft.CodeSigning/codeSigningAccounts/ioka-llc-signing/certificateProfiles/WritingForgePro'
SIGN_ACTION='Microsoft.CodeSigning/certificateProfiles/Sign/action'
identity=json.loads((ROOT/'scripts/signing-identity.json').read_bytes())
assert identity['windows']['artifact_signing']['account']=='ioka-llc-signing'
assert identity['windows']['artifact_signing']['certificate_profile']=='WritingForgePro'
assert identity['windows']['artifact_signing']['endpoint']=='https://wus2.codesigning.azure.net'
records=[]
def az(name,arguments):
    command=[str(AZPY),'-IBm','azure.cli',*arguments,'--output','json','--only-show-errors']
    start=time.monotonic()
    result=subprocess.run(command,capture_output=True,text=True,encoding='utf-8',timeout=60)
    row={'name':name,'command':command,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start}
    if result.returncode==0:
        row['result']=json.loads(result.stdout)
    else:
        # None of these commands requests tokens; do not retain unexpected raw text.
        row['failure']='read-only Azure CLI command failed; no login/signing attempted'
        row['stderr_sha256']=hashlib.sha256(result.stderr.encode()).hexdigest()
    records.append(row)
    return row

account=az('existing-account',['account','show','--query','{state:state,subscription:id,tenant:tenantId,user_type:user.type}'])
profile=az('pinned-profile',['resource','show','--ids',PROFILE,'--query',
    '{id:id,name:name,profileType:properties.profileType,provisioningState:properties.provisioningState,status:properties.status}'])
permission=az('effective-profile-signer-permission',['rest','--method','get','--url',
    'https://management.azure.com'+PROFILE+'/providers/Microsoft.Authorization/permissions?api-version=2022-04-01',
    '--query',"value[?contains(dataActions, 'Microsoft.CodeSigning/certificateProfiles/Sign/action')].{dataActions:dataActions,notDataActions:notDataActions}"])
spec=importlib.util.spec_from_file_location('existing_local_signing_owner',ROOT/'scripts/local_signing.py')
adapter=importlib.util.module_from_spec(spec);spec.loader.exec_module(adapter)
dlib=adapter.client_root()/adapter.CLIENT['dlib']
client={'path':str(dlib),'exists':dlib.is_file(),'pinned_version':adapter.CLIENT['version'],'expected_sha256':adapter.CLIENT['dlib_sha256']}
if dlib.is_file():
    client['actual_sha256']=adapter.SIGN.digest(dlib)
    try:
        adapter.check_client(dlib)
        client['pinned_digest_and_valid_microsoft_signature']=True
    except Exception as error:
        client['pinned_digest_and_valid_microsoft_signature']=False
        client['failure']=type(error).__name__+': '+str(error)
tools={}
for name,path in {'inno':Path('C:/Users/tdotn/AppData/Local/Programs/Inno Setup 6/ISCC.exe'),
    'signtool':Path('C:/Program Files (x86)/Windows Kits/10/bin/10.0.26100.0/x64/signtool.exe'),
    'node':ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'}.items():
    tools[name]={'path':str(path),'exists':path.is_file()}
    if path.is_file():
        tools[name]['sha256']=hashlib.sha256(path.read_bytes()).hexdigest()
enabled=account.get('result',{}).get('state')=='Enabled'
active=profile.get('result',{}).get('status')=='Active' and profile.get('result',{}).get('provisioningState')=='Succeeded' and profile.get('result',{}).get('profileType')=='PublicTrust'
grants=permission.get('result',[])
can_sign=any(SIGN_ACTION in row.get('dataActions',[]) and not row.get('notDataActions') for row in grants)
receipt={'schema':'airk.private-local-signing-preflight/1','read_only':True,'azure':records,'client':client,'tools':tools,
    'existing_account_authenticated_arm_reads':all(row['exit_code']==0 for row in records),
    'pinned_profile_active_and_sign_data_action_granted':enabled and active and can_sign,
    'local_tool_prerequisites_present':all(row['exists'] for row in tools.values()),
    'credentials_or_tokens_printed':False,'login_or_new_auth_flow_started':False,
    'actual_signing_build_install_or_service_change':False,'successful_signature_or_package_claimed':False,
    'authoritative_references':['https://learn.microsoft.com/en-us/azure/artifact-signing/tutorial-assign-roles',
    'https://learn.microsoft.com/en-us/rest/api/authorization/permissions/list-for-resource?view=rest-authorization-2022-04-01']}
out=HERE/'local-signing-preflight.json';assert not out.exists()
out.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps({key:receipt[key] for key in ['existing_account_authenticated_arm_reads','pinned_profile_active_and_sign_data_action_granted','local_tool_prerequisites_present']}))
print(json.dumps({'client':client,'receipt':str(out)}))
