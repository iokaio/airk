// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs'),path=require('node:path');
const {formFillExpressions,checkboxFillExpressions,appliedReviewMatches,observeReviewedApply,privateEditorEnvironment,privateClientEnvironment,openUsageAdministration,retainUiDiagnostic,beginUiDiagnostic,editorActionDeadline,editorRemainingBudget}=require('../editor-ui.cjs');
const label='New index storage folder';
const {clickDispatchExpression,retryUnclicked}=require('../editor-ui.cjs');
const {clearedSearchScope}=require('../editor-ui.cjs');

test('cleared scope accepts the canonical active empty choice and rejects retained authority',()=>{
    const value={outcome:{status:'search_scope_cleared'},searchScope:{active:true,selectedIndexIds:[],selectedProjectIds:[]}};
    assert.equal(clearedSearchScope(value),true);
    assert.equal(clearedSearchScope({...value,searchScope:{...value.searchScope,selectedIndexIds:['idx_retained']}}),false);
    assert.equal(clearedSearchScope({...value,searchScope:{...value.searchScope,selectedProjectIds:['prj_retained']}}),false);
    assert.equal(clearedSearchScope({...value,searchScope:{active:false}}),false);
    assert.equal(clearedSearchScope({...value,outcome:{status:'search_scope_selected'}}),false);
});
test('typed click refuses synchronously before click and captures only bounded control metadata',()=>{
    let clicks=0;const button={disabled:true,click(){clicks++;}},document={visibilityState:'visible',querySelectorAll:()=>[button]};
    const expression=clickDispatchExpression('button');
    const refused=vm.runInNewContext(expression,{button,document});
    assert.equal(refused.dispatched,false);assert.equal(refused.reason,'target_unavailable');assert.equal(clicks,0);
    assert.deepEqual(JSON.parse(JSON.stringify(refused.diagnostic)),{visibility:'visible',button_count:1,target_present:true,target_disabled:true});
    button.disabled=false;assert.equal(vm.runInNewContext(expression,{button,document}).dispatched,true);assert.equal(clicks,1);
});
test('known no-click retries rediscover readiness with increasing waits and dispatch one click',async()=>{
    let time=0,reads=0,attempts=0,clicks=0;const waits=[];
    const result=await retryUnclicked(async()=>++reads,async target=>{assert.equal(target,++attempts);if(attempts<3)return {dispatched:false,reason:'target_unavailable'};clicks++;return {dispatched:true};},
        {deadline:10000,now:()=>time,wallNow:()=>time,pause:async ms=>{waits.push(ms);time+=ms;}});
    assert.deepEqual(result,{dispatched:true});assert.equal(clicks,1);assert.equal(reads,3);assert.deepEqual(waits,[100,200]);
});
test('known no-click exhaustion has seven attempts and cannot dispatch after its original deadline',async()=>{
    let time=0,attempts=0;const waits=[],refused={dispatched:false,reason:'target_unavailable'};
    const options={deadline:10000,now:()=>time,wallNow:()=>time,pause:async ms=>{waits.push(ms);time+=ms;}};
    await assert.rejects(retryUnclicked(async()=>true,async()=>{attempts++;return refused;},options),/budget exhausted/);
    assert.equal(attempts,7);assert.deepEqual(waits,[100,200,400,600,700,800]);assert.equal(time,2800);
    time=0;attempts=0;let reads=0;
    await assert.rejects(retryUnclicked(async()=>{if(++reads===2)time=3000;return true;},async()=>{attempts++;return refused;},options),/deadline exhausted/);
    assert.equal(attempts,1);assert.equal(reads,2);
    time=10000;reads=0;
    await assert.rejects(retryUnclicked(async()=>{reads++;},async()=>{throw Error('must not dispatch');},options),/deadline exhausted/);assert.equal(reads,0);
});
test('unknown click values and CDP failures are never replayed',async()=>{
    for(const value of [undefined,null,false,true,{}, {dispatched:false}, {dispatched:false,reason:'other'}, {dispatched:true,extra:true}, {dispatched:false,reason:'target_unavailable',extra:true}]){
        let attempts=0,reads=0;
        await assert.rejects(retryUnclicked(async()=>{reads++;},async()=>{attempts++;return value;},{deadline:10000,now:()=>0,wallNow:()=>0,pause:async()=>{throw Error('must not wait');}}),/Unknown UI dispatch outcome/);
        assert.equal(attempts,1);assert.equal(reads,1);
    }
    const failure=Error('CDP context disappeared');let attempts=0;
    await assert.rejects(retryUnclicked(async()=>true,async()=>{attempts++;throw failure;},{deadline:10000,now:()=>0,wallNow:()=>0}),error=>error===failure);assert.equal(attempts,1);
});
function acceptedApplyFixture(){
    // Public DTO shape from the actual Mac pending->applied worker/provider drain;
    // fictional identities only, with no retained source paths or private state.
    const service='a'.repeat(32),binding='b'.repeat(64),index='idx_'+'c'.repeat(32),worker={service,worker:'d'.repeat(32),project:binding,fencing_generation:1};
    const request={caller:'e'.repeat(32),key:{namespace:'f'.repeat(32),epoch:0,nonce:'1'.repeat(32)},expected_catalog_revision:4,operation:{operation:'change_index_policy',id:index}};
    const review={request,requestDigest:'2'.repeat(64),previewDigest:'3'.repeat(64)};
    const obligation={index_id:index,backing:binding,workers:[worker],providers:[{worker,history:{kind:'helpers',generations:['4'.repeat(32)]}}]};
    const plan={service,gate_generation:3,targets:[],worker_obligation:obligation};
    const intent={schema:1,request,operation_id:'5'.repeat(32),preview_digest:review.previewDigest,prepared_digest:'6'.repeat(64),stage:'fence_unknown',plan,confirmations:[],worker_confirmations:[]};
    const unknown={status:'catalog_outcome_unknown',code:'catalog_apply_outcome_unknown',caller:request.caller,key:request.key,request_digest:review.requestDigest,observation:{kind:'authority_fence',fence:{state:'pending',value:intent}},future_unknown:{retained:true}};
    const metadata={schema:1,operation_id:intent.operation_id,caller:request.caller,key:request.key,request_digest:review.requestDigest,catalog_revision:5,authority_fence_required:true};
    const full={request,metadata,preview_digest:review.previewDigest,prepared_digest:intent.prepared_digest,proof:{plan_digest:'7'.repeat(64),service,gate_generation:3,confirmations:[],worker_obligation:obligation,worker_confirmations:[{worker,method:'native_worker_reap'}]}};
    const initial={generation:11,namespace:request.key.namespace,busy:false,error:'AIRK service: catalog_apply_outcome_unknown',submittedReview:review,outcome:unknown};
    const pending={status:'catalog_receipt',receipt:null,fence:unknown.observation.fence};
    const terminal={status:'catalog_receipt',receipt:metadata,fence:{state:'applied',value:full},future_terminal:{retained:true}};
    const view=response=>({generation:12,namespace:request.key.namespace,busy:false,outcome:{receipts:[response]}});
    return {review,initial,pending,terminal,view};
}
test('Apply dispatches once and observes exact public receipts with canonical full worker/provider validators',async()=>{
    const f=acceptedApplyFixture(),retained=[],rows=[f.pending,f.terminal];let writes=0,reads=0,time=0;
    const actual=await observeReviewedApply(f.review,{apply:async()=>{writes++;},read:async()=>f.initial,record:value=>retained.push(structuredClone(value)),check:async()=>f.view(rows[reads++])},{deadline:5000,now:()=>time,pause:async ms=>{time+=ms;}});
    assert.equal(writes,1);assert.equal(reads,2);assert.deepEqual(actual.outcome,{receipts:[f.terminal]},'Actual receipt DTO is not synthesized as catalog_applied');
    assert.deepEqual(actual.applyObservation.initial,f.initial);assert.deepEqual(actual.applyObservation.observations,[f.initial.outcome,f.pending,f.terminal]);assert.deepEqual(retained[0],{initial:f.initial});
    assert.equal(actual.applyObservation.terminal.fence.value.proof.worker_confirmations[0].method,'native_worker_reap');
});
test('immediate Apply success retains the actual response and performs no receipt reads',async()=>{
    const f=acceptedApplyFixture(),value={namespace:f.review.request.key.namespace,submittedReview:f.review,outcome:{status:'catalog_applied',receipt:f.terminal.receipt,fenced_receipt:f.terminal.fence.value}};let writes=0,reads=0;
    const observed=await observeReviewedApply(f.review,{apply:async()=>{writes++;},read:async()=>value,check:async()=>{reads++;throw Error('Unexpected receipt observation');}},{deadline:Date.now()+5000});
    assert.equal(observed,value);assert.equal(writes,1);assert.equal(reads,0);
});
test('Apply refuses generic and crossed unknown before any receipt observation',async()=>{
    for(const change of [v=>{v.error='native_read_failed';},v=>{v.outcome.key={...v.outcome.key,nonce:'8'.repeat(32)};},v=>{v.outcome.request_digest='8'.repeat(64);},v=>{v.namespace='8'.repeat(32);},v=>{v.outcome.observation.fence.value.preview_digest='8'.repeat(64);},v=>{v.outcome.observation.fence.value.plan.worker_obligation.providers=[];},v=>{v.submittedReview={};}]){
        const f=acceptedApplyFixture(),value=structuredClone(f.initial);change(value);let writes=0,reads=0;
        await assert.rejects(observeReviewedApply(f.review,{apply:async()=>{writes++;},read:async()=>value,check:async()=>{reads++;return f.view(f.terminal);}},{deadline:Date.now()+5000}));assert.equal(writes,1);assert.equal(reads,0);
    }
});
test('Apply receipt observations reject missing, expired, crossed, regressed, or incomplete native proofs',async()=>{
    for(const change of [v=>{v.receipt=null;delete v.fence;},v=>{v.status='catalog_receipt_unprepared';},v=>{v.receipt={...v.receipt,key:{...v.receipt.key,nonce:'8'.repeat(32)}};},v=>{v.fence.value.prepared_digest='8'.repeat(64);},v=>{v.fence.value.proof.worker_confirmations=[];},v=>{v.fence.value.proof.worker_obligation.providers[0].history.generations=['8'.repeat(32)];},v=>{delete v.fence.value.proof;}]){
        const f=acceptedApplyFixture(),value=structuredClone(f.terminal);change(value);let writes=0;
        await assert.rejects(observeReviewedApply(f.review,{apply:async()=>{writes++;},read:async()=>f.initial,check:async()=>f.view(value)},{deadline:Date.now()+5000}));assert.equal(writes,1);
    }
});
test('accepted Apply shared deadline, request and evidence budgets fail without mutation replay',async()=>{
    for(const mode of ['deadline','requests','bytes']){
        const f=acceptedApplyFixture();let writes=0,reads=0,time=0;
        await assert.rejects(observeReviewedApply(f.review,{apply:async()=>{writes++;},read:async()=>f.initial,check:async()=>{reads++;const value=structuredClone(f.pending);if(mode==='deadline')time=5000;if(mode==='bytes')value.extra='x'.repeat(524288);return f.view(value);}},{deadline:5000,maxReceipts:2,now:()=>time,pause:async ms=>{time+=ms;}}),/deadline|request budget|byte budget/);
        assert.equal(writes,1);assert.ok(reads<=2);
    }
});
test('Apply transport uncertainty and expired preconditions never trigger another dispatch',async()=>{
    const f=acceptedApplyFixture();let writes=0,reads=0;
    await assert.rejects(observeReviewedApply(f.review,{apply:async()=>{writes++;throw Error('CDP outcome unknown');},read:async()=>{reads++;return f.initial;}},{deadline:Date.now()+5000}),/CDP outcome unknown/);assert.equal(writes,1);assert.equal(reads,0);
    await assert.rejects(observeReviewedApply(f.review,{apply:async()=>{writes++;}},{deadline:0}),/deadline/);assert.equal(writes,1);
});
test('full initial catalog snapshot is charged before cloning or recording accepted Apply evidence',async()=>{
    const f=acceptedApplyFixture();f.initial.unrelated_catalog_data='x'.repeat(524288);let writes=0,reads=0,records=0;
    await assert.rejects(observeReviewedApply(f.review,{apply:async()=>{writes++;},read:async()=>f.initial,record:()=>{records++;},check:async()=>{reads++;return f.view(f.terminal);}},{deadline:Date.now()+5000}),/byte budget/);
    assert.equal(writes,1);assert.equal(reads,0);assert.equal(records,0);
});
function fixture(specs){
    const writes=[],events=[];
    class Input{
        constructor(type='text'){this.type=type;this.tagName='INPUT';this.current='';this.flag=false;}
        get value(){return this.current;}set value(value){writes.push(value);this.current=value;}
        get checked(){return this.flag;}set checked(value){writes.push(value);this.flag=value;}
        dispatchEvent(event){events.push(event.type);}
    }
    class Select extends Input{constructor(){super();this.tagName='SELECT';}}
    const inputs=specs.map(row=>new Input(row.type));
    const labels=specs.map((row,i)=>({childNodes:[{nodeType:3,textContent:row.label}],querySelector:()=>inputs[i]}));
    const form={querySelectorAll:()=>labels},button={textContent:'Review new index',closest:()=>form};
    const context=vm.createContext({document:{querySelectorAll:()=>[button]},Node:{TEXT_NODE:3},HTMLInputElement:Input,HTMLSelectElement:Select,Event:class{constructor(type){this.type=type;}}});
    return {run:expression=>vm.runInContext(expression,context),inputs,writes,events};
}
test('actual setup labels remain aligned with renderer; exact reviewed locations are preserved',()=>{
    const source=fs.readFileSync(path.resolve(__dirname,'../../packages/vscode/src/explorerWebview/indexing.tsx'),'utf8');assert.ok(source.includes(label));
    const values={[label]:'/owned/index-storage','New collection name':'fixture-a'};
    const f=fixture(Object.keys(values).map(label=>({label}))),e=formFillExpressions('Review new index',values);
    assert.equal(f.run(e.ready),true);assert.equal(f.run(e.apply),true);assert.equal(f.run(e.readback),true);
    assert.deepEqual(f.writes,Object.values(values));assert.deepEqual(f.events,['input','input']);
});
test('missing later field causes no earlier write, not partial fill or a submit',()=>{
    const f=fixture([{label}]),e=formFillExpressions('Review new index',{[label]:'exact-index','Missing later field':'exact-project'});
    assert.equal(f.run(e.ready),false);assert.equal(f.run(e.apply),false);assert.deepEqual(f.writes,[]);assert.deepEqual(f.events,[]);
});
test('old label and duplicate labels refuse without writes',()=>{
    const f=fixture([{label}]),old=formFillExpressions('Review new index',{'Index ID from this grant':'exact-index'});
    assert.equal(f.run(old.ready),false);assert.equal(f.run(old.apply),false);assert.deepEqual(f.writes,[]);
    const dup=fixture([{label},{label}]),e=formFillExpressions('Review new index',{[label]:'exact-index'});
    assert.equal(dup.run(e.apply),false);assert.deepEqual(dup.writes,[]);
});
test('readonly readback catches lost exact values before a separate submit',()=>{
    const f=fixture([{label}]),e=formFillExpressions('Review new index',{[label]:'exact-index'});
    assert.equal(f.run(e.apply),true);f.inputs[0].current='';assert.equal(f.run(e.readback),false);assert.deepEqual(f.events,['input']);
});

function checkboxFixture(specs){
    const writes=[],events=[];
    class Input{constructor(){this.type='checkbox';this.flag=false;this.disabled=false;}get checked(){return this.flag;}set checked(value){this.flag=value;writes.push(value);}dispatchEvent(event){events.push(event.type);}}
    const fields=specs.map(group=>({disabled:!!group.disabled,querySelector:()=>({textContent:group.legend}),labels:group.names.map(name=>{const node=new Input();return {node,textContent:name,querySelector:selector=>selector==='strong'?null:node};}),querySelectorAll(){return this.labels;}}));
    const form={querySelectorAll:()=>fields},button={textContent:'Apply selection',closest:()=>form};
    const context=vm.createContext({document:{querySelectorAll:()=>[button]},HTMLInputElement:Input,Event:class{constructor(type){this.type=type;}}});
    return {run:expression=>vm.runInContext(expression,context),fields,writes,events};
}
test('Apply waits for the exact current review receipt, preserving nonce/digest/fence binding',()=>{
    const review={request:{caller:'caller',key:{namespace:'namespace',epoch:1,nonce:'new-request'},expected_catalog_revision:8,operation:{operation:'delete_project'}},requestDigest:'new-digest',previewDigest:'preview-digest'};
    const value={submittedReview:structuredClone(review),outcome:{status:'catalog_applied',receipt:{schema:1,caller:'caller',key:structuredClone(review.request.key),request_digest:'new-digest',catalog_revision:9,authority_fence_required:true},fenced_receipt:{request:structuredClone(review.request),preview_digest:'preview-digest',proof:{bound:true}}}};
    assert.equal(appliedReviewMatches(value,review),true);
    const reordered=structuredClone(value);reordered.outcome.receipt.key={epoch:1,nonce:'new-request',namespace:'namespace'};
    assert.equal(appliedReviewMatches(reordered,review),true,'Native JSON field order does not change a reviewed request identity');
    for(const change of [item=>{item.outcome.receipt.key.nonce='prior-request';},item=>{item.outcome.receipt.request_digest='prior-digest';},item=>{item.submittedReview.requestDigest='prior-digest';},item=>{item.outcome.fenced_receipt.request.key.nonce='crossed-request';},item=>{delete item.outcome.fenced_receipt.proof;}]){
        const stale=structuredClone(value);change(stale);assert.equal(appliedReviewMatches(stale,review),false);
    }
    assert.equal(appliedReviewMatches({outcome:{status:'catalog_applied'}},review),false);
});
test('current checkbox selection sets every row exactly before a separate Apply dispatch',()=>{
    const source=fs.readFileSync(path.resolve(__dirname,'../../packages/vscode/src/explorerWebview/indexing.tsx'),'utf8');
    for(const label of ['Apply selection','Index sets','Create index set','Include indexes'])assert.ok(source.includes(label));
    const f=checkboxFixture([{legend:'Indexes',names:['Index A','Index B']},{legend:'Index sets',names:['Shared A','Shared B']}]);
    f.fields[0].labels[0].node.flag=true;const e=checkboxFillExpressions('Apply selection',[{legend:'Indexes',names:[]},{legend:'Index sets',names:['Shared B']}]);
    assert.equal(f.run(e.ready),true);assert.equal(f.run(e.apply),true);assert.equal(f.run(e.readback),true);
    assert.deepEqual(f.writes,[false,false,false,true]);assert.deepEqual(f.events,['change','change','change','change']);
    f.fields[1].labels[1].node.flag=false;assert.equal(f.run(e.readback),false);
});
test('ambiguous, absent or disabled checkbox groups refuse before any field changes',()=>{
    for(const specs of [[{legend:'Index sets',names:['A','A']}],[{legend:'Index sets',names:['B']}],[{legend:'Index sets',names:['A'],disabled:true}],[{legend:'Index sets',names:['A']},{legend:'Index sets',names:['A']}]]){
        const f=checkboxFixture(specs),e=checkboxFillExpressions('Apply selection',[{legend:'Index sets',names:['A']}]);
        assert.equal(f.run(e.ready),false);assert.equal(f.run(e.apply),false);assert.deepEqual(f.writes,[]);
    }
    assert.throws(()=>checkboxFillExpressions('Apply selection',[{legend:'Index sets',names:['A','A']}]),/Invalid exact/);
});
test('checkbox booleans retain type and change event; invalid input types refuse',()=>{
    const f=fixture([{label:'Watch',type:'checkbox'}]),e=formFillExpressions('Review new index',{Watch:false});
    assert.equal(f.run(e.apply),true);assert.equal(f.run(e.readback),true);assert.deepEqual(f.events,['change']);
    assert.throws(()=>formFillExpressions('Review new index',{Watch:[]}));
});
test('child environment isolates inherited editor role/cache/IPC without changing host or explicit fixture scope',()=>{
    const parent={PATH:'owned-tools',AIRK_EDITOR_SCENARIOS:'same',AIRK_PRIVATE_CENSUS_BASE:'fictional-private',VSCODE_CWD:'normal-code',VSCODE_NODE_COMPILE_CACHE_ROOT:'normal-cache',VSCODE_CODE_CACHE_PATH:'normal-user-cache',VSCODE_ESM_ENTRYPOINT:'extensionHostProcess',VSCODE_IPC_HOOK_CLI:'normal-hook',vscode_fake_cache:'case-variant',ELECTRON_RUN_AS_NODE:'1',NODE_COMPILE_CACHE:'normal-node-cache'};
    const before={...parent},child=privateEditorEnvironment(parent);
    assert.deepEqual(parent,before);assert.ok(!Object.keys(child).some(key=>key.toUpperCase().startsWith('VSCODE_')));
    assert.equal(child.ELECTRON_RUN_AS_NODE,undefined);assert.equal(child.NODE_COMPILE_CACHE,undefined);
    assert.equal(child.PATH,parent.PATH);assert.equal(child.AIRK_EDITOR_SCENARIOS,'same');assert.equal(child.AIRK_PRIVATE_CENSUS_BASE,'fictional-private');
    assert.equal({...child,ELECTRON_RUN_AS_NODE:'1'}.ELECTRON_RUN_AS_NODE,'1','explicit CLI role remains possible per canonical caller');
});

const exactUsage={index:'idx_'+'a'.repeat(32),collection:'fixture-b'};
test('usage administration explicitly chooses exact native combined Index before observing report',async()=>{
    const calls=[],result={text:'qualified entire-Index report'};
    const actual=await openUsageAdministration(exactUsage,{open:async()=>calls.push('open'),choose:async(id,title)=>calls.push({id,title}),observe:async()=>{calls.push('observe');return result;}});
    assert.equal(actual,result);assert.deepEqual(calls,['open',{id:exactUsage.index,title:'Choose an index for administration'},'observe']);
    const host=fs.readFileSync(path.resolve(__dirname,'../../packages/vscode/src/serviceExtension.ts'),'utf8');assert.ok(host.includes("title:'Choose an index for administration'"));
    const driver=fs.readFileSync(path.resolve(__dirname,'../../packages/vscode/src/test/installedEditor.ts'),'utf8');assert.ok(driver.includes("ui('usageAdmin',{index:target.index,collection:target.backing.collection})"));
    assert.ok(driver.includes('value?.administrative_scope?.index_id===target.index'),'actual generated native report still checks exact Index');
});
test('invalid usage selection refuses before any UI action',async()=>{
    for(const request of [{collection:'fixture-b'},{index:'fixture-b',collection:'fixture-b'},{index:exactUsage.index,collection:''}]){
        let calls=0;await assert.rejects(openUsageAdministration(request,{open:async()=>calls++,choose:async()=>calls++,observe:async()=>calls++}),/Invalid exact usage/);assert.equal(calls,0);
    }
});
test('unknown usage picker outcome is not replayed or replaced with report observation',async()=>{
    let opens=0,picks=0,observes=0;const failure=new Error('unknown picker outcome');
    await assert.rejects(openUsageAdministration(exactUsage,{open:async()=>opens++,choose:async()=>{picks++;throw failure;},observe:async()=>observes++}),error=>error===failure);
    assert.deepEqual([opens,picks,observes],[1,1,0]);
});
test('usage report failure preserves exact error without reopening or choosing again',async()=>{
    let opens=0,picks=0,observes=0;const failure=new Error('missing actual report');
    await assert.rejects(openUsageAdministration(exactUsage,{open:async()=>opens++,choose:async()=>picks++,observe:async()=>{observes++;throw failure;}}),error=>error===failure);
    assert.deepEqual([opens,picks,observes],[1,1,1]);
});

test('bounded UI diagnostics preserve first and latest distinct observations without changing raw snapshots',()=>{
    const samples=[],first={widgets:[{visible:false,rows:[]}],notifications:[]};retainUiDiagnostic(samples,first);
    for(let i=0;i<8;i++)retainUiDiagnostic(samples,{widgets:[{title:'actual picker',rows:[{text:'idx_'+i}]}]});
    assert.equal(samples.length,4);assert.deepEqual(samples[0].value,first);assert.equal(samples.at(-1).value.widgets[0].rows[0].text,'idx_7');
    const count=samples.length;retainUiDiagnostic(samples,structuredClone(samples.at(-1).value));assert.equal(samples.length,count);
    samples[0].value.widgets[0].visible=true;assert.equal(first.widgets[0].visible,false,'Retained evidence is an independent raw snapshot');
});
test('oversized DOM diagnostic explicitly discloses unavailable observation within the fixed bound',()=>{
    const samples=[];retainUiDiagnostic(samples,{text:'x'.repeat(65537)});
    assert.equal(samples[0].value.truncated,true);assert.ok(samples[0].value.observed_bytes>65536);assert.match(samples[0].value.observation_unavailable,/64KiB/);
    assert.ok(Buffer.byteLength(JSON.stringify(samples))<1024);
});


test('one UI action deadline precedes the driver receipt deadline and nested waits cannot renew it',()=>{
    const started=10000,scenario=started+600000,deadline=editorActionDeadline(scenario,started);
    assert.equal(deadline,started+120000);assert.ok(deadline<started+150000);
    assert.equal(editorRemainingBudget(deadline,180000,started),120000);
    assert.equal(editorRemainingBudget(deadline,180000,started+119000),1000,'nested catalog wait shares original deadline');
    assert.throws(()=>editorRemainingBudget(deadline,180000,deadline),/deadline exhausted/);
    assert.equal(editorActionDeadline(started+30000,started),started+30000,'scenario bound can only shorten an action');
    assert.equal(editorRemainingBudget(deadline,2000,started),2000,'CDP call ceiling remains bounded');
});

test('action diagnostics preserve first/latest independent native and rendered samples within fixed memory bounds',()=>{
    const actions=[],original={action:'page',sequence:1,native:[],rendered:[]};
    const first=beginUiDiagnostic(actions,original);
    for(let index=0;index<20;index++){
        const entry=beginUiDiagnostic(actions,{action:'page',sequence:index+2,native:[],rendered:[]});
        for(let sample=0;sample<10;sample++){
            retainUiDiagnostic(entry.native,{generation:sample,retained_native:'x'.repeat(40000)});
            retainUiDiagnostic(entry.rendered,{text:'y'.repeat(32768),notice:'actual local failure '+sample});
        }
        assert.equal(entry.native.length,4);assert.equal(entry.rendered.length,4);assert.equal(entry.native.at(-1).value.generation,9);
    }
    assert.equal(actions.length,8);assert.equal(actions[0],first);assert.equal(actions.at(-1).sequence,21);
    assert.deepEqual(original.native,[]);assert.deepEqual(original.rendered,[]);
    assert.ok(Buffer.byteLength(JSON.stringify(actions))<8*8*65536+8192);
});

test('actual editor client homes are isolated only in child environment copies',()=>{
    const parent={HOME:'ambient-home',USERPROFILE:'ambient-user',CODEX_HOME:'ambient-codex',CLAUDE_CONFIG_DIR:'ambient-claude',APPDATA:'ambient-appdata',PATH:'retain',OTHER:'retain'},before={...parent},owned=path.resolve('owned-fixture','client-home');
    const child=privateClientEnvironment(parent,owned);assert.deepEqual(parent,before);
    assert.equal(child.HOME,owned);assert.equal(child.USERPROFILE,owned);assert.equal(child.CODEX_HOME,path.join(owned,'.codex'));assert.equal(child.CLAUDE_CONFIG_DIR,path.join(owned,'.claude'));
    assert.equal(child.APPDATA,path.join(owned,'AppData','Roaming'));assert.equal(child.XDG_CONFIG_HOME,path.join(owned,'.config'));assert.equal(child.PATH,parent.PATH);assert.equal(child.OTHER,parent.OTHER);
    assert.throws(()=>privateClientEnvironment(parent,'relative-home'));
});
