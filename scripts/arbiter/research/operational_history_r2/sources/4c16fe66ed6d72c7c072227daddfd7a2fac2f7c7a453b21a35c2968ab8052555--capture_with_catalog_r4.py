# SPDX-License-Identifier: Apache-2.0
"""Actual private native I3 scope/domain capture; lexical only, no build/model."""
from pathlib import Path
import argparse
import hashlib
import http.client
import json
import os
import re
import ssl
import time
import types
import urllib.parse

ROOT=Path(__file__).resolve().parents[3]

def sha(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def load(path):
    raw=path.read_bytes();module=types.ModuleType('_captured_'+path.stem.replace('-','_'));module.__file__=str(path)
    exec(compile(raw,str(path),'exec'),module.__dict__)
    return module

def prepare_owned_paths(owner):
    owner=owner.resolve(strict=True)
    roots=[owner/'source-a',owner/'source-b'];data=owner/'data';state=owner/'state';control=state/'control'
    # Native layout.rs rejects unknown nonempty state BEFORE it claims ownership.
    # Match the working W4 fixture: harness creates source/data only; native stage
    # alone creates and claims state/control. Never repair/delete a foreign state.
    if os.path.lexists(state) or os.path.lexists(control):raise ValueError('native-owned state/control must be absent before stage')
    planned=[*roots,data,control]
    if any(not path.is_relative_to(owner) for path in planned):raise ValueError('planned fixture path escaped owner')
    if any(a==b or a.is_relative_to(b) or b.is_relative_to(a) for i,a in enumerate(planned) for b in planned[i+1:]):raise ValueError('source/data/control overlap')
    for path in [*roots,data]:path.mkdir(mode=0o700)
    if any(path.resolve(strict=True)!=path or not path.is_dir() for path in [*roots,data]):raise ValueError('source/data path redirected')
    if os.path.lexists(state) or os.path.lexists(control):raise ValueError('harness created native-owned state')
    return roots,data,control

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime-dir',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();runtime=args.runtime_dir.resolve(strict=True);output=args.output.absolute()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts/advanced-split'):
        raise ValueError('fresh private campaign output required')
    output.mkdir(parents=True);started=time.monotonic()
    pins={p:sha(p) for p in (ROOT/'scripts').glob('*.py')}
    pins[Path(__file__).resolve()]=sha(Path(__file__).resolve())
    for name in ['airk.exe','airkctl.exe','airk-runtime.json','airk-build.json','airk-install.json']:
        path=runtime/name
        if not path.is_file() or path.is_symlink():raise ValueError('required native pair input missing/linked: '+name)
        pins[path]=sha(path)
    clients=load(ROOT/'scripts/check-service-clients.py');transport=load(ROOT/'scripts/http_fixture.py')
    commands=[];fixture=None;primary=None;result={};owner_path=None
    def persist(name,value):
        (output/name).write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8')
    class RecordedFixture(clients.Fixture):
        def execute(self,command,**kw):
            ordinal=len(commands)+1
            check=kw.pop('check',True)
            response=super().execute(command,check=False,**kw)
            # Launch code is an explicit native secret; it stays only in RAM.
            secret=command[-2:]==['admin','url']
            row={'argv':command,'exit':response.returncode,'stdout_sha256':hashlib.sha256(response.stdout.encode()).hexdigest(),'stderr_sha256':hashlib.sha256(response.stderr.encode()).hexdigest(),'launch_secret_not_retained':secret}
            commands.append(row)
            if not secret:
                (output/f'command-{ordinal:03}.stdout').write_text(response.stdout,encoding='utf-8')
                (output/f'command-{ordinal:03}.stderr').write_text(response.stderr,encoding='utf-8')
            persist('commands.json',commands)
            if check and response.returncode:
                raise AssertionError(f'Native command {ordinal} refused: {response.stderr[-4096:]}')
            return response
    try:
        with clients.owned_client_fixture() as temporary:
            owner_path=Path(temporary);roots,data,planned_control=prepare_owned_paths(owner_path)
            persist('path-preflight.json',{'owner':str(owner_path.resolve()),'sources':[str(p.resolve()) for p in roots],'data':str(data.resolve()),'control':str(planned_control),'sources_data_existing_disjoint':True,'native_state_control_absent':True,'control_creation_owner':'native runtime stage'})
            source_seals={}
            for n,root in enumerate(roots):
                path=root/'native-scope.txt';path.write_text(f'Native lexical scope marker boundaryscopeproof origin {n}.\n',encoding='utf-8');source_seals[str(path)]=sha(path)
            fixture=RecordedFixture(runtime,owner_path,command_timeout=120,job_timeout=180)
            assert fixture.managed_installation and fixture.control.resolve(strict=False)==planned_control
            fixture_error=None
            try:
                assert not planned_control.parent.exists() and not planned_control.exists()
                fixture.start();assert fixture.control.samefile(planned_control) and planned_control.is_dir();health=fixture.rpc('health')['observation'];persist('health-before.json',health)
                for n,root in enumerate(roots):
                    name=f'scope-{n}';fixture.register(name,root,data)
                    fixture.configure(name,{'operation':'vectors','enabled':False})
                    job=fixture.job(name,'index');persist(f'index-{n}.json',job)
                    status=fixture.published_status(name);persist(f'status-{n}.json',status)
                descriptor=fixture.rpc('http_endpoint')['endpoint']
                assert descriptor['service_instance']==health['instance']
                parsed=urllib.parse.urlsplit(descriptor['url']);assert parsed.hostname=='127.0.0.1' and parsed.scheme=='https'
                ca=transport.private_bytes(str(fixture.control/'http-1/ca.pem'),fixture.control,8192).decode('ascii')
                assert hashlib.sha256(ssl.PEM_cert_to_DER_cert(ca)).hexdigest()==descriptor['ca_sha256']
                context=ssl.create_default_context();context.verify_flags|=ssl.VERIFY_X509_STRICT;context.load_verify_locations(cadata=ca)
                network=http.client.HTTPSConnection('127.0.0.1',parsed.port,context=context,timeout=60)
                network.connect();assert hashlib.sha256(network.sock.getpeercert(binary_form=True)).hexdigest()==descriptor['certificate_sha256']
                origin=f'https://127.0.0.1:{parsed.port}';cookie=None;csrf=None
                def request(path,body,retain=None):
                    nonlocal cookie,csrf
                    raw=json.dumps(body,separators=(',',':')).encode()
                    headers={'Content-Type':'application/json','Origin':origin,'Sec-Fetch-Site':'same-origin'}
                    if cookie:headers['Cookie']=cookie
                    if csrf:headers['X-AIRK-CSRF']=csrf
                    if network.sock is None:network.connect()
                    assert hashlib.sha256(network.sock.getpeercert(binary_form=True)).hexdigest()==descriptor['certificate_sha256']
                    network.request('POST',path,raw,headers);response=network.getresponse();payload=response.read(32*1024*1024+1)
                    assert len(payload)<=32*1024*1024
                    value=json.loads(payload)
                    if retain:
                        (output/f'{retain}.response.raw').write_bytes(payload)
                        persist(f'{retain}.json',{'path':path,'request':body,'status':response.status,'headers':response.getheaders(),'envelope':value})
                    assert response.status==200 and value.get('ok') is True,(path,response.status,value)
                    return value,response.getheaders()
                launch=fixture.run('--service-runtime-dir',fixture.control,'admin','url').stdout.strip()
                code=urllib.parse.parse_qs(urllib.parse.urlsplit(launch).fragment)['c'][0]
                exchange,headers=request('/admin/api/v1/session',{'code':code})
                assert exchange['result']['service_instance']==health['instance']
                cookie=next(v.split(';',1)[0] for k,v in headers if k.lower()=='set-cookie');csrf=exchange['result']['csrf'];del launch,code,exchange
                selection=fixture.scopes['scope-0'];catalog=fixture.rpc({'catalog_host_context':{'key':selection['context_key'],'expected_catalog_revision':None}})['descriptor']
                indexes=sorted(fixture.scopes[name]['index_id'] for name in ['scope-0','scope-1'])
                projects=sorted(fixture.scopes[name]['project_ids'][0] for name in ['scope-0','scope-1'])
                for kind in ['indexes','projects','memberships']:
                    page_body={'catalog_page':{'request':{'kind':kind,'index_id':None,'project_id':None,'after':None,'expected_catalog_revision':catalog['catalog_revision'],'limit':32,'max_bytes':48000}}}
                    page,_=request('/admin/api/v1/call/catalog.page',page_body,'catalog-'+kind)
                    observed=page['result']['page']
                    assert observed['namespace']==catalog['namespace'] and observed['revision']==catalog['catalog_revision'] and observed['continuation'] is None
                grant_body={'admin_scope_grant':{'selection':{'namespace':catalog['namespace'],'expected_catalog_revision':catalog['catalog_revision'],'index_ids':indexes,'project_ids':projects}}}
                grant,_=request('/admin/api/v1/call/search.select_scope',grant_body,'grant')
                arguments={'query':'boundaryscopeproof','mode':'lexical','limit':4,'max_bytes':32768,'verification':'off'}
                read_body={'tools_call':{'tool':'search','arguments':arguments,'selection':'admin','response_mode':'full','usage_client':'unknown'}}
                read,_=request('/admin/api/v1/call/search.call',read_body,'read')
                tool=read['result']['result'];combined=tool.get('structuredContent') or json.loads(tool['content'][0]['text'])
                assert combined['schema']=='airk.combined-read/1'
                expected={'owner':'admin','scope_identity':grant['result']['scope_identity'],'catalog_revision':grant['result']['catalog_revision']}
                assert combined['selection']['selection']==expected
                assert combined['publication_vector']['authority']!=expected['scope_identity']
                assert combined['selection']['index_ids']==indexes and combined['selection']['project_ids']==projects
                assert [m['index_id'] for m in combined['members']]==indexes
                assert [m['index_id'] for m in combined['publication_vector']['members']]==indexes
                request('/admin/api/v1/session/logout',{});network.close();del cookie,csrf
                fixture.assert_alive();assert all(sha(Path(path))==pin for path,pin in source_seals.items())
                persist('source-seals.json',source_seals)
                result={'grant_source':expected,'aggregate_authority':combined['publication_vector']['authority'],'index_ids':indexes,'project_ids':projects,'runtime_identity':fixture.compatibility,'native_scope_domain_controls_passed':True}
            except BaseException as error:
                fixture_error=error
                raise
            finally:
                try:fixture.close()
                except BaseException as cleanup:
                    persist('cleanup-error.json',{'type':type(cleanup).__name__,'message':str(cleanup)[:4096]})
                    if fixture_error is None:raise
                    fixture_error.add_note('Cleanup also failed; preserve owned fixture: '+str(owner_path))
                for name in ['managed-start.json','managed-stop-request.json','managed-stop-result.json','service.stderr']:
                    path=owner_path/name
                    if path.exists():(output/name).write_bytes(path.read_bytes())
                result['cleanup']={'managed_stop':fixture.managed_stop,'owned_child_exit':fixture.child.returncode if fixture.child else None}
    except BaseException as error:
        primary={'type':type(error).__name__,'message':str(error)[:4096]}
        raise
    finally:
        drift=[str(path) for path,pin in pins.items() if not path.exists() or sha(path)!=pin]
        persist('execution.json',{'schema':'airk.private-actual-i3-scope-capture/1','complete':primary is None,'error':primary,'elapsed_seconds':time.monotonic()-started,'owner_fixture':str(owner_path),'fixture_removed':owner_path is not None and not owner_path.exists(),'input_pins':{str(k):v for k,v in pins.items()},'post_input_drift':drift,'result':result,'qualification':'Actual private native lexical I3 transport capture; no model, installer publication or user-install change'})
        if drift:raise ValueError('capture inputs changed')
    print(json.dumps({'complete':True,'output':str(output),'grant_sha256':sha(output/'grant.response.raw'),'read_sha256':sha(output/'read.response.raw'),'cleanup_observed':result['cleanup']}),flush=True)

if __name__=='__main__':main()
