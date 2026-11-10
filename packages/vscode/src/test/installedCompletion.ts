// SPDX-License-Identifier: Apache-2.0
/** Test-only contracts for actual installed UI/configuration observations. */
import {strict as assert} from 'node:assert';
import {serializeServers} from '../model';
import {refreshHttpAgentConfig,type LocalAgent} from '../agentMigration';
import {sameHttpAuthority,type HttpConnectionInfo} from '../serviceHttp';
import {renderExport} from '../workbenchRender';
import {toolFormFields,type WorkbenchSession} from '../workbenchContracts';
import {materializeNativeMetadata} from '../nativeMetadata';
type Row=Record<string,any>;
export type InstalledAgent='codex'|'claude'|'copilot';

export function ownedConfigurationSource(agent:InstalledAgent,connection:HttpConnectionInfo):string {
    const source=serializeServers(agent==='copilot'?'vscode':agent,[{key:'airk',command:'',args:[],label:'AIRK',http:connection}]);
    return agent==='codex'?'# unrelated fixture bytes\nfixture_marker = "preserve exactly"\n\n'+source:'// unrelated fixture bytes\n'+source;
}
export function assertOwnedConfigurationRefresh(agent:InstalledAgent,before:string,after:string,original:HttpConnectionInfo,current:HttpConnectionInfo):void {
    assert.equal(current.connection_id,original.connection_id,'Owned configuration credential identity');
    assert.ok(sameHttpAuthority(current.selection,original.selection),'Owned configuration authority/scope');
    assert.equal(current.ca_certificate_file,original.ca_certificate_file);assert.equal(current.credential_file,original.credential_file);
    if(current.selection.kind==='combined'&&original.selection.kind==='combined')assert.ok(current.selection.selection.expected_catalog_revision>=original.selection.selection.expected_catalog_revision,'Owned configuration clock cannot regress');
    for(const field of ['installation','user','namespace','ca_sha256','executable_sha256'] as const)assert.equal(current.endpoint[field],original.endpoint[field]);
    assert.ok(current.endpoint.generation>original.endpoint.generation);assert.notEqual(current.endpoint.service_instance,original.endpoint.service_instance);assert.notEqual(current.endpoint.url,original.endpoint.url);
    const expected=refreshHttpAgentConfig(agent as LocalAgent,before,current,original.endpoint.url);
    assert.notEqual(expected,before,'Expected a saved configuration endpoint rewrite');
    assert.equal(after,expected,'Automatic configuration rewrite must preserve the exact unrelated preimage');
}

/** Same declared observation-only normalization as scripts/http_fixture.py.
 * Callers retain raw replies and validate native delivery/scope BEFORE comparing.
 * Unknown fields, failure/retry state, publications and all byte budgets remain. */
export function installedParityValue(value:any):any {
    if(Array.isArray(value))return value.map(installedParityValue);
    if(!value||typeof value!=='object')return value;
    const timing=new Set(['request_id','observed_unix_ms','observed_from_unix_ms','observed_to_unix_ms','cache_age_ms','cached','elapsed_ms']);
    const result:Row=Object.fromEntries(Object.entries(value).filter(([key])=>!timing.has(key)).map(([key,item])=>[key,installedParityValue(item)]));
    if(result.allocation&&Object.hasOwn(result.allocation,'deadline_ms')){assert.ok(Number.isSafeInteger(result.allocation.deadline_ms)&&result.allocation.deadline_ms>0,'Original allocation deadline must be positive');result.allocation.deadline_ms=0;}
    if(result.delivery_observation?.receipt)delete result.delivery_observation.receipt.result_digest;
    // Separate requests report different remaining verification windows. As in
    // the native parity fixture (docs/read-interface-parity.md), only a member's
    // source_verification deadline observations are elapsed timing, and only after
    // they are validated against that member's admitted verification allocation.
    const limits=result.result?.source_verification?.limits,admitted=value.allocation;
    if(admitted&&typeof admitted==='object'&&limits&&typeof limits==='object'&&Object.hasOwn(limits,'effective_deadline_ms')){
        const window=Object.hasOwn(admitted,'verification_deadline_ms')?admitted.verification_deadline_ms:admitted.deadline_ms;
        const sources=Object.hasOwn(admitted,'verification_max_sources')?admitted.verification_max_sources:admitted.max_sources;
        const bytes=Object.hasOwn(admitted,'verification_max_bytes')?admitted.verification_max_bytes:admitted.max_bytes;
        assert.ok([window,limits.deadline_ms,limits.effective_deadline_ms].every(item=>Number.isSafeInteger(item)&&item>=0)&&window<=30000
            &&limits.effective_deadline_ms<=limits.deadline_ms&&limits.deadline_ms<=window,'Verification receipt exceeded its admitted deadline');
        assert.ok(Number.isSafeInteger(limits.max_sources)&&Number.isSafeInteger(limits.max_bytes)&&limits.max_sources<=sources&&limits.max_bytes<=bytes,'Verification receipt exceeded its admitted source/byte budget');
        limits.deadline_ms=0;limits.effective_deadline_ms=0;
    }
    // Native JSON text content and structuredContent describe the same result.
    if(Array.isArray(result.content))result.content=result.content.map((block:Row)=>{if(block.type!=='text'||typeof block.text!=='string')return block;let text:any;try{text=JSON.parse(block.text);}catch{return block;}return {...block,text:installedParityValue(text)};});
    return result;
}
export function assertInstalledEnvelopeParity(mcp:Row,cli:Row):void {
    const envelope=(raw:Row)=>{
        assert.ok(Array.isArray(raw.content),'Complete original tool envelope required');
        if(Object.hasOwn(raw,'resultType'))assert.equal(raw.resultType,'complete','Only negotiated complete resultType may differ');
        const copy={...raw};delete copy.resultType;return installedParityValue(copy);
    };
    assert.deepEqual(envelope(mcp),envelope(cli),'Complete native/MCP tool envelopes including future fields');
}
export function assertInstalledInputCatalog(operations:Row,displayed:Row):void {
    assert.deepEqual([...displayed.names].sort(),Object.keys(operations).sort(),'Complete Workbench operation catalog');
    assert.ok(displayed.forms&&typeof displayed.forms==='object','Every actual form must be inventoried');
    assert.deepEqual(Object.keys(displayed.forms).sort(),Object.keys(operations).sort(),'Complete Workbench form catalog');
    assert.ok(Object.hasOwn(operations,displayed.tool),'Selected operation is advertised');
    const fields=(schema:Row)=>toolFormFields(schema).map(field=>{
        const select=field.type==='boolean'||!!field.values;
        return {label:field.label,tag:select?'SELECT':'INPUT',type:select?'select-one':field.type==='integer'?'number':'text',required:field.required,min:select?'':String(field.minimum??''),max:select?'':String(field.maximum??''),minLength:select?0:field.minLength??0,maxLength:select?4096:field.maxLength??4096,options:select?['',...(field.type==='boolean'?['true','false']:field.values??[])]:[]};
    });
    for(const name of Object.keys(operations))assert.deepEqual(displayed.forms[name],fields(operations[name]),'Complete supported form field/input constraints: '+name);
    assert.deepEqual(displayed.fields,displayed.forms[displayed.tool],'Selected form field/input constraints');
}
export interface InstalledCopies {json:string;markdown:string;selectedJson?:string;selectedMarkdown?:string}
export function installedWorkbenchCopy(session:WorkbenchSession,format:'json'|'markdown',selected=false):string {
    const turn=session.turns.at(-1);assert.ok(turn,'Actual Workbench turn required');
    const logical=(value:WorkbenchSession['turns'][number])=>materializeNativeMetadata(value.envelope??{});
    return renderExport(session,format,selected?turn.id:undefined,selected?['record-0']:undefined,logical);
}
export function assertInstalledWorkbenchCopies(session:WorkbenchSession,copies:InstalledCopies,selected:boolean):void {
    assert.ok(session.turns.length>0,'Actual Workbench turn required');
    assert.deepEqual(JSON.parse(copies.json),JSON.parse(installedWorkbenchCopy(session,'json')),'Complete full JSON copy');
    assert.equal(copies.markdown,installedWorkbenchCopy(session,'markdown'),'Complete full Markdown copy');
    if(selected){
        assert.ok(copies.selectedJson&&copies.selectedMarkdown,'Actual selected UI copies required');
        assert.deepEqual(JSON.parse(copies.selectedJson),JSON.parse(installedWorkbenchCopy(session,'json',true)),'Explicit selected JSON copy');
        assert.equal(copies.selectedMarkdown,installedWorkbenchCopy(session,'markdown',true),'Explicit selected Markdown copy');
    }
}
