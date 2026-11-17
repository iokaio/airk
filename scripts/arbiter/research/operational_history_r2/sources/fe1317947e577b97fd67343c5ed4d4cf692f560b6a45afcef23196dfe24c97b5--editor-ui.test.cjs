// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs'),path=require('node:path');
const {formFillExpressions,privateEditorEnvironment}=require('../editor-ui.cjs');
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
