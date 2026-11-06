// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {splitFixture} from './fixtures/split';
import {SPLIT_JOURNAL_BYTES} from '../indexingSplit';
import {IntentJournal,IntentStorage} from '../serviceIntents';
import {ServiceClient,ServiceError,ControlTransport} from '../serviceClient';
import {ReferenceRefreshIntent,ReferenceRefreshFlow,REFRESH_JOURNAL_BYTES,refreshDigest,refreshJobId,validateRefreshIntent,refreshExport,refreshCapability,refreshSelectionIdentity,refreshSelectionCanonicalJson,validateRefreshReceipt,refreshQueryTool} from '../referenceRefresh';
type Value=Record<string,unknown>;
class Memory implements IntentStorage {data=new Map<string,unknown>();keys(){return [...this.data.keys()];}get<T>(key:string){return this.data.get(key) as T|undefined;}async update(key:string,value:unknown){if(value===undefined)this.data.delete(key);else this.data.set(key,structuredClone(value));}}
const id=(byte:string)=>byte.repeat(32),hash=(byte:string)=>byte.repeat(64);
function fixture(nonce='4'):ReferenceRefreshIntent {
    const originalQuery={tool:'airk_search',arguments:{query:'selected original',max_bytes:8192,reply_detail:'full',verification:'required',semantic:'off',future_query:{preserve:true}}};
    const originalResult={publication_vector:[{index_id:'idx_'+id('6'),publication:'airk-r1:'+id('7')+':1:'+hash('8')}],hits:[{text:'Original historical snippet',content_hash:hash('9')}],future_native:{all_fields:['preserved']}};
    const members=[{index_id:'idx_'+id('6'),original_publication:'airk-r1:'+id('7')+':1:'+hash('8'),sources:[{source_root_id:'src_'+id('a'),source_relative_path:'one.txt',canonical_root:'/owned/source',indexed_hash:hash('9')}]}];
    const key={namespace:id('1'),epoch:0,nonce:id(nonce)},caller=id('2');
    const selection={schema:1,caller,key,original_result_digest:refreshDigest(originalResult),original_query_digest:refreshDigest(originalQuery),original_evidence_selection_digest:refreshDigest(members),mode:'selected_only',selection:{schema:1,context_key:hash('3'),namespace:id('e'),expected_catalog_revision:1,grant_id:'grt_'+id('5'),expected_grant_revision:1,project_ids:['prj_'+id('b')],index_ids:['idx_'+id('6')]},members};
    return validateRefreshIntent({reference_refresh:{configure_project:{project:hash('c'),caller,key,request:{incarnation:id('7'),settings:hash('d'),configuration:{operation:'refresh_references',selection}}},original_query:originalQuery,original_result:originalResult,evidence_retention:'memory_only'}});
}
function receipt(intent:ReferenceRefreshIntent):Value {
    const selected=intent.reference_refresh.configure_project.request.configuration.selection as Value,members=selected.members as Value[],before=String(members[0].original_publication),after='airk-r1:'+id('7')+':2:'+hash('e');
    return {status:'reference_refresh_receipt',job:{id:refreshJobId(intent),project:intent.reference_refresh.configure_project.project,kind:'configure',state:'succeeded',fencing_generation:1,attempts:1,outcome:{state:'succeeded',revision:after}},configuration:{engine:hash('f'),request:structuredClone(intent.reference_refresh.configure_project.request)},result:{schema:'airk.reference-refresh-result/1',selection_digest:refreshSelectionIdentity(selected),original_query_digest:selected.original_query_digest,effective_mode:'selected_only',effective_scope:'selected_sources',before_publication:before,after_publication:after,report:{generation:2,path_scoped:true,future_report:'retained'},status:{publication:{token:after},future_status:[1,2,3]},requery_executed:false,future_result:{retained:true}},expired:false,future_envelope:'retained'};
}
function environment(intent:ReferenceRefreshIntent,lost=false){
    let response=receipt(intent),trusted=true,reconfirmChanged=false,queryCalls=0,mutationCalls=0,readCalls=0;const requests:unknown[]=[];
    const transport:ControlTransport={dispose(){},async request(method){requests.push(structuredClone(method));const name=Object.keys(method as object)[0];if(name==='configure_project'){mutationCalls++;if(lost)throw new ServiceError('lost_native_reply',true);return {status:'job',job:structuredClone(response.job)};}
        if(name==='reference_refresh_receipt'){readCalls++;return structuredClone(response);}throw new Error('unexpected_native_method:'+name);}};
    const storage=new Memory(),journal=new IntentJournal(storage,'owned-control',id('1')),client=new ServiceClient(transport);
    const host={trusted:()=>trusted,confirm:async()=>true,reconfirm:async(selected:Value)=>({...structuredClone(selected),...(reconfirmChanged?{expected_grant_revision:2}:{})}),query:async(query:Value)=>{queryCalls++;assert.deepEqual(query,intent.reference_refresh.original_query);return {publication_vector:[{current:true}],future_query_result:{preserve:true}};}};
    const flow=new ReferenceRefreshFlow(client,journal,host);
    return {flow,journal,storage,client,host,requests,setResponse:(value:Value)=>{response=value;},revoke:()=>{trusted=false;},changeGrant:()=>{reconfirmChanged=true;},counts:()=>({queryCalls,mutationCalls,readCalls})};
}
test('refresh requires explicit native capability and current reviewed grant before reservation/dispatch',async()=>{
    const intent=fixture(),e=environment(intent);assert.equal(refreshCapability({capabilities:{policy_contract:2}}),false);
    await assert.rejects(e.flow.reviewAndSubmit(intent,{capabilities:{policy_contract:2}}),/capability_required/);e.changeGrant();
    await assert.rejects(e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}}),/scope_changed/);assert.equal(e.journal.list().length,0);assert.equal(e.counts().mutationCalls,0);
});
test('lost response survives restart; only exact read receipt reconciliation can settle',async()=>{
    const intent=fixture(),e=environment(intent,true);await assert.rejects(e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}}),/outcome_unknown/);
    assert.deepEqual(e.counts(),{mutationCalls:1,readCalls:0,queryCalls:0});const saved=e.journal.list()[0];assert.equal(saved.value.reservedObservationBytes,REFRESH_JOURNAL_BYTES);assert.equal((saved.value.intent as ReferenceRefreshIntent).reference_refresh.original_query,null);assert.equal((saved.value.intent as ReferenceRefreshIntent).reference_refresh.original_result,null);
    const reopened=new IntentJournal(e.storage,'owned-control',id('1'));await assert.rejects(reopened.submit(e.client,saved.id),/receipt_reconciliation/);
    await e.flow.check(saved.id);assert.deepEqual(e.counts(),{mutationCalls:1,readCalls:1,queryCalls:0});assert.deepEqual(reopened.list()[0].value.nativeRefreshReceipt,receipt(intent));
});
test('missing or expired latest lookup preserves complete historical acknowledgment and never replays',async()=>{
    const intent=fixture(),e=environment(intent);const saved=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});await e.flow.check(saved!);
    const known=structuredClone(e.journal.list()[0].value.nativeRefreshReceipt);e.setResponse({status:'reference_refresh_receipt',job:null,configuration:null,result:null,expired:true,future_unknown:'retained'});await e.flow.check(saved!);
    assert.deepEqual(e.journal.list()[0].value.nativeRefreshReceipt,known);assert.equal(e.journal.list()[0].value.nativeRefreshLookup?.expired,true);await assert.rejects(e.flow.requery(saved!),/fresh_completed/);assert.equal(e.counts().mutationCalls,1);assert.equal(e.counts().queryCalls,0);
});
test('explicit requery keeps complete original query/verification and distinct full historical/current results',async()=>{
    const intent=fixture(),original=structuredClone(intent.reference_refresh.original_result),e=environment(intent);const saved=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});
    assert.equal(e.counts().queryCalls,0);const result=await e.flow.requery(saved!);assert.deepEqual(result.original,original);assert.deepEqual(intent.reference_refresh.original_result,original);assert.equal(e.counts().queryCalls,1);
    const exported=refreshExport(intent,e.journal.list()[0].value.nativeRefreshLookup,e.journal.list()[0].value.nativeRefreshReceipt,result.current);const json=JSON.parse(exported.json);
    assert.deepEqual(json.original_historical_result,original);assert.deepEqual(json.original_query,intent.reference_refresh.original_query);assert.deepEqual(json.last_known_historical_native_receipt,receipt(intent));assert.deepEqual(json.current_requery_result,result.current);assert.match(exported.markdown,/future_envelope/);assert.match(exported.markdown,/future_query_result/);
});
test('crossed native proof is retained for diagnosis without overwriting accepted historical proof',async()=>{
    const intent=fixture(),e=environment(intent);const saved=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});await e.flow.check(saved!);const known=structuredClone(e.journal.list()[0].value.nativeRefreshReceipt),bad=receipt(intent);(bad.result as Value).original_query_digest=hash('0');e.setResponse(bad);
    await assert.rejects(e.flow.check(saved!));assert.deepEqual(e.journal.list()[0].value.nativeRefreshReceipt,known);assert.deepEqual(e.journal.list()[0].value.nativeRefreshLookup,bad);assert.equal(e.counts().mutationCalls,1);
});
test('workspace trust loss refuses mutation or later requery without new grant or replay',async()=>{
    const intent=fixture(),e=environment(intent);const saved=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});e.revoke();await assert.rejects(e.flow.requery(saved!),/not_trusted/);assert.equal(e.counts().queryCalls,0);assert.equal(e.counts().mutationCalls,1);
});
test('all terminal space is reserved before dispatch, aggregate remains2MiB and ordinary record cap remains unchanged',async()=>{
    const intent=fixture(),e=environment(intent);for(const digit of ['1','2','3','4','5','6','7','8'])await e.journal.prepare('Refresh',fixture(digit));assert.equal(e.journal.list().length,8);
    await assert.rejects(e.journal.prepare('Would exceed aggregate',fixture('9')),/byte_capacity/);assert.equal(e.counts().mutationCalls,0);
    const ordinary={enqueue:{project:hash('c'),caller:id('2'),key:{namespace:id('1'),epoch:1,nonce:id('a')},kind:'index' as const}},storage=new Memory(),journal=new IntentJournal(storage,'other-control',id('1'));await journal.prepare('Ordinary',ordinary);const entry=storage.keys()[0],saved=storage.get<Value>(entry)!;assert.equal(saved.reservedObservationBytes,undefined);saved.extra='x'.repeat(65536);await storage.update(entry,saved);assert.throws(()=>journal.list(),/record_invalid/);
});

test('mixed split/refresh terminal reservations preserve existing192KiB split and2MiB aggregate',async()=>{
    const split=splitFixture(),storage=new Memory(),journal=new IntentJournal(storage,'mixed-control',split.namespace);
    const splitId=await journal.prepare('Split',{catalog_split_apply:{draft:split.draft}});assert.equal(journal.list().find(v=>v.id===splitId)?.value.reservedObservationBytes,SPLIT_JOURNAL_BYTES);assert.equal(SPLIT_JOURNAL_BYTES,192*1024);
    const refresh=(digit:string)=>{const intent=fixture(digit),request=intent.reference_refresh.configure_project,selection=request.request.configuration.selection as Value;request.key.namespace=split.namespace;(selection.key as Value).namespace=split.namespace;return validateRefreshIntent(intent);};
    for(const digit of ['1','2','3','4','6','7','8'])await journal.prepare('Refresh',refresh(digit));
    await assert.rejects(journal.prepare('Exceeds mixed aggregate',refresh('9')),/byte_capacity/);assert.equal(journal.list().length,8);
});

test('memory-only restart retains exact native intent without query/snippets and refuses automatic or unavailable requery',async()=>{
    const intent=fixture(),e=environment(intent);const saved=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});const reopened=new IntentJournal(e.storage,'owned-control',id('1'));const serialized=JSON.stringify(reopened.list());assert.ok(!serialized.includes('Original historical snippet'));assert.ok(!serialized.includes('selected original'));
    const restarted=new ReferenceRefreshFlow(e.client,reopened,e.host);await restarted.check(saved!);await assert.rejects(restarted.requery(saved!),/not_retained/);assert.equal(e.counts().mutationCalls,1);assert.equal(e.counts().queryCalls,0);
});
test('local evidence retention requires existing explicit approval and preserves complete originals after restart',async()=>{
    const intent=fixture();intent.reference_refresh.evidence_retention='local_approved';const e=environment(intent);await assert.rejects(e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}}),/persistence_not_authorized/);assert.equal(e.journal.list().length,0);
    const flow=new ReferenceRefreshFlow(e.client,e.journal,{...e.host,persistEvidenceAllowed:()=>true});const saved=await flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});const reopened=new IntentJournal(e.storage,'owned-control',id('1'));assert.deepEqual((reopened.list()[0].value.intent as ReferenceRefreshIntent).reference_refresh.original_result,intent.reference_refresh.original_result);
    const restarted=new ReferenceRefreshFlow(e.client,reopened,{...e.host,persistEvidenceAllowed:()=>true});const result=await restarted.requery(saved!);assert.deepEqual(result.original,intent.reference_refresh.original_result);
});

test('an uncertain prior intent blocks a newly prepared key for same project instead of blind duplicate job',async()=>{
    const first=fixture(),e=environment(first,true);await assert.rejects(e.flow.reviewAndSubmit(first,{capabilities:{reference_refresh_contract:1}}));
    await assert.rejects(e.flow.reviewAndSubmit(fixture('6'),{capabilities:{reference_refresh_contract:1}}),/prior_outcome/);assert.equal(e.counts().mutationCalls,1);
    await assert.rejects(e.flow.reviewAndSubmit(first,{capabilities:{reference_refresh_contract:1}}),/already_tracked/);assert.equal(e.counts().mutationCalls,1);
});

test('typed accepted guard refusal remains failed and never authorizes requery or false completion',async()=>{
    const intent=fixture(),e=environment(intent);const saved=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}}),response=receipt(intent);(response.job as Value).state='failed';(response.job as Value).outcome={state:'failed',code:'refresh_scope_changed_review_required'};
    const selection=intent.reference_refresh.configure_project.request.configuration.selection as Value;response.result={schema:'airk.reference-refresh-review-required/1',selection_digest:refreshSelectionIdentity(selection),original_query_digest:selection.original_query_digest,review:{reasons:['pending_recovery']},publication_applied:false,requery_executed:false};e.setResponse(response);await e.flow.check(saved!);await assert.rejects(e.flow.requery(saved!),/fresh_completed/);assert.equal(e.counts().queryCalls,0);assert.equal(e.counts().mutationCalls,1);
});
test('typed pre-admission refusal binds full request/key and cannot erase historical accepted proof',async()=>{
    const intent=fixture(),e=environment(intent),saved=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});await e.flow.check(saved!);const original=structuredClone(e.journal.list()[0].value.nativeRefreshReceipt),request=intent.reference_refresh.configure_project;
    const refusal={status:'reference_refresh_review_required',project:request.project,caller:request.caller,key:request.key,request:request.request,review:{reasons:['unqualified_control_provenance']},future_review:'preserved'};
    await assert.rejects(e.journal.observeRefresh(saved!,refusal),/cannot_become_unaccepted/);assert.deepEqual(e.journal.list()[0].value.nativeRefreshReceipt,original);assert.deepEqual(e.journal.list()[0].value.nativeRefreshLookup,refusal);
});

test('actual flow export restores only retained in-memory original and never writes it to metadata-only journal',async()=>{
    const intent=fixture(),e=environment(intent);const id=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});await e.flow.check(id!);
    const saved=structuredClone(e.journal.list()[0].value),output=JSON.parse(e.flow.exportObservation(id!).json);
    assert.deepEqual(output.original_query,intent.reference_refresh.original_query);assert.deepEqual(output.original_historical_result,intent.reference_refresh.original_result);
    assert.deepEqual(output.last_known_historical_native_receipt,receipt(intent));assert.deepEqual(e.journal.list()[0].value,saved);
    const restarted=new ReferenceRefreshFlow(e.client,e.journal,{trusted:()=>true,confirm:async()=>true,reconfirm:async(value)=>value,query:async()=>{throw new Error('must not invent missing query');}});
    const recovered=JSON.parse(restarted.exportObservation(id!).json);assert.equal(recovered.original_query,null);assert.equal(recovered.original_historical_result,null);assert.deepEqual(recovered.last_known_historical_native_receipt,receipt(intent));
});

test('authority invalidated during durable journal write retains exact reservation but dispatches no mutation',{timeout:5000},async()=>{
    const intent=fixture(),e=environment(intent);let entered!:()=>void,release!:()=>void;
    const writing=new Promise<void>(resolve=>{entered=resolve;}),resume=new Promise<void>(resolve=>{release=resolve;}),persist=e.storage.update.bind(e.storage);
    e.storage.update=async(key:string,value:unknown)=>{entered();await resume;await persist(key,value);};
    const request=e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}});
    await writing;e.revoke();release();await assert.rejects(request,/workspace_not_trusted/);
    assert.deepEqual(e.counts(),{queryCalls:0,mutationCalls:0,readCalls:0});
    const saved=e.journal.list();assert.equal(saved.length,1);assert.equal(saved[0].value.reservedObservationBytes,REFRESH_JOURNAL_BYTES);
    const reserved=saved[0].value.intent as ReferenceRefreshIntent;assert.equal(reserved.reference_refresh.original_query,null);assert.equal(reserved.reference_refresh.original_result,null);
    await assert.rejects(e.journal.submit(e.client,saved[0].id),/receipt_reconciliation/);assert.equal(e.counts().mutationCalls,0);
    // Missing native acceptance remains an observation, never permission to replay.
    e.setResponse({status:'reference_refresh_receipt',job:null,configuration:null,result:null,expired:false});
    const reopened=new ReferenceRefreshFlow(e.client,e.journal,{...e.host,trusted:()=>true});await reopened.check(saved[0].id);
    assert.equal(e.counts().readCalls,1);assert.equal(e.counts().mutationCalls,0);assert.equal(e.journal.list().length,1);
});


test('native selection identity matches Core typed JSON/framing golden and ignores caller insertion order',()=>{
    const canonical="{\"schema\":1,\"caller\":\"01010101010101010101010101010101\",\"key\":{\"namespace\":\"01010101010101010101010101010101\",\"epoch\":1,\"nonce\":\"01010101010101010101010101010101\"},\"original_result_digest\":\"0202020202020202020202020202020202020202020202020202020202020202\",\"original_evidence_selection_digest\":\"0202020202020202020202020202020202020202020202020202020202020202\",\"original_query_digest\":\"0202020202020202020202020202020202020202020202020202020202020202\",\"selection\":{\"schema\":1,\"context_key\":\"0202020202020202020202020202020202020202020202020202020202020202\",\"namespace\":\"01010101010101010101010101010101\",\"expected_catalog_revision\":1,\"grant_id\":\"grt_01010101010101010101010101010101\",\"expected_grant_revision\":1,\"project_ids\":[\"prj_01010101010101010101010101010101\"],\"index_ids\":[\"idx_01010101010101010101010101010101\"]},\"mode\":\"selected_only\",\"members\":[{\"index_id\":\"idx_01010101010101010101010101010101\",\"original_publication\":\"airk-r1:01010101010101010101010101010101:1:0202020202020202020202020202020202020202020202020202020202020202\",\"sources\":[{\"source_root_id\":\"src_01010101010101010101010101010101\",\"source_relative_path\":\"api/é-政策😀.md\",\"canonical_root\":\"//owned/source/é\",\"indexed_hash\":\"0202020202020202020202020202020202020202020202020202020202020202\"}]}]}";
    const expected="4630540e46f495609e14aec592677c465636aa0f32edd24c2b0b8f9f7a220298";
    const selection=JSON.parse(canonical) as Value;
    assert.equal(refreshSelectionCanonicalJson(selection),canonical);
    assert.equal(refreshSelectionIdentity(selection),expected);
    const separated=JSON.parse(canonical) as Value;(separated.selection as Value).namespace='03'.repeat(16);
    assert.notEqual((separated.key as Value).namespace,(separated.selection as Value).namespace);
    assert.equal(refreshSelectionIdentity(separated),'a3610bdf29b2117a4ac43d8fc6838b7dad405c5b4dc36a56e206f628fb6591d4');
    const initial=structuredClone(separated);(initial.key as Value).epoch=0;
    assert.equal(refreshSelectionIdentity(initial),'be7b6287f8cdacaee862bc8be0746b05502630b49cf2bd523b5cbf2e5ccede35');
    const reverse=(value:unknown):unknown=>Array.isArray(value)?value.map(reverse):value!==null&&typeof value==='object'?Object.fromEntries(Object.entries(value as Value).reverse().map(([k,v])=>[k,reverse(v)])):value;
    assert.equal(refreshSelectionIdentity(reverse(selection)),expected);
    assert.throws(()=>refreshSelectionIdentity({...selection,future_request_field:true}),/fields/);
    const source=((selection.members as Value[])[0].sources as Value[])[0];source.future_request_field=true;
    assert.throws(()=>refreshSelectionIdentity(selection),/fields/);
    for(const relative of ['api/\ud800.md','api/\u0085.md']){
        const invalid=JSON.parse(canonical) as Value;((invalid.members as Value[])[0].sources as Value[])[0].source_relative_path=relative;
        assert.throws(()=>refreshSelectionIdentity(invalid),/native_string/);
    }
});

test('success and typed refusal reject crossed source identity and arbitrary digest despite exact job/query/publications',()=>{
    const intent=fixture(),selection=intent.reference_refresh.configure_project.request.configuration.selection as Value;
    const crossed=structuredClone(selection);((crossed.members as Value[])[0].sources as Value[])[0].source_relative_path='other.txt';
    crossed.original_evidence_selection_digest=refreshDigest(crossed.members);
    assert.notEqual(refreshSelectionIdentity(crossed),refreshSelectionIdentity(selection));
    for(const schema of ['airk.reference-refresh-result/1','airk.reference-refresh-review-required/1']){
        const native=receipt(intent);
        if(schema==='airk.reference-refresh-review-required/1'){
            (native.job as Value).state='failed';(native.job as Value).outcome={state:'failed',code:'refresh_scope_changed_review_required'};
            native.result={schema,selection_digest:refreshSelectionIdentity(selection),original_query_digest:selection.original_query_digest,review:{reasons:['pending_recovery']},publication_applied:false,requery_executed:false,future_refusal:{complete:true}};
        }
        assert.deepEqual(validateRefreshReceipt(native,intent),native);
        const complete=structuredClone(native);
        for(const digest of [refreshSelectionIdentity(crossed),hash('0')]){
            const bad=structuredClone(native);(bad.result as Value).selection_digest=digest;
            assert.throws(()=>validateRefreshReceipt(bad,intent),/crossed_refresh_selection_digest/);
            assert.deepEqual(native,complete,'validation cannot mutate complete native proof');
        }
    }
});


test('typed control-input and control-context drift remain failed review-required observations without requery',async()=>{
    for(const reason of ['control_inputs_changed','control_context_changed']){
        const intent=fixture(),e=environment(intent),saved=await e.flow.reviewAndSubmit(intent,{capabilities:{reference_refresh_contract:1}}),native=receipt(intent);
        const selected=intent.reference_refresh.configure_project.request.configuration.selection as Value;
        (native.job as Value).state='failed';(native.job as Value).outcome={state:'failed',code:'refresh_scope_changed_review_required'};
        native.result={schema:'airk.reference-refresh-review-required/1',selection_digest:refreshSelectionIdentity(selected),original_query_digest:selected.original_query_digest,review:{reasons:[reason]},publication_applied:false,requery_executed:false,future_native_refusal:{complete:true}};
        e.setResponse(native);await e.flow.check(saved!);
        assert.deepEqual(e.journal.list()[0].value.nativeRefreshReceipt,native);
        await assert.rejects(e.flow.requery(saved!),/fresh_completed/);
        assert.deepEqual(e.counts(),{mutationCalls:1,readCalls:2,queryCalls:0});
        const unknown=structuredClone(native);((unknown.result as Value).review as Value).reasons=[reason+'_unknown'];
        assert.throws(()=>validateRefreshReceipt(unknown,intent),/review_reason/);
        const all=structuredClone(native);((all.result as Value).review as Value).reasons=['no_current_index','pending_recovery','configuration_changed','extractor_changed','incomplete_scan','directory_selection','control_input','dependency_input','unqualified_control_provenance','control_inputs_changed','control_context_changed'];
        assert.deepEqual(validateRefreshReceipt(all,intent),all);
        const duplicate=structuredClone(all);(((duplicate.result as Value).review as Value).reasons as string[]).push(reason);
        assert.throws(()=>validateRefreshReceipt(duplicate,intent),/selection|review_reason/);
    }
});


test('refresh requery checks its existing read-only tool contract without string coercion or broader Workbench tools',()=>{
    assert.equal(refreshQueryTool('airk_search'),'airk_search');
    assert.equal(refreshQueryTool('airk_file_info'),'airk_file_info');
    for(const input of ['configure_project','airk_parse','airk_status','airk_search ',{toString:()=> 'airk_search'},null])assert.throws(()=>refreshQueryTool(input),/invalid_refresh_original_query/);
});


test('request epoch and catalog scope namespaces remain independent while each is validated and bound',()=>{
    const intent=fixture(),request=intent.reference_refresh.configure_project,selection=request.request.configuration.selection as Value,scope=selection.selection as Value;
    assert.notEqual(request.key.namespace,scope.namespace);
    assert.deepEqual(validateRefreshIntent(intent),intent);
    const original=refreshSelectionIdentity(selection);
    const changedCatalog=structuredClone(selection);(changedCatalog.selection as Value).namespace=id('a');
    const changedRequest=structuredClone(selection);(changedRequest.key as Value).namespace=id('b');
    assert.notEqual(refreshSelectionIdentity(changedCatalog),original);
    assert.notEqual(refreshSelectionIdentity(changedRequest),original);
    assert.notEqual(refreshSelectionIdentity(changedRequest),refreshSelectionIdentity(changedCatalog));
    const crossedKey=structuredClone(intent);crossedKey.reference_refresh.configure_project.key={...request.key,namespace:id('a')};
    assert.throws(()=>validateRefreshIntent(crossedKey),/request_key/);
    for(const field of ['namespace','nonce','epoch']){
        const bad=structuredClone(selection);(bad.key as Value)[field]=field==='epoch'?-1:'invalid';
        assert.throws(()=>refreshSelectionIdentity(bad));
    }
    const badCatalog=structuredClone(selection);(badCatalog.selection as Value).namespace='invalid';
    assert.throws(()=>refreshSelectionIdentity(badCatalog));
});


test('request epoch zero is canonical and independent of positive catalog revisions',()=>{
    const intent=fixture(),request=intent.reference_refresh.configure_project,selection=request.request.configuration.selection as Value;
    assert.equal(request.key.epoch,0);assert.notEqual(request.key.namespace,(selection.selection as Value).namespace);
    assert.deepEqual(validateRefreshIntent(intent),intent);
    const zero=refreshSelectionIdentity(selection);
    for(const epoch of [1,Number.MAX_SAFE_INTEGER]){
        const valid=structuredClone(selection);(valid.key as Value).epoch=epoch;
        assert.notEqual(refreshSelectionIdentity(valid),zero);
    }
    for(const epoch of [-1,'0','1',0.5,Number.MAX_SAFE_INTEGER+1,NaN,Infinity,null]){
        const bad=structuredClone(selection);(bad.key as Value).epoch=epoch;
        assert.throws(()=>refreshSelectionIdentity(bad),/request_epoch/);
    }
    for(const field of ['expected_catalog_revision','expected_grant_revision']){
        const bad=structuredClone(selection);(bad.selection as Value)[field]=0;
        assert.throws(()=>refreshSelectionIdentity(bad),/catalog_revision/);
    }
});
