// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as path from 'node:path';
import {ownedConfigurationSource,assertOwnedConfigurationRefresh,assertInstalledWorkbenchCopies,installedParityValue,assertInstalledInputCatalog,assertInstalledEnvelopeParity} from './installedCompletion';
import type {HttpConnectionInfo} from '../serviceHttp';
import {renderExport} from '../workbenchRender';
import {refreshHttpAgentConfig} from '../agentMigration';
import type {WorkbenchSession} from '../workbenchContracts';

function connection():HttpConnectionInfo{return {schema:1,connection_id:'11'.repeat(16),endpoint:{schema:1,installation:'22'.repeat(32),user:'fixture',namespace:'33'.repeat(16),generation:1,service_instance:'44'.repeat(16),executable_sha256:'55'.repeat(32),url:'https://127.0.0.1:57421/mcp',ca_sha256:'66'.repeat(32),certificate_sha256:'77'.repeat(32),state:'ready',predecessor_digest:null},selection:{kind:'combined',selection:{schema:1,context_key:'88'.repeat(32),namespace:'33'.repeat(16),expected_catalog_revision:1,grant_id:'grt_'+'99'.repeat(16),expected_grant_revision:1,project_ids:['prj_'+'aa'.repeat(16)],index_ids:['idx_'+'bb'.repeat(16)]}},ca_certificate_file:path.resolve('owned','ca.pem'),credential_file:path.resolve('owned','credential-'+ '11'.repeat(16)+'.token')};}

test('owned endpoint acceptance requires exact automatic configuration rewrite and retained non-AIRK preimage',()=>{
    for(const agent of ['codex','claude','copilot'] as const){
        const before=connection(),after=structuredClone(before);after.endpoint.url='https://127.0.0.1:57422/mcp';after.endpoint.generation++;after.endpoint.service_instance='ab'.repeat(16);
        const original=ownedConfigurationSource(agent,before),updated=refreshHttpAgentConfig(agent,original,after,before.endpoint.url);
        assertOwnedConfigurationRefresh(agent,original,updated,before,after);
        assert.throws(()=>assertOwnedConfigurationRefresh(agent,original,original,before,after),/configuration|rewrite/);
        assert.throws(()=>assertOwnedConfigurationRefresh(agent,original,updated.replace('unrelated fixture bytes','changed'),before,after),/configuration|preimage/);
        const crossed=structuredClone(after);assert.equal(crossed.selection.kind,'combined');if(crossed.selection.kind==='combined')crossed.selection.selection.expected_grant_revision++;
        assert.throws(()=>assertOwnedConfigurationRefresh(agent,original,updated,before,crossed),/authority|scope/);
    }
});

test('installed parity normalizes only declared observation fields without discarding future data or uncertainty',()=>{
    const raw={future:{retained:true},error:{code:'uncertain',retryable:false},allocation:{deadline_ms:900,max_bytes:42},observed_unix_ms:1,delivery_observation:{receipt:{result_digest:'aa',scope_identity:'original'}}};
    const later=structuredClone(raw);later.observed_unix_ms=2;later.allocation.deadline_ms=800;later.delivery_observation.receipt.result_digest='bb';
    assert.deepEqual(installedParityValue(raw),installedParityValue(later));
    later.future.retained=false;assert.notDeepEqual(installedParityValue(raw),installedParityValue(later));
    assert.equal(raw.allocation.deadline_ms,900);assert.equal(raw.delivery_observation.receipt.result_digest,'aa');
    assert.throws(()=>installedParityValue({...raw,allocation:{deadline_ms:0}}),/deadline/);
});

test('full tool envelope comparison asserts negotiated resultType and preserves future transport result fields',()=>{
    const mcp={content:[{type:'text',text:'{"future_native_field":true}'}],isError:false,future_envelope:{retained:true}},cli={...structuredClone(mcp),resultType:'complete'};
    assertInstalledEnvelopeParity(mcp,cli);
    assert.throws(()=>assertInstalledEnvelopeParity(mcp,{...cli,resultType:'unknown'}),/resultType/);
    assert.throws(()=>assertInstalledEnvelopeParity(mcp,{...cli,future_envelope:{retained:false}}),/envelopes/);
});

test('installed input catalog refuses dropped operations and changed supported field constraints',()=>{
    const operations={search:{type:'object',properties:{query:{type:'string',maxLength:100},mode:{type:'string',enum:['lexical','vector']},limit:{type:'integer',minimum:1,maximum:10}},required:['query']}};
    const displayed={names:['search'],tool:'search',forms:{} as Record<string,unknown>,fields:[{label:'query',tag:'INPUT',type:'text',required:true,min:'',max:'',minLength:0,maxLength:100,options:[]},{label:'mode',tag:'SELECT',type:'select-one',required:false,min:'',max:'',minLength:0,maxLength:4096,options:['','lexical','vector']},{label:'limit',tag:'INPUT',type:'number',required:false,min:'1',max:'10',minLength:0,maxLength:4096,options:[]}]};displayed.forms={search:displayed.fields};
    assertInstalledInputCatalog(operations,displayed);
    assert.throws(()=>assertInstalledInputCatalog(operations,{...displayed,names:[]}),/operation/);
    assert.throws(()=>assertInstalledInputCatalog(operations,{...displayed,fields:displayed.fields.slice(1)}),/field/);
});

test('installed full and selected UI copies preserve native future fields and explicitly selected origins',()=>{
    const hit=(file:string)=>({citation:{path:file,content_hash:'11'.repeat(32),start_line:1,end_line:1},text:'fixture '+file});
    const session={schema:1,id:'session',project:'22'.repeat(32),title:'Fixture',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[{id:'turn',at:1,input:'search form',format:'structured',request:{schema:'airk.parse.request/1',mode:'execute',plan:{steps:[{id:'form',tool:'search',args:{query:'fixture'}}]}},envelope:{status:'complete',executed:true,steps:[{id:'form',tool:'search',status:'complete',result:{hits:[hit('first.ts'),hit('second.ts')],future_native_field:'retained'}}]}}]} as WorkbenchSession;
    const copies={json:renderExport(session,'json'),markdown:renderExport(session,'markdown'),selectedJson:renderExport(session,'json','turn',['record-0']),selectedMarkdown:renderExport(session,'markdown','turn',['record-0'])};
    assertInstalledWorkbenchCopies(session,copies,true);
    assert.throws(()=>assertInstalledWorkbenchCopies(session,{...copies,json:copies.json.replace('future_native_field','lost')},true),/full/);
    assert.throws(()=>assertInstalledWorkbenchCopies(session,{...copies,selectedJson:copies.json},true),/selected/);
    assert.throws(()=>assertInstalledWorkbenchCopies(session,{...copies,markdown:'summary only'},true),/Markdown/);
});
