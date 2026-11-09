// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import {X509Certificate,createHash} from 'node:crypto';
import {validateHttpConnection,httpUrl,httpMaterial,HttpConnectionInfo,HttpSelection,credentialEnvironment,validateHttpSelection,sameHttpAuthority,httpAuthorityDigest,validateAttachmentRetirement,publishNativeInput,removeNativeInput} from '../serviceHttp';
import {serializeServers,LaunchSpec} from '../model';
import {migrateAgentConfig} from '../agentMigration';
import {ca} from './fixtures/httpTlsFixture';
const digest=(value:Buffer)=>createHash('sha256').update(value).digest('hex');

test('native setup input requires an existing directory and preserves its policy and retained preimage',async()=>{
    const owner=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-native-input-'))),control=path.join(owner,'control'),directory=path.join(control,'http-1');
    try{
        await assert.rejects(publishNativeInput(control,'{}'));
        assert.deepEqual(await fs.readdir(owner),[],'Publisher never creates a native control root');
        await fs.mkdir(directory,{recursive:true,mode:0o700});
        const before=await fs.stat(directory),name='native-input-legacy-batch-fixture.json';
        const file=await publishNativeInput(control,'{"accepted":1}',()=>{},name);
        assert.equal(await fs.readFile(file,'utf8'),'{"accepted":1}');assert.equal((await fs.stat(file)).nlink,1);
        assert.equal(await publishNativeInput(control,'{"later":2}',()=>{},name),file);
        assert.equal(await fs.readFile(file,'utf8'),'{"accepted":1}','A retry never overwrites an accepted batch');
        assert.deepEqual(await fs.readdir(directory),[name],'Staged publication leaves no linked/temp sibling');
        const after=await fs.stat(directory);assert.equal(after.mode,before.mode);assert.equal(after.uid,before.uid);
        await assert.rejects(publishNativeInput(control,'x'.repeat(1048577)),/native_input_exceeds_budget/);
        await assert.rejects(publishNativeInput(control,'{}',()=>{},'../foreign.json'),/invalid_native_input_name/);
        await assert.rejects(publishNativeInput(control,'{}',()=>{throw Error('stale');}),/stale/);
        let checks=0;
        await assert.rejects(publishNativeInput(control,'{}',()=>{if(++checks===4)throw Error('late-stale');}),/late-stale/);
        assert.deepEqual(await fs.readdir(directory),[name],'A revoked write removes only its exact transient stage');
        const transient=await publishNativeInput(control,'{"transient":true}');await removeNativeInput(transient);
        await assert.rejects(fs.lstat(transient),{code:'ENOENT'});
        const crossed=await publishNativeInput(control,'{"owned":true}');
        await fs.rename(crossed,crossed+'.original');await fs.writeFile(crossed,'foreign',{mode:0o600});
        await assert.rejects(removeNativeInput(crossed),/native_input_file_changed/);assert.equal(await fs.readFile(crossed,'utf8'),'foreign');
        await fs.link(file,path.join(directory,'foreign-link'));
        await assert.rejects(publishNativeInput(control,'{}',()=>{},name),/unsafe_native_input_file/);
        assert.equal(await fs.readFile(file,'utf8'),'{"accepted":1}');
    }finally{await fs.rm(owner,{recursive:true,force:true});}
});

test('native setup input refuses a redirected HTTP directory without changing its destination',async()=>{
    const owner=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-native-input-link-'))),control=path.join(owner,'control'),target=path.join(owner,'target');
    try{
        await fs.mkdir(control);await fs.mkdir(target,{mode:0o700});await fs.symlink(target,path.join(control,'http-1'),process.platform==='win32'?'junction':'dir');
        await assert.rejects(publishNativeInput(control,'{}'),/unsafe_native_input_directory/);assert.deepEqual(await fs.readdir(target),[]);
    }finally{await fs.rm(owner,{recursive:true,force:true});}
});
function selection():HttpSelection{return {kind:'scoped',selection:{schema:1,context_key:'a'.repeat(64),namespace:'b'.repeat(32),expected_catalog_revision:7,grant_id:'grt_'+'c'.repeat(32),expected_grant_revision:1,index_id:'idx_'+'d'.repeat(32),project_ids:['prj_'+'e'.repeat(32)]}};}
function info(directory:string):HttpConnectionInfo{return {schema:1,connection_id:'f'.repeat(32),endpoint:{schema:1,installation:'1'.repeat(64),user:'fixture',namespace:'b'.repeat(32),generation:1,service_instance:'2'.repeat(32),executable_sha256:'3'.repeat(64),url:'https://127.0.0.1:57421/mcp',ca_sha256:digest(new X509Certificate(ca).raw),certificate_sha256:'4'.repeat(64),state:'ready',predecessor_digest:null},selection:selection(),ca_certificate_file:path.join(directory,'ca.pem'),credential_file:path.join(directory,'credential-'+ 'f'.repeat(32)+'.token')};}

test('native authority digest uses shared frozen vectors and normalizes only the catalog minimum',async()=>{
    const fixture=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../src/test/fixtures/httpAuthorityDigest.json'),'utf8'));assert.equal(fixture.schema,1);
    for(const vector of fixture.vectors){const original=structuredClone(vector.selection);assert.equal(httpAuthorityDigest(vector.selection),vector.authority_sha256,vector.name);assert.deepEqual(vector.selection,original);vector.selection.selection.expected_catalog_revision++;assert.equal(httpAuthorityDigest(vector.selection),vector.authority_sha256);vector.selection.selection.expected_grant_revision++;assert.notEqual(httpAuthorityDigest(vector.selection),vector.authority_sha256);}
});
test('attachment retirement proof binds exact native connection, installation, client and complete authority',()=>{
    const connection=info(path.resolve('fixture')),proof={schema:1,connection_id:connection.connection_id,installation:connection.endpoint.installation,namespace:connection.endpoint.namespace,client:'claude_code',expires_unix_ms:200,explorer_descriptor_sha256:'a'.repeat(64),authority_sha256:httpAuthorityDigest(connection.selection)};
    assert.deepEqual(validateAttachmentRetirement(proof,connection,'claude',100),proof);
    for(const mutate of [
        (value:any)=>value.schema=2,(value:any)=>value.connection_id='0'.repeat(32),(value:any)=>value.installation='0'.repeat(64),
        (value:any)=>value.namespace='0'.repeat(32),(value:any)=>value.client='vscode',(value:any)=>value.expires_unix_ms=100,
        (value:any)=>value.expires_unix_ms=Number.MAX_SAFE_INTEGER+1,(value:any)=>value.explorer_descriptor_sha256='bad',
        (value:any)=>value.authority_sha256='0'.repeat(64),(value:any)=>value.extra='unreviewed',
    ]){const crossed=structuredClone(proof);mutate(crossed);assert.throws(()=>validateAttachmentRetirement(crossed,connection,'claude',100));}
    assert.throws(()=>validateAttachmentRetirement(proof,connection,'unknown',100));
});
test('HTTP DTO preserves exact authority and permits only a monotonic catalog clock refresh',()=>{
    const value=info(path.resolve('fixture'));assert.deepEqual(validateHttpConnection(value,selection()),value);
    const next=structuredClone(value);(next.selection.selection as any).expected_catalog_revision++;assert.equal((validateHttpConnection(next,selection()).selection.selection as any).expected_catalog_revision,8);
    for(const change of [(v:any)=>v.selection.selection.grant_id='grt_'+'0'.repeat(32),(v:any)=>v.selection.selection.expected_grant_revision++,(v:any)=>v.endpoint.namespace='0'.repeat(32),(v:any)=>v.selection.selection.expected_catalog_revision--,(v:any)=>v.credential_file=path.join(path.dirname(v.credential_file),'foreign.token'),(v:any)=>v.endpoint.extra='hidden']){const candidate=structuredClone(value);change(candidate);assert.throws(()=>validateHttpConnection(candidate,selection()));}
    for(const url of ['http://127.0.0.1:57421/mcp','https://localhost:57421/mcp','https://127.0.0.1:057421/mcp','https://127.0.0.1:65536/mcp','https://127.0.0.1:57421/mcp?token=x','https://user@127.0.0.1:57421/mcp'])assert.throws(()=>httpUrl(url));
});
test('owner-private HTTP material binds DER issuer and exact native credential identity',async()=>{
    const directory=await fs.mkdtemp(path.join(os.tmpdir(),'airk-http-material-'));
    try{
        const value=info(directory);await fs.writeFile(value.ca_certificate_file,ca,{mode:0o600});await fs.writeFile(value.credential_file,'airk1.'+value.connection_id+'.'+'a'.repeat(64),{mode:0o600});
        assert.equal((await httpMaterial(value)).url,value.endpoint.url);
        await fs.writeFile(value.credential_file,'airk1.'+'0'.repeat(32)+'.'+'a'.repeat(64));await assert.rejects(httpMaterial(value),/credential/);
        await fs.writeFile(value.credential_file,'airk1.'+value.connection_id+'.'+'a'.repeat(64)+'\n');await assert.rejects(httpMaterial(value),/private|credential/);
        await fs.writeFile(value.credential_file,'airk1.'+value.connection_id+'.'+'a'.repeat(64));
        const crossed=structuredClone(value);crossed.endpoint.ca_sha256='0'.repeat(64);await assert.rejects(httpMaterial(crossed),/ca/);
    }finally{assert.equal(path.dirname(path.resolve(directory)),path.resolve(os.tmpdir()));assert.match(path.basename(directory),/^airk-http-material-/);await fs.rm(directory,{recursive:true});}
});
test('HTTP serializers use references, preserve policies and never emit executable launches or tokens',()=>{
    const value=info(path.resolve('fixture')),spec:LaunchSpec={command:path.resolve('native/airk'),root:path.resolve('source'),dataDir:path.resolve('data'),collection:'fixture',name:'Fixture',catalogSession:(selection() as Extract<HttpSelection,{kind:'scoped'}>).selection,service:{installDir:path.resolve('native'),project:'1'.repeat(64)},http:value};
    const tokenName=credentialEnvironment(value);
    for(const client of ['codex','claude','vscode'] as const){const source=serializeServers(client,[{key:'airk',command:spec.command,args:[],label:'AIRK',http:value}]);assert.ok(source.includes(value.endpoint.url));assert.ok(source.includes(client==='vscode'?'airkToken_'+value.connection_id:tokenName));assert.ok(!source.includes('credential-'));assert.ok(!source.includes(spec.command));assert.ok(!source.includes('airk1.'));}
    const source=JSON.stringify({mcpServers:{airk:{type:'http',url:value.endpoint.url,headers:{Authorization:'Bearer ${'+tokenName+'}','X-AIRK-Response-Mode':'full'},disabled:true},other:{command:'preserve'}},policy:'keep'});
    const output=migrateAgentConfig('claude',spec,path.resolve('extension'),source);const parsed=JSON.parse(output);assert.equal(parsed.policy,'keep');assert.equal(parsed.mcpServers.airk.disabled,true);assert.equal(parsed.mcpServers.airk.headers['X-AIRK-Response-Mode'],'full');assert.equal(parsed.mcpServers.other.command,'preserve');assert.deepEqual(JSON.parse(migrateAgentConfig('claude',spec,path.resolve('extension'),output)),parsed);
    const foreign=JSON.parse(source);foreign.mcpServers.airk.url='https://127.0.0.1:60000/mcp';assert.throws(()=>migrateAgentConfig('claude',spec,path.resolve('extension'),JSON.stringify(foreign)),/Unrecognized/);
    assert.throws(()=>serializeServers('amazon-q',[{key:'airk',command:spec.command,args:[],label:'AIRK',http:value}]),/compatibility_unverified/);
});

test('imported legacy unions retain every original authority through the native frame budgets',()=>{
    const members=Array.from({length:32},(_,index)=>{
        const binding=index.toString(16).padStart(64,'0');return {namespace:'b'.repeat(32),expected_catalog_revision:7,registration:{binding,root:path.resolve('source-'+index),data_root:path.resolve('data-'+index),collection:'source-'+index},grant:{grant:{id:'grt_'+'91'.repeat(16),host:'a'.repeat(64),revision:1,revoked:false,memberships:[{membership:{project_id:'prj_'+'11'.repeat(16),index_id:'idx_'+'22'.repeat(16),revision:1,scope:[{source_root_id:'src_'+'33'.repeat(16),kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'imported_legacy_snapshot',upgrade_id:'c'.repeat(32),plan_digest:'d'.repeat(64),legacy_binding:binding,configuration_digest:'e'.repeat(64)}},configuration_receipt:'f'.repeat(64)};
    });
    const value=info(path.resolve('fixture'));value.selection={kind:'imported_legacy_union',selection:{namespace:'b'.repeat(32),selections:members,no_default_project:true}};
    assert.ok(Buffer.byteLength(JSON.stringify(value))>16384);assert.deepEqual(validateHttpConnection(value),value);
    const digest=httpAuthorityDigest(value.selection),single:HttpSelection={kind:'imported_legacy',selection:structuredClone(members[0])},singleDigest=httpAuthorityDigest(single);
    const advancedSingle=structuredClone(single);advancedSingle.selection.expected_catalog_revision++;assert.equal(httpAuthorityDigest(advancedSingle),singleDigest);
    const changedSingle=structuredClone(single);changedSingle.selection.grant.provenance.configuration_digest='0'.repeat(64);assert.notEqual(httpAuthorityDigest(changedSingle),singleDigest,'Imported original configuration provenance is part of exact authority');
    const refreshed=structuredClone(value.selection);if(refreshed.kind==='imported_legacy_union')for(const member of refreshed.selection.selections)member.expected_catalog_revision++;assert.equal(sameHttpAuthority(refreshed,value.selection),true);
    assert.equal(httpAuthorityDigest(refreshed),digest,'Every union member minimum clock is normalized');
    const reordered=structuredClone(value.selection);if(reordered.kind==='imported_legacy_union')reordered.selection.selections.reverse();assert.notEqual(httpAuthorityDigest(reordered),digest,'Original union member order is retained');
    const crossed=structuredClone(refreshed);if(crossed.kind==='imported_legacy_union')crossed.selection.selections[1].grant.grant.revision++;assert.equal(sameHttpAuthority(crossed,value.selection),false);
    assert.notEqual(httpAuthorityDigest(crossed),digest,'A changed member grant is never normalized away');
    const duplicate=structuredClone(value.selection);if(duplicate.kind==='imported_legacy_union')duplicate.selection.selections[1]=duplicate.selection.selections[0];assert.throws(()=>validateHttpSelection(duplicate),/union/);
    const defaulted=structuredClone(value.selection) as any;defaulted.selection.no_default_project=false;assert.throws(()=>validateHttpSelection(defaulted),/union/);
    const tooLarge=structuredClone(value);if(tooLarge.selection.kind==='imported_legacy_union')tooLarge.selection.selection.selections[0].grant.future='x'.repeat(61440);assert.throws(()=>validateHttpConnection(tooLarge),/bound/);
});
