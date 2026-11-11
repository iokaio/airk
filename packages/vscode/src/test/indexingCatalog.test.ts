// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { CatalogApplyIntent, CatalogObject, CatalogRequest, CatalogReview, WindowContextDescriptor, CATALOG_UI_OPERATIONS, catalogExport, catalogItems, catalogMembershipDraft, validateCatalogScope, validateCatalogApply, validateCatalogOperation, validateNativeHostContext } from '../indexingCatalog';
import { CatalogControl, IndexingCatalogController } from '../indexingCatalogController';
import { IntentJournal, IntentStorage } from '../serviceIntents';
import { ServiceClient, ServiceError } from '../serviceClient';
import { parseWebviewMessage } from '../explorerContracts';
import { parseRoute } from '../explorerRoutes';

const namespace='ab'.repeat(16), caller='cd'.repeat(16), nonce='ef'.repeat(16), digest='12'.repeat(32), project='prj_'+'34'.repeat(16), index='idx_'+'56'.repeat(16), source='src_'+'78'.repeat(16);
const requestDigest='13'.repeat(32);
const projectValue={id:project,name:'Project <safe>',definition_revision:1,scope_revision:1,defaults_revision:1,default_indexes:[]};
const operation={operation:'rename_project',id:project,expected_definition_revision:1,name:'Renamed'};
const request:CatalogRequest={caller,key:{namespace,epoch:1,nonce},expected_catalog_revision:1,operation};
const intent:CatalogApplyIntent={catalog_apply:{request,preview_digest:digest},request_digest:requestDigest};
const review:CatalogReview={request,response:{status:'catalog_preview',preview:{request_digest:requestDigest,catalog_revision:1,next_revision:2,operation,before:[],after:[],authority_fence_required:false,file_data_changes:false},preview_digest:digest},previewDigest:digest,requestDigest};
const page=(extra:CatalogObject={})=>({status:'catalog_page',page:{namespace,revision:1,items:[{kind:'project',value:projectValue}],continuation:null,...extra}});
function storage(){
    const values=new Map<string,unknown>();
    const adapter:IntentStorage={keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}};
    return {values,adapter};
}
function controller(initialDescriptor?:WindowContextDescriptor){
    let trusted=true,identity='owner';const calls:CatalogObject[]=[];
    let descriptor=initialDescriptor;
    const control:CatalogControl={catalogRead:async method=>{calls.push(method);return 'catalog_detail' in method?{status:'catalog_detail',detail:{namespace,revision:1,item:{kind:'project',value:projectValue}}}:page();},
        catalogPrepare:async()=>structuredClone(review),catalogApply:async()=>({status:'catalog_applied',receipt:{full:'receipt'}}),catalogReconcile:async()=>[],catalogPending:()=>[]};
    const value=new IndexingCatalogController({control,identity:()=>identity,authorized:()=>trusted,windowContext:()=>descriptor,changed(){}});
    return {value,control,calls,trust:(next:boolean)=>{trusted=next;},replace:()=>{identity+='next';},descriptor:(next:WindowContextDescriptor|undefined)=>{descriptor=next;}};
}
test('catalog UI preserves complete native pages, details, preview and exports',async()=>{
    const f=controller();const rich=page({future_receipt:{coverage:['all native fields'],readiness:'retained'},continuation:{kind:'projects',namespace,revision:1,after:project,index_id:null,project_id:null}});
    f.control.catalogRead=async method=>{f.calls.push(method);return 'catalog_detail' in method?{status:'catalog_detail',detail:{namespace,revision:1,item:{kind:'project',value:{...projectValue,native_provenance:{kept:true}}},additional_native_field:'retained'}}:rich;};
    await f.value.page('projects');
    assert.deepEqual(f.value.snapshot().page,rich);
    await f.value.page('projects',true);
    assert.deepEqual((f.calls[1].catalog_page as CatalogObject).request,{kind:'projects',index_id:null,project_id:null,after:(rich.page as CatalogObject).continuation,expected_catalog_revision:1,limit:16,max_bytes:49152});
    await f.value.detail('project',project);
    const generation=f.value.snapshot().generation;
    await f.value.preview(operation,generation);
    const exported=JSON.parse(catalogExport(f.value.snapshot()));
    assert.deepEqual(exported.page,rich);assert.deepEqual(exported.review.response,review.response);
    assert.deepEqual(exported.detail.detail.item.value.native_provenance,{kept:true});
    await assert.rejects(f.value.apply(generation-1),/stale_catalog_review/);
    assert.equal(f.value.snapshot().review,undefined);
});
test('stale catalog trust, client, namespace, revisions and continuation filters refuse late work',async()=>{
    for(const fence of ['trust','client'] as const){
        const f=controller();let release!:(value:CatalogObject)=>void;
        f.control.catalogRead=()=>new Promise(resolve=>{release=resolve;});
        const pending=f.value.page('projects');
        if(fence==='trust')f.trust(false);else f.replace();
        release(page());await assert.rejects(pending,/stale_catalog_view/);
        assert.equal(f.value.snapshot().page,undefined);
    }
    const f=controller();await f.value.page('projects');
    f.control.catalogRead=async()=>page({namespace:'99'.repeat(16)});
    await assert.rejects(f.value.page('projects'),/namespace_changed/);assert.equal(f.value.snapshot().page,undefined);
    const g=controller();await g.value.page('projects');
    g.control.catalogRead=async()=>({status:'catalog_detail',detail:{namespace,revision:2,item:null}});
    await assert.rejects(g.value.detail('project',project),/revision_changed/);
    const h=controller();h.control.catalogRead=async()=>page({continuation:{kind:'projects',namespace,revision:1,after:project,index_id:index,project_id:null}});
    await assert.rejects(h.value.page('projects'),/crossed_catalog_continuation/);
});
test('uncertain apply consumes the review and permits only receipt reconciliation',async()=>{
    const f=controller();let applies=0,receipts=0;
    f.control.catalogApply=async()=>{applies++;throw new ServiceError('console_transport_closed',true);};
    f.control.catalogReconcile=async()=>{receipts++;return [{status:'catalog_receipt',receipt:null}];};
    await f.value.page('projects');const generation=f.value.snapshot().generation;await f.value.preview(operation,generation);
    await assert.rejects(f.value.apply(generation),/console_transport_closed/);
    await assert.rejects(f.value.apply(generation),/stale_catalog_review/);
    await f.value.reconcile();assert.equal(applies,1);assert.equal(receipts,1);
    assert.deepEqual(f.value.snapshot().outcome,{receipts:[{status:'catalog_receipt',receipt:null}]});
});
test('shared journal persists the exact request and native digest and never replays catalog applies',async()=>{
    const s=storage(),journal=new IntentJournal(s.adapter,'/control',namespace);await journal.prepare('Rename Project',intent);
    assert.deepEqual(journal.list()[0].value.intent,intent);
    let dispatches=0;const client=new ServiceClient({dispose(){},async request(){dispatches++;return {status:'catalog_receipt',receipt:null};}});
    await assert.rejects(journal.submit(client,nonce),/catalog_apply_requires_receipt_reconciliation/);
    await journal.reconcile(client,async()=>{});
    assert.equal(dispatches,0,'routine job reconciliation cannot discard catalog outcomes');
    assert.equal(journal.list().length,1);
    assert.equal(JSON.stringify(journal.list()).includes('before'),false,'do not duplicate the native preview into the journal');
    assert.throws(()=>validateCatalogApply({catalog_apply:{...intent.catalog_apply,preview:{}}}),/fields/);
});
test('native applied receipt is complete, scoped and bounded through the actual ServiceClient adapter',async()=>{
    let submitted:unknown;const receipt={schema:1,operation_id:'90'.repeat(16),caller,key:request.key,request_digest:requestDigest,catalog_revision:2,authority_fence_required:false,extra_native_field:{kept:true}};
    const client=new ServiceClient({dispose(){},async request(method){submitted=method;return {status:'catalog_applied',receipt};}});
    assert.deepEqual(await client.submitCatalog(intent),{status:'catalog_applied',receipt});assert.deepEqual(submitted,{catalog_apply:intent.catalog_apply});
    const crossed=new ServiceClient({dispose(){},async request(){return {status:'catalog_applied',receipt:{...receipt,key:{...request.key,nonce:caller}}};}});
    await assert.rejects(crossed.submitCatalog(intent),/crossed_catalog_receipt/);
    const digestMismatch=new ServiceClient({dispose(){},async request(){return {status:'catalog_applied',receipt:{...receipt,request_digest:'99'.repeat(32)}};}});
    await assert.rejects(digestMismatch.submitCatalog(intent),/crossed_catalog_receipt/);
});
test('scope editor preserves all native atoms, rejects duplicates and matches Rust UTF-8/control boundaries',()=>{
    const atoms=[{source_root_id:source,kind:'subtree' as const,path:''},{source_root_id:'src_'+'91'.repeat(16),kind:'exact' as const,path:'a\n\u0085b'}];
    const draft=catalogMembershipDraft({index_id:index,revision:7,scope:atoms});
    assert.deepEqual(draft,{index,revision:'7',scope:atoms});assert.notEqual(draft.scope,atoms);assert.notEqual(draft.scope[0],atoms[0]);
    assert.deepEqual(validateCatalogScope(atoms),atoms);
    assert.throws(()=>validateCatalogScope([...atoms,{...atoms[0]}]),/duplicate_membership_scope/);
    assert.doesNotThrow(()=>validateCatalogScope([{source_root_id:source,kind:'exact',path:'é'.repeat(2048)}]));
    assert.throws(()=>validateCatalogScope([{source_root_id:source,kind:'exact',path:'é'.repeat(2049)}]),/invalid_membership_scope/);
    assert.doesNotThrow(()=>validateCatalogScope(Array.from({length:128},(_,i)=>({source_root_id:source,kind:'exact',path:String(i)}))));
    assert.throws(()=>validateCatalogScope(Array.from({length:129},(_,i)=>({source_root_id:source,kind:'exact',path:String(i)}))),/invalid_membership_scope/);
    for(const path of ['a\\b','a:b','a\0b','a//b','/a','a/','a/../b','./a'])assert.throws(()=>validateCatalogScope([{source_root_id:source,kind:'exact',path}]),/invalid_membership_scope/);
    for(const name of ['a\u0080b','a\u009fb','a\u007fb','a\nb'])assert.throws(()=>validateCatalogOperation({...operation,name}),/invalid_catalog_name/);
    assert.doesNotThrow(()=>validateCatalogOperation({...operation,name:'a\u2028b'}));
    assert.doesNotThrow(()=>validateCatalogOperation({...operation,name:'\ufeff'}),'Rust White_Space excludes the ECMAScript BOM');
});
test('catalog journal admits native-size requests within the shared 2 MiB and 64-record bounds',async()=>{
    const s=storage(),journal=new IntentJournal(s.adapter,'/control',namespace);
    const largeOperation={operation:'set_membership',expected_scope_revision:1,expected_membership_revision:null,membership:{project_id:project,index_id:index,revision:1,
        scope:Array.from({length:128},(_,i)=>({source_root_id:source,kind:'exact',path:`${'x'.repeat(240)}${i}`}))}};
    const large:CatalogApplyIntent={catalog_apply:{request:{...request,operation:largeOperation},preview_digest:digest},request_digest:digest};
    assert.ok(Buffer.byteLength(JSON.stringify(large))>32768);
    validateCatalogApply(large);
    let admitted=0;
    for(let i=1;i<=64;i++){
        const next=structuredClone(large);next.catalog_apply.request.key.nonce=i.toString(16).padStart(32,'0');
        try{await journal.prepare('Reviewed membership',next);admitted++;}catch(error){assert.match(String(error),/local_intent_byte_capacity_exceeded/);break;}
    }
    assert.ok(admitted>0&&admitted<64);
    assert.ok([...s.values.values()].reduce<number>((total,value)=>total+Buffer.byteLength(JSON.stringify(value)),0)<=2*1024*1024);
    const smallStorage=storage(),smallJournal=new IntentJournal(smallStorage.adapter,'/control',namespace);
    for(let i=1;i<=64;i++){const next=structuredClone(intent);next.catalog_apply.request.key.nonce=i.toString(16).padStart(32,'0');await smallJournal.prepare('Rename',next);}
    const over=structuredClone(intent);over.catalog_apply.request.key.nonce='ff'.repeat(16);
    await assert.rejects(smallJournal.prepare('Rename',over),/local_intent_capacity_exceeded/);
});
test('UI operation subset and serialized field catalogs agree with native Core contracts',()=>{
    const core=readFileSync(resolve(__dirname,'../../../..','crates/airk-core/src/catalog_management.rs'),'utf8');
    const snake=(name:string)=>name.replace(/[A-Z]/g,(letter,at)=>`${at?'_':''}${letter.toLowerCase()}`);
    const variants=[...core.slice(core.indexOf('pub enum CatalogOperation'),core.indexOf('pub struct CatalogRequest')).matchAll(/^    ([A-Z][A-Za-z]+) \{/gm)].map(match=>snake(match[1]));
    assert.ok(CATALOG_UI_OPERATIONS.every(value=>variants.includes(value)));
    const supported=[
        {operation:'create_project',id:project,name:'New'},operation,
        {operation:'delete_project',id:project,expected_definition_revision:1,expected_scope_revision:1,expected_defaults_revision:1},
        {operation:'rename_index',id:index,expected_definition_revision:1,name:'New'},
        {operation:'set_membership',expected_scope_revision:1,expected_membership_revision:null,membership:{project_id:project,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:''}]}},
        {operation:'remove_membership',project_id:project,index_id:index,expected_scope_revision:1,expected_membership_revision:1},
        {operation:'set_project_defaults',project_id:project,expected_defaults_revision:1,index_ids:[index]},
        {operation:'set_context',expected_revision:null,context:{id:'ctx_'+'11'.repeat(16),host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Empty window',revision:1,project_ids:[],index_ids:[]}},
        {operation:'delete_context',id:'ctx_'+'11'.repeat(16),expected_revision:1},
    ];
    assert.deepEqual(supported.map(value=>value.operation).sort(),[...CATALOG_UI_OPERATIONS].sort());
    for(const value of supported){
        assert.deepEqual(validateCatalogOperation(value),value);assert.throws(()=>validateCatalogOperation({...value,unexpected:true}),/fields/);
        const nativeName=value.operation.split('_').map(part=>part[0].toUpperCase()+part.slice(1)).join('');
        const block=new RegExp(`    ${nativeName} \\{([\\s\\S]*?)^    \\}`, 'm').exec(core);
        assert.ok(block,nativeName);
        const nativeFields=[...block![1].matchAll(/^        ([a-z_]+):/gm)].map(match=>match[1]).sort();
        assert.deepEqual(Object.keys(value).filter(key=>key!=='operation').sort(),nativeFields,`complete ${nativeName} field catalog`);
    }
    assert.throws(()=>validateCatalogOperation({...operation,id:index}),/invalid_catalog_id/);
    assert.throws(()=>validateCatalogOperation({...operation,name:'é'.repeat(129)}),/invalid_catalog_name/);
    assert.throws(()=>validateCatalogOperation({...operation,expected_definition_revision:0}),/revision/);
    const service=readFileSync(resolve(__dirname,'../../../..','crates/airk-core/src/service.rs'),'utf8');
    for(const variant of ['CatalogPage','CatalogDetail','CatalogContext','CatalogPreview','CatalogApply','CatalogReceipt'])assert.match(service,new RegExp(`\\b${variant} \\{`));
    assert.deepEqual(parseRoute('#indexing/library'),{workspace:'indexing',tab:'library'});
    assert.deepEqual(parseRoute('#manage/projects'),{workspace:'manage',tab:'overview'});
    assert.ok(parseWebviewMessage({v:1,type:'indexing.page',requestId:'x',kind:'sources',next:false,index}));
    assert.equal(parseWebviewMessage({v:1,type:'indexing.page',requestId:'x',kind:'sources',next:false,index:project}),undefined);
    const contextKinds=['folder','workspace','empty','untitled','legacy_pending'];
    const nativeCatalog=readFileSync(resolve(__dirname,'../../../..','crates/airk-core/src/index_catalog.rs'),'utf8');
    const kindBlock=/pub enum ContextKind \{([\s\S]*?)^\}/m.exec(nativeCatalog);assert.ok(kindBlock);
    assert.deepEqual([...kindBlock![1].matchAll(/^    ([A-Z][A-Za-z]+),/gm)].map(match=>snake(match[1])),contextKinds);
    for(const kind of contextKinds){
        assert.doesNotThrow(()=>validateCatalogOperation({operation:'set_context',expected_revision:null,context:{id:'ctx_'+'11'.repeat(16),host:'22'.repeat(32),key:'33'.repeat(32),kind,name:'Context',revision:1,project_ids:[],index_ids:[]}}));
    }
    assert.throws(()=>validateCatalogOperation({operation:'set_context',expected_revision:null,context:{id:'ctx_'+'11'.repeat(16),host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Context',revision:2,project_ids:[],index_ids:[]}}),/next_context_revision/);
});

test('window associations use only the injected host identity, explicit reads and reviewed zero-Project CRUD',async()=>{
    const descriptor:WindowContextDescriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Empty window'};
    const f=controller(descriptor);let context:CatalogObject|null=null,revision=1;const previews:CatalogObject[]=[];
    f.control.catalogRead=async method=>{f.calls.push(method);return {status:'catalog_detail',detail:{namespace,revision,item:context?{kind:'context',value:context}:null,native_detail_field:{preserved:true}}};};
    f.control.catalogPrepare=async(operation,expected)=>{previews.push(operation);return {...review,request:{...request,expected_catalog_revision:expected,operation},response:{...review.response,preview:{...review.response.preview as CatalogObject,catalog_revision:expected,operation}}};};
    f.control.catalogApply=async received=>{context={...catalogObjectForTest(received.request.operation.context),future_native_context_field:{retained:true}};revision++;return {status:'catalog_applied',receipt:{request_digest:received.requestDigest}};};
    assert.deepEqual(f.value.snapshot().windowDescriptor,descriptor);assert.equal(f.calls.length,0,'opening the screen cannot infer or register an association');
    await f.value.readContext();assert.deepEqual(f.calls,[{catalog_context:{host:descriptor.host,key:descriptor.key,expected_catalog_revision:null}}]);
    let generation=f.value.snapshot().generation;await f.value.previewContext({project_ids:[],index_ids:[]},generation);
    const created=previews[0].context as CatalogObject;
    assert.match(String(created.id),/^ctx_[0-9a-f]{32}$/);assert.equal(created.host,descriptor.host);assert.equal(created.key,descriptor.key);
    assert.deepEqual(created.project_ids,[]);assert.deepEqual(created.index_ids,[]);assert.equal(created.revision,1);
    await f.value.apply(generation);await f.value.readContext();
    assert.equal(((f.value.snapshot().context!.detail as CatalogObject).native_detail_field as CatalogObject).preserved,true);
    assert.match(catalogExport(f.value.snapshot()),/future_native_context_field/);
    generation=f.value.snapshot().generation;await f.value.previewContext({project_ids:[project],index_ids:[index]},generation);
    const updated=previews.at(-1)!.context as CatalogObject;assert.equal(updated.id,created.id);assert.equal(updated.revision,2);
    await f.value.previewContext({project_ids:[],index_ids:[]},generation,true);
    assert.deepEqual(previews.at(-1),{operation:'delete_context',id:created.id,expected_revision:1});
    const before=JSON.stringify(f.value.snapshot().context);
    await assert.rejects(f.value.previewContext({project_ids:[],index_ids:[index]},generation),/invalid_context_selection/);
    assert.equal(JSON.stringify(f.value.snapshot().context),before);
    await assert.rejects(f.value.preview({operation:'set_context',expected_revision:null,context:created},generation),/requires_host_review/);
    assert.equal(parseWebviewMessage({v:1,type:'indexing.context.read',requestId:'read',host:descriptor.host}),undefined);
    assert.equal(parseWebviewMessage({v:1,type:'indexing.context.preview',requestId:'review',generation,selection:{project_ids:[],index_ids:[]},key:descriptor.key}),undefined);
});
function catalogObjectForTest(input:unknown):CatalogObject {assert.ok(input&&typeof input==='object'&&!Array.isArray(input));return input as CatalogObject;}

test('unavailable or replaced window descriptors refuse association work and delayed reviews',async()=>{
    const unavailable=controller();await assert.rejects(unavailable.value.readContext(),/descriptor_unavailable/);assert.deepEqual(unavailable.calls,[]);
    const descriptor:WindowContextDescriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'workspace',name:'Reviewed workspace'};
    const f=controller(descriptor);f.control.catalogRead=async()=>({status:'catalog_detail',detail:{namespace,revision:1,item:null}});
    await f.value.readContext();let release!:(value:CatalogReview)=>void;
    f.control.catalogPrepare=()=>new Promise(resolve=>{release=resolve;});
    const pending=f.value.previewContext({project_ids:[],index_ids:[]},f.value.snapshot().generation);
    f.descriptor({...descriptor,key:'44'.repeat(32)});f.value.snapshot();release(review);
    await assert.rejects(pending,/stale_catalog_view/);assert.equal(f.value.snapshot().context,undefined);assert.equal(f.value.snapshot().review,undefined);
});

test('context operations share exact durable intents and native receipt-only recovery',async()=>{
    const s=storage(),journal=new IntentJournal(s.adapter,'/control',namespace);
    const operation={operation:'set_context',expected_revision:null,context:{id:'ctx_'+'11'.repeat(16),host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Empty window',revision:1,project_ids:[],index_ids:[]}};
    const prepared:CatalogApplyIntent={...intent,catalog_apply:{...intent.catalog_apply,request:{...request,operation}}};
    await journal.prepare('Save association',prepared);assert.deepEqual(journal.list()[0].value.intent,prepared);
    const restored=new IntentJournal(s.adapter,'/control',namespace);assert.deepEqual(restored.list()[0].value.intent,prepared);
    let sent=0;const client=new ServiceClient({dispose(){},async request(){sent++;throw new Error('apply cannot replay');}});
    await assert.rejects(restored.submit(client,nonce),/requires_receipt_reconciliation/);assert.equal(sent,0);
});

test('native host context descriptors preserve complete results and enforce identity, schema and 16 KiB bounds',async()=>{
    const host='22'.repeat(32),key='33'.repeat(32),context={id:'ctx_'+'11'.repeat(16),host,key,kind:'empty',name:'Empty window',revision:2,project_ids:[],index_ids:[],future_context_field:{preserved:true}};
    const descriptor={schema:1,namespace,host,key,catalog_revision:3,context,future_descriptor_field:{full:['native','object']}};
    assert.equal(validateNativeHostContext(descriptor),descriptor);
    assert.doesNotThrow(()=>validateNativeHostContext({...descriptor,context:null}));
    for(const value of [{...descriptor,schema:2},{...descriptor,namespace:'bad'},{...descriptor,host:'bad'},{...descriptor,key:'bad'},
        {...descriptor,context:{...context,host:'44'.repeat(32)}},{...descriptor,context:{...context,key:'44'.repeat(32)}},
        {...descriptor,context:{...context,revision:4}},{...descriptor,catalog_revision:0},{...descriptor,future_descriptor_field:'x'.repeat(16384)}]){
        assert.throws(()=>validateNativeHostContext(value));
    }
    const response={status:'catalog_host_context',descriptor,extra_response_field:{preserved:true}},calls:unknown[]=[];
    const client=new ServiceClient({dispose(){},async request(method){calls.push(method);return calls.length===1?{status:'failure',code:'registry_busy'}:response;}});
    const complete=await client.catalogHostContext(key,3);assert.deepEqual(complete,response);
    assert.deepEqual(calls,[{catalog_host_context:{key,expected_catalog_revision:3}},{catalog_host_context:{key,expected_catalog_revision:3}}]);
    const copied=JSON.parse(catalogExport({generation:1,busy:false,outcome:complete}));assert.deepEqual(copied.outcome,response);
    for(const altered of [{...descriptor,key:'44'.repeat(32)},{...descriptor,catalog_revision:4}]){
        const invalid=new ServiceClient({dispose(){},async request(){return {status:'catalog_host_context',descriptor:{...altered,context:null}};}});
        await assert.rejects(invalid.catalogHostContext(key,3),/crossed_native_host_context/);
    }
});

test('grant page/detail copies preserve the native persisted wrapper and provenance without creating grants',()=>{
    for(const imported of [false,true]){
        const grant={id:'grt_'+(imported?'f1':'01').repeat(16),host:'22'.repeat(32),revision:1,revoked:false,memberships:[{
            membership:{project_id:project,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null,
        }],future_grant_field:{retained:true}};
        const provenance=imported?{kind:'imported_legacy_snapshot',upgrade_id:'11'.repeat(16),plan_digest:'22'.repeat(32),legacy_binding:'33'.repeat(32),configuration_digest:'44'.repeat(32),future_provenance_field:{retained:true}}
            :{kind:'user_reviewed_project_grant',future_provenance_field:{retained:true}};
        const value={grant,provenance,future_wrapper_field:{retained:true}},item={kind:'grant',value};
        const nativePage={status:'catalog_page',page:{namespace,revision:1,items:[item],continuation:null}};
        const detail={status:'catalog_detail',detail:{namespace,revision:1,item}};
        assert.equal(catalogItems(nativePage)[0].value,value);assert.equal(catalogItems(detail,'detail')[0].value,value);
        const copied=JSON.parse(catalogExport({generation:1,busy:false,page:nativePage,detail}));
        assert.deepEqual(copied.page,nativePage);assert.deepEqual(copied.detail,detail);
        assert.throws(()=>validateCatalogOperation({operation:'create_grant',grant}),/unsupported_catalog_ui_operation/);
    }
});
