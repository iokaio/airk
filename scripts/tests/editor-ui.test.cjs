// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs'),path=require('node:path');
const {formFillExpressions,privateEditorEnvironment,openUsageAdministration,retainUiDiagnostic,beginUiDiagnostic,editorActionDeadline,editorRemainingBudget}=require('../editor-ui.cjs');
const label='Optional index subset from this grant (1–32 IDs; blank includes all permitted members)';
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
    const form={querySelectorAll:()=>labels},button={textContent:'Use Grant for this window',closest:()=>form};
    const context=vm.createContext({document:{querySelectorAll:()=>[button]},Node:{TEXT_NODE:3},HTMLInputElement:Input,HTMLSelectElement:Select,Event:class{constructor(type){this.type=type;}}});
    return {run:expression=>vm.runInContext(expression,context),inputs,writes,events};
}
test('actual optional-index label remains aligned with renderer; exact requested subset is preserved',()=>{
    const source=fs.readFileSync(path.resolve(__dirname,'../../packages/vscode/src/explorerWebview/indexing.tsx'),'utf8');assert.ok(source.includes(label));
    const values={[label]:'idx_'+'1'.repeat(32),'Project IDs from this grant (1–32, comma separated)':'prj_'+'2'.repeat(32)};
    const f=fixture(Object.keys(values).map(label=>({label}))),e=formFillExpressions('Use Grant for this window',values);
    assert.equal(f.run(e.ready),true);assert.equal(f.run(e.apply),true);assert.equal(f.run(e.readback),true);
    assert.deepEqual(f.writes,Object.values(values));assert.deepEqual(f.events,['input','input']);
});
test('missing later field causes no earlier write, not partial fill or a submit',()=>{
    const f=fixture([{label}]),e=formFillExpressions('Use Grant for this window',{[label]:'exact-index','Missing later field':'exact-project'});
    assert.equal(f.run(e.ready),false);assert.equal(f.run(e.apply),false);assert.deepEqual(f.writes,[]);assert.deepEqual(f.events,[]);
});
test('old label and duplicate labels refuse without writes',()=>{
    const f=fixture([{label}]),old=formFillExpressions('Use Grant for this window',{'Index ID from this grant':'exact-index'});
    assert.equal(f.run(old.ready),false);assert.equal(f.run(old.apply),false);assert.deepEqual(f.writes,[]);
    const dup=fixture([{label},{label}]),e=formFillExpressions('Use Grant for this window',{[label]:'exact-index'});
    assert.equal(dup.run(e.apply),false);assert.deepEqual(dup.writes,[]);
});
test('readonly readback catches lost exact values before a separate submit',()=>{
    const f=fixture([{label}]),e=formFillExpressions('Use Grant for this window',{[label]:'exact-index'});
    assert.equal(f.run(e.apply),true);f.inputs[0].current='';assert.equal(f.run(e.readback),false);assert.deepEqual(f.events,['input']);
});
test('checkbox booleans retain type and change event; invalid input types refuse',()=>{
    const f=fixture([{label:'Watch',type:'checkbox'}]),e=formFillExpressions('Use Grant for this window',{Watch:false});
    assert.equal(f.run(e.apply),true);assert.equal(f.run(e.readback),true);assert.deepEqual(f.events,['change']);
    assert.throws(()=>formFillExpressions('Use Grant for this window',{Watch:[]}));
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
