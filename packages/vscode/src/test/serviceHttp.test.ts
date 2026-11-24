// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import {validateHttpConnection,validateEndpoint,httpUrl,legacyHttpsUrl,recognizedHttpUrl,HttpConnectionInfo,HttpSelection,credentialEnvironment,validateHttpSelection,httpAuthorityDigest,validateAttachmentRetirement,publishNativeInput,removeNativeInput} from '../serviceHttp';
import {serializeServers,LaunchSpec} from '../model';
import {migrateAgentConfig} from '../agentMigration';

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
function selection():HttpSelection{return {kind:'open',selection:{namespace:'b'.repeat(32)}};}
function info():HttpConnectionInfo{return {schema:1,connection_id:'f'.repeat(32),endpoint:{schema:1,installation:'1'.repeat(64),user:'fixture',namespace:'b'.repeat(32),generation:1,service_instance:'2'.repeat(32),executable_sha256:'3'.repeat(64),url:'http://127.0.0.1:57421/mcp',state:'ready',predecessor_digest:null},selection:selection(),open:{defaults_revision:1,default_index_ids:[],default_index_set_ids:[],agent_choice:'off'}};}

test('native authority digest uses shared frozen vectors and normalizes only the catalog minimum',async()=>{
    const fixture=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../src/test/fixtures/httpAuthorityDigest.json'),'utf8'));assert.equal(fixture.schema,1);
    for(const vector of fixture.vectors){const original=structuredClone(vector.selection);assert.equal(httpAuthorityDigest(vector.selection),vector.authority_sha256,vector.name);assert.deepEqual(vector.selection,original);if(vector.selection.kind==='open'){vector.selection.selection.namespace='c'.repeat(32);assert.notEqual(httpAuthorityDigest(vector.selection),vector.authority_sha256);continue;}vector.selection.selection.expected_catalog_revision++;assert.equal(httpAuthorityDigest(vector.selection),vector.authority_sha256);vector.selection.selection.expected_grant_revision++;assert.notEqual(httpAuthorityDigest(vector.selection),vector.authority_sha256);}
});
test('attachment retirement proof binds exact native connection, installation, client and complete authority',()=>{
    const connection=info(),proof={schema:1,connection_id:connection.connection_id,installation:connection.endpoint.installation,namespace:connection.endpoint.namespace,client:'claude_code',expires_unix_ms:200,explorer_descriptor_sha256:'a'.repeat(64),authority_sha256:httpAuthorityDigest(connection.selection)};
    assert.deepEqual(validateAttachmentRetirement(proof,connection,'claude',100),proof);
    for(const mutate of [
        (value:any)=>value.schema=2,(value:any)=>value.connection_id='0'.repeat(32),(value:any)=>value.installation='0'.repeat(64),
        (value:any)=>value.namespace='0'.repeat(32),(value:any)=>value.client='vscode',(value:any)=>value.expires_unix_ms=100,
        (value:any)=>value.expires_unix_ms=Number.MAX_SAFE_INTEGER+1,(value:any)=>value.explorer_descriptor_sha256='bad',
        (value:any)=>value.authority_sha256='0'.repeat(64),(value:any)=>value.extra='unreviewed',
    ]){const crossed=structuredClone(proof);mutate(crossed);assert.throws(()=>validateAttachmentRetirement(crossed,connection,'claude',100));}
    assert.throws(()=>validateAttachmentRetirement(proof,connection,'unknown',100));
});
test('(U105) an open connection carries exactly its open state; other connections never do',()=>{
    const open:any=info();open.open={defaults_revision:3,default_index_ids:['idx_'+'1'.repeat(32)],default_index_set_ids:['prj_'+'2'.repeat(32)],agent_choice:'off'};
    assert.deepEqual(validateHttpConnection(open),open);
    for(const change of [(v:any)=>{delete v.open;},(v:any)=>v.open.agent_choice='all',(v:any)=>v.open.defaults_revision=0,(v:any)=>v.open.default_index_ids=['prj_'+'2'.repeat(32)],
        (v:any)=>v.open.default_index_set_ids=['prj_'+'2'.repeat(32),'prj_'+'2'.repeat(32)],(v:any)=>v.open.extra=1]){const candidate=structuredClone(open);change(candidate);assert.throws(()=>validateHttpConnection(candidate));}
});
test('HTTP DTO keeps the exact open authority and refuses the removed imported legacy kinds',()=>{
    const value=info();assert.deepEqual(validateHttpConnection(value,selection()),value);
    assert.throws(()=>validateHttpConnection(value,{kind:'open',selection:{namespace:'c'.repeat(32)}}),/crossed_http_selection/);
    // Change-log entry 68 part 5: the imported legacy selections had no native producer and are no HTTP selection.
    for(const kind of ['imported_legacy','imported_legacy_union'])assert.throws(()=>validateHttpSelection({kind,selection:{namespace:'b'.repeat(32)}}),/invalid_http_selection/);
    assert.equal(httpUrl('http://127.0.0.1:57421/mcp'),'http://127.0.0.1:57421/mcp');
    for(const url of ['https://127.0.0.1:57421/mcp','http://localhost:57421/mcp','http://127.0.0.1:057421/mcp','http://127.0.0.1:65536/mcp','http://127.0.0.1:0/mcp','http://127.0.0.1:57421/mcp?token=x','http://user@127.0.0.1:57421/mcp','http://127.0.0.1:57421/mcp/'])assert.throws(()=>httpUrl(url),url);
});
test('local trust plan C: a legacy https entry URL is recognized only, never a current endpoint',()=>{
    assert.equal(legacyHttpsUrl('https://127.0.0.1:57421/mcp'),'https://127.0.0.1:57421/mcp');
    for(const url of ['http://127.0.0.1:57421/mcp','https://localhost:57421/mcp','https://127.0.0.1:65536/mcp','https://127.0.0.1:57421/mcp?token=x'])assert.throws(()=>legacyHttpsUrl(url),url);
    for(const url of ['http://127.0.0.1:57421/mcp','https://127.0.0.1:57421/mcp'])assert.equal(recognizedHttpUrl(url),url);
    for(const url of ['ftp://127.0.0.1:57421/mcp','https://localhost:57421/mcp','http://127.0.0.1:65536/mcp'])assert.throws(()=>recognizedHttpUrl(url),url);
    const endpoint=info().endpoint;assert.deepEqual(validateEndpoint(structuredClone(endpoint)),endpoint);
    assert.throws(()=>validateEndpoint({...endpoint,url:'https://127.0.0.1:57421/mcp'}),/invalid_http_endpoint/,'a descriptor is never https');
    for(const field of ['ca_sha256','certificate_sha256'])assert.throws(()=>validateEndpoint({...endpoint,[field]:'6'.repeat(64)}),/invalid_http_connection_fields/,field);
    for(const field of ['ca_certificate_file','credential_file'])assert.throws(()=>validateHttpConnection({...info(),[field]:path.resolve('fixture','ca.pem')}),/invalid_http_connection_fields/,field);
});
test('HTTP serializers use references, preserve policies and never emit executable launches or tokens',()=>{
    const value=info(),spec:LaunchSpec={command:path.resolve('native/airk'),root:path.resolve('source'),dataDir:path.resolve('data'),collection:'fixture',name:'Fixture',service:{installDir:path.resolve('native'),project:'1'.repeat(64)},http:value};
    const tokenName=credentialEnvironment(value);
    for(const client of ['codex','claude','vscode'] as const){const source=serializeServers(client,[{key:'airk',command:spec.command,args:[],label:'AIRK',http:value}]);assert.ok(source.includes(value.endpoint.url));assert.ok(source.includes(client==='vscode'?'airkToken_'+value.connection_id:tokenName));assert.ok(!source.includes('credential-'));assert.ok(!source.includes(spec.command));assert.ok(!source.includes('airk1.'));}
    const source=JSON.stringify({mcpServers:{airk:{type:'http',url:value.endpoint.url,headers:{Authorization:'Bearer ${'+tokenName+'}','X-AIRK-Response-Mode':'full'},disabled:true},other:{command:'preserve'}},policy:'keep'});
    const output=migrateAgentConfig('claude',spec,path.resolve('extension'),source);const parsed=JSON.parse(output);assert.equal(parsed.policy,'keep');assert.equal(parsed.mcpServers.airk.disabled,true);assert.equal(parsed.mcpServers.airk.headers['X-AIRK-Response-Mode'],'full');assert.equal(parsed.mcpServers.other.command,'preserve');assert.deepEqual(JSON.parse(migrateAgentConfig('claude',spec,path.resolve('extension'),output)),parsed);
    const foreign=JSON.parse(source);foreign.mcpServers.airk.url='http://127.0.0.1:60000/mcp';assert.throws(()=>migrateAgentConfig('claude',spec,path.resolve('extension'),JSON.stringify(foreign)),/Unrecognized/);
    const legacyForeign=JSON.parse(source);legacyForeign.mcpServers.airk.url='https://127.0.0.1:60000/mcp';assert.throws(()=>migrateAgentConfig('claude',spec,path.resolve('extension'),JSON.stringify(legacyForeign)),/Unrecognized/,'a legacy https entry on another port is not this connection');
    const legacy=JSON.parse(source);legacy.mcpServers.airk.url='https://127.0.0.1:57421/mcp';
    const repaired=JSON.parse(migrateAgentConfig('claude',spec,path.resolve('extension'),JSON.stringify(legacy)));
    assert.equal(repaired.mcpServers.airk.url,'http://127.0.0.1:57421/mcp','the legacy https form of this endpoint is rewritten to http');assert.equal(repaired.mcpServers.airk.disabled,true);assert.equal(repaired.mcpServers.other.command,'preserve');
    assert.throws(()=>serializeServers('amazon-q',[{key:'airk',command:spec.command,args:[],label:'AIRK',http:value}]),/compatibility_unverified/);
});
