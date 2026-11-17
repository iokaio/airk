# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path(__file__).resolve().parents[2]
name='scripts/editor-ui.cjs'
old=(root/name).read_text(encoding='utf-8')
new=old.replace('const dispatchLog=[],pickerDiagnostics=[];', 'const dispatchLog=[],pickerDiagnostics=[],mutationDiagnostics=[],readDiagnostics=[];let activeReadDiagnostic;')
assert new!=old
before='const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));'
after="""function beginUiDiagnostic(entries,value){
    if(entries.length===8)entries.splice(1,1);
    const diagnostic=structuredClone(value);entries.push(diagnostic);return diagnostic;
}
function editorActionDeadline(scenarioDeadline,now=Date.now()){return Math.min(scenarioDeadline,now+120000);}
function editorRemainingBudget(deadline,maximum,now=Date.now()){
    const left=Math.min(maximum,deadline-now);if(left<=0)throw new Error('Editor UI action/scenario deadline exhausted');return left;
}
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));"""
assert new.count(before)==1
new=new.replace(before,after)
before="""        const target=await until(async()=>{const found=await document(predicate,ready,type);return found?.value?found:undefined;},'exact mutation target');"""
after="""        const diagnostic=beginUiDiagnostic(mutationDiagnostics,{next_dispatch:dispatches+1,type,ready_sha256:require('node:crypto').createHash('sha256').update(ready).digest('hex'),samples:[]});
        const target=await until(async()=>{
            const found=await document(predicate,ready,type);
            if(found?.value)return found;
            // Read-only diagnostics never dispatch or retry a mutation. Retain
            // bounded first/latest rendered states when an exact control vanishes.
            if(found){try{retainUiDiagnostic(diagnostic.samples,await found.client.evaluate(found.context,\"({title:document.title,visibility:document.visibilityState,text:document.body.innerText.slice(0,32768),text_truncated:document.body.innerText.length>32768,buttons:[...document.querySelectorAll('button')].slice(0,64).map(b=>({text:b.textContent.trim().slice(0,160),disabled:b.disabled})),buttons_truncated:document.querySelectorAll('button').length>64})\"));}catch(error){retainUiDiagnostic(diagnostic.samples,{observation_unavailable:String(error).slice(0,4096)});}}
            return undefined;
        },'exact mutation target');"""
assert new.count(before)==1
new=new.replace(before,after)
before="""    async function catalog(){
        return until(async()=>{const value=(await service("""
after="""    async function catalog(){
        const diagnostic=activeReadDiagnostic??beginUiDiagnostic(readDiagnostics,{action:'catalog',native:[],rendered:[]});
        return until(async()=>{const value=(await service("""
assert new.count(before)==1
new=new.replace(before,after)
before="""?.value;return value||undefined;},'complete native catalog details');"""
after="""?.value;
            // Retain each bounded native response independently of rendered
            // text so diagnostic overhead cannot erase an otherwise valid reply.
            retainUiDiagnostic(diagnostic.native,value??{observation_unavailable:'Complete response not rendered'});
            try{
                const rendered=(await service(\"({title:document.title,visibility:document.visibilityState,text:document.body.innerText.slice(0,32768),text_truncated:document.body.innerText.length>32768,notices:[...document.querySelectorAll('.notice')].slice(0,8).map(n=>n.innerText.slice(0,2048))})\"))?.value;
                retainUiDiagnostic(diagnostic.rendered,rendered??{observation_unavailable:'AIRK document not available'});
            }catch(error){retainUiDiagnostic(diagnostic.rendered,{observation_unavailable:String(error).slice(0,4096)});}
            return value||undefined;},'complete native catalog details');"""
assert new.count(before)==1
new=new.replace(before,after)
before='diagnostics:()=>({picker:structuredClone(pickerDiagnostics),reads,dispatches})'
after='diagnostics:()=>({picker:structuredClone(pickerDiagnostics),mutations:structuredClone(mutationDiagnostics),read_observations:structuredClone(readDiagnostics),reads,dispatches})'
assert new.count(before)==1
new=new.replace(before,after)
# Every nested UI wait observes one action deadline shorter than the driver's
# 150-second receipt wait, so the browser survives long enough to retain failure.
before="""    let reads=0,dispatches=0;"""
after="""    let actionDeadline=deadline;
    let reads=0,dispatches=0;"""
assert new.count(before)==1
new=new.replace(before,after)
before="""    const budget=max=>{const left=Math.min(max,deadline-Date.now());if(left<=0)throw new Error('Shared editor scenario deadline exhausted');return left;};"""
# Keep connection-level budgets unchanged; the second definition belongs to UI.
at=new.index('async function editorUi(')
assert new[at:].count(before)==1
new=new[:at]+new[at:].replace(before,"""    const budget=max=>editorRemainingBudget(actionDeadline,max);""",1)
before="""    async function indexing(request){"""
after="""    async function indexing(request){
        const previous=actionDeadline,previousDiagnostic=activeReadDiagnostic;actionDeadline=editorActionDeadline(deadline);
        const diagnostic=activeReadDiagnostic=beginUiDiagnostic(readDiagnostics,{action:request.action,sequence:request.sequence,native:[],rendered:[]});
        try{return await indexingAction(request);}catch(error){diagnostic.failure=String(error).slice(0,4096);throw error;}
        finally{actionDeadline=previous;activeReadDiagnostic=previousDiagnostic;}
    }
    async function indexingAction(request){"""
assert new.count(before)==1
new=new.replace(before,after)
before='module.exports={editorUi,until,formFillExpressions,privateEditorEnvironment,openUsageAdministration,retainUiDiagnostic};'
after='module.exports={editorUi,until,formFillExpressions,privateEditorEnvironment,openUsageAdministration,retainUiDiagnostic,beginUiDiagnostic,editorActionDeadline,editorRemainingBudget};'
assert new.count(before)==1
new=new.replace(before,after)
out=root/'artifacts/qa-1.8.9/dispatch-target-diagnostics.patch'
stage=root/'artifacts/qa-1.8.9/dispatch-diagnostics-staged'
target=stage/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(new,encoding='utf-8',newline='\n')
patch=''.join(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
test_name='scripts/tests/editor-ui.test.cjs'
old_tests=(root/test_name).read_text(encoding='utf-8')
new_tests=old_tests.replace("retainUiDiagnostic}=require('../editor-ui.cjs');", "retainUiDiagnostic,beginUiDiagnostic,editorActionDeadline,editorRemainingBudget}=require('../editor-ui.cjs');")
assert new_tests!=old_tests
new_tests+=r'''

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
'''
target=stage/test_name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(new_tests,encoding='utf-8',newline='\n')
patch+=''.join(difflib.unified_diff(old_tests.splitlines(True),new_tests.splitlines(True),fromfile='a/'+test_name,tofile='b/'+test_name))
out.write_text(patch,encoding='utf-8',newline='\n')
print(out)
