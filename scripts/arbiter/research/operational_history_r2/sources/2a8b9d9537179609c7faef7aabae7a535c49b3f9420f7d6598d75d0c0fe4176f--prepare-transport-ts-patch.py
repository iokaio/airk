# SPDX-License-Identifier: Apache-2.0
"""Stage the strict host read-retry consumer without changing live inputs."""
import difflib
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
OUT=Path(__file__).resolve().parent/'transport-ts-staged'
changes={}
def edit(name,before,after):
    original,current=changes.get(name,((ROOT/name).read_text(encoding='utf-8'),None))
    current=original if current is None else current
    assert current.count(before)==1,(name,before[:90],current.count(before))
    changes[name]=(original,current.replace(before,after,1))
def append(name,text):
    original,current=changes.get(name,((ROOT/name).read_text(encoding='utf-8'),None))
    current=original if current is None else current
    changes[name]=(original,current+text)

edit('packages/vscode/src/explorerEvidence.ts',
"import * as path from 'node:path';",
"import * as path from 'node:path';\nimport { isDeepStrictEqual } from 'node:util';")
marker='export function readRetryDelay(envelope: unknown): number | undefined {'
helper=r'''/** Exact counterpart of Core combined_reads::read_retry::validated_retry.
 * The declared pointer retains the original native receipt. A failed sibling in
 * a partial union never grants permission to replay its successful siblings. */
export function singleRequiredMemberRetry(combined: Record<string, any>): Record<string, unknown> | undefined {
    const object=(value:unknown):value is Record<string,any>=>!!value&&typeof value==='object'&&!Array.isArray(value);
    const integer=(value:unknown,min:number,max=Number.MAX_SAFE_INTEGER):value is number=>Number.isSafeInteger(value)&&Number(value)>=min&&Number(value)<=max;
    const fields=(value:Record<string,unknown>,keys:string[])=>Object.keys(value).every(key=>keys.includes(key));
    const id=(value:unknown,prefix:string)=>typeof value==='string'&&new RegExp(`^${prefix}_[0-9a-f]{32}$`).test(value);
    const digest=(value:unknown)=>typeof value==='string'&&/^[0-9a-f]{64}$/.test(value);
    try {
        if(Buffer.byteLength(JSON.stringify(combined),'utf8')>65536||combined.schema!=='airk.combined-read/1'
            ||combined.partial!==true||combined.truncated!==false||!Array.isArray(combined.ranking)||combined.ranking.length!==0
            ||combined.shared_values!==undefined&&combined.shared_values!==null)return;
        const selected=combined.selection;
        if(!object(selected)||!fields(selected,['project_ids','index_ids','grant_id','grant_revision','member_availability','identity'])
            ||selected.member_availability!=='require'||!Array.isArray(selected.index_ids)||selected.index_ids.length!==1||!id(selected.index_ids[0],'idx')
            ||!Array.isArray(selected.project_ids)||selected.project_ids.length<1||selected.project_ids.length>32
            ||!selected.project_ids.every((value:unknown)=>id(value,'prj'))||new Set(selected.project_ids).size!==selected.project_ids.length
            ||!id(selected.grant_id,'grt')||!integer(selected.grant_revision,1)||!digest(selected.identity))return;
        const index=selected.index_ids[0];
        if(!Array.isArray(combined.members)||combined.members.length!==1)return;
        const member=combined.members[0];
        if(!object(member)||member.index_id!==index||member.outcome!=='failed'||member.is_error!==true
            ||member.cutoff!==undefined&&member.cutoff!==null)return;
        const allocation=member.allocation;
        if(!object(allocation)||!fields(allocation,['index_id','limit','max_bytes','deadline_ms','verification_max_sources','verification_max_bytes','verification_deadline_ms'])
            ||allocation.index_id!==index||!integer(allocation.limit,1,100)||!integer(allocation.max_bytes,1,65536)||!integer(allocation.deadline_ms,1,30000)
            ||!integer(allocation.verification_max_sources,0,256)||!integer(allocation.verification_max_bytes,0,1073741824)
            ||!integer(allocation.verification_deadline_ms,0,30000))return;
        const error=combined.error;
        if(!object(error)||error.code!=='member_unavailable'||error.retryable!==false
            ||!Array.isArray(error.failed_members)||error.failed_members.length!==1||error.failed_members[0]!==index)return;
        const vector=combined.publication_vector;
        if(!object(vector)||!fields(vector,['authority','observed_from_unix_ms','observed_to_unix_ms','members'])||!digest(vector.authority)
            ||!integer(vector.observed_from_unix_ms,0)||!integer(vector.observed_to_unix_ms,vector.observed_from_unix_ms)
            ||!Array.isArray(vector.members)||vector.members.length!==1)return;
        const publication=vector.members[0];
        if(!object(publication)||!fields(publication,['index_id','publication'])||publication.index_id!==index
            ||publication.publication!==undefined&&publication.publication!==null)return;
        const result=member.result;
        if(!object(result)||Object.hasOwn(result,'isError')&&result.isError!==true)return;
        let body=result,suffix='';
        if(Object.hasOwn(result,'structuredContent')){
            if(result.isError!==true||!Array.isArray(result.content)||result.content.length!==1
                ||result.content[0]?.type!=='text'||typeof result.content[0]?.text!=='string')return;
            if(!isDeepStrictEqual(JSON.parse(result.content[0].text),result.structuredContent))return;
            body=result.structuredContent;suffix='/structuredContent';
        }else if(Object.hasOwn(result,'content'))return;
        if(!object(body)||Object.hasOwn(body,'query_replay_safe')&&body.query_replay_safe!==true
            ||Object.hasOwn(body,'query_submitted')&&body.query_submitted!==false)return;
        if(Object.hasOwn(body,'request_id')&&(typeof body.request_id!=='string'||body.request_id.length===0||Buffer.byteLength(body.request_id,'utf8')>128))return;
        if(Object.hasOwn(body,'scope_observation')){
            const scope=body.scope_observation;
            if(!object(scope)||!fields(scope,['schema','qualifier','fingerprint','provenance','grant','members'])||scope.schema!==1
                ||scope.qualifier!=='selected_subset'||!digest(scope.fingerprint)||!['user_reviewed_project_grant','imported_legacy_snapshot'].includes(scope.provenance)
                ||!object(scope.grant)||!fields(scope.grant,['id','revision'])||scope.grant.id!==selected.grant_id||scope.grant.revision!==selected.grant_revision
                ||!Array.isArray(scope.members)||scope.members.length!==1||Buffer.byteLength(JSON.stringify(scope),'utf8')>16384)return;
            const scoped=scope.members[0];
            if(!object(scoped)||!fields(scoped,['index_id','memberships','source_root_ids','source_revision','policy_epoch','policy_digest'])
                ||scoped.index_id!==index||!integer(scoped.source_revision,1)||!integer(scoped.policy_epoch,0)
                ||(scoped.policy_epoch===0?scoped.policy_digest!==undefined&&scoped.policy_digest!==null:!digest(scoped.policy_digest))
                ||!Array.isArray(scoped.memberships)||scoped.memberships.length<1||scoped.memberships.length>32
                ||!scoped.memberships.every((item:unknown)=>object(item)&&fields(item,['project_id','membership_revision'])
                    &&id(item.project_id,'prj')&&selected.project_ids.includes(item.project_id)&&integer(item.membership_revision,1))
                ||new Set(scoped.memberships.map((item:any)=>item.project_id)).size!==scoped.memberships.length
                ||!Array.isArray(scoped.source_root_ids)||scoped.source_root_ids.length>17
                ||!scoped.source_root_ids.every((source:unknown)=>id(source,'src'))||new Set(scoped.source_root_ids).size!==scoped.source_root_ids.length)return;
        }
        let code:string,delay:unknown,fresh:boolean;
        if(body.status==='retry'){
            if(!fields(body,['status','code','retryable','action','retry_after_ms','request_id','scope_observation','query_replay_safe','query_submitted','fresh_brokerage_required'])
                ||!['publication_busy','selection_busy'].includes(body.code)||body.retryable!==true||body.action!=='repeat_same_request'
                ||Object.hasOwn(body,'fresh_brokerage_required')&&body.fresh_brokerage_required!==false)return;
            code=body.code;delay=body.retry_after_ms;fresh=false;
        }else{
            if(!fields(body,['error','request_id','scope_observation','query_replay_safe','query_submitted','fresh_brokerage_required'])||!object(body.error))return;
            const refusal=body.error;
            if(!fields(refusal,['code','reason','query_submitted','retryable','retry_after_ms','fresh_brokerage_required','query_replay_safe'])
                ||!['direct_query_admission','direct_query_setup'].includes(refusal.code))return;
            code=refusal.code;fresh=code==='direct_query_setup';
            if(refusal.reason!=='busy'||refusal.query_submitted!==false||refusal.retryable!==true||refusal.fresh_brokerage_required!==fresh
                ||Object.hasOwn(refusal,'query_replay_safe')&&refusal.query_replay_safe!==true
                ||Object.hasOwn(body,'fresh_brokerage_required')&&body.fresh_brokerage_required!==fresh)return;
            delay=refusal.retry_after_ms;
        }
        if(!integer(delay,1,1000))return;
        const proposed={schema:1,scope:'same_single_required_member',index_id:index,selection_identity:selected.identity,
            action:'repeat_same_request',query_replay_safe:true,result_pointer:`/members/0/result${suffix}`,code,retry_after_ms:delay,fresh_brokerage_required:fresh};
        if(isDeepStrictEqual(combined.read_retry,proposed))return proposed;
    }catch{/* Unparseable or unbounded metadata never permits another read. */}
    return;
}

'''
edit('packages/vscode/src/explorerEvidence.ts',marker,helper+marker)
edit('packages/vscode/src/explorerEvidence.ts',
'''    let delay: unknown;
    if (hint.status === 'retry') {''',
'''    let delay: unknown;
    if (hint.schema === 'airk.combined-read/1') {
        if(value.structuredContent!==undefined&&!isDeepStrictEqual(value.structuredContent,hint))return;
        return singleRequiredMemberRetry(hint)?.retry_after_ms as number|undefined;
    }
    if (hint.status === 'retry') {''')
edit('packages/vscode/src/explorerEvidence.ts',
"import * as path from 'node:path';",
"import * as path from 'node:path';\nimport { validateCatalogSession } from './catalogSession';")
edit('packages/vscode/src/explorerEvidence.ts',
'''export function readRetryDelay(envelope: unknown): number | undefined {''',
'''export function readRetryDelay(envelope: unknown, expectedScope?: unknown): number | undefined {''')
edit('packages/vscode/src/explorerEvidence.ts',
'''    if (hint.status === 'retry') {''',
'''    if (hint.error?.code === 'scoped_status_prepare_timeout' || hint.error?.code === 'scoped_connection_unavailable') {
        const error=hint.error,scope=hint.requested_scope;
        const fields=(value:unknown,keys:string[]):value is Record<string,any>=>!!value&&typeof value==='object'&&!Array.isArray(value)
            &&Object.keys(value).every(key=>keys.includes(key));
        if(!fields(hint,['error','requested_scope'])||!fields(error,['code','query_submitted','retryable','retry_after_ms','fresh_brokerage_required','native_control_response','query_replay_safe'])
            ||error.query_submitted!==false||error.retryable!==true||error.fresh_brokerage_required!==true
            ||Object.hasOwn(error,'query_replay_safe')&&error.query_replay_safe!==true
            ||!fields(scope,['confirmed','selection'])||scope.confirmed!==false||expectedScope===undefined
            ||!isDeepStrictEqual(value.structuredContent,hint))return;
        try {if(!isDeepStrictEqual(validateCatalogSession(scope.selection),validateCatalogSession(expectedScope)))return;}catch{return;}
        if(error.code==='scoped_connection_unavailable'){
            const native=error.native_control_response;
            if(!fields(native,['status','code'])||native.status!=='failure'||!['broker_busy','project_starting_or_unavailable',
                'project_unavailable_or_busy','previous_workers_draining','catalog_revision_changed','catalog_clock_pending',
                'catalog_connection_busy','project_capacity_busy','project_restart_backoff'].includes(native.code))return;
        }else if(error.native_control_response!==undefined)return;
        return Number.isInteger(error.retry_after_ms)&&error.retry_after_ms>=1&&error.retry_after_ms<=1000?error.retry_after_ms:undefined;
    }
    if (hint.status === 'retry') {''')
edit('packages/vscode/src/explorerEvidence.ts',
'''readRetryDelay(envelope);''',
'''readRetryDelay(envelope,this.spec.catalogSession);''')

edit('packages/vscode/src/test/nativeReadRetry.ts',
'''observer?:QuickStatusObserver):Promise<RecordValue>{''',
'''observer?:QuickStatusObserver,expectedScope?:unknown):Promise<RecordValue>{''')
name='packages/vscode/src/test/nativeReadRetry.ts'
original,current=changes[name]
start=current.index('export async function observeQuickStatus(')
end=current.index('export async function readNative(',start)
part=current[start:end].replace('readRetryDelay(status)','readRetryDelay(status,expectedScope)')
current=current[:start]+part+current[end:]
changes[name]=original,current
edit(name,
'''    const deadline=Math.min(until??Infinity,performance.now()+120000);
    while(true){''',
'''    const deadline=Math.min(until??Infinity,performance.now()+120000);
    // Only the original launch argument can bind an unconfirmed setup receipt.
    const selectors=args.flatMap((arg,index)=>arg==='--catalog-session'?[args[index+1]]:[]);
    let expectedScope:unknown;
    if(selectors.length===1){try{expectedScope=JSON.parse(selectors[0]);}catch{/* Refuse scoped replay without a valid selector. */}}
    while(true){''')
edit(name,'''                const delay=readRetryDelay(envelope);''',
'''                const delay=readRetryDelay(envelope,expectedScope);''')
edit('packages/vscode/src/test/nativeSmoke.test.ts',
'''async function quickStatus(session:McpStdioSession,observer?:QuickStatusObserver):Promise<any>{''',
'''async function quickStatus(session:McpStdioSession,observer?:QuickStatusObserver,expectedScope?:LaunchSpec['catalogSession']):Promise<any>{''')
edit('packages/vscode/src/test/nativeSmoke.test.ts',
'''    },undefined,observer);''',
'''    },undefined,observer,expectedScope);''')
# Each explicit scope is supplied by that fixture's original launch descriptor.
name='packages/vscode/src/test/nativeSmoke.test.ts'
original,current=changes[name]
current=current.replace('quickStatus(session);','quickStatus(session,undefined,spec.catalogSession);')
current=current.replace('quickStatus(session,observe)','quickStatus(session,observe,spec.catalogSession)')
current=current.replace('sessions.map(session=>quickStatus(session))','sessions.map(session=>quickStatus(session,undefined,binding===\'pinned\'?spec.catalogSession:undefined))')
# Source discovery is deliberately unselected even after the fixture registers.
start=current.index("test('one root-scoped MCP session")
end=current.index("test('reviewed native setup",start)
current=current[:start]+current[start:end].replace('quickStatus(session,undefined,spec.catalogSession)','quickStatus(session)')+current[end:]
changes[name]=original,current

edit('packages/vscode/src/native.ts',
'''import { McpSessionError, McpStdioSession } from './mcpStdioSession';''',
'''import { McpSessionError, McpStdioSession } from './mcpStdioSession';
import { readRetryDelay } from './explorerEvidence';''')
edit('packages/vscode/src/native.ts',
'''        session = (await McpStdioSession.open({ command: entry?.command??spec.command, args: entry?[...entry.args]:launchArgs(spec), cwd: spec.service ? path.dirname(spec.command) : spec.root,''',
'''        const selectedArgs=entry?[...entry.args]:launchArgs(spec);
        const selectors=selectedArgs.flatMap((arg,index)=>arg==='--catalog-session'?[selectedArgs[index+1]]:[]);
        let expectedScope:unknown;
        if(selectors.length===1){try{expectedScope=JSON.parse(selectors[0]);}catch{/* No scoped retry without the original validated selector. */}}
        session = (await McpStdioSession.open({ command: entry?.command??spec.command, args:selectedArgs, cwd: spec.service ? path.dirname(spec.command) : spec.root,''')
edit('packages/vscode/src/native.ts',
'''if (!message.includes('workspace is not indexed')) { throw new Error(`AIRK MCP status failed: ${message.slice(0, 300)}`); }''',
'''if (!message.includes('workspace is not indexed')) { throw Object.assign(new Error(`AIRK MCP status failed: ${message.slice(0, 300)}`),{nativeResponse:status}); }''')
edit('packages/vscode/src/native.ts',
'''        throw new Error(`AIRK connection check failed while ${stage}: ${error instanceof Error?error.message:String(error)}`);''',
'''        const detail=error as {nativeResponse?:unknown}|null;
        throw Object.assign(new Error(`AIRK connection check failed while ${stage}: ${error instanceof Error?error.message:String(error)}`),
            detail?.nativeResponse!==undefined?{nativeResponse:detail.nativeResponse}:{});''')

edit('packages/vscode/src/native.ts',
'''        const status = await session.request('tools/call', { name: expectedTools>=26?'status':'airk_status', arguments: {} }, remaining());''',
'''        let status;
        while(true){
            status=await session.request('tools/call',{name:expectedTools>=26?'status':'airk_status',arguments:{}},remaining());
            const delay=readRetryDelay(status,expectedScope);
            if(delay===undefined||Date.now()+delay>=deadline)break;
            await new Promise(resolve=>setTimeout(resolve,delay));
        }''')

append('packages/vscode/src/test/nativeReadRetry.test.ts',r'''

const preparedSelection={schema:1 as const,context_key:'06'.repeat(32),namespace:'07'.repeat(16),expected_catalog_revision:7,
    grant_id:'grt_'+'03'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'01'.repeat(16),project_ids:['prj_'+'02'.repeat(16)]};
function scopedSetupReceipt(code='scoped_status_prepare_timeout',nativeCode?:string):any{
    const body={error:{code,retryable:true,query_submitted:false,retry_after_ms:1,fresh_brokerage_required:true,
        ...(nativeCode?{native_control_response:{status:'failure',code:nativeCode}}:{})},requested_scope:{confirmed:false,selection:preparedSelection}};
    return {isError:true,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body};
}
test('scoped preparation retry requires the original exact launch selection and an explicit known pre-query hint',async()=>{
    const timeout=scopedSetupReceipt();
    assert.equal(readRetryDelay(timeout,preparedSelection),1);
    assert.equal(readRetryDelay(timeout),undefined);
    for(const code of ['catalog_connection_busy','project_capacity_busy','project_restart_backoff','previous_workers_draining'])
        assert.equal(readRetryDelay(scopedSetupReceipt('scoped_connection_unavailable',code),preparedSelection),1);
    for(const code of ['scoped_connection_unavailable','scoped_read_unavailable','catalog_connection_outcome_unknown','native catalog exchange still pending'])
        assert.equal(readRetryDelay(scopedSetupReceipt('scoped_connection_unavailable',code),preparedSelection),undefined);
    for(const mutation of [
        (body:any)=>{body.error.query_submitted=true;},(body:any)=>{body.error.query_submitted=null;},
        (body:any)=>{body.error.retryable=false;},(body:any)=>{body.error.retry_after_ms=1001;},
        (body:any)=>{delete body.error.fresh_brokerage_required;},(body:any)=>{body.requested_scope.confirmed=true;},
        (body:any)=>{body.requested_scope.selection.grant_id='grt_'+'09'.repeat(16);},
        (body:any)=>{body.requested_scope.selection.expected_catalog_revision++;},(body:any)=>{body.hits=[];},
        (body:any)=>{body.error.native_control_response={status:'failure',code:'broker_busy'};},
    ]){
        const reply=structuredClone(timeout);mutation(reply.structuredContent);reply.content[0].text=JSON.stringify(reply.structuredContent);
        assert.equal(readRetryDelay(reply,preparedSelection),undefined,mutation.toString());
    }
    const replies=[timeout,{index_state:'available'}];let calls=0;const deadlines:number[]=[];
    assert.deepEqual(await observeQuickStatus(async deadline=>{deadlines.push(deadline);return replies[calls++];},undefined,undefined,preparedSelection),{index_state:'available'});
    assert.equal(calls,2);assert.equal(new Set(deadlines).size,1);
});

test('connection probe follows only scope-bound preparation hints and retains a refused native result',async t=>{
    const {probeMcp}=await import('../native'),{UNIFIED_TOOLS}=await import('../model');
    const spec={command:process.execPath,root:process.cwd(),dataDir:process.cwd(),collection:'fixture',name:'fixture',catalogSession:preparedSelection,
        service:{installDir:process.cwd(),project:'01'.repeat(32)}};
    const setup=scopedSetupReceipt();
    let calls=0,closed=0;
    const requests:unknown[]=[];
    let replies:any[]=[setup,{content:[{type:'text',text:'{"index_state":"available"}'}]}];
    const session={request:async(method:string,args:unknown)=>{
        if(method==='tools/list')return {tools:UNIFIED_TOOLS.map(name=>({name}))};
        requests.push(args);return replies[calls++];
    },close:()=>{closed++;}};
    t.mock.method(McpStdioSession,'open',async()=>({session} as any));
    await probeMcp(spec);assert.equal(calls,2);assert.equal(closed,1);assert.deepEqual(requests[0],requests[1]);
    const refused=structuredClone(setup);refused.structuredContent.requested_scope.selection.expected_grant_revision=2;
    refused.content[0].text=JSON.stringify(refused.structuredContent);replies=[refused];calls=0;
    await assert.rejects(probeMcp(spec),error=>{
        assert.deepEqual((error as Error&{nativeResponse:unknown}).nativeResponse,refused);return true;
    });
    assert.equal(calls,1);assert.equal(closed,2);
});
''')

append('packages/vscode/src/test/nativeReadRetryProcess.test.ts',r'''

test('CLI setup-timeout retries retain the original exact catalog launch selection',async()=>{
    const selection={schema:1,context_key:'06'.repeat(32),namespace:'07'.repeat(16),expected_catalog_revision:7,
        grant_id:'grt_'+'03'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'01'.repeat(16),project_ids:['prj_'+'02'.repeat(16)]};
    const body={error:{code:'scoped_status_prepare_timeout',query_submitted:false,retryable:true,retry_after_ms:1,fresh_brokerage_required:true},
        requested_scope:{confirmed:false,selection}};
    const setup={isError:true,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body};
    const confirmed={content:[{type:'text',text:'{"files":0}'}]};
    await fixture(async(directory,counter)=>{
        const source=program(counter,`console.log(JSON.stringify(calls===1?${JSON.stringify(setup)}:${JSON.stringify(confirmed)}));process.exitCode=calls===1?1:0;`);
        const args=['-e',source,'--','--catalog-session',JSON.stringify(selection)];
        assert.deepEqual(await readNative(process.execPath,args,directory),confirmed);
        assert.equal(await fs.readFile(counter,'utf8'),'2');
    });
    await fixture(async(directory,counter)=>{
        const source=program(counter,`console.log(${JSON.stringify(JSON.stringify(setup))});process.exitCode=1;`);
        const args=['-e',source,'--','--catalog-session',JSON.stringify({...selection,expected_grant_revision:2})];
        await assert.rejects(readNative(process.execPath,args,directory),error=>error instanceof NativeProcessError&&error.code===1);
        assert.equal(await fs.readFile(counter,'utf8'),'1');
    });
});
''')

edit('packages/vscode/src/test/nativeReadRetry.ts',
'''    const body=JSON.parse(reply.content[0].text);
    return body.error?{code:body.error.code,query_submitted:false}:{code:body.code,action:'repeat_same_request'};''',
'''    const body=JSON.parse(reply.content[0].text);
    if(body.schema==='airk.combined-read/1'){
        const code=body.read_retry.code;
        return ['direct_query_admission','direct_query_setup'].includes(code)?{code,query_submitted:false}:{code,action:'repeat_same_request'};
    }
    return body.error?{code:body.error.code,query_submitted:false}:{code:body.code,action:'repeat_same_request'};''')
append('packages/vscode/src/test/nativeReadRetry.test.ts',r'''

function singleCombinedRetry(hint:any={status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:25}):any{
    const index='idx_'+'01'.repeat(16),identity='04'.repeat(32);
    const result={isError:true,content:[{type:'text',text:JSON.stringify(hint)}],structuredContent:hint};
    const body={schema:'airk.combined-read/1',selection:{project_ids:['prj_'+'02'.repeat(16)],index_ids:[index],grant_id:'grt_'+'03'.repeat(16),grant_revision:1,member_availability:'require',identity},
        publication_vector:{authority:'05'.repeat(32),observed_from_unix_ms:1,observed_to_unix_ms:2,members:[{index_id:index,publication:null}]},
        members:[{index_id:index,is_error:true,outcome:'failed',allocation:{index_id:index,limit:1,max_bytes:4096,deadline_ms:2000,verification_max_sources:1,verification_max_bytes:4096,verification_deadline_ms:2000},result}],
        ranking:[],partial:true,truncated:false,error:{code:'member_unavailable',retryable:false,failed_members:[index]},
        read_retry:{schema:1,scope:'same_single_required_member',index_id:index,selection_identity:identity,action:'repeat_same_request',query_replay_safe:true,
            result_pointer:'/members/0/result/structuredContent',code:hint.code??hint.error?.code,retry_after_ms:hint.retry_after_ms??hint.error?.retry_after_ms,
            fresh_brokerage_required:hint.error?.code==='direct_query_setup'}};
    return body;
}
const combinedEnvelope=(body:any)=>({isError:true,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body});

test('combined retry preserves complete qualified framing and rejects unknown evidence or crossed scope',()=>{
    const original=singleCombinedRetry();
    const scope={schema:1,qualifier:'selected_subset',fingerprint:'08'.repeat(32),provenance:'user_reviewed_project_grant',
        grant:{id:original.selection.grant_id,revision:1},members:[{index_id:original.selection.index_ids[0],
            memberships:[{project_id:original.selection.project_ids[0],membership_revision:1}],source_root_ids:['src_'+'07'.repeat(16)],source_revision:1,policy_epoch:0,policy_digest:null}]};
    const hint={...original.members[0].result.structuredContent,scope_observation:scope,request_id:'bounded-request'};
    assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry(hint))),25);
    for(const key of ['unknown_future','index_state','chunks','symbols','metadata','hits']){
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({...hint,[key]:[]}))),undefined,key);
        const error={code:'direct_query_admission',reason:'busy',query_submitted:false,retryable:true,fresh_brokerage_required:false,retry_after_ms:25,[key]:[]};
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({error}))),undefined,'error.'+key);
    }
    for(const change of [
        (value:any)=>{value.grant.revision=2;},(value:any)=>{value.members[0].index_id='idx_'+'09'.repeat(16);},
        (value:any)=>{value.members[0].memberships[0].project_id='prj_'+'09'.repeat(16);},
        (value:any)=>{value.members[0].policy_epoch=1;},(value:any)=>{value.members[0].memberships=[];},
        (value:any)=>{value.unrecognized=true;},
    ]){
        const invalid=structuredClone(scope);change(invalid);
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({...hint,scope_observation:invalid}))),undefined,change.toString());
    }
    assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({...hint,request_id:'é'.repeat(65)}))),undefined);
});

test('combined retry requires declared exact single-member admission without partial evidence or widening',()=>{
    const original=singleCombinedRetry();
    assert.equal(readRetryDelay(combinedEnvelope(original)),25);
    for(const code of ['publication_busy','selection_busy']){
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({status:'retry',code,retryable:true,action:'repeat_same_request',retry_after_ms:1}))),1);
    }
    for(const code of ['direct_query_admission','direct_query_setup']){
        const body=singleCombinedRetry({error:{code,reason:'busy',query_submitted:false,retryable:true,fresh_brokerage_required:code==='direct_query_setup',retry_after_ms:50}});
        assert.equal(readRetryDelay(combinedEnvelope(body)),50);
    }
    const mutations:Array<(body:any)=>void>=[
        body=>{delete body.read_retry;},body=>{body.selection.member_availability='report';},body=>{body.selection.identity='09'.repeat(32);},
        body=>{body.members.push(structuredClone(body.members[0]));},body=>{body.members[0].outcome='complete';},
        body=>{body.members[0].is_error=false;},body=>{body.members[0].cutoff='budget';},body=>{body.members[0].allocation.limit=0;},
        body=>{body.members[0].allocation.verification_max_sources=257;},body=>{body.publication_vector.members[0].publication={token:'actual-result'};},
        body=>{body.ranking.push({result_pointer:'/members/0/result/hits/0'});},body=>{body.partial=false;},body=>{body.truncated=true;},
        body=>{body.error.retryable=true;},body=>{body.read_retry.query_replay_safe=false;},body=>{body.read_retry.result_pointer='/members/0/result/error';},
        body=>{body.read_retry.retry_after_ms=1;},body=>{body.members[0].result.content[0].text='different';},
        body=>{const hint=body.members[0].result.structuredContent;hint.hits=[];body.members[0].result.content[0].text=JSON.stringify(hint);},
        body=>{const hint=body.members[0].result.structuredContent;hint.query_replay_safe=false;body.members[0].result.content[0].text=JSON.stringify(hint);},
    ];
    for(const mutate of mutations){const body=structuredClone(original);mutate(body);assert.equal(readRetryDelay(combinedEnvelope(body)),undefined,mutate.toString());}
    for(const hint of [
        {status:'retry',code:'unknown_busy',retryable:true,action:'repeat_same_request',retry_after_ms:25},
        {status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1001},
        {error:{code:'unknown',retryable:true,nested:original.members[0].result.structuredContent}},
        {error:{code:'direct_query_setup',reason:'busy',query_submitted:true,retryable:true,fresh_brokerage_required:true,retry_after_ms:25}},
    ])assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry(hint))),undefined);
    const mismatched=combinedEnvelope(original);mismatched.structuredContent=singleCombinedRetry({status:'retry',code:'publication_busy',retryable:true,action:'repeat_same_request',retry_after_ms:25});
    assert.equal(readRetryDelay(mismatched),undefined);
});
''')

edit('packages/vscode/src/test/nativeReadRetry.ts',
'''export function readRetryReceipt(reply:any):ReadRetryReceipt{''',
'''export function readRetryReceipt(reply:any,expectedScope?:unknown):ReadRetryReceipt{''')
edit('packages/vscode/src/test/nativeReadRetry.ts',
'''assert.notEqual(readRetryDelay(reply),undefined,''',
'''assert.notEqual(readRetryDelay(reply,expectedScope),undefined,''')
edit('packages/vscode/src/test/nativeReadRetry.ts',
'''onReply?:(reply:any)=>void):Promise<any>{''',
'''onReply?:(reply:any)=>void,expectedScope?:unknown):Promise<any>{''')
edit('packages/vscode/src/test/nativeReadRetry.ts',
'''        const delay=readRetryDelay(result);''',
'''        const delay=readRetryDelay(result,expectedScope);''')
edit('packages/vscode/src/test/nativeSmoke.test.ts',
'''requestWithReadRetry(session!,name,args,name.endsWith('diagnostics')?60000:15000)''',
'''requestWithReadRetry(session!,name,args,name.endsWith('diagnostics')?60000:15000,undefined,spec.catalogSession)''')
edit('packages/vscode/src/test/nativeSmoke.test.ts',
'''requestWithReadRetry(session,'read_file',{path:'session.rs',start_line:1,end_line:3},30000)''',
'''requestWithReadRetry(session,'read_file',{path:'session.rs',start_line:1,end_line:3},30000,undefined,spec.catalogSession)''')
edit('packages/vscode/src/test/nativeSmoke.test.ts',
'''                        refusals.push({binding,round,tool:name,...readRetryReceipt(reply)});
                    })))''',
'''                        refusals.push({binding,round,tool:name,...readRetryReceipt(reply,spec.catalogSession)});
                    },spec.catalogSession)))''')
edit('packages/vscode/src/test/explorerAssistanceNative.test.ts',
'''requestWithReadRetry(evidence!,name,args,120000,reply=>{lastRead={name,reply};})''',
'''requestWithReadRetry(evidence!,name,args,120000,reply=>{lastRead={name,reply};},catalogSpec.catalogSession)''')
edit('packages/vscode/src/test/explorerAssistanceNative.test.ts',
'''requestWithReadRetry(selectedSession,'search',{query,mode:'lexical',response_mode:'full'},120000,reply=>{lastRead={query,reply};})''',
'''requestWithReadRetry(selectedSession,'search',{query,mode:'lexical',response_mode:'full'},120000,reply=>{lastRead={query,reply};},selectedSpec.catalogSession)''')
edit('packages/vscode/src/test/explorerAssistanceNative.test.ts',
'''requestWithReadRetry(selectedSession,'search',{query:foreignQuery,mode:'lexical',response_mode:'full'},120000,reply=>{lastRead={query:foreignQuery,reply};})''',
'''requestWithReadRetry(selectedSession,'search',{query:foreignQuery,mode:'lexical',response_mode:'full'},120000,reply=>{lastRead={query:foreignQuery,reply};},selectedSpec.catalogSession)''')

edit('packages/vscode/src/test/nativeReadRetry.ts',
'''                    reject(new NativeProcessError(command,error,stderr,Math.round(performance.now()-started),timeout));return;''',
'''                    reject(Object.assign(new NativeProcessError(command,error,stderr,Math.round(performance.now()-started),timeout),
                        {nativeStdout:stdout,nativeStderr:stderr,...(envelope!==undefined?{nativeResponse:envelope}:{})}));return;''')
edit('packages/vscode/src/test/nativeReadRetryProcess.test.ts',
'''            await assert.rejects(readNative(process.execPath,['-e',source],directory),error=>
                error instanceof NativeProcessError&&error.code===code&&error.stderr.includes('original process failure'));''',
'''            await assert.rejects(readNative(process.execPath,['-e',source],directory),error=>{
                assert.ok(error instanceof NativeProcessError);
                assert.equal(error.code,code);assert.ok(error.stderr.includes('original process failure'));
                const failure=error as NativeProcessError&{nativeStdout:string;nativeStderr:string;nativeResponse:unknown};
                assert.equal(failure.nativeStdout,JSON.stringify(reply)+'\\n');
                assert.equal(failure.nativeStderr,'original process failure\\n');
                assert.deepEqual(failure.nativeResponse,reply);return true;
            });''')
edit('packages/vscode/src/test/explorerNative.test.ts',
'''        await assert.rejects(client.call('airk_context', { path: 'session.ts', line: 1, before: 5, after: 5, revision: token, known_hash: hits[0].citation.content_hash }), /citation|index|revision/i);
    } finally {
        client?.dispose();''',
'''        await assert.rejects(client.call('airk_context', { path: 'session.ts', line: 1, before: 5, after: 5, revision: token, known_hash: hits[0].citation.content_hash }), /citation|index|revision/i);
    } catch(error) {await fixture.retainFailure(error);throw error;} finally {
        client?.dispose();''')
edit('packages/vscode/src/test/diagnosticsNative.test.ts',
'''    }finally{await fixture.dispose();}''',
'''    }catch(error){await fixture.retainFailure(error);throw error;}finally{await fixture.dispose();}''')
edit('packages/vscode/src/test/universalLauncher.test.ts',
'''    } finally {
        // Closing a probe''',
'''    } catch(error) {await fixture.retainFailure(error);throw error;} finally {
        // Closing a probe''')

OUT.mkdir(parents=True,exist_ok=True)
patch=[];manifest=[]
for name,(original,value) in changes.items():
    destination=OUT/name;destination.parent.mkdir(parents=True,exist_ok=True);destination.write_text(value,encoding='utf-8',newline='\n')
    patch.extend(difflib.unified_diff(original.splitlines(True),value.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
    manifest.append({'path':name,'original_sha256':hashlib.sha256((ROOT/name).read_bytes()).hexdigest(),'staged_sha256':hashlib.sha256(destination.read_bytes()).hexdigest()})
(OUT.parent/'transport-ts.patch').write_text(''.join(patch),encoding='utf-8',newline='\n')
(OUT.parent/'transport-ts-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(changes),'patch':str(OUT.parent/'transport-ts.patch')}))
