# SPDX-License-Identifier: Apache-2.0
"""Compose external installed-test contract corrections only; no compilation/run."""
from pathlib import Path
import hashlib,json,difflib,subprocess
A=Path(__file__).parent;W=A/'p10-worktree';O=A/'p14-installed-editor-contract-proposed-r2'
(O/'base').mkdir(parents=True,exist_ok=True);(O/'source').mkdir(exist_ok=True);files={}
def put(name,s):
 b=(W/name).read_bytes() if (W/name).exists() else b''
 if b'\r\n' in b:s=s.replace('\r\n','\n').replace('\n','\r\n')
 for folder,data in [('base',b),('source',s.encode())]:
  p=O/folder/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
 files[name]=(b,s.encode())
put('packages/vscode/src/test/installedEditorContracts.ts',r'''// SPDX-License-Identifier: Apache-2.0
/** External qualification driver only. Raw native replies remain authoritative. */
import {strict as assert} from 'node:assert';
import {validateCombinedCatalogSession} from '../catalogSession';
import {materializeNativeMetadata} from '../nativeMetadata';
import {readResultBody} from '../sourceVerification';
import {combinedRows} from '../explorerSearch';
export function installedSelector(definition:any,result:any,selection:any):any{
    assert.ok(Array.isArray(definition.args));
    assert.equal(definition.args.filter((arg:string)=>arg==='--combined-session').length,1,'Actual combined native selector');
    for(const arg of ['--catalog-session','--root','--data-dir','--collection','--service-project'])assert.ok(!definition.args.includes(arg),'No reconstructed legacy selector');
    const at=definition.args.indexOf('--combined-session');assert.ok(at>=0&&typeof definition.args[at+1]==='string');
    const selector=validateCombinedCatalogSession(JSON.parse(definition.args[at+1]));
    assert.deepEqual(selector,result.outcome.selection.session,'Complete native reviewed selection');
    assert.deepEqual(selector.index_ids,[selection.index]);assert.deepEqual(selector.project_ids,[selection.project]);assert.equal(selector.grant_id,selection.grant);
    return selector;
}
export function installedToolBody(wire:any):any{
    assert.ok(wire&&typeof wire==='object');assert.equal(wire.error,undefined,'No RPC error in successful tool envelope');
    const result=wire.result;assert.ok(result&&typeof result==='object'&&!Array.isArray(result));
    assert.ok(result.isError===undefined||typeof result.isError==='boolean','Actual typed outer error flag');
    assert.ok(Array.isArray(result.content)&&result.content.length===1&&result.content[0].type==='text'&&typeof result.content[0].text==='string');
    const body=JSON.parse(result.content[0].text);
    if(result.structuredContent!==undefined)assert.deepEqual(result.structuredContent,body,'Complete text and structured native copies agree');
    assert.ok(Buffer.byteLength(JSON.stringify(body))<=65536,'Native canonical body bound');return body;
}
export function installedMember(raw:any,selection:any,denied=false):any{
    assert.equal(raw?.schema,'airk.combined-read/1');
    // This is the shipped lossless logical projection, not independent fullreply
    // cryptographic authentication. Untouched raw delivery stays in transport evidence.
    const value=materializeNativeMetadata(raw),scope:any=value.selection;
    assert.deepEqual(scope.project_ids,[selection.project]);assert.deepEqual(scope.index_ids,[selection.index]);
    assert.equal(scope.grant_id,selection.grant);assert.equal(scope.grant_revision,selection.session.expected_grant_revision);
    assert.equal(scope.member_availability,'report');assert.match(scope.identity,/^[a-f0-9]{64}$/);
    assert.ok(Array.isArray(value.members)&&value.members.length===1);
    const member:any=value.members[0];assert.equal(member.index_id,selection.index);assert.equal(member.allocation?.index_id,selection.index);
    assert.ok(member.allocation.max_bytes>0&&member.allocation.deadline_ms>0);
    assert.equal(member.outcome,denied?'failed':'complete');assert.equal(member.is_error,denied);
    if(Object.hasOwn(member.result,'isError'))assert.equal(member.result.isError,denied,'Complete child error envelope agrees');
    assert.equal(value.partial,denied);assert.equal(typeof value.truncated,'boolean');
    const vector:any=value.publication_vector;assert.ok(Array.isArray(vector.members)&&vector.members.length===1);
    assert.equal(vector.members[0].index_id,selection.index);assert.match(vector.authority,/^[a-f0-9]{64}$/);
    assert.ok(vector.observed_to_unix_ms>=vector.observed_from_unix_ms);
    const delivery=raw.delivery_observation?.receipt;
    assert.equal(delivery?.schema,1);assert.equal(delivery.namespace,selection.session.namespace);
    assert.equal(delivery.scope_identity,vector.authority,'Native delivery/vector domain');
    assert.ok(delivery.catalog_revision>=selection.session.expected_catalog_revision,'Delivery may advance catalog revision');
    const body:any=readResultBody(member.result),observed=body.scope_observation;
    assert.equal(observed?.qualifier,'selected_subset');assert.equal(observed.provenance,'user_reviewed_project_grant');
    assert.equal(observed.grant.id,selection.grant);assert.equal(observed.grant.revision,selection.session.expected_grant_revision);assert.match(observed.fingerprint,/^[a-f0-9]{64}$/);
    assert.equal(observed.members.length,1);assert.equal(observed.members[0].index_id,selection.index);
    assert.deepEqual(observed.members[0].memberships.map((row:any)=>row.project_id),[selection.project]);
    if(denied){
        assert.deepEqual(body.error,{schema:1,code:'scoped_read_unavailable',retryable:false});
        assert.equal(vector.members[0].publication,null,'Denied path does not invent an artifact publication');
        assert.ok(Array.isArray(value.ranking));assert.equal(value.ranking.length,0);
        for(const field of ['items','hits','content','evidence'])assert.equal(body[field],undefined,'Denied source carries no evidence');
    }else{
        const rows=combinedRows(value);assert.ok(rows);
        for(const {origin}of rows){
            assert.equal(origin.index_id,selection.index);assert.deepEqual(origin.project_ids,[selection.project]);
            assert.ok(Array.isArray(selection.source_ids)&&selection.source_ids.includes(origin.source_root_id),'Native configured originating source');
            assert.deepEqual(origin.publication,vector.members[0].publication,'Origin retains exact native member publication');
        }
        if(body.publication!==undefined)assert.deepEqual(body.publication,vector.members[0].publication);
    }
    return body; // Derived assertion view only; never replaces canonical export/evidence.
}
export function installedRevocation(body:any,selector:any):void{
    assert.equal(body?.error?.code,'combined_scope_unavailable');assert.equal(body.error.query_submitted,false);
    assert.equal(body.error.retryable,false);assert.equal(body.error.query_replay_safe,false);
    assert.equal(body.requested_scope.confirmed,false);assert.deepEqual(body.requested_scope.selection,selector);
    for(const field of ['items','hits','evidence','result','members'])assert.equal(body[field],undefined);
}
export async function finishInstalledClient(client:{observations:unknown[];close():Promise<unknown>},evidence:any,persist:(name:string,value:unknown)=>Promise<unknown>):Promise<void>{
    evidence.native.push({transport:client.observations});let cleanup:unknown;
    try{await client.close();}catch(error){cleanup=error;evidence.transport_cleanup_failure=String(error);}
    try{await persist('retained-evidence',evidence);}catch(error){
        evidence.retention_failure=String(error);if(!cleanup)cleanup=error;
        console.error('Private native evidence retention failed:',String(error));
    }
    if(cleanup&&!evidence.failure)throw cleanup; // Preserve a primary failure and disclose failed cleanup separately.
}
''')
name='packages/vscode/src/test/installedEditor.ts';s=(W/name).read_text(encoding='utf-8')
s=s.replace("import { readRetryDelay } from '../explorerEvidence';", "import { readRetryDelay } from '../explorerEvidence';\nimport { installedSelector, installedToolBody, installedMember, installedRevocation, finishInstalledClient } from './installedEditorContracts';")
s=s.replace("        if(result.error)throw new NativeReplyError(result,result.error.data);", "        this.observations.push({method:'initialize',result});\n        if(result.error)throw new NativeReplyError(result,result.error.data);")
s=s.replace("        assert.deepEqual(tools.result.tools.map", "        this.observations.push({method:'tools/list',result:tools});assert.equal(tools.error,undefined);assert.ok(Array.isArray(tools.result?.tools));\n        assert.deepEqual(tools.result.tools.map")
old='''            let body:any;
            try { body=JSON.parse(result.result?.content?.[0]?.text); } catch { /* Preserve protocol errors below. */ }'''
new='''            if(result.error)throw new NativeReplyError(result,result.error.data);
            const body=installedToolBody(result);'''
assert old in s;s=s.replace(old,new)
start=s.index('        const argument=current[0].args.indexOf(\'--catalog-session\')');end=s.index('        return current[0];',start)
s=s[:start]+'''        evidence.definitions=(evidence.definitions??[]).concat(current.map(value=>({command:value.command,args:[...value.args],env:value.env,cwd:value.cwd?.fsPath,cwd_explicit:value.cwd!==undefined})));
        await mark('retained-evidence',evidence); // Complete definitions/UI result retained even on an early contract refusal.
        const selector=installedSelector(current[0],result,selection);selection.session=selector;
'''+s[end:]
s=s.replace("        assert.equal(result.outcome.selected_subset,true);\n        const current=await definitions();assert.equal(current.length,1,'One deliberately selected Index, independent of open folder count');", "        const current=await definitions();")
s=s.replace("        const selector=installedSelector(current[0],result,selection);selection.session=selector;", "        assert.equal(result.outcome.selected_subset,true);assert.equal(current.length,1,'One deliberately selected Index, independent of open folder count');\n        const selector=installedSelector(current[0],result,selection);selection.session=selector;")
start=s.index('    const scoped=(body:any,selection:any)=>{');end=s.index('    const finish=',start)
s=s[:start]+'''    const scoped=(body:any,selection:any,denied=false)=>installedMember(body,selection,denied);
'''+s[end:]
s=s.replace("return {index,project,grant,backing,name:nativeIndex.name,source:primary,additional:extra,data:leaf};", "return {index,project,grant,backing,name:nativeIndex.name,source:primary,additional:extra,data:leaf,source_ids:member.review.request.operation.membership.scope.map((atom:any)=>atom.source_root_id)};")
s=s.replace("assert.equal((await client.call('search',{query:foreign,mode:'lexical',limit:5})).hits.length,0);", "assert.equal(scoped(await client.call('search',{query:foreign,mode:'lexical',limit:5}),selection).hits.length,0);")
s=s.replace("assert.equal((await client.call('search',{query:'HiddenPrivateEvidence',mode:'lexical',limit:5})).hits.length,0);", "assert.equal(scoped(await client.call('search',{query:'HiddenPrivateEvidence',mode:'lexical',limit:5}),selection).hits.length,0);")
s=s.replace("const denied=await client.call('read_file',{path:'hidden.ts',start_line:1,end_line:2},true);scoped(denied,selection);", "const denied=await client.call('read_file',{path:'hidden.ts',start_line:1,end_line:2});scoped(denied,selection,true);")
s=s.replace("await client.call('read_file',{path:'../workspace-b/fixture.ts',start_line:1,end_line:2},true);", "const escaped=await client.call('read_file',{path:'../workspace-b/fixture.ts',start_line:1,end_line:2});scoped(escaped,selection,true);")
s=s.replace("changed=await client.call('symbols',{name:'SharedEditEvidence',compact:false});", "changed=scoped(await client.call('symbols',{name:'SharedEditEvidence',compact:false}),selection);")
s=s.replace('            scoped(changed,selection);assert.equal(changed.items.length,expected);','            assert.equal(changed.items.length,expected);')
s=s.replace("checks.push('actual raw definition command/args/env','complete native tool catalog','native complete scope observation','citation bytes and out-of-grant isolation');", "checks.push('actual raw definition command/args/env','advertised native tool catalog retained with inputs; supported names match','complete combined scope/member projection; original raw transport retained','citation bytes and out-of-grant isolation');")
old='''            assert.equal(error?.code,'scoped_observation_unavailable');assert.equal(error.query_submitted,null);
            assert.equal(error.retryable,true);assert.equal(error.fresh_brokerage_required,true);assert.equal(error.query_replay_safe,false);
            assert.equal(body.requested_scope.confirmed,false);
            assert.deepEqual(body.requested_scope.selection,JSON.parse(definition.args[definition.args.indexOf('--catalog-session')+1]));
            for(const field of ['items','hits','evidence','result'])assert.equal(body[field],undefined);'''
assert old in s;s=s.replace(old,"            installedRevocation(body,selection.session);")
s=s.replace("        const selector=JSON.parse(definition.args[definition.args.indexOf('--catalog-session')+1]);\n",'')
s=s.replace(',namespace:selector.namespace}', '}')
old="finally{evidence.native.push({transport:client.observations});await mark('retained-evidence',evidence);await client.close();}"
assert s.count(old)==2;s=s.replace(old,"finally{await finishInstalledClient(client,evidence,mark);}")
old="finally{evidence.native.push({transport:otherClient.observations});await mark('retained-evidence',evidence);await otherClient.close();}"
assert s.count(old)==1;s=s.replace(old,"finally{await finishInstalledClient(otherClient,evidence,mark);}")
put(name,s)
# Service namespace is derived by the actual canonical native metadata read,
# never copied from a catalog/combined selector; retain every existing Stop fence.
name='scripts/check-native-editor.cjs';s=(W/name).read_text(encoding='utf-8')
old="                        if(request.action==='serviceStop'){await canonicalOwned(fixture,request.installDir);if(request.installDir!==path.join(folder,'native-service')||request.runtimeSha256!==bundle.digest||!/^[a-f0-9]{32}$/.test(request.namespace))throw new Error('Crossed owned Stop fixture binding');}"
new="""                        if(request.action==='serviceStop'){
                            await canonicalOwned(fixture,request.installDir);
                            if(request.installDir!==path.join(folder,'native-service')||request.runtimeSha256!==bundle.digest)throw new Error('Crossed owned Stop fixture binding');
                            const observed=await execute(bundle.main,['runtime','inspect','--install-dir',request.installDir],{windowsHide:true,env:environment,timeout:30000,maxBuffer:65536});
                            const metadata=JSON.parse(observed.stdout);
                            if(metadata.payload_verified!==true||metadata.approval?.manifest_sha256!==bundle.digest||!/^[a-f0-9]{32}$/.test(metadata.registry_namespace))throw new Error('Owned service namespace unavailable before Stop');
                            await fs.writeFile(path.join(folder,'native-pre-stop-inspect.json'),JSON.stringify(metadata,null,2));
                            request.namespace=metadata.registry_namespace;
                        }"""
assert old in s;s=s.replace(old,new);put(name,s)
put('packages/vscode/src/test/installedEditorContracts.test.ts',r'''// SPDX-License-Identifier: Apache-2.0
// Synthetic Core-shaped consumer regressions; actual native/editor proof is separate.
import {test} from 'node:test';import {strict as assert} from 'node:assert';
import {installedSelector,installedToolBody,installedMember,installedRevocation,finishInstalledClient} from './installedEditorContracts';
const id=(prefix:string,byte:string)=>prefix+byte.repeat(32),hash=(byte:string)=>byte.repeat(64);
function fixture(denied=false):any{
    const index=id('idx_','1'),project=id('prj_','2'),grant=id('grt_','3'),source=id('src_','4');
    const session={schema:1,context_key:hash('a'),namespace:'5'.repeat(32),expected_catalog_revision:2,grant_id:grant,expected_grant_revision:1,project_ids:[project],index_ids:[index]};
    const selection={index,project,grant,source_ids:[source],session};
    const publication={collection:'fixture',incarnation:'6'.repeat(32),generation:1,revision:hash('7'),token:'airk-r1:fixture-test-token'};
    const observed={qualifier:'selected_subset',provenance:'user_reviewed_project_grant',grant:{id:grant,revision:1},fingerprint:hash('8'),members:[{index_id:index,memberships:[{project_id:project}]}]};
    const body=denied?{error:{schema:1,code:'scoped_read_unavailable',retryable:false},scope_observation:observed}
        :{items:[{name:'Fixture',citation:{path:'fixture.ts',content_hash:hash('9'),start_line:1,end_line:1}}],publication,scope_observation:observed};
    const result={isError:denied,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};
    const raw={schema:'airk.combined-read/1',selection:{project_ids:[project],index_ids:[index],grant_id:grant,grant_revision:1,member_availability:'report',identity:hash('a')},
        publication_vector:{authority:hash('b'),observed_from_unix_ms:1,observed_to_unix_ms:2,members:[{index_id:index,publication:denied?null:publication}]},
        members:[{index_id:index,is_error:denied,allocation:{index_id:index,limit:20,max_bytes:65536,deadline_ms:30000,verification_max_sources:1,verification_max_bytes:65536,verification_deadline_ms:1},outcome:denied?'failed':'complete',result}],
        ranking:denied?[]:[{index_id:index,result_pointer:'/members/0/result/structuredContent/items/0',local_rank:1,rank_contribution:1/61,origin:{index_id:index,project_ids:[project],source_root_id:source,source_relative_path:'fixture.ts',publication}}],
        rank_policy:'fixture',partial:denied,truncated:false,delivery_observation:{receipt:{schema:1,namespace:session.namespace,catalog_revision:3,scope_identity:hash('b'),result_digest:hash('c')}}};
    return {selection,raw,body,definition:{args:['--combined-session',JSON.stringify(session),'mcp']},ui:{outcome:{selection:{session}}}};
}
test('complete current combined definition binds native reviewed singleton and rejects obsolete flag',()=>{
    const f=fixture();assert.deepEqual(installedSelector(f.definition,f.ui,f.selection),f.selection.session);
    assert.throws(()=>installedSelector({args:['--catalog-session',JSON.stringify(f.selection.session),'mcp']},f.ui,f.selection));
});
test('crossed index, Project and native session refuse',()=>{
    for(const field of ['index_ids','project_ids','context_key']){
        const f=fixture(),bad=structuredClone(f.selection.session);bad[field]=field==='context_key'?hash('d'):[id(field==='index_ids'?'idx_':'prj_','e')];
        assert.throws(()=>installedSelector({args:['--combined-session',JSON.stringify(bad),'mcp']},f.ui,f.selection));
    }
});
test('lossless member projection preserves raw body, floats and distinct identity domains',()=>{
    const f=fixture(),before=structuredClone(f.raw);assert.equal(installedMember(f.raw,f.selection).items.length,1);
    assert.deepEqual(f.raw,before);assert.notEqual(f.raw.selection.identity,f.raw.publication_vector.authority);
});
test('Report denied source is failed singleton with null publication, not outer-error success',()=>{
    const f=fixture(true);assert.equal(installedMember(f.raw,f.selection,true).error.code,'scoped_read_unavailable');
    assert.throws(()=>installedMember(f.raw,f.selection));const bad=structuredClone(f.raw);bad.members[0].is_error=false;assert.throws(()=>installedMember(bad,f.selection,true));
    const published=structuredClone(f.raw);published.publication_vector.members[0].publication={token:'not-a-denial-publication'};assert.throws(()=>installedMember(published,f.selection,true));
});
test('crossed allocation, unexpected member and wrong citation origin refuse',()=>{
    for(const mutation of [(x:any)=>{x.members[0].allocation.index_id=id('idx_','e');},(x:any)=>{x.members.push(x.members[0]);},(x:any)=>{x.ranking[0].origin.project_ids=[id('prj_','e')];},(x:any)=>{x.members[0].result.structuredContent.scope_observation.grant.revision=2;}]){
        const f=fixture();mutation(f.raw);assert.throws(()=>installedMember(f.raw,f.selection));
    }
});
test('outer flags and complete text/structured copies remain strict',()=>{
    const f=fixture(),wire={result:{isError:false,structuredContent:f.raw,content:[{type:'text',text:JSON.stringify(f.raw)}]}};
    assert.deepEqual(installedToolBody(wire),f.raw);
    assert.throws(()=>installedToolBody({result:{...wire.result,isError:'false'}}));
    assert.throws(()=>installedToolBody({result:{...wire.result,structuredContent:{different:true}}}));
    assert.throws(()=>installedToolBody({error:{code:1},result:wire.result}));
});
test('current revocation refuses before submission and never invents fresh brokerage authority',()=>{
    const f=fixture(),body={error:{code:'combined_scope_unavailable',query_submitted:false,retryable:false,query_replay_safe:false},requested_scope:{confirmed:false,selection:f.selection.session}};
    installedRevocation(body,f.selection.session);assert.throws(()=>installedRevocation({...body,error:{...body.error,retryable:true}},f.selection.session));
    assert.throws(()=>installedRevocation({...body,items:[]},f.selection.session));
});
test('manual legacy flat reply stays intact under transport decoding',()=>{
    const legacy={items:[],scope_observation:{provenance:'imported_legacy_snapshot'}};
    assert.deepEqual(installedToolBody({result:{content:[{type:'text',text:JSON.stringify(legacy)}]}}),legacy);
});
test('primary native failure survives cleanup failure with complete raw evidence retained',async()=>{
    const cleanup=new Error('owned close failed'),evidence:any={native:[],failure:{error:'primary query failed'}},saved:any[]=[];
    await finishInstalledClient({observations:[{raw:'complete'}],close:async()=>{throw cleanup;}},evidence,async(_name,value)=>{saved.push(structuredClone(value));});
    assert.equal(saved[0].failure.error,'primary query failed');assert.match(saved[0].transport_cleanup_failure,/owned close failed/);
    assert.deepEqual(saved[0].native,[{transport:[{raw:'complete'}]}]);
    await assert.rejects(finishInstalledClient({observations:[],close:async()=>{throw cleanup;}},{native:[]},async()=>{}),error=>error===cleanup);
});
''')
manifest={'status':'INERT_EXTERNAL_TEST_CONTRACT_CORRECTION_NOT_COMPILED_OR_RUN','files':[],
 'native_product_unchanged':True,'manual_legacy_branch_preserved':True,
 'scope':['Modern combined selector and complete raw/member/shared-metadata view, not old flat singleton','Report member denial vs top-level revocation semantics','Retain initialize/toolcatalog/early definition evidence','Source service namespace from actual runtime inspect, preserve native lifecycle fences'],
 'validation_pending':['Nine synthetic consumer tests proposed, not executed','Target-only semantic TypeScript compile with production source/out+VSIX-testexclusion proof','Fresh actual canonical five+legacy after prior owned native cleanup; no replay'],
 'limits':['Projection helper is shipped lossless metadata restoration, not independent raw delivery checksum validation','Current Raw transport remains untouched and complete','Manual legacy flat response remains on its original imported scope path']}
patch=''
for name,(b,s) in files.items():
 manifest['files'].append({'path':name,'base_sha256':hashlib.sha256(b).hexdigest() if b else None,'proposed_sha256':hashlib.sha256(s).hexdigest()})
 patch+=''.join(difflib.unified_diff(b.decode().splitlines(keepends=True),s.decode().splitlines(keepends=True),fromfile='a/'+name if b else '/dev/null',tofile='b/'+name))
(O/'installed-editor.patch').write_bytes(patch.encode());manifest['patch_sha256']=hashlib.sha256(patch.encode()).hexdigest()
r=subprocess.run(['git','-c','safe.directory='+str(W.resolve()),'-C',str(W),'apply','--check',str((O/'installed-editor.patch').resolve())],capture_output=True,text=True);assert r.returncode==0,r.stderr
(O/'manifest.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode());print('proposal',O,'patch',manifest['patch_sha256'])
